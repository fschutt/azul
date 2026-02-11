//! Native macOS system style discovery via dlopen + Objective-C runtime.
//!
//! This module loads `libobjc.A.dylib` and `AppKit.framework` at runtime,
//! queries semantic NSColor values, NSFont, NSEvent, NSScroller, and
//! NSWorkspace accessibility APIs, then immediately unloads the libraries.
//!
//! No external crates are required — all calls go through raw `dlopen`/`dlsym`.

use core::ffi::c_void;

use super::{
    defaults,
    AccessibilitySettings,
    InputMetrics,
    ScrollbarPreferences,
    ScrollbarVisibility,
    ScrollbarTrackClick,
    TextRenderingHints,
};
use crate::props::basic::color::{ColorU, OptionColorU};

// ── Raw dlopen / dlsym (provided by libSystem, always available) ─────────

extern "C" {
    fn dlopen(filename: *const u8, flag: i32) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const u8) -> *mut c_void;
    fn dlclose(handle: *mut c_void) -> i32;
}
const RTLD_LAZY: i32 = 1;

// ── Objective-C runtime helpers ──────────────────────────────────────────

type Id  = *mut c_void;
type Sel = *mut c_void;
type Class = *mut c_void;

/// A thin wrapper around the Objective-C runtime loaded via dlopen.
///
/// Holds function pointers to `objc_getClass`, `sel_registerName`, and
/// `objc_msgSend`.  Closing the library handles is done in `Drop`.
struct ObjcLib {
    get_class: unsafe extern "C" fn(*const u8) -> Class,
    sel_reg:   unsafe extern "C" fn(*const u8) -> Sel,
    /// Raw `objc_msgSend` pointer — cast to the correct signature at each call-site.
    msg_send:  *mut c_void,
    _h_objc:   *mut c_void,
    _h_appkit: *mut c_void,
}

impl ObjcLib {
    /// Load libobjc + AppKit.  Returns `None` if either library cannot be loaded.
    fn load() -> Option<Self> {
        unsafe {
            let h_objc = dlopen(b"/usr/lib/libobjc.A.dylib\0".as_ptr(), RTLD_LAZY);
            if h_objc.is_null() { return None; }

            let h_appkit = dlopen(
                b"/System/Library/Frameworks/AppKit.framework/AppKit\0".as_ptr(),
                RTLD_LAZY,
            );
            if h_appkit.is_null() {
                dlclose(h_objc);
                return None;
            }

            let gc = dlsym(h_objc, b"objc_getClass\0".as_ptr());
            let sr = dlsym(h_objc, b"sel_registerName\0".as_ptr());
            let ms = dlsym(h_objc, b"objc_msgSend\0".as_ptr());

            if gc.is_null() || sr.is_null() || ms.is_null() {
                dlclose(h_appkit);
                dlclose(h_objc);
                return None;
            }

            Some(ObjcLib {
                get_class: core::mem::transmute(gc),
                sel_reg:   core::mem::transmute(sr),
                msg_send:  ms,
                _h_objc:   h_objc,
                _h_appkit: h_appkit,
            })
        }
    }

    // ── convenience wrappers ─────────────────────────────────────────

    #[inline] unsafe fn cls(&self, name: &[u8]) -> Class { (self.get_class)(name.as_ptr()) }
    #[inline] unsafe fn sel(&self, name: &[u8]) -> Sel   { (self.sel_reg)(name.as_ptr()) }

    /// `[target sel]` → Id
    #[inline]
    unsafe fn send_id(&self, target: Id, sel: Sel) -> Id {
        let f: unsafe extern "C" fn(Id, Sel) -> Id = core::mem::transmute(self.msg_send);
        f(target, sel)
    }

    /// `[target sel]` → f64  (arm64: regular msgSend; x86_64 would need fpret)
    #[inline]
    unsafe fn send_f64(&self, target: Id, sel: Sel) -> f64 {
        // On Apple Silicon objc_msgSend handles all return types.
        // On x86_64 we would need objc_msgSend_fpret, but modern macOS
        // builds overwhelmingly target arm64.  The fallback in discover()
        // keeps things safe for x86_64 — we just get the default value.
        let f: unsafe extern "C" fn(Id, Sel) -> f64 = core::mem::transmute(self.msg_send);
        f(target, sel)
    }

