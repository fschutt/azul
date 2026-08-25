//! Windows screen read for the eyedropper: `BitBlt` of the screen DC into a
//! 32 bpp top-down DIB section. Freely readable - Win32 has no permission
//! model for the desktop (the `CAPTUREBLT` flag includes layered windows).
//! The primary monitor is captured; the loupe window opens fullscreen on it.

use azul_core::geom::LogicalPosition;

use super::Screenshot;
use crate::desktop::shell2::windows::{
    dlopen::{BitmapInfoHeader, DIB_RGB_COLORS, SRCCOPY},
    Win32Window,
};

/// `GetSystemMetrics` indices for the primary monitor's size.
const SM_CXSCREEN: i32 = 0;
const SM_CYSCREEN: i32 = 1;
/// `BitBlt` flag: include layered (per-pixel alpha) windows in the copy.
const CAPTUREBLT: u32 = 0x4000_0000;

/// Read the primary monitor. `None` if GDI refused any step.
#[must_use]
pub fn capture(window: &Win32Window) -> Option<Screenshot> {
    let user32 = &window.win32.user32;
    let gdi32 = &window.win32.gdi32;
    unsafe {
        let width = (user32.GetSystemMetrics)(SM_CXSCREEN);
        let height = (user32.GetSystemMetrics)(SM_CYSCREEN);
        if width <= 0 || height <= 0 {
            return None;
        }
        let screen_dc = (user32.GetDC)(core::ptr::null_mut());
        if screen_dc.is_null() {
            return None;
        }
        let mem_dc = (gdi32.CreateCompatibleDC)(screen_dc);
        if mem_dc.is_null() {
            (user32.ReleaseDC)(core::ptr::null_mut(), screen_dc);
            return None;
        }
        let header = BitmapInfoHeader {
            biSize: core::mem::size_of::<BitmapInfoHeader>() as u32,
            biWidth: width,
            biHeight: -height, // top-down
            biPlanes: 1,
            biBitCount: 32,
            biCompression: 0, // BI_RGB: BGRX
            biSizeImage: 0,
            biXPelsPerMeter: 0,
            biYPelsPerMeter: 0,
            biClrUsed: 0,
            biClrImportant: 0,
        };
        let mut bits: *mut core::ffi::c_void = core::ptr::null_mut();
        let dib = (gdi32.CreateDIBSection)(
            screen_dc,
            &header,
            DIB_RGB_COLORS,
            &mut bits,
            core::ptr::null_mut(),
            0,
        );
        let mut out = None;
        if !dib.is_null() && !bits.is_null() {
            let old = (gdi32.SelectObject)(mem_dc, dib);
            let ok = (gdi32.BitBlt)(mem_dc, 0, 0, width, height, screen_dc, 0, 0, SRCCOPY | CAPTUREBLT);
            if ok != 0 {
                #[allow(clippy::cast_sign_loss)] // checked positive
                let n = width as usize * height as usize * 4;
                let bgra = core::slice::from_raw_parts(bits.cast::<u8>(), n);
                #[allow(clippy::cast_sign_loss)]
                let shot = Screenshot {
                    width: width as u32,
                    height: height as u32,
                    rgba: super::bgra_to_rgba(bgra),
                    origin: LogicalPosition::zero(),
                    scale: window.common.current_window_state().size.get_hidpi_factor().inner.get().max(0.01),
                };
                out = Some(shot);
            } else {
                crate::plog_warn!("[eyedropper] windows: BitBlt of the screen failed");
            }
            (gdi32.SelectObject)(mem_dc, old);
            (gdi32.DeleteObject)(dib);
        }
        (gdi32.DeleteDC)(mem_dc);
        (user32.ReleaseDC)(core::ptr::null_mut(), screen_dc);
        out
    }
}
