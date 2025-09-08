use crate::{css_properties::*, parser::*};

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StyleBorderSide {
    pub border_width: PixelValue,
    pub border_style: BorderStyle,
    pub border_color: ColorU,
}


#[derive(Clone, PartialEq)]
pub enum CssBorderParseError<'a> {
    MissingThickness(&'a str),
    InvalidBorderStyle(InvalidValueErr<'a>),
    InvalidBorderDeclaration(&'a str),
    ThicknessParseError(CssPixelValueParseError<'a>),
    ColorParseError(CssColorParseError<'a>),
}
impl_debug_as_display!(CssBorderParseError<'a>);
impl_display! { CssBorderParseError<'a>, {
    MissingThickness(e) => format!("Missing border thickness: \"{}\"", e),
    InvalidBorderStyle(e) => format!("Invalid style: {}", e.0),
    InvalidBorderDeclaration(e) => format!("Invalid declaration: \"{}\"", e),
    ThicknessParseError(e) => format!("Invalid thickness: {}", e),
    ColorParseError(e) => format!("Invalid color: {}", e),
}}

/// Owned version of CssBorderParseError.
#[derive(Debug, Clone, PartialEq)]
pub enum CssBorderParseErrorOwned {
    MissingThickness(String),
    InvalidBorderStyle(InvalidValueErrOwned),
    InvalidBorderDeclaration(String),
    ThicknessParseError(CssPixelValueParseErrorOwned),
    ColorParseError(CssColorParseErrorOwned),
}

/// Parse a CSS border such as
///
/// "5px solid red", "solid black" (1px)
pub fn parse_style_border<'a>(input: &'a str) -> Result<StyleBorderSide, CssBorderParseError<'a>> {
    use self::CssBorderParseError::*;

    let oi = input.trim().split_whitespace().collect::<Vec<_>>();

    match oi.len() {
        0 => return Err(CssBorderParseError::InvalidBorderDeclaration(input)),
        1 => {
            // First argument is the one and only argument,
            // therefore has to be a style such as "double"
            Ok(StyleBorderSide {
                border_style: parse_style_border_style(&oi[0])
                    .map_err(|e| InvalidBorderStyle(e))?,
                border_width: MEDIUM_BORDER_THICKNESS,
                border_color: DEFAULT_BORDER_COLOR,
            })
        }
        2 => Ok(StyleBorderSide {
            border_style: parse_style_border_style(&oi[0]).map_err(|e| InvalidBorderStyle(e))?,
            border_width: MEDIUM_BORDER_THICKNESS,
            border_color: parse_css_color(&oi[1]).map_err(|e| ColorParseError(e))?,
        }),
        _ => Ok(StyleBorderSide {
            border_width: match oi[0].trim() {
                "medium" => MEDIUM_BORDER_THICKNESS,
                "thin" => THIN_BORDER_THICKNESS,
                "thick" => THICK_BORDER_THICKNESS,
                _ => parse_pixel_value(&oi[0]).map_err(|e| ThicknessParseError(e))?,
            },
            border_style: parse_style_border_style(&oi[1]).map_err(|e| InvalidBorderStyle(e))?,
            border_color: parse_css_color(&oi[2]).map_err(|e| ColorParseError(e))?,
        }),
    }
}
