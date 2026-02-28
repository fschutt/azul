//! CSS properties for backgrounds, including colors, images, and gradients.

use alloc::{
    string::{String, ToString},
    vec::Vec,
};
use core::fmt;

#[cfg(feature = "parser")]
use crate::props::basic::{
    error::{InvalidValueErr, InvalidValueErrOwned},
    image::{parse_image, CssImageParseError, CssImageParseErrorOwned},
    parse::{
        parse_parentheses, split_string_respect_comma, ParenthesisParseError,
        ParenthesisParseErrorOwned,
    },
    color::parse_color_or_system,
};
use crate::{
    corety::AzString,
    format_rust_code::GetHash,
    props::{
        basic::{
            angle::{
                parse_angle_value, AngleValue, CssAngleValueParseError,
                CssAngleValueParseErrorOwned, OptionAngleValue,
            },
            color::{parse_css_color, ColorU, ColorOrSystem, CssColorParseError, CssColorParseErrorOwned},
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
pub enum ExtendMode {
    Clamp,
    Repeat,
}
impl Default for ExtendMode {
    fn default() -> Self {
        ExtendMode::Clamp
    }
}

// -- Main Background Content Type --

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
pub enum StyleBackgroundContent {
    LinearGradient(LinearGradient),
    RadialGradient(RadialGradient),
    ConicGradient(ConicGradient),
    Image(AzString),
    Color(ColorU),
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
    fn default() -> StyleBackgroundContent {
        StyleBackgroundContent::Color(ColorU::TRANSPARENT)
    }
}

impl PrintAsCssValue for StyleBackgroundContent {
    fn print_as_css_value(&self) -> String {
        match self {
            StyleBackgroundContent::LinearGradient(lg) => {
                let prefix = if lg.extend_mode == ExtendMode::Repeat {
                    "repeating-linear-gradient"
                } else {
                    "linear-gradient"
                };
                format!("{}({})", prefix, lg.print_as_css_value())
            }
            StyleBackgroundContent::RadialGradient(rg) => {
                let prefix = if rg.extend_mode == ExtendMode::Repeat {
                    "repeating-radial-gradient"
                } else {
                    "radial-gradient"
                };
                format!("{}({})", prefix, rg.print_as_css_value())
            }
            StyleBackgroundContent::ConicGradient(cg) => {
                let prefix = if cg.extend_mode == ExtendMode::Repeat {
                    "repeating-conic-gradient"
                } else {
                    "conic-gradient"
                };
                format!("{}({})", prefix, cg.print_as_css_value())
            }
            StyleBackgroundContent::Image(id) => format!("url(\"{}\")", id.as_str()),
            StyleBackgroundContent::Color(c) => c.to_hash(),
        }
    }
}

// Formatting to Rust code for background-related vecs

impl crate::format_rust_code::FormatAsRustCode for StyleBackgroundContent {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        // Delegate to the CSS value representation for single backgrounds
        format!("StyleBackgroundContent::from_css(\"{}\")", self.print_as_css_value())
    }
}

impl crate::format_rust_code::FormatAsRustCode for StyleBackgroundSizeVec {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!(
            "StyleBackgroundSizeVec::from_const_slice(STYLE_BACKGROUND_SIZE_{}_ITEMS)",
            self.get_hash()
        )
    }
}

impl crate::format_rust_code::FormatAsRustCode for StyleBackgroundRepeatVec {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!(
            "StyleBackgroundRepeatVec::from_const_slice(STYLE_BACKGROUND_REPEAT_{}_ITEMS)",
            self.get_hash()
        )
    }
}

