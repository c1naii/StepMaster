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
fn smoke_diagnostics_and_downloads() {
    let t = std::time::Instant::now();
    let ps: crate::diagnostics::PsRaw = crate::util::ps_json(crate::diagnostics::PS_SCRIPT).expect("diagnostics PowerShell must parse");
    println!("\n== diag raw in {:?}", t.elapsed());
    println!("shutdowns {:?} bugchecks {:?} whea {:?} disk {:?} ntfs {:?} throttling {:?} wu {:?}", ps.shutdowns, ps.bugchecks, ps.whea, ps.disk_errors, ps.ntfs_errors, ps.throttling, ps.wu_failures);
    println!("crashes {:?}", ps.app_crashes);
    println!("devices {:?}", ps.devices);
    println!("disks {:?} smart {:?} thermal {:?} battery {:?}/{:?}", ps.disks, ps.smart_fail, ps.thermal_c, ps.battery_design, ps.battery_full);
    println!("defender available={} rt={} sig={} scan={} threats={} recent={} excl={}", ps.defender.available, ps.defender.realtime, ps.defender.sig_age_days, ps.defender.quick_scan_age_days, ps.defender.threats.len(), ps.defender.recent_detections, ps.defender.exclusions.len());
    println!("otherAv {:?} firewallOff {:?} tasks {}", ps.other_av, ps.firewall_off, ps.tasks.len());
    println!("suspicious tasks: {:?}", crate::diagnostics::suspicious_tasks(&ps.tasks));
    let hosts = std::fs::read_to_string(crate::util::system_root().join(r"System32\drivers\etc\hosts")).unwrap_or_default();
    println!("hosts suspicious: {:?}", crate::diagnostics::hosts_suspicious(&hosts));

    let installed = crate::programs::installed_names();
    for place in ["{374DE290-123F-4565-9164-39C4925E467B}", "Desktop"] {
        let fallback = if place == "Desktop" { "Desktop" } else { "Downloads" };
        if let Some(root) = crate::downloads::known_folder(place, fallback) {
            let t = std::time::Instant::now();
            let r = crate::downloads::analyze(&root, &installed);
            let v = serde_json::to_value(&r).unwrap();
            let items = v["items"].as_array().unwrap();
            let count = |f: &str| items.iter().filter(|i| i["flags"].as_array().unwrap().iter().any(|x| x == f)).count();
            println!(
                "\n== {} : {} items, total {} MB in {:?}; installed={} extracted={} duplicate={} partial={} old={}",
                root.display(),
                items.len(),
                v["total"].as_u64().unwrap() / 1024 / 1024,
                t.elapsed(),
                count("installed"),
                count("extracted"),
                count("duplicate"),
                count("partial"),
                count("old")
            );
            for i in items.iter().filter(|i| i["flags"].as_array().unwrap().iter().any(|x| x == "installed")).take(80) {
                println!("   installer: {} -> {}", i["name"], i["installedAs"]);
            }
        }
    }
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

#[test]
#[ignore]
fn smoke_display_and_wifi() {
    show("displays", &crate::display::list());
    let t = std::time::Instant::now();
    match block_on(crate::network::wifi_analyze()) {
        Ok(v) => show(&format!("wifi_analyze ({:?})", t.elapsed()), &v),
        Err(e) => println!("\n== wifi_analyze ERROR: {e}"),
    }
}
