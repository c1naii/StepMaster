//! Window/app commands and launching Windows tools.
use crate::util::{self, tr, Res};
use crate::AppState;
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Manager, State};

/// Official download pages for programs that winget doesn't have. Only these can be opened.
const SITES: [&str; 4] = [
    "https://www.nvidia.com/ru-ru/software/nvidia-app/",
    "https://www.amd.com/ru/support/download/drivers.html",
    "https://www.whatsapp.com/download",
    "https://www.spotify.com/download/windows/",
];

#[tauri::command]
pub fn open_site(url: String) -> Res<()> {
    if !SITES.contains(&url.as_str()) {
        return Err(tr("Неизвестный сайт", "Unknown site"));
    }
    util::shell_open(&url, None, "open")
}

/// Fixed list of tools the UI may open. Nothing else can be launched through this command.
#[tauri::command]
pub fn open_tool(tool: String) -> Res<()> {
    // System programs are opened by full path (never searched in the current folder).
    let exe = |n: &str| util::sys_tool(n).to_string_lossy().to_string();
    let msc = |n: &str| (exe("mmc.exe"), Some(format!("\"{}\"", exe(n))));
    let uri = |u: &str| (u.to_string(), None);
    let (target, params): (String, Option<String>) = match tool.as_str() {
        "taskmgr" => (exe("taskmgr.exe"), None),
        "devmgmt" => msc("devmgmt.msc"),
        "control" => (exe("control.exe"), None),
        "msinfo" => (exe("msinfo32.exe"), None),
        "diskmgmt" => msc("diskmgmt.msc"),
        "services" => msc("services.msc"),
        "rstrui" => (exe("rstrui.exe"), None),
        "firewall" => msc("wf.msc"),
        "defender" => uri("windowsdefender:"),
        "updates" => uri("ms-settings:windowsupdate"),
        "taskschd" => msc("taskschd.msc"),
        "appinstaller" => uri("ms-windows-store://pdp/?productid=9NBLGGH4NNS1"),
        _ => return Err(tr("Неизвестный инструмент", "Unknown tool")),
    };
    util::shell_open(&target, params.as_deref(), "open")
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
    let mut args = format!("--wait-pid={}", std::process::id());
    // Keep the uninstall screen when it asks for admin rights.
    if std::env::args().any(|a| a == "--uninstall") {
        args.push_str(" --uninstall");
    }
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
