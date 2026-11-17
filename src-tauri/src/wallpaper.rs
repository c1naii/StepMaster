//! Wallpapers: built-in StepMaster set + the user's own library, set as the desktop background,
//! optional timed rotation (works while StepMaster lives in the tray) and restore of the original.
use crate::settings::write_json;
use crate::util::{self, tr, Res};
use crate::AppState;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager};
use winreg::enums::*;
use winreg::RegKey;

struct Builtin {
    key: &'static str,
    name: &'static str,
    full: &'static [u8],
    thumb: &'static [u8],
}

macro_rules! builtin {
    ($key:literal, $name:literal) => {
        Builtin {
            key: $key,
            name: $name,
            full: include_bytes!(concat!("../../assets/wallpapers/", $key, ".jpg")),
            thumb: include_bytes!(concat!("../../assets/wallpapers/", $key, ".thumb.jpg")),
        }
    };
}

const BUILTIN: &[Builtin] = &[
    builtin!("win10", "10"),
    builtin!("win7", "7"),
    builtin!("xp", "XP"),
    builtin!("win11", "11"),
    builtin!("cat", "Cat"),
    builtin!("iceberg", "Iceberg"),
    builtin!("pinky", "Pinky"),
    builtin!("backrooms", "Backrooms"),
    builtin!("vi", "VI"),
];

/// Formats accepted when adding; jpg/png/bmp are kept as is, the rest is converted to jpg.
const IMPORT_EXT: &[&str] = &["jpg", "jpeg", "jfif", "png", "bmp", "webp", "gif"];
const STORED_EXT: &[&str] = &["jpg", "png", "bmp"];
const MAX_IMPORT: u64 = 200 * 1024 * 1024;
const FITS: &[&str] = &["fill", "fit", "stretch", "center", "span"];
const SOURCES: &[&str] = &["all", "mine", "builtin"];
const DESKTOP: &str = r"Control Panel\Desktop";

/// Serializes every read-modify-write of wallpaper.json (UI commands + rotation thread).
static LOCK: Mutex<()> = Mutex::new(());

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Options {
    pub fit: String,
    pub auto: bool,
    /// Minutes between changes; 0 = once at Windows sign-in.
    pub interval: u32,
    pub shuffle: bool,
    pub source: String,
}

impl Default for Options {
    fn default() -> Self {
        Self { fit: "fill".into(), auto: false, interval: 60, shuffle: true, source: "all".into() }
    }
}

#[derive(Serialize, Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase", default)]
struct Saved {
    options: Options,
    last_change: u64,
    last_id: Option<String>,
    /// Library file name -> name shown to the user (the original file name).
    names: HashMap<String, String>,
    /// The wallpaper that was on the desktop when StepMaster started ("Restore" puts it back).
    previous: Option<Previous>,
}

