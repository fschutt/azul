//! CSS properties for styling scrollbars.

use alloc::string::{String, ToString};
use crate::corety::AzString;

use crate::props::{
    basic::{
        color::{parse_css_color, ColorU, CssColorParseError, CssColorParseErrorOwned},
    },
    formatter::PrintAsCssValue,
    layout::{
        dimensions::LayoutWidth,
        spacing::{LayoutPaddingLeft, LayoutPaddingRight},
    },
    style::background::StyleBackgroundContent,
};

// ============================================================================
// CSS Standard Scroll Behavior Properties
// ============================================================================

/// CSS `scroll-behavior` property - controls smooth scrolling
/// <https://developer.mozilla.org/en-US/docs/Web/CSS/scroll-behavior>
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(C)]
pub enum ScrollBehavior {
    /// Scrolling jumps instantly to the final position
    #[default]
    Auto,
    /// Scrolling animates smoothly to the final position
    Smooth,
}

impl PrintAsCssValue for ScrollBehavior {
    fn print_as_css_value(&self) -> String {
        match self {
            Self::Auto => "auto".to_string(),
            Self::Smooth => "smooth".to_string(),
        }
    }
}

/// CSS `overscroll-behavior` property - controls overscroll effects
/// <https://developer.mozilla.org/en-US/docs/Web/CSS/overscroll-behavior>
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(C)]
pub enum OverscrollBehavior {
    /// Default scroll overflow behavior (bounce/glow effects, scroll chaining)
    #[default]
    Auto,
    /// Prevents scroll chaining to parent elements, but allows local overscroll effects
    Contain,
    /// No scroll chaining and no overscroll effects (hard stop at boundaries)
    None,
}

impl PrintAsCssValue for OverscrollBehavior {
    fn print_as_css_value(&self) -> String {
        match self {
            Self::Auto => "auto".to_string(),
            Self::Contain => "contain".to_string(),
            Self::None => "none".to_string(),
        }
    }
}

// ============================================================================
// Extended Scroll Configuration (Azul-specific)
// ============================================================================

/// Scroll physics configuration for momentum scrolling
///
/// This controls how scrolling feels - the "weight" and "friction" of the scroll.
/// Different platforms have different scroll physics (iOS vs Android vs Windows).
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct ScrollPhysics {
    /// Smooth scroll animation duration in milliseconds (default: 300ms)
    /// Only used when scroll-behavior: smooth
    pub smooth_scroll_duration_ms: u32,

    /// Deceleration rate for momentum scrolling (0.0 = instant stop, 1.0 = never stops)
    /// Typical values: 0.95 (fast deceleration) to 0.998 (slow, iOS-like)
    /// Default: 0.95
    pub deceleration_rate: f32,

    /// Minimum velocity threshold to start momentum scrolling (pixels/second)
    /// Below this, scrolling stops immediately. Default: 50.0
    pub min_velocity_threshold: f32,

    /// Maximum scroll velocity (pixels/second). Default: 8000.0
    pub max_velocity: f32,

    /// Scroll wheel multiplier. Default: 1.0
    /// Values > 1.0 make scrolling faster, < 1.0 slower
    pub wheel_multiplier: f32,

    /// Whether to invert scroll direction (natural scrolling). Default: false
    pub invert_direction: bool,

    /// Overscroll elasticity (0.0 = no bounce, 1.0 = full bounce like iOS)
    /// Only applies when overscroll-behavior: auto. Default: 0.0 (no bounce)
    pub overscroll_elasticity: f32,

    /// Maximum overscroll distance in pixels before rubber-banding stops
    /// Default: 100.0
    pub max_overscroll_distance: f32,

    /// Bounce-back duration when releasing overscroll (milliseconds)
    /// Default: 400
    pub bounce_back_duration_ms: u32,

    /// Timer tick interval in milliseconds for the scroll physics timer.
    /// Should match the monitor refresh rate (e.g. 16ms for 60Hz, 8ms for 120Hz).
    /// Default: 16 (60 Hz)
    pub timer_interval_ms: u32,
}

impl Default for ScrollPhysics {
    fn default() -> Self {
        Self {
            smooth_scroll_duration_ms: 300,
            deceleration_rate: 0.95,
            min_velocity_threshold: 50.0,
            max_velocity: 8000.0,
            wheel_multiplier: 1.0,
            invert_direction: false,
            overscroll_elasticity: 0.0, // No bounce by default (Windows-like)
            max_overscroll_distance: 100.0,
            bounce_back_duration_ms: 400,
            timer_interval_ms: 16,
        }
    }
}

