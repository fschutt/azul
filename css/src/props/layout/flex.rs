//! CSS properties for flexbox layout.

use alloc::string::{String, ToString};
use core::num::ParseFloatError;
use crate::corety::AzString;

use crate::{
    format_rust_code::FormatAsRustCode,
    props::{
        basic::{
            error::ParseFloatErrorWithInput,
            length::{parse_float_value, FloatValue},
        },
        formatter::PrintAsCssValue,
    },
};

// --- flex-grow ---

/// Represents a `flex-grow` attribute, which dictates what proportion of the
/// remaining space in the flex container should be assigned to the item.
/// Default: 0
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct LayoutFlexGrow {
    pub inner: FloatValue,
}

impl core::fmt::Debug for LayoutFlexGrow {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "{}", self.inner.get())
    }
}

impl Default for LayoutFlexGrow {
    fn default() -> Self {
        Self {
            inner: FloatValue::const_new(0),
        }
    }
}

impl PrintAsCssValue for LayoutFlexGrow {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}

impl LayoutFlexGrow {
    pub fn new(value: isize) -> Self {
        Self {
            inner: FloatValue::new(value as f32),
        }
    }

    pub const fn const_new(value: isize) -> Self {
        Self {
            inner: FloatValue::const_new(value),
        }
    }

    pub fn interpolate(&self, other: &Self, t: f32) -> Self {
        Self {
            inner: self.inner.interpolate(&other.inner, t),
        }
    }
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq)]
pub enum FlexGrowParseError<'a> {
    ParseFloat(ParseFloatError, &'a str),
    NegativeValue(&'a str),
}

#[cfg(feature = "parser")]
impl_debug_as_display!(FlexGrowParseError<'a>);
#[cfg(feature = "parser")]
impl_display! { FlexGrowParseError<'a>, {
    ParseFloat(e, s) => format!("Invalid flex-grow value: \"{}\". Reason: {}", s, e),
    NegativeValue(s) => format!("Invalid flex-grow value: \"{}\". Flex-grow cannot be negative", s),
}}

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum FlexGrowParseErrorOwned {
    ParseFloat(ParseFloatErrorWithInput),
    NegativeValue(AzString),
}

#[cfg(feature = "parser")]
impl<'a> FlexGrowParseError<'a> {
    pub fn to_contained(&self) -> FlexGrowParseErrorOwned {
        match self {
            FlexGrowParseError::ParseFloat(e, s) => {
                FlexGrowParseErrorOwned::ParseFloat(ParseFloatErrorWithInput { error: e.clone().into(), input: s.to_string() })
            }
            FlexGrowParseError::NegativeValue(s) => {
                FlexGrowParseErrorOwned::NegativeValue(s.to_string().into())
            }
        }
    }
}

#[cfg(feature = "parser")]
impl FlexGrowParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> FlexGrowParseError<'a> {
        match self {
            FlexGrowParseErrorOwned::ParseFloat(e) => {
                FlexGrowParseError::ParseFloat(e.error.to_std(), e.input.as_str())
            }
            FlexGrowParseErrorOwned::NegativeValue(s) => {
                FlexGrowParseError::NegativeValue(s.as_str())
            }
        }
    }
}

#[cfg(feature = "parser")]
pub fn parse_layout_flex_grow<'a>(
    input: &'a str,
) -> Result<LayoutFlexGrow, FlexGrowParseError<'a>> {
    match parse_float_value(input) {
        Ok(o) => {
            if o.get() < 0.0 {
                Err(FlexGrowParseError::NegativeValue(input))
            } else {
                Ok(LayoutFlexGrow { inner: o })
            }
        }
        Err(e) => Err(FlexGrowParseError::ParseFloat(e, input)),
    }
}

// --- flex-shrink ---

/// Represents a `flex-shrink` attribute, which dictates what proportion of
/// the negative space in the flex container should be removed from the item.
/// Default: 1
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct LayoutFlexShrink {
    pub inner: FloatValue,
}

