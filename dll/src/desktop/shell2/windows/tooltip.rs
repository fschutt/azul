//! Windows tooltip implementation using TOOLTIPS_CLASS.
//!
//! The native tooltip control draws, measures and wraps the text itself: it is
//! told the width to wrap at (the tooltip width at the window's DPI, less on a
//! narrow monitor) and asked the size it came out at (`TTM_GETBUBBLESIZE`). A
//! tracking tooltip at an absolute position is not kept on the screen by the
//! control, so it is placed by the rule every floating surface follows
//! (`azul_layout::placement`): below its anchor, above it where there is no
//! room, inside the work area of the monitor the anchor is on, in physical
//! screen pixels.
//!
//! Architecture:
//! - TooltipWindow: Wraps a Win32 tooltip control (HWND)
//! - Lifecycle: Create once, show/hide as needed
//! - Positioning: TTM_TRACKPOSITION at the placed frame's top-left

use std::ptr;

use azul_core::{
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    resources::DpiScaleFactor,
};
use azul_layout::placement::{
    floating_frame, screen_for_anchor, tooltip_text_width, FloatingPlacement,
};

use super::dlopen::{Win32Libraries, HWND, LPARAM};

// Win32 Constants for tooltips
const WM_USER: u32 = 0x0400;
const TTM_ADDTOOLW: u32 = WM_USER + 50;
const TTM_TRACKACTIVATE: u32 = WM_USER + 17;
const TTM_TRACKPOSITION: u32 = WM_USER + 18;
const TTM_SETMAXTIPWIDTH: u32 = WM_USER + 24;
const TTM_UPDATETIPTEXTW: u32 = WM_USER + 57;
const TTM_GETBUBBLESIZE: u32 = WM_USER + 30;

const TTS_ALWAYSTIP: u32 = 0x01;
const TTS_NOPREFIX: u32 = 0x02;
const TTS_NOANIMATE: u32 = 0x10;
const TTS_NOFADE: u32 = 0x20;

const TTF_TRACK: u32 = 0x0020;
const TTF_ABSOLUTE: u32 = 0x0080;
const TTF_IDISHWND: u32 = 0x0001;


const WS_POPUP: u32 = 0x80000000;
const WS_EX_TOPMOST: u32 = 0x00000008;
const WS_EX_TOOLWINDOW: u32 = 0x00000080;
const WS_EX_TRANSPARENT: u32 = 0x00000020;

const CW_USEDEFAULT: i32 = 0x80000000u32 as i32;

// TOOLINFOW structure for Win32 tooltip API
#[repr(C)]
struct TOOLINFOW {
    cbSize: u32,
    uFlags: u32,
    hwnd: HWND,
    uId: usize,
    rect: super::dlopen::RECT,
    hinst: super::dlopen::HINSTANCE,
    lpszText: *mut u16,
    lParam: LPARAM,
    lpReserved: *mut std::ffi::c_void,
}

impl Default for TOOLINFOW {
    fn default() -> Self {
        Self {
            cbSize: std::mem::size_of::<TOOLINFOW>() as u32,
            uFlags: 0,
            hwnd: ptr::null_mut(),
            uId: 0,
            rect: super::dlopen::RECT::default(),
            hinst: ptr::null_mut(),
            lpszText: ptr::null_mut(),
            lParam: 0,
            lpReserved: ptr::null_mut(),
        }
    }
}

/// Wrapper for a Win32 tooltip window
pub struct TooltipWindow {
    /// Tooltip control handle
    pub hwnd_tooltip: HWND,
    /// Parent window handle
    pub hwnd_parent: HWND,
    /// Win32 libraries for SendMessageW
    pub win32: Win32Libraries,
    /// Current text buffer (must stay alive while tooltip is shown)
    text_buffer: Vec<u16>,
    /// Is tooltip currently visible
    is_visible: bool,
}

impl TooltipWindow {
    /// Create a new tooltip window
    ///
    /// Creates a Win32 tooltip control attached to the parent window.
    /// The tooltip is initially hidden and can be shown with `show()`.
    pub fn new(hwnd_parent: HWND, win32: Win32Libraries) -> Result<Self, String> {
        // TOOLTIPS_CLASS name as wide string
        let class_name = "tooltips_class32\0".encode_utf16().collect::<Vec<u16>>();

        // Create tooltip control
        let hwnd_tooltip = unsafe {
            (win32.user32.CreateWindowExW)(
                WS_EX_TOPMOST | WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW,
                class_name.as_ptr(),
                ptr::null(),
                WS_POPUP | TTS_ALWAYSTIP | TTS_NOPREFIX | TTS_NOANIMATE | TTS_NOFADE,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                hwnd_parent,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
            )
        };

        if hwnd_tooltip.is_null() {
            return Err("Failed to create tooltip window".to_string());
        }

        Ok(Self {
            hwnd_tooltip,
            hwnd_parent,
            win32,
            text_buffer: Vec::new(),
            is_visible: false,
        })
    }

