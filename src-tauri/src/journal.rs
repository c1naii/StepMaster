//! "What StepMaster changed": every change to the system is written here, with what is needed
//! to put it back. Entries without `undo` are for information only (cleanup, uninstall…).
use crate::settings::write_json;
use crate::util::{self, psq, tr, Res};
use crate::AppState;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager};

const MAX_ENTRIES: usize = 300;
static LOCK: Mutex<()> = Mutex::new(());

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Undo {
    /// Previous start type, as `sc config` takes it: auto / delayed-auto / demand / disabled.
    ServiceMode { name: String, mode: String },
    /// Start (true) or stop (false) the service again.
    ServiceRun { name: String, run: bool },
    Startup { id: String, enabled: bool },
    /// Per adapter (ifIndex): the static DNS list it had, or "" for automatic (DHCP).
    Dns { adapters: Vec<(u32, String)> },
    /// Firewall profile name → was it on.
    Firewall { profiles: Vec<(String, bool)> },
    /// Previous registry value of the GPU preference for this exe (None = there was none).
    GpuPref { exe: String, value: Option<String> },
    GameMode,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: u64,
    /// Unix seconds.
    pub at: u64,
    /// What happened; the UI turns it into text (journal.kinds.<kind>).
    pub kind: String,
    /// What it happened to (service, program, file…), shown as is.
    pub target: String,
    /// Extra value for the text (new mode, size…).
    pub detail: String,
    pub undo: Option<Undo>,
    pub reverted: bool,
}

/// The journal holds commands that run elevated on "Put back", so it must not live in the
/// user's own (writable without admin rights) data folder: it goes to C:\ProgramData\StepMaster,
/// owned by Administrators, writable only by Administrators and SYSTEM.
fn secure_dir() -> Option<PathBuf> {
    static DIR: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
    DIR.get_or_init(|| {
        if !util::is_admin() {
            return None;
        }
        let drive = util::system_root().ancestors().last()?.to_path_buf();
        let dir = drive.join("ProgramData").join("StepMaster");
        std::fs::create_dir_all(&dir).ok()?;
        let d = dir.to_string_lossy().to_string();
        // Take ownership first (a folder planted by someone else must not stay theirs), then
        // replace the whole ACL: Administrators + SYSTEM full, Users read only.
        let (own, _, _) = util::run_hidden("icacls.exe", &[&d, "/setowner", "*S-1-5-32-544", "/T", "/C", "/Q"]).ok()?;
        let (acl, _, _) = util::run_hidden(
            "icacls.exe",
            &[&d, "/inheritance:r", "/grant:r", "*S-1-5-32-544:(OI)(CI)F", "*S-1-5-18:(OI)(CI)F", "*S-1-5-32-545:(OI)(CI)RX", "/T", "/C", "/Q"],
        )
        .ok()?;
        (own && acl).then_some(dir)
    })
    .clone()
}

/// Where the journal file lives. Without the protected folder (no admin rights) nothing can be
/// put back anyway, so the app's own folder is used just to show the list.
fn store(dir: &Path) -> PathBuf {
    if cfg!(test) {
        return dir.to_path_buf();
    }
    secure_dir().unwrap_or_else(|| dir.to_path_buf())
}

fn path(dir: &Path) -> PathBuf {
    store(dir).join("journal.json")
}

/// Removes the protected folder (uninstall with "remove data").
pub fn remove_store() {
    if let Some(d) = secure_dir() {
        let _ = std::fs::remove_dir_all(d);
    }
}

/// Entries one by one: an entry this version doesn't understand is skipped, not the whole list.
/// A file that isn't a JSON list at all is kept as journal.bad.json instead of being overwritten.
fn load(dir: &Path) -> Vec<Entry> {
    let p = path(dir);
    let Ok(text) = std::fs::read_to_string(&p) else { return Vec::new() };
    match serde_json::from_str::<Vec<serde_json::Value>>(&text) {
        Ok(items) => items.into_iter().filter_map(|v| serde_json::from_value(v).ok()).collect(),
        Err(_) => {
            let _ = std::fs::rename(&p, p.with_file_name("journal.bad.json"));
            Vec::new()
        }
    }
}

