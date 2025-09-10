//! Flexbox layout properties

use alloc::string::String;
use core::fmt;

use crate::{
    error::{CssParsingError, CssPixelValueParseError},
    props::{basic::value::FloatValue, formatter::FormatAsCssValue},
};

/// CSS flex-direction property
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum LayoutFlexDirection {
    /// Items are placed in the same direction as text direction
    Row,
    /// Items are placed opposite to text direction
    RowReverse,
    /// Items are placed top to bottom
    Column,
    /// Items are placed bottom to top
    ColumnReverse,
}

/// CSS flex-wrap property
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum LayoutFlexWrap {
    /// Items stay on single line
    NoWrap,
    /// Items wrap to new lines as needed
    Wrap,
    /// Items wrap to new lines in reverse order
    WrapReverse,
}

/// CSS justify-content property
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum LayoutJustifyContent {
    /// Items are packed at the start
    FlexStart,
    /// Items are packed at the end
    FlexEnd,
    /// Items are centered
    Center,
    /// Items are evenly distributed with space between them
    SpaceBetween,
    /// Items are evenly distributed with space around them
    SpaceAround,
    /// Items are evenly distributed with equal space around them
    SpaceEvenly,
    /// Items are packed at the start (CSS Grid/Flexbox alignment)
    Start,
    /// Items are packed at the end (CSS Grid/Flexbox alignment)
    End,
}

/// CSS align-items property
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum LayoutAlignItems {
    /// Items are stretched to fill container
    Stretch,
    /// Items are aligned at the start of cross axis
    FlexStart,
    /// Items are aligned at the end of cross axis
    FlexEnd,
    /// Items are centered on cross axis
    Center,
    /// Items are aligned to their baselines
    Baseline,
    /// Items are packed at the start (CSS Grid/Flexbox alignment)
    Start,
    /// Items are packed at the end (CSS Grid/Flexbox alignment)
    End,
}

/// CSS align-content property
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum LayoutAlignContent {
    /// Lines are stretched to fill container
    Stretch,
    /// Lines are packed at the start
    FlexStart,
    /// Lines are packed at the end
    FlexEnd,
    /// Lines are centered
    Center,
    /// Lines are evenly distributed with space between them
    SpaceBetween,
    /// Lines are evenly distributed with space around them
    SpaceAround,
    /// Lines are evenly distributed with equal space around them
    SpaceEvenly,
    /// Lines are packed at the start (CSS Grid/Flexbox alignment)
    Start,
    /// Lines are packed at the end (CSS Grid/Flexbox alignment)
    End,
}

/// CSS flex-grow property
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct LayoutFlexGrow {
    pub inner: FloatValue,
}

/// CSS flex-shrink property
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct LayoutFlexShrink {
    pub inner: FloatValue,
}

// Default implementations
impl Default for LayoutFlexDirection {
    fn default() -> Self {
        LayoutFlexDirection::Row
    }
}

impl Default for LayoutFlexWrap {
    fn default() -> Self {
        LayoutFlexWrap::NoWrap
    }
}

impl Default for LayoutJustifyContent {
    fn default() -> Self {
        LayoutJustifyContent::FlexStart
    }
}

impl Default for LayoutAlignItems {
    fn default() -> Self {
        LayoutAlignItems::Stretch
    }
}

impl Default for LayoutAlignContent {
    fn default() -> Self {
        LayoutAlignContent::Stretch
    }
}

impl Default for LayoutFlexGrow {
    fn default() -> Self {
        Self {
            inner: FloatValue::new(0.0),
        }
    }
}

impl Default for LayoutFlexShrink {
    fn default() -> Self {
        Self {
            inner: FloatValue::new(1.0),
        }
    }
}

// Display implementations
impl fmt::Display for LayoutFlexDirection {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::LayoutFlexDirection::*;
        let s = match self {
            Row => "row",
            RowReverse => "row-reverse",
            Column => "column",
            ColumnReverse => "column-reverse",
        };
        write!(f, "{}", s)
    }
}

impl fmt::Display for LayoutFlexWrap {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::LayoutFlexWrap::*;
        let s = match self {
            NoWrap => "nowrap",
            Wrap => "wrap",
            WrapReverse => "wrap-reverse",
        };
        write!(f, "{}", s)
    }
}

impl fmt::Display for LayoutJustifyContent {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::LayoutJustifyContent::*;
        let s = match self {
            FlexStart => "flex-start",
            FlexEnd => "flex-end",
            Center => "center",
            SpaceBetween => "space-between",
            SpaceAround => "space-around",
            SpaceEvenly => "space-evenly",
            Start => "start",
            End => "end",
        };
        write!(f, "{}", s)
    }
}

impl fmt::Display for LayoutAlignItems {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::LayoutAlignItems::*;
        let s = match self {
            Stretch => "stretch",
            FlexStart => "flex-start",
            FlexEnd => "flex-end",
            Center => "center",
            Baseline => "baseline",
            Start => "start",
            End => "end",
        };
        write!(f, "{}", s)
    }
}

impl fmt::Display for LayoutAlignContent {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::LayoutAlignContent::*;
        let s = match self {
            Stretch => "stretch",
            FlexStart => "flex-start",
            FlexEnd => "flex-end",
            Center => "center",
            SpaceBetween => "space-between",
            SpaceAround => "space-around",
            SpaceEvenly => "space-evenly",
            Start => "start",
            End => "end",
        };
        write!(f, "{}", s)
    }
}

