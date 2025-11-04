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

/// Win32 COMPOSITIONFORM structure for IME composition window positioning
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct COMPOSITIONFORM {
    pub dwStyle: u32,
    pub ptCurrentPos: POINT,
    pub rcArea: RECT,
}

// IME Composition Form Styles
pub const CFS_DEFAULT: u32 = 0x0000;
pub const CFS_RECT: u32 = 0x0001;
pub const CFS_POINT: u32 = 0x0002;
pub const CFS_FORCE_POSITION: u32 = 0x0020;

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

    // Special constants
    pub const CW_USEDEFAULT: i32 = 0x80000000_u32 as i32;
    pub const HWND_TOP: *mut core::ffi::c_void = 0 as *mut core::ffi::c_void;

    // Window Messages
    pub const WM_CLOSE: u32 = 0x0010;
    pub const WM_PAINT: u32 = 0x000F;
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

    // Messages
    pub SendMessageW: unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT,
    pub PostMessageW: unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> BOOL,
    pub GetMessageW: unsafe extern "system" fn(*mut MSG, HWND, u32, u32) -> BOOL,
    pub PeekMessageW: unsafe extern "system" fn(*mut MSG, HWND, u32, u32, u32) -> BOOL,
    pub TranslateMessage: unsafe extern "system" fn(*const MSG) -> BOOL,
    pub DispatchMessageW: unsafe extern "system" fn(*const MSG) -> LRESULT,
    pub WaitMessage: unsafe extern "system" fn() -> BOOL,

    // Module handle
    pub GetModuleHandleW: unsafe extern "system" fn(*const u16) -> HINSTANCE,

    // Timers
    pub SetTimer: unsafe extern "system" fn(HWND, usize, u32, *const core::ffi::c_void) -> usize,
    pub KillTimer: unsafe extern "system" fn(HWND, usize) -> BOOL,
}

/// Win32 gdi32.dll function pointers for brushes
#[derive(Copy, Clone)]
pub struct Gdi32Functions {
    pub CreateSolidBrush: unsafe extern "system" fn(u32) -> HBRUSH,
    pub DeleteObject: unsafe extern "system" fn(*mut core::ffi::c_void) -> BOOL,
}

/// Win32 imm32.dll function pointers for IME (Input Method Editor)
#[derive(Copy, Clone)]
pub struct Imm32Functions {
    pub ImmGetContext: unsafe extern "system" fn(HWND) -> HIMC,
    pub ImmReleaseContext: unsafe extern "system" fn(HWND, HIMC) -> BOOL,
    pub ImmGetCompositionStringW:
        unsafe extern "system" fn(HIMC, u32, *mut core::ffi::c_void, u32) -> i32,
    pub ImmSetCompositionWindow: unsafe extern "system" fn(HIMC, *const COMPOSITIONFORM) -> BOOL,
}

/// Win32 shell32.dll function pointers for drag-and-drop
#[derive(Copy, Clone)]
pub struct Shell32Functions {
    pub DragAcceptFiles: unsafe extern "system" fn(HWND, BOOL),
    pub DragQueryFileW: unsafe extern "system" fn(HDROP, UINT, *mut u16, UINT) -> UINT,
    pub DragQueryPoint: unsafe extern "system" fn(HDROP, *mut POINT) -> BOOL,
    pub DragFinish: unsafe extern "system" fn(HDROP),
}

/// Win32 kernel32.dll function pointers for power management
#[derive(Copy, Clone)]
pub struct Kernel32Functions {
    pub SetThreadExecutionState: unsafe extern "system" fn(u32) -> u32,
}

/// Pre-load commonly used Win32 DLLs
pub struct Win32Libraries {
    pub user32_dll: Option<DynamicLibrary>,
    pub user32: User32Functions,
    pub gdi32_dll: Option<DynamicLibrary>,
    pub gdi32: Gdi32Functions,
    pub imm32_dll: Option<DynamicLibrary>,
    pub imm32: Option<Imm32Functions>,
    pub shell32_dll: Option<DynamicLibrary>,
    pub shell32: Option<Shell32Functions>,
    pub kernel32_dll: Option<DynamicLibrary>,
    pub kernel32: Option<Kernel32Functions>,
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
                DrawMenuBar: user32_dll
                    .get_symbol("DrawMenuBar")
                    .ok_or_else(|| "DrawMenuBar not found".to_string())?,
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
                SetWindowTextW: user32_dll
                    .get_symbol("SetWindowTextW")
                    .ok_or_else(|| "SetWindowTextW not found".to_string())?,

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
                TrackMouseEvent: user32_dll
                    .get_symbol("TrackMouseEvent")
                    .ok_or_else(|| "TrackMouseEvent not found".to_string())?,

