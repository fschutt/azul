//! CSS properties related to dimensions and sizing.

use alloc::{
    string::{String, ToString},
    vec::Vec,
};

use crate::{
    impl_option, impl_option_inner, impl_vec, impl_vec_clone, impl_vec_debug, impl_vec_eq,
    impl_vec_hash, impl_vec_mut, impl_vec_ord, impl_vec_partialeq, impl_vec_partialord,
    props::{
        basic::pixel::{CssPixelValueParseError, CssPixelValueParseErrorOwned, PixelValue},
        formatter::PrintAsCssValue,
        macros::PixelValueTaker,
    },
};

// -- Calc AST --

/// A single item in a `calc()` expression, stored as a flat stack-machine representation.
///
/// The expression `calc(33.333% - 10px)` is stored as:
/// ```text
/// [Value(33.333%), Sub, Value(10px)]
/// ```
///
/// For nested expressions like `calc(100% - (20px + 5%))`:
/// ```text
/// [Value(100%), Sub, BraceOpen, Value(20px), Add, Value(5%), BraceClose]
/// ```
///
/// **Resolution**: Walk left to right. When `BraceClose` is hit, resolve everything
/// back to the matching `BraceOpen`, replace that span with a single `Value`, and continue.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
pub enum CalcAstItem {
    /// A literal value (e.g. `10px`, `33.333%`, `2em`)
    Value(PixelValue),
    /// `+` operator
    Add,
    /// `-` operator
    Sub,
    /// `*` operator
    Mul,
    /// `/` operator
    Div,
    /// `(` — opens a sub-expression
    BraceOpen,
    /// `)` — closes a sub-expression; triggers resolution of the inner span
    BraceClose,
}

// C-compatible Vec for CalcAstItem
impl_vec!(
    CalcAstItem,
    CalcAstItemVec,
    CalcAstItemVecDestructor,
    CalcAstItemVecDestructorType,
    CalcAstItemVecSlice,
    OptionCalcAstItem
);
impl_vec_clone!(CalcAstItem, CalcAstItemVec, CalcAstItemVecDestructor);
impl_vec_debug!(CalcAstItem, CalcAstItemVec);
impl_vec_partialeq!(CalcAstItem, CalcAstItemVec);
impl_vec_eq!(CalcAstItem, CalcAstItemVec);
impl_vec_partialord!(CalcAstItem, CalcAstItemVec);
impl_vec_ord!(CalcAstItem, CalcAstItemVec);
impl_vec_hash!(CalcAstItem, CalcAstItemVec);
impl_vec_mut!(CalcAstItem, CalcAstItemVec);

impl_option!(
    CalcAstItem,
    OptionCalcAstItem,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

/// Parse a calc() inner expression (the part between the parentheses) into
/// a flat `CalcAstItemVec` suitable for stack-machine evaluation.
///
/// Examples:
/// - `"100% - 20px"` → `[Value(100%), Sub, Value(20px)]`
/// - `"(100% - 20px) / 3"` → `[BraceOpen, Value(100%), Sub, Value(20px), BraceClose, Div, Value(3)]`
///
/// **Tokenisation rules**:
///  - Whitespace is skipped between tokens.
///  - `+`, `-`, `*`, `/` are operators (but `-` at the start of a number is
///    part of the number literal, e.g. `-10px`).
///  - `(` / `)` produce `BraceOpen` / `BraceClose`.
///  - Anything else is parsed as a `PixelValue` via `parse_pixel_value`.
#[cfg(feature = "parser")]
fn parse_calc_expression(input: &str) -> Result<CalcAstItemVec, ()> {
    use crate::props::basic::pixel::parse_pixel_value;

    let mut items: Vec<CalcAstItem> = Vec::new();
    let input = input.trim();
    let bytes = input.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        // Skip whitespace
        if bytes[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }

        match bytes[i] {
            b'+' => { items.push(CalcAstItem::Add); i += 1; }
            b'*' => { items.push(CalcAstItem::Mul); i += 1; }
            b'/' => { items.push(CalcAstItem::Div); i += 1; }
            b'(' => { items.push(CalcAstItem::BraceOpen); i += 1; }
            b')' => { items.push(CalcAstItem::BraceClose); i += 1; }
            b'-' => {
                // Decide: is this a subtraction operator or a negative number?
                // It's a negative number if:
                //   - it's the first token, OR
                //   - the previous token is an operator or BraceOpen
                let is_negative_number = items.is_empty()
                    || matches!(
                        items.last(),
                        Some(CalcAstItem::Add)
                            | Some(CalcAstItem::Sub)
                            | Some(CalcAstItem::Mul)
                            | Some(CalcAstItem::Div)
                            | Some(CalcAstItem::BraceOpen)
                    );

                if is_negative_number {
                    // Parse as negative number value
                    let rest = &input[i..];
                    let end = find_value_end(rest);
                    if end == 0 { return Err(()); }
                    let val_str = &rest[..end];
                    let pv = parse_pixel_value(val_str).map_err(|_| ())?;
                    items.push(CalcAstItem::Value(pv));
                    i += end;
                } else {
                    items.push(CalcAstItem::Sub);
                    i += 1;
                }
            }
            _ => {
                // Must be a numeric value (e.g. 100%, 20px, 3, 1.5em)
                let rest = &input[i..];
                let end = find_value_end(rest);
                if end == 0 { return Err(()); }
                let val_str = &rest[..end];
                let pv = parse_pixel_value(val_str).map_err(|_| ())?;
                items.push(CalcAstItem::Value(pv));
                i += end;
            }
        }
    }

    if items.is_empty() {
        return Err(());
    }

    Ok(CalcAstItemVec::from(items))
}

