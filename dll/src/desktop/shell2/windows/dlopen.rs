//! Dynamic loading of Win32 DLLs.
//!
//! This module provides safe abstractions for loading Win32 DLLs and their functions
//! using `dlopen`-style dynamic loading. This is necessary for cross-compilation
//! from macOS where the Win32 API is not available at link time.
//!
//! Safety: All function pointers are checked for null before being wrapped in Option types.

use std::sync::Arc;

use super::super::common::debug_server::LogCategory;
use super::super::common::{
    dlopen::DynamicLibrary as DynamicLibraryTrait, error::DlError,
};
use crate::{log_debug, log_error, log_info, log_trace, log_warn};

// Re-export types that will be used by Win32 API
pub type HINSTANCE = *mut std::ffi::c_void;
pub type HWND = *mut std::ffi::c_void;
pub type HDC = *mut std::ffi::c_void;
pub type HGLRC = *mut std::ffi::c_void;
pub type HMENU = *mut std::ffi::c_void;
pub type HMONITOR = *mut std::ffi::c_void;
pub type HICON = *mut std::ffi::c_void;
pub type HCURSOR = *mut std::ffi::c_void;
pub type HBRUSH = *mut std::ffi::c_void;
pub type HDROP = *mut std::ffi::c_void;
pub type HIMC = *mut std::ffi::c_void; // IME input context handle
pub type WPARAM = usize;
pub type LPARAM = isize;
pub type LRESULT = isize;
pub type BOOL = i32;
pub type UINT = u32;
pub type HRESULT = i32;
pub type ATOM = u16;

/// Window procedure callback type
pub type WNDPROC = Option<unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT>;

