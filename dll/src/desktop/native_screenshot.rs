//! Native screenshot extension trait for CallbackInfo
//!
//! This module provides the `NativeScreenshotExt` trait that extends `CallbackInfo`
//! with native OS-level screenshot capabilities. The implementation uses dlopen
//! at runtime to avoid static linking to X11 on Linux.

use azul_css::AzString;
use azul_layout::callbacks::CallbackInfo;

#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
fn encode_rgba_png(pixels: Vec<u8>, width: u32, height: u32) -> Result<Vec<u8>, String> {
    let mut buf = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut buf, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(|e| format!("PNG header error: {}", e))?;
        writer.write_image_data(&pixels).map_err(|e| format!("PNG write error: {}", e))?;
    }
    Ok(buf)
}

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
    /// - **macOS**: Uses `CGWindowListCreateImage` (Core Graphics)
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
    /// instead of saving to a file. The capture is performed entirely in
    /// memory — no temporary files are touched.
    fn take_native_screenshot_bytes(&self) -> Result<Vec<u8>, AzString>;

    /// Take a native OS-level screenshot and return as a Base64 data URI
    ///
    /// Returns the screenshot as a "data:image/png;base64,..." string.
    fn take_native_screenshot_base64(&self) -> Result<AzString, AzString>;
}

impl NativeScreenshotExt for CallbackInfo {
    fn take_native_screenshot(&self, path: &str) -> Result<(), AzString> {
        let png_bytes = NativeScreenshotExt::take_native_screenshot_bytes(self)?;
        std::fs::write(path, png_bytes)
            .map_err(|e| AzString::from(format!("Failed to write file: {}", e)))?;
        Ok(())
    }

