//! LCD stripe order of the monitor a window is on (ClearType per-monitor setting, then the system one).

use azul_layout::glyph_cache::LcdSubpixelOrder;

use super::dlopen::{User32Functions, Win32Libraries, HWND, MONITORINFOEXW, MONITOR_DEFAULTTONEAREST, RECT};

const SPI_GETFONTSMOOTHINGORIENTATION: u32 = 0x2012;
const FE_FONTSMOOTHINGORIENTATIONBGR: u32 = 0;
const HKEY_CURRENT_USER: isize = 0x8000_0001_u32 as i32 as isize;
const RRF_RT_REG_DWORD: u32 = 0x0000_0010;

/// The monitor `hwnd` is (mostly) on, as an opaque identity for change
/// detection. Cheap enough to call on every move.
pub(super) fn monitor_of(user32: &User32Functions, hwnd: HWND) -> isize {
    unsafe { (user32.MonitorFromWindow)(hwnd, MONITOR_DEFAULTTONEAREST) as isize }
}

/// Flat or unreported panels count as RGB.
pub(super) fn subpixel_order_of(win32: &Win32Libraries, monitor: isize) -> LcdSubpixelOrder {
    per_monitor_pixel_structure(win32, monitor)
        .or_else(|| system_orientation(&win32.user32))
        .unwrap_or(LcdSubpixelOrder::Rgb)
}

/// `Avalon.Graphics\<DISPLAYn>\PixelStructure` for this monitor, if the tuner
/// ever recorded one.
fn per_monitor_pixel_structure(win32: &Win32Libraries, monitor: isize) -> Option<LcdSubpixelOrder> {
    let advapi32 = win32.advapi32?;
    if monitor == 0 {
        return None;
    }
    // The registry key is the device name without the `\\.\` prefix.
    let mut info = MONITORINFOEXW {
        cbSize: core::mem::size_of::<MONITORINFOEXW>() as u32,
        rcMonitor: RECT::default(),
        rcWork: RECT::default(),
        dwFlags: 0,
        szDevice: [0; 32],
    };
    if unsafe { (win32.user32.GetMonitorInfoW)(monitor as _, &mut info) } == 0 {
        return None;
    }
    let len = info.szDevice.iter().position(|&c| c == 0).unwrap_or(32);
    let device = String::from_utf16_lossy(&info.szDevice[..len]);
    let name = device.trim_start_matches(r"\\.\");
    if name.is_empty() {
        return None;
    }

    let key: Vec<u16> = format!(r"Software\Microsoft\Avalon.Graphics\{name}")
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let value: Vec<u16> = "PixelStructure".encode_utf16().chain(Some(0)).collect();
    let mut data: u32 = 0;
    let mut cb = core::mem::size_of::<u32>() as u32;
    let status = unsafe {
        (advapi32.RegGetValueW)(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_DWORD,
            core::ptr::null_mut(),
            (&mut data as *mut u32).cast(),
            &mut cb,
        )
    };
    if status != 0 {
        return None;
    }
    match data {
        1 => Some(LcdSubpixelOrder::Rgb),
        2 => Some(LcdSubpixelOrder::Bgr),
        // 0 = flat: no subpixel structure recorded — let the system value decide.
        _ => None,
    }
}

/// The system-wide `SPI_GETFONTSMOOTHINGORIENTATION`.
fn system_orientation(user32: &User32Functions) -> Option<LcdSubpixelOrder> {
    let mut orientation: u32 = 1;
    let ok = unsafe {
        (user32.SystemParametersInfoW)(
            SPI_GETFONTSMOOTHINGORIENTATION,
            0,
            (&mut orientation as *mut u32).cast(),
            0,
        )
    };
    if ok == 0 {
        return None;
    }
    Some(if orientation == FE_FONTSMOOTHINGORIENTATIONBGR {
        LcdSubpixelOrder::Bgr
    } else {
        LcdSubpixelOrder::Rgb
    })
}
