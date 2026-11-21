//! Network: adapters, DNS, reset, speed test (Cloudflare), connections, saved Wi-Fi passwords.
use crate::util::{self, tr, Res};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Adapter {
    name: String,
    description: String,
    status: String,
    kind: String,
    mac: String,
    ipv4: Vec<String>,
    gateway: String,
    dns: Vec<String>,
    link_speed: String,
}

#[tauri::command]
pub async fn network_adapters() -> Res<Vec<Adapter>> {
    util::blocking(|| {
        util::ps_json(
            r#"
$cfg = @(Get-NetIPConfiguration -ErrorAction SilentlyContinue)
ConvertTo-Json -Compress -Depth 4 -InputObject @(Get-NetAdapter -ErrorAction SilentlyContinue |
  Where-Object { $_.HardwareInterface -or $_.Status -eq 'Up' } | Sort-Object { $_.Status -ne 'Up' }, Name | ForEach-Object {
  $a = $_
  $c = $cfg | Where-Object { $_.InterfaceIndex -eq $a.ifIndex } | Select-Object -First 1
  $media = "$($a.PhysicalMediaType) $($a.NdisPhysicalMedium) $($a.InterfaceDescription) $($a.Name)"
  $kind = if ($media -match '802\.11|Wireless|Wi-?Fi|WLAN') { 'wifi' } elseif ($media -match '802\.3|Ethernet') { 'ethernet' } else { 'other' }
  [pscustomobject]@{
    name=[string]$a.Name; description=[string]$a.InterfaceDescription; status=[string]$a.Status; kind=$kind
    mac=[string]$a.MacAddress; linkSpeed=[string]$a.LinkSpeed
    ipv4=@($c.IPv4Address | Where-Object { $_ } | ForEach-Object { [string]$_.IPAddress })
    gateway=[string](@($c.IPv4DefaultGateway | ForEach-Object { $_.NextHop }) | Select-Object -First 1)
    dns=@($c.DNSServer | Where-Object { $_.AddressFamily -eq 2 } | ForEach-Object { $_.ServerAddresses } | ForEach-Object { [string]$_ })
  } })
"#,
        )
    })
    .await
}

#[tauri::command]
pub async fn dns_flush() -> Res<()> {
    util::blocking(|| {
        let (ok, out, err) = util::run_hidden("ipconfig.exe", &["/flushdns"])?;
        if ok {
            Ok(())
        } else {
            Err(if err.trim().is_empty() { out } else { err })
        }
    })
    .await
}

