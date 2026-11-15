fn main() {
    // Release builds ask for administrator rights at launch (UAC). Dev builds run normally,
    // so `tauri dev` works from a regular terminal; the app shows an "admin" banner then.
    let release = std::env::var("PROFILE").map(|p| p == "release").unwrap_or(false);
    let manifest = if release {
        include_str!("app.manifest")
    } else {
        include_str!("app-dev.manifest")
    };
    println!("cargo:rerun-if-changed=app.manifest");
    println!("cargo:rerun-if-changed=app-dev.manifest");
    let windows = tauri_build::WindowsAttributes::new().app_manifest(manifest);
    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows)).expect("failed to run tauri-build");
}