/// Find the end of a numeric value token in a calc() expression.
/// Returns the byte offset where the value ends.
#[cfg(feature = "parser")]
fn find_value_end(s: &str) -> usize {
    let bytes = s.as_bytes();
    let mut i = 0;

    // Optional leading sign
    if i < bytes.len() && (bytes[i] == b'-' || bytes[i] == b'+') {
        i += 1;
    }

    // Digits and decimal point
    while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
        i += 1;
    }

    // Unit suffix (alphabetic characters like px, %, em, rem, vw, vh, etc.)
    while i < bytes.len() && (bytes[i].is_ascii_alphabetic() || bytes[i] == b'%') {
        i += 1;
    }

    i
}

// -- Type Definitions --

macro_rules! define_dimension_property {
    ($struct_name:ident, $default_fn:expr) => {
        #[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[repr(C)]
        pub struct $struct_name {
            pub inner: PixelValue,
        }

        impl Default for $struct_name {
            fn default() -> Self {
                $default_fn()
            }
        }

        impl PixelValueTaker for $struct_name {
            fn from_pixel_value(inner: PixelValue) -> Self {
                Self { inner }
            }
        }

        impl_pixel_value!($struct_name);

        impl PrintAsCssValue for $struct_name {
            fn print_as_css_value(&self) -> String {
                self.inner.to_string()
            }
        }
    };
}

// Custom implementation for LayoutWidth to support min-content, max-content, and calc()
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
pub enum LayoutWidth {
    Auto,
    Px(PixelValue),
    MinContent,
    MaxContent,
    /// `calc()` expression stored as a flat stack-machine AST
    Calc(CalcAstItemVec),
}

impl Default for LayoutWidth {
    fn default() -> Self {
        LayoutWidth::Auto
    }
}

impl PixelValueTaker for LayoutWidth {
    fn from_pixel_value(inner: PixelValue) -> Self {
        LayoutWidth::Px(inner)
    }
}

impl PrintAsCssValue for LayoutWidth {
    fn print_as_css_value(&self) -> String {
        match self {
            LayoutWidth::Auto => "auto".to_string(),
            LayoutWidth::Px(v) => v.to_string(),
            LayoutWidth::MinContent => "min-content".to_string(),
            LayoutWidth::MaxContent => "max-content".to_string(),
            LayoutWidth::Calc(items) => {
                let inner: Vec<String> = items.iter().map(|i| match i {
                    CalcAstItem::Value(v) => v.to_string(),
                    CalcAstItem::Add => "+".to_string(),
                    CalcAstItem::Sub => "-".to_string(),
                    CalcAstItem::Mul => "*".to_string(),
                    CalcAstItem::Div => "/".to_string(),
                    CalcAstItem::BraceOpen => "(".to_string(),
                    CalcAstItem::BraceClose => ")".to_string(),
                }).collect();
                alloc::format!("calc({})", inner.join(" "))
            }
        }
    }
}