impl core::fmt::Debug for LayoutFlexShrink {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "{}", self.inner.get())
    }
}

impl Default for LayoutFlexShrink {
    fn default() -> Self {
        Self {
            inner: FloatValue::const_new(1),
        }
    }
}

impl PrintAsCssValue for LayoutFlexShrink {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}

impl LayoutFlexShrink {
    pub fn interpolate(&self, other: &Self, t: f32) -> Self {
        Self {
            inner: self.inner.interpolate(&other.inner, t),
        }
    }
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq)]
pub enum FlexShrinkParseError<'a> {
    ParseFloat(ParseFloatError, &'a str),
    NegativeValue(&'a str),
}

#[cfg(feature = "parser")]
impl_debug_as_display!(FlexShrinkParseError<'a>);
#[cfg(feature = "parser")]
impl_display! { FlexShrinkParseError<'a>, {
    ParseFloat(e, s) => format!("Invalid flex-shrink value: \"{}\". Reason: {}", s, e),
    NegativeValue(s) => format!("Invalid flex-shrink value: \"{}\". Flex-shrink cannot be negative", s),
}}

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum FlexShrinkParseErrorOwned {
    ParseFloat(ParseFloatErrorWithInput),
    NegativeValue(AzString),
}

#[cfg(feature = "parser")]
impl<'a> FlexShrinkParseError<'a> {
    pub fn to_contained(&self) -> FlexShrinkParseErrorOwned {
        match self {
            FlexShrinkParseError::ParseFloat(e, s) => {
                FlexShrinkParseErrorOwned::ParseFloat(ParseFloatErrorWithInput { error: e.clone().into(), input: s.to_string() })
            }
            FlexShrinkParseError::NegativeValue(s) => {
                FlexShrinkParseErrorOwned::NegativeValue(s.to_string().into())
            }
        }
    }
}

#[cfg(feature = "parser")]
impl FlexShrinkParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> FlexShrinkParseError<'a> {
        match self {
            FlexShrinkParseErrorOwned::ParseFloat(e) => {
                FlexShrinkParseError::ParseFloat(e.error.to_std(), e.input.as_str())
            }
            FlexShrinkParseErrorOwned::NegativeValue(s) => {
                FlexShrinkParseError::NegativeValue(s.as_str())
            }
        }
    }
}

#[cfg(feature = "parser")]
pub fn parse_layout_flex_shrink<'a>(
    input: &'a str,
) -> Result<LayoutFlexShrink, FlexShrinkParseError<'a>> {
    match parse_float_value(input) {
        Ok(o) => {
            if o.get() < 0.0 {
                Err(FlexShrinkParseError::NegativeValue(input))
            } else {
                Ok(LayoutFlexShrink { inner: o })
            }
        }
        Err(e) => Err(FlexShrinkParseError::ParseFloat(e, input)),
    }
}

// --- flex-direction ---

/// Represents a `flex-direction` attribute, which establishes the main-axis,
/// thus defining the direction flex items are placed in the flex container.
/// Default: `Row`
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum LayoutFlexDirection {
    Row,
    RowReverse,
    Column,
    ColumnReverse,
}

impl Default for LayoutFlexDirection {
    fn default() -> Self {
        Self::Row
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum LayoutAxis {
    Horizontal,
    Vertical,
}

impl LayoutFlexDirection {
    pub fn get_axis(&self) -> LayoutAxis {
        match self {
            Self::Row | Self::RowReverse => LayoutAxis::Horizontal,
            Self::Column | Self::ColumnReverse => LayoutAxis::Vertical,
        }
    }

    pub fn is_reverse(&self) -> bool {
        matches!(self, Self::RowReverse | Self::ColumnReverse)
    }
}

impl PrintAsCssValue for LayoutFlexDirection {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            Self::Row => "row",
            Self::RowReverse => "row-reverse",
            Self::Column => "column",
            Self::ColumnReverse => "column-reverse",
        })
    }
}
// Formatting to Rust code
impl crate::format_rust_code::FormatAsRustCode for LayoutFlexBasis {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        match self {
            LayoutFlexBasis::Auto => String::from("LayoutFlexBasis::Auto"),
            LayoutFlexBasis::Exact(px) => {
                format!(
                    "LayoutFlexBasis::Exact({})",
                    crate::format_rust_code::format_pixel_value(px)
                )
            }
        }
    }
}
#[cfg(feature = "parser")]
#[derive(Clone, PartialEq)]
pub enum FlexDirectionParseError<'a> {
    InvalidValue(&'a str),
}

