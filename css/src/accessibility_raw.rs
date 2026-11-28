//! What each platform's readers hand over, and the typed preferences it means.
//!
//! The shells read the platform (`UIKit`, the Android framework, the registry, gsettings, `AppKit`)
//! and fill one `Raw*` struct each: a field is `None` where the platform has no such option, the
//! symbol or the key is missing on this version, or the read failed - and `None` becomes
//! [`SettingState::Unknown`], never `Off`. The mapping from the raw values to the
//! [`AccessibilityPreferences`] is HERE, as pure functions of plain data, so every platform's
//! decisions (which iOS size category is which factor, what Android's zero animation scale means,
//! what a Windows text scale of 225 is) are tested on every host, and the shells stay thin.

use alloc::string::String;

use crate::{
    accessibility::{AccessibilityPreferences, ContrastPreference, SettingState, TextSizeCategory},
    corety::{OptionF32, OptionI16, OptionU32},
};

/// `Some(on)` as a state, `None` as unknown.
#[must_use]
pub const fn state_of(answer: Option<bool>) -> SettingState {
    match answer {
        Some(true) => SettingState::On,
        Some(false) => SettingState::Off,
        None => SettingState::Unknown,
    }
}

/// `Some(on)` as a contrast preference (`on` is "more"), `None` as unknown.
#[must_use]
pub const fn contrast_of(answer: Option<bool>) -> ContrastPreference {
    match answer {
        Some(true) => ContrastPreference::More,
        Some(false) => ContrastPreference::NoPreference,
        None => ContrastPreference::Unknown,
    }
}

/// `a` where it is a state, else `b`; the conjunction of two readers of one option (an option
/// that is on if either says so, unknown only when neither answered).
#[must_use]
pub const fn either(a: Option<bool>, b: Option<bool>) -> Option<bool> {
    match (a, b) {
        (Some(true), _) | (_, Some(true)) => Some(true),
        (Some(false), _) | (_, Some(false)) => Some(false),
        (None, None) => None,
    }
}

// ---------------------------------------------------------------------------------------------
// iOS and iPadOS
// ---------------------------------------------------------------------------------------------

/// What `UIKit` answered. `UIApplication.preferredContentSizeCategory` (or the view's trait
/// collection), the `UIAccessibility` flags, and `UIFontMetrics` as the fallback for a category
/// name this build does not know.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RawIos {
    /// The raw string of `preferredContentSizeCategory` (`UICTContentSizeCategoryL`).
    pub content_size_category: Option<String>,
    /// `UIFontMetrics(forTextStyle: .body).scaledValue(for: 17)`: the body size now, in points.
    pub scaled_body_points: Option<f32>,
    /// `UIAccessibility.isBoldTextEnabled`.
    pub bold_text: Option<bool>,
    /// `UIAccessibility.isDarkerSystemColorsEnabled` ("Increase contrast").
    pub darker_system_colors: Option<bool>,
    /// `UIAccessibility.isReduceMotionEnabled`.
    pub reduce_motion: Option<bool>,
    /// `UIAccessibility.prefersCrossFadeTransitions`.
    pub prefers_cross_fade: Option<bool>,
    /// `UIAccessibility.isReduceTransparencyEnabled`.
    pub reduce_transparency: Option<bool>,
    /// `UIAccessibility.isInvertColorsEnabled`.
    pub invert_colors: Option<bool>,
    /// `UIAccessibility.isGrayscaleEnabled`.
    pub grayscale: Option<bool>,
    /// `UIAccessibility.shouldDifferentiateWithoutColor`.
    pub differentiate_without_color: Option<bool>,
    /// `UIAccessibility.isButtonShapesEnabled`.
    pub button_shapes: Option<bool>,
    /// `UIAccessibility.isOnOffSwitchLabelsEnabled`.
    pub on_off_labels: Option<bool>,
    /// `UIAccessibility.isVoiceOverRunning`.
    pub voice_over: Option<bool>,
    /// `UIAccessibility.isSwitchControlRunning`.
    pub switch_control: Option<bool>,
    /// `UIAccessibility.isClosedCaptioningEnabled`.
    pub closed_captioning: Option<bool>,
    /// `UIAccessibility.isMonoAudioEnabled`.
    pub mono_audio: Option<bool>,
    /// `UIAccessibility.isVideoAutoplayEnabled`.
    pub video_autoplay: Option<bool>,
    /// `UIAccessibility.isReduceWhitePointEnabled` ("Reduce white point"): dim bright colours.
    pub reduce_white_point: Option<bool>,
    /// `UIAccessibility.isZoomEnabled`.
    pub zoom: Option<bool>,
}

