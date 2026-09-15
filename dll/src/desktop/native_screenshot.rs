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

    /// [`Self::take_native_screenshot_bytes`], with the drop shadow decided per
    /// call instead of by the environment.
    ///
    /// `None` keeps the [`screenshot_includes_shadow`] default. This is a
    /// separate method rather than a parameter on the original because the
    /// three methods above are part of the published FFI surface (`api.json`),
    /// and the scenario runner is the only caller that needs to choose.
    fn take_native_screenshot_bytes_with(
        &self,
        render_shadow: Option<bool>,
    ) -> Result<Vec<u8>, AzString>;

    /// [`Self::take_native_screenshot_base64`] with the same per-call override.
    fn take_native_screenshot_base64_with(
        &self,
        render_shadow: Option<bool>,
    ) -> Result<AzString, AzString>;
}

impl NativeScreenshotExt for CallbackInfo {
    fn take_native_screenshot(&self, path: &str) -> Result<(), AzString> {
        let png_bytes = NativeScreenshotExt::take_native_screenshot_bytes(self)?;
        std::fs::write(path, png_bytes)
            .map_err(|e| AzString::from(format!("Failed to write file: {}", e)))?;
        Ok(())
    }

    fn take_native_screenshot_bytes(&self) -> Result<Vec<u8>, AzString> {
        NativeScreenshotExt::take_native_screenshot_bytes_with(self, None)
    }

    fn take_native_screenshot_base64_with(
        &self,
        render_shadow: Option<bool>,
    ) -> Result<AzString, AzString> {
        let png_bytes = NativeScreenshotExt::take_native_screenshot_bytes_with(self, render_shadow)?;
        let base64_str = azul_layout::callbacks::base64_encode(&png_bytes);
        Ok(AzString::from(format!(
            "data:image/png;base64,{}",
            base64_str
        )))
    }