impl crate::format_rust_code::FormatAsRustCode for StyleBackgroundContentVec {
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
            .map(|f| f.print_as_css_value())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

// -- Gradient Types --

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
            .map(|s| s.print_as_css_value())
            .collect::<Vec<_>>()
            .join(", ");
        if stops_str.is_empty() {
            dir_str
        } else {
            format!("{}, {}", dir_str, stops_str)
        }
    }
}

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
            .map(|s| s.print_as_css_value())
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
            center: StyleBackgroundPosition::default(),
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
            .map(|s| s.print_as_css_value())
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

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum Shape {
    Ellipse,
    Circle,
}
impl Default for Shape {
    fn default() -> Self {
        Shape::Ellipse
    }
}
impl fmt::Display for Shape {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Shape::Ellipse => "ellipse",
                Shape::Circle => "circle",
            }
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum RadialGradientSize {
    ClosestSide,
    ClosestCorner,
    FarthestSide,
    FarthestCorner,
}
impl Default for RadialGradientSize {
    fn default() -> Self {
        RadialGradientSize::FarthestCorner
    }
}
impl fmt::Display for RadialGradientSize {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
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

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct NormalizedLinearColorStop {
    pub offset: PercentageValue,
    /// Color for this gradient stop. Can be a concrete color or a system color reference.
    pub color: ColorOrSystem,
}

impl NormalizedLinearColorStop {
    /// Create a new normalized linear color stop with a concrete color.
    pub const fn new(offset: PercentageValue, color: ColorU) -> Self {
        Self { offset, color: ColorOrSystem::color(color) }
    }
    
    /// Create a new normalized linear color stop with a system color.
    pub const fn system(offset: PercentageValue, system_ref: crate::props::basic::color::SystemColorRef) -> Self {
        Self { offset, color: ColorOrSystem::system(system_ref) }
    }
    
    /// Create a normalized linear color stop with a ColorOrSystem directly.
    pub const fn with_color_or_system(offset: PercentageValue, color: ColorOrSystem) -> Self {
        Self { offset, color }
    }
    
