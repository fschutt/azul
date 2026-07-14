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
#[allow(variant_size_differences)] // repr(C,u8) FFI enum: boxing the large variant would change the C ABI (api.json bindings); size disparity accepted
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
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
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

    #[allow(clippy::wildcard_imports)] // parser submodule reuses the parent module's value types
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

            /// # Errors
            ///
            /// Returns an error if `input` is not a valid CSS value for this property.
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

            /// # Errors
            ///
            /// Returns an error if `input` is not a valid CSS value for this property.
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

    /// # Errors
    ///
    /// Returns an error if `input` is not a valid CSS `box-sizing` value.
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

#[cfg(all(test, feature = "parser"))]
mod autotest_generated {
    #[allow(clippy::wildcard_imports)]
    use super::*;
    use alloc::{
        format,
        string::{String, ToString},
        vec,
        vec::Vec,
    };

    /// Maps a `CalcAstItem` to a discriminant tag, so tests can compare the *shape*
    /// of two ASTs without depending on `FloatValue`'s 1/1000 quantisation.
    fn tag(item: &CalcAstItem) -> u8 {
        match item {
            CalcAstItem::Value(_) => 0,
            CalcAstItem::Add => 1,
            CalcAstItem::Sub => 2,
            CalcAstItem::Mul => 3,
            CalcAstItem::Div => 4,
            CalcAstItem::BraceOpen => 5,
            CalcAstItem::BraceClose => 6,
        }
    }

    fn shape(items: &CalcAstItemVec) -> Vec<u8> {
        items.iter().map(tag).collect()
    }

    fn calc_items(w: &LayoutWidth) -> Vec<CalcAstItem> {
        match w {
            LayoutWidth::Calc(items) => items.as_slice().to_vec(),
            other => panic!("expected LayoutWidth::Calc, got {other:?}"),
        }
    }

    fn shape_of_width(w: &LayoutWidth) -> Vec<u8> {
        calc_items(w).iter().map(tag).collect()
    }

    // ---------------------------------------------------------------------
    // parse_calc_expression — malformed / boundary / unicode
    // ---------------------------------------------------------------------

    #[test]
    fn calc_empty_and_whitespace_only_input_is_err() {
        assert!(parse_calc_expression("").is_err());
        assert!(parse_calc_expression("   ").is_err());
        assert!(parse_calc_expression("\t\n\r ").is_err());
    }

    #[test]
    fn calc_garbage_input_is_err_never_panics() {
        for garbage in [
            "???", "@@@", "px", "em", "%", "#", "1px;", "abc", "!!!", "\0", "\u{7f}", ",", ";",
            "1,2", "10 px 20 %%", "--", "-", "-.", "1..px", "1.2.3px",
        ] {
            assert!(
                parse_calc_expression(garbage).is_err(),
                "expected Err for {garbage:?}"
            );
        }
    }

    #[test]
    fn calc_valid_minimal_matches_documented_ast() {
        // Positive control, straight out of the doc comment on `parse_calc_expression`.
        let parsed = parse_calc_expression("100% - 20px").unwrap();
        let expected = vec![
            CalcAstItem::Value(PixelValue::percent(100.0)),
            CalcAstItem::Sub,
            CalcAstItem::Value(PixelValue::px(20.0)),
        ];
        assert_eq!(parsed.as_slice(), expected.as_slice());
    }

    #[test]
    fn calc_documented_nested_example_parses_exactly() {
        let parsed = parse_calc_expression("(100% - 20px) / 3").unwrap();
        let expected = vec![
            CalcAstItem::BraceOpen,
            CalcAstItem::Value(PixelValue::percent(100.0)),
            CalcAstItem::Sub,
            CalcAstItem::Value(PixelValue::px(20.0)),
            CalcAstItem::BraceClose,
            CalcAstItem::Div,
            // A bare `3` is a unit-less number and becomes `px`.
            CalcAstItem::Value(PixelValue::px(3.0)),
        ];
        assert_eq!(parsed.as_slice(), expected.as_slice());
    }

