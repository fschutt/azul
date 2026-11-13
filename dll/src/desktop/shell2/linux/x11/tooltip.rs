//! X11 tooltip: an override-redirect window showing text drawn by azul's CPU
//! text path.
//!
//! The core X text path (`XDrawString`) draws Latin-1 bytes in a server font that
//! cannot be measured without its `XFontStruct` and cannot wrap. So the text is
//! shaped, wrapped and rasterized by the same CPU path the Wayland tooltip uses
//! (`render_wrapped_text_to_pixmap`) and put into the window with `XPutImage`.
//! The window is exactly the measured text's size and is placed by the rule every
//! floating surface follows (`azul_layout::placement`): below its anchor, above it
//! where there is no room, inside the work area of the monitor (XRandR) the anchor
//! is on - placed in the parent's logical px and put on the root in physical ones.
//!
//! Architecture:
//! - TooltipWindow: Wrapper around X11 Window with override-redirect
//! - Lifecycle: Create once, show/hide as needed

use std::{
    ffi::{c_char, c_uint, c_void},
    rc::Rc,
    sync::Arc,
};

use azul_core::geom::{LogicalRect, LogicalSize};
use azul_css::props::basic::ColorU;
use azul_layout::placement::{
    floating_frame, screen_for_anchor, tooltip_text_width, FloatingPlacement, ScreenSize,
    WindowToScreen,
};
use rust_fontconfig::FcFontCache;

use super::{defines::*, dlopen::Xlib};

/// Tooltip font size in logical pixels.
const TOOLTIP_FONT_SIZE_PX: f32 = 12.0;
/// Padding around the tooltip's text in logical pixels.
const TOOLTIP_PADDING_PX: f32 = 4.0;
/// Tooltip background colour (light yellow).
const TOOLTIP_BG_COLOR: ColorU = ColorU {
    r: 0xFF,
    g: 0xFF,
    b: 0xF0,
    a: 0xFF,
};
/// Tooltip text colour (black).
const TOOLTIP_TEXT_COLOR: ColorU = ColorU {
    r: 0x00,
    g: 0x00,
    b: 0x00,
    a: 0xFF,
};
/// The window's own background, behind the text image (light yellow).
const TOOLTIP_BG_PIXEL: std::os::raw::c_ulong = 0xFFFFF0;
const TOOLTIP_BORDER_COLOR: std::os::raw::c_ulong = 0x000000; // Black
const TOOLTIP_INITIAL_WIDTH: u32 = 200;
const TOOLTIP_INITIAL_HEIGHT: u32 = 30;
const TOOLTIP_BORDER_WIDTH: u32 = 1;
/// `ZPixmap`: the image format of 32-bit BGRA rows.
const Z_PIXMAP: i32 = 2;

/// Wrapper for an X11 tooltip window
pub struct TooltipWindow {
    /// Xlib function pointers
    xlib: Rc<Xlib>,
    /// X11 display connection
    display: *mut super::dlopen::Display,
    /// Tooltip window handle
    window: super::dlopen::Window,
    /// Graphics context the text image is put with
    gc: super::dlopen::GC,
    /// The fonts the tooltip's text is shaped with
    fc_cache: Arc<FcFontCache>,
    /// Is tooltip currently visible
    is_visible: bool,
}

impl TooltipWindow {
    /// Create a new tooltip window
    ///
    /// Creates an X11 window with override-redirect for tooltip display.
    /// The window is initially unmapped and can be shown with `show()`.
    pub fn new(
        xlib: Rc<Xlib>,
        display: *mut super::dlopen::Display,
        fc_cache: Arc<FcFontCache>,
    ) -> Result<Self, String> {
        unsafe {
            // Get screen and root window
            let screen = (xlib.XDefaultScreen)(display);
            let root = (xlib.XRootWindow)(display, screen);

            // Create window attributes for override-redirect
            let mut attributes: super::dlopen::XSetWindowAttributes = std::mem::zeroed();
            attributes.override_redirect = 1; // Don't let WM manage this window
            attributes.background_pixel = TOOLTIP_BG_PIXEL;
            attributes.border_pixel = TOOLTIP_BORDER_COLOR;

            // The default visual and depth (CopyFromParent from the root), the
            // ones the text image is created in.
            let window = (xlib.XCreateWindow)(
                display,
                root,
                0,
                0,
                TOOLTIP_INITIAL_WIDTH,
                TOOLTIP_INITIAL_HEIGHT,
                TOOLTIP_BORDER_WIDTH, // Border width
                CopyFromParent as i32,
                InputOutput as u32,
                std::ptr::null_mut(),
                CWOverrideRedirect | CWBackPixel | CWBorderPixel,
                &mut attributes,
            );

            if window == 0 {
                return Err("Failed to create tooltip window".to_string());
            }

            let gc = (xlib.XCreateGC)(display, window, 0, std::ptr::null_mut());
            if gc.is_null() {
                (xlib.XDestroyWindow)(display, window);
                return Err("Failed to create graphics context".to_string());
            }

            Ok(Self {
                xlib,
                display,
                window,
                gc,
                fc_cache,
                is_visible: false,
            })
        }
    }