    /// Resolve the color against system colors.
    pub fn resolve(&self, system_colors: &crate::system::SystemColors, fallback: ColorU) -> ColorU {
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

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct NormalizedRadialColorStop {
    pub angle: AngleValue,
    /// Color for this gradient stop. Can be a concrete color or a system color reference.
    pub color: ColorOrSystem,
}

impl NormalizedRadialColorStop {
    /// Create a new normalized radial color stop with a concrete color.
    pub const fn new(angle: AngleValue, color: ColorU) -> Self {
        Self { angle, color: ColorOrSystem::color(color) }
    }
    
    /// Create a new normalized radial color stop with a system color.
    pub const fn system(angle: AngleValue, system_ref: crate::props::basic::color::SystemColorRef) -> Self {
        Self { angle, color: ColorOrSystem::system(system_ref) }
    }
    
    /// Create a normalized radial color stop with a ColorOrSystem directly.
    pub const fn with_color_or_system(angle: AngleValue, color: ColorOrSystem) -> Self {
        Self { angle, color }
    }
    
    /// Resolve the color against system colors.
    pub fn resolve(&self, system_colors: &crate::system::SystemColors, fallback: ColorU) -> ColorU {
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
            .map(|v| v.print_as_css_value())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

// Formatting to Rust code for StyleBackgroundPositionVec
impl crate::format_rust_code::FormatAsRustCode for StyleBackgroundPositionVec {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!(
            "StyleBackgroundPositionVec::from_const_slice(STYLE_BACKGROUND_POSITION_{}_ITEMS)",
            self.get_hash()
        )
    }
}

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
        match self {
            BackgroundPositionHorizontal::Exact(s) => {
                s.scale_for_dpi(scale_factor);
            }
            _ => {}
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
        match self {
            BackgroundPositionVertical::Exact(s) => {
                s.scale_for_dpi(scale_factor);
            }
            _ => {}
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

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
pub enum StyleBackgroundSize {
    ExactSize(PixelValueSize),
    Contain,
    Cover,
}

impl_option!(
    StyleBackgroundSize,
    OptionStyleBackgroundSize,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

/// Two-dimensional size in PixelValue units (width, height)
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
impl Default for StyleBackgroundSize {
    fn default() -> Self {
        StyleBackgroundSize::Contain
    }
}

impl StyleBackgroundSize {
    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        match self {
            StyleBackgroundSize::ExactSize(size) => {
                size.width.scale_for_dpi(scale_factor);
                size.height.scale_for_dpi(scale_factor);
            }
            _ => {}
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
            .map(|v| v.print_as_css_value())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum StyleBackgroundRepeat {
    NoRepeat,
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
impl Default for StyleBackgroundRepeat {
    fn default() -> Self {
        StyleBackgroundRepeat::PatternRepeat
    }
}
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
            .map(|v| v.print_as_css_value())
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

impl<'a> CssBackgroundParseError<'a> {
    pub fn to_contained(&self) -> CssBackgroundParseErrorOwned {
        match self {
            Self::Error(s) => CssBackgroundParseErrorOwned::Error(s.to_string().into()),
            Self::InvalidBackground(e) => {
                CssBackgroundParseErrorOwned::InvalidBackground(e.to_contained())
            }
            Self::UnclosedGradient(s) => {
                CssBackgroundParseErrorOwned::UnclosedGradient(s.to_string().into())
            }
            Self::NoDirection(s) => CssBackgroundParseErrorOwned::NoDirection(s.to_string().into()),
            Self::TooFewGradientStops(s) => {
                CssBackgroundParseErrorOwned::TooFewGradientStops(s.to_string().into())
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
    pub fn to_shared<'a>(&'a self) -> CssBackgroundParseError<'a> {
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

impl<'a> CssGradientStopParseError<'a> {
    pub fn to_contained(&self) -> CssGradientStopParseErrorOwned {
        match self {
            Self::Error(s) => CssGradientStopParseErrorOwned::Error(s.to_string().into()),
            Self::Percentage(e) => CssGradientStopParseErrorOwned::Percentage(e.to_contained()),
            Self::Angle(e) => CssGradientStopParseErrorOwned::Angle(e.to_contained()),
            Self::ColorParseError(e) => {
                CssGradientStopParseErrorOwned::ColorParseError(e.to_contained())
            }
        }
    }
}

impl CssGradientStopParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> CssGradientStopParseError<'a> {
        match self {
            Self::Error(s) => CssGradientStopParseError::Error(s),
            Self::Percentage(e) => CssGradientStopParseError::Percentage(e.to_shared()),
            Self::Angle(e) => CssGradientStopParseError::Angle(e.to_shared()),
            Self::ColorParseError(e) => CssGradientStopParseError::ColorParseError(e.to_shared()),
        }
    }
}

#[derive(Clone, PartialEq)]
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

#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum CssConicGradientParseErrorOwned {
    Position(CssBackgroundPositionParseErrorOwned),
    Angle(CssAngleValueParseErrorOwned),
    NoAngle(AzString),
}
impl<'a> CssConicGradientParseError<'a> {
    pub fn to_contained(&self) -> CssConicGradientParseErrorOwned {
        match self {
            Self::Position(e) => CssConicGradientParseErrorOwned::Position(e.to_contained()),
            Self::Angle(e) => CssConicGradientParseErrorOwned::Angle(e.to_contained()),
            Self::NoAngle(s) => CssConicGradientParseErrorOwned::NoAngle(s.to_string().into()),
        }
    }
}
impl CssConicGradientParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> CssConicGradientParseError<'a> {
        match self {
            Self::Position(e) => CssConicGradientParseError::Position(e.to_shared()),
            Self::Angle(e) => CssConicGradientParseError::Angle(e.to_shared()),
            Self::NoAngle(s) => CssConicGradientParseError::NoAngle(s),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum CssShapeParseError<'a> {
    ShapeErr(InvalidValueErr<'a>),
}
impl_display! {CssShapeParseError<'a>, {
    ShapeErr(e) => format!("\"{}\"", e.0),
}}
#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum CssShapeParseErrorOwned {
    ShapeErr(InvalidValueErrOwned),
}
impl<'a> CssShapeParseError<'a> {
    pub fn to_contained(&self) -> CssShapeParseErrorOwned {
        match self {
            Self::ShapeErr(err) => CssShapeParseErrorOwned::ShapeErr(err.to_contained()),
        }
    }
}
impl CssShapeParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> CssShapeParseError<'a> {
        match self {
            Self::ShapeErr(err) => CssShapeParseError::ShapeErr(err.to_shared()),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
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
#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum CssBackgroundPositionParseErrorOwned {
    NoPosition(AzString),
    TooManyComponents(AzString),
    FirstComponentWrong(CssPixelValueParseErrorOwned),
    SecondComponentWrong(CssPixelValueParseErrorOwned),
}
impl<'a> CssBackgroundPositionParseError<'a> {
    pub fn to_contained(&self) -> CssBackgroundPositionParseErrorOwned {
        match self {
            Self::NoPosition(s) => CssBackgroundPositionParseErrorOwned::NoPosition(s.to_string().into()),
            Self::TooManyComponents(s) => {
                CssBackgroundPositionParseErrorOwned::TooManyComponents(s.to_string().into())
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
    pub fn to_shared<'a>(&'a self) -> CssBackgroundPositionParseError<'a> {
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
    use super::*;

    /// Internal enum to help dispatch parsing logic within the `parse_gradient` function.
    #[derive(Debug, Copy, Clone, PartialEq, Eq)]
    pub enum GradientType {
        LinearGradient,
        RepeatingLinearGradient,
        RadialGradient,
        RepeatingRadialGradient,
        ConicGradient,
        RepeatingConicGradient,
    }

    impl GradientType {
        pub const fn get_extend_mode(&self) -> ExtendMode {
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
    pub fn parse_style_background_content_multiple<'a>(
        input: &'a str,
    ) -> Result<StyleBackgroundContentVec, CssBackgroundParseError<'a>> {
        Ok(split_string_respect_comma(input)
            .iter()
            .map(|i| parse_style_background_content(i))
            .collect::<Result<Vec<_>, _>>()?
            .into())
    }

    /// Parses a single background value, which can be a color, image, or gradient.
    pub fn parse_style_background_content<'a>(
        input: &'a str,
    ) -> Result<StyleBackgroundContent, CssBackgroundParseError<'a>> {
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
                            parse_image(brace_contents)?.into(),
                        ))
                    }
                    _ => unreachable!(),
                };
                parse_gradient(brace_contents, gradient_type)
            }
            Err(_) => Ok(StyleBackgroundContent::Color(parse_css_color(input)?)),
        }
    }

    /// Parses multiple `background-position` values.
    pub fn parse_style_background_position_multiple<'a>(
        input: &'a str,
    ) -> Result<StyleBackgroundPositionVec, CssBackgroundPositionParseError<'a>> {
        Ok(split_string_respect_comma(input)
            .iter()
            .map(|i| parse_style_background_position(i))
            .collect::<Result<Vec<_>, _>>()?
            .into())
    }

