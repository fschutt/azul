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
use crate::props::basic::parse::{strip_quotes, UnclosedQuotesError};
use crate::system::SystemFontType;
use crate::{
    corety::{AzString, U8Vec},
    format_rust_code::{FormatAsRustCode, GetHash},
    props::{
        basic::{
            error::{InvalidValueErr, InvalidValueErrOwned},
            pixel::{
                parse_pixel_value, CssPixelValueParseError, CssPixelValueParseErrorOwned,
                PixelValue,
            },
        },
        formatter::PrintAsCssValue,
    },
};

// --- Font Weight ---

/// Represents the `font-weight` property.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum StyleFontWeight {
    Lighter,
    W100,
    W200,
    W300,
    Normal,
    W500,
    W600,
    Bold,
    W800,
    W900,
    Bolder,
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
            StyleFontWeight::W100 => "100".to_string(),
            StyleFontWeight::W200 => "200".to_string(),
            StyleFontWeight::W300 => "300".to_string(),
            StyleFontWeight::Normal => "normal".to_string(),
            StyleFontWeight::W500 => "500".to_string(),
            StyleFontWeight::W600 => "600".to_string(),
            StyleFontWeight::Bold => "bold".to_string(),
            StyleFontWeight::W800 => "800".to_string(),
            StyleFontWeight::W900 => "900".to_string(),
            StyleFontWeight::Bolder => "bolder".to_string(),
        }
    }
}

impl crate::format_rust_code::FormatAsRustCode for StyleFontWeight {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        use StyleFontWeight::*;
        format!(
            "StyleFontWeight::{}",
            match self {
                Lighter => "Lighter",
                W100 => "W100",
                W200 => "W200",
                W300 => "W300",
                Normal => "Normal",
                W500 => "W500",
                W600 => "W600",
                Bold => "Bold",
                W800 => "W800",
                W900 => "W900",
                Bolder => "Bolder",
            }
        )
    }
}

impl StyleFontWeight {
    /// Convert to fontconfig weight value for font selection
    pub const fn to_fc_weight(self) -> i32 {
        match self {
            StyleFontWeight::Lighter => 50, // FC_WEIGHT_LIGHT
            StyleFontWeight::W100 => 0,     // FC_WEIGHT_THIN
            StyleFontWeight::W200 => 40,    // FC_WEIGHT_EXTRALIGHT
            StyleFontWeight::W300 => 50,    // FC_WEIGHT_LIGHT
            StyleFontWeight::Normal => 80,  // FC_WEIGHT_REGULAR / FC_WEIGHT_NORMAL
            StyleFontWeight::W500 => 100,   // FC_WEIGHT_MEDIUM
            StyleFontWeight::W600 => 180,   // FC_WEIGHT_SEMIBOLD
            StyleFontWeight::Bold => 200,   // FC_WEIGHT_BOLD
            StyleFontWeight::W800 => 205,   // FC_WEIGHT_EXTRABOLD
            StyleFontWeight::W900 => 210,   // FC_WEIGHT_BLACK / FC_WEIGHT_HEAVY
            StyleFontWeight::Bolder => 215, // Slightly heavier than W900
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

impl crate::format_rust_code::FormatAsRustCode for StyleFontStyle {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        use StyleFontStyle::*;
        format!(
            "StyleFontStyle::{}",
            match self {
                Normal => "Normal",
                Italic => "Italic",
                Oblique => "Oblique",
            }
        )
    }
}

// --- Font Size ---

/// Represents a `font-size` attribute
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
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

/// Callback type for FontRef destructor - must be extern "C" for FFI safety
pub type FontRefDestructorCallbackType = extern "C" fn(*mut c_void);

/// FontRef is a reference-counted pointer to a parsed font.
/// It holds a *const c_void that points to the actual parsed font data
/// (typically a ParsedFont from the layout crate).
///
/// The parsed data is managed via atomic reference counting, allowing
/// safe sharing across threads without duplicating the font data.
#[repr(C)]
pub struct FontRef {
    /// Pointer to the parsed font data (e.g., ParsedFont)
    pub parsed: *const c_void,
    /// Reference counter for memory management
    pub copies: *const AtomicUsize,
    /// Whether to run the destructor on drop
    pub run_destructor: bool,
    /// Destructor function for the parsed data
    pub parsed_destructor: FontRefDestructorCallbackType,
}

impl fmt::Debug for FontRef {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "FontRef(0x{:x}", self.parsed as usize)?;
        if let Some(c) = unsafe { self.copies.as_ref() } {
            write!(f, ", copies: {})", c.load(AtomicOrdering::SeqCst))?;
        } else {
            write!(f, ")")?;
        }
        Ok(())
    }
}

