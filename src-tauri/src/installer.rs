//! Built-in installer / uninstaller. The same StepMaster.exe is both: started from anywhere other
//! than its install folder it shows the setup screen; installed, it behaves normally.
//! Install = copy self, Start menu + Desktop shortcuts, "Apps & features" entry.
use crate::util::{self, env_path, is_protected_path, psq, tr, Res};
use serde::Serialize;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter, Manager};
use winreg::enums::*;
use winreg::RegKey;

const UNINSTALL_KEY: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\StepMaster";
const EXE_NAME: &str = "StepMaster.exe";
/// Shown as the developer in "Apps & features" (and as the company in the exe properties via tauri.conf.json).
const PUBLISHER: &str = "Cinailo Corp.";
const TASK_NAME: &str = "StepMaster";

fn norm(p: &Path) -> String {
    p.to_string_lossy().replace('/', "\\").trim_end_matches('\\').to_lowercase()
}

/// Folder StepMaster is installed to (from the "Apps & features" entry), if it still exists.
pub fn installed_dir() -> Option<PathBuf> {
    let k = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey_with_flags(UNINSTALL_KEY, KEY_READ | KEY_WOW64_64KEY).ok()?;
    let dir: String = k.get_value("InstallLocation").ok()?;
    let p = PathBuf::from(dir);
    p.join(EXE_NAME).is_file().then_some(p)
}

fn current_dir() -> Option<PathBuf> {
    std::env::current_exe().ok()?.parent().map(|p| p.to_path_buf())
}

static MODE: std::sync::OnceLock<&'static str> = std::sync::OnceLock::new();

/// "setup" (not installed here), "uninstall" (--uninstall from the installed copy), "normal".
/// Computed once per process: args and the install location don't change while running.
pub fn mode() -> &'static str {
    MODE.get_or_init(compute_mode)
}

fn compute_mode() -> &'static str {
    // Debug/dev builds never show the installer.
    if cfg!(debug_assertions) {
        return "normal";
    }
    let args: Vec<String> = std::env::args().collect();
    let has = |f: &str| args.iter().any(|a| a == f);
    let Some(cur) = current_dir() else { return "normal" };
    let inside_install = installed_dir().map(|d| norm(&d) == norm(&cur)).unwrap_or(false);
    if inside_install {
        return if has("--uninstall") { "uninstall" } else { "normal" };
    }
    // An autostart task left by an older portable copy still opens the program normally.
    if has("--autostart") {
        return "normal";
    }
    "setup"
}

