//! Shared helpers: hidden process execution, PowerShell, admin check, path safety, localized errors.
use base64::Engine;
use once_cell::sync::Lazy;
use parking_lot::RwLock;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub type Res<T> = Result<T, String>;

/// Current UI language ("ru" / "en"), used for backend error messages.
pub static LANG: Lazy<RwLock<String>> = Lazy::new(|| RwLock::new("ru".into()));

pub fn tr(ru: &str, en: &str) -> String {
    if LANG.read().as_str() == "en" {
        en.to_string()
    } else {
        ru.to_string()
    }
}

pub fn need_admin() -> String {
    tr("Нужны права администратора", "Administrator rights required")
}

/// Is this process elevated?
pub fn is_admin() -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
    unsafe {
        let mut token: HANDLE = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return false;
        }
        let mut elev = TOKEN_ELEVATION { TokenIsElevated: 0 };
        let mut len = 0u32;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            &mut elev as *mut _ as *mut _,
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut len,
        );
        CloseHandle(token);
        ok != 0 && elev.TokenIsElevated != 0
    }
}

/// Absolute path of a Windows system tool. Bare names would be looked up in the app's own
/// folder first, where anything could be planted next to stepmaster.exe.
pub fn sys_tool(name: &str) -> PathBuf {
    let root = system_root();
    match name.to_lowercase().as_str() {
        "powershell.exe" => root.join(r"System32\WindowsPowerShell\v1.0\powershell.exe"),
        "explorer.exe" => root.join("explorer.exe"),
        _ => root.join("System32").join(name),
    }
}

/// Run a Windows system tool without a console window. Returns (success, stdout, stderr).
pub fn run_hidden(program: &str, args: &[&str]) -> Res<(bool, String, String)> {
    let out = Command::new(sys_tool(program))
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("{program}: {e}"))?;
    Ok((
        out.status.success(),
        decode_console(&out.stdout),
        decode_console(&out.stderr),
    ))
}

/// Console tools (ipconfig, netsh, powercfg) print in the OEM code page; decode safely.
pub fn decode_console(bytes: &[u8]) -> String {
    if let Ok(s) = std::str::from_utf8(bytes) {
        return s.to_string();
    }
    // Fallback: CP866 (Russian OEM) for the common case, lossy otherwise.
    let mut s = String::with_capacity(bytes.len());
    for &b in bytes {
        if b < 0x80 {
            s.push(b as char);
        } else {
            let c = match b {
                0x80..=0xAF => char::from_u32(0x0410 + (b as u32 - 0x80)),
                0xE0..=0xEF => char::from_u32(0x0440 + (b as u32 - 0xE0)),
                0xF0 => Some('Ё'),
                0xF1 => Some('ё'),
                _ => Some('?'),
            };
            s.push(c.unwrap_or('?'));
        }
    }
    s
}

/// Quote a string for a single-quoted PowerShell literal.
pub fn psq(s: &str) -> String {
    // PowerShell treats the typographic single quotes like ' — double all of them.
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for c in s.chars() {
        if matches!(c, '\'' | '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}') {
            out.push(c);
        }
        out.push(c);
    }
    out.push('\'');
    out
}

/// Run a PowerShell script (Windows PowerShell 5.1, always present on Win10/11).
/// Errors thrown in the script come back as Err(message). Output is UTF-8.
pub fn ps(script: &str) -> Res<String> {
    let full = format!(
        "$ProgressPreference='SilentlyContinue'; $ErrorActionPreference='Stop'; \
         [Console]::OutputEncoding=[System.Text.Encoding]::UTF8; \
         try {{ {script} }} catch {{ [Console]::Out.Write('__SMERR__' + $_.Exception.Message); exit 1 }}"
    );
    let utf16: Vec<u8> = full.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
    let b64 = base64::engine::general_purpose::STANDARD.encode(utf16);
    let out = Command::new(sys_tool("powershell.exe"))
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-EncodedCommand", &b64])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("PowerShell: {e}"))?;
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    if let Some(i) = stdout.find("__SMERR__") {
        let msg = stdout[i + 9..].trim();
        return Err(if msg.is_empty() { tr("Ошибка PowerShell", "PowerShell error") } else { msg.to_string() });
    }
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(if err.is_empty() { tr("Команда завершилась с ошибкой", "Command failed") } else { err });
    }
    Ok(stdout.trim().trim_start_matches('\u{feff}').to_string())
}

/// PowerShell returning JSON (script must output a single JSON value).
pub fn ps_json<T: serde::de::DeserializeOwned>(script: &str) -> Res<T> {
    let out = ps(script)?;
    let text = if out.is_empty() { "null" } else { out.as_str() };
    let parse_err = |e: serde_json::Error| format!("{}: {e}", tr("Не удалось разобрать ответ системы", "Couldn't parse system output"));
    let mut v: serde_json::Value = serde_json::from_str(text).map_err(parse_err)?;
    strip_nulls(&mut v);
    serde_json::from_value(v).map_err(parse_err)
}

/// PowerShell emits null for missing properties. Remove null fields and null array items,
/// so `#[serde(default)]` fills them in instead of failing.
pub fn strip_nulls(v: &mut serde_json::Value) {
    match v {
        serde_json::Value::Object(m) => {
            m.retain(|_, x| !x.is_null());
            m.values_mut().for_each(strip_nulls);
        }
        serde_json::Value::Array(a) => {
            a.retain(|x| !x.is_null());
            a.iter_mut().for_each(strip_nulls);
        }
        _ => {}
    }
}

