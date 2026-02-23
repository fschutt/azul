//! CSS properties for styling scrollbars.

use alloc::string::{String, ToString};
use crate::corety::AzString;

use crate::props::{
    basic::{
        color::{parse_css_color, ColorU, CssColorParseError, CssColorParseErrorOwned},
        pixel::PixelValue,
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
/// https://developer.mozilla.org/en-US/docs/Web/CSS/scroll-behavior
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
/// https://developer.mozilla.org/en-US/docs/Web/CSS/overscroll-behavior
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

/// Combined overscroll behavior for X and Y axes
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(C)]
pub struct OverscrollBehaviorXY {
    pub x: OverscrollBehavior,
    pub y: OverscrollBehavior,
}

// ============================================================================
// Extended Scroll Configuration (Azul-specific)
// ============================================================================

/// Scroll physics configuration for momentum scrolling
///
/// This controls how scrolling feels - the "weight" and "friction" of the scroll.
/// Different platforms have different scroll physics (iOS vs Android vs Windows).
#[derive(Debug, Clone, PartialEq, PartialOrd)]
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
    pub const fn ios() -> Self {
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
    pub const fn macos() -> Self {
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
    pub const fn windows() -> Self {
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
    pub const fn android() -> Self {
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
pub enum LayoutScrollbarWidth {
    Auto,
    Thin,
    None,
}

impl Default for LayoutScrollbarWidth {
    fn default() -> Self {
        Self::Auto
    }
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
pub enum StyleScrollbarColor {
    Auto,
    Custom(ScrollbarColorCustom),
}

impl Default for StyleScrollbarColor {
    fn default() -> Self {
        Self::Auto
    }
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
impl crate::format_rust_code::FormatAsRustCode for ScrollbarStyle {
    fn format_as_rust_code(&self, tabs: usize) -> String {
        let t = String::from("    ").repeat(tabs);
        let t1 = String::from("    ").repeat(tabs + 1);
        format!(
            "ScrollbarStyle {{\r\n{}horizontal: {},\r\n{}vertical: {},\r\n{}}}",
            t1,
            crate::format_rust_code::format_scrollbar_info(&self.horizontal, tabs + 1),
            t1,
            crate::format_rust_code::format_scrollbar_info(&self.vertical, tabs + 1),
            t,
        )
    }
}

impl crate::format_rust_code::FormatAsRustCode for LayoutScrollbarWidth {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        match self {
            LayoutScrollbarWidth::Auto => String::from("LayoutScrollbarWidth::Auto"),
            LayoutScrollbarWidth::Thin => String::from("LayoutScrollbarWidth::Thin"),
            LayoutScrollbarWidth::None => String::from("LayoutScrollbarWidth::None"),
        }
    }
}

impl crate::format_rust_code::FormatAsRustCode for StyleScrollbarColor {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        match self {
            StyleScrollbarColor::Auto => String::from("StyleScrollbarColor::Auto"),
            StyleScrollbarColor::Custom(c) => format!(
                "StyleScrollbarColor::Custom(ScrollbarColorCustom {{ thumb: {}, track: {} }})",
                crate::format_rust_code::format_color_value(&c.thumb),
                crate::format_rust_code::format_color_value(&c.track)
            ),
        }
    }
}

// --- Final Computed Style ---

/// The final, resolved style for a scrollbar, after considering both
/// standard and -webkit- properties. This struct is intended for use by the layout engine.
#[derive(Debug, Clone, PartialEq)]
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

#[derive(Clone, PartialEq)]
pub enum LayoutScrollbarWidthParseError<'a> {
    InvalidValue(&'a str),
}
impl_debug_as_display!(LayoutScrollbarWidthParseError<'a>);
impl_display! { LayoutScrollbarWidthParseError<'a>, {
    InvalidValue(v) => format!("Invalid scrollbar-width value: \"{}\"", v),
}}

#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum LayoutScrollbarWidthParseErrorOwned {
    InvalidValue(AzString),
}
impl<'a> LayoutScrollbarWidthParseError<'a> {
    pub fn to_contained(&self) -> LayoutScrollbarWidthParseErrorOwned {
        match self {
            Self::InvalidValue(s) => {
                LayoutScrollbarWidthParseErrorOwned::InvalidValue(s.to_string().into())
            }
        }
    }
}
impl LayoutScrollbarWidthParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> LayoutScrollbarWidthParseError<'a> {
        match self {
            Self::InvalidValue(s) => LayoutScrollbarWidthParseError::InvalidValue(s.as_str()),
        }
    }
}

#[cfg(feature = "parser")]
pub fn parse_layout_scrollbar_width<'a>(
    input: &'a str,
) -> Result<LayoutScrollbarWidth, LayoutScrollbarWidthParseError<'a>> {
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
impl<'a> StyleScrollbarColorParseError<'a> {
    pub fn to_contained(&self) -> StyleScrollbarColorParseErrorOwned {
        match self {
            Self::InvalidValue(s) => {
                StyleScrollbarColorParseErrorOwned::InvalidValue(s.to_string().into())
            }
            Self::Color(e) => StyleScrollbarColorParseErrorOwned::Color(e.to_contained()),
        }
    }
}
impl StyleScrollbarColorParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> StyleScrollbarColorParseError<'a> {
        match self {
            Self::InvalidValue(s) => StyleScrollbarColorParseError::InvalidValue(s.as_str()),
            Self::Color(e) => StyleScrollbarColorParseError::Color(e.to_shared()),
        }
    }
}