#[derive(Serialize, Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase", default)]
struct Previous {
    path: String,
    backup: Option<String>,
    style: Option<String>,
    tile: Option<String>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    id: String,
    name: String,
    builtin: bool,
    path: String,
    thumb: String,
    width: u32,
    height: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WallState {
    builtin: Vec<Item>,
    mine: Vec<Item>,
    current: Option<String>,
    options: Options,
    can_restore: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AddResult {
    state: WallState,
    added: u32,
    failed: Vec<String>,
}

// ---------- storage ----------

fn root(data: &Path) -> PathBuf {
    data.join("wallpapers")
}
fn lib_dir(data: &Path) -> PathBuf {
    root(data).join("library")
}
fn thumbs_dir(data: &Path) -> PathBuf {
    root(data).join("thumbs")
}
fn state_path(data: &Path) -> PathBuf {
    data.join("wallpaper.json")
}

fn load(data: &Path) -> Saved {
    fs::read_to_string(state_path(data)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn save(data: &Path, s: &Saved) -> Res<()> {
    write_json(&state_path(data), s)
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn data_of(app: &AppHandle) -> PathBuf {
    app.state::<AppState>().data_dir.clone()
}

/// Writes the embedded wallpapers to disk (Windows needs a file path) if missing or changed.
fn ensure_builtin(data: &Path) -> Res<()> {
    let dir = root(data).join("builtin");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    for b in BUILTIN {
        for (file, bytes) in [(format!("{}.jpg", b.key), b.full), (format!("{}.thumb.jpg", b.key), b.thumb)] {
            let p = dir.join(file);
            let same = fs::metadata(&p).map(|m| m.len() == bytes.len() as u64).unwrap_or(false);
            if !same {
                fs::write(&p, bytes).map_err(|e| e.to_string())?;
            }
        }
    }
    Ok(())
}

fn dims(p: &Path) -> (u32, u32) {
    image::image_dimensions(p).unwrap_or((0, 0))
}

fn builtin_items(data: &Path) -> Vec<Item> {
    let dir = root(data).join("builtin");
    BUILTIN
        .iter()
        .map(|b| {
            let path = dir.join(format!("{}.jpg", b.key));
            let (width, height) = dims(&path);
            Item {
                id: format!("b:{}", b.key),
                name: b.name.into(),
                builtin: true,
                path: path.to_string_lossy().into(),
                thumb: data_url(&dir.join(format!("{}.thumb.jpg", b.key))),
                width,
                height,
            }
        })
        .collect()
}

/// The page cannot read local files, so pictures travel as data URLs.
fn data_url(p: &Path) -> String {
    use base64::Engine;
    let mime = match p.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref() {
        Some("png") => "image/png",
        Some("bmp") => "image/bmp",
        _ => "image/jpeg",
    };
    match fs::read(p) {
        Ok(b) => format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(b)),
        Err(_) => String::new(),
    }
}

fn stored_ext(p: &Path) -> bool {
    p.extension().and_then(|e| e.to_str()).map(|e| STORED_EXT.contains(&e.to_ascii_lowercase().as_str())).unwrap_or(false)
}

fn make_thumb(img: &image::DynamicImage, dst: &Path) -> Res<()> {
    let t = img.thumbnail(640, 640).to_rgb8();
    let mut out = fs::File::create(dst).map_err(|e| e.to_string())?;
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 84).encode_image(&t).map_err(|e| e.to_string())
}

/// User library, newest first. Missing thumbnails are rebuilt; unreadable files are skipped.
fn mine_items(data: &Path, names: &HashMap<String, String>) -> Vec<Item> {
    let lib = lib_dir(data);
    let thumbs = thumbs_dir(data);
    let Ok(rd) = fs::read_dir(&lib) else { return vec![] };
    let mut files: Vec<(SystemTime, PathBuf)> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && stored_ext(p))
        .map(|p| (fs::metadata(&p).and_then(|m| m.modified()).unwrap_or(UNIX_EPOCH), p))
        .collect();
    files.sort_by_key(|x| std::cmp::Reverse(x.0));
    files
        .into_iter()
        .filter_map(|(_, p)| {
            let file = p.file_name()?.to_string_lossy().to_string();
            let stem = p.file_stem()?.to_string_lossy().to_string();
            let thumb = thumbs.join(format!("{stem}.jpg"));
            if !thumb.is_file() {
                let img = image::open(&p).ok()?;
                let _ = fs::create_dir_all(&thumbs);
                make_thumb(&img, &thumb).ok()?;
            }
            let (width, height) = dims(&p);
            Some(Item {
                id: format!("u:{file}"),
                name: names.get(&file).cloned().unwrap_or(stem),
                builtin: false,
                path: p.to_string_lossy().into(),
                thumb: data_url(&thumb),
                width,
                height,
            })
        })
        .collect()
}

/// Library file names are ours ("<hash>.<ext>"): no separators, no "..".
fn valid_file(name: &str) -> bool {
    !name.is_empty()
        && name.len() < 100
        && !name.contains(['/', '\\', ':'])
        && !name.starts_with('.')
        && !name.contains("..")
        && stored_ext(Path::new(name))
}

fn resolve(data: &Path, id: &str) -> Res<PathBuf> {
    let missing = || tr("Обои не найдены", "Wallpaper not found");
    let p = if let Some(key) = id.strip_prefix("b:") {
        let b = BUILTIN.iter().find(|b| b.key == key).ok_or_else(missing)?;
        ensure_builtin(data)?;
        root(data).join("builtin").join(format!("{}.jpg", b.key))
    } else if let Some(file) = id.strip_prefix("u:") {
        if !valid_file(file) {
            return Err(missing());
        }
        lib_dir(data).join(file)
    } else {
        return Err(missing());
    };
    if p.is_file() {
        Ok(p)
    } else {
        Err(missing())
    }
}

fn same_path(a: &str, b: &str) -> bool {
    !a.is_empty() && a.replace('/', "\\").eq_ignore_ascii_case(&b.replace('/', "\\"))
}

fn build_state(data: &Path, s: &Saved) -> WallState {
    let _ = ensure_builtin(data);
    let builtin = builtin_items(data);
    let mine = mine_items(data, &s.names);
    let cur = spi_get();
    let current = builtin.iter().chain(mine.iter()).find(|i| same_path(&i.path, &cur)).map(|i| i.id.clone());
    WallState { builtin, mine, current, options: s.options.clone(), can_restore: s
            .previous
            .as_ref()
            .is_some_and(|p| !same_path(&p.path, &cur) && !p.backup.as_deref().is_some_and(|b| same_path(b, &cur))) }
}

// ---------- Windows ----------

fn spi_get() -> String {
    use windows_sys::Win32::UI::WindowsAndMessaging::{SystemParametersInfoW, SPI_GETDESKWALLPAPER};
    let mut buf = [0u16; 1024];
    let ok = unsafe { SystemParametersInfoW(SPI_GETDESKWALLPAPER, buf.len() as u32, buf.as_mut_ptr().cast(), 0) };
    if ok == 0 {
        return String::new();
    }
    let n = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..n])
}