#[cfg(feature = "parser")]
impl_debug_as_display!(FlexDirectionParseError<'a>);
#[cfg(feature = "parser")]
impl_display! { FlexDirectionParseError<'a>, {
    InvalidValue(s) => format!("Invalid flex-direction value: \"{}\"", s),
}}

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum FlexDirectionParseErrorOwned {
    InvalidValue(AzString),
}

#[cfg(feature = "parser")]
impl<'a> FlexDirectionParseError<'a> {
    pub fn to_contained(&self) -> FlexDirectionParseErrorOwned {
        match self {
            Self::InvalidValue(s) => FlexDirectionParseErrorOwned::InvalidValue(s.to_string().into()),
        }
    }
}

#[cfg(feature = "parser")]
impl FlexDirectionParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> FlexDirectionParseError<'a> {
        match self {
            Self::InvalidValue(s) => FlexDirectionParseError::InvalidValue(s.as_str()),
        }
    }
}

#[cfg(feature = "parser")]
pub fn parse_layout_flex_direction<'a>(
    input: &'a str,
) -> Result<LayoutFlexDirection, FlexDirectionParseError<'a>> {
    match input.trim() {
        "row" => Ok(LayoutFlexDirection::Row),
        "row-reverse" => Ok(LayoutFlexDirection::RowReverse),
        "column" => Ok(LayoutFlexDirection::Column),
        "column-reverse" => Ok(LayoutFlexDirection::ColumnReverse),
        _ => Err(FlexDirectionParseError::InvalidValue(input)),
    }
}

// --- flex-wrap ---

/// Represents a `flex-wrap` attribute, which determines whether flex items
/// are forced onto one line or can wrap onto multiple lines.
/// Default: `NoWrap`
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum LayoutFlexWrap {
    Wrap,
    NoWrap,
    WrapReverse,
}

impl Default for LayoutFlexWrap {
    fn default() -> Self {
        Self::NoWrap
    }
}

impl PrintAsCssValue for LayoutFlexWrap {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            Self::Wrap => "wrap",
            Self::NoWrap => "nowrap",
            Self::WrapReverse => "wrap-reverse",
        })
    }
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq)]
pub enum FlexWrapParseError<'a> {
    InvalidValue(&'a str),
}

#[cfg(feature = "parser")]
impl_debug_as_display!(FlexWrapParseError<'a>);
#[cfg(feature = "parser")]
impl_display! { FlexWrapParseError<'a>, {
    InvalidValue(s) => format!("Invalid flex-wrap value: \"{}\"", s),
}}

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum FlexWrapParseErrorOwned {
    InvalidValue(AzString),
}

#[cfg(feature = "parser")]
impl<'a> FlexWrapParseError<'a> {
    pub fn to_contained(&self) -> FlexWrapParseErrorOwned {
        match self {
            Self::InvalidValue(s) => FlexWrapParseErrorOwned::InvalidValue(s.to_string().into()),
        }
    }
}

#[cfg(feature = "parser")]
impl FlexWrapParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> FlexWrapParseError<'a> {
        match self {
            Self::InvalidValue(s) => FlexWrapParseError::InvalidValue(s.as_str()),
        }
    }
}

