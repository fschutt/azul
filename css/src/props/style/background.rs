//! CSS properties for backgrounds, including colors, images, and gradients.

use alloc::{
    string::{String, ToString},
    vec::Vec,
};
use core::fmt;

#[cfg(feature = "parser")]
use crate::props::basic::{
    error::{InvalidValueErr, InvalidValueErrOwned},
    parse::{
        parse_parentheses, parse_image, split_string_respect_comma,
        CssImageParseError, CssImageParseErrorOwned,
        ParenthesisParseError, ParenthesisParseErrorOwned,
    },
    color::parse_color_or_system,
};
use crate::{
    corety::AzString,
    codegen::format::GetHash,
    props::{
        basic::{
            angle::{
                parse_angle_value, AngleValue, CssAngleValueParseError,
                CssAngleValueParseErrorOwned, OptionAngleValue,
            },
            color::{ColorU, ColorOrSystem, SystemColorRef, CssColorParseError, CssColorParseErrorOwned},
            direction::{
                parse_direction, CssDirectionParseError, CssDirectionParseErrorOwned, Direction,
            },
            length::{
                parse_percentage_value, OptionPercentageValue, PercentageParseError,
                PercentageParseErrorOwned, PercentageValue,
            },
            pixel::{
                parse_pixel_value, CssPixelValueParseError, CssPixelValueParseErrorOwned,
                PixelValue,
            },
        },
        formatter::PrintAsCssValue,
    },
};

// --- TYPE DEFINITIONS ---

/// Whether a `gradient` should be repeated or clamped to the edges.
#[derive(Debug, Copy, Clone, PartialEq, Ord, PartialOrd, Eq, Hash)]
#[repr(C)]
#[derive(Default)]
pub enum ExtendMode {
    #[default]
    Clamp,
    Repeat,
}

// -- Main Background Content Type --

/// A single CSS background layer: a solid color, image URL, or gradient.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
pub enum StyleBackgroundContent {
    LinearGradient(LinearGradient),
    RadialGradient(RadialGradient),
    ConicGradient(ConicGradient),
    Image(AzString),
    Color(ColorU),
    /// A theme-aware system color (e.g. `background: system:accent`), kept unresolved
    /// and resolved at render time. Mirrors the `ColorOrSystem::System` value that
    /// gradient color stops already accept.
    SystemColor(SystemColorRef),
}

impl_option!(
    StyleBackgroundContent,
    OptionStyleBackgroundContent,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

impl_vec!(StyleBackgroundContent, StyleBackgroundContentVec, StyleBackgroundContentVecDestructor, StyleBackgroundContentVecDestructorType, StyleBackgroundContentVecSlice, OptionStyleBackgroundContent);
impl_vec_debug!(StyleBackgroundContent, StyleBackgroundContentVec);
impl_vec_partialord!(StyleBackgroundContent, StyleBackgroundContentVec);
impl_vec_ord!(StyleBackgroundContent, StyleBackgroundContentVec);
impl_vec_clone!(
    StyleBackgroundContent,
    StyleBackgroundContentVec,
    StyleBackgroundContentVecDestructor
);
impl_vec_partialeq!(StyleBackgroundContent, StyleBackgroundContentVec);
impl_vec_eq!(StyleBackgroundContent, StyleBackgroundContentVec);
impl_vec_hash!(StyleBackgroundContent, StyleBackgroundContentVec);

impl Default for StyleBackgroundContent {
    fn default() -> Self {
        Self::Color(ColorU::TRANSPARENT)
    }
}

impl PrintAsCssValue for StyleBackgroundContent {
    fn print_as_css_value(&self) -> String {
        match self {
            Self::LinearGradient(lg) => {
                let prefix = if lg.extend_mode == ExtendMode::Repeat {
                    "repeating-linear-gradient"
                } else {
                    "linear-gradient"
                };
                format!("{}({})", prefix, lg.print_as_css_value())
            }
            Self::RadialGradient(rg) => {
                let prefix = if rg.extend_mode == ExtendMode::Repeat {
                    "repeating-radial-gradient"
                } else {
                    "radial-gradient"
                };
                format!("{}({})", prefix, rg.print_as_css_value())
            }
            Self::ConicGradient(cg) => {
                let prefix = if cg.extend_mode == ExtendMode::Repeat {
                    "repeating-conic-gradient"
                } else {
                    "conic-gradient"
                };
                format!("{}({})", prefix, cg.print_as_css_value())
            }
            Self::Image(id) => format!("url(\"{}\")", id.as_str()),
            Self::Color(c) => c.to_hash(),
            Self::SystemColor(s) => s.as_css_str().to_string(),
        }
    }
}

// Formatting to Rust code for background-related vecs

impl crate::codegen::format::FormatAsRustCode for StyleBackgroundContent {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        // Delegate to the CSS value representation for single backgrounds
        format!("StyleBackgroundContent::from_css(\"{}\")", self.print_as_css_value())
    }
}

impl crate::codegen::format::FormatAsRustCode for StyleBackgroundSizeVec {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!(
            "StyleBackgroundSizeVec::from_const_slice(STYLE_BACKGROUND_SIZE_{}_ITEMS)",
            self.get_hash()
        )
    }
}

impl crate::codegen::format::FormatAsRustCode for StyleBackgroundRepeatVec {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!(
            "StyleBackgroundRepeatVec::from_const_slice(STYLE_BACKGROUND_REPEAT_{}_ITEMS)",
            self.get_hash()
        )
    }
}

impl crate::codegen::format::FormatAsRustCode for StyleBackgroundContentVec {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!(
            "StyleBackgroundContentVec::from_const_slice(STYLE_BACKGROUND_CONTENT_{}_ITEMS)",
            self.get_hash()
        )
    }
}

impl PrintAsCssValue for StyleBackgroundContentVec {
    fn print_as_css_value(&self) -> String {
        self.as_ref()
            .iter()
            .map(PrintAsCssValue::print_as_css_value)
            .collect::<Vec<_>>()
            .join(", ")
    }
}

// -- Gradient Types --

/// A CSS `linear-gradient()` or `repeating-linear-gradient()` value.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct LinearGradient {
    pub direction: Direction,
    pub extend_mode: ExtendMode,
    pub stops: NormalizedLinearColorStopVec,
}
impl Default for LinearGradient {
    fn default() -> Self {
        Self {
            direction: Direction::default(),
            extend_mode: ExtendMode::default(),
            stops: Vec::new().into(),
        }
    }
}
impl PrintAsCssValue for LinearGradient {
    fn print_as_css_value(&self) -> String {
        let dir_str = self.direction.print_as_css_value();
        let stops_str = self
            .stops
            .iter()
            .map(PrintAsCssValue::print_as_css_value)
            .collect::<Vec<_>>()
            .join(", ");
        if stops_str.is_empty() {
            dir_str
        } else {
            format!("{dir_str}, {stops_str}")
        }
    }
}

/// A CSS `radial-gradient()` or `repeating-radial-gradient()` value.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct RadialGradient {
    pub shape: Shape,
    pub size: RadialGradientSize,
    pub position: StyleBackgroundPosition,
    pub extend_mode: ExtendMode,
    pub stops: NormalizedLinearColorStopVec,
}
impl Default for RadialGradient {
    fn default() -> Self {
        Self {
            shape: Shape::default(),
            size: RadialGradientSize::default(),
            position: StyleBackgroundPosition::default(),
            extend_mode: ExtendMode::default(),
            stops: Vec::new().into(),
        }
    }
}
impl PrintAsCssValue for RadialGradient {
    fn print_as_css_value(&self) -> String {
        let stops_str = self
            .stops
            .iter()
            .map(PrintAsCssValue::print_as_css_value)
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "{} {} at {}, {}",
            self.shape,
            self.size,
            self.position.print_as_css_value(),
            stops_str
        )
    }
}

/// A CSS `conic-gradient()` or `repeating-conic-gradient()` value.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct ConicGradient {
    pub extend_mode: ExtendMode,
    pub center: StyleBackgroundPosition,
    pub angle: AngleValue,
    pub stops: NormalizedRadialColorStopVec,
}
impl Default for ConicGradient {
    fn default() -> Self {
        Self {
            extend_mode: ExtendMode::default(),
            // CSS default for conic-gradient is `at center` (50% 50%), NOT the generic
            // Left/Top of StyleBackgroundPosition::default() — a corner-anchored cone maps
            // the whole element into one angular slice and renders a flat color.
            center: StyleBackgroundPosition {
                horizontal: BackgroundPositionHorizontal::Center,
                vertical: BackgroundPositionVertical::Center,
            },
            angle: AngleValue::default(),
            stops: Vec::new().into(),
        }
    }
}
impl PrintAsCssValue for ConicGradient {
    fn print_as_css_value(&self) -> String {
        let stops_str = self
            .stops
            .iter()
            .map(PrintAsCssValue::print_as_css_value)
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "from {} at {}, {}",
            self.angle,
            self.center.print_as_css_value(),
            stops_str
        )
    }
}

// -- Gradient Sub-types --

/// The shape of a radial gradient: `circle` or `ellipse`.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
#[derive(Default)]
pub enum Shape {
    #[default]
    Ellipse,
    Circle,
}
impl fmt::Display for Shape {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Ellipse => "ellipse",
                Self::Circle => "circle",
            }
        )
    }
}

/// The sizing keyword for a radial gradient (e.g. `closest-side`, `farthest-corner`).
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
#[derive(Default)]
pub enum RadialGradientSize {
    ClosestSide,
    ClosestCorner,
    FarthestSide,
    #[default]
    FarthestCorner,
}
impl fmt::Display for RadialGradientSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::ClosestSide => "closest-side",
                Self::ClosestCorner => "closest-corner",
                Self::FarthestSide => "farthest-side",
                Self::FarthestCorner => "farthest-corner",
            }
        )
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct NormalizedLinearColorStop {
    pub offset: PercentageValue,
    /// Color for this gradient stop. Can be a concrete color or a system color reference.
    pub color: ColorOrSystem,
}

impl NormalizedLinearColorStop {
    /// Create a new normalized linear color stop with a concrete color.
    #[must_use] pub const fn new(offset: PercentageValue, color: ColorU) -> Self {
        Self { offset, color: ColorOrSystem::color(color) }
    }

    /// Resolve the color against system colors.
    #[must_use] pub fn resolve(&self, system_colors: &crate::system::SystemColors, fallback: ColorU) -> ColorU {
        self.color.resolve(system_colors, fallback)
    }
}

impl_option!(
    NormalizedLinearColorStop,
    OptionNormalizedLinearColorStop,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
impl_vec!(NormalizedLinearColorStop, NormalizedLinearColorStopVec, NormalizedLinearColorStopVecDestructor, NormalizedLinearColorStopVecDestructorType, NormalizedLinearColorStopVecSlice, OptionNormalizedLinearColorStop);
impl_vec_debug!(NormalizedLinearColorStop, NormalizedLinearColorStopVec);
impl_vec_partialord!(NormalizedLinearColorStop, NormalizedLinearColorStopVec);
impl_vec_ord!(NormalizedLinearColorStop, NormalizedLinearColorStopVec);
impl_vec_clone!(
    NormalizedLinearColorStop,
    NormalizedLinearColorStopVec,
    NormalizedLinearColorStopVecDestructor
);
impl_vec_partialeq!(NormalizedLinearColorStop, NormalizedLinearColorStopVec);
impl_vec_eq!(NormalizedLinearColorStop, NormalizedLinearColorStopVec);
impl_vec_hash!(NormalizedLinearColorStop, NormalizedLinearColorStopVec);
impl PrintAsCssValue for NormalizedLinearColorStop {
    fn print_as_css_value(&self) -> String {
        match &self.color {
            ColorOrSystem::Color(c) => format!("{} {}", c.to_hash(), self.offset),
            ColorOrSystem::System(s) => format!("{} {}", s.as_css_str(), self.offset),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct NormalizedRadialColorStop {
    pub angle: AngleValue,
    /// Color for this gradient stop. Can be a concrete color or a system color reference.
    pub color: ColorOrSystem,
}

impl NormalizedRadialColorStop {
    /// Create a new normalized radial color stop with a concrete color.
    #[must_use] pub const fn new(angle: AngleValue, color: ColorU) -> Self {
        Self { angle, color: ColorOrSystem::color(color) }
    }

    /// Resolve the color against system colors.
    #[must_use] pub fn resolve(&self, system_colors: &crate::system::SystemColors, fallback: ColorU) -> ColorU {
        self.color.resolve(system_colors, fallback)
    }
}

impl_option!(
    NormalizedRadialColorStop,
    OptionNormalizedRadialColorStop,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
impl_vec!(NormalizedRadialColorStop, NormalizedRadialColorStopVec, NormalizedRadialColorStopVecDestructor, NormalizedRadialColorStopVecDestructorType, NormalizedRadialColorStopVecSlice, OptionNormalizedRadialColorStop);
impl_vec_debug!(NormalizedRadialColorStop, NormalizedRadialColorStopVec);
impl_vec_partialord!(NormalizedRadialColorStop, NormalizedRadialColorStopVec);
impl_vec_ord!(NormalizedRadialColorStop, NormalizedRadialColorStopVec);
impl_vec_clone!(
    NormalizedRadialColorStop,
    NormalizedRadialColorStopVec,
    NormalizedRadialColorStopVecDestructor
);
impl_vec_partialeq!(NormalizedRadialColorStop, NormalizedRadialColorStopVec);
impl_vec_eq!(NormalizedRadialColorStop, NormalizedRadialColorStopVec);
impl_vec_hash!(NormalizedRadialColorStop, NormalizedRadialColorStopVec);
impl PrintAsCssValue for NormalizedRadialColorStop {
    fn print_as_css_value(&self) -> String {
        match &self.color {
            ColorOrSystem::Color(c) => format!("{} {}", c.to_hash(), self.angle),
            ColorOrSystem::System(s) => format!("{} {}", s.as_css_str(), self.angle),
        }
    }
}

/// Transient struct for parsing linear color stops before normalization.
///
/// Per W3C CSS Images Level 3, a color stop can have 0, 1, or 2 positions:
/// - `red` (no position)
/// - `red 50%` (one position)
/// - `red 10% 30%` (two positions - creates two stops at same color)
/// 
/// Supports system colors like `system:accent` for theme-aware gradients.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LinearColorStop {
    pub color: ColorOrSystem,
    /// First position (optional)
    pub offset1: OptionPercentageValue,
    /// Second position (optional, only valid if offset1 is Some)
    /// When present, creates two color stops at the same color.
    pub offset2: OptionPercentageValue,
}

/// Transient struct for parsing radial/conic color stops before normalization.
///
/// Per W3C CSS Images Level 3, a color stop can have 0, 1, or 2 positions:
/// - `red` (no position)
/// - `red 90deg` (one position)
/// - `red 45deg 90deg` (two positions - creates two stops at same color)
/// 
/// Supports system colors like `system:accent` for theme-aware gradients.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RadialColorStop {
    pub color: ColorOrSystem,
    /// First position (optional)
    pub offset1: OptionAngleValue,
    /// Second position (optional, only valid if offset1 is Some)
    /// When present, creates two color stops at the same color.
    pub offset2: OptionAngleValue,
}

// -- Other Background Properties --

/// The `background-position` property (horizontal + vertical components).
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleBackgroundPosition {
    pub horizontal: BackgroundPositionHorizontal,
    pub vertical: BackgroundPositionVertical,
}

impl_option!(
    StyleBackgroundPosition,
    OptionStyleBackgroundPosition,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
impl_vec!(StyleBackgroundPosition, StyleBackgroundPositionVec, StyleBackgroundPositionVecDestructor, StyleBackgroundPositionVecDestructorType, StyleBackgroundPositionVecSlice, OptionStyleBackgroundPosition);
impl_vec_debug!(StyleBackgroundPosition, StyleBackgroundPositionVec);
impl_vec_partialord!(StyleBackgroundPosition, StyleBackgroundPositionVec);
impl_vec_ord!(StyleBackgroundPosition, StyleBackgroundPositionVec);
impl_vec_clone!(
    StyleBackgroundPosition,
    StyleBackgroundPositionVec,
    StyleBackgroundPositionVecDestructor
);
impl_vec_partialeq!(StyleBackgroundPosition, StyleBackgroundPositionVec);
impl_vec_eq!(StyleBackgroundPosition, StyleBackgroundPositionVec);
impl_vec_hash!(StyleBackgroundPosition, StyleBackgroundPositionVec);
impl Default for StyleBackgroundPosition {
    fn default() -> Self {
        Self {
            horizontal: BackgroundPositionHorizontal::Left,
            vertical: BackgroundPositionVertical::Top,
        }
    }
}

impl StyleBackgroundPosition {
    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        self.horizontal.scale_for_dpi(scale_factor);
        self.vertical.scale_for_dpi(scale_factor);
    }
}

impl PrintAsCssValue for StyleBackgroundPosition {
    fn print_as_css_value(&self) -> String {
        format!(
            "{} {}",
            self.horizontal.print_as_css_value(),
            self.vertical.print_as_css_value()
        )
    }
}
impl PrintAsCssValue for StyleBackgroundPositionVec {
    fn print_as_css_value(&self) -> String {
        self.iter()
            .map(PrintAsCssValue::print_as_css_value)
            .collect::<Vec<_>>()
            .join(", ")
    }
}

// Formatting to Rust code for StyleBackgroundPositionVec
impl crate::codegen::format::FormatAsRustCode for StyleBackgroundPositionVec {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!(
            "StyleBackgroundPositionVec::from_const_slice(STYLE_BACKGROUND_POSITION_{}_ITEMS)",
            self.get_hash()
        )
    }
}
#[allow(variant_size_differences)] // repr(C,u8) FFI enum: boxing the large variant would change the C ABI (api.json bindings); size disparity accepted
/// Horizontal component of `background-position`: a keyword or exact pixel value.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
pub enum BackgroundPositionHorizontal {
    Left,
    Center,
    Right,
    Exact(PixelValue),
}

impl BackgroundPositionHorizontal {
    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        if let Self::Exact(s) = self {
            s.scale_for_dpi(scale_factor);
        }
    }
}

impl PrintAsCssValue for BackgroundPositionHorizontal {
    fn print_as_css_value(&self) -> String {
        match self {
            Self::Left => "left".to_string(),
            Self::Center => "center".to_string(),
            Self::Right => "right".to_string(),
            Self::Exact(px) => px.print_as_css_value(),
        }
    }
}
#[allow(variant_size_differences)] // repr(C,u8) FFI enum: boxing the large variant would change the C ABI (api.json bindings); size disparity accepted
/// Vertical component of `background-position`: a keyword or exact pixel value.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
pub enum BackgroundPositionVertical {
    Top,
    Center,
    Bottom,
    Exact(PixelValue),
}

impl BackgroundPositionVertical {
    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        if let Self::Exact(s) = self {
            s.scale_for_dpi(scale_factor);
        }
    }
}

impl PrintAsCssValue for BackgroundPositionVertical {
    fn print_as_css_value(&self) -> String {
        match self {
            Self::Top => "top".to_string(),
            Self::Center => "center".to_string(),
            Self::Bottom => "bottom".to_string(),
            Self::Exact(px) => px.print_as_css_value(),
        }
    }
}
#[allow(variant_size_differences)] // repr(C,u8) FFI enum: boxing the large variant would change the C ABI (api.json bindings); size disparity accepted
/// The `background-size` property: `contain`, `cover`, or an exact size.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
#[derive(Default)]
pub enum StyleBackgroundSize {
    ExactSize(PixelValueSize),
    #[default]
    Contain,
    Cover,
}