/// Ends other StepMaster processes started from `dir` (e.g. the installed copy sitting in the tray),
/// so its files can be replaced or removed and the single-instance guard doesn't catch the new one.
fn stop_instances_in(dir: &Path) {
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
    let me = std::process::id();
    let want = norm(&dir.join(EXE_NAME));
    let mut sys = System::new();
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing().with_exe(UpdateKind::Always));
    let mut killed = false;
    for (pid, p) in sys.processes() {
        if pid.as_u32() != me && p.exe().map(|e| norm(e) == want).unwrap_or(false) {
            killed |= p.kill();
        }
    }
    if killed {
        std::thread::sleep(std::time::Duration::from_millis(700));
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallState {
    mode: String,
    default_dir: String,
    already_installed: Option<String>,
}

#[tauri::command]
pub fn install_state() -> InstallState {
    let pf = env_path("ProgramW6432").or_else(|| env_path("ProgramFiles")).unwrap_or_else(|| PathBuf::from(r"C:\Program Files"));
    InstallState {
        mode: mode().to_string(),
        default_dir: pf.join("StepMaster").to_string_lossy().to_string(),
        already_installed: installed_dir().map(|d| d.to_string_lossy().to_string()),
    }
}

/// Final install folder: always ends with "StepMaster" so nothing is ever dumped into a shared folder.
pub fn final_dir(chosen: &str) -> Res<PathBuf> {
    let p = PathBuf::from(chosen.trim().trim_matches('"'));
    let has_parent = p.components().any(|c| matches!(c, std::path::Component::ParentDir));
    if !p.is_absolute() || chosen.starts_with(r"\\") || has_parent {
        return Err(tr("Выберите папку на локальном диске", "Choose a folder on a local drive"));
    }
    let dir = if p.file_name().map(|n| n.to_string_lossy().eq_ignore_ascii_case("StepMaster")).unwrap_or(false) { p } else { p.join("StepMaster") };
    let n = norm(&dir);
    let win = norm(&util::system_root());
    if n.len() <= 3 || n == win || n.starts_with(&(win + "\\")) || is_protected_path(&dir) {
        return Err(tr("В эту папку устанавливать нельзя", "Can't install into this folder"));
    }
    Ok(dir)
}

#[derive(Serialize, Clone)]
struct Progress {
    step: &'static str,
}

fn make_shortcut(lnk: &Path, target: &Path) -> Res<()> {
    if let Some(d) = lnk.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    let script = format!(
        "$s = (New-Object -ComObject WScript.Shell).CreateShortcut({}); $s.TargetPath = {}; $s.WorkingDirectory = {}; $s.IconLocation = {}; $s.Description = 'StepMaster'; $s.Save()",
        psq(&lnk.to_string_lossy()),
        psq(&target.to_string_lossy()),
        psq(&target.parent().map(|p| p.to_string_lossy().to_string()).unwrap_or_default()),
        psq(&format!("{},0", target.to_string_lossy()))
    );
    util::ps(&script).map(|_| ())
}

fn desktop_dir() -> Option<PathBuf> {
    util::ps("[Environment]::GetFolderPath('Desktop')").ok().map(PathBuf::from).filter(|p| p.is_dir())
}

fn start_menu_lnk() -> PathBuf {
    env_path("ProgramData").unwrap_or_else(|| PathBuf::from(r"C:\ProgramData")).join(r"Microsoft\Windows\Start Menu\Programs\StepMaster.lnk")
}

#[tauri::command]
pub async fn install_run(app: AppHandle, dir: String, desktop: bool, start_menu: bool) -> Res<()> {
    if mode() != "setup" {
        return Err(tr("Установка не требуется", "No installation needed"));
    }
    if !util::is_admin() {
        return Err(util::need_admin());
    }
    let target_dir = final_dir(&dir)?;
    let app2 = app.clone();
    let exe_path = util::blocking(move || {
        let emit = |s: &'static str| {
            let _ = app2.emit("install://progress", Progress { step: s });
        };
        let src = std::env::current_exe().map_err(|e| e.to_string())?;
        emit("copy");
        std::fs::create_dir_all(&target_dir).map_err(|e| format!("{}: {e}", tr("Не удалось создать папку", "Couldn't create the folder")))?;
        let dest = target_dir.join(EXE_NAME);
        stop_instances_in(&target_dir);
        // Already sitting in the chosen folder: nothing to copy.
        if norm(&dest) != norm(&src) {
            if dest.exists() {
                // A previous copy may still be locked: move it aside instead of failing.
                let old = target_dir.join("StepMaster.exe.old");
                let _ = std::fs::remove_file(&old);
                std::fs::rename(&dest, &old).map_err(|e| format!("{}: {e}", tr("Закройте установленный StepMaster и повторите", "Close the installed StepMaster and try again")))?;
            }
            if let Err(e) = std::fs::copy(&src, &dest) {
                // Put the previous copy back so the installed program keeps working.
                let old = target_dir.join("StepMaster.exe.old");
                if !dest.exists() && old.exists() {
                    let _ = std::fs::rename(&old, &dest);
                }
                return Err(format!("{}: {e}", tr("Не удалось скопировать файл", "Couldn't copy the file")));
            }
        }
        let size_kb = std::fs::metadata(&dest).map(|m| m.len() / 1024).unwrap_or(0) as u32;

        emit("register");
        let (k, _) = RegKey::predef(HKEY_LOCAL_MACHINE)
            .create_subkey_with_flags(UNINSTALL_KEY, KEY_READ | KEY_WRITE | KEY_WOW64_64KEY)
            .map_err(|e| e.to_string())?;
        let exe_s = dest.to_string_lossy().to_string();
        let set = |n: &str, v: &str| k.set_value(n, &v.to_string()).map_err(|e| e.to_string());
        set("DisplayName", "StepMaster")?;
        set("DisplayVersion", env!("CARGO_PKG_VERSION"))?;
        set("Publisher", PUBLISHER)?;
        set("InstallLocation", &target_dir.to_string_lossy())?;
        set("DisplayIcon", &format!("{exe_s},0"))?;
        set("UninstallString", &format!("\"{exe_s}\" --uninstall"))?;
        k.set_value("EstimatedSize", &size_kb).map_err(|e| e.to_string())?;
        k.set_value("NoModify", &1u32).map_err(|e| e.to_string())?;
        k.set_value("NoRepair", &1u32).map_err(|e| e.to_string())?;

        emit("shortcuts");
        if start_menu {
            make_shortcut(&start_menu_lnk(), &dest)?;
        }
        if desktop {
            if let Some(d) = desktop_dir() {
                make_shortcut(&d.join("StepMaster.lnk"), &dest)?;
            }
        }
        emit("launch");
        Ok(dest)
    })
    .await?;
    // Start the installed copy once this process is gone (single-instance guard), then exit.
    std::process::Command::new(&exe_path)
        .arg(format!("--wait-pid={}", std::process::id()))
        .spawn()
        .map_err(|e| format!("{}: {e}", tr("Не удалось запустить программу", "Couldn't start the program")))?;
    app.exit(0);
    Ok(())
}