    #[test]
    fn calc_minus_disambiguates_between_sub_and_negative_literal() {
        // Leading `-` is part of the literal.
        assert_eq!(
            parse_calc_expression("-10px").unwrap().as_slice(),
            [CalcAstItem::Value(PixelValue::px(-10.0))].as_slice()
        );
        // `-` after an operator is part of the literal.
        assert_eq!(
            parse_calc_expression("100% * -2").unwrap().as_slice(),
            [
                CalcAstItem::Value(PixelValue::percent(100.0)),
                CalcAstItem::Mul,
                CalcAstItem::Value(PixelValue::px(-2.0)),
            ]
            .as_slice()
        );
        // `-` after `(` is part of the literal.
        assert_eq!(
            parse_calc_expression("(-5px)").unwrap().as_slice(),
            [
                CalcAstItem::BraceOpen,
                CalcAstItem::Value(PixelValue::px(-5.0)),
                CalcAstItem::BraceClose,
            ]
            .as_slice()
        );
        // `-` after a value is subtraction — even when written as `5px -10px`, so a
        // whitespace-separated negative literal silently becomes a subtraction.
        assert_eq!(
            parse_calc_expression("5px -10px").unwrap().as_slice(),
            [
                CalcAstItem::Value(PixelValue::px(5.0)),
                CalcAstItem::Sub,
                CalcAstItem::Value(PixelValue::px(10.0)),
            ]
            .as_slice()
        );
        // `-` after `)` is subtraction.
        assert_eq!(
            shape(&parse_calc_expression("(1px) - 2px").unwrap()),
            vec![5, 0, 6, 2, 0]
        );
    }

    #[test]
    fn calc_leading_minus_followed_by_space_is_rejected() {
        // `- 10px` at the start is treated as a negative literal `-`, which fails to parse.
        assert!(parse_calc_expression("- 10px").is_err());
        assert!(parse_calc_expression("(- 10px)").is_err());
    }

    #[test]
    fn calc_unicode_input_is_rejected_without_panic() {
        // Every one of these puts a multi-byte char where the tokeniser slices `&input[i..]`;
        // if `find_value_end` ever returned a non-char-boundary offset this would panic.
        for input in [
            "\u{1F600}",              // emoji
            "100px\u{1F600}",         // emoji after a valid token
            "10px\u{0301}",           // combining acute accent
            "10px\u{00A0}- 5px",      // non-breaking space is NOT ascii whitespace
            "\u{FF11}\u{FF10}px",     // full-width digits
            "１００%",                // full-width digits + ascii percent
            "π",
            "10\u{2212}5",            // U+2212 MINUS SIGN, not ASCII '-'
            "\u{202E}10px",           // RTL override
            "e\u{0301}m",
        ] {
            assert!(
                parse_calc_expression(input).is_err(),
                "expected Err for {input:?}"
            );
        }
    }

    #[test]
    fn calc_nan_literal_is_accepted_but_coerced_to_zero() {
        // ADVERSARIAL: `parse_pixel_value` delegates to `f32::from_str`, which happily
        // parses "NaN". The value survives into the AST — but `FloatValue::new` casts
        // `NaN * 1000.0` to isize, and `as isize` maps NaN to 0. So no NaN ever reaches
        // the layout solver; the expression silently means `calc(0px)` instead of failing.
        let parsed = parse_calc_expression("NaN").unwrap();
        match parsed.get(0).unwrap() {
            CalcAstItem::Value(v) => {
                assert!(!v.number.get().is_nan(), "NaN must not survive into the AST");
                assert_eq!(v.number.get(), 0.0);
            }
            other => panic!("expected a Value, got {other:?}"),
        }
    }

    #[test]
    fn calc_huge_and_infinite_literals_saturate_to_a_finite_value() {
        // "inf" and out-of-range literals parse to f32::INFINITY, which `FloatValue::new`
        // saturates to isize::MAX. Assert the AST never carries a non-finite number.
        let huge = "9".repeat(50); // ~1e50, far past f32::MAX
        for input in [
            "inf",
            "-inf",
            huge.as_str(),
            "9223372036854775807", // i64::MAX
            "-9223372036854775808",
            "340282350000000000000000000000000000000px", // f32::MAX
        ] {
            let parsed = parse_calc_expression(input)
                .unwrap_or_else(|()| panic!("expected Ok for {input:?}"));
            match parsed.get(0).unwrap() {
                CalcAstItem::Value(v) => {
                    let n = v.number.get();
                    assert!(n.is_finite(), "{input:?} produced a non-finite value: {n}");
                }
                other => panic!("expected a Value for {input:?}, got {other:?}"),
            }
        }
    }

    #[test]
    fn calc_zero_and_negative_zero() {
        for input in ["0", "-0", "0px", "-0px", "0%"] {
            let parsed = parse_calc_expression(input).unwrap();
            match parsed.get(0).unwrap() {
                CalcAstItem::Value(v) => assert_eq!(
                    v.number.get(),
                    0.0,
                    "{input:?} should quantise to exactly zero"
                ),
                other => panic!("expected a Value for {input:?}, got {other:?}"),
            }
        }
        // -0.0 is normalised to +0.0 by the isize round-trip, so it never prints as "-0".
        assert_eq!(
            calc_ast_to_css_string(&parse_calc_expression("-0px").unwrap()),
            "calc(0px)"
        );
    }

