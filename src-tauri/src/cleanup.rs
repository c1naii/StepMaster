//! Junk cleanup: fixed list of cache/temp locations. Personal files are never touched.
use crate::fsops::{self, Deleted, Stats};
use crate::settings::write_json;
use crate::util::{self, env_path, system_root, Res};
use crate::AppState;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager, State};

const IDS: &[&str] = &[
    "user_temp",
    "windows_temp",
    "browser_cache",
    "thumbnails",
    "crash_dumps",
    "error_reports",
    "shader_cache",
    "update_cache",
    "delivery_opt",
    "recycle_bin",
];

/// Temp files younger than this are kept: installers and running apps may still need them.
const TEMP_MIN_AGE: Duration = Duration::from_secs(2 * 3600);

enum Target {
    Dir(PathBuf, Option<Duration>),
    Files(PathBuf, &'static str, &'static str),
    File(PathBuf),
    RecycleBin,
    DeliveryOpt(PathBuf),
}

fn needs_admin(id: &str) -> bool {
    matches!(id, "windows_temp" | "update_cache" | "delivery_opt")
}

fn local() -> Option<PathBuf> {
    env_path("LOCALAPPDATA")
}

fn chromium_cache_dirs() -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Some(local) = local() else { return out };
    let roots = [
        r"Google\Chrome\User Data",
        r"Google\Chrome Beta\User Data",
        r"Microsoft\Edge\User Data",
        r"BraveSoftware\Brave-Browser\User Data",
        r"Yandex\YandexBrowser\User Data",
        r"Vivaldi\User Data",
        r"Chromium\User Data",
    ];
    for r in roots {
        let root = local.join(r);
        if !root.is_dir() {
            continue;
        }
        for sub in ["ShaderCache", "GrShaderCache", "GraphiteDawnCache"] {
            out.push(root.join(sub));
        }
        if let Ok(rd) = std::fs::read_dir(&root) {
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() && p.join("Cache").is_dir() || p.join("Code Cache").is_dir() {
                    for c in ["Cache", "Code Cache", "GPUCache"] {
                        out.push(p.join(c));
                    }
                }
            }
        }
    }
    // Opera keeps caches in both Local and Roaming.
    let roaming = env_path("APPDATA");
    for name in ["Opera Stable", "Opera GX Stable"] {
        for base in [Some(local.clone()), roaming.clone()].into_iter().flatten() {
            let root = base.join("Opera Software").join(name);
            for c in ["Cache", "Code Cache", "GPUCache"] {
                out.push(root.join(c));
            }
        }
    }
    // Firefox
    if let Ok(rd) = std::fs::read_dir(local.join(r"Mozilla\Firefox\Profiles")) {
        for e in rd.flatten() {
            out.push(e.path().join("cache2"));
        }
    }
    out.retain(|p| p.is_dir());
    out.sort();
    out.dedup();
    out
}

fn targets(id: &str) -> Vec<Target> {
    let win = system_root();
    let local = local();
    let mut t = Vec::new();
    match id {
        "user_temp" => {
            if let Some(p) = env_path("TEMP") {
                t.push(Target::Dir(p, Some(TEMP_MIN_AGE)));
            }
        }
        "windows_temp" => t.push(Target::Dir(win.join("Temp"), Some(TEMP_MIN_AGE))),
        "browser_cache" => {
            for d in chromium_cache_dirs() {
                t.push(Target::Dir(d, None));
            }
        }
        "thumbnails" => {
            if let Some(l) = &local {
                t.push(Target::Files(l.join(r"Microsoft\Windows\Explorer"), "thumbcache_", ".db"));
            }
        }
        "crash_dumps" => {
            if let Some(l) = &local {
                t.push(Target::Dir(l.join("CrashDumps"), None));
            }
            t.push(Target::Dir(win.join("Minidump"), None));
            t.push(Target::File(win.join("MEMORY.DMP")));
        }
        "error_reports" => {
            // Only the per-user WER folders. The ProgramData ones are writable by every user,
            // so another account could redirect them; cleaning them as admin isn't safe.
            if let Some(l) = &local {
                for s in ["ReportArchive", "ReportQueue", "Temp"] {
                    t.push(Target::Dir(l.join(r"Microsoft\Windows\WER").join(s), None));
                }
            }
        }
        "shader_cache" => {
            if let Some(l) = &local {
                for s in [
                    "D3DSCache",
                    r"NVIDIA\DXCache",
                    r"NVIDIA\GLCache",
                    r"NVIDIA\OptixCache",
                    r"AMD\DxCache",
                    r"AMD\DxcCache",
                    r"AMD\VkCache",
                    r"AMD\GLCache",
                    r"Intel\ShaderCache",
                ] {
                    t.push(Target::Dir(l.join(s), None));
                }
            }
        }
        // Files younger than a day may belong to an update that is installing right now.
        "update_cache" => t.push(Target::Dir(win.join(r"SoftwareDistribution\Download"), Some(Duration::from_secs(24 * 3600)))),
        "delivery_opt" => t.push(Target::DeliveryOpt(
            win.join(r"ServiceProfiles\NetworkService\AppData\Local\Microsoft\Windows\DeliveryOptimization\Cache"),
        )),
        "recycle_bin" => t.push(Target::RecycleBin),
        _ => {}
    }
    t
}

fn recycle_bin_stats() -> Stats {
    use windows_sys::Win32::UI::Shell::{SHQueryRecycleBinW, SHQUERYRBINFO};
    let mut info = SHQUERYRBINFO {
        cbSize: std::mem::size_of::<SHQUERYRBINFO>() as u32,
        i64Size: 0,
        i64NumItems: 0,
    };
    let hr = unsafe { SHQueryRecycleBinW(std::ptr::null(), &mut info) };
    if hr < 0 {
        return Stats::default();
    }
    Stats { size: info.i64Size.max(0) as u64, files: info.i64NumItems.max(0) as u64 }
}