fn spi_set(path: &str) -> Res<()> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SystemParametersInfoW, SPIF_SENDCHANGE, SPIF_UPDATEINIFILE, SPI_SETDESKWALLPAPER,
    };
    let mut w: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
    let ok = unsafe { SystemParametersInfoW(SPI_SETDESKWALLPAPER, 0, w.as_mut_ptr().cast(), SPIF_UPDATEINIFILE | SPIF_SENDCHANGE) };
    if ok == 0 {
        Err(tr("Windows не дал поменять обои", "Windows refused to change the wallpaper"))
    } else {
        Ok(())
    }
}

/// Registry values Windows uses for the picture position: (WallpaperStyle, TileWallpaper).
fn fit_values(fit: &str) -> (&'static str, &'static str) {
    match fit {
        "fit" => ("6", "0"),
        "stretch" => ("2", "0"),
        "center" => ("0", "0"),
        "span" => ("22", "0"),
        _ => ("10", "0"),
    }
}

fn set_style(style: &str, tile: &str) -> Res<()> {
    let k = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(DESKTOP, KEY_SET_VALUE)
        .map_err(|e| e.to_string())?;
    k.set_value("WallpaperStyle", &style.to_string()).map_err(|e| e.to_string())?;
    k.set_value("TileWallpaper", &tile.to_string()).map_err(|e| e.to_string())
}

fn set_desktop(path: &Path, fit: &str) -> Res<()> {
    let (style, tile) = fit_values(fit);
    set_style(style, tile)?;
    spi_set(&path.to_string_lossy())
}

fn is_ours(data: &Path, p: &str) -> bool {
    let r = root(data).to_string_lossy().to_ascii_lowercase().replace('/', "\\");
    p.to_ascii_lowercase().replace('/', "\\").starts_with(&r)
}

/// Remembers what is on the desktop right now (called when StepMaster starts). A picture from
/// outside StepMaster is copied: Windows' own TranscodedWallpaper is overwritten on every change.
fn capture_previous(data: &Path, s: &mut Saved) {
    let cur = spi_get();
    // Our own picture is on the desktop: the user's original is already remembered, keep it.
    if is_ours(data, &cur) && s.previous.is_some() {
        return;
    }
    let desk = RegKey::predef(HKEY_CURRENT_USER).open_subkey(DESKTOP).ok();
    let get = |n: &str| desk.as_ref().and_then(|k| k.get_value::<String, _>(n).ok());
    let mut backup = None;
    let src = Path::new(&cur);
    if !cur.is_empty() && src.is_file() && !is_ours(data, &cur) {
        let ext = src
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .filter(|e| IMPORT_EXT.contains(&e.as_str()))
            .unwrap_or_else(|| "jpg".into());
        let dir = root(data).join("kept");
        let dst = dir.join(format!("previous.{ext}"));
        if fs::create_dir_all(&dir).is_ok() && fs::copy(src, &dst).is_ok() {
            backup = Some(dst.to_string_lossy().to_string());
        }
    }
    s.previous = Some(Previous { path: cur, backup, style: get("WallpaperStyle"), tile: get("TileWallpaper") });
}