#[cfg(feature = "parser")]
pub fn parse_layout_flex_wrap<'a>(
    input: &'a str,
) -> Result<LayoutFlexWrap, FlexWrapParseError<'a>> {
    match input.trim() {
        "wrap" => Ok(LayoutFlexWrap::Wrap),
        "nowrap" => Ok(LayoutFlexWrap::NoWrap),
        "wrap-reverse" => Ok(LayoutFlexWrap::WrapReverse),
        _ => Err(FlexWrapParseError::InvalidValue(input)),
    }
}

// --- justify-content ---

/// Represents a `justify-content` attribute, which defines the alignment
/// along the main axis.
/// Default: `Start` (flex-start)
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum LayoutJustifyContent {
    FlexStart,
    FlexEnd,
    Start,
    End,
    Center,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

impl Default for LayoutJustifyContent {
    fn default() -> Self {
        Self::Start
    }
}

impl PrintAsCssValue for LayoutJustifyContent {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            Self::Start => "start",
            Self::End => "end",
            Self::FlexStart => "flex-start",
            Self::FlexEnd => "flex-end",
            Self::Center => "center",
            Self::SpaceBetween => "space-between",
            Self::SpaceAround => "space-around",
            Self::SpaceEvenly => "space-evenly",
        })
    }
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq)]
pub enum JustifyContentParseError<'a> {
    InvalidValue(&'a str),
}

#[cfg(feature = "parser")]
impl_debug_as_display!(JustifyContentParseError<'a>);
#[cfg(feature = "parser")]
impl_display! { JustifyContentParseError<'a>, {
    InvalidValue(s) => format!("Invalid justify-content value: \"{}\"", s),
}}

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum JustifyContentParseErrorOwned {
    InvalidValue(AzString),
}

#[cfg(feature = "parser")]
impl<'a> JustifyContentParseError<'a> {
    pub fn to_contained(&self) -> JustifyContentParseErrorOwned {
        match self {
            Self::InvalidValue(s) => JustifyContentParseErrorOwned::InvalidValue(s.to_string().into()),
        }
    }
}

#[cfg(feature = "parser")]
impl JustifyContentParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> JustifyContentParseError<'a> {
        match self {
            Self::InvalidValue(s) => JustifyContentParseError::InvalidValue(s.as_str()),
        }
    }
}

#[cfg(feature = "parser")]
pub fn parse_layout_justify_content<'a>(
    input: &'a str,
) -> Result<LayoutJustifyContent, JustifyContentParseError<'a>> {
    match input.trim() {
        "flex-start" => Ok(LayoutJustifyContent::Start),
        "flex-end" => Ok(LayoutJustifyContent::End),
        "center" => Ok(LayoutJustifyContent::Center),
        "space-between" => Ok(LayoutJustifyContent::SpaceBetween),
        "space-around" => Ok(LayoutJustifyContent::SpaceAround),
        "space-evenly" => Ok(LayoutJustifyContent::SpaceEvenly),
        _ => Err(JustifyContentParseError::InvalidValue(input)),
    }
}

// --- align-items ---

/// Represents an `align-items` attribute, which defines the default behavior for
/// how flex items are laid out along the cross axis on the current line.
/// Default: `Stretch`
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum LayoutAlignItems {
    Stretch,
    Center,
    Start,
    End,
    Baseline,
}

impl Default for LayoutAlignItems {
    fn default() -> Self {
        Self::Stretch
    }
}

impl PrintAsCssValue for LayoutAlignItems {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            Self::Stretch => "stretch",
            Self::Center => "center",
            Self::Start => "flex-start",
            Self::End => "flex-end",
            Self::Baseline => "baseline",
        })
    }
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq)]
pub enum AlignItemsParseError<'a> {
    InvalidValue(&'a str),
}

#[cfg(feature = "parser")]
impl_debug_as_display!(AlignItemsParseError<'a>);
#[cfg(feature = "parser")]
impl_display! { AlignItemsParseError<'a>, {
    InvalidValue(s) => format!("Invalid align-items value: \"{}\"", s),
}}

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum AlignItemsParseErrorOwned {
    InvalidValue(AzString),
}

