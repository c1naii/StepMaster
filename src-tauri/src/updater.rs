//! Self-update from GitHub Releases. The release must contain `StepMaster.exe` and
//! `StepMaster.exe.sig` (minisign signature made with the private key kept outside the repo).
//! The running exe is renamed to *.old (allowed on Windows), the new one takes its place,
//! then the app restarts. The file keeps whatever name it has on disk.
use crate::util::{self, tr, Res};
use crate::AppState;
use base64::Engine;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

/// "owner/repo" on GitHub. Empty = updates disabled (check returns "no update").
pub const REPO: &str = "c1naii/StepMaster";
/// Public key from `tauri signer generate` (base64 of the minisign public key file).
const PUBKEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDE1MzRDRjZERDRFMEM1NzYKUldSMnhlRFViYzgwRlk2NXV3YzBnVjdpYTM3MjMxN2F4c294cGdZNDBtL3lYZTFtT3lFa05YWVMK";

const EXE_ASSET: &str = "StepMaster.exe";
const SIG_ASSET: &str = "StepMaster.exe.sig";

#[derive(Clone, Default)]
pub struct UpdateInfo {
    exe_url: String,
    sig_url: String,
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

/// Compares dotted numeric versions ("1.10.0" > "1.9.3"). Non-numeric parts count as 0.
pub fn is_newer(remote: &str, local: &str) -> bool {
    let parse = |v: &str| -> Vec<u64> {
        v.trim()
            .trim_start_matches(['v', 'V'])
            .split(['.', '-', '+'])
            .take(3)
            .map(|p| p.parse::<u64>().unwrap_or(0))
            .collect()
    };
    let (mut r, mut l) = (parse(remote), parse(local));
    r.resize(3, 0);
    l.resize(3, 0);
    r > l
}

fn client() -> Res<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent("StepMaster-Updater")
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|e| e.to_string())
}

#[derive(Serialize)]
pub struct Check {
    available: bool,
}

/// Latest GitHub release newer than `local`, with both assets present. None = up to date.
pub async fn find_update(local: &str) -> Res<Option<UpdateInfo>> {
    if REPO.is_empty() {
        return Ok(None);
    }
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let resp = client()?
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| format!("{}: {e}", tr("Нет связи с GitHub", "Can't reach GitHub")))?;
    if resp.status().as_u16() == 404 {
        return Ok(None); // no releases yet
    }
    if !resp.status().is_success() {
        return Err(format!("GitHub: {}", resp.status()));
    }
    let rel: Release = resp.json().await.map_err(|e| e.to_string())?;
    if rel.draft || rel.prerelease || !is_newer(&rel.tag_name, local) {
        return Ok(None);
    }
    let find = |n: &str| rel.assets.iter().find(|a| a.name.eq_ignore_ascii_case(n)).map(|a| a.browser_download_url.clone());
    Ok(match (find(EXE_ASSET), find(SIG_ASSET)) {
        (Some(exe_url), Some(sig_url)) => Some(UpdateInfo { exe_url, sig_url }),
        _ => None,
    })
}

#[tauri::command]
pub async fn update_check(app: AppHandle) -> Res<Check> {
    let found = find_update(env!("CARGO_PKG_VERSION")).await?;
    let available = found.is_some();
    *app.state::<AppState>().update.lock() = found;
    Ok(Check { available })
}

/// Downloads the new exe and its signature; returns the bytes only if the signature is valid.
pub async fn download_verified(info: &UpdateInfo, on_progress: impl Fn(f64)) -> Res<Vec<u8>> {
    let c = client()?;
    let net = |e: reqwest::Error| format!("{}: {e}", tr("Ошибка загрузки", "Download error"));
    let sig = c.get(&info.sig_url).send().await.map_err(net)?.text().await.map_err(net)?;
    let resp = c.get(&info.exe_url).send().await.map_err(net)?;
    if !resp.status().is_success() {
        return Err(tr("Файл обновления недоступен", "Update file is unavailable"));
    }
    const MAX_SIZE: u64 = 150 * 1024 * 1024;
    let total = resp.content_length().unwrap_or(0);
    if total > MAX_SIZE {
        return Err(tr("Файл обновления слишком большой", "Update file is too large"));
    }
    let mut data: Vec<u8> = Vec::with_capacity(total as usize);
    let mut stream = resp.bytes_stream();
    let mut last = 0.0;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(net)?;
        data.extend_from_slice(&chunk);
        if data.len() as u64 > MAX_SIZE {
            return Err(tr("Файл обновления слишком большой", "Update file is too large"));
        }
        if total > 0 {
            let p = (data.len() as f64 / total as f64 * 100.0).min(100.0);
            if p - last >= 1.0 {
                last = p;
                on_progress(p);
            }
        }
    }
    verify(&data, &sig)?;
    // A real Windows executable starts with "MZ".
    if data.len() < 1024 || &data[0..2] != b"MZ" {
        return Err(tr("Файл обновления повреждён", "Update file is corrupted"));
    }
    Ok(data)
}