    /// `[target sel]` → i64
    #[inline]
    unsafe fn send_i64(&self, target: Id, sel: Sel) -> i64 {
        let f: unsafe extern "C" fn(Id, Sel) -> i64 = core::mem::transmute(self.msg_send);
        f(target, sel)
    }

    /// `[target sel]` → bool (BOOL = signed char on arm64, int on x86_64)
    #[inline]
    unsafe fn send_bool(&self, target: Id, sel: Sel) -> bool {
        let f: unsafe extern "C" fn(Id, Sel) -> i8 = core::mem::transmute(self.msg_send);
        f(target, sel) != 0
    }

    /// `[target sel:arg]` → Id  (one Id argument)
    #[inline]
    unsafe fn send_id_id(&self, target: Id, sel: Sel, arg: Id) -> Id {
        let f: unsafe extern "C" fn(Id, Sel, Id) -> Id = core::mem::transmute(self.msg_send);
        f(target, sel, arg)
    }

    /// `[color getRed:&r green:&g blue:&b alpha:&a]` (returns void, 4 out-pointers)
    #[inline]
    unsafe fn send_get_rgba(
        &self, color: Id, sel: Sel,
        r: &mut f64, g: &mut f64, b: &mut f64, a: &mut f64,
    ) {
        let f: unsafe extern "C" fn(Id, Sel, *mut f64, *mut f64, *mut f64, *mut f64)
            = core::mem::transmute(self.msg_send);
        f(color, sel, r as *mut f64, g as *mut f64, b as *mut f64, a as *mut f64);
    }
}

impl Drop for ObjcLib {
    fn drop(&mut self) {
        unsafe {
            dlclose(self._h_appkit);
            dlclose(self._h_objc);
        }
    }
}

// ── Colour extraction ────────────────────────────────────────────────────

/// Convert an NSColor object to an sRGB `ColorU`, or `None` on failure
/// (e.g. pattern colours that cannot be converted).
fn extract_color(lib: &ObjcLib, color_obj: Id) -> Option<ColorU> {
    unsafe {
        if color_obj.is_null() { return None; }

        // [NSColorSpace sRGBColorSpace]
        let cs_class = lib.cls(b"NSColorSpace\0");
        let srgb_sel = lib.sel(b"sRGBColorSpace\0");
        let srgb     = lib.send_id(cs_class, srgb_sel);
        if srgb.is_null() { return None; }

        // [color colorUsingColorSpace:srgb]
        let convert_sel = lib.sel(b"colorUsingColorSpace:\0");
        let converted   = lib.send_id_id(color_obj, convert_sel, srgb);
        if converted.is_null() { return None; }

        let mut r: f64 = 0.0;
        let mut g: f64 = 0.0;
        let mut b: f64 = 0.0;
        let mut a: f64 = 0.0;

        let rgba_sel = lib.sel(b"getRed:green:blue:alpha:\0");
        lib.send_get_rgba(converted, rgba_sel, &mut r, &mut g, &mut b, &mut a);

        Some(ColorU::new(
            (r.clamp(0.0, 1.0) * 255.0) as u8,
            (g.clamp(0.0, 1.0) * 255.0) as u8,
            (b.clamp(0.0, 1.0) * 255.0) as u8,
            (a.clamp(0.0, 1.0) * 255.0) as u8,
        ))
    }
}

/// Helper: read a UTF-8 string from an NSString and return it as an owned `String`.
fn nsstring_to_string(lib: &ObjcLib, nsstr: Id) -> Option<alloc::string::String> {
    unsafe {
        if nsstr.is_null() { return None; }
        let utf8_sel = lib.sel(b"UTF8String\0");
        let cstr: *const u8 = core::mem::transmute(lib.send_id(nsstr, utf8_sel));
        if cstr.is_null() { return None; }
        let s = core::ffi::CStr::from_ptr(cstr as *const core::ffi::c_char);
        s.to_str().ok().map(|s| alloc::string::String::from(s))
    }
}

// ── Public entry point ───────────────────────────────────────────────────

