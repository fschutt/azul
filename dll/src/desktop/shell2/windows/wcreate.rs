//! Win32 window creation helper functions
//!
//! Provides [`register_window_class`], [`create_hwnd`], and [`create_gl_context`]
//! (which attempts OpenGL 3.2 Core, then 3.0, then legacy `wglCreateContext`).
//! Also contains helpers for window sizing ([`get_client_rect`], [`set_window_size`]).

use std::{mem, ptr};

use azul_layout::window_state::WindowCreateOptions;

use super::dlopen::{
    constants::*, encode_wide, Win32Libraries, HDC, HGLRC, HINSTANCE, HWND, POINT, RECT, WNDCLASSW,
};
use crate::desktop::shell2::common::debug_server::LogCategory;
use crate::desktop::shell2::common::WindowError;
use crate::{log_debug, log_error, log_trace, log_warn};

/// Win32 window class name
pub const CLASS_NAME: &str = "AzulWindowClass";

/// Register the Win32 window class
///
/// This must be called before creating any windows.
/// It's safe to call multiple times - duplicate registrations are ignored.
/// Decode a Win32 `GetLastError()` code into its system message.
///
/// The logs carried the bare number ("failed with error: 2000"), which needs
/// a lookup table and a browser to act on. `FormatMessageW` is the same
/// lookup the OS uses, so the log can carry the text directly.
///
/// Returns `"<code N>"` when the OS has no message for the code, so callers
/// can always print something.
pub(crate) fn win32_error_string(code: u32) -> String {
    use winapi::um::winbase::{
        FormatMessageW, FORMAT_MESSAGE_FROM_SYSTEM, FORMAT_MESSAGE_IGNORE_INSERTS,
    };
    if code == 0 {
        return "ERROR_SUCCESS".to_string();
    }
    let mut buf = [0u16; 512];
    let len = unsafe {
        FormatMessageW(
            FORMAT_MESSAGE_FROM_SYSTEM | FORMAT_MESSAGE_IGNORE_INSERTS,
            std::ptr::null(),
            code,
            0,
            buf.as_mut_ptr(),
            buf.len() as u32,
            std::ptr::null_mut(),
        )
    };
    if len == 0 {
        return format!("<code {code}>");
    }
    String::from_utf16_lossy(&buf[..len as usize])
        .trim_end_matches(['\r', '\n'])
        .to_string()
}

