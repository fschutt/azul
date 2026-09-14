//! Native screenshot extension trait for CallbackInfo
//!
//! This module provides the `NativeScreenshotExt` trait that extends `CallbackInfo`
//! with native OS-level screenshot capabilities. The implementation uses dlopen
//! at runtime to avoid static linking to X11 on Linux.

use azul_css::AzString;
use azul_layout::callbacks::CallbackInfo;

#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
pub(crate) fn encode_rgba_png(pixels: Vec<u8>, width: u32, height: u32) -> Result<Vec<u8>, String> {
    let mut buf = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut buf, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|e| format!("PNG header error: {}", e))?;
        writer
            .write_image_data(&pixels)
            .map_err(|e| format!("PNG write error: {}", e))?;
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
            // A Wayland client cannot read the compositor's output, and with
            // server-side decorations the titlebar is not in our surface at
            // all. `ext-image-copy-capture-v1` is the way in; it identifies the
            // window by the title it is showing, so pass that. On a compositor
            // without the protocol this returns a descriptive `Err`, which is
            // what this arm always returned.
            #[cfg(target_os = "linux")]
            RawWindowHandle::Wayland(_) => {
                let title = self.get_current_window_state().title.as_str().to_string();
                crate::desktop::shell2::linux::wayland::screencopy::capture_toplevel(&title)
            }
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
        let msg_send: ObjcMsgSendWindowNumberFn = std::mem::transmute(objc_msgSend as *const ());
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
                            return Err(AzString::from("CGImage pixel data shorter than expected"));
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
fn take_native_screenshot_windows_bytes(hwnd: *mut core::ffi::c_void) -> Result<Vec<u8>, AzString> {
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

/// Set by the temporary X error handler installed around the decorated
/// (root-window) grab, so the caller can fall back to the plain client grab
/// instead of returning a half-read image. Process-global because Xlib's error
/// handler is process-global; the window that sets it is the one that just
/// called `XGetImage`, and captures are not concurrent.
#[cfg(target_os = "linux")]
static FRAME_GRAB_FAILED: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(false);

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
    type XQueryTreeFn = unsafe extern "C" fn(
        *mut Display,
        Window,
        *mut Window,
        *mut Window,
        *mut *mut Window,
        *mut u32,
    ) -> i32;
    type XFreeFn = unsafe extern "C" fn(*mut c_void) -> i32;
    type XErrorHandlerFn = unsafe extern "C" fn(*mut Display, *mut c_void) -> i32;
    type XSetErrorHandlerFn = unsafe extern "C" fn(Option<XErrorHandlerFn>) -> Option<XErrorHandlerFn>;
    type XSyncFn = unsafe extern "C" fn(*mut Display, i32) -> i32;

    /// Swallow the X error instead of letting Xlib's default handler call
    /// `exit()`. Installed only around the frame-window grab below: a window
    /// that moved partly off-screen between the geometry read and the
    /// `XGetImage` answers `BadMatch`, and a screenshot is never worth killing
    /// the application over.
    unsafe extern "C" fn swallow_x_error(_: *mut Display, _: *mut c_void) -> i32 {
        FRAME_GRAB_FAILED.store(true, core::sync::atomic::Ordering::SeqCst);
        0
    }

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

        // Optional: present on every real Xlib, absent under a stub. Missing
        // symbols simply disable the decorated path.
        let query_tree: Option<XQueryTreeFn> = {
            let n = CString::new("XQueryTree").unwrap();
            let sym = libc::dlsym(lib, n.as_ptr());
            if sym.is_null() { None } else { Some(std::mem::transmute(sym)) }
        };
        let x_free: Option<XFreeFn> = {
            let n = CString::new("XFree").unwrap();
            let sym = libc::dlsym(lib, n.as_ptr());
            if sym.is_null() { None } else { Some(std::mem::transmute(sym)) }
        };
        let set_error_handler: Option<XSetErrorHandlerFn> = {
            let n = CString::new("XSetErrorHandler").unwrap();
            let sym = libc::dlsym(lib, n.as_ptr());
            if sym.is_null() { None } else { Some(std::mem::transmute(sym)) }
        };
        let x_sync: Option<XSyncFn> = {
            let n = CString::new("XSync").unwrap();
            let sym = libc::dlsym(lib, n.as_ptr());
            if sym.is_null() { None } else { Some(std::mem::transmute(sym)) }
        };

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

            // ── The window as the USER sees it, decorations included ────────
            //
            // macOS (`CGWindowListCreateImage`) and Windows (`PrintWindow` on
            // the top-level HWND) both capture the window WITH its frame, so
            // the committed macOS screenshots carry a titlebar and the Linux
            // ones did not. Matching them on X11 takes two steps, and the
            // OBVIOUS one is wrong:
            //
            //  * WRONG: `XGetImage` on the WM frame window. Under a compositing
            //    WM (KWin, mutter, picom) the decorations are painted by the
            //    compositor's own scene and never land in the frame window's X
            //    drawable, so the titlebar strip comes back as whatever was
            //    last there — on a 300x253 Breeze frame, 28 px of the desktop
            //    behind it.
            //  * RIGHT: grab the ROOT window at the frame's rectangle. That is
            //    the composited output, which is what the user is looking at.
            //
            // The frame is found by walking `XQueryTree` up until the parent IS
            // the root (KWin nests client -> wrapper -> frame, so it is two
            // levels, not one). No WM, an override-redirect window, or Xvfb
            // with no WM at all: the walk stops immediately, frame == client,
            // and this degrades to exactly the old behaviour.
            //
            // The cost of grabbing the root is that it captures whatever is ON
            // SCREEN in that rectangle — an overlapping window lands in the
            // shot. That is the right trade for a screenshot of a window the
            // user can see, and under Xvfb (CI) nothing else is on screen.
            let mut src_window = window;
            let mut src_x = 0i32;
            let mut src_y = 0i32;
            let mut src_w = width;
            let mut src_h = height;

            if let (Some(query_tree), Some(x_free)) = (query_tree, x_free) {
                let root = attr.root;
                let mut cur = window;
                // Bounded: a reparenting WM uses one or two levels, and a cycle
                // here would hang the capture.
                for _ in 0..8 {
                    let mut r: Window = 0;
                    let mut parent: Window = 0;
                    let mut children: *mut Window = core::ptr::null_mut();
                    let mut nchildren: u32 = 0;
                    if query_tree(display, cur, &mut r, &mut parent, &mut children, &mut nchildren)
                        == 0
                    {
                        break;
                    }
                    if !children.is_null() {
                        x_free(children as *mut c_void);
                    }
                    if parent == 0 || parent == root || cur == root {
                        break;
                    }
                    cur = parent;
                }

                if cur != window && cur != root {
                    let mut frame: XWindowAttributes = core::mem::zeroed();
                    let mut root_attr: XWindowAttributes = core::mem::zeroed();
                    if get_window_attrs(display, cur, &mut frame) != 0
                        && get_window_attrs(display, root, &mut root_attr) != 0
                        && frame.width > 0
                        && frame.height > 0
                        // A child of the root has coordinates relative to the
                        // root, i.e. absolute. Only take the frame when it sits
                        // FULLY on screen: `XGetImage` past the root's edge is
                        // a `BadMatch`, and a half-window screenshot is not
                        // what anybody asked for either.
                        && frame.x >= 0
                        && frame.y >= 0
                        && frame.x + frame.width <= root_attr.width
                        && frame.y + frame.height <= root_attr.height
                    {
                        src_window = root;
                        src_x = frame.x;
                        src_y = frame.y;
                        src_w = frame.width as u32;
                        src_h = frame.height as u32;
                    }
                }
            }

            // ZPixmap = 2, AllPlanes = !0
            let mut image = if src_window == window {
                get_image(display, window, 0, 0, width, height, !0u64, 2)
            } else {
                FRAME_GRAB_FAILED.store(false, core::sync::atomic::Ordering::SeqCst);
                let prev = set_error_handler.map(|set| set(Some(swallow_x_error)));
                let img = get_image(display, src_window, src_x, src_y, src_w, src_h, !0u64, 2);
                // Force the round trip so a deferred error is attributed here
                // and not to some unrelated call later.
                if let Some(sync) = x_sync {
                    sync(display, 0);
                }
                if let (Some(set), Some(prev)) = (set_error_handler, prev) {
                    set(prev);
                }
                if FRAME_GRAB_FAILED.load(core::sync::atomic::Ordering::SeqCst) && !img.is_null() {
                    destroy_image(img);
                    core::ptr::null_mut()
                } else {
                    img
                }
            };

            // The decorated grab is best-effort: anything at all going wrong
            // with it falls back to the undecorated client window, which is
            // what this function always used to return.
            let (width, height) = if image.is_null() && src_window != window {
                image = get_image(display, window, 0, 0, width, height, !0u64, 2);
                (width, height)
            } else if src_window != window {
                (src_w, src_h)
            } else {
                (width, height)
            };

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
                    // DEPTH decides whether the fourth byte is alpha, not
                    // bits_per_pixel. A depth-24 drawable is stored 32 bits
                    // wide with the top byte as PADDING, and X leaves padding
                    // undefined — in practice zero. Reading it as alpha turned
                    // the decorated capture (the ROOT window, depth 24 here)
                    // into a fully transparent PNG that renders as a blank
                    // white rectangle, while the client window — a 32-bit ARGB
                    // visual, where the byte really is alpha — came out fine.
                    let a = if img.bits_per_pixel == 32 && img.depth == 32 {
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