    #[test]
    fn calc_sub_millisecond_precision_is_quantised_to_zero() {
        // FloatValue keeps 3 decimal places; anything below 0.001 collapses to 0.
        let parsed = parse_calc_expression("0.0005px").unwrap();
        match parsed.get(0).unwrap() {
            CalcAstItem::Value(v) => assert_eq!(v.number.get(), 0.0),
            other => panic!("expected a Value, got {other:?}"),
        }
        let parsed = parse_calc_expression("0.001px").unwrap();
        match parsed.get(0).unwrap() {
            CalcAstItem::Value(v) => assert!((v.number.get() - 0.001).abs() < 1e-6),
            other => panic!("expected a Value, got {other:?}"),
        }
    }

    #[test]
    fn calc_deeply_nested_braces_do_not_stack_overflow() {
        // The tokeniser is iterative, so 10_000 levels of nesting must not blow the stack.
        const DEPTH: usize = 10_000;
        let input = format!("{}1px{}", "(".repeat(DEPTH), ")".repeat(DEPTH));
        let parsed = parse_calc_expression(&input).unwrap();
        assert_eq!(parsed.len(), DEPTH * 2 + 1);
        assert_eq!(*parsed.get(0).unwrap(), CalcAstItem::BraceOpen);
        assert_eq!(
            *parsed.get(parsed.len() - 1).unwrap(),
            CalcAstItem::BraceClose
        );
        // Printing the same AST must also stay iterative.
        let printed = calc_ast_to_css_string(&parsed);
        assert_eq!(printed.matches('(').count(), DEPTH + 1); // + the "calc(" paren
        assert_eq!(printed.matches(')').count(), DEPTH + 1);
    }

    #[test]
    fn calc_unbalanced_braces_are_accepted_without_validation() {
        // ADVERSARIAL / documents current behaviour: the tokeniser performs NO grammar
        // validation, so structurally meaningless expressions parse to Ok(..). Anything
        // that consumes a CalcAstItemVec (the solver in layout/src/solver3/calc.rs) must
        // therefore be robust against unbalanced braces and dangling operators.
        assert_eq!(
            shape(&parse_calc_expression("(((").unwrap()),
            vec![5, 5, 5]
        );
        assert_eq!(shape(&parse_calc_expression(")))").unwrap()), vec![6, 6, 6]);
        assert_eq!(shape(&parse_calc_expression(")1px(").unwrap()), vec![6, 0, 5]);

        // The same holds through the public parser: `width: calc(()` is accepted.
        assert_eq!(shape_of_width(&parse_layout_width("calc(()").unwrap()), vec![5]);
        assert_eq!(
            shape_of_width(&parse_layout_width("calc()))").unwrap()),
            vec![6, 6]
        );
    }

    #[test]
    fn calc_dangling_operators_and_missing_operands_are_accepted() {
        // Same story as the braces: operators with no operands still yield Ok(..).
        assert_eq!(shape(&parse_calc_expression("+").unwrap()), vec![1]);
        assert_eq!(shape(&parse_calc_expression("*/").unwrap()), vec![3, 4]);
        assert_eq!(shape(&parse_calc_expression("1px 2px").unwrap()), vec![0, 0]);
        assert_eq!(
            shape(&parse_calc_expression("1px + + 2px").unwrap()),
            vec![0, 1, 1, 0]
        );
    }

    #[test]
    fn calc_extremely_long_expression_terminates() {
        // 50_000 terms — the tokeniser is O(n), so this must not hang.
        const TERMS: usize = 50_000;
        let mut input = String::from("1px");
        for _ in 0..TERMS {
            input.push_str(" + 1px");
        }
        let parsed = parse_calc_expression(&input).unwrap();
        assert_eq!(parsed.len(), TERMS * 2 + 1);
    }

    #[test]
    fn calc_extremely_long_garbage_token_is_err() {
        let long_alpha = "a".repeat(100_000);
        assert!(parse_calc_expression(&long_alpha).is_err());

        // A 100k-digit literal overflows f32 to +inf, which then saturates to a finite
        // FloatValue — it must not hang, panic, or produce inf.
        let long_digits = "1".repeat(100_000);
        let parsed = parse_calc_expression(&long_digits).unwrap();
        match parsed.get(0).unwrap() {
            CalcAstItem::Value(v) => assert!(v.number.get().is_finite()),
            other => panic!("expected a Value, got {other:?}"),
        }
    }

    #[test]
    fn calc_leading_and_trailing_junk_is_handled_deterministically() {
        // Surrounding whitespace is trimmed...
        assert_eq!(
            parse_calc_expression("  100% - 20px  ").unwrap().as_slice(),
            parse_calc_expression("100% - 20px").unwrap().as_slice()
        );
        // ...but real trailing junk is rejected.
        assert!(parse_calc_expression("100% - 20px;").is_err());
        assert!(parse_calc_expression("100% - 20px garbage").is_err());
        assert!(parse_calc_expression(";100% - 20px").is_err());
    }

