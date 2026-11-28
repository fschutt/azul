//! The accessibility and display-comfort preferences of the system, as the platform reports them.
//!
//! ONE typed place for every option a user can set in the system's accessibility and display
//! settings (text size, bold text, contrast, reduced motion, reduced transparency, ...): the
//! platform shells fill [`AccessibilityPreferences`] at the start and again whenever the platform
//! says one of them changed ([`crate::system::SystemStyle::set_preferences`]); the cascade reads
//! it through the dynamic-selector context (`@media (prefers-contrast: more)` and its siblings);
//! the layout reads the text scale from the same context; widgets and apps read it from
//! `CallbackInfo`. Nobody below the shell asks the platform.
//!
//! Every option is a [`SettingState`] (`Unknown`, `Off`, `On`) or an optional number: a platform
//! that has no such option, or cannot read it, says `Unknown` and never a silent `false`. An
//! `Unknown` option is treated as the default look by every consumer, and an override
//! ([`AccessibilityPreferences::overlaid`]) leaves an `Unknown` field where it was.
//!
//! The same struct is the TEST override: `AZ_A11Y_<FIELD>` in the environment
//! ([`AccessibilityPreferences::from_lookup`]), the `set_system_accessibility` e2e op and
//! `CallbackInfo::set_accessibility_override` all say "these fields, whatever the system reports",
//! so one scenario file runs the same on a desktop, an emulator and a phone.

use alloc::{string::String, vec::Vec};

use crate::{
    corety::{OptionF32, OptionI16, OptionU32},
    dynamic_selector::BoolCondition,
    env::EnvSource,
};

/// The smallest text scale the engine applies: 50 % of the platform's default text size (GNOME's
/// `text-scaling-factor` allows 0.5, Android's smallest font scale is 0.85, iOS's smallest content
/// size category is 0.82).
pub const MIN_TEXT_SCALE: f32 = 0.5;

/// The greatest text scale the engine applies: 400 % (iOS's largest accessibility size is 3.12,
/// GNOME's `text-scaling-factor` allows 3.0, Windows 2.25, Android 2.0). A larger report is a
/// broken value and is read as this, with a diagnostic.
pub const MAX_TEXT_SCALE: f32 = 4.0;

/// The greatest animation speed factor read (Android's `animator_duration_scale` offers 10).
pub const MAX_ANIMATION_SCALE: f32 = 10.0;

/// The greatest pointer scale read (Windows' pointer size goes to 15 steps of about 0.5).
pub const MAX_POINTER_SCALE: f32 = 8.0;

/// The longest timeout read for a touch hold or a "time to take action": ten minutes.
pub const MAX_TIMEOUT_MS: u32 = 600_000;

/// The weight Android adds to every text when "Bold text" is on (`Configuration.fontWeightAdjustment`
/// is 300 then); the engine uses it for every platform that only says "bold text on".
pub const BOLD_TEXT_WEIGHT_STEP: i16 = 300;

/// One system option that may not be known: the platform has no such option, the platform
/// answered nothing, or the shell has not asked yet.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum SettingState {
    /// Not known: no such option here, no answer, or not asked yet. Every consumer treats it as
    /// the default look.
    #[default]
    Unknown,
    /// The user turned the option off (or never turned it on, where the platform can tell).
    Off,
    /// The user turned the option on.
    On,
}

impl SettingState {
    /// `On` for `true`, `Off` for `false`: a platform that answered.
    #[must_use]
    pub const fn from_bool(on: bool) -> Self {
        if on {
            Self::On
        } else {
            Self::Off
        }
    }

    /// The platform answered (`Off` or `On`).
    #[must_use]
    pub const fn is_known(self) -> bool {
        !matches!(self, Self::Unknown)
    }

    /// The option is on. `Unknown` is not.
    #[must_use]
    pub const fn is_on(self) -> bool {
        matches!(self, Self::On)
    }

    /// The option is known to be off.
    #[must_use]
    pub const fn is_off(self) -> bool {
        matches!(self, Self::Off)
    }

    /// `self` where it is known, `fallback` where it is not (the override rule).
    #[must_use]
    pub const fn or(self, fallback: Self) -> Self {
        if self.is_known() {
            self
        } else {
            fallback
        }
    }

    /// The condition a stylesheet tests: `True` only for `On`.
    #[must_use]
    pub const fn as_condition(self) -> BoolCondition {
        if self.is_on() {
            BoolCondition::True
        } else {
            BoolCondition::False
        }
    }

    /// The word the diagnostics, the JSON dump and the e2e ops use.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Off => "off",
            Self::On => "on",
        }
    }

    /// Reads a state from the words the environment, the e2e ops and the platform tools use
    /// (`1`, `true`, `on`, `yes`, `enabled`, `active`, `reduce`, `more`, `bold`, `inverted`: on;
    /// `0`, `false`, `off`, `no`, `disabled`, `none`, `no-preference`, `normal`: off; `unknown`,
    /// `auto`, `default`, `unset` and an empty value: unknown). `None` for anything else.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "on" | "yes" | "enabled" | "enable" | "active" | "reduce" | "more"
            | "bold" | "inverted" | "differentiate" => Some(Self::On),
            "0" | "false" | "off" | "no" | "disabled" | "disable" | "none" | "no-preference"
            | "normal" | "standard" => Some(Self::Off),
            "" | "unknown" | "auto" | "default" | "unset" => Some(Self::Unknown),
            _ => None,
        }
    }
}

impl From<bool> for SettingState {
    fn from(on: bool) -> Self {
        Self::from_bool(on)
    }
}