pub fn register_window_class(
    hinstance: HINSTANCE,
    window_proc: super::dlopen::WNDPROC,
    win32: &Win32Libraries,
) -> Result<super::dlopen::ATOM, WindowError> {
    unsafe {
        let mut class_name = encode_wide(CLASS_NAME);
        // Use null background brush - we paint the entire window ourselves with OpenGL
        // This prevents Windows from filling the window with black/white during creation
        let hbrBackground = ptr::null_mut();

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
        let mut window_title = encode_wide(options.window_state.title.as_str());

        let parent = parent_hwnd.unwrap_or(ptr::null_mut());

        // Calculate initial window size.
        // For `size_to_content`, create with a 1×1 placeholder. The window is
        // created hidden (no WS_VISIBLE in `style`); the shell will run the
        // first layout, call `set_window_size` to fit content, then
        // `ShowWindow(SW_SHOWNORMAL)`.
        let (width, height) = if options.size_to_content {
            (1, 1)
        } else {
            (
                libm::roundf(options.window_state.size.dimensions.width) as i32,
                libm::roundf(options.window_state.size.dimensions.height) as i32,
            )
        };

        // Window style - based on decorations option
        use azul_core::window::WindowDecorations;
        use super::dlopen::constants::{
            WS_CAPTION, WS_MAXIMIZEBOX, WS_MINIMIZEBOX, WS_OVERLAPPED, WS_POPUP, WS_SYSMENU,
            WS_THICKFRAME,
        };

        let style = match options.window_state.flags.decorations {
            WindowDecorations::Normal => {
                // Full decorations: WS_OVERLAPPEDWINDOW
                WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_THICKFRAME | WS_MINIMIZEBOX | WS_MAXIMIZEBOX
            }
            WindowDecorations::NoTitle | WindowDecorations::NoTitleAutoInject => {
                // Extended frame: controls visible but no title text
                // On Windows, we still use full decorations but will hide title via DWM later
                WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_THICKFRAME | WS_MINIMIZEBOX | WS_MAXIMIZEBOX
            }
            WindowDecorations::NoControls => {
                // Title bar but no minimize/maximize buttons
                WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_THICKFRAME
            }
            WindowDecorations::None => {
                // Frameless — but NOT a bare `WS_POPUP`. The DWM only draws a
                // shadow, Windows 11 rounded corners and the snap-layouts
                // affordance for a window that HAS a frame, and only lets
                // `SC_SIZE` resize one that has `WS_THICKFRAME`; on a bare
                // popup `DwmExtendFrameIntoClientArea` below returns S_OK and
                // draws nothing, and the CSD resize edges do nothing. So keep
                // every frame style and remove the non-client AREA instead,
                // in `WM_NCCALCSIZE` (returns 0 for a frameless window, so
                // the client rect is the whole window). That is the recipe
                // Chromium, Electron and every borderless-window sample use.
                // `WS_CAPTION` must stay for `SW_MAXIMIZE` to respect the
                // taskbar's work area; without it a maximize covers the
                // screen like fullscreen.
                WS_POPUP | WS_THICKFRAME | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_MAXIMIZEBOX
            }
        };

        // An OWNED popup (a transient window, a fallback menu: WS_POPUP with a
        // parent HWND as its owner) stays above its owner, hides and minimises
        // with it, and must not get a taskbar button of its own.
        let owned_popup = !parent.is_null() && style == WS_POPUP;
        let style_ex = if owned_popup {
            super::dlopen::constants::WS_EX_TOOLWINDOW | WS_EX_ACCEPTFILES
        } else {
            WS_EX_APPWINDOW | WS_EX_ACCEPTFILES
        };

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

        // Restore the drop shadow, the Windows 11 rounded corners and the
        // snap-layouts affordance for a frameless window: the frame styles
        // above give the DWM a frame to draw, `WM_NCCALCSIZE` hands the
        // frame's area to the client, and this extends the (now invisible)
        // frame one pixel into the client so the DWM keeps compositing the
        // shadow and corners for it.
        //
        // This must happen HERE as well as in sync_window_state, because that
        // one is diff-gated on `previous.flags.decorations != current` — and a
        // window CREATED frameless never trips that diff. It is the same shape
        // as the maximize flag: a state that is applied on CHANGE is not
        // applied at BIRTH unless someone says so.
        //
        // Electron does not expose this either; Chromium calls it internally
        // for every frameless window, which is why `frame: false` still looks
        // like a real window. One-pixel top margin rather than -1: the "sheet
        // of glass" form composites the entire client area as frame and shows
        // through wherever the app draws with alpha.
        if matches!(options.window_state.flags.decorations, WindowDecorations::None) {
            if let Some(ref dwm) = win32.dwmapi_funcs {
                let margins = crate::desktop::shell2::windows::dlopen::MARGINS {
                    cxLeftWidth: 0,
                    cxRightWidth: 0,
                    cyTopHeight: 1,
                    cyBottomHeight: 0,
                };
                (dwm.DwmExtendFrameIntoClientArea)(hwnd, &margins);
            }
        }

        Ok(hwnd)
    }
}