    #[test]
    fn calc_scientific_notation_is_rejected() {
        // `find_value_end` stops the digit scan at 'e' and then eats it as a unit, so the
        // token handed to parse_pixel_value is "1e" — CSS `calc(1e3px)` is not supported.
        assert!(parse_calc_expression("1e3px").is_err());
        assert!(parse_calc_expression("1e40").is_err());
        assert!(parse_calc_expression("1E3px").is_err());
    }

    #[test]
    fn calc_every_single_ascii_char_is_panic_free() {
        for b in 0u8..128 {
            let s = String::from(b as char);
            // Only requirement: no panic, no hang. (Operators/digits are Ok, the rest Err.)
            let _ = parse_calc_expression(&s);
        }
    }

    #[test]
    fn calc_fuzz_triples_never_panic_and_reprint_keeps_the_shape() {
        // Deterministic mini-fuzz over the tokeniser's decision points, including two
        // multi-byte chars to smoke out any non-char-boundary slicing.
        const ALPHABET: [&str; 16] = [
            "(", ")", "+", "-", "*", "/", ".", "0", "9", "p", "x", "%", " ", "e", "é", "\u{1F600}",
        ];

        for a in ALPHABET {
            for b in ALPHABET {
                for c in ALPHABET {
                    let input = format!("{a}{b}{c}");
                    let Ok(ast) = parse_calc_expression(&input) else {
                        continue;
                    };
                    assert!(!ast.is_empty(), "Ok(..) must never be an empty AST");

                    // encode == decode: printing an AST and re-parsing it must give back
                    // the same sequence of item kinds.
                    let printed = calc_ast_to_css_string(&ast);
                    assert!(printed.starts_with("calc(") && printed.ends_with(')'));
                    let inner = &printed[5..printed.len() - 1];
                    let reparsed = parse_calc_expression(inner).unwrap_or_else(|()| {
                        panic!("re-printed AST {printed:?} (from {input:?}) failed to re-parse")
                    });
                    assert_eq!(
                        shape(&ast),
                        shape(&reparsed),
                        "round-trip changed the AST shape: {input:?} -> {printed:?}"
                    );
                }
            }
        }
    }

    // ---------------------------------------------------------------------
    // find_value_end
    // ---------------------------------------------------------------------

    #[test]
    fn find_value_end_basic_offsets() {
        assert_eq!(find_value_end(""), 0);
        assert_eq!(find_value_end("10px"), 4);
        assert_eq!(find_value_end("100%"), 4);
        assert_eq!(find_value_end("-1.5em"), 6);
        assert_eq!(find_value_end("+2px"), 4);
        assert_eq!(find_value_end("3"), 1);
        // Stops at the first char that is neither sign/digit/dot nor unit.
        assert_eq!(find_value_end("10px)"), 4);
        assert_eq!(find_value_end("10px + 2px"), 4);
        assert_eq!(find_value_end("(1px)"), 0);
        assert_eq!(find_value_end(")"), 0);
        // A lone sign consumes exactly the sign, so the caller gets the un-parsable "-".
        assert_eq!(find_value_end("-"), 1);
        assert_eq!(find_value_end("- 10px"), 1);
    }

    #[test]
    fn find_value_end_is_lax_and_hands_junk_to_the_pixel_parser() {
        // ADVERSARIAL: find_value_end is a *scanner*, not a validator — it happily returns
        // a non-empty span for these. The rejection only happens later, in parse_pixel_value.
        assert_eq!(find_value_end("..."), 3);
        assert_eq!(find_value_end("1.2.3px"), 7);
        assert_eq!(find_value_end("1px%em"), 6);
        assert_eq!(find_value_end("--"), 1);
        // ...which is why all of these end up as Err from the calc parser:
        for junk in ["...", "1.2.3px", "1px%em"] {
            assert!(parse_calc_expression(junk).is_err(), "{junk:?}");
        }
    }

    #[test]
    fn find_value_end_stops_at_an_exponent_marker() {
        // 'e' is treated as the start of a unit, so the digit scan never sees "e40".
        assert_eq!(find_value_end("1e40"), 2);
        assert_eq!(find_value_end("1e40px"), 2);
    }

    #[test]
    fn find_value_end_result_is_always_an_in_bounds_char_boundary() {
        // THE safety invariant: parse_calc_expression slices `&rest[..end]` with this
        // offset, so a non-boundary result would be an instant panic on any unicode input.
        for s in [
            "",
            " ",
            "10px",
            "\u{1F600}",
            "10px\u{1F600}",
            "1\u{0301}px",
            "é",
            "9é",
            "%é",
            "9%é",
            "１０px",
            "\u{00A0}10px",
            "10\u{2212}5",
            "px\u{4e2d}\u{6587}",
        ] {
            let end = find_value_end(s);
            assert!(end <= s.len(), "{s:?}: end {end} out of bounds");
            assert!(
                s.is_char_boundary(end),
                "{s:?}: end {end} is not a char boundary"
            );
            // Slicing with the returned offset must be safe.
            let _ = &s[..end];
        }
    }

