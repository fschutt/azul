//! CSS properties for fonts, such as font-family, font-size, font-weight, and font-style.

use alloc::{
    boxed::Box,
    string::{String, ToString},
    vec::Vec,
};
use core::{
    cmp::Ordering,
    ffi::c_void,
    fmt,
    hash::{Hash, Hasher},
    num::ParseIntError,
    sync::atomic::{AtomicUsize, Ordering as AtomicOrdering},
};

#[cfg(feature = "parser")]
use crate::parser::{strip_quotes, UnclosedQuotesError};
use crate::{
    impl_option, impl_vec, impl_vec_clone, impl_vec_debug, impl_vec_eq, impl_vec_hash,
    impl_vec_ord, impl_vec_partialeq, impl_vec_partialord,
    parser::{
        impl_debug_as_display, impl_display, impl_from, InvalidValueErr, InvalidValueErrOwned,
    },
    props::{
        basic::value::{
            parse_pixel_value, CssPixelValueParseError, CssPixelValueParseErrorOwned, PixelValue,
        },
        formatter::PrintAsCssValue,
        macros::impl_pixel_value,
    },
    AzString, U8Vec,
};

// --- Font Weight ---

/// Represents the `font-weight` property.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u16)]
pub enum StyleFontWeight {
    Lighter = 0,
    Normal = 400,
    Bold = 700,
    Bolder = 1000,
    W100 = 100,
    W200 = 200,
    W300 = 300,
    W400 = 400, // Same as Normal
    W500 = 500,
    W600 = 600,
    W700 = 700, // Same as Bold
    W800 = 800,
    W900 = 900,
}

impl Default for StyleFontWeight {
    fn default() -> Self {
        StyleFontWeight::Normal
    }
}

impl PrintAsCssValue for StyleFontWeight {
    fn print_as_css_value(&self) -> String {
        match self {
            StyleFontWeight::Lighter => "lighter".to_string(),
            StyleFontWeight::Normal | StyleFontWeight::W400 => "normal".to_string(),
            StyleFontWeight::Bold | StyleFontWeight::W700 => "bold".to_string(),
            StyleFontWeight::Bolder => "bolder".to_string(),
            _ => (*self as u16).to_string(),
        }
    }
}

// --- Font Style ---

/// Represents the `font-style` property.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum StyleFontStyle {
    Normal,
    Italic,
    Oblique,
}

impl Default for StyleFontStyle {
    fn default() -> Self {
        StyleFontStyle::Normal
    }
}

impl PrintAsCssValue for StyleFontStyle {
    fn print_as_css_value(&self) -> String {
        match self {
            StyleFontStyle::Normal => "normal".to_string(),
            StyleFontStyle::Italic => "italic".to_string(),
            StyleFontStyle::Oblique => "oblique".to_string(),
        }
    }
}

// --- Font Size ---

/// Represents a `font-size` attribute
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleFontSize {
    pub inner: PixelValue,
}

impl Default for StyleFontSize {
    fn default() -> Self {
        Self {
            // Default font size is 12pt, a common default for print and web.
            inner: PixelValue::const_pt(12),
        }
    }
}

impl_pixel_value!(StyleFontSize);
impl PrintAsCssValue for StyleFontSize {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}

// --- Font Resource Management ---