                // Messages
                SendMessageW: user32_dll
                    .get_symbol("SendMessageW")
                    .ok_or_else(|| "SendMessageW not found".to_string())?,
                PostMessageW: user32_dll
                    .get_symbol("PostMessageW")
                    .ok_or_else(|| "PostMessageW not found".to_string())?,
                GetMessageW: user32_dll
                    .get_symbol("GetMessageW")
                    .ok_or_else(|| "GetMessageW not found".to_string())?,
                PeekMessageW: user32_dll
                    .get_symbol("PeekMessageW")
                    .ok_or_else(|| "PeekMessageW not found".to_string())?,
                TranslateMessage: user32_dll
                    .get_symbol("TranslateMessage")
                    .ok_or_else(|| "TranslateMessage not found".to_string())?,
                DispatchMessageW: user32_dll
                    .get_symbol("DispatchMessageW")
                    .ok_or_else(|| "DispatchMessageW not found".to_string())?,
                WaitMessage: user32_dll
                    .get_symbol("WaitMessage")
                    .ok_or_else(|| "WaitMessage not found".to_string())?,

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

        // Try to load function pointers from shell32.dll (optional - for drag-and-drop)
        let shell32_dll = DynamicLibrary::load("shell32.dll").ok();
        let shell32 = if let Some(ref dll) = shell32_dll {
            unsafe {
                let drag_accept = dll.get_symbol("DragAcceptFiles");
                let drag_query_file = dll.get_symbol("DragQueryFileW");
                let drag_query_point = dll.get_symbol("DragQueryPoint");
                let drag_finish = dll.get_symbol("DragFinish");

                if let (Some(accept), Some(query_file), Some(query_point), Some(finish)) =
                    (drag_accept, drag_query_file, drag_query_point, drag_finish)
                {
                    Some(Shell32Functions {
                        DragAcceptFiles: accept,
                        DragQueryFileW: query_file,
                        DragQueryPoint: query_point,
                        DragFinish: finish,
                    })
                } else {
                    None
                }
            }
        } else {
            None
        };

        // Try to load function pointers from imm32.dll (optional - for IME)
        let imm32_dll = DynamicLibrary::load("imm32.dll").ok();
        let imm32 = if let Some(ref dll) = imm32_dll {
            unsafe {
                let get_context = dll.get_symbol("ImmGetContext");
                let release_context = dll.get_symbol("ImmReleaseContext");
                let get_comp_string = dll.get_symbol("ImmGetCompositionStringW");
                let set_comp_window = dll.get_symbol("ImmSetCompositionWindow");

                if let (Some(get_ctx), Some(rel_ctx), Some(get_str), Some(set_win)) = (
                    get_context,
                    release_context,
                    get_comp_string,
                    set_comp_window,
                ) {
                    Some(Imm32Functions {
                        ImmGetContext: get_ctx,
                        ImmReleaseContext: rel_ctx,
                        ImmGetCompositionStringW: get_str,
                        ImmSetCompositionWindow: set_win,
                    })
                } else {
                    None
                }
            }
        } else {
            None
        };

        // Try to load function pointers from kernel32.dll (optional - for power management)
        let kernel32_dll = DynamicLibrary::load("kernel32.dll").ok();
        let kernel32 = if let Some(ref dll) = kernel32_dll {
            unsafe {
                dll.get_symbol("SetThreadExecutionState")
                    .map(|f| Kernel32Functions {
                        SetThreadExecutionState: f,
                    })
            }
        } else {
            None
        };

        Ok(Self {
            user32_dll: Some(user32_dll),
            user32,
            gdi32_dll: Some(gdi32_dll),
            gdi32,
            imm32_dll,
            imm32,
            shell32_dll,
            shell32,
            kernel32_dll,
            kernel32,
            opengl32: DynamicLibrary::load("opengl32.dll").ok(),
            dwmapi: DynamicLibrary::load("dwmapi.dll").ok(),
        })
    }
}

impl Clone for Win32Libraries {
    fn clone(&self) -> Self {
        Self {
            user32_dll: None, // Don't clone DLL handles, they're shared
            user32: self.user32,
            gdi32_dll: None,
            gdi32: self.gdi32,
            imm32_dll: None,
            imm32: self.imm32,
            shell32_dll: None,
            shell32: self.shell32,
            kernel32_dll: None,
            kernel32: self.kernel32,
            opengl32: None,
            dwmapi: None,
        }
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