    #[test]
    fn find_value_end_long_input_terminates() {
        let long = "9".repeat(200_000);
        assert_eq!(find_value_end(&long), 200_000);
        let long_unit = format!("{}{}", "9".repeat(100_000), "x".repeat(100_000));
        assert_eq!(find_value_end(&long_unit), 200_000);
    }

    // ---------------------------------------------------------------------
    // calc_ast_to_css_string
    // ---------------------------------------------------------------------

    #[test]
    fn calc_ast_to_css_string_of_empty_vec_is_empty_calc() {
        assert_eq!(calc_ast_to_css_string(&CalcAstItemVec::new()), "calc()");
        // ...and that output is not itself re-parsable, i.e. an empty AST is not a
        // legal calc() — parse_layout_width rejects it.
        assert!(parse_layout_width("calc()").is_err());
    }

    #[test]
    fn calc_ast_to_css_string_prints_every_variant() {
        let items = CalcAstItemVec::from_vec(vec![
            CalcAstItem::Value(PixelValue::px(1.0)),
            CalcAstItem::Add,
            CalcAstItem::Sub,
            CalcAstItem::Mul,
            CalcAstItem::Div,
            CalcAstItem::BraceOpen,
            CalcAstItem::BraceClose,
        ]);
        assert_eq!(calc_ast_to_css_string(&items), "calc(1px + - * / ( ))");
    }

    #[test]
    fn calc_ast_to_css_string_never_prints_nan_or_inf() {
        // Even if a caller builds a PixelValue from NaN/inf/f32::MAX (e.g. over FFI),
        // the printed CSS must stay a parsable finite number.
        for v in [
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::MAX,
            f32::MIN,
            f32::MIN_POSITIVE,
        ] {
            let items = CalcAstItemVec::from_vec(vec![CalcAstItem::Value(PixelValue::px(v))]);
            let printed = calc_ast_to_css_string(&items);
            assert!(!printed.contains("NaN"), "{v} printed as {printed:?}");
            assert!(!printed.contains("inf"), "{v} printed as {printed:?}");
            assert!(printed.starts_with("calc(") && printed.ends_with("px)"));
            // and it must survive a re-parse
            let inner = &printed[5..printed.len() - 1];
            assert!(
                parse_calc_expression(inner).is_ok(),
                "{printed:?} did not re-parse"
            );
        }
        // NaN specifically collapses to 0.
        let items = CalcAstItemVec::from_vec(vec![CalcAstItem::Value(PixelValue::px(f32::NAN))]);
        assert_eq!(calc_ast_to_css_string(&items), "calc(0px)");
    }

    #[test]
    fn calc_ast_print_parse_roundtrip_is_exact_for_representable_values() {
        for src in [
            "100% - 20px",
            "(100% - 20px) / 3",
            "-10px + 5px",
            "10px - -5px",
            "1.5em * 2",
            "100vw - 2rem",
            "50% + 1.25in",
            "((1px + 2px) * (3px - 4px))",
        ] {
            let ast = parse_calc_expression(src).unwrap();
            let printed = calc_ast_to_css_string(&ast);
            let inner = &printed[5..printed.len() - 1];
            let reparsed = parse_calc_expression(inner).unwrap();
            assert_eq!(
                ast.as_slice(),
                reparsed.as_slice(),
                "round-trip mismatch for {src:?} (printed as {printed:?})"
            );
        }
    }

    #[test]
    fn calc_ast_to_css_string_is_lossy_for_adjacent_values() {
        // ADVERSARIAL: the printer joins items with a single space and adds no
        // disambiguating parens, so an AST holding two adjacent Values — reachable via the
        // FFI/api constructors, though not via parse_calc_expression — prints to CSS that
        // re-parses as a *subtraction*. The encoding is not injective.
        let items = CalcAstItemVec::from_vec(vec![
            CalcAstItem::Value(PixelValue::px(1.0)),
            CalcAstItem::Value(PixelValue::px(-1.0)),
        ]);
        let printed = calc_ast_to_css_string(&items);
        assert_eq!(printed, "calc(1px -1px)");

        let reparsed = parse_calc_expression(&printed[5..printed.len() - 1]).unwrap();
        assert_eq!(shape(&items), vec![0, 0]);
        assert_eq!(shape(&reparsed), vec![0, 2, 0]); // Value, Sub, Value — not the input!
        assert_ne!(items.as_slice(), reparsed.as_slice());
    }

    #[test]
    fn calc_ast_to_css_string_handles_a_huge_ast() {
        let items = CalcAstItemVec::from_vec(vec![CalcAstItem::BraceOpen; 100_000]);
        let printed = calc_ast_to_css_string(&items);
        // 100_000 "(" joined by 100_000 - 1 spaces, plus "calc(" and ")"
        assert_eq!(printed.len(), 100_000 * 2 - 1 + 6);
    }

