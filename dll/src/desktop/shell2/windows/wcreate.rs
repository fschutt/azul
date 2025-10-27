//! Win32 window creation helper functions
//!
//! This module contains the complex window creation logic extracted from the main module.

use std::{mem, ptr};

use azul_core::window::WindowFrame;
use azul_layout::window_state::WindowCreateOptions;

use super::dlopen::{
    constants::*, encode_wide, Win32Libraries, HDC, HGLRC, HINSTANCE, HWND, POINT, RECT, WNDCLASSW,
};
use crate::desktop::shell2::common::WindowError;

/// Win32 window class name
pub const CLASS_NAME: &str = "AzulWindowClass";

/// Register the Win32 window class
///
/// This must be called before creating any windows.
/// It's safe to call multiple times - duplicate registrations are ignored.
pub fn register_window_class(
    hinstance: HINSTANCE,
    window_proc: super::dlopen::WNDPROC,
    win32: &Win32Libraries,
) -> Result<super::dlopen::ATOM, WindowError> {
    unsafe {
        let mut class_name = encode_wide(CLASS_NAME);
        let hbrBackground = (win32.gdi32.CreateSolidBrush)(0x00000000);

        let wc = WNDCLASSW {
            style: 0,
            lpfnWndProc: window_proc,
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinstance,
            hIcon: ptr::null_mut(),
            hCursor: ptr::null_mut(),
            hbrBackground,
            lpszMenuName: ptr::null(),
            lpszClassName: class_name.as_ptr(),
        };

        let atom = (win32.user32.RegisterClassW)(&wc);

        if atom == 0 {
            return Err(WindowError::PlatformError(
                "Failed to register window class".into(),
            ));
        }

        Ok(atom)
    }
}

/// Create a Win32 HWND window
pub fn create_hwnd(
    hinstance: HINSTANCE,
    options: &WindowCreateOptions,
    parent_hwnd: Option<HWND>,
    user_data: *mut core::ffi::c_void,
    win32: &Win32Libraries,
) -> Result<HWND, WindowError> {
    unsafe {
        let mut class_name = encode_wide(CLASS_NAME);
        let mut window_title = encode_wide(options.state.title.as_str());

        let parent = parent_hwnd.unwrap_or(ptr::null_mut());

        // Calculate initial window size
        let (width, height) = if options.size_to_content {
            (0, 0)
        } else {
            (
                libm::roundf(options.state.size.dimensions.width) as i32,
                libm::roundf(options.state.size.dimensions.height) as i32,
            )
        };

        // Window style
        let style = WS_OVERLAPPED
            | WS_CAPTION
            | WS_SYSMENU
            | WS_THICKFRAME
            | WS_MINIMIZEBOX
            | WS_MAXIMIZEBOX
            | WS_TABSTOP
            | WS_POPUP;

        let style_ex = WS_EX_APPWINDOW | WS_EX_ACCEPTFILES;

        let hwnd = (win32.user32.CreateWindowExW)(
            style_ex,
            class_name.as_ptr(),
            window_title.as_ptr(),
            style,
            CW_USEDEFAULT, // x
            CW_USEDEFAULT, // y
            width,
            height,
            parent,
            ptr::null_mut(), // Menu
            hinstance,
            user_data,
        );

        if hwnd.is_null() {
            return Err(WindowError::PlatformError("Failed to create HWND".into()));
        }

        Ok(hwnd)
    }
}

/// Create an OpenGL context for the window
pub fn create_gl_context(
    hwnd: HWND,
    hinstance: HINSTANCE,
    win32: &Win32Libraries,
) -> Result<HGLRC, WindowError> {
    // TODO: Implement OpenGL context creation
    // This requires:
    // 1. GetDC
    // 2. ChoosePixelFormat / SetPixelFormat
    // 3. wglCreateContext or wglCreateContextAttribsARB

    Err(WindowError::PlatformError(
        "OpenGL context creation not yet implemented".into(),
    ))
}

/// Show or hide a window with the appropriate frame state
pub fn show_window_with_frame(
    hwnd: HWND,
    frame: WindowFrame,
    is_visible: bool,
    win32: &Win32Libraries,
) {
    let mut show_cmd = SW_HIDE;

    if is_visible {
        show_cmd = match frame {
            WindowFrame::Normal => SW_SHOWNORMAL,
            WindowFrame::Minimized => SW_MINIMIZE,
            WindowFrame::Maximized => SW_MAXIMIZE,
            WindowFrame::Fullscreen => SW_MAXIMIZE,
        };
    }

    unsafe { (win32.user32.ShowWindow)(hwnd, show_cmd) };
}

/// Get client rectangle size
pub fn get_client_rect(hwnd: HWND, win32: &Win32Libraries) -> Result<(u32, u32), WindowError> {
    unsafe {
        let mut rect = RECT::default();
        let result = (win32.user32.GetClientRect)(hwnd, &mut rect);

        if result == 0 {
            return Err(WindowError::PlatformError("GetClientRect failed".into()));
        }

        Ok((rect.width(), rect.height()))
    }
}

/// Resize a window to specific dimensions
pub fn set_window_size(
    hwnd: HWND,
    width: i32,
    height: i32,
    win32: &Win32Libraries,
) -> Result<(), WindowError> {
    let result = unsafe {
        (win32.user32.SetWindowPos)(
            hwnd,
            HWND_TOP,
            0,
            0,
            width,
            height,
            SWP_NOMOVE | SWP_NOZORDER | SWP_FRAMECHANGED,
        )
    };

    if result == 0 {
        return Err(WindowError::PlatformError("SetWindowPos failed".into()));
    }

    Ok(())
}
