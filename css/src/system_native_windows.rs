//! Native Windows system style discovery via LoadLibrary + GetProcAddress.
//!
//! This module loads `User32.dll`, `Dwmapi.dll`, and `UxTheme.dll` at runtime,
//! queries system metrics, colours, and input timing, then immediately frees
//! the library handles.
//!
//! No external crates are required — all calls go through `kernel32` functions
//! which are always available on Windows.

#![allow(non_snake_case)]

use core::ffi::c_void;

use super::{defaults, InputMetrics, TextRenderingHints, SubpixelType};
use crate::props::basic::color::{ColorU, OptionColorU};

// ── kernel32 functions (always linked on Windows) ────────────────────────

extern "system" {
    fn LoadLibraryA(name: *const u8) -> *mut c_void;
    fn GetProcAddress(module: *mut c_void, name: *const u8) -> *mut c_void;
    fn FreeLibrary(module: *mut c_void) -> i32;
}

// ── Win32 constants ──────────────────────────────────────────────────────

const SM_CXDOUBLECLK: i32 = 36;
const SM_CYDOUBLECLK: i32 = 37;
const SM_CXDRAG: i32 = 68;
const SM_CXVSCROLL: i32 = 2;

const SPI_GETFONTSMOOTHING: u32 = 0x004A;
const SPI_GETFONTSMOOTHINGTYPE: u32 = 0x200A;
const SPI_GETWHEELSCROLLLINES: u32 = 0x0068;

const FE_FONTSMOOTHINGSTANDARD: u32 = 1;
const FE_FONTSMOOTHINGCLEARTYPE: u32 = 2;

// ── Function pointer types ───────────────────────────────────────────────

type FnGetSystemMetrics = unsafe extern "system" fn(i32) -> i32;
type FnGetDoubleClickTime = unsafe extern "system" fn() -> u32;
type FnGetCaretBlinkTime = unsafe extern "system" fn() -> u32;
type FnSystemParametersInfoW = unsafe extern "system" fn(u32, u32, *mut c_void, u32) -> i32;
type FnGetSysColor = unsafe extern "system" fn(i32) -> u32;
type FnDwmGetColorizationColor = unsafe extern "system" fn(*mut u32, *mut i32) -> i32;

// ── Library wrapper ──────────────────────────────────────────────────────

struct User32 {
    GetSystemMetrics:      FnGetSystemMetrics,
    GetDoubleClickTime:    FnGetDoubleClickTime,
    GetCaretBlinkTime:     FnGetCaretBlinkTime,
    SystemParametersInfoW: FnSystemParametersInfoW,
    GetSysColor:           FnGetSysColor,
    _handle: *mut c_void,
}

impl User32 {
    fn load() -> Option<Self> {
        unsafe {
            let h = LoadLibraryA(b"User32.dll\0".as_ptr());
            if h.is_null() { return None; }

            macro_rules! sym {
                ($name:ident, $ty:ty) => {{
                    let p = GetProcAddress(h, concat!(stringify!($name), "\0").as_ptr());
                    if p.is_null() { FreeLibrary(h); return None; }
                    core::mem::transmute::<_, $ty>(p)
                }};
            }

            Some(User32 {
                GetSystemMetrics:      sym!(GetSystemMetrics, FnGetSystemMetrics),
                GetDoubleClickTime:    sym!(GetDoubleClickTime, FnGetDoubleClickTime),
                GetCaretBlinkTime:     sym!(GetCaretBlinkTime, FnGetCaretBlinkTime),
                SystemParametersInfoW: sym!(SystemParametersInfoW, FnSystemParametersInfoW),
                GetSysColor:           sym!(GetSysColor, FnGetSysColor),
                _handle: h,
            })
        }
    }
}

impl Drop for User32 {
    fn drop(&mut self) { unsafe { FreeLibrary(self._handle); } }
}

struct Dwmapi {
    DwmGetColorizationColor: FnDwmGetColorizationColor,
    _handle: *mut c_void,
}

impl Dwmapi {
    fn load() -> Option<Self> {
        unsafe {
            let h = LoadLibraryA(b"Dwmapi.dll\0".as_ptr());
            if h.is_null() { return None; }
            let p = GetProcAddress(h, b"DwmGetColorizationColor\0".as_ptr());
            if p.is_null() { FreeLibrary(h); return None; }
            Some(Dwmapi {
                DwmGetColorizationColor: core::mem::transmute(p),
                _handle: h,
            })
        }
    }
}

impl Drop for Dwmapi {
    fn drop(&mut self) { unsafe { FreeLibrary(self._handle); } }
}

// ── Helpers ──────────────────────────────────────────────────────────────

fn color_from_sys(u32: &FnGetSysColor, index: i32) -> ColorU {
    let c = unsafe { u32(index) };
    // GetSysColor returns 0x00BBGGRR
    let r = (c & 0xFF) as u8;
    let g = ((c >> 8) & 0xFF) as u8;
    let b = ((c >> 16) & 0xFF) as u8;
    ColorU::new_rgb(r, g, b)
}