/// Win32 RECT structure
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct RECT {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl RECT {
    pub fn width(&self) -> u32 {
        (self.right - self.left).max(0) as u32
    }

    pub fn height(&self) -> u32 {
        (self.bottom - self.top).max(0) as u32
    }
}

// Win32 Constants
pub const PM_REMOVE: u32 = 0x0001;
pub const TME_LEAVE: u32 = 0x00000002;

/// Win32 POINT structure
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct POINT {
    pub x: i32,
    pub y: i32,
}

/// Win32 TRACKMOUSEEVENT structure
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct TRACKMOUSEEVENT {
    pub cbSize: u32,
    pub dwFlags: u32,
    pub hwndTrack: HWND,
    pub dwHoverTime: u32,
}

/// Win32 MSG structure
#[repr(C)]
pub struct MSG {
    pub hwnd: HWND,
    pub message: u32,
    pub wParam: WPARAM,
    pub lParam: LPARAM,
    pub time: u32,
    pub pt: POINT,
}

impl Default for MSG {
    fn default() -> Self {
        Self {
            hwnd: core::ptr::null_mut(),
            message: 0,
            wParam: 0,
            lParam: 0,
            time: 0,
            pt: POINT { x: 0, y: 0 },
        }
    }
}

/// Win32 WNDCLASSW structure
#[repr(C)]
pub struct WNDCLASSW {
    pub style: u32,
    pub lpfnWndProc: WNDPROC,
    pub cbClsExtra: i32,
    pub cbWndExtra: i32,
    pub hInstance: HINSTANCE,
    pub hIcon: HICON,
    pub hCursor: HCURSOR,
    pub hbrBackground: HBRUSH,
    pub lpszMenuName: *const u16,
    pub lpszClassName: *const u16,
}

/// Win32 COMPOSITIONFORM structure for IME composition window positioning
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct COMPOSITIONFORM {
    pub dwStyle: u32,
    pub ptCurrentPos: POINT,
    pub rcArea: RECT,
}

// IME Composition Form Styles
pub const CFS_RECT: u32 = 0x0001;

/// Helper to encode ASCII string for GetProcAddress
pub fn encode_ascii(input: &str) -> Vec<i8> {
    debug_assert!(input.is_ascii(), "encode_ascii called with non-ASCII input: {}", input);
    input
        .chars()
        .filter(|c| c.is_ascii())
        .map(|c| c as i8)
        .chain(Some(0).into_iter())
        .collect::<Vec<_>>()
}

/// Helper to encode wide string for Win32 APIs
pub fn encode_wide(input: &str) -> Vec<u16> {
    input
        .encode_utf16()
        .chain(Some(0).into_iter())
        .collect::<Vec<_>>()
}

// Win32 Constants
pub mod constants {
    // Window Styles
    pub const WS_OVERLAPPED: u32 = 0x00000000;
    pub const WS_CAPTION: u32 = 0x00C00000;
    pub const WS_SYSMENU: u32 = 0x00080000;
    pub const WS_THICKFRAME: u32 = 0x00040000;
    pub const WS_MINIMIZEBOX: u32 = 0x00020000;
    pub const WS_MAXIMIZEBOX: u32 = 0x00010000;
    /// Same value as WS_MAXIMIZEBOX per Win32 API — they share the same bit
    /// but apply in different contexts (child vs. overlapped windows).
    pub const WS_TABSTOP: u32 = 0x00010000;
    pub const WS_POPUP: u32 = 0x80000000;

    // Combined Window Style
    // WS_OVERLAPPEDWINDOW = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_THICKFRAME |
    // WS_MINIMIZEBOX | WS_MAXIMIZEBOX
    pub const WS_OVERLAPPEDWINDOW: u32 =
        WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_THICKFRAME | WS_MINIMIZEBOX | WS_MAXIMIZEBOX;

    // Extended Window Styles
    pub const WS_EX_APPWINDOW: u32 = 0x00040000;
    pub const WS_EX_ACCEPTFILES: u32 = 0x00000010;

    // Window Class Styles
    pub const CS_HREDRAW: u32 = 0x0002;
    pub const CS_VREDRAW: u32 = 0x0001;
    pub const CS_OWNDC: u32 = 0x0020;

    // ShowWindow Commands
    pub const SW_HIDE: i32 = 0;
    pub const SW_SHOWNORMAL: i32 = 1;
    pub const SW_NORMAL: i32 = 1;
    pub const SW_SHOW: i32 = 5;
    pub const SW_MINIMIZE: i32 = 6;
    pub const SW_MAXIMIZE: i32 = 3;
    pub const SW_RESTORE: i32 = 9;

    // SetWindowPos flags
    pub const SWP_NOSIZE: u32 = 0x0001;
    pub const SWP_NOMOVE: u32 = 0x0002;
    pub const SWP_NOZORDER: u32 = 0x0004;
    pub const SWP_FRAMECHANGED: u32 = 0x0020;

    // Cursor constants (for LoadCursorW)
    pub const IDC_ARROW: *const u16 = 32512 as *const u16;
    pub const IDC_IBEAM: *const u16 = 32513 as *const u16;
    pub const IDC_WAIT: *const u16 = 32514 as *const u16;
    pub const IDC_CROSS: *const u16 = 32515 as *const u16;
    pub const IDC_UPARROW: *const u16 = 32516 as *const u16;
    pub const IDC_SIZE: *const u16 = 32640 as *const u16;
    pub const IDC_ICON: *const u16 = 32641 as *const u16;
    pub const IDC_SIZENWSE: *const u16 = 32642 as *const u16;
    pub const IDC_SIZENESW: *const u16 = 32643 as *const u16;
    pub const IDC_SIZEWE: *const u16 = 32644 as *const u16;
    pub const IDC_SIZENS: *const u16 = 32645 as *const u16;
    pub const IDC_SIZEALL: *const u16 = 32646 as *const u16;
    pub const IDC_NO: *const u16 = 32648 as *const u16;
    pub const IDC_HAND: *const u16 = 32649 as *const u16;
    pub const IDC_APPSTARTING: *const u16 = 32650 as *const u16;
    pub const IDC_HELP: *const u16 = 32651 as *const u16;

    // GetWindowLongPtr indices
    pub const GWLP_USERDATA: i32 = -21;
    pub const GWL_STYLE: i32 = -16;
    pub const GWL_EXSTYLE: i32 = -20;

    // Special constants
    pub const CW_USEDEFAULT: i32 = 0x80000000_u32 as i32;
    pub const HWND_TOP: *mut core::ffi::c_void = 0 as *mut core::ffi::c_void;
    pub const HWND_TOPMOST: *mut core::ffi::c_void = -1isize as *mut core::ffi::c_void;
    pub const HWND_NOTOPMOST: *mut core::ffi::c_void = -2isize as *mut core::ffi::c_void;

    // Menu flags
    pub const MF_STRING: u32 = 0x00000000;
    pub const MF_POPUP: u32 = 0x00000010;
    pub const MF_SEPARATOR: u32 = 0x00000800;
    pub const MF_MENUBREAK: u32 = 0x00000040;

    // TrackPopupMenu flags
    pub const TPM_LEFTALIGN: u32 = 0x0000;
    pub const TPM_TOPALIGN: u32 = 0x0000;
    pub const TPM_RIGHTBUTTON: u32 = 0x0008;
}

#[cfg(target_os = "windows")]
mod windows_impl {
    use super::*;

    pub unsafe fn load_library(name: &str) -> Option<HINSTANCE> {
        use winapi::um::libloaderapi::LoadLibraryW;
        let mut dll_name = encode_wide(name);
        let dll = LoadLibraryW(dll_name.as_mut_ptr());
        if dll.is_null() {
            None
        } else {
            Some(dll as HINSTANCE)
        }
    }

    pub unsafe fn get_proc_address(dll: HINSTANCE, name: &str) -> Option<*mut std::ffi::c_void> {
        use winapi::um::libloaderapi::GetProcAddress;
        let mut func_name = encode_ascii(name);
        let addr = GetProcAddress(
            dll as winapi::shared::minwindef::HINSTANCE,
            func_name.as_mut_ptr(),
        );
        if addr.is_null() {
            None
        } else {
            Some(addr as *mut std::ffi::c_void)
        }
    }

    pub unsafe fn free_library(dll: HINSTANCE) {
        use winapi::um::libloaderapi::FreeLibrary;
        FreeLibrary(dll as winapi::shared::minwindef::HINSTANCE);
    }
}

#[cfg(not(target_os = "windows"))]
mod windows_impl {
    use super::*;

    // Stub implementations for non-Windows platforms (for cross-compilation)
    pub unsafe fn load_library(_name: &str) -> Option<HINSTANCE> {
        log_warn!(
            LogCategory::Platform,
            "WARNING: Attempted to load Win32 DLL on non-Windows platform"
        );
        None
    }

    pub unsafe fn get_proc_address(_dll: HINSTANCE, _name: &str) -> Option<*mut std::ffi::c_void> {
        None
    }

    pub unsafe fn free_library(_dll: HINSTANCE) {}
}

/// Wrapper for a dynamically loaded DLL
pub struct DynamicLibrary {
    handle: Option<HINSTANCE>,
    name: String,
}

impl DynamicLibraryTrait for DynamicLibrary {
    fn load(name: &str) -> Result<Self, DlError> {
        unsafe {
            match windows_impl::load_library(name) {
                Some(h) => Ok(Self {
                    handle: Some(h),
                    name: name.to_string(),
                }),
                None => Err(DlError::LibraryNotFound {
                    name: name.to_string(),
                    tried: vec![name.to_string()],
                    suggestion: format!(
                        "LoadLibraryW failed for '{}'. The DLL may be missing or \
                         architecturally incompatible.",
                        name
                    ),
                }),
            }
        }
    }

    unsafe fn get_symbol<T>(&self, name: &str) -> Result<T, DlError> {
        debug_assert_eq!(
            std::mem::size_of::<T>(),
            std::mem::size_of::<*mut core::ffi::c_void>(),
            "get_symbol: T must be pointer-sized"
        );
        let handle = self.handle.ok_or_else(|| DlError::SymbolNotFound {
            symbol: name.to_string(),
            library: self.name.clone(),
            suggestion: "DLL handle is null".to_string(),
        })?;
        let addr = windows_impl::get_proc_address(handle, name).ok_or_else(|| {
            DlError::SymbolNotFound {
                symbol: name.to_string(),
                library: self.name.clone(),
                suggestion: format!("GetProcAddress returned NULL for '{}'", name),
            }
        })?;
        Ok(std::mem::transmute_copy(&addr))
    }

    fn unload(&mut self) {
        if let Some(handle) = self.handle.take() {
            unsafe { windows_impl::free_library(handle) };
        }
    }
}

impl DynamicLibrary {
    /// Convenience wrapper around the trait `load` to keep the inherent-method
    /// call sites readable (`DynamicLibrary::load("user32.dll")`).
    pub fn load(name: &str) -> Result<Self, DlError> {
        <Self as DynamicLibraryTrait>::load(name)
    }

    /// Get the DLL handle
    pub fn handle(&self) -> Option<HINSTANCE> {
        self.handle
    }
}

impl Drop for DynamicLibrary {
    fn drop(&mut self) {
        <Self as DynamicLibraryTrait>::unload(self);
    }
}

/// Win32 user32.dll function pointers
#[derive(Copy, Clone)]
pub struct User32Functions {
    // Menu functions
    pub CreateMenu: unsafe extern "system" fn() -> HMENU,
    pub CreatePopupMenu: unsafe extern "system" fn() -> HMENU,
    pub AppendMenuW: unsafe extern "system" fn(HMENU, u32, usize, *const u16) -> i32,
    pub SetMenu: unsafe extern "system" fn(HWND, HMENU) -> i32,
    pub DrawMenuBar: unsafe extern "system" fn(HWND) -> i32,
    pub DestroyMenu: unsafe extern "system" fn(HMENU) -> i32,
    pub TrackPopupMenu:
        unsafe extern "system" fn(HMENU, u32, i32, i32, i32, HWND, *const core::ffi::c_void) -> i32,
    pub SetForegroundWindow: unsafe extern "system" fn(HWND) -> i32,

    // Window creation and management
    pub CreateWindowExW: unsafe extern "system" fn(
        u32,        // dwExStyle
        *const u16, // lpClassName
        *const u16, // lpWindowName
        u32,        // dwStyle
        i32,
        i32, // x, y
        i32,
        i32,                    // width, height
        HWND,                   // hWndParent
        HMENU,                  // hMenu
        HINSTANCE,              // hInstance
        *mut core::ffi::c_void, // lpParam
    ) -> HWND,
    pub DestroyWindow: unsafe extern "system" fn(HWND) -> BOOL,
    pub ShowWindow: unsafe extern "system" fn(HWND, i32) -> BOOL,
    pub UpdateWindow: unsafe extern "system" fn(HWND) -> BOOL,
    pub SetWindowPos: unsafe extern "system" fn(HWND, HWND, i32, i32, i32, i32, u32) -> BOOL,
    pub GetClientRect: unsafe extern "system" fn(HWND, *mut RECT) -> BOOL,
    pub GetWindowRect: unsafe extern "system" fn(HWND, *mut RECT) -> BOOL,
    pub InvalidateRect: unsafe extern "system" fn(HWND, *const RECT, BOOL) -> BOOL,

    // Window properties
    pub SetWindowLongPtrW: unsafe extern "system" fn(HWND, i32, isize) -> isize,
    pub GetWindowLongPtrW: unsafe extern "system" fn(HWND, i32) -> isize,
    pub SetWindowTextW: unsafe extern "system" fn(HWND, *const u16) -> BOOL,

    // Window class registration
    pub RegisterClassW: unsafe extern "system" fn(*const WNDCLASSW) -> ATOM,
    pub DefWindowProcW: unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT,

    // Device context
    pub GetDC: unsafe extern "system" fn(HWND) -> HDC,
    pub ReleaseDC: unsafe extern "system" fn(HWND, HDC) -> i32,

    // Cursor and position
    pub GetCursorPos: unsafe extern "system" fn(*mut POINT) -> BOOL,
    pub ScreenToClient: unsafe extern "system" fn(HWND, *mut POINT) -> BOOL,
    pub ClientToScreen: unsafe extern "system" fn(HWND, *mut POINT) -> BOOL,
    pub SetCapture: unsafe extern "system" fn(HWND) -> HWND,
    pub ReleaseCapture: unsafe extern "system" fn() -> BOOL,
    pub LoadCursorW: unsafe extern "system" fn(HINSTANCE, *const u16) -> HCURSOR,
    pub SetCursor: unsafe extern "system" fn(HCURSOR) -> HCURSOR,
    pub TrackMouseEvent: unsafe extern "system" fn(*mut TRACKMOUSEEVENT) -> BOOL,

    // Pointer input (touch + pen, Windows 8+). Optional — None on older Windows.
    pub GetPointerType: Option<unsafe extern "system" fn(u32, *mut u32) -> BOOL>,
    pub GetPointerPenInfo:
        Option<unsafe extern "system" fn(u32, *mut winapi::um::winuser::POINTER_PEN_INFO) -> BOOL>,
    pub GetPointerTouchInfo:
        Option<unsafe extern "system" fn(u32, *mut winapi::um::winuser::POINTER_TOUCH_INFO) -> BOOL>,

    // Messages
    pub SendMessageW: unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT,
    pub PostMessageW: unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> BOOL,
    pub GetMessageW: unsafe extern "system" fn(*mut MSG, HWND, u32, u32) -> BOOL,
    pub PeekMessageW: unsafe extern "system" fn(*mut MSG, HWND, u32, u32, u32) -> BOOL,
    pub TranslateMessage: unsafe extern "system" fn(*const MSG) -> BOOL,
    pub DispatchMessageW: unsafe extern "system" fn(*const MSG) -> LRESULT,
    pub WaitMessage: unsafe extern "system" fn() -> BOOL,

    // Timers
    pub SetTimer: unsafe extern "system" fn(HWND, usize, u32, *const core::ffi::c_void) -> usize,
    pub KillTimer: unsafe extern "system" fn(HWND, usize) -> BOOL,
}

/// Win32 gdi32.dll function pointers for brushes, regions, and pixel blitting
#[derive(Copy, Clone)]
pub struct Gdi32Functions {
    pub CreateSolidBrush: unsafe extern "system" fn(u32) -> HBRUSH,
    pub DeleteObject: unsafe extern "system" fn(*mut core::ffi::c_void) -> BOOL,
    /// Create a rectangular region - used for DwmEnableBlurBehindWindow
    /// CreateRectRgn(0, 0, -1, -1) creates a minimal region for transparent backgrounds
    pub CreateRectRgn: unsafe extern "system" fn(i32, i32, i32, i32) -> HRGN,
    /// StretchDIBits - blit pixel data from memory to device context (for CPU rendering)
    /// (hdc, xDest, yDest, wDest, hDest, xSrc, ySrc, wSrc, hSrc, lpBits, lpBmi, iUsage, dwRop)
    pub StretchDIBits: unsafe extern "system" fn(
        HDC, i32, i32, i32, i32, i32, i32, i32, i32,
        *const core::ffi::c_void, *const BitmapInfoHeader, u32, u32,
    ) -> i32,
}

/// BITMAPINFOHEADER for StretchDIBits - describes pixel format of source data
#[repr(C)]
#[derive(Copy, Clone)]
pub struct BitmapInfoHeader {
    pub biSize: u32,
    pub biWidth: i32,
    pub biHeight: i32,       // negative = top-down DIB
    pub biPlanes: u16,
    pub biBitCount: u16,
    pub biCompression: u32,  // BI_RGB = 0
    pub biSizeImage: u32,
    pub biXPelsPerMeter: i32,
    pub biYPelsPerMeter: i32,
    pub biClrUsed: u32,
    pub biClrImportant: u32,
}

/// DIB_RGB_COLORS - color table contains literal RGB values
pub const DIB_RGB_COLORS: u32 = 0;
/// SRCCOPY raster operation - copy source to destination
pub const SRCCOPY: u32 = 0x00CC0020;

/// HRGN type for region handles
pub type HRGN = *mut core::ffi::c_void;

/// Win32 imm32.dll function pointers for IME (Input Method Editor)
#[derive(Copy, Clone)]
pub struct Imm32Functions {
    pub ImmGetContext: unsafe extern "system" fn(HWND) -> HIMC,
    pub ImmReleaseContext: unsafe extern "system" fn(HWND, HIMC) -> BOOL,
    pub ImmGetCompositionStringW:
        unsafe extern "system" fn(HIMC, u32, *mut core::ffi::c_void, u32) -> i32,
    pub ImmSetCompositionWindow: unsafe extern "system" fn(HIMC, *const COMPOSITIONFORM) -> BOOL,
    /// MWA-C-text_input: associate/dissociate the IME context per editable
    /// focus (NULL HIMC = IME disabled for the window). Returns the
    /// previously associated context.
    pub ImmAssociateContext: unsafe extern "system" fn(HWND, HIMC) -> HIMC,
}

/// Win32 shell32.dll function pointers for drag-and-drop
#[derive(Copy, Clone)]
pub struct Shell32Functions {
    pub DragAcceptFiles: unsafe extern "system" fn(HWND, BOOL),
    pub DragQueryFileW: unsafe extern "system" fn(HDROP, UINT, *mut u16, UINT) -> UINT,
    pub DragQueryPoint: unsafe extern "system" fn(HDROP, *mut POINT) -> BOOL,
    pub DragFinish: unsafe extern "system" fn(HDROP),
}

/// Win32 kernel32.dll function pointers for power management and module handles
#[derive(Copy, Clone)]
pub struct Kernel32Functions {
    pub SetThreadExecutionState: unsafe extern "system" fn(u32) -> u32,
    pub GetModuleHandleW: unsafe extern "system" fn(*const u16) -> HINSTANCE,
}

// ============================================================================
// DWM (Desktop Window Manager) API for Windows 11 transparency effects
// ============================================================================

/// DWM_SYSTEMBACKDROP_TYPE enum (Windows 11 22H2+)
/// Used with DWMWA_SYSTEMBACKDROP_TYPE to set Mica/Acrylic effects
#[repr(i32)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum DWM_SYSTEMBACKDROP_TYPE {
    /// Let DWM automatically decide the system backdrop
    DWMSBT_AUTO = 0,
    /// No system backdrop (use for transparent windows)
    DWMSBT_NONE = 1,
    /// Mica effect (main window material)
    DWMSBT_MAINWINDOW = 2,
    /// Acrylic effect (transient/popup material)
    DWMSBT_TRANSIENTWINDOW = 3,
    /// Mica Alt effect (tabbed window material)
    DWMSBT_TABBEDWINDOW = 4,
}

