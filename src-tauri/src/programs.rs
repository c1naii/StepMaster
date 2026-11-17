//! Installed programs: classic (registry), Microsoft Store, and "portable" folders in Program Files.
use crate::disk::recycle_paths;
use crate::fsops;
use crate::util::{self, env_path, is_protected_path, psq, tr, Res};
use crate::AppState;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};
use winreg::enums::*;
use winreg::RegKey;

const UNINSTALL: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall";

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ProgramInfo {
    id: String,
    name: String,
    publisher: String,
    version: String,
    size: u64,
    install_date: String,
    kind: String,
    location: String,
    protected: bool,
    can_uninstall: bool,
    /// Icon source for icons_get ("" = none found).
    icon: String,
}

/// Picks the main .exe in a folder: the one named like the folder/program, else the largest.
fn main_exe_in(dir: &Path, hint: &str) -> Option<PathBuf> {
    let hint = squash_name(hint);
    let mut exes: Vec<(PathBuf, u64)> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            let is_exe = p.extension().map(|x| x.eq_ignore_ascii_case("exe")).unwrap_or(false);
            let name = p.file_stem()?.to_string_lossy().to_lowercase();
            let skip = name.starts_with("unins") || name.contains("uninstall") || name.contains("update") || name.contains("crash") || name.contains("helper");
            (is_exe && !skip).then(|| (p.clone(), e.metadata().map(|m| m.len()).unwrap_or(0)))
        })
        .collect();
    if exes.is_empty() {
        return None;
    }
    if let Some(m) = exes.iter().find(|(p, _)| {
        let s = squash_name(&p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default());
        !hint.is_empty() && (hint.starts_with(&s) || s.starts_with(&hint))
    }) {
        return Some(m.0.clone());
    }
    exes.sort_by_key(|x| std::cmp::Reverse(x.1));
    Some(exes[0].0.clone())
}

/// All Start menu shortcuts (squashed name, path): the most reliable icon source for many apps.
fn start_menu_shortcuts() -> Vec<(String, PathBuf)> {
    fn walk(dir: &Path, out: &mut Vec<(String, PathBuf)>, depth: u32) {
        if depth > 3 {
            return;
        }
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out, depth + 1);
            } else if p.extension().map(|x| x.eq_ignore_ascii_case("lnk")).unwrap_or(false) {
                let stem = p.file_stem().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default();
                if !stem.contains("uninstall") && !stem.contains("удалить") && !stem.contains("деинстал") {
                    out.push((squash_name(&stem), p));
                }
            }
        }
    }
    let mut out = Vec::new();
    for var in ["ProgramData", "APPDATA"] {
        if let Some(root) = env_path(var) {
            walk(&root.join(r"Microsoft\Windows\Start Menu\Programs"), &mut out, 0);
        }
    }
    out
}

/// (name words, display name) of everything installed: registry programs + Start menu apps.
pub fn installed_names() -> Vec<(Vec<String>, String)> {
    let mut out: Vec<(Vec<String>, String)> = read_win32()
        .into_iter()
        .map(|e| (crate::downloads::words(&base_name(&e.info.name)), e.info.name))
        .filter(|(w, _)| !w.is_empty())
        .collect();
    for (_, p) in start_menu_shortcuts() {
        let name = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let w = crate::downloads::words(&name);
        if !w.is_empty() {
            out.push((w, name));
        }
    }
    out
}

/// Generic "application" icon used when a program ships none at all.
const GENERIC_ICON: &str = r"%SystemRoot%\System32\shell32.dll,2";

/// Icon for a classic program, best source first: DisplayIcon, main exe in its folder,
/// a Start menu shortcut, the MSI icon cache, its uninstaller, then a generic icon.
fn win32_icon(display_icon: &str, location: &str, name: &str, uninstall: &str, subkey: &str, shortcuts: &[(String, PathBuf)]) -> String {
    let found = win32_icon_source(display_icon, location, name, uninstall, subkey, shortcuts);
    if found.is_empty() {
        GENERIC_ICON.to_string()
    } else {
        found
    }
}