impl_option!(
    StyleBackgroundSize,
    OptionStyleBackgroundSize,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

/// Two-dimensional size in `PixelValue` units (width, height)
/// Used for background-size and similar properties
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct PixelValueSize {
    pub width: PixelValue,
    pub height: PixelValue,
}

impl_vec!(StyleBackgroundSize, StyleBackgroundSizeVec, StyleBackgroundSizeVecDestructor, StyleBackgroundSizeVecDestructorType, StyleBackgroundSizeVecSlice, OptionStyleBackgroundSize);
impl_vec_debug!(StyleBackgroundSize, StyleBackgroundSizeVec);
impl_vec_partialord!(StyleBackgroundSize, StyleBackgroundSizeVec);
impl_vec_ord!(StyleBackgroundSize, StyleBackgroundSizeVec);
impl_vec_clone!(
    StyleBackgroundSize,
    StyleBackgroundSizeVec,
    StyleBackgroundSizeVecDestructor
);
impl_vec_partialeq!(StyleBackgroundSize, StyleBackgroundSizeVec);
impl_vec_eq!(StyleBackgroundSize, StyleBackgroundSizeVec);
impl_vec_hash!(StyleBackgroundSize, StyleBackgroundSizeVec);

impl StyleBackgroundSize {
    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        if let Self::ExactSize(size) = self {
            size.width.scale_for_dpi(scale_factor);
            size.height.scale_for_dpi(scale_factor);
        }
    }
}

impl PrintAsCssValue for StyleBackgroundSize {
    fn print_as_css_value(&self) -> String {
        match self {
            Self::Contain => "contain".to_string(),
            Self::Cover => "cover".to_string(),
            Self::ExactSize(size) => {
                format!(
                    "{} {}",
                    size.width.print_as_css_value(),
                    size.height.print_as_css_value()
                )
            }
        }
    }
}
impl PrintAsCssValue for StyleBackgroundSizeVec {
    fn print_as_css_value(&self) -> String {
        self.iter()
            .map(PrintAsCssValue::print_as_css_value)
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// The `background-repeat` property.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
#[derive(Default)]
pub enum StyleBackgroundRepeat {
    NoRepeat,
    #[default]
    PatternRepeat,
    RepeatX,
    RepeatY,
}

impl_option!(
    StyleBackgroundRepeat,
    OptionStyleBackgroundRepeat,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
impl_vec!(StyleBackgroundRepeat, StyleBackgroundRepeatVec, StyleBackgroundRepeatVecDestructor, StyleBackgroundRepeatVecDestructorType, StyleBackgroundRepeatVecSlice, OptionStyleBackgroundRepeat);
impl_vec_debug!(StyleBackgroundRepeat, StyleBackgroundRepeatVec);
impl_vec_partialord!(StyleBackgroundRepeat, StyleBackgroundRepeatVec);
impl_vec_ord!(StyleBackgroundRepeat, StyleBackgroundRepeatVec);
impl_vec_clone!(
    StyleBackgroundRepeat,
    StyleBackgroundRepeatVec,
    StyleBackgroundRepeatVecDestructor
);
impl_vec_partialeq!(StyleBackgroundRepeat, StyleBackgroundRepeatVec);
impl_vec_eq!(StyleBackgroundRepeat, StyleBackgroundRepeatVec);
impl_vec_hash!(StyleBackgroundRepeat, StyleBackgroundRepeatVec);
impl PrintAsCssValue for StyleBackgroundRepeat {
    fn print_as_css_value(&self) -> String {
        match self {
            Self::NoRepeat => "no-repeat".to_string(),
            Self::PatternRepeat => "repeat".to_string(),
            Self::RepeatX => "repeat-x".to_string(),
            Self::RepeatY => "repeat-y".to_string(),
        }
    }
}
impl PrintAsCssValue for StyleBackgroundRepeatVec {
    fn print_as_css_value(&self) -> String {
        self.iter()
            .map(PrintAsCssValue::print_as_css_value)
            .collect::<Vec<_>>()
            .join(", ")
    }
}

// --- ERROR DEFINITIONS ---

#[derive(Clone, PartialEq)]
pub enum CssBackgroundParseError<'a> {
    Error(&'a str),
    InvalidBackground(ParenthesisParseError<'a>),
    UnclosedGradient(&'a str),
    NoDirection(&'a str),
    TooFewGradientStops(&'a str),
    DirectionParseError(CssDirectionParseError<'a>),
    GradientParseError(CssGradientStopParseError<'a>),
    ConicGradient(CssConicGradientParseError<'a>),
    ShapeParseError(CssShapeParseError<'a>),
    ImageParseError(CssImageParseError<'a>),
    ColorParseError(CssColorParseError<'a>),
}

impl_debug_as_display!(CssBackgroundParseError<'a>);
impl_display! { CssBackgroundParseError<'a>, {
    Error(e) => e,
    InvalidBackground(val) => format!("Invalid background value: \"{}\"", val),
    UnclosedGradient(val) => format!("Unclosed gradient: \"{}\"", val),
    NoDirection(val) => format!("Gradient has no direction: \"{}\"", val),
    TooFewGradientStops(val) => format!("Failed to parse gradient due to too few gradient steps: \"{}\"", val),
    DirectionParseError(e) => format!("Failed to parse gradient direction: \"{}\"", e),
    GradientParseError(e) => format!("Failed to parse gradient: {}", e),
    ConicGradient(e) => format!("Failed to parse conic gradient: {}", e),
    ShapeParseError(e) => format!("Failed to parse shape of radial gradient: {}", e),
    ImageParseError(e) => format!("Failed to parse image() value: {}", e),
    ColorParseError(e) => format!("Failed to parse color value: {}", e),
}}

#[cfg(feature = "parser")]
impl_from!(
    ParenthesisParseError<'a>,
    CssBackgroundParseError::InvalidBackground
);
#[cfg(feature = "parser")]
impl_from!(
    CssDirectionParseError<'a>,
    CssBackgroundParseError::DirectionParseError
);
#[cfg(feature = "parser")]
impl_from!(
    CssGradientStopParseError<'a>,
    CssBackgroundParseError::GradientParseError
);
#[cfg(feature = "parser")]
impl_from!(
    CssShapeParseError<'a>,
    CssBackgroundParseError::ShapeParseError
);
#[cfg(feature = "parser")]
impl_from!(
    CssImageParseError<'a>,
    CssBackgroundParseError::ImageParseError
);
#[cfg(feature = "parser")]
impl_from!(
    CssColorParseError<'a>,
    CssBackgroundParseError::ColorParseError
);
#[cfg(feature = "parser")]
impl_from!(
    CssConicGradientParseError<'a>,
    CssBackgroundParseError::ConicGradient
);

#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum CssBackgroundParseErrorOwned {
    Error(AzString),
    InvalidBackground(ParenthesisParseErrorOwned),
    UnclosedGradient(AzString),
    NoDirection(AzString),
    TooFewGradientStops(AzString),
    DirectionParseError(CssDirectionParseErrorOwned),
    GradientParseError(CssGradientStopParseErrorOwned),
    ConicGradient(CssConicGradientParseErrorOwned),
    ShapeParseError(CssShapeParseErrorOwned),
    ImageParseError(CssImageParseErrorOwned),
    ColorParseError(CssColorParseErrorOwned),
}

impl CssBackgroundParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> CssBackgroundParseErrorOwned {
        match self {
            Self::Error(s) => CssBackgroundParseErrorOwned::Error((*s).to_string().into()),
            Self::InvalidBackground(e) => {
                CssBackgroundParseErrorOwned::InvalidBackground(e.to_contained())
            }
            Self::UnclosedGradient(s) => {
                CssBackgroundParseErrorOwned::UnclosedGradient((*s).to_string().into())
            }
            Self::NoDirection(s) => CssBackgroundParseErrorOwned::NoDirection((*s).to_string().into()),
            Self::TooFewGradientStops(s) => {
                CssBackgroundParseErrorOwned::TooFewGradientStops((*s).to_string().into())
            }
            Self::DirectionParseError(e) => {
                CssBackgroundParseErrorOwned::DirectionParseError(e.to_contained())
            }
            Self::GradientParseError(e) => {
                CssBackgroundParseErrorOwned::GradientParseError(e.to_contained())
            }
            Self::ConicGradient(e) => CssBackgroundParseErrorOwned::ConicGradient(e.to_contained()),
            Self::ShapeParseError(e) => {
                CssBackgroundParseErrorOwned::ShapeParseError(e.to_contained())
            }
            Self::ImageParseError(e) => {
                CssBackgroundParseErrorOwned::ImageParseError(e.to_contained())
            }
            Self::ColorParseError(e) => {
                CssBackgroundParseErrorOwned::ColorParseError(e.to_contained())
            }
        }
    }
}

impl CssBackgroundParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> CssBackgroundParseError<'_> {
        match self {
            Self::Error(s) => CssBackgroundParseError::Error(s),
            Self::InvalidBackground(e) => CssBackgroundParseError::InvalidBackground(e.to_shared()),
            Self::UnclosedGradient(s) => CssBackgroundParseError::UnclosedGradient(s),
            Self::NoDirection(s) => CssBackgroundParseError::NoDirection(s),
            Self::TooFewGradientStops(s) => CssBackgroundParseError::TooFewGradientStops(s),
            Self::DirectionParseError(e) => {
                CssBackgroundParseError::DirectionParseError(e.to_shared())
            }
            Self::GradientParseError(e) => {
                CssBackgroundParseError::GradientParseError(e.to_shared())
            }
            Self::ConicGradient(e) => CssBackgroundParseError::ConicGradient(e.to_shared()),
            Self::ShapeParseError(e) => CssBackgroundParseError::ShapeParseError(e.to_shared()),
            Self::ImageParseError(e) => CssBackgroundParseError::ImageParseError(e.to_shared()),
            Self::ColorParseError(e) => CssBackgroundParseError::ColorParseError(e.to_shared()),
        }
    }
}

#[derive(Clone, PartialEq)]
pub enum CssGradientStopParseError<'a> {
    Error(&'a str),
    Percentage(PercentageParseError),
    Angle(CssAngleValueParseError<'a>),
    ColorParseError(CssColorParseError<'a>),
}

impl_debug_as_display!(CssGradientStopParseError<'a>);
impl_display! { CssGradientStopParseError<'a>, {
    Error(e) => e,
    Percentage(e) => format!("Failed to parse offset percentage: {}", e),
    Angle(e) => format!("Failed to parse angle: {}", e),
    ColorParseError(e) => format!("{}", e),
}}
#[cfg(feature = "parser")]
impl_from!(
    CssColorParseError<'a>,
    CssGradientStopParseError::ColorParseError
);

#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum CssGradientStopParseErrorOwned {
    Error(AzString),
    Percentage(PercentageParseErrorOwned),
    Angle(CssAngleValueParseErrorOwned),
    ColorParseError(CssColorParseErrorOwned),
}

impl CssGradientStopParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> CssGradientStopParseErrorOwned {
        match self {
            Self::Error(s) => CssGradientStopParseErrorOwned::Error((*s).to_string().into()),
            Self::Percentage(e) => CssGradientStopParseErrorOwned::Percentage(e.to_contained()),
            Self::Angle(e) => CssGradientStopParseErrorOwned::Angle(e.to_contained()),
            Self::ColorParseError(e) => {
                CssGradientStopParseErrorOwned::ColorParseError(e.to_contained())
            }
        }
    }
}

impl CssGradientStopParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> CssGradientStopParseError<'_> {
        match self {
            Self::Error(s) => CssGradientStopParseError::Error(s),
            Self::Percentage(e) => CssGradientStopParseError::Percentage(e.to_shared()),
            Self::Angle(e) => CssGradientStopParseError::Angle(e.to_shared()),
            Self::ColorParseError(e) => CssGradientStopParseError::ColorParseError(e.to_shared()),
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub enum CssConicGradientParseError<'a> {
    Position(CssBackgroundPositionParseError<'a>),
    Angle(CssAngleValueParseError<'a>),
    NoAngle(&'a str),
}
impl_debug_as_display!(CssConicGradientParseError<'a>);
impl_display! { CssConicGradientParseError<'a>, {
    Position(val) => format!("Invalid position attribute: \"{}\"", val),
    Angle(val) => format!("Invalid angle value: \"{}\"", val),
    NoAngle(val) => format!("Expected angle: \"{}\"", val),
}}
#[cfg(feature = "parser")]
impl_from!(
    CssAngleValueParseError<'a>,
    CssConicGradientParseError::Angle
);
#[cfg(feature = "parser")]
impl_from!(
    CssBackgroundPositionParseError<'a>,
    CssConicGradientParseError::Position
);

#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum CssConicGradientParseErrorOwned {
    Position(CssBackgroundPositionParseErrorOwned),
    Angle(CssAngleValueParseErrorOwned),
    NoAngle(AzString),
}
impl CssConicGradientParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> CssConicGradientParseErrorOwned {
        match self {
            Self::Position(e) => CssConicGradientParseErrorOwned::Position(e.to_contained()),
            Self::Angle(e) => CssConicGradientParseErrorOwned::Angle(e.to_contained()),
            Self::NoAngle(s) => CssConicGradientParseErrorOwned::NoAngle((*s).to_string().into()),
        }
    }
}
impl CssConicGradientParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> CssConicGradientParseError<'_> {
        match self {
            Self::Position(e) => CssConicGradientParseError::Position(e.to_shared()),
            Self::Angle(e) => CssConicGradientParseError::Angle(e.to_shared()),
            Self::NoAngle(s) => CssConicGradientParseError::NoAngle(s),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum CssShapeParseError<'a> {
    ShapeErr(InvalidValueErr<'a>),
}
impl_display! {CssShapeParseError<'a>, {
    ShapeErr(e) => format!("\"{}\"", e.0),
}}
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum CssShapeParseErrorOwned {
    ShapeErr(InvalidValueErrOwned),
}
impl CssShapeParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> CssShapeParseErrorOwned {
        match self {
            Self::ShapeErr(err) => CssShapeParseErrorOwned::ShapeErr(err.to_contained()),
        }
    }
}
impl CssShapeParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> CssShapeParseError<'_> {
        match self {
            Self::ShapeErr(err) => CssShapeParseError::ShapeErr(err.to_shared()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CssBackgroundPositionParseError<'a> {
    NoPosition(&'a str),
    TooManyComponents(&'a str),
    FirstComponentWrong(CssPixelValueParseError<'a>),
    SecondComponentWrong(CssPixelValueParseError<'a>),
}

impl_display! {CssBackgroundPositionParseError<'a>, {
    NoPosition(e) => format!("First background position missing: \"{}\"", e),
    TooManyComponents(e) => format!("background-position can only have one or two components, not more: \"{}\"", e),
    FirstComponentWrong(e) => format!("Failed to parse first component: \"{}\"", e),
    SecondComponentWrong(e) => format!("Failed to parse second component: \"{}\"", e),
}}
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum CssBackgroundPositionParseErrorOwned {
    NoPosition(AzString),
    TooManyComponents(AzString),
    FirstComponentWrong(CssPixelValueParseErrorOwned),
    SecondComponentWrong(CssPixelValueParseErrorOwned),
}
impl CssBackgroundPositionParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> CssBackgroundPositionParseErrorOwned {
        match self {
            Self::NoPosition(s) => CssBackgroundPositionParseErrorOwned::NoPosition((*s).to_string().into()),
            Self::TooManyComponents(s) => {
                CssBackgroundPositionParseErrorOwned::TooManyComponents((*s).to_string().into())
            }
            Self::FirstComponentWrong(e) => {
                CssBackgroundPositionParseErrorOwned::FirstComponentWrong(e.to_contained())
            }
            Self::SecondComponentWrong(e) => {
                CssBackgroundPositionParseErrorOwned::SecondComponentWrong(e.to_contained())
            }
        }
    }
}
impl CssBackgroundPositionParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> CssBackgroundPositionParseError<'_> {
        match self {
            Self::NoPosition(s) => CssBackgroundPositionParseError::NoPosition(s),
            Self::TooManyComponents(s) => CssBackgroundPositionParseError::TooManyComponents(s),
            Self::FirstComponentWrong(e) => {
                CssBackgroundPositionParseError::FirstComponentWrong(e.to_shared())
            }
            Self::SecondComponentWrong(e) => {
                CssBackgroundPositionParseError::SecondComponentWrong(e.to_shared())
            }
        }
    }
}

// --- PARSERS ---

#[cfg(feature = "parser")]
pub mod parser {
    #[allow(clippy::wildcard_imports)] // parser submodule reuses the parent module's value types
    use super::*;

    // the `*Gradient` suffix mirrors the CSS gradient function names this enum
    // parses (linear-gradient, radial-gradient, conic-gradient, …).
    #[allow(clippy::enum_variant_names)]
    #[derive(Debug, Copy, Clone, PartialEq, Eq)]
    enum GradientType {
        LinearGradient,
        RepeatingLinearGradient,
        RadialGradient,
        RepeatingRadialGradient,
        ConicGradient,
        RepeatingConicGradient,
    }

    impl GradientType {
        pub(crate) const fn get_extend_mode(self) -> ExtendMode {
            match self {
                Self::LinearGradient | Self::RadialGradient | Self::ConicGradient => {
                    ExtendMode::Clamp
                }
                Self::RepeatingLinearGradient
                | Self::RepeatingRadialGradient
                | Self::RepeatingConicGradient => ExtendMode::Repeat,
            }
        }
    }

    // -- Top-level Parsers for background-* properties --

    /// Parses multiple backgrounds, such as "linear-gradient(red, green), url(image.png)".
    /// # Errors
    ///
    /// Returns an error if `input` is not a valid CSS `background-content-multiple` value.
    pub fn parse_style_background_content_multiple(
        input: &str,
    ) -> Result<StyleBackgroundContentVec, CssBackgroundParseError<'_>> {
        Ok(split_string_respect_comma(input)
            .iter()
            .map(|i| parse_style_background_content(i))
            .collect::<Result<Vec<_>, _>>()?
            .into())
    }

    /// Parses a single background value, which can be a color, image, or gradient.
    /// # Errors
    ///
    /// Returns an error if `input` is not a valid CSS `background-content` value.
    pub fn parse_style_background_content(
        input: &str,
    ) -> Result<StyleBackgroundContent, CssBackgroundParseError<'_>> {
        match parse_parentheses(
            input,
            &[
                "linear-gradient",
                "repeating-linear-gradient",
                "radial-gradient",
                "repeating-radial-gradient",
                "conic-gradient",
                "repeating-conic-gradient",
                "image",
                "url",
            ],
        ) {
            Ok((background_type, brace_contents)) => {
                let gradient_type = match background_type {
                    "linear-gradient" => GradientType::LinearGradient,
                    "repeating-linear-gradient" => GradientType::RepeatingLinearGradient,
                    "radial-gradient" => GradientType::RadialGradient,
                    "repeating-radial-gradient" => GradientType::RepeatingRadialGradient,
                    "conic-gradient" => GradientType::ConicGradient,
                    "repeating-conic-gradient" => GradientType::RepeatingConicGradient,
                    "image" | "url" => {
                        return Ok(StyleBackgroundContent::Image(
                            parse_image(brace_contents)?,
                        ))
                    }
                    _ => unreachable!(),
                };
                parse_gradient(brace_contents, gradient_type)
            }
            // A bare `background:` value is a color. Accept system colors here too
            // (`system:accent`, `system:text`, ...), matching the gradient color stops
            // (which use `parse_color_or_system`). System colors stay unresolved and are
            // theme-resolved at render time. `parse_color_or_system` is a superset of
            // `parse_css_color`, so ordinary colors keep parsing exactly as before.
            Err(_) => Ok(match parse_color_or_system(input)? {
                ColorOrSystem::Color(c) => StyleBackgroundContent::Color(c),
                ColorOrSystem::System(s) => StyleBackgroundContent::SystemColor(s),
            }),
        }
    }

    /// Parses multiple `background-position` values.
    /// # Errors
    ///
    /// Returns an error if `input` is not a valid CSS `background-position-multiple` value.
    pub fn parse_style_background_position_multiple(
        input: &str,
    ) -> Result<StyleBackgroundPositionVec, CssBackgroundPositionParseError<'_>> {
        Ok(split_string_respect_comma(input)
            .iter()
            .map(|i| parse_style_background_position(i))
            .collect::<Result<Vec<_>, _>>()?
            .into())
    }

    /// Parses a single `background-position` value.
    /// # Errors
    ///
    /// Returns an error if `input` is not a valid CSS `background-position` value.
    pub fn parse_style_background_position(
        input: &str,
    ) -> Result<StyleBackgroundPosition, CssBackgroundPositionParseError<'_>> {
        let input = input.trim();
        let mut whitespace_iter = input.split_whitespace();

        let first = whitespace_iter
            .next()
            .ok_or(CssBackgroundPositionParseError::NoPosition(input))?;
        let second = whitespace_iter.next();

        if whitespace_iter.next().is_some() {
            return Err(CssBackgroundPositionParseError::TooManyComponents(input));
        }

        // Try to parse as horizontal first, if that fails, maybe it's a vertical keyword
        if let Ok(horizontal) = parse_background_position_horizontal(first) {
            let vertical = match second {
                Some(s) => parse_background_position_vertical(s)
                    .map_err(CssBackgroundPositionParseError::SecondComponentWrong)?,
                None => BackgroundPositionVertical::Center,
            };
            return Ok(StyleBackgroundPosition {
                horizontal,
                vertical,
            });
        }

        // If the first part wasn't a horizontal keyword, maybe it's a vertical one
        if let Ok(vertical) = parse_background_position_vertical(first) {
            let horizontal = match second {
                Some(s) => parse_background_position_horizontal(s)
                    .map_err(CssBackgroundPositionParseError::FirstComponentWrong)?,
                None => BackgroundPositionHorizontal::Center,
            };
            return Ok(StyleBackgroundPosition {
                horizontal,
                vertical,
            });
        }

        Err(CssBackgroundPositionParseError::FirstComponentWrong(
            CssPixelValueParseError::InvalidPixelValue(first),
        ))
    }

    /// Parses multiple `background-size` values.
    /// # Errors
    ///
    /// Returns an error if `input` is not a valid CSS `background-size-multiple` value.
    pub fn parse_style_background_size_multiple(
        input: &str,
    ) -> Result<StyleBackgroundSizeVec, InvalidValueErr<'_>> {
        Ok(split_string_respect_comma(input)
            .iter()
            .map(|i| parse_style_background_size(i))
            .collect::<Result<Vec<_>, _>>()?
            .into())
    }

    /// Parses a single `background-size` value.
    /// # Errors
    ///
    /// Returns an error if `input` is not a valid CSS `background-size` value.
    pub fn parse_style_background_size(
        input: &str,
    ) -> Result<StyleBackgroundSize, InvalidValueErr<'_>> {
        let input = input.trim();
        match input {
            "contain" => Ok(StyleBackgroundSize::Contain),
            "cover" => Ok(StyleBackgroundSize::Cover),
            other => {
                let mut iter = other.split_whitespace();
                let x_val = iter.next().ok_or(InvalidValueErr(input))?;
                let x_pos = parse_pixel_value(x_val).map_err(|_| InvalidValueErr(input))?;
                let y_pos = match iter.next() {
                    Some(y_val) => parse_pixel_value(y_val).map_err(|_| InvalidValueErr(input))?,
                    None => x_pos, // If only one value, it applies to both width and height
                };
                Ok(StyleBackgroundSize::ExactSize(PixelValueSize {
                    width: x_pos,
                    height: y_pos,
                }))
            }
        }
    }

    /// Parses multiple `background-repeat` values.
    /// # Errors
    ///
    /// Returns an error if `input` is not a valid CSS `background-repeat-multiple` value.
    pub fn parse_style_background_repeat_multiple(
        input: &str,
    ) -> Result<StyleBackgroundRepeatVec, InvalidValueErr<'_>> {
        Ok(split_string_respect_comma(input)
            .iter()
            .map(|i| parse_style_background_repeat(i))
            .collect::<Result<Vec<_>, _>>()?
            .into())
    }

    /// Parses a single `background-repeat` value.
    /// # Errors
    ///
    /// Returns an error if `input` is not a valid CSS `background-repeat` value.
    pub fn parse_style_background_repeat(
        input: &str,
    ) -> Result<StyleBackgroundRepeat, InvalidValueErr<'_>> {
        match input.trim() {
            "no-repeat" => Ok(StyleBackgroundRepeat::NoRepeat),
            "repeat" => Ok(StyleBackgroundRepeat::PatternRepeat),
            "repeat-x" => Ok(StyleBackgroundRepeat::RepeatX),
            "repeat-y" => Ok(StyleBackgroundRepeat::RepeatY),
            _ => Err(InvalidValueErr(input)),
        }
    }

    // -- Gradient Parsing Logic --

    /// Parses the contents of a gradient function.
    fn parse_gradient(
        input: &str,
        gradient_type: GradientType,
    ) -> Result<StyleBackgroundContent, CssBackgroundParseError<'_>> {
        let input = input.trim();
        let comma_separated_items = split_string_respect_comma(input);
        let mut brace_iterator = comma_separated_items.iter();
        let first_brace_item = brace_iterator
            .next()
            .ok_or(CssBackgroundParseError::NoDirection(input))?;

        match gradient_type {
            GradientType::LinearGradient | GradientType::RepeatingLinearGradient => {
                let mut linear_gradient = LinearGradient {
                    extend_mode: gradient_type.get_extend_mode(),
                    ..Default::default()
                };
                let mut linear_stops = Vec::new();

                if let Ok(dir) = parse_direction(first_brace_item) {
                    linear_gradient.direction = dir;
                } else {
                    linear_stops.push(parse_linear_color_stop(first_brace_item)?);
                }

                for item in brace_iterator {
                    linear_stops.push(parse_linear_color_stop(item)?);
                }

                linear_gradient.stops = get_normalized_linear_stops(&linear_stops).into();
                Ok(StyleBackgroundContent::LinearGradient(linear_gradient))
            }
            GradientType::RadialGradient | GradientType::RepeatingRadialGradient => {
                // Simplified parsing: assumes shape/size/position come first, then stops.
                // A more robust parser would handle them in any order.
                let mut radial_gradient = RadialGradient {
                    extend_mode: gradient_type.get_extend_mode(),
                    ..Default::default()
                };
                let mut radial_stops = Vec::new();
                let mut current_item = *first_brace_item;
                let mut items_consumed = false;

                // Greedily consume shape, size, position keywords
                loop {
                    let mut consumed_in_iteration = false;
                    let mut temp_iter = current_item.split_whitespace();
                    for word in temp_iter {
                        if let Ok(shape) = parse_shape(word) {
                            radial_gradient.shape = shape;
                            consumed_in_iteration = true;
                        } else if let Ok(size) = parse_radial_gradient_size(word) {
                            radial_gradient.size = size;
                            consumed_in_iteration = true;
                        } else if let Ok(pos) = parse_style_background_position(current_item) {
                            radial_gradient.position = pos;
                            consumed_in_iteration = true;
                            break; // position can have multiple words, so consume the rest of the
                                   // item
                        }
                    }
                    if consumed_in_iteration {
                        if let Some(next_item) = brace_iterator.next() {
                            current_item = next_item;
                            items_consumed = true;
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                }

                if items_consumed || parse_linear_color_stop(current_item).is_ok() {
                    radial_stops.push(parse_linear_color_stop(current_item)?);
                }

                for item in brace_iterator {
                    radial_stops.push(parse_linear_color_stop(item)?);
                }

                radial_gradient.stops = get_normalized_linear_stops(&radial_stops).into();
                Ok(StyleBackgroundContent::RadialGradient(radial_gradient))
            }
            GradientType::ConicGradient | GradientType::RepeatingConicGradient => {
                let mut conic_gradient = ConicGradient {
                    extend_mode: gradient_type.get_extend_mode(),
                    ..Default::default()
                };
                let mut conic_stops = Vec::new();

                if let Some((angle, center)) = parse_conic_first_item(first_brace_item)? {
                    conic_gradient.angle = angle;
                    conic_gradient.center = center;
                } else {
                    conic_stops.push(parse_radial_color_stop(first_brace_item)?);
                }

                for item in brace_iterator {
                    conic_stops.push(parse_radial_color_stop(item)?);
                }

                conic_gradient.stops = get_normalized_radial_stops(&conic_stops).into();
                Ok(StyleBackgroundContent::ConicGradient(conic_gradient))
            }
        }
    }

    // -- Gradient Parsing Helpers --

    /// Parses color stops per W3C CSS Images Level 3:
    /// - "red" (no position)
    /// - "red 5%" (one position)
    /// - "red 10% 30%" (two positions - creates a hard color band)
    /// 
    /// Also supports system colors like `system:accent 50%` for theme-aware gradients.
    fn parse_linear_color_stop(
        input: &str,
    ) -> Result<LinearColorStop, CssGradientStopParseError<'_>> {
        let input = input.trim();
        let (color_str, offset1_str, offset2_str) = split_color_and_offsets(input);

        let color = parse_color_or_system(color_str)?;
        let offset1 = match offset1_str {
            None => OptionPercentageValue::None,
            Some(s) => OptionPercentageValue::Some(
                parse_percentage_value(s).map_err(CssGradientStopParseError::Percentage)?,
            ),
        };
        let offset2 = match offset2_str {
            None => OptionPercentageValue::None,
            Some(s) => OptionPercentageValue::Some(
                parse_percentage_value(s).map_err(CssGradientStopParseError::Percentage)?,
            ),
        };

        Ok(LinearColorStop {
            color,
            offset1,
            offset2,
        })
    }

    /// Parses color stops per W3C CSS Images Level 3:
    /// - "red" (no position)
    /// - "red 90deg" (one position)
    /// - "red 45deg 90deg" (two positions - creates a hard color band)
    /// 
    /// Also supports system colors like `system:accent 90deg` for theme-aware gradients.
    fn parse_radial_color_stop(
        input: &str,
    ) -> Result<RadialColorStop, CssGradientStopParseError<'_>> {
        let input = input.trim();
        let (color_str, offset1_str, offset2_str) = split_color_and_offsets(input);

        let color = parse_color_or_system(color_str)?;
        let offset1 = match offset1_str {
            None => OptionAngleValue::None,
            Some(s) => OptionAngleValue::Some(
                parse_angle_value(s).map_err(CssGradientStopParseError::Angle)?,
            ),
        };
        let offset2 = match offset2_str {
            None => OptionAngleValue::None,
            Some(s) => OptionAngleValue::Some(
                parse_angle_value(s).map_err(CssGradientStopParseError::Angle)?,
            ),
        };

        Ok(RadialColorStop {
            color,
            offset1,
            offset2,
        })
    }

    /// Helper to robustly split a string like "rgba(0,0,0,0.5) 10% 30%" into color and offset
    /// parts. Returns (`color_str`, offset1, offset2) where offsets are optional.
    ///
    /// Per W3C CSS Images Level 3, a color stop can have 0, 1, or 2 positions:
    /// - "red" -> ("red", None, None)
    /// - "red 50%" -> ("red", Some("50%"), None)
    /// - "red 10% 30%" -> ("red", Some("10%"), Some("30%"))
    fn split_color_and_offsets(input: &str) -> (&str, Option<&str>, Option<&str>) {
        // Strategy: scan from the end to find position values (contain digits + % or unit).
        // We need to handle complex colors like "rgba(0, 0, 0, 0.5)" that contain spaces and
        // digits.

        let input = input.trim();

        // Try to find the last position value (might be second of two)
        if let Some((remaining, last_offset)) = try_split_last_offset(input) {
            // Try to find another position value before it
            if let Some((color_part, first_offset)) = try_split_last_offset(remaining) {
                return (color_part.trim(), Some(first_offset), Some(last_offset));
            }
            return (remaining.trim(), Some(last_offset), None);
        }

        (input, None, None)
    }

    /// Try to split off the last whitespace-separated token if it looks like a position value.
    /// Returns (remaining, `offset_str`) if successful.
    fn try_split_last_offset(input: &str) -> Option<(&str, &str)> {
        let input = input.trim();
        if let Some(last_ws_idx) = input.rfind(char::is_whitespace) {
            let (potential_color, potential_offset) = input.split_at(last_ws_idx);
            let potential_offset = potential_offset.trim();

            // A valid offset must contain a digit and typically ends with % or a unit
            // This avoids misinterpreting "to right bottom" as containing offsets
            if is_likely_offset(potential_offset) {
                return Some((potential_color, potential_offset));
            }
        }
        None
    }

    /// Check if a string looks like a position value (percentage or length).
    /// Must contain a digit and typically ends with %, px, em, etc.
    fn is_likely_offset(s: &str) -> bool {
        if !s.contains(|c: char| c.is_ascii_digit()) {
            return false;
        }
        // Check if it ends with a known unit or %
        let units = [
            "%", "px", "em", "rem", "ex", "ch", "vw", "vh", "vmin", "vmax", "cm", "mm", "in", "pt",
            "pc", "deg", "rad", "grad", "turn",
        ];
        units.iter().any(|u| s.ends_with(u))
    }

    /// Parses the `from <angle> at <position>` part of a conic gradient.
    fn parse_conic_first_item(
        input: &str,
    ) -> Result<Option<(AngleValue, StyleBackgroundPosition)>, CssConicGradientParseError<'_>> {
        let input = input.trim();
        if !input.starts_with("from") {
            return Ok(None);
        }

        let mut parts = input["from".len()..].trim().split("at");
        let angle_part = parts
            .next()
            .ok_or(CssConicGradientParseError::NoAngle(input))?
            .trim();
        let angle = parse_angle_value(angle_part)?;

        let position = match parts.next() {
            Some(pos_part) => parse_style_background_position(pos_part.trim())?,
            None => StyleBackgroundPosition::default(),
        };

        Ok(Some((angle, position)))
    }