impl RawIos {
    /// The preferences this iOS state means.
    #[must_use]
    pub fn into_preferences(&self) -> AccessibilityPreferences {
        let mut preferences = AccessibilityPreferences::default();
        let category = self
            .content_size_category
            .as_deref()
            .and_then(TextSizeCategory::parse)
            .filter(|category| *category != TextSizeCategory::Unknown);
        match (category, self.scaled_body_points) {
            (Some(category), _) => preferences.set_text_size_category(category),
            // a category this build does not know (a new one, a vendor's): the size UIKit scales
            // the body text to, over its default
            (None, Some(points)) if points.is_finite() && points > 0.0 => {
                drop(preferences.set_text_scale(points / 17.0));
            }
            (None, _) => {}
        }
        preferences.bold_text = state_of(self.bold_text);
        preferences.contrast = contrast_of(self.darker_system_colors);
        preferences.reduced_motion = state_of(self.reduce_motion);
        preferences.cross_fade_transitions = state_of(self.prefers_cross_fade);
        preferences.reduced_transparency = state_of(self.reduce_transparency);
        preferences.invert_colors = state_of(self.invert_colors);
        preferences.grayscale = state_of(self.grayscale);
        preferences.differentiate_without_color = state_of(self.differentiate_without_color);
        preferences.button_shapes = state_of(self.button_shapes);
        preferences.on_off_labels = state_of(self.on_off_labels);
        preferences.screen_reader = state_of(self.voice_over);
        preferences.switch_access = state_of(self.switch_control);
        preferences.captions = state_of(self.closed_captioning);
        preferences.mono_audio = state_of(self.mono_audio);
        preferences.autoplay_video = state_of(self.video_autoplay);
        preferences.dim_flashing_lights = state_of(self.reduce_white_point);
        preferences.magnifier = state_of(self.zoom);
        preferences
    }
}

// ---------------------------------------------------------------------------------------------
// Android
// ---------------------------------------------------------------------------------------------

/// `Configuration.FONT_WEIGHT_ADJUSTMENT_UNDEFINED` (`Integer.MAX_VALUE`): no adjustment reported.
pub const ANDROID_FONT_WEIGHT_UNDEFINED: i32 = i32::MAX;

/// What the Android framework answered, through JNI.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RawAndroid {
    /// `Configuration.fontScale`.
    pub font_scale: Option<f32>,
    /// The factor the framework itself converts an `sp` with at the body size:
    /// `TypedValue.`applyDimension`(COMPLEX_UNIT_SP, 16, metrics) / `applyDimension`(COMPLEX_UNIT_DIP,
    /// 16, metrics)`. Nonlinear on Android 14 and later, where `fontScale` alone is not what an
    /// `sp` becomes.
    pub sp_scale: Option<f32>,
    /// `Configuration.fontWeightAdjustment` (API 31).
    pub font_weight_adjustment: Option<i32>,
    /// `UiModeManager.getContrast()` (API 34): 0 standard, 0.5 medium, 1 high.
    pub contrast: Option<f32>,
    /// "High contrast text" / "Outline text": `AccessibilityManager.isHighContrastTextEnabled()`
    /// (API 36), else `Settings.Secure` `high_text_contrast_enabled`.
    pub high_text_contrast: Option<bool>,
    /// `Settings.Global` `disable_window_blurs` ("Reduce blur effects"): the blur is off.
    pub blur_disabled: Option<bool>,
    /// `Settings.Global` `animator_duration_scale`.
    pub animator_duration_scale: Option<f32>,
    /// `Settings.Secure` `accessibility_display_inversion_enabled`.
    pub invert_colors: Option<bool>,
    /// `Settings.Secure` `accessibility_display_daltonizer_enabled`.
    pub daltonizer_enabled: Option<bool>,
    /// `Settings.Secure` `accessibility_display_daltonizer` (0 is monochromacy: grayscale).
    pub daltonizer_type: Option<i32>,
    /// `AccessibilityManager.isTouchExplorationEnabled()` (`TalkBack`).
    pub touch_exploration: Option<bool>,
    /// `AccessibilityManager.isEnabled()`: some accessibility service is on.
    pub accessibility_enabled: Option<bool>,
    /// `CaptioningManager.isEnabled()`.
    pub captions: Option<bool>,
    /// `Settings.System` `master_mono`.
    pub mono_audio: Option<bool>,
    /// `Settings.Secure` `accessibility_display_magnification_enabled`.
    pub magnification: Option<bool>,
    /// `Settings.Secure` `long_press_timeout`, in milliseconds.
    pub long_press_ms: Option<i32>,
    /// `AccessibilityManager.getRecommendedTimeoutMillis(...)` for a base of 5000 ms.
    pub action_timeout_ms: Option<i32>,
    /// `Settings.System` `haptic_feedback_enabled`.
    pub haptic_feedback: Option<bool>,
    /// `Settings.System` `pointer_scale` (Android 15: the pointer size, 1.0 by default).
    pub pointer_scale: Option<f32>,
}