/// Create an OpenGL context for the window
pub fn create_gl_context(
    hwnd: HWND,
    hinstance: HINSTANCE,
    win32: &Win32Libraries,
    vsync: azul_core::window::Vsync,
) -> Result<HGLRC, WindowError> {
    use super::gl::ExtraWglFunctions;

    log_trace!(LogCategory::Rendering, "[GL] create_gl_context() called");
    log_trace!(
        LogCategory::Rendering,
        "[GL] hwnd: {:?}, hinstance: {:?}",
        hwnd,
        hinstance
    );

    log_trace!(LogCategory::Rendering, "[GL] loading ExtraWglFunctions");
    let extra_wgl = ExtraWglFunctions::load().map_err(|e| {
        log_error!(
            LogCategory::Rendering,
            "[GL] Failed to load WGL extensions: {:?}",
            e
        );
        WindowError::PlatformError(format!("Failed to load WGL extensions: {:?}", e))
    })?;
    log_trace!(
        LogCategory::Rendering,
        "[GL] ExtraWglFunctions loaded successfully"
    );
    log_trace!(
        LogCategory::Rendering,
        "[GL] wglChoosePixelFormatARB: {:?}",
        extra_wgl.wglChoosePixelFormatARB.is_some()
    );
    log_trace!(
        LogCategory::Rendering,
        "[GL] wglCreateContextAttribsARB: {:?}",
        extra_wgl.wglCreateContextAttribsARB.is_some()
    );
    log_trace!(
        LogCategory::Rendering,
        "[GL] wglSwapIntervalEXT: {:?}",
        extra_wgl.wglSwapIntervalEXT.is_some()
    );

    log_trace!(LogCategory::Rendering, "[GL] calling GetDC");
    let hdc = unsafe { (win32.user32.GetDC)(hwnd) };
    if hdc.is_null() {
        log_error!(LogCategory::Rendering, "[GL] GetDC failed");
        return Err(WindowError::PlatformError("GetDC failed".into()));
    }
    log_trace!(LogCategory::Rendering, "[GL] GetDC returned: {:?}", hdc);

    // Choose pixel format using modern ARB extension
    log_trace!(LogCategory::Rendering, "[GL] choosing pixel format");
    let pixel_format = unsafe {
        let float_attribs = [
            WGL_DRAW_TO_WINDOW_ARB as i32,
            1,
            WGL_SUPPORT_OPENGL_ARB as i32,
            1,
            WGL_DOUBLE_BUFFER_ARB as i32,
            1,
            WGL_PIXEL_TYPE_ARB as i32,
            WGL_TYPE_RGBA_ARB as i32,
            WGL_COLOR_BITS_ARB as i32,
            24,
            WGL_ALPHA_BITS_ARB as i32,
            8,
            WGL_DEPTH_BITS_ARB as i32,
            24,
            WGL_STENCIL_BITS_ARB as i32,
            8,
            WGL_ACCELERATION_ARB as i32,
            WGL_FULL_ACCELERATION_ARB as i32,
            0, // Terminate
        ];
        log_trace!(LogCategory::Rendering, "[GL] pixel format attribs set up");

        let mut pixel_format = 0i32;
        let mut num_formats = 0u32;

        let choose_fn = extra_wgl.wglChoosePixelFormatARB.ok_or_else(|| {
            log_error!(
                LogCategory::Rendering,
                "[GL] wglChoosePixelFormatARB not available"
            );
            WindowError::PlatformError("wglChoosePixelFormatARB not available".into())
        })?;
        log_trace!(
            LogCategory::Rendering,
            "[GL] calling wglChoosePixelFormatARB"
        );

        let result = choose_fn(
            hdc as _,
            float_attribs.as_ptr(),
            std::ptr::null(),
            1,
            &mut pixel_format,
            &mut num_formats,
        );
        log_trace!(
            LogCategory::Rendering,
            "[GL] wglChoosePixelFormatARB returned: {}, num_formats: {}, pixel_format: {}",
            result,
            num_formats,
            pixel_format
        );

        if result == 0 || num_formats == 0 {
            log_error!(
                LogCategory::Rendering,
                "[GL] wglChoosePixelFormatARB failed"
            );
            (win32.user32.ReleaseDC)(hwnd, hdc);
            return Err(WindowError::PlatformError(
                "wglChoosePixelFormatARB failed".into(),
            ));
        }

        pixel_format
    };
    log_trace!(
        LogCategory::Rendering,
        "[GL] pixel format chosen: {}",
        pixel_format
    );

    // Set pixel format
    log_trace!(LogCategory::Rendering, "[GL] setting pixel format");
    unsafe {
        use winapi::um::wingdi::{DescribePixelFormat, SetPixelFormat, PIXELFORMATDESCRIPTOR};

        let mut pfd: PIXELFORMATDESCRIPTOR = std::mem::zeroed();
        DescribePixelFormat(
            hdc as _,
            pixel_format,
            std::mem::size_of::<PIXELFORMATDESCRIPTOR>() as u32,
            &mut pfd,
        );
        log_trace!(
            LogCategory::Rendering,
            "[GL] DescribePixelFormat done, pfd.dwFlags: 0x{:x}",
            pfd.dwFlags
        );

        let set_result = SetPixelFormat(hdc as _, pixel_format, &pfd);
        log_trace!(
            LogCategory::Rendering,
            "[GL] SetPixelFormat returned: {}",
            set_result
        );
        if set_result == 0 {
            let error = winapi::um::errhandlingapi::GetLastError();
            log_error!(
                LogCategory::Rendering,
                "[GL] SetPixelFormat failed: {} ({})",
                win32_error_string(error),
                error
            );
            (win32.user32.ReleaseDC)(hwnd, hdc);
            return Err(WindowError::PlatformError("SetPixelFormat failed".into()));
        }
    }
    log_trace!(LogCategory::Rendering, "[GL] pixel format set successfully");

    // Create OpenGL 3.2+ Core Profile context
    log_trace!(LogCategory::Rendering, "[GL] creating OpenGL context");
    let hglrc = unsafe {
        // Try OpenGL 3.2 Core Profile first
        let context_attribs_32 = [
            WGL_CONTEXT_MAJOR_VERSION_ARB as i32,
            3,
            WGL_CONTEXT_MINOR_VERSION_ARB as i32,
            2,
            WGL_CONTEXT_PROFILE_MASK_ARB as i32,
            WGL_CONTEXT_CORE_PROFILE_BIT_ARB as i32,
            WGL_CONTEXT_FLAGS_ARB as i32,
            0,
            0, // Terminate
        ];

        let create_fn = extra_wgl.wglCreateContextAttribsARB.ok_or_else(|| {
            log_error!(
                LogCategory::Rendering,
                "[GL] wglCreateContextAttribsARB not available"
            );
            WindowError::PlatformError("wglCreateContextAttribsARB not available".into())
        })?;
        log_trace!(
            LogCategory::Rendering,
            "[GL] calling wglCreateContextAttribsARB for GL 3.2 Core"
        );

        let mut hglrc = create_fn(hdc as _, std::ptr::null_mut(), context_attribs_32.as_ptr());
        log_trace!(
            LogCategory::Rendering,
            "[GL] wglCreateContextAttribsARB (3.2 Core) returned: {:?}",
            hglrc
        );

        // Fallback to OpenGL 3.0 if 3.2 fails
        if hglrc.is_null() {
            log_trace!(
                LogCategory::Rendering,
                "[GL] GL 3.2 Core failed, trying GL 3.0"
            );
            let context_attribs_30 = [
                WGL_CONTEXT_MAJOR_VERSION_ARB as i32,
                3,
                WGL_CONTEXT_MINOR_VERSION_ARB as i32,
                0,
                0, // Terminate - no profile mask
            ];
            hglrc = create_fn(hdc as _, std::ptr::null_mut(), context_attribs_30.as_ptr());
            log_trace!(
                LogCategory::Rendering,
                "[GL] wglCreateContextAttribsARB (3.0) returned: {:?}",
                hglrc
            );
        }

        // Fallback to legacy OpenGL context if all else fails
        if hglrc.is_null() {
            log_trace!(
                LogCategory::Rendering,
                "[GL] GL 3.0 failed, trying legacy wglCreateContext"
            );
            use winapi::um::wingdi::wglCreateContext;
            hglrc = wglCreateContext(hdc as _) as _;
            log_trace!(
                LogCategory::Rendering,
                "[GL] wglCreateContext (legacy) returned: {:?}",
                hglrc
            );
        }

        if hglrc.is_null() {
            let error = winapi::um::errhandlingapi::GetLastError();
            log_error!(
                LogCategory::Rendering,
                "[GL] All OpenGL context creation attempts failed: {} ({})",
                win32_error_string(error),
                error
            );
            (win32.user32.ReleaseDC)(hwnd, hdc);
            return Err(WindowError::PlatformError(
                "wglCreateContextAttribsARB failed".into(),
            ));
        }

        hglrc as HGLRC
    };
    log_trace!(
        LogCategory::Rendering,
        "[GL] OpenGL context created: {:?}",
        hglrc
    );

    #[cfg(target_os = "windows")]
    unsafe {
        use winapi::um::wingdi::wglMakeCurrent;
        log_trace!(LogCategory::Rendering, "[GL] calling wglMakeCurrent");
        let result = wglMakeCurrent(
            hdc as winapi::shared::windef::HDC,
            hglrc as winapi::shared::windef::HGLRC,
        );
        log_trace!(
            LogCategory::Rendering,
            "[GL] wglMakeCurrent returned: {}",
            result
        );

        if result == 0 {
            let error = winapi::um::errhandlingapi::GetLastError();
            log_error!(
                LogCategory::Rendering,
                "[GL] wglMakeCurrent FAILED! GetLastError: {}",
                error
            );
            (win32.user32.ReleaseDC)(hwnd, hdc);
            return Err(WindowError::PlatformError(
                format!("wglMakeCurrent failed with error {}", error).into(),
            ));
        }

        // Query and log OpenGL info
        log_trace!(LogCategory::Rendering, "[GL] querying OpenGL info");
        use winapi::um::libloaderapi::GetProcAddress;
        use winapi::um::wingdi::wglGetProcAddress;

        // Get glGetString and glGetIntegerv
        let opengl32 = winapi::um::libloaderapi::GetModuleHandleA(b"opengl32.dll\0".as_ptr() as _);
        if !opengl32.is_null() {
            let gl_get_string: Option<extern "system" fn(u32) -> *const i8> =
                std::mem::transmute(GetProcAddress(opengl32, b"glGetString\0".as_ptr() as _));
            let gl_get_integerv: Option<extern "system" fn(u32, *mut i32)> =
                std::mem::transmute(GetProcAddress(opengl32, b"glGetIntegerv\0".as_ptr() as _));
            let gl_get_error: Option<extern "system" fn() -> u32> =
                std::mem::transmute(GetProcAddress(opengl32, b"glGetError\0".as_ptr() as _));

            if let Some(get_string) = gl_get_string {
                const GL_VENDOR: u32 = 0x1F00;
                const GL_RENDERER: u32 = 0x1F01;
                const GL_VERSION: u32 = 0x1F02;

                let vendor = get_string(GL_VENDOR);
                let renderer = get_string(GL_RENDERER);
                let version = get_string(GL_VERSION);

                if !vendor.is_null() {
                    log_debug!(
                        LogCategory::Rendering,
                        "[GL] Vendor: {}",
                        std::ffi::CStr::from_ptr(vendor).to_string_lossy()
                    );
                }
                if !renderer.is_null() {
                    log_debug!(
                        LogCategory::Rendering,
                        "[GL] Renderer: {}",
                        std::ffi::CStr::from_ptr(renderer).to_string_lossy()
                    );
                }
                if !version.is_null() {
                    log_debug!(
                        LogCategory::Rendering,
                        "[GL] Version: {}",
                        std::ffi::CStr::from_ptr(version).to_string_lossy()
                    );
                }
            }

            if let Some(get_integerv) = gl_get_integerv {
                const GL_MAX_TEXTURE_SIZE: u32 = 0x0D33;
                let mut max_texture_size: i32 = 0;
                get_integerv(GL_MAX_TEXTURE_SIZE, &mut max_texture_size);
                log_debug!(
                    LogCategory::Rendering,
                    "[GL] GL_MAX_TEXTURE_SIZE: {}",
                    max_texture_size
                );

                if max_texture_size == 0 {
                    log_warn!(
                        LogCategory::Rendering,
                        "[GL] GL_MAX_TEXTURE_SIZE is 0 - context may be invalid!"
                    );
                    if let Some(get_error) = gl_get_error {
                        let err = get_error();
                        log_error!(
                            LogCategory::Rendering,
                            "[GL] glGetError after glGetIntegerv: 0x{:x}",
                            err
                        );
                    }
                }
            }
        } else {
            log_warn!(
                LogCategory::Rendering,
                "[GL] Could not get opengl32.dll handle for GL info query"
            );
        }
    }

    if let Some(swap_interval_fn) = extra_wgl.wglSwapIntervalEXT {
        use azul_core::window::Vsync;
        let interval = match vsync {
            Vsync::Enabled => 1,
            Vsync::Disabled => 0,
            Vsync::DontCare => 1,
        };
        log_trace!(
            LogCategory::Rendering,
            "[GL] setting swap interval to {}",
            interval
        );
        unsafe { swap_interval_fn(interval) };
    } else {
        log_trace!(
            LogCategory::Rendering,
            "[GL] wglSwapIntervalEXT not available, skipping vsync"
        );
    }

    // NOTE: We do NOT release the DC here - it needs to stay valid for the GL context.
    // The DC will be released when the window is destroyed.

    log_trace!(
        LogCategory::Rendering,
        "[GL] SUCCESS, returning hglrc: {:?}",
        hglrc
    );
    Ok(hglrc)
}

