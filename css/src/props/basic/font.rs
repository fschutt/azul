//! CSS properties for fonts, such as font-family, font-size, font-weight, and font-style.
//!
//! Also contains `FontRef` (reference-counted handle to parsed font data),
//! `FontMetrics` (OpenType font metrics from head/hhea/os2 tables), and
//! `Panose` (font classification).

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
    sync::atomic::{AtomicU64, AtomicUsize, Ordering as AtomicOrdering},
};

#[cfg(feature = "parser")]
use crate::props::basic::parse::{strip_quotes, UnclosedQuotesError};
use crate::system::SystemFontType;
use crate::{
    corety::{AzString, U8Vec},
    codegen::format::{FormatAsRustCode, GetHash},
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
#[derive(Default)]
pub enum StyleFontWeight {
    Lighter,
    W100,
    W200,
    W300,
    #[default]
    Normal,
    W500,
    W600,
    Bold,
    W800,
    W900,
    Bolder,
}


impl PrintAsCssValue for StyleFontWeight {
    fn print_as_css_value(&self) -> String {
        match self {
            Self::Lighter => "lighter".to_string(),
            Self::W100 => "100".to_string(),
            Self::W200 => "200".to_string(),
            Self::W300 => "300".to_string(),
            Self::Normal => "normal".to_string(),
            Self::W500 => "500".to_string(),
            Self::W600 => "600".to_string(),
            Self::Bold => "bold".to_string(),
            Self::W800 => "800".to_string(),
            Self::W900 => "900".to_string(),
            Self::Bolder => "bolder".to_string(),
        }
    }
}

impl crate::codegen::format::FormatAsRustCode for StyleFontWeight {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        use StyleFontWeight::{Lighter, W100, W200, W300, Normal, W500, W600, Bold, W800, W900, Bolder};
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

// --- Font Style ---

/// Represents the `font-style` property.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
#[derive(Default)]
pub enum StyleFontStyle {
    #[default]
    Normal,
    Italic,
    Oblique,
}


impl PrintAsCssValue for StyleFontStyle {
    fn print_as_css_value(&self) -> String {
        match self {
            Self::Normal => "normal".to_string(),
            Self::Italic => "italic".to_string(),
            Self::Oblique => "oblique".to_string(),
        }
    }
}

impl crate::codegen::format::FormatAsRustCode for StyleFontStyle {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        use StyleFontStyle::{Normal, Italic, Oblique};
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

/// Callback type for `FontRef` destructor - must be extern "C" for FFI safety
pub type FontRefDestructorCallbackType = extern "C" fn(*mut c_void);

/// `FontRef` is a reference-counted pointer to a parsed font.
/// It holds a *const `c_void` that points to the actual parsed font data
/// (typically a `ParsedFont` from the layout crate).
///
/// The parsed data is managed via atomic reference counting, allowing
/// safe sharing across threads without duplicating the font data.
#[repr(C)]
pub struct FontRef {
    /// Pointer to the parsed font data (e.g., `ParsedFont`)
    pub parsed: *const c_void,
    /// Reference counter for memory management
    pub copies: *const AtomicUsize,
    /// Process-unique, monotonically-assigned identity of this parsed font.
    /// Shared by shallow clones (same font), fresh for each `new`. Used for
    /// `Eq`/`Ord`/`Hash` instead of the `parsed` pointer so that freeing a
    /// font and reusing its heap address can't forge identity — the same
    /// aliasing fix applied to `ImageRef`. (Content-level dedup still uses the
    /// separate content hash via `font_ref_get_hash`.)
    pub id: u64,
    /// Whether to run the destructor on drop
    pub run_destructor: bool,
    /// Destructor function for the parsed data
    pub parsed_destructor: FontRefDestructorCallbackType,
}

/// Never-reused source of [`FontRef::id`]. Starts at 1 so `id == 0` can flag
/// an un-initialised / raw-reconstructed handle.
static FONT_REF_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

#[must_use]
fn next_font_ref_id() -> u64 {
    FONT_REF_ID_COUNTER.fetch_add(1, AtomicOrdering::SeqCst)
}

impl fmt::Debug for FontRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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
    /// Create a new `FontRef` from parsed font data
    ///
    /// # Arguments
    /// * `parsed` - Pointer to parsed font data (e.g., `Arc::into_raw(Arc::new(ParsedFont))`)
    /// * `destructor` - Function to clean up the parsed data
    pub fn new(parsed: *const c_void, destructor: FontRefDestructorCallbackType) -> Self {
        Self {
            parsed,
            copies: Box::into_raw(Box::new(AtomicUsize::new(1))),
            id: next_font_ref_id(),
            run_destructor: true,
            parsed_destructor: destructor,
        }
    }

    /// Get a raw pointer to the parsed font data
    #[inline]
    #[must_use] pub const fn get_parsed(&self) -> *const c_void {
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
// Identity is the never-reused `id`, NOT the `parsed` pointer (which is freed
// when the last ref drops and whose address may be reused by a later font).
impl PartialEq for FontRef {
    fn eq(&self, rhs: &Self) -> bool {
        self.id == rhs.id
    }
}
impl PartialOrd for FontRef {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.id.cmp(&other.id))
    }
}
impl Ord for FontRef {
    fn cmp(&self, other: &Self) -> Ordering {
        self.id.cmp(&other.id)
    }
}
impl Eq for FontRef {}
impl Hash for FontRef {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
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
            id: self.id, // same font → same identity
            run_destructor: self.run_destructor,
            parsed_destructor: self.parsed_destructor,
        }
    }
}
impl Drop for FontRef {
    fn drop(&mut self) {
        if self.run_destructor && !self.copies.is_null()
            && unsafe { (*self.copies).fetch_sub(1, AtomicOrdering::SeqCst) } == 1 {
                unsafe {
                    (self.parsed_destructor)(self.parsed.cast_mut());
                    drop(Box::from_raw(self.copies.cast_mut()));
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
            Self::System(s) => {
                let owned = s.clone().into_library_owned_string();
                if owned.contains(char::is_whitespace) {
                    format!("\"{owned}\"")
                } else {
                    owned
                }
            }
            Self::SystemType(st) => st.as_css_str().to_string(),
            Self::File(s) => format!("url({})", s.clone().into_library_owned_string()),
            Self::Ref(s) => format!("font-ref(0x{:x})", s.parsed as usize),
        }
    }

    /// The RAW family name, for querying the font backend (fontconfig). Unlike
    /// `as_string()` this does NOT apply CSS serialization — a multi-word name comes
    /// back as `Times New Roman`, not `"Times New Roman"`, since the backend matches on
    /// the bare name and the quotes would corrupt the query.
    pub fn as_query_string(&self) -> String {
        match &self {
            Self::System(s) | Self::File(s) => s.clone().into_library_owned_string(),
            Self::SystemType(st) => st.as_css_str().to_string(),
            Self::Ref(s) => format!("font-ref(0x{:x})", s.parsed as usize),
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
            .map(StyleFontFamily::as_string)
            .collect::<Vec<_>>()
            .join(", ")
    }
}

// Formatting to Rust code for StyleFontFamilyVec
impl crate::codegen::format::FormatAsRustCode for StyleFontFamilyVec {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!(
            "StyleFontFamilyVec::from_const_slice(STYLE_FONT_FAMILY_{}_ITEMS)",
            self.get_hash()
        )
    }
}

// --- PARSERS ---

// -- Font Weight Parser --

#[derive(Clone, PartialEq, Eq)]
pub enum CssFontWeightParseError<'a> {
    InvalidValue(InvalidValueErr<'a>),
    InvalidNumber(ParseIntError),
}

// Formatting to Rust code for StyleFontFamily
impl crate::codegen::format::FormatAsRustCode for StyleFontFamily {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        match self {
            Self::System(id) => {
                format!("StyleFontFamily::System(STRING_{})", id.get_hash())
            }
            Self::SystemType(st) => {
                format!("StyleFontFamily::SystemType(SystemFontType::{st:?})")
            }
            Self::File(path) => {
                format!("StyleFontFamily::File(STRING_{})", path.get_hash())
            }
            Self::Ref(font_ref) => {
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
impl From<ParseIntError> for CssFontWeightParseError<'_> {
    fn from(e: ParseIntError) -> Self {
        CssFontWeightParseError::InvalidNumber(e)
    }
}
#[allow(variant_size_differences)] // repr(C,u8) FFI enum: boxing the large variant would change the C ABI (api.json bindings); size disparity accepted
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum CssFontWeightParseErrorOwned {
    InvalidValue(InvalidValueErrOwned),
    InvalidNumber(crate::props::basic::error::ParseIntError),
}

impl CssFontWeightParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> CssFontWeightParseErrorOwned {
        match self {
            Self::InvalidValue(e) => CssFontWeightParseErrorOwned::InvalidValue(e.to_contained()),
            Self::InvalidNumber(e) => CssFontWeightParseErrorOwned::InvalidNumber(e.clone().into()),
        }
    }
}

impl CssFontWeightParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> CssFontWeightParseError<'_> {
        match self {
            Self::InvalidValue(e) => CssFontWeightParseError::InvalidValue(e.to_shared()),
            Self::InvalidNumber(e) => CssFontWeightParseError::InvalidNumber(e.to_std()),
        }
    }
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `font-weight` value.
pub fn parse_font_weight(
    input: &str,
) -> Result<StyleFontWeight, CssFontWeightParseError<'_>> {
    let input = input.trim();
    match input {
        "lighter" => Ok(StyleFontWeight::Lighter),
        "normal" | "400" => Ok(StyleFontWeight::Normal),
        "bold" | "700" => Ok(StyleFontWeight::Bold),
        "bolder" => Ok(StyleFontWeight::Bolder),
        "100" => Ok(StyleFontWeight::W100),
        "200" => Ok(StyleFontWeight::W200),
        "300" => Ok(StyleFontWeight::W300),
        "500" => Ok(StyleFontWeight::W500),
        "600" => Ok(StyleFontWeight::W600),
        "800" => Ok(StyleFontWeight::W800),
        "900" => Ok(StyleFontWeight::W900),
        _ => Err(InvalidValueErr(input).into()),
    }
}

