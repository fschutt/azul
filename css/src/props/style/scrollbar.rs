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

#[cfg(test)]
#[allow(clippy::unreadable_literal, clippy::float_cmp)]
mod autotest_generated {
    use super::*;
    use crate::codegen::format::FormatAsRustCode;

    /// Largest integer an `f32` represents exactly (`2^24`). Every millisecond
    /// count at or below this survives the `f32` hop inside `parse_duration`;
    /// above it, neighbouring `f32`s are more than 1ms apart.
    #[cfg(feature = "parser")]
    const TWO_POW_24: u32 = 16_777_216;

    /// Inputs that must never parse as anything, whatever the property.
    #[cfg(feature = "parser")]
    const GARBAGE: &[&str] = &[
        "",
        " ",
        "   ",
        "\t\n",
        "\u{a0}",          // non-breaking space (trimmed away -> empty)
        "\0",
        "\u{1F600}",       // emoji
        "e\u{0301}",       // combining acute accent
        "\u{202e}auto",    // RTL override prefix
        "аuto",            // Cyrillic 'а' homoglyph
        "AUTO",
        "auto;",
        "auto garbage",
        "-1",
        "NaN",
        "inf",
        "0x10",
        "9223372036854775807", // i64::MAX
        "1e400",
        "{[(<",
    ];

    // ======================================================================
    // ScrollPhysics presets  (other)
    // ======================================================================

    /// Every preset must be usable as-is by a scroll animator: no NaN/inf can
    /// reach the physics integrator, the deceleration rate has to stay strictly
    /// below 1.0 (at 1.0 momentum never decays -> the scroll timer never stops),
    /// and the timer tick must be non-zero (a 0ms tick is a busy-loop).
    fn assert_physics_invariants(p: ScrollPhysics, name: &str) {
        for (field, v) in [
            ("deceleration_rate", p.deceleration_rate),
            ("min_velocity_threshold", p.min_velocity_threshold),
            ("max_velocity", p.max_velocity),
            ("wheel_multiplier", p.wheel_multiplier),
            ("overscroll_elasticity", p.overscroll_elasticity),
            ("max_overscroll_distance", p.max_overscroll_distance),
        ] {
            assert!(v.is_finite(), "{name}.{field} is not finite: {v}");
            assert!(!v.is_nan(), "{name}.{field} is NaN");
            assert!(v >= 0.0, "{name}.{field} is negative: {v}");
        }

        assert!(
            p.deceleration_rate > 0.0 && p.deceleration_rate < 1.0,
            "{name}.deceleration_rate must stay inside (0.0, 1.0) or momentum never stops: {}",
            p.deceleration_rate
        );
        assert!(
            (0.0..=1.0).contains(&p.overscroll_elasticity),
            "{name}.overscroll_elasticity out of [0.0, 1.0]: {}",
            p.overscroll_elasticity
        );
        assert!(
            p.max_velocity > p.min_velocity_threshold,
            "{name}: max_velocity ({}) must exceed min_velocity_threshold ({})",
            p.max_velocity,
            p.min_velocity_threshold
        );
        assert!(
            p.wheel_multiplier > 0.0,
            "{name}.wheel_multiplier must be > 0 or the wheel does nothing"
        );
        assert!(
            p.timer_interval_ms > 0,
            "{name}.timer_interval_ms == 0 would spin the physics timer"
        );
        assert!(
            p.smooth_scroll_duration_ms > 0,
            "{name}.smooth_scroll_duration_ms == 0 makes `scroll-behavior: smooth` a no-op"
        );
    }

    #[test]
    fn scroll_physics_presets_hold_their_invariants() {
        assert_physics_invariants(ScrollPhysics::default(), "default");
        assert_physics_invariants(ScrollPhysics::ios(), "ios");
        assert_physics_invariants(ScrollPhysics::macos(), "macos");
        assert_physics_invariants(ScrollPhysics::windows(), "windows");
        assert_physics_invariants(ScrollPhysics::android(), "android");
    }

    #[test]
    fn scroll_physics_presets_are_pure_and_distinct() {
        // Called twice: no interior state, no drift.
        assert_eq!(ScrollPhysics::ios(), ScrollPhysics::ios());
        assert_eq!(ScrollPhysics::windows(), ScrollPhysics::windows());

        // A preset that silently equals another would mean a copy/paste bug.
        assert_ne!(ScrollPhysics::ios(), ScrollPhysics::macos());
        assert_ne!(ScrollPhysics::ios(), ScrollPhysics::android());
        assert_ne!(ScrollPhysics::macos(), ScrollPhysics::windows());
        assert_ne!(ScrollPhysics::android(), ScrollPhysics::windows());
        assert_ne!(ScrollPhysics::default(), ScrollPhysics::ios());
    }

    /// The documented platform character of each preset, asserted rather than
    /// assumed: Windows must not bounce, iOS/macOS must scroll naturally.
    #[test]
    fn scroll_physics_presets_match_their_documented_platform_behavior() {
        let win = ScrollPhysics::windows();
        assert_eq!(win.overscroll_elasticity, 0.0);
        assert_eq!(win.max_overscroll_distance, 0.0);
        assert!(!win.invert_direction);

        assert!(ScrollPhysics::ios().invert_direction);
        assert!(ScrollPhysics::macos().invert_direction);
        assert!(!ScrollPhysics::android().invert_direction);

        // iOS is the "slowest to stop" of the presets.
        assert!(ScrollPhysics::ios().deceleration_rate > ScrollPhysics::windows().deceleration_rate);

        // The default is Windows-like: no bounce.
        assert_eq!(ScrollPhysics::default().overscroll_elasticity, 0.0);
    }

