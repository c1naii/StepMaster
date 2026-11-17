//! Smart review of Downloads (and Desktop): groups by type and flags what is most likely safe
//! to remove: installers of programs that are already installed, archives that were already
//! extracted, duplicate copies, unfinished downloads and old files. Deletion goes through
//! disk::fs_recycle (Recycle Bin + protected-path checks) — nothing is deleted here.
use crate::fsops;
use crate::util::{self, env_path, tr, Res, FILE_ATTRIBUTE_REPARSE_POINT};
use rayon::prelude::*;
use serde::Serialize;
use std::collections::HashMap;
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const OLD_DAYS: u64 = 90;
const MAX_HASH_SIZE: u64 = 2 * 1024 * 1024 * 1024;

/// Real location of a shell folder (the user may have moved Downloads to another drive).
pub fn known_folder(value: &str, fallback: &str) -> Option<PathBuf> {
    use winreg::enums::*;
    let from_reg = winreg::RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Explorer\User Shell Folders")
        .and_then(|k| k.get_value::<String, _>(value))
        .ok()
        .and_then(|v| crate::icons::parse_spec(&v).map(|(p, _)| p));
    from_reg.filter(|p| p.is_dir()).or_else(|| env_path("USERPROFILE").map(|h| h.join(fallback)).filter(|p| p.is_dir()))
}

pub fn category_of(ext: &str) -> &'static str {
    match ext {
        "exe" | "msi" | "msix" | "msixbundle" | "appx" | "appxbundle" => "installers",
        "zip" | "rar" | "7z" | "tar" | "gz" | "tgz" | "bz2" | "xz" | "cab" => "archives",
        "iso" | "img" | "vhd" | "vhdx" => "images",
        "mp4" | "mkv" | "avi" | "mov" | "wmv" | "webm" | "flv" | "m4v" | "ts" => "videos",
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp" | "heic" | "tiff" | "svg" | "ico" | "raw" | "psd" => "pictures",
        "mp3" | "wav" | "flac" | "aac" | "ogg" | "m4a" | "wma" | "opus" => "audio",
        "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "txt" | "rtf" | "odt" | "csv" | "md" | "epub" | "djvu" => "documents",
        "torrent" => "torrents",
        "crdownload" | "part" | "partial" | "download" | "opdownload" | "tmp" => "partial",
        _ => "other",
    }
}

