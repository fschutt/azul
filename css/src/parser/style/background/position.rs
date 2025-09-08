use crate::{css_properties::*, parser::*, impl_vec, impl_vec_debug, impl_vec_partialord, impl_vec_ord, impl_vec_clone, impl_vec_partialeq, impl_vec_eq, impl_vec_hash};

/// Represents a `background-position` attribute
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleBackgroundPosition {
    pub horizontal: BackgroundPositionHorizontal,
    pub vertical: BackgroundPositionVertical,
}

impl StyleBackgroundPosition {
    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        self.horizontal.scale_for_dpi(scale_factor);
        self.vertical.scale_for_dpi(scale_factor);
    }
}

impl_vec!(
    StyleBackgroundPosition,
    StyleBackgroundPositionVec,
    StyleBackgroundPositionVecDestructor
);
impl_vec_debug!(StyleBackgroundPosition, StyleBackgroundPositionVec);
impl_vec_partialord!(StyleBackgroundPosition, StyleBackgroundPositionVec);
impl_vec_ord!(StyleBackgroundPosition, StyleBackgroundPositionVec);
impl_vec_clone!(
    StyleBackgroundPosition,
    StyleBackgroundPositionVec,
    StyleBackgroundPositionVecDestructor
);
impl_vec_partialeq!(StyleBackgroundPosition, StyleBackgroundPositionVec);
impl_vec_eq!(StyleBackgroundPosition, StyleBackgroundPositionVec);
impl_vec_hash!(StyleBackgroundPosition, StyleBackgroundPositionVec);

impl Default for StyleBackgroundPosition {
    fn default() -> Self {
        StyleBackgroundPosition {
            horizontal: BackgroundPositionHorizontal::Left,
            vertical: BackgroundPositionVertical::Top,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
pub enum BackgroundPositionHorizontal {
    Left,
    Center,
    Right,
    Exact(PixelValue),
}

impl BackgroundPositionHorizontal {
    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        match self {
            BackgroundPositionHorizontal::Exact(s) => {
                s.scale_for_dpi(scale_factor);
            }
            _ => {}
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
pub enum BackgroundPositionVertical {
    Top,
    Center,
    Bottom,
    Exact(PixelValue),
}

impl BackgroundPositionVertical {
    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        match self {
            BackgroundPositionVertical::Exact(s) => {
                s.scale_for_dpi(scale_factor);
            }
            _ => {}
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum CssBackgroundPositionParseError<'a> {
    NoPosition(&'a str),
    TooManyComponents(&'a str),
    FirstComponentWrong(CssPixelValueParseError<'a>),
    SecondComponentWrong(CssPixelValueParseError<'a>),
}

impl_display! {CssBackgroundPositionParseError<'a>, {
    NoPosition(e) => format!("First background position missing: \"{}\"", e),
    TooManyComponents(e) => format!("background-position can only have one or two components, not more: \"{}\"", e),
    FirstComponentWrong(e) => format!("Failed to parse first component: \"{}\"", e),
    SecondComponentWrong(e) => format!("Failed to parse second component: \"{}\"", e),
}}

/// Owned version of CssBackgroundPositionParseError.
#[derive(Debug, Clone, PartialEq)]
pub enum CssBackgroundPositionParseErrorOwned {
    NoPosition(String),
    TooManyComponents(String),
    FirstComponentWrong(CssPixelValueParseErrorOwned),
    SecondComponentWrong(CssPixelValueParseErrorOwned),
}

// parses multiple background-positions
pub fn parse_style_background_position_multiple<'a>(
    input: &'a str,
) -> Result<StyleBackgroundPositionVec, CssBackgroundPositionParseError<'a>> {
    Ok(split_string_respect_comma(input)
        .iter()
        .map(|i| parse_style_background_position(i))
        .collect::<Result<Vec<_>, _>>()?
        .into())
}

pub fn parse_style_background_position<'a>(
    input: &'a str,
) -> Result<StyleBackgroundPosition, CssBackgroundPositionParseError<'a>> {
    use self::CssBackgroundPositionParseError::*;

    let input = input.trim();
    let mut whitespace_iter = input.split_whitespace();

    let first = whitespace_iter.next().ok_or(NoPosition(input))?;
    let second = whitespace_iter.next();

    if whitespace_iter.next().is_some() {
        return Err(TooManyComponents(input));
    }

    let horizontal =
        parse_background_position_horizontal(first).map_err(|e| FirstComponentWrong(e))?;

    let vertical = match second {
        Some(second) => {
            parse_background_position_vertical(second).map_err(|e| SecondComponentWrong(e))?
        }
        None => BackgroundPositionVertical::Center,
    };

    Ok(StyleBackgroundPosition {
        horizontal,
        vertical,
    })
}
