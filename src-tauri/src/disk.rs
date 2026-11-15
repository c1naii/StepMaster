//! Disk analyzer (parallel scan, folder drill-down, largest files), duplicates, recycle bin moves.
use crate::util::{self, is_protected_path, tr, Res, FILE_ATTRIBUTE_REPARSE_POINT};
use crate::AppState;
use parking_lot::Mutex;
use rayon::prelude::*;
use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};

const TOP_FILES_PER_DIR: usize = 60;
const LARGE_FILE_MIN: u64 = 50 * 1024 * 1024;
const LARGE_FILES_KEEP: usize = 100;

#[derive(Clone, Default)]
pub struct Node {
    size: u64,
    files: u64,
    subdirs: Vec<PathBuf>,
    top_files: Vec<(String, u64)>,
}

#[derive(Default)]
pub struct ScanCache {
    root: Option<PathBuf>,
    nodes: HashMap<PathBuf, Node>,
    large: Vec<(PathBuf, u64)>,
}

pub struct DiskState {
    pub cache: Mutex<ScanCache>,
    pub cancel: AtomicBool,
    pub running: AtomicBool,
}

impl Default for DiskState {
    fn default() -> Self {
        Self { cache: Mutex::new(ScanCache::default()), cancel: AtomicBool::new(false), running: AtomicBool::new(false) }
    }
}