impl ScrollPhysics {
    /// iOS-like scroll physics with momentum and bounce
    #[must_use] pub const fn ios() -> Self {
        Self {
            smooth_scroll_duration_ms: 300,
            deceleration_rate: 0.998,
            min_velocity_threshold: 20.0,
            max_velocity: 8000.0,
            wheel_multiplier: 1.0,
            invert_direction: true, // Natural scrolling
            overscroll_elasticity: 0.5,
            max_overscroll_distance: 120.0,
            bounce_back_duration_ms: 500,
            timer_interval_ms: 16,
        }
    }

    /// macOS-like scroll physics
    #[must_use] pub const fn macos() -> Self {
        Self {
            smooth_scroll_duration_ms: 250,
            deceleration_rate: 0.997,
            min_velocity_threshold: 30.0,
            max_velocity: 6000.0,
            wheel_multiplier: 1.0,
            invert_direction: true, // Natural scrolling by default
            overscroll_elasticity: 0.3,
            max_overscroll_distance: 80.0,
            bounce_back_duration_ms: 400,
            timer_interval_ms: 16,
        }
    }

    /// Windows-like scroll physics (no momentum, no bounce)
    #[must_use] pub const fn windows() -> Self {
        Self {
            smooth_scroll_duration_ms: 200,
            deceleration_rate: 0.9,
            min_velocity_threshold: 100.0,
            max_velocity: 4000.0,
            wheel_multiplier: 1.0,
            invert_direction: false,
            overscroll_elasticity: 0.0,
            max_overscroll_distance: 0.0,
            bounce_back_duration_ms: 200,
            timer_interval_ms: 16,
        }
    }

    /// Android-like scroll physics
    #[must_use] pub const fn android() -> Self {
        Self {
            smooth_scroll_duration_ms: 250,
            deceleration_rate: 0.996,
            min_velocity_threshold: 40.0,
            max_velocity: 8000.0,
            wheel_multiplier: 1.0,
            invert_direction: false,
            overscroll_elasticity: 0.2, // Subtle glow effect
            max_overscroll_distance: 60.0,
            bounce_back_duration_ms: 300,
            timer_interval_ms: 16,
        }
    }
}

// ============================================================================
// Scrollbar Visibility Mode (CSS: -azul-scrollbar-visibility)
// ============================================================================

/// Controls when the scrollbar is displayed.
///
/// This is a per-element CSS property (`-azul-scrollbar-visibility`) that
/// determines the scrollbar presentation style. It interacts with the
/// OS-level `ScrollbarPreferences.visibility` (from System Preferences)
/// when set to `Auto`.
///
/// - `Always`: Classic, always-visible scrollbar (Chrome/Windows/Linux default).
///   Scrollbar reserves layout space.
/// - `WhenScrolling`: Overlay scrollbar that fades in on scroll activity
///   and fades out after a delay. Does not reserve layout space.
/// - `Auto`: Use the OS preference. On macOS this typically means `WhenScrolling`,
///   on Windows/Linux this typically means `Always`.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(C)]
pub enum ScrollbarVisibilityMode {
    /// Scrollbar is always visible (Chrome/Windows/Linux default).
    /// Reserves layout space.
    #[default]
    Always,
    /// Scrollbar appears on scroll and fades out after inactivity.
    /// Does not reserve layout space (overlay).
    WhenScrolling,
    /// Use the OS-level scrollbar preference.
    Auto,
}

impl PrintAsCssValue for ScrollbarVisibilityMode {
    fn print_as_css_value(&self) -> String {
        match self {
            Self::Always => "always".to_string(),
            Self::WhenScrolling => "when-scrolling".to_string(),
            Self::Auto => "auto".to_string(),
        }
    }
}

// ============================================================================
// Scrollbar Fade Delay (CSS: -azul-scrollbar-fade-delay)
// ============================================================================

/// Time in milliseconds before the overlay scrollbar starts fading out.
///
/// A value of 0 means the scrollbar never fades (always visible).
/// Typical values: 500ms (macOS), 0ms (Windows).
///
/// CSS syntax: `-azul-scrollbar-fade-delay: 500ms;` or `-azul-scrollbar-fade-delay: 0;`
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(C)]
pub struct ScrollbarFadeDelay {
    /// Delay in milliseconds
    pub ms: u32,
}

impl ScrollbarFadeDelay {
    #[must_use] pub const fn new(ms: u32) -> Self { Self { ms } }
    pub const ZERO: Self = Self { ms: 0 };
}

impl PrintAsCssValue for ScrollbarFadeDelay {
    fn print_as_css_value(&self) -> String {
        if self.ms == 0 { "0".to_string() } else { format!("{}ms", self.ms) }
    }
}

// ============================================================================
// Scrollbar Fade Duration (CSS: -azul-scrollbar-fade-duration)
// ============================================================================

/// Duration in milliseconds of the scrollbar fade-out animation.
///
/// A value of 0 means instant disappearance (no animation).
/// Typical values: 200ms (macOS), 0ms (Windows).
///
/// CSS syntax: `-azul-scrollbar-fade-duration: 200ms;` or `-azul-scrollbar-fade-duration: 0;`
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(C)]
pub struct ScrollbarFadeDuration {
    /// Duration in milliseconds
    pub ms: u32,
}

impl ScrollbarFadeDuration {
    #[must_use] pub const fn new(ms: u32) -> Self { Self { ms } }
    pub const ZERO: Self = Self { ms: 0 };
}

impl PrintAsCssValue for ScrollbarFadeDuration {
    fn print_as_css_value(&self) -> String {
        if self.ms == 0 { "0".to_string() } else { format!("{}ms", self.ms) }
    }
}

// ============================================================================
// Per-node Overflow Scrolling Mode (CSS: -azul-overflow-scrolling)
// ============================================================================

/// Controls per-node rubber-banding / momentum scrolling behavior.
///
/// Analogous to `-webkit-overflow-scrolling` on iOS Safari.
///
/// - `Auto`: Use the global `ScrollPhysics` from `SystemStyle`. On platforms
///   with `overscroll_elasticity == 0.0` (e.g. Windows), this means no rubber-banding.
/// - `Touch`: Force momentum scrolling with rubber-banding on this node,
///   regardless of the global `ScrollPhysics` setting. Uses iOS-like elasticity
///   if the global elasticity is zero.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(C)]
pub enum OverflowScrolling {
    /// Use the global scroll physics (platform default). No rubber-banding on Windows.
    #[default]
    Auto,
    /// Force rubber-banding / momentum scrolling on this node (like iOS/macOS).
    Touch,
}

