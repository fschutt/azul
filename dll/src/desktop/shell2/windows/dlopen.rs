//! Dynamic loading of Win32 DLLs.
//!
//! This module provides safe abstractions for loading Win32 DLLs and their functions
//! using `dlopen`-style dynamic loading. This is necessary for cross-compilation
//! from macOS where the Win32 API is not available at link time.
//!
//! Safety: All function pointers are checked for null before being wrapped in Option types.

use std::{ffi::CString, ptr};

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

/// Win32 POINT structure
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct POINT {
    pub x: i32,
    pub y: i32,
}

/// Win32 MSG structure
#[repr(C)]
#[derive(Default)]
pub struct MSG {
    pub hwnd: HWND,
    pub message: u32,
    pub wParam: WPARAM,
    pub lParam: LPARAM,
    pub time: u32,
    pub pt: POINT,
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

/// Helper to encode ASCII string for GetProcAddress
pub fn encode_ascii(input: &str) -> Vec<i8> {
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
    pub const WS_TABSTOP: u32 = 0x00010000;
    pub const WS_POPUP: u32 = 0x80000000;

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
    pub const SW_MINIMIZE: i32 = 6;
    pub const SW_MAXIMIZE: i32 = 3;

    // SetWindowPos flags
    pub const SWP_NOMOVE: u32 = 0x0002;
    pub const SWP_NOZORDER: u32 = 0x0004;
    pub const SWP_FRAMECHANGED: u32 = 0x0020;

    // GetWindowLongPtr indices
    pub const GWLP_USERDATA: i32 = -21;

    // Special constants
    pub const CW_USEDEFAULT: i32 = 0x80000000_u32 as i32;
    pub const HWND_TOP: *mut core::ffi::c_void = 0 as *mut core::ffi::c_void;
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
        eprintln!("WARNING: Attempted to load Win32 DLL on non-Windows platform");
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

impl DynamicLibrary {
    /// Load a DLL by name
    pub fn load(name: &str) -> Result<Self, String> {
        unsafe {
            let handle = windows_impl::load_library(name);
            if handle.is_none() {
                return Err(format!("Failed to load DLL: {}", name));
            }
            Ok(Self {
                handle,
                name: name.to_string(),
            })
        }
    }

    /// Get a function pointer from the loaded DLL
    pub unsafe fn get_symbol<T>(&self, name: &str) -> Option<T>
    where
        T: Copy,
    {
        let handle = self.handle?;
        let addr = windows_impl::get_proc_address(handle, name)?;
        Some(std::mem::transmute_copy(&addr))
    }

    /// Get the DLL handle
    pub fn handle(&self) -> Option<HINSTANCE> {
        self.handle
    }
}

impl Drop for DynamicLibrary {
    fn drop(&mut self) {
        if let Some(handle) = self.handle {
            unsafe {
                windows_impl::free_library(handle);
            }
        }
    }
}

/// Win32 user32.dll function pointers
pub struct User32Functions {
    // Menu functions
    pub CreateMenu: unsafe extern "system" fn() -> HMENU,
    pub CreatePopupMenu: unsafe extern "system" fn() -> HMENU,
    pub AppendMenuW: unsafe extern "system" fn(HMENU, u32, usize, *const u16) -> i32,
    pub SetMenu: unsafe extern "system" fn(HWND, HMENU) -> i32,
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
    pub SetWindowPos: unsafe extern "system" fn(HWND, HWND, i32, i32, i32, i32, u32) -> BOOL,
    pub GetClientRect: unsafe extern "system" fn(HWND, *mut RECT) -> BOOL,
    pub GetWindowRect: unsafe extern "system" fn(HWND, *mut RECT) -> BOOL,
    pub InvalidateRect: unsafe extern "system" fn(HWND, *const RECT, BOOL) -> BOOL,

    // Window properties
    pub SetWindowLongPtrW: unsafe extern "system" fn(HWND, i32, isize) -> isize,
    pub GetWindowLongPtrW: unsafe extern "system" fn(HWND, i32) -> isize,

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

    // Messages
    pub PostMessageW: unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> BOOL,
    pub GetMessageW: unsafe extern "system" fn(*mut MSG, HWND, u32, u32) -> BOOL,
    pub TranslateMessage: unsafe extern "system" fn(*const MSG) -> BOOL,
    pub DispatchMessageW: unsafe extern "system" fn(*const MSG) -> LRESULT,

    // Module handle
    pub GetModuleHandleW: unsafe extern "system" fn(*const u16) -> HINSTANCE,

    // Timers
    pub SetTimer: unsafe extern "system" fn(HWND, usize, u32, *const core::ffi::c_void) -> usize,
    pub KillTimer: unsafe extern "system" fn(HWND, usize) -> BOOL,
}

/// Win32 gdi32.dll function pointers
#[derive(Copy, Clone)]
pub struct Gdi32Functions {
    pub CreateSolidBrush: unsafe extern "system" fn(u32) -> HBRUSH,
    pub DeleteObject: unsafe extern "system" fn(*mut core::ffi::c_void) -> BOOL,
}

/// Pre-load commonly used Win32 DLLs
pub struct Win32Libraries {
    pub user32_dll: Option<DynamicLibrary>,
    pub user32: User32Functions,
    pub gdi32_dll: Option<DynamicLibrary>,
    pub gdi32: Gdi32Functions,
    pub opengl32: Option<DynamicLibrary>,
    pub dwmapi: Option<DynamicLibrary>,
}

impl Win32Libraries {
    pub fn load() -> Result<Self, String> {
        let user32_dll = DynamicLibrary::load("user32.dll")?;
        let gdi32_dll = DynamicLibrary::load("gdi32.dll")?;

        // Load function pointers from user32.dll
        let user32 = unsafe {
            User32Functions {
                // Menu functions
                CreateMenu: user32_dll
                    .get_symbol("CreateMenu")
                    .ok_or_else(|| "CreateMenu not found".to_string())?,
                CreatePopupMenu: user32_dll
                    .get_symbol("CreatePopupMenu")
                    .ok_or_else(|| "CreatePopupMenu not found".to_string())?,
                AppendMenuW: user32_dll
                    .get_symbol("AppendMenuW")
                    .ok_or_else(|| "AppendMenuW not found".to_string())?,
                SetMenu: user32_dll
                    .get_symbol("SetMenu")
                    .ok_or_else(|| "SetMenu not found".to_string())?,
                DestroyMenu: user32_dll
                    .get_symbol("DestroyMenu")
                    .ok_or_else(|| "DestroyMenu not found".to_string())?,
                TrackPopupMenu: user32_dll
                    .get_symbol("TrackPopupMenu")
                    .ok_or_else(|| "TrackPopupMenu not found".to_string())?,
                SetForegroundWindow: user32_dll
                    .get_symbol("SetForegroundWindow")
                    .ok_or_else(|| "SetForegroundWindow not found".to_string())?,

                // Window creation
                CreateWindowExW: user32_dll
                    .get_symbol("CreateWindowExW")
                    .ok_or_else(|| "CreateWindowExW not found".to_string())?,
                DestroyWindow: user32_dll
                    .get_symbol("DestroyWindow")
                    .ok_or_else(|| "DestroyWindow not found".to_string())?,
                ShowWindow: user32_dll
                    .get_symbol("ShowWindow")
                    .ok_or_else(|| "ShowWindow not found".to_string())?,
                SetWindowPos: user32_dll
                    .get_symbol("SetWindowPos")
                    .ok_or_else(|| "SetWindowPos not found".to_string())?,
                GetClientRect: user32_dll
                    .get_symbol("GetClientRect")
                    .ok_or_else(|| "GetClientRect not found".to_string())?,
                GetWindowRect: user32_dll
                    .get_symbol("GetWindowRect")
                    .ok_or_else(|| "GetWindowRect not found".to_string())?,
                InvalidateRect: user32_dll
                    .get_symbol("InvalidateRect")
                    .ok_or_else(|| "InvalidateRect not found".to_string())?,

                // Window properties
                SetWindowLongPtrW: user32_dll
                    .get_symbol("SetWindowLongPtrW")
                    .ok_or_else(|| "SetWindowLongPtrW not found".to_string())?,
                GetWindowLongPtrW: user32_dll
                    .get_symbol("GetWindowLongPtrW")
                    .ok_or_else(|| "GetWindowLongPtrW not found".to_string())?,

                // Window class
                RegisterClassW: user32_dll
                    .get_symbol("RegisterClassW")
                    .ok_or_else(|| "RegisterClassW not found".to_string())?,
                DefWindowProcW: user32_dll
                    .get_symbol("DefWindowProcW")
                    .ok_or_else(|| "DefWindowProcW not found".to_string())?,

                // Device context
                GetDC: user32_dll
                    .get_symbol("GetDC")
                    .ok_or_else(|| "GetDC not found".to_string())?,
                ReleaseDC: user32_dll
                    .get_symbol("ReleaseDC")
                    .ok_or_else(|| "ReleaseDC not found".to_string())?,

                // Cursor
                GetCursorPos: user32_dll
                    .get_symbol("GetCursorPos")
                    .ok_or_else(|| "GetCursorPos not found".to_string())?,
                ScreenToClient: user32_dll
                    .get_symbol("ScreenToClient")
                    .ok_or_else(|| "ScreenToClient not found".to_string())?,
                ClientToScreen: user32_dll
                    .get_symbol("ClientToScreen")
                    .ok_or_else(|| "ClientToScreen not found".to_string())?,
                SetCapture: user32_dll
                    .get_symbol("SetCapture")
                    .ok_or_else(|| "SetCapture not found".to_string())?,
                ReleaseCapture: user32_dll
                    .get_symbol("ReleaseCapture")
                    .ok_or_else(|| "ReleaseCapture not found".to_string())?,
                LoadCursorW: user32_dll
                    .get_symbol("LoadCursorW")
                    .ok_or_else(|| "LoadCursorW not found".to_string())?,
                SetCursor: user32_dll
                    .get_symbol("SetCursor")
                    .ok_or_else(|| "SetCursor not found".to_string())?,

                // Messages
                PostMessageW: user32_dll
                    .get_symbol("PostMessageW")
                    .ok_or_else(|| "PostMessageW not found".to_string())?,
                GetMessageW: user32_dll
                    .get_symbol("GetMessageW")
                    .ok_or_else(|| "GetMessageW not found".to_string())?,
                TranslateMessage: user32_dll
                    .get_symbol("TranslateMessage")
                    .ok_or_else(|| "TranslateMessage not found".to_string())?,
                DispatchMessageW: user32_dll
                    .get_symbol("DispatchMessageW")
                    .ok_or_else(|| "DispatchMessageW not found".to_string())?,

                // Module
                GetModuleHandleW: user32_dll
                    .get_symbol("GetModuleHandleW")
                    .ok_or_else(|| "GetModuleHandleW not found".to_string())?,

                // Timers
                SetTimer: user32_dll
                    .get_symbol("SetTimer")
                    .ok_or_else(|| "SetTimer not found".to_string())?,
                KillTimer: user32_dll
                    .get_symbol("KillTimer")
                    .ok_or_else(|| "KillTimer not found".to_string())?,
            }
        };

        // Load function pointers from gdi32.dll
        let gdi32 = unsafe {
            Gdi32Functions {
                CreateSolidBrush: gdi32_dll
                    .get_symbol("CreateSolidBrush")
                    .ok_or_else(|| "CreateSolidBrush not found".to_string())?,
                DeleteObject: gdi32_dll
                    .get_symbol("DeleteObject")
                    .ok_or_else(|| "DeleteObject not found".to_string())?,
            }
        };

        Ok(Self {
            user32_dll: Some(user32_dll),
            user32,
            gdi32_dll: Some(gdi32_dll),
            gdi32,
            opengl32: DynamicLibrary::load("opengl32.dll").ok(),
            dwmapi: DynamicLibrary::load("dwmapi.dll").ok(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(target_os = "windows")]
    fn test_load_user32() {
        let lib = DynamicLibrary::load("user32.dll").unwrap();
        assert!(lib.handle().is_some());
    }
}