/// How much contrast the user asks for (`prefers-contrast` of CSS Media Queries 5).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum ContrastPreference {
    /// Not known (see [`SettingState::Unknown`]).
    #[default]
    Unknown,
    /// The platform answered and the user asks for nothing special.
    NoPreference,
    /// More contrast than usual: iOS / macOS "Increase contrast", Android's medium or high
    /// "Color contrast" (14+) and "High contrast text", GNOME's high-contrast, the XDG portal's
    /// `contrast`.
    More,
    /// Less contrast than usual (the portal and CSS allow it; no mobile platform has it).
    Less,
}

impl ContrastPreference {
    /// The user asks for more contrast.
    #[must_use]
    pub const fn is_more(self) -> bool {
        matches!(self, Self::More)
    }

    /// The word the diagnostics and the e2e ops use.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::NoPreference => "no-preference",
            Self::More => "more",
            Self::Less => "less",
        }
    }

    /// Reads a contrast preference from its words (`more`, `high`, `increase`, `on`, `1`;
    /// `less`, `low`; `no-preference`, `standard`, `normal`, `off`, `none`, `0`; `unknown`,
    /// `auto`, `default`, empty). `None` for anything else.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "more" | "high" | "increase" | "increased" | "on" | "true" | "1" | "active" => {
                Some(Self::More)
            }
            "less" | "low" | "decrease" | "decreased" => Some(Self::Less),
            "no-preference" | "standard" | "normal" | "off" | "false" | "none" | "0" => {
                Some(Self::NoPreference)
            }
            "" | "unknown" | "auto" | "default" | "unset" => Some(Self::Unknown),
            _ => None,
        }
    }
}

/// The text size the user chose, in the categories of iOS's Dynamic Type (the other platforms
/// derive the nearest one from their factor: [`Self::nearest`]).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum TextSizeCategory {
    /// Not known.
    #[default]
    Unknown,
    /// `UIContentSizeCategoryExtraSmall` (body text 14 pt).
    ExtraSmall,
    /// `UIContentSizeCategorySmall` (15 pt).
    Small,
    /// `UIContentSizeCategoryMedium` (16 pt).
    Medium,
    /// `UIContentSizeCategoryLarge`, the default (17 pt).
    Large,
    /// `UIContentSizeCategoryExtraLarge` (19 pt).
    ExtraLarge,
    /// `UIContentSizeCategoryExtraExtraLarge` (21 pt).
    ExtraExtraLarge,
    /// `UIContentSizeCategoryExtraExtraExtraLarge` (23 pt).
    ExtraExtraExtraLarge,
    /// `UIContentSizeCategoryAccessibilityMedium`, AX1 (28 pt).
    AccessibilityMedium,
    /// `UIContentSizeCategoryAccessibilityLarge`, AX2 (33 pt).
    AccessibilityLarge,
    /// `UIContentSizeCategoryAccessibilityExtraLarge`, AX3 (40 pt).
    AccessibilityExtraLarge,
    /// `UIContentSizeCategoryAccessibilityExtraExtraLarge`, AX4 (47 pt).
    AccessibilityExtraExtraLarge,
    /// `UIContentSizeCategoryAccessibilityExtraExtraExtraLarge`, AX5 (53 pt).
    AccessibilityExtraExtraExtraLarge,
}

impl TextSizeCategory {
    /// Every known category, smallest first.
    pub const ALL: [Self; 12] = [
        Self::ExtraSmall,
        Self::Small,
        Self::Medium,
        Self::Large,
        Self::ExtraLarge,
        Self::ExtraExtraLarge,
        Self::ExtraExtraExtraLarge,
        Self::AccessibilityMedium,
        Self::AccessibilityLarge,
        Self::AccessibilityExtraLarge,
        Self::AccessibilityExtraExtraLarge,
        Self::AccessibilityExtraExtraExtraLarge,
    ];

    /// The size of the Body text style at this category, in points (the Typography page of
    /// Apple's Human Interface Guidelines: iOS and iPadOS), `None` for `Unknown`.
    #[must_use]
    pub const fn body_points(self) -> Option<u8> {
        match self {
            Self::Unknown => None,
            Self::ExtraSmall => Some(14),
            Self::Small => Some(15),
            Self::Medium => Some(16),
            Self::Large => Some(17),
            Self::ExtraLarge => Some(19),
            Self::ExtraExtraLarge => Some(21),
            Self::ExtraExtraExtraLarge => Some(23),
            Self::AccessibilityMedium => Some(28),
            Self::AccessibilityLarge => Some(33),
            Self::AccessibilityExtraLarge => Some(40),
            Self::AccessibilityExtraExtraLarge => Some(47),
            Self::AccessibilityExtraExtraExtraLarge => Some(53),
        }
    }

    /// The factor of this category against the default one (`Large`, 17 pt): 0.82 for the
    /// smallest, 1.0 for the default, 3.12 for AX5. `None` for `Unknown`.
    #[must_use]
    pub fn scale_factor(self) -> Option<f32> {
        self.body_points().map(|points| f32::from(points) / 17.0)
    }

    /// This is one of the five accessibility sizes (AX1 to AX5).
    #[must_use]
    pub const fn is_accessibility(self) -> bool {
        matches!(
            self,
            Self::AccessibilityMedium
                | Self::AccessibilityLarge
                | Self::AccessibilityExtraLarge
                | Self::AccessibilityExtraExtraLarge
                | Self::AccessibilityExtraExtraExtraLarge
        )
    }

    /// The category whose factor is nearest to `factor` (a factor below the smallest is the
    /// smallest, one above the largest is the largest, a factor that is not a number is
    /// `Unknown`).
    #[must_use]
    pub fn nearest(factor: f32) -> Self {
        if !factor.is_finite() {
            return Self::Unknown;
        }
        let mut best = Self::Large;
        let mut best_distance = f32::MAX;
        for category in Self::ALL {
            let Some(candidate) = category.scale_factor() else {
                continue;
            };
            let distance = (candidate - factor).abs();
            if distance < best_distance {
                best_distance = distance;
                best = category;
            }
        }
        best
    }

