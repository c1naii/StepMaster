//! Tray icon: open/quit menu, click to show, optional live CPU/RAM tooltip.
use crate::util::tr;
use crate::AppState;
use std::time::Duration;
use sysinfo::System;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};

const TRAY_ID: &str = "main";

fn build_menu(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let open = MenuItem::with_id(app, "open", tr("Открыть StepMaster", "Open StepMaster"), true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", tr("Выход", "Quit"), true, None::<&str>)?;
    Menu::with_items(app, &[&open, &sep, &quit])
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let menu = build_menu(app)?;
    let mut b = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("StepMaster")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => crate::tools::show_main(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                crate::tools::show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        b = b.icon(icon.clone());
    }
    b.build(app)?;

    // Live tooltip: CPU and memory, refreshed every 3 s when enabled.
    let handle = app.clone();
    std::thread::spawn(move || {
        let mut sys = System::new();
        loop {
            std::thread::sleep(Duration::from_secs(3));
            let enabled = handle.state::<AppState>().settings.read().tray_monitor;
            let text = if enabled {
                sys.refresh_cpu_usage();
                sys.refresh_memory();
                let cpu = sys.global_cpu_usage().clamp(0.0, 100.0).round();
                let mem = if sys.total_memory() > 0 {
                    (sys.used_memory() as f64 / sys.total_memory() as f64 * 100.0).round()
                } else {
                    0.0
                };
                format!("StepMaster\n{} {cpu}% · {} {mem}%", tr("ЦП", "CPU"), tr("Память", "RAM"))
            } else {
                "StepMaster".to_string()
            };
            if let Some(t) = handle.tray_by_id(TRAY_ID) {
                let _ = t.set_tooltip(Some(text));
            }
        }
    });
    Ok(())
}

pub fn refresh_menu(app: &AppHandle) {
    if let (Some(t), Ok(menu)) = (app.tray_by_id(TRAY_ID), build_menu(app)) {
        let _ = t.set_menu(Some(menu));
    }
}
