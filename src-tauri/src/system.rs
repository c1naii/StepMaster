//! App info, live load (CPU/RAM/disks), hardware overview, processes.
use crate::util::{self, tr, Res};
use crate::AppState;
use serde::{Deserialize, Serialize};
use sysinfo::{DiskKind, Disks, ProcessesToUpdate, System};
use tauri::{AppHandle, Manager, State};
use winreg::enums::HKEY_LOCAL_MACHINE;
use winreg::RegKey;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    version: String,
    is_admin: bool,
    user_name: String,
    exe_path: String,
    data_dir: String,
    os_name: String,
    os_build: String,
    is_win11: bool,
}

/// (name, build, display version) — ProductName says "Windows 10" on Windows 11, so fix it by build.
pub fn os_info() -> (String, String, String) {
    let key = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion");
    let (mut name, build, ubr, display) = match key {
        Ok(k) => (
            k.get_value::<String, _>("ProductName").unwrap_or_else(|_| "Windows".into()),
            k.get_value::<String, _>("CurrentBuildNumber").unwrap_or_default(),
            k.get_value::<u32, _>("UBR").ok(),
            k.get_value::<String, _>("DisplayVersion").unwrap_or_default(),
        ),
        Err(_) => ("Windows".into(), String::new(), None, String::new()),
    };
    let build_num: u32 = build.parse().unwrap_or(0);
    if build_num >= 22000 {
        name = name.replace("Windows 10", "Windows 11");
    }
    let full_build = match ubr {
        Some(u) => format!("{build}.{u}"),
        None => build,
    };
    (name, full_build, display)
}

