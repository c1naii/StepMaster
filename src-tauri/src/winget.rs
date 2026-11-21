//! Programs → "Install": installs popular apps through winget (Windows Package Manager).
//! winget downloads the official installer from the vendor, checks its hash and runs it silently.
use crate::util::{self, tr, Res};
use crate::AppState;
use serde::Serialize;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};

/// One install queue at a time.
static BUSY: AtomicBool = AtomicBool::new(false);
/// Set by the "Cancel" button: stops the running install and skips the rest of the queue.
static CANCEL: AtomicBool = AtomicBool::new(false);

const COMMON: [&str; 3] = ["--source", "winget", "--accept-source-agreements"];

// winget exit codes (HRESULTs) we treat specially.
const OK_CODES: [u32; 2] = [
    0x8A15_002B, // no applicable upgrade: already the newest version
    0x8A15_0061, // package already installed
];
const REBOOT_CODES: [u32; 1] = [
    0x8A15_0109, // installed, a restart finishes it
];

/// winget is an App Execution Alias in the user's WindowsApps folder (not in System32).
fn winget_exe() -> Option<PathBuf> {
    let p = util::env_path("LOCALAPPDATA")?.join(r"Microsoft\WindowsApps\winget.exe");
    p.exists().then_some(p)
}

fn winget(args: &[&str]) -> Res<Command> {
    let exe = winget_exe().ok_or_else(no_winget)?;
    let mut c = Command::new(exe);
    c.args(args).creation_flags(util::CREATE_NO_WINDOW).stdin(Stdio::null());
    Ok(c)
}

fn no_winget() -> String {
    tr(
        "В Windows нет winget. Установите «Установщик приложений» (App Installer) из Microsoft Store",
        "winget isn't available. Install “App Installer” from the Microsoft Store",
    )
}

/// Microsoft Store product ids look like "9NKSQGP7F2NH" (no dot); winget package ids always have a dot.
pub fn is_store_id(id: &str) -> bool {
    !id.contains('.')
}

/// Package ids come from the app's own catalog, but still: only plain id characters.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 128 && id.chars().all(|c| c.is_ascii_alphanumeric() || ".+-_".contains(c))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WingetState {
    available: bool,
    version: String,
    /// winget package ids installed on this PC.
    installed: Vec<String>,
}

#[tauri::command]
pub async fn winget_state(state: State<'_, AppState>) -> Res<WingetState> {
    let tmp = state.data_dir.join("winget-export.json");
    util::blocking(move || read_state(&tmp)).await
}