fn win32_icon_source(display_icon: &str, location: &str, name: &str, uninstall: &str, subkey: &str, shortcuts: &[(String, PathBuf)]) -> String {
    let di = display_icon.trim();
    if !di.is_empty() {
        if let Some((p, _)) = crate::icons::parse_spec(di) {
            if p.exists() {
                return di.to_string();
            }
        }
    }
    if !location.is_empty() {
        if let Some(exe) = main_exe_in(Path::new(location), name) {
            return exe.to_string_lossy().to_string();
        }
    }
    let base = squash_name(&base_name(name));
    if base.chars().count() >= 3 {
        if let Some((_, p)) = shortcuts.iter().find(|(s, _)| *s == base).or_else(|| shortcuts.iter().find(|(s, _)| s.starts_with(&base))) {
            return p.to_string_lossy().to_string();
        }
    }
    // MSI keeps product icons in C:\Windows\Installer\{ProductCode}\.
    if subkey.starts_with('{') && subkey.ends_with('}') {
        let dir = crate::util::system_root().join("Installer").join(subkey);
        if let Ok(rd) = std::fs::read_dir(&dir) {
            if let Some(f) = rd.flatten().map(|e| e.path()).find(|p| {
                p.extension().map(|x| x.eq_ignore_ascii_case("ico") || x.eq_ignore_ascii_case("exe")).unwrap_or(false)
            }) {
                return f.to_string_lossy().to_string();
            }
        }
    }
    win32_icon_legacy(uninstall)
}

fn win32_icon_legacy(uninstall: &str) -> String {
    if !uninstall.to_ascii_lowercase().contains("msiexec") {
        if let Some(exe) = crate::icons::exe_from_command(uninstall) {
            if Path::new(&exe).exists() {
                return exe;
            }
        }
    }
    String::new()
}

/// Store apps: the best PNG for the logo named in AppxManifest.xml (prefers unplated 48–64 px).
fn store_logo(install: &str) -> String {
    let dir = Path::new(install);
    let Ok(manifest) = std::fs::read_to_string(dir.join("AppxManifest.xml")) else { return String::new() };
    let attr = |name: &str| -> Option<String> {
        let key = format!("{name}=\"");
        let i = manifest.find(&key)? + key.len();
        let j = manifest[i..].find('"')? + i;
        Some(manifest[i..j].to_string())
    };
    let tag = || -> Option<String> {
        let i = manifest.find("<Logo>")? + 6;
        let j = manifest[i..].find("</Logo>")? + i;
        Some(manifest[i..j].trim().to_string())
    };
    let Some(rel) = attr("Square44x44Logo").or_else(|| attr("Square150x150Logo")).or_else(tag) else { return String::new() };
    let rel_path = dir.join(rel.replace('/', "\\"));
    if rel_path.is_file() {
        return rel_path.to_string_lossy().to_string();
    }
    let (Some(folder), Some(stem)) = (rel_path.parent(), rel_path.file_stem()) else { return String::new() };
    let stem = stem.to_string_lossy().to_lowercase();
    let score = |n: &str| -> i32 {
        let mut s = 0;
        if n.contains("targetsize-64") { s += 60 } else if n.contains("targetsize-48") { s += 55 } else if n.contains("targetsize-96") { s += 50 } else if n.contains("targetsize-32") { s += 40 }
        if n.contains("scale-200") { s += 30 } else if n.contains("scale-150") { s += 25 } else if n.contains("scale-100") { s += 20 }
        if n.contains("altform-unplated") { s += 10 }
        if n.contains("contrast-") || n.contains("lightunplated") { s -= 100 }
        s
    };
    std::fs::read_dir(folder)
        .ok()
        .and_then(|rd| {
            rd.flatten()
                .map(|e| e.file_name().to_string_lossy().to_string())
                .filter(|n| {
                    let l = n.to_lowercase();
                    l.starts_with(&stem) && l.ends_with(".png")
                })
                .max_by_key(|n| score(&n.to_lowercase()))
        })
        .map(|n| folder.join(n).to_string_lossy().to_string())
        .unwrap_or_default()
}