/// `value` as a number of milliseconds a preference holds: negative or absent is unknown.
fn millis_of(value: Option<i32>) -> OptionU32 {
    match value.and_then(|ms| u32::try_from(ms).ok()) {
        Some(ms) => OptionU32::Some(ms),
        None => OptionU32::None,
    }
}

impl RawAndroid {
    /// The preferences this Android state means.
    #[must_use]
    pub fn into_preferences(&self) -> AccessibilityPreferences {
        let mut preferences = AccessibilityPreferences::default();
        // The framework's own conversion of the body size, else the plain font scale.
        let scale = self
            .sp_scale
            .filter(|scale| scale.is_finite() && *scale > 0.0)
            .or(self.font_scale.filter(|scale| scale.is_finite() && *scale > 0.0));
        if let Some(scale) = scale {
            drop(preferences.set_text_scale(scale));
        }
        match self.font_weight_adjustment {
            Some(ANDROID_FONT_WEIGHT_UNDEFINED) | None => {}
            Some(adjustment) => {
                let step = i16::try_from(adjustment.clamp(0, 1000)).unwrap_or(0);
                preferences.bold_weight_adjustment = OptionI16::Some(step);
                preferences.bold_text = SettingState::from_bool(step > 0);
            }
        }
        preferences.contrast = match (self.contrast, self.high_text_contrast) {
            (Some(level), _) if level >= 0.33 => ContrastPreference::More,
            (_, Some(true)) => ContrastPreference::More,
            (Some(level), _) if level <= -0.33 => ContrastPreference::Less,
            (Some(_), _) | (None, Some(false)) => ContrastPreference::NoPreference,
            (None, None) => ContrastPreference::Unknown,
        };
        if let Some(scale) = self.animator_duration_scale.filter(|s| s.is_finite() && *s >= 0.0) {
            preferences.animation_scale = OptionF32::Some(scale);
            // "Remove animations" (and developer options) set the scale to zero
            preferences.reduced_motion = SettingState::from_bool(scale <= 0.0);
        }
        preferences.invert_colors = state_of(self.invert_colors);
        preferences.color_filter = state_of(self.daltonizer_enabled);
        preferences.grayscale = match (self.daltonizer_enabled, self.daltonizer_type) {
            (Some(true), Some(0)) => SettingState::On,
            (Some(_), Some(_)) => SettingState::Off,
            (Some(false), None) => SettingState::Off,
            _ => SettingState::Unknown,
        };
        // "Reduce bright colors" (`reduce_bright_colors_activated`) cannot be read by an app, and
        // neither can the buttons' shapes (no such option in AOSP): both stay unknown.
        preferences.reduced_transparency = state_of(self.blur_disabled);
        preferences.screen_reader = state_of(self.touch_exploration);
        // an accessibility service is on that is not touch exploration: Switch Access, Voice
        // Access, a third party's; the precise ones need the service list (not read)
        preferences.captions = state_of(self.captions);
        preferences.mono_audio = state_of(self.mono_audio);
        preferences.magnifier = state_of(self.magnification);
        preferences.long_press_ms = millis_of(self.long_press_ms);
        preferences.action_timeout_ms = millis_of(self.action_timeout_ms);
        preferences.haptics = state_of(self.haptic_feedback);
        if let Some(scale) = self.pointer_scale.filter(|s| s.is_finite() && *s > 0.0) {
            preferences.pointer_scale = OptionF32::Some(scale.min(crate::accessibility::MAX_POINTER_SCALE));
        }
        preferences
    }
}

// ---------------------------------------------------------------------------------------------
// Windows
// ---------------------------------------------------------------------------------------------

