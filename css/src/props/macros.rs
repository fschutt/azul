//! Internal macros for reducing boilerplate in property definitions.

/// Creates `pt`, `px` and `em` constructors for any struct that has a
/// `PixelValue` as it's self.0 field.
macro_rules! impl_pixel_value {
    ($struct:ident) => {
        impl $struct {
            #[inline]
            pub const fn zero() -> Self {
                Self {
                    inner: crate::props::basic::pixel::PixelValue::zero(),
                }
            }

            #[inline]
            pub const fn const_px(value: isize) -> Self {
                Self {
                    inner: crate::props::basic::pixel::PixelValue::const_px(value),
                }
            }

            #[inline]
            pub const fn const_em(value: isize) -> Self {
                Self {
                    inner: crate::props::basic::pixel::PixelValue::const_em(value),
                }
            }

            #[inline]
            pub const fn const_pt(value: isize) -> Self {
                Self {
                    inner: crate::props::basic::pixel::PixelValue::const_pt(value),
                }
            }

            #[inline]
            pub const fn const_percent(value: isize) -> Self {
                Self {
                    inner: crate::props::basic::pixel::PixelValue::const_percent(value),
                }
            }

            #[inline]
            pub const fn const_in(value: isize) -> Self {
                Self {
                    inner: crate::props::basic::pixel::PixelValue::const_in(value),
                }
            }

            #[inline]
            pub const fn const_cm(value: isize) -> Self {
                Self {
                    inner: crate::props::basic::pixel::PixelValue::const_cm(value),
                }
            }

            #[inline]
            pub const fn const_mm(value: isize) -> Self {
                Self {
                    inner: crate::props::basic::pixel::PixelValue::const_mm(value),
                }
            }

            #[inline]
            pub const fn const_from_metric(
                metric: crate::props::basic::length::SizeMetric,
                value: isize,
            ) -> Self {
                Self {
                    inner: crate::props::basic::pixel::PixelValue::const_from_metric(metric, value),
                }
            }

            #[inline]
            pub fn px(value: f32) -> Self {
                Self {
                    inner: crate::props::basic::pixel::PixelValue::px(value),
                }
            }

            #[inline]
            pub fn em(value: f32) -> Self {
                Self {
                    inner: crate::props::basic::pixel::PixelValue::em(value),
                }
            }

            #[inline]
            pub fn pt(value: f32) -> Self {
                Self {
                    inner: crate::props::basic::pixel::PixelValue::pt(value),
                }
            }

            #[inline]
            pub fn percent(value: f32) -> Self {
                Self {
                    inner: crate::props::basic::pixel::PixelValue::percent(value),
                }
            }

            #[inline]
            pub fn from_metric(
                metric: crate::props::basic::length::SizeMetric,
                value: f32,
            ) -> Self {
                Self {
                    inner: crate::props::basic::pixel::PixelValue::from_metric(metric, value),
                }
            }

            #[inline]
            pub fn interpolate(&self, other: &Self, t: f32) -> Self {
                $struct {
                    inner: self.inner.interpolate(&other.inner, t),
                }
            }
        }
    };
}

macro_rules! impl_percentage_value {
    ($struct:ident) => {
        impl ::core::fmt::Display for $struct {
            fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                write!(f, "{}%", self.inner.normalized() * 100.0)
            }
        }

        impl ::core::fmt::Debug for $struct {
            fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                write!(f, "{}%", self.inner.normalized() * 100.0)
            }
        }

        impl $struct {
            /// Same as `PercentageValue::new()`, but only accepts whole numbers,
            /// since using `f32` in const fn is not yet stabilized.
            #[inline]
            pub const fn const_new(value: isize) -> Self {
                Self {
                    inner: PercentageValue::const_new(value),
                }
            }

            #[inline]
            pub fn new(value: f32) -> Self {
                Self {
                    inner: PercentageValue::new(value),
                }
            }

            #[inline]
            pub fn interpolate(&self, other: &Self, t: f32) -> Self {
                $struct {
                    inner: self.inner.interpolate(&other.inner, t),
                }
            }
        }
    };
}