    /// The name `xcrun simctl ui <device> content_size` takes (`large`, `extra-extra-large`,
    /// `accessibility-extra-extra-extra-large`), `None` for `Unknown`.
    #[must_use]
    pub const fn simctl_name(self) -> Option<&'static str> {
        match self {
            Self::Unknown => None,
            Self::ExtraSmall => Some("extra-small"),
            Self::Small => Some("small"),
            Self::Medium => Some("medium"),
            Self::Large => Some("large"),
            Self::ExtraLarge => Some("extra-large"),
            Self::ExtraExtraLarge => Some("extra-extra-large"),
            Self::ExtraExtraExtraLarge => Some("extra-extra-extra-large"),
            Self::AccessibilityMedium => Some("accessibility-medium"),
            Self::AccessibilityLarge => Some("accessibility-large"),
            Self::AccessibilityExtraLarge => Some("accessibility-extra-large"),
            Self::AccessibilityExtraExtraLarge => Some("accessibility-extra-extra-large"),
            Self::AccessibilityExtraExtraExtraLarge => {
                Some("accessibility-extra-extra-extra-large")
            }
        }
    }

    /// Reads a category from any spelling it has: the simctl names, UIKit's constants
    /// (`UIContentSizeCategoryExtraSmall`), the raw strings `preferredContentSizeCategory`
    /// answers (`UICTContentSizeCategoryXS`, `UICTContentSizeCategoryAccessibilityXXXL`), and the
    /// short forms `xs`, `s`, `m`, `l`, `xl`, `xxl`, `xxxl`, `ax1` to `ax5`. Case, dashes and
    /// underscores do not matter. `None` for a name that is none of them; `unspecified` is
    /// `Some(Unknown)`.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let mut key = String::new();
        for c in text.chars() {
            if c.is_ascii_alphanumeric() {
                key.push(c.to_ascii_lowercase());
            }
        }
        let key = key.as_str();
        let key = key
            .strip_prefix("uictcontentsizecategory")
            .or_else(|| key.strip_prefix("uicontentsizecategory"))
            .or_else(|| key.strip_prefix("contentsizecategory"))
            .unwrap_or(key);
        Some(match key {
            "xs" | "extrasmall" => Self::ExtraSmall,
            "s" | "small" => Self::Small,
            "m" | "medium" => Self::Medium,
            "l" | "large" => Self::Large,
            "xl" | "extralarge" => Self::ExtraLarge,
            "xxl" | "extraextralarge" => Self::ExtraExtraLarge,
            "xxxl" | "extraextraextralarge" => Self::ExtraExtraExtraLarge,
            "ax1" | "accessibilitym" | "accessibilitymedium" => Self::AccessibilityMedium,
            "ax2" | "accessibilityl" | "accessibilitylarge" => Self::AccessibilityLarge,
            "ax3" | "accessibilityxl" | "accessibilityextralarge" => {
                Self::AccessibilityExtraLarge
            }
            "ax4" | "accessibilityxxl" | "accessibilityextraextralarge" => {
                Self::AccessibilityExtraExtraLarge
            }
            "ax5" | "accessibilityxxxl" | "accessibilityextraextraextralarge" => {
                Self::AccessibilityExtraExtraExtraLarge
            }
            "unspecified" | "unknown" => Self::Unknown,
            _ => return None,
        })
    }
}

/// A text scale as the engine takes it: finite, within [`MIN_TEXT_SCALE`] and
/// [`MAX_TEXT_SCALE`]. `None` for a value that is not a number. The second value says the scale
/// had to be capped (the caller logs it).
#[must_use]
pub fn clamp_text_scale(factor: f32) -> Option<(f32, bool)> {
    if factor.is_nan() {
        return None;
    }
    let clamped = factor.clamp(MIN_TEXT_SCALE, MAX_TEXT_SCALE);
    // The bits tell a real cap from a value that was already in range (no float comparison).
    Some((clamped, clamped.to_bits() != factor.to_bits()))
}