impl FontRef {
    /// Create a new FontRef from parsed font data
    ///
    /// # Arguments
    /// * `parsed` - Pointer to parsed font data (e.g., Arc::into_raw(Arc::new(ParsedFont)))
    /// * `destructor` - Function to clean up the parsed data
    pub fn new(parsed: *const c_void, destructor: FontRefDestructorCallbackType) -> Self {
        Self {
            parsed,
            copies: Box::into_raw(Box::new(AtomicUsize::new(1))),
            run_destructor: true,
            parsed_destructor: destructor,
        }
    }

    /// Get a raw pointer to the parsed font data
    #[inline]
    pub fn get_parsed(&self) -> *const c_void {
        self.parsed
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
        self.parsed as usize == rhs.parsed as usize
    }
}
impl PartialOrd for FontRef {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some((self.parsed as usize).cmp(&(other.parsed as usize)))
    }
}
impl Ord for FontRef {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.parsed as usize).cmp(&(other.parsed as usize))
    }
}
impl Eq for FontRef {}
impl Hash for FontRef {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (self.parsed as usize).hash(state);
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
            parsed: self.parsed,
            copies: self.copies,
            run_destructor: true,
            parsed_destructor: self.parsed_destructor,
        }
    }
}
impl Drop for FontRef {
    fn drop(&mut self) {
        if self.run_destructor && !self.copies.is_null() {
            if unsafe { (*self.copies).fetch_sub(1, AtomicOrdering::SeqCst) } == 1 {
                unsafe {
                    (self.parsed_destructor)(self.parsed as *mut c_void);
                    let _ = Box::from_raw(self.copies as *mut AtomicUsize);
                }
            }
        }
    }
}

// --- Font Family ---

/// Represents a `font-family` attribute.
/// 
/// Can be:
/// - `System(AzString)`: A named font family (e.g., "Arial", "Times New Roman")
/// - `SystemType(SystemFontType)`: A semantic system font type (e.g., `system:ui`, `system:monospace`)
/// - `File(AzString)`: A font loaded from a file URL
/// - `Ref(FontRef)`: A reference to a pre-loaded font
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
pub enum StyleFontFamily {
    /// Named font family (e.g., "Arial", "Times New Roman", "monospace")
    System(AzString),
    /// Semantic system font type (e.g., `system:ui`, `system:monospace:bold`)
    /// Resolved at runtime based on platform and accessibility settings
    SystemType(SystemFontType),
    /// Font loaded from a file URL
    File(AzString),
    /// Reference to a pre-loaded font
    Ref(FontRef),
}

impl_option!(
    StyleFontFamily,
    OptionStyleFontFamily,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

impl StyleFontFamily {
    pub fn as_string(&self) -> String {
        match &self {
            StyleFontFamily::System(s) => {
                let owned = s.clone().into_library_owned_string();
                if owned.contains(char::is_whitespace) {
                    format!("\"{}\"", owned)
                } else {
                    owned
                }
            }
            StyleFontFamily::SystemType(st) => st.as_css_str().to_string(),
            StyleFontFamily::File(s) => format!("url({})", s.clone().into_library_owned_string()),
            StyleFontFamily::Ref(s) => format!("font-ref(0x{:x})", s.parsed as usize),
        }
    }
}

impl_vec!(StyleFontFamily, StyleFontFamilyVec, StyleFontFamilyVecDestructor, StyleFontFamilyVecDestructorType, StyleFontFamilyVecSlice, OptionStyleFontFamily);
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

// Formatting to Rust code for StyleFontFamilyVec
impl crate::format_rust_code::FormatAsRustCode for StyleFontFamilyVec {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!(
            "StyleFontFamilyVec::from_const_slice(STYLE_FONT_FAMILY_{}_ITEMS)",
            self.get_hash()
        )
    }
}

