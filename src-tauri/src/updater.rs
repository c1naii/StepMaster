//! Self-update from GitHub Releases. The release must contain `StepMaster.exe` and
//! `StepMaster.exe.sig` (minisign signature made with the private key kept outside the repo).
//! The running exe is renamed to *.old (allowed on Windows), the new one takes its place,
//! then the app restarts. The file keeps whatever name it has on disk.
use crate::util::{self, tr, Res};
use crate::AppState;
use base64::Engine;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

/// "owner/repo" on GitHub. Empty = updates disabled (check returns "no update").
pub const REPO: &str = "c1naii/StepMaster";
/// Public key from `tauri signer generate` (base64 of the minisign public key file).
const PUBKEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDE1MzRDRjZERDRFMEM1NzYKUldSMnhlRFViYzgwRlk2NXV3YzBnVjdpYTM3MjMxN2F4c294cGdZNDBtL3lYZTFtT3lFa05YWVMK";

const EXE_ASSET: &str = "StepMaster.exe";
const SIG_ASSET: &str = "StepMaster.exe.sig";

#[derive(Clone, Default)]
pub struct UpdateInfo {
    exe_url: String,
    sig_url: String,
    notes: Notes,
}

/// Picture shown on top of the update window: a release asset with one of these names.
const BANNER_ASSETS: &[&str] = &["banner.webp", "banner.png", "banner.jpg", "banner.jpeg"];
const BANNER_MAX: u64 = 4 * 1024 * 1024;

/// "What's new" for the update window, taken from the release description on GitHub.
#[derive(Serialize, Clone, Default, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Notes {
    pub sections: Vec<NoteSection>,
    /// Banner picture as a data: URL (None if the release has none).
    pub image: Option<String>,
}

#[derive(Serialize, Clone, Default, Debug, PartialEq)]
pub struct NoteSection {
    pub title: String,
    pub items: Vec<String>,
}

/// Plain text of a markdown line: no **bold**, `code`, [links](url) or leading markers.
fn plain(line: &str) -> String {
    let mut out = String::new();
    let mut rest = line;
    while let Some(i) = rest.find('[') {
        let Some(j) = rest[i..].find("](") else { break };
        let Some(k) = rest[i + j..].find(')') else { break };
        out.push_str(&rest[..i]);
        out.push_str(&rest[i + 1..i + j]);
        rest = &rest[i + j + k + 1..];
    }
    out.push_str(rest);
    out.replace("**", "").replace("__", "").replace('`', "").trim().to_string()
}

/// Release description → short sections. Headings ("## Новое") start a section, list lines
/// ("- ...") are its items. Text before the first heading (download instructions for the GitHub
/// page) is skipped when there are headings, and everything after a "---" line is for GitHub only.
pub fn parse_notes(body: &str) -> Vec<NoteSection> {
    let is_heading = |l: &str| l.starts_with('#') && l.trim_start_matches('#').starts_with(' ');
    let has_headings = body.lines().any(|l| is_heading(l.trim()));
    let mut out: Vec<NoteSection> = Vec::new();
    for raw in body.lines() {
        let line = raw.trim();
        if line.starts_with("---") {
            break;
        }
        if line.is_empty() || line.starts_with("<!--") || line.starts_with("![") {
            continue;
        }
        if has_headings && out.is_empty() && !is_heading(line) {
            continue;
        }
        if let Some(h) = line.strip_prefix('#').filter(|_| is_heading(line)) {
            out.push(NoteSection { title: plain(h.trim_start_matches('#')), items: Vec::new() });
            continue;
        }
        let item = ["- ", "* ", "• ", "+ "]
            .iter()
            .find_map(|m| line.strip_prefix(m))
            .or_else(|| line.split_once(". ").filter(|(n, _)| n.chars().all(|c| c.is_ascii_digit())).map(|(_, t)| t))
            .unwrap_or(line);
        let text: String = plain(item).chars().take(160).collect();
        if text.is_empty() {
            continue;
        }
        if out.is_empty() {
            out.push(NoteSection::default());
        }
        out.last_mut().unwrap().items.push(text);
    }
    out.retain(|s| !s.items.is_empty());
    out.truncate(4);
    for s in &mut out {
        s.items.truncate(8);
    }
    out
}