#[cfg(feature = "parser")]
impl<'a> AlignItemsParseError<'a> {
    pub fn to_contained(&self) -> AlignItemsParseErrorOwned {
        match self {
            Self::InvalidValue(s) => AlignItemsParseErrorOwned::InvalidValue(s.to_string().into()),
        }
    }
}

#[cfg(feature = "parser")]
impl AlignItemsParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> AlignItemsParseError<'a> {
        match self {
            Self::InvalidValue(s) => AlignItemsParseError::InvalidValue(s.as_str()),
        }
    }
}

#[cfg(feature = "parser")]
pub fn parse_layout_align_items<'a>(
    input: &'a str,
) -> Result<LayoutAlignItems, AlignItemsParseError<'a>> {
    match input.trim() {
        "stretch" => Ok(LayoutAlignItems::Stretch),
        "center" => Ok(LayoutAlignItems::Center),
        "start" | "flex-start" => Ok(LayoutAlignItems::Start),
        "end" | "flex-end" => Ok(LayoutAlignItems::End),
        "baseline" => Ok(LayoutAlignItems::Baseline),
        _ => Err(AlignItemsParseError::InvalidValue(input)),
    }
}

// --- align-content ---

/// Represents an `align-content` attribute, which aligns a flex container's lines
/// within it when there is extra space in the cross-axis.
/// Default: `Stretch`
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum LayoutAlignContent {
    Stretch,
    Center,
    Start,
    End,
    SpaceBetween,
    SpaceAround,
}

impl Default for LayoutAlignContent {
    fn default() -> Self {
        Self::Stretch
    }
}

impl PrintAsCssValue for LayoutAlignContent {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            Self::Stretch => "stretch",
            Self::Center => "center",
            Self::Start => "flex-start",
            Self::End => "flex-end",
            Self::SpaceBetween => "space-between",
            Self::SpaceAround => "space-around",
        })
    }
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq)]
pub enum AlignContentParseError<'a> {
    InvalidValue(&'a str),
}

#[cfg(feature = "parser")]
impl_debug_as_display!(AlignContentParseError<'a>);
#[cfg(feature = "parser")]
impl_display! { AlignContentParseError<'a>, {
    InvalidValue(s) => format!("Invalid align-content value: \"{}\"", s),
}}

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum AlignContentParseErrorOwned {
    InvalidValue(AzString),
}

#[cfg(feature = "parser")]
impl<'a> AlignContentParseError<'a> {
    pub fn to_contained(&self) -> AlignContentParseErrorOwned {
        match self {
            Self::InvalidValue(s) => AlignContentParseErrorOwned::InvalidValue(s.to_string().into()),
        }
    }
}

#[cfg(feature = "parser")]
impl AlignContentParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> AlignContentParseError<'a> {
        match self {
            Self::InvalidValue(s) => AlignContentParseError::InvalidValue(s.as_str()),
        }
    }
}

#[cfg(feature = "parser")]
pub fn parse_layout_align_content<'a>(
    input: &'a str,
) -> Result<LayoutAlignContent, AlignContentParseError<'a>> {
    match input.trim() {
        "stretch" => Ok(LayoutAlignContent::Stretch),
        "center" => Ok(LayoutAlignContent::Center),
        "flex-start" => Ok(LayoutAlignContent::Start),
        "flex-end" => Ok(LayoutAlignContent::End),
        "space-between" => Ok(LayoutAlignContent::SpaceBetween),
        "space-around" => Ok(LayoutAlignContent::SpaceAround),
        _ => Err(AlignContentParseError::InvalidValue(input)),
    }
}

// --- align-self ---

/// Represents an `align-self` attribute, which allows the default alignment
/// (or the one specified by align-items) to be overridden for individual flex items.
/// Default: `Auto`
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum LayoutAlignSelf {
    Auto,
    Stretch,
    Center,
    Start,
    End,
    Baseline,
}

impl Default for LayoutAlignSelf {
    fn default() -> Self {
        Self::Auto
    }
}