impl fmt::Display for LayoutFlexGrow {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.inner)
    }
}

impl fmt::Display for LayoutFlexShrink {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.inner)
    }
}

// FormatAsCssValue implementations
impl FormatAsCssValue for LayoutFlexDirection {
    fn format_as_css_value(&self) -> String {
        self.to_string()
    }
}

impl FormatAsCssValue for LayoutFlexWrap {
    fn format_as_css_value(&self) -> String {
        self.to_string()
    }
}

impl FormatAsCssValue for LayoutJustifyContent {
    fn format_as_css_value(&self) -> String {
        self.to_string()
    }
}

impl FormatAsCssValue for LayoutAlignItems {
    fn format_as_css_value(&self) -> String {
        self.to_string()
    }
}

impl FormatAsCssValue for LayoutAlignContent {
    fn format_as_css_value(&self) -> String {
        self.to_string()
    }
}

impl FormatAsCssValue for LayoutFlexGrow {
    fn format_as_css_value(&self) -> String {
        self.inner.format_as_css_value()
    }
}

impl FormatAsCssValue for LayoutFlexShrink {
    fn format_as_css_value(&self) -> String {
        self.inner.format_as_css_value()
    }
}

// Parsing functions
#[cfg(feature = "parser")]
pub fn parse_layout_flex_direction<'a>(
    input: &'a str,
) -> Result<LayoutFlexDirection, CssParsingError<'a>> {
    use self::LayoutFlexDirection::*;
    match input.trim() {
        "row" => Ok(Row),
        "row-reverse" => Ok(RowReverse),
        "column" => Ok(Column),
        "column-reverse" => Ok(ColumnReverse),
        _ => Err(CssParsingError::InvalidValue(input)),
    }
}

#[cfg(feature = "parser")]
pub fn parse_layout_flex_wrap<'a>(input: &'a str) -> Result<LayoutFlexWrap, CssParsingError<'a>> {
    use self::LayoutFlexWrap::*;
    match input.trim() {
        "nowrap" => Ok(NoWrap),
        "wrap" => Ok(Wrap),
        "wrap-reverse" => Ok(WrapReverse),
        _ => Err(CssParsingError::InvalidValue(input)),
    }
}

#[cfg(feature = "parser")]
pub fn parse_layout_justify_content<'a>(
    input: &'a str,
) -> Result<LayoutJustifyContent, CssParsingError<'a>> {
    use self::LayoutJustifyContent::*;
    match input.trim() {
        "flex-start" => Ok(FlexStart),
        "flex-end" => Ok(FlexEnd),
        "center" => Ok(Center),
        "space-between" => Ok(SpaceBetween),
        "space-around" => Ok(SpaceAround),
        "space-evenly" => Ok(SpaceEvenly),
        "start" => Ok(Start),
        "end" => Ok(End),
        _ => Err(CssParsingError::InvalidValue(input)),
    }
}

#[cfg(feature = "parser")]
pub fn parse_layout_align_items<'a>(
    input: &'a str,
) -> Result<LayoutAlignItems, CssParsingError<'a>> {
    use self::LayoutAlignItems::*;
    match input.trim() {
        "stretch" => Ok(Stretch),
        "flex-start" => Ok(FlexStart),
        "flex-end" => Ok(FlexEnd),
        "center" => Ok(Center),
        "baseline" => Ok(Baseline),
        "start" => Ok(Start),
        "end" => Ok(End),
        _ => Err(CssParsingError::InvalidValue(input)),
    }
}

#[cfg(feature = "parser")]
pub fn parse_layout_align_content<'a>(
    input: &'a str,
) -> Result<LayoutAlignContent, CssParsingError<'a>> {
    use self::LayoutAlignContent::*;
    match input.trim() {
        "stretch" => Ok(Stretch),
        "flex-start" => Ok(FlexStart),
        "flex-end" => Ok(FlexEnd),
        "center" => Ok(Center),
        "space-between" => Ok(SpaceBetween),
        "space-around" => Ok(SpaceAround),
        "space-evenly" => Ok(SpaceEvenly),
        "start" => Ok(Start),
        "end" => Ok(End),
        _ => Err(CssParsingError::InvalidValue(input)),
    }
}

#[cfg(feature = "parser")]
pub fn parse_layout_flex_grow<'a>(
    input: &'a str,
) -> Result<LayoutFlexGrow, CssPixelValueParseError<'a>> {
    let float_value = crate::props::basic::value::parse_float_value(input)?;
    Ok(LayoutFlexGrow { inner: float_value })
}

#[cfg(feature = "parser")]
pub fn parse_layout_flex_shrink<'a>(
    input: &'a str,
) -> Result<LayoutFlexShrink, CssPixelValueParseError<'a>> {
    let float_value = crate::props::basic::value::parse_float_value(input)?;
    Ok(LayoutFlexShrink { inner: float_value })
}

// Constructor implementations
impl LayoutFlexGrow {
    pub fn new(value: f32) -> Self {
        Self {
            inner: FloatValue::new(value),
        }
    }
}

impl LayoutFlexShrink {
    pub fn new(value: f32) -> Self {
        Self {
            inner: FloatValue::new(value),
        }
    }
}