#[tauri::command]
pub async fn uninstall_run(app: AppHandle, remove_data: bool) -> Res<()> {
    if mode() != "uninstall" {
        return Err(tr("Удаление недоступно", "Uninstall isn't available"));
    }
    if !util::is_admin() {
        return Err(util::need_admin());
    }
    let install_dir = installed_dir().ok_or_else(|| tr("Программа не установлена", "The program isn't installed"))?;
    let data_dir = app_data_dir(&app);
    // Undo our system changes first: their saved values live in the data folder.
    let state_dir = app.state::<crate::AppState>().data_dir.clone();
    let app2 = app.clone();
    util::blocking(move || {
        let _ = crate::games::revert_quiet(&state_dir);
        crate::wallpaper::restore_if_ours(&app2);
        Ok(())
    })
    .await?;
    util::blocking(move || {
        stop_instances_in(&install_dir);
        // Autostart task, shortcuts, "Apps & features" entry.
        let _ = util::run_hidden("schtasks.exe", &["/Delete", "/TN", TASK_NAME, "/F"]);
        let _ = util::run_hidden("schtasks.exe", &["/Delete", "/TN", crate::notify::TASK_NAME, "/F"]);
        let _ = std::fs::remove_file(start_menu_lnk());
        if let Some(d) = desktop_dir() {
            let _ = std::fs::remove_file(d.join("StepMaster.lnk"));
        }
        let _ = RegKey::predef(HKEY_LOCAL_MACHINE).delete_subkey_with_flags(UNINSTALL_KEY, KEY_WOW64_64KEY);
        let data_dir = data_dir.filter(|d| remove_data && is_our_data_dir(d));
        if remove_data {
            crate::journal::remove_store();
        }
        // Settings, history, diagnostics go right now; only WebView's own files stay busy.
        if let Some(d) = &data_dir {
            for e in std::fs::read_dir(d).into_iter().flatten().flatten() {
                if e.file_type().map(|t| t.is_file()).unwrap_or(false) {
                    let _ = std::fs::remove_file(e.path());
                }
            }
        }
        // The running exe (and WebView files in the data folder) can't be deleted while we run:
        // a hidden cmd finishes the job after we exit.
        use std::os::windows::process::CommandExt;
        let _ = std::process::Command::new(util::sys_tool("cmd.exe"))
            .raw_arg(cleanup_command(&install_dir, data_dir.as_deref()))
            .creation_flags(util::CREATE_NO_WINDOW)
            .spawn();
        Ok(())
    })
    .await?;
    app.exit(0);
    Ok(())
}

/// cmd.exe arguments that remove what install/update created, after this process exits.
/// Only our own files are deleted, and the install folder only if it is then empty — so a
/// folder that held other files (even one named StepMaster) is never wiped.
fn cleanup_command(install_dir: &Path, data_dir: Option<&Path>) -> String {
    let q = |p: &Path| format!("\"{}\"", p.to_string_lossy().replace('"', ""));
    let exe = install_dir.join(EXE_NAME);
    let files = ["", ".old", ".new"]
        .iter()
        .map(|ext| q(Path::new(&format!("{}{ext}", exe.to_string_lossy()))))
        .collect::<Vec<_>>()
        .join(" ");
    // The exe may stay locked a bit longer (antivirus scan): retry for ~20 s.
    let main = q(&exe);
    let mut cmd = format!(
        "/C ping 127.0.0.1 -n 3 >nul & del /f /q {files} & for /L %i in (1,1,10) do @if exist {main} (del /f /q {main} 2>nul & ping 127.0.0.1 -n 3 >nul) & rmdir {}",
        q(install_dir)
    );
    // The data folder is entirely ours (named after the app id), so it may go recursively.
    // WebView2 helper processes can hold its files a few seconds longer: retry for ~20 s.
    if let Some(d) = data_dir.filter(|d| is_our_data_dir(d)) {
        let d = q(d);
        cmd.push_str(&format!(" & for /L %i in (1,1,10) do @if exist {d} (rmdir /s /q {d} 2>nul & ping 127.0.0.1 -n 3 >nul)"));
    }
    cmd
}

fn is_our_data_dir(d: &Path) -> bool {
    norm(d).ends_with(r"\com.stepmaster.app") && !is_protected_path(d)
}

