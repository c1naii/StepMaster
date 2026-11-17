//! Game mode: reversible tweaks with saved previous values; per-game GPU preference.
use crate::settings::write_json;
use crate::system::{is_critical, list_processes, ProcessInfo};
use crate::util::{self, tr, Res};
use crate::AppState;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager, State};
use winreg::enums::*;
use winreg::{RegKey, HKEY};

const HIGH_PERF: &str = "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c";
const GAMEBAR: &str = r"Software\Microsoft\GameBar";
const PERSONALIZE: &str = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";
const WU_UX: &str = r"SOFTWARE\Microsoft\WindowsUpdate\UX\Settings";
const WU_VALUES: &[&str] = &[
    "PauseUpdatesStartTime",
    "PauseUpdatesExpiryTime",
    "PauseQualityUpdatesStartTime",
    "PauseQualityUpdatesEndTime",
    "PauseFeatureUpdatesStartTime",
    "PauseFeatureUpdatesEndTime",
];

#[derive(Serialize, Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase", default)]
struct Saved {
    active: bool,
    since: Option<u64>,
    applied: Vec<String>,
    power_scheme: Option<String>,
    /// Power plan we created by duplicating "High performance" (deleted on revert).
    created_scheme: Option<String>,
    auto_game_mode: Option<PrevDword>,
    transparency: Option<PrevDword>,
    client_anim: Option<bool>,
    wu_values: Option<HashMap<String, String>>,
}

/// Previous registry value, explicitly recording "did not exist" (so revert deletes it).
#[derive(Serialize, Deserialize, Default, Clone, Copy, PartialEq, Debug)]
#[serde(rename_all = "camelCase", default)]
struct PrevDword {
    existed: bool,
    value: u32,
}

impl PrevDword {
    fn of(v: Option<u32>) -> Self {
        Self { existed: v.is_some(), value: v.unwrap_or(0) }
    }
    fn restore(self) -> Option<u32> {
        self.existed.then_some(self.value)
    }
}

fn state_path(dir: &Path) -> PathBuf {
    dir.join("game-mode.json")
}

pub fn is_active(dir: &Path) -> bool {
    load(dir).active
}

fn load(dir: &Path) -> Saved {
    std::fs::read_to_string(state_path(dir)).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

pub(crate) fn is_laptop() -> bool {
    use windows_sys::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
    let mut s: SYSTEM_POWER_STATUS = unsafe { std::mem::zeroed() };
    if unsafe { GetSystemPowerStatus(&mut s) } == 0 {
        return false;
    }
    // 128 = no system battery, 255 = unknown.
    s.BatteryFlag != 128 && s.BatteryFlag != 255
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    active: bool,
    since: Option<u64>,
    applied: Vec<String>,
    is_laptop: bool,
}

fn status_of(s: &Saved) -> Status {
    Status { active: s.active, since: s.since, applied: s.applied.clone(), is_laptop: is_laptop() }
}

#[tauri::command]
pub fn game_status(state: State<'_, AppState>) -> Status {
    status_of(&load(&state.data_dir))
}

fn active_scheme() -> Option<String> {
    let (ok, out, _) = util::run_hidden("powercfg.exe", &["/getactivescheme"]).ok()?;
    if !ok {
        return None;
    }
    find_guid(&out)
}

/// First GUID-looking token (8-4-4-4-12 hex) in text.
pub fn find_guid(text: &str) -> Option<String> {
    let b = text.as_bytes();
    if b.len() < 36 {
        return None;
    }
    for i in 0..=b.len() - 36 {
        if !text.is_char_boundary(i) || !text.is_char_boundary(i + 36) {
            continue;
        }
        let cand = &text[i..i + 36];
        let ok = cand.char_indices().all(|(j, c)| if [8, 13, 18, 23].contains(&j) { c == '-' } else { c.is_ascii_hexdigit() });
        if ok {
            return Some(cand.to_lowercase());
        }
    }
    None
}

fn set_scheme(guid: &str) -> bool {
    util::run_hidden("powercfg.exe", &["/setactive", guid]).map(|r| r.0).unwrap_or(false)
}

/// Activates High performance. Returns Some(guid) if a copy had to be created (to delete later).
fn enable_high_performance() -> Res<Option<String>> {
    if set_scheme(HIGH_PERF) {
        return Ok(None);
    }
    // Hidden on many laptops (Modern Standby): create a copy, then activate it.
    let (ok, out, _) = util::run_hidden("powercfg.exe", &["-duplicatescheme", HIGH_PERF])?;
    if ok {
        if let Some(g) = find_guid(&out.to_lowercase().replace(HIGH_PERF, "")) {
            if set_scheme(&g) {
                return Ok(Some(g));
            }
            let _ = util::run_hidden("powercfg.exe", &["/delete", &g]);
        }
    }
    Err(tr("План «Высокая производительность» недоступен на этом ПК", "High performance plan isn't available on this PC"))
}

fn get_dword(hive: HKEY, path: &str, name: &str) -> Option<u32> {
    RegKey::predef(hive).open_subkey(path).ok()?.get_value::<u32, _>(name).ok()
}

fn set_dword(hive: HKEY, path: &str, name: &str, v: Option<u32>) -> Res<()> {
    let (k, _) = RegKey::predef(hive).create_subkey(path).map_err(|e| e.to_string())?;
    match v {
        Some(v) => k.set_value(name, &v).map_err(|e| e.to_string()),
        None => {
            let _ = k.delete_value(name);
            Ok(())
        }
    }
}

fn client_animation() -> Option<bool> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{SystemParametersInfoW, SPI_GETCLIENTAREAANIMATION};
    let mut v: i32 = 0;
    let ok = unsafe { SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION, 0, &mut v as *mut _ as *mut _, 0) };
    (ok != 0).then_some(v != 0)
}