/// Deletes copies in kept/ (and the old history/ folder) that nothing points to any more.
fn sweep(data: &Path, s: &Saved) {
    let cur = spi_get();
    for dir in ["history", "kept"] {
        let Ok(rd) = fs::read_dir(root(data).join(dir)) else { continue };
        for f in rd.flatten().map(|e| e.path()) {
            let p = f.to_string_lossy().to_string();
            let used = same_path(&p, &cur)
                || s.previous.as_ref().is_some_and(|h| same_path(&h.path, &p) || h.backup.as_deref().is_some_and(|b| same_path(b, &p)));
            if !used {
                let _ = fs::remove_file(&f);
            }
        }
    }
}

/// Before a library picture is deleted: if "Restore" points to it, keep a copy so Restore still works.
fn keep_if_used(data: &Path, s: &mut Saved, path: &Path) -> Res<()> {
    let p = path.to_string_lossy().to_string();
    let Some(prev) = s.previous.as_mut() else { return Ok(()) };
    if !same_path(&prev.path, &p) {
        return Ok(());
    }
    let dir = root(data).join("kept");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let kept = dir.join(path.file_name().ok_or_else(|| tr("Обои не найдены", "Wallpaper not found"))?);
    fs::copy(path, &kept).map_err(|e| e.to_string())?;
    prev.path = kept.to_string_lossy().to_string();
    Ok(())
}

fn apply(data: &Path, s: &mut Saved, id: &str) -> Res<()> {
    let path = resolve(data, id)?;
    set_desktop(&path, &s.options.fit)?;
    s.last_id = Some(id.to_string());
    s.last_change = now();
    save(data, s)?;
    sweep(data, s);
    Ok(())
}

// ---------- rotation ----------

/// Next index: in order after `cur`, or random (never the same one twice in a row).
fn pick(n: usize, cur: Option<usize>, shuffle: bool, seed: u64) -> usize {
    if n <= 1 {
        return 0;
    }
    match (shuffle, cur) {
        (false, Some(c)) => (c + 1) % n,
        (false, None) => 0,
        (true, Some(c)) => {
            let i = (seed % (n as u64 - 1)) as usize;
            if i >= c {
                i + 1
            } else {
                i
            }
        }
        (true, None) => (seed % n as u64) as usize,
    }
}

fn seed() -> u64 {
    let n = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let h = blake3::hash(&n.to_le_bytes());
    u64::from_le_bytes(h.as_bytes()[..8].try_into().unwrap_or([0; 8]))
}

fn next(data: &Path) -> Res<()> {
    let _g = LOCK.lock();
    let mut s = load(data);
    let st = build_state(data, &s);
    let pool: Vec<Item> = match s.options.source.as_str() {
        "mine" => st.mine,
        "builtin" => st.builtin,
        _ => st.mine.into_iter().chain(st.builtin).collect(),
    };
    if pool.is_empty() {
        return Err(tr("Нет обоев для смены", "No wallpapers to rotate"));
    }
    let last = st.current.or_else(|| s.last_id.clone());
    let cur = last.and_then(|l| pool.iter().position(|i| i.id == l));
    let id = pool[pick(pool.len(), cur, s.options.shuffle, seed())].id.clone();
    apply(data, &mut s, &id)
}

/// Background rotation. Runs only in the normal (installed) mode, while the process is alive.
pub fn start(app: AppHandle, at_login: bool) {
    std::thread::spawn(move || {
        let data = data_of(&app);
        {
            let _g = LOCK.lock();
            let mut s = load(&data);
            capture_previous(&data, &mut s);
            let _ = save(&data, &s);
            sweep(&data, &s);
        }
        std::thread::sleep(Duration::from_secs(5));
        if at_login {
            let o = load(&data).options;
            if o.auto && o.interval == 0 && next(&data).is_ok() {
                let _ = app.emit("wallpaper://changed", ());
            }
        }
        loop {
            std::thread::sleep(Duration::from_secs(30));
            let s = load(&data);
            let due = s.options.auto
                && s.options.interval > 0
                && now().saturating_sub(s.last_change) >= u64::from(s.options.interval) * 60;
            if due && next(&data).is_ok() {
                let _ = app.emit("wallpaper://changed", ());
            }
        }
    });
}