// WGL extension constants
const WGL_DRAW_TO_WINDOW_ARB: u32 = 0x2001;
const WGL_SUPPORT_OPENGL_ARB: u32 = 0x2010;
const WGL_DOUBLE_BUFFER_ARB: u32 = 0x2011;
const WGL_PIXEL_TYPE_ARB: u32 = 0x2013;
const WGL_TYPE_RGBA_ARB: u32 = 0x202B;
const WGL_COLOR_BITS_ARB: u32 = 0x2014;
const WGL_ALPHA_BITS_ARB: u32 = 0x201B;
const WGL_DEPTH_BITS_ARB: u32 = 0x2022;
const WGL_STENCIL_BITS_ARB: u32 = 0x2023;
const WGL_ACCELERATION_ARB: u32 = 0x2003;
const WGL_FULL_ACCELERATION_ARB: u32 = 0x2027;
const WGL_CONTEXT_MAJOR_VERSION_ARB: u32 = 0x2091;
const WGL_CONTEXT_MINOR_VERSION_ARB: u32 = 0x2092;
const WGL_CONTEXT_PROFILE_MASK_ARB: u32 = 0x9126;
const WGL_CONTEXT_CORE_PROFILE_BIT_ARB: u32 = 0x00000001;
const WGL_CONTEXT_FLAGS_ARB: u32 = 0x2094;

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

