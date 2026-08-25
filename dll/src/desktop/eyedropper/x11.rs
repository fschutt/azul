//! X11 screen read for the eyedropper: `XGetImage` of the root window.
//!
//! No permission model on X11 - any client may read the root. With a
//! compositing manager the root carries the composited frame, which is what
//! `scrot` / `import` read as well.

use azul_core::geom::LogicalPosition;

use super::Screenshot;
use crate::desktop::shell2::linux::x11::X11Window;

/// Read the whole default screen. `None` if the server refused the image.
#[must_use]
pub fn capture(window: &X11Window) -> Option<Screenshot> {
    use crate::desktop::shell2::linux::x11::defines::{AllPlanes, ZPixmap};
    let xlib = &window.xlib;
    let display = window.display;
    unsafe {
        let screen = (xlib.XDefaultScreen)(display);
        let root = (xlib.XRootWindow)(display, screen);
        let width = (xlib.XDisplayWidth)(display, screen);
        let height = (xlib.XDisplayHeight)(display, screen);
        if width <= 0 || height <= 0 {
            return None;
        }
        #[allow(clippy::cast_sign_loss)] // checked positive
        let (w, h) = (width as u32, height as u32);
        let image = (xlib.XGetImage)(display, root, 0, 0, w, h, AllPlanes, ZPixmap);
        if image.is_null() {
            crate::plog_warn!("[eyedropper] x11: XGetImage of the root failed");
            return None;
        }
        let img = &*image;
        let rgba = if img.bits_per_pixel == 32 {
            // One row at a time: `bytes_per_line` may pad past `width * 4`.
            let mut out = Vec::with_capacity(w as usize * h as usize * 4);
            #[allow(clippy::cast_sign_loss)]
            let stride = img.bytes_per_line as usize;
            for y in 0..h as usize {
                let row = core::slice::from_raw_parts(img.data.cast::<u8>().add(y * stride), w as usize * 4);
                out.extend(super::bgra_to_rgba(row));
            }
            out
        } else {
            crate::plog_warn!(
                "[eyedropper] x11: root image is {} bpp, only 32 bpp is read",
                img.bits_per_pixel
            );
            (xlib.XDestroyImage)(image);
            return None;
        };
        (xlib.XDestroyImage)(image);
        Some(Screenshot {
            width: w,
            height: h,
            rgba,
            origin: LogicalPosition::zero(),
            scale: window.hidpi().max(0.01),
        })
    }
}