/// What the user set in the accessibility and display settings of the system. `Default` knows
/// nothing: every option `Unknown`.
///
/// Fields are APPENDED, never reordered. A field a platform cannot read stays `Unknown`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct AccessibilityPreferences {
    // ---- text ----
    /// How much bigger (or smaller) than its default the user wants text: 1.0 is the default.
    /// iOS: the content size category; Android: `Configuration.fontScale`; Windows:
    /// `UISettings.TextScaleFactor`; GNOME: `text-scaling-factor`; KDE: the font DPI against 96.
    pub text_scale: OptionF32,
    /// The category the text scale is (iOS reports it; the other platforms derive it).
    pub text_size_category: TextSizeCategory,
    /// "Bold text" (iOS, Android 12+): heavier weights for all text.
    pub bold_text: SettingState,
    /// The weight Android adds to text (`Configuration.fontWeightAdjustment`, 300 with Bold
    /// text on, 0 off); unknown elsewhere (the engine then uses [`BOLD_TEXT_WEIGHT_STEP`]).
    pub bold_weight_adjustment: OptionI16,
    // ---- contrast and colour ----
    /// "Increase contrast" and its siblings.
    pub contrast: ContrastPreference,
    /// A contrast theme replaces the colours of the app (Windows high contrast themes, the
    /// GNOME HighContrast theme): the app must use the system's palette, not its own.
    pub forced_colors: SettingState,
    /// "Invert colors" (classic or smart).
    pub invert_colors: SettingState,
    /// Grayscale display.
    pub grayscale: SettingState,
    /// A colour filter or correction (Android daltonizer, Windows colour filters, iOS colour
    /// filters).
    pub color_filter: SettingState,
    /// "Differentiate without colour": information must not rest on colour alone.
    pub differentiate_without_color: SettingState,
    /// "Reduce transparency" (iOS, macOS, Windows "Transparency effects" off).
    pub reduced_transparency: SettingState,
    /// "Button shapes" (iOS): an underline or outline marks what is tappable.
    pub button_shapes: SettingState,
    /// "On/Off labels" (iOS): a switch shows 0 and 1 as well as its colour.
    pub on_off_labels: SettingState,
    // ---- motion and media ----
    /// "Reduce motion" (iOS, macOS), "Remove animations" (Android), "Show animations in
    /// Windows" off, GNOME `enable-animations` off, KDE's animation factor of 0.
    pub reduced_motion: SettingState,
    /// iOS "Prefer cross-fade transitions": slides and zooms become fades.
    pub cross_fade_transitions: SettingState,
    /// The animation speed factor of the system (Android `animator_duration_scale`, KDE
    /// `AnimationDurationFactor`): 1.0 normal, 0.0 animations off.
    pub animation_scale: OptionF32,
    /// Animated images may play by themselves (iOS "Auto-Play Animated Images").
    pub autoplay_animated_images: SettingState,
    /// Video previews may play by themselves (iOS "Auto-Play Video Previews").
    pub autoplay_video: SettingState,
    /// "Dim flashing lights" (iOS 17+), Android "Reduce bright colors".
    pub dim_flashing_lights: SettingState,
    // ---- assistive technology that is running ----
    /// A screen reader runs: VoiceOver, TalkBack, Narrator, NVDA, JAWS, Orca.
    pub screen_reader: SettingState,
    /// Switch Control (iOS), Switch Access (Android).
    pub switch_access: SettingState,
    /// Voice Control (iOS, macOS), Voice Access (Android), voice access (Windows 11).
    pub voice_control: SettingState,
    /// Full keyboard navigation: every control can be reached with Tab (macOS "Full Keyboard
    /// Access", iPadOS the same).
    pub keyboard_navigation: SettingState,
    /// A screen magnifier runs (Zoom, Magnification, Magnifier).
    pub magnifier: SettingState,
    /// Captions are wanted (closed captions, Live Captions).
    pub captions: SettingState,
    /// Mono audio.
    pub mono_audio: SettingState,
    /// Haptic feedback is on in the system's settings.
    pub haptics: SettingState,
    // ---- input and timing ----
    /// How much larger than usual the pointer is (1.0 normal).
    pub pointer_scale: OptionF32,
    /// The touch-and-hold delay the user chose, in milliseconds (Android
    /// `LONG_PRESS_TIMEOUT`).
    pub long_press_ms: OptionU32,
    /// How long the user wants transient UI to stay, in milliseconds (Android "Time to take
    /// action", Windows "Show notifications for").
    pub action_timeout_ms: OptionU32,
}

/// The names of the fields [`AccessibilityPreferences::set_from_text`] takes, in the order of the
/// struct: also the names of `AZ_A11Y_<NAME>` (upper case) and of the e2e op's keys.
pub const PREFERENCE_FIELDS: [&str; 30] = [
    "text_scale",
    "text_size_category",
    "bold_text",
    "bold_weight_adjustment",
    "contrast",
    "forced_colors",
    "invert_colors",
    "grayscale",
    "color_filter",
    "differentiate_without_color",
    "reduced_transparency",
    "button_shapes",
    "on_off_labels",
    "reduced_motion",
    "cross_fade_transitions",
    "animation_scale",
    "autoplay_animated_images",
    "autoplay_video",
    "dim_flashing_lights",
    "screen_reader",
    "switch_access",
    "voice_control",
    "keyboard_navigation",
    "magnifier",
    "captions",
    "mono_audio",
    "haptics",
    "pointer_scale",
    "long_press_ms",
    "action_timeout_ms",
];

impl AccessibilityPreferences {
    /// The text scale the layout applies: 1.0 where the platform said nothing, otherwise the
    /// reported factor within [`MIN_TEXT_SCALE`] and [`MAX_TEXT_SCALE`].
    #[must_use]
    pub fn effective_text_scale(&self) -> f32 {
        match self.text_scale {
            OptionF32::Some(scale) => clamp_text_scale(scale).map_or(1.0, |(scale, _)| scale),
            OptionF32::None => 1.0,
        }
    }

    /// The text scale differs from the default (the layout then resolves font sizes through the
    /// scale).
    #[must_use]
    pub fn text_scale_is_active(&self) -> bool {
        (self.effective_text_scale() - 1.0).abs() > 0.001
    }

    /// Motion is to be reduced: the user asked for it, or the system's animation speed is zero.
    #[must_use]
    pub fn reduces_motion(&self) -> bool {
        self.reduced_motion.is_on()
            || matches!(self.animation_scale, OptionF32::Some(scale) if scale <= 0.0)
    }

    /// The user wants more contrast, or a contrast theme is in force.
    #[must_use]
    pub const fn high_contrast(&self) -> bool {
        self.contrast.is_more() || self.forced_colors.is_on()
    }

    /// The weight to add to every text's weight: 0 without Bold text, the platform's own
    /// adjustment where it reported one, [`BOLD_TEXT_WEIGHT_STEP`] where it only said "on".
    #[must_use]
    pub fn bold_weight_step(&self) -> i16 {
        match self.bold_weight_adjustment {
            OptionI16::Some(step) if step > 0 => step.min(1000),
            _ if self.bold_text.is_on() => BOLD_TEXT_WEIGHT_STEP,
            _ => 0,
        }
    }