macro_rules! impl_float_value {
    ($struct:ident) => {
        impl $struct {
            /// Same as `FloatValue::new()`, but only accepts whole numbers,
            /// since using `f32` in const fn is not yet stabilized.
            pub const fn const_new(value: isize) -> Self {
                Self {
                    inner: FloatValue::const_new(value),
                }
            }

            pub fn new(value: f32) -> Self {
                Self {
                    inner: FloatValue::new(value),
                }
            }

            pub fn get(&self) -> f32 {
                self.inner.get()
            }

            #[inline]
            pub fn interpolate(&self, other: &Self, t: f32) -> Self {
                Self {
                    inner: self.inner.interpolate(&other.inner, t),
                }
            }
        }

        impl From<f32> for $struct {
            fn from(val: f32) -> Self {
                Self {
                    inner: FloatValue::from(val),
                }
            }
        }

        impl ::core::fmt::Display for $struct {
            fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                write!(f, "{}", self.inner.get())
            }
        }

        impl ::core::fmt::Debug for $struct {
            fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                write!(f, "{}", self.inner.get())
            }
        }
    };
}

/// Trait to allow `define_dimension_property!` to work.
pub(crate) trait PixelValueTaker {
    fn from_pixel_value(inner: crate::props::basic::pixel::PixelValue) -> Self;
}

/// A parser that can accept a list of items and mappings
macro_rules! multi_type_parser {
    ($fn:ident, $return_str:expr, $return:ident, $import_str:expr, $([$identifier_string:expr, $enum_type:ident, $parse_str:expr]),+) => {
        #[doc = "Parses a `"]
        #[doc = $return_str]
        #[doc = "` attribute from a `&str`"]
        #[doc = ""]
        #[doc = "# Example"]
        #[doc = ""]
        #[doc = "```rust"]
        #[doc = $import_str]
        $(
            #[doc = $parse_str]
        )+
        #[doc = "```"]
        pub fn $fn<'a>(input: &'a str)
        -> Result<$return, InvalidValueErr<'a>>
        {
            let input = input.trim();
            match input {
                $(
                    $identifier_string => Ok($return::$enum_type),
                )+
                _ => Err(InvalidValueErr(input)),
            }
        }

        impl FormatAsCssValue for $return {
            fn format_as_css_value(&self, f: &mut fmt::Formatter) -> fmt::Result {
                match self {
                    $(
                        $return::$enum_type => write!(f, $identifier_string),
                    )+
                }
            }
        }
    };
    ($fn:ident, $return:ident, $([$identifier_string:expr, $enum_type:ident]),+) => {
        multi_type_parser!($fn, stringify!($return), $return,
            concat!(
                "# extern crate azul_css;", "\r\n",
                "# use azul_css::parser2::", stringify!($fn), ";", "\r\n",
                "# use azul_css::", stringify!($return), ";"
            ),
            $([
                $identifier_string, $enum_type,
                concat!("assert_eq!(", stringify!($fn), "(\"", $identifier_string, "\"), Ok(", stringify!($return), "::", stringify!($enum_type), "));")
            ]),+
        );
    };
}