/// winget appears a few seconds after App Installer is registered: wait for the alias.
fn wait_exe(secs: u64) -> bool {
    let end = Instant::now() + Duration::from_secs(secs);
    while Instant::now() < end {
        if winget_exe().is_some() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    winget_exe().is_some()
}

const REGISTER: &str = "Add-AppxPackage -RegisterByFamilyName -MainPackage Microsoft.DesktopAppInstaller_8wekyb3d8bbwe";

/// Puts winget on a PC that lacks it (Windows 10 without "App Installer"), without the Store.
/// The way from Microsoft's docs: register App Installer if it's only unregistered, otherwise the
/// official Microsoft.WinGet.Client module (PSGallery) runs Repair-WinGetPackageManager, which
/// downloads winget and its dependencies from Microsoft; Windows checks their signature on install.
/// Runs once at a time: the startup run and the "Install" tab share it.
pub fn ensure_winget() -> Res<()> {
    static SETUP: parking_lot::Mutex<()> = parking_lot::Mutex::new(());
    let _g = SETUP.lock();
    if winget_exe().is_some() {
        return Ok(());
    }
    let _ = util::ps_timeout(REGISTER, 90);
    if wait_exe(5) {
        return Ok(());
    }
    let script = "[Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12; \
        Install-PackageProvider -Name NuGet -MinimumVersion 2.8.5.201 -Force | Out-Null; \
        Install-Module -Name Microsoft.WinGet.Client -Force -Repository PSGallery -Scope AllUsers | Out-Null; \
        Import-Module Microsoft.WinGet.Client; \
        Repair-WinGetPackageManager -AllUsers | Out-Null";
    let res = util::ps_timeout(script, 900);
    let _ = util::ps_timeout(REGISTER, 90);
    if wait_exe(15) {
        return Ok(());
    }
    Err(match res {
        Err(e) => format!("{} ({e})", tr("Не удалось установить winget", "Couldn't install winget")),
        Ok(_) => tr("Не удалось установить winget", "Couldn't install winget"),
    })
}

/// "Install" tab without winget: installs it, then returns the fresh state.
#[tauri::command]
pub async fn winget_setup(state: State<'_, AppState>) -> Res<WingetState> {
    let tmp = state.data_dir.join("winget-export.json");
    util::blocking(move || {
        ensure_winget()?;
        read_state(&tmp)
    })
    .await
}

fn read_state(tmp: &std::path::Path) -> Res<WingetState> {
    {
        let Some(_) = winget_exe() else {
            return Ok(WingetState { available: false, version: String::new(), installed: vec![] });
        };
        let out = winget(&["--version"])?.output().map_err(|e| e.to_string())?;
        let version = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let _ = std::fs::remove_file(tmp);
        let tmp_s = tmp.to_string_lossy().to_string();
        let mut args = vec!["export", "-o", tmp_s.as_str(), "--disable-interactivity"];
        args.extend(COMMON);
        let _ = run_with_timeout(winget(&args)?, Duration::from_secs(90));
        let installed = std::fs::read_to_string(tmp).ok().map(|s| parse_export(&s)).unwrap_or_default();
        let _ = std::fs::remove_file(tmp);
        Ok(WingetState { available: true, version, installed })
    }
}

/// `winget export` JSON → installed package ids.
pub fn parse_export(json: &str) -> Vec<String> {
    let v: serde_json::Value = match serde_json::from_str(json.trim_start_matches('\u{feff}')) {
        Ok(v) => v,
        Err(_) => return vec![],
    };
    let mut ids: Vec<String> = v["Sources"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|s| s["Packages"].as_array().cloned().unwrap_or_default())
        .filter_map(|p| p["PackageIdentifier"].as_str().map(String::from))
        .collect();
    ids.sort();
    ids.dedup();
    ids
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Progress {
    id: String,
    /// queued | running | done | failed
    state: &'static str,
    /// Extra info: "already" (was installed), "reboot" (needs restart) or an error text.
    note: Option<String>,
    /// Download progress of the running package, 0..=100 (None = not known yet / installing).
    percent: Option<u8>,
    /// Bytes downloaded / total size of the installer, when known.
    done: Option<u64>,
    total: Option<u64>,
}

/// Runs a command, killing it after `limit`. Returns the exit code (None if killed / failed to start).
fn run_with_timeout(mut cmd: Command, limit: Duration) -> Option<i32> {
    let mut child = cmd.stdout(Stdio::null()).stderr(Stdio::null()).spawn().ok()?;
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(s)) => return s.code(),
            Ok(None) if start.elapsed() > limit => {
                // Kill the whole tree, not only winget itself.
                let _ = util::run_hidden("taskkill.exe", &["/PID", &child.id().to_string(), "/T", "/F"]);
                let _ = child.kill();
                return None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(300)),
            Err(_) => return None,
        }
    }
}

/// Installer download URL from `winget show` ("Installer Url:" / "URL установщика:" line).
pub fn installer_url(show: &str) -> Option<String> {
    show.lines()
        .filter(|l| {
            let l = l.to_lowercase();
            l.contains("installer") || l.contains("установщ")
        })
        .find_map(|l| l.split_whitespace().find(|w| w.starts_with("https://")).map(|u| u.trim().to_string()))
}

/// Size of the installer winget is going to download (None if the server doesn't say).
fn installer_size(id: &str) -> Option<u64> {
    if is_store_id(id) || CANCEL.load(Ordering::SeqCst) {
        return None; // the Store downloads by itself, no installer URL to measure
    }
    let mut args = vec!["show", "--id", id, "-e", "--disable-interactivity"];
    args.extend(COMMON);
    let out = winget(&args).ok()?.output().ok()?;
    let url = installer_url(&util::decode_console(&out.stdout))?;
    tauri::async_runtime::block_on(async {
        let resp = crate::updater::client().ok()?.head(&url).timeout(std::time::Duration::from_secs(15)).send().await.ok()?;
        // content_length() is 0 for HEAD responses in reqwest: read the header itself.
        resp.headers().get(reqwest::header::CONTENT_LENGTH)?.to_str().ok()?.parse::<u64>().ok().filter(|n| *n > 0)
    })
}