    // ======================================================================
    // ScrollbarFadeDelay::new / ScrollbarFadeDuration::new  (constructors)
    // ======================================================================

    #[test]
    fn fade_delay_and_duration_constructors_store_their_argument_verbatim() {
        for ms in [0u32, 1, 16, 500, u32::MAX / 2, u32::MAX - 1, u32::MAX] {
            assert_eq!(ScrollbarFadeDelay::new(ms).ms, ms);
            assert_eq!(ScrollbarFadeDuration::new(ms).ms, ms);
        }
    }

    #[test]
    fn fade_zero_constants_agree_with_new_and_default() {
        assert_eq!(ScrollbarFadeDelay::ZERO, ScrollbarFadeDelay::new(0));
        assert_eq!(ScrollbarFadeDelay::ZERO, ScrollbarFadeDelay::default());
        assert_eq!(ScrollbarFadeDuration::ZERO, ScrollbarFadeDuration::new(0));
        assert_eq!(ScrollbarFadeDuration::ZERO, ScrollbarFadeDuration::default());
        assert_eq!(ScrollbarFadeDelay::ZERO.ms, 0);
        assert_eq!(ScrollbarFadeDuration::ZERO.ms, 0);
    }

    /// The derived `Ord` must order by milliseconds, not by declaration order of
    /// some future field, otherwise "fades sooner" comparisons invert.
    #[test]
    fn fade_delay_orders_by_millisecond_count() {
        assert!(ScrollbarFadeDelay::new(0) < ScrollbarFadeDelay::new(1));
        assert!(ScrollbarFadeDelay::new(499) < ScrollbarFadeDelay::new(500));
        assert!(ScrollbarFadeDelay::new(u32::MAX) > ScrollbarFadeDelay::new(u32::MAX - 1));
        assert!(ScrollbarFadeDuration::new(0) < ScrollbarFadeDuration::new(u32::MAX));
    }

    // ======================================================================
    // print_as_css_value  (encoders)
    // ======================================================================

    #[test]
    fn enum_printers_emit_the_css_keywords() {
        assert_eq!(ScrollBehavior::Auto.print_as_css_value(), "auto");
        assert_eq!(ScrollBehavior::Smooth.print_as_css_value(), "smooth");
        assert_eq!(ScrollBehavior::default(), ScrollBehavior::Auto);

        assert_eq!(OverscrollBehavior::Auto.print_as_css_value(), "auto");
        assert_eq!(OverscrollBehavior::Contain.print_as_css_value(), "contain");
        assert_eq!(OverscrollBehavior::None.print_as_css_value(), "none");
        assert_eq!(OverscrollBehavior::default(), OverscrollBehavior::Auto);

        assert_eq!(OverflowScrolling::Auto.print_as_css_value(), "auto");
        assert_eq!(OverflowScrolling::Touch.print_as_css_value(), "touch");
        assert_eq!(OverflowScrolling::default(), OverflowScrolling::Auto);

        assert_eq!(LayoutScrollbarWidth::Auto.print_as_css_value(), "auto");
        assert_eq!(LayoutScrollbarWidth::Thin.print_as_css_value(), "thin");
        assert_eq!(LayoutScrollbarWidth::None.print_as_css_value(), "none");
        assert_eq!(LayoutScrollbarWidth::default(), LayoutScrollbarWidth::Auto);

        assert_eq!(ScrollbarVisibilityMode::Always.print_as_css_value(), "always");
        assert_eq!(
            ScrollbarVisibilityMode::WhenScrolling.print_as_css_value(),
            "when-scrolling"
        );
        assert_eq!(ScrollbarVisibilityMode::Auto.print_as_css_value(), "auto");
        assert_eq!(
            ScrollbarVisibilityMode::default(),
            ScrollbarVisibilityMode::Always
        );
    }

    /// `0` is printed unit-less (a bare `0` is legal CSS for a time), everything
    /// else carries the `ms` unit — dropping the unit on a non-zero value would
    /// emit invalid CSS.
    #[test]
    fn fade_printers_special_case_zero_and_keep_the_unit_otherwise() {
        assert_eq!(ScrollbarFadeDelay::new(0).print_as_css_value(), "0");
        assert_eq!(ScrollbarFadeDelay::new(1).print_as_css_value(), "1ms");
        assert_eq!(ScrollbarFadeDelay::new(500).print_as_css_value(), "500ms");
        assert_eq!(
            ScrollbarFadeDelay::new(u32::MAX).print_as_css_value(),
            "4294967295ms"
        );
        assert_eq!(ScrollbarFadeDuration::new(0).print_as_css_value(), "0");
        assert_eq!(ScrollbarFadeDuration::new(200).print_as_css_value(), "200ms");
        assert_eq!(
            ScrollbarFadeDuration::new(u32::MAX).print_as_css_value(),
            "4294967295ms"
        );
    }

    #[test]
    fn scrollbar_color_printer_emits_two_eight_digit_hashes() {
        assert_eq!(StyleScrollbarColor::Auto.print_as_css_value(), "auto");
        assert_eq!(StyleScrollbarColor::default(), StyleScrollbarColor::Auto);

        let custom = StyleScrollbarColor::Custom(ScrollbarColorCustom {
            thumb: ColorU::RED,
            track: ColorU::TRANSPARENT,
        });
        assert_eq!(custom.print_as_css_value(), "#ff0000ff #00000000");
    }

    /// The aggregate printers are non-standard debug formats; they must at least
    /// not panic and must include both sub-scrollbars.
    #[test]
    fn aggregate_printers_do_not_panic_and_mention_both_axes() {
        let printed = ScrollbarStyle::default().print_as_css_value();
        assert!(printed.contains("horz("), "{printed}");
        assert!(printed.contains("vert("), "{printed}");

        let info = ScrollbarInfo::default().print_as_css_value();
        assert!(info.contains("width:"), "{info}");
        assert!(info.contains("thumb:"), "{info}");
        assert!(info.contains("resizer:"), "{info}");
    }

