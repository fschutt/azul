//! CSS properties for flexbox layout.

use alloc::string::{String, ToString};
use core::num::ParseFloatError;

use crate::props::{
    basic::length::{parse_float_value, FloatValue},
    formatter::PrintAsCssValue,
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

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq)]
pub enum FlexGrowParseError<'a> {
    ParseFloat(ParseFloatError, &'a str),
}

#[cfg(feature = "parser")]
impl_debug_as_display!(FlexGrowParseError<'a>);
#[cfg(feature = "parser")]
impl_display! { FlexGrowParseError<'a>, {
    ParseFloat(e, s) => format!("Invalid flex-grow value: \"{}\". Reason: {}", s, e),
}}

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq)]
pub enum FlexGrowParseErrorOwned {
    ParseFloat(ParseFloatError, String),
}

#[cfg(feature = "parser")]
impl<'a> FlexGrowParseError<'a> {
    pub fn to_contained(&self) -> FlexGrowParseErrorOwned {
        match self {
            FlexGrowParseError::ParseFloat(e, s) => {
                FlexGrowParseErrorOwned::ParseFloat(e.clone(), s.to_string())
            }
        }
    }
}

#[cfg(feature = "parser")]
impl FlexGrowParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> FlexGrowParseError<'a> {
        match self {
            FlexGrowParseErrorOwned::ParseFloat(e, s) => {
                FlexGrowParseError::ParseFloat(e.clone(), s.as_str())
            }
        }
    }
}

#[cfg(feature = "parser")]
pub fn parse_layout_flex_grow<'a>(
    input: &'a str,
) -> Result<LayoutFlexGrow, FlexGrowParseError<'a>> {
    match parse_float_value(input) {
        Ok(o) => Ok(LayoutFlexGrow { inner: o }),
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

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq)]
pub enum FlexShrinkParseError<'a> {
    ParseFloat(ParseFloatError, &'a str),
}

#[cfg(feature = "parser")]
impl_debug_as_display!(FlexShrinkParseError<'a>);
#[cfg(feature = "parser")]
impl_display! { FlexShrinkParseError<'a>, {
    ParseFloat(e, s) => format!("Invalid flex-shrink value: \"{}\". Reason: {}", s, e),
}}

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq)]
pub enum FlexShrinkParseErrorOwned {
    ParseFloat(ParseFloatError, String),
}

#[cfg(feature = "parser")]
impl<'a> FlexShrinkParseError<'a> {
    pub fn to_contained(&self) -> FlexShrinkParseErrorOwned {
        match self {
            FlexShrinkParseError::ParseFloat(e, s) => {
                FlexShrinkParseErrorOwned::ParseFloat(e.clone(), s.to_string())
            }
        }
    }
}

#[cfg(feature = "parser")]
impl FlexShrinkParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> FlexShrinkParseError<'a> {
        match self {
            FlexShrinkParseErrorOwned::ParseFloat(e, s) => {
                FlexShrinkParseError::ParseFloat(e.clone(), s.as_str())
            }
        }
    }
}

#[cfg(feature = "parser")]
pub fn parse_layout_flex_shrink<'a>(
    input: &'a str,
) -> Result<LayoutFlexShrink, FlexShrinkParseError<'a>> {
    match parse_float_value(input) {
        Ok(o) => Ok(LayoutFlexShrink { inner: o }),
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
pub enum FlexDirectionParseErrorOwned {
    InvalidValue(String),
}

#[cfg(feature = "parser")]
impl<'a> FlexDirectionParseError<'a> {
    pub fn to_contained(&self) -> FlexDirectionParseErrorOwned {
        match self {
            Self::InvalidValue(s) => FlexDirectionParseErrorOwned::InvalidValue(s.to_string()),
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
pub enum FlexWrapParseErrorOwned {
    InvalidValue(String),
}

#[cfg(feature = "parser")]
impl<'a> FlexWrapParseError<'a> {
    pub fn to_contained(&self) -> FlexWrapParseErrorOwned {
        match self {
            Self::InvalidValue(s) => FlexWrapParseErrorOwned::InvalidValue(s.to_string()),
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
            Self::Start => "flex-start",
            Self::End => "flex-end",
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
pub enum JustifyContentParseErrorOwned {
    InvalidValue(String),
}

#[cfg(feature = "parser")]
impl<'a> JustifyContentParseError<'a> {
    pub fn to_contained(&self) -> JustifyContentParseErrorOwned {
        match self {
            Self::InvalidValue(s) => JustifyContentParseErrorOwned::InvalidValue(s.to_string()),
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
pub enum AlignItemsParseErrorOwned {
    InvalidValue(String),
}

#[cfg(feature = "parser")]
impl<'a> AlignItemsParseError<'a> {
    pub fn to_contained(&self) -> AlignItemsParseErrorOwned {
        match self {
            Self::InvalidValue(s) => AlignItemsParseErrorOwned::InvalidValue(s.to_string()),
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
        "flex-start" => Ok(LayoutAlignItems::Start),
        "flex-end" => Ok(LayoutAlignItems::End),
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
pub enum AlignContentParseErrorOwned {
    InvalidValue(String),
}

#[cfg(feature = "parser")]
impl<'a> AlignContentParseError<'a> {
    pub fn to_contained(&self) -> AlignContentParseErrorOwned {
        match self {
            Self::InvalidValue(s) => AlignContentParseErrorOwned::InvalidValue(s.to_string()),
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