/// What Windows answered: the registry values and `SystemParametersInfo`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RawWindows {
    /// `HKCU\SOFTWARE\Microsoft\Accessibility` `TextScaleFactor` (percent, 100 to 225); absent
    /// while the slider is at its default.
    pub text_scale_percent: Option<u32>,
    /// `SPI_GETHIGHCONTRAST` `HIGHCONTRAST.dwFlags & HCF_HIGHCONTRASTON`.
    pub high_contrast: Option<bool>,
    /// `SPI_GETCLIENTAREAANIMATION`.
    pub client_area_animation: Option<bool>,
    /// `HKCU\...\Themes\Personalize` `EnableTransparency`.
    pub transparency_effects: Option<bool>,
    /// `HKCU\Control Panel\Cursors` `CursorBaseSize` (pixels, 32 by default).
    pub cursor_base_size: Option<u32>,
    /// `HKCU\Software\Microsoft\ColorFiltering` `Active`.
    pub color_filter_active: Option<bool>,
    /// `HKCU\Software\Microsoft\ColorFiltering` `FilterType` (0 grayscale, 1 inverted ...).
    pub color_filter_type: Option<u32>,
    /// `SPI_GETSCREENREADER`, or a UI Automation client that is listening.
    pub screen_reader: Option<bool>,
    /// `SPI_GETMESSAGEDURATION`, in seconds.
    pub message_duration_s: Option<u32>,
    /// `HKCU\Software\Microsoft\Multimedia\Audio` `AccessibilityMonoMixState`.
    pub mono_audio: Option<bool>,
    /// `SPI_GETKEYBOARDCUES` / "always underline access keys": keyboard navigation is the way.
    pub keyboard_cues: Option<bool>,
}

impl RawWindows {
    /// The preferences this Windows state means.
    #[must_use]
    pub fn into_preferences(&self) -> AccessibilityPreferences {
        let mut preferences = AccessibilityPreferences::default();
        // The slider is at 100 % while the value is absent: that IS an answer. (It reaches 225;
        // a value past 1000 is no slider's, and the engine caps the scale on its own.)
        let percent = u16::try_from(self.text_scale_percent.unwrap_or(100).min(1000)).unwrap_or(100);
        drop(preferences.set_text_scale(f32::from(percent) / 100.0));
        // No "bold text" on Windows: left unknown.
        preferences.forced_colors = state_of(self.high_contrast);
        preferences.contrast = contrast_of(self.high_contrast);
        preferences.reduced_motion = state_of(self.client_area_animation.map(|on| !on));
        preferences.reduced_transparency = state_of(self.transparency_effects.map(|on| !on));
        if let Some(size) = self.cursor_base_size.filter(|size| *size > 0) {
            let size = u16::try_from(size.min(1000)).unwrap_or(32);
            preferences.pointer_scale = OptionF32::Some(f32::from(size) / 32.0);
        }
        preferences.color_filter = state_of(self.color_filter_active);
        // FilterType: 0 grayscale, 1 inverted, 2 grayscale inverted, 3 to 5 the colour blindness filters
        preferences.invert_colors = match (self.color_filter_active, self.color_filter_type) {
            (Some(true), Some(1 | 2)) => SettingState::On,
            (Some(_), _) => SettingState::Off,
            (None, _) => SettingState::Unknown,
        };
        preferences.grayscale = match (self.color_filter_active, self.color_filter_type) {
            (Some(true), Some(0 | 2)) => SettingState::On,
            (Some(_), _) => SettingState::Off,
            (None, _) => SettingState::Unknown,
        };
        preferences.screen_reader = state_of(self.screen_reader);
        preferences.action_timeout_ms = match self.message_duration_s {
            Some(seconds) if seconds > 0 => OptionU32::Some(seconds.saturating_mul(1000)),
            _ => OptionU32::None,
        };
        preferences.mono_audio = state_of(self.mono_audio);
        preferences.keyboard_navigation = state_of(self.keyboard_cues);
        preferences
    }
}

// ---------------------------------------------------------------------------------------------
// macOS
// ---------------------------------------------------------------------------------------------

/// What `AppKit` answered (`NSWorkspace.shared.accessibilityDisplayShould...`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RawMacos {
    /// `accessibilityDisplayShouldReduceMotion`.
    pub reduce_motion: Option<bool>,
    /// `accessibilityDisplayShouldIncreaseContrast`.
    pub increase_contrast: Option<bool>,
    /// `accessibilityDisplayShouldReduceTransparency`.
    pub reduce_transparency: Option<bool>,
    /// `accessibilityDisplayShouldDifferentiateWithoutColor`.
    pub differentiate_without_color: Option<bool>,
    /// `accessibilityDisplayShouldInvertColors`.
    pub invert_colors: Option<bool>,
    /// `isVoiceOverEnabled`.
    pub voice_over: Option<bool>,
    /// `isSwitchControlEnabled`.
    pub switch_control: Option<bool>,
    /// `NSApplication.isFullKeyboardAccessEnabled`.
    pub full_keyboard_access: Option<bool>,
    /// The scroll bar style: `NSScroller.preferredScrollerStyle` (0 legacy: always shown).
    pub scrollers_always_shown: Option<bool>,
}