    fn take_native_screenshot_bytes_with(
        &self,
        render_shadow: Option<bool>,
    ) -> Result<Vec<u8>, AzString> {
        use azul_core::window::RawWindowHandle;

        let _ = &render_shadow;
        let window_handle = self.get_current_window_handle();

        match window_handle {
            #[cfg(target_os = "macos")]
            RawWindowHandle::MacOS(handle) => take_native_screenshot_macos_bytes(handle.ns_window),
            #[cfg(target_os = "windows")]
            RawWindowHandle::Windows(handle) => {
                take_native_screenshot_windows_bytes(handle.hwnd, wants_shadow(render_shadow))
            }
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
                // KWin first. KDE declined the ext-foreign-toplevel-list
                // protocol the portable path needs (bugs.kde.org 483227, NOT A
                // BUG) and has no ext-image-copy-capture either; its supported
                // capture is its own ScreenShot2 D-Bus interface, present on
                // Plasma 5.27 and 6 alike. The ext path stays for wlroots-style
                // compositors (sway, niri, labwc, ...). Both errors are kept so a
                // failure says why EACH route was unavailable.
                match take_native_screenshot_kwin_bytes(&title) {
                    Ok(png) => Ok(png),
                    Err(kwin) => {
                        match crate::desktop::shell2::linux::wayland::screencopy::capture_toplevel(
                            &title,
                        ) {
                            Ok(png) => Ok(png),
                            Err(ext) => Err(AzString::from(format!(
                                "no Wayland capture route worked. KWin ScreenShot2: {}. \
                                 ext-image-copy-capture: {}",
                                kwin.as_str(),
                                ext.as_str()
                            ))),
                        }
                    }
                }
            }
            _ => Err(AzString::from(
                "Native screenshot not supported on this platform",
            )),
        }
    }

    fn take_native_screenshot_base64(&self) -> Result<AzString, AzString> {
        // Explicitly call the trait method, not the inherent method on CallbackInfo
        NativeScreenshotExt::take_native_screenshot_base64_with(self, None)
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
    // around windows. Only set when AZ_SCREENSHOT_SHADOW turns the shadow off —
    // see `screenshot_includes_shadow`.
    const KCG_WINDOW_IMAGE_BOUNDS_IGNORE_FRAMING: CGWindowImageOption = 1 << 0;
    const KCG_WINDOW_IMAGE_DEFAULT: CGWindowImageOption = 0;

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
            if screenshot_includes_shadow() {
                KCG_WINDOW_IMAGE_DEFAULT
            } else {
                KCG_WINDOW_IMAGE_BOUNDS_IGNORE_FRAMING
            },
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
fn take_native_screenshot_windows_bytes(
    hwnd: *mut core::ffi::c_void,
    render_shadow: bool,
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

    // DWMWA_EXTENDED_FRAME_BOUNDS. `GetWindowRect` on Windows 10/11 reports the
    // window's INPUT bounds, which include the invisible resize border the DWM
    // draws nothing into — about 8 px each side and below on a standard frame.
    // Capturing that rect put black bands down the left, right and bottom of
    // every screenshot: `PrintWindow` renders only the visible frame, leaving
    // the rest of the bitmap at its initial (black) contents. This attribute is
    // the rect the window actually OCCUPIES on screen.
    const DWMWA_EXTENDED_FRAME_BOUNDS: u32 = 9;

    #[link(name = "dwmapi")]
    extern "system" {
        fn DwmGetWindowAttribute(
            hwnd: HWND,
            dwAttribute: u32,
            pvAttribute: *mut core::ffi::c_void,
            cbAttribute: u32,
        ) -> i32;
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

        // The visible frame, as an offset + size INSIDE the window rect.
        // PrintWindow always renders the window at the origin of the target
        // DC, so the bitmap stays window-rect sized and the crop happens when
        // the pixels are read back. Falls back to the whole rect if the DWM
        // declines the attribute (it is composition-dependent).
        let (crop_x, crop_y, crop_w, crop_h) = {
            let mut efb = RECT {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            };
            let ok = DwmGetWindowAttribute(
                hwnd,
                DWMWA_EXTENDED_FRAME_BOUNDS,
                (&mut efb as *mut RECT).cast(),
                core::mem::size_of::<RECT>() as u32,
            ) == 0;
            let (x, y, w, h) = (
                efb.left - rect.left,
                efb.top - rect.top,
                efb.right - efb.left,
                efb.bottom - efb.top,
            );
            // Only trust it when it really is a sub-rect of what we captured.
            if ok && x >= 0 && y >= 0 && w > 0 && h > 0 && x + w <= width && y + h <= height {
                (x, y, w, h)
            } else {
                (0, 0, width, height)
            }
        };

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

            // Crop to the visible frame and convert BGRA to RGBA in one pass.
            // The full-frame case (no usable DWM bounds) copies row-for-row.
            let mut out = Vec::with_capacity((crop_w * crop_h * 4) as usize);
            for row in 0..crop_h as usize {
                let start = (row + crop_y as usize) * row_bytes + (crop_x as usize) * 4;
                out.extend_from_slice(&pixels[start..start + (crop_w as usize) * 4]);
            }
            for chunk in out.chunks_exact_mut(4) {
                chunk.swap(0, 2);
            }
            // `PrintWindow` renders the window and nothing else, and the DWM's
            // own drop shadow is not readable through any public API — a screen
            // grab of the margin would bring the desktop with it instead of
            // transparency, which is exactly why the X11 path refuses to
            // include one. So the shadow is DRAWN here, onto a transparent
            // margin, when the caller asks for it.
            let (out, crop_w, crop_h) = if render_shadow {
                add_synthetic_shadow(out, crop_w as u32, crop_h as u32)
            } else {
                (out, crop_w as u32, crop_h as u32)
            };

            encode_rgba_png(out, crop_w, crop_h).map_err(AzString::from)
        })();

        SelectObject(mem_dc, old_bitmap);
        DeleteObject(bitmap);
        DeleteDC(mem_dc);
        ReleaseDC(hwnd, window_dc);

        result
    }
}