/// Words in a name: "OBS-Studio-30.1-Full-Installer-x64" -> [obs, studio, 30, 1, full, installer, x64],
/// "DiscordSetup" -> [discord, setup], "OperaGXSetup" -> [opera, gx, setup].
pub fn words(stem: &str) -> Vec<String> {
    let chars: Vec<char> = stem.chars().collect();
    let mut out = Vec::new();
    let mut cur = String::new();
    for (i, &c) in chars.iter().enumerate() {
        if !c.is_alphanumeric() {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            continue;
        }
        if c.is_uppercase() && !cur.is_empty() {
            let prev = chars[i - 1];
            let next_lower = chars.get(i + 1).map(|n| n.is_lowercase()).unwrap_or(false);
            // "aB" -> a|B ; "GXSetup" -> GX|Setup (end of an acronym)
            if prev.is_lowercase() || prev.is_ascii_digit() || (prev.is_uppercase() && next_lower) {
                out.push(std::mem::take(&mut cur));
            }
        }
        cur.extend(c.to_lowercase());
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Vendor names: on their own they don't identify a product (GoogleDrive ≠ Google Chrome).
const VENDORS: &[&str] = &[
    "google", "microsoft", "adobe", "nvidia", "intel", "amd", "apple", "mozilla", "oracle", "logitech", "razer",
    "asus", "msi", "epic", "valve", "autodesk", "corel", "jetbrains", "yandex", "mail", "tencent", "realtek",
];

const NOISE: &[&str] = &[
    "setup", "install", "installer", "installation", "x64", "x86", "x32", "win", "win32", "win64", "windows", "amd64",
    "arm64", "full", "offline", "online", "web", "latest", "release", "stable", "portable", "update", "v", "version",
    "bit", "64bit", "32bit", "en", "ru", "us", "multi", "lang", "beta", "final", "client", "launcher", "app", "pc",
];

/// Product words of an installer name, without noise and version numbers:
/// "OBS-Studio-30.1-Full-Installer-x64" -> [obs, studio].
pub fn product_words(stem: &str) -> Vec<String> {
    words(stem)
        .into_iter()
        .filter(|x| !NOISE.contains(&x.as_str()) && !x.chars().all(|c| c.is_ascii_digit()))
        .map(|x| x.trim_end_matches(|c: char| c.is_ascii_digit()).to_string())
        .filter(|x| x.chars().count() >= 2 && !NOISE.contains(&x.as_str()))
        .take(3)
        .collect()
}

/// The product part of an installer name joined: "OBS-Studio-30.1-Full-Installer-x64" -> "obsstudio".
#[cfg(test)]
pub fn installer_key(stem: &str) -> Option<String> {
    let key: String = product_words(stem).concat();
    (key.chars().count() >= 3).then_some(key)
}

/// Does `key` equal a run of consecutive words of the installed name (joined)?
/// Returns (matched, run starts at the first word, run covers the whole name).
fn run_match(key: &str, installed: &[String]) -> Option<(bool, bool)> {
    for i in 0..installed.len() {
        let mut cat = String::new();
        for (j, w) in installed.iter().enumerate().skip(i) {
            cat.push_str(w);
            if cat == key {
                return Some((i == 0, i == 0 && j + 1 == installed.len()));
            }
            if cat.len() > key.len() {
                break;
            }
        }
    }
    None
}

/// Finds the installed program this installer belongs to. Strict on purpose: a wrong
/// "already installed" would make the user delete an installer they still need.
pub fn match_installed<'a>(stem: &str, installed: &'a [(Vec<String>, String)]) -> Option<&'a str> {
    let p = product_words(stem);
    if p.is_empty() {
        return None;
    }
    let key = p.concat();
    let n = key.chars().count();
    let vendor_only = p.len() == 1 && VENDORS.contains(&p[0].as_str());
    for (iw, name) in installed {
        let Some((at_start, whole)) = run_match(&key, iw) else { continue };
        if vendor_only && !whole {
            continue; // "Google" alone never means "Google Chrome"
        }
        if n >= 4 || (n == 3 && at_start) {
            return Some(name.as_str());
        }
    }
    None
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    path: String,
    name: String,
    size: u64,
    is_dir: bool,
    category: String,
    age_days: u64,
    /// Reasons it's likely removable: installed, extracted, duplicate, partial, old.
    flags: Vec<String>,
    /// Name of the installed program for "installed".
    installed_as: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    root: String,
    total: u64,
    items: Vec<Item>,
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn age_days(md: &std::fs::Metadata) -> u64 {
    let newest = [md.modified().ok(), md.created().ok()].into_iter().flatten().max();
    newest
        .and_then(|t| SystemTime::now().duration_since(t).ok())
        .map(|d| d.as_secs() / 86400)
        .unwrap_or(0)
}

fn created_secs(md: &std::fs::Metadata) -> u64 {
    md.created().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or_else(now_secs)
}

/// Categories where "old" is a useful hint. Personal files (documents, photos, videos, folders)
/// are never marked just because they are old.
fn old_matters(category: &str) -> bool {
    matches!(category, "installers" | "archives" | "images" | "partial" | "torrents")
}

pub fn analyze(root: &Path, installed: &[(Vec<String>, String)]) -> Report {
    let mut items: Vec<Item> = Vec::new();
    let mut dir_names: HashMap<String, ()> = HashMap::new();
    let mut created: HashMap<String, u64> = HashMap::new();
    let entries: Vec<(PathBuf, std::fs::Metadata)> = std::fs::read_dir(root)
        .map(|rd| rd.flatten().filter_map(|e| e.metadata().ok().map(|m| (e.path(), m))).collect())
        .unwrap_or_default();
    for (p, md) in &entries {
        if md.is_dir() {
            if let Some(n) = p.file_name() {
                dir_names.insert(n.to_string_lossy().to_lowercase(), ());
            }
        }
    }
    let sized: Vec<(PathBuf, std::fs::Metadata, u64)> = entries
        .into_par_iter()
        .filter(|(p, md)| {
            let name = p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
            name != "desktop.ini" && md.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0
        })
        .map(|(p, md)| {
            let size = if md.is_dir() { fsops::folder_size(&p, 40_000) } else { md.len() };
            (p, md, size)
        })
        .collect();
    for (p, md, size) in &sized {
        let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        let stem = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let is_dir = md.is_dir();
        let category = if is_dir { "folders" } else if ext == "lnk" || ext == "url" { "shortcuts" } else { category_of(&ext) };
        let age = age_days(md);
        let mut flags = Vec::new();
        let mut installed_as = None;
        if !is_dir {
            created.insert(p.to_string_lossy().to_string(), created_secs(md));
            if category == "installers" {
                if let Some(n) = match_installed(&stem, installed) {
                    flags.push("installed".to_string());
                    installed_as = Some(n.to_string());
                }
            }
            if category == "archives" && dir_names.contains_key(&stem.to_lowercase()) {
                flags.push("extracted".to_string());
            }
            if category == "partial" && age >= 1 {
                flags.push("partial".to_string());
            }
        }
        if age >= OLD_DAYS && !is_dir && old_matters(category) {
            flags.push("old".to_string());
        }
        items.push(Item {
            path: p.to_string_lossy().to_string(),
            name,
            size: *size,
            is_dir,
            category: category.to_string(),
            age_days: age,
            flags,
            installed_as,
        });
    }
    // Duplicates: same size, then same content. The oldest copy is the "original" and stays unflagged.
    let mut by_size: HashMap<u64, Vec<usize>> = HashMap::new();
    for (i, it) in items.iter().enumerate() {
        if !it.is_dir && it.size > 0 && it.size <= MAX_HASH_SIZE && it.category != "shortcuts" {
            by_size.entry(it.size).or_default().push(i);
        }
    }
    let groups: Vec<Vec<usize>> = by_size.into_values().filter(|v| v.len() > 1).collect();
    // Cheap pass first (first + last 64 KB); the full hash only for files that still match.
    // Same-size multi-volume archives (part01..part40) are separated without reading them fully.
    let quick: Vec<(usize, [u8; 32])> = groups
        .par_iter()
        .flat_map(|g| g.iter().filter_map(|&i| quick_hash(&items[i].path).map(|h| (i, h))).collect::<Vec<_>>())
        .collect();
    let mut by_quick: HashMap<(u64, [u8; 32]), Vec<usize>> = HashMap::new();
    for (i, h) in quick {
        by_quick.entry((items[i].size, h)).or_default().push(i);
    }
    let candidates: Vec<Vec<usize>> = by_quick.into_values().filter(|v| v.len() > 1).collect();
    let hashed: Vec<(usize, [u8; 32])> = candidates
        .par_iter()
        .flat_map(|g| g.iter().filter_map(|&i| hash(&items[i].path).map(|h| (i, h))).collect::<Vec<_>>())
        .collect();
    let mut by_hash: HashMap<[u8; 32], Vec<usize>> = HashMap::new();
    for (i, h) in hashed {
        by_hash.entry(h).or_default().push(i);
    }
    for (_, mut same) in by_hash.into_iter().filter(|(_, v)| v.len() > 1) {
        same.sort_by_key(|&i| created.get(&items[i].path).copied().unwrap_or(u64::MAX));
        for &i in same.iter().skip(1) {
            items[i].flags.push("duplicate".to_string());
        }
    }
    items.sort_by_key(|x| std::cmp::Reverse(x.size));
    let total = items.iter().map(|i| i.size).sum();
    Report { root: root.to_string_lossy().to_string(), total, items }
}

fn quick_hash(path: &str) -> Option<[u8; 32]> {
    use std::io::{Read, Seek, SeekFrom};
    const PART: u64 = 64 * 1024;
    let mut f = std::fs::File::open(path).ok()?;
    let len = f.metadata().ok()?.len();
    let mut h = blake3::Hasher::new();
    let mut buf = vec![0u8; PART as usize];
    let n = f.read(&mut buf).ok()?;
    h.update(&buf[..n]);
    if len > PART * 2 {
        f.seek(SeekFrom::End(-(PART as i64))).ok()?;
        let n = f.read(&mut buf).ok()?;
        h.update(&buf[..n]);
    }
    Some(*h.finalize().as_bytes())
}

fn hash(path: &str) -> Option<[u8; 32]> {
    use std::io::Read;
    let mut f = std::fs::File::open(path).ok()?;
    let mut h = blake3::Hasher::new();
    let mut buf = vec![0u8; 512 * 1024];
    loop {
        let n = f.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Some(*h.finalize().as_bytes())
}

#[tauri::command]
pub async fn downloads_scan(place: String) -> Res<Report> {
    util::blocking(move || {
        let root = match place.as_str() {
            "desktop" => known_folder("Desktop", "Desktop"),
            _ => known_folder("{374DE290-123F-4565-9164-39C4925E467B}", "Downloads"),
        }
        .ok_or_else(|| tr("Папка не найдена", "Folder not found"))?;
        let installed = crate::programs::installed_names();
        Ok(analyze(&root, &installed))
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys() {
        assert_eq!(installer_key("DiscordSetup").as_deref(), Some("discord"));
        assert_eq!(installer_key("ChromeSetup").as_deref(), Some("chrome"));
        assert_eq!(installer_key("OBS-Studio-30.1-Full-Installer-x64").as_deref(), Some("obsstudio"));
        assert_eq!(installer_key("vlc-3.0.20-win64").as_deref(), Some("vlc"));
        assert_eq!(installer_key("python-3.12.0-amd64").as_deref(), Some("python"));
        assert_eq!(installer_key("setup"), None);
        assert_eq!(installer_key("x64"), None);
    }

    fn inst(names: &[&str]) -> Vec<(Vec<String>, String)> {
        names.iter().map(|n| (words(n), n.to_string())).collect()
    }

    #[test]
    fn matches_installed() {
        let installed = inst(&["Google Chrome", "Discord", "OBS Studio", "FL Studio", "TikTok LIVE Studio", "Microsoft Office", "Git", "Steam"]);
        assert_eq!(match_installed("ChromeSetup", &installed), Some("Google Chrome"));
        assert_eq!(match_installed("DiscordSetup", &installed), Some("Discord"));
        assert_eq!(match_installed("OBS-Studio-30.1-Full-Installer-x64", &installed), Some("OBS Studio"));
        assert_eq!(match_installed("flstudio_win64_24.2.1.4526", &installed), Some("FL Studio"));
        assert_eq!(match_installed("tiktok_live_studio-1.36.6-x64", &installed), Some("TikTok LIVE Studio"));
        assert_eq!(match_installed("Microsoft Office 2016 x64", &installed), Some("Microsoft Office"));
        assert_eq!(match_installed("Git-2.47.0-64-bit", &installed), Some("Git"));
        assert_eq!(match_installed("SteamSetup", &installed), Some("Steam"));
        assert_eq!(match_installed("setup", &installed), None);
    }

    #[test]
    fn never_claims_not_installed_programs() {
        // The review's false positives: vendor prefixes, short keys, product variants.
        let installed = inst(&["Google Chrome", "Logitech G HUB", "Opera", "Microsoft 365"]);
        assert_eq!(match_installed("GoogleDriveSetup", &installed), None);
        assert_eq!(match_installed("Git-2.47.0-64-bit", &installed), None);
        assert_eq!(match_installed("OperaGXSetup", &installed), None);
        assert_eq!(match_installed("MicrosoftTeamsSetup", &installed), None);
        assert_eq!(words("OperaGXSetup"), vec!["opera", "gx", "setup"]);
    }

    #[test]
    fn analyze_flags() {
        let root = std::env::temp_dir().join(format!("sm-dl-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("project")).unwrap();
        std::fs::write(root.join("project.zip"), b"zipdata").unwrap();
        std::fs::write(root.join("DiscordSetup.exe"), b"MZ...").unwrap();
        std::fs::write(root.join("movie.mkv"), vec![7u8; 4096]).unwrap();
        std::fs::write(root.join("movie (1).mkv"), vec![7u8; 4096]).unwrap();
        let installed = inst(&["Discord"]);
        let r = analyze(&root, &installed);
        let get = |n: &str| r.items.iter().find(|i| i.name == n).unwrap();
        assert!(get("project.zip").flags.contains(&"extracted".to_string()));
        assert!(get("DiscordSetup.exe").flags.contains(&"installed".to_string()));
        assert_eq!(get("DiscordSetup.exe").category, "installers");
        let dups = r.items.iter().filter(|i| i.flags.contains(&"duplicate".to_string())).count();
        assert_eq!(dups, 1, "exactly one of the two identical videos is a copy");
        assert_eq!(get("project").category, "folders");
        let _ = std::fs::remove_dir_all(&root);
    }
}