macro_rules! typed_pixel_value_parser {
    (
        $fn:ident, $fn_str:expr, $return:ident, $return_str:expr, $import_str:expr, $test_str:expr
    ) => {
        ///Parses a `
        #[doc = $return_str]
        ///` attribute from a `&str`
        ///
        ///# Example
        ///
        ///```rust
        #[doc = $import_str]
        #[doc = $test_str]
        ///```
        pub fn $fn<'a>(input: &'a str) -> Result<$return, CssPixelValueParseError<'a>> {
            crate::props::basic::parse_pixel_value(input).and_then(|e| Ok($return { inner: e }))
        }

        impl crate::props::formatter::FormatAsCssValue for $return {
            fn format_as_css_value(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
                self.inner.format_as_css_value(f)
            }
        }
    };
    ($fn:ident, $return:ident) => {
        typed_pixel_value_parser!(
            $fn,
            stringify!($fn),
            $return,
            stringify!($return),
            concat!(
                "# extern crate azul_css;",
                "\r\n",
                "# use azul_css::parser2::",
                stringify!($fn),
                ";",
                "\r\n",
                "# use azul_css::props::basic::pixel::PixelValue;\r\n",
                "# use azul_css::{",
                stringify!($return),
                "};"
            ),
            concat!(
                "assert_eq!(",
                stringify!($fn),
                "(\"5px\"), Ok(",
                stringify!($return),
                " { inner: PixelValue::px(5.0) }));"
            )
        );
    };
}

