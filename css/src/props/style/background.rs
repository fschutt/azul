//! Background-related CSS properties

use alloc::{string::String, vec::Vec};
use core::fmt;

use crate::{
    error::CssParsingError,
    impl_vec, impl_vec_clone, impl_vec_debug, impl_vec_eq, impl_vec_hash, impl_vec_ord,
    impl_vec_partialeq, impl_vec_partialord,
    props::{
        basic::{
            angle::AngleValue,
            color::ColorU,
            direction::Direction,
            value::{PercentageValue, PixelValue},
        },
        formatter::FormatAsCssValue,
    },
    AzString,
};

/// CSS background-content property (background images, gradients, colors)
#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C)]
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
impl_vec_clone!(
    StyleBackgroundContent,
    StyleBackgroundContentVec,
    StyleBackgroundContentVecDestructor
);
impl_vec_partialeq!(StyleBackgroundContent, StyleBackgroundContentVec);

impl Default for StyleBackgroundContent {
    fn default() -> StyleBackgroundContent {
        StyleBackgroundContent::Color(ColorU::TRANSPARENT)
    }
}

impl<'a> From<AzString> for StyleBackgroundContent {
    fn from(id: AzString) -> StyleBackgroundContent {
        StyleBackgroundContent::Image(id)
    }
}

/// CSS background-position property
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct StyleBackgroundPosition {
    pub horizontal: BackgroundPositionHorizontal,
    pub vertical: BackgroundPositionVertical,
}

impl StyleBackgroundPosition {
    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        self.horizontal.scale_for_dpi(scale_factor);
        self.vertical.scale_for_dpi(scale_factor);
    }
}

impl_vec!(
    StyleBackgroundPosition,
    StyleBackgroundPositionVec,
    StyleBackgroundPositionVecDestructor
);
impl_vec_debug!(StyleBackgroundPosition, StyleBackgroundPositionVec);
impl_vec_partialord!(StyleBackgroundPosition, StyleBackgroundPositionVec);
impl_vec_clone!(
    StyleBackgroundPosition,
    StyleBackgroundPositionVec,
    StyleBackgroundPositionVecDestructor
);
impl_vec_partialeq!(StyleBackgroundPosition, StyleBackgroundPositionVec);

impl Default for StyleBackgroundPosition {
    fn default() -> Self {
        StyleBackgroundPosition {
            horizontal: BackgroundPositionHorizontal::Left,
            vertical: BackgroundPositionVertical::Top,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
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

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
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

/// CSS background-size property
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub enum StyleBackgroundSize {
    ExactSize([PixelValue; 2]),
    Contain,
    Cover,
}

impl StyleBackgroundSize {
    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        match self {
            StyleBackgroundSize::ExactSize(a) => {
                for q in a.iter_mut() {
                    q.scale_for_dpi(scale_factor);
                }
            }
            _ => {}
        }
    }
}

impl Default for StyleBackgroundSize {
    fn default() -> Self {
        StyleBackgroundSize::Contain
    }
}

impl_vec!(
    StyleBackgroundSize,
    StyleBackgroundSizeVec,
    StyleBackgroundSizeVecDestructor
);
impl_vec_debug!(StyleBackgroundSize, StyleBackgroundSizeVec);
impl_vec_partialord!(StyleBackgroundSize, StyleBackgroundSizeVec);
impl_vec_clone!(
    StyleBackgroundSize,
    StyleBackgroundSizeVec,
    StyleBackgroundSizeVecDestructor
);
impl_vec_partialeq!(StyleBackgroundSize, StyleBackgroundSizeVec);

impl FormatAsCssValue for StyleBackgroundSize {
    fn format_as_css_value(&self) -> String {
        match self {
            StyleBackgroundSize::ExactSize([w, h]) => {
                format!("{} {}", w.format_as_css_value(), h.format_as_css_value())
            }
            StyleBackgroundSize::Contain => "contain".to_string(),
            StyleBackgroundSize::Cover => "cover".to_string(),
        }
    }
}

/// CSS background-repeat property
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

/// Linear gradient definition
#[derive(Debug, Clone, PartialEq, PartialOrd)]
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

/// Radial gradient definition
#[derive(Debug, Clone, PartialEq, PartialOrd)]
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

/// Conic gradient definition
#[derive(Debug, Clone, PartialEq, PartialOrd)]
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
            center: StyleBackgroundPosition {
                horizontal: BackgroundPositionHorizontal::Center,
                vertical: BackgroundPositionVertical::Center,
            },
            angle: AngleValue::default(),
            stops: Vec::new().into(),
        }
    }
}