/// Capture this window through KWin's `org.kde.KWin.ScreenShot2` D-Bus
/// interface, decorations included, and return it as PNG bytes.
///
/// The route KDE actually supports for a client-initiated window capture. KWin
/// has no `ext-image-copy-capture-v1`, and it declined the
/// `ext-foreign-toplevel-list-v1` a portable window capture needs, so on KDE
/// this is the ONLY way to get a window with its server-side titlebar. The
/// interface exists on Plasma 5.27 and Plasma 6.
///
/// Three things the KWin source (`effects/screenshot/screenshotdbusinterface2`)
/// decides, and this function follows:
///
///  * AUTHORIZATION. KWin resolves the caller's PID to `/proc/<pid>/exe` and looks for an installed
///    `.desktop` application whose `Exec` is that same file and whose
///    `X-KDE-DBUS-Restricted-Interfaces` lists `org.kde.KWin.ScreenShot2`. Anything else gets
///    `org.kde.KWin.ScreenShot2.Error.NoAuthorized` — reported below with the exact executable
///    path to authorize.
///  * ORDER. KWin sends the D-Bus reply (the metadata) FIRST and writes the pixels into the pipe
///    afterwards, from a worker thread. So: take the reply, close our copy of the write end, then
///    read to EOF. Reading before the reply would deadlock on anything larger than the pipe
///    buffer.
///  * WHICH WINDOW. `CaptureActiveWindow` captures whatever is focused. The reply names the window
///    it captured (`windowId`), and `getWindowInfo` turns that into its caption; if that is not
///    this window's title, the pixels belong to someone else and are NOT returned.
#[cfg(target_os = "linux")]
pub(crate) fn take_native_screenshot_kwin_bytes(title: &str) -> Result<Vec<u8>, AzString> {
    use std::{
        collections::HashMap,
        io::Read,
        os::fd::{AsFd, FromRawFd, OwnedFd},
    };

    use zbus::{
        blocking::{Connection, Proxy},
        zvariant::{Fd, OwnedValue, Value},
    };

    let conn = Connection::session()
        .map_err(|e| AzString::from(format!("no D-Bus session bus ({e})")))?;
    let shot = Proxy::new(
        &conn,
        "org.kde.KWin",
        "/org/kde/KWin/ScreenShot2",
        "org.kde.KWin.ScreenShot2",
    )
    .map_err(|e| AzString::from(format!("no org.kde.KWin.ScreenShot2 ({e})")))?;

    // This function does NOT activate the window, and must not. Activating a
    // window makes KWin PING it (`sendPing(PingReason::FocusWindow)`), and the
    // pong can only go out once this thread returns to the event loop — which
    // it cannot while it is blocked in this capture. KWin then marks the window
    // unresponsive and appends "(Not Responding)" to its title, and
    // `CaptureActiveWindow` photographs exactly that. Whoever needs the window
    // focused (scripts/screenshot_single.sh does it through a KWin script, by
    // PID, while the app is idle) has to do it BEFORE calling this.

    let mut fds = [0 as libc::c_int; 2];
    if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) } != 0 {
        return Err(AzString::from("pipe2 failed"));
    }
    let read_end = unsafe { OwnedFd::from_raw_fd(fds[0]) };
    let write_end = unsafe { OwnedFd::from_raw_fd(fds[1]) };

    let mut options: HashMap<&str, Value<'_>> = HashMap::new();
    options.insert("include-decoration", Value::from(true));
    options.insert("include-cursor", Value::from(false));
    options.insert("native-resolution", Value::from(true));

    let reply: HashMap<String, OwnedValue> = shot
        .call(
            "CaptureActiveWindow",
            &(options, Fd::from(write_end.as_fd())),
        )
        .map_err(|e| {
            let text = e.to_string();
            if text.contains("NoAuthorized") {
                let exe = std::fs::read_link("/proc/self/exe")
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|_| "<this executable>".to_string());
                AzString::from(format!(
                    "KWin refused: not authorized. KWin only answers ScreenShot2 for an \
                     application whose installed .desktop file has Exec={exe} and \
                     X-KDE-DBUS-Restricted-Interfaces=org.kde.KWin.ScreenShot2 (then run \
                     kbuildsycoca5/kbuildsycoca6)"
                ))
            } else {
                AzString::from(format!("CaptureActiveWindow failed ({text})"))
            }
        })?;
    // Our copy of the write end. KWin holds its own (passed over the socket) and
    // closes it when the pixels are written; until OUR copy is closed too, the
    // read below would never see EOF.
    drop(write_end);

    let mut raw = Vec::new();
    std::fs::File::from(read_end)
        .read_to_end(&mut raw)
        .map_err(|e| AzString::from(format!("reading the capture pipe failed ({e})")))?;

    let get_u32 = |key: &str| -> Result<u32, AzString> {
        reply
            .get(key)
            .and_then(|v| u32::try_from(v).ok())
            .ok_or_else(|| AzString::from(format!("reply has no u32 `{key}`")))
    };
    let (width, height, stride, format) = (
        get_u32("width")?,
        get_u32("height")?,
        get_u32("stride")?,
        get_u32("format")?,
    );

    // Is it OUR window? Never hand back another application's pixels.
    let window_id = reply
        .get("windowId")
        .and_then(|v| <&str>::try_from(v).ok())
        .map(str::to_string)
        .ok_or_else(|| AzString::from("reply has no windowId"))?;
    let kwin = Proxy::new(&conn, "org.kde.KWin", "/KWin", "org.kde.KWin")
        .map_err(|e| AzString::from(format!("no org.kde.KWin ({e})")))?;
    let info: HashMap<String, OwnedValue> = kwin
        .call("getWindowInfo", &(window_id.as_str()))
        .map_err(|e| AzString::from(format!("getWindowInfo failed ({e})")))?;
    let caption = info
        .get("caption")
        .and_then(|v| <&str>::try_from(v).ok())
        .unwrap_or("")
        .to_string();
    if caption != title {
        return Err(AzString::from(format!(
            "KWin captured the ACTIVE window, which is {caption:?}, not this window \
             ({title:?}); refusing to return another window's pixels — focus this window first"
        )));
    }

    let (w, h, st) = (width as usize, height as usize, stride as usize);
    if w == 0 || h == 0 || st < w * 4 || raw.len() < st * h {
        return Err(AzString::from(format!(
            "capture is truncated or malformed: {w}x{h} stride {st}, {} bytes",
            raw.len()
        )));
    }

    // QImage::Format values KWin produces. The 32-bit ARGB family is a native
    // (little-endian) 0xAARRGGBB, i.e. B,G,R,A in memory; the RGBA8888 family is
    // R,G,B,A in memory regardless of endianness.
    const FORMAT_RGB32: u32 = 4;
    const FORMAT_ARGB32: u32 = 5;
    const FORMAT_ARGB32_PREMULTIPLIED: u32 = 6;
    const FORMAT_RGBX8888: u32 = 16;
    const FORMAT_RGBA8888: u32 = 17;
    const FORMAT_RGBA8888_PREMULTIPLIED: u32 = 18;

    let bgra = matches!(format, FORMAT_RGB32 | FORMAT_ARGB32 | FORMAT_ARGB32_PREMULTIPLIED);
    let rgba = matches!(
        format,
        FORMAT_RGBX8888 | FORMAT_RGBA8888 | FORMAT_RGBA8888_PREMULTIPLIED
    );
    if !bgra && !rgba {
        return Err(AzString::from(format!(
            "unsupported QImage format {format} in the capture"
        )));
    }
    let opaque = matches!(format, FORMAT_RGB32 | FORMAT_RGBX8888);
    let premultiplied = matches!(
        format,
        FORMAT_ARGB32_PREMULTIPLIED | FORMAT_RGBA8888_PREMULTIPLIED
    );

    let mut pixels = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        let row = &raw[y * st..y * st + w * 4];
        for px in row.chunks_exact(4) {
            let (mut r, mut g, mut b, a) = if bgra {
                (px[2], px[1], px[0], if opaque { 255 } else { px[3] })
            } else {
                (px[0], px[1], px[2], if opaque { 255 } else { px[3] })
            };
            if premultiplied && a != 0 && a != 255 {
                let un = |c: u8| ((c as u32 * 255 + a as u32 / 2) / a as u32).min(255) as u8;
                r = un(r);
                g = un(g);
                b = un(b);
            }
            pixels.extend_from_slice(&[r, g, b, a]);
        }
    }

    // The compositor's DROP SHADOW, kept or cropped per AZ_SCREENSHOT_SHADOW.
    // `include-decoration` hands back the window as KWin paints it — shadow
    // included, on a transparent margin (a 400x328 window came back 530x458) —
    // and KWin 5.27 has no option to leave it out, so removing it is a crop.
    // The window is opaque and the shadow is not:
    // the bounding box of the near-opaque pixels IS the window rectangle. (A
    // translucent window would defeat that, so if no such box exists the capture
    // is returned uncropped rather than guessed at.)
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0usize, 0usize);
    for y in 0..h {
        for x in 0..w {
            if pixels[(y * w + x) * 4 + 3] >= 250 {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
        }
    }
    let crop = !screenshot_includes_shadow();
    let (pixels, width, height) = if crop
        && x1 >= x0
        && y1 >= y0
        && (x0, y0, x1, y1) != (0, 0, w - 1, h - 1)
    {
        let (cw, ch) = (x1 - x0 + 1, y1 - y0 + 1);
        let mut cropped = Vec::with_capacity(cw * ch * 4);
        for y in y0..=y1 {
            cropped.extend_from_slice(&pixels[(y * w + x0) * 4..(y * w + x1 + 1) * 4]);
        }
        (cropped, cw as u32, ch as u32)
    } else {
        (pixels, width, height)
    };

    encode_rgba_png(pixels, width, height).map_err(AzString::from)
}

