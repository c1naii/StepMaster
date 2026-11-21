//! Windows services and System Restore.
use crate::util::{self, psq, tr, Res};
use crate::journal::{self, Undo};
use serde::{Deserialize, Serialize};

/// Services the app refuses to stop or reconfigure.
const CRITICAL: &[&str] = &[
    "rpcss", "rpceptmapper", "dcomlaunch", "lsm", "samss", "eventlog", "plugplay", "power", "profsvc",
    "schedule", "bfe", "mpssvc", "windefend", "wdnissvc", "securityhealthservice", "cryptsvc", "dhcp",
    "dnscache", "nsi", "winmgmt", "audiosrv", "audioendpointbuilder", "brokerinfrastructure",
    "coremessagingregistrar", "gpsvc", "lanmanworkstation", "netprofm", "nlasvc", "sens", "systemeventsbroker",
    "themes", "userManager", "usermanager", "wlansvc", "wscsvc", "trustedinstaller", "msiserver", "vds",
    "vaultsvc", "keyiso", "appinfo", "staterepository", "tokenbroker", "termservice", "sgrmbroker",
    "eventsystem", "storsvc", "timebrokersvc", "shellhwdetection", "cdpusersvc", "wpnusersvc", "onesyncsvc",
    // Display policy (screen setup) and the connection manager (Wi-Fi / Ethernet choice).
    "dispbrokerdesktopsvc", "wcmsvc",
];

pub(crate) fn is_critical(name: &str) -> bool {
    let n = name.to_lowercase();
    // Per-user copies are named like "CDPUserSvc_1a2b3c": check the part before "_".
    let base = n.split('_').next().unwrap_or(&n);
    CRITICAL.contains(&n.as_str()) || CRITICAL.contains(&base)
}

pub(crate) fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 256
        && name.chars().all(|c| c.is_alphanumeric() || "_-.@ $".contains(c))
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceInfo {
    #[serde(default)]
    name: String,
    #[serde(default)]
    display_name: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    start_mode: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    critical: bool,
}

#[tauri::command]
pub async fn services_list() -> Res<Vec<ServiceInfo>> {
    util::blocking(|| {
        let mut list: Vec<ServiceInfo> = util::ps_json_timeout(
            r#"ConvertTo-Json -Compress -InputObject @(Get-CimInstance Win32_Service | ForEach-Object {
                [pscustomobject]@{ name=[string]$_.Name; displayName=[string]$_.DisplayName;
                  status=$(if ($_.State -eq 'Running') { 'Running' } else { [string]$_.State });
                  startMode=[string]$_.StartMode; description=[string]$_.Description } })"#,
            60,
        )?;
        for s in &mut list {
            s.critical = is_critical(&s.name);
            if s.display_name.is_empty() {
                s.display_name = s.name.clone();
            }
        }
        Ok(list)
    })
    .await
}

fn guard(name: &str) -> Res<()> {
    if !valid_name(name) {
        return Err(tr("Неверное имя службы", "Invalid service name"));
    }
    if is_critical(name) {
        return Err(tr("Эта служба важна для Windows — менять её нельзя", "This service is critical for Windows"));
    }
    if !util::is_admin() {
        return Err(util::need_admin());
    }
    Ok(())
}

#[tauri::command]
pub async fn service_action(app: tauri::AppHandle, name: String, action: String) -> Res<()> {
    guard(&name)?;
    let cmd = match action.as_str() {
        "start" => format!("Start-Service -Name {}", psq(&name)),
        "stop" => format!("Stop-Service -Name {} -Force", psq(&name)),
        "restart" => format!("Restart-Service -Name {} -Force", psq(&name)),
        _ => return Err(tr("Неизвестное действие", "Unknown action")),
    };
    util::blocking(move || {
        let was_running = util::ps_timeout(&format!("[string](Get-Service -Name {}).Status", psq(&name)), 30)
            .map(|s| s.trim().eq_ignore_ascii_case("running"))
            .ok();
        util::ps_timeout(&cmd, 120)?;
        // Undo puts the service back the way it was (restart changes nothing to undo).
        let undo = match (action.as_str(), was_running) {
            ("stop", Some(true)) => Some(Undo::ServiceRun { name: name.clone(), run: true }),
            ("start", Some(false)) => Some(Undo::ServiceRun { name: name.clone(), run: false }),
            _ => None,
        };
        journal::add(&app, &format!("service_{action}"), &name, "", undo);
        Ok(())
    })
    .await
}

