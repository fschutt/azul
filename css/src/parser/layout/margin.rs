use crate::{css_properties::*, parser::*};

#[derive(Debug, Clone, PartialEq)]
pub enum LayoutMarginParseError<'a> {
    CssPixelValueParseError(CssPixelValueParseError<'a>),
    TooManyValues,
    TooFewValues,
}

impl_display! { LayoutMarginParseError<'a>, {
    CssPixelValueParseError(e) => format!("Could not parse pixel value: {}", e),
    TooManyValues => format!("Too many values - margin property has a maximum of 4 values."),
    TooFewValues => format!("Too few values - margin property has a minimum of 1 value."),
}}

impl_from!(
    CssPixelValueParseError<'a>,
    LayoutMarginParseError::CssPixelValueParseError
);

/// Owned version of LayoutMarginParseError.
#[derive(Debug, Clone, PartialEq)]
pub enum LayoutMarginParseErrorOwned {
    CssPixelValueParseError(CssPixelValueParseErrorOwned),
    TooManyValues,
    TooFewValues,
}

impl<'a> LayoutMarginParseError<'a> {
    pub fn to_contained(&self) -> LayoutMarginParseErrorOwned {
        match self {
            LayoutMarginParseError::CssPixelValueParseError(e) => {
                LayoutMarginParseErrorOwned::CssPixelValueParseError(e.to_contained())
            }
            LayoutMarginParseError::TooManyValues => LayoutMarginParseErrorOwned::TooManyValues,
            LayoutMarginParseError::TooFewValues => LayoutMarginParseErrorOwned::TooFewValues,
        }
    }
}

impl LayoutMarginParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> LayoutMarginParseError<'a> {
        match self {
            LayoutMarginParseErrorOwned::CssPixelValueParseError(e) => {
                LayoutMarginParseError::CssPixelValueParseError(e.to_shared())
            }
            LayoutMarginParseErrorOwned::TooManyValues => LayoutMarginParseError::TooManyValues,
            LayoutMarginParseErrorOwned::TooFewValues => LayoutMarginParseError::TooFewValues,
        }
    }
}

/// Represents a parsed `padding` attribute
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LayoutMargin {
    pub top: PixelValueWithAuto,
    pub bottom: PixelValueWithAuto,
    pub left: PixelValueWithAuto,
    pub right: PixelValueWithAuto,
}

pub fn parse_layout_margin<'a>(input: &'a str) -> Result<LayoutMargin, LayoutMarginParseError> {
    match parse_layout_padding(input) {
        Ok(padding) => Ok(LayoutMargin {
            top: padding.top,
            left: padding.left,
            right: padding.right,
            bottom: padding.bottom,
        }),
        Err(LayoutPaddingParseError::CssPixelValueParseError(e)) => Err(e.into()),
        Err(LayoutPaddingParseError::TooManyValues) => Err(LayoutMarginParseError::TooManyValues),
        Err(LayoutPaddingParseError::TooFewValues) => Err(LayoutMarginParseError::TooFewValues),
    }
}
