//! Monitors: current refresh rate, the rates the monitor supports at its current resolution,
//! and changing it (reversible through the Journal).
use crate::util::{tr, Res};
use serde::Serialize;
use windows_sys::Win32::Graphics::Gdi::{
    ChangeDisplaySettingsExW, EnumDisplayDevicesW, EnumDisplaySettingsW, CDS_TEST, CDS_UPDATEREGISTRY, DEVMODEW,
    DISPLAY_DEVICEW, DISPLAY_DEVICE_ATTACHED_TO_DESKTOP, DISPLAY_DEVICE_PRIMARY_DEVICE, DISP_CHANGE_SUCCESSFUL,
    DM_DISPLAYFREQUENCY, ENUM_CURRENT_SETTINGS,
};

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DisplayInfo {
    /// GDI device name, e.g. \\.\DISPLAY1 (what Windows uses to change the mode).
    pub device: String,
    /// Monitor name as Windows knows it.
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub current: u32,
    /// Supported refresh rates at the current resolution, ascending.
    pub rates: Vec<u32>,
    pub primary: bool,
}

impl DisplayInfo {
    pub fn max_rate(&self) -> u32 {
        self.rates.iter().copied().max().unwrap_or(self.current)
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn from_wide(w: &[u16]) -> String {
    let end = w.iter().position(|&c| c == 0).unwrap_or(w.len());
    String::from_utf16_lossy(&w[..end]).trim().to_string()
}

fn empty_mode() -> DEVMODEW {
    let mut m: DEVMODEW = unsafe { std::mem::zeroed() };
    m.dmSize = std::mem::size_of::<DEVMODEW>() as u16;
    m
}

fn current_mode(device_w: &[u16]) -> Option<DEVMODEW> {
    let mut m = empty_mode();
    (unsafe { EnumDisplaySettingsW(device_w.as_ptr(), ENUM_CURRENT_SETTINGS, &mut m) } != 0).then_some(m)
}

/// Rates 0 and 1 mean "hardware default", not a real rate.
pub fn clean_rates(mut rates: Vec<u32>) -> Vec<u32> {
    rates.retain(|r| *r > 1 && *r <= 1000);
    rates.sort_unstable();
    rates.dedup();
    rates
}

/// All monitors attached to the desktop. Empty if Windows reports none (e.g. remote session quirks).
pub fn list() -> Vec<DisplayInfo> {
    let mut out = Vec::new();
    for i in 0..16u32 {
        let mut dd: DISPLAY_DEVICEW = unsafe { std::mem::zeroed() };
        dd.cb = std::mem::size_of::<DISPLAY_DEVICEW>() as u32;
        if unsafe { EnumDisplayDevicesW(std::ptr::null(), i, &mut dd, 0) } == 0 {
            break;
        }
        if dd.StateFlags & DISPLAY_DEVICE_ATTACHED_TO_DESKTOP == 0 {
            continue;
        }
        let device = from_wide(&dd.DeviceName);
        let device_w = wide(&device);
        let Some(cur) = current_mode(&device_w) else { continue };
        // The monitor itself is the first child of the adapter output.
        let mut mon: DISPLAY_DEVICEW = unsafe { std::mem::zeroed() };
        mon.cb = std::mem::size_of::<DISPLAY_DEVICEW>() as u32;
        let name = if unsafe { EnumDisplayDevicesW(device_w.as_ptr(), 0, &mut mon, 0) } != 0 {
            from_wide(&mon.DeviceString)
        } else {
            from_wide(&dd.DeviceString)
        };
        let mut rates = Vec::new();
        let mut n = 0u32;
        loop {
            let mut m = empty_mode();
            if unsafe { EnumDisplaySettingsW(device_w.as_ptr(), n, &mut m) } == 0 {
                break;
            }
            n += 1;
            if n > 5000 {
                break;
            }
            // Same resolution and colour depth as now; interlaced modes (flag 2) are skipped.
            let interlaced = unsafe { m.Anonymous2.dmDisplayFlags } & 2 != 0;
            if m.dmPelsWidth == cur.dmPelsWidth && m.dmPelsHeight == cur.dmPelsHeight && m.dmBitsPerPel == cur.dmBitsPerPel && !interlaced {
                rates.push(m.dmDisplayFrequency);
            }
        }
        rates.push(cur.dmDisplayFrequency);
        out.push(DisplayInfo {
            device,
            name: if name.is_empty() { tr("Монитор", "Monitor") } else { name },
            width: cur.dmPelsWidth,
            height: cur.dmPelsHeight,
            current: cur.dmDisplayFrequency,
            rates: clean_rates(rates),
            primary: dd.StateFlags & DISPLAY_DEVICE_PRIMARY_DEVICE != 0,
        });
    }
    out.sort_by_key(|d| !d.primary);
    out
}

fn valid_device(device: &str) -> bool {
    let rest = device.strip_prefix(r"\\.\DISPLAY").unwrap_or("");
    !rest.is_empty() && rest.len() <= 3 && rest.chars().all(|c| c.is_ascii_digit())
}

/// Sets the refresh rate of one monitor, keeping its resolution. Only rates the monitor itself
/// reports are accepted; Windows tests the mode before applying it. Returns the previous rate.
pub fn set_rate(device: &str, hz: u32) -> Res<u32> {
    if !valid_device(device) {
        return Err(tr("Монитор не найден", "Monitor not found"));
    }
    let info = list().into_iter().find(|d| d.device.eq_ignore_ascii_case(device)).ok_or_else(|| tr("Монитор не найден", "Monitor not found"))?;
    if !info.rates.contains(&hz) {
        return Err(tr("Монитор не поддерживает эту частоту", "The monitor doesn't support this refresh rate"));
    }
    let prev = info.current;
    if prev == hz {
        return Ok(prev);
    }
    let device_w = wide(device);
    let mut m = current_mode(&device_w).ok_or_else(|| tr("Монитор не найден", "Monitor not found"))?;
    m.dmDisplayFrequency = hz;
    m.dmFields = DM_DISPLAYFREQUENCY;
    let test = unsafe { ChangeDisplaySettingsExW(device_w.as_ptr(), &m, std::ptr::null_mut(), CDS_TEST, std::ptr::null()) };
    if test != DISP_CHANGE_SUCCESSFUL {
        return Err(tr("Windows не может включить эту частоту", "Windows can't switch to this refresh rate"));
    }
    let res = unsafe { ChangeDisplaySettingsExW(device_w.as_ptr(), &m, std::ptr::null_mut(), CDS_UPDATEREGISTRY, std::ptr::null()) };
    if res != DISP_CHANGE_SUCCESSFUL {
        return Err(format!("{} ({res})", tr("Не удалось сменить частоту", "Couldn't change the refresh rate")));
    }
    Ok(prev)
}

#[tauri::command]
pub async fn display_list() -> Res<Vec<DisplayInfo>> {
    crate::util::blocking(|| Ok(list())).await
}

#[tauri::command]
pub async fn display_set_rate(app: tauri::AppHandle, device: String, hz: u32) -> Res<()> {
    crate::util::blocking(move || {
        let name = list().into_iter().find(|d| d.device.eq_ignore_ascii_case(&device)).map(|d| d.name).unwrap_or_else(|| device.clone());
        let prev = set_rate(&device, hz)?;
        if prev != hz {
            crate::journal::add(&app, "refresh_rate", &name, &hz.to_string(), Some(crate::journal::Undo::Refresh { device: device.clone(), hz: prev }));
        }
        Ok(())
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rates_cleaned() {
        assert_eq!(clean_rates(vec![75, 60, 1, 0, 60, 50]), vec![50, 60, 75]);
    }
    #[test]
    fn device_names() {
        assert!(valid_device(r"\\.\DISPLAY1"));
        assert!(valid_device(r"\\.\DISPLAY12"));
        assert!(!valid_device(r"\\.\DISPLAY"));
        assert!(!valid_device(r"\\.\DISPLAY1\Monitor0"));
        assert!(!valid_device("C:\\x"));
    }
}