/// Components the app itself or Windows depends on.
fn is_protected_program(name: &str) -> bool {
    let n = name.to_lowercase();
    n.contains("webview2")
        || n.starts_with("microsoft visual c++")
        || n.starts_with("microsoft .net")
        || n.starts_with("microsoft windows desktop runtime")
        || n.starts_with("microsoft asp.net")
        || n.contains("update health tools")
        || n == "microsoft edge"
        || n.starts_with("microsoft edge update")
        || n.starts_with("windows sdk")
        || n.starts_with("windows software development kit")
}

struct Win32Entry {
    hive: &'static str,
    view: &'static str,
    subkey: String,
    info: ProgramInfo,
}

fn open_uninstall(hive: &str, view: &str) -> Option<RegKey> {
    let root = match hive {
        "hklm" => RegKey::predef(HKEY_LOCAL_MACHINE),
        _ => RegKey::predef(HKEY_CURRENT_USER),
    };
    let flag = if view == "32" { KEY_WOW64_32KEY } else { KEY_WOW64_64KEY };
    root.open_subkey_with_flags(UNINSTALL, KEY_READ | flag).ok()
}

fn read_win32() -> Vec<Win32Entry> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let shortcuts = start_menu_shortcuts();
    for (hive, view) in [("hklm", "64"), ("hklm", "32"), ("hkcu", "64")] {
        let Some(root) = open_uninstall(hive, view) else { continue };
        for sub in root.enum_keys().flatten() {
            let Ok(k) = root.open_subkey(&sub) else { continue };
            let name: String = k.get_value("DisplayName").unwrap_or_default();
            let name = name.trim_matches(|c: char| c.is_whitespace() || c == '\0').to_string();
            if name.is_empty() {
                continue;
            }
            if k.get_value::<u32, _>("SystemComponent").unwrap_or(0) == 1 {
                continue;
            }
            if k.get_value::<String, _>("ParentKeyName").is_ok() {
                continue;
            }
            let release: String = k.get_value("ReleaseType").unwrap_or_default();
            if ["update", "hotfix", "security update"].contains(&release.to_lowercase().as_str()) {
                continue;
            }
            let version: String = k.get_value("DisplayVersion").unwrap_or_default();
            if !seen.insert((name.to_lowercase(), version.clone())) {
                continue;
            }
            let uninstall: String = k.get_value("UninstallString").unwrap_or_default();
            let date: String = k.get_value("InstallDate").unwrap_or_default();
            let install_date = if date.len() == 8 && date.chars().all(|c| c.is_ascii_digit()) {
                format!("{}-{}-{}", &date[0..4], &date[4..6], &date[6..8])
            } else {
                String::new()
            };
            let size_kb: u32 = k.get_value("EstimatedSize").unwrap_or(0);
            let location: String = k.get_value::<String, _>("InstallLocation").unwrap_or_default();
            let location = location.trim().trim_matches('"').trim_end_matches('\\').to_string();
            let location = if Path::new(&location).is_dir() { location } else { String::new() };
            let protected = is_protected_program(&name);
            let display_icon: String = k.get_value("DisplayIcon").unwrap_or_default();
            let icon = win32_icon(&display_icon, &location, &name, &uninstall, &sub, &shortcuts);
            out.push(Win32Entry {
                hive,
                view,
                subkey: sub.clone(),
                info: ProgramInfo {
                    id: format!("w|{hive}|{view}|{sub}"),
                    publisher: k.get_value::<String, _>("Publisher").unwrap_or_default().trim().to_string(),
                    version,
                    size: size_kb as u64 * 1024,
                    install_date,
                    kind: "win32".into(),
                    location,
                    protected,
                    can_uninstall: !protected && !uninstall.trim().is_empty(),
                    icon,
                    name,
                },
            });
        }
    }
    out
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoreRaw {
    #[serde(default)]
    full: String,
    #[serde(default)]
    family: String,
    #[serde(default)]
    #[allow(dead_code)]
    name: String,
    #[serde(default)]
    display: String,
    #[serde(default)]
    publisher: String,
    #[serde(default)]
    version: String,
    #[serde(default)]
    location: String,
}