    fn take_native_screenshot_bytes(&self) -> Result<Vec<u8>, AzString> {
        use azul_core::window::RawWindowHandle;

        let window_handle = self.get_current_window_handle();

        match window_handle {
            #[cfg(target_os = "macos")]
            RawWindowHandle::MacOS(handle) => take_native_screenshot_macos_bytes(handle.ns_window),
            #[cfg(target_os = "windows")]
            RawWindowHandle::Windows(handle) => take_native_screenshot_windows_bytes(handle.hwnd),
            #[cfg(target_os = "linux")]
            RawWindowHandle::Xlib(handle) => {
                take_native_screenshot_xlib_bytes(handle.display, handle.window)
            }
            #[cfg(target_os = "linux")]
            RawWindowHandle::Xcb(handle) => {
                take_native_screenshot_xcb_bytes(handle.connection, handle.window)
            }
            #[cfg(target_os = "linux")]
            RawWindowHandle::Wayland(_) => Err(AzString::from(
                "Native screenshot not supported on Wayland - use X11/Xlib backend",
            )),
            _ => Err(AzString::from(
                "Native screenshot not supported on this platform",
            )),
        }
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

/// Take a native screenshot on macOS using CGWindowListCreateImage.
///
/// Captures the target window's contents (including frame) entirely in memory
/// and encodes the result as PNG without touching the filesystem.
#[cfg(target_os = "macos")]
fn take_native_screenshot_macos_bytes(
    ns_window: *mut core::ffi::c_void,
) -> Result<Vec<u8>, AzString> {
    use core::ffi::c_void;

    if ns_window.is_null() {
        return Err(AzString::from("Invalid window handle"));
    }

    type CGWindowID = u32;
    type CGImageRef = *mut c_void;
    type CGDataProviderRef = *mut c_void;
    type CFDataRef = *const c_void;
    type CFTypeRef = *const c_void;
    type CGFloat = f64;
    type CGWindowListOption = u32;
    type CGWindowImageOption = u32;

    // kCGWindowListOptionIncludingWindow: capture only the named window.
    const KCG_WINDOW_LIST_OPTION_INCLUDING_WINDOW: CGWindowListOption = 1 << 3;
    // kCGWindowImageBoundsIgnoreFraming: exclude the drop shadow Apple draws
    // around windows, matching the prior `screencapture -x` behavior.
    const KCG_WINDOW_IMAGE_BOUNDS_IGNORE_FRAMING: CGWindowImageOption = 1 << 0;

    #[repr(C)]
    struct CGPoint {
        x: CGFloat,
        y: CGFloat,
    }
    #[repr(C)]
    struct CGSize {
        width: CGFloat,
        height: CGFloat,
    }
    #[repr(C)]
    struct CGRect {
        origin: CGPoint,
        size: CGSize,
    }

    // Declare objc_msgSend as a non-variadic function pointer to ensure
    // correct calling convention on ARM64 macOS (variadic and non-variadic
    // functions use different ABIs on aarch64-apple-darwin).
    type ObjcMsgSendWindowNumberFn =
        unsafe extern "C" fn(receiver: *mut c_void, sel: *const c_void) -> i64;

    #[link(name = "objc")]
    extern "C" {
        fn objc_msgSend();
        fn sel_registerName(name: *const i8) -> *const c_void;
    }

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGWindowListCreateImage(
            screenBounds: CGRect,
            listOption: CGWindowListOption,
            windowID: CGWindowID,
            imageOption: CGWindowImageOption,
        ) -> CGImageRef;
        fn CGImageGetWidth(image: CGImageRef) -> usize;
        fn CGImageGetHeight(image: CGImageRef) -> usize;
        fn CGImageGetBytesPerRow(image: CGImageRef) -> usize;
        fn CGImageGetBitsPerPixel(image: CGImageRef) -> usize;
        fn CGImageGetDataProvider(image: CGImageRef) -> CGDataProviderRef;
        fn CGImageRelease(image: CGImageRef);
        fn CGDataProviderCopyData(provider: CGDataProviderRef) -> CFDataRef;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFDataGetLength(data: CFDataRef) -> isize;
        fn CFDataGetBytePtr(data: CFDataRef) -> *const u8;
        fn CFRelease(cf: CFTypeRef);
    }

    unsafe {
        let sel = sel_registerName(b"windowNumber\0".as_ptr() as *const i8);
        let msg_send: ObjcMsgSendWindowNumberFn =
            std::mem::transmute(objc_msgSend as *const ());
        let window_id = msg_send(ns_window, sel);

        if window_id <= 0 {
            return Err(AzString::from("Failed to get window ID"));
        }

        // CGRectNull — sentinel value telling CGWindowListCreateImage to use
        // the captured window's natural bounds.
        let null_rect = CGRect {
            origin: CGPoint {
                x: f64::INFINITY,
                y: f64::INFINITY,
            },
            size: CGSize {
                width: 0.0,
                height: 0.0,
            },
        };

        let image = CGWindowListCreateImage(
            null_rect,
            KCG_WINDOW_LIST_OPTION_INCLUDING_WINDOW,
            window_id as CGWindowID,
            KCG_WINDOW_IMAGE_BOUNDS_IGNORE_FRAMING,
        );

        if image.is_null() {
            return Err(AzString::from("CGWindowListCreateImage failed"));
        }

        let result = (|| -> Result<Vec<u8>, AzString> {
            let width = CGImageGetWidth(image);
            let height = CGImageGetHeight(image);
            let bytes_per_row = CGImageGetBytesPerRow(image);
            let bits_per_pixel = CGImageGetBitsPerPixel(image);

            if width == 0 || height == 0 {
                return Err(AzString::from("Captured image has zero dimensions"));
            }
            if bits_per_pixel != 32 {
                return Err(AzString::from(
                    "Unsupported pixel format from CGWindowListCreateImage",
                ));
            }

            let provider = CGImageGetDataProvider(image);
            if provider.is_null() {
                return Err(AzString::from("Failed to get CGImage data provider"));
            }

            let data = CGDataProviderCopyData(provider);
            if data.is_null() {
                return Err(AzString::from("Failed to copy CGImage pixel data"));
            }

            let inner = (|| -> Result<Vec<u8>, AzString> {
                let length = CFDataGetLength(data);
                let byte_ptr = CFDataGetBytePtr(data);
                if byte_ptr.is_null() || length <= 0 {
                    return Err(AzString::from("CGImage pixel data is empty"));
                }
                let length = length as usize;

                let mut pixels: Vec<u8> = Vec::with_capacity(width * height * 4);
                for y in 0..height {
                    let row_start = y * bytes_per_row;
                    for x in 0..width {
                        let pixel_off = row_start + x * 4;
                        if pixel_off + 3 >= length {
                            return Err(AzString::from(
                                "CGImage pixel data shorter than expected",
                            ));
                        }
                        // CGWindowListCreateImage returns BGRA in host byte
                        // order with kCGImageAlphaPremultipliedFirst. Window
                        // captures are fully opaque (a == 255), so swizzle
                        // BGRA -> RGBA without unpremultiplication.
                        let b = *byte_ptr.add(pixel_off);
                        let g = *byte_ptr.add(pixel_off + 1);
                        let r = *byte_ptr.add(pixel_off + 2);
                        let a = *byte_ptr.add(pixel_off + 3);
                        pixels.push(r);
                        pixels.push(g);
                        pixels.push(b);
                        pixels.push(a);
                    }
                }

                encode_rgba_png(pixels, width as u32, height as u32).map_err(AzString::from)
            })();

            CFRelease(data);
            inner
        })();

        CGImageRelease(image);
        result
    }
}

/// Take a native screenshot on Windows using PrintWindow API
#[cfg(target_os = "windows")]
fn take_native_screenshot_windows_bytes(
    hwnd: *mut core::ffi::c_void,
) -> Result<Vec<u8>, AzString> {
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

        let result = (|| -> Result<Vec<u8>, AzString> {
            const PW_RENDERFULLCONTENT: u32 = 2;
            if PrintWindow(hwnd, mem_dc, PW_RENDERFULLCONTENT) == 0 {
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
                return Err(AzString::from("GetDIBits failed"));
            }

            // Convert BGRA to RGBA
            for chunk in pixels.chunks_exact_mut(4) {
                chunk.swap(0, 2);
            }

            encode_rgba_png(pixels, width as u32, height as u32).map_err(AzString::from)
        })();

        SelectObject(mem_dc, old_bitmap);
        DeleteObject(bitmap);
        DeleteDC(mem_dc);
        ReleaseDC(hwnd, window_dc);

        result
    }
}

