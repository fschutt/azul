//! X11 tooltip implementation using a transient override-redirect window.
//!
//! This module provides a native X11 tooltip system using a simple
//! override-redirect window with text rendering. The tooltip follows
//! X11 conventions for transient, non-interactive overlays.
//!
//! Architecture:
//! - TooltipWindow: Wrapper around X11 Window with override-redirect
//! - Lifecycle: Create once, show/hide as needed
//! - Positioning: Uses XMoveWindow for absolute positioning

use std::{ffi::CString, rc::Rc};

use azul_core::{geom::LogicalPosition, resources::DpiScaleFactor};

use super::{defines::*, dlopen::Xlib};

/// Wrapper for an X11 tooltip window
pub struct TooltipWindow {
    /// Xlib function pointers
    xlib: Rc<Xlib>,
    /// X11 display connection
    display: *mut super::dlopen::Display,
    /// Tooltip window handle
    window: super::dlopen::Window,
    /// Graphics context for drawing text
    gc: super::dlopen::GC,
    /// Current text displayed in tooltip
    text: String,
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
        parent: super::dlopen::Window,
    ) -> Result<Self, String> {
        unsafe {
            // Get screen and root window
            let screen = (xlib.XDefaultScreen)(display);
            let root = (xlib.XRootWindow)(display, screen);

            // Create window attributes for override-redirect
            let mut attributes: super::dlopen::XSetWindowAttributes = std::mem::zeroed();
            attributes.override_redirect = 1; // Don't let WM manage this window
            attributes.background_pixel = 0xFFFFF0; // Light yellow background
            attributes.border_pixel = 0x000000; // Black border

            // Create the tooltip window (initially 200x30 pixels)
            let window = (xlib.XCreateWindow)(
                display,
                root,
                0,
                0,
                200,
                30,
                1, // Border width
                CopyFromParent as i32,
                InputOutput as u32,
                std::ptr::null_mut(),
                CWOverrideRedirect | CWBackPixel | CWBorderPixel,
                &mut attributes,
            );

            if window == 0 {
                return Err("Failed to create tooltip window".to_string());
            }

            // Create graphics context for text rendering
            let gc = (xlib.XCreateGC)(display, window, 0, std::ptr::null_mut());
            if gc.is_null() {
                (xlib.XDestroyWindow)(display, window);
                return Err("Failed to create graphics context".to_string());
            }

            // Set text color to black
            (xlib.XSetForeground)(display, gc, 0x000000);

            Ok(Self {
                xlib,
                display,
                window,
                gc,
                text: String::new(),
                is_visible: false,
            })
        }
    }

    /// Show tooltip with text at the given position
    ///
    /// If tooltip is already visible, updates text and position.
    /// Position is in logical coordinates (will be converted to physical).
    pub fn show(
        &mut self,
        text: &str,
        position: LogicalPosition,
        dpi_factor: DpiScaleFactor,
    ) -> Result<(), String> {
        unsafe {
            self.text = text.to_string();

            // Calculate window size based on text length
            let text_width = (text.len() as f32 * 7.0 + 10.0) as u32; // ~7px per char + padding
            let text_width = text_width.min(400).max(50); // Clamp between 50-400px
            let text_height = 25u32;

            // Convert position to physical coordinates
            let physical_pos = position.to_physical(dpi_factor.inner.get());
            let x = physical_pos.x as i32;
            let y = physical_pos.y as i32;

            // Resize and reposition window
            (self.xlib.XMoveResizeWindow)(self.display, self.window, x, y, text_width, text_height);

            // Clear window
            (self.xlib.XClearWindow)(self.display, self.window);

            // Draw text
            let c_text = CString::new(text).unwrap_or_else(|_| CString::new("").unwrap());
            (self.xlib.XDrawString)(
                self.display,
                self.window,
                self.gc,
                5,  // X offset
                17, // Y offset (baseline)
                c_text.as_ptr(),
                text.len() as i32,
            );

            // Show window if not already visible
            if !self.is_visible {
                (self.xlib.XMapWindow)(self.display, self.window);
                self.is_visible = true;
            }

            // Flush changes to display
            (self.xlib.XFlush)(self.display);

            Ok(())
        }
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
        }
    }
}