/// DWMWINDOWATTRIBUTE constants for DwmSetWindowAttribute
pub const DWMWA_USE_IMMERSIVE_DARK_MODE: u32 = 20;
pub const DWMWA_SYSTEMBACKDROP_TYPE: u32 = 38;

/// MARGINS structure for DwmExtendFrameIntoClientArea
#[repr(C)]
#[derive(Debug, Copy, Clone, Default)]
pub struct MARGINS {
    pub cxLeftWidth: i32,
    pub cxRightWidth: i32,
    pub cyTopHeight: i32,
    pub cyBottomHeight: i32,
}

impl MARGINS {
    /// Create margins that extend the frame into the entire client area
    /// This is required for Mica/Acrylic effects to work
    pub const fn full_window() -> Self {
        Self {
            cxLeftWidth: -1,
            cxRightWidth: -1,
            cyTopHeight: -1,
            cyBottomHeight: -1,
        }
    }
}

/// DWM_BLURBEHIND structure for DwmEnableBlurBehindWindow
/// Used to achieve true transparent background with OpenGL
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct DWM_BLURBEHIND {
    pub dwFlags: u32,
    pub fEnable: i32,                     // BOOL
    pub hRgnBlur: *mut core::ffi::c_void, // HRGN
    pub fTransitionOnMaximized: i32,      // BOOL
}

