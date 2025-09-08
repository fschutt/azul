use crate::{css_properties::*, parser::*};

#[derive(Debug, Clone, PartialEq, Eq, Ord, PartialOrd)]
pub enum CssStyleBorderRadiusParseError<'a> {
    TooManyValues(&'a str),
    CssPixelValueParseError(CssPixelValueParseError<'a>),
}

impl_display! { CssStyleBorderRadiusParseError<'a>, {
    TooManyValues(val) => format!("Too many values: \"{}\"", val),
    CssPixelValueParseError(e) => format!("{}", e),
}}

impl_from!(
    CssPixelValueParseError<'a>,
    CssStyleBorderRadiusParseError::CssPixelValueParseError
);

/// Owned version
#[derive(Debug, Clone, PartialEq)]
pub enum CssStyleBorderRadiusParseErrorOwned {
    TooManyValues(String),
    CssPixelValueParseError(CssPixelValueParseErrorOwned),
}

impl<'a> CssStyleBorderRadiusParseError<'a> {
    pub fn to_contained(&self) -> CssStyleBorderRadiusParseErrorOwned {
        match self {
            CssStyleBorderRadiusParseError::TooManyValues(s) => {
                CssStyleBorderRadiusParseErrorOwned::TooManyValues(s.to_string())
            }
            CssStyleBorderRadiusParseError::CssPixelValueParseError(e) => {
                CssStyleBorderRadiusParseErrorOwned::CssPixelValueParseError(e.to_contained())
            }
        }
    }
}

impl CssStyleBorderRadiusParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> CssStyleBorderRadiusParseError<'a> {
        match self {
            CssStyleBorderRadiusParseErrorOwned::TooManyValues(s) => {
                CssStyleBorderRadiusParseError::TooManyValues(s)
            }
            CssStyleBorderRadiusParseErrorOwned::CssPixelValueParseError(e) => {
                CssStyleBorderRadiusParseError::CssPixelValueParseError(e.to_shared())
            }
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub struct StyleBorderRadius {
    // TODO: Should technically be PixelSize because the border radius doesn't have to be uniform
    // but the parsing for that is complicated...
    pub top_left: PixelValue,
    pub top_right: PixelValue,
    pub bottom_left: PixelValue,
    pub bottom_right: PixelValue,
}

impl Default for StyleBorderRadius {
    fn default() -> Self {
        Self::zero()
    }
}

impl StyleBorderRadius {
    pub const fn zero() -> Self {
        Self::uniform(PixelValue::zero())
    }

    pub const fn uniform(value: PixelValue) -> Self {
        Self {
            top_left: value,
            top_right: value,
            bottom_left: value,
            bottom_right: value,
        }
    }
}

/// parse the border-radius like "5px 10px" or "5px 10px 6px 10px"
pub fn parse_style_border_radius<'a>(
    input: &'a str,
) -> Result<StyleBorderRadius, CssStyleBorderRadiusParseError<'a>> {
    let mut components = input.split_whitespace();
    let len = components.clone().count();

    match len {
        1 => {
            // One value - border-radius: 15px;
            // (the value applies to all four corners, which are rounded equally:

            let uniform_radius = parse_pixel_value(components.next().unwrap())?;
            Ok(StyleBorderRadius::uniform(uniform_radius))
        }
        2 => {
            // Two values - border-radius: 15px 50px;
            // (first value applies to top-left and bottom-right corners,
            // and the second value applies to top-right and bottom-left corners):

            let top_left_bottom_right = parse_pixel_value(components.next().unwrap())?;
            let top_right_bottom_left = parse_pixel_value(components.next().unwrap())?;

            Ok(StyleBorderRadius {
                top_left: top_left_bottom_right,
                bottom_right: top_left_bottom_right,
                top_right: top_right_bottom_left,
                bottom_left: top_right_bottom_left,
            })
        }
        3 => {
            // Three values - border-radius: 15px 50px 30px;
            // (first value applies to top-left corner,
            // second value applies to top-right and bottom-left corners,
            // and third value applies to bottom-right corner):
            let top_left = parse_pixel_value(components.next().unwrap())?;
            let top_right_bottom_left = parse_pixel_value(components.next().unwrap())?;
            let bottom_right = parse_pixel_value(components.next().unwrap())?;

            Ok(StyleBorderRadius {
                top_left,
                bottom_right,
                top_right: top_right_bottom_left,
                bottom_left: top_right_bottom_left,
            })
        }
        4 => {
            // Four values - border-radius: 15px 50px 30px 5px;
            //
            // first value applies to top-left corner,
            // second value applies to top-right corner,
            // third value applies to bottom-right corner,
            // fourth value applies to bottom-left corner

            let top_left = parse_pixel_value(components.next().unwrap())?;
            let top_right = parse_pixel_value(components.next().unwrap())?;
            let bottom_right = parse_pixel_value(components.next().unwrap())?;
            let bottom_left = parse_pixel_value(components.next().unwrap())?;

            Ok(StyleBorderRadius {
                top_left,
                bottom_right,
                top_right,
                bottom_left,
            })
        }
        _ => Err(CssStyleBorderRadiusParseError::TooManyValues(input)),
    }
}