    // ---------------------------------------------------------------------
    // parse_layout_box_sizing + LayoutBoxSizingParseError round-trips
    // ---------------------------------------------------------------------

    #[test]
    fn box_sizing_valid_inputs_and_trimming() {
        assert_eq!(
            parse_layout_box_sizing("content-box").unwrap(),
            LayoutBoxSizing::ContentBox
        );
        assert_eq!(
            parse_layout_box_sizing("border-box").unwrap(),
            LayoutBoxSizing::BorderBox
        );
        assert_eq!(
            parse_layout_box_sizing("\t\n  border-box \r\n ").unwrap(),
            LayoutBoxSizing::BorderBox
        );
        // encode == decode
        for v in [LayoutBoxSizing::ContentBox, LayoutBoxSizing::BorderBox] {
            assert_eq!(parse_layout_box_sizing(&v.print_as_css_value()).unwrap(), v);
        }
    }

    #[test]
    fn box_sizing_empty_whitespace_and_garbage_are_err() {
        for input in [
            "",
            "   ",
            "\t\n",
            "padding-box",
            "borderbox",
            "border box",
            "content-box;",
            "content-box border-box",
            "content-box!",
            "\0",
            "-",
            "\u{1F600}",
            "content-box\u{0301}",
            "cöntent-box",
            "content\u{2010}box", // unicode hyphen, not ASCII '-'
        ] {
            assert!(
                parse_layout_box_sizing(input).is_err(),
                "expected Err for {input:?}"
            );
        }
    }

    #[test]
    fn box_sizing_rejects_numeric_boundary_strings() {
        for input in [
            "0",
            "-0",
            "NaN",
            "inf",
            "9223372036854775807",
            "-9223372036854775808",
            "3.4028235e38",
            "1e-45",
        ] {
            assert!(
                parse_layout_box_sizing(input).is_err(),
                "expected Err for {input:?}"
            );
        }
    }

    #[test]
    fn box_sizing_keyword_matching_is_case_sensitive() {
        // CSS keywords are ASCII case-insensitive per spec; this parser is not.
        // Documenting the current behaviour so a future fix has to update this test.
        assert!(parse_layout_box_sizing("Content-Box").is_err());
        assert!(parse_layout_box_sizing("BORDER-BOX").is_err());
    }

    #[test]
    fn box_sizing_extremely_long_input_is_err_and_terminates() {
        let long = "a".repeat(1_000_000);
        assert!(parse_layout_box_sizing(&long).is_err());

        // A long *valid* keyword surrounded by whitespace still trims down to Ok.
        let padded = format!("{}border-box{}", " ".repeat(100_000), " ".repeat(100_000));
        assert_eq!(
            parse_layout_box_sizing(&padded).unwrap(),
            LayoutBoxSizing::BorderBox
        );
    }

    #[test]
    fn box_sizing_error_payload_is_the_trimmed_input() {
        let err = parse_layout_box_sizing("  bogus  ").unwrap_err();
        match &err {
            LayoutBoxSizingParseError::InvalidValue(s) => assert_eq!(*s, "bogus"),
        }
        assert_eq!(format!("{err}"), "Invalid box-sizing value: \"bogus\"");
    }

    #[test]
    fn box_sizing_error_to_contained_to_shared_roundtrip() {
        let err = parse_layout_box_sizing("padding-box").unwrap_err();
        let owned = err.to_contained();
        assert_eq!(
            owned,
            LayoutBoxSizingParseErrorOwned::InvalidValue("padding-box".to_string().into())
        );
        // to_shared() must reproduce exactly what to_contained() consumed.
        assert_eq!(owned.to_shared(), err);
        // ...and be idempotent under repeated round-trips.
        assert_eq!(owned.to_shared().to_contained(), owned);
    }

    #[test]
    fn box_sizing_error_roundtrip_with_empty_and_unicode_payloads() {
        for input in ["", "   ", "\u{1F600}\u{0301}é", "a\0b"] {
            let err = parse_layout_box_sizing(input).unwrap_err();
            let owned = err.to_contained();
            assert_eq!(owned.to_shared(), err, "round-trip failed for {input:?}");

            match &owned {
                LayoutBoxSizingParseErrorOwned::InvalidValue(s) => {
                    assert_eq!(s.as_str(), input.trim());
                }
            }
        }
    }

    #[test]
    fn box_sizing_error_roundtrip_with_a_huge_payload() {
        let long = "x".repeat(200_000);
        let err = parse_layout_box_sizing(&long).unwrap_err();
        let owned = err.to_contained();
        match &owned {
            LayoutBoxSizingParseErrorOwned::InvalidValue(s) => {
                assert_eq!(s.as_str().len(), 200_000);
            }
        }
        assert_eq!(owned.to_shared(), err);
    }