/// Bytes winget has downloaded so far for `id` (files in %TEMP%\WinGet\<id>.<version>\ changed since `since`).
fn downloaded_bytes(id: &str, since: std::time::SystemTime) -> u64 {
    let Some(root) = util::env_path("TEMP").map(|t| t.join("WinGet")) else { return 0 };
    let prefix = format!("{}.", id.to_lowercase());
    let mut total = 0;
    for dir in std::fs::read_dir(root).into_iter().flatten().flatten() {
        if !dir.file_name().to_string_lossy().to_lowercase().starts_with(&prefix) {
            continue;
        }
        for f in std::fs::read_dir(dir.path()).into_iter().flatten().flatten() {
            if let Ok(m) = f.metadata() {
                if m.is_file() && m.modified().map(|t| t >= since).unwrap_or(false) {
                    total += m.len();
                }
            }
        }
    }
    total
}

/// Runs an install, reporting download percent (by watching winget's download folder) until it ends.
fn run_install(id: &str, limit: Duration, on_percent: impl Fn(u8, u64, u64)) -> Option<i32> {
    let size = installer_size(id);
    // --force: install directly. winget may still list a package it installed earlier even after it was
    // removed by other means, and would then answer "already installed" without installing anything.
    // We only ever install what Windows doesn't have.
    let mut args = vec!["install", "--id", id, "-e", "--silent", "--accept-package-agreements", "--disable-interactivity"];
    if is_store_id(id) {
        args.extend(["--source", "msstore", "--accept-source-agreements"]);
    } else {
        args.push("--force");
        args.extend(COMMON);
    }
    let since = std::time::SystemTime::now() - Duration::from_secs(2);
    let mut child = winget(&args).ok()?.stdout(Stdio::null()).stderr(Stdio::null()).spawn().ok()?;
    let start = Instant::now();
    let mut last: Option<u8> = None;
    loop {
        match child.try_wait() {
            Ok(Some(s)) => return s.code(),
            Ok(None) if CANCEL.load(Ordering::SeqCst) => {
                // winget starts the installer as a child: end the whole tree.
                let _ = util::run_hidden("taskkill.exe", &["/PID", &child.id().to_string(), "/T", "/F"]);
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            Ok(None) if start.elapsed() > limit => {
                // Kill the whole tree, not only winget itself.
                let _ = util::run_hidden("taskkill.exe", &["/PID", &child.id().to_string(), "/T", "/F"]);
                let _ = child.kill();
                return None;
            }
            Ok(None) => {
                if let Some(total) = size {
                    let done = downloaded_bytes(id, since).min(total);
                    let p = ((done as f64 / total as f64) * 100.0).min(100.0) as u8;
                    if last != Some(p) {
                        last = Some(p);
                        on_percent(p, done, total);
                    }
                }
                std::thread::sleep(Duration::from_millis(400));
            }
            Err(_) => return None,
        }
    }
}

/// Plain-language text for a winget error code (winget's own localized description when possible).
fn describe(code: Option<i32>) -> String {
    let Some(code) = code else {
        return tr("Установка не завершилась за 20 минут и была остановлена", "The install didn't finish in 20 minutes and was stopped");
    };
    let hex = format!("0x{:08X}", code as u32);
    if let Ok(mut c) = winget(&["error", "--", &hex]) {
        if let Ok(out) = c.output() {
            // "0x8a150014 : APPINSTALLER_CLI_ERROR_..." then the localized description on the next line.
            if let Some(desc) = error_text(&util::decode_console(&out.stdout)) {
                return desc;
            }
        }
    }
    format!("{} {hex}", tr("Ошибка установщика", "Installer error"))
}

/// The localized description line from `winget error` output.
pub fn error_text(out: &str) -> Option<String> {
    let mut lines = out.lines().map(str::trim).filter(|l| !l.is_empty());
    lines.find(|l| l.contains(" : "))?;
    lines.next().map(String::from)
}

/// Classifies an exit code: (state, note).
pub fn classify(code: Option<i32>) -> (&'static str, Option<&'static str>) {
    match code {
        Some(0) => ("done", None),
        Some(c) if OK_CODES.contains(&(c as u32)) => ("done", Some("already")),
        Some(c) if REBOOT_CODES.contains(&(c as u32)) => ("done", Some("reboot")),
        _ => ("failed", None),
    }
}

/// Stops the running install and the rest of the queue.
#[tauri::command]
pub fn winget_cancel() {
    CANCEL.store(true, Ordering::SeqCst);
}

/// Installs packages one by one in the background; progress goes out as "winget://progress".
#[tauri::command]
pub fn winget_install(app: AppHandle, ids: Vec<String>) -> Res<()> {
    if !util::is_admin() {
        return Err(util::need_admin());
    }
    winget_exe().ok_or_else(no_winget)?;
    let ids: Vec<String> = ids.into_iter().filter(|i| valid_id(i)).collect();
    if ids.is_empty() {
        return Err(tr("Ничего не выбрано", "Nothing selected"));
    }
    if BUSY.swap(true, Ordering::SeqCst) {
        return Err(tr("Установка уже идёт", "An install is already running"));
    }
    // Reset only after we own the queue: a second click must not clear a pending cancel.
    CANCEL.store(false, Ordering::SeqCst);
    std::thread::spawn(move || {
        let emit = |id: &str, state: &'static str, note: Option<String>| {
            let _ = app.emit("winget://progress", Progress { id: id.to_string(), state, note, percent: None, done: None, total: None });
        };
        for id in &ids {
            emit(id, "queued", None);
        }
        for id in &ids {
            if CANCEL.load(Ordering::SeqCst) {
                emit(id, "failed", Some("cancelled".into()));
                continue;
            }
            emit(id, "running", None);
            let report = |p: u8, done: u64, total: u64| {
                let _ = app.emit("winget://progress", Progress { id: id.clone(), state: "running", note: None, percent: Some(p), done: Some(done), total: Some(total) });
            };
            let code = run_install(id, Duration::from_secs(20 * 60), report);
            match classify(code) {
                ("done", note) => emit(id, "done", note.map(String::from)),
                _ if CANCEL.load(Ordering::SeqCst) => emit(id, "failed", Some("cancelled".into())),
                _ => emit(id, "failed", Some(describe(code))),
            }
        }
        BUSY.store(false, Ordering::SeqCst);
        let _ = app.emit("winget://finished", ());
        // Icons of newly installed programs should be looked up again.
        app.state::<AppState>().icons.lock().clear();
    });
    Ok(())
}

