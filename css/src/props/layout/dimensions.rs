//! CSS properties related to dimensions and sizing.
//!
//! Key types: [`LayoutWidth`] / [`LayoutHeight`] (support `auto`, pixel values,
//! `min-content`, `max-content`, `fit-content()`, and `calc()` expressions),
//! [`LayoutMinWidth`], [`LayoutMinHeight`], [`LayoutMaxWidth`], [`LayoutMaxHeight`]
//! (simple pixel-value constraints), and [`LayoutBoxSizing`].
//!
//! `calc()` expressions use a flat stack-machine representation via [`CalcAstItem`]
//! — see its documentation for the encoding scheme. The layout solver in
//! `layout/src/solver3/calc.rs` evaluates these at resolve time.

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

/// C-compatible `Vec<CalcAstItem>` for FFI interop.
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

/// Parse a `calc()` inner expression (the part between the parentheses) into
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
                        Some(CalcAstItem::Add | CalcAstItem::Sub | CalcAstItem::Mul | CalcAstItem::Div
| CalcAstItem::BraceOpen)
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

/// Find the end of a numeric value token in a `calc()` expression.
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

/// Format a `CalcAstItemVec` as a CSS `calc(...)` string.
fn calc_ast_to_css_string(items: &CalcAstItemVec) -> String {
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

macro_rules! define_sizing_enum {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[repr(C, u8)]
        #[derive(Default)]
        pub enum $name {
            #[default]
            Auto,
            Px(PixelValue),
            MinContent,
            MaxContent,
            /// `fit-content(<length-percentage>)` = `min(max-content, max(min-content, <length-percentage>))`
            FitContent(PixelValue),
            /// `calc()` expression stored as a flat stack-machine AST
            Calc(CalcAstItemVec),
        }

        impl PixelValueTaker for $name {
            fn from_pixel_value(inner: PixelValue) -> Self {
                $name::Px(inner)
            }
        }

        impl PrintAsCssValue for $name {
            fn print_as_css_value(&self) -> String {
                match self {
                    $name::Auto => "auto".to_string(),
                    $name::Px(v) => v.to_string(),
                    $name::MinContent => "min-content".to_string(),
                    $name::MaxContent => "max-content".to_string(),
                    $name::FitContent(v) => alloc::format!("fit-content({})", v),
                    $name::Calc(items) => calc_ast_to_css_string(items),
                }
            }
        }

        impl $name {
            #[must_use] pub fn px(value: f32) -> Self {
                $name::Px(PixelValue::px(value))
            }

            #[must_use] pub const fn const_px(value: isize) -> Self {
                $name::Px(PixelValue::const_px(value))
            }

            #[must_use] pub fn interpolate(&self, other: &Self, t: f32) -> Self {
                match (self, other) {
                    ($name::Px(a), $name::Px(b)) => $name::Px(a.interpolate(b, t)),
                    ($name::FitContent(a), $name::FitContent(b)) => $name::FitContent(a.interpolate(b, t)),
                    (_, $name::Px(b)) if t >= 0.5 => $name::Px(*b),
                    ($name::Px(a), _) if t < 0.5 => $name::Px(*a),
                    ($name::Auto, $name::Auto) => $name::Auto,
                    (a, _) if t < 0.5 => a.clone(),
                    (_, b) => b.clone(),
                }
            }
        }
    };
}

define_sizing_enum!(LayoutWidth);
define_sizing_enum!(LayoutHeight);

/// CSS `min-width` property. Defaults to `0px`.
define_dimension_property!(LayoutMinWidth, || Self {
    inner: PixelValue::zero()
});
/// CSS `min-height` property. Defaults to `0px`.
define_dimension_property!(LayoutMinHeight, || Self {
    inner: PixelValue::zero()
});
/// CSS `max-width` property. Defaults to `f32::MAX` pixels (i.e. unconstrained).
///
/// NOTE: The layout solver must handle `f32::MAX` gracefully — adding
/// padding/margin to this sentinel would overflow to infinity.
define_dimension_property!(LayoutMaxWidth, || Self {
    inner: PixelValue::px(core::f32::MAX)
});
/// CSS `max-height` property. Defaults to `f32::MAX` pixels (i.e. unconstrained).
///
/// NOTE: The layout solver must handle `f32::MAX` gracefully — adding
/// padding/margin to this sentinel would overflow to infinity.
define_dimension_property!(LayoutMaxHeight, || Self {
    inner: PixelValue::px(core::f32::MAX)
});

/// Represents a `box-sizing` attribute
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
#[derive(Default)]
pub enum LayoutBoxSizing {
    #[default]
    ContentBox,
    BorderBox,
}


impl PrintAsCssValue for LayoutBoxSizing {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            Self::ContentBox => "content-box",
            Self::BorderBox => "border-box",
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
            #[derive(Clone, PartialEq, Eq)]
            pub enum $error_name<'a> {
                PixelValue(CssPixelValueParseError<'a>),
            }

            impl_debug_as_display!($error_name<'a>);
            impl_display! { $error_name<'a>, {
                PixelValue(e) => format!("{}", e),
            }}

