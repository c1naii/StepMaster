//! Microsoft Defender status/scan/update, firewall profiles, update history.
use crate::util::{self, tr, Res};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Defender {
    available: bool,
    antivirus_enabled: bool,
    realtime: bool,
    signature_updated: String,
    signature_age_days: u32,
    last_quick_scan: String,
    threats: u32,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct FirewallProfile {
    name: String,
    enabled: bool,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Status {
    defender: Defender,
    other_antivirus: Vec<String>,
    firewall: Vec<FirewallProfile>,
}

#[tauri::command]
pub async fn protection_status() -> Res<Status> {
    util::blocking(|| {
        util::ps_json(
            r#"
$d = [pscustomobject]@{ available=$false; antivirusEnabled=$false; realtime=$false; signatureUpdated=''; signatureAgeDays=0; lastQuickScan=''; threats=0 }
try {
  $mp = Get-MpComputerStatus -ErrorAction Stop
  $d.available = [bool]$mp.AMServiceEnabled
  $d.antivirusEnabled = [bool]$mp.AntivirusEnabled
  $d.realtime = [bool]$mp.RealTimeProtectionEnabled
  if ($mp.AntivirusSignatureLastUpdated) { $d.signatureUpdated = $mp.AntivirusSignatureLastUpdated.ToString('yyyy-MM-dd HH:mm') }
  $age = [int]$mp.AntivirusSignatureAge; if ($age -lt 0 -or $age -gt 100000) { $age = 0 }; $d.signatureAgeDays = $age
  if ($mp.QuickScanEndTime) { $d.lastQuickScan = $mp.QuickScanEndTime.ToString('yyyy-MM-dd HH:mm') }
  try { $d.threats = @(Get-MpThreat -ErrorAction Stop | Where-Object { $_.IsActive }).Count } catch {}
} catch {}
$fw = @()
try { $fw = @(Get-NetFirewallProfile -ErrorAction Stop | ForEach-Object { [pscustomobject]@{ name=[string]$_.Name; enabled=([string]$_.Enabled -eq 'True') } }) } catch {}
$av = @()
try {
  $av = @(Get-CimInstance -Namespace root/SecurityCenter2 -ClassName AntiVirusProduct -ErrorAction Stop |
    Where-Object { $_.displayName -notmatch 'Defender' -and ((([int]$_.productState) -band 0x1000) -ne 0) } |
    ForEach-Object { [string]$_.displayName })
} catch {}
[pscustomobject]@{ defender=$d; otherAntivirus=$av; firewall=$fw } | ConvertTo-Json -Depth 4 -Compress
"#,
        )
    })
    .await
}

#[tauri::command]
pub async fn defender_quick_scan() -> Res<()> {
    util::blocking(|| util::ps("Start-MpScan -ScanType QuickScan").map(|_| ())).await
}

#[tauri::command]
pub async fn defender_update() -> Res<()> {
    util::blocking(|| util::ps("Update-MpSignature").map(|_| ())).await
}

#[tauri::command]
pub async fn firewall_set(enabled: bool) -> Res<()> {
    if !util::is_admin() {
        return Err(util::need_admin());
    }
    let v = if enabled { "True" } else { "False" };
    util::blocking(move || {
        util::ps(&format!("Set-NetFirewallProfile -Profile Domain,Public,Private -Enabled {v}"))
            .map(|_| ())
            .map_err(|e| format!("{}: {e}", tr("Не удалось изменить брандмауэр", "Couldn't change the firewall")))
    })
    .await
}

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct HotFix {
    id: String,
    description: String,
    installed_on: String,
}

#[tauri::command]
pub async fn updates_history() -> Res<Vec<HotFix>> {
    util::blocking(|| {
        util::ps_json(
            r#"ConvertTo-Json -Compress -InputObject @(Get-HotFix -ErrorAction SilentlyContinue | Sort-Object { $_.InstalledOn } -Descending | Select-Object -First 30 | ForEach-Object {
                [pscustomobject]@{ id=[string]$_.HotFixID; description=[string]$_.Description;
                  installedOn=$(if ($_.InstalledOn) { $_.InstalledOn.ToString('yyyy-MM-dd') } else { '' }) } })"#,
        )
    })
    .await
}