    // ======================================================================
    // FormatAsRustCode  (codegen encoders)
    // ======================================================================

    #[test]
    fn format_as_rust_code_emits_constructible_expressions() {
        assert_eq!(
            LayoutScrollbarWidth::Thin.format_as_rust_code(0),
            "LayoutScrollbarWidth::Thin"
        );
        assert_eq!(
            LayoutScrollbarWidth::None.format_as_rust_code(7),
            "LayoutScrollbarWidth::None",
            "indent depth must not leak into a unit-variant literal"
        );
        assert_eq!(
            ScrollbarVisibilityMode::WhenScrolling.format_as_rust_code(0),
            "ScrollbarVisibilityMode::WhenScrolling"
        );
        assert_eq!(
            StyleScrollbarColor::Auto.format_as_rust_code(0),
            "StyleScrollbarColor::Auto"
        );

        // The `new(..)` codegen must round-trip the exact u32, including the extremes.
        assert_eq!(
            ScrollbarFadeDelay::new(0).format_as_rust_code(0),
            "ScrollbarFadeDelay::new(0)"
        );
        assert_eq!(
            ScrollbarFadeDelay::new(u32::MAX).format_as_rust_code(3),
            "ScrollbarFadeDelay::new(4294967295)"
        );
        assert_eq!(
            ScrollbarFadeDuration::new(u32::MAX).format_as_rust_code(0),
            "ScrollbarFadeDuration::new(4294967295)"
        );
    }

    #[test]
    fn format_as_rust_code_of_aggregates_does_not_panic() {
        let custom = StyleScrollbarColor::Custom(ScrollbarColorCustom {
            thumb: ColorU::TRANSPARENT,
            track: ColorU::WHITE,
        })
        .format_as_rust_code(0);
        assert!(
            custom.starts_with("StyleScrollbarColor::Custom(ScrollbarColorCustom {"),
            "{custom}"
        );
        assert!(custom.contains("thumb:") && custom.contains("track:"), "{custom}");

        for tabs in [0usize, 1, 4] {
            let code = ScrollbarStyle::default().format_as_rust_code(tabs);
            assert!(code.starts_with("ScrollbarStyle {"), "{code}");
            assert!(code.contains("horizontal:"), "{code}");
            assert!(code.contains("vertical:"), "{code}");
        }
    }

    // ======================================================================
    // Defaults / constants  (invariants)
    // ======================================================================

    #[test]
    fn scrollbar_info_default_is_the_classic_light_constant() {
        assert_eq!(ScrollbarInfo::default(), SCROLLBAR_CLASSIC_LIGHT);

        let style = ScrollbarStyle::default();
        assert_eq!(style.horizontal, SCROLLBAR_CLASSIC_LIGHT);
        assert_eq!(style.vertical, SCROLLBAR_CLASSIC_LIGHT);
    }

    /// `ComputedScrollbarStyle::default()` reads its colors out of the default
    /// `ScrollbarInfo`; if the default track/thumb ever became a gradient the
    /// `match` would silently fall through to `None` (UA default) instead.
    #[test]
    fn computed_default_mirrors_the_default_scrollbar_info() {
        let computed = ComputedScrollbarStyle::default();
        let info = ScrollbarInfo::default();

        assert_eq!(computed.width, Some(info.width));
        assert_eq!(
            computed.thumb_color,
            Some(ColorU { r: 193, g: 193, b: 193, a: 255 })
        );
        assert_eq!(
            computed.track_color,
            Some(ColorU { r: 241, g: 241, b: 241, a: 255 })
        );
        assert!(
            computed.thumb_color.is_some() && computed.track_color.is_some(),
            "the classic-light default must resolve to solid colors, not None"
        );
    }

    /// Overlay presets must clip to the container border, classic (space-reserving)
    /// ones must not — the flag is what decides whether the bar is drawn inside
    /// rounded corners.
    #[test]
    fn preset_constants_agree_on_the_overlay_clipping_flag() {
        for info in [SCROLLBAR_CLASSIC_LIGHT, SCROLLBAR_CLASSIC_DARK] {
            assert!(!info.clip_to_container_border);
            assert_eq!(info.scroll_behavior, ScrollBehavior::Auto);
        }
        for info in [
            SCROLLBAR_MACOS_LIGHT,
            SCROLLBAR_MACOS_DARK,
            SCROLLBAR_IOS_LIGHT,
            SCROLLBAR_IOS_DARK,
            SCROLLBAR_ANDROID_LIGHT,
            SCROLLBAR_ANDROID_DARK,
        ] {
            assert!(info.clip_to_container_border);
            assert_eq!(info.scroll_behavior, ScrollBehavior::Smooth);
        }
        for info in [SCROLLBAR_WINDOWS_LIGHT, SCROLLBAR_WINDOWS_DARK] {
            assert!(!info.clip_to_container_border);
            assert_eq!(info.overscroll_behavior_x, OverscrollBehavior::None);
            assert_eq!(info.overscroll_behavior_y, OverscrollBehavior::None);
        }
    }

