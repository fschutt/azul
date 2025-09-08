use crate::{css_properties::*, parser::*};

#[derive(Debug, Clone, PartialEq)]
pub enum LayoutPaddingParseError<'a> {
    CssPixelValueParseError(CssPixelValueParseError<'a>),
    TooManyValues,
    TooFewValues,
}

impl_display! { LayoutPaddingParseError<'a>, {
    CssPixelValueParseError(e) => format!("Could not parse pixel value: {}", e),
    TooManyValues => format!("Too many values - padding property has a maximum of 4 values."),
    TooFewValues => format!("Too few values - padding property has a minimum of 1 value."),
}}

impl_from!(
    CssPixelValueParseError<'a>,
    LayoutPaddingParseError::CssPixelValueParseError
);

/// Owned version of LayoutPaddingParseError.
#[derive(Debug, Clone, PartialEq)]
pub enum LayoutPaddingParseErrorOwned {
    CssPixelValueParseError(CssPixelValueParseErrorOwned),
    TooManyValues,
    TooFewValues,
}

impl<'a> LayoutPaddingParseError<'a> {
    pub fn to_contained(&self) -> LayoutPaddingParseErrorOwned {
        match self {
            LayoutPaddingParseError::CssPixelValueParseError(e) => {
                LayoutPaddingParseErrorOwned::CssPixelValueParseError(e.to_contained())
            }
            LayoutPaddingParseError::TooManyValues => LayoutPaddingParseErrorOwned::TooManyValues,
            LayoutPaddingParseError::TooFewValues => LayoutPaddingParseErrorOwned::TooFewValues,
        }
    }
}

impl LayoutPaddingParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> LayoutPaddingParseError<'a> {
        match self {
            LayoutPaddingParseErrorOwned::CssPixelValueParseError(e) => {
                LayoutPaddingParseError::CssPixelValueParseError(e.to_shared())
            }
            LayoutPaddingParseErrorOwned::TooManyValues => LayoutPaddingParseError::TooManyValues,
            LayoutPaddingParseErrorOwned::TooFewValues => LayoutPaddingParseError::TooFewValues,
        }
    }
}

/// Represents a parsed `padding` attribute
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LayoutPadding {
    pub top: PixelValueWithAuto,
    pub bottom: PixelValueWithAuto,
    pub left: PixelValueWithAuto,
    pub right: PixelValueWithAuto,
}

/// Parse a padding value such as
///
/// "10px 10px"
pub fn parse_layout_padding<'a>(input: &'a str) -> Result<LayoutPadding, LayoutPaddingParseError> {
    let mut input_iter = input.split_whitespace();
    let first = parse_pixel_value_with_auto(
        input_iter
            .next()
            .ok_or(LayoutPaddingParseError::TooFewValues)?,
    )?;
    let second = parse_pixel_value_with_auto(match input_iter.next() {
        Some(s) => s,
        None => {
            return Ok(LayoutPadding {
                top: first,
                bottom: first,
                left: first,
                right: first,
            });
        }
    })?;
    let third = parse_pixel_value_with_auto(match input_iter.next() {
        Some(s) => s,
        None => {
            return Ok(LayoutPadding {
                top: first,
                bottom: first,
                left: second,
                right: second,
            });
        }
    })?;
    let fourth = parse_pixel_value_with_auto(match input_iter.next() {
        Some(s) => s,
        None => {
            return Ok(LayoutPadding {
                top: first,
                left: second,
                right: second,
                bottom: third,
            });
        }
    })?;

    if input_iter.next().is_some() {
        return Err(LayoutPaddingParseError::TooManyValues);
    }

    Ok(LayoutPadding {
        top: first,
        right: second,
        bottom: third,
        left: fourth,
    })
}
