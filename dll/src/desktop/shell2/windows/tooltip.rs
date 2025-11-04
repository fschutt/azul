//! Windows tooltip implementation using TOOLTIPS_CLASS.
//!
//! This module provides a native Windows tooltip system using the
//! standard Win32 TOOLTIPS_CLASS control. Tooltips can be shown at
//! specific positions and hidden programmatically.
//!
//! Architecture:
//! - TooltipWindow: Wraps a Win32 tooltip control (HWND)
//! - Lifecycle: Create once, show/hide as needed
//! - Positioning: TTM_TRACKPOSITION for absolute positioning

use std::ptr;

use azul_core::{
    geom::{LogicalPosition, PhysicalPosition},
    resources::DpiScaleFactor,
};

use super::dlopen::{Win32Libraries, HWND, LPARAM, WPARAM};

// Win32 Constants for tooltips
const WM_USER: u32 = 0x0400;
const TTM_ADDTOOLW: u32 = WM_USER + 50;
const TTM_TRACKACTIVATE: u32 = WM_USER + 17;
const TTM_TRACKPOSITION: u32 = WM_USER + 18;
const TTM_SETTIPBKCOLOR: u32 = WM_USER + 19;
const TTM_SETTIPTEXTCOLOR: u32 = WM_USER + 20;
const TTM_SETMAXTIPWIDTH: u32 = WM_USER + 24;
const TTM_UPDATETIPTEXTW: u32 = WM_USER + 57;

const TTS_ALWAYSTIP: u32 = 0x01;
const TTS_NOPREFIX: u32 = 0x02;
const TTS_NOANIMATE: u32 = 0x10;
const TTS_NOFADE: u32 = 0x20;
const TTS_BALLOON: u32 = 0x40;

const TTF_TRACK: u32 = 0x0020;
const TTF_ABSOLUTE: u32 = 0x0080;
const TTF_TRANSPARENT: u32 = 0x0100;
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

        // Set max width for multi-line tooltips (300 pixels)
        unsafe {
            (win32.user32.SendMessageW)(hwnd_tooltip, TTM_SETMAXTIPWIDTH, 0, 300);
        }

        Ok(Self {
            hwnd_tooltip,
            hwnd_parent,
            win32,
            text_buffer: Vec::new(),
            is_visible: false,
        })
    }

    /// Show tooltip with text at the given position
    ///
    /// If tooltip is already visible, updates text and position.
    /// Position is in logical coordinates and will be converted to physical.
    pub fn show(
        &mut self,
        text: &str,
        position: LogicalPosition,
        dpi_factor: DpiScaleFactor,
    ) -> Result<(), String> {
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

        // Convert position to physical coordinates
        let physical_pos = position.to_physical(dpi_factor.inner.get());
        let x = physical_pos.x as i32;
        let y = physical_pos.y as i32;

        // Set tooltip position
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

        Ok(())
    }

    /// Check if tooltip is currently visible
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