impl PrintAsCssValue for LayoutAlignSelf {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            Self::Auto => "auto",
            Self::Stretch => "stretch",
            Self::Center => "center",
            Self::Start => "flex-start",
            Self::End => "flex-end",
            Self::Baseline => "baseline",
        })
    }
}

impl FormatAsRustCode for LayoutAlignSelf {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!(
            "LayoutAlignSelf::{}",
            match self {
                LayoutAlignSelf::Auto => "Auto",
                LayoutAlignSelf::Stretch => "Stretch",
                LayoutAlignSelf::Center => "Center",
                LayoutAlignSelf::Start => "Start",
                LayoutAlignSelf::End => "End",
                LayoutAlignSelf::Baseline => "Baseline",
            }
        )
    }
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq)]
pub enum AlignSelfParseError<'a> {
    InvalidValue(&'a str),
}

#[cfg(feature = "parser")]
impl_debug_as_display!(AlignSelfParseError<'a>);
#[cfg(feature = "parser")]
impl_display! { AlignSelfParseError<'a>, {
    InvalidValue(s) => format!("Invalid align-self value: \"{}\"", s),
}}

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum AlignSelfParseErrorOwned {
    InvalidValue(AzString),
}

#[cfg(feature = "parser")]
impl<'a> AlignSelfParseError<'a> {
    pub fn to_contained(&self) -> AlignSelfParseErrorOwned {
        match self {
            Self::InvalidValue(s) => AlignSelfParseErrorOwned::InvalidValue(s.to_string().into()),
        }
    }
}

#[cfg(feature = "parser")]
impl AlignSelfParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> AlignSelfParseError<'a> {
        match self {
            Self::InvalidValue(s) => AlignSelfParseError::InvalidValue(s.as_str()),
        }
    }
}

#[cfg(feature = "parser")]
pub fn parse_layout_align_self<'a>(
    input: &'a str,
) -> Result<LayoutAlignSelf, AlignSelfParseError<'a>> {
    match input.trim() {
        "auto" => Ok(LayoutAlignSelf::Auto),
        "stretch" => Ok(LayoutAlignSelf::Stretch),
        "center" => Ok(LayoutAlignSelf::Center),
        "flex-start" => Ok(LayoutAlignSelf::Start),
        "flex-end" => Ok(LayoutAlignSelf::End),
        "baseline" => Ok(LayoutAlignSelf::Baseline),
        _ => Err(AlignSelfParseError::InvalidValue(input)),
    }
}

// --- flex-basis ---

/// Represents a `flex-basis` attribute
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
pub enum LayoutFlexBasis {
    /// auto
    Auto,
    /// Fixed size
    Exact(crate::props::basic::pixel::PixelValue),
}

impl core::fmt::Debug for LayoutFlexBasis {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "{}", self.print_as_css_value())
    }
}

impl Default for LayoutFlexBasis {
    fn default() -> Self {
        LayoutFlexBasis::Auto
    }
}

impl PrintAsCssValue for LayoutFlexBasis {
    fn print_as_css_value(&self) -> String {
        match self {
            LayoutFlexBasis::Auto => "auto".to_string(),
            LayoutFlexBasis::Exact(px) => px.print_as_css_value(),
        }
    }
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq)]
pub enum FlexBasisParseError<'a> {
    InvalidValue(&'a str),
}

#[cfg(feature = "parser")]
impl_debug_as_display!(FlexBasisParseError<'a>);
#[cfg(feature = "parser")]
impl_display! { FlexBasisParseError<'a>, {
    InvalidValue(e) => format!("Invalid flex-basis value: \"{}\"", e),
}}

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum FlexBasisParseErrorOwned {
    InvalidValue(AzString),
}

#[cfg(feature = "parser")]
impl<'a> FlexBasisParseError<'a> {
    pub fn to_contained(&self) -> FlexBasisParseErrorOwned {
        match self {
            FlexBasisParseError::InvalidValue(s) => {
                FlexBasisParseErrorOwned::InvalidValue(s.to_string().into())
            }
        }
    }
}