#[tauri::command]
pub fn app_info(state: State<'_, AppState>) -> AppInfo {
    let (os_name, os_build, _) = os_info();
    let build_num: u32 = os_build.split('.').next().and_then(|b| b.parse().ok()).unwrap_or(0);
    AppInfo {
        version: env!("CARGO_PKG_VERSION").into(),
        is_admin: util::is_admin(),
        user_name: std::env::var("USERNAME").unwrap_or_default(),
        exe_path: std::env::current_exe().map(|p| p.to_string_lossy().to_string()).unwrap_or_default(),
        data_dir: state.data_dir.to_string_lossy().to_string(),
        os_name,
        os_build,
        is_win11: build_num >= 22000,
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskInfo {
    mount: String,
    label: String,
    total: u64,
    available: u64,
    kind: String,
    removable: bool,
    fs: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    cpu: f32,
    cpu_freq_mhz: u64,
    mem_used: u64,
    mem_total: u64,
    disks: Vec<DiskInfo>,
}

pub fn list_disks() -> Vec<DiskInfo> {
    let disks = Disks::new_with_refreshed_list();
    let mut out: Vec<DiskInfo> = disks
        .list()
        .iter()
        .filter(|d| d.total_space() > 0)
        .map(|d| DiskInfo {
            mount: d.mount_point().to_string_lossy().to_string(),
            label: d.name().to_string_lossy().to_string(),
            total: d.total_space(),
            available: d.available_space(),
            kind: match d.kind() {
                DiskKind::SSD => "SSD".into(),
                DiskKind::HDD => "HDD".into(),
                _ => "Unknown".into(),
            },
            removable: d.is_removable(),
            fs: d.file_system().to_string_lossy().to_string(),
        })
        .collect();
    out.sort_by(|a, b| a.mount.cmp(&b.mount));
    out.dedup_by(|a, b| a.mount == b.mount);
    out
}

/// (mount, total, available) for every disk.
pub fn list_disks_raw() -> Vec<(String, u64, u64)> {
    list_disks().into_iter().map(|d| (d.mount, d.total, d.available)).collect()
}

#[tauri::command]
pub async fn system_snapshot(state: State<'_, AppState>) -> Res<Snapshot> {
    let (cpu, freq, used, total) = {
        let mut sys = state.sys.lock();
        sys.refresh_cpu_all();
        sys.refresh_memory();
        let freq = sys.cpus().iter().map(|c| c.frequency()).max().unwrap_or(0);
        (sys.global_cpu_usage(), freq, sys.used_memory(), sys.total_memory())
    };
    let disks = util::blocking(|| Ok(list_disks())).await?;
    Ok(Snapshot {
        cpu: if cpu.is_finite() { cpu.clamp(0.0, 100.0) } else { 0.0 },
        cpu_freq_mhz: freq,
        mem_used: used,
        mem_total: total,
        disks,
    })
}

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BatteryInfo {
    percent: u32,
    charging: bool,
    design_capacity: Option<u64>,
    full_capacity: Option<u64>,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    cpu_name: String,
    cores: u32,
    threads: u32,
    mem_total: u64,
    gpus: Vec<String>,
    os_name: String,
    os_version: String,
    os_build: String,
    host_name: String,
    uptime_sec: u64,
    board: String,
    battery: Option<BatteryInfo>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct PsOverview {
    cpu_name: String,
    cores: u32,
    threads: u32,
    gpus: Vec<String>,
    board: String,
    battery: Option<BatteryInfo>,
}

#[tauri::command]
pub async fn system_overview(state: State<'_, AppState>) -> Res<Overview> {
    let mem_total = {
        let mut sys = state.sys.lock();
        sys.refresh_memory();
        sys.total_memory()
    };
    util::blocking(move || {
        let script = r#"
$cpu = Get-CimInstance Win32_Processor | Select-Object -First 1
$gpus = @(Get-CimInstance Win32_VideoController | ForEach-Object { [string]$_.Name } | Where-Object { $_ -and $_ -notmatch 'Basic Display|Remote Display|Virtual' })
$cs = Get-CimInstance Win32_ComputerSystem
$bat = $null
$b = Get-CimInstance Win32_Battery -ErrorAction SilentlyContinue | Select-Object -First 1
if ($b) {
  $design = $null; $full = $null
  try { $design = (Get-CimInstance -Namespace root/wmi -ClassName BatteryStaticData -ErrorAction Stop | Select-Object -First 1).DesignedCapacity } catch {}
  try { $full = (Get-CimInstance -Namespace root/wmi -ClassName BatteryFullChargedCapacity -ErrorAction Stop | Select-Object -First 1).FullChargedCapacity } catch {}
  $bat = [pscustomobject]@{ percent = [int]$b.EstimatedChargeRemaining; charging = ([int]$b.BatteryStatus -ne 1); designCapacity = $design; fullCapacity = $full }
}
[pscustomobject]@{
  cpuName = ([string]$cpu.Name).Trim()
  cores = [int]$cpu.NumberOfCores
  threads = [int]$cpu.NumberOfLogicalProcessors
  gpus = $gpus
  board = ("{0} {1}" -f $cs.Manufacturer, $cs.Model).Trim()
  battery = $bat
} | ConvertTo-Json -Depth 4 -Compress
"#;
        let ps: PsOverview = util::ps_json(script)?;
        let (os_name, os_build, display) = os_info();
        Ok(Overview {
            cpu_name: ps.cpu_name,
            cores: ps.cores,
            threads: ps.threads,
            mem_total,
            gpus: ps.gpus,
            os_name,
            os_version: display,
            os_build,
            host_name: std::env::var("COMPUTERNAME").unwrap_or_default(),
            uptime_sec: System::uptime(),
            board: ps.board,
            battery: ps.battery,
        })
    })
    .await
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub cpu: f32,
    pub memory: u64,
    pub exe: Option<String>,
    /// Start time, unix seconds (internal).
    #[serde(skip)]
    pub started: u64,
}

/// Processes that must never be terminated from the app.
pub const CRITICAL_PROCESSES: &[&str] = &[
    "system",
    "registry",
    "smss.exe",
    "csrss.exe",
    "wininit.exe",
    "winlogon.exe",
    "services.exe",
    "lsass.exe",
    "lsaiso.exe",
    "svchost.exe",
    "dwm.exe",
    "fontdrvhost.exe",
    "memory compression",
    "secure system",
    "msmpeng.exe",
    "sihost.exe",
    "ctfmon.exe",
    "conhost.exe",
    "audiodg.exe",
    "wudfhost.exe",
    "spoolsv.exe",
];

pub fn is_critical(name: &str) -> bool {
    let n = name.to_lowercase();
    CRITICAL_PROCESSES.contains(&n.as_str())
}

pub fn list_processes(state: &AppState) -> Vec<ProcessInfo> {
    let mut sys = state.sys.lock();
    sys.refresh_processes(ProcessesToUpdate::All, true);
    let cpus = sys.cpus().len().max(1) as f32;
    let me = std::process::id();
    sys.processes()
        .values()
        .filter(|p| {
            let pid = p.pid().as_u32();
            pid > 4 && pid != me
        })
        .map(|p| ProcessInfo {
            pid: p.pid().as_u32(),
            name: p.name().to_string_lossy().to_string(),
            cpu: {
                let c = p.cpu_usage() / cpus;
                if c.is_finite() { c.clamp(0.0, 100.0) } else { 0.0 }
            },
            memory: p.memory(),
            exe: p.exe().map(|e| e.to_string_lossy().to_string()),
            started: p.start_time(),
        })
        .collect()
}

#[tauri::command]
pub async fn processes_list(app: AppHandle) -> Res<Vec<ProcessInfo>> {
    util::blocking(move || Ok(list_processes(&app.state::<AppState>()))).await
}

pub fn kill_pid(state: &AppState, pid: u32) -> Res<()> {
    if pid <= 4 || pid == std::process::id() {
        return Err(tr("Этот процесс нельзя завершить", "This process can't be ended"));
    }
    let mut sys = state.sys.lock();
    sys.refresh_processes(ProcessesToUpdate::Some(&[sysinfo::Pid::from_u32(pid)]), true);
    let p = sys
        .process(sysinfo::Pid::from_u32(pid))
        .ok_or_else(|| tr("Процесс уже завершён", "Process already ended"))?;
    if is_critical(&p.name().to_string_lossy()) {
        return Err(tr("Это системный процесс — его нельзя завершать", "This is a system process and can't be ended"));
    }
    if p.kill() {
        Ok(())
    } else {
        Err(tr("Не удалось завершить процесс (нет доступа)", "Couldn't end the process (access denied)"))
    }
}

#[tauri::command]
pub async fn process_kill(app: AppHandle, pid: u32) -> Res<()> {
    util::blocking(move || kill_pid(&app.state::<AppState>(), pid)).await
}
