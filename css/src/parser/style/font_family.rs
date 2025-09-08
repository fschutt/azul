use alloc::{string::ToString, vec::Vec};

use crate::{css_properties::*, parser::*, AzString, impl_vec, impl_vec_debug, impl_vec_partialord, impl_vec_ord, impl_vec_clone, impl_vec_partialeq, impl_vec_eq, impl_vec_hash};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
pub enum StyleFontFamily {
    /// Native font, such as "Webly Sleeky UI", "monospace", etc.
    System(AzString),
    /// Font loaded from a file
    File(AzString),
    /// Reference-counted, already-decoded font,
    /// so that specific DOM nodes are required to use this font
    Ref(FontRef),
}

impl StyleFontFamily {
    pub(crate) fn as_string(&self) -> String {
        match &self {
            StyleFontFamily::System(s) => s.clone().into_library_owned_string(),
            StyleFontFamily::File(s) => s.clone().into_library_owned_string(),
            StyleFontFamily::Ref(s) => format!("{:0x}", s.data as usize),
        }
    }
}

impl_vec!(
    StyleFontFamily,
    StyleFontFamilyVec,
    StyleFontFamilyVecDestructor
);
impl_vec_clone!(
    StyleFontFamily,
    StyleFontFamilyVec,
    StyleFontFamilyVecDestructor
);
impl_vec_debug!(StyleFontFamily, StyleFontFamilyVec);
impl_vec_eq!(StyleFontFamily, StyleFontFamilyVec);
impl_vec_ord!(StyleFontFamily, StyleFontFamilyVec);
impl_vec_hash!(StyleFontFamily, StyleFontFamilyVec);
impl_vec_partialeq!(StyleFontFamily, StyleFontFamilyVec);
impl_vec_partialord!(StyleFontFamily, StyleFontFamilyVec);

#[derive(Debug, PartialEq, Copy, Clone)]
pub enum CssStyleFontFamilyParseError<'a> {
    InvalidStyleFontFamily(&'a str),
    UnclosedQuotes(&'a str),
}

impl_display! {CssStyleFontFamilyParseError<'a>, {
    InvalidStyleFontFamily(val) => format!("Invalid font-family: \"{}\"", val),
    UnclosedQuotes(val) => format!("Unclosed quotes: \"{}\"", val),
}}

impl<'a> From<UnclosedQuotesError<'a>> for CssStyleFontFamilyParseError<'a> {
    fn from(err: UnclosedQuotesError<'a>) -> Self {
        CssStyleFontFamilyParseError::UnclosedQuotes(err.0)
    }
}

/// Owned version of CssStyleFontFamilyParseError.
#[derive(Debug, Clone, PartialEq)]
pub enum CssStyleFontFamilyParseErrorOwned {
    InvalidStyleFontFamily(String),
    UnclosedQuotes(String),
}

impl<'a> CssStyleFontFamilyParseError<'a> {
    pub fn to_contained(&self) -> CssStyleFontFamilyParseErrorOwned {
        match self {
            CssStyleFontFamilyParseError::InvalidStyleFontFamily(s) => {
                CssStyleFontFamilyParseErrorOwned::InvalidStyleFontFamily(s.to_string())
            }
            CssStyleFontFamilyParseError::UnclosedQuotes(s) => {
                CssStyleFontFamilyParseErrorOwned::UnclosedQuotes(s.to_string())
            }
        }
    }
}

impl CssStyleFontFamilyParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> CssStyleFontFamilyParseError<'a> {
        match self {
            CssStyleFontFamilyParseErrorOwned::InvalidStyleFontFamily(s) => {
                CssStyleFontFamilyParseError::InvalidStyleFontFamily(s.as_str())
            }
            CssStyleFontFamilyParseErrorOwned::UnclosedQuotes(s) => {
                CssStyleFontFamilyParseError::UnclosedQuotes(s.as_str())
            }
        }
    }
}

/// Parses a `StyleFontFamily` declaration from a `&str`
///
/// # Example
///
/// ```rust
/// # extern crate azul_css;
/// # use azul_css::parser::parse_style_font_family;
/// # use azul_css::{StyleFontFamily, StyleFontFamilyVec};
/// let input = "\"Helvetica\", 'Arial', Times New Roman";
/// let fonts: StyleFontFamilyVec = vec![
///     StyleFontFamily::System("Helvetica".into()),
///     StyleFontFamily::System("Arial".into()),
///     StyleFontFamily::System("Times New Roman".into()),
/// ]
/// .into();
///
/// assert_eq!(parse_style_font_family(input), Ok(fonts));
/// ```
pub fn parse_style_font_family<'a>(
    input: &'a str,
) -> Result<StyleFontFamilyVec, CssStyleFontFamilyParseError<'a>> {
    let multiple_fonts = input.split(',');
    let mut fonts = Vec::with_capacity(1);

    for font in multiple_fonts {
        let font = font.trim();
        let font = font.trim_matches('\'');
        let font = font.trim_matches('\"');
        let font = font.trim();
        fonts.push(StyleFontFamily::System(font.to_string().into()));
    }

    Ok(fonts.into())
}