struct Ctx {
    nodes: Mutex<HashMap<PathBuf, Node>>,
    large: Mutex<Vec<(PathBuf, u64)>>,
    cancel: Arc<AtomicBool>,
    files: AtomicU64,
    bytes: AtomicU64,
    last_emit: Mutex<Instant>,
    app: AppHandle,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Progress {
    files: u64,
    bytes: u64,
    current: String,
}

/// Files that really occupy disk space. Symlinked files and cloud-only placeholders
/// (OneDrive "online-only") are skipped; fully downloaded OneDrive and compressed (WOF) files count.
pub fn counts_on_disk(md: &fs::Metadata) -> bool {
    const OFFLINE: u32 = 0x1000;
    const RECALL_ON_OPEN: u32 = 0x40000;
    const RECALL_ON_DATA_ACCESS: u32 = 0x400000;
    let a = md.file_attributes();
    if md.file_type().is_symlink() {
        return false;
    }
    a & (OFFLINE | RECALL_ON_OPEN | RECALL_ON_DATA_ACCESS) == 0
}

fn walk(dir: &Path, ctx: &Ctx) -> (u64, u64) {
    if ctx.cancel.load(Ordering::Relaxed) {
        return (0, 0);
    }
    let mut own_size = 0u64;
    let mut own_files = 0u64;
    let mut subdirs = Vec::new();
    let mut top: Vec<(String, u64)> = Vec::new();
    if let Ok(rd) = fs::read_dir(dir) {
        for e in rd.flatten() {
            let Ok(md) = e.metadata() else { continue };
            if md.is_dir() {
                // Linked folders (junctions, symlinks) are never followed.
                if md.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0 {
                    subdirs.push(e.path());
                }
            } else if counts_on_disk(&md) {
                let len = md.len();
                own_size += len;
                own_files += 1;
                top.push((e.file_name().to_string_lossy().to_string(), len));
                if len >= LARGE_FILE_MIN {
                    ctx.large.lock().push((e.path(), len));
                }
            }
        }
    }
    let total_files = ctx.files.fetch_add(own_files, Ordering::Relaxed) + own_files;
    let total_bytes = ctx.bytes.fetch_add(own_size, Ordering::Relaxed) + own_size;
    {
        let mut last = ctx.last_emit.lock();
        if last.elapsed() > Duration::from_millis(150) {
            *last = Instant::now();
            let _ = ctx.app.emit(
                "disk://progress",
                Progress { files: total_files, bytes: total_bytes, current: dir.to_string_lossy().to_string() },
            );
        }
    }
    let child: Vec<(u64, u64)> = subdirs.par_iter().map(|d| walk(d, ctx)).collect();
    let size = own_size + child.iter().map(|c| c.0).sum::<u64>();
    let files = own_files + child.iter().map(|c| c.1).sum::<u64>();
    top.sort_unstable_by(|a, b| b.1.cmp(&a.1));
    top.truncate(TOP_FILES_PER_DIR);
    ctx.nodes.lock().insert(dir.to_path_buf(), Node { size, files, subdirs, top_files: top });
    (size, files)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    name: String,
    path: String,
    size: u64,
    is_dir: bool,
    files: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Folder {
    path: String,
    size: u64,
    files: u64,
    entries: Vec<Entry>,
}

fn find_node<'a>(cache: &'a ScanCache, path: &Path) -> Option<(&'a PathBuf, &'a Node)> {
    if let Some((k, v)) = cache.nodes.get_key_value(path) {
        return Some((k, v));
    }
    let want = path.to_string_lossy().trim_end_matches('\\').to_lowercase();
    cache
        .nodes
        .iter()
        .find(|(k, _)| k.to_string_lossy().trim_end_matches('\\').to_lowercase() == want)
}

fn folder_view(cache: &ScanCache, path: &Path) -> Res<Folder> {
    let (key, node) = find_node(cache, path).ok_or_else(|| {
        tr("Эта папка не входит в последний анализ — запустите его заново", "This folder isn't in the last scan — run it again")
    })?;
    let mut entries: Vec<Entry> = node
        .subdirs
        .iter()
        .filter_map(|d| {
            let n = cache.nodes.get(d)?;
            Some(Entry {
                name: d.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(),
                path: d.to_string_lossy().to_string(),
                size: n.size,
                is_dir: true,
                files: n.files,
            })
        })
        .collect();
    for (name, size) in &node.top_files {
        entries.push(Entry {
            name: name.clone(),
            path: key.join(name).to_string_lossy().to_string(),
            size: *size,
            is_dir: false,
            files: 1,
        });
    }
    entries.sort_unstable_by(|a, b| b.size.cmp(&a.size));
    entries.truncate(300);
    Ok(Folder { path: key.to_string_lossy().to_string(), size: node.size, files: node.files, entries })
}

#[tauri::command]
pub async fn disk_scan(app: AppHandle, path: String) -> Res<Folder> {
    let root = PathBuf::from(&path);
    if !root.is_absolute() || !root.is_dir() {
        return Err(tr("Папка не найдена", "Folder not found"));
    }
    let state = app.state::<AppState>();
    if state.disk.running.swap(true, Ordering::SeqCst) {
        return Err(tr("Анализ уже идёт", "A scan is already running"));
    }
    state.disk.cancel.store(false, Ordering::SeqCst);
    let cancel = Arc::new(AtomicBool::new(false));
    // Bridge the shared cancel flag into this scan.
    let cancel_watch = cancel.clone();
    let app2 = app.clone();
    std::thread::spawn(move || loop {
        let st = app2.state::<AppState>();
        if !st.disk.running.load(Ordering::SeqCst) {
            break;
        }
        if st.disk.cancel.load(Ordering::SeqCst) {
            cancel_watch.store(true, Ordering::SeqCst);
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    });

    let app3 = app.clone();
    let root2 = root.clone();
    let result = util::blocking(move || {
        let ctx = Ctx {
            nodes: Mutex::new(HashMap::new()),
            large: Mutex::new(Vec::new()),
            cancel,
            files: AtomicU64::new(0),
            bytes: AtomicU64::new(0),
            last_emit: Mutex::new(Instant::now()),
            app: app3.clone(),
        };
        walk(&root2, &ctx);
        if ctx.cancel.load(Ordering::SeqCst) {
            return Err(tr("Анализ остановлен", "Scan stopped"));
        }
        let mut large = ctx.large.into_inner();
        large.sort_unstable_by(|a, b| b.1.cmp(&a.1));
        large.truncate(LARGE_FILES_KEEP);
        let st = app3.state::<AppState>();
        let mut cache = st.disk.cache.lock();
        cache.root = Some(root2.clone());
        cache.nodes = ctx.nodes.into_inner();
        cache.large = large;
        folder_view(&cache, &root2)
    })
    .await;
    state.disk.running.store(false, Ordering::SeqCst);
    result
}

#[tauri::command]
pub async fn disk_children(app: AppHandle, path: String) -> Res<Folder> {
    util::blocking(move || {
        let state = app.state::<AppState>();
        let cache = state.disk.cache.lock();
        folder_view(&cache, Path::new(&path))
    })
    .await
}

#[tauri::command]
pub async fn disk_large_files(app: AppHandle) -> Res<Vec<Entry>> {
    util::blocking(move || {
        let state = app.state::<AppState>();
        let list: Vec<(PathBuf, u64)> = state.disk.cache.lock().large.clone();
        Ok(list
            .into_iter()
            .filter(|(p, _)| p.exists())
            .map(|(p, s)| Entry {
                name: p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
                path: p.to_string_lossy().to_string(),
                size: s,
                is_dir: false,
                files: 1,
            })
            .collect())
    })
    .await
}

#[tauri::command]
pub fn disk_cancel(state: State<'_, AppState>) {
    state.disk.cancel.store(true, Ordering::SeqCst);
}

/// Removes a recycled path from the scan cache and subtracts its size from all parents.
fn forget_path(cache: &mut ScanCache, path: &Path, file_size: Option<u64>) {
    let (size, files) = if let Some(n) = cache.nodes.get(path) {
        (n.size, n.files)
    } else if let Some(s) = file_size {
        (s, 1)
    } else {
        let parent = path.parent().and_then(|p| cache.nodes.get(p));
        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        match parent.and_then(|n| n.top_files.iter().find(|(f, _)| *f == name)) {
            Some((_, s)) => (*s, 1),
            None => (0, 0),
        }
    };
    // Drop the subtree.
    let prefix = path.to_path_buf();
    cache.nodes.retain(|k, _| !k.starts_with(&prefix));
    cache.large.retain(|(p, _)| !p.starts_with(&prefix));
    if let Some(parent) = path.parent() {
        if let Some(n) = cache.nodes.get_mut(parent) {
            n.subdirs.retain(|d| d != path);
            let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            n.top_files.retain(|(f, _)| *f != name);
        }
    }
    let mut cur = path.parent();
    while let Some(p) = cur {
        if let Some(n) = cache.nodes.get_mut(p) {
            n.size = n.size.saturating_sub(size);
            n.files = n.files.saturating_sub(files);
        }
        cur = p.parent();
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecycleResult {
    ok: u32,
    failed: Vec<String>,
}

/// `\\?\C:\x` -> `C:\x` (canonicalize returns verbatim paths on Windows).
fn strip_verbatim(p: &Path) -> PathBuf {
    let s = p.to_string_lossy();
    match s.strip_prefix(r"\\?\") {
        Some(rest) if rest.len() >= 2 && rest.as_bytes()[1] == b':' => PathBuf::from(rest),
        _ => p.to_path_buf(),
    }
}

/// Moves files/folders to the Recycle Bin (recoverable). Protected system paths are refused.
pub fn recycle_paths(paths: &[String]) -> (Vec<PathBuf>, Vec<String>) {
    let mut done = Vec::new();
    let mut failed = Vec::new();
    for p in paths {
        let raw = PathBuf::from(p);
        if !raw.exists() && fs::symlink_metadata(&raw).is_err() {
            continue;
        }
        // Resolve "..", 8.3 names and links in parent folders BEFORE checking, and recycle
        // exactly the checked path.
        let parent_real = raw.parent().and_then(|par| fs::canonicalize(par).ok());
        let path = match (parent_real, raw.file_name()) {
            (Some(par), Some(name)) => strip_verbatim(&par.join(name)),
            // No clear file name (e.g. ends with ".."): refuse rather than guess.
            _ => {
                failed.push(format!("{p} — {}", tr("неверный путь", "invalid path")));
                continue;
            }
        };
        // The full path resolved (8.3 short names like C:\PROGRA~1 included) must be allowed too.
        let full_real = fs::canonicalize(&raw).ok().map(|c| strip_verbatim(&c));
        if is_protected_path(&path) || is_protected_path(&raw) || full_real.as_deref().map(is_protected_path).unwrap_or(false) {
            failed.push(format!("{p} — {}", tr("системный путь, удалять нельзя", "system path, not allowed")));
            continue;
        }
        match trash::delete(&path) {
            Ok(()) => done.push(raw),
            Err(e) => failed.push(format!("{p} — {e}")),
        }
    }
    (done, failed)
}

#[tauri::command]
pub async fn fs_recycle(app: AppHandle, paths: Vec<String>) -> Res<RecycleResult> {
    // File sizes are taken before deletion, so the analyzer can update folder totals.
    let (done, failed, sizes) = util::blocking(move || {
        let sizes: HashMap<PathBuf, u64> = paths
            .iter()
            .filter_map(|p| fs::symlink_metadata(p).ok().filter(|m| m.is_file()).map(|m| (PathBuf::from(p), m.len())))
            .collect();
        let (done, failed) = recycle_paths(&paths);
        Ok((done, failed, sizes))
    })
    .await?;
    let state = app.state::<AppState>();
    let mut cache = state.disk.cache.lock();
    for p in &done {
        forget_path(&mut cache, p, sizes.get(p).copied());
    }
    Ok(RecycleResult { ok: done.len() as u32, failed })
}

/* ---------------- Duplicates ---------------- */

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DupGroup {
    size: u64,
    files: Vec<String>,
}

const DUP_MIN: u64 = 1024 * 1024;
const DUP_MAX_FILES: usize = 300_000;

fn collect_files(dir: &Path, out: &mut Vec<(PathBuf, u64)>, depth: u32) {
    if depth > 40 || out.len() >= DUP_MAX_FILES {
        return;
    }
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let Ok(md) = e.metadata() else { continue };
        if md.is_dir() {
            if md.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0 {
                collect_files(&e.path(), out, depth + 1);
            }
        } else if md.len() >= DUP_MIN && counts_on_disk(&md) {
            out.push((e.path(), md.len()));
        }
    }
}

fn hash_file(path: &Path, limit: Option<u64>) -> Option<[u8; 32]> {
    use std::io::Read;
    let f = fs::File::open(path).ok()?;
    let mut hasher = blake3::Hasher::new();
    let mut reader: Box<dyn Read> = match limit {
        Some(l) => Box::new(f.take(l)),
        None => Box::new(f),
    };
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        let n = reader.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Some(*hasher.finalize().as_bytes())
}

#[tauri::command]
pub async fn duplicates_scan(path: String) -> Res<Vec<DupGroup>> {
    let root = PathBuf::from(&path);
    if !root.is_absolute() || !root.is_dir() {
        return Err(tr("Папка не найдена", "Folder not found"));
    }
    util::blocking(move || {
        let mut files = Vec::new();
        collect_files(&root, &mut files, 0);
        // 1) group by size
        let mut by_size: HashMap<u64, Vec<PathBuf>> = HashMap::new();
        for (p, s) in files {
            by_size.entry(s).or_default().push(p);
        }
        let candidates: Vec<(u64, Vec<PathBuf>)> = by_size.into_iter().filter(|(_, v)| v.len() > 1).collect();
        // 2) quick hash of the first 64 KB, then 3) full hash.
        let groups: Vec<DupGroup> = candidates
            .into_par_iter()
            .flat_map(|(size, paths)| {
                let mut quick: HashMap<[u8; 32], Vec<PathBuf>> = HashMap::new();
                for p in paths {
                    if let Some(h) = hash_file(&p, Some(64 * 1024)) {
                        quick.entry(h).or_default().push(p);
                    }
                }
                let mut out = Vec::new();
                for (_, same) in quick.into_iter().filter(|(_, v)| v.len() > 1) {
                    let mut full: HashMap<[u8; 32], Vec<PathBuf>> = HashMap::new();
                    for p in same {
                        if let Some(h) = if size <= 64 * 1024 { hash_file(&p, Some(64 * 1024)) } else { hash_file(&p, None) } {
                            full.entry(h).or_default().push(p);
                        }
                    }
                    for (_, mut v) in full.into_iter().filter(|(_, v)| v.len() > 1) {
                        v.sort();
                        out.push(DupGroup { size, files: v.iter().map(|p| p.to_string_lossy().to_string()).collect() });
                    }
                }
                out
            })
            .collect();
        let mut groups = groups;
        groups.sort_unstable_by(|a, b| (b.size * (b.files.len() as u64 - 1)).cmp(&(a.size * (a.files.len() as u64 - 1))));
        groups.truncate(300);
        Ok(groups)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cache_with(paths: &[(&str, u64, u64, &[&str], &[(&str, u64)])]) -> ScanCache {
        let mut c = ScanCache::default();
        for (p, size, files, subs, top) in paths {
            c.nodes.insert(
                PathBuf::from(p),
                Node {
                    size: *size,
                    files: *files,
                    subdirs: subs.iter().map(PathBuf::from).collect(),
                    top_files: top.iter().map(|(n, s)| (n.to_string(), *s)).collect(),
                },
            );
        }
        c
    }

    #[test]
    fn forget_subtracts_from_parents() {
        let mut c = cache_with(&[
            (r"X:\", 1000, 10, &[r"X:\a"], &[("f.bin", 100)]),
            (r"X:\a", 900, 9, &[r"X:\a\b"], &[]),
            (r"X:\a\b", 900, 9, &[], &[("big.iso", 900)]),
        ]);
        forget_path(&mut c, Path::new(r"X:\a\b\big.iso"), None);
        assert_eq!(c.nodes[Path::new(r"X:\a\b")].size, 0);
        assert_eq!(c.nodes[Path::new(r"X:\a")].size, 0);
        assert_eq!(c.nodes[Path::new(r"X:\")].size, 100);
        forget_path(&mut c, Path::new(r"X:\a"), None);
        assert!(!c.nodes.contains_key(Path::new(r"X:\a")));
        assert!(!c.nodes.contains_key(Path::new(r"X:\a\b")));
        assert!(c.nodes[Path::new(r"X:\")].subdirs.is_empty());
    }

    #[test]
    fn folder_view_sorts_and_mixes() {
        let c = cache_with(&[
            (r"X:\", 1000, 10, &[r"X:\a"], &[("f.bin", 600)]),
            (r"X:\a", 400, 9, &[], &[]),
        ]);
        let v = folder_view(&c, Path::new(r"X:\")).unwrap();
        assert_eq!(v.entries.len(), 2);
        assert_eq!(v.entries[0].name, "f.bin");
        assert!(v.entries[1].is_dir);
        assert!(folder_view(&c, Path::new(r"X:\missing")).is_err());
    }

    #[test]
    fn recycle_refuses_system_paths() {
        let (done, failed) = recycle_paths(&[r"C:\Windows".into(), r"C:\".into()]);
        assert!(done.is_empty());
        assert_eq!(failed.len(), 2);
        // ".." tricks are resolved before the check.
        if let Some(home) = crate::util::env_path("USERPROFILE") {
            let sneaky = home.join(r"..\..\Program Files").to_string_lossy().to_string();
            let (done, failed) = recycle_paths(&[sneaky]);
            assert!(done.is_empty());
            assert_eq!(failed.len(), 1);
        }
    }
}