// ---------- commands ----------

#[tauri::command]
pub async fn wallpaper_state(app: AppHandle) -> Res<WallState> {
    util::blocking(move || {
        let data = data_of(&app);
        let _g = LOCK.lock();
        Ok(build_state(&data, &load(&data)))
    })
    .await
}

/// Big picture for the viewer. Large user photos are shrunk once and cached next to the thumbnails.
#[tauri::command]
pub async fn wallpaper_view(app: AppHandle, id: String) -> Res<String> {
    util::blocking(move || {
        let data = data_of(&app);
        let path = resolve(&data, &id)?;
        let small = fs::metadata(&path).map(|m| m.len() <= 3 * 1024 * 1024).unwrap_or(false);
        let (w, h) = dims(&path);
        if id.starts_with("b:") || (small && w <= 2560 && h <= 2560 && !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("bmp"))) {
            return Ok(data_url(&path));
        }
        let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let view = thumbs_dir(&data).join(format!("{stem}.view.jpg"));
        if !view.is_file() {
            let img = image::open(&path).map_err(|e| e.to_string())?;
            let v = img.thumbnail(2560, 2560).to_rgb8();
            let _ = fs::create_dir_all(thumbs_dir(&data));
            let mut out = fs::File::create(&view).map_err(|e| e.to_string())?;
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 90).encode_image(&v).map_err(|e| e.to_string())?;
        }
        Ok(data_url(&view))
    })
    .await
}

#[tauri::command]
pub async fn wallpaper_set(app: AppHandle, id: String) -> Res<WallState> {
    util::blocking(move || {
        let data = data_of(&app);
        let _g = LOCK.lock();
        let mut s = load(&data);
        apply(&data, &mut s, &id)?;
        Ok(build_state(&data, &s))
    })
    .await
}

#[tauri::command]
pub async fn wallpaper_next(app: AppHandle) -> Res<WallState> {
    util::blocking(move || {
        let data = data_of(&app);
        next(&data)?;
        let _g = LOCK.lock();
        Ok(build_state(&data, &load(&data)))
    })
    .await
}

#[tauri::command]
pub async fn wallpaper_options(app: AppHandle, options: Options) -> Res<WallState> {
    util::blocking(move || {
        if !FITS.contains(&options.fit.as_str()) || !SOURCES.contains(&options.source.as_str()) || options.interval > 60 * 24 * 7 {
            return Err(tr("Неверные настройки", "Invalid options"));
        }
        let data = data_of(&app);
        let _g = LOCK.lock();
        let mut s = load(&data);
        let refit = s.options.fit != options.fit;
        // Turning rotation on (or changing the interval) starts the countdown now, not in the past.
        if options.auto && (!s.options.auto || s.options.interval != options.interval) {
            s.last_change = now();
        }
        s.options = options;
        if refit {
            let st = build_state(&data, &s);
            if let Some(id) = st.current {
                let path = resolve(&data, &id)?;
                set_desktop(&path, &s.options.fit)?;
            }
        }
        save(&data, &s)?;
        Ok(build_state(&data, &s))
    })
    .await
}