impl PrintAsCssValue for OverflowScrolling {
    fn print_as_css_value(&self) -> String {
        match self {
            Self::Auto => "auto".to_string(),
            Self::Touch => "touch".to_string(),
        }
    }
}

// ============================================================================
// Standard Properties
// ============================================================================

/// Represents the standard `scrollbar-width` property.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
#[derive(Default)]
pub enum LayoutScrollbarWidth {
    #[default]
    Auto,
    Thin,
    None,
}


impl PrintAsCssValue for LayoutScrollbarWidth {
    fn print_as_css_value(&self) -> String {
        match self {
            Self::Auto => "auto".to_string(),
            Self::Thin => "thin".to_string(),
            Self::None => "none".to_string(),
        }
    }
}

/// Wrapper struct for custom scrollbar colors (thumb and track)
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct ScrollbarColorCustom {
    pub thumb: ColorU,
    pub track: ColorU,
}

/// Represents the standard `scrollbar-color` property.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
#[derive(Default)]
pub enum StyleScrollbarColor {
    #[default]
    Auto,
    Custom(ScrollbarColorCustom),
}


impl PrintAsCssValue for StyleScrollbarColor {
    fn print_as_css_value(&self) -> String {
        match self {
            Self::Auto => "auto".to_string(),
            Self::Custom(c) => format!("{} {}", c.thumb.to_hash(), c.track.to_hash()),
        }
    }
}

// -- -webkit-prefixed Properties --

/// Holds info necessary for layouting / styling -webkit-scrollbar properties.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct ScrollbarInfo {
    /// Total width (or height for vertical scrollbars) of the scrollbar in pixels
    pub width: LayoutWidth,
    /// Padding of the scrollbar tracker, in pixels. The inner bar is `width - padding` pixels
    /// wide.
    pub padding_left: LayoutPaddingLeft,
    /// Padding of the scrollbar (right)
    pub padding_right: LayoutPaddingRight,
    /// Style of the scrollbar background
    /// (`-webkit-scrollbar` / `-webkit-scrollbar-track` / `-webkit-scrollbar-track-piece`
    /// combined)
    pub track: StyleBackgroundContent,
    /// Style of the scrollbar thumbs (the "up" / "down" arrows), (`-webkit-scrollbar-thumb`)
    pub thumb: StyleBackgroundContent,
    /// Styles the directional buttons on the scrollbar (`-webkit-scrollbar-button`)
    pub button: StyleBackgroundContent,
    /// If two scrollbars are present, addresses the (usually) bottom corner
    /// of the scrollable element, where two scrollbars might meet (`-webkit-scrollbar-corner`)
    pub corner: StyleBackgroundContent,
    /// Addresses the draggable resizing handle that appears above the
    /// `corner` at the bottom corner of some elements (`-webkit-resizer`)
    pub resizer: StyleBackgroundContent,
    /// Whether to clip the scrollbar to the container's border-radius.
    /// When true, if the container has rounded corners, the scrollbar will be
    /// clipped to those rounded corners instead of having rectangular edges.
    /// Default is false for classic scrollbars, true for overlay scrollbars.
    pub clip_to_container_border: bool,
    /// Scroll behavior for this scrollbar's container (auto or smooth)
    pub scroll_behavior: ScrollBehavior,
    /// Overscroll behavior for the X axis
    pub overscroll_behavior_x: OverscrollBehavior,
    /// Overscroll behavior for the Y axis  
    pub overscroll_behavior_y: OverscrollBehavior,
    /// Per-node overflow scrolling mode (`-azul-overflow-scrolling: auto | touch`)
    /// `Touch` forces rubber-banding on this node even when the global physics has no bounce.
    pub overflow_scrolling: OverflowScrolling,
}

impl Default for ScrollbarInfo {
    fn default() -> Self {
        SCROLLBAR_CLASSIC_LIGHT
    }
}

impl PrintAsCssValue for ScrollbarInfo {
    fn print_as_css_value(&self) -> String {
        // This is a custom format, not standard CSS
        format!(
            "width: {}; padding-left: {}; padding-right: {}; track: {}; thumb: {}; button: {}; \
             corner: {}; resizer: {}",
            self.width.print_as_css_value(),
            self.padding_left.print_as_css_value(),
            self.padding_right.print_as_css_value(),
            self.track.print_as_css_value(),
            self.thumb.print_as_css_value(),
            self.button.print_as_css_value(),
            self.corner.print_as_css_value(),
            self.resizer.print_as_css_value(),
        )
    }
}

/// Scrollbar style for both horizontal and vertical scrollbars.
#[derive(Debug, Default, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct ScrollbarStyle {
    /// Horizontal scrollbar style, if any
    pub horizontal: ScrollbarInfo,
    /// Vertical scrollbar style, if any
    pub vertical: ScrollbarInfo,
}

impl PrintAsCssValue for ScrollbarStyle {
    fn print_as_css_value(&self) -> String {
        // This is a custom format, not standard CSS
        format!(
            "horz({}), vert({})",
            self.horizontal.print_as_css_value(),
            self.vertical.print_as_css_value()
        )
    }
}

