//! Safe file-system walking and deletion. Never follows junctions/symlinks.
use crate::util::{FILE_ATTRIBUTE_READONLY, FILE_ATTRIBUTE_REPARSE_POINT};
use std::fs;
use std::os::windows::fs::MetadataExt;
use std::path::Path;
use std::time::{Duration, SystemTime};

#[derive(Default, Clone, Copy, Debug)]
pub struct Stats {
    pub size: u64,
    pub files: u64,
}

#[derive(Default, Clone, Copy, Debug)]
pub struct Deleted {
    pub freed: u64,
    pub deleted: u64,
    pub skipped: u64,
}

impl std::ops::AddAssign for Deleted {
    fn add_assign(&mut self, o: Self) {
        self.freed += o.freed;
        self.deleted += o.deleted;
        self.skipped += o.skipped;
    }
}

fn is_reparse(md: &fs::Metadata) -> bool {
    md.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

/// Age = time since the newest of created/modified. Copied or unpacked files keep an old
/// "modified" date but get a fresh "created" date, so a just-unpacked installer counts as new.
fn old_enough(md: &fs::Metadata, min_age: Option<Duration>) -> bool {
    match min_age {
        None => true,
        Some(age) => {
            let newest = [md.modified().ok(), md.created().ok()].into_iter().flatten().max();
            match newest {
                Some(t) => SystemTime::now().duration_since(t).map(|d| d >= age).unwrap_or(false),
                None => false,
            }
        }
    }
}

/// Size and count of files inside `root` (recursive, not following links).
pub fn stats(root: &Path, min_age: Option<Duration>) -> Stats {
    let mut s = Stats::default();
    if is_reparse_path(root) {
        return s; // a redirected root is never cleaned, so it shows 0
    }
    stats_inner(root, min_age, &mut s, 0);
    s
}

fn stats_inner(dir: &Path, min_age: Option<Duration>, s: &mut Stats, depth: u32) {
    if depth > 64 {
        return;
    }
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let Ok(md) = e.metadata() else { continue };
        if is_reparse(&md) {
            continue;
        }
        if md.is_dir() {
            stats_inner(&e.path(), min_age, s, depth + 1);
        } else if old_enough(&md, min_age) {
            s.size += md.len();
            s.files += 1;
        }
    }
}

/// Deletes everything inside `root` (but not `root` itself). Locked files are skipped.
/// Links inside are removed as links, never followed.
pub fn delete_contents(root: &Path, min_age: Option<Duration>) -> Deleted {
    let mut d = Deleted::default();
    // The root itself must be a real folder, not a junction/symlink planted to redirect us.
    if is_unsafe_clean_root(root) || is_reparse_path(root) || !root.is_dir() {
        return d;
    }
    let Ok(canon_root) = fs::canonicalize(root) else { return d };
    delete_inner(root, &canon_root, min_age, &mut d, 0);
    d
}

/// True if the path itself (not its target) is a junction/symlink, or can't be inspected.
pub fn is_reparse_path(p: &Path) -> bool {
    match fs::symlink_metadata(p) {
        Ok(md) => is_reparse(&md),
        Err(_) => true,
    }
}

/// Re-checks a folder right before entering it: still not a link, and its real location is
/// still inside the cleanup root. This closes the "swap a folder for a junction" trick.
fn still_inside(path: &Path, canon_root: &Path) -> bool {
    if is_reparse_path(path) {
        return false;
    }
    match fs::canonicalize(path) {
        Ok(c) => c.starts_with(canon_root),
        Err(_) => false,
    }
}

fn delete_inner(dir: &Path, canon_root: &Path, min_age: Option<Duration>, d: &mut Deleted, depth: u32) {
    if depth > 64 || !still_inside(dir, canon_root) {
        return;
    }
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        // "Stop" pressed in Cleanup: leave the rest as it is.
        if crate::ops::cancelled(crate::ops::Op::Cleanup) {
            return;
        }
        let path = e.path();
        let Ok(md) = e.metadata() else {
            d.skipped += 1;
            continue;
        };
        if is_reparse(&md) {
            // Remove the link itself, not its target.
            let r = if md.is_dir() { fs::remove_dir(&path) } else { fs::remove_file(&path) };
            if r.is_err() {
                d.skipped += 1;
            }
            continue;
        }
        if md.is_dir() {
            delete_inner(&path, canon_root, min_age, d, depth + 1);
            // Only succeeds when empty; remove_dir never follows links. Fresh folders are kept:
            // a service or installer may have just created them.
            if old_enough(&md, min_age) {
                let _ = fs::remove_dir(&path);
            }
        } else if old_enough(&md, min_age) {
            // The parent folder must still be inside the root at the moment of deletion.
            if !still_inside(dir, canon_root) {
                d.skipped += 1;
                continue;
            }
            let len = md.len();
            match remove_file_force(&path, &md) {
                Ok(()) => {
                    d.freed += len;
                    d.deleted += 1;
                }
                Err(_) => d.skipped += 1,
            }
        }
    }
}