// -- Font Style Parser --

#[derive(Clone, PartialEq, Eq)]
pub enum CssFontStyleParseError<'a> {
    InvalidValue(InvalidValueErr<'a>),
}
impl_debug_as_display!(CssFontStyleParseError<'a>);
impl_display! { CssFontStyleParseError<'a>, {
    InvalidValue(e) => format!("Invalid font-style: \"{}\"", e.0),
}}
impl_from! { InvalidValueErr<'a>, CssFontStyleParseError::InvalidValue }

#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum CssFontStyleParseErrorOwned {
    InvalidValue(InvalidValueErrOwned),
}
impl CssFontStyleParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> CssFontStyleParseErrorOwned {
        match self {
            Self::InvalidValue(e) => CssFontStyleParseErrorOwned::InvalidValue(e.to_contained()),
        }
    }
}
impl CssFontStyleParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> CssFontStyleParseError<'_> {
        match self {
            Self::InvalidValue(e) => CssFontStyleParseError::InvalidValue(e.to_shared()),
        }
    }
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `font-style` value.
pub fn parse_font_style(input: &str) -> Result<StyleFontStyle, CssFontStyleParseError<'_>> {
    match input.trim() {
        "normal" => Ok(StyleFontStyle::Normal),
        "italic" => Ok(StyleFontStyle::Italic),
        "oblique" => Ok(StyleFontStyle::Oblique),
        other => Err(InvalidValueErr(other).into()),
    }
}

// -- Font Size Parser --

#[derive(Clone, PartialEq, Eq)]
pub enum CssStyleFontSizeParseError<'a> {
    PixelValue(CssPixelValueParseError<'a>),
}
impl_debug_as_display!(CssStyleFontSizeParseError<'a>);
impl_display! { CssStyleFontSizeParseError<'a>, {
    PixelValue(e) => format!("Invalid font-size: {}", e),
}}
impl_from! { CssPixelValueParseError<'a>, CssStyleFontSizeParseError::PixelValue }

#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum CssStyleFontSizeParseErrorOwned {
    PixelValue(CssPixelValueParseErrorOwned),
}
impl CssStyleFontSizeParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> CssStyleFontSizeParseErrorOwned {
        match self {
            Self::PixelValue(e) => CssStyleFontSizeParseErrorOwned::PixelValue(e.to_contained()),
        }
    }
}
impl CssStyleFontSizeParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> CssStyleFontSizeParseError<'_> {
        match self {
            Self::PixelValue(e) => CssStyleFontSizeParseError::PixelValue(e.to_shared()),
        }
    }
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `font-size` value.
pub fn parse_style_font_size(
    input: &str,
) -> Result<StyleFontSize, CssStyleFontSizeParseError<'_>> {
    Ok(StyleFontSize {
        inner: parse_pixel_value(input)?,
    })
}

// -- Font Family Parser --

#[derive(PartialEq, Eq, Clone)]
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

#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum CssStyleFontFamilyParseErrorOwned {
    InvalidStyleFontFamily(AzString),
    UnclosedQuotes(AzString),
}
impl CssStyleFontFamilyParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> CssStyleFontFamilyParseErrorOwned {
        match self {
            CssStyleFontFamilyParseError::InvalidStyleFontFamily(s) => {
                CssStyleFontFamilyParseErrorOwned::InvalidStyleFontFamily((*s).to_string().into())
            }
            CssStyleFontFamilyParseError::UnclosedQuotes(e) => {
                CssStyleFontFamilyParseErrorOwned::UnclosedQuotes(e.0.to_string().into())
            }
        }
    }
}
impl CssStyleFontFamilyParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> CssStyleFontFamilyParseError<'_> {
        match self {
            Self::InvalidStyleFontFamily(s) => {
                CssStyleFontFamilyParseError::InvalidStyleFontFamily(s)
            }
            Self::UnclosedQuotes(s) => {
                CssStyleFontFamilyParseError::UnclosedQuotes(UnclosedQuotesError(s))
            }
        }
    }
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `font-family` value.
pub fn parse_style_font_family(
    input: &str,
) -> Result<StyleFontFamilyVec, CssStyleFontFamilyParseError<'_>> {
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
/// See <https://learn.microsoft.com/en-us/typography/opentype/spec/os2#panose>
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
#[derive(Default)]
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


impl Panose {
    #[must_use] pub const fn zero() -> Self {
        Self {
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

/// Font metrics structure containing all font-related measurements from
/// the font file tables (head, hhea, and os/2 tables).
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct FontMetrics {
    // os/2 version 1 table (u32 fields - align 4, placed first)
    pub ul_code_page_range1: OptionU32,
    pub ul_code_page_range2: OptionU32,

    // os/2 table (u32 fields)
    pub ul_unicode_range1: u32,
    pub ul_unicode_range2: u32,
    pub ul_unicode_range3: u32,
    pub ul_unicode_range4: u32,
    pub ach_vend_id: u32,

    // os/2 version 0 table (Option<i16>/Option<u16> - align 2)
    pub s_typo_ascender: OptionI16,
    pub s_typo_descender: OptionI16,
    pub s_typo_line_gap: OptionI16,
    pub us_win_ascent: OptionU16,
    pub us_win_descent: OptionU16,

    // +spec:font-metrics:d3b654 - cap-height and x-height metrics for visual text centering (leading-trim)
    // os/2 version 2 table
    pub sx_height: OptionI16,
    pub s_cap_height: OptionI16,
    pub us_default_char: OptionU16,
    pub us_break_char: OptionU16,
    pub us_max_context: OptionU16,

    // os/2 version 3 table
    pub us_lower_optical_point_size: OptionU16,
    pub us_upper_optical_point_size: OptionU16,

    // head table (u16/i16 - align 2)
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

    // os/2 table (u16/i16 fields)
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
    pub fs_selection: u16,
    pub us_first_char_index: u16,
    pub us_last_char_index: u16,

    // panose (align 1 - last)
    pub panose: Panose,
}

impl Default for FontMetrics {
    fn default() -> Self {
        Self::zero()
    }
}

impl FontMetrics {
    /// Only for testing, zero-sized font, will always return 0 for every metric
    /// (`units_per_em = 1000`)
    #[must_use] pub const fn zero() -> Self {
        Self {
            ul_code_page_range1: OptionU32::None,
            ul_code_page_range2: OptionU32::None,
            ul_unicode_range1: 0,
            ul_unicode_range2: 0,
            ul_unicode_range3: 0,
            ul_unicode_range4: 0,
            ach_vend_id: 0,
            s_typo_ascender: OptionI16::None,
            s_typo_descender: OptionI16::None,
            s_typo_line_gap: OptionI16::None,
            us_win_ascent: OptionU16::None,
            us_win_descent: OptionU16::None,
            sx_height: OptionI16::None,
            s_cap_height: OptionI16::None,
            us_default_char: OptionU16::None,
            us_break_char: OptionU16::None,
            us_max_context: OptionU16::None,
            us_lower_optical_point_size: OptionU16::None,
            us_upper_optical_point_size: OptionU16::None,
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
            fs_selection: 0,
            us_first_char_index: 0,
            us_last_char_index: 0,
            panose: Panose::zero(),
        }
    }

    /// Returns the ascender value from the hhea table
    #[must_use] pub const fn get_ascender(&self) -> i16 {
        self.ascender
    }