#[cfg(feature = "parser")]
pub fn parse_style_scrollbar_color<'a>(
    input: &'a str,
) -> Result<StyleScrollbarColor, StyleScrollbarColorParseError<'a>> {
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

#[derive(Clone, PartialEq)]
pub enum CssScrollbarStyleParseError<'a> {
    Invalid(&'a str),
}

impl_debug_as_display!(CssScrollbarStyleParseError<'a>);
impl_display! { CssScrollbarStyleParseError<'a>, {
    Invalid(e) => format!("Invalid scrollbar style: \"{}\"", e),
}}

#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum CssScrollbarStyleParseErrorOwned {
    Invalid(AzString),
}

impl<'a> CssScrollbarStyleParseError<'a> {
    pub fn to_contained(&self) -> CssScrollbarStyleParseErrorOwned {
        match self {
            CssScrollbarStyleParseError::Invalid(s) => {
                CssScrollbarStyleParseErrorOwned::Invalid(s.to_string().into())
            }
        }
    }
}

impl CssScrollbarStyleParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> CssScrollbarStyleParseError<'a> {
        match self {
            CssScrollbarStyleParseErrorOwned::Invalid(s) => {
                CssScrollbarStyleParseError::Invalid(s.as_str())
            }
        }
    }
}

#[cfg(feature = "parser")]
pub fn parse_scrollbar_style<'a>(
    _input: &'a str,
) -> Result<ScrollbarStyle, CssScrollbarStyleParseError<'a>> {
    // A real implementation would parse the custom format used for -webkit-scrollbar.
    // For now, it returns the default style.
    Ok(ScrollbarStyle::default())
}

// --- STYLE RESOLUTION ---

/// Resolves the final scrollbar style for a node based on standard and
/// non-standard CSS properties.
///
/// This function implements the specified override behavior: if `scrollbar-width`
/// or `scrollbar-color` are set to anything other than `auto`, they take
/// precedence over any `::-webkit-scrollbar` styling.
pub fn resolve_scrollbar_style(
    scrollbar_width: Option<&LayoutScrollbarWidth>,
    scrollbar_color: Option<&StyleScrollbarColor>,
    webkit_scrollbar_style: Option<&ScrollbarStyle>,
) -> ComputedScrollbarStyle {
    let final_width = scrollbar_width
        .cloned()
        .unwrap_or(LayoutScrollbarWidth::Auto);
    let final_color = scrollbar_color
        .cloned()
        .unwrap_or(StyleScrollbarColor::Auto);

    // If standard properties are used (not 'auto'), they win.
    if final_width != LayoutScrollbarWidth::Auto || final_color != StyleScrollbarColor::Auto {
        let width = match final_width {
            LayoutScrollbarWidth::None => None,
            // Use a reasonable default for "thin"
            LayoutScrollbarWidth::Thin => Some(LayoutWidth::Px(PixelValue::px(8.0))),
            // If auto, fall back to -webkit- width or the UA default
            LayoutScrollbarWidth::Auto => Some(
                webkit_scrollbar_style
                    .map_or_else(|| ScrollbarInfo::default().width, |s| s.vertical.width.clone()),
            ),
        };

        let (thumb_color, track_color) = match final_color {
            StyleScrollbarColor::Custom(c) => (Some(c.thumb), Some(c.track)),
            StyleScrollbarColor::Auto => (None, None), // UA default
        };

        return ComputedScrollbarStyle {
            width: width.clone(),
            thumb_color,
            track_color,
        };
    }

    // Otherwise, fall back to -webkit-scrollbar properties if they exist.
    if let Some(webkit_style) = webkit_scrollbar_style {
        // For simplicity, we'll use the vertical scrollbar's info.
        let info = &webkit_style.vertical;

        // The -webkit-scrollbar `display: none;` is often implemented by setting width to 0.
        let width_pixels = match info.width {
            LayoutWidth::Px(px) => {
                use crate::props::basic::pixel::DEFAULT_FONT_SIZE;
                px.to_pixels_internal(0.0, DEFAULT_FONT_SIZE)
            }
            _ => 8.0, // Default for min-content/max-content
        };
        if width_pixels <= 0.0 {
            return ComputedScrollbarStyle {
                width: None,
                thumb_color: None,
                track_color: None,
            };
        }

        let thumb = match &info.thumb {
            StyleBackgroundContent::Color(c) => Some(*c),
            _ => None, // Gradients, images are not directly mapped to a single color
        };

        let track = match &info.track {
            StyleBackgroundContent::Color(c) => Some(*c),
            _ => None,
        };

        return ComputedScrollbarStyle {
            width: Some(info.width.clone()),
            thumb_color: thumb,
            track_color: track,
        };
    }

    // If no styling is provided at all, use UA defaults.
    ComputedScrollbarStyle::default()
}