// Formatting to Rust code
impl crate::codegen::format::FormatAsRustCode for ScrollbarStyle {
    fn format_as_rust_code(&self, tabs: usize) -> String {
        let t = String::from("    ").repeat(tabs);
        let t1 = String::from("    ").repeat(tabs + 1);
        format!(
            "ScrollbarStyle {{\r\n{}horizontal: {},\r\n{}vertical: {},\r\n{}}}",
            t1,
            crate::codegen::format::format_scrollbar_info(&self.horizontal, tabs + 1),
            t1,
            crate::codegen::format::format_scrollbar_info(&self.vertical, tabs + 1),
            t,
        )
    }
}

impl crate::codegen::format::FormatAsRustCode for LayoutScrollbarWidth {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        match self {
            Self::Auto => String::from("LayoutScrollbarWidth::Auto"),
            Self::Thin => String::from("LayoutScrollbarWidth::Thin"),
            Self::None => String::from("LayoutScrollbarWidth::None"),
        }
    }
}

impl crate::codegen::format::FormatAsRustCode for StyleScrollbarColor {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        match self {
            Self::Auto => String::from("StyleScrollbarColor::Auto"),
            Self::Custom(c) => format!(
                "StyleScrollbarColor::Custom(ScrollbarColorCustom {{ thumb: {}, track: {} }})",
                crate::codegen::format::format_color_value(&c.thumb),
                crate::codegen::format::format_color_value(&c.track)
            ),
        }
    }
}

impl crate::codegen::format::FormatAsRustCode for ScrollbarVisibilityMode {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        match self {
            Self::Always => String::from("ScrollbarVisibilityMode::Always"),
            Self::WhenScrolling => String::from("ScrollbarVisibilityMode::WhenScrolling"),
            Self::Auto => String::from("ScrollbarVisibilityMode::Auto"),
        }
    }
}

impl crate::codegen::format::FormatAsRustCode for ScrollbarFadeDelay {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!("ScrollbarFadeDelay::new({})", self.ms)
    }
}

impl crate::codegen::format::FormatAsRustCode for ScrollbarFadeDuration {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!("ScrollbarFadeDuration::new({})", self.ms)
    }
}

// --- Final Computed Style ---

/// The final, resolved style for a scrollbar, after considering both
/// standard and -webkit- properties. This struct is intended for use by the layout engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComputedScrollbarStyle {
    /// The width of the scrollbar. `None` signifies `scrollbar-width: none`.
    pub width: Option<LayoutWidth>,
    /// The color of the scrollbar thumb. `None` means use UA default.
    pub thumb_color: Option<ColorU>,
    /// The color of the scrollbar track. `None` means use UA default.
    pub track_color: Option<ColorU>,
}

impl Default for ComputedScrollbarStyle {
    fn default() -> Self {
        let default_info = ScrollbarInfo::default();
        Self {
            width: Some(default_info.width), // Default width from UA/platform
            thumb_color: match default_info.thumb {
                StyleBackgroundContent::Color(c) => Some(c),
                _ => None,
            },
            track_color: match default_info.track {
                StyleBackgroundContent::Color(c) => Some(c),
                _ => None,
            },
        }
    }
}

// --- Default Style Constants ---

/// A classic light-themed scrollbar, similar to older Windows versions.
pub const SCROLLBAR_CLASSIC_LIGHT: ScrollbarInfo = ScrollbarInfo {
    width: LayoutWidth::Px(crate::props::basic::pixel::PixelValue::const_px(17)),
    padding_left: LayoutPaddingLeft {
        inner: crate::props::basic::pixel::PixelValue::const_px(2),
    },
    padding_right: LayoutPaddingRight {
        inner: crate::props::basic::pixel::PixelValue::const_px(2),
    },
    track: StyleBackgroundContent::Color(ColorU {
        r: 241,
        g: 241,
        b: 241,
        a: 255,
    }),
    thumb: StyleBackgroundContent::Color(ColorU {
        r: 193,
        g: 193,
        b: 193,
        a: 255,
    }),
    button: StyleBackgroundContent::Color(ColorU {
        r: 163,
        g: 163,
        b: 163,
        a: 255,
    }),
    corner: StyleBackgroundContent::Color(ColorU {
        r: 241,
        g: 241,
        b: 241,
        a: 255,
    }),
    resizer: StyleBackgroundContent::Color(ColorU {
        r: 241,
        g: 241,
        b: 241,
        a: 255,
    }),
    clip_to_container_border: false,
    scroll_behavior: ScrollBehavior::Auto,
    overscroll_behavior_x: OverscrollBehavior::Auto,
    overscroll_behavior_y: OverscrollBehavior::Auto,
    overflow_scrolling: OverflowScrolling::Auto,
};

/// A classic dark-themed scrollbar.
pub const SCROLLBAR_CLASSIC_DARK: ScrollbarInfo = ScrollbarInfo {
    width: LayoutWidth::Px(crate::props::basic::pixel::PixelValue::const_px(17)),
    padding_left: LayoutPaddingLeft {
        inner: crate::props::basic::pixel::PixelValue::const_px(2),
    },
    padding_right: LayoutPaddingRight {
        inner: crate::props::basic::pixel::PixelValue::const_px(2),
    },
    track: StyleBackgroundContent::Color(ColorU {
        r: 45,
        g: 45,
        b: 45,
        a: 255,
    }),
    thumb: StyleBackgroundContent::Color(ColorU {
        r: 100,
        g: 100,
        b: 100,
        a: 255,
    }),
    button: StyleBackgroundContent::Color(ColorU {
        r: 120,
        g: 120,
        b: 120,
        a: 255,
    }),
    corner: StyleBackgroundContent::Color(ColorU {
        r: 45,
        g: 45,
        b: 45,
        a: 255,
    }),
    resizer: StyleBackgroundContent::Color(ColorU {
        r: 45,
        g: 45,
        b: 45,
        a: 255,
    }),
    clip_to_container_border: false,
    scroll_behavior: ScrollBehavior::Auto,
    overscroll_behavior_x: OverscrollBehavior::Auto,
    overscroll_behavior_y: OverscrollBehavior::Auto,
    overflow_scrolling: OverflowScrolling::Auto,
};