    // -- Normalization Functions --

    macro_rules! impl_get_normalized_stops {
        (
            fn $fn_name:ident($input_stop:ty) -> Vec<$output_stop:ident>,
            pos_type = $pos_ty:ty,
            default_start = $default_start:expr,
            default_end = $default_end:expr,
            pos_ctor = $pos_ctor:expr,
            pos_to_f32 = $pos_to_f32:expr,
            output_field = $out_field:ident,
        ) => {
            #[allow(clippy::suboptimal_flops)] // explicit FP; mul_add slower without +fma
            fn $fn_name(stops: &[$input_stop]) -> Vec<$output_stop> {
                if stops.is_empty() {
                    return Vec::new();
                }

                let mut expanded: Vec<(ColorOrSystem, Option<$pos_ty>)> = Vec::new();

                for stop in stops {
                    match (stop.offset1.into_option(), stop.offset2.into_option()) {
                        (None, _) => {
                            expanded.push((stop.color, None));
                        }
                        (Some(pos1), None) => {
                            expanded.push((stop.color, Some(pos1)));
                        }
                        (Some(pos1), Some(pos2)) => {
                            expanded.push((stop.color, Some(pos1)));
                            expanded.push((stop.color, Some(pos2)));
                        }
                    }
                }

                if expanded.is_empty() {
                    return Vec::new();
                }

                let pos_ctor: fn(f32) -> $pos_ty = $pos_ctor;
                let pos_to_f32: fn(&$pos_ty) -> f32 = $pos_to_f32;

                if expanded[0].1.is_none() {
                    expanded[0].1 = Some(pos_ctor($default_start));
                }
                let last_idx = expanded.len() - 1;
                if expanded[last_idx].1.is_none() {
                    expanded[last_idx].1 = Some(pos_ctor($default_end));
                }

                let mut max_so_far: f32 = 0.0;
                for (_, pos) in expanded.iter_mut() {
                    if let Some(p) = pos {
                        let val = pos_to_f32(p);
                        if val < max_so_far {
                            *p = pos_ctor(max_so_far);
                        } else {
                            max_so_far = val;
                        }
                    }
                }

                let mut i = 0;
                while i < expanded.len() {
                    if expanded[i].1.is_none() {
                        let run_start = i;
                        let mut run_end = i;
                        while run_end < expanded.len() && expanded[run_end].1.is_none() {
                            run_end += 1;
                        }

                        let prev_pos = if run_start > 0 {
                            pos_to_f32(&expanded[run_start - 1].1.unwrap())
                        } else {
                            $default_start
                        };

                        let next_pos = if run_end < expanded.len() {
                            pos_to_f32(&expanded[run_end].1.unwrap())
                        } else {
                            $default_end
                        };

                        let run_len = run_end - run_start;
                        let step = (next_pos - prev_pos) / crate::cast::usize_to_f32(run_len + 1);

                        for j in 0..run_len {
                            expanded[run_start + j].1 =
                                Some(pos_ctor(prev_pos + step * crate::cast::usize_to_f32(j + 1)));
                        }

                        i = run_end;
                    } else {
                        i += 1;
                    }
                }

                expanded
                    .into_iter()
                    .map(|(color, pos)| {
                        $output_stop {
                            $out_field: pos.unwrap_or(pos_ctor($default_start)),
                            color,
                        }
                    })
                    .collect()
            }
        };
    }

    impl_get_normalized_stops! {
        fn get_normalized_linear_stops(LinearColorStop) -> Vec<NormalizedLinearColorStop>,
        pos_type = PercentageValue,
        default_start = 0.0,
        default_end = 100.0,
        pos_ctor = (|v| PercentageValue::new(v)),
        pos_to_f32 = (|p: &PercentageValue| p.normalized() * 100.0),
        output_field = offset,
    }

    impl_get_normalized_stops! {
        fn get_normalized_radial_stops(RadialColorStop) -> Vec<NormalizedRadialColorStop>,
        pos_type = AngleValue,
        default_start = 0.0,
        default_end = 360.0,
        pos_ctor = (|v| AngleValue::deg(v)),
        pos_to_f32 = (|p: &AngleValue| p.to_degrees_raw()),
        output_field = angle,
    }

    // -- Other Background Helpers --