pub fn env_path(var: &str) -> Option<PathBuf> {
    std::env::var_os(var).map(PathBuf::from).filter(|p| p.is_absolute())
}

pub fn system_root() -> PathBuf {
    env_path("SystemRoot").unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
}

fn norm(p: &Path) -> String {
    let s = p.to_string_lossy().replace('/', "\\");
    let s = s.trim_end_matches('\\').to_lowercase();
    s.strip_prefix(r"\\?\").unwrap_or(&s).to_string()
}

/// Paths that must never be deleted or recycled by the app (exact roots, and anything inside
/// the Windows folder). Inside e.g. Program Files is allowed; Program Files itself is not.
pub fn is_protected_path(p: &Path) -> bool {
    if !p.is_absolute() {
        return true;
    }
    let n = norm(p);
    // Drive roots like "c:" or UNC roots.
    if n.len() <= 3 || n.starts_with(r"\\") {
        return true;
    }
    let windir = norm(&system_root());
    if n == windir || n.starts_with(&(windir.clone() + "\\")) {
        return true;
    }
    let mut exact: Vec<String> = Vec::new();
    for var in [
        "ProgramFiles",
        "ProgramFiles(x86)",
        "ProgramW6432",
        "ProgramData",
        "USERPROFILE",
        "APPDATA",
        "LOCALAPPDATA",
        "PUBLIC",
        "SystemDrive",
        "TEMP",
        "TMP",
    ] {
        if let Some(v) = std::env::var_os(var) {
            exact.push(norm(Path::new(&v)));
        }
    }
    if let Some(home) = env_path("USERPROFILE") {
        for sub in ["Desktop", "Documents", "Downloads", "Pictures", "Music", "Videos", "OneDrive", "AppData", r"AppData\LocalLow"] {
            exact.push(norm(&home.join(sub)));
        }
        if let Some(users) = home.parent() {
            exact.push(norm(users));
        }
    }
    if exact.iter().any(|e| *e == n) {
        return true;
    }
    // Special system entries at the drive root.
    let name = n.rsplit('\\').next().unwrap_or("");
    let at_root = n.matches('\\').count() == 1;
    if at_root
        && [
            "pagefile.sys",
            "hiberfil.sys",
            "swapfile.sys",
            "system volume information",
            "$recycle.bin",
            "recovery",
            "boot",
            "bootmgr",
            "efi",
            "program files",
            "program files (x86)",
            "programdata",
            "users",
            "windows",
        ]
        .contains(&name)
    {
        return true;
    }
    // Never touch our own executable folder.
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            if norm(dir) == n || norm(&exe) == n {
                return true;
            }
        }
    }
    false
}

pub const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
pub const FILE_ATTRIBUTE_READONLY: u32 = 0x1;

/// Open a file / URL / tool with the shell (like double-click / Win+R).
pub fn shell_open(target: &str, params: Option<&str>, verb: &str) -> Res<()> {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let w = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
    let verb_w = w(verb);
    let target_w = w(target);
    let params_w = params.map(w);
    let r = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb_w.as_ptr(),
            target_w.as_ptr(),
            params_w.as_ref().map(|p| p.as_ptr()).unwrap_or(std::ptr::null()),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    if (r as isize) <= 32 {
        return Err(tr("Не удалось открыть", "Couldn't open") + &format!(": {target}"));
    }
    Ok(())
}

/// Run blocking work off the async runtime.
pub async fn blocking<T, F>(f: F) -> Res<T>
where
    T: Send + 'static,
    F: FnOnce() -> Res<T> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| format!("task: {e}"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protected_paths() {
        assert!(is_protected_path(Path::new(r"C:\")));
        assert!(is_protected_path(Path::new(r"C:")));
        assert!(is_protected_path(Path::new(r"C:\Windows")));
        assert!(is_protected_path(Path::new(r"C:\Windows\System32\drivers")));
        assert!(is_protected_path(Path::new(r"C:\pagefile.sys")));
        assert!(is_protected_path(Path::new(r"C:\Program Files")));
        assert!(is_protected_path(Path::new(r"relative\path")));
        assert!(is_protected_path(Path::new(r"\\server\share")));
        if let Some(home) = env_path("USERPROFILE") {
            assert!(is_protected_path(&home));
            assert!(is_protected_path(&home.join("Downloads")));
            assert!(!is_protected_path(&home.join("Downloads").join("movie.mkv")));
        }
        assert!(!is_protected_path(Path::new(r"C:\Program Files\SomeApp")));
        assert!(!is_protected_path(Path::new(r"D:\Games\old")));
    }

    #[test]
    fn nulls_are_stripped() {
        let mut v = serde_json::json!({"a": null, "b": [null, "x", {"c": null}], "d": 1});
        strip_nulls(&mut v);
        assert_eq!(v, serde_json::json!({"b": ["x", {}], "d": 1}));
    }

    #[test]
    fn ps_quoting() {
        assert_eq!(psq("a'b"), "'a''b'");
        assert_eq!(psq(""), "''");
        assert_eq!(psq("a\u{2019}b"), "'a\u{2019}\u{2019}b'");
    }

    #[test]
    fn cp866_decode() {
        // "Ошибка" in CP866
        let bytes = [0x8E, 0xE8, 0xA8, 0xA1, 0xAA, 0xA0];
        assert_eq!(decode_console(&bytes), "Ошибка");
    }
}