fn read_store() -> Vec<ProgramInfo> {
    let script = r#"
$start = @{}
try { Get-StartApps | ForEach-Object { $fam = ([string]$_.AppID).Split('!')[0]; if (-not $start.ContainsKey($fam)) { $start[$fam] = [string]$_.Name } } } catch {}
ConvertTo-Json -Compress -InputObject @(Get-AppxPackage | Where-Object { -not $_.IsFramework -and -not $_.NonRemovable -and $_.SignatureKind -eq 'Store' } | ForEach-Object {
  [pscustomobject]@{ full=[string]$_.PackageFullName; family=[string]$_.PackageFamilyName; name=[string]$_.Name;
    display=[string]$start[[string]$_.PackageFamilyName]; publisher=([string]$_.Publisher -replace '^CN=([^,]+).*','$1');
    version=[string]$_.Version; location=[string]$_.InstallLocation } })
"#;
    let raw: Vec<StoreRaw> = util::ps_json_timeout(script, 60).unwrap_or_default();
    // Only packages with a Start menu entry are real apps. The rest are runtimes, codecs and
    // language packs that other apps depend on — removing them would break things.
    raw.into_iter()
        .filter(|r| !r.full.is_empty() && !r.display.trim().is_empty())
        .map(|r| {
            let name = r.display.trim().to_string();
            ProgramInfo {
                id: format!("s|{}|{}", r.full, r.family),
                name,
                publisher: r.publisher,
                version: r.version,
                size: 0,
                install_date: String::new(),
                kind: "store".into(),
                icon: Some(store_logo(&r.location)).filter(|s| !s.is_empty()).unwrap_or_else(|| GENERIC_ICON.to_string()),
                location: if Path::new(&r.location).is_dir() { r.location } else { String::new() },
                protected: false,
                can_uninstall: true,
            }
        })
        .collect()
}

const SKIP_FOLDERS: &[&str] = &[
    "common files", "internet explorer", "modifiablewindowsapps", "windowsapps", "uninstall information",
    "reference assemblies", "msbuild", "dotnet", "packagemanagement", "windowspowershell", "microsoft.net",
    "windows defender", "windows defender advanced threat protection", "windows nt", "windows mail",
    "windows media player", "windows photo viewer", "windows sidebar", "windows security", "windows kits",
    "windows portable devices", "nvidia corporation", "nvidia", "intel", "amd", "ati technologies", "realtek",
    "dolby", "microsoft", "microsoft office", "microsoft update health tools", "microsoft visual studio",
    "microsoft sdks", "iis", "iis express", "application verifier", "mozilla maintenance service", "temp",
    "programs", "steam", "epic games", "riot games", "installshield installation information", "easyanticheat",
    "easyanticheat_eos", "battleye", "common", "package cache", "windowspowershell", "vulkanrt", "nodejs",
];

/// Lowercase letters and digits only: "Node.js" and "nodejs" compare equal.
fn squash(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(|c| c.to_lowercase()).collect()
}