    fn parse_background_position_horizontal(
        input: &str,
    ) -> Result<BackgroundPositionHorizontal, CssPixelValueParseError<'_>> {
        Ok(match input {
            "left" => BackgroundPositionHorizontal::Left,
            "center" => BackgroundPositionHorizontal::Center,
            "right" => BackgroundPositionHorizontal::Right,
            other => BackgroundPositionHorizontal::Exact(parse_pixel_value(other)?),
        })
    }

    fn parse_background_position_vertical(
        input: &str,
    ) -> Result<BackgroundPositionVertical, CssPixelValueParseError<'_>> {
        Ok(match input {
            "top" => BackgroundPositionVertical::Top,
            "center" => BackgroundPositionVertical::Center,
            "bottom" => BackgroundPositionVertical::Bottom,
            other => BackgroundPositionVertical::Exact(parse_pixel_value(other)?),
        })
    }

    fn parse_shape(input: &str) -> Result<Shape, CssShapeParseError<'_>> {
        match input.trim() {
            "circle" => Ok(Shape::Circle),
            "ellipse" => Ok(Shape::Ellipse),
            _ => Err(CssShapeParseError::ShapeErr(InvalidValueErr(input))),
        }
    }

    fn parse_radial_gradient_size(
        input: &str,
    ) -> Result<RadialGradientSize, InvalidValueErr<'_>> {
        match input.trim() {
            "closest-side" => Ok(RadialGradientSize::ClosestSide),
            "closest-corner" => Ok(RadialGradientSize::ClosestCorner),
            "farthest-side" => Ok(RadialGradientSize::FarthestSide),
            "farthest-corner" => Ok(RadialGradientSize::FarthestCorner),
            _ => Err(InvalidValueErr(input)),
        }
    }

    /// Adversarial tests. Lives inside `mod parser` (not at file scope) because
    /// the interesting helpers -- `parse_gradient`, `split_color_and_offsets`,
    /// `try_split_last_offset`, `is_likely_offset`, `parse_conic_first_item`,
    /// `parse_shape`, ... -- are private to this module.
    #[cfg(test)]
    #[allow(
        clippy::float_cmp,
        clippy::too_many_lines,
        clippy::unreadable_literal,
        clippy::cognitive_complexity,
        clippy::wildcard_imports
    )]
    mod autotest_generated {
        // `super::*` = the private parser helpers under test; the second glob pulls in
        // the value/error types from the enclosing `background` module.
        use super::*;
        use crate::props::style::background::*;
        use crate::{
            props::basic::{
                angle::CssAngleValueParseError,
                color::{CssColorParseError, OptionColorU, SystemColorRef},
                direction::CssDirectionParseError,
                error::InvalidValueErr,
                length::PercentageParseError,
                parse::{CssImageParseError, ParenthesisParseError},
                pixel::CssPixelValueParseError,
            },
            system::SystemColors,
        };
        use alloc::{string::ToString, vec::Vec};

        // ---------------------------------------------------------------
        // fixtures
        // ---------------------------------------------------------------

        /// Inputs that every `&str` parser in this file is swept over: empty,
        /// whitespace, garbage, boundary numbers, unbalanced braces, unicode.
        const ADVERSARIAL: &[&str] = &[
            "",
            " ",
            "   ",
            "\t\n\r",
            "\u{0}",
            "!!!",
            ";",
            ",",
            ",,",
            "(",
            ")",
            "()",
            "((((",
            "0",
            "-0",
            "+0",
            "NaN",
            "nan",
            "inf",
            "-inf",
            "1e40",
            "-1e40",
            "1e-45",
            "3.4028235e38",
            "9223372036854775807",
            "-9223372036854775808",
            "\u{1F600}",
            "e\u{0301}\u{0301}\u{0301}",
            "\u{00a0}",
            "red\u{00a0}50%",
            "  valid  ",
            "valid;garbage",
            "red;blue",
            "linear-gradient",
            "linear-gradient(",
            "linear-gradient()",
            "url(",
            "url()",
            "rgba(",
            "rgb(0,0,0",
            "to right",
            "circle",
            "from",
        ];

        const ALL_SYSTEM_REFS: [SystemColorRef; 9] = [
            SystemColorRef::Text,
            SystemColorRef::Background,
            SystemColorRef::Accent,
            SystemColorRef::AccentText,
            SystemColorRef::ButtonFace,
            SystemColorRef::ButtonText,
            SystemColorRef::WindowBackground,
            SystemColorRef::SelectionBackground,
            SystemColorRef::SelectionText,
        ];

        const ALL_GRADIENT_TYPES: [GradientType; 6] = [
            GradientType::LinearGradient,
            GradientType::RepeatingLinearGradient,
            GradientType::RadialGradient,
            GradientType::RepeatingRadialGradient,
            GradientType::ConicGradient,
            GradientType::RepeatingConicGradient,
        ];

        fn blue() -> ColorU {
            ColorU::new_rgb(0, 0, 255)
        }

        fn linear(input: &str) -> LinearGradient {
            match parse_style_background_content(input) {
                Ok(StyleBackgroundContent::LinearGradient(g)) => g,
                other => panic!("expected a linear gradient for {input:?}, got {other:?}"),
            }
        }

        fn radial(input: &str) -> RadialGradient {
            match parse_style_background_content(input) {
                Ok(StyleBackgroundContent::RadialGradient(g)) => g,
                other => panic!("expected a radial gradient for {input:?}, got {other:?}"),
            }
        }

        fn conic(input: &str) -> ConicGradient {
            match parse_style_background_content(input) {
                Ok(StyleBackgroundContent::ConicGradient(g)) => g,
                other => panic!("expected a conic gradient for {input:?}, got {other:?}"),
            }
        }

        /// Offsets of a linear/radial gradient, in percent.
        fn offsets(stops: &NormalizedLinearColorStopVec) -> Vec<f32> {
            stops
                .iter()
                .map(|s| s.offset.normalized() * 100.0)
                .collect()
        }

        // ---------------------------------------------------------------
        // serializers: Shape::fmt / RadialGradientSize::fmt
        // ---------------------------------------------------------------

        #[test]
        fn autotest_shape_display_is_exact_and_never_empty() {
            assert_eq!(Shape::Ellipse.to_string(), "ellipse");
            assert_eq!(Shape::Circle.to_string(), "circle");
            assert_eq!(Shape::default(), Shape::Ellipse);
            assert_eq!(Shape::default().to_string(), "ellipse");
            for s in [Shape::Ellipse, Shape::Circle] {
                assert!(!s.to_string().is_empty());
                // The serialized form is a valid input for the parser.
                assert_eq!(parse_shape(&s.to_string()).unwrap(), s);
            }
        }

        #[test]
        fn autotest_radial_gradient_size_display_is_exact_and_never_empty() {
            assert_eq!(RadialGradientSize::ClosestSide.to_string(), "closest-side");
            assert_eq!(
                RadialGradientSize::ClosestCorner.to_string(),
                "closest-corner"
            );
            assert_eq!(
                RadialGradientSize::FarthestSide.to_string(),
                "farthest-side"
            );
            assert_eq!(
                RadialGradientSize::FarthestCorner.to_string(),
                "farthest-corner"
            );
            assert_eq!(
                RadialGradientSize::default(),
                RadialGradientSize::FarthestCorner
            );
            for s in [
                RadialGradientSize::ClosestSide,
                RadialGradientSize::ClosestCorner,
                RadialGradientSize::FarthestSide,
                RadialGradientSize::FarthestCorner,
            ] {
                assert!(!s.to_string().is_empty());
                assert_eq!(parse_radial_gradient_size(&s.to_string()).unwrap(), s);
            }
        }

        // ---------------------------------------------------------------
        // constructors: Normalized{Linear,Radial}ColorStop::new
        // ---------------------------------------------------------------

        #[test]
        fn autotest_normalized_linear_stop_new_keeps_its_arguments() {
            let stop = NormalizedLinearColorStop::new(PercentageValue::new(42.5), ColorU::RED);
            assert_eq!(stop.offset.normalized() * 100.0, 42.5);
            assert_eq!(stop.color, ColorOrSystem::Color(ColorU::RED));

            // Extreme offsets must not panic, and must stay finite: FloatValue
            // encodes f32*1000 into an isize, and `as` saturates (NaN -> 0).
            for f in [
                0.0_f32,
                -0.0,
                f32::NAN,
                f32::INFINITY,
                f32::NEG_INFINITY,
                f32::MAX,
                f32::MIN,
                f32::MIN_POSITIVE,
                -100.0,
                1e30,
            ] {
                let stop = NormalizedLinearColorStop::new(
                    PercentageValue::new(f),
                    ColorU::TRANSPARENT,
                );
                assert!(
                    stop.offset.normalized().is_finite(),
                    "offset went non-finite for {f}"
                );
                assert_eq!(stop.color, ColorOrSystem::Color(ColorU::TRANSPARENT));
            }
            // NaN is flushed to 0, not propagated.
            assert_eq!(
                NormalizedLinearColorStop::new(PercentageValue::new(f32::NAN), ColorU::RED).offset,
                PercentageValue::new(0.0)
            );
        }

        #[test]
        fn autotest_normalized_radial_stop_new_keeps_its_arguments() {
            let stop = NormalizedRadialColorStop::new(AngleValue::deg(90.0), ColorU::RED);
            assert_eq!(stop.angle, AngleValue::deg(90.0));
            assert_eq!(stop.angle.to_degrees_raw(), 90.0);
            assert_eq!(stop.color, ColorOrSystem::Color(ColorU::RED));

            for f in [
                0.0_f32,
                -0.0,
                f32::NAN,
                f32::INFINITY,
                f32::NEG_INFINITY,
                f32::MAX,
                f32::MIN,
                720.0,
                -360.0,
            ] {
                let stop = NormalizedRadialColorStop::new(AngleValue::deg(f), ColorU::WHITE);
                assert!(
                    stop.angle.to_degrees_raw().is_finite(),
                    "angle went non-finite for {f}"
                );
                assert_eq!(stop.color, ColorOrSystem::Color(ColorU::WHITE));
            }
            assert_eq!(
                NormalizedRadialColorStop::new(AngleValue::deg(f32::NAN), ColorU::RED).angle,
                AngleValue::deg(0.0)
            );
        }

        // ---------------------------------------------------------------
        // Normalized{Linear,Radial}ColorStop::resolve
        // ---------------------------------------------------------------

        #[test]
        fn autotest_resolve_concrete_color_ignores_system_colors() {
            let stop = NormalizedLinearColorStop::new(PercentageValue::new(0.0), ColorU::RED);
            assert_eq!(stop.resolve(&SystemColors::default(), ColorU::WHITE), ColorU::RED);

            let populated = SystemColors {
                accent: OptionColorU::Some(ColorU::new_rgb(1, 2, 3)),
                ..SystemColors::default()
            };
            assert_eq!(stop.resolve(&populated, ColorU::WHITE), ColorU::RED);

            let rstop = NormalizedRadialColorStop::new(AngleValue::deg(0.0), ColorU::RED);
            assert_eq!(rstop.resolve(&populated, ColorU::WHITE), ColorU::RED);
        }

        #[test]
        fn autotest_resolve_system_stop_falls_back_for_every_variant() {
            let fallback = ColorU::rgba(9, 8, 7, 6);
            for r in ALL_SYSTEM_REFS {
                let lin = NormalizedLinearColorStop {
                    offset: PercentageValue::new(50.0),
                    color: ColorOrSystem::System(r),
                };
                let rad = NormalizedRadialColorStop {
                    angle: AngleValue::deg(180.0),
                    color: ColorOrSystem::System(r),
                };
                // Nothing is populated -> every variant resolves to the fallback.
                assert_eq!(lin.resolve(&SystemColors::default(), fallback), fallback);
                assert_eq!(rad.resolve(&SystemColors::default(), fallback), fallback);
            }

            // A populated key resolves; the others still fall back.
            let accent = ColorU::new_rgb(0, 122, 255);
            let populated = SystemColors {
                accent: OptionColorU::Some(accent),
                ..SystemColors::default()
            };
            let stop = NormalizedLinearColorStop {
                offset: PercentageValue::new(0.0),
                color: ColorOrSystem::System(SystemColorRef::Accent),
            };
            assert_eq!(stop.resolve(&populated, fallback), accent);
            let other = NormalizedLinearColorStop {
                offset: PercentageValue::new(0.0),
                color: ColorOrSystem::System(SystemColorRef::ButtonText),
            };
            assert_eq!(other.resolve(&populated, fallback), fallback);
        }

        // ---------------------------------------------------------------
        // numeric: scale_for_dpi
        // ---------------------------------------------------------------

        #[test]
        fn autotest_background_position_horizontal_scale_for_dpi() {
            // Keywords are immune to scaling, for *any* factor.
            for f in [0.0_f32, 1.0, -1.0, f32::NAN, f32::INFINITY, f32::MIN, f32::MAX] {
                for keyword in [
                    BackgroundPositionHorizontal::Left,
                    BackgroundPositionHorizontal::Center,
                    BackgroundPositionHorizontal::Right,
                ] {
                    let mut k = keyword;
                    k.scale_for_dpi(f);
                    assert_eq!(k, keyword, "keyword mutated by scale factor {f}");
                }
            }

            let mut exact = BackgroundPositionHorizontal::Exact(PixelValue::px(10.0));
            exact.scale_for_dpi(2.0);
            assert_eq!(exact, BackgroundPositionHorizontal::Exact(PixelValue::px(20.0)));

            // zero, negative
            let mut zeroed = BackgroundPositionHorizontal::Exact(PixelValue::px(10.0));
            zeroed.scale_for_dpi(0.0);
            assert_eq!(zeroed, BackgroundPositionHorizontal::Exact(PixelValue::px(0.0)));

            let mut negated = BackgroundPositionHorizontal::Exact(PixelValue::px(10.0));
            negated.scale_for_dpi(-1.0);
            assert_eq!(
                negated,
                BackgroundPositionHorizontal::Exact(PixelValue::px(-10.0))
            );

            // NaN is flushed to 0 by the isize cast, never propagated.
            let mut nan = BackgroundPositionHorizontal::Exact(PixelValue::px(10.0));
            nan.scale_for_dpi(f32::NAN);
            assert_eq!(nan, BackgroundPositionHorizontal::Exact(PixelValue::px(0.0)));

            // +-inf and MIN/MAX saturate to the isize bounds -- finite, no panic.
            for f in [f32::INFINITY, f32::NEG_INFINITY, f32::MAX, f32::MIN] {
                let mut v = BackgroundPositionHorizontal::Exact(PixelValue::px(10.0));
                v.scale_for_dpi(f);
                let BackgroundPositionHorizontal::Exact(px) = v else {
                    panic!("variant changed under scaling");
                };
                assert!(px.number.get().is_finite(), "non-finite result for {f}");
                assert_eq!(px.number.get().is_sign_negative(), f.is_sign_negative());
            }
        }

        #[test]
        fn autotest_background_position_vertical_scale_for_dpi() {
            for f in [0.0_f32, 1.0, -1.0, f32::NAN, f32::INFINITY, f32::MIN, f32::MAX] {
                for keyword in [
                    BackgroundPositionVertical::Top,
                    BackgroundPositionVertical::Center,
                    BackgroundPositionVertical::Bottom,
                ] {
                    let mut k = keyword;
                    k.scale_for_dpi(f);
                    assert_eq!(k, keyword, "keyword mutated by scale factor {f}");
                }
            }

            let mut exact = BackgroundPositionVertical::Exact(PixelValue::em(4.0));
            exact.scale_for_dpi(0.5);
            assert_eq!(exact, BackgroundPositionVertical::Exact(PixelValue::em(2.0)));

            let mut nan = BackgroundPositionVertical::Exact(PixelValue::px(10.0));
            nan.scale_for_dpi(f32::NAN);
            assert_eq!(nan, BackgroundPositionVertical::Exact(PixelValue::px(0.0)));

            // Saturation is a fixed point: scaling an already-saturated value again
            // must not wrap around into a negative number.
            let mut saturated = BackgroundPositionVertical::Exact(PixelValue::px(f32::MAX));
            saturated.scale_for_dpi(f32::MAX);
            let once = saturated;
            saturated.scale_for_dpi(f32::MAX);
            assert_eq!(saturated, once);
            let BackgroundPositionVertical::Exact(px) = saturated else {
                panic!("variant changed under scaling");
            };
            assert!(px.number.get() > 0.0);
            assert!(px.number.get().is_finite());
        }

        #[test]
        fn autotest_style_background_position_scale_for_dpi_scales_both_axes() {
            let mut pos = StyleBackgroundPosition {
                horizontal: BackgroundPositionHorizontal::Exact(PixelValue::px(10.0)),
                vertical: BackgroundPositionVertical::Exact(PixelValue::px(20.0)),
            };
            pos.scale_for_dpi(3.0);
            assert_eq!(
                pos.horizontal,
                BackgroundPositionHorizontal::Exact(PixelValue::px(30.0))
            );
            assert_eq!(
                pos.vertical,
                BackgroundPositionVertical::Exact(PixelValue::px(60.0))
            );

            // Scaling compounds -- pinned, because a double-applied DPI scale is a
            // classic layout bug.
            pos.scale_for_dpi(2.0);
            assert_eq!(
                pos.horizontal,
                BackgroundPositionHorizontal::Exact(PixelValue::px(60.0))
            );

            // The all-keyword default is a fixed point for every factor.
            for f in [0.0_f32, 1.0, -2.5, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
                let mut default = StyleBackgroundPosition::default();
                default.scale_for_dpi(f);
                assert_eq!(default, StyleBackgroundPosition::default());
            }
        }

        #[test]
        fn autotest_style_background_size_scale_for_dpi() {
            // Contain / Cover carry no number and must survive any factor.
            for f in [0.0_f32, 2.0, -1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
                for keyword in [StyleBackgroundSize::Contain, StyleBackgroundSize::Cover] {
                    let mut k = keyword;
                    k.scale_for_dpi(f);
                    assert_eq!(k, keyword, "keyword mutated by scale factor {f}");
                }
            }

            let mut size = StyleBackgroundSize::ExactSize(PixelValueSize {
                width: PixelValue::px(10.0),
                height: PixelValue::percent(50.0),
            });
            size.scale_for_dpi(2.0);
            assert_eq!(
                size,
                StyleBackgroundSize::ExactSize(PixelValueSize {
                    width: PixelValue::px(20.0),
                    // NOTE: percentages are scaled too, which is arguably wrong for a
                    // DPI change -- pinned as current behaviour.
                    height: PixelValue::percent(100.0),
                })
            );

            let mut nan = StyleBackgroundSize::ExactSize(PixelValueSize {
                width: PixelValue::px(10.0),
                height: PixelValue::px(20.0),
            });
            nan.scale_for_dpi(f32::NAN);
            assert_eq!(
                nan,
                StyleBackgroundSize::ExactSize(PixelValueSize {
                    width: PixelValue::px(0.0),
                    height: PixelValue::px(0.0),
                })
            );

            let mut inf = StyleBackgroundSize::ExactSize(PixelValueSize {
                width: PixelValue::px(1.0),
                height: PixelValue::px(-1.0),
            });
            inf.scale_for_dpi(f32::INFINITY);
            let StyleBackgroundSize::ExactSize(s) = inf else {
                panic!("variant changed under scaling");
            };
            assert!(s.width.number.get().is_finite() && s.width.number.get() > 0.0);
            assert!(s.height.number.get().is_finite() && s.height.number.get() < 0.0);
        }

        // ---------------------------------------------------------------
        // getters: to_contained / to_shared round-trips
        // ---------------------------------------------------------------

        #[test]
        fn autotest_css_background_parse_error_round_trips() {
            let errors = [
                CssBackgroundParseError::Error(""),
                CssBackgroundParseError::Error("boom \u{1F600}"),
                CssBackgroundParseError::InvalidBackground(ParenthesisParseError::EmptyInput),
                CssBackgroundParseError::InvalidBackground(
                    ParenthesisParseError::StopWordNotFound("nope"),
                ),
                CssBackgroundParseError::UnclosedGradient(""),
                CssBackgroundParseError::NoDirection("nodir"),
                CssBackgroundParseError::TooFewGradientStops("few"),
                CssBackgroundParseError::DirectionParseError(CssDirectionParseError::Error("d")),
                CssBackgroundParseError::DirectionParseError(
                    CssDirectionParseError::InvalidArguments("args"),
                ),
                CssBackgroundParseError::GradientParseError(CssGradientStopParseError::Error("g")),
                CssBackgroundParseError::ConicGradient(CssConicGradientParseError::NoAngle("a")),
                CssBackgroundParseError::ShapeParseError(CssShapeParseError::ShapeErr(
                    InvalidValueErr("s"),
                )),
                CssBackgroundParseError::ImageParseError(CssImageParseError::UnclosedQuotes("q")),
                CssBackgroundParseError::ColorParseError(CssColorParseError::InvalidColor("c")),
                CssBackgroundParseError::ColorParseError(CssColorParseError::EmptyInput),
            ];
            for e in &errors {
                let owned = e.to_contained();
                assert_eq!(&owned.to_shared(), e, "round-trip changed {e:?}");
                // Display must survive the round-trip as well.
                assert_eq!(
                    alloc::format!("{}", owned.to_shared()),
                    alloc::format!("{e}")
                );
            }
        }

        #[test]
        fn autotest_error_round_trip_survives_huge_and_unicode_payloads() {
            let huge = "x".repeat(100_000);
            let weird = "\u{1F600}\u{0}\u{00a0}e\u{0301}";
            for s in [huge.as_str(), weird, "", " "] {
                let e = CssBackgroundParseError::UnclosedGradient(s);
                assert_eq!(e.to_contained().to_shared(), e);

                let e = CssGradientStopParseError::Error(s);
                assert_eq!(e.to_contained().to_shared(), e);

                let e = CssConicGradientParseError::NoAngle(s);
                assert_eq!(e.to_contained().to_shared(), e);

                let e = CssShapeParseError::ShapeErr(InvalidValueErr(s));
                assert_eq!(e.to_contained().to_shared(), e);

                let e = CssBackgroundPositionParseError::NoPosition(s);
                assert_eq!(e.to_contained().to_shared(), e);
            }
        }

        #[test]
        fn autotest_css_gradient_stop_parse_error_round_trips() {
            let errors = [
                CssGradientStopParseError::Error("boom"),
                CssGradientStopParseError::Percentage(PercentageParseError::NoPercentSign),
                CssGradientStopParseError::Percentage(PercentageParseError::InvalidUnit(
                    "px".to_string().into(),
                )),
                CssGradientStopParseError::Angle(CssAngleValueParseError::EmptyString),
                CssGradientStopParseError::Angle(CssAngleValueParseError::InvalidAngle("q")),
                CssGradientStopParseError::ColorParseError(CssColorParseError::InvalidColor("c")),
            ];
            for e in &errors {
                assert_eq!(&e.to_contained().to_shared(), e, "round-trip changed {e:?}");
            }
        }

        #[test]
        fn autotest_css_conic_and_shape_parse_error_round_trip() {
            let errors = [
                CssConicGradientParseError::NoAngle("n"),
                CssConicGradientParseError::Angle(CssAngleValueParseError::EmptyString),
                CssConicGradientParseError::Position(
                    CssBackgroundPositionParseError::NoPosition("p"),
                ),
            ];
            for e in &errors {
                assert_eq!(&e.to_contained().to_shared(), e);
            }

            let shape = CssShapeParseError::ShapeErr(InvalidValueErr("blob"));
            assert_eq!(shape.to_contained().to_shared(), shape);
        }

        #[test]
        fn autotest_css_background_position_parse_error_round_trips() {
            let errors = [
                CssBackgroundPositionParseError::NoPosition(""),
                CssBackgroundPositionParseError::TooManyComponents("a b c"),
                CssBackgroundPositionParseError::FirstComponentWrong(
                    CssPixelValueParseError::EmptyString,
                ),
                CssBackgroundPositionParseError::FirstComponentWrong(
                    CssPixelValueParseError::InvalidPixelValue("q"),
                ),
                CssBackgroundPositionParseError::SecondComponentWrong(
                    CssPixelValueParseError::InvalidPixelValue("\u{1F600}"),
                ),
            ];
            for e in &errors {
                assert_eq!(&e.to_contained().to_shared(), e, "round-trip changed {e:?}");
            }
        }

        #[test]
        fn autotest_real_parse_errors_round_trip_through_the_owned_form() {
            // Errors as actually produced by the parsers, not hand-built ones.
            for input in ADVERSARIAL {
                if let Err(e) = parse_style_background_content(input) {
                    assert_eq!(e.to_contained().to_shared(), e, "for input {input:?}");
                }
                if let Err(e) = parse_style_background_position(input) {
                    assert_eq!(e.to_contained().to_shared(), e, "for input {input:?}");
                }
            }
        }

        // ---------------------------------------------------------------
        // GradientType::get_extend_mode (private)
        // ---------------------------------------------------------------

        #[test]
        fn autotest_get_extend_mode_is_repeat_exactly_for_the_repeating_variants() {
            assert_eq!(
                GradientType::LinearGradient.get_extend_mode(),
                ExtendMode::Clamp
            );
            assert_eq!(
                GradientType::RadialGradient.get_extend_mode(),
                ExtendMode::Clamp
            );
            assert_eq!(
                GradientType::ConicGradient.get_extend_mode(),
                ExtendMode::Clamp
            );
            assert_eq!(
                GradientType::RepeatingLinearGradient.get_extend_mode(),
                ExtendMode::Repeat
            );
            assert_eq!(
                GradientType::RepeatingRadialGradient.get_extend_mode(),
                ExtendMode::Repeat
            );
            assert_eq!(
                GradientType::RepeatingConicGradient.get_extend_mode(),
                ExtendMode::Repeat
            );
            // Total + pure: same input, same answer.
            for t in ALL_GRADIENT_TYPES {
                assert_eq!(t.get_extend_mode(), t.get_extend_mode());
            }
            assert_eq!(ExtendMode::default(), ExtendMode::Clamp);
        }

        // ---------------------------------------------------------------
        // parser: parse_style_background_content
        // ---------------------------------------------------------------

        #[test]
        fn autotest_background_content_never_panics_and_is_deterministic() {
            for input in ADVERSARIAL {
                let a = parse_style_background_content(input);
                let b = parse_style_background_content(input);
                assert_eq!(a, b, "non-deterministic for {input:?}");
            }
        }

        #[test]
        fn autotest_background_content_rejects_empty_whitespace_and_garbage() {
            for input in ["", " ", "   ", "\t\n\r", "\u{0}", "!!!", ";", "valid;garbage"] {
                assert!(
                    parse_style_background_content(input).is_err(),
                    "{input:?} should not parse as a background"
                );
            }
        }

        #[test]
        fn autotest_background_content_valid_minimal_positive_controls() {
            assert_eq!(
                parse_style_background_content("red").unwrap(),
                StyleBackgroundContent::Color(ColorU::RED)
            );
            // Leading/trailing whitespace is trimmed, not rejected.
            assert_eq!(
                parse_style_background_content("  red  ").unwrap(),
                StyleBackgroundContent::Color(ColorU::RED)
            );
            assert_eq!(
                parse_style_background_content("system:accent").unwrap(),
                StyleBackgroundContent::SystemColor(SystemColorRef::Accent)
            );
            assert_eq!(
                parse_style_background_content("url(a.png)").unwrap(),
                StyleBackgroundContent::Image("a.png".into())
            );
        }

        #[test]
        fn autotest_background_content_unicode_is_rejected_without_panicking() {
            for input in [
                "\u{1F600}",
                "url(\u{1F600}.png)",
                "linear-gradient(\u{1F600}, red)",
                "e\u{0301}\u{0301}\u{0301}",
                "\u{00a0}",
            ] {
                let parsed = parse_style_background_content(input);
                // url() accepts any payload; everything else must be an error.
                if input.starts_with("url(") {
                    assert!(parsed.is_ok());
                } else {
                    assert!(parsed.is_err(), "{input:?} unexpectedly parsed");
                }
            }
        }

        #[test]
        fn autotest_background_content_extremely_long_input_terminates() {
            let huge = "a".repeat(100_000);
            assert!(parse_style_background_content(&huge).is_err());

            let huge_gradient =
                alloc::format!("linear-gradient({})", "red, ".repeat(2_000) + "blue");
            let g = linear(&huge_gradient);
            assert_eq!(g.stops.len(), 2_001);

            let huge_url = alloc::format!("url({})", "a".repeat(100_000));
            assert!(matches!(
                parse_style_background_content(&huge_url),
                Ok(StyleBackgroundContent::Image(_))
            ));
        }

        #[test]
        fn autotest_background_content_deep_nesting_does_not_stack_overflow() {
            let nested = alloc::format!(
                "linear-gradient({}red{})",
                "(".repeat(10_000),
                ")".repeat(10_000)
            );
            assert!(parse_style_background_content(&nested).is_err());

            let unbalanced = alloc::format!("linear-gradient({}", "(".repeat(10_000));
            assert!(parse_style_background_content(&unbalanced).is_err());
        }

        #[test]
        fn autotest_unclosed_gradient_reports_a_color_error_not_unclosed_gradient() {
            // parse_parentheses fails (no ')'), so the input falls through to the
            // color branch -- the `UnclosedGradient` variant is never produced here.
            let err = parse_style_background_content("linear-gradient(red, blue").unwrap_err();
            assert!(
                matches!(err, CssBackgroundParseError::ColorParseError(_)),
                "got {err:?}"
            );
        }

        #[test]
        fn autotest_empty_gradient_body_is_a_no_direction_error() {
            let err = parse_style_background_content("linear-gradient()").unwrap_err();
            assert!(matches!(err, CssBackgroundParseError::NoDirection(_)), "got {err:?}");
            for f in [
                "radial-gradient()",
                "conic-gradient()",
                "repeating-linear-gradient()",
            ] {
                assert!(parse_style_background_content(f).is_err(), "{f:?}");
            }
        }

        #[test]
        fn autotest_url_with_empty_payload_is_accepted_as_an_empty_image() {
            // Pinned: `url()` yields an empty image id rather than an error.
            assert_eq!(
                parse_style_background_content("url()").unwrap(),
                StyleBackgroundContent::Image("".into())
            );
        }

        #[test]
        fn autotest_gradient_boundary_number_directions_stay_finite() {
            // "NaN" parses as a bare number -> a NaN angle, which the isize cast
            // flushes to 0deg. Pinned: it is silently accepted, not rejected.
            let g = linear("linear-gradient(NaN, red, blue)");
            assert_eq!(g.direction, Direction::Angle(AngleValue::deg(0.0)));

            // Overflowing / tiny literals must not panic and must stay finite.
            for input in [
                "linear-gradient(0deg, red, blue)",
                "linear-gradient(-0deg, red, blue)",
                "linear-gradient(1e40deg, red, blue)",
                "linear-gradient(-1e40deg, red, blue)",
                "linear-gradient(1e-45deg, red, blue)",
                "linear-gradient(inf, red, blue)",
                "linear-gradient(-inf, red, blue)",
                "linear-gradient(9223372036854775807deg, red, blue)",
            ] {
                let g = linear(input);
                let Direction::Angle(a) = g.direction else {
                    panic!("expected an angle direction for {input:?}");
                };
                assert!(a.to_degrees_raw().is_finite(), "non-finite angle for {input:?}");
                assert_eq!(g.stops.len(), 2, "{input:?}");
            }
        }

        #[test]
        fn autotest_gradient_stop_offsets_are_monotonic_and_finite() {
            for input in [
                "linear-gradient(red, blue)",
                "linear-gradient(red, green, blue)",
                "linear-gradient(red 50%, blue 20%)",
                "linear-gradient(red -50%, blue)",
                "linear-gradient(red 0%, yellow, green, blue 100%)",
                "linear-gradient(red 10% 30%, blue)",
                "linear-gradient(red 200%, blue 10%)",
                "repeating-linear-gradient(red, blue 20%)",
                "radial-gradient(circle, red, blue)",
            ] {
                let content = parse_style_background_content(input).unwrap();
                let stops = match &content {
                    StyleBackgroundContent::LinearGradient(g) => &g.stops,
                    StyleBackgroundContent::RadialGradient(g) => &g.stops,
                    other => panic!("unexpected content {other:?}"),
                };
                let mut prev = f32::NEG_INFINITY;
                for o in offsets(stops) {
                    assert!(o.is_finite(), "non-finite offset in {input:?}");
                    assert!(o >= prev, "offsets not monotonic in {input:?}: {o} < {prev}");
                    prev = o;
                }
            }
        }

        #[test]
        fn autotest_negative_and_overflowing_stop_offsets_are_clamped() {
            // A negative first offset is clamped to the running maximum (0%).
            let g = linear("linear-gradient(red -50%, blue)");
            assert_eq!(offsets(&g.stops), alloc::vec![0.0, 100.0]);

            // An out-of-range offset is *not* clamped down to 100% -- the later
            // stop is dragged up to it instead.
            let g = linear("linear-gradient(red 200%, blue 10%)");
            assert_eq!(offsets(&g.stops), alloc::vec![200.0, 200.0]);
        }

        #[test]
        fn autotest_offsets_that_are_not_percentages_are_rejected() {
            // "50px" looks like an offset (is_likely_offset), but a linear stop
            // offset must be a percentage -> hard error, no silent fallback.
            let err = parse_style_background_content("linear-gradient(red 50px, blue)").unwrap_err();
            assert!(
                matches!(
                    err,
                    CssBackgroundParseError::GradientParseError(
                        CssGradientStopParseError::Percentage(_)
                    )
                ),
                "got {err:?}"
            );

            // A bare number is *not* recognised as an offset at all, so the whole
            // token is treated as part of the color and fails to parse.
            assert!(parse_style_background_content("linear-gradient(red 0.5, blue)").is_err());
            // Neither is "NaN%" (no ASCII digit).
            assert!(parse_style_background_content("linear-gradient(red NaN%, blue)").is_err());
        }

        #[test]
        fn autotest_huge_stop_offsets_do_not_produce_nan_or_inf() {
            let g = linear("linear-gradient(red 1e40%, blue)");
            assert_eq!(g.stops.len(), 2);
            for o in offsets(&g.stops) {
                assert!(o.is_finite(), "offset leaked a non-finite value: {o}");
            }
        }

        // ---------------------------------------------------------------
        // parser: parse_style_background_content_multiple
        // ---------------------------------------------------------------

        #[test]
        fn autotest_background_content_multiple_empty_input_yields_an_empty_vec() {
            // Pinned: empty input is *not* an error -- split_string_respect_comma
            // returns no items, so the result is an empty layer list.
            let parsed = parse_style_background_content_multiple("").unwrap();
            assert_eq!(parsed.len(), 0);

            // Whitespace-only *is* an error (one empty item that fails to parse).
            assert!(parse_style_background_content_multiple("   ").is_err());
            assert!(parse_style_background_content_multiple(",").is_err());
            assert!(parse_style_background_content_multiple("red,,blue").is_err());
        }

        #[test]
        fn autotest_background_content_multiple_valid_and_adversarial() {
            let parsed =
                parse_style_background_content_multiple("linear-gradient(red, blue), url(a.png)")
                    .unwrap();
            assert_eq!(parsed.len(), 2);
            assert!(matches!(
                parsed.as_slice()[0],
                StyleBackgroundContent::LinearGradient(_)
            ));
            assert!(matches!(
                parsed.as_slice()[1],
                StyleBackgroundContent::Image(_)
            ));

            // One bad layer poisons the whole list.
            assert!(parse_style_background_content_multiple("red, !!!").is_err());

            // Long repeated input terminates.
            let many = "red,".repeat(2_000) + "blue";
            assert_eq!(
                parse_style_background_content_multiple(&many).unwrap().len(),
                2_001
            );

            for input in ADVERSARIAL {
                let a = parse_style_background_content_multiple(input);
                let b = parse_style_background_content_multiple(input);
                assert_eq!(a, b, "non-deterministic for {input:?}");
            }
        }

        // ---------------------------------------------------------------
        // parser: parse_style_background_position(_multiple)
        // ---------------------------------------------------------------

        #[test]
        fn autotest_background_position_empty_and_whitespace() {
            assert_eq!(
                parse_style_background_position(""),
                Err(CssBackgroundPositionParseError::NoPosition(""))
            );
            assert_eq!(
                parse_style_background_position("   "),
                Err(CssBackgroundPositionParseError::NoPosition(""))
            );
            assert_eq!(
                parse_style_background_position("\t\n\r"),
                Err(CssBackgroundPositionParseError::NoPosition(""))
            );
        }

        #[test]
        fn autotest_background_position_valid_minimal_and_keyword_order() {
            let p = parse_style_background_position("left").unwrap();
            assert_eq!(p.horizontal, BackgroundPositionHorizontal::Left);
            assert_eq!(p.vertical, BackgroundPositionVertical::Center);

            // A lone vertical keyword also works: the horizontal falls back to center.
            let p = parse_style_background_position("top").unwrap();
            assert_eq!(p.horizontal, BackgroundPositionHorizontal::Center);
            assert_eq!(p.vertical, BackgroundPositionVertical::Top);

            // Either order is accepted for keyword pairs.
            assert_eq!(
                parse_style_background_position("left top").unwrap(),
                parse_style_background_position("top left").unwrap()
            );

            // ... but "left right" is a vertical-slot error, not silently accepted.
            assert!(matches!(
                parse_style_background_position("left right"),
                Err(CssBackgroundPositionParseError::SecondComponentWrong(_))
            ));
        }

        #[test]
        fn autotest_background_position_too_many_components() {
            assert!(matches!(
                parse_style_background_position("left 10px top 20px"),
                Err(CssBackgroundPositionParseError::TooManyComponents(_))
            ));
            assert!(matches!(
                parse_style_background_position("a b c"),
                Err(CssBackgroundPositionParseError::TooManyComponents(_))
            ));
        }

        #[test]
        fn autotest_background_position_boundary_numbers_are_accepted_and_saturate() {
            // Pinned: parse_pixel_value accepts a bare number as px -- so "NaN" and
            // "inf" are *valid* background positions, flushed to 0 / saturated.
            assert_eq!(
                parse_style_background_position("NaN").unwrap().horizontal,
                BackgroundPositionHorizontal::Exact(PixelValue::px(0.0))
            );
            assert_eq!(
                parse_style_background_position("-0").unwrap().horizontal,
                BackgroundPositionHorizontal::Exact(PixelValue::px(0.0))
            );
            assert_eq!(
                parse_style_background_position("inf").unwrap().horizontal,
                BackgroundPositionHorizontal::Exact(PixelValue::px(f32::INFINITY))
            );
            for input in ["0", "1e40px", "-1e40px", "1e-45px", "3.4028235e38px"] {
                let p = parse_style_background_position(input).unwrap();
                let BackgroundPositionHorizontal::Exact(px) = p.horizontal else {
                    panic!("expected an exact value for {input:?}");
                };
                assert!(px.number.get().is_finite(), "non-finite for {input:?}");
            }
        }

        #[test]
        fn autotest_background_position_garbage_unicode_and_long_input() {
            for input in ["garbage", "!!!", "\u{1F600}", "e\u{0301}", "left;top"] {
                assert!(
                    parse_style_background_position(input).is_err(),
                    "{input:?} unexpectedly parsed"
                );
            }
            let huge = "a".repeat(100_000);
            assert!(parse_style_background_position(&huge).is_err());
            let nested = "(".repeat(10_000);
            assert!(parse_style_background_position(&nested).is_err());

            for input in ADVERSARIAL {
                let a = parse_style_background_position(input);
                let b = parse_style_background_position(input);
                assert_eq!(a, b, "non-deterministic for {input:?}");
            }
        }

        #[test]
        fn autotest_background_position_multiple() {
            // Empty input -> empty vec (no error), same as the other *_multiple fns.
            assert_eq!(parse_style_background_position_multiple("").unwrap().len(), 0);

            let parsed = parse_style_background_position_multiple("left top, 10px 20px").unwrap();
            assert_eq!(parsed.len(), 2);
            assert_eq!(
                parsed.as_slice()[1].horizontal,
                BackgroundPositionHorizontal::Exact(PixelValue::px(10.0))
            );

            assert!(parse_style_background_position_multiple("left top, !!!").is_err());
            assert!(parse_style_background_position_multiple("   ").is_err());

            let many = "left top,".repeat(2_000) + "center";
            assert_eq!(
                parse_style_background_position_multiple(&many).unwrap().len(),
                2_001
            );
        }

        // ---------------------------------------------------------------
        // parser: parse_style_background_size(_multiple)
        // ---------------------------------------------------------------

        #[test]
        fn autotest_background_size_empty_whitespace_and_garbage() {
            for input in ["", "   ", "\t\n", "auto", "!!!", "\u{1F600}", "CONTAIN", "Cover"] {
                assert!(
                    parse_style_background_size(input).is_err(),
                    "{input:?} unexpectedly parsed"
                );
            }
            let huge = "a".repeat(100_000);
            assert!(parse_style_background_size(&huge).is_err());
        }

        #[test]
        fn autotest_background_size_valid_minimal_and_trimming() {
            assert_eq!(
                parse_style_background_size("  contain  ").unwrap(),
                StyleBackgroundSize::Contain
            );
            assert_eq!(
                parse_style_background_size("cover").unwrap(),
                StyleBackgroundSize::Cover
            );
            // A single value applies to both axes.
            assert_eq!(
                parse_style_background_size("50%").unwrap(),
                StyleBackgroundSize::ExactSize(PixelValueSize {
                    width: PixelValue::percent(50.0),
                    height: PixelValue::percent(50.0),
                })
            );
        }

        #[test]
        fn autotest_background_size_silently_ignores_extra_components() {
            // BUG-ish, pinned: unlike background-position (TooManyComponents), a third
            // component is dropped on the floor instead of being rejected.
            assert_eq!(
                parse_style_background_size("10px 20px 30px").unwrap(),
                StyleBackgroundSize::ExactSize(PixelValueSize {
                    width: PixelValue::px(10.0),
                    height: PixelValue::px(20.0),
                })
            );
        }

        #[test]
        fn autotest_background_size_boundary_numbers_saturate_without_panicking() {
            // Bare numbers parse as px, so "NaN" is accepted and flushed to 0px.
            assert_eq!(
                parse_style_background_size("NaN").unwrap(),
                StyleBackgroundSize::ExactSize(PixelValueSize {
                    width: PixelValue::px(0.0),
                    height: PixelValue::px(0.0),
                })
            );
            assert_eq!(
                parse_style_background_size("inf").unwrap(),
                StyleBackgroundSize::ExactSize(PixelValueSize {
                    width: PixelValue::px(f32::INFINITY),
                    height: PixelValue::px(f32::INFINITY),
                })
            );
            for input in ["0", "-0", "1e40px", "-1e40px", "1e-45px"] {
                let StyleBackgroundSize::ExactSize(s) =
                    parse_style_background_size(input).unwrap()
                else {
                    panic!("expected an exact size for {input:?}");
                };
                assert!(s.width.number.get().is_finite(), "non-finite for {input:?}");
                assert!(s.height.number.get().is_finite(), "non-finite for {input:?}");
            }

            for input in ADVERSARIAL {
                let a = parse_style_background_size(input);
                let b = parse_style_background_size(input);
                assert_eq!(a, b, "non-deterministic for {input:?}");
            }
        }

        #[test]
        fn autotest_background_size_multiple() {
            assert_eq!(parse_style_background_size_multiple("").unwrap().len(), 0);

            let parsed = parse_style_background_size_multiple("contain, 10px 20px, cover").unwrap();
            assert_eq!(parsed.len(), 3);
            assert_eq!(parsed.as_slice()[0], StyleBackgroundSize::Contain);
            assert_eq!(parsed.as_slice()[2], StyleBackgroundSize::Cover);

            assert!(parse_style_background_size_multiple("cover, auto").is_err());
            assert!(parse_style_background_size_multiple("   ").is_err());

            let many = "cover,".repeat(2_000) + "contain";
            assert_eq!(
                parse_style_background_size_multiple(&many).unwrap().len(),
                2_001
            );
        }

        // ---------------------------------------------------------------
        // parser: parse_style_background_repeat(_multiple)
        // ---------------------------------------------------------------

        #[test]
        fn autotest_background_repeat_valid_and_invalid() {
            assert_eq!(
                parse_style_background_repeat("  repeat  ").unwrap(),
                StyleBackgroundRepeat::PatternRepeat
            );
            assert_eq!(
                parse_style_background_repeat("no-repeat").unwrap(),
                StyleBackgroundRepeat::NoRepeat
            );
            assert_eq!(
                parse_style_background_repeat("repeat-x").unwrap(),
                StyleBackgroundRepeat::RepeatX
            );
            assert_eq!(
                parse_style_background_repeat("repeat-y").unwrap(),
                StyleBackgroundRepeat::RepeatY
            );
            assert_eq!(StyleBackgroundRepeat::default(), StyleBackgroundRepeat::PatternRepeat);

            for input in [
                "",
                "   ",
                "\t\n",
                "REPEAT",
                "Repeat",
                "repeat-xy",
                "repeat repeat",
                "!!!",
                "\u{1F600}",
                "0",
                "NaN",
            ] {
                assert!(
                    parse_style_background_repeat(input).is_err(),
                    "{input:?} unexpectedly parsed"
                );
            }

            let huge = "repeat".repeat(20_000);
            assert!(parse_style_background_repeat(&huge).is_err());

            for input in ADVERSARIAL {
                let a = parse_style_background_repeat(input);
                let b = parse_style_background_repeat(input);
                assert_eq!(a, b, "non-deterministic for {input:?}");
            }
        }

        #[test]
        fn autotest_background_repeat_multiple() {
            assert_eq!(parse_style_background_repeat_multiple("").unwrap().len(), 0);

            let parsed = parse_style_background_repeat_multiple("repeat, no-repeat").unwrap();
            assert_eq!(parsed.len(), 2);
            assert_eq!(parsed.as_slice()[0], StyleBackgroundRepeat::PatternRepeat);
            assert_eq!(parsed.as_slice()[1], StyleBackgroundRepeat::NoRepeat);

            assert!(parse_style_background_repeat_multiple("repeat,,repeat").is_err());
            assert!(parse_style_background_repeat_multiple("   ").is_err());

            let many = "repeat,".repeat(2_000) + "no-repeat";
            assert_eq!(
                parse_style_background_repeat_multiple(&many).unwrap().len(),
                2_001
            );
        }

        // ---------------------------------------------------------------
        // parser: parse_gradient (private)
        // ---------------------------------------------------------------

        #[test]
        fn autotest_parse_gradient_empty_input_is_no_direction_for_every_type() {
            for t in ALL_GRADIENT_TYPES {
                assert!(
                    matches!(
                        parse_gradient("", t),
                        Err(CssBackgroundParseError::NoDirection(_))
                    ),
                    "empty body accepted for {t:?}"
                );
            }
        }

        #[test]
        fn autotest_parse_gradient_extend_mode_follows_the_gradient_type() {
            let StyleBackgroundContent::LinearGradient(g) =
                parse_gradient("red, blue", GradientType::LinearGradient).unwrap()
            else {
                panic!("expected a linear gradient");
            };
            assert_eq!(g.extend_mode, ExtendMode::Clamp);

            let StyleBackgroundContent::LinearGradient(g) =
                parse_gradient("red, blue", GradientType::RepeatingLinearGradient).unwrap()
            else {
                panic!("expected a linear gradient");
            };
            assert_eq!(g.extend_mode, ExtendMode::Repeat);

            let StyleBackgroundContent::RadialGradient(g) =
                parse_gradient("red, blue", GradientType::RepeatingRadialGradient).unwrap()
            else {
                panic!("expected a radial gradient");
            };
            assert_eq!(g.extend_mode, ExtendMode::Repeat);

            let StyleBackgroundContent::ConicGradient(g) =
                parse_gradient("red, blue", GradientType::RepeatingConicGradient).unwrap()
            else {
                panic!("expected a conic gradient");
            };
            assert_eq!(g.extend_mode, ExtendMode::Repeat);
        }

        #[test]
        fn autotest_parse_gradient_accepts_gradients_with_too_few_stops() {
            // W3C requires >= 2 color stops. Pinned: this parser happily returns
            // gradients with one or zero stops -- `TooFewGradientStops` is dead code.
            let StyleBackgroundContent::LinearGradient(g) =
                parse_gradient("red", GradientType::LinearGradient).unwrap()
            else {
                panic!("expected a linear gradient");
            };
            assert_eq!(g.stops.len(), 1);
            assert_eq!(offsets(&g.stops), alloc::vec![0.0]);

            // A direction with no stops at all -> zero stops, still Ok.
            let StyleBackgroundContent::LinearGradient(g) =
                parse_gradient("to right", GradientType::LinearGradient).unwrap()
            else {
                panic!("expected a linear gradient");
            };
            assert_eq!(g.stops.len(), 0);

            // Same for a radial gradient that only names a shape.
            let StyleBackgroundContent::RadialGradient(g) =
                parse_gradient("circle", GradientType::RadialGradient).unwrap()
            else {
                panic!("expected a radial gradient");
            };
            assert_eq!(g.shape, Shape::Circle);
            assert_eq!(g.stops.len(), 0);
        }

        #[test]
        fn autotest_parse_gradient_never_panics_on_adversarial_input() {
            let huge = "a".repeat(100_000);
            let nested = "(".repeat(10_000) + &")".repeat(10_000);
            let many_commas = ",".repeat(10_000);
            for t in ALL_GRADIENT_TYPES {
                for input in ADVERSARIAL {
                    let a = parse_gradient(input, t);
                    let b = parse_gradient(input, t);
                    assert_eq!(a, b, "non-deterministic for {input:?} / {t:?}");
                }
                for input in [huge.as_str(), nested.as_str(), many_commas.as_str()] {
                    let a = parse_gradient(input, t);
                    let b = parse_gradient(input, t);
                    assert_eq!(a, b, "non-deterministic for a long input / {t:?}");
                }
                // An empty body is rejected for every gradient type.
                assert!(parse_gradient("", t).is_err(), "empty body accepted for {t:?}");
                // A comma-only body has nothing but empty stops -> always an error.
                assert!(
                    parse_gradient(&many_commas, t).is_err(),
                    "comma soup accepted for {t:?}"
                );
            }
            // Linear and conic reject junk outright. (Radial does not -- see
            // `autotest_radial_gradient_silently_drops_unparseable_items`.)
            for t in [
                GradientType::LinearGradient,
                GradientType::RepeatingLinearGradient,
                GradientType::ConicGradient,
                GradientType::RepeatingConicGradient,
            ] {
                assert!(parse_gradient(&huge, t).is_err(), "junk accepted for {t:?}");
                assert!(parse_gradient(&nested, t).is_err(), "junk accepted for {t:?}");
                assert!(parse_gradient("!!!", t).is_err(), "junk accepted for {t:?}");
            }
        }

        #[test]
        fn autotest_radial_gradient_silently_drops_unparseable_items() {
            // BUG (pinned): in the radial branch, a comma-item that is neither a
            // shape/size/position *nor* a valid color stop is skipped rather than
            // rejected -- so pure garbage parses as a gradient with no stops, and a
            // junk leading item simply disappears.
            let StyleBackgroundContent::RadialGradient(g) =
                parse_gradient("!!!", GradientType::RadialGradient).unwrap()
            else {
                panic!("expected a radial gradient");
            };
            assert_eq!(g.stops.len(), 0);

            let g = radial("radial-gradient(!!!, red)");
            assert_eq!(g.stops.len(), 1, "the junk item should have been dropped");
            assert_eq!(
                g.stops.as_ref()[0].color,
                ColorOrSystem::Color(ColorU::RED)
            );

            // The same input is a hard error for a linear gradient.
            assert!(parse_style_background_content("linear-gradient(!!!, red)").is_err());
        }

        #[test]
        fn autotest_radial_gradient_position_is_ignored_when_combined_with_a_shape() {
            // BUG (pinned): `parse_style_background_position` is handed the *whole*
            // comma-item ("circle at 50% 50%"), which has 4 whitespace components and
            // therefore fails -- so the `at <position>` part is silently dropped and
            // the position stays at its Left/Top default.
            let g = radial("radial-gradient(circle at 50% 50%, red, blue)");
            assert_eq!(g.shape, Shape::Circle);
            assert_eq!(g.position, StyleBackgroundPosition::default());
            assert_eq!(g.stops.len(), 2);

            // A position on its own (no shape/size in the same item) *is* honoured.
            let g = radial("radial-gradient(50% 50%, red, blue)");
            assert_eq!(
                g.position,
                StyleBackgroundPosition {
                    horizontal: BackgroundPositionHorizontal::Exact(PixelValue::percent(50.0)),
                    vertical: BackgroundPositionVertical::Exact(PixelValue::percent(50.0)),
                }
            );
            assert_eq!(g.stops.len(), 2);
        }

        #[test]
        fn autotest_radial_gradient_shape_and_size_keywords() {
            let g = radial("radial-gradient(circle closest-side, red, blue)");
            assert_eq!(g.shape, Shape::Circle);
            assert_eq!(g.size, RadialGradientSize::ClosestSide);

            let g = radial("radial-gradient(ellipse farthest-side, red, blue)");
            assert_eq!(g.shape, Shape::Ellipse);
            assert_eq!(g.size, RadialGradientSize::FarthestSide);

            // Defaults when nothing is named.
            let g = radial("radial-gradient(red, blue)");
            assert_eq!(g.shape, Shape::default());
            assert_eq!(g.size, RadialGradientSize::default());
        }

        // ---------------------------------------------------------------
        // parser: parse_linear_color_stop / parse_radial_color_stop (private)
        // ---------------------------------------------------------------

        #[test]
        fn autotest_parse_linear_color_stop_valid_minimal() {
            let s = parse_linear_color_stop("red").unwrap();
            assert_eq!(s.color, ColorOrSystem::Color(ColorU::RED));
            assert_eq!(s.offset1, OptionPercentageValue::None);
            assert_eq!(s.offset2, OptionPercentageValue::None);

            let s = parse_linear_color_stop("  red 50%  ").unwrap();
            assert_eq!(
                s.offset1,
                OptionPercentageValue::Some(PercentageValue::new(50.0))
            );
            assert_eq!(s.offset2, OptionPercentageValue::None);

            let s = parse_linear_color_stop("red 10% 30%").unwrap();
            assert_eq!(
                s.offset1,
                OptionPercentageValue::Some(PercentageValue::new(10.0))
            );
            assert_eq!(
                s.offset2,
                OptionPercentageValue::Some(PercentageValue::new(30.0))
            );

            // Colors that themselves contain spaces and digits still split correctly.
            let s = parse_linear_color_stop("rgba(0, 0, 0, 0.5) 50%").unwrap();
            assert_eq!(
                s.offset1,
                OptionPercentageValue::Some(PercentageValue::new(50.0))
            );

            // System colors are accepted as stop colors.
            let s = parse_linear_color_stop("system:accent 50%").unwrap();
            assert_eq!(s.color, ColorOrSystem::System(SystemColorRef::Accent));
        }

        #[test]
        fn autotest_parse_linear_color_stop_rejects_junk() {
            for input in [
                "",
                "   ",
                "\t\n",
                "!!!",
                "\u{1F600}",
                "red 50px",       // offset must be a percentage
                "red 0.5",        // bare number is not recognised as an offset
                "red 10% 20% 30%", // three offsets -> the color part is junk
                "red blue",
            ] {
                assert!(
                    parse_linear_color_stop(input).is_err(),
                    "{input:?} unexpectedly parsed"
                );
            }
            let huge = "a".repeat(100_000);
            assert!(parse_linear_color_stop(&huge).is_err());
        }

        #[test]
        fn autotest_parse_radial_color_stop_valid_and_junk() {
            let s = parse_radial_color_stop("red").unwrap();
            assert_eq!(s.color, ColorOrSystem::Color(ColorU::RED));
            assert_eq!(s.offset1, OptionAngleValue::None);

            let s = parse_radial_color_stop("red 90deg").unwrap();
            assert_eq!(s.offset1, OptionAngleValue::Some(AngleValue::deg(90.0)));
            assert_eq!(s.offset2, OptionAngleValue::None);

            let s = parse_radial_color_stop("red 45deg 90deg").unwrap();
            assert_eq!(s.offset1, OptionAngleValue::Some(AngleValue::deg(45.0)));
            assert_eq!(s.offset2, OptionAngleValue::Some(AngleValue::deg(90.0)));

            // Pinned: a *percentage* is a valid angle for a conic stop.
            assert!(parse_radial_color_stop("red 50%").is_ok());

            for input in ["", "   ", "!!!", "\u{1F600}", "red 5", "red 90deg 45deg 10deg"] {
                assert!(
                    parse_radial_color_stop(input).is_err(),
                    "{input:?} unexpectedly parsed"
                );
            }
            let huge = "a".repeat(100_000);
            assert!(parse_radial_color_stop(&huge).is_err());
        }

        // ---------------------------------------------------------------
        // other: split_color_and_offsets / try_split_last_offset (private)
        // ---------------------------------------------------------------

        #[test]
        fn autotest_split_color_and_offsets_w3c_shapes() {
            assert_eq!(split_color_and_offsets("red"), ("red", None, None));
            assert_eq!(
                split_color_and_offsets("red 50%"),
                ("red", Some("50%"), None)
            );
            assert_eq!(
                split_color_and_offsets("red 10% 30%"),
                ("red", Some("10%"), Some("30%"))
            );
            assert_eq!(
                split_color_and_offsets("rgba(0, 0, 0, 0.5) 10% 30%"),
                ("rgba(0, 0, 0, 0.5)", Some("10%"), Some("30%"))
            );
            // A direction is never mistaken for offsets (no digits).
            assert_eq!(
                split_color_and_offsets("to right bottom"),
                ("to right bottom", None, None)
            );
        }

        #[test]
        fn autotest_split_color_and_offsets_never_panics_on_edges() {
            assert_eq!(split_color_and_offsets(""), ("", None, None));
            assert_eq!(split_color_and_offsets("   "), ("", None, None));
            // Multibyte input: splitting must land on a char boundary.
            assert_eq!(
                split_color_and_offsets("\u{1F600} 50%"),
                ("\u{1F600}", Some("50%"), None)
            );
            // A non-breaking space counts as whitespace for rfind *and* for trim.
            assert_eq!(
                split_color_and_offsets("red\u{00a0}50%"),
                ("red", Some("50%"), None)
            );
            let huge = "a".repeat(100_000);
            assert_eq!(split_color_and_offsets(&huge), (huge.as_str(), None, None));

            for input in ADVERSARIAL {
                assert_eq!(
                    split_color_and_offsets(input),
                    split_color_and_offsets(input)
                );
            }
        }

        #[test]
        fn autotest_try_split_last_offset() {
            assert_eq!(try_split_last_offset("red 50%"), Some(("red", "50%")));
            assert_eq!(try_split_last_offset("red 10px"), Some(("red", "10px")));
            // No whitespace -> nothing to split off.
            assert_eq!(try_split_last_offset("50%"), None);
            // Not offset-shaped.
            assert_eq!(try_split_last_offset("red blue"), None);
            assert_eq!(try_split_last_offset("red 5"), None);
            assert_eq!(try_split_last_offset("to right"), None);
            // Empty / whitespace-only.
            assert_eq!(try_split_last_offset(""), None);
            assert_eq!(try_split_last_offset("   "), None);
            assert_eq!(try_split_last_offset("\t\n"), None);

            for input in ADVERSARIAL {
                assert_eq!(try_split_last_offset(input), try_split_last_offset(input));
            }
        }

        // ---------------------------------------------------------------
        // predicate: is_likely_offset (private)
        // ---------------------------------------------------------------

        #[test]
        fn autotest_is_likely_offset_basic_true_false() {
            for s in [
                "50%", "10px", "0.5turn", "90deg", "1rem", "2vmin", "3vmax", "4grad", "5rad",
                "-50%", "1e40%",
            ] {
                assert!(is_likely_offset(s), "{s:?} should look like an offset");
            }
            for s in [
                "", " ", "red", "px", "%", "5", "0.5", "NaN%", "to", "right", "\u{1F600}",
                "contain",
            ] {
                assert!(!is_likely_offset(s), "{s:?} should not look like an offset");
            }
        }

        #[test]
        fn autotest_is_likely_offset_is_a_shape_check_not_a_validator() {
            // Pinned: it only requires "contains an ASCII digit" + "ends with a unit",
            // so plainly invalid tokens pass. The real parse still rejects them.
            assert!(is_likely_offset("abc1px"));
            assert!(is_likely_offset("\u{1F600}5%"));
            assert!(is_likely_offset("--1--px"));
            assert!(is_likely_offset("1%%%"));
            // ... and the digit check must be ASCII: an Arabic-Indic digit does not
            // count, so this is *not* treated as an offset.
            assert!(!is_likely_offset("\u{0661}%"));

            let huge = "9".repeat(100_000) + "%";
            assert!(is_likely_offset(&huge));
            for input in ADVERSARIAL {
                assert_eq!(is_likely_offset(input), is_likely_offset(input));
            }
        }

        // ---------------------------------------------------------------
        // parser: parse_conic_first_item (private)
        // ---------------------------------------------------------------

        #[test]
        fn autotest_parse_conic_first_item_valid_and_absent() {
            // Not a "from ..." prelude -> Ok(None), so the item is a color stop.
            assert_eq!(parse_conic_first_item("").unwrap(), None);
            assert_eq!(parse_conic_first_item("red").unwrap(), None);
            assert_eq!(parse_conic_first_item("   ").unwrap(), None);

            let (angle, pos) = parse_conic_first_item("from 90deg").unwrap().unwrap();
            assert_eq!(angle, AngleValue::deg(90.0));
            assert_eq!(pos, StyleBackgroundPosition::default());

            let (angle, pos) = parse_conic_first_item("from 0deg at center").unwrap().unwrap();
            assert_eq!(angle, AngleValue::deg(0.0));
            assert_eq!(pos.horizontal, BackgroundPositionHorizontal::Center);
            assert_eq!(pos.vertical, BackgroundPositionVertical::Center);
        }

        #[test]
        fn autotest_parse_conic_first_item_rejects_malformed_preludes() {
            // "from" with no angle.
            assert!(parse_conic_first_item("from").is_err());
            assert!(parse_conic_first_item("from at center").is_err());
            // Pinned quirk: any token *starting with* "from" enters the prelude branch,
            // so a would-be color like "fromage" becomes an angle error.
            assert!(parse_conic_first_item("fromage").is_err());
            // Too many position components.
            assert!(matches!(
                parse_conic_first_item("from 90deg at left top center"),
                Err(CssConicGradientParseError::Position(_))
            ));

            let huge = alloc::format!("from {}", "9".repeat(100_000));
            let _ = parse_conic_first_item(&huge);
            for input in ADVERSARIAL {
                let a = parse_conic_first_item(input);
                let b = parse_conic_first_item(input);
                assert_eq!(a, b, "non-deterministic for {input:?}");
            }
        }

        #[test]
        fn autotest_conic_gradient_end_to_end() {
            let g = conic("conic-gradient(from 45deg, red, blue)");
            assert_eq!(g.angle, AngleValue::deg(45.0));
            assert_eq!(g.extend_mode, ExtendMode::Clamp);
            assert_eq!(g.stops.len(), 2);
            assert_eq!(g.stops.as_ref()[0].angle.to_degrees_raw(), 0.0);
            assert_eq!(g.stops.as_ref()[1].angle.to_degrees_raw(), 360.0);

            // Conic stop angles are monotonic and finite, even for silly inputs.
            for input in [
                "conic-gradient(red, blue)",
                "conic-gradient(red 0deg, blue 180deg, green 360deg)",
                "conic-gradient(red 180deg, blue 90deg)",
                "conic-gradient(red -90deg, blue)",
                "repeating-conic-gradient(red, blue 30deg)",
            ] {
                let g = conic(input);
                let mut prev = f32::NEG_INFINITY;
                for s in g.stops.iter() {
                    let deg = s.angle.to_degrees_raw();
                    assert!(deg.is_finite(), "non-finite angle in {input:?}");
                    assert!(deg >= prev, "angles not monotonic in {input:?}");
                    prev = deg;
                }
            }

            // An overflowing angle saturates instead of leaking inf into the stops.
            let g = conic("conic-gradient(red 1e40deg, blue)");
            assert_eq!(g.stops.len(), 2);
            for s in g.stops.iter() {
                assert!(s.angle.to_degrees_raw().is_finite());
            }

            assert!(parse_style_background_content("conic-gradient(from, red)").is_err());
        }

        // ---------------------------------------------------------------
        // parser: parse_background_position_{horizontal,vertical} (private)
        // ---------------------------------------------------------------

        #[test]
        fn autotest_parse_background_position_horizontal() {
            assert_eq!(
                parse_background_position_horizontal("left").unwrap(),
                BackgroundPositionHorizontal::Left
            );
            assert_eq!(
                parse_background_position_horizontal("center").unwrap(),
                BackgroundPositionHorizontal::Center
            );
            assert_eq!(
                parse_background_position_horizontal("right").unwrap(),
                BackgroundPositionHorizontal::Right
            );
            assert_eq!(
                parse_background_position_horizontal("10px").unwrap(),
                BackgroundPositionHorizontal::Exact(PixelValue::px(10.0))
            );
            // Vertical keywords are not horizontal ones.
            assert!(parse_background_position_horizontal("top").is_err());
            // Pinned: no trimming here (callers pass whitespace-split tokens).
            assert!(parse_background_position_horizontal(" left").is_err());
            assert!(parse_background_position_horizontal("").is_err());
            assert!(parse_background_position_horizontal("\u{1F600}").is_err());

            let huge = "a".repeat(100_000);
            assert!(parse_background_position_horizontal(&huge).is_err());
        }

        #[test]
        fn autotest_parse_background_position_vertical() {
            assert_eq!(
                parse_background_position_vertical("top").unwrap(),
                BackgroundPositionVertical::Top
            );
            assert_eq!(
                parse_background_position_vertical("center").unwrap(),
                BackgroundPositionVertical::Center
            );
            assert_eq!(
                parse_background_position_vertical("bottom").unwrap(),
                BackgroundPositionVertical::Bottom
            );
            assert_eq!(
                parse_background_position_vertical("-10px").unwrap(),
                BackgroundPositionVertical::Exact(PixelValue::px(-10.0))
            );
            assert!(parse_background_position_vertical("left").is_err());
            assert!(parse_background_position_vertical("").is_err());
            assert!(parse_background_position_vertical("\u{1F600}").is_err());

            let huge = "a".repeat(100_000);
            assert!(parse_background_position_vertical(&huge).is_err());
        }

        // ---------------------------------------------------------------
        // parser: parse_shape / parse_radial_gradient_size (private)
        // ---------------------------------------------------------------

        #[test]
        fn autotest_parse_shape() {
            assert_eq!(parse_shape("circle").unwrap(), Shape::Circle);
            assert_eq!(parse_shape("  ellipse  ").unwrap(), Shape::Ellipse);
            for input in ["", "   ", "Circle", "CIRCLE", "circles", "!!!", "\u{1F600}", "0"] {
                assert!(parse_shape(input).is_err(), "{input:?} unexpectedly parsed");
            }
            let huge = "a".repeat(100_000);
            assert!(parse_shape(&huge).is_err());
            for input in ADVERSARIAL {
                assert_eq!(parse_shape(input), parse_shape(input));
            }
        }

        #[test]
        fn autotest_parse_radial_gradient_size() {
            assert_eq!(
                parse_radial_gradient_size("closest-side").unwrap(),
                RadialGradientSize::ClosestSide
            );
            assert_eq!(
                parse_radial_gradient_size("  closest-corner ").unwrap(),
                RadialGradientSize::ClosestCorner
            );
            assert_eq!(
                parse_radial_gradient_size("farthest-side").unwrap(),
                RadialGradientSize::FarthestSide
            );
            assert_eq!(
                parse_radial_gradient_size("farthest-corner").unwrap(),
                RadialGradientSize::FarthestCorner
            );
            for input in [
                "",
                "   ",
                "closest",
                "CLOSEST-SIDE",
                "farthest-corners",
                "!!!",
                "\u{1F600}",
            ] {
                assert!(
                    parse_radial_gradient_size(input).is_err(),
                    "{input:?} unexpectedly parsed"
                );
            }
            let huge = "a".repeat(100_000);
            assert!(parse_radial_gradient_size(&huge).is_err());
        }

        // ---------------------------------------------------------------
        // round-trips: print_as_css_value -> parse
        // ---------------------------------------------------------------

        #[test]
        fn autotest_round_trip_background_repeat() {
            for r in [
                StyleBackgroundRepeat::NoRepeat,
                StyleBackgroundRepeat::PatternRepeat,
                StyleBackgroundRepeat::RepeatX,
                StyleBackgroundRepeat::RepeatY,
            ] {
                let printed = r.print_as_css_value();
                assert!(!printed.is_empty());
                assert_eq!(parse_style_background_repeat(&printed).unwrap(), r);
            }
        }

        #[test]
        fn autotest_round_trip_background_size() {
            for s in [
                StyleBackgroundSize::Contain,
                StyleBackgroundSize::Cover,
                StyleBackgroundSize::ExactSize(PixelValueSize {
                    width: PixelValue::px(100.0),
                    height: PixelValue::em(20.0),
                }),
                StyleBackgroundSize::ExactSize(PixelValueSize {
                    width: PixelValue::percent(50.0),
                    height: PixelValue::percent(50.0),
                }),
                StyleBackgroundSize::ExactSize(PixelValueSize {
                    width: PixelValue::px(0.0),
                    height: PixelValue::px(-25.5),
                }),
            ] {
                let printed = s.print_as_css_value();
                assert_eq!(
                    parse_style_background_size(&printed).unwrap(),
                    s,
                    "round-trip failed for {printed:?}"
                );
            }
        }

        #[test]
        fn autotest_round_trip_background_position() {
            let horizontals = [
                BackgroundPositionHorizontal::Left,
                BackgroundPositionHorizontal::Center,
                BackgroundPositionHorizontal::Right,
                BackgroundPositionHorizontal::Exact(PixelValue::px(50.0)),
                BackgroundPositionHorizontal::Exact(PixelValue::percent(25.0)),
            ];
            let verticals = [
                BackgroundPositionVertical::Top,
                BackgroundPositionVertical::Center,
                BackgroundPositionVertical::Bottom,
                BackgroundPositionVertical::Exact(PixelValue::px(-10.0)),
                BackgroundPositionVertical::Exact(PixelValue::em(2.5)),
            ];
            for horizontal in horizontals {
                for vertical in verticals {
                    let pos = StyleBackgroundPosition {
                        horizontal,
                        vertical,
                    };
                    let printed = pos.print_as_css_value();
                    assert_eq!(
                        parse_style_background_position(&printed).unwrap(),
                        pos,
                        "round-trip failed for {printed:?}"
                    );
                }
            }
        }

        #[test]
        fn autotest_round_trip_background_content_colors_and_images() {
            for content in [
                StyleBackgroundContent::Color(ColorU::RED),
                StyleBackgroundContent::Color(ColorU::TRANSPARENT),
                StyleBackgroundContent::Color(ColorU::rgba(1, 2, 3, 4)),
                StyleBackgroundContent::Color(ColorU::WHITE),
                StyleBackgroundContent::Image("a.png".into()),
                StyleBackgroundContent::Image("some/deep/path.jpeg".into()),
                StyleBackgroundContent::SystemColor(SystemColorRef::Accent),
                StyleBackgroundContent::SystemColor(SystemColorRef::SelectionText),
            ] {
                let printed = content.print_as_css_value();
                assert_eq!(
                    parse_style_background_content(&printed).unwrap(),
                    content,
                    "round-trip failed for {printed:?}"
                );
            }
            assert_eq!(
                StyleBackgroundContent::default(),
                StyleBackgroundContent::Color(ColorU::TRANSPARENT)
            );
        }

        #[test]
        fn autotest_round_trip_gradients() {
            for input in [
                "linear-gradient(to right, red 0%, blue 100%)",
                "repeating-linear-gradient(to bottom, red 25%, blue 75%)",
                "linear-gradient(45deg, red 0%, blue 50%)",
                "radial-gradient(circle farthest-corner at left top, red 0%, blue 100%)",
                "conic-gradient(from 90deg at left top, red 0deg, blue 360deg)",
                "repeating-conic-gradient(from 0deg at left top, red 0deg, blue 180deg)",
            ] {
                let parsed = parse_style_background_content(input).unwrap();
                let printed = parsed.print_as_css_value();
                let reparsed = parse_style_background_content(&printed).unwrap();
                assert_eq!(
                    parsed, reparsed,
                    "gradient did not survive print -> parse ({printed:?})"
                );
                // ... and printing is stable across the round-trip.
                assert_eq!(printed, reparsed.print_as_css_value());
            }
        }

        #[test]
        fn autotest_round_trip_vec_printing_is_comma_separated() {
            let contents = parse_style_background_content_multiple("red, blue").unwrap();
            assert_eq!(contents.print_as_css_value(), "#ff0000ff, #0000ffff");
            assert_eq!(contents.as_slice()[1], StyleBackgroundContent::Color(blue()));
            let reparsed =
                parse_style_background_content_multiple(&contents.print_as_css_value()).unwrap();
            assert_eq!(reparsed, contents);

            let sizes = parse_style_background_size_multiple("contain, 10px 20px").unwrap();
            assert_eq!(
                parse_style_background_size_multiple(&sizes.print_as_css_value()).unwrap(),
                sizes
            );

            let repeats = parse_style_background_repeat_multiple("repeat, no-repeat").unwrap();
            assert_eq!(
                parse_style_background_repeat_multiple(&repeats.print_as_css_value()).unwrap(),
                repeats
            );

            let positions = parse_style_background_position_multiple("left top, 10px 20px").unwrap();
            assert_eq!(
                parse_style_background_position_multiple(&positions.print_as_css_value()).unwrap(),
                positions
            );
        }

        #[test]
        fn autotest_normalized_stop_printing_is_reparseable() {
            let stop = NormalizedLinearColorStop::new(PercentageValue::new(25.0), ColorU::RED);
            assert_eq!(stop.print_as_css_value(), "#ff0000ff 25%");
            let reparsed = parse_linear_color_stop(&stop.print_as_css_value()).unwrap();
            assert_eq!(reparsed.color, stop.color);
            assert_eq!(
                reparsed.offset1,
                OptionPercentageValue::Some(PercentageValue::new(25.0))
            );

            let rstop = NormalizedRadialColorStop::new(AngleValue::deg(90.0), blue());
            assert_eq!(rstop.print_as_css_value(), "#0000ffff 90deg");
            let reparsed = parse_radial_color_stop(&rstop.print_as_css_value()).unwrap();
            assert_eq!(reparsed.color, rstop.color);
            assert_eq!(
                reparsed.offset1,
                OptionAngleValue::Some(AngleValue::deg(90.0))
            );

            // System-colored stops print their `system:*` name and re-parse.
            let sys = NormalizedLinearColorStop {
                offset: PercentageValue::new(50.0),
                color: ColorOrSystem::System(SystemColorRef::Accent),
            };
            assert_eq!(sys.print_as_css_value(), "system:accent 50%");
            assert_eq!(
                parse_linear_color_stop(&sys.print_as_css_value()).unwrap().color,
                ColorOrSystem::System(SystemColorRef::Accent)
            );
        }

        #[test]
        fn autotest_empty_gradient_printing_does_not_panic() {
            // Default gradients have zero stops -- printing must still produce
            // something well-formed (and not, say, index out of bounds).
            let lg = StyleBackgroundContent::LinearGradient(LinearGradient::default());
            assert!(lg.print_as_css_value().starts_with("linear-gradient("));

            let rg = StyleBackgroundContent::RadialGradient(RadialGradient::default());
            assert!(rg.print_as_css_value().starts_with("radial-gradient("));

            let cg = StyleBackgroundContent::ConicGradient(ConicGradient::default());
            assert!(cg.print_as_css_value().starts_with("conic-gradient("));

            // A gradient built from an empty stop list is also printable.
            let empty_stops = StyleBackgroundContent::LinearGradient(LinearGradient {
                extend_mode: ExtendMode::Repeat,
                stops: Vec::<NormalizedLinearColorStop>::new().into(),
                ..LinearGradient::default()
            });
            assert!(empty_stops
                .print_as_css_value()
                .starts_with("repeating-linear-gradient("));
        }
    }
}