#[repr(C)]
pub struct FontRef {
    pub data: *const FontData,
    pub copies: *const AtomicUsize,
    pub run_destructor: bool,
}
// ... (Implementations for FontRef are unchanged from your provided code) ...
impl fmt::Debug for FontRef {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "FontRef(0x{:x}", self.data as usize)?;
        if let Some(c) = unsafe { self.copies.as_ref() } {
            write!(f, ", copies: {})", c.load(AtomicOrdering::SeqCst))?;
        } else {
            write!(f, ")")?;
        }
        Ok(())
    }
}
impl FontRef {
    #[inline]
    pub fn get_data<'a>(&'a self) -> &'a FontData {
        unsafe { &*self.data }
    }
    pub fn new(data: FontData) -> Self {
        Self {
            data: Box::into_raw(Box::new(data)),
            copies: Box::into_raw(Box::new(AtomicUsize::new(1))),
            run_destructor: true,
        }
    }
    pub fn get_bytes(&self) -> U8Vec {
        self.get_data().bytes.clone()
    }
}
impl_option!(
    FontRef,
    OptionFontRef,
    copy = false,
    [Debug, Clone, PartialEq, Eq, Hash]
);
unsafe impl Send for FontRef {}
unsafe impl Sync for FontRef {}
impl PartialEq for FontRef {
    fn eq(&self, rhs: &Self) -> bool {
        self.data as usize == rhs.data as usize
    }
}
impl PartialOrd for FontRef {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some((self.data as usize).cmp(&(other.data as usize)))
    }
}
impl Ord for FontRef {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.data as usize).cmp(&(other.data as usize))
    }
}
impl Eq for FontRef {}
impl Hash for FontRef {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (self.data as usize).hash(state);
    }
}
impl Clone for FontRef {
    fn clone(&self) -> Self {
        if !self.copies.is_null() {
            unsafe {
                (*self.copies).fetch_add(1, AtomicOrdering::SeqCst);
            }
        }
        Self {
            data: self.data,
            copies: self.copies,
            run_destructor: true,
        }
    }
}
impl Drop for FontRef {
    fn drop(&mut self) {
        if self.run_destructor && !self.copies.is_null() {
            if unsafe { (*self.copies).fetch_sub(1, AtomicOrdering::SeqCst) } == 1 {
                unsafe {
                    let _ = Box::from_raw(self.data as *mut FontData);
                    let _ = Box::from_raw(self.copies as *mut AtomicUsize);
                }
            }
        }
    }
}

pub struct FontData {
    pub bytes: U8Vec,
    pub font_index: u32,
    pub parsed: *const c_void,
    pub parsed_destructor: fn(*mut c_void),
}
// ... (Implementations for FontData are unchanged from your provided code) ...
impl fmt::Debug for FontData {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "FontData {{ bytes: {} bytes, font_index: {} }}",
            self.bytes.len(),
            self.font_index
        )
    }
}
unsafe impl Send for FontData {}
unsafe impl Sync for FontData {}
impl Drop for FontData {
    fn drop(&mut self) {
        (self.parsed_destructor)(self.parsed as *mut c_void);
    }
}

// --- Font Family ---

/// Represents a `font-family` attribute
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
pub enum StyleFontFamily {
    System(AzString),
    File(AzString),
    Ref(FontRef),
}