fn read_portable(registered: &[Win32Entry], store: &[ProgramInfo]) -> Vec<ProgramInfo> {
    let mut known: HashSet<String> = HashSet::new();
    let mut names: Vec<String> = Vec::new();
    for e in registered {
        if !e.info.location.is_empty() {
            known.insert(e.info.location.to_lowercase());
        }
        names.push(squash(&e.info.name));
        names.push(squash(&e.info.publisher));
    }
    for s in store {
        names.push(squash(&s.name));
    }
    names.retain(|n| n.chars().count() >= 3);
    let mut roots: Vec<PathBuf> = Vec::new();
    for var in ["ProgramW6432", "ProgramFiles", "ProgramFiles(x86)"] {
        if let Some(p) = env_path(var) {
            roots.push(p);
        }
    }
    if let Some(l) = env_path("LOCALAPPDATA") {
        roots.push(l.join("Programs"));
    }
    roots.sort();
    roots.dedup();

    let mut dirs: Vec<PathBuf> = Vec::new();
    for root in roots {
        let Ok(rd) = std::fs::read_dir(&root) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if !p.is_dir() {
                continue;
            }
            let folder = e.file_name().to_string_lossy().to_lowercase();
            if SKIP_FOLDERS.contains(&folder.as_str()) || folder.starts_with("windows") || folder.starts_with("microsoft") {
                continue;
            }
            let pl = p.to_string_lossy().to_lowercase();
            // Registered install location equals or is inside this folder?
            if known.iter().any(|k| *k == pl || k.starts_with(&(pl.clone() + "\\"))) {
                continue;
            }
            // Folder name appears in a registered program name or publisher -> belongs to it.
            // Very short names ("VB") are too ambiguous to judge, so they are skipped entirely.
            let sq = squash(&folder);
            if sq.chars().count() < 3 || names.iter().any(|n| n.contains(&sq) || sq.contains(n.as_str())) {
                continue;
            }
            // Must contain an .exe at top level or one level down.
            let has_exe = |d: &Path| {
                std::fs::read_dir(d)
                    .map(|rd| rd.flatten().any(|f| f.path().extension().map(|x| x.eq_ignore_ascii_case("exe")).unwrap_or(false)))
                    .unwrap_or(false)
            };
            let mut exe = has_exe(&p);
            if !exe {
                if let Ok(rd) = std::fs::read_dir(&p) {
                    exe = rd.flatten().filter(|s| s.path().is_dir()).take(20).any(|s| has_exe(&s.path()));
                }
            }
            if exe {
                dirs.push(p);
            }
        }
    }
    dirs.par_iter()
        .map(|p| {
            let modified = std::fs::metadata(p)
                .and_then(|m| m.modified())
                .ok()
                .map(|t| {
                    let secs = t.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) as i64;
                    let days = secs / 86400;
                    civil_from_days(days)
                })
                .unwrap_or_default();
            ProgramInfo {
                id: format!("p|{}", p.to_string_lossy()),
                name: p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
                publisher: String::new(),
                version: String::new(),
                size: fsops::folder_size(p, 200_000),
                install_date: modified,
                kind: "portable".into(),
                icon: main_exe_in(p, &p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default())
                    .or_else(|| {
                        // One level down, e.g. "Program\bin\app.exe".
                        std::fs::read_dir(p).ok()?.flatten().filter(|s| s.path().is_dir()).take(20).find_map(|s| main_exe_in(&s.path(), ""))
                    })
                    .map(|e| e.to_string_lossy().to_string())
                    .unwrap_or_else(|| GENERIC_ICON.to_string()),
                location: p.to_string_lossy().to_string(),
                protected: false,
                can_uninstall: !is_protected_path(p),
            }
        })
        .collect()
}