#[cfg(feature = "parser")]
pub use self::parser::*;

#[cfg(all(test, feature = "parser"))]
mod tests {
    use super::*;
    use crate::props::basic::{DirectionCorner, DirectionCorners};

    #[test]
    fn test_parse_single_background_content() {
        // Color
        assert_eq!(
            parse_style_background_content("red").unwrap(),
            StyleBackgroundContent::Color(ColorU::RED)
        );
        assert_eq!(
            parse_style_background_content("#ff00ff").unwrap(),
            StyleBackgroundContent::Color(ColorU::new_rgb(255, 0, 255))
        );

        // Image
        assert_eq!(
            parse_style_background_content("url(\"image.png\")").unwrap(),
            StyleBackgroundContent::Image("image.png".into())
        );

        // Linear Gradient
        let lg = parse_style_background_content("linear-gradient(to right, red, blue)").unwrap();
        assert!(matches!(lg, StyleBackgroundContent::LinearGradient(_)));
        if let StyleBackgroundContent::LinearGradient(grad) = lg {
            assert_eq!(grad.stops.len(), 2);
            assert_eq!(
                grad.direction,
                Direction::FromTo(DirectionCorners {
                    dir_from: DirectionCorner::Left,
                    dir_to: DirectionCorner::Right
                })
            );
        }

        // Radial Gradient
        let rg = parse_style_background_content("radial-gradient(circle, white, black)").unwrap();
        assert!(matches!(rg, StyleBackgroundContent::RadialGradient(_)));
        if let StyleBackgroundContent::RadialGradient(grad) = rg {
            assert_eq!(grad.stops.len(), 2);
            assert_eq!(grad.shape, Shape::Circle);
        }

        // Conic Gradient
        let cg = parse_style_background_content("conic-gradient(from 90deg, red, blue)").unwrap();
        assert!(matches!(cg, StyleBackgroundContent::ConicGradient(_)));
        if let StyleBackgroundContent::ConicGradient(grad) = cg {
            assert_eq!(grad.stops.len(), 2);
            assert_eq!(grad.angle, AngleValue::deg(90.0));
        }
    }