impl Default for DWM_BLURBEHIND {
    fn default() -> Self {
        Self {
            dwFlags: 0,
            fEnable: 0,
            hRgnBlur: std::ptr::null_mut(),
            fTransitionOnMaximized: 0,
        }
    }
}

/// DWM_BB flags for DWM_BLURBEHIND.dwFlags
pub const DWM_BB_ENABLE: u32 = 0x00000001;
pub const DWM_BB_BLURREGION: u32 = 0x00000002;
pub const DWM_BB_TRANSITIONONMAXIMIZED: u32 = 0x00000004;

/// Dwmapi.dll function pointers for Desktop Window Manager effects
#[derive(Copy, Clone)]
pub struct DwmapiFunctions {
    /// Set window attributes (backdrop type, dark mode, etc.)
    pub DwmSetWindowAttribute: unsafe extern "system" fn(
        hwnd: HWND,
        dwAttribute: u32,
        pvAttribute: *const core::ffi::c_void,
        cbAttribute: u32,
    ) -> HRESULT,
    /// Extend the window frame into the client area (required for Mica/Acrylic)
    pub DwmExtendFrameIntoClientArea:
        unsafe extern "system" fn(hwnd: HWND, pMarInset: *const MARGINS) -> HRESULT,
    /// Enable blur-behind effect for transparent backgrounds
    /// This is the key function for making OpenGL windows transparent
    pub DwmEnableBlurBehindWindow:
        unsafe extern "system" fn(hwnd: HWND, pBlurBehind: *const DWM_BLURBEHIND) -> HRESULT,
    /// Flush the DWM compositor - blocks until the current frame is presented
    /// Critical for avoiding black flash when showing window after first render
    pub DwmFlush: unsafe extern "system" fn() -> HRESULT,
}

