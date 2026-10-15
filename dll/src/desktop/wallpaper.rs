//! The desktop wallpaper, for the metals to reflect.
//!
//! A polished metal in a window mirrors what is behind the window: the
//! desktop's wallpaper (`azul_layout::texture::environment`). This module is
//! the platform half - where each desktop keeps its wallpaper:
//!
//! - macOS: `NSWorkspace.desktopImageURL(for:)` per screen, decoded and scaled down by ImageIO
//!   (which reads HEIC, the dynamic wallpapers' format);
//! - Windows: `SystemParametersInfoW(SPI_GETDESKWALLPAPER)`, the wallpaper's file;
//! - GNOME: `org.gnome.desktop.background picture-uri` (`picture-uri-dark` in the dark mode);
//!   KDE Plasma: the `Image=` of the desktop containment in `plasma-org.kde.plasma.desktop-appletsrc`.
//!
//! [`install`] registers the reader; it runs the first time a metal is drawn.
//! [`refresh`] reads again - the shells call it when a window is activated,
//! the moment a user comes back from changing the wallpaper (one `stat` per
//! screen when nothing changed).
//! Where no reader works (a headless run, an unknown desktop) the metals
//! reflect the system's highlight colour instead.

#[cfg(feature = "materials")]
use azul_layout::texture::environment::{self, Wallpaper};

/// Register this platform's wallpaper reader (`App::create` calls it).
pub fn install() {
    #[cfg(feature = "materials")]
    environment::set_wallpaper_loader(read_wallpapers);
}

/// Read the wallpapers again now (the user may have changed them while the
/// app was in the background). Returns whether they changed: the caller
/// redraws, and every material frame renders again.
pub fn refresh() -> bool {
    #[cfg(feature = "materials")]
    {
        environment::refresh_desktop_wallpapers()
    }
    #[cfg(not(feature = "materials"))]
    {
        false
    }
}

/// The desktop's wallpapers, one per screen this platform can tell.
#[cfg(feature = "materials")]
fn read_wallpapers() -> Vec<Wallpaper> {
    platform::read()
}

/// The wallpaper of the file at `path` for `screen`, made by `load` - or the
/// one made before, while the file and the screen are unchanged (a refresh
/// on every window activation then costs one `stat`, and the metals keep
/// their frames).
#[cfg(feature = "materials")]
fn cached(
    path: &std::path::Path,
    screen: [f32; 4],
    load: impl FnOnce() -> Option<Wallpaper>,
) -> Option<Wallpaper> {
    type Entry = (std::path::PathBuf, Option<std::time::SystemTime>, [u32; 4], Wallpaper);
    static CACHE: std::sync::Mutex<Vec<Entry>> = std::sync::Mutex::new(Vec::new());
    let modified = std::fs::metadata(path).and_then(|m| m.modified()).ok();
    let screen_bits = screen.map(f32::to_bits);
    if let Ok(cache) = CACHE.lock() {
        if let Some(hit) = cache
            .iter()
            .find(|e| e.0 == path && e.1 == modified && e.2 == screen_bits)
        {
            return Some(hit.3.clone());
        }
    }
    let wallpaper = load()?;
    if let Ok(mut cache) = CACHE.lock() {
        cache.retain(|e| e.0 != path || e.2 != screen_bits);
        cache.push((path.to_path_buf(), modified, screen_bits, wallpaper.clone()));
        if cache.len() > 8 {
            cache.remove(0);
        }
    }
    Some(wallpaper)
}