impl LayoutWidth {
    pub fn px(value: f32) -> Self {
        LayoutWidth::Px(PixelValue::px(value))
    }

    pub const fn const_px(value: isize) -> Self {
        LayoutWidth::Px(PixelValue::const_px(value))
    }

    pub fn interpolate(&self, other: &Self, t: f32) -> Self {
        match (self, other) {
            (LayoutWidth::Px(a), LayoutWidth::Px(b)) => LayoutWidth::Px(a.interpolate(b, t)),
            (_, LayoutWidth::Px(b)) if t >= 0.5 => LayoutWidth::Px(*b),
            (LayoutWidth::Px(a), _) if t < 0.5 => LayoutWidth::Px(*a),
            (LayoutWidth::Auto, LayoutWidth::Auto) => LayoutWidth::Auto,
            (a, _) if t < 0.5 => a.clone(),
            (_, b) => b.clone(),
        }
    }
}

// Custom implementation for LayoutHeight to support min-content, max-content, and calc()
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
pub enum LayoutHeight {
    Auto,
    Px(PixelValue),
    MinContent,
    MaxContent,
    /// `calc()` expression stored as a flat stack-machine AST
    Calc(CalcAstItemVec),
}

impl Default for LayoutHeight {
    fn default() -> Self {
        LayoutHeight::Auto
    }
}

impl PixelValueTaker for LayoutHeight {
    fn from_pixel_value(inner: PixelValue) -> Self {
        LayoutHeight::Px(inner)
    }
}

impl PrintAsCssValue for LayoutHeight {
    fn print_as_css_value(&self) -> String {
        match self {
            LayoutHeight::Auto => "auto".to_string(),
            LayoutHeight::Px(v) => v.to_string(),
            LayoutHeight::MinContent => "min-content".to_string(),
            LayoutHeight::MaxContent => "max-content".to_string(),
            LayoutHeight::Calc(items) => {
                let inner: Vec<String> = items.iter().map(|i| match i {
                    CalcAstItem::Value(v) => v.to_string(),
                    CalcAstItem::Add => "+".to_string(),
                    CalcAstItem::Sub => "-".to_string(),
                    CalcAstItem::Mul => "*".to_string(),
                    CalcAstItem::Div => "/".to_string(),
                    CalcAstItem::BraceOpen => "(".to_string(),
                    CalcAstItem::BraceClose => ")".to_string(),
                }).collect();
                alloc::format!("calc({})", inner.join(" "))
            }
        }
    }
}

impl LayoutHeight {
    pub fn px(value: f32) -> Self {
        LayoutHeight::Px(PixelValue::px(value))
    }

    pub const fn const_px(value: isize) -> Self {
        LayoutHeight::Px(PixelValue::const_px(value))
    }

    pub fn interpolate(&self, other: &Self, t: f32) -> Self {
        match (self, other) {
            (LayoutHeight::Px(a), LayoutHeight::Px(b)) => LayoutHeight::Px(a.interpolate(b, t)),
            (_, LayoutHeight::Px(b)) if t >= 0.5 => LayoutHeight::Px(*b),
            (LayoutHeight::Px(a), _) if t < 0.5 => LayoutHeight::Px(*a),
            (LayoutHeight::Auto, LayoutHeight::Auto) => LayoutHeight::Auto,
            (a, _) if t < 0.5 => a.clone(),
            (_, b) => b.clone(),
        }
    }
}

define_dimension_property!(LayoutMinWidth, || Self {
    inner: PixelValue::zero()
});
define_dimension_property!(LayoutMinHeight, || Self {
    inner: PixelValue::zero()
});
define_dimension_property!(LayoutMaxWidth, || Self {
    inner: PixelValue::px(core::f32::MAX)
});
define_dimension_property!(LayoutMaxHeight, || Self {
    inner: PixelValue::px(core::f32::MAX)
});

