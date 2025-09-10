//! Filter CSS properties

use alloc::{string::String, vec::Vec};
use core::fmt;

use crate::{
    impl_vec, impl_vec_clone, impl_vec_debug, impl_vec_eq, impl_vec_hash, impl_vec_ord,
    impl_vec_partialeq, impl_vec_partialord,
    props::{
        basic::{
            color::ColorU,
            value::{FloatValue, PercentageValue, PixelValue},
        },
        formatter::FormatAsCssValue,
        style::{box_shadow::StyleBoxShadow, effects::StyleMixBlendMode},
    },
};

/// CSS filter property
#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub enum StyleFilter {
    Blend(StyleMixBlendMode),
    Flood(ColorU),
    Blur(StyleBlur),
    Opacity(PercentageValue),
    ColorMatrix(StyleColorMatrix),
    DropShadow(StyleBoxShadow),
    ComponentTransfer,
    Offset(StyleFilterOffset),
    Composite(StyleCompositeFilter),
}

impl_vec!(StyleFilter, StyleFilterVec, StyleFilterVecDestructor);
impl_vec_clone!(StyleFilter, StyleFilterVec, StyleFilterVecDestructor);
impl_vec_debug!(StyleFilter, StyleFilterVec);
impl_vec_partialeq!(StyleFilter, StyleFilterVec);
impl_vec_partialord!(StyleFilter, StyleFilterVec);

impl Default for StyleFilter {
    fn default() -> Self {
        StyleFilter::Opacity(PercentageValue::new(100.0))
    }
}

impl fmt::Display for StyleFilter {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            StyleFilter::Blend(blend) => write!(f, "blend({})", blend),
            StyleFilter::Flood(color) => write!(f, "flood({:?})", color),
            StyleFilter::Blur(blur) => write!(f, "blur({} {})", blur.width, blur.height),
            StyleFilter::Opacity(opacity) => write!(f, "opacity({})", opacity),
            StyleFilter::ColorMatrix(matrix) => write!(f, "color-matrix({:?})", matrix.matrix),
            StyleFilter::DropShadow(shadow) => write!(f, "drop-shadow({:?})", shadow),
            StyleFilter::ComponentTransfer => write!(f, "component-transfer"),
            StyleFilter::Offset(offset) => write!(f, "offset({} {})", offset.x, offset.y),
            StyleFilter::Composite(composite) => write!(f, "composite({:?})", composite),
        }
    }
}

impl FormatAsCssValue for StyleFilter {
    fn format_as_css_value(&self) -> String {
        self.to_string()
    }
}

/// Blur filter
#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct StyleBlur {
    pub width: PixelValue,
    pub height: PixelValue,
}

impl Default for StyleBlur {
    fn default() -> Self {
        Self {
            width: PixelValue::zero(),
            height: PixelValue::zero(),
        }
    }
}

impl StyleBlur {
    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        self.width.scale_for_dpi(scale_factor);
        self.height.scale_for_dpi(scale_factor);
    }
}

impl FormatAsCssValue for StyleBlur {
    fn format_as_css_value(&self) -> String {
        format!(
            "blur({} {})",
            self.width.format_as_css_value(),
            self.height.format_as_css_value()
        )
    }
}

/// Color matrix filter (4x5 matrix for color transformations)
#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct StyleColorMatrix {
    pub matrix: [FloatValue; 20],
}

impl Default for StyleColorMatrix {
    fn default() -> Self {
        Self {
            // Identity matrix
            matrix: [
                FloatValue::new(1.0),
                FloatValue::new(0.0),
                FloatValue::new(0.0),
                FloatValue::new(0.0),
                FloatValue::new(0.0),
                FloatValue::new(0.0),
                FloatValue::new(1.0),
                FloatValue::new(0.0),
                FloatValue::new(0.0),
                FloatValue::new(0.0),
                FloatValue::new(0.0),
                FloatValue::new(0.0),
                FloatValue::new(1.0),
                FloatValue::new(0.0),
                FloatValue::new(0.0),
                FloatValue::new(0.0),
                FloatValue::new(0.0),
                FloatValue::new(0.0),
                FloatValue::new(1.0),
                FloatValue::new(0.0),
            ],
        }
    }
}

impl FormatAsCssValue for StyleColorMatrix {
    fn format_as_css_value(&self) -> String {
        let values: Vec<String> = self
            .matrix
            .iter()
            .map(|v| v.format_as_css_value())
            .collect();
        format!("color-matrix({})", values.join(" "))
    }
}

/// Filter offset transformation
#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct StyleFilterOffset {
    pub x: PixelValue,
    pub y: PixelValue,
}

impl Default for StyleFilterOffset {
    fn default() -> Self {
        Self {
            x: PixelValue::zero(),
            y: PixelValue::zero(),
        }
    }
}

impl StyleFilterOffset {
    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        self.x.scale_for_dpi(scale_factor);
        self.y.scale_for_dpi(scale_factor);
    }
}

impl FormatAsCssValue for StyleFilterOffset {
    fn format_as_css_value(&self) -> String {
        format!(
            "offset({} {})",
            self.x.format_as_css_value(),
            self.y.format_as_css_value()
        )
    }
}

/// Composite filter operations
#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C, u8)]
pub enum StyleCompositeFilter {
    Over,
    In,
    Atop,
    Out,
    Xor,
    Lighter,
    Arithmetic([FloatValue; 4]),
}

impl Default for StyleCompositeFilter {
    fn default() -> Self {
        StyleCompositeFilter::Over
    }
}

impl fmt::Display for StyleCompositeFilter {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            StyleCompositeFilter::Over => write!(f, "over"),
            StyleCompositeFilter::In => write!(f, "in"),
            StyleCompositeFilter::Atop => write!(f, "atop"),
            StyleCompositeFilter::Out => write!(f, "out"),
            StyleCompositeFilter::Xor => write!(f, "xor"),
            StyleCompositeFilter::Lighter => write!(f, "lighter"),
            StyleCompositeFilter::Arithmetic(values) => {
                write!(
                    f,
                    "arithmetic({} {} {} {})",
                    values[0], values[1], values[2], values[3]
                )
            }
        }
    }
}

impl FormatAsCssValue for StyleCompositeFilter {
    fn format_as_css_value(&self) -> String {
        self.to_string()
    }
}

#[cfg(feature = "parser")]
use crate::parser_ext::{parse_style_filter, parse_style_filter_vec};
