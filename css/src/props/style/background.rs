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
};
use crate::{
    corety::AzString,
    props::{
        basic::{
            angle::{
                parse_angle_value, AngleValue, CssAngleValueParseError,
                CssAngleValueParseErrorOwned, OptionAngleValue,
            },
            color::{parse_css_color, ColorU, CssColorParseError, CssColorParseErrorOwned},
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

impl_vec!(
    StyleBackgroundContent,
    StyleBackgroundContentVec,
    StyleBackgroundContentVecDestructor
);
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
    pub color: ColorU,
}
impl_vec!(
    NormalizedLinearColorStop,
    NormalizedLinearColorStopVec,
    NormalizedLinearColorStopVecDestructor
);
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
        format!("{} {}", self.color.to_hash(), self.offset)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct NormalizedRadialColorStop {
    pub angle: AngleValue,
    pub color: ColorU,
}
impl_vec!(
    NormalizedRadialColorStop,
    NormalizedRadialColorStopVec,
    NormalizedRadialColorStopVecDestructor
);
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
        format!("{} {}", self.color.to_hash(), self.angle)
    }
}

/// Transient struct for parsing linear color stops before normalization.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LinearColorStop {
    pub offset: OptionPercentageValue,
    pub color: ColorU,
}

/// Transient struct for parsing radial/conic color stops before normalization.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RadialColorStop {
    pub offset: OptionAngleValue,
    pub color: ColorU,
}