// --- PARSERS ---

// -- Font Weight Parser --

#[derive(Clone, PartialEq)]
pub enum CssFontWeightParseError<'a> {
    InvalidValue(InvalidValueErr<'a>),
    InvalidNumber(ParseIntError),
}

// Formatting to Rust code for StyleFontFamily
impl crate::format_rust_code::FormatAsRustCode for StyleFontFamily {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        match self {
            StyleFontFamily::System(id) => {
                format!("StyleFontFamily::System(STRING_{})", id.get_hash())
            }
            StyleFontFamily::SystemType(st) => {
                format!("StyleFontFamily::SystemType(SystemFontType::{:?})", st)
            }
            StyleFontFamily::File(path) => {
                format!("StyleFontFamily::File(STRING_{})", path.get_hash())
            }
            StyleFontFamily::Ref(font_ref) => {
                format!("StyleFontFamily::Ref({:0x})", font_ref.parsed as usize)
            }
        }
    }
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
#[repr(C, u8)]
pub enum CssFontWeightParseErrorOwned {
    InvalidValue(InvalidValueErrOwned),
    InvalidNumber(crate::props::basic::error::ParseIntError),
}

impl<'a> CssFontWeightParseError<'a> {
    pub fn to_contained(&self) -> CssFontWeightParseErrorOwned {
        match self {
            Self::InvalidValue(e) => CssFontWeightParseErrorOwned::InvalidValue(e.to_contained()),
            Self::InvalidNumber(e) => CssFontWeightParseErrorOwned::InvalidNumber(e.clone().into()),
        }
    }
}

impl CssFontWeightParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> CssFontWeightParseError<'a> {
        match self {
            Self::InvalidValue(e) => CssFontWeightParseError::InvalidValue(e.to_shared()),
            Self::InvalidNumber(e) => CssFontWeightParseError::InvalidNumber(e.to_std()),
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
        "400" => Ok(StyleFontWeight::Normal),
        "500" => Ok(StyleFontWeight::W500),
        "600" => Ok(StyleFontWeight::W600),
        "700" => Ok(StyleFontWeight::Bold),
        "800" => Ok(StyleFontWeight::W800),
        "900" => Ok(StyleFontWeight::W900),
        _ => Err(InvalidValueErr(input).into()),
    }
}

// -- Font Style Parser --