/// Cleanup roots are specific cache folders (Temp, Windows\Temp, browser Cache...).
/// Refuse anything broad: drive roots, the Windows folder itself, System32, profile roots.
pub fn is_unsafe_clean_root(root: &Path) -> bool {
    use crate::util::{env_path, system_root};
    if !root.is_absolute() {
        return true;
    }
    let n = |p: &Path| p.to_string_lossy().trim_end_matches('\\').to_lowercase();
    let r = n(root);
    // Must be at least 3 levels deep, e.g. C:\Windows\Temp.
    if r.len() <= 3 || r.matches('\\').count() < 2 {
        return true;
    }
    let win = system_root();
    let mut bad = vec![n(&win), n(&win.join("System32")), n(&win.join("SysWOW64")), n(&win.join("WinSxS"))];
    for var in ["USERPROFILE", "APPDATA", "LOCALAPPDATA", "ProgramData", "ProgramFiles", "ProgramFiles(x86)"] {
        if let Some(p) = env_path(var) {
            bad.push(n(&p));
        }
    }
    if let Some(home) = env_path("USERPROFILE") {
        for sub in ["Desktop", "Documents", "Downloads", "Pictures", "Music", "Videos", "OneDrive"] {
            bad.push(n(&home.join(sub)));
        }
    }
    // Also refuse any parent of a protected folder (e.g. TEMP set to %USERPROFILE%\AppData).
    bad.iter().any(|b| *b == r || b.starts_with(&(r.clone() + "\\")))
}

fn remove_file_force(path: &Path, md: &fs::Metadata) -> std::io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) => {
            if md.file_attributes() & FILE_ATTRIBUTE_READONLY != 0 {
                let mut perms = md.permissions();
                #[allow(clippy::permissions_set_readonly_false)]
                perms.set_readonly(false);
                fs::set_permissions(path, perms)?;
                fs::remove_file(path)
            } else {
                Err(e)
            }
        }
    }
}

/// Files in `dir` (non-recursive) whose lowercase name starts with `prefix` and ends with `suffix`.
pub fn matching_files(dir: &Path, prefix: &str, suffix: &str) -> Vec<(std::path::PathBuf, u64)> {
    let Ok(rd) = fs::read_dir(dir) else { return vec![] };
    rd.flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_lowercase();
            let md = e.metadata().ok()?;
            (md.is_file() && name.starts_with(prefix) && name.ends_with(suffix)).then(|| (e.path(), md.len()))
        })
        .collect()
}

/// Total size of a folder (used for program sizes). Stops early at `limit_files`.
pub fn folder_size(root: &Path, limit_files: u64) -> u64 {
    fn inner(dir: &Path, acc: &mut (u64, u64), limit: u64, depth: u32) {
        if depth > 32 || acc.1 >= limit {
            return;
        }
        let Ok(rd) = fs::read_dir(dir) else { return };
        for e in rd.flatten() {
            let Ok(md) = e.metadata() else { continue };
            if is_reparse(&md) {
                continue;
            }
            if md.is_dir() {
                inner(&e.path(), acc, limit, depth + 1);
            } else {
                acc.0 += md.len();
                acc.1 += 1;
            }
            if acc.1 >= limit {
                return;
            }
        }
    }
    let mut acc = (0u64, 0u64);
    inner(root, &mut acc, limit_files, 0);
    acc.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delete_contents_keeps_root_and_counts() {
        let root = std::env::temp_dir().join(format!("sm-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("a/b")).unwrap();
        fs::write(root.join("x.txt"), b"hello").unwrap();
        fs::write(root.join("a/b/y.bin"), vec![0u8; 1000]).unwrap();
        let st = stats(&root, None);
        assert_eq!(st.files, 2);
        assert_eq!(st.size, 1005);
        let d = delete_contents(&root, None);
        assert_eq!(d.deleted, 2);
        assert_eq!(d.freed, 1005);
        assert!(root.exists());
        assert_eq!(fs::read_dir(&root).unwrap().count(), 0);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn min_age_skips_fresh_files() {
        let root = std::env::temp_dir().join(format!("sm-test-age-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("fresh.txt"), b"new").unwrap();
        let st = stats(&root, Some(Duration::from_secs(3600)));
        assert_eq!(st.files, 0);
        let d = delete_contents(&root, Some(Duration::from_secs(3600)));
        assert_eq!(d.deleted, 0);
        assert!(root.join("fresh.txt").exists());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn junction_root_and_child_are_not_followed() {
        let base = std::env::temp_dir().join(format!("sm-test-jn-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let victim = base.join("victim");
        let root = base.join("root");
        fs::create_dir_all(&victim).unwrap();
        fs::create_dir_all(&root).unwrap();
        fs::write(victim.join("keep.txt"), b"important").unwrap();
        // Junction inside the root pointing at the victim (mklink /J needs no admin).
        let link = root.join("link");
        let ok = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J", &link.to_string_lossy(), &victim.to_string_lossy()])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if ok {
            let d = delete_contents(&root, None);
            assert!(victim.join("keep.txt").exists(), "must not follow a junction inside the root");
            assert_eq!(d.deleted, 0);
            // Junction as the root itself.
            let link_root = base.join("link-root");
            let _ = std::process::Command::new("cmd")
                .args(["/C", "mklink", "/J", &link_root.to_string_lossy(), &victim.to_string_lossy()])
                .output();
            let d = delete_contents(&link_root, None);
            assert_eq!(d.deleted, 0);
            assert!(victim.join("keep.txt").exists(), "must not clean through a junction root");
        }
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn refuses_protected_roots() {
        let d = delete_contents(Path::new(r"C:\Windows"), None);
        assert_eq!(d.deleted, 0);
        let d = delete_contents(Path::new(r"C:\"), None);
        assert_eq!(d.deleted, 0);
    }
}