fn set_client_animation(on: bool) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SystemParametersInfoW, SPIF_SENDCHANGE, SPIF_UPDATEINIFILE, SPI_SETCLIENTAREAANIMATION,
    };
    unsafe {
        SystemParametersInfoW(
            SPI_SETCLIENTAREAANIMATION,
            0,
            on as usize as *mut _,
            SPIF_UPDATEINIFILE | SPIF_SENDCHANGE,
        );
    }
}

fn iso(t: SystemTime) -> String {
    let secs = t.duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let days = (secs / 86400) as i64;
    let rem = secs % 86400;
    let date = crate::programs::civil_from_days(days);
    format!("{date}T{:02}:{:02}:{:02}Z", rem / 3600, (rem % 3600) / 60, rem % 60)
}

fn read_wu() -> HashMap<String, String> {
    let mut m = HashMap::new();
    if let Ok(k) = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey_with_flags(WU_UX, KEY_READ | KEY_WOW64_64KEY) {
        for n in WU_VALUES {
            if let Ok(v) = k.get_value::<String, _>(n) {
                m.insert((*n).to_string(), v);
            }
        }
    }
    m
}

/// Only well-formed pause dates no further than 40 days ahead are ever written
/// (game-mode.json lives in a user-writable folder, so its values are not trusted).
fn valid_pause_value(v: &str) -> bool {
    let b = v.as_bytes();
    let shape = b.len() == 20
        && b.iter().enumerate().all(|(i, c)| match i {
            4 | 7 => *c == b'-',
            10 => *c == b'T',
            13 | 16 => *c == b':',
            19 => *c == b'Z',
            _ => c.is_ascii_digit(),
        });
    if !shape {
        return false;
    }
    let limit = iso(SystemTime::now() + std::time::Duration::from_secs(40 * 86400));
    v <= limit.as_str()
}