impl StyleFontFamily {
    pub(crate) fn as_string(&self) -> String {
        match &self {
            StyleFontFamily::System(s) => {
                let owned = s.clone().into_library_owned_string();
                if owned.contains(char::is_whitespace) {
                    format!("\"{}\"", owned)
                } else {
                    owned
                }
            }
            StyleFontFamily::File(s) => format!("url({})", s.clone().into_library_owned_string()),
            StyleFontFamily::Ref(s) => format!("font-ref(0x{:x})", s.data as usize),
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

impl PrintAsCssValue for StyleFontFamilyVec {
    fn print_as_css_value(&self) -> String {
        self.iter()
            .map(|f| f.as_string())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

// --- PARSERS ---

// -- Font Weight Parser --

#[derive(Debug, Clone, PartialEq)]
pub enum CssFontWeightParseError<'a> {
    InvalidValue(InvalidValueErr<'a>),
    InvalidNumber(ParseIntError),
}
impl_debug_as_display!(CssFontWeightParseError<'a>);
impl_display! { CssFontWeightParseError<'a>, {
    InvalidValue(e) => format!("Invalid font-weight keyword: \"{}\"", e.0),
    InvalidNumber(e) => format!("Invalid font-weight number: {}", e),
}}
impl<'a> From<InvalidValueErr<'a>> for CssFontWeightParseError<'a> {
    fn from(e: InvalidValueErr<'a>) -> Self {
        CssFontWeightParseError::InvalidValue(e)
    }
}
impl<'a> From<ParseIntError> for CssFontWeightParseError<'a> {
    fn from(e: ParseIntError) -> Self {
        CssFontWeightParseError::InvalidNumber(e)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum CssFontWeightParseErrorOwned {
    InvalidValue(InvalidValueErrOwned),
    InvalidNumber(ParseIntError),
}

impl<'a> CssFontWeightParseError<'a> {
    pub fn to_contained(&self) -> CssFontWeightParseErrorOwned {
        match self {
            Self::InvalidValue(e) => CssFontWeightParseErrorOwned::InvalidValue(e.to_contained()),
            Self::InvalidNumber(e) => CssFontWeightParseErrorOwned::InvalidNumber(e.clone()),
        }
    }
}

impl CssFontWeightParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> CssFontWeightParseError<'a> {
        match self {
            Self::InvalidValue(e) => CssFontWeightParseError::InvalidValue(e.to_shared()),
            Self::InvalidNumber(e) => CssFontWeightParseError::InvalidNumber(e.clone()),
        }
    }
}

#[cfg(feature = "parser")]
pub fn parse_font_weight<'a>(
    input: &'a str,
) -> Result<StyleFontWeight, CssFontWeightParseError<'a>> {
    let input = input.trim();
    match input {
        "lighter" => Ok(StyleFontWeight::Lighter),
        "normal" => Ok(StyleFontWeight::Normal),
        "bold" => Ok(StyleFontWeight::Bold),
        "bolder" => Ok(StyleFontWeight::Bolder),
        "100" => Ok(StyleFontWeight::W100),
        "200" => Ok(StyleFontWeight::W200),
        "300" => Ok(StyleFontWeight::W300),
        "400" => Ok(StyleFontWeight::W400),
        "500" => Ok(StyleFontWeight::W500),
        "600" => Ok(StyleFontWeight::W600),
        "700" => Ok(StyleFontWeight::W700),
        "800" => Ok(StyleFontWeight::W800),
        "900" => Ok(StyleFontWeight::W900),
        _ => Err(InvalidValueErr(input).into()),
    }
}

// -- Font Style Parser --

#[derive(Debug, Clone, PartialEq)]
pub enum CssFontStyleParseError<'a> {
    InvalidValue(InvalidValueErr<'a>),
}
impl_debug_as_display!(CssFontStyleParseError<'a>);
impl_display! { CssFontStyleParseError<'a>, {
    InvalidValue(e) => format!("Invalid font-style: \"{}\"", e.0),
}}
impl_from! { InvalidValueErr<'a>, CssFontStyleParseError::InvalidValue }

#[derive(Debug, Clone, PartialEq)]
pub enum CssFontStyleParseErrorOwned {
    InvalidValue(InvalidValueErrOwned),
}
impl<'a> CssFontStyleParseError<'a> {
    pub fn to_contained(&self) -> CssFontStyleParseErrorOwned {
        match self {
            Self::InvalidValue(e) => CssFontStyleParseErrorOwned::InvalidValue(e.to_contained()),
        }
    }
}
impl CssFontStyleParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> CssFontStyleParseError<'a> {
        match self {
            Self::InvalidValue(e) => CssFontStyleParseError::InvalidValue(e.to_shared()),
        }
    }
}

#[cfg(feature = "parser")]
pub fn parse_font_style<'a>(input: &'a str) -> Result<StyleFontStyle, CssFontStyleParseError<'a>> {
    match input.trim() {
        "normal" => Ok(StyleFontStyle::Normal),
        "italic" => Ok(StyleFontStyle::Italic),
        "oblique" => Ok(StyleFontStyle::Oblique),
        other => Err(InvalidValueErr(other).into()),
    }
}

