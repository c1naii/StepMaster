//! Window/app commands and launching Windows tools.
use crate::util::{self, tr, Res};
use crate::AppState;
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Manager, State};

/// Fixed list of tools the UI may open. Nothing else can be launched through this command.
#[tauri::command]
pub fn open_tool(tool: String) -> Res<()> {
    let (target, params): (&str, Option<&str>) = match tool.as_str() {
        "taskmgr" => ("taskmgr.exe", None),
        "devmgmt" => ("devmgmt.msc", None),
        "control" => ("control.exe", None),
        "msinfo" => ("msinfo32.exe", None),
        "diskmgmt" => ("diskmgmt.msc", None),
        "services" => ("services.msc", None),
        "rstrui" => ("rstrui.exe", None),
        "firewall" => ("wf.msc", None),
        "defender" => ("windowsdefender:", None),
        "updates" => ("ms-settings:windowsupdate", None),
        _ => return Err(tr("Неизвестный инструмент", "Unknown tool")),
    };
    util::shell_open(target, params, "open")
}

/// Opens Explorer with the item selected (or the folder itself).
#[tauri::command]
pub fn reveal_path(path: String) -> Res<()> {
    let p = Path::new(&path);
    if !p.is_absolute() || !p.exists() {
        return Err(tr("Файл не найден", "File not found"));
    }
    std::process::Command::new(util::sys_tool("explorer.exe"))
        .raw_arg(format!("/select,\"{}\"", path.replace('"', "")))
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

fn open_dir(p: &Path) -> Res<()> {
    std::fs::create_dir_all(p).ok();
    std::process::Command::new(util::sys_tool("explorer.exe"))
        .raw_arg(format!("\"{}\"", p.to_string_lossy().replace('"', "")))
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn open_data_dir(state: State<'_, AppState>) -> Res<()> {
    open_dir(&state.data_dir)
}

#[tauri::command]
pub fn open_app_dir() -> Res<()> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    reveal_path(exe.to_string_lossy().to_string())
}

#[tauri::command]
pub fn relaunch_as_admin(app: AppHandle) -> Res<()> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let args = format!("--wait-pid={}", std::process::id());
    util::shell_open(&exe.to_string_lossy(), Some(&args), "runas")
        .map_err(|_| tr("Запуск от администратора отменён", "Elevation was cancelled"))?;
    app.exit(0);
    Ok(())
}

pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// Called by the UI once it has rendered, so the window never flashes empty.
#[tauri::command]
pub fn show_main_window(app: AppHandle, state: State<'_, AppState>) {
    state.ui_ready.store(true, Ordering::SeqCst);
    if !state.start_hidden.load(Ordering::SeqCst) {
        show_main(&app);
    }
}

#[tauri::command]
pub fn quit_app(app: AppHandle) {
    app.exit(0);
}

/// Before starting, wait until a previous instance (updater / elevation relaunch) has exited.
pub fn wait_for_pid(pid: u32) {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE};
    unsafe {
        let h = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
        if !h.is_null() {
            WaitForSingleObject(h, 15_000);
            CloseHandle(h);
        }
    }
    let _ = util::CREATE_NO_WINDOW; // keep import used on all paths
}