    /// Show the tooltip with `text` for `anchor`, the box it explains in the
    /// parent's client area (logical px). If the tooltip is already visible,
    /// its text and place are updated.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        clippy::cast_sign_loss
    )] // screen pixels and packed words
    pub fn show(
        &mut self,
        text: &str,
        anchor: LogicalRect,
        dpi_factor: DpiScaleFactor,
    ) -> Result<(), String> {
        let scale = dpi_factor.inner.get();

        // The anchor in physical SCREEN pixels, the space the monitors are
        // listed in: TTM_TRACKPOSITION takes screen coordinates, `anchor` is
        // client-area logical.
        let mut origin = super::dlopen::POINT {
            x: (anchor.origin.x * scale).round() as i32,
            y: (anchor.origin.y * scale).round() as i32,
        };
        unsafe {
            (self.win32.user32.ClientToScreen)(self.hwnd_parent, &mut origin);
        }
        let anchor_px = LogicalRect::new(
            LogicalPosition::new(origin.x as f32, origin.y as f32),
            LogicalSize::new(anchor.size.width * scale, anchor.size.height * scale),
        );
        let screen = screen_for_anchor(anchor_px, &crate::desktop::display::screen_rects());

        // Wrap at the tooltip width at this DPI, or the monitor's if narrower.
        let screen_width = screen.map_or(f32::INFINITY, |s| s.size.width / scale);
        let max_width = (tooltip_text_width(screen_width, 0.0) * scale).round() as LPARAM;
        unsafe {
            (self.win32.user32.SendMessageW)(self.hwnd_tooltip, TTM_SETMAXTIPWIDTH, 0, max_width);
        }

        // Convert text to UTF-16
        self.text_buffer = text.encode_utf16().chain(Some(0u16)).collect();

        // If first time showing, register the tooltip
        if !self.is_visible {
            let mut ti = TOOLINFOW {
                uFlags: TTF_TRACK | TTF_ABSOLUTE | TTF_IDISHWND,
                hwnd: self.hwnd_parent,
                uId: self.hwnd_parent as usize,
                lpszText: self.text_buffer.as_mut_ptr(),
                ..Default::default()
            };

            unsafe {
                (self.win32.user32.SendMessageW)(
                    self.hwnd_tooltip,
                    TTM_ADDTOOLW,
                    0,
                    &mut ti as *mut TOOLINFOW as LPARAM,
                );
            }

            self.is_visible = true;
        } else {
            // Update text
            let mut ti = TOOLINFOW {
                uFlags: TTF_IDISHWND,
                hwnd: self.hwnd_parent,
                uId: self.hwnd_parent as usize,
                lpszText: self.text_buffer.as_mut_ptr(),
                ..Default::default()
            };

            unsafe {
                (self.win32.user32.SendMessageW)(
                    self.hwnd_tooltip,
                    TTM_UPDATETIPTEXTW,
                    0,
                    &mut ti as *mut TOOLINFOW as LPARAM,
                );
            }
        }

        // The size the control shows this text at, wrapped: width in the low
        // word, height in the high word.
        let mut ti = TOOLINFOW {
            uFlags: TTF_IDISHWND,
            hwnd: self.hwnd_parent,
            uId: self.hwnd_parent as usize,
            ..Default::default()
        };
        let bubble = unsafe {
            (self.win32.user32.SendMessageW)(
                self.hwnd_tooltip,
                TTM_GETBUBBLESIZE,
                0,
                &mut ti as *mut TOOLINFOW as LPARAM,
            )
        } as usize;
        let size = LogicalSize::new((bubble & 0xFFFF) as f32, ((bubble >> 16) & 0xFFFF) as f32);

        let placement = FloatingPlacement::tooltip(anchor_px);
        let frame = match screen {
            Some(work_area) => floating_frame(&placement, size, work_area),
            None => LogicalRect::new(placement.preferred_origin(size), size),
        };
        let (x, y) = (frame.origin.x.round() as i32, frame.origin.y.round() as i32);

        // Set tooltip position (screen coordinates, a signed word each)
        let pos_param = ((y as u32) << 16) | (x as u32 & 0xFFFF);
        unsafe {
            (self.win32.user32.SendMessageW)(
                self.hwnd_tooltip,
                TTM_TRACKPOSITION,
                0,
                pos_param as LPARAM,
            );
        }

        // Activate tooltip
        let mut ti = TOOLINFOW {
            uFlags: TTF_IDISHWND,
            hwnd: self.hwnd_parent,
            uId: self.hwnd_parent as usize,
            ..Default::default()
        };

        unsafe {
            (self.win32.user32.SendMessageW)(
                self.hwnd_tooltip,
                TTM_TRACKACTIVATE,
                1, // TRUE = activate
                &mut ti as *mut TOOLINFOW as LPARAM,
            );
        }

        Ok(())
    }

    /// Hide the tooltip
    ///
    /// Deactivates the tooltip without destroying it.
    /// Can be shown again with `show()`.
    pub fn hide(&mut self) -> Result<(), String> {
        if !self.is_visible {
            return Ok(());
        }

        let mut ti = TOOLINFOW {
            uFlags: TTF_IDISHWND,
            hwnd: self.hwnd_parent,
            uId: self.hwnd_parent as usize,
            ..Default::default()
        };

        unsafe {
            (self.win32.user32.SendMessageW)(
                self.hwnd_tooltip,
                TTM_TRACKACTIVATE,
                0, // FALSE = deactivate
                &mut ti as *mut TOOLINFOW as LPARAM,
            );
        }

        self.is_visible = false;

        Ok(())
    }

    /// Check if tooltip is currently visible
    #[must_use]
    pub fn is_visible(&self) -> bool {
        self.is_visible
    }
}

impl Drop for TooltipWindow {
    fn drop(&mut self) {
        // Destroy tooltip window
        if !self.hwnd_tooltip.is_null() {
            unsafe {
                (self.win32.user32.DestroyWindow)(self.hwnd_tooltip);
            }
        }
    }
}