/// Represents a `box-sizing` attribute
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum LayoutBoxSizing {
    ContentBox,
    BorderBox,
}

impl Default for LayoutBoxSizing {
    fn default() -> Self {
        LayoutBoxSizing::ContentBox
    }
}

impl PrintAsCssValue for LayoutBoxSizing {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            LayoutBoxSizing::ContentBox => "content-box",
            LayoutBoxSizing::BorderBox => "border-box",
        })
    }
}

// -- Parser --

#[cfg(feature = "parser")]
pub mod parser {

    use alloc::string::ToString;
    use crate::corety::AzString;

    use super::*;
    use crate::props::basic::pixel::parse_pixel_value;

    macro_rules! define_pixel_dimension_parser {
        ($fn_name:ident, $struct_name:ident, $error_name:ident, $error_owned_name:ident) => {
            #[derive(Clone, PartialEq)]
            pub enum $error_name<'a> {
                PixelValue(CssPixelValueParseError<'a>),
            }

            impl_debug_as_display!($error_name<'a>);
            impl_display! { $error_name<'a>, {
                PixelValue(e) => format!("{}", e),
            }}

            impl_from! { CssPixelValueParseError<'a>, $error_name::PixelValue }

            #[derive(Debug, Clone, PartialEq)]
            #[repr(C, u8)]
            pub enum $error_owned_name {
                PixelValue(CssPixelValueParseErrorOwned),
            }

            impl<'a> $error_name<'a> {
                pub fn to_contained(&self) -> $error_owned_name {
                    match self {
                        $error_name::PixelValue(e) => {
                            $error_owned_name::PixelValue(e.to_contained())
                        }
                    }
                }
            }

            impl $error_owned_name {
                pub fn to_shared<'a>(&'a self) -> $error_name<'a> {
                    match self {
                        $error_owned_name::PixelValue(e) => $error_name::PixelValue(e.to_shared()),
                    }
                }
            }

            pub fn $fn_name<'a>(input: &'a str) -> Result<$struct_name, $error_name<'a>> {
                parse_pixel_value(input)
                    .map(|v| $struct_name { inner: v })
                    .map_err($error_name::PixelValue)
            }
        };
    }

    // Custom parsers for LayoutWidth and LayoutHeight with min-content/max-content support

    #[derive(Clone, PartialEq)]
    pub enum LayoutWidthParseError<'a> {
        PixelValue(CssPixelValueParseError<'a>),
        InvalidKeyword(&'a str),
    }

    impl_debug_as_display!(LayoutWidthParseError<'a>);
    impl_display! { LayoutWidthParseError<'a>, {
        PixelValue(e) => format!("{}", e),
        InvalidKeyword(k) => format!("Invalid width keyword: \"{}\"", k),
    }}

    impl_from! { CssPixelValueParseError<'a>, LayoutWidthParseError::PixelValue }

    #[derive(Debug, Clone, PartialEq)]
    #[repr(C, u8)]
    pub enum LayoutWidthParseErrorOwned {
        PixelValue(CssPixelValueParseErrorOwned),
        InvalidKeyword(AzString),
    }

    impl<'a> LayoutWidthParseError<'a> {
        pub fn to_contained(&self) -> LayoutWidthParseErrorOwned {
            match self {
                LayoutWidthParseError::PixelValue(e) => {
                    LayoutWidthParseErrorOwned::PixelValue(e.to_contained())
                }
                LayoutWidthParseError::InvalidKeyword(k) => {
                    LayoutWidthParseErrorOwned::InvalidKeyword(k.to_string().into())
                }
            }
        }
    }

    impl LayoutWidthParseErrorOwned {
        pub fn to_shared<'a>(&'a self) -> LayoutWidthParseError<'a> {
            match self {
                LayoutWidthParseErrorOwned::PixelValue(e) => {
                    LayoutWidthParseError::PixelValue(e.to_shared())
                }
                LayoutWidthParseErrorOwned::InvalidKeyword(k) => {
                    LayoutWidthParseError::InvalidKeyword(k)
                }
            }
        }
    }

    pub fn parse_layout_width<'a>(
        input: &'a str,
    ) -> Result<LayoutWidth, LayoutWidthParseError<'a>> {
        let trimmed = input.trim();
        match trimmed {
            "auto" => Ok(LayoutWidth::Auto),
            "min-content" => Ok(LayoutWidth::MinContent),
            "max-content" => Ok(LayoutWidth::MaxContent),
            s if s.starts_with("calc(") && s.ends_with(')') => {
                let inner = &s[5..s.len() - 1];
                parse_calc_expression(inner)
                    .map(LayoutWidth::Calc)
                    .map_err(|_| LayoutWidthParseError::InvalidKeyword(input))
            }
            _ => parse_pixel_value(trimmed)
                .map(LayoutWidth::Px)
                .map_err(LayoutWidthParseError::PixelValue),
        }
    }

    #[derive(Clone, PartialEq)]
    pub enum LayoutHeightParseError<'a> {
        PixelValue(CssPixelValueParseError<'a>),
        InvalidKeyword(&'a str),
    }

    impl_debug_as_display!(LayoutHeightParseError<'a>);
    impl_display! { LayoutHeightParseError<'a>, {
        PixelValue(e) => format!("{}", e),
        InvalidKeyword(k) => format!("Invalid height keyword: \"{}\"", k),
    }}

    impl_from! { CssPixelValueParseError<'a>, LayoutHeightParseError::PixelValue }

    #[derive(Debug, Clone, PartialEq)]
    #[repr(C, u8)]
    pub enum LayoutHeightParseErrorOwned {
        PixelValue(CssPixelValueParseErrorOwned),
        InvalidKeyword(AzString),
    }

    impl<'a> LayoutHeightParseError<'a> {
        pub fn to_contained(&self) -> LayoutHeightParseErrorOwned {
            match self {
                LayoutHeightParseError::PixelValue(e) => {
                    LayoutHeightParseErrorOwned::PixelValue(e.to_contained())
                }
                LayoutHeightParseError::InvalidKeyword(k) => {
                    LayoutHeightParseErrorOwned::InvalidKeyword(k.to_string().into())
                }
            }
        }
    }

    impl LayoutHeightParseErrorOwned {
        pub fn to_shared<'a>(&'a self) -> LayoutHeightParseError<'a> {
            match self {
                LayoutHeightParseErrorOwned::PixelValue(e) => {
                    LayoutHeightParseError::PixelValue(e.to_shared())
                }
                LayoutHeightParseErrorOwned::InvalidKeyword(k) => {
                    LayoutHeightParseError::InvalidKeyword(k)
                }
            }
        }
    }

    pub fn parse_layout_height<'a>(
        input: &'a str,
    ) -> Result<LayoutHeight, LayoutHeightParseError<'a>> {
        let trimmed = input.trim();
        match trimmed {
            "auto" => Ok(LayoutHeight::Auto),
            "min-content" => Ok(LayoutHeight::MinContent),
            "max-content" => Ok(LayoutHeight::MaxContent),
            s if s.starts_with("calc(") && s.ends_with(')') => {
                let inner = &s[5..s.len() - 1];
                parse_calc_expression(inner)
                    .map(LayoutHeight::Calc)
                    .map_err(|_| LayoutHeightParseError::InvalidKeyword(input))
            }
            _ => parse_pixel_value(trimmed)
                .map(LayoutHeight::Px)
                .map_err(LayoutHeightParseError::PixelValue),
        }
    }
    define_pixel_dimension_parser!(
        parse_layout_min_width,
        LayoutMinWidth,
        LayoutMinWidthParseError,
        LayoutMinWidthParseErrorOwned
    );
    define_pixel_dimension_parser!(
        parse_layout_min_height,
        LayoutMinHeight,
        LayoutMinHeightParseError,
        LayoutMinHeightParseErrorOwned
    );
    define_pixel_dimension_parser!(
        parse_layout_max_width,
        LayoutMaxWidth,
        LayoutMaxWidthParseError,
        LayoutMaxWidthParseErrorOwned
    );
    define_pixel_dimension_parser!(
        parse_layout_max_height,
        LayoutMaxHeight,
        LayoutMaxHeightParseError,
        LayoutMaxHeightParseErrorOwned
    );

    // -- Box Sizing Parser --

    #[derive(Clone, PartialEq)]
    pub enum LayoutBoxSizingParseError<'a> {
        InvalidValue(&'a str),
    }

    impl_debug_as_display!(LayoutBoxSizingParseError<'a>);
    impl_display! { LayoutBoxSizingParseError<'a>, {
        InvalidValue(v) => format!("Invalid box-sizing value: \"{}\"", v),
    }}

    #[derive(Debug, Clone, PartialEq)]
    #[repr(C, u8)]
    pub enum LayoutBoxSizingParseErrorOwned {
        InvalidValue(AzString),
    }

    impl<'a> LayoutBoxSizingParseError<'a> {
        pub fn to_contained(&self) -> LayoutBoxSizingParseErrorOwned {
            match self {
                LayoutBoxSizingParseError::InvalidValue(s) => {
                    LayoutBoxSizingParseErrorOwned::InvalidValue(s.to_string().into())
                }
            }
        }
    }

    impl LayoutBoxSizingParseErrorOwned {
        pub fn to_shared<'a>(&'a self) -> LayoutBoxSizingParseError<'a> {
            match self {
                LayoutBoxSizingParseErrorOwned::InvalidValue(s) => {
                    LayoutBoxSizingParseError::InvalidValue(s)
                }
            }
        }
    }

    pub fn parse_layout_box_sizing<'a>(
        input: &'a str,
    ) -> Result<LayoutBoxSizing, LayoutBoxSizingParseError<'a>> {
        match input.trim() {
            "content-box" => Ok(LayoutBoxSizing::ContentBox),
            "border-box" => Ok(LayoutBoxSizing::BorderBox),
            other => Err(LayoutBoxSizingParseError::InvalidValue(other)),
        }
    }
}