fn empty_recycle_bin() -> Deleted {
    use windows_sys::Win32::UI::Shell::{SHEmptyRecycleBinW, SHERB_NOCONFIRMATION, SHERB_NOPROGRESSUI, SHERB_NOSOUND};
    let before = recycle_bin_stats();
    unsafe {
        SHEmptyRecycleBinW(std::ptr::null_mut(), std::ptr::null(), SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND);
    }
    let after = recycle_bin_stats();
    Deleted {
        freed: before.size.saturating_sub(after.size),
        deleted: before.files.saturating_sub(after.files),
        skipped: after.files,
    }
}

fn target_stats(t: &Target) -> Stats {
    match t {
        Target::Dir(p, age) => fsops::stats(p, *age),
        Target::Files(dir, pre, suf) => {
            let f = fsops::matching_files(dir, pre, suf);
            Stats { size: f.iter().map(|x| x.1).sum(), files: f.len() as u64 }
        }
        Target::File(p) => match std::fs::metadata(p) {
            Ok(m) if m.is_file() => Stats { size: m.len(), files: 1 },
            _ => Stats::default(),
        },
        Target::RecycleBin => recycle_bin_stats(),
        Target::DeliveryOpt(p) => fsops::stats(p, None),
    }
}

fn target_clean(t: &Target) -> Deleted {
    match t {
        Target::Dir(p, age) => fsops::delete_contents(p, *age),
        Target::Files(dir, pre, suf) => {
            let mut d = Deleted::default();
            for (f, len) in fsops::matching_files(dir, pre, suf) {
                if std::fs::remove_file(&f).is_ok() {
                    d.freed += len;
                    d.deleted += 1;
                } else {
                    d.skipped += 1;
                }
            }
            d
        }
        Target::File(p) => {
            let len = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
            if len > 0 && std::fs::remove_file(p).is_ok() {
                Deleted { freed: len, deleted: 1, skipped: 0 }
            } else if len > 0 {
                Deleted { skipped: 1, ..Default::default() }
            } else {
                Deleted::default()
            }
        }
        Target::RecycleBin => empty_recycle_bin(),
        Target::DeliveryOpt(p) => {
            let before = fsops::stats(p, None);
            // The supported way; falls back to deleting the cache folder contents.
            if util::ps("Delete-DeliveryOptimizationCache -Force").is_err() {
                fsops::delete_contents(p, None);
            }
            let after = fsops::stats(p, None);
            Deleted {
                freed: before.size.saturating_sub(after.size),
                deleted: before.files.saturating_sub(after.files),
                skipped: after.files,
            }
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Category {
    id: String,
    size: u64,
    files: u64,
    needs_admin: bool,
    available: bool,
}

#[tauri::command]
pub async fn cleanup_scan() -> Res<Vec<Category>> {
    util::blocking(|| {
        let admin = util::is_admin();
        Ok(IDS
            .par_iter()
            .map(|id| {
                let ts = targets(id);
                let mut st = Stats::default();
                if !(needs_admin(id) && !admin) {
                    for t in &ts {
                        let s = target_stats(t);
                        st.size += s.size;
                        st.files += s.files;
                    }
                }
                Category {
                    id: (*id).to_string(),
                    size: st.size,
                    files: st.files,
                    needs_admin: needs_admin(id),
                    available: !ts.is_empty(),
                }
            })
            .collect())
    })
    .await
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Progress {
    id: String,
    phase: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PerCategory {
    id: String,
    freed: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanResult {
    freed: u64,
    deleted: u64,
    skipped: u64,
    per_category: Vec<PerCategory>,
}

#[tauri::command]
pub async fn cleanup_run(app: AppHandle, ids: Vec<String>) -> Res<CleanResult> {
    let data_dir = app.state::<AppState>().data_dir.clone();
    util::blocking(move || {
        let admin = util::is_admin();
        let mut total = Deleted::default();
        let mut per = Vec::new();
        for id in ids.iter().filter(|i| IDS.contains(&i.as_str())) {
            let _ = app.emit("cleanup://progress", Progress { id: id.clone(), phase: "start" });
            let mut d = Deleted::default();
            if !(needs_admin(id) && !admin) {
                for t in targets(id) {
                    d += target_clean(&t);
                }
            }
            total += d;
            per.push(PerCategory { id: id.clone(), freed: d.freed });
            let _ = app.emit("cleanup://progress", Progress { id: id.clone(), phase: "done" });
        }
        if total.freed > 0 {
            add_history(&data_dir, total.freed);
        }
        Ok(CleanResult { freed: total.freed, deleted: total.deleted, skipped: total.skipped, per_category: per })
    })
    .await
}

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct History {
    total_freed: u64,
    runs: Vec<HistoryEntry>,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct HistoryEntry {
    at: u64,
    freed: u64,
}

fn history_path(dir: &Path) -> PathBuf {
    dir.join("history.json")
}

fn read_history(dir: &Path) -> History {
    std::fs::read_to_string(history_path(dir))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn add_history(dir: &Path, freed: u64) {
    let mut h = read_history(dir);
    h.total_freed = h.total_freed.saturating_add(freed);
    let at = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    h.runs.push(HistoryEntry { at, freed });
    if h.runs.len() > 100 {
        let cut = h.runs.len() - 100;
        h.runs.drain(0..cut);
    }
    let _ = write_json(&history_path(dir), &h);
}

#[tauri::command]
pub fn history_get(state: State<'_, AppState>) -> History {
    read_history(&state.data_dir)
}
