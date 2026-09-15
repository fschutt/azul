//! Which LCD stripe order the panel under a window has.
//!
//! Subpixel text is blended for one physical stripe order, and it is wrong on
//! a panel with the other: the colour fringes trade sides. The order is a
//! property of the MONITOR, not of the system, so a window has to re-check it
//! whenever it may have moved to a different one.
//!
//! Windows keeps it in two places:
//!
//!  * `HKCU\Software\Microsoft\Avalon.Graphics\<DISPLAYn>\PixelStructure` — written per monitor by
//!    the ClearType Text Tuner (`cttune.exe`): 0 flat, 1 RGB, 2 BGR. This is the per-monitor answer.
//!  * `SPI_GETFONTSMOOTHINGORIENTATION` — the one system-wide value GDI uses (0 BGR, 1 RGB), and
//!    the fallback for a monitor the tuner never ran on.

use core::ffi::c_void;

use azul_layout::glyph_cache::LcdSubpixelOrder;

type Hwnd = *mut c_void;
type Hmonitor = *mut c_void;

const MONITOR_DEFAULTTONEAREST: u32 = 2;
const SPI_GETFONTSMOOTHINGORIENTATION: u32 = 0x2012;
const FE_FONTSMOOTHINGORIENTATIONBGR: u32 = 0;
const HKEY_CURRENT_USER: isize = 0x8000_0001_u32 as i32 as isize;
const RRF_RT_REG_DWORD: u32 = 0x0000_0010;

#[repr(C)]
struct Rect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[repr(C)]
struct MonitorInfoExW {
    cb_size: u32,
    rc_monitor: Rect,
    rc_work: Rect,
    dw_flags: u32,
    sz_device: [u16; 32],
}

#[link(name = "user32")]
extern "system" {
    fn MonitorFromWindow(hwnd: Hwnd, flags: u32) -> Hmonitor;
    fn GetMonitorInfoW(monitor: Hmonitor, info: *mut MonitorInfoExW) -> i32;
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
pub(super) fn monitor_of(hwnd: Hwnd) -> isize {
    unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) as isize }
}

/// The stripe order of the panel `monitor` (from [`monitor_of`]).
///
/// A flat or unreported panel maps to RGB, the order every LCD blend used
/// before the order was selectable: it is also what GDI assumes.
pub(super) fn subpixel_order_of(monitor: isize) -> LcdSubpixelOrder {
    per_monitor_pixel_structure(monitor)
        .or_else(system_orientation)
        .unwrap_or(LcdSubpixelOrder::Rgb)
}

/// `Avalon.Graphics\<DISPLAYn>\PixelStructure` for this monitor, if the tuner
/// ever recorded one.
fn per_monitor_pixel_structure(monitor: isize) -> Option<LcdSubpixelOrder> {
    if monitor == 0 {
        return None;
    }
    // "\\.\DISPLAY1" -> "DISPLAY1": the registry key is the device name
    // without the Win32 device namespace prefix.
    let mut info = MonitorInfoExW {
        cb_size: core::mem::size_of::<MonitorInfoExW>() as u32,
        rc_monitor: Rect { left: 0, top: 0, right: 0, bottom: 0 },
        rc_work: Rect { left: 0, top: 0, right: 0, bottom: 0 },
        dw_flags: 0,
        sz_device: [0; 32],
    };
    if unsafe { GetMonitorInfoW(monitor as Hmonitor, &mut info) } == 0 {
        return None;
    }
    let len = info.sz_device.iter().position(|&c| c == 0).unwrap_or(32);
    let device = String::from_utf16_lossy(&info.sz_device[..len]);
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