#[cfg(all(test, feature = "parser"))]
mod tests {
    use super::*;
    use crate::props::{basic::color::ColorU, layout::dimensions::LayoutWidth};

    // --- Parser Tests ---

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

    // --- Resolution Logic Tests ---

    // Helper to create a default -webkit- style for testing
    fn get_webkit_style() -> ScrollbarStyle {
        let mut info = ScrollbarInfo::default();
        info.width = LayoutWidth::px(15.0);
        info.thumb = StyleBackgroundContent::Color(ColorU::GREEN);
        info.track = StyleBackgroundContent::Color(ColorU::new_rgb(100, 100, 100));
        ScrollbarStyle {
            horizontal: info.clone(),
            vertical: info,
        }
    }

    #[test]
    fn test_resolve_standard_overrides_webkit() {
        let width = LayoutScrollbarWidth::Thin;
        let color = StyleScrollbarColor::Custom(ScrollbarColorCustom {
            thumb: ColorU::RED,
            track: ColorU::BLUE,
        });
        let webkit_style = get_webkit_style();

        let resolved = resolve_scrollbar_style(Some(&width), Some(&color), Some(&webkit_style));

        // "thin" resolves to a specific px value (e.g., 8px)
        assert_eq!(resolved.width, Some(LayoutWidth::px(8.0)));
        assert_eq!(resolved.thumb_color, Some(ColorU::RED));
        assert_eq!(resolved.track_color, Some(ColorU::BLUE));
    }

    #[test]
    fn test_resolve_standard_auto_falls_back_to_webkit() {
        let width = LayoutScrollbarWidth::Auto;
        let color = StyleScrollbarColor::Auto;
        let webkit_style = get_webkit_style();

        let resolved = resolve_scrollbar_style(Some(&width), Some(&color), Some(&webkit_style));

        assert_eq!(resolved.width, Some(LayoutWidth::px(15.0)));
        assert_eq!(resolved.thumb_color, Some(ColorU::GREEN));
        assert_eq!(resolved.track_color, Some(ColorU::new_rgb(100, 100, 100)));
    }

    #[test]
    fn test_resolve_no_styles_uses_default() {
        let resolved = resolve_scrollbar_style(None, None, None);
        assert_eq!(resolved, ComputedScrollbarStyle::default());
    }

    #[test]
    fn test_resolve_scrollbar_width_none() {
        let width = LayoutScrollbarWidth::None;
        let webkit_style = get_webkit_style();

        let resolved = resolve_scrollbar_style(Some(&width), None, Some(&webkit_style));
        assert_eq!(resolved.width, None);
    }

    #[test]
    fn test_resolve_webkit_display_none_equivalent() {
        let mut webkit_style = get_webkit_style();
        webkit_style.vertical.width = LayoutWidth::px(0.0);

        let resolved = resolve_scrollbar_style(None, None, Some(&webkit_style));
        assert_eq!(resolved.width, None);
    }

    #[test]
    fn test_resolve_only_color_is_set() {
        let color = StyleScrollbarColor::Custom(ScrollbarColorCustom {
            thumb: ColorU::RED,
            track: ColorU::BLUE,
        });
        let webkit_style = get_webkit_style();

        let resolved = resolve_scrollbar_style(None, Some(&color), Some(&webkit_style));

        // Width should fall back to webkit, but colors should be standard
        assert_eq!(resolved.width, Some(LayoutWidth::px(15.0)));
        assert_eq!(resolved.thumb_color, Some(ColorU::RED));
        assert_eq!(resolved.track_color, Some(ColorU::BLUE));
    }

    #[test]
    fn test_resolve_only_width_is_set() {
        let width = LayoutScrollbarWidth::Thin;
        let webkit_style = get_webkit_style();

        let resolved = resolve_scrollbar_style(Some(&width), None, Some(&webkit_style));

        // Width should be from standard, colors should be UA default (since standard color is auto)
        assert_eq!(resolved.width, Some(LayoutWidth::px(8.0)));
        assert_eq!(resolved.thumb_color, None);
        assert_eq!(resolved.track_color, None);
    }
}