#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    name: String,
    id: String,
    version: String,
}

/// Columns of a winget table: the header line right above the "-----" line gives column starts.
pub fn parse_table(out: &str) -> Vec<Vec<String>> {
    let lines: Vec<&str> = out.lines().map(|l| l.rsplit('\r').next().unwrap_or(l)).collect();
    let Some(dash) = lines.iter().position(|l| l.trim_start().starts_with("---") && l.trim().chars().all(|c| c == '-')) else {
        return vec![];
    };
    let Some(header) = dash.checked_sub(1).map(|i| lines[i]) else { return vec![] };
    let hc: Vec<char> = header.chars().collect();
    let mut starts = vec![];
    for i in 0..hc.len() {
        if hc[i] != ' ' && (i == 0 || hc[i - 1] == ' ') {
            starts.push(i);
        }
    }
    lines[dash + 1..]
        .iter()
        .filter(|l| !l.trim().is_empty() && !l.trim_start().starts_with('<'))
        .map(|l| {
            let c: Vec<char> = l.chars().collect();
            starts
                .iter()
                .enumerate()
                .map(|(k, &s)| {
                    let e = starts.get(k + 1).copied().unwrap_or(c.len()).min(c.len());
                    if s >= c.len() { String::new() } else { c[s..e].iter().collect::<String>().trim().to_string() }
                })
                .collect()
        })
        .collect()
}