fn import(data: &Path, src: &Path) -> Res<(String, String)> {
    let bad = || tr("Это не картинка", "Not an image");
    let meta = fs::metadata(src).map_err(|e| e.to_string())?;
    let ext = src.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).unwrap_or_default();
    if !meta.is_file() || !IMPORT_EXT.contains(&ext.as_str()) {
        return Err(bad());
    }
    if meta.len() > MAX_IMPORT {
        return Err(tr("Файл слишком большой", "File is too large"));
    }
    let bytes = fs::read(src).map_err(|e| e.to_string())?;
    let img = image::load_from_memory(&bytes).map_err(|_| bad())?;
    let hash = blake3::hash(&bytes).to_hex()[..16].to_string();
    let keep = match ext.as_str() {
        "jpg" | "jpeg" | "jfif" => Some("jpg"),
        "png" => Some("png"),
        "bmp" => Some("bmp"),
        _ => None,
    };
    let lib = lib_dir(data);
    let thumbs = thumbs_dir(data);
    fs::create_dir_all(&lib).map_err(|e| e.to_string())?;
    fs::create_dir_all(&thumbs).map_err(|e| e.to_string())?;
    let file = format!("{hash}.{}", keep.unwrap_or("jpg"));
    let dst = lib.join(&file);
    if !dst.is_file() {
        let tmp = lib.join(format!("{hash}.tmp"));
        if keep.is_some() {
            fs::write(&tmp, &bytes).map_err(|e| e.to_string())?;
        } else {
            let mut out = fs::File::create(&tmp).map_err(|e| e.to_string())?;
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 95)
                .encode_image(&img.to_rgb8())
                .map_err(|e| e.to_string())?;
        }
        fs::rename(&tmp, &dst).map_err(|e| e.to_string())?;
    }
    make_thumb(&img, &thumbs.join(format!("{hash}.jpg")))?;
    let name = src.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| hash.clone());
    Ok((file, name))
}

#[tauri::command]
pub async fn wallpaper_add(app: AppHandle, paths: Vec<String>) -> Res<AddResult> {
    util::blocking(move || {
        let data = data_of(&app);
        let mut added = 0;
        let mut failed = vec![];
        let mut got = vec![];
        for p in paths.iter().take(200) {
            match import(&data, Path::new(p)) {
                Ok(x) => {
                    got.push(x);
                    added += 1;
                }
                Err(_) => failed.push(Path::new(p).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()),
            }
        }
        let _g = LOCK.lock();
        let mut s = load(&data);
        for (file, name) in got {
            s.names.insert(file, name);
        }
        save(&data, &s)?;
        Ok(AddResult { state: build_state(&data, &s), added, failed })
    })
    .await
}

/// Renames a picture in "My wallpapers" (only the shown name; the file stays as is).
#[tauri::command]
pub async fn wallpaper_rename(app: AppHandle, id: String, name: String) -> Res<WallState> {
    util::blocking(move || {
        let data = data_of(&app);
        let file = id.strip_prefix("u:").filter(|f| valid_file(f)).ok_or_else(|| tr("Обои не найдены", "Wallpaper not found"))?;
        let name: String = name.trim().chars().filter(|c| !c.is_control()).take(60).collect();
        if name.is_empty() {
            return Err(tr("Имя не может быть пустым", "Name cannot be empty"));
        }
        let _g = LOCK.lock();
        if !lib_dir(&data).join(file).is_file() {
            return Err(tr("Обои не найдены", "Wallpaper not found"));
        }
        let mut s = load(&data);
        s.names.insert(file.to_string(), name);
        save(&data, &s)?;
        Ok(build_state(&data, &s))
    })
    .await
}

/// Removes a picture from the StepMaster library (our own copy; the user's original is untouched).
#[tauri::command]
pub async fn wallpaper_remove(app: AppHandle, id: String) -> Res<WallState> {
    util::blocking(move || {
        let data = data_of(&app);
        let file = id.strip_prefix("u:").filter(|f| valid_file(f)).ok_or_else(|| tr("Обои не найдены", "Wallpaper not found"))?;
        let _g = LOCK.lock();
        let path = lib_dir(&data).join(file);
        let mut s = load(&data);
        let was_on_desktop = same_path(&spi_get(), &path.to_string_lossy());
        if path.is_file() {
            keep_if_used(&data, &mut s, &path)?;
            fs::remove_file(&path).map_err(|e| e.to_string())?;
        }
        if let Some(stem) = path.file_stem() {
            let _ = fs::remove_file(thumbs_dir(&data).join(format!("{}.jpg", stem.to_string_lossy())));
            let _ = fs::remove_file(thumbs_dir(&data).join(format!("{}.view.jpg", stem.to_string_lossy())));
        }
        s.names.remove(file);
        if s.last_id.as_deref() == Some(id.as_str()) {
            s.last_id = None;
        }
        save(&data, &s)?;
        // The removed picture was on the desktop: put a random StepMaster wallpaper instead.
        if was_on_desktop {
            let b = &BUILTIN[(seed() % BUILTIN.len() as u64) as usize];
            apply(&data, &mut s, &format!("b:{}", b.key))?;
        }
        sweep(&data, &s);
        Ok(build_state(&data, &s))
    })
    .await
}