/// Downloads the banner picture (small, image types only) as a data: URL.
async fn banner(url: &str, name: &str) -> Option<String> {
    use base64::Engine;
    let mime = match name.rsplit('.').next()?.to_ascii_lowercase().as_str() {
        "webp" => "image/webp",
        "png" => "image/png",
        _ => "image/jpeg",
    };
    let resp = client().ok()?.get(url).timeout(Duration::from_secs(20)).send().await.ok()?;
    if !resp.status().is_success() || resp.content_length().is_some_and(|n| n > BANNER_MAX) {
        return None;
    }
    // Read in chunks and stop at the limit (the server may not send a size up front).
    let mut resp = resp;
    let mut bytes: Vec<u8> = Vec::new();
    while let Some(chunk) = resp.chunk().await.ok()? {
        bytes.extend_from_slice(&chunk);
        if bytes.len() as u64 > BANNER_MAX {
            return None;
        }
    }
    // Only real pictures: PNG, JPEG or WebP by their first bytes.
    let ok = bytes.starts_with(&[0x89, b'P', b'N', b'G']) || bytes.starts_with(&[0xFF, 0xD8, 0xFF]) || (bytes.len() > 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP");
    if !ok {
        return None;
    }
    Some(format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(&bytes)))
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<Asset>,
    #[serde(default)]
    body: Option<String>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

/// Compares dotted numeric versions ("1.10.0" > "1.9.3"). Non-numeric parts count as 0.
pub fn is_newer(remote: &str, local: &str) -> bool {
    let parse = |v: &str| -> Vec<u64> {
        v.trim()
            .trim_start_matches(['v', 'V'])
            .split(['.', '-', '+'])
            .take(3)
            .map(|p| p.parse::<u64>().unwrap_or(0))
            .collect()
    };
    let (mut r, mut l) = (parse(remote), parse(local));
    r.resize(3, 0);
    l.resize(3, 0);
    r > l
}

pub(crate) fn client() -> Res<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent("StepMaster-Updater")
        // No total timeout: a 25 MB exe on a slow line takes longer than any fixed limit.
        // A stalled connection is still cut off by the per-read timeout.
        .connect_timeout(Duration::from_secs(20))
        .read_timeout(Duration::from_secs(60))
        .build()
        .map_err(|e| e.to_string())
}

#[derive(Serialize)]
pub struct Check {
    available: bool,
}

/// Latest GitHub release newer than `local`, with both assets present. None = up to date.
pub async fn find_update(local: &str) -> Res<Option<UpdateInfo>> {
    if REPO.is_empty() {
        return Ok(None);
    }
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let resp = client()?
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| format!("{}: {e}", tr("Нет связи с GitHub", "Can't reach GitHub")))?;
    if resp.status().as_u16() == 404 {
        return Ok(None); // no releases yet
    }
    if !resp.status().is_success() {
        return Err(format!("GitHub: {}", resp.status()));
    }
    let rel: Release = resp.json().await.map_err(|e| e.to_string())?;
    if rel.draft || rel.prerelease || !is_newer(&rel.tag_name, local) {
        return Ok(None);
    }
    let find = |n: &str| rel.assets.iter().find(|a| a.name.eq_ignore_ascii_case(n)).map(|a| a.browser_download_url.clone());
    let (Some(exe_url), Some(sig_url)) = (find(EXE_ASSET), find(SIG_ASSET)) else {
        return Ok(None);
    };
    let mut notes = Notes { sections: parse_notes(rel.body.as_deref().unwrap_or("")), image: None };
    if let Some((name, url)) = BANNER_ASSETS.iter().find_map(|n| find(n).map(|u| (*n, u))) {
        notes.image = banner(&url, name).await;
    }
    Ok(Some(UpdateInfo { exe_url, sig_url, notes }))
}

// ---------- "What's new" after an update ----------

fn seen_path(data: &Path) -> PathBuf {
    data.join("seen-version.txt")
}

#[derive(Serialize)]
pub struct WhatsNew {
    version: String,
    notes: Notes,
}

/// Shown once after the program was updated (also from old versions whose update window had
/// no notes). A fresh install shows nothing: no settings file yet means it never ran before.
#[tauri::command]
pub async fn whats_new(app: AppHandle) -> Res<Option<WhatsNew>> {
    let data = app.state::<AppState>().data_dir.clone();
    let cur = env!("CARGO_PKG_VERSION");
    let seen = std::fs::read_to_string(seen_path(&data)).ok().map(|s| s.trim().to_string());
    let ran_before = data.join("settings.json").is_file();
    match seen.as_deref() {
        Some(v) if !is_newer(cur, v) => return Ok(None),
        None if !ran_before => {
            let _ = std::fs::write(seen_path(&data), cur);
            return Ok(None);
        }
        _ => {}
    }
    if REPO.is_empty() {
        return Ok(None);
    }
    // Offline: try again next start (nothing is marked as seen).
    let url = format!("https://api.github.com/repos/{REPO}/releases/tags/v{cur}");
    let resp = client()?
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        let _ = std::fs::write(seen_path(&data), cur);
        return Ok(None);
    }
    let rel: Release = resp.json().await.map_err(|e| e.to_string())?;
    let mut notes = Notes { sections: parse_notes(rel.body.as_deref().unwrap_or("")), image: None };
    if notes.sections.iter().all(|s| s.items.is_empty()) {
        let _ = std::fs::write(seen_path(&data), cur);
        return Ok(None);
    }
    let find = |n: &str| rel.assets.iter().find(|a| a.name.eq_ignore_ascii_case(n)).map(|a| a.browser_download_url.clone());
    if let Some((name, url)) = BANNER_ASSETS.iter().find_map(|n| find(n).map(|u| (*n, u))) {
        notes.image = banner(&url, name).await;
    }
    Ok(Some(WhatsNew { version: cur.to_string(), notes }))
}

