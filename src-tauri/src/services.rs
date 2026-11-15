//! Windows services and System Restore.
use crate::util::{self, psq, tr, Res};
use serde::{Deserialize, Serialize};

/// Services the app refuses to stop or reconfigure.
const CRITICAL: &[&str] = &[
    "rpcss", "rpceptmapper", "dcomlaunch", "lsm", "samss", "eventlog", "plugplay", "power", "profsvc",
    "schedule", "bfe", "mpssvc", "windefend", "wdnissvc", "securityhealthservice", "cryptsvc", "dhcp",
    "dnscache", "nsi", "winmgmt", "audiosrv", "audioendpointbuilder", "brokerinfrastructure",
    "coremessagingregistrar", "gpsvc", "lanmanworkstation", "netprofm", "nlasvc", "sens", "systemeventsbroker",
    "themes", "userManager", "usermanager", "wlansvc", "wscsvc", "trustedinstaller", "msiserver", "vds",
    "vaultsvc", "keyiso", "appinfo", "staterepository", "tokenbroker", "termservice", "sgrmbroker",
];

fn is_critical(name: &str) -> bool {
    CRITICAL.contains(&name.to_lowercase().as_str())
}

fn valid_name(name: &str) -> bool {
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
        let mut list: Vec<ServiceInfo> = util::ps_json(
            r#"ConvertTo-Json -Compress -InputObject @(Get-CimInstance Win32_Service | ForEach-Object {
                [pscustomobject]@{ name=[string]$_.Name; displayName=[string]$_.DisplayName;
                  status=$(if ($_.State -eq 'Running') { 'Running' } else { [string]$_.State });
                  startMode=[string]$_.StartMode; description=[string]$_.Description } })"#,
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
pub async fn service_action(name: String, action: String) -> Res<()> {
    guard(&name)?;
    let cmd = match action.as_str() {
        "start" => format!("Start-Service -Name {}", psq(&name)),
        "stop" => format!("Stop-Service -Name {} -Force", psq(&name)),
        "restart" => format!("Restart-Service -Name {} -Force", psq(&name)),
        _ => return Err(tr("Неизвестное действие", "Unknown action")),
    };
    util::blocking(move || util::ps(&cmd).map(|_| ())).await
}

#[tauri::command]
pub async fn service_start_mode(name: String, mode: String) -> Res<()> {
    guard(&name)?;
    let m = match mode.as_str() {
        "auto" => "Automatic",
        "manual" => "Manual",
        "disabled" => "Disabled",
        _ => return Err(tr("Неизвестный режим", "Unknown mode")),
    };
    let cmd = format!("Set-Service -Name {} -StartupType {m}", psq(&name));
    util::blocking(move || util::ps(&cmd).map(|_| ())).await
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
pub async fn restore_create(description: String) -> Res<String> {
    util::blocking(move || create_point(&description)).await
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