// -- Other Background Properties --

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleBackgroundPosition {
    pub horizontal: BackgroundPositionHorizontal,
    pub vertical: BackgroundPositionVertical,
}
impl_vec!(
    StyleBackgroundPosition,
    StyleBackgroundPositionVec,
    StyleBackgroundPositionVecDestructor
);
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

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
pub enum BackgroundPositionHorizontal {
    Left,
    Center,
    Right,
    Exact(PixelValue),
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
    ExactSize([PixelValue; 2]),
    Contain,
    Cover,
}
impl_vec!(
    StyleBackgroundSize,
    StyleBackgroundSizeVec,
    StyleBackgroundSizeVecDestructor
);
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
impl PrintAsCssValue for StyleBackgroundSize {
    fn print_as_css_value(&self) -> String {
        match self {
            Self::Contain => "contain".to_string(),
            Self::Cover => "cover".to_string(),
            Self::ExactSize([w, h]) => {
                format!("{} {}", w.print_as_css_value(), h.print_as_css_value())
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
    Repeat,
    RepeatX,
    RepeatY,
}
impl_vec!(
    StyleBackgroundRepeat,
    StyleBackgroundRepeatVec,
    StyleBackgroundRepeatVecDestructor
);
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
        StyleBackgroundRepeat::Repeat
    }
}
impl PrintAsCssValue for StyleBackgroundRepeat {
    fn print_as_css_value(&self) -> String {
        match self {
            Self::NoRepeat => "no-repeat".to_string(),
            Self::Repeat => "repeat".to_string(),
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
pub enum CssBackgroundParseErrorOwned {
    Error(String),
    InvalidBackground(ParenthesisParseErrorOwned),
    UnclosedGradient(String),
    NoDirection(String),
    TooFewGradientStops(String),
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
            Self::Error(s) => CssBackgroundParseErrorOwned::Error(s.to_string()),
            Self::InvalidBackground(e) => {
                CssBackgroundParseErrorOwned::InvalidBackground(e.to_contained())
            }
            Self::UnclosedGradient(s) => {
                CssBackgroundParseErrorOwned::UnclosedGradient(s.to_string())
            }
            Self::NoDirection(s) => CssBackgroundParseErrorOwned::NoDirection(s.to_string()),
            Self::TooFewGradientStops(s) => {
                CssBackgroundParseErrorOwned::TooFewGradientStops(s.to_string())
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
pub enum CssGradientStopParseErrorOwned {
    Error(String),
    Percentage(PercentageParseErrorOwned),
    Angle(CssAngleValueParseErrorOwned),
    ColorParseError(CssColorParseErrorOwned),
}

impl<'a> CssGradientStopParseError<'a> {
    pub fn to_contained(&self) -> CssGradientStopParseErrorOwned {
        match self {
            Self::Error(s) => CssGradientStopParseErrorOwned::Error(s.to_string()),
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
pub enum CssConicGradientParseErrorOwned {
    Position(CssBackgroundPositionParseErrorOwned),
    Angle(CssAngleValueParseErrorOwned),
    NoAngle(String),
}
impl<'a> CssConicGradientParseError<'a> {
    pub fn to_contained(&self) -> CssConicGradientParseErrorOwned {
        match self {
            Self::Position(e) => CssConicGradientParseErrorOwned::Position(e.to_contained()),
            Self::Angle(e) => CssConicGradientParseErrorOwned::Angle(e.to_contained()),
            Self::NoAngle(s) => CssConicGradientParseErrorOwned::NoAngle(s.to_string()),
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
pub enum CssBackgroundPositionParseErrorOwned {
    NoPosition(String),
    TooManyComponents(String),
    FirstComponentWrong(CssPixelValueParseErrorOwned),
    SecondComponentWrong(CssPixelValueParseErrorOwned),
}
impl<'a> CssBackgroundPositionParseError<'a> {
    pub fn to_contained(&self) -> CssBackgroundPositionParseErrorOwned {
        match self {
            Self::NoPosition(s) => CssBackgroundPositionParseErrorOwned::NoPosition(s.to_string()),
            Self::TooManyComponents(s) => {
                CssBackgroundPositionParseErrorOwned::TooManyComponents(s.to_string())
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
mod parser {
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
                Ok(StyleBackgroundSize::ExactSize([x_pos, y_pos]))
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
            "repeat" => Ok(StyleBackgroundRepeat::Repeat),
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

    /// Parses "red" or "red 5%".
    fn parse_linear_color_stop<'a>(
        input: &'a str,
    ) -> Result<LinearColorStop, CssGradientStopParseError<'a>> {
        let input = input.trim();
        let (color_str, offset_str) = split_color_and_offset(input);

        let color = parse_css_color(color_str)?;
        let offset = match offset_str {
            None => OptionPercentageValue::None,
            Some(s) => OptionPercentageValue::Some(
                parse_percentage_value(s).map_err(CssGradientStopParseError::Percentage)?,
            ),
        };

        Ok(LinearColorStop { offset, color })
    }

    /// Parses "red" or "red 90deg".
    fn parse_radial_color_stop<'a>(
        input: &'a str,
    ) -> Result<RadialColorStop, CssGradientStopParseError<'a>> {
        let input = input.trim();
        let (color_str, offset_str) = split_color_and_offset(input);

        let color = parse_css_color(color_str)?;
        let offset = match offset_str {
            None => OptionAngleValue::None,
            Some(s) => OptionAngleValue::Some(
                parse_angle_value(s).map_err(CssGradientStopParseError::Angle)?,
            ),
        };

        Ok(RadialColorStop { offset, color })
    }

    /// Helper to robustly split a string like "rgba(0,0,0,0.5) 50%" into color and offset parts.
    fn split_color_and_offset<'a>(input: &'a str) -> (&'a str, Option<&'a str>) {
        if let Some(last_ws_idx) = input.rfind(char::is_whitespace) {
            let (potential_color, potential_offset) = input.split_at(last_ws_idx);
            let potential_offset = potential_offset.trim();

            // Check if the part after the last space is a valid offset (contains a number).
            // This avoids misinterpreting "to right bottom" as a color stop.
            if potential_offset.contains(|c: char| c.is_digit(10)) {
                return (potential_color.trim(), Some(potential_offset));
            }
        }
        // If no whitespace or the part after it is not a valid offset, the whole string is the
        // color.
        (input, None)
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

    fn get_normalized_linear_stops(stops: &[LinearColorStop]) -> Vec<NormalizedLinearColorStop> {
        const MIN_STOP_DEGREE: f32 = 0.0;
        const MAX_STOP_DEGREE: f32 = 100.0;

        if stops.is_empty() {
            return Vec::new();
        }

        let self_stops = stops;

        let mut stops = self_stops
            .iter()
            .map(|s| NormalizedLinearColorStop {
                offset: s
                    .offset
                    .as_ref()
                    .copied()
                    .unwrap_or(PercentageValue::new(MIN_STOP_DEGREE)),
                color: s.color,
            })
            .collect::<Vec<_>>();

        let mut stops_to_distribute = 0;
        let mut last_stop = None;
        let stops_len = stops.len();

        for (stop_id, stop) in self_stops.iter().enumerate() {
            if let Some(s) = stop.offset.into_option() {
                let current_stop_val = s.normalized() * 100.0;
                if stops_to_distribute != 0 {
                    let last_stop_val =
                        stops[(stop_id - stops_to_distribute)].offset.normalized() * 100.0;
                    let value_to_add_per_stop = (current_stop_val.max(last_stop_val)
                        - last_stop_val)
                        / (stops_to_distribute - 1) as f32;
                    for (s_id, s) in stops[(stop_id - stops_to_distribute)..stop_id]
                        .iter_mut()
                        .enumerate()
                    {
                        s.offset = PercentageValue::new(
                            last_stop_val + (s_id as f32 * value_to_add_per_stop),
                        );
                    }
                }
                stops_to_distribute = 0;
                last_stop = Some(s);
            } else {
                stops_to_distribute += 1;
            }
        }

        if stops_to_distribute != 0 {
            let last_stop_val = last_stop
                .unwrap_or(PercentageValue::new(MIN_STOP_DEGREE))
                .normalized()
                * 100.0;
            let value_to_add_per_stop = (MAX_STOP_DEGREE.max(last_stop_val) - last_stop_val)
                / (stops_to_distribute - 1) as f32;
            for (s_id, s) in stops[(stops_len - stops_to_distribute)..]
                .iter_mut()
                .enumerate()
            {
                s.offset =
                    PercentageValue::new(last_stop_val + (s_id as f32 * value_to_add_per_stop));
            }
        }

        stops
    }

    fn get_normalized_radial_stops(stops: &[RadialColorStop]) -> Vec<NormalizedRadialColorStop> {
        const MIN_STOP_DEGREE: f32 = 0.0;
        const MAX_STOP_DEGREE: f32 = 360.0;

        if stops.is_empty() {
            return Vec::new();
        }

        let self_stops = stops;

        let mut stops = self_stops
            .iter()
            .map(|s| NormalizedRadialColorStop {
                angle: s
                    .offset
                    .as_ref()
                    .copied()
                    .unwrap_or(AngleValue::deg(MIN_STOP_DEGREE)),
                color: s.color,
            })
            .collect::<Vec<_>>();

        let mut stops_to_distribute = 0;
        let mut last_stop = None;
        let stops_len = stops.len();

        for (stop_id, stop) in self_stops.iter().enumerate() {
            if let Some(s) = stop.offset.into_option() {
                let current_stop_val = s.to_degrees();
                if stops_to_distribute != 0 {
                    let last_stop_val = stops[(stop_id - stops_to_distribute)].angle.to_degrees();
                    let value_to_add_per_stop = (current_stop_val.max(last_stop_val)
                        - last_stop_val)
                        / (stops_to_distribute - 1) as f32;
                    for (s_id, s) in stops[(stop_id - stops_to_distribute)..stop_id]
                        .iter_mut()
                        .enumerate()
                    {
                        s.angle =
                            AngleValue::deg(last_stop_val + (s_id as f32 * value_to_add_per_stop));
                    }
                }
                stops_to_distribute = 0;
                last_stop = Some(s);
            } else {
                stops_to_distribute += 1;
            }
        }

        if stops_to_distribute != 0 {
            let last_stop_val = last_stop
                .unwrap_or(AngleValue::deg(MIN_STOP_DEGREE))
                .to_degrees();
            let value_to_add_per_stop = (MAX_STOP_DEGREE.max(last_stop_val) - last_stop_val)
                / (stops_to_distribute - 1) as f32;
            for (s_id, s) in stops[(stops_len - stops_to_distribute)..]
                .iter_mut()
                .enumerate()
            {
                s.angle = AngleValue::deg(last_stop_val + (s_id as f32 * value_to_add_per_stop));
            }
        }

        stops
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