/// A modern, thin, overlay scrollbar inspired by macOS (Light Theme).
pub const SCROLLBAR_MACOS_LIGHT: ScrollbarInfo = ScrollbarInfo {
    width: LayoutWidth::Px(crate::props::basic::pixel::PixelValue::const_px(8)),
    padding_left: LayoutPaddingLeft {
        inner: crate::props::basic::pixel::PixelValue::const_px(0),
    },
    padding_right: LayoutPaddingRight {
        inner: crate::props::basic::pixel::PixelValue::const_px(0),
    },
    track: StyleBackgroundContent::Color(ColorU::TRANSPARENT),
    thumb: StyleBackgroundContent::Color(ColorU {
        r: 0,
        g: 0,
        b: 0,
        a: 100,
    }), // semi-transparent black
    button: StyleBackgroundContent::Color(ColorU::TRANSPARENT),
    corner: StyleBackgroundContent::Color(ColorU::TRANSPARENT),
    resizer: StyleBackgroundContent::Color(ColorU::TRANSPARENT),
    clip_to_container_border: true, // Overlay scrollbars should clip to rounded borders
    scroll_behavior: ScrollBehavior::Smooth,
    overscroll_behavior_x: OverscrollBehavior::Auto,
    overscroll_behavior_y: OverscrollBehavior::Auto,
    overflow_scrolling: OverflowScrolling::Auto,
};

/// A modern, thin, overlay scrollbar inspired by macOS (Dark Theme).
pub const SCROLLBAR_MACOS_DARK: ScrollbarInfo = ScrollbarInfo {
    width: LayoutWidth::Px(crate::props::basic::pixel::PixelValue::const_px(8)),
    padding_left: LayoutPaddingLeft {
        inner: crate::props::basic::pixel::PixelValue::const_px(0),
    },
    padding_right: LayoutPaddingRight {
        inner: crate::props::basic::pixel::PixelValue::const_px(0),
    },
    track: StyleBackgroundContent::Color(ColorU::TRANSPARENT),
    thumb: StyleBackgroundContent::Color(ColorU {
        r: 255,
        g: 255,
        b: 255,
        a: 100,
    }), // semi-transparent white
    button: StyleBackgroundContent::Color(ColorU::TRANSPARENT),
    corner: StyleBackgroundContent::Color(ColorU::TRANSPARENT),
    resizer: StyleBackgroundContent::Color(ColorU::TRANSPARENT),
    clip_to_container_border: true, // Overlay scrollbars should clip to rounded borders
    scroll_behavior: ScrollBehavior::Smooth,
    overscroll_behavior_x: OverscrollBehavior::Auto,
    overscroll_behavior_y: OverscrollBehavior::Auto,
    overflow_scrolling: OverflowScrolling::Auto,
};

/// A modern scrollbar inspired by Windows 11 (Light Theme).
pub const SCROLLBAR_WINDOWS_LIGHT: ScrollbarInfo = ScrollbarInfo {
    width: LayoutWidth::Px(crate::props::basic::pixel::PixelValue::const_px(12)),
    padding_left: LayoutPaddingLeft {
        inner: crate::props::basic::pixel::PixelValue::const_px(0),
    },
    padding_right: LayoutPaddingRight {
        inner: crate::props::basic::pixel::PixelValue::const_px(0),
    },
    track: StyleBackgroundContent::Color(ColorU {
        r: 241,
        g: 241,
        b: 241,
        a: 255,
    }),
    thumb: StyleBackgroundContent::Color(ColorU {
        r: 130,
        g: 130,
        b: 130,
        a: 255,
    }),
    button: StyleBackgroundContent::Color(ColorU::TRANSPARENT),
    corner: StyleBackgroundContent::Color(ColorU::TRANSPARENT),
    resizer: StyleBackgroundContent::Color(ColorU::TRANSPARENT),
    clip_to_container_border: false,
    scroll_behavior: ScrollBehavior::Auto,
    overscroll_behavior_x: OverscrollBehavior::None,
    overscroll_behavior_y: OverscrollBehavior::None,
    overflow_scrolling: OverflowScrolling::Auto,
};

/// A modern scrollbar inspired by Windows 11 (Dark Theme).
pub const SCROLLBAR_WINDOWS_DARK: ScrollbarInfo = ScrollbarInfo {
    width: LayoutWidth::Px(crate::props::basic::pixel::PixelValue::const_px(12)),
    padding_left: LayoutPaddingLeft {
        inner: crate::props::basic::pixel::PixelValue::const_px(0),
    },
    padding_right: LayoutPaddingRight {
        inner: crate::props::basic::pixel::PixelValue::const_px(0),
    },
    track: StyleBackgroundContent::Color(ColorU {
        r: 32,
        g: 32,
        b: 32,
        a: 255,
    }),
    thumb: StyleBackgroundContent::Color(ColorU {
        r: 110,
        g: 110,
        b: 110,
        a: 255,
    }),
    button: StyleBackgroundContent::Color(ColorU::TRANSPARENT),
    corner: StyleBackgroundContent::Color(ColorU::TRANSPARENT),
    resizer: StyleBackgroundContent::Color(ColorU::TRANSPARENT),
    clip_to_container_border: false,
    scroll_behavior: ScrollBehavior::Auto,
    overscroll_behavior_x: OverscrollBehavior::None,
    overscroll_behavior_y: OverscrollBehavior::None,
    overflow_scrolling: OverflowScrolling::Auto,
};