#[tauri::command]
pub async fn dns_set(app: AppHandle, preset: String) -> Res<()> {
    if !util::is_admin() {
        return Err(util::need_admin());
    }
    let servers = match preset.as_str() {
        "auto" => None,
        "cloudflare" => Some("'1.1.1.1','1.0.0.1'"),
        "google" => Some("'8.8.8.8','8.8.4.4'"),
        "quad9" => Some("'9.9.9.9','149.112.112.112'"),
        "adguard" => Some("'94.140.14.14','94.140.15.15'"),
        _ => return Err(tr("Неизвестный DNS", "Unknown DNS preset")),
    };
    let action = match servers {
        None => "Set-DnsClientServerAddress -InterfaceIndex $_.ifIndex -ResetServerAddresses".to_string(),
        Some(s) => format!("Set-DnsClientServerAddress -InterfaceIndex $_.ifIndex -ServerAddresses @({s})"),
    };
    let script = format!(
        // Only real network cards: changing DNS on a VPN/virtual adapter breaks the VPN or leaks DNS.
        // One adapter refusing must not stop the others.
        "$n = 0; $prev = @(); Get-NetAdapter | Where-Object {{ $_.Status -eq 'Up' -and \
         $_.InterfaceDescription -notmatch 'VPN|TAP-|WireGuard|Wintun|Loopback|\\bTUN\\b' }} | \
         ForEach-Object {{ $key = 'HKLM:\\SYSTEM\\CurrentControlSet\\Services\\Tcpip\\Parameters\\Interfaces\\' + $_.InterfaceGuid; \
           $ns = [string](Get-ItemProperty -Path $key -ErrorAction SilentlyContinue).NameServer; \
           try {{ {action} -ErrorAction Stop; $prev += ,@([int]$_.ifIndex, $ns); $n++ }} catch {{}} }}; \
         Clear-DnsClientCache; if ($n -eq 0) {{ throw '{}' }}; ConvertTo-Json -Compress -InputObject @($prev)",
        tr("Нет активных подключений", "No active connections")
    );
    let preset_name = preset.clone();
    util::blocking(move || {
        // Each adapter's previous static DNS ("" = automatic): the journal can put exactly that back.
        let prev: Vec<(u32, String)> = util::ps_json_timeout(&script, 90)?;
        crate::journal::add(&app, "dns", "", &preset_name, Some(crate::journal::Undo::Dns { adapters: prev }));
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn network_reset(app: AppHandle) -> Res<()> {
    if !util::is_admin() {
        return Err(util::need_admin());
    }
    util::blocking(move || {
        let (ok1, o1, e1) = util::run_hidden("netsh.exe", &["winsock", "reset"])?;
        let (ok2, o2, e2) = util::run_hidden("netsh.exe", &["int", "ip", "reset"])?;
        if ok1 || ok2 {
            crate::journal::add(&app, "network_reset", "", "", None);
        }
        if ok1 && ok2 {
            Ok(())
        } else {
            Err(format!("{} {} {} {}", o1.trim(), e1.trim(), o2.trim(), e2.trim()).trim().to_string())
        }
    })
    .await
}

#[derive(Serialize, Clone)]
struct SpeedProgress {
    phase: &'static str,
    value: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeedResult {
    ping_ms: f64,
    down_mbps: f64,
    up_mbps: f64,
}

const CF: &str = "https://speed.cloudflare.com";

fn client() -> Res<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent("StepMaster")
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())
}

fn net_err(e: reqwest::Error) -> String {
    format!("{}: {e}", tr("Нет связи с сервером теста", "Can't reach the test server"))
}

#[tauri::command]
pub async fn speed_test(app: AppHandle) -> Res<SpeedResult> {
    use crate::ops::{self, Op};
    let _running = ops::start(Op::Speed)?;
    // "Stop" ends the test at once: the measuring future is dropped (its download tasks abort).
    let watch = async {
        while !ops::cancelled(Op::Speed) {
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    };
    let test = speed_inner(&app);
    futures_util::pin_mut!(watch, test);
    match futures_util::future::select(test, watch).await {
        futures_util::future::Either::Left((r, _)) => r,
        futures_util::future::Either::Right(_) => Err(ops::stopped()),
    }
}

/// Download tasks are aborted when the test ends early (Stop / error), not left running.
struct AbortOnDrop(Vec<tokio::task::JoinHandle<()>>);
impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.iter().for_each(|t| t.abort());
    }
}

async fn speed_inner(app: &AppHandle) -> Res<SpeedResult> {
    let c = client()?;

    // Ping: median of small requests on a warm connection.
    let mut pings = Vec::new();
    for i in 0..6 {
        let t = Instant::now();
        c.get(format!("{CF}/__down?bytes=0")).send().await.map_err(net_err)?.bytes().await.map_err(net_err)?;
        if i > 0 {
            pings.push(t.elapsed().as_secs_f64() * 1000.0);
        }
    }
    pings.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let ping = pings.get(pings.len() / 2).copied().unwrap_or(0.0);
    let _ = app.emit("speed://progress", SpeedProgress { phase: "ping", value: ping });

    // Download: 4 parallel streams for up to 10 s.
    let total = Arc::new(AtomicU64::new(0));
    let start = Instant::now();
    let limit = Duration::from_secs(10);
    let mut tasks = Vec::new();
    for _ in 0..4 {
        let c = c.clone();
        let total = total.clone();
        tasks.push(tokio::spawn(async move {
            while start.elapsed() < limit {
                let Ok(resp) = c.get(format!("{CF}/__down?bytes=25000000")).send().await else { break };
                let mut stream = resp.bytes_stream();
                while let Some(chunk) = stream.next().await {
                    let Ok(chunk) = chunk else { break };
                    total.fetch_add(chunk.len() as u64, Ordering::Relaxed);
                    if start.elapsed() >= limit {
                        break;
                    }
                }
            }
        }));
    }
    let tasks = AbortOnDrop(tasks);
    while tasks.0.iter().any(|t| !t.is_finished()) && start.elapsed() < limit + Duration::from_secs(2) {
        tokio::time::sleep(Duration::from_millis(250)).await;
        let secs = start.elapsed().as_secs_f64().max(0.001);
        let mbps = total.load(Ordering::Relaxed) as f64 * 8.0 / secs / 1_000_000.0;
        let _ = app.emit("speed://progress", SpeedProgress { phase: "download", value: mbps });
    }
    drop(tasks);
    let down_secs = start.elapsed().as_secs_f64().min(limit.as_secs_f64()).max(0.001);
    let down = total.load(Ordering::Relaxed) as f64 * 8.0 / down_secs / 1_000_000.0;
    if total.load(Ordering::Relaxed) == 0 {
        return Err(tr("Не удалось измерить скорость загрузки", "Couldn't measure download speed"));
    }

    // Upload: sequential 5 MB posts for up to 8 s.
    let body = vec![0u8; 5_000_000];
    let up_start = Instant::now();
    let mut sent = 0u64;
    while up_start.elapsed() < Duration::from_secs(8) {
        match c.post(format!("{CF}/__up")).body(body.clone()).send().await {
            Ok(r) if r.status().is_success() => {
                sent += body.len() as u64;
                let mbps = sent as f64 * 8.0 / up_start.elapsed().as_secs_f64().max(0.001) / 1_000_000.0;
                let _ = app.emit("speed://progress", SpeedProgress { phase: "upload", value: mbps });
            }
            _ => break,
        }
    }
    let up = sent as f64 * 8.0 / up_start.elapsed().as_secs_f64().max(0.001) / 1_000_000.0;
    Ok(SpeedResult { ping_ms: ping, down_mbps: down, up_mbps: up })
}

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Connection {
    process: String,
    pid: u32,
    local: String,
    remote: String,
    state: String,
}

#[tauri::command]
pub async fn connections_list() -> Res<Vec<Connection>> {
    util::blocking(|| {
        util::ps_json(
            r#"
$procs = @{}; Get-Process | ForEach-Object { $procs[[int]$_.Id] = [string]$_.ProcessName }
ConvertTo-Json -Compress -InputObject @(Get-NetTCPConnection -State Established -ErrorAction SilentlyContinue |
  Where-Object { $_.RemoteAddress -notin @('127.0.0.1','::1','0.0.0.0','::') } | ForEach-Object {
  [pscustomobject]@{ process=[string]$procs[[int]$_.OwningProcess]; pid=[int]$_.OwningProcess;
    local="$($_.LocalAddress):$($_.LocalPort)"; remote="$($_.RemoteAddress):$($_.RemotePort)"; state=[string]$_.State } })
"#,
        )
    })
    .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WifiProfile {
    pub(crate) ssid: String,
    pub(crate) password: Option<String>,
    pub(crate) auth: String,
}

fn xml_tag(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let s = xml.find(&open)? + open.len();
    let e = xml[s..].find(&close)? + s;
    Some(
        xml[s..e]
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&apos;", "'")
            .replace("&amp;", "&"),
    )
}

fn wide_to_string(w: &[u16]) -> String {
    let end = w.iter().position(|&c| c == 0).unwrap_or(w.len());
    String::from_utf16_lossy(&w[..end])
}

/// Reads saved Wi-Fi profiles with keys through the WLAN API, entirely in memory.
/// PCs without Wi-Fi (no WLAN service) just get an empty list.
pub(crate) fn read_wifi_profiles() -> Vec<WifiProfile> {
    use windows_sys::Win32::Foundation::HANDLE;
    use windows_sys::Win32::NetworkManagement::WiFi::*;
    const GET_PLAINTEXT_KEY: u32 = 4;
    let mut out = Vec::new();
    unsafe {
        let mut version = 0u32;
        let mut h: HANDLE = std::ptr::null_mut();
        if WlanOpenHandle(2, std::ptr::null(), &mut version, &mut h) != 0 {
            return out;
        }
        let mut ifaces: *mut WLAN_INTERFACE_INFO_LIST = std::ptr::null_mut();
        if WlanEnumInterfaces(h, std::ptr::null(), &mut ifaces) == 0 && !ifaces.is_null() {
            let n = (*ifaces).dwNumberOfItems as usize;
            let first = (*ifaces).InterfaceInfo.as_ptr();
            for i in 0..n {
                let guid = (*first.add(i)).InterfaceGuid;
                let mut profiles: *mut WLAN_PROFILE_INFO_LIST = std::ptr::null_mut();
                if WlanGetProfileList(h, &guid, std::ptr::null(), &mut profiles) != 0 || profiles.is_null() {
                    continue;
                }
                let pn = (*profiles).dwNumberOfItems as usize;
                let pfirst = (*profiles).ProfileInfo.as_ptr();
                for j in 0..pn {
                    let name_w = (*pfirst.add(j)).strProfileName;
                    let mut xml_ptr: windows_sys::core::PWSTR = std::ptr::null_mut();
                    let mut flags = GET_PLAINTEXT_KEY;
                    let mut access = 0u32;
                    if WlanGetProfile(h, &guid, name_w.as_ptr(), std::ptr::null(), &mut xml_ptr, &mut flags, &mut access) != 0
                        || xml_ptr.is_null()
                    {
                        continue;
                    }
                    let mut len = 0usize;
                    while *xml_ptr.add(len) != 0 {
                        len += 1;
                    }
                    let xml = String::from_utf16_lossy(std::slice::from_raw_parts(xml_ptr, len));
                    WlanFreeMemory(xml_ptr as *const _);
                    let ssid = xml_tag(&xml, "name").unwrap_or_else(|| wide_to_string(&name_w));
                    let auth = xml_tag(&xml, "authentication").unwrap_or_default();
                    // keyMaterial is plain text only when the key is not protected (needs admin).
                    let protected = xml_tag(&xml, "protected").map(|p| p == "true").unwrap_or(false);
                    let password = xml_tag(&xml, "keyMaterial").filter(|k| !k.is_empty() && !protected);
                    if !out.iter().any(|w: &WifiProfile| w.ssid == ssid) {
                        out.push(WifiProfile { ssid, password, auth });
                    }
                }
                WlanFreeMemory(profiles as *const _);
            }
            WlanFreeMemory(ifaces as *const _);
        }
        WlanCloseHandle(h, std::ptr::null());
    }
    out
}

#[tauri::command]
pub async fn wifi_profiles() -> Res<Vec<WifiProfile>> {
    if !util::is_admin() {
        return Err(util::need_admin());
    }
    util::blocking(|| {
        let mut out = read_wifi_profiles();
        out.sort_by_key(|w| w.ssid.to_lowercase());
        Ok(out)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn xml() {
        let x = "<WLANProfile><name>Cafe &amp; Bar</name><authentication>WPA2PSK</authentication><keyMaterial>p&lt;w</keyMaterial></WLANProfile>";
        assert_eq!(xml_tag(x, "name").as_deref(), Some("Cafe & Bar"));
        assert_eq!(xml_tag(x, "keyMaterial").as_deref(), Some("p<w"));
        assert_eq!(xml_tag(x, "missing"), None);
    }
}