fn verify(data: &[u8], sig_b64: &str) -> Res<()> {
    let b64 = base64::engine::general_purpose::STANDARD;
    let bad = || tr("Подпись обновления не прошла проверку", "Update signature check failed");
    let pk_text = String::from_utf8(b64.decode(PUBKEY).map_err(|_| bad())?).map_err(|_| bad())?;
    let sig_text = String::from_utf8(b64.decode(sig_b64.trim()).map_err(|_| bad())?).map_err(|_| bad())?;
    let pk = minisign_verify::PublicKey::decode(&pk_text).map_err(|_| bad())?;
    let sig = minisign_verify::Signature::decode(&sig_text).map_err(|_| bad())?;
    pk.verify(data, &sig, false).map_err(|_| bad())
}

#[derive(Serialize, Clone)]
struct Progress {
    percent: f64,
}

#[tauri::command]
pub async fn update_install(app: AppHandle) -> Res<()> {
    let info = app
        .state::<AppState>()
        .update
        .lock()
        .clone()
        .ok_or_else(|| tr("Обновление не найдено — проверьте ещё раз", "No update found — check again"))?;
    let app2 = app.clone();
    let data = download_verified(&info, move |p| {
        let _ = app2.emit("update://progress", Progress { percent: p });
    })
    .await?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    util::blocking(move || swap_and_restart(&exe, &data)).await?;
    app.exit(0);
    Ok(())
}

fn sibling(exe: &Path, ext: &str) -> PathBuf {
    let mut s = exe.as_os_str().to_owned();
    s.push(ext);
    PathBuf::from(s)
}

fn swap_and_restart(exe: &Path, data: &[u8]) -> Res<()> {
    let new = sibling(exe, ".new");
    let old = sibling(exe, ".old");
    {
        let mut f = std::fs::File::create(&new).map_err(|e| format!("{}: {e}", tr("Нет доступа к папке программы", "Can't write to the program folder")))?;
        f.write_all(data).map_err(|e| e.to_string())?;
        f.sync_all().ok();
    }
    let _ = std::fs::remove_file(&old);
    std::fs::rename(exe, &old).map_err(|e| {
        let _ = std::fs::remove_file(&new);
        e.to_string()
    })?;
    if let Err(e) = std::fs::rename(&new, exe) {
        let _ = std::fs::rename(&old, exe); // roll back
        return Err(e.to_string());
    }
    let arg = format!("--wait-pid={}", std::process::id());
    if std::process::Command::new(exe).arg(&arg).spawn().is_err() {
        // Not elevated (dev run): ask Windows to elevate.
        util::shell_open(&exe.to_string_lossy(), Some(&arg), "runas")?;
    }
    Ok(())
}

/// Remove the previous version left after an update.
pub fn cleanup_old() {
    if let Ok(exe) = std::env::current_exe() {
        let old = sibling(&exe, ".old");
        for _ in 0..5 {
            if !old.exists() || std::fs::remove_file(&old).is_ok() {
                break;
            }
            std::thread::sleep(Duration::from_millis(500));
        }
        let _ = std::fs::remove_file(sibling(&exe, ".new"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn versions() {
        assert!(is_newer("v1.0.1", "1.0.0"));
        assert!(is_newer("1.10.0", "1.9.9"));
        assert!(!is_newer("1.0.0", "1.0.0"));
        assert!(!is_newer("v0.9.0", "1.0.0"));
        assert!(!is_newer("garbage", "1.0.0"));
    }
    #[test]
    fn rejects_bad_signature() {
        assert!(verify(b"hello", "bm90IGEgc2lnbmF0dXJl").is_err());
    }

    /// Live check against the published GitHub release: finds it, downloads it, verifies the
    /// signature. Does not replace anything. Run: cargo test --lib live_github -- --ignored --nocapture
    #[test]
    #[ignore]
    fn live_github_update_pipeline() {
        use tauri::async_runtime::block_on;
        assert!(block_on(find_update(env!("CARGO_PKG_VERSION"))).unwrap().is_none(), "current version must be up to date");
        let info = block_on(find_update("0.0.1")).unwrap().expect("an older version must see the update");
        let bytes = block_on(download_verified(&info, |_| {})).expect("download + signature");
        println!("downloaded {} bytes from {}", bytes.len(), info.exe_url);
    }

    /// Checks the real signed build in /release against the embedded public key.
    /// Run after `npm run release`: cargo test --lib updater -- --ignored
    #[test]
    #[ignore]
    fn release_build_signature_is_valid() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("release");
        let exe = std::fs::read(dir.join(EXE_ASSET)).expect("release exe");
        let sig = std::fs::read_to_string(dir.join(SIG_ASSET)).expect("sig");
        verify(&exe, &sig).expect("signature must verify");
        let mut tampered = exe.clone();
        let n = tampered.len() / 2;
        tampered[n] ^= 0xFF;
        assert!(verify(&tampered, &sig).is_err(), "tampered file must be rejected");
    }
}