/// RGBA8 bytes from a decoded image of 8-bit grey, grey + alpha, RGB or RGBA.
#[cfg(all(feature = "materials", not(target_os = "macos")))]
fn rgba8_of(raw: &azul_core::resources::RawImage) -> Option<Vec<u8>> {
    use azul_core::resources::{RawImageData, RawImageFormat};
    let RawImageData::U8(bytes) = &raw.pixels else {
        return None;
    };
    let b = bytes.as_ref();
    let n = raw.width * raw.height;
    Some(match raw.data_format {
        RawImageFormat::RGBA8 => b.get(..n * 4)?.to_vec(),
        RawImageFormat::BGRA8 => b.get(..n * 4)?.chunks_exact(4).flat_map(|p| [p[2], p[1], p[0], p[3]]).collect(),
        RawImageFormat::RGB8 => b.get(..n * 3)?.chunks_exact(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
        RawImageFormat::R8 => b.get(..n)?.iter().flat_map(|&v| [v, v, v, 255]).collect(),
        RawImageFormat::RG8 => b.get(..n * 2)?.chunks_exact(2).flat_map(|p| [p[0], p[0], p[0], p[1]]).collect(),
        _ => return None,
    })
}

/// The wallpaper in the image file at `path`, covering `screen` (desktop
/// points: x, y, width, height).
#[cfg(all(feature = "materials", not(target_os = "macos")))]
fn wallpaper_from_file(path: &std::path::Path, screen: [f32; 4]) -> Option<Wallpaper> {
    use azul_layout::image::decode::{decode_raw_image_from_any_bytes, ResultRawImageDecodeImageError};
    let bytes = std::fs::read(path).ok()?;
    let ResultRawImageDecodeImageError::Ok(raw) = decode_raw_image_from_any_bytes(&bytes) else {
        return None;
    };
    let rgba = rgba8_of(&raw)?;
    Wallpaper::from_rgba8(raw.width, raw.height, &rgba, screen)
}

#[cfg(all(feature = "materials", target_os = "macos"))]
mod platform {
    use core::ffi::c_void;

    use azul_layout::texture::environment::{Wallpaper, WALLPAPER_WIDTH};
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSScreen, NSWorkspace};

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CGPoint {
        x: f64,
        y: f64,
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CGSize {
        width: f64,
        height: f64,
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CGRect {
        origin: CGPoint,
        size: CGSize,
    }
    /// An opaque CoreFoundation callbacks table (only its address is used).
    #[repr(C)]
    struct CfCallbacks {
        _opaque: [u8; 0],
    }

    #[link(name = "ImageIO", kind = "framework")]
    extern "C" {
        fn CGImageSourceCreateWithURL(url: *const c_void, options: *const c_void) -> *mut c_void;
        fn CGImageSourceCreateThumbnailAtIndex(
            source: *mut c_void,
            index: usize,
            options: *const c_void,
        ) -> *mut c_void;
        static kCGImageSourceCreateThumbnailFromImageAlways: *const c_void;
        static kCGImageSourceThumbnailMaxPixelSize: *const c_void;
        static kCGImageSourceCreateThumbnailWithTransform: *const c_void;
    }

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGImageGetWidth(image: *mut c_void) -> usize;
        fn CGImageGetHeight(image: *mut c_void) -> usize;
        fn CGImageRelease(image: *mut c_void);
        fn CGColorSpaceCreateDeviceRGB() -> *mut c_void;
        fn CGColorSpaceRelease(space: *mut c_void);
        fn CGBitmapContextCreate(
            data: *mut c_void,
            width: usize,
            height: usize,
            bits_per_component: usize,
            bytes_per_row: usize,
            space: *mut c_void,
            bitmap_info: u32,
        ) -> *mut c_void;
        fn CGContextDrawImage(context: *mut c_void, rect: CGRect, image: *mut c_void);
        fn CGContextRelease(context: *mut c_void);
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFDictionaryCreate(
            allocator: *const c_void,
            keys: *const *const c_void,
            values: *const *const c_void,
            count: isize,
            key_callbacks: *const CfCallbacks,
            value_callbacks: *const CfCallbacks,
        ) -> *const c_void;
        fn CFNumberCreate(allocator: *const c_void, kind: isize, value: *const c_void) -> *const c_void;
        fn CFRelease(cf: *const c_void);
        static kCFTypeDictionaryKeyCallBacks: CfCallbacks;
        static kCFTypeDictionaryValueCallBacks: CfCallbacks;
        static kCFBooleanTrue: *const c_void;
    }

    /// `kCFNumberIntType`.
    const CF_NUMBER_INT: isize = 9;
    /// `kCGImageAlphaPremultipliedLast | kCGBitmapByteOrder32Big`: RGBA bytes.
    const RGBA_PREMULTIPLIED: u32 = 1 | (4 << 12);

    /// The image at `url` (an `NSURL`, toll-free a `CFURLRef`) as RGBA8, at
    /// most [`WALLPAPER_WIDTH`] px on its long side.
    unsafe fn thumbnail(url: *const c_void) -> Option<(usize, usize, Vec<u8>)> {
        let source = CGImageSourceCreateWithURL(url, core::ptr::null());
        if source.is_null() {
            return None;
        }
        let max = WALLPAPER_WIDTH as i32;
        let number = CFNumberCreate(core::ptr::null(), CF_NUMBER_INT, core::ptr::from_ref(&max).cast());
        let keys = [
            kCGImageSourceCreateThumbnailFromImageAlways,
            kCGImageSourceThumbnailMaxPixelSize,
            kCGImageSourceCreateThumbnailWithTransform,
        ];
        let values = [kCFBooleanTrue, number, kCFBooleanTrue];
        let options = CFDictionaryCreate(
            core::ptr::null(),
            keys.as_ptr(),
            values.as_ptr(),
            keys.len() as isize,
            &kCFTypeDictionaryKeyCallBacks,
            &kCFTypeDictionaryValueCallBacks,
        );
        let image = CGImageSourceCreateThumbnailAtIndex(source, 0, options);
        if !options.is_null() {
            CFRelease(options);
        }
        if !number.is_null() {
            CFRelease(number);
        }
        CFRelease(source.cast_const());
        if image.is_null() {
            return None;
        }
        let (w, h) = (CGImageGetWidth(image), CGImageGetHeight(image));
        if w == 0 || h == 0 {
            CGImageRelease(image);
            return None;
        }
        let mut pixels = vec![0_u8; w * h * 4];
        let space = CGColorSpaceCreateDeviceRGB();
        let context = CGBitmapContextCreate(
            pixels.as_mut_ptr().cast(),
            w,
            h,
            8,
            w * 4,
            space,
            RGBA_PREMULTIPLIED,
        );
        CGColorSpaceRelease(space);
        if context.is_null() {
            CGImageRelease(image);
            return None;
        }
        let rect = CGRect {
            origin: CGPoint { x: 0.0, y: 0.0 },
            size: CGSize {
                width: w as f64,
                height: h as f64,
            },
        };
        CGContextDrawImage(context, rect, image);
        CGContextRelease(context);
        CGImageRelease(image);
        Some((w, h, pixels))
    }

    pub(super) fn read() -> Vec<Wallpaper> {
        // AppKit's screens are the main thread's: elsewhere nothing is read.
        let Some(mtm) = MainThreadMarker::new() else {
            return Vec::new();
        };
        let screens = NSScreen::screens(mtm);
        let workspace = NSWorkspace::sharedWorkspace();
        // AppKit puts the origin at the primary screen's bottom-left; the
        // desktop points the windows use start at its top-left.
        let primary_height = screens.iter().next().map_or(0.0, |s| s.frame().size.height);
        let mut out = Vec::new();
        for screen in screens.iter() {
            let frame = screen.frame();
            let rect = [
                frame.origin.x as f32,
                (primary_height - frame.origin.y - frame.size.height) as f32,
                frame.size.width as f32,
                frame.size.height as f32,
            ];
            let Some(url) = workspace.desktopImageURLForScreen(&screen) else {
                continue;
            };
            let url_ptr: *const c_void = objc2::rc::Retained::as_ptr(&url).cast();
            let path = url.path().map(|p| p.to_string()).unwrap_or_default();
            let load = || {
                // SAFETY: `url` is alive for the call; the CoreFoundation
                // objects made inside are released there.
                let (w, h, rgba) = unsafe { thumbnail(url_ptr) }?;
                Wallpaper::from_rgba8(w, h, &rgba, rect)
            };
            if let Some(wallpaper) = super::cached(std::path::Path::new(&path), rect, load) {
                out.push(wallpaper);
            }
        }
        out
    }
}

#[cfg(all(feature = "materials", target_os = "windows"))]
mod platform {
    use azul_layout::texture::environment::Wallpaper;

    #[link(name = "user32")]
    extern "system" {
        fn SystemParametersInfoW(action: u32, param: u32, data: *mut core::ffi::c_void, flags: u32) -> i32;
        fn GetSystemMetrics(index: i32) -> i32;
    }

    const SPI_GETDESKWALLPAPER: u32 = 0x0073;
    const SM_CXSCREEN: i32 = 0;
    const SM_CYSCREEN: i32 = 1;

    pub(super) fn read() -> Vec<Wallpaper> {
        let mut buf = [0_u16; 1024];
        // SAFETY: the buffer holds `buf.len()` UTF-16 units, the size passed.
        let ok = unsafe {
            SystemParametersInfoW(SPI_GETDESKWALLPAPER, buf.len() as u32, buf.as_mut_ptr().cast(), 0)
        };
        if ok == 0 {
            return Vec::new();
        }
        let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        let path = String::from_utf16_lossy(&buf[..len]);
        if path.is_empty() {
            return Vec::new();
        }
        // SAFETY: plain metric queries.
        let (w, h) = unsafe { (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)) };
        let screen = [0.0, 0.0, w.max(1) as f32, h.max(1) as f32];
        let path = std::path::Path::new(&path);
        super::cached(path, screen, || super::wallpaper_from_file(path, screen))
            .into_iter()
            .collect()
    }
}