    #[test]
    fn test_parse_multiple_background_content() {
        let result =
            parse_style_background_content_multiple("url(foo.png), linear-gradient(red, blue)")
                .unwrap();
        assert_eq!(result.len(), 2);
        assert!(matches!(
            result.as_slice()[0],
            StyleBackgroundContent::Image(_)
        ));
        assert!(matches!(
            result.as_slice()[1],
            StyleBackgroundContent::LinearGradient(_)
        ));
    }

    #[test]
    fn test_parse_background_position() {
        // One value
        let result = parse_style_background_position("center").unwrap();
        assert_eq!(result.horizontal, BackgroundPositionHorizontal::Center);
        assert_eq!(result.vertical, BackgroundPositionVertical::Center);

        let result = parse_style_background_position("25%").unwrap();
        assert_eq!(
            result.horizontal,
            BackgroundPositionHorizontal::Exact(PixelValue::percent(25.0))
        );
        assert_eq!(result.vertical, BackgroundPositionVertical::Center);

        // Two values
        let result = parse_style_background_position("right 50px").unwrap();
        assert_eq!(result.horizontal, BackgroundPositionHorizontal::Right);
        assert_eq!(
            result.vertical,
            BackgroundPositionVertical::Exact(PixelValue::px(50.0))
        );

        // Four values (not supported by this parser, should fail)
        assert!(parse_style_background_position("left 10px top 20px").is_err());
    }