#[cfg(feature = "parser")]
pub use self::parser::*;

#[cfg(all(test, feature = "parser"))]
mod tests {
    use super::*;
    use crate::props::basic::pixel::PixelValue;

    #[test]
    fn test_parse_layout_width() {
        assert_eq!(
            parse_layout_width("150px").unwrap(),
            LayoutWidth::Px(PixelValue::px(150.0))
        );
        assert_eq!(
            parse_layout_width("2.5em").unwrap(),
            LayoutWidth::Px(PixelValue::em(2.5))
        );
        assert_eq!(
            parse_layout_width("75%").unwrap(),
            LayoutWidth::Px(PixelValue::percent(75.0))
        );
        assert_eq!(
            parse_layout_width("0").unwrap(),
            LayoutWidth::Px(PixelValue::px(0.0))
        );
        assert_eq!(
            parse_layout_width("  100pt  ").unwrap(),
            LayoutWidth::Px(PixelValue::pt(100.0))
        );
        assert_eq!(
            parse_layout_width("min-content").unwrap(),
            LayoutWidth::MinContent
        );
        assert_eq!(
            parse_layout_width("max-content").unwrap(),
            LayoutWidth::MaxContent
        );
    }

    #[test]
    fn test_parse_layout_height_invalid() {
        // "auto" is now a valid value for height (CSS spec)
        assert!(parse_layout_height("auto").is_ok());
        // Liberal parsing accepts whitespace between number and unit
        assert!(parse_layout_height("150 px").is_ok());
        assert!(parse_layout_height("px").is_err());
        assert!(parse_layout_height("invalid").is_err());
    }

    #[test]
    fn test_parse_layout_box_sizing() {
        assert_eq!(
            parse_layout_box_sizing("content-box").unwrap(),
            LayoutBoxSizing::ContentBox
        );
        assert_eq!(
            parse_layout_box_sizing("border-box").unwrap(),
            LayoutBoxSizing::BorderBox
        );
        assert_eq!(
            parse_layout_box_sizing("  border-box  ").unwrap(),
            LayoutBoxSizing::BorderBox
        );
    }

    #[test]
    fn test_parse_layout_box_sizing_invalid() {
        assert!(parse_layout_box_sizing("padding-box").is_err());
        assert!(parse_layout_box_sizing("borderbox").is_err());
        assert!(parse_layout_box_sizing("").is_err());
    }
}