#[cfg(feature = "parser")]
impl FlexBasisParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> FlexBasisParseError<'a> {
        match self {
            FlexBasisParseErrorOwned::InvalidValue(s) => {
                FlexBasisParseError::InvalidValue(s.as_str())
            }
        }
    }
}

#[cfg(feature = "parser")]
pub fn parse_layout_flex_basis<'a>(
    input: &'a str,
) -> Result<LayoutFlexBasis, FlexBasisParseError<'a>> {
    use crate::props::basic::pixel::parse_pixel_value;

    match input.trim() {
        "auto" => Ok(LayoutFlexBasis::Auto),
        s => parse_pixel_value(s)
            .map(LayoutFlexBasis::Exact)
            .map_err(|_| FlexBasisParseError::InvalidValue(input)),
    }
}

#[cfg(all(test, feature = "parser"))]
mod tests {
    use super::*;
    use crate::props::basic::pixel::PixelValue;

    #[test]
    fn test_parse_layout_flex_grow() {
        assert_eq!(parse_layout_flex_grow("0").unwrap().inner.get(), 0.0);
        assert_eq!(parse_layout_flex_grow("1").unwrap().inner.get(), 1.0);
        assert_eq!(parse_layout_flex_grow("2.5").unwrap().inner.get(), 2.5);
        assert_eq!(parse_layout_flex_grow("  0.5  ").unwrap().inner.get(), 0.5);
        assert!(parse_layout_flex_grow("none").is_err());
        assert!(parse_layout_flex_grow("-1").is_err()); // Negative values are invalid
    }

    #[test]
    fn test_parse_layout_flex_shrink() {
        assert_eq!(parse_layout_flex_shrink("0").unwrap().inner.get(), 0.0);
        assert_eq!(parse_layout_flex_shrink("1").unwrap().inner.get(), 1.0);
        assert_eq!(parse_layout_flex_shrink("3.0").unwrap().inner.get(), 3.0);
        assert_eq!(parse_layout_flex_shrink(" 0.2 ").unwrap().inner.get(), 0.2);
        assert!(parse_layout_flex_shrink("auto").is_err());
        assert!(parse_layout_flex_shrink("-1").is_err()); // Negative values are invalid
    }

    #[test]
    fn test_parse_layout_flex_direction() {
        assert_eq!(
            parse_layout_flex_direction("row").unwrap(),
            LayoutFlexDirection::Row
        );
        assert_eq!(
            parse_layout_flex_direction("row-reverse").unwrap(),
            LayoutFlexDirection::RowReverse
        );
        assert_eq!(
            parse_layout_flex_direction("column").unwrap(),
            LayoutFlexDirection::Column
        );
        assert_eq!(
            parse_layout_flex_direction("column-reverse").unwrap(),
            LayoutFlexDirection::ColumnReverse
        );
        assert_eq!(
            parse_layout_flex_direction("  row  ").unwrap(),
            LayoutFlexDirection::Row
        );
        assert!(parse_layout_flex_direction("reversed-row").is_err());
    }

    #[test]
    fn test_parse_layout_flex_wrap() {
        assert_eq!(
            parse_layout_flex_wrap("nowrap").unwrap(),
            LayoutFlexWrap::NoWrap
        );
        assert_eq!(
            parse_layout_flex_wrap("wrap").unwrap(),
            LayoutFlexWrap::Wrap
        );
        assert_eq!(
            parse_layout_flex_wrap("wrap-reverse").unwrap(),
            LayoutFlexWrap::WrapReverse
        );
        assert_eq!(
            parse_layout_flex_wrap("  wrap  ").unwrap(),
            LayoutFlexWrap::Wrap
        );
        assert!(parse_layout_flex_wrap("wrap reverse").is_err());
    }