/// A modern, thin, overlay scrollbar inspired by iOS (Light Theme).
pub const SCROLLBAR_IOS_LIGHT: ScrollbarInfo = ScrollbarInfo {
    width: LayoutWidth::Px(crate::props::basic::pixel::PixelValue::const_px(7)),
    padding_left: LayoutPaddingLeft {
        inner: crate::props::basic::pixel::PixelValue::const_px(0),
    },
    padding_right: LayoutPaddingRight {
        inner: crate::props::basic::pixel::PixelValue::const_px(0),
    },
    track: StyleBackgroundContent::Color(ColorU::TRANSPARENT),
    thumb: StyleBackgroundContent::Color(ColorU {
        r: 0,
        g: 0,
        b: 0,
        a: 102,
    }), // rgba(0,0,0,0.4)
    button: StyleBackgroundContent::Color(ColorU::TRANSPARENT),
    corner: StyleBackgroundContent::Color(ColorU::TRANSPARENT),
    resizer: StyleBackgroundContent::Color(ColorU::TRANSPARENT),
    clip_to_container_border: true, // Overlay scrollbars should clip to rounded borders
    scroll_behavior: ScrollBehavior::Smooth,
    overscroll_behavior_x: OverscrollBehavior::Auto,
    overscroll_behavior_y: OverscrollBehavior::Auto,
    overflow_scrolling: OverflowScrolling::Auto,
};

/// A modern, thin, overlay scrollbar inspired by iOS (Dark Theme).
pub const SCROLLBAR_IOS_DARK: ScrollbarInfo = ScrollbarInfo {
    width: LayoutWidth::Px(crate::props::basic::pixel::PixelValue::const_px(7)),
    padding_left: LayoutPaddingLeft {
        inner: crate::props::basic::pixel::PixelValue::const_px(0),
    },
    padding_right: LayoutPaddingRight {
        inner: crate::props::basic::pixel::PixelValue::const_px(0),
    },
    track: StyleBackgroundContent::Color(ColorU::TRANSPARENT),
    thumb: StyleBackgroundContent::Color(ColorU {
        r: 255,
        g: 255,
        b: 255,
        a: 102,
    }), // rgba(255,255,255,0.4)
    button: StyleBackgroundContent::Color(ColorU::TRANSPARENT),
    corner: StyleBackgroundContent::Color(ColorU::TRANSPARENT),
    resizer: StyleBackgroundContent::Color(ColorU::TRANSPARENT),
    clip_to_container_border: true, // Overlay scrollbars should clip to rounded borders
    scroll_behavior: ScrollBehavior::Smooth,
    overscroll_behavior_x: OverscrollBehavior::Auto,
    overscroll_behavior_y: OverscrollBehavior::Auto,
    overflow_scrolling: OverflowScrolling::Auto,
};

/// A modern, thin, overlay scrollbar inspired by Android (Light Theme).
pub const SCROLLBAR_ANDROID_LIGHT: ScrollbarInfo = ScrollbarInfo {
    width: LayoutWidth::Px(crate::props::basic::pixel::PixelValue::const_px(6)),
    padding_left: LayoutPaddingLeft {
        inner: crate::props::basic::pixel::PixelValue::const_px(0),
    },
    padding_right: LayoutPaddingRight {
        inner: crate::props::basic::pixel::PixelValue::const_px(0),
    },
    track: StyleBackgroundContent::Color(ColorU::TRANSPARENT),
    thumb: StyleBackgroundContent::Color(ColorU {
        r: 0,
        g: 0,
        b: 0,
        a: 102,
    }), // rgba(0,0,0,0.4)
    button: StyleBackgroundContent::Color(ColorU::TRANSPARENT),
    corner: StyleBackgroundContent::Color(ColorU::TRANSPARENT),
    resizer: StyleBackgroundContent::Color(ColorU::TRANSPARENT),
    clip_to_container_border: true, // Overlay scrollbars should clip to rounded borders
    scroll_behavior: ScrollBehavior::Smooth,
    overscroll_behavior_x: OverscrollBehavior::Contain,
    overscroll_behavior_y: OverscrollBehavior::Auto,
    overflow_scrolling: OverflowScrolling::Auto,
};

