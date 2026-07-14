//! CSS properties for flexbox layout.

use alloc::string::{String, ToString};
use core::num::ParseFloatError;
use crate::corety::AzString;

use crate::{
    codegen::format::FormatAsRustCode,
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
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
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
    #[must_use] pub fn new(value: isize) -> Self {
        Self {
            inner: FloatValue::new(crate::cast::isize_to_f32(value)),
        }
    }

    #[must_use] pub const fn const_new(value: isize) -> Self {
        Self {
            inner: FloatValue::const_new(value),
        }
    }

    #[must_use] pub fn interpolate(&self, other: &Self, t: f32) -> Self {
        Self {
            inner: self.inner.interpolate(&other.inner, t),
        }
    }
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq, Eq)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum FlexGrowParseErrorOwned {
    ParseFloat(ParseFloatErrorWithInput),
    NegativeValue(AzString),
}

#[cfg(feature = "parser")]
impl FlexGrowParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> FlexGrowParseErrorOwned {
        match self {
            FlexGrowParseError::ParseFloat(e, s) => {
                FlexGrowParseErrorOwned::ParseFloat(ParseFloatErrorWithInput { error: e.clone().into(), input: (*s).to_string().into() })
            }
            FlexGrowParseError::NegativeValue(s) => {
                FlexGrowParseErrorOwned::NegativeValue((*s).to_string().into())
            }
        }
    }
}

#[cfg(feature = "parser")]
impl FlexGrowParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> FlexGrowParseError<'_> {
        match self {
            Self::ParseFloat(e) => {
                FlexGrowParseError::ParseFloat(e.error.to_std(), e.input.as_str())
            }
            Self::NegativeValue(s) => {
                FlexGrowParseError::NegativeValue(s.as_str())
            }
        }
    }
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `flex-grow` value.
pub fn parse_layout_flex_grow(
    input: &str,
) -> Result<LayoutFlexGrow, FlexGrowParseError<'_>> {
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
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
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
    #[must_use] pub fn interpolate(&self, other: &Self, t: f32) -> Self {
        Self {
            inner: self.inner.interpolate(&other.inner, t),
        }
    }
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq, Eq)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum FlexShrinkParseErrorOwned {
    ParseFloat(ParseFloatErrorWithInput),
    NegativeValue(AzString),
}

#[cfg(feature = "parser")]
impl FlexShrinkParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> FlexShrinkParseErrorOwned {
        match self {
            FlexShrinkParseError::ParseFloat(e, s) => {
                FlexShrinkParseErrorOwned::ParseFloat(ParseFloatErrorWithInput { error: e.clone().into(), input: (*s).to_string().into() })
            }
            FlexShrinkParseError::NegativeValue(s) => {
                FlexShrinkParseErrorOwned::NegativeValue((*s).to_string().into())
            }
        }
    }
}

#[cfg(feature = "parser")]
impl FlexShrinkParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> FlexShrinkParseError<'_> {
        match self {
            Self::ParseFloat(e) => {
                FlexShrinkParseError::ParseFloat(e.error.to_std(), e.input.as_str())
            }
            Self::NegativeValue(s) => {
                FlexShrinkParseError::NegativeValue(s.as_str())
            }
        }
    }
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `flex-shrink` value.
pub fn parse_layout_flex_shrink(
    input: &str,
) -> Result<LayoutFlexShrink, FlexShrinkParseError<'_>> {
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
#[derive(Default)]
pub enum LayoutFlexDirection {
    #[default]
    Row,
    RowReverse,
    Column,
    ColumnReverse,
}


/// Represents the main or cross axis of a flex container.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum LayoutAxis {
    Horizontal,
    Vertical,
}

impl LayoutFlexDirection {
    #[must_use] pub const fn get_axis(&self) -> LayoutAxis {
        match self {
            Self::Row | Self::RowReverse => LayoutAxis::Horizontal,
            Self::Column | Self::ColumnReverse => LayoutAxis::Vertical,
        }
    }

    #[must_use] pub const fn is_reverse(&self) -> bool {
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
#[derive(Clone, PartialEq, Eq)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum FlexDirectionParseErrorOwned {
    InvalidValue(AzString),
}

#[cfg(feature = "parser")]
impl FlexDirectionParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> FlexDirectionParseErrorOwned {
        match self {
            Self::InvalidValue(s) => FlexDirectionParseErrorOwned::InvalidValue((*s).to_string().into()),
        }
    }
}

#[cfg(feature = "parser")]
impl FlexDirectionParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> FlexDirectionParseError<'_> {
        match self {
            Self::InvalidValue(s) => FlexDirectionParseError::InvalidValue(s.as_str()),
        }
    }
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `flex-direction` value.
pub fn parse_layout_flex_direction(
    input: &str,
) -> Result<LayoutFlexDirection, FlexDirectionParseError<'_>> {
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
#[derive(Default)]
pub enum LayoutFlexWrap {
    Wrap,
    #[default]
    NoWrap,
    WrapReverse,
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
#[derive(Clone, PartialEq, Eq)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum FlexWrapParseErrorOwned {
    InvalidValue(AzString),
}

#[cfg(feature = "parser")]
impl FlexWrapParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> FlexWrapParseErrorOwned {
        match self {
            Self::InvalidValue(s) => FlexWrapParseErrorOwned::InvalidValue((*s).to_string().into()),
        }
    }
}

#[cfg(feature = "parser")]
impl FlexWrapParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> FlexWrapParseError<'_> {
        match self {
            Self::InvalidValue(s) => FlexWrapParseError::InvalidValue(s.as_str()),
        }
    }
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `flex-wrap` value.
pub fn parse_layout_flex_wrap(
    input: &str,
) -> Result<LayoutFlexWrap, FlexWrapParseError<'_>> {
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
#[derive(Default)]
pub enum LayoutJustifyContent {
    FlexStart,
    FlexEnd,
    #[default]
    Start,
    End,
    Center,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
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
#[derive(Clone, PartialEq, Eq)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum JustifyContentParseErrorOwned {
    InvalidValue(AzString),
}