    #[test]
    fn test_parse_layout_justify_content() {
        assert_eq!(
            parse_layout_justify_content("flex-start").unwrap(),
            LayoutJustifyContent::Start
        );
        assert_eq!(
            parse_layout_justify_content("flex-end").unwrap(),
            LayoutJustifyContent::End
        );
        assert_eq!(
            parse_layout_justify_content("center").unwrap(),
            LayoutJustifyContent::Center
        );
        assert_eq!(
            parse_layout_justify_content("space-between").unwrap(),
            LayoutJustifyContent::SpaceBetween
        );
        assert_eq!(
            parse_layout_justify_content("space-around").unwrap(),
            LayoutJustifyContent::SpaceAround
        );
        assert_eq!(
            parse_layout_justify_content("space-evenly").unwrap(),
            LayoutJustifyContent::SpaceEvenly
        );
        assert_eq!(
            parse_layout_justify_content("  center  ").unwrap(),
            LayoutJustifyContent::Center
        );
        assert!(parse_layout_justify_content("start").is_err());
    }

    #[test]
    fn test_parse_layout_align_items() {
        assert_eq!(
            parse_layout_align_items("stretch").unwrap(),
            LayoutAlignItems::Stretch
        );
        assert_eq!(
            parse_layout_align_items("flex-start").unwrap(),
            LayoutAlignItems::Start
        );
        assert_eq!(
            parse_layout_align_items("flex-end").unwrap(),
            LayoutAlignItems::End
        );
        assert_eq!(
            parse_layout_align_items("start").unwrap(),
            LayoutAlignItems::Start
        );
        assert_eq!(
            parse_layout_align_items("end").unwrap(),
            LayoutAlignItems::End
        );
        assert_eq!(
            parse_layout_align_items("center").unwrap(),
            LayoutAlignItems::Center
        );
        assert_eq!(
            parse_layout_align_items("baseline").unwrap(),
            LayoutAlignItems::Baseline
        );
        assert!(parse_layout_align_items("invalid").is_err());
    }

    #[test]
    fn test_parse_layout_align_content() {
        assert_eq!(
            parse_layout_align_content("stretch").unwrap(),
            LayoutAlignContent::Stretch
        );
        assert_eq!(
            parse_layout_align_content("flex-start").unwrap(),
            LayoutAlignContent::Start
        );
        assert_eq!(
            parse_layout_align_content("flex-end").unwrap(),
            LayoutAlignContent::End
        );
        assert_eq!(
            parse_layout_align_content("center").unwrap(),
            LayoutAlignContent::Center
        );
        assert_eq!(
            parse_layout_align_content("space-between").unwrap(),
            LayoutAlignContent::SpaceBetween
        );
        assert_eq!(
            parse_layout_align_content("space-around").unwrap(),
            LayoutAlignContent::SpaceAround
        );
        assert!(parse_layout_align_content("space-evenly").is_err()); // Not valid for align-content
    }

    #[test]
    fn test_parse_layout_flex_basis() {
        assert_eq!(
            parse_layout_flex_basis("auto").unwrap(),
            LayoutFlexBasis::Auto
        );
        assert_eq!(
            parse_layout_flex_basis("200px").unwrap(),
            LayoutFlexBasis::Exact(PixelValue::px(200.0))
        );
        assert_eq!(
            parse_layout_flex_basis("50%").unwrap(),
            LayoutFlexBasis::Exact(PixelValue::percent(50.0))
        );
        assert_eq!(
            parse_layout_flex_basis("  10em  ").unwrap(),
            LayoutFlexBasis::Exact(PixelValue::em(10.0))
        );
        assert!(parse_layout_flex_basis("none").is_err());
        // Liberal parsing accepts unitless numbers (treated as px)
        assert_eq!(
            parse_layout_flex_basis("200").unwrap(),
            LayoutFlexBasis::Exact(PixelValue::px(200.0))
        );
        assert_eq!(
            parse_layout_flex_basis("0").unwrap(),
            LayoutFlexBasis::Exact(PixelValue::px(0.0))
        );
    }
}