    /// No option is known.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// `self` with every field `over` knows taken from `over`: the override rule. An `Unknown`
    /// field of `over` leaves `self`'s alone.
    #[must_use]
    pub fn overlaid(&self, over: &Self) -> Self {
        let state = |mine: SettingState, theirs: SettingState| theirs.or(mine);
        let number = |mine: OptionF32, theirs: OptionF32| match theirs {
            OptionF32::Some(_) => theirs,
            OptionF32::None => mine,
        };
        Self {
            text_scale: number(self.text_scale, over.text_scale),
            text_size_category: if over.text_size_category == TextSizeCategory::Unknown {
                self.text_size_category
            } else {
                over.text_size_category
            },
            bold_text: state(self.bold_text, over.bold_text),
            bold_weight_adjustment: match over.bold_weight_adjustment {
                OptionI16::Some(_) => over.bold_weight_adjustment,
                OptionI16::None => self.bold_weight_adjustment,
            },
            contrast: if over.contrast == ContrastPreference::Unknown {
                self.contrast
            } else {
                over.contrast
            },
            forced_colors: state(self.forced_colors, over.forced_colors),
            invert_colors: state(self.invert_colors, over.invert_colors),
            grayscale: state(self.grayscale, over.grayscale),
            color_filter: state(self.color_filter, over.color_filter),
            differentiate_without_color: state(
                self.differentiate_without_color,
                over.differentiate_without_color,
            ),
            reduced_transparency: state(self.reduced_transparency, over.reduced_transparency),
            button_shapes: state(self.button_shapes, over.button_shapes),
            on_off_labels: state(self.on_off_labels, over.on_off_labels),
            reduced_motion: state(self.reduced_motion, over.reduced_motion),
            cross_fade_transitions: state(self.cross_fade_transitions, over.cross_fade_transitions),
            animation_scale: number(self.animation_scale, over.animation_scale),
            autoplay_animated_images: state(
                self.autoplay_animated_images,
                over.autoplay_animated_images,
            ),
            autoplay_video: state(self.autoplay_video, over.autoplay_video),
            dim_flashing_lights: state(self.dim_flashing_lights, over.dim_flashing_lights),
            screen_reader: state(self.screen_reader, over.screen_reader),
            switch_access: state(self.switch_access, over.switch_access),
            voice_control: state(self.voice_control, over.voice_control),
            keyboard_navigation: state(self.keyboard_navigation, over.keyboard_navigation),
            magnifier: state(self.magnifier, over.magnifier),
            captions: state(self.captions, over.captions),
            mono_audio: state(self.mono_audio, over.mono_audio),
            haptics: state(self.haptics, over.haptics),
            pointer_scale: number(self.pointer_scale, over.pointer_scale),
            long_press_ms: match over.long_press_ms {
                OptionU32::Some(_) => over.long_press_ms,
                OptionU32::None => self.long_press_ms,
            },
            action_timeout_ms: match over.action_timeout_ms {
                OptionU32::Some(_) => over.action_timeout_ms,
                OptionU32::None => self.action_timeout_ms,
            },
        }
    }

    /// Sets the text scale from a platform factor, capped to the range the engine applies, and
    /// derives the category the factor is nearest to (unless the caller names one afterwards).
    /// Returns a diagnostic when the factor had to be capped.
    pub fn set_text_scale(&mut self, factor: f32) -> Option<String> {
        let Some((scale, capped)) = clamp_text_scale(factor) else {
            self.text_scale = OptionF32::None;
            self.text_size_category = TextSizeCategory::Unknown;
            return Some(alloc::format!(
                "[azul][a11y] a text scale of {factor} is not a number: the text keeps its size"
            ));
        };
        self.text_scale = OptionF32::Some(scale);
        self.text_size_category = TextSizeCategory::nearest(scale);
        capped.then(|| {
            alloc::format!(
                "[azul][a11y] the system reports a text scale of {factor}; read as {scale} \
                 (the range is {MIN_TEXT_SCALE} to {MAX_TEXT_SCALE})"
            )
        })
    }

    /// Sets the text scale of a category (its factor, and the category itself).
    pub fn set_text_size_category(&mut self, category: TextSizeCategory) {
        self.text_size_category = category;
        self.text_scale = OptionF32::from(category.scale_factor());
    }

    /// Sets one field from its text: the field names of [`PREFERENCE_FIELDS`], the values of
    /// [`SettingState::parse`] (and the number, category or word each other field takes). `Ok`
    /// carries a diagnostic when the value had to be capped.
    ///
    /// # Errors
    ///
    /// A name that is no option (the message lists the options), or a value that is none of the
    /// words the option takes. The field is left as it was.
    pub fn set_from_text(&mut self, name: &str, value: &str) -> Result<Option<String>, String> {
        let name = name.trim().to_ascii_lowercase();
        let value = value.trim();
        match name.as_str() {
            "text_scale" => self.set_text_scale_from_text(value),
            "text_size_category" => {
                let category = TextSizeCategory::parse(value)
                    .ok_or_else(|| alloc::format!("text_size_category: \"{value}\" is no category"))?;
                self.set_text_size_category(category);
                Ok(None)
            }
            "bold_weight_adjustment" => {
                self.bold_weight_adjustment = if is_unset_word(value) {
                    OptionI16::None
                } else {
                    let step: i16 = value.parse().map_err(|_| {
                        alloc::format!("bold_weight_adjustment: \"{value}\" is not a weight step")
                    })?;
                    OptionI16::Some(step.clamp(0, 1000))
                };
                Ok(None)
            }
            "contrast" => {
                self.contrast = ContrastPreference::parse(value).ok_or_else(|| {
                    alloc::format!("contrast: \"{value}\" is not more, less, no-preference or unknown")
                })?;
                Ok(None)
            }
            "animation_scale" => {
                self.animation_scale = parse_scale(&name, value, MAX_ANIMATION_SCALE)?;
                Ok(None)
            }
            "pointer_scale" => {
                self.pointer_scale = parse_scale(&name, value, MAX_POINTER_SCALE)?;
                Ok(None)
            }
            "long_press_ms" => {
                self.long_press_ms = parse_millis(&name, value)?;
                Ok(None)
            }
            "action_timeout_ms" => {
                self.action_timeout_ms = parse_millis(&name, value)?;
                Ok(None)
            }
            other => {
                let parsed = SettingState::parse(value).ok_or_else(|| {
                    alloc::format!(
                        "{other}: \"{value}\" is not on, off or unknown (use 1/0, on/off, true/false)"
                    )
                });
                let slot = self.state_slot(other).ok_or_else(|| {
                    alloc::format!(
                        "\"{other}\" is no accessibility option (known: {})",
                        PREFERENCE_FIELDS.join(", ")
                    )
                })?;
                *slot = parsed?;
                Ok(None)
            }
        }
    }