/// Pre-load commonly used Win32 DLLs.
///
/// DLL handles are wrapped in [`Arc`] so cheap clones (e.g. handing a copy
/// to a tooltip window) keep the underlying libraries alive until every
/// holder has dropped. The previous design replaced cloned handles with
/// `None`, which left function pointers dangling once the original was
/// dropped — a use-after-free risk fixed by refcounting here.
#[derive(Clone)]
pub struct Win32Libraries {
    pub user32_dll: Option<Arc<DynamicLibrary>>,
    pub user32: User32Functions,
    pub gdi32_dll: Option<Arc<DynamicLibrary>>,
    pub gdi32: Gdi32Functions,
    pub imm32_dll: Option<Arc<DynamicLibrary>>,
    pub imm32: Option<Imm32Functions>,
    pub shell32_dll: Option<Arc<DynamicLibrary>>,
    pub shell32: Option<Shell32Functions>,
    pub kernel32_dll: Option<Arc<DynamicLibrary>>,
    pub kernel32: Option<Kernel32Functions>,
    pub dwmapi: Option<Arc<DynamicLibrary>>,
    /// DWM functions for Windows 11 transparency effects (Mica, Acrylic)
    pub dwmapi_funcs: Option<DwmapiFunctions>,
}

impl Win32Libraries {
    pub fn load() -> Result<Self, DlError> {
        let user32_dll = DynamicLibrary::load("user32.dll")?;
        let gdi32_dll = DynamicLibrary::load("gdi32.dll")?;

        // Load function pointers from user32.dll
        let user32 = unsafe {
            User32Functions {
                // Menu functions
                CreateMenu: user32_dll.get_symbol("CreateMenu")?,
                CreatePopupMenu: user32_dll.get_symbol("CreatePopupMenu")?,
                AppendMenuW: user32_dll.get_symbol("AppendMenuW")?,
                SetMenu: user32_dll.get_symbol("SetMenu")?,
                DrawMenuBar: user32_dll.get_symbol("DrawMenuBar")?,
                DestroyMenu: user32_dll.get_symbol("DestroyMenu")?,
                TrackPopupMenu: user32_dll.get_symbol("TrackPopupMenu")?,
                SetForegroundWindow: user32_dll.get_symbol("SetForegroundWindow")?,

                // Window creation
                CreateWindowExW: user32_dll.get_symbol("CreateWindowExW")?,
                DestroyWindow: user32_dll.get_symbol("DestroyWindow")?,
                ShowWindow: user32_dll.get_symbol("ShowWindow")?,
                UpdateWindow: user32_dll.get_symbol("UpdateWindow")?,
                SetWindowPos: user32_dll.get_symbol("SetWindowPos")?,
                GetClientRect: user32_dll.get_symbol("GetClientRect")?,
                GetWindowRect: user32_dll.get_symbol("GetWindowRect")?,
                InvalidateRect: user32_dll.get_symbol("InvalidateRect")?,

                // Window properties
                // SetWindowLongPtrW/GetWindowLongPtrW are 64-bit-aware wrappers
                // that only exist as real exports on 64-bit Windows. On 32-bit
                // Windows (including Win9x/XP), they're #defines to SetWindowLongW/
                // GetWindowLongW. Fall back to the non-Ptr versions for compat.
                SetWindowLongPtrW: user32_dll
                    .get_symbol("SetWindowLongPtrW")
                    .or_else(|_| user32_dll.get_symbol("SetWindowLongW"))?,
                GetWindowLongPtrW: user32_dll
                    .get_symbol("GetWindowLongPtrW")
                    .or_else(|_| user32_dll.get_symbol("GetWindowLongW"))?,
                SetWindowTextW: user32_dll.get_symbol("SetWindowTextW")?,

                // Window class
                RegisterClassW: user32_dll.get_symbol("RegisterClassW")?,
                DefWindowProcW: user32_dll.get_symbol("DefWindowProcW")?,

                // Device context
                GetDC: user32_dll.get_symbol("GetDC")?,
                ReleaseDC: user32_dll.get_symbol("ReleaseDC")?,

                // Cursor
                GetCursorPos: user32_dll.get_symbol("GetCursorPos")?,
                ScreenToClient: user32_dll.get_symbol("ScreenToClient")?,
                ClientToScreen: user32_dll.get_symbol("ClientToScreen")?,
                SetCapture: user32_dll.get_symbol("SetCapture")?,
                ReleaseCapture: user32_dll.get_symbol("ReleaseCapture")?,
                LoadCursorW: user32_dll.get_symbol("LoadCursorW")?,
                SetCursor: user32_dll.get_symbol("SetCursor")?,
                TrackMouseEvent: user32_dll.get_symbol("TrackMouseEvent")?,
                GetPointerType: user32_dll.get_symbol("GetPointerType").ok(),
                GetPointerPenInfo: user32_dll.get_symbol("GetPointerPenInfo").ok(),
                GetPointerTouchInfo: user32_dll.get_symbol("GetPointerTouchInfo").ok(),

                // Messages
                SendMessageW: user32_dll.get_symbol("SendMessageW")?,
                PostMessageW: user32_dll.get_symbol("PostMessageW")?,
                GetMessageW: user32_dll.get_symbol("GetMessageW")?,
                PeekMessageW: user32_dll.get_symbol("PeekMessageW")?,
                TranslateMessage: user32_dll.get_symbol("TranslateMessage")?,
                DispatchMessageW: user32_dll.get_symbol("DispatchMessageW")?,
                WaitMessage: user32_dll.get_symbol("WaitMessage")?,

                // Timers
                SetTimer: user32_dll.get_symbol("SetTimer")?,
                KillTimer: user32_dll.get_symbol("KillTimer")?,
            }
        };

        // Load function pointers from gdi32.dll
        let gdi32 = unsafe {
            Gdi32Functions {
                CreateSolidBrush: gdi32_dll.get_symbol("CreateSolidBrush")?,
                DeleteObject: gdi32_dll.get_symbol("DeleteObject")?,
                CreateRectRgn: gdi32_dll.get_symbol("CreateRectRgn")?,
                StretchDIBits: gdi32_dll.get_symbol("StretchDIBits")?,
            }
        };

        // Try to load function pointers from shell32.dll (optional - for drag-and-drop)
        let shell32_dll = DynamicLibrary::load("shell32.dll").ok();
        let shell32 = shell32_dll.as_ref().and_then(|dll| unsafe {
            Some(Shell32Functions {
                DragAcceptFiles: dll.get_symbol("DragAcceptFiles").ok()?,
                DragQueryFileW: dll.get_symbol("DragQueryFileW").ok()?,
                DragQueryPoint: dll.get_symbol("DragQueryPoint").ok()?,
                DragFinish: dll.get_symbol("DragFinish").ok()?,
            })
        });

        // Try to load function pointers from imm32.dll (optional - for IME)
        let imm32_dll = DynamicLibrary::load("imm32.dll").ok();
        let imm32 = imm32_dll.as_ref().and_then(|dll| unsafe {
            Some(Imm32Functions {
                ImmGetContext: dll.get_symbol("ImmGetContext").ok()?,
                ImmReleaseContext: dll.get_symbol("ImmReleaseContext").ok()?,
                ImmGetCompositionStringW: dll.get_symbol("ImmGetCompositionStringW").ok()?,
                ImmSetCompositionWindow: dll.get_symbol("ImmSetCompositionWindow").ok()?,
                ImmAssociateContext: dll.get_symbol("ImmAssociateContext").ok()?,
            })
        });

        // Try to load function pointers from kernel32.dll (optional - for power management)
        let kernel32_dll = DynamicLibrary::load("kernel32.dll").ok();
        let kernel32 = kernel32_dll.as_ref().and_then(|dll| unsafe {
            Some(Kernel32Functions {
                SetThreadExecutionState: dll.get_symbol("SetThreadExecutionState").ok()?,
                GetModuleHandleW: dll.get_symbol("GetModuleHandleW").ok()?,
            })
        });

        // Try to load function pointers from dwmapi.dll (optional - for Windows 11 transparency)
        let dwmapi = DynamicLibrary::load("dwmapi.dll").ok();
        let dwmapi_funcs = if let Some(ref dll) = dwmapi {
            unsafe {
                let funcs = (|| -> Option<DwmapiFunctions> {
                    Some(DwmapiFunctions {
                        DwmSetWindowAttribute: dll.get_symbol("DwmSetWindowAttribute").ok()?,
                        DwmExtendFrameIntoClientArea: dll
                            .get_symbol("DwmExtendFrameIntoClientArea")
                            .ok()?,
                        DwmEnableBlurBehindWindow: dll
                            .get_symbol("DwmEnableBlurBehindWindow")
                            .ok()?,
                        DwmFlush: dll.get_symbol("DwmFlush").ok()?,
                    })
                })();
                if funcs.is_some() {
                    log_debug!(
                        LogCategory::Platform,
                        "Loaded dwmapi.dll - DWM transparency effects available"
                    );
                } else {
                    log_debug!(
                        LogCategory::Platform,
                        "dwmapi.dll loaded but DWM functions not found"
                    );
                }
                funcs
            }
        } else {
            None
        };

        Ok(Self {
            user32_dll: Some(Arc::new(user32_dll)),
            user32,
            gdi32_dll: Some(Arc::new(gdi32_dll)),
            gdi32,
            imm32_dll: imm32_dll.map(Arc::new),
            imm32,
            shell32_dll: shell32_dll.map(Arc::new),
            shell32,
            kernel32_dll: kernel32_dll.map(Arc::new),
            kernel32,
            dwmapi: dwmapi.map(Arc::new),
            dwmapi_funcs,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(target_os = "windows")]
    #[cfg_attr(miri, ignore)] // Miri doesn't support LoadLibraryW
    fn test_load_user32() {
        let lib = DynamicLibrary::load("user32.dll").unwrap();
        assert!(lib.handle().is_some());
    }
}