    // ---------------------------------------------------------------------
    // The sizing parsers — the only public path into the calc()/fit-content() slicing
    // ---------------------------------------------------------------------

    #[test]
    fn sizing_parser_paren_slicing_is_panic_free() {
        // Both branches slice with hard-coded byte offsets (`s[5..len-1]`, `s[12..len-1]`).
        // These inputs are the ones that would trip an off-by-one or a char-boundary bug.
        for input in [
            "calc()",
            "fit-content()",
            "fit-content(\u{1F600})",
            "calc(\u{1F600})",
            "calc(é)",
            "fit-content(é)",
            "calc( )",
            "fit-content( )",
            "calc(1px)garbage)",
            "fit-content(1px)garbage)",
            "fit-content(1px",
            "calc(1px",
        ] {
            // Only requirement: no panic. (All of these must also be Err.)
            assert!(
                parse_layout_width(input).is_err(),
                "expected Err for {input:?}"
            );
            assert!(
                parse_layout_height(input).is_err(),
                "expected Err for {input:?}"
            );
        }
    }

    #[test]
    fn sizing_parser_keywords_and_calc_roundtrip() {
        let cases = [
            (LayoutWidth::Auto, "auto"),
            (LayoutWidth::MinContent, "min-content"),
            (LayoutWidth::MaxContent, "max-content"),
            (LayoutWidth::Px(PixelValue::px(150.0)), "150px"),
            (LayoutWidth::FitContent(PixelValue::percent(50.0)), "fit-content(50%)"),
        ];
        for (value, css) in cases {
            assert_eq!(parse_layout_width(css).unwrap(), value, "parse of {css:?}");
            assert_eq!(value.print_as_css_value(), css, "print of {css:?}");
        }

        // calc() survives the full encode/decode cycle.
        let parsed = parse_layout_width("calc(100% - 20px)").unwrap();
        assert_eq!(parsed.print_as_css_value(), "calc(100% - 20px)");
        assert_eq!(parse_layout_width(&parsed.print_as_css_value()).unwrap(), parsed);
        assert_eq!(
            calc_items(&parsed),
            vec![
                CalcAstItem::Value(PixelValue::percent(100.0)),
                CalcAstItem::Sub,
                CalcAstItem::Value(PixelValue::px(20.0)),
            ]
        );
    }

    #[test]
    fn sizing_parser_keywords_are_case_sensitive() {
        // Same spec deviation as box-sizing: CSS keywords should be case-insensitive.
        for input in ["AUTO", "Auto", "MIN-CONTENT", "CALC(1px)", "FIT-CONTENT(1px)"] {
            assert!(
                parse_layout_width(input).is_err(),
                "expected Err for {input:?}"
            );
        }
    }

    #[test]
    fn fit_content_clamps_negative_values_to_zero() {
        // Documented invariant of the parser: fit-content() can never be negative.
        assert_eq!(
            parse_layout_width("fit-content(-10px)").unwrap(),
            LayoutWidth::FitContent(PixelValue::zero())
        );
        assert_eq!(
            parse_layout_height("fit-content(-99999%)").unwrap(),
            LayoutHeight::FitContent(PixelValue::zero())
        );
        // NaN quantises to 0, which is >= 0.0, so it takes the non-negative branch.
        assert_eq!(
            parse_layout_width("fit-content(NaN)").unwrap(),
            LayoutWidth::FitContent(PixelValue::zero())
        );
        // A huge value is kept, but saturated to a finite number.
        match parse_layout_width("fit-content(99999999999999999999999999999999999999999px)")
            .unwrap()
        {
            LayoutWidth::FitContent(v) => assert!(v.number.get().is_finite()),
            other => panic!("expected FitContent, got {other:?}"),
        }
    }

    #[test]
    fn sizing_parser_deeply_nested_calc_does_not_stack_overflow() {
        const DEPTH: usize = 5_000;
        let input = format!(
            "calc({}1px{})",
            "(".repeat(DEPTH),
            ")".repeat(DEPTH)
        );
        let parsed = parse_layout_width(&input).unwrap();
        assert_eq!(calc_items(&parsed).len(), DEPTH * 2 + 1);
        // Printing must survive it too (this is what the CSS serialiser calls).
        let printed = parsed.print_as_css_value();
        assert_eq!(printed.matches('(').count(), DEPTH + 1);
        assert_eq!(printed.matches(')').count(), DEPTH + 1);
    }