/// Whether a native window screenshot keeps the compositor's DROP SHADOW.
///
/// `AZ_SCREENSHOT_SHADOW=0` (or `false` / `off` / `no`) strips it; anything else,
/// including unset, keeps it. On by default because the website screenshots look
/// better with it (USER RULING 2026-09-14) — the shadow sits on a transparent
/// margin, so the page shows it over whatever it is placed on.
///
/// Honoured where the platform can deliver both: KWin (ScreenShot2 hands back
/// the shadow and it is cropped off when not wanted) and macOS
/// (`kCGWindowImageBoundsIgnoreFraming`). The X11 capture reads the composited
/// ROOT, where a shadow would come with the desktop wallpaper behind it rather
/// than transparency, so it never includes one.
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
pub(crate) fn screenshot_includes_shadow() -> bool {
    !matches!(
        std::env::var("AZ_SCREENSHOT_SHADOW")
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str(),
        "0" | "false" | "off" | "no"
    )
}

/// Resolve a per-capture `render_shadow` request against the environment
/// default. `Some(_)` is the caller's explicit choice (the `render_shadow`
/// field on the `take_native_screenshot` scenario op); `None` falls back to
/// [`screenshot_includes_shadow`].
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
pub(crate) fn wants_shadow(render_shadow: Option<bool>) -> bool {
    render_shadow.unwrap_or_else(screenshot_includes_shadow)
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

    // X11 types. `Window` and `plane_mask` are C `unsigned long` — 64 bits on
    // x86_64 but 32 on i686 and armv7. They were spelled `u64`, which was an
    // ABI mismatch on 32-bit targets all along (Xlib read half of each u64) and
    // became a compile error there once the frame walk compared a `Window`
    // against `XWindowAttributes::root` (a real `c_ulong`). X11 resource IDs
    // are 29-bit, so narrowing the public `u64` handle once, here, loses nothing.
    type Display = c_void;
    type Window = c_ulong;
    type XImage = c_void;
    let window = window as Window;

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
        unsafe extern "C" fn(*mut Display, Window, i32, i32, u32, u32, c_ulong, i32) -> *mut XImage;
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
                get_image(display, window, 0, 0, width, height, c_ulong::MAX, 2)
            } else {
                FRAME_GRAB_FAILED.store(false, core::sync::atomic::Ordering::SeqCst);
                let prev = set_error_handler.map(|set| set(Some(swallow_x_error)));
                let img = get_image(display, src_window, src_x, src_y, src_w, src_h, c_ulong::MAX, 2);
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
                image = get_image(display, window, 0, 0, width, height, c_ulong::MAX, 2);
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

/// Wrap an RGBA window capture in a transparent margin carrying a drawn drop
/// shadow, and return the enlarged buffer.
///
/// The DWM's real shadow cannot be captured. `PrintWindow` renders the window
/// alone, the shadow lives OUTSIDE the window's bounds, and no public Windows
/// API hands it back with an alpha channel — a screen grab of the margin gets
/// the desktop wallpaper behind it instead of transparency, which is the same
/// reason `take_native_screenshot_xlib_bytes` never includes one. What the
/// website wants from a shadow is the LOOK, so this draws one.
///
/// It is an approximation on purpose, shaped to match the Windows 11 frame
/// shadow: a rounded-rectangle silhouette the size of the window, pushed down
/// by `OFFSET_Y`, blurred, and laid down in black at `PEAK_ALPHA`. The blur is
/// three box passes, which converges on a Gaussian closely enough that no
/// banding is visible at these radii and costs two linear scans per axis.
///
/// The window itself is composited on top at full opacity, so the frame is the
/// real capture — only the margin is synthetic.
#[cfg(target_os = "windows")]
fn add_synthetic_shadow(rgba: Vec<u8>, w: u32, h: u32) -> (Vec<u8>, u32, u32) {
    /// Transparent margin on every side, in px. Must exceed
    /// `BLUR_RADIUS + OFFSET_Y` or the shadow is clipped at the bottom.
    const MARGIN: u32 = 40;
    /// Box-blur radius per pass.
    const BLUR_RADIUS: u32 = 9;
    /// Downward offset — Windows 11 lights windows from above.
    const OFFSET_Y: u32 = 6;
    /// Darkest the shadow ever gets, as an alpha byte.
    const PEAK_ALPHA: f32 = 64.0;
    /// Corner radius of the silhouette, matching the Win11 frame.
    const CORNER: i32 = 8;

    if w == 0 || h == 0 {
        return (rgba, w, h);
    }

    let ow = w + MARGIN * 2;
    let oh = h + MARGIN * 2;
    let (ow_us, oh_us) = (ow as usize, oh as usize);

    // ── The silhouette ──────────────────────────────────────────────────
    // One coverage byte per pixel: the window's rounded rect, already at its
    // final offset so the blur below carries the offset with it.
    let mut mask = vec![0u8; ow_us * oh_us];
    let x0 = MARGIN as i32;
    let y0 = (MARGIN + OFFSET_Y) as i32;
    let x1 = x0 + w as i32;
    let y1 = y0 + h as i32;
    for y in y0.max(0)..y1.min(oh as i32) {
        for x in x0.max(0)..x1.min(ow as i32) {
            // Distance into the rect from each edge; a pixel is outside the
            // rounded corner when it is within CORNER of TWO adjacent edges
            // and beyond the quarter-circle joining them.
            let dx = (x - x0).min(x1 - 1 - x);
            let dy = (y - y0).min(y1 - 1 - y);
            if dx < CORNER && dy < CORNER {
                let ex = (CORNER - dx) as f32;
                let ey = (CORNER - dy) as f32;
                if ex * ex + ey * ey > (CORNER * CORNER) as f32 {
                    continue;
                }
            }
            mask[y as usize * ow_us + x as usize] = 255;
        }
    }

    // ── Blur ────────────────────────────────────────────────────────────
    // Separable box blur, three passes. Each pass is a sliding window sum, so
    // the cost does not grow with the radius.
    let r = BLUR_RADIUS as i32;
    let mut scratch = vec![0u8; ow_us * oh_us];
    for _ in 0..3 {
        // Horizontal: mask -> scratch
        for y in 0..oh_us {
            let row = y * ow_us;
            let mut sum: i32 = 0;
            for x in -r..=r {
                sum += i32::from(mask[row + x.clamp(0, ow as i32 - 1) as usize]);
            }
            let denom = 2 * r + 1;
            for x in 0..ow as i32 {
                scratch[row + x as usize] = (sum / denom) as u8;
                let out_x = (x - r).clamp(0, ow as i32 - 1) as usize;
                let in_x = (x + r + 1).clamp(0, ow as i32 - 1) as usize;
                sum += i32::from(mask[row + in_x]) - i32::from(mask[row + out_x]);
            }
        }
        // Vertical: scratch -> mask
        for x in 0..ow_us {
            let mut sum: i32 = 0;
            for y in -r..=r {
                sum += i32::from(scratch[y.clamp(0, oh as i32 - 1) as usize * ow_us + x]);
            }
            let denom = 2 * r + 1;
            for y in 0..oh as i32 {
                mask[y as usize * ow_us + x] = (sum / denom) as u8;
                let out_y = (y - r).clamp(0, oh as i32 - 1) as usize;
                let in_y = (y + r + 1).clamp(0, oh as i32 - 1) as usize;
                sum += i32::from(scratch[in_y * ow_us + x]) - i32::from(scratch[out_y * ow_us + x]);
            }
        }
    }

    // ── Compose ─────────────────────────────────────────────────────────
    // Shadow first (black, alpha from the blurred mask), then the capture on
    // top at full opacity. Straight (non-premultiplied) RGBA, matching what
    // `encode_rgba_png` expects.
    let mut out = vec![0u8; ow_us * oh_us * 4];
    for i in 0..ow_us * oh_us {
        out[i * 4 + 3] = (f32::from(mask[i]) / 255.0 * PEAK_ALPHA) as u8;
    }
    let src_stride = w as usize * 4;
    for row in 0..h as usize {
        let src = row * src_stride;
        let dst = ((row + MARGIN as usize) * ow_us + MARGIN as usize) * 4;
        out[dst..dst + src_stride].copy_from_slice(&rgba[src..src + src_stride]);
        // PrintWindow leaves the frame opaque; make that explicit so the
        // shadow underneath cannot bleed through a zero alpha byte.
        for px in 0..w as usize {
            out[dst + px * 4 + 3] = 255;
        }
    }

    (out, ow, oh)
}
