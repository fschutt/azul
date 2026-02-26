//! Native screenshot extension trait for CallbackInfo
//!
//! This module provides the `NativeScreenshotExt` trait that extends `CallbackInfo`
//! with native OS-level screenshot capabilities. The implementation uses dlopen
//! at runtime to avoid static linking to X11 on Linux.

use azul_css::AzString;
use azul_layout::callbacks::CallbackInfo;

#[cfg(any(target_os = "windows", target_os = "linux"))]
use tiny_skia;

/// Extension trait for native screenshot functionality
///
/// This trait provides methods to take native OS-level screenshots that include
/// window decorations (title bar, borders, etc.). The implementation uses
/// runtime dynamic loading (dlopen) for platform libraries to avoid static
/// linking dependencies.
pub trait NativeScreenshotExt {
    /// Take a native OS-level screenshot including window decorations
    ///
    /// This captures the window exactly as it appears on screen, including
    /// the title bar, window borders, and any OS-provided window decorations.
    ///
    /// # Platform Support
    /// - **macOS**: Uses `screencapture` command with window ID
    /// - **Windows**: Uses PrintWindow API (BitBlt from window DC)
    /// - **Linux**: Uses XGetImage (X11) via dlopen
    ///
    /// # Arguments
    /// * `path` - The file path to save the PNG screenshot to
    ///
    /// # Returns
    /// * `Ok(())` - Screenshot saved successfully
    /// * `Err(String)` - Error message if screenshot failed
    fn take_native_screenshot(&self, path: &str) -> Result<(), AzString>;

    /// Take a native OS-level screenshot and return the PNG data as bytes
    ///
    /// Same as `take_native_screenshot` but returns the PNG data directly
    /// instead of saving to a file.
    fn take_native_screenshot_bytes(&self) -> Result<Vec<u8>, AzString>;

    /// Take a native OS-level screenshot and return as a Base64 data URI
    ///
    /// Returns the screenshot as a "data:image/png;base64,..." string.
    fn take_native_screenshot_base64(&self) -> Result<AzString, AzString>;
}

impl NativeScreenshotExt for CallbackInfo {
    fn take_native_screenshot(&self, path: &str) -> Result<(), AzString> {
        use azul_core::window::RawWindowHandle;

        let window_handle = self.get_current_window_handle();

        match window_handle {
            #[cfg(target_os = "macos")]
            RawWindowHandle::MacOS(handle) => take_native_screenshot_macos(handle.ns_window, path),
            #[cfg(target_os = "windows")]
            RawWindowHandle::Windows(handle) => take_native_screenshot_windows(handle.hwnd, path),
            #[cfg(target_os = "linux")]
            RawWindowHandle::Xlib(handle) => {
                take_native_screenshot_xlib(handle.display, handle.window, path)
            }
            #[cfg(target_os = "linux")]
            RawWindowHandle::Xcb(handle) => {
                take_native_screenshot_xcb(handle.connection, handle.window, path)
            }
            _ => Err(AzString::from(
                "Native screenshot not supported on this platform",
            )),
        }
    }

    fn take_native_screenshot_bytes(&self) -> Result<Vec<u8>, AzString> {
        let temp_path = std::env::temp_dir().join("azul_screenshot_temp.png");
        let temp_path_str = temp_path.to_string_lossy().to_string();

        // Explicitly call the trait method, not the inherent method on CallbackInfo
        NativeScreenshotExt::take_native_screenshot(self, &temp_path_str)?;

        let bytes = std::fs::read(&temp_path)
            .map_err(|e| AzString::from(format!("Failed to read screenshot: {}", e)))?;

        let _ = std::fs::remove_file(&temp_path);

        Ok(bytes)
    }

    fn take_native_screenshot_base64(&self) -> Result<AzString, AzString> {
        // Explicitly call the trait method, not the inherent method on CallbackInfo
        let png_bytes = NativeScreenshotExt::take_native_screenshot_bytes(self)?;
        let base64_str = azul_layout::callbacks::base64_encode(&png_bytes);
        Ok(AzString::from(format!(
            "data:image/png;base64,{}",
            base64_str
        )))
    }
}

// ============================================================================
// Platform-specific native screenshot implementations
// ============================================================================