/// Days since 1970-01-01 -> "YYYY-MM-DD" (Howard Hinnant's algorithm).
pub fn civil_from_days(z: i64) -> String {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

#[tauri::command]
pub async fn programs_list() -> Res<Vec<ProgramInfo>> {
    util::blocking(|| {
        let win32 = read_win32();
        let store = read_store();
        let portable = read_portable(&win32, &store);
        let mut all: Vec<ProgramInfo> = win32.into_iter().map(|e| e.info).collect();
        all.extend(store);
        all.extend(portable);
        Ok(all)
    })
    .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UninstallResult {
    finished: bool,
    leftovers: Vec<String>,
}

/// Splits an UninstallString into (exe, args). Handles quoted and unquoted paths with spaces.
pub fn split_command(cmd: &str) -> Option<(String, String)> {
    let cmd = cmd.trim();
    if cmd.is_empty() {
        return None;
    }
    if let Some(rest) = cmd.strip_prefix('"') {
        let end = rest.find('"')?;
        return Some((rest[..end].to_string(), rest[end + 1..].trim().to_string()));
    }
    // ASCII-only lowercase keeps byte positions identical to `cmd`.
    let lower = cmd.to_ascii_lowercase();
    if let Some(i) = lower.find(".exe") {
        let (exe, args) = cmd.split_at(i + 4);
        return Some((exe.trim().to_string(), args.trim().to_string()));
    }
    match cmd.split_once(' ') {
        Some((a, b)) => Some((a.to_string(), b.trim().to_string())),
        None => Some((cmd.to_string(), String::new())),
    }
}

/// Extracts an MSI product code "{GUID}" from an uninstall string.
pub fn msi_guid(cmd: &str) -> Option<String> {
    let lower = cmd.to_lowercase();
    if !lower.contains("msiexec") {
        return None;
    }
    let start = cmd.find('{')?;
    let end = cmd[start..].find('}')? + start;
    let guid = &cmd[start..=end];
    (guid.len() == 38 && guid[1..37].chars().all(|c| c.is_ascii_hexdigit() || c == '-')).then(|| guid.to_string())
}

fn key_exists(hive: &str, view: &str, sub: &str) -> bool {
    open_uninstall(hive, view).map(|k| k.open_subkey(sub).is_ok()).unwrap_or(false)
}

/// Typical names of the temporary copies uninstallers spawn (NSIS, Inno Setup, InstallShield...).
/// Only processes started after we launched the uninstaller are considered.
fn looks_like_uninstaller(name: &str) -> bool {
    let n = name.to_lowercase();
    if n == "trustedinstaller.exe" || n == "tiworker.exe" {
        return false;
    }
    n.starts_with("au_")
        || n.starts_with("un_")
        || n.starts_with("_iu")
        || n.starts_with("unins")
        || n.contains("uninst")
        || n == "msiexec.exe"
        || n.ends_with(".tmp")
}

/// Vendor / shared folder names that must never be offered as "leftovers" of one program.
const SHARED_FOLDERS: &[&str] = &[
    "microsoft", "windows", "intel", "nvidia", "nvidiacorporation", "amd", "ati", "google", "adobe", "apple",
    "apple computer", "packages", "programs", "python", "java", "oracle", "mozilla", "realtek", "dell", "hp",
    "lenovo", "asus", "msi", "steam", "valve", "epic games", "ubisoft", "electronic arts", "ea", "riot games",
    "common files", "temp", "cache", "crashdumps", "default", "public", "games", "program files", "app",
];

/// Product name without version / edition: "7-Zip 26.06 (x64 edition)" -> "7-Zip",
/// "Hiddify, версия 2.0.5" -> "Hiddify", "Microsoft 365 - en-us" -> "Microsoft".
fn base_name(display: &str) -> String {
    let mut words = Vec::new();
    for (i, w) in display.split_whitespace().enumerate() {
        let lw = w.to_lowercase();
        let starts_digit = w.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false);
        let is_version = lw.starts_with('v') && lw.chars().nth(1).map(|c| c.is_ascii_digit()).unwrap_or(false);
        if i > 0 && (starts_digit || is_version || w.starts_with('(') || w == "-" || w == "—" || lw == "version" || lw == "версия") {
            break;
        }
        words.push(w.trim_end_matches(',').to_string());
    }
    words.join(" ")
}

/// Lowercase letters/digits only.
fn squash_name(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(|c| c.to_lowercase()).collect()
}

fn is_shared_folder(name: &str) -> bool {
    let n = name.trim().to_lowercase();
    SHARED_FOLDERS.contains(&n.as_str()) || SHARED_FOLDERS.iter().any(|s| squash_name(s) == squash_name(&n))
}

pub fn leftovers_for(info: &ProgramInfo) -> Vec<String> {
    let base = base_name(&info.name);
    let base_sq = squash_name(&base);
    if base_sq.chars().count() < 3 || is_shared_folder(&base) {
        return vec![];
    }
    let mut candidates: Vec<PathBuf> = Vec::new();
    // The install folder only when its name IS the product name (never "D:\Games", "Microsoft Office"...).
    if !info.location.is_empty() {
        if let Some(leaf) = Path::new(&info.location).file_name().map(|l| l.to_string_lossy().to_string()) {
            if squash_name(&leaf) == base_sq && !is_shared_folder(&leaf) {
                candidates.push(PathBuf::from(&info.location));
            }
        }
    }
    for var in ["APPDATA", "LOCALAPPDATA", "ProgramData"] {
        if let Some(root) = env_path(var) {
            candidates.push(root.join(&base));
        }
    }
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for c in candidates {
        let key = c.to_string_lossy().to_lowercase();
        if c.is_dir() && !crate::fsops::is_reparse_path(&c) && !is_protected_path(&c) && seen.insert(key) {
            out.push(c.to_string_lossy().to_string());
        }
    }
    out
}

fn uninstall_win32(state: &AppState, hive: &str, view: &str, sub: &str) -> Res<UninstallResult> {
    let entry = read_win32()
        .into_iter()
        .find(|e| e.hive == hive && e.view == view && e.subkey == sub)
        .ok_or_else(|| tr("Программа уже удалена", "The program is already removed"))?;
    if entry.info.protected {
        return Err(tr("Это системный компонент — удалять нельзя", "This is a system component"));
    }
    let key = open_uninstall(hive, view).and_then(|k| k.open_subkey(sub).ok()).ok_or_else(|| tr("Программа уже удалена", "Already removed"))?;
    let cmd: String = key.get_value("UninstallString").unwrap_or_default();
    let is_msi = key.get_value::<u32, _>("WindowsInstaller").unwrap_or(0) == 1;

    let mut command = if let Some(guid) = msi_guid(&cmd).or_else(|| is_msi.then(|| sub.to_string()).filter(|s| s.starts_with('{'))) {
        let mut c = std::process::Command::new(util::sys_tool("msiexec.exe"));
        c.raw_arg(format!("/x {guid}"));
        c
    } else {
        let (exe, args) = split_command(&cmd).ok_or_else(|| tr("Нет команды удаления", "No uninstall command"))?;
        let exe = expand_env(&exe);
        // "RunDll32 ..." style commands: bare names resolve to System32, never the app folder.
        let exe_path = if Path::new(&exe).is_absolute() {
            PathBuf::from(&exe)
        } else {
            let name = if exe.to_lowercase().ends_with(".exe") { exe.clone() } else { format!("{exe}.exe") };
            util::sys_tool(&name)
        };
        let mut c = std::process::Command::new(&exe_path);
        if !args.is_empty() {
            c.raw_arg(&args);
        }
        c
    };
    let spawned_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().saturating_sub(2))
        .unwrap_or(0);
    let mut child = command
        .spawn()
        .map_err(|e| format!("{}: {e}", tr("Не удалось запустить деинсталлятор", "Couldn't start the uninstaller")))?;
    let _ = child.wait();

    // Many uninstallers relaunch themselves from Temp and exit at once. Wait until the registry
    // entry disappears, or until no uninstaller-like process started by us has been seen for 20 s.
    let started = Instant::now();
    let mut last_seen = Instant::now();
    while key_exists(hive, view, sub) && started.elapsed() < Duration::from_secs(30 * 60) {
        let busy = crate::system::list_processes(state)
            .iter()
            .any(|p| p.started >= spawned_at && looks_like_uninstaller(&p.name));
        if busy {
            last_seen = Instant::now();
        } else if last_seen.elapsed() > Duration::from_secs(20) {
            break;
        }
        std::thread::sleep(Duration::from_millis(1000));
    }
    if key_exists(hive, view, sub) {
        return Ok(UninstallResult { finished: false, leftovers: vec![] });
    }
    Ok(UninstallResult { finished: true, leftovers: leftovers_for(&entry.info) })
}

