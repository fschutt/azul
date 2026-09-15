//! LCD stripe order of the monitor a window is on (ClearType per-monitor setting, then the system one).

use core::ffi::c_void;

use azul_layout::glyph_cache::LcdSubpixelOrder;

use super::dlopen::{User32Functions, HWND, MONITORINFOEXW, MONITOR_DEFAULTTONEAREST, RECT};

const SPI_GETFONTSMOOTHINGORIENTATION: u32 = 0x2012;
const FE_FONTSMOOTHINGORIENTATIONBGR: u32 = 0;
const HKEY_CURRENT_USER: isize = 0x8000_0001_u32 as i32 as isize;
const RRF_RT_REG_DWORD: u32 = 0x0000_0010;

#[link(name = "user32")]
extern "system" {
    fn SystemParametersInfoW(action: u32, param: u32, pv: *mut c_void, winini: u32) -> i32;
}

#[link(name = "advapi32")]
extern "system" {
    fn RegGetValueW(
        hkey: isize,
        sub_key: *const u16,
        value: *const u16,
        flags: u32,
        ty: *mut u32,
        data: *mut c_void,
        cb_data: *mut u32,
    ) -> i32;
}

/// The monitor `hwnd` is (mostly) on, as an opaque identity for change
/// detection. Cheap enough to call on every move.
pub(super) fn monitor_of(user32: &User32Functions, hwnd: HWND) -> isize {
    unsafe { (user32.MonitorFromWindow)(hwnd, MONITOR_DEFAULTTONEAREST) as isize }
}

/// Flat or unreported panels count as RGB.
pub(super) fn subpixel_order_of(user32: &User32Functions, monitor: isize) -> LcdSubpixelOrder {
    per_monitor_pixel_structure(user32, monitor)
        .or_else(system_orientation)
        .unwrap_or(LcdSubpixelOrder::Rgb)
}

/// `Avalon.Graphics\<DISPLAYn>\PixelStructure` for this monitor, if the tuner
/// ever recorded one.
fn per_monitor_pixel_structure(user32: &User32Functions, monitor: isize) -> Option<LcdSubpixelOrder> {
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
    if unsafe { (user32.GetMonitorInfoW)(monitor as _, &mut info) } == 0 {
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
        RegGetValueW(
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
fn system_orientation() -> Option<LcdSubpixelOrder> {
    let mut orientation: u32 = 1;
    let ok = unsafe {
        SystemParametersInfoW(
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