impl RawMacos {
    /// The preferences this macOS state means.
    #[must_use]
    pub fn into_preferences(&self) -> AccessibilityPreferences {
        AccessibilityPreferences {
            reduced_motion: state_of(self.reduce_motion),
            contrast: contrast_of(self.increase_contrast),
            reduced_transparency: state_of(self.reduce_transparency),
            differentiate_without_color: state_of(self.differentiate_without_color),
            invert_colors: state_of(self.invert_colors),
            screen_reader: state_of(self.voice_over),
            switch_access: state_of(self.switch_control),
            keyboard_navigation: state_of(self.full_keyboard_access),
            ..AccessibilityPreferences::default()
        }
    }
}

// ---------------------------------------------------------------------------------------------
// GNOME and KDE
// ---------------------------------------------------------------------------------------------

/// What GNOME's settings (gsettings, or the settings portal) answered.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RawGnome {
    /// `org.gnome.desktop.interface text-scaling-factor` (1.0 normal, 1.25 "Large text").
    pub text_scaling_factor: Option<f32>,
    /// `org.gnome.desktop.interface enable-animations`.
    pub enable_animations: Option<bool>,
    /// `org.gnome.desktop.a11y.interface high-contrast`.
    pub high_contrast: Option<bool>,
    /// `org.gnome.desktop.interface cursor-size` (24 by default).
    pub cursor_size: Option<u32>,
    /// `org.gnome.desktop.a11y.applications screen-reader-enabled`.
    pub screen_reader: Option<bool>,
    /// `org.gnome.desktop.a11y.applications screen-magnifier-enabled`.
    pub screen_magnifier: Option<bool>,
    /// `org.freedesktop.appearance contrast` of the settings portal: 0 no preference, 1 more.
    pub portal_contrast: Option<u32>,
    /// `org.freedesktop.appearance reduced-motion` of the settings portal (1 reduce).
    pub portal_reduced_motion: Option<u32>,
}

impl RawGnome {
    /// The preferences this GNOME state means.
    #[must_use]
    pub fn into_preferences(&self) -> AccessibilityPreferences {
        let mut preferences = AccessibilityPreferences::default();
        if let Some(factor) = self.text_scaling_factor.filter(|factor| factor.is_finite() && *factor > 0.0) {
            drop(preferences.set_text_scale(factor));
        }
        let portal_motion = self.portal_reduced_motion.map(|value| value == 1);
        preferences.reduced_motion = state_of(either(self.enable_animations.map(|on| !on), portal_motion));
        let portal_contrast = self.portal_contrast.map(|value| value == 1);
        preferences.contrast = contrast_of(either(self.high_contrast, portal_contrast));
        if let Some(size) = self.cursor_size.filter(|size| *size > 0) {
            let size = u16::try_from(size.min(1000)).unwrap_or(24);
            preferences.pointer_scale = OptionF32::Some(f32::from(size) / 24.0);
        }
        preferences.screen_reader = state_of(self.screen_reader);
        preferences.magnifier = state_of(self.screen_magnifier);
        preferences
    }
}

/// What KDE Plasma's configuration answered.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RawKde {
    /// `kdeglobals` `[KDE] AnimationDurationFactor`: 1 normal, 0 animations off.
    pub animation_duration_factor: Option<f32>,
    /// `kcmfonts` `[General] forceFontDPI` (96 is the default size).
    pub force_font_dpi: Option<f32>,
}

impl RawKde {
    /// The preferences this KDE state means.
    #[must_use]
    pub fn into_preferences(&self) -> AccessibilityPreferences {
        let mut preferences = AccessibilityPreferences::default();
        if let Some(factor) = self
            .animation_duration_factor
            .filter(|factor| factor.is_finite() && *factor >= 0.0)
        {
            preferences.animation_scale = OptionF32::Some(factor);
            preferences.reduced_motion = SettingState::from_bool(factor <= 0.0);
        }
        if let Some(dpi) = self.force_font_dpi.filter(|dpi| dpi.is_finite() && *dpi > 0.0) {
            drop(preferences.set_text_scale(dpi / 96.0));
        }
        preferences
    }
}

#[cfg(test)]
#[path = "accessibility_raw_test.rs"]
mod accessibility_raw_test;