fn write_wu(values: &HashMap<String, String>) -> Res<()> {
    let (k, _) = RegKey::predef(HKEY_LOCAL_MACHINE)
        .create_subkey_with_flags(WU_UX, KEY_READ | KEY_WRITE | KEY_WOW64_64KEY)
        .map_err(|e| e.to_string())?;
    for n in WU_VALUES {
        match values.get(*n).filter(|v| valid_pause_value(v)) {
            Some(v) => k.set_value(n, v).map_err(|e| e.to_string())?,
            None => {
                let _ = k.delete_value(n);
            }
        }
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Options {
    power_plan: bool,
    game_mode: bool,
    visual_effects: bool,
    pause_updates: bool,
    close_apps: Vec<String>,
}

#[tauri::command]
pub async fn game_apply(app: AppHandle, options: Options) -> Res<Status> {
    if !util::is_admin() {
        return Err(util::need_admin());
    }
    util::blocking(move || {
        let st = app.state::<AppState>();
        let dir = st.data_dir.clone();
        let mut saved = load(&dir);
        if saved.active {
            return Ok(status_of(&saved));
        }
        saved = Saved {
            active: true,
            since: Some(SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)),
            ..Saved::default()
        };
        let path = state_path(&dir);
        let mut errors = Vec::new();
        // The previous value is saved BEFORE each change, so a crash mid-way can still be reverted.
        let persist = |s: &Saved| write_json(&path, s);

        if options.power_plan {
            saved.power_scheme = active_scheme();
            persist(&saved)?;
            match enable_high_performance() {
                Ok(created) => {
                    saved.created_scheme = created;
                    saved.applied.push("powerPlan".into());
                }
                Err(e) => errors.push(e),
            }
        }
        if options.game_mode {
            saved.auto_game_mode = Some(PrevDword::of(get_dword(HKEY_CURRENT_USER, GAMEBAR, "AutoGameModeEnabled")));
            persist(&saved)?;
            match set_dword(HKEY_CURRENT_USER, GAMEBAR, "AutoGameModeEnabled", Some(1)) {
                Ok(()) => saved.applied.push("gameMode".into()),
                Err(e) => errors.push(e),
            }
        }
        if options.visual_effects {
            saved.transparency = Some(PrevDword::of(get_dword(HKEY_CURRENT_USER, PERSONALIZE, "EnableTransparency")));
            saved.client_anim = client_animation();
            persist(&saved)?;
            let _ = set_dword(HKEY_CURRENT_USER, PERSONALIZE, "EnableTransparency", Some(0));
            set_client_animation(false);
            saved.applied.push("visualEffects".into());
        }
        if options.pause_updates {
            let prev = read_wu();
            saved.wu_values = Some(prev.clone());
            persist(&saved)?;
            let now = SystemTime::now();
            let until = now + std::time::Duration::from_secs(24 * 3600);
            let mut v = HashMap::new();
            for (a, b) in [
                ("PauseUpdatesStartTime", now),
                ("PauseUpdatesExpiryTime", until),
                ("PauseQualityUpdatesStartTime", now),
                ("PauseQualityUpdatesEndTime", until),
                ("PauseFeatureUpdatesStartTime", now),
                ("PauseFeatureUpdatesEndTime", until),
            ] {
                v.insert(a.to_string(), iso(b));
            }
            match write_wu(&v) {
                Ok(()) => saved.applied.push("pauseUpdates".into()),
                Err(e) => errors.push(e),
            }
        }
        if !options.close_apps.is_empty() {
            let wanted: Vec<String> = options.close_apps.iter().map(|n| n.to_lowercase()).collect();
            for p in list_processes(&st) {
                let n = p.name.to_lowercase();
                if wanted.contains(&n) && !is_critical(&n) {
                    let _ = crate::system::kill_pid(&st, p.pid);
                }
            }
        }
        persist(&saved)?;
        if saved.applied.is_empty() && !errors.is_empty() {
            // Nothing worked — don't pretend game mode is on.
            let _ = std::fs::remove_file(&path);
            return Err(errors.join("; "));
        }
        crate::journal::add_at(&dir, "game_mode", "", "", Some(crate::journal::Undo::GameMode));
        Ok(status_of(&saved))
    })
    .await
}

#[tauri::command]
pub async fn game_revert(app: AppHandle) -> Res<Status> {
    let dir = app.state::<AppState>().data_dir.clone();
    util::blocking(move || revert_at(&dir)).await
}

/// Puts back everything game mode changed (also used by uninstall).
pub fn revert_at(dir: &std::path::Path) -> Res<Status> {
    {
        let dir = dir.to_path_buf();
        let saved = load(&dir);
        if !saved.active {
            return Ok(status_of(&saved));
        }
        if !util::is_admin() {
            return Err(util::need_admin());
        }
        let mut errors = Vec::new();
        // What is still not restored; written back on failure so "Revert" can be retried.
        let mut left = saved.clone();
        // Restore whatever was saved, even if the step didn't finish (crash mid-apply).
        if let Some(g) = &saved.power_scheme {
            if find_guid(g).is_some() && !set_scheme(g) {
                errors.push(tr("Не удалось вернуть план питания", "Couldn't restore the power plan"));
            } else {
                left.power_scheme = None;
            }
        }
        if let Some(created) = &saved.created_scheme {
            // Never delete the plan we may still be running on.
            if left.power_scheme.is_none() {
                if find_guid(created).is_some() && saved.power_scheme.as_deref() != Some(created.as_str()) {
                    let _ = util::run_hidden("powercfg.exe", &["/delete", created]);
                }
                left.created_scheme = None;
            }
        }
        if let Some(prev) = saved.auto_game_mode {
            match set_dword(HKEY_CURRENT_USER, GAMEBAR, "AutoGameModeEnabled", prev.restore()) {
                Ok(()) => left.auto_game_mode = None,
                Err(e) => errors.push(e),
            }
        }
        if let Some(prev) = saved.transparency {
            let _ = set_dword(HKEY_CURRENT_USER, PERSONALIZE, "EnableTransparency", prev.restore());
            left.transparency = None;
        }
        if let Some(prev) = saved.client_anim {
            set_client_animation(prev);
            left.client_anim = None;
        }
        if let Some(prev) = &saved.wu_values {
            match write_wu(prev) {
                Ok(()) => left.wu_values = None,
                Err(e) => errors.push(e),
            }
        }
        if !errors.is_empty() {
            write_json(&state_path(&dir), &left)?;
            return Err(errors.join("; "));
        }
        let cleared = Saved::default();
        write_json(&state_path(&dir), &cleared)?;
        crate::journal::mark_reverted(&dir, |u| matches!(u, crate::journal::Undo::GameMode));
        Ok(status_of(&cleared))
    }
}

/// Known heavy background apps + anything above 500 MB that isn't a system process.
const HEAVY: &[&str] = &[
    "chrome.exe", "msedge.exe", "firefox.exe", "opera.exe", "browser.exe", "brave.exe", "vivaldi.exe",
    "discord.exe", "telegram.exe", "teams.exe", "ms-teams.exe", "onedrive.exe", "spotify.exe", "slack.exe",
    "skype.exe", "zoom.exe", "viber.exe", "whatsapp.exe", "qbittorrent.exe", "utorrent.exe", "creative cloud.exe",
    "adobecollabsync.exe", "dropbox.exe", "yandexdisk2.exe", "googledrivefs.exe",
];

#[tauri::command]
pub async fn game_candidates(app: AppHandle) -> Res<Vec<ProcessInfo>> {
    util::blocking(move || Ok(candidates(&app.state::<AppState>()))).await
}

fn candidates(state: &AppState) -> Vec<ProcessInfo> {
    let mut by_name: HashMap<String, ProcessInfo> = HashMap::new();
    for p in list_processes(state) {
        let n = p.name.to_lowercase();
        if is_critical(&n) || n == "explorer.exe" || n.starts_with("stepmaster") {
            continue;
        }
        by_name
            .entry(n)
            .and_modify(|e| {
                e.memory += p.memory;
                e.cpu += p.cpu;
            })
            .or_insert(p);
    }
    let mut out: Vec<ProcessInfo> = by_name
        .into_iter()
        .filter(|(n, p)| {
            HEAVY.contains(&n.as_str())
                || (p.memory > 500 * 1024 * 1024
                    && p.exe.as_deref().map(|e| !e.to_lowercase().contains(r"\windows\")).unwrap_or(false))
        })
        .map(|(_, p)| p)
        .collect();
    out.sort_by_key(|x| std::cmp::Reverse(x.memory));
    out
}

const GPU_KEY: &str = r"Software\Microsoft\DirectX\UserGpuPreferences";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuPref {
    exe: String,
    pref: u32,
}

#[tauri::command]
pub fn gpu_prefs() -> Vec<GpuPref> {
    let Ok(k) = RegKey::predef(HKEY_CURRENT_USER).open_subkey(GPU_KEY) else { return vec![] };
    let mut out: Vec<GpuPref> = k
        .enum_values()
        .flatten()
        .filter_map(|(name, _)| {
            let data: String = k.get_value(&name).ok()?;
            let pref = data
                .split(';')
                .find_map(|kv| kv.strip_prefix("GpuPreference="))
                .and_then(|v| v.trim().parse::<u32>().ok())?;
            name.to_lowercase().ends_with(".exe").then_some(GpuPref { exe: name, pref })
        })
        .collect();
    out.sort_by_key(|g| g.exe.to_lowercase());
    out
}

#[tauri::command]
pub fn gpu_pref_set(app: AppHandle, exe: String, pref: u32) -> Res<()> {
    let p = Path::new(&exe);
    if !p.is_absolute() || !exe.to_lowercase().ends_with(".exe") || !p.is_file() || pref > 2 {
        return Err(tr("Выберите .exe файл игры", "Choose the game's .exe file"));
    }
    let (k, _) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(GPU_KEY).map_err(|e| e.to_string())?;
    let old = k.get_value::<String, _>(&exe).ok();
    let v = merge_gpu(old.as_deref(), Some(pref)).unwrap_or_default();
    k.set_value(&exe, &v).map_err(|e| e.to_string())?;
    crate::journal::add(&app, "gpu_pref", &exe, &pref.to_string(), Some(crate::journal::Undo::GpuPref { exe: exe.clone(), value: old }));
    Ok(())
}

/// Journal undo: puts the exact previous registry value back (or removes it if there was none).
pub fn gpu_pref_restore(exe: &str, value: Option<&str>) -> Res<()> {
    let (k, _) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(GPU_KEY).map_err(|e| e.to_string())?;
    match value {
        Some(v) => k.set_value(exe, &v.to_string()).map_err(|e| e.to_string()),
        None => match k.delete_value(exe) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
            _ => Ok(()),
        },
    }
}

/// The same registry value also holds other Windows graphics settings for this exe
/// (VRR, windowed-game optimisations, Auto HDR): change only the GpuPreference token.
fn merge_gpu(old: Option<&str>, pref: Option<u32>) -> Option<String> {
    let mut parts: Vec<String> = old
        .unwrap_or("")
        .split(';')
        .map(str::trim)
        .filter(|t| !t.is_empty() && !t.to_ascii_lowercase().starts_with("gpupreference="))
        .map(String::from)
        .collect();
    if let Some(p) = pref {
        parts.insert(0, format!("GpuPreference={p}"));
    }
    (!parts.is_empty()).then(|| parts.iter().map(|t| format!("{t};")).collect())
}

#[tauri::command]
pub fn gpu_pref_remove(app: AppHandle, exe: String) -> Res<()> {
    let k = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(GPU_KEY, KEY_READ | KEY_WRITE)
        .map_err(|e| e.to_string())?;
    let old = k.get_value::<String, _>(&exe).ok();
    match merge_gpu(old.as_deref(), None) {
        Some(rest) => k.set_value(&exe, &rest).map_err(|e| e.to_string())?,
        None => k.delete_value(&exe).map_err(|e| e.to_string())?,
    }
    crate::journal::add(&app, "gpu_pref_remove", &exe, "", Some(crate::journal::Undo::GpuPref { exe: exe.clone(), value: old }));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gpu_tokens_kept() {
        assert_eq!(merge_gpu(None, Some(2)).as_deref(), Some("GpuPreference=2;"));
        assert_eq!(merge_gpu(Some("SwapEffectUpgradeEnable=1;GpuPreference=1;"), Some(2)).as_deref(), Some("GpuPreference=2;SwapEffectUpgradeEnable=1;"));
        assert_eq!(merge_gpu(Some("GpuPreference=1;VRROptimizeEnable=0;"), None).as_deref(), Some("VRROptimizeEnable=0;"));
        assert_eq!(merge_gpu(Some("GpuPreference=1;"), None), None);
    }
    #[test]
    fn guid() {
        assert_eq!(
            find_guid("Power Scheme GUID: 381b4222-f694-41f0-9685-ff5bb260df2e  (Balanced)").as_deref(),
            Some("381b4222-f694-41f0-9685-ff5bb260df2e")
        );
        assert_eq!(find_guid("nothing here"), None);
        assert_eq!(find_guid("Схема питания: 8C5E7FDA-E8BF-4A96-9A85-A6E23A8C635C").as_deref(), Some(HIGH_PERF));
    }
    #[test]
    fn iso_format() {
        assert_eq!(iso(UNIX_EPOCH), "1970-01-01T00:00:00Z");
    }
    #[test]
    fn prev_value_round_trip_keeps_missing() {
        let s = Saved { auto_game_mode: Some(PrevDword::of(None)), transparency: Some(PrevDword::of(Some(1))), ..Saved::default() };
        let back: Saved = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(back.auto_game_mode.unwrap().restore(), None); // was absent -> delete on revert
        assert_eq!(back.transparency.unwrap().restore(), Some(1));
    }

    #[test]
    fn pause_values_validated() {
        let soon = iso(SystemTime::now() + std::time::Duration::from_secs(86400));
        assert!(valid_pause_value(&soon));
        assert!(!valid_pause_value("2099-01-01T00:00:00Z"));
        assert!(!valid_pause_value("garbage"));
        assert!(!valid_pause_value("2026-11-16T17:00:00"));
    }
}