/// Puts back the wallpaper that was there before StepMaster changed it; rotation is switched off.
#[tauri::command]
pub async fn wallpaper_restore(app: AppHandle) -> Res<WallState> {
    util::blocking(move || {
        let data = data_of(&app);
        let _g = LOCK.lock();
        let mut s = load(&data);
        restore(&mut s)?;
        s.options.auto = false;
        save(&data, &s)?;
        sweep(&data, &s);
        Ok(build_state(&data, &s))
    })
    .await
}

/// Puts back the wallpaper that was on the desktop when StepMaster started.
/// Uninstall: if a StepMaster picture is on the desktop, put the user's own one back.
pub fn restore_if_ours(app: &AppHandle) {
    let data = data_of(app);
    let _g = LOCK.lock();
    let mut s = load(&data);
    if is_ours(&data, &spi_get()) && restore(&mut s).is_ok() {
        let _ = save(&data, &s);
    }
}

fn restore(s: &mut Saved) -> Res<()> {
    let p = s.previous.clone().ok_or_else(|| tr("Нечего возвращать", "Nothing to restore"))?;
    let original_ok = !p.path.is_empty()
        && Path::new(&p.path).is_file()
        && !p.path.to_ascii_lowercase().contains("transcodedwallpaper");
    let target = if original_ok {
        p.path.clone()
    } else if let Some(b) = p.backup.clone().filter(|b| Path::new(b).is_file()) {
        b
    } else if p.path.is_empty() {
        String::new() // there was no picture (solid colour)
    } else {
        return Err(tr("Прежние обои не найдены", "Previous wallpaper not found"));
    };
    let (style, tile) = fit_values("fill");
    set_style(p.style.as_deref().unwrap_or(style), p.tile.as_deref().unwrap_or(tile))?;
    spi_set(&target)?;
    s.last_id = None;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pick_rules() {
        assert_eq!(pick(0, None, true, 5), 0);
        assert_eq!(pick(1, Some(0), true, 5), 0);
        assert_eq!(pick(4, Some(3), false, 0), 0);
        assert_eq!(pick(4, None, false, 9), 0);
        for seed in 0..50 {
            for c in 0..5 {
                let i = pick(5, Some(c), true, seed);
                assert!(i < 5 && i != c);
            }
        }
    }

    #[test]
    fn ids_are_safe() {
        assert!(valid_file("0123abcd.jpg"));
        assert!(valid_file("a.PNG"));
        for bad in ["", "..\\x.jpg", "a/b.jpg", "C:x.jpg", ".jpg", "x.exe", "x.tmp"] {
            assert!(!valid_file(bad), "{bad}");
        }
        let dir = std::env::temp_dir().join("sm-wall-test");
        assert!(resolve(&dir, "u:..\\..\\secret.jpg").is_err());
        assert!(resolve(&dir, "b:nope").is_err());
        assert!(resolve(&dir, "x:win10").is_err());
        assert!(resolve(&dir, "b:win10").is_ok());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn fits_map() {
        assert_eq!(fit_values("fill"), ("10", "0"));
        assert_eq!(fit_values("fit"), ("6", "0"));
        assert_eq!(fit_values("weird"), ("10", "0"));
        assert!(same_path("C:/A/b.jpg", "c:\\a\\B.JPG"));
        assert!(!same_path("", ""));
    }

    /// Real system: sets a built-in wallpaper, then restores the original. Run with --ignored.
    #[test]
    #[ignore]
    fn live_set_and_restore() {
        let dir = std::env::temp_dir().join("sm-wall-live");
        let mut s = Saved::default();
        let before = spi_get();
        capture_previous(&dir, &mut s);
        apply(&dir, &mut s, "b:cat").unwrap();
        apply(&dir, &mut s, "b:xp").unwrap();
        let st = build_state(&dir, &s);
        println!("before={before} now={} current={:?}", spi_get(), st.current);
        assert_eq!(st.current.as_deref(), Some("b:xp"));
        std::thread::sleep(Duration::from_secs(2));
        assert!(build_state(&dir, &s).can_restore);
        restore(&mut s).unwrap();
        println!("restored={}", spi_get());
        assert!(same_path(&spi_get(), &before) || before.is_empty());
    }
}
