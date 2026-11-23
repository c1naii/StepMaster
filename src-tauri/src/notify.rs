//! Update popup in the bottom-right corner, in StepMaster's own style.
//! - In the tray (started with Windows): checks for an update shortly after start and shows the
//!   popup if the main window isn't open.
//! - Not running at all: a logon task starts `StepMaster.exe --check-update`, which only checks,
//!   shows the popup if there is an update and exits otherwise (or when the popup is closed).
use crate::util::{tr, Res};
use crate::AppState;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

/// This process was started only to check for updates (no tray, no main window).
pub static CHECK_ONLY: AtomicBool = AtomicBool::new(false);

pub const TASK_NAME: &str = "StepMaster Update Check";
const LABEL: &str = "toast";
const WIDTH: f64 = 372.0;
const HEIGHT: f64 = 276.0;
const MARGIN: f64 = 12.0;

/// Creates or removes the logon task (follows the "update notifications" setting).
pub fn apply_task(enabled: bool) -> Res<()> {
    if enabled {
        // An hour is plenty for someone to notice the popup; then the check process is ended.
        crate::settings::register_logon_task(TASK_NAME, "StepMaster update check", "--check-update", "PT45S", "PT1H")
    } else {
        crate::settings::delete_task(TASK_NAME)
    }
}

fn main_visible(app: &AppHandle) -> bool {
    app.get_webview_window("main").and_then(|w| w.is_visible().ok()).unwrap_or(false)
}

/// Opens the popup (hidden until the page reports it has drawn itself).
pub fn show(app: &AppHandle) -> Res<()> {
    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.show();
        return Ok(());
    }
    let mon = app
        .primary_monitor()
        .ok()
        .flatten()
        .ok_or_else(|| tr("Монитор не найден", "Monitor not found"))?;
    let scale = mon.scale_factor();
    // Work area = the screen without the taskbar, so the popup sits right above it.
    let area = mon.work_area();
    let right = (area.position.x as f64 + area.size.width as f64) / scale;
    let bottom = (area.position.y as f64 + area.size.height as f64) / scale;
    let w = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("index.html?view=toast".into()))
        .title("StepMaster")
        .inner_size(WIDTH, HEIGHT)
        .position(right - WIDTH - MARGIN, bottom - HEIGHT - MARGIN)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .skip_taskbar(true)
        .always_on_top(true)
        .focused(false)
        .visible(false)
        .build()
        .map_err(|e| e.to_string())?;
    // Safety net: show it even if the page never reports ready.
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(4));
        let _ = w.show();
    });
    Ok(())
}

/// Dev builds only: SM_FAKE_VERSION=1.0.6 pretends to be an older version, so the popup can be
/// seen without publishing anything. Release builds always use the real version.
pub fn preview_version() -> Option<String> {
    if cfg!(debug_assertions) {
        std::env::var("SM_FAKE_VERSION").ok().filter(|v| !v.is_empty())
    } else {
        None
    }
}

fn local_version() -> String {
    preview_version().unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string())
}

/// Checks GitHub for a newer version (a few tries: the network may still be coming up after
/// logon) and shows the popup. In check-only mode the process exits when there is nothing to show.
pub fn spawn_check(app: AppHandle, delay_secs: u64) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(delay_secs)).await;
        let mut found = None;
        for attempt in 0..3 {
            match crate::updater::find_update(&local_version()).await {
                Ok(f) => {
                    found = f;
                    break;
                }
                Err(_) if attempt < 2 => tokio::time::sleep(Duration::from_secs(20)).await,
                Err(_) => {}
            }
        }
        let check_only = CHECK_ONLY.load(Ordering::SeqCst);
        match found {
            Some(info) => {
                *app.state::<AppState>().update.lock() = Some(info);
                // An open window already shows its own update dialog.
                if (check_only || !main_visible(&app)) && show(&app).is_err() && check_only {
                    app.exit(0);
                }
            }
            None if check_only => app.exit(0),
            None => {}
        }
    });
}

/// The popup page has drawn itself: show it without taking focus from what the user is doing.
#[tauri::command]
pub fn toast_ready(app: AppHandle) {
    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.show();
    }
}

/// "Later" / close: hide the popup. A check-only process has nothing else to do and exits.
#[tauri::command]
pub fn toast_close(app: AppHandle) {
    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.destroy();
    }
    if CHECK_ONLY.load(Ordering::SeqCst) {
        app.exit(0);
    }
}