    /// Light and dark variants of the same platform must differ only in color,
    /// never in geometry — a width drift would make theme switching relayout.
    #[test]
    fn light_and_dark_presets_share_their_geometry() {
        for (light, dark) in [
            (SCROLLBAR_CLASSIC_LIGHT, SCROLLBAR_CLASSIC_DARK),
            (SCROLLBAR_MACOS_LIGHT, SCROLLBAR_MACOS_DARK),
            (SCROLLBAR_WINDOWS_LIGHT, SCROLLBAR_WINDOWS_DARK),
            (SCROLLBAR_IOS_LIGHT, SCROLLBAR_IOS_DARK),
            (SCROLLBAR_ANDROID_LIGHT, SCROLLBAR_ANDROID_DARK),
        ] {
            assert_eq!(light.width, dark.width);
            assert_eq!(light.padding_left, dark.padding_left);
            assert_eq!(light.padding_right, dark.padding_right);
            assert_eq!(light.clip_to_container_border, dark.clip_to_container_border);
            assert_ne!(light.thumb, dark.thumb, "light/dark thumbs must differ");
        }
    }

    // ======================================================================
    // parse_layout_scrollbar_width  (parser)
    // ======================================================================

    #[cfg(feature = "parser")]
    #[test]
    fn scrollbar_width_parses_the_three_legal_keywords() {
        assert_eq!(
            parse_layout_scrollbar_width("auto"),
            Ok(LayoutScrollbarWidth::Auto)
        );
        assert_eq!(
            parse_layout_scrollbar_width("thin"),
            Ok(LayoutScrollbarWidth::Thin)
        );
        assert_eq!(
            parse_layout_scrollbar_width("none"),
            Ok(LayoutScrollbarWidth::None)
        );
    }

    #[cfg(feature = "parser")]
    #[test]
    fn scrollbar_width_trims_surrounding_whitespace_but_rejects_inner_junk() {
        assert_eq!(
            parse_layout_scrollbar_width("  \t thin \n "),
            Ok(LayoutScrollbarWidth::Thin)
        );
        assert!(parse_layout_scrollbar_width("thin;").is_err());
        assert!(parse_layout_scrollbar_width("thin thin").is_err());
        assert!(parse_layout_scrollbar_width("th in").is_err());
    }

    /// Keyword matching is byte-exact: CSS keywords are case-insensitive in the
    /// spec, so an upper-case `AUTO` being rejected here is a real (if minor)
    /// conformance gap. Pinned so a future fix is a deliberate change.
    #[cfg(feature = "parser")]
    #[test]
    fn scrollbar_width_keyword_matching_is_case_sensitive() {
        assert!(parse_layout_scrollbar_width("AUTO").is_err());
        assert!(parse_layout_scrollbar_width("Thin").is_err());
        assert!(parse_layout_scrollbar_width("NONE").is_err());
    }

    #[cfg(feature = "parser")]
    #[test]
    fn scrollbar_width_rejects_every_garbage_input_without_panicking() {
        for input in GARBAGE {
            assert!(
                parse_layout_scrollbar_width(input).is_err(),
                "expected {input:?} to be rejected"
            );
        }
    }

    /// The error must carry the caller's *untrimmed* slice so diagnostics can
    /// point back at the original source text.
    #[cfg(feature = "parser")]
    #[test]
    fn scrollbar_width_error_keeps_the_raw_untrimmed_input() {
        let raw = "  thick  ";
        assert_eq!(
            parse_layout_scrollbar_width(raw),
            Err(LayoutScrollbarWidthParseError::InvalidValue(raw))
        );
        let msg = format!("{}", parse_layout_scrollbar_width(raw).unwrap_err());
        assert!(msg.contains(raw), "{msg}");
    }

    #[cfg(feature = "parser")]
    #[test]
    fn scrollbar_width_survives_a_megabyte_of_input_and_deep_nesting() {
        let huge = "a".repeat(1_000_000);
        assert!(parse_layout_scrollbar_width(&huge).is_err());

        let repeated_token = "auto".repeat(250_000);
        assert!(parse_layout_scrollbar_width(&repeated_token).is_err());

        let nested = "(".repeat(10_000);
        assert!(parse_layout_scrollbar_width(&nested).is_err());
    }

    #[cfg(feature = "parser")]
    #[test]
    fn scrollbar_width_round_trips_through_its_printer() {
        for value in [
            LayoutScrollbarWidth::Auto,
            LayoutScrollbarWidth::Thin,
            LayoutScrollbarWidth::None,
        ] {
            let encoded = value.print_as_css_value();
            assert_eq!(
                parse_layout_scrollbar_width(&encoded),
                Ok(value),
                "{encoded} did not decode back to {value:?}"
            );
        }
    }

    // ======================================================================
    // parse_style_scrollbar_color  (parser)
    // ======================================================================

    #[cfg(feature = "parser")]
    #[test]
    fn scrollbar_color_needs_exactly_two_colors_or_the_auto_keyword() {
        assert_eq!(parse_style_scrollbar_color("auto"), Ok(StyleScrollbarColor::Auto));
        assert_eq!(
            parse_style_scrollbar_color("  auto  "),
            Ok(StyleScrollbarColor::Auto)
        );
        assert_eq!(
            parse_style_scrollbar_color("red blue"),
            Ok(StyleScrollbarColor::Custom(ScrollbarColorCustom {
                thumb: ColorU::RED,
                track: ColorU::BLUE,
            }))
        );

        // Too few / too many components: rejected as InvalidValue, not as a color error.
        for input in ["red", "#fff", "red blue green", "a b c d"] {
            assert!(
                matches!(
                    parse_style_scrollbar_color(input),
                    Err(StyleScrollbarColorParseError::InvalidValue(_))
                ),
                "expected {input:?} to be an InvalidValue error"
            );
        }
    }