    /// Returns the descender value from the hhea table
    #[must_use] pub const fn get_descender(&self) -> i16 {
        self.descender
    }

    /// Returns the line gap value from the hhea table
    #[must_use] pub const fn get_line_gap(&self) -> i16 {
        self.line_gap
    }

    /// Returns the maximum advance width from the hhea table
    #[must_use] pub const fn get_advance_width_max(&self) -> u16 {
        self.advance_width_max
    }

    /// Returns the minimum left side bearing from the hhea table
    #[must_use] pub const fn get_min_left_side_bearing(&self) -> i16 {
        self.min_left_side_bearing
    }

    /// Returns the minimum right side bearing from the hhea table
    #[must_use] pub const fn get_min_right_side_bearing(&self) -> i16 {
        self.min_right_side_bearing
    }

    /// Returns the `x_min` value from the head table
    #[must_use] pub const fn get_x_min(&self) -> i16 {
        self.x_min
    }

    /// Returns the `y_min` value from the head table
    #[must_use] pub const fn get_y_min(&self) -> i16 {
        self.y_min
    }

    /// Returns the `x_max` value from the head table
    #[must_use] pub const fn get_x_max(&self) -> i16 {
        self.x_max
    }

    /// Returns the `y_max` value from the head table
    #[must_use] pub const fn get_y_max(&self) -> i16 {
        self.y_max
    }

    /// Returns the maximum extent in the x direction from the hhea table
    #[must_use] pub const fn get_x_max_extent(&self) -> i16 {
        self.x_max_extent
    }

    /// Returns the average character width from the os/2 table
    #[must_use] pub const fn get_x_avg_char_width(&self) -> i16 {
        self.x_avg_char_width
    }

    /// Returns the subscript x size from the os/2 table
    #[must_use] pub const fn get_y_subscript_x_size(&self) -> i16 {
        self.y_subscript_x_size
    }

    /// Returns the subscript y size from the os/2 table
    #[must_use] pub const fn get_y_subscript_y_size(&self) -> i16 {
        self.y_subscript_y_size
    }

    /// Returns the subscript x offset from the os/2 table
    #[must_use] pub const fn get_y_subscript_x_offset(&self) -> i16 {
        self.y_subscript_x_offset
    }

    /// Returns the subscript y offset from the os/2 table
    #[must_use] pub const fn get_y_subscript_y_offset(&self) -> i16 {
        self.y_subscript_y_offset
    }

    /// Returns the superscript x size from the os/2 table
    #[must_use] pub const fn get_y_superscript_x_size(&self) -> i16 {
        self.y_superscript_x_size
    }

    /// Returns the superscript y size from the os/2 table
    #[must_use] pub const fn get_y_superscript_y_size(&self) -> i16 {
        self.y_superscript_y_size
    }

    /// Returns the superscript x offset from the os/2 table
    #[must_use] pub const fn get_y_superscript_x_offset(&self) -> i16 {
        self.y_superscript_x_offset
    }

    /// Returns the superscript y offset from the os/2 table
    #[must_use] pub const fn get_y_superscript_y_offset(&self) -> i16 {
        self.y_superscript_y_offset
    }

    /// Returns the strikeout size from the os/2 table
    #[must_use] pub const fn get_y_strikeout_size(&self) -> i16 {
        self.y_strikeout_size
    }

    /// Returns the strikeout position from the os/2 table
    #[must_use] pub const fn get_y_strikeout_position(&self) -> i16 {
        self.y_strikeout_position
    }

    /// Returns whether typographic metrics should be used (from `fs_selection` flag)
    #[must_use] pub const fn use_typo_metrics(&self) -> bool {
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
            assert_eq!(*ft, parsed, "Roundtrip failed for {ft:?}");
        }
    }
}

#[cfg(test)]
#[allow(clippy::too_many_lines, clippy::float_cmp)]
mod autotest_generated {
    use std::collections::hash_map::DefaultHasher;

    use super::*;
    use crate::props::basic::{
        error::ParseIntError as CParseIntError, length::SizeMetric,
    };

    fn hash_of<T: Hash>(value: &T) -> u64 {
        let mut hasher = DefaultHasher::new();
        value.hash(&mut hasher);
        hasher.finish()
    }

    /// Leaks nothing: the matching destructor below reconstructs the `Box`.
    fn boxed_font_data(value: u64) -> *const c_void {
        Box::into_raw(Box::new(value)).cast::<c_void>().cast_const()
    }

    extern "C" fn noop_destructor(_ptr: *mut c_void) {}

    // One counter per test: `cargo test` runs tests in parallel within a single
    // process, so a shared counter would race.
    static SINGLE_DTOR_CALLS: AtomicUsize = AtomicUsize::new(0);
    extern "C" fn single_counting_destructor(ptr: *mut c_void) {
        SINGLE_DTOR_CALLS.fetch_add(1, AtomicOrdering::SeqCst);
        if !ptr.is_null() {
            unsafe { drop(Box::from_raw(ptr.cast::<u64>())) };
        }
    }

    static CLONE_DTOR_CALLS: AtomicUsize = AtomicUsize::new(0);
    extern "C" fn clone_counting_destructor(ptr: *mut c_void) {
        CLONE_DTOR_CALLS.fetch_add(1, AtomicOrdering::SeqCst);
        if !ptr.is_null() {
            unsafe { drop(Box::from_raw(ptr.cast::<u64>())) };
        }
    }

    static MANY_DTOR_CALLS: AtomicUsize = AtomicUsize::new(0);
    extern "C" fn many_counting_destructor(ptr: *mut c_void) {
        MANY_DTOR_CALLS.fetch_add(1, AtomicOrdering::SeqCst);
        if !ptr.is_null() {
            unsafe { drop(Box::from_raw(ptr.cast::<u64>())) };
        }
    }

    // ---------------------------------------------------------------------
    // next_font_ref_id (private)
    // ---------------------------------------------------------------------

    #[test]
    fn next_font_ref_id_is_monotonic_and_never_zero() {
        let a = next_font_ref_id();
        let b = next_font_ref_id();
        // `id == 0` is the "un-initialised / raw-reconstructed" sentinel, so the
        // counter must never hand it out.
        assert!(a >= 1, "id 0 is reserved as the null-handle sentinel");
        assert!(b > a, "ids must be strictly increasing ({a} -> {b})");
    }

    // ---------------------------------------------------------------------
    // FontRef::new / FontRef::get_parsed
    // ---------------------------------------------------------------------

    #[test]
    fn font_ref_new_post_construction_invariants() {
        let ptr = boxed_font_data(0xDEAD);
        let font = FontRef::new(ptr, single_counting_destructor);

        assert_eq!(font.get_parsed(), ptr, "get_parsed must return the pointer passed to new()");
        assert!(font.run_destructor);
        assert!(font.id >= 1);
        assert!(!font.copies.is_null());
        assert_eq!(unsafe { (*font.copies).load(AtomicOrdering::SeqCst) }, 1);

        assert_eq!(SINGLE_DTOR_CALLS.load(AtomicOrdering::SeqCst), 0);
        drop(font);
        assert_eq!(
            SINGLE_DTOR_CALLS.load(AtomicOrdering::SeqCst),
            1,
            "the destructor must run exactly once when the last handle drops"
        );
    }