#[cfg(feature = "parser")]
impl JustifyContentParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> JustifyContentParseErrorOwned {
        match self {
            Self::InvalidValue(s) => JustifyContentParseErrorOwned::InvalidValue((*s).to_string().into()),
        }
    }
}

#[cfg(feature = "parser")]
impl JustifyContentParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> JustifyContentParseError<'_> {
        match self {
            Self::InvalidValue(s) => JustifyContentParseError::InvalidValue(s.as_str()),
        }
    }
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `justify-content` value.
pub fn parse_layout_justify_content(
    input: &str,
) -> Result<LayoutJustifyContent, JustifyContentParseError<'_>> {
    match input.trim() {
        "flex-start" => Ok(LayoutJustifyContent::FlexStart),
        "flex-end" => Ok(LayoutJustifyContent::FlexEnd),
        "start" => Ok(LayoutJustifyContent::Start),
        "end" => Ok(LayoutJustifyContent::End),
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
#[derive(Default)]
pub enum LayoutAlignItems {
    #[default]
    Stretch,
    Center,
    Start,
    End,
    Baseline,
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
#[derive(Clone, PartialEq, Eq)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum AlignItemsParseErrorOwned {
    InvalidValue(AzString),
}

#[cfg(feature = "parser")]
impl AlignItemsParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> AlignItemsParseErrorOwned {
        match self {
            Self::InvalidValue(s) => AlignItemsParseErrorOwned::InvalidValue((*s).to_string().into()),
        }
    }
}

#[cfg(feature = "parser")]
impl AlignItemsParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> AlignItemsParseError<'_> {
        match self {
            Self::InvalidValue(s) => AlignItemsParseError::InvalidValue(s.as_str()),
        }
    }
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `align-items` value.
pub fn parse_layout_align_items(
    input: &str,
) -> Result<LayoutAlignItems, AlignItemsParseError<'_>> {
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
#[derive(Default)]
pub enum LayoutAlignContent {
    #[default]
    Stretch,
    Center,
    Start,
    End,
    SpaceBetween,
    SpaceAround,
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
#[derive(Clone, PartialEq, Eq)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum AlignContentParseErrorOwned {
    InvalidValue(AzString),
}

#[cfg(feature = "parser")]
impl AlignContentParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> AlignContentParseErrorOwned {
        match self {
            Self::InvalidValue(s) => AlignContentParseErrorOwned::InvalidValue((*s).to_string().into()),
        }
    }
}

#[cfg(feature = "parser")]
impl AlignContentParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> AlignContentParseError<'_> {
        match self {
            Self::InvalidValue(s) => AlignContentParseError::InvalidValue(s.as_str()),
        }
    }
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `align-content` value.
pub fn parse_layout_align_content(
    input: &str,
) -> Result<LayoutAlignContent, AlignContentParseError<'_>> {
    match input.trim() {
        "stretch" => Ok(LayoutAlignContent::Stretch),
        "center" => Ok(LayoutAlignContent::Center),
        "start" | "flex-start" => Ok(LayoutAlignContent::Start),
        "end" | "flex-end" => Ok(LayoutAlignContent::End),
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
#[derive(Default)]
pub enum LayoutAlignSelf {
    #[default]
    Auto,
    Stretch,
    Center,
    Start,
    End,
    Baseline,
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
                Self::Auto => "Auto",
                Self::Stretch => "Stretch",
                Self::Center => "Center",
                Self::Start => "Start",
                Self::End => "End",
                Self::Baseline => "Baseline",
            }
        )
    }
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq, Eq)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum AlignSelfParseErrorOwned {
    InvalidValue(AzString),
}

#[cfg(feature = "parser")]
impl AlignSelfParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> AlignSelfParseErrorOwned {
        match self {
            Self::InvalidValue(s) => AlignSelfParseErrorOwned::InvalidValue((*s).to_string().into()),
        }
    }
}

#[cfg(feature = "parser")]
impl AlignSelfParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> AlignSelfParseError<'_> {
        match self {
            Self::InvalidValue(s) => AlignSelfParseError::InvalidValue(s.as_str()),
        }
    }
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `align-self` value.
pub fn parse_layout_align_self(
    input: &str,
) -> Result<LayoutAlignSelf, AlignSelfParseError<'_>> {
    match input.trim() {
        "auto" => Ok(LayoutAlignSelf::Auto),
        "stretch" => Ok(LayoutAlignSelf::Stretch),
        "center" => Ok(LayoutAlignSelf::Center),
        "start" | "flex-start" => Ok(LayoutAlignSelf::Start),
        "end" | "flex-end" => Ok(LayoutAlignSelf::End),
        "baseline" => Ok(LayoutAlignSelf::Baseline),
        _ => Err(AlignSelfParseError::InvalidValue(input)),
    }
}

// --- flex-basis ---
#[allow(variant_size_differences)] // repr(C,u8) FFI enum: boxing the large variant would change the C ABI (api.json bindings); size disparity accepted
/// Represents a `flex-basis` attribute
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
#[derive(Default)]
pub enum LayoutFlexBasis {
    /// auto
    #[default]
    Auto,
    /// Fixed size
    Exact(crate::props::basic::pixel::PixelValue),
}

impl core::fmt::Debug for LayoutFlexBasis {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.print_as_css_value())
    }
}


impl PrintAsCssValue for LayoutFlexBasis {
    fn print_as_css_value(&self) -> String {
        match self {
            Self::Auto => "auto".to_string(),
            Self::Exact(px) => px.print_as_css_value(),
        }
    }
}

impl FormatAsRustCode for LayoutFlexBasis {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        match self {
            Self::Auto => String::from("LayoutFlexBasis::Auto"),
            Self::Exact(px) => {
                format!(
                    "LayoutFlexBasis::Exact({})",
                    crate::codegen::format::format_pixel_value(px)
                )
            }
        }
    }
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq, Eq)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum FlexBasisParseErrorOwned {
    InvalidValue(AzString),
}