    /// Component splitting is on *any* whitespace run, so tabs, newlines and
    /// repeated spaces are all legal separators.
    #[cfg(feature = "parser")]
    #[test]
    fn scrollbar_color_accepts_any_whitespace_run_as_the_separator() {
        let expected = StyleScrollbarColor::Custom(ScrollbarColorCustom {
            thumb: ColorU::RED,
            track: ColorU::BLUE,
        });
        assert_eq!(parse_style_scrollbar_color("red\tblue"), Ok(expected));
        assert_eq!(parse_style_scrollbar_color("red\n blue"), Ok(expected));
        assert_eq!(parse_style_scrollbar_color("  red     blue  "), Ok(expected));
    }

    /// Color *names* are case-insensitive (the color parser lowercases), unlike
    /// the `auto` keyword right above it, which is compared verbatim.
    #[cfg(feature = "parser")]
    #[test]
    fn scrollbar_color_names_are_case_insensitive_but_the_auto_keyword_is_not() {
        assert_eq!(
            parse_style_scrollbar_color("RED BLUE"),
            Ok(StyleScrollbarColor::Custom(ScrollbarColorCustom {
                thumb: ColorU::RED,
                track: ColorU::BLUE,
            }))
        );
        // "AUTO" is a single token -> not the auto keyword, and not two colors.
        assert!(matches!(
            parse_style_scrollbar_color("AUTO"),
            Err(StyleScrollbarColorParseError::InvalidValue(_))
        ));
        // ...and `auto` is not a named color either, so it cannot sneak in as one.
        assert!(matches!(
            parse_style_scrollbar_color("auto auto"),
            Err(StyleScrollbarColorParseError::Color(_))
        ));
    }

    /// Whitespace-splitting happens *before* the color parser runs, so a
    /// functional color with spaces after its commas is torn into pieces.
    /// `rgb(255, 0, 0) blue` is valid CSS but is rejected here; the space-free
    /// spelling works. Pinned as a known limitation.
    #[cfg(feature = "parser")]
    #[test]
    fn scrollbar_color_rejects_functional_colors_containing_spaces() {
        assert_eq!(
            parse_style_scrollbar_color("rgb(255,0,0) blue"),
            Ok(StyleScrollbarColor::Custom(ScrollbarColorCustom {
                thumb: ColorU::RED,
                track: ColorU::BLUE,
            }))
        );
        assert!(matches!(
            parse_style_scrollbar_color("rgb(255, 0, 0) blue"),
            Err(StyleScrollbarColorParseError::InvalidValue(_))
        ));
    }

    #[cfg(feature = "parser")]
    #[test]
    fn scrollbar_color_reports_which_component_failed() {
        // A bad thumb is reported as a color error, not as InvalidValue.
        assert!(matches!(
            parse_style_scrollbar_color("notacolor blue"),
            Err(StyleScrollbarColorParseError::Color(_))
        ));
        assert!(matches!(
            parse_style_scrollbar_color("red notacolor"),
            Err(StyleScrollbarColorParseError::Color(_))
        ));
        assert!(matches!(
            parse_style_scrollbar_color("#gggggg #000000"),
            Err(StyleScrollbarColorParseError::Color(_))
        ));
    }

