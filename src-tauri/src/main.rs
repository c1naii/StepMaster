// No console window in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // After an update or "restart as admin", wait for the previous process to exit first,
    // otherwise the single-instance guard would hand control back to it.
    for arg in std::env::args() {
        if let Some(pid) = arg.strip_prefix("--wait-pid=").and_then(|p| p.parse::<u32>().ok()) {
            stepmaster_lib::wait_for_pid(pid);
        }
    }
    stepmaster_lib::run();
}