/// Marks every not yet reverted entry with this kind of undo as reverted (e.g. game mode was
/// turned off on its own page, so older "turn it off" rows must not offer it again).
pub fn mark_reverted(dir: &Path, which: impl Fn(&Undo) -> bool) {
    let _g = LOCK.lock();
    let mut list = load(dir);
    let mut changed = false;
    for e in list.iter_mut().filter(|e| !e.reverted && e.undo.as_ref().is_some_and(&which)) {
        e.reverted = true;
        changed = true;
    }
    if changed {
        let _ = write_json(&path(dir), &list);
    }
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Adds an entry (newest first). Never fails the action it describes.
pub fn add_at(dir: &Path, kind: &str, target: &str, detail: &str, undo: Option<Undo>) {
    let _g = LOCK.lock();
    let mut list = load(dir);
    let id = list.iter().map(|e| e.id).max().unwrap_or(0) + 1;
    list.insert(0, Entry { id, at: now(), kind: kind.into(), target: target.into(), detail: detail.into(), undo, reverted: false });
    list.truncate(MAX_ENTRIES);
    let _ = write_json(&path(dir), &list);
}

pub fn add(app: &AppHandle, kind: &str, target: &str, detail: &str, undo: Option<Undo>) {
    add_at(&app.state::<AppState>().data_dir, kind, target, detail, undo);
}

#[tauri::command]
pub fn journal_get(state: tauri::State<'_, AppState>) -> Vec<Entry> {
    load(&state.data_dir)
}

#[tauri::command]
pub async fn journal_revert(app: AppHandle, id: u64) -> Res<Vec<Entry>> {
    let dir = app.state::<AppState>().data_dir.clone();
    util::blocking(move || {
        let entry = load(&dir).into_iter().find(|e| e.id == id).ok_or_else(|| tr("Запись не найдена", "Entry not found"))?;
        if entry.reverted {
            return Ok(load(&dir));
        }
        let undo = entry.undo.clone().ok_or_else(|| tr("Это действие нельзя отменить", "This action can't be undone"))?;
        apply(&dir, &undo)?;
        let _g = LOCK.lock();
        let mut list = load(&dir);
        if let Some(e) = list.iter_mut().find(|e| e.id == id) {
            e.reverted = true;
        }
        write_json(&path(&dir), &list)?;
        Ok(list)
    })
    .await
}

fn need_admin() -> Res<()> {
    if util::is_admin() {
        Ok(())
    } else {
        Err(util::need_admin())
    }
}

fn apply(dir: &Path, undo: &Undo) -> Res<()> {
    match undo {
        Undo::ServiceMode { name, mode } => {
            need_admin()?;
            // Same rules as the forward action: never touch services Windows depends on.
            if crate::services::is_critical(name) {
                return Err(tr("Эта служба важна для Windows — менять её нельзя", "This service is critical for Windows"));
            }
            if !crate::services::valid_name(name) || !["auto", "delayed-auto", "demand", "disabled"].contains(&mode.as_str()) {
                return Err(tr("Неверная запись", "Invalid entry"));
            }
            let (ok, out, err) = util::run_hidden("sc.exe", &["config", name, "start=", mode])?;
            if ok {
                Ok(())
            } else {
                Err(format!("{} {}", out.trim(), err.trim()).trim().to_string())
            }
        }
        Undo::ServiceRun { name, run } => {
            need_admin()?;
            if !crate::services::valid_name(name) || crate::services::is_critical(name) {
                return Err(tr("Неверная запись", "Invalid entry"));
            }
            let cmd = if *run { format!("Start-Service -Name {}", psq(name)) } else { format!("Stop-Service -Name {} -Force", psq(name)) };
            util::ps_timeout(&cmd, 60).map(|_| ())
        }
        Undo::Startup { id, enabled } => crate::startup::set_enabled(id, *enabled),
        Undo::Dns { adapters } => {
            need_admin()?;
            let mut script = String::new();
            for (idx, ns) in adapters {
                let servers: Vec<String> = ns
                    .split([',', ' '])
                    .map(str::trim)
                    .filter(|s| !s.is_empty() && s.parse::<std::net::IpAddr>().is_ok())
                    .map(psq)
                    .collect();
                if servers.is_empty() {
                    script.push_str(&format!("Set-DnsClientServerAddress -InterfaceIndex {idx} -ResetServerAddresses; "));
                } else {
                    script.push_str(&format!("Set-DnsClientServerAddress -InterfaceIndex {idx} -ServerAddresses @({}); ", servers.join(",")));
                }
            }
            script.push_str("Clear-DnsClientCache");
            util::ps_timeout(&script, 60).map(|_| ())
        }
        Undo::Firewall { profiles } => {
            need_admin()?;
            let mut script = String::new();
            for (name, on) in profiles {
                if !["Domain", "Private", "Public"].contains(&name.as_str()) {
                    continue;
                }
                script.push_str(&format!("Set-NetFirewallProfile -Profile {name} -Enabled {}; ", if *on { "True" } else { "False" }));
            }
            if script.is_empty() {
                return Ok(());
            }
            util::ps_timeout(&script, 60).map(|_| ())
        }
        Undo::GpuPref { exe, value } => crate::games::gpu_pref_restore(exe, value.as_deref()),
        Undo::GameMode => {
            if !crate::games::is_active(dir) {
                return Err(tr("Игровой режим уже выключен", "Game mode is already off"));
            }
            crate::games::revert_at(dir).map(|_| ())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_keeps_newest_first_and_caps() {
        let dir = std::env::temp_dir().join(format!("sm-journal-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let _ = std::fs::remove_file(path(&dir));
        add_at(&dir, "cleanup", "", "10", None);
        add_at(&dir, "startup_off", "App", "", Some(Undo::Startup { id: "hkcu|App".into(), enabled: true }));
        let list = load(&dir);
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].kind, "startup_off");
        assert_eq!(list[0].id, 2);
        for _ in 0..MAX_ENTRIES + 5 {
            add_at(&dir, "cleanup", "", "1", None);
        }
        assert_eq!(load(&dir).len(), MAX_ENTRIES);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn undo_roundtrips_as_json() {
        let u = Undo::Dns { adapters: vec![(12, "1.1.1.1,8.8.8.8".into()), (7, String::new())] };
        let s = serde_json::to_string(&u).unwrap();
        assert!(s.contains("\"type\":\"dns\""));
        assert_eq!(serde_json::from_str::<Undo>(&s).unwrap(), u);
    }
}