    #[cfg(feature = "parser")]
    #[test]
    fn scrollbar_color_rejects_garbage_without_panicking() {
        for input in GARBAGE {
            assert!(
                parse_style_scrollbar_color(input).is_err(),
                "expected {input:?} to be rejected"
            );
        }
        // Boundary numerics as color components.
        for input in [
            "0 0",
            "-0 -0",
            "NaN NaN",
            "inf inf",
            "9223372036854775807 1",
            "1e400 1e400",
            "-1 -1",
        ] {
            assert!(
                parse_style_scrollbar_color(input).is_err(),
                "expected {input:?} to be rejected"
            );
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn scrollbar_color_survives_huge_and_deeply_nested_input() {
        let huge = "z".repeat(500_000);
        let two_huge = format!("{huge} {huge}");
        assert!(parse_style_scrollbar_color(&two_huge).is_err());

        let nested = "(".repeat(10_000);
        assert!(parse_style_scrollbar_color(&format!("{nested} {nested}")).is_err());

        // Many components: must be rejected on count, not walked color-by-color.
        let many = "red ".repeat(100_000);
        assert!(matches!(
            parse_style_scrollbar_color(&many),
            Err(StyleScrollbarColorParseError::InvalidValue(_))
        ));
    }

    /// The color error carries the *trimmed* input (the function rebinds `input`
    /// to the trimmed slice), unlike `parse_layout_scrollbar_width`, which keeps
    /// the raw slice. Pinned so the inconsistency is visible.
    #[cfg(feature = "parser")]
    #[test]
    fn scrollbar_color_error_carries_the_trimmed_input() {
        assert_eq!(
            parse_style_scrollbar_color("  red  "),
            Err(StyleScrollbarColorParseError::InvalidValue("red"))
        );
    }

    #[cfg(feature = "parser")]
    #[test]
    fn scrollbar_color_round_trips_through_its_printer() {
        let samples = [
            StyleScrollbarColor::Auto,
            StyleScrollbarColor::Custom(ScrollbarColorCustom {
                thumb: ColorU::RED,
                track: ColorU::BLUE,
            }),
            StyleScrollbarColor::Custom(ScrollbarColorCustom {
                thumb: ColorU::TRANSPARENT,
                track: ColorU::TRANSPARENT,
            }),
            StyleScrollbarColor::Custom(ScrollbarColorCustom {
                thumb: ColorU { r: 0, g: 0, b: 0, a: 100 },
                track: ColorU { r: 1, g: 2, b: 3, a: 4 },
            }),
            StyleScrollbarColor::Custom(ScrollbarColorCustom {
                thumb: ColorU::WHITE,
                track: ColorU::BLACK,
            }),
        ];
        for value in samples {
            let encoded = value.print_as_css_value();
            assert_eq!(
                parse_style_scrollbar_color(&encoded),
                Ok(value),
                "{encoded} did not decode back to {value:?}"
            );
        }
    }

    // ======================================================================
    // parse_scrollbar_visibility_mode  (parser)
    // ======================================================================

    #[cfg(feature = "parser")]
    #[test]
    fn visibility_mode_parses_its_three_keywords_and_trims() {
        assert_eq!(
            parse_scrollbar_visibility_mode("always"),
            Ok(ScrollbarVisibilityMode::Always)
        );
        assert_eq!(
            parse_scrollbar_visibility_mode(" when-scrolling\t"),
            Ok(ScrollbarVisibilityMode::WhenScrolling)
        );
        assert_eq!(
            parse_scrollbar_visibility_mode("auto"),
            Ok(ScrollbarVisibilityMode::Auto)
        );
    }

    #[cfg(feature = "parser")]
    #[test]
    fn visibility_mode_rejects_near_misses_and_garbage() {
        for input in [
            "when scrolling", // space instead of hyphen
            "whenscrolling",
            "when-scrolling-",
            "-when-scrolling",
            "ALWAYS",
            "always;",
            "always auto",
        ] {
            assert!(
                parse_scrollbar_visibility_mode(input).is_err(),
                "expected {input:?} to be rejected"
            );
        }
        for input in GARBAGE {
            assert!(
                parse_scrollbar_visibility_mode(input).is_err(),
                "expected {input:?} to be rejected"
            );
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn visibility_mode_survives_huge_and_nested_input() {
        assert!(parse_scrollbar_visibility_mode(&"a".repeat(1_000_000)).is_err());
        assert!(parse_scrollbar_visibility_mode(&"always".repeat(200_000)).is_err());
        assert!(parse_scrollbar_visibility_mode(&"[".repeat(10_000)).is_err());
    }

    #[cfg(feature = "parser")]
    #[test]
    fn visibility_mode_round_trips_through_its_printer() {
        for value in [
            ScrollbarVisibilityMode::Always,
            ScrollbarVisibilityMode::WhenScrolling,
            ScrollbarVisibilityMode::Auto,
        ] {
            let encoded = value.print_as_css_value();
            assert_eq!(
                parse_scrollbar_visibility_mode(&encoded),
                Ok(value),
                "{encoded} did not decode back to {value:?}"
            );
        }
    }

    // ======================================================================
    // parse_time_ms  (private parser)
    // ======================================================================

    #[cfg(feature = "parser")]
    #[test]
    fn parse_time_ms_accepts_bare_zero_and_both_units() {
        assert_eq!(parse_time_ms("0"), Some(0));
        assert_eq!(parse_time_ms("0ms"), Some(0));
        assert_eq!(parse_time_ms("0s"), Some(0));
        assert_eq!(parse_time_ms("500ms"), Some(500));
        assert_eq!(parse_time_ms("1s"), Some(1000));
        assert_eq!(parse_time_ms("1.5s"), Some(1500));
        assert_eq!(parse_time_ms("  200ms  "), Some(200));
        assert_eq!(parse_time_ms("200MS"), Some(200), "units are case-insensitive");
    }

    /// A unit is mandatory (except for a bare `0`) and must be attached to the
    /// number — `"1 s"` has an interior space and cannot parse.
    #[cfg(feature = "parser")]
    #[test]
    fn parse_time_ms_requires_an_attached_unit() {
        assert_eq!(parse_time_ms("500"), None);
        assert_eq!(parse_time_ms("1 s"), None);
        assert_eq!(parse_time_ms("500 ms"), None);
        assert_eq!(parse_time_ms("ms"), None);
        assert_eq!(parse_time_ms("s"), None);
        assert_eq!(parse_time_ms("500px"), None);
        assert_eq!(parse_time_ms("500msms"), None);
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_time_ms_rejects_empty_blank_unicode_and_garbage() {
        for input in ["", " ", "   ", "\t\n", "\u{1F600}", "e\u{0301}", "٥ms", "５００ms"] {
            assert_eq!(parse_time_ms(input), None, "expected {input:?} to be rejected");
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_time_ms_rejects_negative_durations() {
        assert_eq!(parse_time_ms("-1ms"), None);
        assert_eq!(parse_time_ms("-0.5s"), None);
        assert_eq!(parse_time_ms("-inf ms"), None);
    }

    /// Negative *zero* is not less than zero in IEEE-754, so it slips past the
    /// `< 0.0` guard and casts to 0 — harmless, but worth pinning.
    #[cfg(feature = "parser")]
    #[test]
    fn parse_time_ms_accepts_negative_zero_as_zero() {
        assert_eq!(parse_time_ms("-0ms"), Some(0));
        assert_eq!(parse_time_ms("-0.0s"), Some(0));
    }

    /// The float -> u32 cast saturates instead of wrapping or panicking:
    /// `inf` clamps to `u32::MAX`, `NaN` becomes 0. Both are *safe* (no UB, no
    /// panic), but note that `"infms"` and `"NaNms"` are accepted as durations
    /// at all — a stricter parser would reject non-finite times outright.
    #[cfg(feature = "parser")]
    #[test]
    fn parse_time_ms_saturates_on_non_finite_and_huge_values() {
        assert_eq!(parse_time_ms("infms"), Some(u32::MAX));
        assert_eq!(parse_time_ms("infinityms"), Some(u32::MAX));
        assert_eq!(parse_time_ms("infs"), Some(u32::MAX));
        assert_eq!(parse_time_ms("nanms"), Some(0));
        assert_eq!(parse_time_ms("NaNms"), Some(0));

        assert_eq!(parse_time_ms("1e30ms"), Some(u32::MAX));
        assert_eq!(parse_time_ms("1e400ms"), Some(u32::MAX), "overflows f32 to inf");
        assert_eq!(parse_time_ms("4294967296ms"), Some(u32::MAX), "2^32 clamps");
        assert_eq!(parse_time_ms("1e-30ms"), Some(0), "underflows to zero");

        // A million digits must saturate, not hang.
        let long_number = format!("{}ms", "9".repeat(100_000));
        assert_eq!(parse_time_ms(&long_number), Some(u32::MAX));
    }

    /// Seconds are multiplied by 1000 *before* the cast, so a value that fits in
    /// a u32 as seconds can still saturate as milliseconds.
    #[cfg(feature = "parser")]
    #[test]
    fn parse_time_ms_saturates_when_seconds_overflow_milliseconds() {
        // Exact while the millisecond product stays inside f32's integer range.
        assert_eq!(parse_time_ms("1000s"), Some(1_000_000));
        assert_eq!(parse_time_ms("16777s"), Some(16_777_000));

        // Past u32::MAX milliseconds the cast clamps instead of wrapping.
        assert_eq!(parse_time_ms("4294968s"), Some(u32::MAX));
        assert_eq!(parse_time_ms("5000000s"), Some(u32::MAX));

        // Just under the clamp, the f32 product is only accurate to ~256ms
        // (the ulp at that magnitude) — near, but no longer exact.
        let ms = parse_time_ms("4294967s").expect("4294967s must parse");
        assert!(
            ms.abs_diff(4_294_967_000) <= 512,
            "4294967s decoded to {ms}, which is nowhere near 4294967000ms"
        );

        assert_eq!(parse_time_ms("0.0005s"), Some(0), "sub-ms truncates toward zero");
    }

    // ======================================================================
    // parse_scrollbar_fade_delay / parse_scrollbar_fade_duration  (parsers)
    // ======================================================================

    #[cfg(feature = "parser")]
    #[test]
    fn fade_parsers_accept_the_documented_syntax() {
        assert_eq!(
            parse_scrollbar_fade_delay("500ms"),
            Ok(ScrollbarFadeDelay::new(500))
        );
        assert_eq!(parse_scrollbar_fade_delay("0"), Ok(ScrollbarFadeDelay::ZERO));
        assert_eq!(
            parse_scrollbar_fade_delay(" 1s "),
            Ok(ScrollbarFadeDelay::new(1000))
        );
        assert_eq!(
            parse_scrollbar_fade_duration("200ms"),
            Ok(ScrollbarFadeDuration::new(200))
        );
        assert_eq!(
            parse_scrollbar_fade_duration("0"),
            Ok(ScrollbarFadeDuration::ZERO)
        );
    }

    #[cfg(feature = "parser")]
    #[test]
    fn fade_parsers_reject_garbage_and_keep_the_raw_input_in_the_error() {
        for input in GARBAGE {
            assert!(
                parse_scrollbar_fade_delay(input).is_err(),
                "delay: expected {input:?} to be rejected"
            );
            assert!(
                parse_scrollbar_fade_duration(input).is_err(),
                "duration: expected {input:?} to be rejected"
            );
        }

        let raw = "  bogus  ";
        assert_eq!(
            parse_scrollbar_fade_delay(raw),
            Err(ScrollbarFadeDelayParseError::InvalidValue(raw))
        );
        assert_eq!(
            parse_scrollbar_fade_duration(raw),
            Err(ScrollbarFadeDurationParseError::InvalidValue(raw))
        );
    }

    #[cfg(feature = "parser")]
    #[test]
    fn fade_parsers_reject_negative_delays() {
        assert!(parse_scrollbar_fade_delay("-1ms").is_err());
        assert!(parse_scrollbar_fade_delay("-500ms").is_err());
        assert!(parse_scrollbar_fade_duration("-0.5s").is_err());
    }

    #[cfg(feature = "parser")]
    #[test]
    fn fade_parsers_saturate_instead_of_overflowing() {
        assert_eq!(
            parse_scrollbar_fade_delay("1e30ms"),
            Ok(ScrollbarFadeDelay::new(u32::MAX))
        );
        assert_eq!(
            parse_scrollbar_fade_duration("99999999999999s"),
            Ok(ScrollbarFadeDuration::new(u32::MAX))
        );
    }

    #[cfg(feature = "parser")]
    #[test]
    fn fade_parsers_survive_huge_and_nested_input() {
        assert!(parse_scrollbar_fade_delay(&"a".repeat(1_000_000)).is_err());
        assert!(parse_scrollbar_fade_duration(&"0ms".repeat(300_000)).is_err());
        assert!(parse_scrollbar_fade_delay(&"(".repeat(10_000)).is_err());
        assert!(parse_scrollbar_fade_duration(&"[".repeat(10_000)).is_err());
    }

    /// encode -> decode is the identity for every millisecond count an `f32` can
    /// represent exactly (`<= 2^24`), including the `0` special case and the
    /// `u32::MAX` extreme (whose f32 rounding lands back on `u32::MAX` after the
    /// saturating cast).
    #[cfg(feature = "parser")]
    #[test]
    fn fade_delay_round_trips_exactly_up_to_two_pow_24() {
        for ms in [
            0u32,
            1,
            8,
            16,
            200,
            500,
            65_535,
            1_000_000,
            TWO_POW_24 - 1,
            TWO_POW_24,
            u32::MAX,
        ] {
            let value = ScrollbarFadeDelay::new(ms);
            let encoded = value.print_as_css_value();
            assert_eq!(
                parse_scrollbar_fade_delay(&encoded),
                Ok(value),
                "{ms}ms encoded as {encoded:?} did not decode back"
            );

            let value = ScrollbarFadeDuration::new(ms);
            let encoded = value.print_as_css_value();
            assert_eq!(
                parse_scrollbar_fade_duration(&encoded),
                Ok(value),
                "{ms}ms encoded as {encoded:?} did not decode back"
            );
        }
    }

    /// Above 2^24 the round-trip is lossy: the value is snapped to the nearest
    /// representable `f32`. Pinned as a precision limit of the shared duration
    /// parser (a delay is never realistically > 4.6 hours, so this is benign).
    #[cfg(feature = "parser")]
    #[test]
    fn fade_delay_round_trip_is_lossy_above_two_pow_24() {
        let value = ScrollbarFadeDelay::new(TWO_POW_24 + 1);
        let decoded = parse_scrollbar_fade_delay(&value.print_as_css_value()).unwrap();
        assert_ne!(decoded, value, "expected precision loss above 2^24");
        assert_eq!(decoded.ms, TWO_POW_24, "must snap down to the nearest f32");
    }

    // ======================================================================
    // Error to_contained / to_shared  (getters)
    // ======================================================================

    /// Strings that stress the owned<->borrowed error conversions: empty, blank,
    /// multibyte, embedded NUL, and a 100k-byte payload.
    fn error_payloads() -> [String; 6] {
        [
            String::new(),
            String::from(" "),
            String::from("thick"),
            String::from("\u{1F600}\u{0301}"),
            String::from("nul\0inside"),
            "x".repeat(100_000),
        ]
    }

    #[test]
    fn layout_scrollbar_width_error_round_trips_through_owned_and_back() {
        for payload in error_payloads() {
            let shared = LayoutScrollbarWidthParseError::InvalidValue(&payload);
            let owned = shared.to_contained();
            assert_eq!(
                owned,
                LayoutScrollbarWidthParseErrorOwned::InvalidValue(payload.clone().into())
            );
            assert_eq!(owned.to_shared(), shared, "owned -> shared lost information");
            assert_eq!(
                owned.to_shared().to_contained(),
                owned,
                "conversion is not idempotent"
            );
        }
    }

    #[test]
    fn visibility_mode_error_round_trips_through_owned_and_back() {
        for payload in error_payloads() {
            let shared = ScrollbarVisibilityModeParseError::InvalidValue(&payload);
            let owned = shared.to_contained();
            assert_eq!(owned.to_shared(), shared);
            assert_eq!(owned.to_shared().to_contained(), owned);
        }
    }

    #[test]
    fn fade_delay_and_duration_errors_round_trip_through_owned_and_back() {
        for payload in error_payloads() {
            let delay = ScrollbarFadeDelayParseError::InvalidValue(&payload);
            let owned_delay = delay.to_contained();
            assert_eq!(owned_delay.to_shared(), delay);
            assert_eq!(owned_delay.to_shared().to_contained(), owned_delay);

            let duration = ScrollbarFadeDurationParseError::InvalidValue(&payload);
            let owned_duration = duration.to_contained();
            assert_eq!(owned_duration.to_shared(), duration);
            assert_eq!(owned_duration.to_shared().to_contained(), owned_duration);
        }
    }

    #[test]
    fn scrollbar_color_invalid_value_error_round_trips_through_owned_and_back() {
        for payload in error_payloads() {
            let shared = StyleScrollbarColorParseError::InvalidValue(&payload);
            let owned = shared.to_contained();
            assert_eq!(
                owned,
                StyleScrollbarColorParseErrorOwned::InvalidValue(payload.clone().into())
            );
            assert_eq!(owned.to_shared(), shared);
            assert_eq!(owned.to_shared().to_contained(), owned);
        }
    }

    /// The nested `Color` variant must delegate to the color error's own
    /// conversion rather than flattening to a string.
    #[cfg(feature = "parser")]
    #[test]
    fn scrollbar_color_nested_color_error_round_trips_through_owned_and_back() {
        let shared = parse_style_scrollbar_color("notacolor blue").unwrap_err();
        assert!(matches!(shared, StyleScrollbarColorParseError::Color(_)));

        let owned = shared.to_contained();
        assert!(matches!(owned, StyleScrollbarColorParseErrorOwned::Color(_)));
        assert_eq!(owned.to_shared(), shared, "nested color error lost information");
        assert_eq!(owned.to_shared().to_contained(), owned);
    }

    /// Error `Display` must always name the offending input, otherwise a CSS
    /// diagnostic is useless. (`Debug` is implemented as `Display` here.)
    #[cfg(feature = "parser")]
    #[test]
    fn error_display_mentions_the_offending_input() {
        let width = parse_layout_scrollbar_width("thick").unwrap_err();
        assert!(format!("{width}").contains("thick"), "{width}");
        assert!(format!("{width:?}").contains("thick"), "{width:?}");

        let color = parse_style_scrollbar_color("red").unwrap_err();
        assert!(format!("{color}").contains("red"), "{color}");

        let vis = parse_scrollbar_visibility_mode("sometimes").unwrap_err();
        assert!(format!("{vis}").contains("sometimes"), "{vis}");

        let delay = parse_scrollbar_fade_delay("soon").unwrap_err();
        assert!(format!("{delay}").contains("soon"), "{delay}");

        let duration = parse_scrollbar_fade_duration("briefly").unwrap_err();
        assert!(format!("{duration}").contains("briefly"), "{duration}");
    }

    /// Displaying an error whose payload is empty or exotic must not panic on a
    /// byte/char boundary.
    #[test]
    fn error_display_does_not_panic_on_exotic_payloads() {
        for payload in error_payloads() {
            let err = LayoutScrollbarWidthParseError::InvalidValue(&payload);
            assert!(!format!("{err}").is_empty());
            let owned = err.to_contained();
            assert!(!format!("{}", owned.to_shared()).is_empty());
        }
    }
}