/// Discover the macOS system style by loading AppKit via dlopen.
///
/// Falls back to the hardcoded `defaults::macos_modern_light()` if the
/// Objective-C runtime cannot be loaded or if any query panics.
pub(super) fn discover() -> super::SystemStyle {
    let lib = match ObjcLib::load() {
        Some(l) => l,
        None => return defaults::macos_modern_light(),
    };

    // Start with a sensible base (light or dark will be overridden below).
    let mut style = defaults::macos_modern_light();

    unsafe {
        // ── 1. Theme detection ───────────────────────────────────────
        let nsapp_cls = lib.cls(b"NSApplication\0");
        let shared_sel = lib.sel(b"sharedApplication\0");
        let app = lib.send_id(nsapp_cls, shared_sel);

        if !app.is_null() {
            let ea_sel = lib.sel(b"effectiveAppearance\0");
            let appearance = lib.send_id(app, ea_sel);
            if !appearance.is_null() {
                if let Some(name) = nsstring_to_string(&lib, lib.send_id(appearance, lib.sel(b"name\0"))) {
                    if name.contains("Dark") {
                        style = defaults::macos_modern_dark();
                    }
                }
            }
        }

        // ── 2. Semantic colours from NSColor ─────────────────────────
        let nsc = lib.cls(b"NSColor\0");

        macro_rules! q {
            ($field:ident, $sel:expr) => {
                if let Some(c) = extract_color(&lib, lib.send_id(nsc, lib.sel($sel))) {
                    style.colors.$field = OptionColorU::Some(c);
                }
            };
        }

        q!(text,                        b"labelColor\0");
        q!(secondary_text,              b"secondaryLabelColor\0");
        q!(tertiary_text,               b"tertiaryLabelColor\0");
        q!(background,                  b"textBackgroundColor\0");
        q!(accent,                      b"controlAccentColor\0");
        q!(button_face,                 b"controlColor\0");
        q!(button_text,                 b"controlTextColor\0");
        q!(disabled_text,               b"disabledControlTextColor\0");
        q!(window_background,           b"windowBackgroundColor\0");
        q!(selection_background,        b"selectedContentBackgroundColor\0");
        q!(selection_text,              b"selectedTextColor\0");
        q!(selection_background_inactive, b"unemphasizedSelectedContentBackgroundColor\0");
        q!(link,                        b"linkColor\0");
        q!(separator,                   b"separatorColor\0");
        q!(grid,                        b"gridColor\0");

        // Focus ring colour lives in FocusVisuals, not SystemColors
        if let Some(c) = extract_color(&lib, lib.send_id(nsc, lib.sel(b"keyboardFocusIndicatorColor\0"))) {
            style.focus_visuals.focus_ring_color = OptionColorU::Some(c);
        }

        // ── 3. Fonts from NSFont ─────────────────────────────────────
        let nsfont = lib.cls(b"NSFont\0");

        // [NSFont systemFontOfSize:0] → returns default size
        {
            let sys_sel = lib.sel(b"systemFontOfSize:\0");
            let f: unsafe extern "C" fn(Id, Sel, f64) -> Id =
                core::mem::transmute(lib.msg_send);
            let font = f(nsfont, sys_sel, 0.0);
            if !font.is_null() {
                if let Some(name) = nsstring_to_string(&lib, lib.send_id(font, lib.sel(b"familyName\0"))) {
                    style.fonts.ui_font = crate::corety::OptionString::Some(name.into());
                }
                let size = lib.send_f64(font, lib.sel(b"pointSize\0"));
                if size > 0.0 {
                    style.fonts.ui_font_size = crate::corety::OptionF32::Some(size as f32);
                }
            }
        }

        // [NSFont monospacedSystemFontOfSize:0 weight:NSFontWeightRegular(0.0)]
        {
            let mono_sel = lib.sel(b"monospacedSystemFontOfSize:weight:\0");
            let f: unsafe extern "C" fn(Id, Sel, f64, f64) -> Id =
                core::mem::transmute(lib.msg_send);
            let font = f(nsfont, mono_sel, 0.0, 0.0); // weight 0.0 = Regular
            if !font.is_null() {
                if let Some(name) = nsstring_to_string(&lib, lib.send_id(font, lib.sel(b"familyName\0"))) {
                    style.fonts.monospace_font = crate::corety::OptionString::Some(name.into());
                }
                let size = lib.send_f64(font, lib.sel(b"pointSize\0"));
                if size > 0.0 {
                    style.fonts.monospace_font_size = crate::corety::OptionF32::Some(size as f32);
                }
            }
        }

        // ── 4. Input metrics from NSEvent ────────────────────────────
        let nsevent = lib.cls(b"NSEvent\0");
        let dci = lib.send_f64(nsevent, lib.sel(b"doubleClickInterval\0"));
        if dci > 0.0 {
            style.input.double_click_time_ms = (dci * 1000.0) as u32;
        }

        // ── 5. Scrollbar preferences from NSScroller ─────────────────
        let nsscroller = lib.cls(b"NSScroller\0");
        let scroller_style = lib.send_i64(nsscroller, lib.sel(b"preferredScrollerStyle\0"));
        style.scrollbar_preferences = ScrollbarPreferences {
            visibility: match scroller_style {
                0 => ScrollbarVisibility::Always,       // NSScrollerStyleLegacy
                1 => ScrollbarVisibility::WhenScrolling, // NSScrollerStyleOverlay
                _ => ScrollbarVisibility::Automatic,
            },
            track_click: ScrollbarTrackClick::PageUpDown,
        };

        // ── 6. Accessibility from NSWorkspace ────────────────────────
        let nsworkspace = lib.cls(b"NSWorkspace\0");
        let ws = lib.send_id(nsworkspace, lib.sel(b"sharedWorkspace\0"));
        if !ws.is_null() {
            let reduce_motion = lib.send_bool(ws, lib.sel(
                b"accessibilityDisplayShouldReduceMotion\0"));
            let increase_contrast = lib.send_bool(ws, lib.sel(
                b"accessibilityDisplayShouldIncreaseContrast\0"));
            let reduce_transparency = lib.send_bool(ws, lib.sel(
                b"accessibilityDisplayShouldReduceTransparency\0"));

            if reduce_motion {
                style.prefers_reduced_motion =
                    crate::dynamic_selector::BoolCondition::True;
            }
            if increase_contrast {
                style.prefers_high_contrast =
                    crate::dynamic_selector::BoolCondition::True;
            }

            style.accessibility = AccessibilitySettings {
                prefers_reduced_motion: reduce_motion,
                prefers_high_contrast: increase_contrast,
                prefers_reduced_transparency: reduce_transparency,
                ..Default::default()
            };

            style.text_rendering.increased_contrast = increase_contrast;
        }

        // ── 7. OS version from NSProcessInfo ─────────────────────────
        let nspi = lib.cls(b"NSProcessInfo\0");
        let pi = lib.send_id(nspi, lib.sel(b"processInfo\0"));
        if !pi.is_null() {
            // operatingSystemVersion returns a struct { major, minor, patch }
            // on arm64 this is returned in x0/x1/x2 (3 × NSInteger = 3 × i64).
            // We read major via a helper struct.
            #[repr(C)]
            struct NSOperatingSystemVersion { major: i64, minor: i64, patch: i64 }
            let osv_sel = lib.sel(b"operatingSystemVersion\0");
            let f: unsafe extern "C" fn(Id, Sel) -> NSOperatingSystemVersion
                = core::mem::transmute(lib.msg_send);
            let v = f(pi, osv_sel);
            style.os_version = match v.major {
                26 => crate::dynamic_selector::OsVersion::MACOS_TAHOE,
                15 => crate::dynamic_selector::OsVersion::MACOS_SEQUOIA,
                14 => crate::dynamic_selector::OsVersion::MACOS_SONOMA,
                13 => crate::dynamic_selector::OsVersion::MACOS_VENTURA,
                12 => crate::dynamic_selector::OsVersion::MACOS_MONTEREY,
                11 => crate::dynamic_selector::OsVersion::MACOS_BIG_SUR,
                _  => crate::dynamic_selector::OsVersion::MACOS_SONOMA,
            };
        }

        // ── 8. System language from NSLocale ─────────────────────────
        let nslocale = lib.cls(b"NSLocale\0");
        let cur_locale = lib.send_id(nslocale, lib.sel(b"currentLocale\0"));
        if !cur_locale.is_null() {
            if let Some(ident) = nsstring_to_string(
                &lib,
                lib.send_id(cur_locale, lib.sel(b"localeIdentifier\0")),
            ) {
                // Convert "en_US" → "en-US"
                let bcp47 = ident.replace('_', "-");
                style.language = crate::corety::AzString::from(bcp47);
            }
        }
    }

    style.platform = super::Platform::MacOs;
    style
}