/// A modern, thin, overlay scrollbar inspired by Android (Dark Theme).
pub const SCROLLBAR_ANDROID_DARK: ScrollbarInfo = ScrollbarInfo {
    width: LayoutWidth::Px(crate::props::basic::pixel::PixelValue::const_px(6)),
    padding_left: LayoutPaddingLeft {
        inner: crate::props::basic::pixel::PixelValue::const_px(0),
    },
    padding_right: LayoutPaddingRight {
        inner: crate::props::basic::pixel::PixelValue::const_px(0),
    },
    track: StyleBackgroundContent::Color(ColorU::TRANSPARENT),
    thumb: StyleBackgroundContent::Color(ColorU {
        r: 255,
        g: 255,
        b: 255,
        a: 102,
    }), // rgba(255,255,255,0.4)
    button: StyleBackgroundContent::Color(ColorU::TRANSPARENT),
    corner: StyleBackgroundContent::Color(ColorU::TRANSPARENT),
    resizer: StyleBackgroundContent::Color(ColorU::TRANSPARENT),
    clip_to_container_border: true, // Overlay scrollbars should clip to rounded borders
    scroll_behavior: ScrollBehavior::Smooth,
    overscroll_behavior_x: OverscrollBehavior::Contain,
    overscroll_behavior_y: OverscrollBehavior::Auto,
    overflow_scrolling: OverflowScrolling::Auto,
};

// --- PARSERS ---

#[derive(Clone, PartialEq, Eq)]
pub enum LayoutScrollbarWidthParseError<'a> {
    InvalidValue(&'a str),
}
impl_debug_as_display!(LayoutScrollbarWidthParseError<'a>);
impl_display! { LayoutScrollbarWidthParseError<'a>, {
    InvalidValue(v) => format!("Invalid scrollbar-width value: \"{}\"", v),
}}

#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum LayoutScrollbarWidthParseErrorOwned {
    InvalidValue(AzString),
}
impl LayoutScrollbarWidthParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> LayoutScrollbarWidthParseErrorOwned {
        match self {
            Self::InvalidValue(s) => {
                LayoutScrollbarWidthParseErrorOwned::InvalidValue((*s).to_string().into())
            }
        }
    }
}
impl LayoutScrollbarWidthParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> LayoutScrollbarWidthParseError<'_> {
        match self {
            Self::InvalidValue(s) => LayoutScrollbarWidthParseError::InvalidValue(s.as_str()),
        }
    }
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `scrollbar-width` value.
pub fn parse_layout_scrollbar_width(
    input: &str,
) -> Result<LayoutScrollbarWidth, LayoutScrollbarWidthParseError<'_>> {
    match input.trim() {
        "auto" => Ok(LayoutScrollbarWidth::Auto),
        "thin" => Ok(LayoutScrollbarWidth::Thin),
        "none" => Ok(LayoutScrollbarWidth::None),
        _ => Err(LayoutScrollbarWidthParseError::InvalidValue(input)),
    }
}

#[derive(Clone, PartialEq)]
pub enum StyleScrollbarColorParseError<'a> {
    InvalidValue(&'a str),
    Color(CssColorParseError<'a>),
}
impl_debug_as_display!(StyleScrollbarColorParseError<'a>);
impl_display! { StyleScrollbarColorParseError<'a>, {
    InvalidValue(v) => format!("Invalid scrollbar-color value: \"{}\"", v),
    Color(e) => format!("Invalid color in scrollbar-color: {}", e),
}}
impl_from!(CssColorParseError<'a>, StyleScrollbarColorParseError::Color);

#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum StyleScrollbarColorParseErrorOwned {
    InvalidValue(AzString),
    Color(CssColorParseErrorOwned),
}
impl StyleScrollbarColorParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> StyleScrollbarColorParseErrorOwned {
        match self {
            Self::InvalidValue(s) => {
                StyleScrollbarColorParseErrorOwned::InvalidValue((*s).to_string().into())
            }
            Self::Color(e) => StyleScrollbarColorParseErrorOwned::Color(e.to_contained()),
        }
    }
}
impl StyleScrollbarColorParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> StyleScrollbarColorParseError<'_> {
        match self {
            Self::InvalidValue(s) => StyleScrollbarColorParseError::InvalidValue(s.as_str()),
            Self::Color(e) => StyleScrollbarColorParseError::Color(e.to_shared()),
        }
    }
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `scrollbar-color` value.
pub fn parse_style_scrollbar_color(
    input: &str,
) -> Result<StyleScrollbarColor, StyleScrollbarColorParseError<'_>> {
    let input = input.trim();
    if input == "auto" {
        return Ok(StyleScrollbarColor::Auto);
    }

    let mut parts = input.split_whitespace();
    let thumb_str = parts
        .next()
        .ok_or(StyleScrollbarColorParseError::InvalidValue(input))?;
    let track_str = parts
        .next()
        .ok_or(StyleScrollbarColorParseError::InvalidValue(input))?;

    if parts.next().is_some() {
        return Err(StyleScrollbarColorParseError::InvalidValue(input));
    }

    let thumb = parse_css_color(thumb_str)?;
    let track = parse_css_color(track_str)?;

    Ok(StyleScrollbarColor::Custom(ScrollbarColorCustom {
        thumb,
        track,
    }))
}

// --- Scrollbar Visibility Mode Parser ---

#[derive(Clone, PartialEq, Eq)]
pub enum ScrollbarVisibilityModeParseError<'a> {
    InvalidValue(&'a str),
}
impl_debug_as_display!(ScrollbarVisibilityModeParseError<'a>);
impl_display! { ScrollbarVisibilityModeParseError<'a>, {
    InvalidValue(v) => format!("Invalid scrollbar-visibility value: \"{}\"", v),
}}