    /// The `text_scale` field from its text: a size category (`ax5`, `large`, the simctl and
    /// UIKit spellings), a factor (`1.3`), or the words that put it back to unknown.
    fn set_text_scale_from_text(&mut self, value: &str) -> Result<Option<String>, String> {
        if let Some(category) = TextSizeCategory::parse(value) {
            self.set_text_size_category(category);
            return Ok(None);
        }
        if is_unset_word(value) {
            self.text_scale = OptionF32::None;
            self.text_size_category = TextSizeCategory::Unknown;
            return Ok(None);
        }
        let factor: f32 = value.parse().map_err(|_| {
            alloc::format!(
                "text_scale: \"{value}\" is neither a factor (1.3) nor a size category \
                 (large, accessibility-extra-large, ax3)"
            )
        })?;
        if factor.is_nan() {
            return Err(String::from("text_scale: NaN is not a factor"));
        }
        Ok(self.set_text_scale(factor))
    }

    /// The three-state field called `name`, `None` for a name that is not one.
    fn state_slot(&mut self, name: &str) -> Option<&mut SettingState> {
        Some(match name {
            "bold_text" => &mut self.bold_text,
            "forced_colors" => &mut self.forced_colors,
            "invert_colors" => &mut self.invert_colors,
            "grayscale" => &mut self.grayscale,
            "color_filter" => &mut self.color_filter,
            "differentiate_without_color" => &mut self.differentiate_without_color,
            "reduced_transparency" => &mut self.reduced_transparency,
            "button_shapes" => &mut self.button_shapes,
            "on_off_labels" => &mut self.on_off_labels,
            "reduced_motion" => &mut self.reduced_motion,
            "cross_fade_transitions" => &mut self.cross_fade_transitions,
            "autoplay_animated_images" => &mut self.autoplay_animated_images,
            "autoplay_video" => &mut self.autoplay_video,
            "dim_flashing_lights" => &mut self.dim_flashing_lights,
            "screen_reader" => &mut self.screen_reader,
            "switch_access" => &mut self.switch_access,
            "voice_control" => &mut self.voice_control,
            "keyboard_navigation" => &mut self.keyboard_navigation,
            "magnifier" => &mut self.magnifier,
            "captions" => &mut self.captions,
            "mono_audio" => &mut self.mono_audio,
            "haptics" => &mut self.haptics,
            _ => return None,
        })
    }

    /// The preferences the environment pins: `AZ_A11Y_<FIELD>` for every field of
    /// [`PREFERENCE_FIELDS`] (`AZ_A11Y_TEXT_SCALE=1.5`, `AZ_A11Y_BOLD_TEXT=on`,
    /// `AZ_A11Y_CONTRAST=more`). A field no variable names stays `Unknown`, so the pin leaves
    /// what the system reports. The second value lists what was wrong with a variable (a word
    /// that is none of the accepted ones, a number out of range): the caller logs each.
    #[must_use]
    pub fn from_lookup(env: &dyn EnvSource) -> (Self, Vec<String>) {
        let mut pin = Self::default();
        let mut notes = Vec::new();
        for field in PREFERENCE_FIELDS {
            let mut variable = String::from("AZ_A11Y_");
            variable.push_str(&field.to_ascii_uppercase());
            let Some(value) = env.var(&variable) else {
                continue;
            };
            match pin.set_from_text(field, &value) {
                Ok(None) => {}
                Ok(Some(note)) => notes.push(alloc::format!("{variable}: {note}")),
                Err(why) => notes.push(alloc::format!("{variable}: {why}")),
            }
        }
        (pin, notes)
    }

