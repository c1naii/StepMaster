//! Startup apps: Run keys + Startup folders, toggled the same way Task Manager does (StartupApproved).
use crate::util::{self, env_path, tr, Res};
use serde::Serialize;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use winreg::enums::*;
use winreg::{RegKey, RegValue, HKEY};

const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN32: &str = r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Run";
const APPROVED: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved";

#[derive(Clone, Copy, PartialEq)]
enum Src {
    HkcuRun,
    HklmRun,
    HklmRun32,
    UserFolder,
    CommonFolder,
}

impl Src {
    fn tag(self) -> &'static str {
        match self {
            Src::HkcuRun => "hkcu",
            Src::HklmRun => "hklm",
            Src::HklmRun32 => "hklm32",
            Src::UserFolder => "ufolder",
            Src::CommonFolder => "cfolder",
        }
    }
    fn from_tag(t: &str) -> Option<Self> {
        Some(match t {
            "hkcu" => Src::HkcuRun,
            "hklm" => Src::HklmRun,
            "hklm32" => Src::HklmRun32,
            "ufolder" => Src::UserFolder,
            "cfolder" => Src::CommonFolder,
            _ => return None,
        })
    }
    fn label(self) -> String {
        match self {
            Src::HkcuRun => tr("Реестр (пользователь)", "Registry (user)"),
            Src::HklmRun | Src::HklmRun32 => tr("Реестр (все)", "Registry (all users)"),
            Src::UserFolder => tr("Папка автозагрузки", "Startup folder"),
            Src::CommonFolder => tr("Папка автозагрузки (все)", "Startup folder (all)"),
        }
    }
    /// (hive, StartupApproved subkey)
    fn approved(self) -> (HKEY, String) {
        match self {
            Src::HkcuRun => (HKEY_CURRENT_USER, format!(r"{APPROVED}\Run")),
            Src::HklmRun => (HKEY_LOCAL_MACHINE, format!(r"{APPROVED}\Run")),
            Src::HklmRun32 => (HKEY_LOCAL_MACHINE, format!(r"{APPROVED}\Run32")),
            Src::UserFolder => (HKEY_CURRENT_USER, format!(r"{APPROVED}\StartupFolder")),
            Src::CommonFolder => (HKEY_LOCAL_MACHINE, format!(r"{APPROVED}\StartupFolder")),
        }
    }
}

fn user_folder() -> Option<PathBuf> {
    env_path("APPDATA").map(|p| p.join(r"Microsoft\Windows\Start Menu\Programs\Startup"))
}

fn common_folder() -> Option<PathBuf> {
    env_path("ProgramData").map(|p| p.join(r"Microsoft\Windows\Start Menu\Programs\StartUp"))
}

/// Missing value = enabled. First byte even (0x02, 0x06) = enabled, odd (0x03) = disabled.
fn is_enabled(src: Src, name: &str) -> bool {
    let (hive, path) = src.approved();
    RegKey::predef(hive)
        .open_subkey_with_flags(&path, KEY_READ | KEY_WOW64_64KEY)
        .and_then(|k| k.get_raw_value(name))
        .map(|v| v.bytes.first().map(|b| b & 1 == 0).unwrap_or(true))
        .unwrap_or(true)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupItem {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) command: String,
    location: String,
    pub(crate) enabled: bool,
    pub(crate) exe_path: Option<String>,
}

fn read_run(hive: HKEY, path: &str, src: Src, out: &mut Vec<StartupItem>) {
    let flags = KEY_READ | if src == Src::HklmRun32 { KEY_WOW64_32KEY } else { KEY_WOW64_64KEY };
    let Ok(key) = RegKey::predef(hive).open_subkey_with_flags(path, flags) else { return };
    for (name, value) in key.enum_values().flatten() {
        if name.is_empty() {
            continue;
        }
        let command = match value.vtype {
            REG_SZ | REG_EXPAND_SZ => String::from_utf16_lossy(
                &value.bytes.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect::<Vec<_>>(),
            )
            .trim_end_matches('\0')
            .to_string(),
            _ => continue,
        };
        out.push(StartupItem {
            id: format!("{}|{}", src.tag(), name),
            enabled: is_enabled(src, &name),
            exe_path: crate::icons::exe_from_command(&command),
            name,
            command,
            location: src.label(),
        });
    }
}

fn read_folder(dir: Option<PathBuf>, src: Src, out: &mut Vec<StartupItem>) {
    let Some(dir) = dir else { return };
    let Ok(rd) = std::fs::read_dir(&dir) else { return };
    for e in rd.flatten() {
        let file = e.file_name().to_string_lossy().to_string();
        if file.eq_ignore_ascii_case("desktop.ini") || e.path().is_dir() {
            continue;
        }
        let display = std::path::Path::new(&file)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| file.clone());
        out.push(StartupItem {
            id: format!("{}|{}", src.tag(), file),
            enabled: is_enabled(src, &file),
            name: display,
            command: e.path().to_string_lossy().to_string(),
            location: src.label(),
            exe_path: Some(e.path().to_string_lossy().to_string()),
        });
    }
}

#[tauri::command]
pub async fn startup_list() -> Res<Vec<StartupItem>> {
    util::blocking(|| Ok(read_all())).await
}

pub fn read_all() -> Vec<StartupItem> {
    let mut out = Vec::new();
    read_run(HKEY_CURRENT_USER, RUN, Src::HkcuRun, &mut out);
    read_run(HKEY_LOCAL_MACHINE, RUN, Src::HklmRun, &mut out);
    read_run(HKEY_LOCAL_MACHINE, RUN32, Src::HklmRun32, &mut out);
    read_folder(user_folder(), Src::UserFolder, &mut out);
    read_folder(common_folder(), Src::CommonFolder, &mut out);
    out.sort_by_key(|s| s.name.to_lowercase());
    out
}

#[tauri::command]
pub fn startup_set(id: String, enabled: bool) -> Res<()> {
    set_enabled(&id, enabled)
}

pub fn set_enabled(id: &str, enabled: bool) -> Res<()> {
    let (tag, name) = id.split_once('|').ok_or_else(|| tr("Неверный элемент", "Invalid item"))?;
    let src = Src::from_tag(tag).ok_or_else(|| tr("Неверный элемент", "Invalid item"))?;
    if name.is_empty() || name.len() > 512 {
        return Err(tr("Неверный элемент", "Invalid item"));
    }
    let (hive, path) = src.approved();
    if hive == HKEY_LOCAL_MACHINE && !util::is_admin() {
        return Err(util::need_admin());
    }
    let (key, _) = RegKey::predef(hive)
        .create_subkey_with_flags(&path, KEY_READ | KEY_WRITE | KEY_WOW64_64KEY)
        .map_err(|e| format!("{}: {e}", tr("Нет доступа к реестру", "Registry access denied")))?;
    let mut bytes = vec![0u8; 12];
    if enabled {
        bytes[0] = 0x02;
    } else {
        bytes[0] = 0x03;
        // FILETIME of when it was disabled (100ns ticks since 1601).
        let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos() / 100).unwrap_or(0) as u64;
        let ft = now + 116_444_736_000_000_000;
        bytes[4..12].copy_from_slice(&ft.to_le_bytes());
    }
    key.set_raw_value(name, &RegValue { bytes, vtype: REG_BINARY })
        .map_err(|e| format!("{}: {e}", tr("Не удалось изменить автозагрузку", "Couldn't change startup")))
}