macro_rules! css_property_from_type {
    ($prop_type:expr, $content_type:ident) => {{
        match $prop_type {
            CssPropertyType::TextColor => CssProperty::TextColor(CssPropertyValue::$content_type),
            CssPropertyType::FontSize => CssProperty::FontSize(CssPropertyValue::$content_type),
            CssPropertyType::FontFamily => CssProperty::FontFamily(CssPropertyValue::$content_type),
            CssPropertyType::TextAlign => CssProperty::TextAlign(CssPropertyValue::$content_type),
            CssPropertyType::LetterSpacing => {
                CssProperty::LetterSpacing(CssPropertyValue::$content_type)
            }
            CssPropertyType::LineHeight => CssProperty::LineHeight(CssPropertyValue::$content_type),
            CssPropertyType::WordSpacing => {
                CssProperty::WordSpacing(CssPropertyValue::$content_type)
            }
            CssPropertyType::TabWidth => CssProperty::TabWidth(CssPropertyValue::$content_type),
            CssPropertyType::Cursor => CssProperty::Cursor(CssPropertyValue::$content_type),
            CssPropertyType::Display => CssProperty::Display(CssPropertyValue::$content_type),
            CssPropertyType::Float => CssProperty::Float(CssPropertyValue::$content_type),
            CssPropertyType::BoxSizing => CssProperty::BoxSizing(CssPropertyValue::$content_type),
            CssPropertyType::Width => CssProperty::Width(CssPropertyValue::$content_type),
            CssPropertyType::Height => CssProperty::Height(CssPropertyValue::$content_type),
            CssPropertyType::MinWidth => CssProperty::MinWidth(CssPropertyValue::$content_type),
            CssPropertyType::MinHeight => CssProperty::MinHeight(CssPropertyValue::$content_type),
            CssPropertyType::MaxWidth => CssProperty::MaxWidth(CssPropertyValue::$content_type),
            CssPropertyType::MaxHeight => CssProperty::MaxHeight(CssPropertyValue::$content_type),
            CssPropertyType::Position => CssProperty::Position(CssPropertyValue::$content_type),
            CssPropertyType::Top => CssProperty::Top(CssPropertyValue::$content_type),
            CssPropertyType::Right => CssProperty::Right(CssPropertyValue::$content_type),
            CssPropertyType::Left => CssProperty::Left(CssPropertyValue::$content_type),
            CssPropertyType::Bottom => CssProperty::Bottom(CssPropertyValue::$content_type),
            CssPropertyType::ZIndex => CssProperty::ZIndex(CssPropertyValue::$content_type),
            CssPropertyType::FlexWrap => CssProperty::FlexWrap(CssPropertyValue::$content_type),
            CssPropertyType::FlexDirection => {
                CssProperty::FlexDirection(CssPropertyValue::$content_type)
            }
            CssPropertyType::FlexGrow => CssProperty::FlexGrow(CssPropertyValue::$content_type),
            CssPropertyType::FlexShrink => CssProperty::FlexShrink(CssPropertyValue::$content_type),
            CssPropertyType::FlexBasis => CssProperty::FlexBasis(CssPropertyValue::$content_type),
            CssPropertyType::JustifyContent => {
                CssProperty::JustifyContent(CssPropertyValue::$content_type)
            }
            CssPropertyType::AlignItems => CssProperty::AlignItems(CssPropertyValue::$content_type),
            CssPropertyType::AlignContent => {
                CssProperty::AlignContent(CssPropertyValue::$content_type)
            }
            CssPropertyType::ColumnGap => CssProperty::ColumnGap(CssPropertyValue::$content_type),
            CssPropertyType::RowGap => CssProperty::RowGap(CssPropertyValue::$content_type),
            CssPropertyType::GridTemplateColumns => {
                CssProperty::GridTemplateColumns(CssPropertyValue::$content_type)
            }
            CssPropertyType::GridTemplateRows => {
                CssProperty::GridTemplateRows(CssPropertyValue::$content_type)
            }
            CssPropertyType::GridAutoColumns => {
                CssProperty::GridAutoColumns(CssPropertyValue::$content_type)
            }
            CssPropertyType::GridAutoRows => {
                CssProperty::GridAutoRows(CssPropertyValue::$content_type)
            }
            CssPropertyType::GridColumn => CssProperty::GridColumn(CssPropertyValue::$content_type),
            CssPropertyType::GridRow => CssProperty::GridRow(CssPropertyValue::$content_type),
            CssPropertyType::WritingMode => {
                CssProperty::WritingMode(CssPropertyValue::$content_type)
            }
            CssPropertyType::Clear => CssProperty::Clear(CssPropertyValue::$content_type),
            CssPropertyType::OverflowX => CssProperty::OverflowX(CssPropertyValue::$content_type),
            CssPropertyType::OverflowY => CssProperty::OverflowY(CssPropertyValue::$content_type),
            CssPropertyType::PaddingTop => CssProperty::PaddingTop(CssPropertyValue::$content_type),
            CssPropertyType::PaddingLeft => {
                CssProperty::PaddingLeft(CssPropertyValue::$content_type)
            }
            CssPropertyType::PaddingRight => {
                CssProperty::PaddingRight(CssPropertyValue::$content_type)
            }
            CssPropertyType::PaddingBottom => {
                CssProperty::PaddingBottom(CssPropertyValue::$content_type)
            }
            CssPropertyType::MarginTop => CssProperty::MarginTop(CssPropertyValue::$content_type),
            CssPropertyType::MarginLeft => CssProperty::MarginLeft(CssPropertyValue::$content_type),
            CssPropertyType::MarginRight => {
                CssProperty::MarginRight(CssPropertyValue::$content_type)
            }
            CssPropertyType::MarginBottom => {
                CssProperty::MarginBottom(CssPropertyValue::$content_type)
            }
            CssPropertyType::BackgroundContent => {
                CssProperty::BackgroundContent(CssPropertyValue::$content_type)
            }
            CssPropertyType::BackgroundPosition => {
                CssProperty::BackgroundPosition(CssPropertyValue::$content_type)
            }
            CssPropertyType::BackgroundSize => {
                CssProperty::BackgroundSize(CssPropertyValue::$content_type)
            }
            CssPropertyType::BackgroundRepeat => {
                CssProperty::BackgroundRepeat(CssPropertyValue::$content_type)
            }
            CssPropertyType::BorderTopLeftRadius => {
                CssProperty::BorderTopLeftRadius(CssPropertyValue::$content_type)
            }
            CssPropertyType::BorderTopRightRadius => {
                CssProperty::BorderTopRightRadius(CssPropertyValue::$content_type)
            }
            CssPropertyType::BorderBottomLeftRadius => {
                CssProperty::BorderBottomLeftRadius(CssPropertyValue::$content_type)
            }
            CssPropertyType::BorderBottomRightRadius => {
                CssProperty::BorderBottomRightRadius(CssPropertyValue::$content_type)
            }
            CssPropertyType::BorderTopColor => {
                CssProperty::BorderTopColor(CssPropertyValue::$content_type)
            }
            CssPropertyType::BorderRightColor => {
                CssProperty::BorderRightColor(CssPropertyValue::$content_type)
            }
            CssPropertyType::BorderLeftColor => {
                CssProperty::BorderLeftColor(CssPropertyValue::$content_type)
            }
            CssPropertyType::BorderBottomColor => {
                CssProperty::BorderBottomColor(CssPropertyValue::$content_type)
            }
            CssPropertyType::BorderTopStyle => {
                CssProperty::BorderTopStyle(CssPropertyValue::$content_type)
            }
            CssPropertyType::BorderRightStyle => {
                CssProperty::BorderRightStyle(CssPropertyValue::$content_type)
            }
            CssPropertyType::BorderLeftStyle => {
                CssProperty::BorderLeftStyle(CssPropertyValue::$content_type)
            }
            CssPropertyType::BorderBottomStyle => {
                CssProperty::BorderBottomStyle(CssPropertyValue::$content_type)
            }
            CssPropertyType::BorderTopWidth => {
                CssProperty::BorderTopWidth(CssPropertyValue::$content_type)
            }
            CssPropertyType::BorderRightWidth => {
                CssProperty::BorderRightWidth(CssPropertyValue::$content_type)
            }
            CssPropertyType::BorderLeftWidth => {
                CssProperty::BorderLeftWidth(CssPropertyValue::$content_type)
            }
            CssPropertyType::BorderBottomWidth => {
                CssProperty::BorderBottomWidth(CssPropertyValue::$content_type)
            }
            CssPropertyType::BoxShadowLeft => {
                CssProperty::BoxShadowLeft(CssPropertyValue::$content_type)
            }
            CssPropertyType::BoxShadowRight => {
                CssProperty::BoxShadowRight(CssPropertyValue::$content_type)
            }
            CssPropertyType::BoxShadowTop => {
                CssProperty::BoxShadowTop(CssPropertyValue::$content_type)
            }
            CssPropertyType::BoxShadowBottom => {
                CssProperty::BoxShadowBottom(CssPropertyValue::$content_type)
            }
            CssPropertyType::ScrollbarStyle => {
                CssProperty::ScrollbarStyle(CssPropertyValue::$content_type)
            }
            CssPropertyType::Opacity => CssProperty::Opacity(CssPropertyValue::$content_type),
            CssPropertyType::Visibility => CssProperty::Visibility(CssPropertyValue::$content_type),
            CssPropertyType::Transform => CssProperty::Transform(CssPropertyValue::$content_type),
            CssPropertyType::PerspectiveOrigin => {
                CssProperty::PerspectiveOrigin(CssPropertyValue::$content_type)
            }
            CssPropertyType::TransformOrigin => {
                CssProperty::TransformOrigin(CssPropertyValue::$content_type)
            }
            CssPropertyType::BackfaceVisibility => {
                CssProperty::BackfaceVisibility(CssPropertyValue::$content_type)
            }
            CssPropertyType::MixBlendMode => {
                CssProperty::MixBlendMode(CssPropertyValue::$content_type)
            }
            CssPropertyType::Filter => CssProperty::Filter(CssPropertyValue::$content_type),
            CssPropertyType::BackdropFilter => {
                CssProperty::BackdropFilter(CssPropertyValue::$content_type)
            }
            CssPropertyType::TextShadow => CssProperty::TextShadow(CssPropertyValue::$content_type),
            CssPropertyType::Direction => CssProperty::Direction(CssPropertyValue::$content_type),
            CssPropertyType::Hyphens => CssProperty::Hyphens(CssPropertyValue::$content_type),
            CssPropertyType::WhiteSpace => CssProperty::WhiteSpace(CssPropertyValue::$content_type),
        }
    }};
}
