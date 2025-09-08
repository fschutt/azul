//! Font-related CSS properties

use crate::props::basic::value::PixelValue;
use crate::props::formatter::FormatAsCssValue;
use crate::{
    impl_vec, impl_vec_clone, impl_vec_debug, impl_vec_eq, impl_vec_hash, impl_vec_ord,
    impl_vec_partialeq, impl_vec_partialord, AzString, U8Vec,
};
use alloc::string::String;
use core::{ffi::c_void, fmt, sync::atomic::AtomicUsize};

/// CSS font-size property
#[derive(Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct StyleFontSize {
    pub inner: PixelValue,
}

impl Default for StyleFontSize {
    fn default() -> Self {
        Self {
            inner: PixelValue::const_em(1),
        }
    }
}

impl fmt::Debug for StyleFontSize {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self.inner)
    }
}

impl fmt::Display for StyleFontSize {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.inner)
    }
}

impl StyleFontSize {
    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        self.inner.scale_for_dpi(scale_factor);
    }

    pub fn interpolate(&self, other: &Self, t: f32) -> Self {
        Self {
            inner: self.inner.interpolate(&other.inner, t),
        }
    }
}

impl FormatAsCssValue for StyleFontSize {
    fn format_as_css_value(&self) -> String {
        self.inner.format_as_css_value()
    }
}

/// CSS font-family property
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
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
impl_vec_debug!(StyleFontFamily, StyleFontFamilyVec);
impl_vec_partialord!(StyleFontFamily, StyleFontFamilyVec);
impl_vec_ord!(StyleFontFamily, StyleFontFamilyVec);
impl_vec_clone!(
    StyleFontFamily,
    StyleFontFamilyVec,
    StyleFontFamilyVecDestructor
);
impl_vec_partialeq!(StyleFontFamily, StyleFontFamilyVec);
impl_vec_eq!(StyleFontFamily, StyleFontFamilyVec);
impl_vec_hash!(StyleFontFamily, StyleFontFamilyVec);

impl Default for StyleFontFamily {
    fn default() -> Self {
        StyleFontFamily::System("sans-serif".into())
    }
}

impl FormatAsCssValue for StyleFontFamily {
    fn format_as_css_value(&self) -> String {
        match self {
            StyleFontFamily::System(name) => name.as_str().to_string(),
            StyleFontFamily::File(path) => format!("url(\"{}\")", path.as_str()),
            StyleFontFamily::Ref(_) => "ref".to_string(), // Can't represent this in CSS
        }
    }
}

/// Reference to a loaded font
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct FontRef {
    /// shared pointer to an opaque implementation of the parsed font
    pub data: *const FontData,
    /// How many copies does this font have (if 0, the font data will be deleted on drop)
    pub copies: *const AtomicUsize,
    pub run_destructor: bool,
}

impl fmt::Debug for FontRef {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "printing FontRef 0x{:0x}", self.data as usize)?;
        if let Some(d) = unsafe { self.data.as_ref() } {
            d.fmt(f)?;
        }
        if let Some(c) = unsafe { self.copies.as_ref() } {
            c.fmt(f)?;
        }
        Ok(())
    }
}

/// Font data container
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct FontData {
    /// Bytes of the font file, either &'static (never changing bytes) or a Vec<u8>.
    pub bytes: U8Vec,
    /// Index of the font in the file (if not known, set to 0) -
    /// only relevant if the file is a font collection
    pub font_index: u32,
    /// Opaque pointer to parsed font data
    pub parsed: *const c_void,
    /// destructor of the ParsedFont
    pub parsed_destructor: fn(*mut c_void),
}

impl fmt::Debug for FontData {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "FontData: {{")?;
        write!(f, "    bytes: {} bytes", self.bytes.len())?;
        write!(f, "    font_index: {}", self.font_index)?;
        write!(f, "}}")
    }
}

/// Font metrics from the font file
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
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
    // OS/2 table
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
    pub panose: [u8; 10],
    pub ul_unicode_range_1: u32,
    pub ul_unicode_range_2: u32,
    pub ul_unicode_range_3: u32,
    pub ul_unicode_range_4: u32,
    pub ach_vend_id: [u8; 4],
    pub fs_selection: u16,
    pub us_first_char_index: u16,
    pub us_last_char_index: u16,
    pub s_typo_ascender: i16,
    pub s_typo_descender: i16,
    pub s_typo_line_gap: i16,
    pub us_win_ascent: u16,
    pub us_win_descent: u16,
    pub ul_code_page_range_1: u32,
    pub ul_code_page_range_2: u32,
    pub sx_height: i16,
    pub s_cap_height: i16,
    pub us_default_char: u16,
    pub us_break_char: u16,
    pub us_max_context: u16,
}

impl Default for FontMetrics {
    fn default() -> Self {
        Self {
            units_per_em: 1000,
            font_flags: 0,
            x_min: 0,
            y_min: 0,
            x_max: 1000,
            y_max: 1000,
            ascender: 800,
            descender: -200,
            line_gap: 200,
            advance_width_max: 1000,
            min_left_side_bearing: 0,
            min_right_side_bearing: 0,
            x_max_extent: 1000,
            caret_slope_rise: 1,
            caret_slope_run: 0,
            caret_offset: 0,
            num_h_metrics: 0,
            x_avg_char_width: 500,
            us_weight_class: 400,
            us_width_class: 5,
            fs_type: 0,
            y_subscript_x_size: 650,
            y_subscript_y_size: 600,
            y_subscript_x_offset: 0,
            y_subscript_y_offset: 75,
            y_superscript_x_size: 650,
            y_superscript_y_size: 600,
            y_superscript_x_offset: 0,
            y_superscript_y_offset: 350,
            y_strikeout_size: 50,
            y_strikeout_position: 300,
            s_family_class: 0,
            panose: [0; 10],
            ul_unicode_range_1: 0,
            ul_unicode_range_2: 0,
            ul_unicode_range_3: 0,
            ul_unicode_range_4: 0,
            ach_vend_id: [0; 4],
            fs_selection: 0,
            us_first_char_index: 32,
            us_last_char_index: 126,
            s_typo_ascender: 800,
            s_typo_descender: -200,
            s_typo_line_gap: 200,
            us_win_ascent: 1000,
            us_win_descent: 200,
            ul_code_page_range_1: 0,
            ul_code_page_range_2: 0,
            sx_height: 500,
            s_cap_height: 700,
            us_default_char: 0,
            us_break_char: 32,
            us_max_context: 0,
        }
    }
}

// TODO: Add parsing functions
// fn parse_style_font_family<'a>(input: &'a str) -> Result<StyleFontFamilyVec, CssStyleFontFamilyParseError<'a>>
// fn parse_style_font_size<'a>(input: &'a str) -> Result<StyleFontSize, CssPixelValueParseError<'a>>