    #[test]
    fn test_parse_background_size() {
        assert_eq!(
            parse_style_background_size("contain").unwrap(),
            StyleBackgroundSize::Contain
        );
        assert_eq!(
            parse_style_background_size("cover").unwrap(),
            StyleBackgroundSize::Cover
        );
        assert_eq!(
            parse_style_background_size("50%").unwrap(),
            StyleBackgroundSize::ExactSize(PixelValueSize {
                width: PixelValue::percent(50.0),
                height: PixelValue::percent(50.0)
            })
        );
        assert_eq!(
            parse_style_background_size("100px 20em").unwrap(),
            StyleBackgroundSize::ExactSize(PixelValueSize {
                width: PixelValue::px(100.0),
                height: PixelValue::em(20.0)
            })
        );
        assert!(parse_style_background_size("auto").is_err());
    }

    #[test]
    fn test_parse_background_repeat() {
        assert_eq!(
            parse_style_background_repeat("repeat").unwrap(),
            StyleBackgroundRepeat::PatternRepeat
        );
        assert_eq!(
            parse_style_background_repeat("repeat-x").unwrap(),
            StyleBackgroundRepeat::RepeatX
        );
        assert_eq!(
            parse_style_background_repeat("repeat-y").unwrap(),
            StyleBackgroundRepeat::RepeatY
        );
        assert_eq!(
            parse_style_background_repeat("no-repeat").unwrap(),
            StyleBackgroundRepeat::NoRepeat
        );
        assert!(parse_style_background_repeat("repeat-xy").is_err());
    }

