//! Real program icons. A "spec" is what Windows itself stores: `C:\path\app.exe,0`,
//! `C:\path\icon.ico`, `%SystemRoot%\...dll,-123`, or a Store app's PNG logo.
//! Icons are extracted at 64 px (like Explorer), encoded as PNG data URLs and cached.
use crate::util::{self, Res};
use crate::AppState;
use base64::Engine;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

const SIZE: i32 = 64;
const MAX_IMAGE_FILE: u64 = 2 * 1024 * 1024;

/// Splits `"C:\x\app.exe",-101` into (path, index). Expands %VARS%.
pub fn parse_spec(spec: &str) -> Option<(PathBuf, i32)> {
    let s = spec.trim().trim_matches('"').trim();
    if s.is_empty() {
        return None;
    }
    let (path, index) = match s.rfind(',') {
        Some(i) if s[i + 1..].trim().parse::<i32>().is_ok() => (s[..i].trim().trim_matches('"'), s[i + 1..].trim().parse().unwrap_or(0)),
        _ => (s, 0),
    };
    let expanded = expand_env(path);
    let p = PathBuf::from(expanded.trim());
    p.is_absolute().then_some((p, index))
}

/// Expands %VARS% (unknown ones are kept as-is).
pub fn expand_env_pub(s: &str) -> String {
    expand_env(s)
}

fn expand_env(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('%') {
            Some(end) => {
                let var = &after[..end];
                match std::env::var(var) {
                    Ok(v) => out.push_str(&v),
                    Err(_) => {
                        out.push('%');
                        out.push_str(var);
                        out.push('%');
                    }
                }
                rest = &after[end + 1..];
            }
            None => {
                out.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

fn data_url_png(bytes: &[u8]) -> String {
    format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes))
}

fn read_image_file(p: &Path) -> Option<String> {
    let md = std::fs::metadata(p).ok()?;
    if !md.is_file() || md.len() > MAX_IMAGE_FILE {
        return None;
    }
    let bytes = std::fs::read(p).ok()?;
    let mime = match p.extension()?.to_string_lossy().to_lowercase().as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        _ => return None,
    };
    Some(format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)))
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// HICON -> RGBA pixels (handles icons with and without an alpha channel).
unsafe fn hicon_to_rgba(hicon: windows_sys::Win32::UI::WindowsAndMessaging::HICON) -> Option<(Vec<u8>, u32, u32)> {
    use windows_sys::Win32::Graphics::Gdi::*;
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetIconInfo, ICONINFO};
    let mut info: ICONINFO = std::mem::zeroed();
    if GetIconInfo(hicon, &mut info) == 0 {
        return None;
    }
    let cleanup = |info: &ICONINFO| {
        if !info.hbmColor.is_null() {
            DeleteObject(info.hbmColor as _);
        }
        if !info.hbmMask.is_null() {
            DeleteObject(info.hbmMask as _);
        }
    };
    if info.hbmColor.is_null() {
        cleanup(&info);
        return None; // monochrome icon: not worth showing
    }
    let mut bm: BITMAP = std::mem::zeroed();
    if GetObjectW(info.hbmColor as _, std::mem::size_of::<BITMAP>() as i32, &mut bm as *mut _ as *mut _) == 0 {
        cleanup(&info);
        return None;
    }
    let (w, h) = (bm.bmWidth, bm.bmHeight.abs());
    if w <= 0 || h <= 0 || w > 512 || h > 512 {
        cleanup(&info);
        return None;
    }
    let hdc = CreateCompatibleDC(std::ptr::null_mut());
    let read = |hbm: HBITMAP| -> Option<Vec<u8>> {
        let mut bmi: BITMAPINFO = std::mem::zeroed();
        bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        bmi.bmiHeader.biWidth = w;
        bmi.bmiHeader.biHeight = -h; // top-down
        bmi.bmiHeader.biPlanes = 1;
        bmi.bmiHeader.biBitCount = 32;
        bmi.bmiHeader.biCompression = BI_RGB;
        let mut buf = vec![0u8; (w * h * 4) as usize];
        let lines = GetDIBits(hdc, hbm, 0, h as u32, buf.as_mut_ptr() as *mut _, &mut bmi, DIB_RGB_COLORS);
        (lines == h).then_some(buf)
    };
    let color = read(info.hbmColor);
    let mask = if info.hbmMask.is_null() { None } else { read(info.hbmMask) };
    DeleteDC(hdc);
    cleanup(&info);
    let mut px = color?;
    let has_alpha = px.chunks_exact(4).any(|p| p[3] != 0);
    for (i, p) in px.chunks_exact_mut(4).enumerate() {
        p.swap(0, 2); // BGRA -> RGBA
        if !has_alpha {
            // Old-style icon: transparency comes from the AND mask (white = transparent).
            let transparent = mask.as_ref().map(|m| m[i * 4] != 0).unwrap_or(false);
            p[3] = if transparent { 0 } else { 255 };
        }
    }
    if px.chunks_exact(4).all(|p| p[3] == 0) {
        return None;
    }
    Some((px, w as u32, h as u32))
}