    /// Show the tooltip with `text` for `anchor`, the box it explains in the
    /// logical coordinates of the window `window` maps onto the root. If the
    /// tooltip is already visible, its text and place are updated.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        clippy::cast_sign_loss
    )] // screen pixels
    pub fn show(
        &mut self,
        text: &str,
        anchor: LogicalRect,
        window: WindowToScreen,
    ) -> Result<(), String> {
        let screens = window.screens(&crate::desktop::display::desktop_screens());
        let screen = screen_for_anchor(anchor, &screens);
        let dpi = window.scale;

        // The text wraps at the tooltip width (less on a narrow monitor) and
        // grows at most to the monitor's height.
        let (screen_width, screen_height) = screen
            .map_or((f32::INFINITY, f32::INFINITY), |s| (s.size.width, s.size.height));
        let max_text = LogicalSize::new(
            tooltip_text_width(screen_width, TOOLTIP_PADDING_PX),
            (screen_height - TOOLTIP_PADDING_PX - TOOLTIP_PADDING_PX).max(1.0),
        );
        let pixmap = azul_layout::cpurender::render_wrapped_text_to_pixmap(
            &self.fc_cache,
            text,
            TOOLTIP_FONT_SIZE_PX,
            TOOLTIP_TEXT_COLOR,
            TOOLTIP_BG_COLOR,
            TOOLTIP_PADDING_PX,
            dpi,
            max_text,
        )
        .ok_or_else(|| "no font to draw the tooltip's text with".to_string())?;

        let (width, height) = (pixmap.width(), pixmap.height());
        let size = window.window_size(ScreenSize::new(width as f32, height as f32));
        let placement = FloatingPlacement::tooltip(anchor);
        let frame = match screen {
            Some(work_area) => floating_frame(&placement, size, work_area),
            None => LogicalRect::new(placement.preferred_origin(size), size),
        };
        // The frame on the root, in physical pixels.
        let frame = window.rect(frame);
        let shown_height = (frame.size.height.round() as u32).clamp(1, height.max(1));

        // RGBA rows to the BGRA of a 24/32-bit TrueColor visual.
        let mut bgra: Vec<u8> = pixmap
            .data()
            .chunks_exact(4)
            .take(width as usize * shown_height as usize)
            .flat_map(|px| [px[2], px[1], px[0], px[3]])
            .collect();

        unsafe {
            (self.xlib.XMoveResizeWindow)(
                self.display,
                self.window,
                frame.origin.x.round() as i32,
                frame.origin.y.round() as i32,
                width.max(1),
                shown_height,
            );

            // Map BEFORE drawing: X11 discards output drawn to an unviewable
            // window, and this window selects no ExposureMask (nobody would
            // repaint it). XSync so the map has been processed by the server
            // before the image is put.
            if !self.is_visible {
                (self.xlib.XMapWindow)(self.display, self.window);
                (self.xlib.XSync)(self.display, 0);
                self.is_visible = true;
            }

            let screen_number = (self.xlib.XDefaultScreen)(self.display);
            let visual = (self.xlib.XDefaultVisual)(self.display, screen_number);
            let depth = (self.xlib.XDefaultDepth)(self.display, screen_number) as c_uint;
            let image = (self.xlib.XCreateImage)(
                self.display,
                visual.cast::<c_void>(),
                depth,
                Z_PIXMAP,
                0,
                bgra.as_mut_ptr().cast::<c_char>(),
                width,
                shown_height,
                32, // bitmap_pad
                0,  // bytes_per_line (0 = width * 4)
            );
            if !image.is_null() {
                (self.xlib.XPutImage)(
                    self.display,
                    self.window,
                    self.gc,
                    image,
                    0,
                    0,
                    0,
                    0,
                    width,
                    shown_height,
                );
                // The pixels are ours (`bgra`), not Xlib's to free.
                (*image).data = std::ptr::null_mut();
                (self.xlib.XDestroyImage)(image);
            }

            (self.xlib.XFlush)(self.display);
        }

        Ok(())
    }

    /// Hide the tooltip
    ///
    /// Unmaps the tooltip window without destroying it.
    /// Can be shown again with `show()`.
    pub fn hide(&mut self) -> Result<(), String> {
        if !self.is_visible {
            return Ok(());
        }

        unsafe {
            (self.xlib.XUnmapWindow)(self.display, self.window);
            (self.xlib.XFlush)(self.display);
            self.is_visible = false;
        }

        Ok(())
    }

    /// Check if tooltip is currently visible
    pub fn is_visible(&self) -> bool {
        self.is_visible
    }
}

impl Drop for TooltipWindow {
    fn drop(&mut self) {
        unsafe {
            if !self.gc.is_null() {
                (self.xlib.XFreeGC)(self.display, self.gc);
            }
            if self.window != 0 {
                (self.xlib.XDestroyWindow)(self.display, self.window);
            }
            (self.xlib.XFlush)(self.display);
        }
    }
}