#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum ScrollbarVisibilityModeParseErrorOwned {
    InvalidValue(AzString),
}
impl ScrollbarVisibilityModeParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> ScrollbarVisibilityModeParseErrorOwned {
        match self {
            Self::InvalidValue(s) => ScrollbarVisibilityModeParseErrorOwned::InvalidValue((*s).to_string().into()),
        }
    }
}
impl ScrollbarVisibilityModeParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> ScrollbarVisibilityModeParseError<'_> {
        match self {
            Self::InvalidValue(s) => ScrollbarVisibilityModeParseError::InvalidValue(s.as_str()),
        }
    }
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `scrollbar-visibility-mode` value.
pub fn parse_scrollbar_visibility_mode(
    input: &str,
) -> Result<ScrollbarVisibilityMode, ScrollbarVisibilityModeParseError<'_>> {
    match input.trim() {
        "always" => Ok(ScrollbarVisibilityMode::Always),
        "when-scrolling" => Ok(ScrollbarVisibilityMode::WhenScrolling),
        "auto" => Ok(ScrollbarVisibilityMode::Auto),
        _ => Err(ScrollbarVisibilityModeParseError::InvalidValue(input)),
    }
}

// --- Scrollbar Fade Delay Parser ---

#[derive(Clone, PartialEq, Eq)]
pub enum ScrollbarFadeDelayParseError<'a> {
    InvalidValue(&'a str),
}
impl_debug_as_display!(ScrollbarFadeDelayParseError<'a>);
impl_display! { ScrollbarFadeDelayParseError<'a>, {
    InvalidValue(v) => format!("Invalid scrollbar-fade-delay value: \"{}\"", v),
}}

#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum ScrollbarFadeDelayParseErrorOwned {
    InvalidValue(AzString),
}
impl ScrollbarFadeDelayParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> ScrollbarFadeDelayParseErrorOwned {
        match self {
            Self::InvalidValue(s) => ScrollbarFadeDelayParseErrorOwned::InvalidValue((*s).to_string().into()),
        }
    }
}
impl ScrollbarFadeDelayParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> ScrollbarFadeDelayParseError<'_> {
        match self {
            Self::InvalidValue(s) => ScrollbarFadeDelayParseError::InvalidValue(s.as_str()),
        }
    }
}

#[cfg(feature = "parser")]
fn parse_time_ms(input: &str) -> Option<u32> {
    crate::props::basic::time::parse_duration(input).ok().map(|d| d.inner)
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `scrollbar-fade-delay` value.
pub fn parse_scrollbar_fade_delay(
    input: &str,
) -> Result<ScrollbarFadeDelay, ScrollbarFadeDelayParseError<'_>> {
    parse_time_ms(input)
        .map(ScrollbarFadeDelay::new)
        .ok_or(ScrollbarFadeDelayParseError::InvalidValue(input))
}

// --- Scrollbar Fade Duration Parser ---

#[derive(Clone, PartialEq, Eq)]
pub enum ScrollbarFadeDurationParseError<'a> {
    InvalidValue(&'a str),
}
impl_debug_as_display!(ScrollbarFadeDurationParseError<'a>);
impl_display! { ScrollbarFadeDurationParseError<'a>, {
    InvalidValue(v) => format!("Invalid scrollbar-fade-duration value: \"{}\"", v),
}}

#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum ScrollbarFadeDurationParseErrorOwned {
    InvalidValue(AzString),
}
impl ScrollbarFadeDurationParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> ScrollbarFadeDurationParseErrorOwned {
        match self {
            Self::InvalidValue(s) => ScrollbarFadeDurationParseErrorOwned::InvalidValue((*s).to_string().into()),
        }
    }
}
impl ScrollbarFadeDurationParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> ScrollbarFadeDurationParseError<'_> {
        match self {
            Self::InvalidValue(s) => ScrollbarFadeDurationParseError::InvalidValue(s.as_str()),
        }
    }
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `scrollbar-fade-duration` value.
pub fn parse_scrollbar_fade_duration(
    input: &str,
) -> Result<ScrollbarFadeDuration, ScrollbarFadeDurationParseError<'_>> {
    parse_time_ms(input)
        .map(ScrollbarFadeDuration::new)
        .ok_or(ScrollbarFadeDurationParseError::InvalidValue(input))
}

#[cfg(all(test, feature = "parser"))]
mod tests {
    use super::*;
    use crate::props::basic::color::ColorU;

    #[test]
    fn test_parse_scrollbar_width() {
        assert_eq!(
            parse_layout_scrollbar_width("auto").unwrap(),
            LayoutScrollbarWidth::Auto
        );
        assert_eq!(
            parse_layout_scrollbar_width("thin").unwrap(),
            LayoutScrollbarWidth::Thin
        );
        assert_eq!(
            parse_layout_scrollbar_width("none").unwrap(),
            LayoutScrollbarWidth::None
        );
        assert!(parse_layout_scrollbar_width("thick").is_err());
    }

    #[test]
    fn test_parse_scrollbar_color() {
        assert_eq!(
            parse_style_scrollbar_color("auto").unwrap(),
            StyleScrollbarColor::Auto
        );

        let custom = parse_style_scrollbar_color("red blue").unwrap();
        assert_eq!(
            custom,
            StyleScrollbarColor::Custom(ScrollbarColorCustom {
                thumb: ColorU::RED,
                track: ColorU::BLUE
            })
        );

        let custom_hex = parse_style_scrollbar_color("#ff0000 #0000ff").unwrap();
        assert_eq!(
            custom_hex,
            StyleScrollbarColor::Custom(ScrollbarColorCustom {
                thumb: ColorU::RED,
                track: ColorU::BLUE
            })
        );

        assert!(parse_style_scrollbar_color("red").is_err());
        assert!(parse_style_scrollbar_color("red blue green").is_err());
    }
}
