//! Read-only checks against the real system. Run: cargo test --lib smoke -- --ignored --nocapture
//! Nothing here changes the system.
use tauri::async_runtime::block_on;

fn show<T: serde::Serialize>(label: &str, v: &T) {
    let s = serde_json::to_string(v).unwrap();
    let short: String = s.chars().take(600).collect();
    println!("\n== {label} ({} bytes)\n{short}", s.len());
}

#[test]
#[ignore]
fn smoke_icons() {
    let p = block_on(crate::programs::programs_list()).unwrap();
    let v = serde_json::to_value(&p).unwrap();
    let list = v.as_array().unwrap();
    let t = std::time::Instant::now();
    let mut with_source = 0;
    let mut extracted = 0;
    let mut missing = Vec::new();
    for x in list {
        let spec = x["icon"].as_str().unwrap_or("");
        if !spec.is_empty() {
            with_source += 1;
        }
        if !spec.is_empty() && crate::icons::icon_for(spec).is_some() {
            extracted += 1;
        } else {
            missing.push(format!("{} [{}] '{}'", x["name"], x["kind"], spec));
        }
    }
    println!("\n== icons: {} programs, {} with source, {} extracted in {:?}", list.len(), with_source, extracted, t.elapsed());
    for m in missing.iter().take(40) {
        println!("   no icon: {m}");
    }
}

#[test]
#[ignore]
fn smoke_read_only() {
    println!("admin: {}", crate::util::is_admin());
    show("os_info", &crate::system::os_info());
    show("disks", &crate::system::list_disks());
    show("startup", &crate::startup::read_all());
    show("gpu_prefs", &crate::games::gpu_prefs());
    // Never print passwords: only whether one was readable.
    let wifi: Vec<(String, String, bool)> = crate::network::read_wifi_profiles()
        .into_iter()
        .map(|w| (w.ssid, w.auth, w.password.is_some()))
        .collect();
    show("wifi (ssid, auth, has_password)", &wifi);

    let t = std::time::Instant::now();
    match block_on(crate::programs::programs_list()) {
        Ok(p) => {
            let kinds = ["win32", "store", "portable"].map(|k| {
                let v = serde_json::to_value(&p).unwrap();
                v.as_array().unwrap().iter().filter(|x| x["kind"] == k).count()
            });
            println!("\n== programs: {} total (win32/store/portable = {:?}) in {:?}", p.len(), kinds, t.elapsed());
            show("programs sample", &p.iter().take(3).collect::<Vec<_>>());
            let portable: Vec<_> = serde_json::to_value(&p).unwrap().as_array().unwrap().iter()
                .filter(|x| x["kind"] == "portable").map(|x| x["location"].clone()).collect();
            show("portable folders", &portable);
        }
        Err(e) => println!("programs ERROR: {e}"),
    }

    for (name, r) in [
        ("services", block_on(crate::services::services_list()).map(|v| serde_json::to_value(v.iter().take(3).collect::<Vec<_>>()).unwrap())),
        ("protection", block_on(crate::protection::protection_status()).map(|v| serde_json::to_value(v).unwrap())),
        ("hotfixes", block_on(crate::protection::updates_history()).map(|v| serde_json::to_value(v.iter().take(3).collect::<Vec<_>>()).unwrap())),
        ("adapters", block_on(crate::network::network_adapters()).map(|v| serde_json::to_value(v).unwrap())),
        ("connections", block_on(crate::network::connections_list()).map(|v| serde_json::to_value(v.iter().take(3).collect::<Vec<_>>()).unwrap())),
    ] {
        let t = std::time::Instant::now();
        match r {
            Ok(v) => show(&format!("{name} ({:?})", t.elapsed()), &v),
            Err(e) => println!("\n== {name} ERROR: {e}"),
        }
    }

    let t = std::time::Instant::now();
    match block_on(crate::cleanup::cleanup_scan()) {
        Ok(c) => show(&format!("cleanup_scan ({:?})", t.elapsed()), &c),
        Err(e) => println!("cleanup ERROR: {e}"),
    }
}