    // =========================================================================
    // W3C CSS Images Level 3 - Gradient Parsing Tests
    // =========================================================================

    #[test]
    fn test_gradient_no_position_stops() {
        // "linear-gradient(red, blue)" - no positions specified
        let lg = parse_style_background_content("linear-gradient(red, blue)").unwrap();
        if let StyleBackgroundContent::LinearGradient(grad) = lg {
            assert_eq!(grad.stops.len(), 2);
            // First stop should default to 0%
            assert!((grad.stops.as_ref()[0].offset.normalized() - 0.0).abs() < 0.001);
            // Last stop should default to 100%
            assert!((grad.stops.as_ref()[1].offset.normalized() - 1.0).abs() < 0.001);
        } else {
            panic!("Expected LinearGradient");
        }
    }

    #[test]
    fn test_gradient_single_position_stops() {
        // "linear-gradient(red 25%, blue 75%)" - one position per stop
        let lg = parse_style_background_content("linear-gradient(red 25%, blue 75%)").unwrap();
        if let StyleBackgroundContent::LinearGradient(grad) = lg {
            assert_eq!(grad.stops.len(), 2);
            assert!((grad.stops.as_ref()[0].offset.normalized() - 0.25).abs() < 0.001);
            assert!((grad.stops.as_ref()[1].offset.normalized() - 0.75).abs() < 0.001);
        } else {
            panic!("Expected LinearGradient");
        }
    }

    #[test]
    fn test_gradient_multi_position_stops() {
        // "linear-gradient(red 10% 30%, blue)" - two positions create two stops
        let lg = parse_style_background_content("linear-gradient(red 10% 30%, blue)").unwrap();
        if let StyleBackgroundContent::LinearGradient(grad) = lg {
            // Should have 3 stops: red@10%, red@30%, blue@100%
            assert_eq!(grad.stops.len(), 3, "Expected 3 stops for multi-position");
            assert!((grad.stops.as_ref()[0].offset.normalized() - 0.10).abs() < 0.001);
            assert!((grad.stops.as_ref()[1].offset.normalized() - 0.30).abs() < 0.001);
            assert!((grad.stops.as_ref()[2].offset.normalized() - 1.0).abs() < 0.001);
            // Both first two stops should have same color (red)
            assert_eq!(grad.stops.as_ref()[0].color, grad.stops.as_ref()[1].color);
        } else {
            panic!("Expected LinearGradient");
        }
    }

    #[test]
    fn test_gradient_three_colors_no_positions() {
        // "linear-gradient(red, green, blue)" - evenly distributed
        let lg = parse_style_background_content("linear-gradient(red, green, blue)").unwrap();
        if let StyleBackgroundContent::LinearGradient(grad) = lg {
            assert_eq!(grad.stops.len(), 3);
            // Positions: 0%, 50%, 100%
            assert!((grad.stops.as_ref()[0].offset.normalized() - 0.0).abs() < 0.001);
            assert!((grad.stops.as_ref()[1].offset.normalized() - 0.5).abs() < 0.001);
            assert!((grad.stops.as_ref()[2].offset.normalized() - 1.0).abs() < 0.001);
        } else {
            panic!("Expected LinearGradient");
        }
    }

    #[test]
    fn test_gradient_fixup_ascending_order() {
        // "linear-gradient(red 50%, blue 20%)" - blue position < red position
        // W3C says: clamp to max of previous positions
        let lg = parse_style_background_content("linear-gradient(red 50%, blue 20%)").unwrap();
        if let StyleBackgroundContent::LinearGradient(grad) = lg {
            assert_eq!(grad.stops.len(), 2);
            // First stop at 50%
            assert!((grad.stops.as_ref()[0].offset.normalized() - 0.50).abs() < 0.001);
            // Second stop clamped to 50% (not 20%)
            assert!((grad.stops.as_ref()[1].offset.normalized() - 0.50).abs() < 0.001);
        } else {
            panic!("Expected LinearGradient");
        }
    }

    #[test]
    fn test_gradient_distribute_unpositioned() {
        // "linear-gradient(red 0%, yellow, green, blue 100%)"
        // yellow and green should be distributed evenly between 0% and 100%
        let lg =
            parse_style_background_content("linear-gradient(red 0%, yellow, green, blue 100%)")
                .unwrap();
        if let StyleBackgroundContent::LinearGradient(grad) = lg {
            assert_eq!(grad.stops.len(), 4);
            // Positions: 0%, 33.3%, 66.6%, 100%
            assert!((grad.stops.as_ref()[0].offset.normalized() - 0.0).abs() < 0.001);
            assert!((grad.stops.as_ref()[1].offset.normalized() - 0.333).abs() < 0.01);
            assert!((grad.stops.as_ref()[2].offset.normalized() - 0.666).abs() < 0.01);
            assert!((grad.stops.as_ref()[3].offset.normalized() - 1.0).abs() < 0.001);
        } else {
            panic!("Expected LinearGradient");
        }
    }

    #[test]
    fn test_gradient_direction_to_corner() {
        // "linear-gradient(to top right, red, blue)"
        let lg =
            parse_style_background_content("linear-gradient(to top right, red, blue)").unwrap();
        if let StyleBackgroundContent::LinearGradient(grad) = lg {
            assert_eq!(
                grad.direction,
                Direction::FromTo(DirectionCorners {
                    dir_from: DirectionCorner::BottomLeft,
                    dir_to: DirectionCorner::TopRight
                })
            );
        } else {
            panic!("Expected LinearGradient");
        }
    }

    #[test]
    fn test_gradient_direction_angle() {
        // "linear-gradient(45deg, red, blue)"
        let lg = parse_style_background_content("linear-gradient(45deg, red, blue)").unwrap();
        if let StyleBackgroundContent::LinearGradient(grad) = lg {
            assert_eq!(grad.direction, Direction::Angle(AngleValue::deg(45.0)));
        } else {
            panic!("Expected LinearGradient");
        }
    }

    #[test]
    fn test_repeating_gradient() {
        // "repeating-linear-gradient(red, blue 20%)"
        let lg =
            parse_style_background_content("repeating-linear-gradient(red, blue 20%)").unwrap();
        if let StyleBackgroundContent::LinearGradient(grad) = lg {
            assert_eq!(grad.extend_mode, ExtendMode::Repeat);
        } else {
            panic!("Expected LinearGradient");
        }
    }

    #[test]
    fn test_radial_gradient_circle() {
        // "radial-gradient(circle, red, blue)"
        let rg = parse_style_background_content("radial-gradient(circle, red, blue)").unwrap();
        if let StyleBackgroundContent::RadialGradient(grad) = rg {
            assert_eq!(grad.shape, Shape::Circle);
            assert_eq!(grad.stops.len(), 2);
            // Check default position is center
            assert_eq!(grad.position.horizontal, BackgroundPositionHorizontal::Left);
            assert_eq!(grad.position.vertical, BackgroundPositionVertical::Top);
        } else {
            panic!("Expected RadialGradient");
        }
    }

    #[test]
    fn test_radial_gradient_ellipse() {
        // "radial-gradient(ellipse, red, blue)"
        let rg = parse_style_background_content("radial-gradient(ellipse, red, blue)").unwrap();
        if let StyleBackgroundContent::RadialGradient(grad) = rg {
            assert_eq!(grad.shape, Shape::Ellipse);
            assert_eq!(grad.stops.len(), 2);
        } else {
            panic!("Expected RadialGradient");
        }
    }

    #[test]
    fn test_radial_gradient_size_keywords() {
        // Test different size keywords
        let rg = parse_style_background_content("radial-gradient(circle closest-side, red, blue)")
            .unwrap();
        if let StyleBackgroundContent::RadialGradient(grad) = rg {
            assert_eq!(grad.shape, Shape::Circle);
            assert_eq!(grad.size, RadialGradientSize::ClosestSide);
        } else {
            panic!("Expected RadialGradient");
        }
    }

    #[test]
    fn test_radial_gradient_stop_positions() {
        // "radial-gradient(red 0%, blue 100%)"
        let rg = parse_style_background_content("radial-gradient(red 0%, blue 100%)").unwrap();
        if let StyleBackgroundContent::RadialGradient(grad) = rg {
            assert_eq!(grad.stops.len(), 2);
            assert!((grad.stops.as_ref()[0].offset.normalized() - 0.0).abs() < 0.001);
            assert!((grad.stops.as_ref()[1].offset.normalized() - 1.0).abs() < 0.001);
        } else {
            panic!("Expected RadialGradient");
        }
    }

    #[test]
    fn test_repeating_radial_gradient() {
        let rg = parse_style_background_content("repeating-radial-gradient(circle, red, blue 20%)")
            .unwrap();
        if let StyleBackgroundContent::RadialGradient(grad) = rg {
            assert_eq!(grad.extend_mode, ExtendMode::Repeat);
            assert_eq!(grad.shape, Shape::Circle);
        } else {
            panic!("Expected RadialGradient");
        }
    }

    #[test]
    fn test_conic_gradient_angle() {
        // "conic-gradient(from 45deg, red, blue)"
        let cg = parse_style_background_content("conic-gradient(from 45deg, red, blue)").unwrap();
        if let StyleBackgroundContent::ConicGradient(grad) = cg {
            assert_eq!(grad.angle, AngleValue::deg(45.0));
            assert_eq!(grad.stops.len(), 2);
        } else {
            panic!("Expected ConicGradient");
        }
    }

    #[test]
    fn test_conic_gradient_default() {
        // "conic-gradient(red, blue)" - no angle specified
        let cg = parse_style_background_content("conic-gradient(red, blue)").unwrap();
        if let StyleBackgroundContent::ConicGradient(grad) = cg {
            assert_eq!(grad.stops.len(), 2);
            // First stop defaults to 0deg
            assert!(
                (grad.stops.as_ref()[0].angle.to_degrees_raw() - 0.0).abs() < 0.001,
                "First stop should be 0deg, got {}",
                grad.stops.as_ref()[0].angle.to_degrees_raw()
            );
            // Last stop defaults to 360deg (use to_degrees_raw to preserve 360)
            assert!(
                (grad.stops.as_ref()[1].angle.to_degrees_raw() - 360.0).abs() < 0.001,
                "Last stop should be 360deg, got {}",
                grad.stops.as_ref()[1].angle.to_degrees_raw()
            );
        } else {
            panic!("Expected ConicGradient");
        }
    }

    #[test]
    fn test_conic_gradient_with_positions() {
        // "conic-gradient(red 0deg, blue 180deg, green 360deg)"
        let cg =
            parse_style_background_content("conic-gradient(red 0deg, blue 180deg, green 360deg)")
                .unwrap();
        if let StyleBackgroundContent::ConicGradient(grad) = cg {
            assert_eq!(grad.stops.len(), 3);
            // Use to_degrees_raw() to preserve 360deg
            assert!(
                (grad.stops.as_ref()[0].angle.to_degrees_raw() - 0.0).abs() < 0.001,
                "First stop should be 0deg, got {}",
                grad.stops.as_ref()[0].angle.to_degrees_raw()
            );
            assert!(
                (grad.stops.as_ref()[1].angle.to_degrees_raw() - 180.0).abs() < 0.001,
                "Second stop should be 180deg, got {}",
                grad.stops.as_ref()[1].angle.to_degrees_raw()
            );
            assert!(
                (grad.stops.as_ref()[2].angle.to_degrees_raw() - 360.0).abs() < 0.001,
                "Last stop should be 360deg, got {}",
                grad.stops.as_ref()[2].angle.to_degrees_raw()
            );
        } else {
            panic!("Expected ConicGradient");
        }
    }

    #[test]
    fn test_repeating_conic_gradient() {
        let cg =
            parse_style_background_content("repeating-conic-gradient(red, blue 30deg)").unwrap();
        if let StyleBackgroundContent::ConicGradient(grad) = cg {
            assert_eq!(grad.extend_mode, ExtendMode::Repeat);
        } else {
            panic!("Expected ConicGradient");
        }
    }

    #[test]
    fn test_gradient_with_rgba_color() {
        // Test parsing gradient with rgba color (contains spaces)
        let lg =
            parse_style_background_content("linear-gradient(rgba(255,0,0,0.5), blue)").unwrap();
        if let StyleBackgroundContent::LinearGradient(grad) = lg {
            assert_eq!(grad.stops.len(), 2);
            // First color should have alpha of ~128 (0.5 * 255, may be 127 or 128 due to rounding)
            let first_color = grad.stops.as_ref()[0].color.to_color_u_default();
            assert!(first_color.a >= 127 && first_color.a <= 128);
        } else {
            panic!("Expected LinearGradient");
        }
    }

    #[test]
    fn test_gradient_with_rgba_and_position() {
        // Test parsing "rgba(0,0,0,0.5) 50%"
        let lg =
            parse_style_background_content("linear-gradient(rgba(0,0,0,0.5) 50%, white)").unwrap();
        if let StyleBackgroundContent::LinearGradient(grad) = lg {
            assert_eq!(grad.stops.len(), 2);
            assert!((grad.stops.as_ref()[0].offset.normalized() - 0.5).abs() < 0.001);
        } else {
            panic!("Expected LinearGradient");
        }
    }

    #[test]
    fn test_gradient_resolves_system_color_stop() {
        // A `system:accent` stop should round-trip through the parser as a
        // System variant and resolve against a populated `SystemColors` to
        // the live accent color, falling back to the supplied default when
        // the key is unset.
        use crate::props::basic::color::ColorOrSystem;
        use crate::system::SystemColors;

        let lg = parse_style_background_content(
            "linear-gradient(red, system:accent)",
        )
        .unwrap();
        let StyleBackgroundContent::LinearGradient(grad) = lg else {
            panic!("Expected LinearGradient");
        };
        let stops = grad.stops.as_ref();
        assert_eq!(stops.len(), 2);

        let accent_stop = &stops[1];
        assert!(matches!(accent_stop.color, ColorOrSystem::System(_)));

        let populated = SystemColors {
            accent: crate::props::basic::color::OptionColorU::Some(ColorU::new_rgb(0, 122, 255)),
            ..SystemColors::default()
        };

        let resolved = accent_stop.resolve(&populated, ColorU::TRANSPARENT);
        assert_eq!(resolved, ColorU::new_rgb(0, 122, 255));

        let empty = SystemColors::default();
        let fallback = accent_stop.resolve(&empty, ColorU::TRANSPARENT);
        assert_eq!(fallback, ColorU::TRANSPARENT);
    }
}