/// After a self-update the installed copy refreshes its version and developer in "Apps & features".
pub fn refresh_registered_version() {
    if mode() != "normal" || installed_dir().is_none() {
        return;
    }
    if let Ok(k) = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey_with_flags(UNINSTALL_KEY, KEY_READ | KEY_WRITE | KEY_WOW64_64KEY) {
        let _ = k.set_value("DisplayVersion", &env!("CARGO_PKG_VERSION").to_string());
        let _ = k.set_value("Publisher", &PUBLISHER.to_string());
    }
}

fn app_data_dir(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_local_data_dir().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_dir_rules() {
        assert_eq!(final_dir(r"C:\Program Files\StepMaster").unwrap(), PathBuf::from(r"C:\Program Files\StepMaster"));
        assert_eq!(final_dir(r"D:\Programs").unwrap(), PathBuf::from(r"D:\Programs\StepMaster"));
        assert_eq!(final_dir(r#""D:\Games""#).unwrap(), PathBuf::from(r"D:\Games\StepMaster"));
        assert!(final_dir("relative\\path").is_err());
        assert!(final_dir(r"\\server\share").is_err());
        assert!(final_dir(r"C:\Windows").is_err());
        assert!(final_dir(r"C:\Foo\..\Windows").is_err());
        assert!(final_dir(r"C:\Windows\System32").is_err());
        assert!(final_dir("").is_err());
    }

    #[test]
    fn uninstall_never_wipes_a_folder() {
        let c = cleanup_command(Path::new(r"D:\Projects\StepMaster"), None);
        assert!(c.contains(r#"del /f /q "D:\Projects\StepMaster\StepMaster.exe" "D:\Projects\StepMaster\StepMaster.exe.old""#));
        assert!(c.contains(r#"rmdir "D:\Projects\StepMaster""#));
        assert!(!c.contains("/s"));
        let data = Path::new(r"C:\Users\x\AppData\Local\com.stepmaster.app");
        assert!(cleanup_command(Path::new(r"C:\Program Files\StepMaster"), Some(data)).contains(r#"rmdir /s /q "C:\Users\x\AppData\Local\com.stepmaster.app""#));
        // Anything that isn't our data folder is never removed recursively.
        assert!(!cleanup_command(Path::new(r"C:\Program Files\StepMaster"), Some(Path::new(r"C:\Users\x"))).contains("/s"));
    }

    /// Runs the real cleanup command on temp folders while a WebView-like file stays locked for a while.
    /// `cargo test --lib cleanup_command_runs -- --ignored`
    #[test]
    #[ignore]
    fn cleanup_command_runs() {
        use std::os::windows::fs::OpenOptionsExt;
        use std::os::windows::process::CommandExt;
        let root = std::env::temp_dir().join(format!("sm-uninstall-test-{}", std::process::id()));
        let inst = root.join("StepMaster");
        let data = root.join("com.stepmaster.app");
        let keep = root.join("Keep").join("StepMaster");
        std::fs::create_dir_all(data.join(r"EBWebView\Default")).unwrap();
        std::fs::create_dir_all(&inst).unwrap();
        std::fs::create_dir_all(&keep).unwrap();
        std::fs::write(inst.join(EXE_NAME), b"x").unwrap();
        std::fs::write(inst.join("StepMaster.exe.old"), b"x").unwrap();
        std::fs::write(keep.join(EXE_NAME), b"x").unwrap();
        std::fs::write(keep.join("my-notes.txt"), b"x").unwrap();
        std::fs::write(data.join("settings.json"), b"x").unwrap();
        std::fs::write(data.join(r"EBWebView\Default\lock.db"), b"x").unwrap();
        // Hold the file without share-delete, like a WebView2 helper still running.
        let locked = std::fs::OpenOptions::new().read(true).share_mode(0).open(data.join(r"EBWebView\Default\lock.db")).unwrap();
        let mut a = std::process::Command::new(util::sys_tool("cmd.exe")).raw_arg(cleanup_command(&inst, Some(&data))).spawn().unwrap();
        let mut b = std::process::Command::new(util::sys_tool("cmd.exe")).raw_arg(cleanup_command(&keep, None)).spawn().unwrap();
        std::thread::sleep(std::time::Duration::from_secs(4));
        drop(locked);
        a.wait().unwrap();
        b.wait().unwrap();
        assert!(!inst.exists(), "install folder removed");
        assert!(!data.exists(), "data folder removed after the lock was released");
        assert!(keep.join("my-notes.txt").exists() && !keep.join(EXE_NAME).exists(), "foreign files kept");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn debug_builds_never_show_setup() {
        assert_eq!(mode(), "normal");
    }
}