/// The "What's new" window was closed: don't show it again for this version.
#[tauri::command]
pub fn whats_new_seen(state: tauri::State<'_, AppState>) {
    let _ = std::fs::write(seen_path(&state.data_dir), env!("CARGO_PKG_VERSION"));
}

/// "What's new" of the update found by the last check (empty if none).
#[tauri::command]
pub fn update_notes(state: tauri::State<'_, AppState>) -> Notes {
    state.update.lock().as_ref().map(|u| u.notes.clone()).unwrap_or_default()
}

#[tauri::command]
pub async fn update_check(app: AppHandle) -> Res<Check> {
    let found = find_update(env!("CARGO_PKG_VERSION")).await?;
    let available = found.is_some();
    *app.state::<AppState>().update.lock() = found;
    Ok(Check { available })
}

/// Downloads the new exe and its signature; returns the bytes only if the signature is valid.
pub async fn download_verified(info: &UpdateInfo, on_progress: impl Fn(f64)) -> Res<Vec<u8>> {
    let c = client()?;
    let net = |e: reqwest::Error| format!("{}: {e}", tr("Ошибка загрузки", "Download error"));
    let sig_resp = c.get(&info.sig_url).send().await.map_err(net)?;
    if !sig_resp.status().is_success() {
        return Err(tr("Файл обновления недоступен", "Update file is unavailable"));
    }
    let sig = sig_resp.text().await.map_err(net)?;
    let resp = c.get(&info.exe_url).send().await.map_err(net)?;
    if !resp.status().is_success() {
        return Err(tr("Файл обновления недоступен", "Update file is unavailable"));
    }
    const MAX_SIZE: u64 = 150 * 1024 * 1024;
    let total = resp.content_length().unwrap_or(0);
    if total > MAX_SIZE {
        return Err(tr("Файл обновления слишком большой", "Update file is too large"));
    }
    let mut data: Vec<u8> = Vec::with_capacity(total as usize);
    let mut stream = resp.bytes_stream();
    let mut last = 0.0;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(net)?;
        data.extend_from_slice(&chunk);
        if data.len() as u64 > MAX_SIZE {
            return Err(tr("Файл обновления слишком большой", "Update file is too large"));
        }
        if total > 0 {
            let p = (data.len() as f64 / total as f64 * 100.0).min(100.0);
            if p - last >= 1.0 {
                last = p;
                on_progress(p);
            }
        }
    }
    verify(&data, &sig)?;
    // A real Windows executable starts with "MZ".
    if data.len() < 1024 || &data[0..2] != b"MZ" {
        return Err(tr("Файл обновления повреждён", "Update file is corrupted"));
    }
    Ok(data)
}

fn verify(data: &[u8], sig_b64: &str) -> Res<()> {
    let b64 = base64::engine::general_purpose::STANDARD;
    let bad = || tr("Подпись обновления не прошла проверку", "Update signature check failed");
    let pk_text = String::from_utf8(b64.decode(PUBKEY).map_err(|_| bad())?).map_err(|_| bad())?;
    let sig_text = String::from_utf8(b64.decode(sig_b64.trim()).map_err(|_| bad())?).map_err(|_| bad())?;
    let pk = minisign_verify::PublicKey::decode(&pk_text).map_err(|_| bad())?;
    let sig = minisign_verify::Signature::decode(&sig_text).map_err(|_| bad())?;
    pk.verify(data, &sig, false).map_err(|_| bad())
}

#[derive(Serialize, Clone)]
struct Progress {
    percent: f64,
}

#[tauri::command]
pub async fn update_install(app: AppHandle) -> Res<()> {
    // A double click on "Update" must not run two downloads/swaps at once.
    static UPDATING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if UPDATING.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return Err(tr("Обновление уже идёт", "Update is already in progress"));
    }
    let r = update_install_inner(&app).await;
    UPDATING.store(false, std::sync::atomic::Ordering::SeqCst);
    r?;
    app.exit(0);
    Ok(())
}

async fn update_install_inner(app: &AppHandle) -> Res<()> {
    let info = app
        .state::<AppState>()
        .update
        .lock()
        .clone()
        .ok_or_else(|| tr("Обновление не найдено — проверьте ещё раз", "No update found — check again"))?;
    let app2 = app.clone();
    let data = download_verified(&info, move |p| {
        let _ = app2.emit("update://progress", Progress { percent: p });
    })
    .await?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    util::blocking(move || swap_and_restart(&exe, &data)).await
}