/// Take a native screenshot on Linux/X11 using XGetImage via dlopen
#[cfg(target_os = "linux")]
fn take_native_screenshot_xlib_bytes(
    display: *mut core::ffi::c_void,
    window: u64,
) -> Result<Vec<u8>, AzString> {
    use std::ffi::CString;

    if display.is_null() {
        return Err(AzString::from("Invalid display handle"));
    }

    use core::ffi::{c_int, c_long, c_ulong, c_void};

    // X11 types
    type Display = c_void;
    type Window = u64;
    type XImage = c_void;

    #[repr(C)]
    struct XWindowAttributes {
        x: c_int,
        y: c_int,
        width: c_int,
        height: c_int,
        border_width: c_int,
        depth: c_int,
        visual: *mut c_void,
        root: c_ulong,
        class: c_int,
        bit_gravity: c_int,
        win_gravity: c_int,
        backing_store: c_int,
        backing_planes: c_ulong,
        backing_pixel: c_ulong,
        save_under: c_int,
        colormap: c_ulong,
        map_installed: c_int,
        map_state: c_int,
        all_event_masks: c_long,
        your_event_masks: c_long,
        do_not_propagate_mask: c_long,
        override_redirect: c_int,
        screen: *mut c_void,
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

    // Load libX11 dynamically. Miri can't call `dlopen`, so treat the library
    // as unavailable (null) under it and let the is_null() guard below bail.
    #[cfg(miri)]
    let lib: *mut core::ffi::c_void = core::ptr::null_mut();
    #[cfg(not(miri))]
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

    let result = unsafe {
        // Load function pointers
        let sym_name = CString::new("XGetWindowAttributes").unwrap();
        let sym = libc::dlsym(lib, sym_name.as_ptr());
        if sym.is_null() {
            libc::dlclose(lib);
            return Err(AzString::from("Failed to find XGetWindowAttributes"));
        }
        let get_window_attrs: XGetWindowAttributesFn = std::mem::transmute(sym);

        let sym_name = CString::new("XGetImage").unwrap();
        let sym = libc::dlsym(lib, sym_name.as_ptr());
        if sym.is_null() {
            libc::dlclose(lib);
            return Err(AzString::from("Failed to find XGetImage"));
        }
        let get_image: XGetImageFn = std::mem::transmute(sym);

        let sym_name = CString::new("XDestroyImage").unwrap();
        let sym = libc::dlsym(lib, sym_name.as_ptr());
        if sym.is_null() {
            libc::dlclose(lib);
            return Err(AzString::from("Failed to find XDestroyImage"));
        }
        let destroy_image: XDestroyImageFn = std::mem::transmute(sym);

        (|| -> Result<Vec<u8>, AzString> {
            let mut attr: XWindowAttributes = core::mem::zeroed();
            if get_window_attrs(display, window, &mut attr) == 0 {
                return Err(AzString::from("Failed to get window attributes"));
            }

            let width = attr.width as u32;
            let height = attr.height as u32;

            if width == 0 || height == 0 {
                return Err(AzString::from("Invalid window dimensions"));
            }

            // ZPixmap = 2, AllPlanes = !0
            let image = get_image(display, window, 0, 0, width, height, !0u64, 2);
            if image.is_null() {
                return Err(AzString::from("XGetImage failed"));
            }

            let img = &*(image as *const XImageData);

            // Extract pixel data
            let mut pixels: Vec<u8> = Vec::with_capacity((width * height * 4) as usize);

            for y in 0..height {
                for x in 0..width {
                    let offset = (y as i32 * img.bytes_per_line
                        + x as i32 * (img.bits_per_pixel / 8))
                        as isize;
                    let pixel_ptr = img.data.offset(offset) as *const u8;

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

            encode_rgba_png(pixels, width, height).map_err(AzString::from)
        })()
    };

    unsafe {
        libc::dlclose(lib);
    }

    result
}

/// Take a native screenshot on Linux/X11 using xcb
#[cfg(target_os = "linux")]
fn take_native_screenshot_xcb_bytes(
    connection: *mut core::ffi::c_void,
    _window: u32,
) -> Result<Vec<u8>, AzString> {
    if connection.is_null() {
        return Err(AzString::from("Invalid XCB connection"));
    }

    Err(AzString::from(
        "XCB screenshot not yet implemented - please use X11/Xlib backend",
    ))
}