    /// Parses a single `background-position` value.
    pub fn parse_style_background_position<'a>(
        input: &'a str,
    ) -> Result<StyleBackgroundPosition, CssBackgroundPositionParseError<'a>> {
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
    pub fn parse_style_background_size_multiple<'a>(
        input: &'a str,
    ) -> Result<StyleBackgroundSizeVec, InvalidValueErr<'a>> {
        Ok(split_string_respect_comma(input)
            .iter()
            .map(|i| parse_style_background_size(i))
            .collect::<Result<Vec<_>, _>>()?
            .into())
    }

    /// Parses a single `background-size` value.
    pub fn parse_style_background_size<'a>(
        input: &'a str,
    ) -> Result<StyleBackgroundSize, InvalidValueErr<'a>> {
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
    pub fn parse_style_background_repeat_multiple<'a>(
        input: &'a str,
    ) -> Result<StyleBackgroundRepeatVec, InvalidValueErr<'a>> {
        Ok(split_string_respect_comma(input)
            .iter()
            .map(|i| parse_style_background_repeat(i))
            .collect::<Result<Vec<_>, _>>()?
            .into())
    }

    /// Parses a single `background-repeat` value.
    pub fn parse_style_background_repeat<'a>(
        input: &'a str,
    ) -> Result<StyleBackgroundRepeat, InvalidValueErr<'a>> {
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
    fn parse_gradient<'a>(
        input: &'a str,
        gradient_type: GradientType,
    ) -> Result<StyleBackgroundContent, CssBackgroundParseError<'a>> {
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
                    while let Some(word) = temp_iter.next() {
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
    fn parse_linear_color_stop<'a>(
        input: &'a str,
    ) -> Result<LinearColorStop, CssGradientStopParseError<'a>> {
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
    fn parse_radial_color_stop<'a>(
        input: &'a str,
    ) -> Result<RadialColorStop, CssGradientStopParseError<'a>> {
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
    /// parts. Returns (color_str, offset1, offset2) where offsets are optional.
    ///
    /// Per W3C CSS Images Level 3, a color stop can have 0, 1, or 2 positions:
    /// - "red" -> ("red", None, None)
    /// - "red 50%" -> ("red", Some("50%"), None)
    /// - "red 10% 30%" -> ("red", Some("10%"), Some("30%"))
    fn split_color_and_offsets<'a>(input: &'a str) -> (&'a str, Option<&'a str>, Option<&'a str>) {
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
    /// Returns (remaining, offset_str) if successful.
    fn try_split_last_offset<'a>(input: &'a str) -> Option<(&'a str, &'a str)> {
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
    fn parse_conic_first_item<'a>(
        input: &'a str,
    ) -> Result<Option<(AngleValue, StyleBackgroundPosition)>, CssConicGradientParseError<'a>> {
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

    /// Normalize linear color stops according to W3C CSS Images Level 3 spec.
    ///
    /// This handles:
    /// 1. Multi-position stops (e.g., "red 10% 30%" creates two stops)
    /// 2. Default positions (first=0%, last=100%)
    /// 3. Enforcing ascending order (positions < previous are clamped)
    /// 4. Distributing unpositioned stops evenly between neighbors
    /// 
    /// System colors (like `system:accent`) are preserved and resolved later at render time.
    fn get_normalized_linear_stops(stops: &[LinearColorStop]) -> Vec<NormalizedLinearColorStop> {
        if stops.is_empty() {
            return Vec::new();
        }

        // Phase 1: Expand multi-position stops and create intermediate list
        // Each entry is (color, Option<percentage>)
        let mut expanded: Vec<(ColorOrSystem, Option<PercentageValue>)> = Vec::new();

        for stop in stops {
            match (stop.offset1.into_option(), stop.offset2.into_option()) {
                (None, _) => {
                    // No position specified
                    expanded.push((stop.color, None));
                }
                (Some(pos1), None) => {
                    // Single position
                    expanded.push((stop.color, Some(pos1)));
                }
                (Some(pos1), Some(pos2)) => {
                    // Two positions - create two stops at the same color
                    expanded.push((stop.color, Some(pos1)));
                    expanded.push((stop.color, Some(pos2)));
                }
            }
        }

        if expanded.is_empty() {
            return Vec::new();
        }

        // Phase 2: Apply W3C fixup rules
        // Rule 1: If first stop has no position, default to 0%
        if expanded[0].1.is_none() {
            expanded[0].1 = Some(PercentageValue::new(0.0));
        }

        // Rule 1: If last stop has no position, default to 100%
        let last_idx = expanded.len() - 1;
        if expanded[last_idx].1.is_none() {
            expanded[last_idx].1 = Some(PercentageValue::new(100.0));
        }

        // Rule 2: Clamp positions to be at least as large as any previous position
        let mut max_so_far: f32 = 0.0;
        for (_, pos) in expanded.iter_mut() {
            if let Some(p) = pos {
                let val = p.normalized() * 100.0;
                if val < max_so_far {
                    *p = PercentageValue::new(max_so_far);
                } else {
                    max_so_far = val;
                }
            }
        }

        // Rule 3: Distribute unpositioned stops evenly between positioned neighbors
        let mut i = 0;
        while i < expanded.len() {
            if expanded[i].1.is_none() {
                // Find the run of unpositioned stops
                let run_start = i;
                let mut run_end = i;
                while run_end < expanded.len() && expanded[run_end].1.is_none() {
                    run_end += 1;
                }

                // Find the previous positioned stop (must exist due to Rule 1)
                let prev_pos = if run_start > 0 {
                    expanded[run_start - 1].1.unwrap().normalized() * 100.0
                } else {
                    0.0
                };

                // Find the next positioned stop (must exist due to Rule 1)
                let next_pos = if run_end < expanded.len() {
                    expanded[run_end].1.unwrap().normalized() * 100.0
                } else {
                    100.0
                };

                // Distribute evenly
                let run_len = run_end - run_start;
                let step = (next_pos - prev_pos) / (run_len + 1) as f32;

                for j in 0..run_len {
                    expanded[run_start + j].1 =
                        Some(PercentageValue::new(prev_pos + step * (j + 1) as f32));
                }

                i = run_end;
            } else {
                i += 1;
            }
        }

        // Phase 3: Convert to final normalized stops
        expanded
            .into_iter()
            .map(|(color, pos)| NormalizedLinearColorStop {
                offset: pos.unwrap_or(PercentageValue::new(0.0)),
                color,
            })
            .collect()
    }

    /// Normalize radial/conic color stops according to W3C CSS Images Level 3 spec.
    ///
    /// This handles:
    /// 1. Multi-position stops (e.g., "red 45deg 90deg" creates two stops)
    /// 2. Default positions (first=0deg, last=360deg for conic)
    /// 3. Enforcing ascending order (positions < previous are clamped)
    /// 4. Distributing unpositioned stops evenly between neighbors
    /// 
    /// System colors (like `system:accent`) are preserved and resolved later at render time.
    fn get_normalized_radial_stops(stops: &[RadialColorStop]) -> Vec<NormalizedRadialColorStop> {
        if stops.is_empty() {
            return Vec::new();
        }

        // Phase 1: Expand multi-position stops
        let mut expanded: Vec<(ColorOrSystem, Option<AngleValue>)> = Vec::new();

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

        // Phase 2: Apply fixup rules
        // Rule 1: Default first to 0deg, last to 360deg
        if expanded[0].1.is_none() {
            expanded[0].1 = Some(AngleValue::deg(0.0));
        }
        let last_idx = expanded.len() - 1;
        if expanded[last_idx].1.is_none() {
            expanded[last_idx].1 = Some(AngleValue::deg(360.0));
        }

        // Rule 2: Clamp to ascending order
        // Use to_degrees_raw() to preserve 360deg as distinct from 0deg
        let mut max_so_far: f32 = 0.0;
        for (_, pos) in expanded.iter_mut() {
            if let Some(p) = pos {
                let val = p.to_degrees_raw();
                if val < max_so_far {
                    *p = AngleValue::deg(max_so_far);
                } else {
                    max_so_far = val;
                }
            }
        }

        // Rule 3: Distribute unpositioned stops evenly
        let mut i = 0;
        while i < expanded.len() {
            if expanded[i].1.is_none() {
                let run_start = i;
                let mut run_end = i;
                while run_end < expanded.len() && expanded[run_end].1.is_none() {
                    run_end += 1;
                }

                let prev_pos = if run_start > 0 {
                    expanded[run_start - 1].1.unwrap().to_degrees_raw()
                } else {
                    0.0
                };

                let next_pos = if run_end < expanded.len() {
                    expanded[run_end].1.unwrap().to_degrees_raw()
                } else {
                    360.0
                };

                let run_len = run_end - run_start;
                let step = (next_pos - prev_pos) / (run_len + 1) as f32;

                for j in 0..run_len {
                    expanded[run_start + j].1 =
                        Some(AngleValue::deg(prev_pos + step * (j + 1) as f32));
                }

                i = run_end;
            } else {
                i += 1;
            }
        }

        // Phase 3: Convert to final normalized stops
        expanded
            .into_iter()
            .map(|(color, pos)| NormalizedRadialColorStop {
                angle: pos.unwrap_or(AngleValue::deg(0.0)),
                color,
            })
            .collect()
    }

    // -- Other Background Helpers --

    fn parse_background_position_horizontal<'a>(
        input: &'a str,
    ) -> Result<BackgroundPositionHorizontal, CssPixelValueParseError<'a>> {
        Ok(match input {
            "left" => BackgroundPositionHorizontal::Left,
            "center" => BackgroundPositionHorizontal::Center,
            "right" => BackgroundPositionHorizontal::Right,
            other => BackgroundPositionHorizontal::Exact(parse_pixel_value(other)?),
        })
    }

    fn parse_background_position_vertical<'a>(
        input: &'a str,
    ) -> Result<BackgroundPositionVertical, CssPixelValueParseError<'a>> {
        Ok(match input {
            "top" => BackgroundPositionVertical::Top,
            "center" => BackgroundPositionVertical::Center,
            "bottom" => BackgroundPositionVertical::Bottom,
            other => BackgroundPositionVertical::Exact(parse_pixel_value(other)?),
        })
    }

    fn parse_shape<'a>(input: &'a str) -> Result<Shape, CssShapeParseError<'a>> {
        match input.trim() {
            "circle" => Ok(Shape::Circle),
            "ellipse" => Ok(Shape::Ellipse),
            _ => Err(CssShapeParseError::ShapeErr(InvalidValueErr(input))),
        }
    }

    fn parse_radial_gradient_size<'a>(
        input: &'a str,
    ) -> Result<RadialGradientSize, InvalidValueErr<'a>> {
        match input.trim() {
            "closest-side" => Ok(RadialGradientSize::ClosestSide),
            "closest-corner" => Ok(RadialGradientSize::ClosestCorner),
            "farthest-side" => Ok(RadialGradientSize::FarthestSide),
            "farthest-corner" => Ok(RadialGradientSize::FarthestCorner),
            _ => Err(InvalidValueErr(input)),
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
}