#[cfg(feature = "parser")]
impl FlexBasisParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> FlexBasisParseErrorOwned {
        match self {
            FlexBasisParseError::InvalidValue(s) => {
                FlexBasisParseErrorOwned::InvalidValue((*s).to_string().into())
            }
        }
    }
}

#[cfg(feature = "parser")]
impl FlexBasisParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> FlexBasisParseError<'_> {
        match self {
            Self::InvalidValue(s) => {
                FlexBasisParseError::InvalidValue(s.as_str())
            }
        }
    }
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `flex-basis` value.
pub fn parse_layout_flex_basis(
    input: &str,
) -> Result<LayoutFlexBasis, FlexBasisParseError<'_>> {
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
    // Tests assert that parsed values equal the exact source literals.
    #![allow(clippy::float_cmp)]
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
            LayoutJustifyContent::FlexStart
        );
        assert_eq!(
            parse_layout_justify_content("flex-end").unwrap(),
            LayoutJustifyContent::FlexEnd
        );
        assert_eq!(
            parse_layout_justify_content("start").unwrap(),
            LayoutJustifyContent::Start
        );
        assert_eq!(
            parse_layout_justify_content("end").unwrap(),
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

#[cfg(test)]
#[allow(clippy::float_cmp)] // fixed-point (1/1000) values are exactly representable in f32
mod autotest_generated {
    use super::*;
    use crate::props::basic::{length::FloatValue, pixel::PixelValue};

    // ---------------------------------------------------------------------
    // LayoutFlexGrow::new / const_new  (constructor + numeric)
    // ---------------------------------------------------------------------

    /// `new()` goes through f32 (`isize -> f32 -> *1000 -> isize`) while
    /// `const_new()` uses pure integer math (`value * 1000`). For magnitudes
    /// where both encodings are exact they must agree bit-for-bit.
    #[test]
    fn flex_grow_new_agrees_with_const_new_for_exact_ints() {
        for v in [-10_000_isize, -1000, -7, -1, 0, 1, 7, 1000, 10_000] {
            assert_eq!(
                LayoutFlexGrow::new(v).inner.number(),
                LayoutFlexGrow::const_new(v).inner.number(),
                "new()/const_new() disagree for {v}"
            );
            assert_eq!(LayoutFlexGrow::new(v).inner.get(), v as f32);
        }
    }

    /// `new()` runs the value through `f32 as isize`, which saturates rather
    /// than wrapping or panicking: MIN/MAX must survive without UB or overflow.
    #[test]
    fn flex_grow_new_saturates_at_isize_extremes() {
        let max = LayoutFlexGrow::new(isize::MAX);
        let min = LayoutFlexGrow::new(isize::MIN);

        assert_eq!(max.inner.number(), isize::MAX, "MAX must saturate, not wrap");
        assert_eq!(min.inner.number(), isize::MIN, "MIN must saturate, not wrap");
        assert!(max.inner.get().is_finite());
        assert!(min.inner.get().is_finite());
        assert!(max.inner.get() > 0.0);
        assert!(min.inner.get() < 0.0);
    }

    /// `const_new()` stores `value * 1000` in an `isize`, so the largest safe
    /// input is `isize::MAX / 1000`. Anything above that overflows the
    /// multiplication (debug-panic / release-wrap) — this pins the documented
    /// safe boundary. See the report note on `const_new` overflow.
    #[test]
    fn flex_grow_const_new_at_safe_encoding_boundary() {
        let hi = isize::MAX / 1000;
        let lo = isize::MIN / 1000;

        assert_eq!(LayoutFlexGrow::const_new(hi).inner.number(), hi * 1000);
        assert_eq!(LayoutFlexGrow::const_new(lo).inner.number(), lo * 1000);
        assert!(LayoutFlexGrow::const_new(hi).inner.get().is_finite());
        assert!(LayoutFlexGrow::const_new(lo).inner.get().is_finite());
    }

    /// Zero / negative inputs are stored verbatim: the constructors perform no
    /// CSS validation (`flex-grow` may not be negative), only the parser does.
    #[test]
    fn flex_grow_const_new_zero_and_negative_are_not_clamped() {
        const ZERO: LayoutFlexGrow = LayoutFlexGrow::const_new(0);
        const NEG: LayoutFlexGrow = LayoutFlexGrow::const_new(-3);

        assert_eq!(ZERO.inner.get(), 0.0);
        assert_eq!(ZERO.inner.number(), 0);
        assert_eq!(NEG.inner.get(), -3.0);
        assert_eq!(LayoutFlexGrow::new(-3).inner.get(), -3.0);
    }

    #[test]
    fn flex_grow_and_shrink_defaults_match_css_initial_values() {
        assert_eq!(LayoutFlexGrow::default().inner.get(), 0.0);
        assert_eq!(LayoutFlexShrink::default().inner.get(), 1.0);
        // Ord is derived over the fixed-point isize, so it must track the float.
        assert!(LayoutFlexGrow::const_new(1) < LayoutFlexGrow::const_new(2));
        assert!(LayoutFlexGrow::const_new(-1) < LayoutFlexGrow::const_new(0));
    }

    // ---------------------------------------------------------------------
    // interpolate()  (numeric: zero / limits / NaN / inf / overflow)
    // ---------------------------------------------------------------------

    fn grow(v: f32) -> LayoutFlexGrow {
        LayoutFlexGrow {
            inner: FloatValue::new(v),
        }
    }

    fn shrink(v: f32) -> LayoutFlexShrink {
        LayoutFlexShrink {
            inner: FloatValue::new(v),
        }
    }

    #[test]
    fn flex_grow_interpolate_endpoints_midpoint_and_extrapolation() {
        let a = grow(0.0);
        let b = grow(10.0);

        assert_eq!(a.interpolate(&b, 0.0).inner.get(), 0.0);
        assert_eq!(a.interpolate(&b, 1.0).inner.get(), 10.0);
        assert_eq!(a.interpolate(&b, 0.5).inner.get(), 5.0);
        // t is not clamped: extrapolation past the endpoints is well-defined.
        assert_eq!(a.interpolate(&b, 2.0).inner.get(), 20.0);
        assert_eq!(a.interpolate(&b, -1.0).inner.get(), -10.0);
    }

    /// A NaN `t` produces NaN internally; the `f32 as isize` cast maps NaN to 0,
    /// so the result is a defined 0.0 rather than a NaN or a panic.
    #[test]
    fn flex_grow_interpolate_nan_t_collapses_to_zero() {
        let a = grow(2.0);
        let b = grow(8.0);

        let out = a.interpolate(&b, f32::NAN);
        assert!(out.inner.get().is_finite(), "NaN must not leak into the value");
        assert_eq!(out.inner.get(), 0.0);
        assert_eq!(out.inner.number(), 0);
    }

    /// Infinite `t` overflows the lerp; the saturating cast must keep the result
    /// finite and correctly signed instead of panicking.
    #[test]
    fn flex_grow_interpolate_infinite_t_saturates_finite() {
        let a = grow(0.0);
        let b = grow(10.0);

        let pos = a.interpolate(&b, f32::INFINITY);
        assert!(pos.inner.get().is_finite());
        assert!(pos.inner.get() > 0.0);
        assert_eq!(pos.inner.number(), isize::MAX);

        let neg = a.interpolate(&b, f32::NEG_INFINITY);
        assert!(neg.inner.get().is_finite());
        assert!(neg.inner.get() < 0.0);
        assert_eq!(neg.inner.number(), isize::MIN);

        // Degenerate case: equal endpoints => (b - a) * inf == NaN => 0.0.
        let same = grow(4.0).interpolate(&grow(4.0), f32::INFINITY);
        assert!(same.inner.get().is_finite());
        assert_eq!(same.inner.get(), 0.0);
    }

    /// Interpolating between the saturated extremes must never panic and must
    /// always yield a finite, decodable value.
    #[test]
    fn flex_grow_interpolate_extreme_endpoints_stay_finite() {
        let a = LayoutFlexGrow::new(isize::MAX);
        let b = LayoutFlexGrow::new(isize::MIN);

        for t in [
            0.0_f32,
            0.5,
            1.0,
            -1.0,
            1e30,
            -1e30,
            f32::MIN_POSITIVE,
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
        ] {
            let out = a.interpolate(&b, t);
            assert!(
                out.inner.get().is_finite(),
                "interpolate(MAX, MIN, {t}) produced a non-finite value"
            );
        }

        // t = 0 must return `self` even at the saturation boundary.
        assert_eq!(a.interpolate(&b, 0.0).inner.get(), a.inner.get());
    }

    #[test]
    fn flex_shrink_interpolate_endpoints_nan_and_inf() {
        let a = shrink(1.0);
        let b = shrink(3.0);

        assert_eq!(a.interpolate(&b, 0.0).inner.get(), 1.0);
        assert_eq!(a.interpolate(&b, 1.0).inner.get(), 3.0);
        assert_eq!(a.interpolate(&b, 0.5).inner.get(), 2.0);
        assert_eq!(a.interpolate(&b, -2.0).inner.get(), -3.0);

        assert_eq!(a.interpolate(&b, f32::NAN).inner.get(), 0.0);
        assert!(a.interpolate(&b, f32::INFINITY).inner.get().is_finite());
        assert!(a.interpolate(&b, f32::NEG_INFINITY).inner.get().is_finite());

        let extreme = LayoutFlexShrink {
            inner: FloatValue::new(f32::MAX),
        };
        assert!(extreme.interpolate(&a, 0.5).inner.get().is_finite());
        assert!(extreme.interpolate(&a, f32::NAN).inner.get().is_finite());
    }

    // ---------------------------------------------------------------------
    // LayoutFlexDirection::get_axis / is_reverse  (getter + predicate)
    // ---------------------------------------------------------------------

    const ALL_DIRECTIONS: [LayoutFlexDirection; 4] = [
        LayoutFlexDirection::Row,
        LayoutFlexDirection::RowReverse,
        LayoutFlexDirection::Column,
        LayoutFlexDirection::ColumnReverse,
    ];

    #[test]
    fn flex_direction_get_axis_is_exhaustive_and_reverse_invariant() {
        assert_eq!(LayoutFlexDirection::Row.get_axis(), LayoutAxis::Horizontal);
        assert_eq!(
            LayoutFlexDirection::RowReverse.get_axis(),
            LayoutAxis::Horizontal
        );
        assert_eq!(LayoutFlexDirection::Column.get_axis(), LayoutAxis::Vertical);
        assert_eq!(
            LayoutFlexDirection::ColumnReverse.get_axis(),
            LayoutAxis::Vertical
        );

        // Invariant: reversing a direction never changes its axis.
        assert_eq!(
            LayoutFlexDirection::Row.get_axis(),
            LayoutFlexDirection::RowReverse.get_axis()
        );
        assert_eq!(
            LayoutFlexDirection::Column.get_axis(),
            LayoutFlexDirection::ColumnReverse.get_axis()
        );
        // Default (`row`) must be the horizontal, non-reversed axis.
        assert_eq!(LayoutFlexDirection::default().get_axis(), LayoutAxis::Horizontal);
        assert!(!LayoutFlexDirection::default().is_reverse());
    }

    #[test]
    fn flex_direction_is_reverse_exhaustive() {
        assert!(!LayoutFlexDirection::Row.is_reverse());
        assert!(LayoutFlexDirection::RowReverse.is_reverse());
        assert!(!LayoutFlexDirection::Column.is_reverse());
        assert!(LayoutFlexDirection::ColumnReverse.is_reverse());

        // Every variant answers deterministically, and exactly half are reverse.
        let reversed = ALL_DIRECTIONS.iter().filter(|d| d.is_reverse()).count();
        assert_eq!(reversed, 2);
        // get_axis()/is_reverse() are orthogonal: both axes have a reverse form.
        for axis in [LayoutAxis::Horizontal, LayoutAxis::Vertical] {
            assert_eq!(
                ALL_DIRECTIONS
                    .iter()
                    .filter(|d| d.get_axis() == axis && d.is_reverse())
                    .count(),
                1
            );
        }
    }

    // =====================================================================
    // Parser-gated tests
    // =====================================================================

    // --- flex-grow / flex-shrink (numeric parsers) -----------------------

    #[cfg(feature = "parser")]
    #[test]
    fn flex_grow_parse_empty_and_whitespace_only_are_err() {
        for input in ["", " ", "   ", "\t", "\n", "\r\n", "\t \n "] {
            assert!(
                parse_layout_flex_grow(input).is_err(),
                "empty/whitespace input {input:?} must not parse"
            );
            assert!(parse_layout_flex_shrink(input).is_err());
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn flex_grow_parse_garbage_and_unicode_never_panics() {
        let deep_nesting = "(".repeat(10_000) + &")".repeat(10_000);
        let long_junk = "x".repeat(1_000_000);
        let long_digits = "9".repeat(100_000);

        let garbage = [
            "none",
            "auto",
            "null",
            "1/2",
            "0x10",
            "1,0",
            "--1",
            "1-",
            "+-1",
            "1e",
            "e1",
            "\u{1F600}",             // emoji
            "1\u{1F600}",            // digit + emoji
            "e\u{0301}",             // combining acute accent
            "\u{0661}\u{0662}",      // arabic-indic digits
            "１",                     // fullwidth digit
            "1\u{0}",                // embedded NUL
            "\u{200B}1",             // zero-width space
            deep_nesting.as_str(),
            long_junk.as_str(),
        ];

        for input in garbage {
            assert!(
                parse_layout_flex_grow(input).is_err(),
                "garbage input {:?} must be rejected",
                input.chars().take(8).collect::<String>()
            );
            assert!(parse_layout_flex_shrink(input).is_err());
        }

        // 100k digits overflows f32 to +inf; it must saturate, not hang or panic.
        let huge = parse_layout_flex_grow(&long_digits).expect("huge finite-overflow input");
        assert!(huge.inner.get().is_finite());
        assert!(huge.inner.get() >= 0.0);
    }

    #[cfg(feature = "parser")]
    #[test]
    fn flex_grow_parse_boundary_numbers() {
        // Positive controls.
        assert_eq!(parse_layout_flex_grow("0").unwrap().inner.get(), 0.0);
        assert_eq!(parse_layout_flex_grow("1").unwrap().inner.get(), 1.0);
        assert_eq!(parse_layout_flex_grow("+2.5").unwrap().inner.get(), 2.5);
        assert_eq!(parse_layout_flex_grow(".5").unwrap().inner.get(), 0.5);

        // Signed zero is accepted and normalised to +0.
        let neg_zero = parse_layout_flex_grow("-0").unwrap();
        assert_eq!(neg_zero.inner.get(), 0.0);
        assert_eq!(neg_zero.inner.number(), 0);

        // Genuinely negative values are rejected.
        assert!(matches!(
            parse_layout_flex_grow("-1"),
            Err(FlexGrowParseError::NegativeValue("-1"))
        ));
        assert!(parse_layout_flex_grow("-0.01").is_err());
        assert!(parse_layout_flex_grow("-1e10").is_err());

        // -inf underflows the fixed point to isize::MIN => still caught as negative.
        assert!(matches!(
            parse_layout_flex_grow("-inf"),
            Err(FlexGrowParseError::NegativeValue(_))
        ));

        // Sub-quantum negatives (|v| < 0.001) truncate to 0 and are ACCEPTED —
        // the negative check runs after the fixed-point quantisation.
        let tiny_neg = parse_layout_flex_grow("-0.0001").unwrap();
        assert_eq!(tiny_neg.inner.get(), 0.0);
        assert_eq!(parse_layout_flex_grow("-1e-30").unwrap().inner.get(), 0.0);

        // Values beyond f32 range become +inf, then saturate to a finite maximum.
        for input in ["inf", "1e40", "9223372036854775807", "3.5e38"] {
            let parsed = parse_layout_flex_grow(input)
                .unwrap_or_else(|e| panic!("{input:?} unexpectedly rejected: {e}"));
            assert!(
                parsed.inner.get().is_finite(),
                "{input:?} decoded to a non-finite value"
            );
            assert!(parsed.inner.get() >= 0.0);
        }

        // Denormal-scale positives quantise to 0 rather than erroring.
        assert_eq!(parse_layout_flex_grow("1e-40").unwrap().inner.get(), 0.0);

        // "NaN" is a valid Rust float literal, so it reaches the fixed-point
        // cast, which maps NaN -> 0. It is accepted as flex-grow: 0.
        let nan = parse_layout_flex_grow("NaN").unwrap();
        assert!(nan.inner.get().is_finite());
        assert_eq!(nan.inner.get(), 0.0);
    }

    #[cfg(feature = "parser")]
    #[test]
    fn flex_shrink_parse_boundary_numbers() {
        assert_eq!(parse_layout_flex_shrink("0").unwrap().inner.get(), 0.0);
        assert_eq!(parse_layout_flex_shrink("1").unwrap().inner.get(), 1.0);
        assert_eq!(parse_layout_flex_shrink("-0").unwrap().inner.get(), 0.0);

        assert!(matches!(
            parse_layout_flex_shrink("-1"),
            Err(FlexShrinkParseError::NegativeValue("-1"))
        ));
        assert!(parse_layout_flex_shrink("-inf").is_err());

        let huge = parse_layout_flex_shrink("1e40").unwrap();
        assert!(huge.inner.get().is_finite() && huge.inner.get() > 0.0);
        assert_eq!(parse_layout_flex_shrink("NaN").unwrap().inner.get(), 0.0);
    }

    #[cfg(feature = "parser")]
    #[test]
    fn flex_grow_parse_leading_trailing_junk() {
        // Surrounding whitespace is trimmed.
        assert_eq!(parse_layout_flex_grow("  0.5  ").unwrap().inner.get(), 0.5);
        assert_eq!(parse_layout_flex_grow("\t2\n").unwrap().inner.get(), 2.0);

        // Trailing junk / units / extra tokens are rejected.
        for input in ["1;", "1 2", "1px", "1%", "valid;garbage", "1 1 1"] {
            assert!(
                parse_layout_flex_grow(input).is_err(),
                "{input:?} must be rejected"
            );
        }
    }

    /// The error must carry the *original* (untrimmed) input, not the trimmed
    /// slice — callers rely on it to point back into the source CSS.
    #[cfg(feature = "parser")]
    #[test]
    fn flex_grow_error_preserves_untrimmed_input() {
        match parse_layout_flex_grow("  bogus  ") {
            Err(FlexGrowParseError::ParseFloat(_, s)) => assert_eq!(s, "  bogus  "),
            other => panic!("expected ParseFloat error, got {other:?}"),
        }
        match parse_layout_flex_shrink(" -2 ") {
            Err(FlexShrinkParseError::NegativeValue(s)) => assert_eq!(s, " -2 "),
            other => panic!("expected NegativeValue error, got {other:?}"),
        }
    }

    /// Round-trip: print -> parse must reproduce the value for anything that is
    /// exactly representable in the 1/1000 fixed point.
    #[cfg(feature = "parser")]
    #[test]
    fn flex_grow_shrink_print_parse_round_trip() {
        for v in [0.0_f32, 1.0, 2.5, 0.25, 0.125, 100.0, 12.5] {
            let g = grow(v);
            let printed = g.print_as_css_value();
            assert_eq!(
                parse_layout_flex_grow(&printed).unwrap().inner.number(),
                g.inner.number(),
                "flex-grow round-trip failed for {printed}"
            );

            let s = shrink(v);
            let printed = s.print_as_css_value();
            assert_eq!(
                parse_layout_flex_shrink(&printed).unwrap().inner.number(),
                s.inner.number(),
                "flex-shrink round-trip failed for {printed}"
            );
        }
    }

    // --- keyword parsers -------------------------------------------------

    /// Every enum variant must survive `print_as_css_value() -> parse()`.
    #[cfg(feature = "parser")]
    #[test]
    fn keyword_enums_print_parse_round_trip() {
        for d in ALL_DIRECTIONS {
            assert_eq!(parse_layout_flex_direction(&d.print_as_css_value()).unwrap(), d);
        }
        for w in [
            LayoutFlexWrap::Wrap,
            LayoutFlexWrap::NoWrap,
            LayoutFlexWrap::WrapReverse,
        ] {
            assert_eq!(parse_layout_flex_wrap(&w.print_as_css_value()).unwrap(), w);
        }
        for j in [
            LayoutJustifyContent::FlexStart,
            LayoutJustifyContent::FlexEnd,
            LayoutJustifyContent::Start,
            LayoutJustifyContent::End,
            LayoutJustifyContent::Center,
            LayoutJustifyContent::SpaceBetween,
            LayoutJustifyContent::SpaceAround,
            LayoutJustifyContent::SpaceEvenly,
        ] {
            assert_eq!(parse_layout_justify_content(&j.print_as_css_value()).unwrap(), j);
        }
        for a in [
            LayoutAlignItems::Stretch,
            LayoutAlignItems::Center,
            LayoutAlignItems::Start,
            LayoutAlignItems::End,
            LayoutAlignItems::Baseline,
        ] {
            assert_eq!(parse_layout_align_items(&a.print_as_css_value()).unwrap(), a);
        }
        for a in [
            LayoutAlignContent::Stretch,
            LayoutAlignContent::Center,
            LayoutAlignContent::Start,
            LayoutAlignContent::End,
            LayoutAlignContent::SpaceBetween,
            LayoutAlignContent::SpaceAround,
        ] {
            assert_eq!(parse_layout_align_content(&a.print_as_css_value()).unwrap(), a);
        }
        for a in [
            LayoutAlignSelf::Auto,
            LayoutAlignSelf::Stretch,
            LayoutAlignSelf::Center,
            LayoutAlignSelf::Start,
            LayoutAlignSelf::End,
            LayoutAlignSelf::Baseline,
        ] {
            assert_eq!(parse_layout_align_self(&a.print_as_css_value()).unwrap(), a);
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn keyword_parsers_reject_empty_whitespace_and_garbage() {
        let deep_nesting = "[".repeat(10_000) + &"]".repeat(10_000);
        let long_junk = "row".repeat(300_000); // ~900k chars, no hang
        let bad = [
            "",
            " ",
            "\t\n",
            "0",
            "-1",
            "NaN",
            "inf",
            "9223372036854775807",
            "\u{1F600}",
            "row\u{200B}",  // zero-width space is NOT css whitespace
            "row\u{0}",     // embedded NUL
            "row row",
            "row;",
            ";row",
            "row/**/",
            deep_nesting.as_str(),
            long_junk.as_str(),
        ];

        for input in bad {
            assert!(parse_layout_flex_direction(input).is_err());
            assert!(parse_layout_flex_wrap(input).is_err());
            assert!(parse_layout_justify_content(input).is_err());
            assert!(parse_layout_align_items(input).is_err());
            assert!(parse_layout_align_content(input).is_err());
            assert!(parse_layout_align_self(input).is_err());
        }
    }

    /// CSS keywords are ASCII case-insensitive, but these parsers match
    /// case-sensitively. Pinned as current behaviour (see report).
    #[cfg(feature = "parser")]
    #[test]
    fn keyword_parsers_are_case_sensitive() {
        assert!(parse_layout_flex_direction("ROW").is_err());
        assert!(parse_layout_flex_direction("Row").is_err());
        assert!(parse_layout_flex_wrap("NoWrap").is_err());
        assert!(parse_layout_justify_content("Center").is_err());
        assert!(parse_layout_align_items("STRETCH").is_err());
        assert!(parse_layout_align_content("Stretch").is_err());
        assert!(parse_layout_align_self("AUTO").is_err());

        // lowercase positive controls still work
        assert_eq!(
            parse_layout_flex_direction("row").unwrap(),
            LayoutFlexDirection::Row
        );
        assert_eq!(parse_layout_align_self("auto").unwrap(), LayoutAlignSelf::Auto);
    }

    /// Keyword errors must echo the original, untrimmed input.
    #[cfg(feature = "parser")]
    #[test]
    fn keyword_errors_preserve_untrimmed_input() {
        assert_eq!(
            parse_layout_flex_direction("  bogus  ").unwrap_err(),
            FlexDirectionParseError::InvalidValue("  bogus  ")
        );
        assert_eq!(
            parse_layout_flex_wrap("\twrap!\n").unwrap_err(),
            FlexWrapParseError::InvalidValue("\twrap!\n")
        );
        assert_eq!(
            parse_layout_justify_content("").unwrap_err(),
            JustifyContentParseError::InvalidValue("")
        );
        assert_eq!(
            parse_layout_align_items(" nope ").unwrap_err(),
            AlignItemsParseError::InvalidValue(" nope ")
        );
        assert_eq!(
            parse_layout_align_content(" nope ").unwrap_err(),
            AlignContentParseError::InvalidValue(" nope ")
        );
        assert_eq!(
            parse_layout_align_self(" nope ").unwrap_err(),
            AlignSelfParseError::InvalidValue(" nope ")
        );
    }

    /// Aliases: `start`/`flex-start` and `end`/`flex-end` collapse to the same
    /// variant for align-*, while justify-content keeps them distinct.
    #[cfg(feature = "parser")]
    #[test]
    fn align_aliases_collapse_but_justify_keeps_them_distinct() {
        assert_eq!(
            parse_layout_align_items("start").unwrap(),
            parse_layout_align_items("flex-start").unwrap()
        );
        assert_eq!(
            parse_layout_align_content("end").unwrap(),
            parse_layout_align_content("flex-end").unwrap()
        );
        assert_eq!(
            parse_layout_align_self("start").unwrap(),
            parse_layout_align_self("flex-start").unwrap()
        );
        assert_ne!(
            parse_layout_justify_content("start").unwrap(),
            parse_layout_justify_content("flex-start").unwrap()
        );
        // space-evenly exists for justify-content but not for align-content.
        assert!(parse_layout_justify_content("space-evenly").is_ok());
        assert!(parse_layout_align_content("space-evenly").is_err());
        // align-self has `auto`; align-items does not.
        assert!(parse_layout_align_self("auto").is_ok());
        assert!(parse_layout_align_items("auto").is_err());
    }

    // --- flex-basis ------------------------------------------------------

    #[cfg(feature = "parser")]
    #[test]
    fn flex_basis_print_parse_round_trip() {
        for basis in [
            LayoutFlexBasis::Auto,
            LayoutFlexBasis::Exact(PixelValue::px(0.0)),
            LayoutFlexBasis::Exact(PixelValue::px(200.0)),
            LayoutFlexBasis::Exact(PixelValue::px(-5.0)),
            LayoutFlexBasis::Exact(PixelValue::percent(50.0)),
            LayoutFlexBasis::Exact(PixelValue::em(10.5)),
            LayoutFlexBasis::Exact(PixelValue::rem(1.25)),
            LayoutFlexBasis::Exact(PixelValue::pt(12.0)),
        ] {
            let printed = basis.print_as_css_value();
            assert_eq!(
                parse_layout_flex_basis(&printed).unwrap(),
                basis,
                "flex-basis round-trip failed for {printed}"
            );
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn flex_basis_rejects_empty_units_only_and_garbage() {
        let deep_nesting = "(".repeat(10_000) + &")".repeat(10_000);
        let long_junk = "z".repeat(1_000_000);

        for input in [
            "",
            "   ",
            "\t\n",
            "px",          // unit with no value
            "%",
            "em",
            " px ",
            "none",
            "auto auto",
            "200px;",
            "200 px extra",
            "AUTO",        // case-sensitive
            "5PX",
            "\u{1F600}",
            "５０px",       // fullwidth digits
            "200\u{200B}px",
            deep_nesting.as_str(),
            long_junk.as_str(),
        ] {
            assert!(
                parse_layout_flex_basis(input).is_err(),
                "flex-basis {:?} must be rejected",
                input.chars().take(10).collect::<String>()
            );
        }

        // The error echoes the original, untrimmed input.
        assert_eq!(
            parse_layout_flex_basis("  none  ").unwrap_err(),
            FlexBasisParseError::InvalidValue("  none  ")
        );
    }

    /// Adversarial numeric flex-basis inputs: NaN/inf reach the fixed-point cast
    /// through the unit suffix and must saturate to a finite, defined value.
    #[cfg(feature = "parser")]
    #[test]
    fn flex_basis_nan_and_inf_units_saturate() {
        // "NaN" is a valid float literal => NaNpx decodes to 0px, not an error.
        assert_eq!(
            parse_layout_flex_basis("NaNpx").unwrap(),
            LayoutFlexBasis::Exact(PixelValue::px(0.0))
        );
        // Overflowing magnitudes saturate rather than panicking.
        assert_eq!(
            parse_layout_flex_basis("infpx").unwrap(),
            LayoutFlexBasis::Exact(PixelValue::px(f32::INFINITY))
        );
        assert_eq!(
            parse_layout_flex_basis("1e40px").unwrap(),
            LayoutFlexBasis::Exact(PixelValue::px(f32::INFINITY))
        );
        assert_eq!(
            parse_layout_flex_basis(&"9".repeat(100_000)).unwrap(),
            LayoutFlexBasis::Exact(PixelValue::px(f32::INFINITY))
        );

        // Unitless numbers are accepted and treated as px (liberal parsing).
        assert_eq!(
            parse_layout_flex_basis("-0").unwrap(),
            LayoutFlexBasis::Exact(PixelValue::px(0.0))
        );
        // Negative lengths are accepted even though CSS forbids them (see report).
        assert_eq!(
            parse_layout_flex_basis("-5px").unwrap(),
            LayoutFlexBasis::Exact(PixelValue::px(-5.0))
        );
        // Whitespace *inside* the token is tolerated (see report).
        assert_eq!(
            parse_layout_flex_basis("5 px").unwrap(),
            LayoutFlexBasis::Exact(PixelValue::px(5.0))
        );
    }

    // ---------------------------------------------------------------------
    // Error to_contained() / to_shared()  (getters, borrow <-> owned)
    // ---------------------------------------------------------------------

    /// `to_contained()` then `to_shared()` must be the identity for every error
    /// variant, including empty / unicode / long payloads.
    #[cfg(feature = "parser")]
    #[test]
    fn flex_grow_shrink_error_owned_round_trip() {
        let invalid = "x".parse::<f32>().unwrap_err();
        let empty = "".parse::<f32>().unwrap_err();
        let long = "q".repeat(10_000);

        for payload in ["", "abc", "\u{1F600}\u{0301}", "  spaced  ", long.as_str()] {
            for err in [
                FlexGrowParseError::ParseFloat(invalid.clone(), payload),
                FlexGrowParseError::ParseFloat(empty.clone(), payload),
                FlexGrowParseError::NegativeValue(payload),
            ] {
                assert_eq!(err.to_contained().to_shared(), err);
            }
            for err in [
                FlexShrinkParseError::ParseFloat(invalid.clone(), payload),
                FlexShrinkParseError::NegativeValue(payload),
            ] {
                assert_eq!(err.to_contained().to_shared(), err);
            }
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn keyword_error_owned_round_trip() {
        let long = "k".repeat(10_000);

        for payload in ["", " ", "\u{1F600}", "bogus", long.as_str()] {
            let d = FlexDirectionParseError::InvalidValue(payload);
            assert_eq!(d.to_contained().to_shared(), d);

            let w = FlexWrapParseError::InvalidValue(payload);
            assert_eq!(w.to_contained().to_shared(), w);

            let j = JustifyContentParseError::InvalidValue(payload);
            assert_eq!(j.to_contained().to_shared(), j);

            let ai = AlignItemsParseError::InvalidValue(payload);
            assert_eq!(ai.to_contained().to_shared(), ai);

            let ac = AlignContentParseError::InvalidValue(payload);
            assert_eq!(ac.to_contained().to_shared(), ac);

            let asf = AlignSelfParseError::InvalidValue(payload);
            assert_eq!(asf.to_contained().to_shared(), asf);

            let b = FlexBasisParseError::InvalidValue(payload);
            assert_eq!(b.to_contained().to_shared(), b);
        }
    }

    /// `to_shared()` must not panic on a directly-constructed owned error with a
    /// degenerate (empty) payload, and must hand back the exact same string.
    #[cfg(feature = "parser")]
    #[test]
    fn owned_errors_to_shared_on_degenerate_payloads() {
        let empty: AzString = String::new().into();

        assert_eq!(
            FlexDirectionParseErrorOwned::InvalidValue(empty.clone()).to_shared(),
            FlexDirectionParseError::InvalidValue("")
        );
        assert_eq!(
            FlexWrapParseErrorOwned::InvalidValue(empty.clone()).to_shared(),
            FlexWrapParseError::InvalidValue("")
        );
        assert_eq!(
            JustifyContentParseErrorOwned::InvalidValue(empty.clone()).to_shared(),
            JustifyContentParseError::InvalidValue("")
        );
        assert_eq!(
            AlignItemsParseErrorOwned::InvalidValue(empty.clone()).to_shared(),
            AlignItemsParseError::InvalidValue("")
        );
        assert_eq!(
            AlignContentParseErrorOwned::InvalidValue(empty.clone()).to_shared(),
            AlignContentParseError::InvalidValue("")
        );
        assert_eq!(
            AlignSelfParseErrorOwned::InvalidValue(empty.clone()).to_shared(),
            AlignSelfParseError::InvalidValue("")
        );
        assert_eq!(
            FlexBasisParseErrorOwned::InvalidValue(empty.clone()).to_shared(),
            FlexBasisParseError::InvalidValue("")
        );
        assert_eq!(
            FlexGrowParseErrorOwned::NegativeValue(empty.clone()).to_shared(),
            FlexGrowParseError::NegativeValue("")
        );
        assert_eq!(
            FlexShrinkParseErrorOwned::NegativeValue(empty).to_shared(),
            FlexShrinkParseError::NegativeValue("")
        );
    }

    /// Errors surfaced by the real parsers must convert to owned form and
    /// render a non-empty message that names the offending property.
    #[cfg(feature = "parser")]
    #[test]
    fn parser_errors_to_contained_and_display() {
        let e = parse_layout_flex_grow("bogus").unwrap_err();
        assert_eq!(e.to_contained().to_shared(), e);
        assert!(format!("{e}").contains("flex-grow"));

        let e = parse_layout_flex_shrink("-1").unwrap_err();
        assert_eq!(e.to_contained().to_shared(), e);
        assert!(format!("{e}").contains("flex-shrink"));

        let e = parse_layout_flex_direction("\u{1F600}").unwrap_err();
        assert_eq!(e.to_contained().to_shared(), e);
        assert!(format!("{e}").contains("flex-direction"));

        let e = parse_layout_flex_wrap("").unwrap_err();
        assert_eq!(e.to_contained().to_shared(), e);
        assert!(format!("{e}").contains("flex-wrap"));

        let e = parse_layout_justify_content("nope").unwrap_err();
        assert_eq!(e.to_contained().to_shared(), e);
        assert!(format!("{e}").contains("justify-content"));

        let e = parse_layout_align_items("nope").unwrap_err();
        assert_eq!(e.to_contained().to_shared(), e);
        assert!(format!("{e}").contains("align-items"));

        let e = parse_layout_align_content("nope").unwrap_err();
        assert_eq!(e.to_contained().to_shared(), e);
        assert!(format!("{e}").contains("align-content"));

        let e = parse_layout_align_self("nope").unwrap_err();
        assert_eq!(e.to_contained().to_shared(), e);
        assert!(format!("{e}").contains("align-self"));

        let e = parse_layout_flex_basis("none").unwrap_err();
        assert_eq!(e.to_contained().to_shared(), e);
        assert!(format!("{e}").contains("flex-basis"));
    }
}