/// Current start type of a service as `sc config` takes it (auto / delayed-auto / demand / disabled).
pub(crate) fn start_type(name: &str) -> Option<String> {
    use winreg::{enums::HKEY_LOCAL_MACHINE, RegKey};
    let k = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey(format!(r"SYSTEM\CurrentControlSet\Services\{name}")).ok()?;
    let start: u32 = k.get_value("Start").ok()?;
    let delayed: u32 = k.get_value("DelayedAutostart").unwrap_or(0);
    Some(
        match (start, delayed) {
            (2, 1) => "delayed-auto",
            (2, _) => "auto",
            (3, _) => "demand",
            (4, _) => "disabled",
            _ => return None,
        }
        .to_string(),
    )
}

#[tauri::command]
pub async fn service_start_mode(app: tauri::AppHandle, name: String, mode: String) -> Res<()> {
    guard(&name)?;
    let m = match mode.as_str() {
        "auto" => "Automatic",
        "manual" => "Manual",
        "disabled" => "Disabled",
        _ => return Err(tr("Неизвестный режим", "Unknown mode")),
    };
    let cmd = format!("Set-Service -Name {} -StartupType {m}", psq(&name));
    util::blocking(move || {
        let before = start_type(&name);
        util::ps_timeout(&cmd, 120)?;
        let undo = before.map(|mode| Undo::ServiceMode { name: name.clone(), mode });
        journal::add(&app, "service_mode", &name, &mode, undo);
        Ok(())
    })
    .await
}

/* ---------------- System Restore ---------------- */

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestorePoint {
    #[serde(default)]
    sequence: u32,
    #[serde(default)]
    description: String,
    #[serde(default)]
    created_at: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreStatus {
    enabled: bool,
    points: Vec<RestorePoint>,
}

fn restore_enabled() -> bool {
    use winreg::enums::*;
    let k = winreg::RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey_with_flags(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\SystemRestore", KEY_READ | KEY_WOW64_64KEY);
    match k {
        Ok(k) => {
            let disabled_policy = winreg::RegKey::predef(HKEY_LOCAL_MACHINE)
                .open_subkey(r"SOFTWARE\Policies\Microsoft\Windows NT\SystemRestore")
                .and_then(|p| p.get_value::<u32, _>("DisableSR"))
                .map(|v| v == 1)
                .unwrap_or(false);
            !disabled_policy && k.get_value::<u32, _>("RPSessionInterval").map(|v| v != 0).unwrap_or(true)
        }
        Err(_) => false,
    }
}

fn list_points() -> Res<Vec<RestorePoint>> {
    util::ps_json(
        r#"ConvertTo-Json -Compress -InputObject @(Get-ComputerRestorePoint -ErrorAction SilentlyContinue | ForEach-Object {
            [pscustomobject]@{ sequence=[int]$_.SequenceNumber; description=[string]$_.Description;
              createdAt=([Management.ManagementDateTimeConverter]::ToDateTime($_.CreationTime)).ToString('yyyy-MM-dd HH:mm') } })"#,
    )
}

#[tauri::command]
pub async fn restore_status() -> Res<RestoreStatus> {
    if !util::is_admin() {
        return Err(util::need_admin());
    }
    util::blocking(|| Ok(RestoreStatus { enabled: restore_enabled(), points: list_points()? })).await
}

/// Returns "created" or "skipped" (Windows allows one point per 24h by default).
pub fn create_point(description: &str) -> Res<String> {
    if !util::is_admin() {
        return Err(util::need_admin());
    }
    let desc: String = description.chars().filter(|c| !c.is_control()).take(120).collect();
    let before = list_points().map(|p| p.len()).unwrap_or(0);
    util::ps(&format!(
        "Checkpoint-Computer -Description {} -RestorePointType MODIFY_SETTINGS -WarningAction SilentlyContinue",
        psq(&desc)
    ))?;
    let after = list_points().map(|p| p.len()).unwrap_or(0);
    Ok(if after > before { "created".into() } else { "skipped".into() })
}

#[tauri::command]
pub async fn restore_create(app: tauri::AppHandle, description: String) -> Res<String> {
    util::blocking(move || {
        let r = create_point(&description)?;
        if r == "created" {
            journal::add(&app, "restore_point", &description, "", None);
        }
        Ok(r)
    })
    .await
}

#[tauri::command]
pub async fn restore_enable() -> Res<()> {
    if !util::is_admin() {
        return Err(util::need_admin());
    }
    util::blocking(|| util::ps(r#"Enable-ComputerRestore -Drive "$env:SystemDrive\""#).map(|_| ())).await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn names() {
        assert!(valid_name("wuauserv"));
        assert!(valid_name("MSSQL$SQLEXPRESS"));
        assert!(!valid_name("x'; Remove-Item C:\\"));
        assert!(!valid_name(""));
        assert!(is_critical("RpcSs"));
        assert!(!is_critical("Spooler"));
    }
}