#[derive(Clone, PartialEq)]
pub enum CssFontStyleParseError<'a> {
    InvalidValue(InvalidValueErr<'a>),
}
impl_debug_as_display!(CssFontStyleParseError<'a>);
impl_display! { CssFontStyleParseError<'a>, {
    InvalidValue(e) => format!("Invalid font-style: \"{}\"", e.0),
}}
impl_from! { InvalidValueErr<'a>, CssFontStyleParseError::InvalidValue }

#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
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

#[derive(Clone, PartialEq)]
pub enum CssStyleFontSizeParseError<'a> {
    PixelValue(CssPixelValueParseError<'a>),
}
impl_debug_as_display!(CssStyleFontSizeParseError<'a>);
impl_display! { CssStyleFontSizeParseError<'a>, {
    PixelValue(e) => format!("Invalid font-size: {}", e),
}}
impl_from! { CssPixelValueParseError<'a>, CssStyleFontSizeParseError::PixelValue }

#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
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

#[derive(PartialEq, Clone)]
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
#[repr(C, u8)]
pub enum CssStyleFontFamilyParseErrorOwned {
    InvalidStyleFontFamily(AzString),
    UnclosedQuotes(AzString),
}
impl<'a> CssStyleFontFamilyParseError<'a> {
    pub fn to_contained(&self) -> CssStyleFontFamilyParseErrorOwned {
        match self {
            CssStyleFontFamilyParseError::InvalidStyleFontFamily(s) => {
                CssStyleFontFamilyParseErrorOwned::InvalidStyleFontFamily(s.to_string().into())
            }
            CssStyleFontFamilyParseError::UnclosedQuotes(e) => {
                CssStyleFontFamilyParseErrorOwned::UnclosedQuotes(e.0.to_string().into())
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
        
        // Check for system font type syntax: system:ui, system:monospace:bold, etc.
        if font.starts_with("system:") {
            if let Some(system_type) = SystemFontType::from_css_str(font) {
                fonts.push(StyleFontFamily::SystemType(system_type));
                continue;
            }
            // Invalid system font type, fall through to treat as regular font name
        }
        
        if let Ok(stripped) = strip_quotes(font) {
            fonts.push(StyleFontFamily::System(stripped.0.to_string().into()));
        } else {
            // It could be an unquoted font name like `Times New Roman`.
            fonts.push(StyleFontFamily::System(font.to_string().into()));
        }
    }

    Ok(fonts.into())
}

// --- Font Metrics ---

use crate::corety::{OptionI16, OptionU16, OptionU32};

/// PANOSE classification values for font identification (10 bytes).
/// See https://learn.microsoft.com/en-us/typography/opentype/spec/os2#panose
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct Panose {
    pub family_type: u8,
    pub serif_style: u8,
    pub weight: u8,
    pub proportion: u8,
    pub contrast: u8,
    pub stroke_variation: u8,
    pub arm_style: u8,
    pub letterform: u8,
    pub midline: u8,
    pub x_height: u8,
}

impl Default for Panose {
    fn default() -> Self {
        Panose {
            family_type: 0,
            serif_style: 0,
            weight: 0,
            proportion: 0,
            contrast: 0,
            stroke_variation: 0,
            arm_style: 0,
            letterform: 0,
            midline: 0,
            x_height: 0,
        }
    }
}

impl Panose {
    pub const fn zero() -> Self {
        Panose {
            family_type: 0,
            serif_style: 0,
            weight: 0,
            proportion: 0,
            contrast: 0,
            stroke_variation: 0,
            arm_style: 0,
            letterform: 0,
            midline: 0,
            x_height: 0,
        }
    }

    /// Create from a 10-byte array
    pub const fn from_array(arr: [u8; 10]) -> Self {
        Panose {
            family_type: arr[0],
            serif_style: arr[1],
            weight: arr[2],
            proportion: arr[3],
            contrast: arr[4],
            stroke_variation: arr[5],
            arm_style: arr[6],
            letterform: arr[7],
            midline: arr[8],
            x_height: arr[9],
        }
    }

    /// Convert to a 10-byte array
    pub const fn to_array(&self) -> [u8; 10] {
        [
            self.family_type,
            self.serif_style,
            self.weight,
            self.proportion,
            self.contrast,
            self.stroke_variation,
            self.arm_style,
            self.letterform,
            self.midline,
            self.x_height,
        ]
    }
}

/// Font metrics structure containing all font-related measurements from
/// the font file tables (head, hhea, and os/2 tables).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct FontMetrics {
    // head table
    pub units_per_em: u16,
    pub font_flags: u16,
    pub x_min: i16,
    pub y_min: i16,
    pub x_max: i16,
    pub y_max: i16,

    // hhea table
    pub ascender: i16,
    pub descender: i16,
    pub line_gap: i16,
    pub advance_width_max: u16,
    pub min_left_side_bearing: i16,
    pub min_right_side_bearing: i16,
    pub x_max_extent: i16,
    pub caret_slope_rise: i16,
    pub caret_slope_run: i16,
    pub caret_offset: i16,
    pub num_h_metrics: u16,

    // os/2 table
    pub x_avg_char_width: i16,
    pub us_weight_class: u16,
    pub us_width_class: u16,
    pub fs_type: u16,
    pub y_subscript_x_size: i16,
    pub y_subscript_y_size: i16,
    pub y_subscript_x_offset: i16,
    pub y_subscript_y_offset: i16,
    pub y_superscript_x_size: i16,
    pub y_superscript_y_size: i16,
    pub y_superscript_x_offset: i16,
    pub y_superscript_y_offset: i16,
    pub y_strikeout_size: i16,
    pub y_strikeout_position: i16,
    pub s_family_class: i16,
    pub panose: Panose,
    pub ul_unicode_range1: u32,
    pub ul_unicode_range2: u32,
    pub ul_unicode_range3: u32,
    pub ul_unicode_range4: u32,
    pub ach_vend_id: u32,
    pub fs_selection: u16,
    pub us_first_char_index: u16,
    pub us_last_char_index: u16,

    // os/2 version 0 table
    pub s_typo_ascender: OptionI16,
    pub s_typo_descender: OptionI16,
    pub s_typo_line_gap: OptionI16,
    pub us_win_ascent: OptionU16,
    pub us_win_descent: OptionU16,

    // os/2 version 1 table
    pub ul_code_page_range1: OptionU32,
    pub ul_code_page_range2: OptionU32,

    // os/2 version 2 table
    pub sx_height: OptionI16,
    pub s_cap_height: OptionI16,
    pub us_default_char: OptionU16,
    pub us_break_char: OptionU16,
    pub us_max_context: OptionU16,

    // os/2 version 3 table
    pub us_lower_optical_point_size: OptionU16,
    pub us_upper_optical_point_size: OptionU16,
}

impl Default for FontMetrics {
    fn default() -> Self {
        FontMetrics::zero()
    }
}

impl FontMetrics {
    /// Only for testing, zero-sized font, will always return 0 for every metric
    /// (`units_per_em = 1000`)
    pub const fn zero() -> Self {
        FontMetrics {
            units_per_em: 1000,
            font_flags: 0,
            x_min: 0,
            y_min: 0,
            x_max: 0,
            y_max: 0,
            ascender: 0,
            descender: 0,
            line_gap: 0,
            advance_width_max: 0,
            min_left_side_bearing: 0,
            min_right_side_bearing: 0,
            x_max_extent: 0,
            caret_slope_rise: 0,
            caret_slope_run: 0,
            caret_offset: 0,
            num_h_metrics: 0,
            x_avg_char_width: 0,
            us_weight_class: 400,
            us_width_class: 5,
            fs_type: 0,
            y_subscript_x_size: 0,
            y_subscript_y_size: 0,
            y_subscript_x_offset: 0,
            y_subscript_y_offset: 0,
            y_superscript_x_size: 0,
            y_superscript_y_size: 0,
            y_superscript_x_offset: 0,
            y_superscript_y_offset: 0,
            y_strikeout_size: 0,
            y_strikeout_position: 0,
            s_family_class: 0,
            panose: Panose::zero(),
            ul_unicode_range1: 0,
            ul_unicode_range2: 0,
            ul_unicode_range3: 0,
            ul_unicode_range4: 0,
            ach_vend_id: 0,
            fs_selection: 0,
            us_first_char_index: 0,
            us_last_char_index: 0,
            s_typo_ascender: OptionI16::None,
            s_typo_descender: OptionI16::None,
            s_typo_line_gap: OptionI16::None,
            us_win_ascent: OptionU16::None,
            us_win_descent: OptionU16::None,
            ul_code_page_range1: OptionU32::None,
            ul_code_page_range2: OptionU32::None,
            sx_height: OptionI16::None,
            s_cap_height: OptionI16::None,
            us_default_char: OptionU16::None,
            us_break_char: OptionU16::None,
            us_max_context: OptionU16::None,
            us_lower_optical_point_size: OptionU16::None,
            us_upper_optical_point_size: OptionU16::None,
        }
    }

    /// Returns the ascender value from the hhea table
    pub fn get_ascender(&self) -> i16 {
        self.ascender
    }

    /// Returns the descender value from the hhea table
    pub fn get_descender(&self) -> i16 {
        self.descender
    }

    /// Returns the line gap value from the hhea table
    pub fn get_line_gap(&self) -> i16 {
        self.line_gap
    }

    /// Returns the maximum advance width from the hhea table
    pub fn get_advance_width_max(&self) -> u16 {
        self.advance_width_max
    }

    /// Returns the minimum left side bearing from the hhea table
    pub fn get_min_left_side_bearing(&self) -> i16 {
        self.min_left_side_bearing
    }

    /// Returns the minimum right side bearing from the hhea table
    pub fn get_min_right_side_bearing(&self) -> i16 {
        self.min_right_side_bearing
    }

    /// Returns the x_min value from the head table
    pub fn get_x_min(&self) -> i16 {
        self.x_min
    }

    /// Returns the y_min value from the head table
    pub fn get_y_min(&self) -> i16 {
        self.y_min
    }

    /// Returns the x_max value from the head table
    pub fn get_x_max(&self) -> i16 {
        self.x_max
    }

    /// Returns the y_max value from the head table
    pub fn get_y_max(&self) -> i16 {
        self.y_max
    }

    /// Returns the maximum extent in the x direction from the hhea table
    pub fn get_x_max_extent(&self) -> i16 {
        self.x_max_extent
    }

    /// Returns the average character width from the os/2 table
    pub fn get_x_avg_char_width(&self) -> i16 {
        self.x_avg_char_width
    }

    /// Returns the subscript x size from the os/2 table
    pub fn get_y_subscript_x_size(&self) -> i16 {
        self.y_subscript_x_size
    }

    /// Returns the subscript y size from the os/2 table
    pub fn get_y_subscript_y_size(&self) -> i16 {
        self.y_subscript_y_size
    }

    /// Returns the subscript x offset from the os/2 table
    pub fn get_y_subscript_x_offset(&self) -> i16 {
        self.y_subscript_x_offset
    }

    /// Returns the subscript y offset from the os/2 table
    pub fn get_y_subscript_y_offset(&self) -> i16 {
        self.y_subscript_y_offset
    }

    /// Returns the superscript x size from the os/2 table
    pub fn get_y_superscript_x_size(&self) -> i16 {
        self.y_superscript_x_size
    }

    /// Returns the superscript y size from the os/2 table
    pub fn get_y_superscript_y_size(&self) -> i16 {
        self.y_superscript_y_size
    }

    /// Returns the superscript x offset from the os/2 table
    pub fn get_y_superscript_x_offset(&self) -> i16 {
        self.y_superscript_x_offset
    }

    /// Returns the superscript y offset from the os/2 table
    pub fn get_y_superscript_y_offset(&self) -> i16 {
        self.y_superscript_y_offset
    }

    /// Returns the strikeout size from the os/2 table
    pub fn get_y_strikeout_size(&self) -> i16 {
        self.y_strikeout_size
    }

    /// Returns the strikeout position from the os/2 table
    pub fn get_y_strikeout_position(&self) -> i16 {
        self.y_strikeout_position
    }

    /// Returns whether typographic metrics should be used (from fs_selection flag)
    pub fn use_typo_metrics(&self) -> bool {
        // Bit 7 of fs_selection indicates USE_TYPO_METRICS
        (self.fs_selection & 0x0080) != 0
    }
}

#[cfg(all(test, feature = "parser"))]
mod tests {
    use super::*;

    #[test]
    fn test_parse_font_weight_keywords() {
        assert_eq!(
            parse_font_weight("normal").unwrap(),
            StyleFontWeight::Normal
        );
        assert_eq!(parse_font_weight("bold").unwrap(), StyleFontWeight::Bold);
        assert_eq!(
            parse_font_weight("lighter").unwrap(),
            StyleFontWeight::Lighter
        );
        assert_eq!(
            parse_font_weight("bolder").unwrap(),
            StyleFontWeight::Bolder
        );
    }

    #[test]
    fn test_parse_font_weight_numbers() {
        assert_eq!(parse_font_weight("100").unwrap(), StyleFontWeight::W100);
        assert_eq!(parse_font_weight("400").unwrap(), StyleFontWeight::Normal);
        assert_eq!(parse_font_weight("700").unwrap(), StyleFontWeight::Bold);
        assert_eq!(parse_font_weight("900").unwrap(), StyleFontWeight::W900);
    }

    #[test]
    fn test_parse_font_weight_invalid() {
        assert!(parse_font_weight("thin").is_err());
        assert!(parse_font_weight("").is_err());
        assert!(parse_font_weight("450").is_err());
        assert!(parse_font_weight("boldest").is_err());
    }

    #[test]
    fn test_parse_font_style() {
        assert_eq!(parse_font_style("normal").unwrap(), StyleFontStyle::Normal);
        assert_eq!(parse_font_style("italic").unwrap(), StyleFontStyle::Italic);
        assert_eq!(
            parse_font_style("oblique").unwrap(),
            StyleFontStyle::Oblique
        );
        assert_eq!(
            parse_font_style("  italic  ").unwrap(),
            StyleFontStyle::Italic
        );
        assert!(parse_font_style("slanted").is_err());
    }

    #[test]
    fn test_parse_font_size() {
        assert_eq!(
            parse_style_font_size("16px").unwrap().inner,
            PixelValue::px(16.0)
        );
        assert_eq!(
            parse_style_font_size("1.2em").unwrap().inner,
            PixelValue::em(1.2)
        );
        assert_eq!(
            parse_style_font_size("12pt").unwrap().inner,
            PixelValue::pt(12.0)
        );
        assert_eq!(
            parse_style_font_size("120%").unwrap().inner,
            PixelValue::percent(120.0)
        );
        assert!(parse_style_font_size("medium").is_err());
    }

    #[test]
    fn test_parse_font_family() {
        // Single unquoted
        let result = parse_style_font_family("Arial").unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(
            result.as_slice()[0],
            StyleFontFamily::System("Arial".into())
        );

        // Single quoted
        let result = parse_style_font_family("\"Times New Roman\"").unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(
            result.as_slice()[0],
            StyleFontFamily::System("Times New Roman".into())
        );

        // Multiple
        let result = parse_style_font_family("Georgia, serif").unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(
            result.as_slice()[0],
            StyleFontFamily::System("Georgia".into())
        );
        assert_eq!(
            result.as_slice()[1],
            StyleFontFamily::System("serif".into())
        );

        // Multiple with quotes and extra whitespace
        let result = parse_style_font_family("  'Courier New'  , monospace  ").unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(
            result.as_slice()[0],
            StyleFontFamily::System("Courier New".into())
        );
        assert_eq!(
            result.as_slice()[1],
            StyleFontFamily::System("monospace".into())
        );
    }
    
    #[test]
    fn test_parse_system_font_type() {
        use crate::system::SystemFontType;
        
        // Single system font type
        let result = parse_style_font_family("system:ui").unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result.as_slice()[0], StyleFontFamily::SystemType(SystemFontType::Ui));
        
        // System font type with bold variant
        let result = parse_style_font_family("system:monospace:bold").unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result.as_slice()[0], StyleFontFamily::SystemType(SystemFontType::MonospaceBold));
        
        // System font type with italic variant
        let result = parse_style_font_family("system:monospace:italic").unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result.as_slice()[0], StyleFontFamily::SystemType(SystemFontType::MonospaceItalic));
        
        // System font type with fallback
        let result = parse_style_font_family("system:ui, Arial, sans-serif").unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result.as_slice()[0], StyleFontFamily::SystemType(SystemFontType::Ui));
        assert_eq!(result.as_slice()[1], StyleFontFamily::System("Arial".into()));
        assert_eq!(result.as_slice()[2], StyleFontFamily::System("sans-serif".into()));
        
        // All system font types
        assert!(parse_style_font_family("system:ui").is_ok());
        assert!(parse_style_font_family("system:ui:bold").is_ok());
        assert!(parse_style_font_family("system:monospace").is_ok());
        assert!(parse_style_font_family("system:monospace:bold").is_ok());
        assert!(parse_style_font_family("system:monospace:italic").is_ok());
        assert!(parse_style_font_family("system:title").is_ok());
        assert!(parse_style_font_family("system:title:bold").is_ok());
        assert!(parse_style_font_family("system:menu").is_ok());
        assert!(parse_style_font_family("system:small").is_ok());
        assert!(parse_style_font_family("system:serif").is_ok());
        assert!(parse_style_font_family("system:serif:bold").is_ok());
        
        // Invalid system font type should be parsed as regular font name
        let result = parse_style_font_family("system:invalid").unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result.as_slice()[0], StyleFontFamily::System("system:invalid".into()));
    }
    
    #[test]
    fn test_system_font_type_css_roundtrip() {
        use crate::system::SystemFontType;
        
        // Test that as_css_str() and from_css_str() are inverses
        let types = [
            SystemFontType::Ui,
            SystemFontType::UiBold,
            SystemFontType::Monospace,
            SystemFontType::MonospaceBold,
            SystemFontType::MonospaceItalic,
            SystemFontType::Title,
            SystemFontType::TitleBold,
            SystemFontType::Menu,
            SystemFontType::Small,
            SystemFontType::Serif,
            SystemFontType::SerifBold,
        ];
        
        for ft in &types {
            let css = ft.as_css_str();
            let parsed = SystemFontType::from_css_str(css).unwrap();
            assert_eq!(*ft, parsed, "Roundtrip failed for {:?}", ft);
        }
    }
}