    #[test]
    fn sizing_parser_error_carries_the_untrimmed_input_for_bad_calc() {
        // Quirk worth pinning: InvalidKeyword gets the *raw* `input`, not the trimmed
        // string (unlike box-sizing, which reports the trimmed value).
        let err = parse_layout_width("  calc(??)  ").unwrap_err();
        match &err {
            LayoutWidthParseError::InvalidKeyword(k) => assert_eq!(*k, "  calc(??)  "),
            other => panic!("expected InvalidKeyword, got {other:?}"),
        }
        // ...and it round-trips through the owned representation unchanged.
        let owned = err.to_contained();
        assert_eq!(owned.to_shared(), err);
    }

    #[test]
    fn pixel_dimension_parser_errors_roundtrip() {
        for input in ["", "   ", "px", "garbage", "\u{1F600}", "1.2.3px"] {
            let err = parse_layout_min_width(input)
                .err()
                .unwrap_or_else(|| panic!("expected Err for {input:?}"));
            let owned = err.to_contained();
            assert_eq!(owned.to_shared(), err, "for {input:?}");

            assert!(parse_layout_max_height(input).is_err(), "for {input:?}");
        }
        assert_eq!(
            parse_layout_min_width("0").unwrap(),
            LayoutMinWidth {
                inner: PixelValue::px(0.0)
            }
        );
    }

    // ---------------------------------------------------------------------
    // Defaults / numeric invariants
    // ---------------------------------------------------------------------

    #[test]
    fn max_dimension_defaults_are_finite_but_not_actually_f32_max() {
        // The doc comment says the default is `f32::MAX` pixels. It isn't: FloatValue
        // stores number * 1000 in an isize, and `f32::MAX * 1000.0` = inf saturates to
        // isize::MAX — so `get()` comes back as ~9.2e15, not 3.4e38. The sentinel is still
        // "effectively unconstrained" and, importantly, finite (so the solver's
        // padding/margin additions cannot reach inf) — but it is NOT f32::MAX.
        for got in [
            LayoutMaxWidth::default().inner.number.get(),
            LayoutMaxHeight::default().inner.number.get(),
        ] {
            assert!(got.is_finite(), "default max dimension is not finite: {got}");
            assert!(got > 0.0);
            assert_ne!(got, f32::MAX);
        }
        // The min-* defaults are exactly zero.
        assert_eq!(LayoutMinWidth::default().inner.number.get(), 0.0);
        assert_eq!(LayoutMinHeight::default().inner.number.get(), 0.0);
        assert_eq!(LayoutWidth::default(), LayoutWidth::Auto);
        assert_eq!(LayoutHeight::default(), LayoutHeight::Auto);
        assert_eq!(LayoutBoxSizing::default(), LayoutBoxSizing::ContentBox);
    }

    #[test]
    fn sizing_interpolate_endpoints_and_nan_t() {
        let a = LayoutWidth::px(10.0);
        let b = LayoutWidth::px(20.0);
        assert_eq!(a.interpolate(&b, 0.0), a);
        assert_eq!(a.interpolate(&b, 1.0), b);
        assert_eq!(a.interpolate(&b, 0.5), LayoutWidth::px(15.0));

        // NaN `t` must not panic and must not leak a NaN into the value.
        match a.interpolate(&b, f32::NAN) {
            LayoutWidth::Px(v) => {
                assert!(!v.number.get().is_nan());
                assert_eq!(v.number.get(), 0.0);
            }
            other => panic!("expected Px, got {other:?}"),
        }

        // Discrete keywords snap rather than blend, and NaN falls through to `other`.
        let auto = LayoutWidth::Auto;
        let min = LayoutWidth::MinContent;
        assert_eq!(auto.interpolate(&min, 0.0), LayoutWidth::Auto);
        assert_eq!(auto.interpolate(&min, 1.0), LayoutWidth::MinContent);
        assert_eq!(auto.interpolate(&min, f32::NAN), LayoutWidth::MinContent);

        // Interpolating a calc() clones the AST (no double-free, no panic).
        let calc = parse_layout_width("calc(100% - 20px)").unwrap();
        assert_eq!(calc.interpolate(&auto, 0.0), calc);
        assert_eq!(auto.interpolate(&calc, 1.0), calc);
    }

    #[test]
    fn sizing_parser_px_quantisation_limits() {
        // Sub-0.001 collapses to zero...
        match parse_layout_width("0.0005px").unwrap() {
            LayoutWidth::Px(v) => assert_eq!(v.number.get(), 0.0),
            other => panic!("expected Px, got {other:?}"),
        }
        // ...and an out-of-f32-range literal saturates to a finite value rather than inf.
        match parse_layout_width(&format!("{}px", "9".repeat(60))).unwrap() {
            LayoutWidth::Px(v) => assert!(v.number.get().is_finite()),
            other => panic!("expected Px, got {other:?}"),
        }
        // A bare NaN literal is accepted as a length and quantises to 0.
        match parse_layout_width("NaN").unwrap() {
            LayoutWidth::Px(v) => assert_eq!(v.number.get(), 0.0),
            other => panic!("expected Px, got {other:?}"),
        }
    }
}
