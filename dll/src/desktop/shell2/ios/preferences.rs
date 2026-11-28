//! The user's accessibility and display settings on iOS, read from UIKit and its siblings.
//!
//! Every flag is a C function (`UIAccessibilityIsBoldTextEnabled()`, `UIAccessibilityIsReduceMotionEnabled()`,
//! `UIAccessibilityDarkerSystemColorsEnabled()`, ...) of UIKit, of the Accessibility framework
//! (`AXAnimatedImagesEnabled()`, `AXShowBordersEnabled()`, iOS 17 and 26.1) or of MediaAccessibility
//! (`MADimFlashingLightsEnabled()`, iOS 16.4). They are looked up by name ONCE, when the window is made
//! (`dlsym`): a function this iOS does not have is `None` and its option is `Unknown` - never `Off`. A
//! wrong name is the same as a missing function, which is why the Simulator run of the QA harness reads
//! them back (`get_system_accessibility`).
//!
//! The text size is `UIApplication.preferredContentSizeCategory`; `UIFontMetrics`' own scaling of the 17 pt
//! body text stands behind a category this build does not know. The meaning of every answer (the
//! categories, the body factors) is `azul_css::accessibility_raw::RawIos::into_preferences`, tested on
//! every host.
//!
//! MAIN THREAD ONLY (UIKit): the window's display tick calls [`PreferenceReader::read`]. UIKit documents no
//! thread for its notifications, so none is used: the tick asks every 250 ms (`PreferenceWatch`), and at
//! once when the app comes back to the front.

use std::ffi::{c_char, c_void, CStr};

use azul_css::accessibility_raw::RawIos;
use objc::{msg_send, runtime::{Class, Object}, sel, sel_impl};

extern "C" {
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    fn dlopen(path: *const c_char, mode: i32) -> *mut c_void;
}

/// `RTLD_DEFAULT` of Apple platforms: every image the process has loaded.
const RTLD_DEFAULT: *mut c_void = -2_isize as *mut c_void;
/// `RTLD_LAZY`.
const RTLD_LAZY: i32 = 1;

/// A flag function: no arguments, a `BOOL` back (a byte on every iOS ABI).
type FlagFn = unsafe extern "C" fn() -> u8;

/// The flags this reader asks, in the order of [`PreferenceReader::flags`]: the name of the C function
/// and the framework that owns it (loaded on demand: UIKit already is).
const FLAGS: [(&CStr, Framework); 18] = [
    (c"UIAccessibilityIsBoldTextEnabled", Framework::UiKit),
    (c"UIAccessibilityDarkerSystemColorsEnabled", Framework::UiKit),
    (c"UIAccessibilityIsReduceMotionEnabled", Framework::UiKit),
    (c"UIAccessibilityPrefersCrossFadeTransitions", Framework::UiKit),
    (c"UIAccessibilityIsReduceTransparencyEnabled", Framework::UiKit),
    (c"UIAccessibilityIsInvertColorsEnabled", Framework::UiKit),
    (c"UIAccessibilityIsGrayscaleEnabled", Framework::UiKit),
    (c"UIAccessibilityShouldDifferentiateWithoutColor", Framework::UiKit),
    (c"AXShowBordersEnabled", Framework::Accessibility),
    (c"UIAccessibilityButtonShapesEnabled", Framework::UiKit),
    (c"UIAccessibilityIsOnOffSwitchLabelsEnabled", Framework::UiKit),
    (c"UIAccessibilityIsVoiceOverRunning", Framework::UiKit),
    (c"UIAccessibilityIsSwitchControlRunning", Framework::UiKit),
    (c"UIAccessibilityIsClosedCaptioningEnabled", Framework::UiKit),
    (c"UIAccessibilityIsMonoAudioEnabled", Framework::UiKit),
    (c"UIAccessibilityIsVideoAutoplayEnabled", Framework::UiKit),
    (c"AXAnimatedImagesEnabled", Framework::Accessibility),
    (c"MADimFlashingLightsEnabled", Framework::MediaAccessibility),
];

/// The framework a flag function lives in.
#[derive(Clone, Copy)]
enum Framework {
    UiKit,
    Accessibility,
    MediaAccessibility,
}

impl Framework {
    /// The path the framework loads from, `None` for UIKit (the app has it).
    const fn path(self) -> Option<&'static CStr> {
        match self {
            Self::UiKit => None,
            Self::Accessibility => {
                Some(c"/System/Library/Frameworks/Accessibility.framework/Accessibility")
            }
            Self::MediaAccessibility => {
                Some(c"/System/Library/Frameworks/MediaAccessibility.framework/MediaAccessibility")
            }
        }
    }
}

/// The index of each flag in [`FLAGS`].
mod at {
    pub const BOLD_TEXT: usize = 0;
    pub const DARKER_COLORS: usize = 1;
    pub const REDUCE_MOTION: usize = 2;
    pub const CROSS_FADE: usize = 3;
    pub const REDUCE_TRANSPARENCY: usize = 4;
    pub const INVERT_COLORS: usize = 5;
    pub const GRAYSCALE: usize = 6;
    pub const DIFFERENTIATE: usize = 7;
    pub const SHOW_BORDERS: usize = 8;
    pub const BUTTON_SHAPES: usize = 9;
    pub const ON_OFF_LABELS: usize = 10;
    pub const VOICE_OVER: usize = 11;
    pub const SWITCH_CONTROL: usize = 12;
    pub const CAPTIONS: usize = 13;
    pub const MONO_AUDIO: usize = 14;
    pub const VIDEO_AUTOPLAY: usize = 15;
    pub const ANIMATED_IMAGES: usize = 16;
    pub const DIM_FLASHING: usize = 17;
}