            impl_from! { CssPixelValueParseError<'a>, $error_name::PixelValue }

            #[derive(Debug, Clone, PartialEq, Eq)]
            #[repr(C, u8)]
            pub enum $error_owned_name {
                PixelValue(CssPixelValueParseErrorOwned),
            }

            impl $error_name<'_> {
                #[must_use] pub fn to_contained(&self) -> $error_owned_name {
                    match self {
                        $error_name::PixelValue(e) => {
                            $error_owned_name::PixelValue(e.to_contained())
                        }
                    }
                }
            }

            impl $error_owned_name {
                #[must_use] pub fn to_shared(&self) -> $error_name<'_> {
                    match self {
                        $error_owned_name::PixelValue(e) => $error_name::PixelValue(e.to_shared()),
                    }
                }
            }

            pub fn $fn_name(input: &str) -> Result<$struct_name, $error_name<'_>> {
                parse_pixel_value(input)
                    .map(|v| $struct_name { inner: v })
                    .map_err($error_name::PixelValue)
            }
        };
    }

    macro_rules! define_sizing_parser {
        ($fn_name:ident, $enum_name:ident, $error_name:ident, $error_owned_name:ident, $keyword_label:expr) => {
            #[derive(Clone, PartialEq, Eq)]
            pub enum $error_name<'a> {
                PixelValue(CssPixelValueParseError<'a>),
                InvalidKeyword(&'a str),
            }

            impl_debug_as_display!($error_name<'a>);
            impl_display! { $error_name<'a>, {
                PixelValue(e) => format!("{}", e),
                InvalidKeyword(k) => format!("Invalid {} keyword: \"{}\"", $keyword_label, k),
            }}

            impl_from! { CssPixelValueParseError<'a>, $error_name::PixelValue }

            #[derive(Debug, Clone, PartialEq, Eq)]
            #[repr(C, u8)]
            pub enum $error_owned_name {
                PixelValue(CssPixelValueParseErrorOwned),
                InvalidKeyword(AzString),
            }

            impl $error_name<'_> {
                #[must_use] pub fn to_contained(&self) -> $error_owned_name {
                    match self {
                        $error_name::PixelValue(e) => {
                            $error_owned_name::PixelValue(e.to_contained())
                        }
                        $error_name::InvalidKeyword(k) => {
                            $error_owned_name::InvalidKeyword(k.to_string().into())
                        }
                    }
                }
            }

            impl $error_owned_name {
                #[must_use] pub fn to_shared(&self) -> $error_name<'_> {
                    match self {
                        $error_owned_name::PixelValue(e) => {
                            $error_name::PixelValue(e.to_shared())
                        }
                        $error_owned_name::InvalidKeyword(k) => {
                            $error_name::InvalidKeyword(k)
                        }
                    }
                }
            }

            pub fn $fn_name(
                input: &str,
            ) -> Result<$enum_name, $error_name<'_>> {
                let trimmed = input.trim();
                match trimmed {
                    "auto" => Ok($enum_name::Auto),
                    "min-content" => Ok($enum_name::MinContent),
                    "max-content" => Ok($enum_name::MaxContent),
                    s if s.starts_with("fit-content(") && s.ends_with(')') => {
                        let inner = &s[12..s.len() - 1].trim();
                        parse_pixel_value(inner)
                            .map(|pv| {
                                if pv.number.get() < 0.0 {
                                    $enum_name::FitContent(PixelValue::zero())
                                } else {
                                    $enum_name::FitContent(pv)
                                }
                            })
                            .map_err($error_name::PixelValue)
                    }
                    s if s.starts_with("calc(") && s.ends_with(')') => {
                        let inner = &s[5..s.len() - 1];
                        parse_calc_expression(inner)
                            .map($enum_name::Calc)
                            .map_err(|_| $error_name::InvalidKeyword(input))
                    }
                    _ => parse_pixel_value(trimmed)
                        .map($enum_name::Px)
                        .map_err($error_name::PixelValue),
                }
            }
        };
    }

    define_sizing_parser!(parse_layout_width, LayoutWidth, LayoutWidthParseError, LayoutWidthParseErrorOwned, "width");
    define_sizing_parser!(parse_layout_height, LayoutHeight, LayoutHeightParseError, LayoutHeightParseErrorOwned, "height");
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

    #[derive(Clone, PartialEq, Eq)]
    pub enum LayoutBoxSizingParseError<'a> {
        InvalidValue(&'a str),
    }

    impl_debug_as_display!(LayoutBoxSizingParseError<'a>);
    impl_display! { LayoutBoxSizingParseError<'a>, {
        InvalidValue(v) => format!("Invalid box-sizing value: \"{}\"", v),
    }}

    #[derive(Debug, Clone, PartialEq, Eq)]
    #[repr(C, u8)]
    pub enum LayoutBoxSizingParseErrorOwned {
        InvalidValue(AzString),
    }

    impl LayoutBoxSizingParseError<'_> {
        #[must_use] pub fn to_contained(&self) -> LayoutBoxSizingParseErrorOwned {
            match self {
                LayoutBoxSizingParseError::InvalidValue(s) => {
                    LayoutBoxSizingParseErrorOwned::InvalidValue((*s).to_string().into())
                }
            }
        }
    }

    impl LayoutBoxSizingParseErrorOwned {
        #[must_use] pub fn to_shared(&self) -> LayoutBoxSizingParseError<'_> {
            match self {
                Self::InvalidValue(s) => {
                    LayoutBoxSizingParseError::InvalidValue(s)
                }
            }
        }
    }

    pub fn parse_layout_box_sizing(
        input: &str,
    ) -> Result<LayoutBoxSizing, LayoutBoxSizingParseError<'_>> {
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