    /// Every option and its value as one JSON object (`{"text_scale": 1.30, "bold_text": "on",
    /// ...}`): what the `get_system_accessibility` e2e op and the diagnostics print. A number
    /// that is not known is `null`.
    #[must_use]
    pub fn to_json(&self) -> String {
        use core::fmt::Write;

        let mut out = String::from("{");
        let number = |value: OptionF32| match value {
            OptionF32::Some(n) => alloc::format!("{n:.3}"),
            OptionF32::None => String::from("null"),
        };
        let millis = |value: OptionU32| match value {
            OptionU32::Some(n) => alloc::format!("{n}"),
            OptionU32::None => String::from("null"),
        };
        let _ = write!(out, "\"text_scale\": {}", number(self.text_scale));
        let _ = write!(
            out,
            ", \"effective_text_scale\": {:.3}",
            self.effective_text_scale()
        );
        let _ = write!(
            out,
            ", \"text_size_category\": \"{}\"",
            self.text_size_category.simctl_name().unwrap_or("unknown")
        );
        let _ = write!(out, ", \"bold_text\": \"{}\"", self.bold_text.name());
        let _ = write!(
            out,
            ", \"bold_weight_adjustment\": {}",
            match self.bold_weight_adjustment {
                OptionI16::Some(step) => alloc::format!("{step}"),
                OptionI16::None => String::from("null"),
            }
        );
        let _ = write!(out, ", \"contrast\": \"{}\"", self.contrast.name());
        for (field, state) in [
            ("forced_colors", self.forced_colors),
            ("invert_colors", self.invert_colors),
            ("grayscale", self.grayscale),
            ("color_filter", self.color_filter),
            ("differentiate_without_color", self.differentiate_without_color),
            ("reduced_transparency", self.reduced_transparency),
            ("button_shapes", self.button_shapes),
            ("on_off_labels", self.on_off_labels),
            ("reduced_motion", self.reduced_motion),
            ("cross_fade_transitions", self.cross_fade_transitions),
        ] {
            let _ = write!(out, ", \"{field}\": \"{}\"", state.name());
        }
        let _ = write!(out, ", \"animation_scale\": {}", number(self.animation_scale));
        for (field, state) in [
            ("autoplay_animated_images", self.autoplay_animated_images),
            ("autoplay_video", self.autoplay_video),
            ("dim_flashing_lights", self.dim_flashing_lights),
            ("screen_reader", self.screen_reader),
            ("switch_access", self.switch_access),
            ("voice_control", self.voice_control),
            ("keyboard_navigation", self.keyboard_navigation),
            ("magnifier", self.magnifier),
            ("captions", self.captions),
            ("mono_audio", self.mono_audio),
            ("haptics", self.haptics),
        ] {
            let _ = write!(out, ", \"{field}\": \"{}\"", state.name());
        }
        let _ = write!(out, ", \"pointer_scale\": {}", number(self.pointer_scale));
        let _ = write!(out, ", \"long_press_ms\": {}", millis(self.long_press_ms));
        let _ = write!(out, ", \"action_timeout_ms\": {}", millis(self.action_timeout_ms));
        out.push('}');
        out
    }
}

/// The words that put an option back to "not known".
fn is_unset_word(text: &str) -> bool {
    matches!(text.to_ascii_lowercase().as_str(), "" | "unknown" | "auto" | "unset")
}

/// A scale from its text (`1.5`), capped at `greatest`; the unset words are `None`.
fn parse_scale(name: &str, text: &str, greatest: f32) -> Result<OptionF32, String> {
    if is_unset_word(text) {
        return Ok(OptionF32::None);
    }
    match text.parse::<f32>() {
        Ok(n) if n.is_finite() && n >= 0.0 => Ok(OptionF32::Some(n.min(greatest))),
        _ => Err(alloc::format!("{name}: \"{text}\" is not a number from 0 to {greatest}")),
    }
}

/// A number of milliseconds from its text, capped at [`MAX_TIMEOUT_MS`]; the unset words are `None`.
fn parse_millis(name: &str, text: &str) -> Result<OptionU32, String> {
    if is_unset_word(text) {
        return Ok(OptionU32::None);
    }
    text.parse::<u32>()
        .map(|ms| OptionU32::Some(ms.min(MAX_TIMEOUT_MS)))
        .map_err(|_| alloc::format!("{name}: \"{text}\" is not a number of milliseconds"))
}

/// A condition of a stylesheet on one of the accessibility preferences.
///
/// The `@media` features that map to them (`prefers-contrast: less`, `forced-colors: active`,
/// `prefers-reduced-transparency: reduce`, `inverted-colors: inverted`) and the azul-specific
/// ones where no standard exists (`min-text-scale: 1.3`, `prefers-bold-text: bold`,
/// `differentiate-without-color: active`, `button-shapes: active`, `on-off-labels: active`,
/// `screen-reader: active`, `prefers-cross-fade: reduce`, `keyboard-navigation: active`).
///
/// The reduced-motion and the "more contrast" features have their own selectors
/// (`DynamicSelector::PrefersReducedMotion`, `DynamicSelector::PrefersHighContrast`), older than
/// this enum.
///
/// `Unknown` options answer as the default look: they do not satisfy a condition that asks for
/// the option to be ON, and they do satisfy one that asks for it OFF (`no-preference`).
#[repr(C, u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AccessibilityCondition {
    /// `(min-text-scale: 1.3)`: the text scale is at least this many thousandths (1300).
    TextScaleAtLeast(u32),
    /// `(max-text-scale: 1.0)`: the text scale is at most this many thousandths.
    TextScaleAtMost(u32),
    /// `(prefers-bold-text: bold)` (True) or `no-preference` (False).
    BoldText(BoolCondition),
    /// `(prefers-contrast: less)`, `more` or `no-preference`. `More` also holds while a
    /// contrast theme is in force (`forced-colors: active`), as in the browsers.
    Contrast(ContrastPreference),
    /// `(forced-colors: active)` (True) or `none` (False).
    ForcedColors(BoolCondition),
    /// `(prefers-reduced-transparency: reduce)` (True) or `no-preference` (False).
    ReducedTransparency(BoolCondition),
    /// `(inverted-colors: inverted)` (True) or `none` (False).
    InvertedColors(BoolCondition),
    /// `(differentiate-without-color: active)` (True) or `none` (False).
    DifferentiateWithoutColor(BoolCondition),
    /// `(button-shapes: active)` (True) or `none` (False).
    ButtonShapes(BoolCondition),
    /// `(on-off-labels: active)` (True) or `none` (False).
    OnOffLabels(BoolCondition),
    /// `(screen-reader: active)` (True) or `none` (False).
    ScreenReader(BoolCondition),
    /// `(prefers-cross-fade: reduce)` (True) or `no-preference` (False).
    CrossFade(BoolCondition),
    /// `(keyboard-navigation: active)` (True) or `none` (False).
    KeyboardNavigation(BoolCondition),
}