fn encode_png(px: &[u8], w: u32, h: u32) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut wr = enc.write_header().ok()?;
        wr.write_image_data(px).ok()?;
    }
    Some(out)
}

fn extract_icon(path: &Path, index: i32) -> Option<String> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{DestroyIcon, PrivateExtractIconsW, HICON};
    let w = wide(&path.to_string_lossy());
    unsafe {
        let mut hicon: HICON = std::ptr::null_mut();
        let mut id = 0u32;
        let n = PrivateExtractIconsW(w.as_ptr(), index, SIZE, SIZE, &mut hicon, &mut id, 1, 0);
        let mut result = None;
        if n != 0 && n != u32::MAX && !hicon.is_null() {
            result = hicon_to_rgba(hicon).and_then(|(px, w, h)| encode_png(&px, w, h)).map(|b| data_url_png(&b));
            DestroyIcon(hicon);
        }
        if result.is_none() {
            result = shell_icon(path);
        }
        result
    }
}

/// Fallback for anything Explorer can show (shortcuts, files with associations): 32 px shell icon.
fn shell_icon(path: &Path) -> Option<String> {
    use windows_sys::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
    use windows_sys::Win32::UI::Shell::{SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON};
    use windows_sys::Win32::UI::WindowsAndMessaging::DestroyIcon;
    if !path.exists() {
        return None;
    }
    unsafe {
        CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32);
        let w = wide(&path.to_string_lossy());
        let mut sfi: SHFILEINFOW = std::mem::zeroed();
        let ok = SHGetFileInfoW(w.as_ptr(), 0, &mut sfi, std::mem::size_of::<SHFILEINFOW>() as u32, SHGFI_ICON | SHGFI_LARGEICON);
        if ok == 0 || sfi.hIcon.is_null() {
            return None;
        }
        let r = hicon_to_rgba(sfi.hIcon).and_then(|(px, w, h)| encode_png(&px, w, h)).map(|b| data_url_png(&b));
        DestroyIcon(sfi.hIcon);
        r
    }
}

pub fn icon_for(spec: &str) -> Option<String> {
    let (path, index) = parse_spec(spec)?;
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "png" | "jpg" | "jpeg" => read_image_file(&path),
        _ => {
            if !path.exists() {
                return None;
            }
            extract_icon(&path, index)
        }
    }
}

#[tauri::command]
pub async fn icons_get(app: AppHandle, specs: Vec<String>) -> Res<HashMap<String, Option<String>>> {
    util::blocking(move || {
        let state = app.state::<AppState>();
        let mut out = HashMap::new();
        for spec in specs.into_iter().take(200) {
            if spec.len() > 1024 {
                continue;
            }
            if let Some(v) = state.icons.lock().get(&spec) {
                out.insert(spec, v.clone());
                continue;
            }
            let v = icon_for(&spec);
            // Only found icons are cached: a missing one may appear later (app still installing).
            if v.is_some() {
                state.icons.lock().insert(spec.clone(), v.clone());
            }
            out.insert(spec, v);
        }
        Ok(out)
    })
    .await
}

/// Best icon source for a command line like `"C:\x\app.exe" --flag` or `C:\x\app.exe -y`.
pub fn exe_from_command(cmd: &str) -> Option<String> {
    let c = cmd.trim();
    let raw = if let Some(rest) = c.strip_prefix('"') {
        rest.split('"').next()?.to_string()
    } else {
        let lower = c.to_ascii_lowercase();
        let end = lower.find(".exe").map(|i| i + 4).unwrap_or(c.len());
        c[..end].trim().to_string()
    };
    let expanded = expand_env(&raw);
    let p = Path::new(&expanded);
    (p.is_absolute()).then(|| expanded.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn specs() {
        let (p, i) = parse_spec(r#""C:\Program Files\App\app.exe",0"#).unwrap();
        assert_eq!(p, PathBuf::from(r"C:\Program Files\App\app.exe"));
        assert_eq!(i, 0);
        let (_, i) = parse_spec(r"C:\Windows\System32\shell32.dll,-16769").unwrap();
        assert_eq!(i, -16769);
        assert!(parse_spec("relative.exe").is_none());
        assert!(parse_spec("").is_none());
        // Comma inside a path is not an index.
        let (p, i) = parse_spec(r"C:\a,b\x.exe").unwrap();
        assert_eq!(p, PathBuf::from(r"C:\a,b\x.exe"));
        assert_eq!(i, 0);
    }

    #[test]
    fn commands() {
        assert_eq!(exe_from_command(r#""C:\A B\x.exe" --run"#).as_deref(), Some(r"C:\A B\x.exe"));
        assert_eq!(exe_from_command(r"C:\Tools\y.exe -silent").as_deref(), Some(r"C:\Tools\y.exe"));
        assert_eq!(exe_from_command("rundll32 something"), None);
    }

    #[test]
    fn extracts_a_real_system_icon() {
        let win = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
        let url = icon_for(&format!(r"{win}\explorer.exe,0")).expect("explorer icon");
        assert!(url.starts_with("data:image/png;base64,"));
        assert!(url.len() > 500);
        assert!(icon_for(r"C:\does\not\exist.exe").is_none());
    }
}