    #[test]
    fn font_ref_new_accepts_null_pointer_without_panicking() {
        let font = FontRef::new(core::ptr::null(), noop_destructor);
        assert!(font.get_parsed().is_null());
        assert!(font.id >= 1);
        // Debug must not choke on a null `parsed`.
        let dbg = format!("{font:?}");
        assert!(dbg.starts_with("FontRef(0x0"), "unexpected Debug output: {dbg}");
        assert!(dbg.contains("copies: 1"), "unexpected Debug output: {dbg}");
    }

    #[test]
    fn font_ref_clone_shares_identity_and_defers_the_destructor() {
        let ptr = boxed_font_data(42);
        let original = FontRef::new(ptr, clone_counting_destructor);
        let copy = original.clone();

        assert_eq!(original, copy, "shallow clones are the same font");
        assert_eq!(original.id, copy.id);
        assert_eq!(hash_of(&original), hash_of(&copy));
        assert_eq!(original.cmp(&copy), Ordering::Equal);
        assert_eq!(original.get_parsed(), copy.get_parsed());
        assert_eq!(unsafe { (*original.copies).load(AtomicOrdering::SeqCst) }, 2);

        drop(copy);
        assert_eq!(
            CLONE_DTOR_CALLS.load(AtomicOrdering::SeqCst),
            0,
            "dropping one of two handles must not free the parsed data"
        );
        drop(original);
        assert_eq!(CLONE_DTOR_CALLS.load(AtomicOrdering::SeqCst), 1);
    }

    #[test]
    fn font_ref_many_clones_run_the_destructor_exactly_once() {
        let ptr = boxed_font_data(7);
        let original = FontRef::new(ptr, many_counting_destructor);
        let clones: Vec<FontRef> = (0..1000).map(|_| original.clone()).collect();

        assert_eq!(unsafe { (*original.copies).load(AtomicOrdering::SeqCst) }, 1001);
        assert!(clones.iter().all(|c| *c == original));

        drop(clones);
        assert_eq!(MANY_DTOR_CALLS.load(AtomicOrdering::SeqCst), 0);
        drop(original);
        assert_eq!(MANY_DTOR_CALLS.load(AtomicOrdering::SeqCst), 1);
    }

    #[test]
    fn font_ref_identity_is_the_id_not_the_pointer() {
        // Two independently-constructed handles over the *same* pointer value must
        // NOT compare equal — that is the whole point of the `id` field (a freed
        // font's heap address can be reused by a later font).
        let a = FontRef::new(core::ptr::null(), noop_destructor);
        let b = FontRef::new(core::ptr::null(), noop_destructor);

        assert_eq!(a.get_parsed(), b.get_parsed(), "same (null) pointer");
        assert_ne!(a, b, "same pointer must not forge identity");
        assert_ne!(a.id, b.id);
        assert_ne!(hash_of(&a), hash_of(&b));
        assert_eq!(a.cmp(&b), Ordering::Less, "ids are handed out in increasing order");
        assert_eq!(a.partial_cmp(&b), Some(Ordering::Less));
    }

    #[test]
    fn font_ref_raw_zero_handle_is_drop_safe() {
        // A handle reconstructed from raw parts (id == 0, no refcount) must not be
        // dereferenced by Debug/Drop.
        let make = || FontRef {
            parsed: core::ptr::null(),
            copies: core::ptr::null(),
            id: 0,
            run_destructor: false,
            parsed_destructor: noop_destructor,
        };
        let raw = make();
        let raw2 = make();

        assert!(raw.get_parsed().is_null());
        assert_eq!(format!("{raw:?}"), "FontRef(0x0)");
        assert_eq!(raw, raw2, "both carry the id==0 sentinel");

        let cloned = raw.clone();
        assert_eq!(cloned.id, 0);
        assert!(cloned.copies.is_null(), "cloning must not allocate a refcount for a raw handle");

        drop(cloned);
        drop(raw2);
        drop(raw); // must not double-free / deref null
    }

    // ---------------------------------------------------------------------
    // StyleFontFamily::as_string
    // ---------------------------------------------------------------------

    #[test]
    fn style_font_family_as_string_quotes_only_when_whitespace_is_present() {
        assert_eq!(StyleFontFamily::System("Arial".into()).as_string(), "Arial");
        assert_eq!(
            StyleFontFamily::System("Times New Roman".into()).as_string(),
            "\"Times New Roman\""
        );
        // An empty family name is not quoted (it has no whitespace).
        assert_eq!(StyleFontFamily::System("".into()).as_string(), "");
        // Tabs / newlines count as whitespace.
        assert_eq!(StyleFontFamily::System("a\tb".into()).as_string(), "\"a\tb\"");
        assert_eq!(StyleFontFamily::System("a\nb".into()).as_string(), "\"a\nb\"");
    }

    #[test]
    fn style_font_family_as_string_handles_unicode() {
        // No ASCII whitespace -> unquoted, bytes preserved.
        assert_eq!(StyleFontFamily::System("日本語".into()).as_string(), "日本語");
        assert_eq!(StyleFontFamily::System("\u{1F600}".into()).as_string(), "\u{1F600}");
        // Combining marks are not whitespace.
        assert_eq!(StyleFontFamily::System("e\u{0301}".into()).as_string(), "e\u{0301}");
        // U+00A0 NO-BREAK SPACE *is* `char::is_whitespace`, so it gets quoted.
        assert_eq!(
            StyleFontFamily::System("a\u{00A0}b".into()).as_string(),
            "\"a\u{00A0}b\""
        );
    }

    #[test]
    fn style_font_family_as_string_file_and_systemtype_and_ref() {
        // `File` is never quoted, even when it contains whitespace.
        assert_eq!(
            StyleFontFamily::File("my font.ttf".into()).as_string(),
            "url(my font.ttf)"
        );
        assert_eq!(
            StyleFontFamily::SystemType(SystemFontType::MonospaceBold).as_string(),
            "system:monospace:bold"
        );

        let ptr = 0xdead_beef_usize as *const c_void;
        let fam = StyleFontFamily::Ref(FontRef::new(ptr, noop_destructor));
        assert_eq!(fam.as_string(), "font-ref(0xdeadbeef)");
    }

    #[test]
    fn style_font_family_as_string_on_huge_name_does_not_panic() {
        let huge = "x".repeat(1_000_000);
        let fam = StyleFontFamily::System(huge.as_str().into());
        assert_eq!(fam.as_string().len(), 1_000_000);
    }

    // ---------------------------------------------------------------------
    // parse_font_weight
    // ---------------------------------------------------------------------