/// The flag functions of this iOS, looked up once.
pub struct PreferenceReader {
    flags: [Option<FlagFn>; FLAGS.len()],
}

impl std::fmt::Debug for PreferenceReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let found = self.flags.iter().filter(|flag| flag.is_some()).count();
        write!(f, "PreferenceReader({found} of {} flags)", FLAGS.len())
    }
}

impl PreferenceReader {
    /// Looks up the flag functions (and loads the two frameworks that own some). A function this iOS
    /// does not export is `None`.
    #[must_use]
    pub fn new() -> Self {
        // SAFETY: the paths are NUL-terminated; a handle that cannot be made is not used (the symbol
        // lookup below then finds nothing in that framework), and the handles are never closed.
        unsafe {
            for (_, framework) in FLAGS {
                if let Some(path) = framework.path() {
                    drop(dlopen(path.as_ptr(), RTLD_LAZY));
                }
            }
        }
        let mut flags: [Option<FlagFn>; FLAGS.len()] = [None; FLAGS.len()];
        for (slot, (name, _)) in flags.iter_mut().zip(FLAGS) {
            // SAFETY: `name` is NUL-terminated; a symbol found is a function that takes nothing and
            // answers a `BOOL` (UIKit's and its siblings' headers declare every one so).
            let symbol = unsafe { dlsym(RTLD_DEFAULT, name.as_ptr()) };
            if !symbol.is_null() {
                *slot = Some(unsafe { core::mem::transmute::<*mut c_void, FlagFn>(symbol) });
            }
        }
        Self { flags }
    }

    /// One flag: `None` where this iOS has no such function.
    fn flag(&self, index: usize) -> Option<bool> {
        // SAFETY: a `FlagFn` takes nothing; it reads a preference and answers a byte.
        self.flags.get(index).copied().flatten().map(|f| unsafe { f() } != 0)
    }

    /// Everything the app may read of the user's accessibility settings, now. MAIN THREAD ONLY.
    #[must_use]
    pub fn read(&self) -> RawIos {
        // The older name of "Show Borders" answers where the new function is not there (before 26.1).
        let button_shapes = self
            .flag(at::SHOW_BORDERS)
            .or_else(|| self.flag(at::BUTTON_SHAPES));
        RawIos {
            content_size_category: content_size_category(),
            scaled_body_points: scaled_body_points(),
            bold_text: self.flag(at::BOLD_TEXT),
            darker_system_colors: self.flag(at::DARKER_COLORS),
            reduce_motion: self.flag(at::REDUCE_MOTION),
            prefers_cross_fade: self.flag(at::CROSS_FADE),
            reduce_transparency: self.flag(at::REDUCE_TRANSPARENCY),
            invert_colors: self.flag(at::INVERT_COLORS),
            grayscale: self.flag(at::GRAYSCALE),
            differentiate_without_color: self.flag(at::DIFFERENTIATE),
            button_shapes,
            on_off_labels: self.flag(at::ON_OFF_LABELS),
            voice_over: self.flag(at::VOICE_OVER),
            switch_control: self.flag(at::SWITCH_CONTROL),
            closed_captioning: self.flag(at::CAPTIONS),
            mono_audio: self.flag(at::MONO_AUDIO),
            video_autoplay: self.flag(at::VIDEO_AUTOPLAY),
            dim_flashing_lights: self.flag(at::DIM_FLASHING),
            animated_images: self.flag(at::ANIMATED_IMAGES),
        }
    }
}

impl Default for PreferenceReader {
    fn default() -> Self {
        Self::new()
    }
}

/// An `NSString` as a Rust string; `None` for nil or text that is not UTF-8.
unsafe fn nsstring(string: *mut Object) -> Option<String> {
    if string.is_null() {
        return None;
    }
    let utf8: *const c_char = msg_send![string, UTF8String];
    if utf8.is_null() {
        return None;
    }
    CStr::from_ptr(utf8).to_str().ok().map(String::from)
}

/// The user's content size category, as the string UIKit holds (`UICTContentSizeCategoryL`): the
/// application's, and before `UIApplicationMain` made one (nil), the main screen's trait collection.
fn content_size_category() -> Option<String> {
    // SAFETY: UIKit messages on the main thread, to classes that exist on every iOS this builds for; a nil
    // receiver answers nil.
    unsafe {
        let application_class = Class::get("UIApplication")?;
        let application: *mut Object = msg_send![application_class, sharedApplication];
        if !application.is_null() {
            let category: *mut Object = msg_send![application, preferredContentSizeCategory];
            if let Some(category) = nsstring(category) {
                return Some(category);
            }
        }
        let screen_class = Class::get("UIScreen")?;
        let screen: *mut Object = msg_send![screen_class, mainScreen];
        if screen.is_null() {
            return None;
        }
        let traits: *mut Object = msg_send![screen, traitCollection];
        if traits.is_null() {
            return None;
        }
        let category: *mut Object = msg_send![traits, preferredContentSizeCategory];
        nsstring(category)
    }
}

/// The size `UIFontMetrics` scales 17 pt body text to at the user's content size (iOS 11).
fn scaled_body_points() -> Option<f32> {
    // SAFETY: as above; `scaledValueForValue:` takes and answers a `CGFloat` (a double on 64-bit).
    unsafe {
        let metrics_class = Class::get("UIFontMetrics")?;
        let metrics: *mut Object = msg_send![metrics_class, defaultMetrics];
        if metrics.is_null() {
            return None;
        }
        let scaled: f64 = msg_send![metrics, scaledValueForValue: 17.0_f64];
        // a finite, positive size, within what a float holds
        (scaled.is_finite() && scaled > 0.0).then_some(scaled as f32)
    }
}