/// Take a native screenshot on macOS using screencapture command
#[cfg(target_os = "macos")]
fn take_native_screenshot_macos(
    ns_window: *mut core::ffi::c_void,
    path: &str,
) -> Result<(), AzString> {
    use std::process::Command;

    if ns_window.is_null() {
        return Err(AzString::from("Invalid window handle"));
    }

    // Get the window ID from the NSWindow
    let window_id = unsafe {
        #[link(name = "AppKit", kind = "framework")]
        extern "C" {
            fn objc_msgSend(
                receiver: *mut core::ffi::c_void,
                sel: *const core::ffi::c_void,
                ...
            ) -> i64;
        }

        #[link(name = "objc")]
        extern "C" {
            fn sel_registerName(name: *const i8) -> *const core::ffi::c_void;
        }

        let sel = sel_registerName(b"windowNumber\0".as_ptr() as *const i8);
        objc_msgSend(ns_window, sel)
    };

    if window_id <= 0 {
        return Err(AzString::from("Failed to get window ID"));
    }

    let output = Command::new("screencapture")
        .args(["-l", &window_id.to_string(), "-x", path])
        .output()
        .map_err(|e| AzString::from(format!("Failed to run screencapture: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AzString::from(format!("screencapture failed: {}", stderr)));
    }

    Ok(())
}

/// Take a native screenshot on Windows using PrintWindow API
#[cfg(target_os = "windows")]
fn take_native_screenshot_windows(
    hwnd: *mut core::ffi::c_void,
    path: &str,
) -> Result<(), AzString> {
    use std::ptr;

    if hwnd.is_null() {
        return Err(AzString::from("Invalid window handle"));
    }

    type HWND = *mut core::ffi::c_void;
    type HDC = *mut core::ffi::c_void;
    type HBITMAP = *mut core::ffi::c_void;
    type BOOL = i32;

    #[repr(C)]
    struct RECT {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }

    #[repr(C)]
    struct BITMAPINFOHEADER {
        biSize: u32,
        biWidth: i32,
        biHeight: i32,
        biPlanes: u16,
        biBitCount: u16,
        biCompression: u32,
        biSizeImage: u32,
        biXPelsPerMeter: i32,
        biYPelsPerMeter: i32,
        biClrUsed: u32,
        biClrImportant: u32,
    }

    #[link(name = "user32")]
    extern "system" {
        fn GetWindowRect(hWnd: HWND, lpRect: *mut RECT) -> BOOL;
        fn GetWindowDC(hWnd: HWND) -> HDC;
        fn ReleaseDC(hWnd: HWND, hDC: HDC) -> i32;
        fn PrintWindow(hWnd: HWND, hdcBlt: HDC, nFlags: u32) -> BOOL;
    }

    #[link(name = "gdi32")]
    extern "system" {
        fn CreateCompatibleDC(hdc: HDC) -> HDC;
        fn CreateCompatibleBitmap(hdc: HDC, cx: i32, cy: i32) -> HBITMAP;
        fn SelectObject(hdc: HDC, h: *mut core::ffi::c_void) -> *mut core::ffi::c_void;
        fn DeleteDC(hdc: HDC) -> BOOL;
        fn DeleteObject(ho: *mut core::ffi::c_void) -> BOOL;
        fn GetDIBits(
            hdc: HDC,
            hbm: HBITMAP,
            start: u32,
            cLines: u32,
            lpvBits: *mut u8,
            lpbmi: *mut BITMAPINFOHEADER,
            usage: u32,
        ) -> i32;
    }

    unsafe {
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        if GetWindowRect(hwnd, &mut rect) == 0 {
            return Err(AzString::from("Failed to get window rect"));
        }

        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;

        if width <= 0 || height <= 0 {
            return Err(AzString::from("Invalid window dimensions"));
        }

        let window_dc = GetWindowDC(hwnd);
        if window_dc.is_null() {
            return Err(AzString::from("Failed to get window DC"));
        }

        let mem_dc = CreateCompatibleDC(window_dc);
        if mem_dc.is_null() {
            ReleaseDC(hwnd, window_dc);
            return Err(AzString::from("Failed to create compatible DC"));
        }

        let bitmap = CreateCompatibleBitmap(window_dc, width, height);
        if bitmap.is_null() {
            DeleteDC(mem_dc);
            ReleaseDC(hwnd, window_dc);
            return Err(AzString::from("Failed to create bitmap"));
        }

        let old_bitmap = SelectObject(mem_dc, bitmap);

        const PW_RENDERFULLCONTENT: u32 = 2;
        if PrintWindow(hwnd, mem_dc, PW_RENDERFULLCONTENT) == 0 {
            SelectObject(mem_dc, old_bitmap);
            DeleteObject(bitmap);
            DeleteDC(mem_dc);
            ReleaseDC(hwnd, window_dc);
            return Err(AzString::from("PrintWindow failed"));
        }

        let mut bmi = BITMAPINFOHEADER {
            biSize: core::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height, // Top-down DIB
            biPlanes: 1,
            biBitCount: 32,
            biCompression: 0, // BI_RGB
            biSizeImage: 0,
            biXPelsPerMeter: 0,
            biYPelsPerMeter: 0,
            biClrUsed: 0,
            biClrImportant: 0,
        };

        let row_bytes = (width * 4) as usize;
        let mut pixels: Vec<u8> = vec![0u8; row_bytes * height as usize];

        if GetDIBits(
            mem_dc,
            bitmap,
            0,
            height as u32,
            pixels.as_mut_ptr(),
            &mut bmi,
            0,
        ) == 0
        {
            SelectObject(mem_dc, old_bitmap);
            DeleteObject(bitmap);
            DeleteDC(mem_dc);
            ReleaseDC(hwnd, window_dc);
            return Err(AzString::from("GetDIBits failed"));
        }

        SelectObject(mem_dc, old_bitmap);
        DeleteObject(bitmap);
        DeleteDC(mem_dc);
        ReleaseDC(hwnd, window_dc);

        // Convert BGRA to RGBA
        for chunk in pixels.chunks_exact_mut(4) {
            chunk.swap(0, 2);
        }

        let pixmap = tiny_skia::Pixmap::from_vec(
            pixels,
            tiny_skia::IntSize::from_wh(width as u32, height as u32)
                .ok_or_else(|| AzString::from("Invalid image dimensions"))?,
        )
        .ok_or_else(|| AzString::from("Failed to create pixmap"))?;

        let png_data = pixmap
            .encode_png()
            .map_err(|e| AzString::from(format!("PNG encoding failed: {}", e)))?;

        std::fs::write(path, png_data)
            .map_err(|e| AzString::from(format!("Failed to write file: {}", e)))?;

        Ok(())
    }
}

/// Take a native screenshot on Linux/X11 using XGetImage via dlopen
#[cfg(target_os = "linux")]
fn take_native_screenshot_xlib(
    display: *mut core::ffi::c_void,
    window: u64,
    path: &str,
) -> Result<(), AzString> {
    use std::ffi::CString;

    if display.is_null() {
        return Err(AzString::from("Invalid display handle"));
    }

    // X11 types
    type Display = core::ffi::c_void;
    type Window = u64;
    type XImage = core::ffi::c_void;

    #[repr(C)]
    struct XWindowAttributes {
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        border_width: i32,
        depth: i32,
        _padding: [u8; 256],
    }

    #[repr(C)]
    struct XImageData {
        width: i32,
        height: i32,
        xoffset: i32,
        format: i32,
        data: *mut i8,
        byte_order: i32,
        bitmap_unit: i32,
        bitmap_bit_order: i32,
        bitmap_pad: i32,
        depth: i32,
        bytes_per_line: i32,
        bits_per_pixel: i32,
    }

    // Function pointer types
    type XGetWindowAttributesFn =
        unsafe extern "C" fn(*mut Display, Window, *mut XWindowAttributes) -> i32;
    type XGetImageFn =
        unsafe extern "C" fn(*mut Display, Window, i32, i32, u32, u32, u64, i32) -> *mut XImage;
    type XDestroyImageFn = unsafe extern "C" fn(*mut XImage) -> i32;

    // Load libX11 dynamically
    let lib_name = CString::new("libX11.so.6").unwrap();
    let lib = unsafe { libc::dlopen(lib_name.as_ptr(), libc::RTLD_LAZY) };
    if lib.is_null() {
        // Try without version
        let lib_name2 = CString::new("libX11.so").unwrap();
        let lib = unsafe { libc::dlopen(lib_name2.as_ptr(), libc::RTLD_LAZY) };
        if lib.is_null() {
            return Err(AzString::from(
                "Failed to load libX11.so - X11 not available",
            ));
        }
    }

    // Reload with correct handle
    let lib = unsafe {
        let lib_name = CString::new("libX11.so.6").unwrap();
        let lib = libc::dlopen(lib_name.as_ptr(), libc::RTLD_LAZY);
        if lib.is_null() {
            let lib_name2 = CString::new("libX11.so").unwrap();
            libc::dlopen(lib_name2.as_ptr(), libc::RTLD_LAZY)
        } else {
            lib
        }
    };

    if lib.is_null() {
        return Err(AzString::from(
            "Failed to load libX11.so - X11 not available",
        ));
    }

    // Load function pointers
    let get_window_attrs: XGetWindowAttributesFn = unsafe {
        let sym_name = CString::new("XGetWindowAttributes").unwrap();
        let sym = libc::dlsym(lib, sym_name.as_ptr());
        if sym.is_null() {
            libc::dlclose(lib);
            return Err(AzString::from("Failed to find XGetWindowAttributes"));
        }
        std::mem::transmute(sym)
    };

    let get_image: XGetImageFn = unsafe {
        let sym_name = CString::new("XGetImage").unwrap();
        let sym = libc::dlsym(lib, sym_name.as_ptr());
        if sym.is_null() {
            libc::dlclose(lib);
            return Err(AzString::from("Failed to find XGetImage"));
        }
        std::mem::transmute(sym)
    };

    let destroy_image: XDestroyImageFn = unsafe {
        let sym_name = CString::new("XDestroyImage").unwrap();
        let sym = libc::dlsym(lib, sym_name.as_ptr());
        if sym.is_null() {
            libc::dlclose(lib);
            return Err(AzString::from("Failed to find XDestroyImage"));
        }
        std::mem::transmute(sym)
    };

    let result = unsafe {
        let mut attr: XWindowAttributes = core::mem::zeroed();
        if get_window_attrs(display, window, &mut attr) == 0 {
            libc::dlclose(lib);
            return Err(AzString::from("Failed to get window attributes"));
        }

        let width = attr.width as u32;
        let height = attr.height as u32;

        if width == 0 || height == 0 {
            libc::dlclose(lib);
            return Err(AzString::from("Invalid window dimensions"));
        }

        // ZPixmap = 2, AllPlanes = !0
        let image = get_image(display, window, 0, 0, width, height, !0u64, 2);
        if image.is_null() {
            libc::dlclose(lib);
            return Err(AzString::from("XGetImage failed"));
        }

        let img = &*(image as *const XImageData);

        // Extract pixel data
        let mut pixels: Vec<u8> = Vec::with_capacity((width * height * 4) as usize);

        for y in 0..height {
            for x in 0..width {
                let offset =
                    (y as i32 * img.bytes_per_line + x as i32 * (img.bits_per_pixel / 8)) as isize;
                let pixel_ptr = img.data.offset(offset) as *const u8;

                // BGRA format (common on X11 with 32-bit depth)
                let b = *pixel_ptr;
                let g = *pixel_ptr.offset(1);
                let r = *pixel_ptr.offset(2);
                let a = if img.bits_per_pixel == 32 {
                    *pixel_ptr.offset(3)
                } else {
                    255
                };

                pixels.push(r);
                pixels.push(g);
                pixels.push(b);
                pixels.push(a);
            }
        }

        destroy_image(image);

        // Create PNG using tiny-skia
        let pixmap = tiny_skia::Pixmap::from_vec(
            pixels,
            tiny_skia::IntSize::from_wh(width, height)
                .ok_or_else(|| AzString::from("Invalid image dimensions"))?,
        )
        .ok_or_else(|| AzString::from("Failed to create pixmap"))?;

        let png_data = pixmap
            .encode_png()
            .map_err(|e| AzString::from(format!("PNG encoding failed: {}", e)))?;

        std::fs::write(path, png_data)
            .map_err(|e| AzString::from(format!("Failed to write file: {}", e)))?;

        Ok(())
    };

    // Close library
    unsafe {
        libc::dlclose(lib);
    }

    result
}

/// Take a native screenshot on Linux/X11 using xcb
#[cfg(target_os = "linux")]
fn take_native_screenshot_xcb(
    connection: *mut core::ffi::c_void,
    window: u32,
    path: &str,
) -> Result<(), AzString> {
    if connection.is_null() {
        return Err(AzString::from("Invalid XCB connection"));
    }

    Err(AzString::from(
        "XCB screenshot not yet implemented - please use X11/Xlib backend",
    ))
}