/// Gradient extend mode
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
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

/// Radial gradient size
#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub enum RadialGradientSize {
    ClosestSide,
    ClosestCorner,
    FarthestSide,
    FarthestCorner,
    ExactSize([PixelValue; 2]),
}

impl Default for RadialGradientSize {
    fn default() -> Self {
        RadialGradientSize::FarthestCorner
    }
}

impl RadialGradientSize {
    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        match self {
            RadialGradientSize::ExactSize(s) => {
                s[0].scale_for_dpi(scale_factor);
                s[1].scale_for_dpi(scale_factor);
            }
            _ => {}
        }
    }
}

/// Gradient shape
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum Shape {
    Circle,
    Ellipse,
}

impl Default for Shape {
    fn default() -> Self {
        Shape::Ellipse
    }
}

/// Normalized linear color stop
#[derive(Debug, Clone, PartialEq, PartialOrd)]
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
impl_vec_clone!(
    NormalizedLinearColorStop,
    NormalizedLinearColorStopVec,
    NormalizedLinearColorStopVecDestructor
);
impl_vec_partialeq!(NormalizedLinearColorStop, NormalizedLinearColorStopVec);

/// Normalized radial color stop
#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct NormalizedRadialColorStop {
    pub offset: PercentageValue,
    pub color: ColorU,
}

impl_vec!(
    NormalizedRadialColorStop,
    NormalizedRadialColorStopVec,
    NormalizedRadialColorStopVecDestructor
);
impl_vec_debug!(NormalizedRadialColorStop, NormalizedRadialColorStopVec);
impl_vec_partialord!(NormalizedRadialColorStop, NormalizedRadialColorStopVec);
impl_vec_clone!(
    NormalizedRadialColorStop,
    NormalizedRadialColorStopVec,
    NormalizedRadialColorStopVecDestructor
);
impl_vec_partialeq!(NormalizedRadialColorStop, NormalizedRadialColorStopVec);

/// Linear color stop (before normalization)
#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct LinearColorStop {
    pub offset: Option<PercentageValue>,
    pub color: ColorU,
}

impl_vec!(
    LinearColorStop,
    LinearColorStopVec,
    LinearColorStopVecDestructor
);
impl_vec_debug!(LinearColorStop, LinearColorStopVec);
impl_vec_partialord!(LinearColorStop, LinearColorStopVec);
impl_vec_clone!(
    LinearColorStop,
    LinearColorStopVec,
    LinearColorStopVecDestructor
);
impl_vec_partialeq!(LinearColorStop, LinearColorStopVec);

/// Radial color stop (before normalization)
#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct RadialColorStop {
    pub offset: Option<PercentageValue>,
    pub color: ColorU,
}

impl_vec!(
    RadialColorStop,
    RadialColorStopVec,
    RadialColorStopVecDestructor
);
impl_vec_debug!(RadialColorStop, RadialColorStopVec);
impl_vec_partialord!(RadialColorStop, RadialColorStopVec);
impl_vec_clone!(
    RadialColorStop,
    RadialColorStopVec,
    RadialColorStopVecDestructor
);
impl_vec_partialeq!(RadialColorStop, RadialColorStopVec);

// TODO: Add parsing functions and error types
// This file is getting long, so I'll add the parsing functions in a follow-up