/// Resize a window so its CLIENT area is `client_w` × `client_h` PHYSICAL px.
///
/// `CreateWindowExW`/`SetWindowPos` size the OUTER frame; callers holding a
/// client size (azul's logical `size.dimensions` × DPI scale) must add the
/// frame delta or the client area comes out smaller by the border + title
/// bar. The delta is measured from the live window (`GetWindowRect` −
/// `GetClientRect`) so it is correct for any style/DPI without needing
/// `AdjustWindowRectExForDpi`; for `WS_POPUP` (no frame) it is zero.
pub fn set_client_size(
    hwnd: HWND,
    client_w: i32,
    client_h: i32,
    win32: &Win32Libraries,
) -> Result<(), WindowError> {
    let (frame_w, frame_h) = unsafe {
        let mut wr = RECT::default();
        let mut cr = RECT::default();
        if (win32.user32.GetWindowRect)(hwnd, &mut wr) == 0
            || (win32.user32.GetClientRect)(hwnd, &mut cr) == 0
        {
            (0, 0)
        } else {
            (
                (wr.right - wr.left) - (cr.right - cr.left),
                (wr.bottom - wr.top) - (cr.bottom - cr.top),
            )
        }
    };
    set_window_size(hwnd, client_w + frame_w, client_h + frame_h, win32)
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