// ── Public entry point ───────────────────────────────────────────────────

/// Discover Windows system style via LoadLibrary.
///
/// Falls back to `defaults::windows_11_light()` if any DLL fails to load.
pub(super) fn discover() -> super::SystemStyle {
    let u32_lib = match User32::load() {
        Some(l) => l,
        None => return defaults::windows_11_light(),
    };

    let mut style = defaults::windows_11_light();

    unsafe {
        // ── Input metrics ────────────────────────────────────────────
        style.input = InputMetrics {
            double_click_time_ms:    (u32_lib.GetDoubleClickTime)(),
            double_click_distance_px: (u32_lib.GetSystemMetrics)(SM_CXDOUBLECLK) as f32,
            drag_threshold_px:       (u32_lib.GetSystemMetrics)(SM_CXDRAG) as f32,
            caret_blink_rate_ms:     (u32_lib.GetCaretBlinkTime)(),
            wheel_scroll_lines: {
                let mut lines: u32 = 3;
                (u32_lib.SystemParametersInfoW)(
                    SPI_GETWHEELSCROLLLINES,
                    0,
                    &mut lines as *mut u32 as *mut c_void,
                    0,
                );
                lines
            },
        };

        // ── System colours (classic GetSysColor) ─────────────────────
        // COLOR_WINDOW = 5, COLOR_WINDOWTEXT = 8, COLOR_HIGHLIGHT = 13,
        // COLOR_HIGHLIGHTTEXT = 14, COLOR_BTNFACE = 15, COLOR_BTNTEXT = 18,
        // COLOR_GRAYTEXT = 17
        style.colors.window_background = OptionColorU::Some(color_from_sys(&u32_lib.GetSysColor, 5));
        style.colors.text              = OptionColorU::Some(color_from_sys(&u32_lib.GetSysColor, 8));
        style.colors.selection_background = OptionColorU::Some(color_from_sys(&u32_lib.GetSysColor, 13));
        style.colors.selection_text    = OptionColorU::Some(color_from_sys(&u32_lib.GetSysColor, 14));
        style.colors.button_face       = OptionColorU::Some(color_from_sys(&u32_lib.GetSysColor, 15));
        style.colors.button_text       = OptionColorU::Some(color_from_sys(&u32_lib.GetSysColor, 18));
        style.colors.disabled_text     = OptionColorU::Some(color_from_sys(&u32_lib.GetSysColor, 17));

        // ── Text rendering hints ─────────────────────────────────────
        {
            let mut smoothing: i32 = 0;
            (u32_lib.SystemParametersInfoW)(
                SPI_GETFONTSMOOTHING, 0,
                &mut smoothing as *mut i32 as *mut c_void, 0,
            );
            let mut smooth_type: u32 = 0;
            (u32_lib.SystemParametersInfoW)(
                SPI_GETFONTSMOOTHINGTYPE, 0,
                &mut smooth_type as *mut u32 as *mut c_void, 0,
            );
            style.text_rendering = TextRenderingHints {
                font_smoothing_enabled: smoothing != 0,
                subpixel_type: if smooth_type == FE_FONTSMOOTHINGCLEARTYPE {
                    SubpixelType::Rgb // ClearType defaults to horizontal RGB
                } else {
                    SubpixelType::None
                },
                font_smoothing_gamma: 1000,
                increased_contrast: false,
            };
        }

        // ── DWM accent colour ────────────────────────────────────────
        if let Some(dwm) = Dwmapi::load() {
            let mut colorization: u32 = 0;
            let mut opaque_blend: i32 = 0;
            let hr = (dwm.DwmGetColorizationColor)(&mut colorization, &mut opaque_blend);
            if hr >= 0 {
                // DwmGetColorizationColor returns 0xAARRGGBB
                let a = ((colorization >> 24) & 0xFF) as u8;
                let r = ((colorization >> 16) & 0xFF) as u8;
                let g = ((colorization >> 8)  & 0xFF) as u8;
                let b = ( colorization        & 0xFF) as u8;
                style.colors.accent = OptionColorU::Some(ColorU::new(r, g, b, a));
            }
        }

        // ── Dark mode detection (registry-based, same as old `io` path)
        // We keep this simple: check HKCU\...\Personalize\AppsUseLightTheme
        // via the already-loaded SystemParametersInfoW path is not possible,
        // so we rely on the GetSysColor heuristic: if window background
        // luminance < 128, assume dark.
        if let Some(ref bg) = style.colors.window_background.as_option() {
            let luma = (bg.r as u16 + bg.g as u16 + bg.b as u16) / 3;
            if luma < 128 {
                style.theme = super::Theme::Dark;
            }
        }
    }

    style.platform = super::Platform::Windows;
    style
}
