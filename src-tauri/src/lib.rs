//! StepMaster backend. Each module = one section of the app (see src/modules in the UI).
mod cleanup;
mod diagnostics;
mod disk;
mod display;
mod downloads;
mod fsops;
mod games;
mod icons;
mod installer;
mod journal;
mod network;
mod notify;
mod ops;
mod programs;
mod protection;
mod services;
mod settings;
#[cfg(test)]
mod smoke;
mod startup;
mod system;
mod tools;
mod tray;
mod updater;
mod wallpaper;
mod winget;
pub mod util;

use parking_lot::{Mutex, RwLock};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use sysinfo::System;
use tauri::{Emitter, Manager, WindowEvent};

pub struct AppState {
    pub data_dir: PathBuf,
    pub settings: RwLock<settings::Settings>,
    pub sys: Mutex<System>,
    pub disk: disk::DiskState,
    pub update: Mutex<Option<updater::UpdateInfo>>,
    /// Icon cache: spec -> PNG data URL (None = no icon available).
    pub icons: Mutex<std::collections::HashMap<String, Option<String>>>,
    /// Started by autostart with "start minimized": keep the window hidden.
    pub start_hidden: AtomicBool,
    pub ui_ready: AtomicBool,
}

pub use tools::wait_for_pid;

pub fn run() {
    let autostart_launch = std::env::args().any(|a| a == "--autostart");
    // Started by the logon task only to look for an update (see notify.rs).
    let check_launch = std::env::args().any(|a| a == "--check-update");
    notify::CHECK_ONLY.store(check_launch, Ordering::SeqCst);

    let normal = installer::mode() == "normal";
    let mut builder = tauri::Builder::default();
    // Dev preview of the update popup runs next to an installed copy, so it skips the guard.
    if normal && notify::preview_version().is_none() {
        // Must be first: a second launch just focuses the running window.
        // Setup / uninstall run alongside an installed copy, so they skip the guard.
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            // The update-check task found StepMaster already running: it checks by itself.
            if args.iter().any(|a| a == "--check-update") {
                return;
            }
            // Opened by the user while only the update check was running: become the normal app.
            if notify::CHECK_ONLY.swap(false, Ordering::SeqCst) {
                let _ = tray::create(app);
                app.state::<AppState>().start_hidden.store(false, Ordering::SeqCst);
            }
            tools::show_main(app);
        }));
    }
    builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state() == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                        tools::show_main(app);
                        let _ = app.emit("palette://open", ());
                    }
                })
                .build(),
        )
        .setup(move |app| {
            let handle = app.handle().clone();
            let data_dir = app
                .path()
                .app_local_data_dir()
                .unwrap_or_else(|_| std::env::temp_dir().join("StepMaster"));
            let _ = std::fs::create_dir_all(&data_dir);
            let s = settings::load(&data_dir);
            *util::LANG.write() = s.language.clone();
            let start_hidden = (autostart_launch && s.start_minimized) || check_launch;
            let update_notify = s.update_notify;
            let hotkey = s.hotkey.clone();
            let autostart = s.autostart;

            let mut sys = System::new();
            sys.refresh_cpu_usage();
            sys.refresh_memory();

            app.manage(AppState {
                data_dir,
                settings: RwLock::new(s),
                sys: Mutex::new(sys),
                disk: disk::DiskState::default(),
                update: Mutex::new(None),
                icons: Mutex::new(std::collections::HashMap::new()),
                start_hidden: AtomicBool::new(start_hidden),
                ui_ready: AtomicBool::new(false),
            });

            // The setup / uninstall screen is a small plain window: no tray, hotkey or autostart work.
            if !normal {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.set_min_size(None::<tauri::Size>);
                    let _ = w.set_size(tauri::LogicalSize::new(600.0, 680.0));
                    let _ = w.set_resizable(false);
                    let _ = w.set_maximizable(false);
                    let _ = w.center();
                }
                // A portable copy that just self-updated lands here: drop its old exe.
                std::thread::spawn(updater::cleanup_old);
            } else if check_launch {
                // Only the update check: no tray, hotkey or wallpapers. Exits by itself.
                if update_notify {
                    notify::spawn_check(handle.clone(), 0);
                } else {
                    handle.exit(0);
                }
            } else {
                if autostart_launch && update_notify {
                    notify::spawn_check(handle.clone(), 20);
                }
                if let Err(e) = tray::create(&handle) {
                    eprintln!("tray: {e}");
                }
                wallpaper::start(handle.clone(), autostart_launch);
                if let Err(e) = settings::apply_hotkey(&handle, &hotkey) {
                    eprintln!("hotkey: {e}");
                }
                // Background housekeeping: remove the old exe after an update, keep the
                // autostart task pointing at the current exe and the installed version current.
                std::thread::spawn(move || {
                    updater::cleanup_old();
                    installer::refresh_registered_version();
                    if autostart && util::is_admin() {
                        let _ = settings::apply_autostart(true);
                    }
                    // The logon update check follows the setting (also keeps its exe path current).
                    if util::is_admin() {
                        let _ = notify::apply_task(update_notify);
                    }
                    // First run on a PC without winget: put it in now, so "Install" just works.
                    if util::is_admin() {
                        let _ = winget::ensure_winget();
                    }
                });
            }

            // Safety net: show the window even if the UI never reports ready.
            let h2 = handle.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_secs(6));
                let st = h2.state::<AppState>();
                if !st.ui_ready.load(Ordering::SeqCst) && !st.start_hidden.load(Ordering::SeqCst) {
                    tools::show_main(&h2);
                }
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let app = window.app_handle();
                let to_tray = installer::mode() == "normal" && app.state::<AppState>().settings.read().close_to_tray;
                if to_tray {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            // app
            system::app_info,
            settings::settings_get,
            settings::settings_set,
            tools::relaunch_as_admin,
            tools::open_tool,
            tools::open_data_dir,
            tools::open_app_dir,
            tools::reveal_path,
            tools::show_main_window,
            tools::quit_app,
            updater::update_check,
            updater::update_install,
            updater::update_notes,
            updater::whats_new,
            updater::whats_new_seen,
            notify::toast_ready,
            notify::toast_close,
            icons::icons_get,
            installer::install_state,
            installer::install_run,
            installer::uninstall_run,
            winget::winget_state,
            winget::winget_setup,
            winget::winget_install,
            winget::winget_cancel,
            winget::winget_search,
            wallpaper::wallpaper_state,
            wallpaper::wallpaper_set,
            wallpaper::wallpaper_view,
            wallpaper::wallpaper_next,
            wallpaper::wallpaper_options,
            wallpaper::wallpaper_add,
            wallpaper::wallpaper_remove,
            wallpaper::wallpaper_rename,
            wallpaper::wallpaper_restore,
            tools::open_site,
            // system
            system::system_snapshot,
            system::system_overview,
            system::processes_list,
            system::process_kill,
            // cleanup
            cleanup::cleanup_scan,
            cleanup::cleanup_run,
            cleanup::history_get,
            // disk
            disk::disk_scan,
            disk::disk_children,
            disk::disk_large_files,
            disk::disk_cancel,
            ops::op_cancel,
            journal::journal_get,
            journal::journal_revert,
            disk::duplicates_scan,
            disk::fs_recycle,
            downloads::downloads_scan,
            // diagnostics
            diagnostics::diag_run,
            diagnostics::diag_last,
            diagnostics::diag_fix,
            // startup, services, restore
            startup::startup_list,
            startup::startup_set,
            services::services_list,
            services::service_action,
            services::service_start_mode,
            services::restore_status,
            services::restore_create,
            services::restore_enable,
            // programs
            programs::programs_list,
            programs::program_uninstall,
            // protection
            protection::protection_status,
            protection::defender_quick_scan,
            protection::defender_update,
            protection::firewall_set,
            protection::updates_history,
            // network
            network::network_adapters,
            network::dns_flush,
            network::dns_set,
            network::network_reset,
            network::speed_test,
            network::connections_list,
            network::wifi_profiles,
            network::wifi_analyze,
            // monitors
            display::display_list,
            display::display_set_rate,
            // games
            games::game_status,
            games::game_apply,
            games::game_revert,
            games::game_candidates,
            games::gpu_prefs,
            games::gpu_pref_set,
            games::gpu_pref_remove,
        ])
        .run(tauri::generate_context!())
        .expect("error while running StepMaster");
}