/// Search the whole winget catalog (name, id, version).
#[tauri::command]
pub async fn winget_search(query: String) -> Res<Vec<SearchHit>> {
    let q: String = query.chars().filter(|c| !c.is_control() && *c != '"').collect::<String>().trim().to_string();
    if q.chars().count() < 2 || q.chars().count() > 64 {
        return Ok(vec![]);
    }
    util::blocking(move || {
        let mut args = vec!["search", "--query", q.as_str(), "--count", "40", "--disable-interactivity"];
        args.extend(COMMON);
        let out = winget(&args)?.output().map_err(|e| e.to_string())?;
        let text = util::decode_console(&out.stdout);
        Ok(parse_table(&text)
            .into_iter()
            .filter(|r| r.len() >= 3 && valid_id(&r[1]))
            .map(|r| SearchHit { name: r[0].clone(), id: r[1].clone(), version: r[2].clone() })
            .collect())
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Real winget on this PC (read-only): `cargo test --lib winget_live -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn winget_live_state() {
        let st = read_state(&std::env::temp_dir().join("sm-winget-test.json")).unwrap();
        println!("winget {} installed={}", st.version, st.installed.len());
        assert!(st.available && st.version.starts_with('v'));
        assert!(!st.installed.is_empty());
        println!("error text: {}", describe(Some(0x8A15_0014u32 as i32)));
        let hits = tauri::async_runtime::block_on(winget_search("notepad++".into())).unwrap();
        println!("search: {:?}", hits.iter().take(3).collect::<Vec<_>>());
        assert!(hits.iter().any(|h| h.id == "Notepad++.Notepad++"));
        let size = installer_size("voidtools.Everything");
        println!("Everything installer size: {size:?}");
        assert!(size.unwrap_or(0) > 100_000);
    }

    #[test]
    fn installer_url_line() {
        let en = "Found Everything [voidtools.Everything]\nVersion: 1.4\nHomepage: https://www.voidtools.com\nInstaller:\n  Installer Type: msi\n  Installer Url: https://www.voidtools.com/Everything-1.4.msi\n";
        assert_eq!(installer_url(en).as_deref(), Some("https://www.voidtools.com/Everything-1.4.msi"));
        let ru = "Домашняя страница: https://www.voidtools.com\nУстановщик:\n  URL-адрес установщика: https://x.example/setup.exe\n";
        assert_eq!(installer_url(ru).as_deref(), Some("https://x.example/setup.exe"));
        assert_eq!(installer_url("nothing here"), None);
    }

    #[test]
    fn search_table() {
        let out = "\r-\r\\\rИмя        ИД                     Версия   Совпадение\r\n-------------------------------------------------------\r\nAkelPad    AkelPad.AkelPad        4.9.9    Tag: notepad\r\nNotepad++  Notepad++.Notepad++    8.8.1\r\n<дополнительные записи усечены>\r\n";
        let t = parse_table(out);
        assert_eq!(t.len(), 2);
        assert_eq!(t[0], vec!["AkelPad", "AkelPad.AkelPad", "4.9.9", "Tag: notepad"]);
        assert_eq!(t[1][1], "Notepad++.Notepad++");
        assert_eq!(t[1][3], "");
        assert!(parse_table("Не найдены пакеты").is_empty());
    }

    #[test]
    fn store_ids() {
        assert!(is_store_id("9NKSQGP7F2NH") && is_store_id("XP8CLZL93F5Z4P"));
        assert!(!is_store_id("Discord.Discord") && !is_store_id("Microsoft.VCRedist.2015+.x64"));
        assert!(valid_id("9NKSQGP7F2NH"));
    }

    #[test]
    fn ids_are_plain() {
        assert!(valid_id("Discord.Discord"));
        assert!(valid_id("Microsoft.VCRedist.2015+.x64"));
        assert!(valid_id("Notepad++.Notepad++"));
        assert!(!valid_id("a b"));
        assert!(!valid_id("x\" --override \"y"));
        assert!(!valid_id(""));
    }

    #[test]
    fn export_parsing() {
        let j = "\u{feff}{\"Sources\":[{\"Packages\":[{\"PackageIdentifier\":\"Valve.Steam\"},{\"PackageIdentifier\":\"7zip.7zip\"}],\"SourceDetails\":{}}]}";
        assert_eq!(parse_export(j), vec!["7zip.7zip", "Valve.Steam"]);
        assert!(parse_export("not json").is_empty());
    }

    #[test]
    fn exit_codes() {
        assert_eq!(classify(Some(0)), ("done", None));
        assert_eq!(classify(Some(0x8A15_0061u32 as i32)), ("done", Some("already")));
        assert_eq!(classify(Some(0x8A15_0109u32 as i32)), ("done", Some("reboot")));
        assert_eq!(classify(Some(1)).0, "failed");
        assert_eq!(classify(None).0, "failed");
        assert_eq!(classify(Some(0x8A15_010Au32 as i32)).0, "failed");
        assert_eq!(
            error_text("0x8a150014 : APPINSTALLER_CLI_ERROR_NO_APPLICATIONS_FOUND\r\nПакеты не найдены\r\n").as_deref(),
            Some("Пакеты не найдены")
        );
    }
}
