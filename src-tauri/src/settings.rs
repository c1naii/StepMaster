//! App settings (JSON in the data folder), autostart via Task Scheduler, global hotkey.
use crate::util::{self, tr, Res};
use crate::AppState;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter, Manager, State};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub language: String,
    pub autostart: bool,
    pub start_minimized: bool,
    pub close_to_tray: bool,
    pub motion: String,
    pub hotkey: String,
    pub tray_monitor: bool,
    /// Show the update popup (bottom-right) even when the window isn't open.
    pub update_notify: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: "ru".into(),
            autostart: false,
            start_minimized: false,
            close_to_tray: true,
            motion: "full".into(),
            hotkey: "Ctrl+Shift+Space".into(),
            tray_monitor: true,
            update_notify: true,
        }
    }
}

impl Settings {
    fn sanitized(mut self) -> Self {
        if !crate::util::LANGS.contains(&self.language.as_str()) {
            self.language = "ru".into();
        }
        if !["full", "reduced", "off"].contains(&self.motion.as_str()) {
            self.motion = "full".into();
        }
        if self.hotkey.len() > 40 {
            self.hotkey = String::new();
        }
        self
    }
}

fn settings_path(dir: &Path) -> PathBuf {
    dir.join("settings.json")
}

pub fn load(dir: &Path) -> Settings {
    std::fs::read_to_string(settings_path(dir))
        .ok()
        .and_then(|s| serde_json::from_str::<Settings>(&s).ok())
        .unwrap_or_default()
        .sanitized()
}

/// Atomic write: temp file + rename, so a crash never leaves a broken settings file.
pub fn write_json<T: Serialize>(path: &Path, value: &T) -> Res<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("tmp");
    let text = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

const TASK_NAME: &str = "StepMaster";

/// Registers (or removes) a logon task that starts the app elevated without a UAC prompt.
/// Task Scheduler is used instead of the Run key because the app requires admin rights.
pub fn apply_autostart(enabled: bool) -> Res<()> {
    if enabled {
        register_logon_task(TASK_NAME, "StepMaster autostart", "--autostart", "PT10S", "PT0S")
    } else {
        delete_task(TASK_NAME)
    }
}

/// Removes a scheduled task if it exists.
pub fn delete_task(name: &str) -> Res<()> {
    let (exists, _, _) = util::run_hidden("schtasks.exe", &["/Query", "/TN", name])?;
    if !exists {
        return Ok(());
    }
    let (ok, out, err) = util::run_hidden("schtasks.exe", &["/Delete", "/TN", name, "/F"])?;
    if !ok {
        return Err(if err.trim().is_empty() { out } else { err });
    }
    Ok(())
}

/// Registers a task that starts this exe elevated at the current user's logon, without a UAC
/// prompt. `delay` / `limit` are ISO 8601 durations ("PT10S"; "PT0S" = no time limit).
pub fn register_logon_task(name: &str, description: &str, args: &str, delay: &str, limit: &str) -> Res<()> {
    if !util::is_admin() {
        return Err(util::need_admin());
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let user = format!(
        "{}\\{}",
        std::env::var("USERDOMAIN").unwrap_or_default(),
        std::env::var("USERNAME").unwrap_or_default()
    );
    let xml_escape = |s: &str| s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;");
    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo><Description>{description}</Description></RegistrationInfo>
  <Triggers>
    <LogonTrigger><Enabled>true</Enabled><UserId>{user}</UserId><Delay>{delay}</Delay></LogonTrigger>
  </Triggers>
  <Principals>
    <Principal id="Author"><UserId>{user}</UserId><LogonType>InteractiveToken</LogonType><RunLevel>HighestAvailable</RunLevel></Principal>
  </Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <AllowHardTerminate>true</AllowHardTerminate>
    <StartWhenAvailable>false</StartWhenAvailable>
    <RunOnlyIfNetworkAvailable>false</RunOnlyIfNetworkAvailable>
    <IdleSettings><StopOnIdleEnd>false</StopOnIdleEnd><RestartOnIdle>false</RestartOnIdle></IdleSettings>
    <AllowStartOnDemand>true</AllowStartOnDemand>
    <Enabled>true</Enabled>
    <Hidden>false</Hidden>
    <RunOnlyIfIdle>false</RunOnlyIfIdle>
    <ExecutionTimeLimit>{limit}</ExecutionTimeLimit>
    <Priority>7</Priority>
  </Settings>
  <Actions Context="Author">
    <Exec><Command>{exe}</Command><Arguments>{args}</Arguments></Exec>
  </Actions>
</Task>"#,
        description = xml_escape(description),
        user = xml_escape(&user),
        delay = xml_escape(delay),
        limit = xml_escape(limit),
        exe = xml_escape(&exe.to_string_lossy()),
        args = xml_escape(args),
    );
    // Registered straight from memory: no temp file another process could swap.
    util::ps(&format!(
        "Register-ScheduledTask -TaskName {} -Xml {} -Force | Out-Null",
        util::psq(name),
        util::psq(&xml)
    ))
    .map(|_| ())
}

/// (Re)registers the global hotkey. Empty string = off.
pub fn apply_hotkey(app: &AppHandle, hotkey: &str) -> Res<()> {
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    if hotkey.trim().is_empty() {
        return Ok(());
    }
    let sc: Shortcut = hotkey
        .parse()
        .map_err(|_| tr("Неверная горячая клавиша", "Invalid hotkey"))?;
    gs.register(sc).map_err(|e| {
        tr(
            "Сочетание занято другой программой",
            "This shortcut is used by another app",
        ) + &format!(" ({e})")
    })
}

#[tauri::command]
pub fn settings_get(state: State<'_, AppState>) -> Settings {
    state.settings.read().clone()
}

#[tauri::command]
pub async fn settings_set(app: AppHandle, settings: Settings) -> Res<Settings> {
    let state = app.state::<AppState>();
    let new = settings.sanitized();
    let old = state.settings.read().clone();

    // Hotkey first (cheap, may fail), then autostart. On any failure nothing is half-applied.
    if new.hotkey != old.hotkey {
        if let Err(e) = apply_hotkey(&app, &new.hotkey) {
            let _ = apply_hotkey(&app, &old.hotkey);
            return Err(e);
        }
    }
    if new.autostart != old.autostart {
        let enable = new.autostart;
        if let Err(e) = util::blocking(move || apply_autostart(enable)).await {
            if new.hotkey != old.hotkey {
                let _ = apply_hotkey(&app, &old.hotkey);
            }
            return Err(e);
        }
    }
    if new.update_notify != old.update_notify {
        let enable = new.update_notify;
        util::blocking(move || crate::notify::apply_task(enable)).await?;
    }
    if new.language != old.language {
        *util::LANG.write() = new.language.clone();
        crate::tray::refresh_menu(&app);
    }
    write_json(&settings_path(&state.data_dir), &new)?;
    *state.settings.write() = new.clone();
    let _ = app.emit("settings://changed", &new);
    Ok(new)
}