    #[cfg(feature = "parser")]
    #[test]
    fn parse_font_weight_valid_minimal_and_full_roundtrip() {
        assert_eq!(parse_font_weight("normal").unwrap(), StyleFontWeight::Normal);

        for weight in [
            StyleFontWeight::Lighter,
            StyleFontWeight::W100,
            StyleFontWeight::W200,
            StyleFontWeight::W300,
            StyleFontWeight::Normal,
            StyleFontWeight::W500,
            StyleFontWeight::W600,
            StyleFontWeight::Bold,
            StyleFontWeight::W800,
            StyleFontWeight::W900,
            StyleFontWeight::Bolder,
        ] {
            let css = weight.print_as_css_value();
            assert_eq!(
                parse_font_weight(&css).unwrap(),
                weight,
                "encode==decode failed for {weight:?} (printed as {css:?})"
            );
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_font_weight_numeric_aliases_collapse_onto_keywords() {
        // 400/700 are accepted but re-print as keywords, so the *string* round-trip
        // is deliberately lossy in one direction.
        assert_eq!(parse_font_weight("400").unwrap(), StyleFontWeight::Normal);
        assert_eq!(parse_font_weight("700").unwrap(), StyleFontWeight::Bold);
        assert_eq!(StyleFontWeight::Normal.print_as_css_value(), "normal");
        assert_eq!(StyleFontWeight::Bold.print_as_css_value(), "bold");
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_font_weight_rejects_empty_and_whitespace_only() {
        for input in ["", " ", "   ", "\t\n", "\r\n\t "] {
            let err = parse_font_weight(input).unwrap_err();
            assert!(
                matches!(err, CssFontWeightParseError::InvalidValue(InvalidValueErr(""))),
                "expected trimmed InvalidValue(\"\") for {input:?}, got {err:?}"
            );
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_font_weight_rejects_garbage_and_reports_the_trimmed_input() {
        assert_eq!(
            parse_font_weight("  thin  ").unwrap_err(),
            CssFontWeightParseError::InvalidValue(InvalidValueErr("thin")),
            "the error must carry the trimmed input"
        );
        for input in ["thin", "boldest", "bold;garbage", "normal!", "\u{0}\u{1}\u{7f}", "-"] {
            assert!(parse_font_weight(input).is_err(), "{input:?} must not parse");
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_font_weight_rejects_boundary_numbers_without_ever_yielding_invalidnumber() {
        // `parse_font_weight` only matches literal keyword/number strings; it never
        // calls `str::parse`, so the `InvalidNumber` variant is unreachable here.
        for input in [
            "0",
            "-0",
            "450",
            "1000",
            "0400",
            "+400",
            "400.0",
            "4e2",
            "9223372036854775807",
            "-9223372036854775808",
            "18446744073709551616",
            "NaN",
            "inf",
            "-inf",
            "1e309",
        ] {
            let err = parse_font_weight(input).unwrap_err();
            assert!(
                matches!(err, CssFontWeightParseError::InvalidValue(_)),
                "{input:?} should be an InvalidValue, got {err:?}"
            );
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_font_weight_trims_unicode_whitespace() {
        // `str::trim` uses `char::is_whitespace`, so U+00A0 / U+2028 are stripped —
        // stricter CSS tokenisers would not do this.
        assert_eq!(
            parse_font_weight("\u{00A0}bold\u{00A0}").unwrap(),
            StyleFontWeight::Bold
        );
        assert_eq!(parse_font_weight("\u{2028}400").unwrap(), StyleFontWeight::Normal);
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_font_weight_unicode_garbage_is_rejected_and_displayable() {
        let err = parse_font_weight("\u{1F600}").unwrap_err();
        assert_eq!(
            err,
            CssFontWeightParseError::InvalidValue(InvalidValueErr("\u{1F600}"))
        );
        // Display / Debug must not panic on multibyte payloads.
        let msg = format!("{err}");
        assert!(msg.contains('\u{1F600}'), "unexpected message: {msg}");
        assert!(!format!("{err:?}").is_empty());

        assert!(parse_font_weight("bold\u{0301}").is_err(), "combining mark must not be trimmed");
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_font_weight_survives_extremely_long_and_deeply_nested_input() {
        let long = "bold".repeat(250_000); // 1_000_000 chars
        assert!(parse_font_weight(&long).is_err());

        let nested = "(".repeat(10_000);
        assert!(parse_font_weight(&nested).is_err());

        let long_digits = "9".repeat(100_000);
        assert!(parse_font_weight(&long_digits).is_err());
    }

    // ---------------------------------------------------------------------
    // parse_font_style
    // ---------------------------------------------------------------------

    #[cfg(feature = "parser")]
    #[test]
    fn parse_font_style_valid_minimal_and_full_roundtrip() {
        assert_eq!(parse_font_style("normal").unwrap(), StyleFontStyle::Normal);
        for style in [
            StyleFontStyle::Normal,
            StyleFontStyle::Italic,
            StyleFontStyle::Oblique,
        ] {
            let css = style.print_as_css_value();
            assert_eq!(parse_font_style(&css).unwrap(), style, "encode==decode failed for {style:?}");
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_font_style_rejects_empty_whitespace_and_garbage() {
        for input in ["", "   ", "\t\n"] {
            assert_eq!(
                parse_font_style(input).unwrap_err(),
                CssFontStyleParseError::InvalidValue(InvalidValueErr(""))
            );
        }
        for input in [
            "slanted",
            "italics",
            "ITALIC",
            "italic;garbage",
            "oblique 14deg",
            "0",
            "-0",
            "NaN",
            "inf",
            "9223372036854775807",
        ] {
            assert!(parse_font_style(input).is_err(), "{input:?} must not parse");
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_font_style_leading_trailing_junk_and_unicode() {
        assert_eq!(parse_font_style("  italic  ").unwrap(), StyleFontStyle::Italic);
        assert_eq!(
            parse_font_style(" italic;").unwrap_err(),
            CssFontStyleParseError::InvalidValue(InvalidValueErr("italic;"))
        );
        let err = parse_font_style("\u{1F600}\u{0301}").unwrap_err();
        assert!(!format!("{err}").is_empty());
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_font_style_survives_extremely_long_and_deeply_nested_input() {
        let long = "italic".repeat(200_000); // 1_200_000 chars
        assert!(parse_font_style(&long).is_err());
        let nested = "[".repeat(10_000);
        assert!(parse_font_style(&nested).is_err());
    }

    // ---------------------------------------------------------------------
    // parse_style_font_size
    // ---------------------------------------------------------------------

    #[cfg(feature = "parser")]
    #[test]
    fn parse_style_font_size_valid_minimal_and_metric_roundtrip() {
        assert_eq!(parse_style_font_size("16px").unwrap().inner, PixelValue::px(16.0));

        // NOTE: `SizeMetric::Vmin` is deliberately excluded — see
        // `parse_style_font_size_vmin_is_shadowed_by_the_in_suffix`.
        for metric in [
            SizeMetric::Px,
            SizeMetric::Pt,
            SizeMetric::Em,
            SizeMetric::Rem,
            SizeMetric::In,
            SizeMetric::Cm,
            SizeMetric::Mm,
            SizeMetric::Percent,
            SizeMetric::Vw,
            SizeMetric::Vh,
            SizeMetric::Vmax,
        ] {
            let size = StyleFontSize {
                inner: PixelValue::from_metric(metric, 12.0),
            };
            let css = size.print_as_css_value();
            assert_eq!(
                parse_style_font_size(&css).unwrap(),
                size,
                "encode==decode failed for {metric:?} (printed as {css:?})"
            );
        }

        // The default (12pt) must survive a print/parse round-trip too.
        let default = StyleFontSize::default();
        assert_eq!(default.print_as_css_value(), "12pt");
        assert_eq!(parse_style_font_size("12pt").unwrap(), default);
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_style_font_size_vmin_is_shadowed_by_the_in_suffix() {
        // FIXED (was a characterization of the bug): "in" used to be tested before
        // "vmin", so "12vmin" stripped to "12vm" and failed to parse. The suffix
        // table in css/src/props/basic/pixel.rs now orders "vmin" ahead of "in", so
        // font-size in vmin round-trips.
        let size = StyleFontSize {
            inner: PixelValue::from_metric(SizeMetric::Vmin, 12.0),
        };
        let css = size.print_as_css_value();
        assert_eq!(css, "12vmin");

        assert_eq!(
            parse_style_font_size(&css).unwrap().inner,
            PixelValue::from_metric(SizeMetric::Vmin, 12.0)
        );
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_style_font_size_rejects_empty_and_whitespace_only() {
        for input in ["", " ", "   ", "\t\n"] {
            let err = parse_style_font_size(input).unwrap_err();
            assert_eq!(
                err,
                CssStyleFontSizeParseError::PixelValue(CssPixelValueParseError::EmptyString),
                "unexpected error for {input:?}"
            );
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_style_font_size_rejects_garbage_and_bare_units() {
        let err = parse_style_font_size("px").unwrap_err();
        assert!(
            matches!(
                err,
                CssStyleFontSizeParseError::PixelValue(CssPixelValueParseError::NoValueGiven(
                    "px",
                    SizeMetric::Px
                ))
            ),
            "expected NoValueGiven, got {err:?}"
        );

        for input in [
            "medium",
            "larger",
            "16PX",      // unit matching is case-sensitive
            "16px;junk",
            "16 px junk",
            "\u{1F600}",
            "--",
            "px16",
        ] {
            assert!(parse_style_font_size(input).is_err(), "{input:?} must not parse");
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_style_font_size_accepts_unitless_numbers_as_px() {
        // Deviation from CSS (which only allows a unitless `0`): any bare number is
        // accepted and silently treated as `px`.
        assert_eq!(parse_style_font_size("0").unwrap().inner, PixelValue::px(0.0));
        assert_eq!(parse_style_font_size("16").unwrap().inner, PixelValue::px(16.0));
        assert_eq!(parse_style_font_size("-16").unwrap().inner, PixelValue::px(-16.0));
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_style_font_size_boundary_numbers_saturate_instead_of_panicking() {
        // Signed zero collapses to +0.
        assert_eq!(parse_style_font_size("-0px").unwrap().inner.number.get(), 0.0);
        assert_eq!(parse_style_font_size("0px").unwrap().inner.number.get(), 0.0);

        // f32 overflow -> +/-inf -> the isize encoding saturates (no UB, no panic).
        let big = parse_style_font_size("1e40px").unwrap().inner.number.get();
        assert!(big.is_finite() && big > 0.0, "expected a saturated finite value, got {big}");
        let small = parse_style_font_size("-1e40px").unwrap().inner.number.get();
        assert!(small.is_finite() && small < 0.0, "expected a saturated finite value, got {small}");

        // Literal infinities are accepted by `f32::from_str` and saturate as well.
        let inf = parse_style_font_size("inf").unwrap().inner.number.get();
        assert!(inf.is_finite() && inf > 0.0);
        let neg_inf = parse_style_font_size("-infinitypx").unwrap().inner.number.get();
        assert!(neg_inf.is_finite() && neg_inf < 0.0);

        // i64::MAX / u64::MAX as bare numbers: no overflow panic.
        for input in ["9223372036854775807", "18446744073709551615px"] {
            let v = parse_style_font_size(input).unwrap().inner.number.get();
            assert!(v.is_finite(), "{input:?} produced {v}");
        }

        // Sub-milli precision is truncated by the fixed-point encoding, not rounded.
        assert_eq!(parse_style_font_size("16.0004px").unwrap().inner, PixelValue::px(16.0));
        assert_eq!(parse_style_font_size("1e-40px").unwrap().inner.number.get(), 0.0);
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_style_font_size_nan_is_silently_coerced_to_zero() {
        // `f32::from_str` accepts "NaN", and the fixed-point cast maps NaN -> 0.
        // A stricter CSS parser would reject this outright; asserted as-is.
        let parsed = parse_style_font_size("NaN").unwrap();
        assert_eq!(parsed.inner.metric, SizeMetric::Px);
        assert_eq!(parsed.inner.number.get(), 0.0);
        assert_eq!(parse_style_font_size("nanpx").unwrap().inner.number.get(), 0.0);
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_style_font_size_leading_trailing_whitespace_is_trimmed() {
        assert_eq!(parse_style_font_size("  16px  ").unwrap().inner, PixelValue::px(16.0));
        // Whitespace *between* number and unit is tolerated as well.
        assert_eq!(parse_style_font_size("16 px").unwrap().inner, PixelValue::px(16.0));
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_style_font_size_survives_extremely_long_and_deeply_nested_input() {
        let long_digits = "1".repeat(50_000);
        let parsed = parse_style_font_size(&long_digits).unwrap();
        assert!(parsed.inner.number.get().is_finite());

        let long_garbage = "z".repeat(1_000_000);
        assert!(parse_style_font_size(&long_garbage).is_err());

        let nested = "(".repeat(10_000);
        assert!(parse_style_font_size(&nested).is_err());
    }

    // ---------------------------------------------------------------------
    // parse_style_font_family
    // ---------------------------------------------------------------------

    #[cfg(feature = "parser")]
    #[test]
    fn parse_style_font_family_valid_minimal() {
        let parsed = parse_style_font_family("Arial").unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed.as_slice()[0], StyleFontFamily::System("Arial".into()));
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_style_font_family_never_returns_err() {
        // Every failure path inside the parser falls back to "treat it as an
        // unquoted family name", so the `Err` half of the signature (and with it
        // `CssStyleFontFamilyParseError`) is unreachable. Documented, not weakened.
        let nested = "(".repeat(10_000);
        let long = "x".repeat(1_000_000);
        let inputs: Vec<&str> = vec![
            "",
            "   ",
            "\t\n",
            ",",
            ",,,",
            "'unclosed",
            "\"unclosed",
            "\"Arial'",
            "'Arial\"",
            "\u{1F600}",
            "system:",
            "system:bogus",
            "url(x.ttf)",
            "font-ref(0xdeadbeef)",
            "\u{0}\u{7f}",
            &nested,
            &long,
        ];
        for input in inputs {
            assert!(
                parse_style_font_family(input).is_ok(),
                "parse_style_font_family unexpectedly failed for {:?}",
                &input[..input.len().min(32)]
            );
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_style_font_family_empty_input_yields_one_empty_family() {
        // Deviation from CSS (an empty font-family list is invalid there): the
        // parser produces a single, empty `System` name instead of erroring.
        let parsed = parse_style_font_family("").unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed.as_slice()[0], StyleFontFamily::System("".into()));

        let parsed = parse_style_font_family("   ").unwrap();
        assert_eq!(parsed.as_slice()[0], StyleFontFamily::System("".into()));

        let parsed = parse_style_font_family(",,,").unwrap();
        assert_eq!(parsed.len(), 4, "N commas produce N+1 (empty) families");
        assert!(parsed
            .iter()
            .all(|f| *f == StyleFontFamily::System("".into())));
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_style_font_family_unclosed_quotes_keep_the_quote_character() {
        // `strip_quotes` errors, and the parser then keeps the *raw* token — so the
        // quote survives into the family name rather than surfacing UnclosedQuotes.
        let parsed = parse_style_font_family("'unclosed").unwrap();
        assert_eq!(parsed.as_slice()[0], StyleFontFamily::System("'unclosed".into()));

        let parsed = parse_style_font_family("\"Arial'").unwrap();
        assert_eq!(parsed.as_slice()[0], StyleFontFamily::System("\"Arial'".into()));

        // An empty quoted string strips down to an empty family name.
        let parsed = parse_style_font_family("\"\"").unwrap();
        assert_eq!(parsed.as_slice()[0], StyleFontFamily::System("".into()));
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_style_font_family_system_prefix_is_case_sensitive_and_falls_back() {
        // Unknown `system:` types fall through to a literal family name.
        let parsed = parse_style_font_family("system:bogus").unwrap();
        assert_eq!(parsed.as_slice()[0], StyleFontFamily::System("system:bogus".into()));
        let parsed = parse_style_font_family("system:").unwrap();
        assert_eq!(parsed.as_slice()[0], StyleFontFamily::System("system:".into()));
        // Uppercase prefix is not recognised as a system font.
        let parsed = parse_style_font_family("SYSTEM:UI").unwrap();
        assert_eq!(parsed.as_slice()[0], StyleFontFamily::System("SYSTEM:UI".into()));
        let parsed = parse_style_font_family("system:UI").unwrap();
        assert_eq!(parsed.as_slice()[0], StyleFontFamily::System("system:UI".into()));
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_style_font_family_never_produces_file_or_ref_variants() {
        for input in ["url(x.ttf)", "font-ref(0x1)", "Arial", "system:ui", "'a'", "\u{1F600}"] {
            let parsed = parse_style_font_family(input).unwrap();
            assert!(
                parsed.iter().all(|f| matches!(
                    f,
                    StyleFontFamily::System(_) | StyleFontFamily::SystemType(_)
                )),
                "the parser must only ever yield System/SystemType, got {parsed:?}"
            );
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_style_font_family_handles_unicode_names() {
        let parsed = parse_style_font_family("日本語, \u{1F600}, e\u{0301}").unwrap();
        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed.as_slice()[0], StyleFontFamily::System("日本語".into()));
        assert_eq!(parsed.as_slice()[1], StyleFontFamily::System("\u{1F600}".into()));
        assert_eq!(parsed.as_slice()[2], StyleFontFamily::System("e\u{0301}".into()));
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_style_font_family_survives_extremely_long_and_deeply_nested_input() {
        let huge_name = "x".repeat(1_000_000);
        let parsed = parse_style_font_family(&huge_name).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed.as_slice()[0], StyleFontFamily::System(huge_name.as_str().into()));

        let many = "Arial,".repeat(10_000);
        let parsed = parse_style_font_family(&many).unwrap();
        assert_eq!(parsed.len(), 10_001, "trailing comma adds one empty family");

        let nested = "(".repeat(10_000);
        let parsed = parse_style_font_family(&nested).unwrap();
        assert_eq!(parsed.len(), 1);
    }

    // ---------------------------------------------------------------------
    // as_string <-> parse_style_font_family round-trips
    // ---------------------------------------------------------------------

    #[cfg(feature = "parser")]
    #[test]
    fn style_font_family_as_string_roundtrips_through_the_parser() {
        for name in ["Arial", "Times New Roman", "", "日本語", "a\u{00A0}b", "Fo\"o", "serif"] {
            let family = StyleFontFamily::System(name.into());
            let css = family.as_string();
            let parsed = parse_style_font_family(&css).unwrap();
            assert_eq!(parsed.len(), 1, "{name:?} printed as {css:?}");
            assert_eq!(parsed.as_slice()[0], family, "encode==decode failed for {name:?}");
        }

        for ft in [
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
        ] {
            let family = StyleFontFamily::SystemType(ft);
            let parsed = parse_style_font_family(&family.as_string()).unwrap();
            assert_eq!(parsed.as_slice()[0], family, "encode==decode failed for {ft:?}");
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn style_font_family_as_string_does_not_escape_commas() {
        // LOSSY: `as_string()` quotes on whitespace only, so a comma inside a family
        // name re-parses as two families. Asserted as-is; reported as a defect.
        let family = StyleFontFamily::System("Foo,Bar".into());
        assert_eq!(family.as_string(), "Foo,Bar");
        let parsed = parse_style_font_family(&family.as_string()).unwrap();
        assert_eq!(parsed.len(), 2);
    }

    #[cfg(feature = "parser")]
    #[test]
    fn style_font_family_file_and_ref_do_not_roundtrip() {
        // `url(...)` / `font-ref(...)` are printable but not parseable: they come
        // back as plain `System` names.
        let file = StyleFontFamily::File("f.ttf".into());
        let parsed = parse_style_font_family(&file.as_string()).unwrap();
        assert_eq!(parsed.as_slice()[0], StyleFontFamily::System("url(f.ttf)".into()));

        let font = StyleFontFamily::Ref(FontRef::new(core::ptr::null(), noop_destructor));
        let parsed = parse_style_font_family(&font.as_string()).unwrap();
        assert_eq!(parsed.as_slice()[0], StyleFontFamily::System("font-ref(0x0)".into()));
    }

    #[cfg(feature = "parser")]
    #[test]
    fn style_font_family_vec_print_as_css_value_roundtrips() {
        let css = "Arial, \"Times New Roman\", system:ui";
        let parsed = parse_style_font_family(css).unwrap();
        assert_eq!(parsed.print_as_css_value(), css);
        let reparsed = parse_style_font_family(&parsed.print_as_css_value()).unwrap();
        assert_eq!(reparsed, parsed, "encode==decode failed for a font stack");
    }

    // ---------------------------------------------------------------------
    // Error to_contained / to_shared
    // ---------------------------------------------------------------------

    #[test]
    fn css_font_weight_parse_error_invalid_value_roundtrips() {
        for value in ["", "thin", "\u{1F600}", "a\u{0}b"] {
            let shared = CssFontWeightParseError::InvalidValue(InvalidValueErr(value));
            let owned = shared.to_contained();
            assert_eq!(
                owned,
                CssFontWeightParseErrorOwned::InvalidValue(InvalidValueErrOwned {
                    value: value.into()
                })
            );
            assert_eq!(owned.to_shared(), shared, "to_contained/to_shared must round-trip");
            assert!(!format!("{shared}").is_empty());
        }
    }

    #[test]
    fn css_font_weight_parse_error_invalid_number_roundtrips() {
        let cases = [
            "".parse::<i32>().unwrap_err(),
            "x".parse::<i32>().unwrap_err(),
            "99999999999999999999".parse::<i32>().unwrap_err(),
            "-99999999999999999999".parse::<i32>().unwrap_err(),
        ];
        for err in cases {
            let shared = CssFontWeightParseError::InvalidNumber(err);
            let owned = shared.to_contained();
            assert_eq!(owned.to_shared(), shared, "kind must survive the FFI round-trip");
            assert!(!format!("{shared}").is_empty());
        }
    }

    #[test]
    fn css_font_weight_parse_error_zero_kind_roundtrip_is_lossy() {
        // `IntErrorKind::Zero` cannot be reconstructed on stable Rust — the source
        // documents this; assert the documented degradation to InvalidDigit.
        let zero_err = "0".parse::<core::num::NonZeroU32>().unwrap_err();
        let shared = CssFontWeightParseError::InvalidNumber(zero_err);
        let owned = shared.to_contained();
        assert_eq!(
            owned,
            CssFontWeightParseErrorOwned::InvalidNumber(CParseIntError::Zero),
            "the Zero kind must survive into the owned form"
        );
        assert_ne!(
            owned.to_shared(),
            shared,
            "to_std() cannot rebuild a Zero-kind ParseIntError (documented)"
        );
    }

    #[test]
    fn css_font_style_parse_error_roundtrips() {
        for value in ["", "slanted", "\u{1F600}"] {
            let shared = CssFontStyleParseError::InvalidValue(InvalidValueErr(value));
            let owned = shared.to_contained();
            assert_eq!(
                owned,
                CssFontStyleParseErrorOwned::InvalidValue(InvalidValueErrOwned {
                    value: value.into()
                })
            );
            assert_eq!(owned.to_shared(), shared);
            assert!(!format!("{shared}").is_empty());
        }
    }

    #[test]
    fn css_style_font_size_parse_error_roundtrips_every_variant() {
        let cases = [
            CssPixelValueParseError::EmptyString,
            CssPixelValueParseError::NoValueGiven("px", SizeMetric::Px),
            CssPixelValueParseError::NoValueGiven("%", SizeMetric::Percent),
            CssPixelValueParseError::ValueParseErr("abc".parse::<f32>().unwrap_err(), "abc"),
            CssPixelValueParseError::ValueParseErr("".parse::<f32>().unwrap_err(), ""),
            CssPixelValueParseError::InvalidPixelValue("medium"),
            CssPixelValueParseError::InvalidPixelValue("\u{1F600}"),
        ];
        for inner in cases {
            let shared = CssStyleFontSizeParseError::PixelValue(inner);
            let owned = shared.to_contained();
            assert_eq!(owned.to_shared(), shared, "to_contained/to_shared must round-trip");
            assert!(!format!("{shared}").is_empty());
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn css_style_font_family_parse_error_roundtrips_every_variant() {
        let cases = [
            CssStyleFontFamilyParseError::InvalidStyleFontFamily(""),
            CssStyleFontFamilyParseError::InvalidStyleFontFamily("bogus"),
            CssStyleFontFamilyParseError::UnclosedQuotes(UnclosedQuotesError("\"Arial")),
            CssStyleFontFamilyParseError::UnclosedQuotes(UnclosedQuotesError("\u{1F600}")),
        ];
        for shared in cases {
            let owned = shared.to_contained();
            assert_eq!(owned.to_shared(), shared, "to_contained/to_shared must round-trip");
            assert!(!format!("{shared}").is_empty());
        }
    }

    // ---------------------------------------------------------------------
    // FormatAsRustCode / defaults / ordering
    // ---------------------------------------------------------------------

    #[test]
    fn font_enum_defaults_and_ordering() {
        assert_eq!(StyleFontWeight::default(), StyleFontWeight::Normal);
        assert_eq!(StyleFontStyle::default(), StyleFontStyle::Normal);
        assert_eq!(StyleFontSize::default().inner, PixelValue::const_pt(12));

        // Derived Ord follows declaration order (numeric weights are ordered).
        assert!(StyleFontWeight::W100 < StyleFontWeight::W900);
        assert!(StyleFontWeight::Normal < StyleFontWeight::Bold);
        assert!(StyleFontWeight::Lighter < StyleFontWeight::W100);
        assert!(StyleFontWeight::Bolder > StyleFontWeight::W900);
    }

    #[test]
    fn format_as_rust_code_matches_the_debug_variant_names() {
        for weight in [
            StyleFontWeight::Lighter,
            StyleFontWeight::W100,
            StyleFontWeight::W200,
            StyleFontWeight::W300,
            StyleFontWeight::Normal,
            StyleFontWeight::W500,
            StyleFontWeight::W600,
            StyleFontWeight::Bold,
            StyleFontWeight::W800,
            StyleFontWeight::W900,
            StyleFontWeight::Bolder,
        ] {
            assert_eq!(
                weight.format_as_rust_code(0),
                format!("StyleFontWeight::{weight:?}")
            );
        }
        for style in [
            StyleFontStyle::Normal,
            StyleFontStyle::Italic,
            StyleFontStyle::Oblique,
        ] {
            assert_eq!(
                style.format_as_rust_code(0),
                format!("StyleFontStyle::{style:?}")
            );
        }
        assert_eq!(
            StyleFontFamily::SystemType(SystemFontType::Ui).format_as_rust_code(0),
            "StyleFontFamily::SystemType(SystemFontType::Ui)"
        );
        assert!(StyleFontFamily::System("Arial".into())
            .format_as_rust_code(0)
            .starts_with("StyleFontFamily::System(STRING_"));
        assert!(StyleFontFamily::File("a.ttf".into())
            .format_as_rust_code(0)
            .starts_with("StyleFontFamily::File(STRING_"));
    }

    // ---------------------------------------------------------------------
    // Panose::zero / FontMetrics::zero + getters
    // ---------------------------------------------------------------------

    #[test]
    fn panose_zero_is_the_neutral_element() {
        const P: Panose = Panose::zero();
        assert_eq!(P, Panose::default());
        assert_eq!(hash_of(&P), hash_of(&Panose::default()));
        assert_eq!(P.family_type, 0);
        assert_eq!(P.serif_style, 0);
        assert_eq!(P.weight, 0);
        assert_eq!(P.proportion, 0);
        assert_eq!(P.contrast, 0);
        assert_eq!(P.stroke_variation, 0);
        assert_eq!(P.arm_style, 0);
        assert_eq!(P.letterform, 0);
        assert_eq!(P.midline, 0);
        assert_eq!(P.x_height, 0);

        let mut max = Panose::zero();
        max.family_type = u8::MAX;
        assert!(max > P, "derived Ord must order by the first field");
    }

    #[test]
    fn font_metrics_zero_invariants() {
        const M: FontMetrics = FontMetrics::zero();
        assert_eq!(M, FontMetrics::default());

        // Documented: a zero font still declares a sane em square / weight class.
        assert_eq!(M.units_per_em, 1000);
        assert_eq!(M.us_weight_class, 400);
        assert_eq!(M.us_width_class, 5);
        assert_eq!(M.panose, Panose::zero());

        assert_eq!(M.get_ascender(), 0);
        assert_eq!(M.get_descender(), 0);
        assert_eq!(M.get_line_gap(), 0);
        assert_eq!(M.get_advance_width_max(), 0);
        assert_eq!(M.get_min_left_side_bearing(), 0);
        assert_eq!(M.get_min_right_side_bearing(), 0);
        assert_eq!(M.get_x_min(), 0);
        assert_eq!(M.get_y_min(), 0);
        assert_eq!(M.get_x_max(), 0);
        assert_eq!(M.get_y_max(), 0);
        assert_eq!(M.get_x_max_extent(), 0);
        assert_eq!(M.get_x_avg_char_width(), 0);
        assert_eq!(M.get_y_subscript_x_size(), 0);
        assert_eq!(M.get_y_subscript_y_size(), 0);
        assert_eq!(M.get_y_subscript_x_offset(), 0);
        assert_eq!(M.get_y_subscript_y_offset(), 0);
        assert_eq!(M.get_y_superscript_x_size(), 0);
        assert_eq!(M.get_y_superscript_y_size(), 0);
        assert_eq!(M.get_y_superscript_x_offset(), 0);
        assert_eq!(M.get_y_superscript_y_offset(), 0);
        assert_eq!(M.get_y_strikeout_size(), 0);
        assert_eq!(M.get_y_strikeout_position(), 0);
        assert!(!M.use_typo_metrics());

        assert!(matches!(M.ul_code_page_range1, OptionU32::None));
        assert!(matches!(M.ul_code_page_range2, OptionU32::None));
        assert!(matches!(M.s_typo_ascender, OptionI16::None));
        assert!(matches!(M.s_typo_descender, OptionI16::None));
        assert!(matches!(M.s_typo_line_gap, OptionI16::None));
        assert!(matches!(M.us_win_ascent, OptionU16::None));
        assert!(matches!(M.us_win_descent, OptionU16::None));
        assert!(matches!(M.sx_height, OptionI16::None));
        assert!(matches!(M.s_cap_height, OptionI16::None));
    }

    #[test]
    fn font_metrics_getters_return_extreme_values_unchanged() {
        let mut m = FontMetrics::zero();
        m.ascender = i16::MAX;
        m.descender = i16::MIN;
        m.line_gap = i16::MIN;
        m.advance_width_max = u16::MAX;
        m.min_left_side_bearing = i16::MIN;
        m.min_right_side_bearing = i16::MAX;
        m.x_min = i16::MIN;
        m.y_min = i16::MIN;
        m.x_max = i16::MAX;
        m.y_max = i16::MAX;
        m.x_max_extent = i16::MAX;
        m.x_avg_char_width = i16::MIN;
        m.y_subscript_x_size = i16::MAX;
        m.y_subscript_y_size = i16::MIN;
        m.y_subscript_x_offset = i16::MAX;
        m.y_subscript_y_offset = i16::MIN;
        m.y_superscript_x_size = i16::MAX;
        m.y_superscript_y_size = i16::MIN;
        m.y_superscript_x_offset = i16::MAX;
        m.y_superscript_y_offset = i16::MIN;
        m.y_strikeout_size = i16::MAX;
        m.y_strikeout_position = i16::MIN;

        // Getters are plain field reads: no clamping, no sign flips, no panics.
        assert_eq!(m.get_ascender(), i16::MAX);
        assert_eq!(m.get_descender(), i16::MIN);
        assert_eq!(m.get_line_gap(), i16::MIN);
        assert_eq!(m.get_advance_width_max(), u16::MAX);
        assert_eq!(m.get_min_left_side_bearing(), i16::MIN);
        assert_eq!(m.get_min_right_side_bearing(), i16::MAX);
        assert_eq!(m.get_x_min(), i16::MIN);
        assert_eq!(m.get_y_min(), i16::MIN);
        assert_eq!(m.get_x_max(), i16::MAX);
        assert_eq!(m.get_y_max(), i16::MAX);
        assert_eq!(m.get_x_max_extent(), i16::MAX);
        assert_eq!(m.get_x_avg_char_width(), i16::MIN);
        assert_eq!(m.get_y_subscript_x_size(), i16::MAX);
        assert_eq!(m.get_y_subscript_y_size(), i16::MIN);
        assert_eq!(m.get_y_subscript_x_offset(), i16::MAX);
        assert_eq!(m.get_y_subscript_y_offset(), i16::MIN);
        assert_eq!(m.get_y_superscript_x_size(), i16::MAX);
        assert_eq!(m.get_y_superscript_y_size(), i16::MIN);
        assert_eq!(m.get_y_superscript_x_offset(), i16::MAX);
        assert_eq!(m.get_y_superscript_y_offset(), i16::MIN);
        assert_eq!(m.get_y_strikeout_size(), i16::MAX);
        assert_eq!(m.get_y_strikeout_position(), i16::MIN);

        // An "inverted" font (ascender < descender) is accepted verbatim — the
        // getters do no validation.
        assert!(m.get_ascender() > m.get_descender());
    }

    #[test]
    fn font_metrics_use_typo_metrics_reads_exactly_bit_7() {
        let mut m = FontMetrics::zero();
        for bit in 0..16u16 {
            m.fs_selection = 1 << bit;
            assert_eq!(
                m.use_typo_metrics(),
                bit == 7,
                "fs_selection bit {bit} must not affect USE_TYPO_METRICS"
            );
        }
        m.fs_selection = u16::MAX;
        assert!(m.use_typo_metrics());
        m.fs_selection = u16::MAX ^ 0x0080;
        assert!(!m.use_typo_metrics(), "clearing bit 7 must clear the flag");
        m.fs_selection = 0;
        assert!(!m.use_typo_metrics());
    }
}