impl AccessibilityCondition {
    /// Whether the condition holds for these preferences.
    #[must_use]
    pub fn matches(&self, preferences: &AccessibilityPreferences) -> bool {
        let wanted = |condition: BoolCondition, state: SettingState| {
            bool::from(condition) == state.is_on()
        };
        match self {
            Self::TextScaleAtLeast(thousandths) => {
                preferences.effective_text_scale().mul_add(1000.0, 0.5)
                    >= f32_of_thousandths(*thousandths)
            }
            Self::TextScaleAtMost(thousandths) => {
                preferences.effective_text_scale().mul_add(1000.0, -0.5)
                    <= f32_of_thousandths(*thousandths)
            }
            Self::BoldText(condition) => wanted(*condition, preferences.bold_text),
            Self::Contrast(ContrastPreference::More) => preferences.high_contrast(),
            Self::Contrast(ContrastPreference::Less) => preferences.contrast == ContrastPreference::Less,
            Self::Contrast(_) => !preferences.high_contrast() && preferences.contrast != ContrastPreference::Less,
            Self::ForcedColors(condition) => wanted(*condition, preferences.forced_colors),
            Self::ReducedTransparency(condition) => {
                wanted(*condition, preferences.reduced_transparency)
            }
            Self::InvertedColors(condition) => wanted(*condition, preferences.invert_colors),
            Self::DifferentiateWithoutColor(condition) => {
                wanted(*condition, preferences.differentiate_without_color)
            }
            Self::ButtonShapes(condition) => wanted(*condition, preferences.button_shapes),
            Self::OnOffLabels(condition) => wanted(*condition, preferences.on_off_labels),
            Self::ScreenReader(condition) => wanted(*condition, preferences.screen_reader),
            Self::CrossFade(condition) => wanted(*condition, preferences.cross_fade_transitions),
            Self::KeyboardNavigation(condition) => {
                wanted(*condition, preferences.keyboard_navigation)
            }
        }
    }

    /// Reads one `@media` feature (`key: value`, the key without its parentheses) that is one of
    /// these conditions; `None` for any other feature or a value it does not take. The reduced
    /// motion feature and `prefers-contrast: more` are not here (their own selectors).
    #[must_use]
    pub fn from_media_feature(key: &str, value: &str) -> Option<Self> {
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim();
        // The words of a feature that is either "on" or "none".
        let flag = |on: &[&str], off: &[&str]| -> Option<BoolCondition> {
            let lower = value.to_ascii_lowercase();
            if on.contains(&lower.as_str()) {
                Some(BoolCondition::True)
            } else if off.contains(&lower.as_str()) {
                Some(BoolCondition::False)
            } else {
                None
            }
        };
        let active = (&["active", "on", "true", "1"][..], &["none", "off", "false", "0"][..]);
        let reduce = (
            &["reduce", "on", "true", "1"][..],
            &["no-preference", "none", "off", "false", "0"][..],
        );
        match key.as_str() {
            "min-text-scale" | "max-text-scale" => {
                let factor: f32 = value.parse().ok()?;
                if !factor.is_finite() || factor < 0.0 {
                    return None;
                }
                let thousandths = thousandths_of_f32(factor);
                Some(if key == "min-text-scale" {
                    Self::TextScaleAtLeast(thousandths)
                } else {
                    Self::TextScaleAtMost(thousandths)
                })
            }
            "prefers-bold-text" | "bold-text" => flag(
                &["bold", "more", "active", "on", "true", "1"],
                &["no-preference", "none", "off", "false", "0"],
            )
            .map(Self::BoldText),
            "prefers-contrast" => match ContrastPreference::parse(value)? {
                ContrastPreference::Less => Some(Self::Contrast(ContrastPreference::Less)),
                ContrastPreference::NoPreference => {
                    Some(Self::Contrast(ContrastPreference::NoPreference))
                }
                // `more` is `DynamicSelector::PrefersHighContrast`; `custom` is a contrast theme
                ContrastPreference::More | ContrastPreference::Unknown => {
                    if value.eq_ignore_ascii_case("custom") {
                        Some(Self::ForcedColors(BoolCondition::True))
                    } else {
                        None
                    }
                }
            },
            "forced-colors" => flag(active.0, active.1).map(Self::ForcedColors),
            "prefers-reduced-transparency" => {
                flag(reduce.0, reduce.1).map(Self::ReducedTransparency)
            }
            "inverted-colors" => {
                flag(&["inverted", "active", "on", "true", "1"], active.1).map(Self::InvertedColors)
            }
            "differentiate-without-color" | "prefers-differentiate-without-color" => {
                flag(active.0, active.1).map(Self::DifferentiateWithoutColor)
            }
            "button-shapes" => flag(active.0, active.1).map(Self::ButtonShapes),
            "on-off-labels" => flag(active.0, active.1).map(Self::OnOffLabels),
            "screen-reader" => flag(active.0, active.1).map(Self::ScreenReader),
            "prefers-cross-fade" => flag(reduce.0, reduce.1).map(Self::CrossFade),
            "keyboard-navigation" => flag(active.0, active.1).map(Self::KeyboardNavigation),
            _ => None,
        }
    }
}

/// A text scale in thousandths, rounded; the table the conditions keep it in.
fn thousandths_of_f32(factor: f32) -> u32 {
    let scaled = (factor * 1000.0).round();
    if scaled >= 1_000_000.0 {
        1_000_000
    } else {
        // In 0..1_000_000 (a finite, non-negative factor under 1000): the cast cannot truncate.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let thousandths = scaled as u32;
        thousandths
    }
}

/// The thousandths of a condition as a float, for the comparison.
const fn f32_of_thousandths(thousandths: u32) -> f32 {
    // A condition holds at most 1_000_000 (see `thousandths_of_f32`): exact in an f32.
    #[allow(clippy::cast_precision_loss)]
    let value = thousandths as f32;
    value
}

#[cfg(test)]
#[path = "accessibility_test.rs"]
mod accessibility_test;