// -- Font Size Parser --

#[derive(Debug, Clone, PartialEq)]
pub enum CssStyleFontSizeParseError<'a> {
    PixelValue(CssPixelValueParseError<'a>),
}
impl_debug_as_display!(CssStyleFontSizeParseError<'a>);
impl_display! { CssStyleFontSizeParseError<'a>, {
    PixelValue(e) => format!("Invalid font-size: {}", e),
}}
impl_from! { CssPixelValueParseError<'a>, CssStyleFontSizeParseError::PixelValue }

#[derive(Debug, Clone, PartialEq)]
pub enum CssStyleFontSizeParseErrorOwned {
    PixelValue(CssPixelValueParseErrorOwned),
}
impl<'a> CssStyleFontSizeParseError<'a> {
    pub fn to_contained(&self) -> CssStyleFontSizeParseErrorOwned {
        match self {
            Self::PixelValue(e) => CssStyleFontSizeParseErrorOwned::PixelValue(e.to_contained()),
        }
    }
}
impl CssStyleFontSizeParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> CssStyleFontSizeParseError<'a> {
        match self {
            Self::PixelValue(e) => CssStyleFontSizeParseError::PixelValue(e.to_shared()),
        }
    }
}

#[cfg(feature = "parser")]
pub fn parse_style_font_size<'a>(
    input: &'a str,
) -> Result<StyleFontSize, CssStyleFontSizeParseError<'a>> {
    Ok(StyleFontSize {
        inner: parse_pixel_value(input)?,
    })
}

// -- Font Family Parser --

#[derive(Debug, PartialEq, Clone)]
pub enum CssStyleFontFamilyParseError<'a> {
    InvalidStyleFontFamily(&'a str),
    UnclosedQuotes(UnclosedQuotesError<'a>),
}
impl_debug_as_display!(CssStyleFontFamilyParseError<'a>);
impl_display! { CssStyleFontFamilyParseError<'a>, {
    InvalidStyleFontFamily(val) => format!("Invalid font-family: \"{}\"", val),
    UnclosedQuotes(val) => format!("Unclosed quotes in font-family: \"{}\"", val.0),
}}
impl<'a> From<UnclosedQuotesError<'a>> for CssStyleFontFamilyParseError<'a> {
    fn from(err: UnclosedQuotesError<'a>) -> Self {
        CssStyleFontFamilyParseError::UnclosedQuotes(err)
    }
}

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
            CssStyleFontFamilyParseError::UnclosedQuotes(e) => {
                CssStyleFontFamilyParseErrorOwned::UnclosedQuotes(e.0.to_string())
            }
        }
    }
}
impl CssStyleFontFamilyParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> CssStyleFontFamilyParseError<'a> {
        match self {
            CssStyleFontFamilyParseErrorOwned::InvalidStyleFontFamily(s) => {
                CssStyleFontFamilyParseError::InvalidStyleFontFamily(s)
            }
            CssStyleFontFamilyParseErrorOwned::UnclosedQuotes(s) => {
                CssStyleFontFamilyParseError::UnclosedQuotes(UnclosedQuotesError(s))
            }
        }
    }
}

#[cfg(feature = "parser")]
pub fn parse_style_font_family<'a>(
    input: &'a str,
) -> Result<StyleFontFamilyVec, CssStyleFontFamilyParseError<'a>> {
    let multiple_fonts = input.split(',');
    let mut fonts = Vec::with_capacity(1);

    for font in multiple_fonts {
        let font = font.trim();
        if let Ok(stripped) = strip_quotes(font) {
            fonts.push(StyleFontFamily::System(stripped.0.to_string().into()));
        } else {
            // It could be an unquoted font name like `Times New Roman`.
            fonts.push(StyleFontFamily::System(font.to_string().into()));
        }
    }

    Ok(fonts.into())
}