fn sibling(exe: &Path, ext: &str) -> PathBuf {
    let mut s = exe.as_os_str().to_owned();
    s.push(ext);
    PathBuf::from(s)
}

fn swap_and_restart(exe: &Path, data: &[u8]) -> Res<()> {
    let new = sibling(exe, ".new");
    let old = sibling(exe, ".old");
    {
        let mut f = std::fs::File::create(&new).map_err(|e| format!("{}: {e}", tr("Нет доступа к папке программы", "Can't write to the program folder")))?;
        f.write_all(data).map_err(|e| e.to_string())?;
        f.sync_all().ok();
    }
    let _ = std::fs::remove_file(&old);
    std::fs::rename(exe, &old).map_err(|e| {
        let _ = std::fs::remove_file(&new);
        e.to_string()
    })?;
    if let Err(e) = std::fs::rename(&new, exe) {
        let _ = std::fs::rename(&old, exe); // roll back
        return Err(e.to_string());
    }
    let arg = format!("--wait-pid={}", std::process::id());
    if std::process::Command::new(exe).arg(&arg).spawn().is_err() {
        // Not elevated (dev run): ask Windows to elevate.
        util::shell_open(&exe.to_string_lossy(), Some(&arg), "runas")?;
    }
    Ok(())
}

/// Remove the previous version left after an update.
pub fn cleanup_old() {
    if let Ok(exe) = std::env::current_exe() {
        let old = sibling(&exe, ".old");
        for _ in 0..5 {
            if !old.exists() || std::fs::remove_file(&old).is_ok() {
                break;
            }
            std::thread::sleep(Duration::from_millis(500));
        }
        let _ = std::fs::remove_file(sibling(&exe, ".new"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_notes_are_parsed() {
        let body = "## Новое
- Кнопка **Остановить** у долгих операций
- [Журнал](https://x) изменений

## Исправлено
* Обои не пропадают
1. Быстрее диагностика
<!-- hidden -->
## Пусто
";
        let n = parse_notes(body);
        assert_eq!(n.len(), 2);
        assert_eq!(n[0].title, "Новое");
        assert_eq!(n[0].items, vec!["Кнопка Остановить у долгих операций", "Журнал изменений"]);
        assert_eq!(n[1].items, vec!["Обои не пропадают", "Быстрее диагностика"]);
        let with_intro = parse_notes("Скачай exe
## Новое
- Журнал
---
Как установить: ...
- не показывать");
        assert_eq!(with_intro.len(), 1);
        assert_eq!(with_intro[0].items, vec!["Журнал"]);
        let plain_text = parse_notes("Просто текст без заголовков");
        assert_eq!(plain_text[0].title, "");
        assert_eq!(plain_text[0].items.len(), 1);
    }
    #[test]
    fn versions() {
        assert!(is_newer("v1.0.1", "1.0.0"));
        assert!(is_newer("1.10.0", "1.9.9"));
        assert!(!is_newer("1.0.0", "1.0.0"));
        assert!(!is_newer("v0.9.0", "1.0.0"));
        assert!(!is_newer("garbage", "1.0.0"));
    }
    #[test]
    fn rejects_bad_signature() {
        assert!(verify(b"hello", "bm90IGEgc2lnbmF0dXJl").is_err());
    }

    /// Live check against the published GitHub release: finds it, downloads it, verifies the
    /// signature. Does not replace anything. Run: cargo test --lib live_github -- --ignored --nocapture
    #[test]
    #[ignore]
    fn live_github_update_pipeline() {
        use tauri::async_runtime::block_on;
        assert!(block_on(find_update(env!("CARGO_PKG_VERSION"))).unwrap().is_none(), "current version must be up to date");
        let info = block_on(find_update("0.0.1")).unwrap().expect("an older version must see the update");
        let bytes = block_on(download_verified(&info, |_| {})).expect("download + signature");
        println!("downloaded {} bytes from {}", bytes.len(), info.exe_url);
    }

    /// Checks the real signed build in /release against the embedded public key.
    /// Run after `npm run release`: cargo test --lib updater -- --ignored
    #[test]
    #[ignore]
    fn release_build_signature_is_valid() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("release");
        let exe = std::fs::read(dir.join(EXE_ASSET)).expect("release exe");
        let sig = std::fs::read_to_string(dir.join(SIG_ASSET)).expect("sig");
        verify(&exe, &sig).expect("signature must verify");
        let mut tampered = exe.clone();
        let n = tampered.len() / 2;
        tampered[n] ^= 0xFF;
        assert!(verify(&tampered, &sig).is_err(), "tampered file must be rejected");
    }
}