fn expand_env(s: &str) -> String {
    let mut out = s.to_string();
    while let Some(start) = out.find('%') {
        let Some(len) = out[start + 1..].find('%') else { break };
        let var = &out[start + 1..start + 1 + len];
        let val = std::env::var(var).unwrap_or_default();
        if val.is_empty() {
            break;
        }
        out = format!("{}{}{}", &out[..start], val, &out[start + 2 + len..]);
    }
    out
}

#[tauri::command]
pub async fn program_uninstall(app: AppHandle, id: String, name: Option<String>) -> Res<UninstallResult> {
    if !util::is_admin() {
        return Err(util::need_admin());
    }
    let app2 = app.clone();
    let r = util::blocking(move || {
        let state = app.state::<AppState>();
        let parts: Vec<&str> = id.splitn(4, '|').collect();
        match parts.as_slice() {
            ["w", hive, view, sub] => uninstall_win32(&state, hive, view, sub),
            ["s", rest @ ..] => {
                let full = rest.first().copied().unwrap_or("");
                let family = rest.get(1).copied().unwrap_or("");
                if full.is_empty() || full.contains('\'') {
                    return Err(tr("Неверное приложение", "Invalid app"));
                }
                util::ps(&format!("Remove-AppxPackage -Package {}", psq(full)))?;
                let leftovers = env_path("LOCALAPPDATA")
                    .map(|l| l.join("Packages").join(family))
                    .filter(|p| !family.is_empty() && p.is_dir())
                    .map(|p| vec![p.to_string_lossy().to_string()])
                    .unwrap_or_default();
                Ok(UninstallResult { finished: true, leftovers })
            }
            ["p", path @ ..] => {
                let p = path.join("|");
                let (done, failed) = recycle_paths(&[p]);
                if let Some(f) = failed.first() {
                    return Err(f.clone());
                }
                Ok(UninstallResult { finished: !done.is_empty(), leftovers: vec![] })
            }
            _ => Err(tr("Неверная программа", "Invalid program")),
        }
    })
    .await?;
    if r.finished {
        let shown: String = name.unwrap_or_default().chars().filter(|c| !c.is_control()).take(200).collect();
        crate::journal::add(&app2, "uninstall", &shown, "", None);
    }
    Ok(r)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split() {
        assert_eq!(
            split_command(r#""C:\Program Files\App\unins000.exe" /SILENT"#),
            Some((r"C:\Program Files\App\unins000.exe".into(), "/SILENT".into()))
        );
        assert_eq!(
            split_command(r"C:\Program Files\App\uninstall.exe --remove"),
            Some((r"C:\Program Files\App\uninstall.exe".into(), "--remove".into()))
        );
        assert_eq!(split_command(""), None);
    }

    #[test]
    fn guid() {
        assert_eq!(
            msi_guid("MsiExec.exe /I{12345678-1234-1234-1234-123456789ABC}"),
            Some("{12345678-1234-1234-1234-123456789ABC}".into())
        );
        assert_eq!(msi_guid(r"C:\x\uninst.exe {abc}"), None);
    }

    #[test]
    fn dates() {
        assert_eq!(civil_from_days(0), "1970-01-01");
        assert_eq!(civil_from_days(20_407), "2025-11-15");
    }

    #[test]
    fn base_names() {
        assert_eq!(base_name("7-Zip 26.06 (x64 edition)"), "7-Zip");
        assert_eq!(base_name("Hiddify, версия 2.0.5+20005"), "Hiddify");
        assert_eq!(base_name("Microsoft 365 - en-us"), "Microsoft");
        assert_eq!(base_name("Discord"), "Discord");
        assert_eq!(base_name("Ubisoft Connect"), "Ubisoft Connect");
        assert_eq!(base_name("OBS Studio v30.1"), "OBS Studio");
    }

    #[test]
    fn leftovers_never_offer_vendor_folders() {
        let mk = |name: &str, loc: &str| ProgramInfo {
            id: String::new(),
            name: name.into(),
            publisher: String::new(),
            version: String::new(),
            size: 0,
            install_date: String::new(),
            kind: "win32".into(),
            location: loc.into(),
            protected: false,
            can_uninstall: true,
            icon: String::new(),
        };
        // "Microsoft 365" -> base "Microsoft" is a shared vendor folder: nothing offered.
        assert!(leftovers_for(&mk("Microsoft 365 - en-us", r"C:\Program Files\Microsoft Office")).is_empty());
        assert!(leftovers_for(&mk("Intel(R) Graphics", "")).is_empty());
        assert!(is_shared_folder("Microsoft"));
        assert!(is_shared_folder("NVIDIA Corporation"));
        assert!(!is_shared_folder("Discord"));
    }

    #[test]
    fn protected_names() {
        assert!(is_protected_program("Microsoft Edge WebView2 Runtime"));
        assert!(is_protected_program("Microsoft Visual C++ 2015-2022 Redistributable (x64)"));
        assert!(!is_protected_program("Google Chrome"));
    }
}