#[cfg(all(feature = "materials", target_os = "linux"))]
mod platform {
    use azul_layout::texture::environment::Wallpaper;

    /// The screen a Linux wallpaper is taken to cover: the platform does not
    /// say here, so a common one (the reflection is blurred anyway).
    const SCREEN: [f32; 4] = [0.0, 0.0, 1920.0, 1080.0];

    /// A `file://` URI or a plain path, as GNOME and KDE store them.
    pub(super) fn path_of(value: &str) -> Option<std::path::PathBuf> {
        let v = value.trim().trim_matches('\'').trim_matches('"');
        let v = v.strip_prefix("file://").unwrap_or(v);
        (!v.is_empty()).then(|| std::path::PathBuf::from(percent_decode(v)))
    }

    /// `%20` and friends back to their bytes (a GNOME URI escapes spaces).
    fn percent_decode(s: &str) -> String {
        let bytes = s.as_bytes();
        let mut out = Vec::with_capacity(bytes.len());
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'%' && i + 2 < bytes.len() {
                if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                    out.push(v);
                    i += 3;
                    continue;
                }
            }
            out.push(bytes[i]);
            i += 1;
        }
        String::from_utf8_lossy(&out).into_owned()
    }

    fn gsettings(schema: &str, key: &str) -> Option<String> {
        let out = std::process::Command::new("gsettings")
            .args(["get", schema, key])
            .output()
            .ok()?;
        out.status
            .success()
            .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
    }

    fn gnome() -> Option<std::path::PathBuf> {
        let dark = gsettings("org.gnome.desktop.interface", "color-scheme")
            .is_some_and(|v| v.contains("dark"));
        let key = if dark { "picture-uri-dark" } else { "picture-uri" };
        gsettings("org.gnome.desktop.background", key)
            .and_then(|v| path_of(&v))
            .or_else(|| {
                gsettings("org.gnome.desktop.background", "picture-uri").and_then(|v| path_of(&v))
            })
    }

    fn kde() -> Option<std::path::PathBuf> {
        let home = std::env::var_os("HOME")?;
        let file = std::path::Path::new(&home).join(".config/plasma-org.kde.plasma.desktop-appletsrc");
        let text = std::fs::read_to_string(file).ok()?;
        // The first `Image=` under a `[...][Wallpaper][org.kde.image][General]`.
        let mut in_image = false;
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with('[') {
                in_image = line.ends_with("[Wallpaper][org.kde.image][General]");
                continue;
            }
            if in_image {
                if let Some(v) = line.strip_prefix("Image=") {
                    return path_of(v);
                }
            }
        }
        None
    }

    pub(super) fn read() -> Vec<Wallpaper> {
        let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default().to_ascii_lowercase();
        let path = if desktop.contains("kde") {
            kde().or_else(gnome)
        } else {
            gnome().or_else(kde)
        };
        path.and_then(|p| super::cached(&p, SCREEN, || super::wallpaper_from_file(&p, SCREEN)))
            .into_iter()
            .collect()
    }
}

#[cfg(all(
    feature = "materials",
    not(any(target_os = "macos", target_os = "windows", target_os = "linux"))
))]
mod platform {
    /// No reader here: the metals reflect the highlight colour.
    pub(super) fn read() -> Vec<azul_layout::texture::environment::Wallpaper> {
        Vec::new()
    }
}

#[cfg(all(test, feature = "materials", target_os = "linux"))]
mod tests {
    #[test]
    fn a_wallpaper_uri_is_read_as_a_path() {
        assert_eq!(
            super::platform::path_of("'file:///home/me/Pictures/Lake%20Garda.jpg'")
                .map(|p| p.to_string_lossy().into_owned()),
            Some("/home/me/Pictures/Lake Garda.jpg".to_string())
        );
    }
}
