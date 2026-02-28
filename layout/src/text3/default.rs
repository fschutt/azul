use std::{path::Path, sync::Arc};

use allsorts::{
    gpos,
    gsub::{self, FeatureInfo, FeatureMask, Features},
};
use azul_core::{geom::LogicalSize, glyph::Placement};
use azul_css::props::basic::FontRef;
use rust_fontconfig::FcFontCache;

use crate::{
    font::parsed::ParsedFont,
    text3::{
        cache::{
            BidiDirection, BidiLevel, FontManager, FontSelector, FontVariantCaps,
            FontVariantLigatures, FontVariantNumeric, Glyph, GlyphOrientation, GlyphSource,
            LayoutError, LayoutFontMetrics, ParsedFontTrait, Point, ShallowClone, StyleProperties,
            TextCombineUpright, TextDecoration, TextOrientation, VerticalMetrics, WritingMode,
        },
        script::Script,
    },
};

/// Creates a FontRef from font bytes by parsing them into a ParsedFont.
///
/// This is a bridge function that:
///
/// 1. Parses the bytes into a ParsedFont
/// 2. Wraps it in a FontRef with proper reference counting
///
/// # Arguments
///
/// - `font_bytes` - The raw font file data
/// - `font_index` - Index of the font in a font collection (0 for single fonts)
/// - `parse_outlines` - Whether to parse glyph outlines (expensive, usually false for layout)
pub fn font_ref_from_bytes(
    font_bytes: &[u8],
    font_index: usize,
    parse_outlines: bool,
) -> Option<FontRef> {
    // Parse the font bytes into ParsedFont
    let mut warnings = Vec::new();
    let parsed_font = ParsedFont::from_bytes(font_bytes, font_index, &mut warnings)?;

    Some(crate::parsed_font_to_font_ref(parsed_font))
}

/// A FontLoader that parses font data from a byte slice.
///
/// It is designed to be used in conjunction with a mechanism that reads font files
/// from paths into memory. This loader simply handles the parsing aspect.
#[derive(Debug, Default, Clone)]
pub struct PathLoader;

impl PathLoader {
    pub fn new() -> Self {
        PathLoader
    }

    /// A helper method to read a font from a path and delegate to the trait's `load_font`.
    /// Note: This is a convenience and not part of the `FontLoaderTrait`.
    pub fn load_from_path(&self, path: &Path, font_index: usize) -> Result<FontRef, LayoutError> {
        let font_bytes = std::fs::read(path).map_err(|e| {
            LayoutError::FontNotFound(FontSelector {
                family: path.to_string_lossy().into_owned(),
                weight: rust_fontconfig::FcWeight::Normal,
                style: crate::text3::cache::FontStyle::Normal,
                unicode_ranges: Vec::new(),
            })
        })?;
        self.load_font(&font_bytes, font_index)
    }
}

impl FontManager<FontRef> {
    pub fn new_with_fc_cache(fc_cache: FcFontCache) -> Result<Self, LayoutError> {
        FontManager::new(fc_cache)
    }
}

impl PathLoader {
    /// Loads a font from a byte slice.
    ///
    /// This implementation parses the bytes into a ParsedFont and wraps it in a FontRef.
    pub fn load_font(&self, font_bytes: &[u8], font_index: usize) -> Result<FontRef, LayoutError> {
        // Parse the font bytes and wrap in FontRef
        font_ref_from_bytes(font_bytes, font_index, true).ok_or_else(|| {
            LayoutError::ShapingError("Failed to parse font with allsorts".to_string())
        })
    }
}

// ParsedFontTrait Implementation for FontRef

// Implement ShallowClone for FontRef
impl crate::text3::cache::ShallowClone for FontRef {
    fn shallow_clone(&self) -> Self {
        // FontRef::clone increments the reference count
        self.clone()
    }
}

// Helper to get the inner ParsedFont from FontRef
#[inline]
fn get_parsed_font(font_ref: &FontRef) -> &ParsedFont {
    unsafe { &*(font_ref.get_parsed() as *const ParsedFont) }
}

impl ParsedFontTrait for FontRef {
    fn shape_text(
        &self,
        text: &str,
        script: Script,
        language: crate::text3::script::Language,
        direction: BidiDirection,
        style: &StyleProperties,
    ) -> Result<Vec<Glyph>, LayoutError> {
        // Delegate to the inner ParsedFont's shape_text, passing self as font_ref
        let parsed = get_parsed_font(self);
        parsed.shape_text_for_font_ref(self, text, script, language, direction, style)
    }

    fn get_hash(&self) -> u64 {
        get_parsed_font(self).hash
    }

    fn get_glyph_size(&self, glyph_id: u16, font_size: f32) -> Option<LogicalSize> {
        get_parsed_font(self).get_glyph_size(glyph_id, font_size)
    }

    fn get_hyphen_glyph_and_advance(&self, font_size: f32) -> Option<(u16, f32)> {
        get_parsed_font(self).get_hyphen_glyph_and_advance(font_size)
    }

    fn get_kashida_glyph_and_advance(&self, font_size: f32) -> Option<(u16, f32)> {
        get_parsed_font(self).get_kashida_glyph_and_advance(font_size)
    }

    fn has_glyph(&self, codepoint: u32) -> bool {
        get_parsed_font(self).has_glyph(codepoint)
    }

    fn get_vertical_metrics(&self, glyph_id: u16) -> Option<VerticalMetrics> {
        get_parsed_font(self).get_vertical_metrics(glyph_id)
    }

    fn get_font_metrics(&self) -> LayoutFontMetrics {
        get_parsed_font(self).font_metrics.clone()
    }

    fn num_glyphs(&self) -> u16 {
        get_parsed_font(self).num_glyphs
    }
}

/// Extension trait for FontRef to provide access to font bytes and metrics
///
/// This trait provides methods that require access to the inner ParsedFont data.
pub trait FontRefExt {
    /// Get the original font bytes
    fn get_bytes(&self) -> &[u8];
    /// Get the full font metrics (PDF-style metrics from HEAD, HHEA, OS/2 tables)
    fn get_full_font_metrics(&self) -> azul_css::props::basic::FontMetrics;
}

impl FontRefExt for FontRef {
    fn get_bytes(&self) -> &[u8] {
        &get_parsed_font(self).original_bytes
    }

    fn get_full_font_metrics(&self) -> azul_css::props::basic::FontMetrics {
        use azul_css::{OptionI16, OptionU16, OptionU32};

        let parsed = get_parsed_font(self);
        let pdf = &parsed.pdf_font_metrics;

        // PdfFontMetrics only has a subset of fields; fill others with defaults
        azul_css::props::basic::FontMetrics {
            // OS/2 version 1 fields (u32 - align 4, placed first)
            ul_code_page_range1: OptionU32::None,
            ul_code_page_range2: OptionU32::None,

            // OS/2 table (u32 fields)
            ul_unicode_range1: 0,   // Not in PdfFontMetrics
            ul_unicode_range2: 0,   // Not in PdfFontMetrics
            ul_unicode_range3: 0,   // Not in PdfFontMetrics
            ul_unicode_range4: 0,   // Not in PdfFontMetrics
            ach_vend_id: 0,         // Not in PdfFontMetrics

            // OS/2 version 0 fields (optional)
            s_typo_ascender: OptionI16::None,
            s_typo_descender: OptionI16::None,
            s_typo_line_gap: OptionI16::None,
            us_win_ascent: OptionU16::None,
            us_win_descent: OptionU16::None,

            // OS/2 version 2 fields (optional)
            sx_height: OptionI16::None,
            s_cap_height: OptionI16::None,
            us_default_char: OptionU16::None,
            us_break_char: OptionU16::None,
            us_max_context: OptionU16::None,

            // OS/2 version 3 fields (optional)
            us_lower_optical_point_size: OptionU16::None,
            us_upper_optical_point_size: OptionU16::None,

            // HEAD table fields
            units_per_em: pdf.units_per_em,
            font_flags: pdf.font_flags,
            x_min: pdf.x_min,
            y_min: pdf.y_min,
            x_max: pdf.x_max,
            y_max: pdf.y_max,

            // HHEA table fields
            ascender: pdf.ascender,
            descender: pdf.descender,
            line_gap: pdf.line_gap,
            advance_width_max: pdf.advance_width_max,
            min_left_side_bearing: 0,  // Not in PdfFontMetrics
            min_right_side_bearing: 0, // Not in PdfFontMetrics
            x_max_extent: 0,           // Not in PdfFontMetrics
            caret_slope_rise: pdf.caret_slope_rise,
            caret_slope_run: pdf.caret_slope_run,
            caret_offset: 0,  // Not in PdfFontMetrics
            num_h_metrics: 0, // Not in PdfFontMetrics

            // OS/2 table fields
            x_avg_char_width: pdf.x_avg_char_width,
            us_weight_class: pdf.us_weight_class,
            us_width_class: pdf.us_width_class,
            fs_type: 0,                // Not in PdfFontMetrics
            y_subscript_x_size: 0,     // Not in PdfFontMetrics
            y_subscript_y_size: 0,     // Not in PdfFontMetrics
            y_subscript_x_offset: 0,   // Not in PdfFontMetrics
            y_subscript_y_offset: 0,   // Not in PdfFontMetrics
            y_superscript_x_size: 0,   // Not in PdfFontMetrics
            y_superscript_y_size: 0,   // Not in PdfFontMetrics
            y_superscript_x_offset: 0, // Not in PdfFontMetrics
            y_superscript_y_offset: 0, // Not in PdfFontMetrics
            y_strikeout_size: pdf.y_strikeout_size,
            y_strikeout_position: pdf.y_strikeout_position,
            s_family_class: 0, // Not in PdfFontMetrics
            fs_selection: 0,        // Not in PdfFontMetrics
            us_first_char_index: 0, // Not in PdfFontMetrics
            us_last_char_index: 0,  // Not in PdfFontMetrics

            // Panose (align 1 - last)
            panose: azul_css::props::basic::Panose::zero(),
        }
    }
}

// ParsedFont helper method for FontRef
//
// This allows ParsedFont to create glyphs that use FontRef
//
// FontRef is just a C-style Arc wrapper around ParsedFont, so we delegate to
// the common shaping implementation and convert the font reference type.

impl ParsedFont {
    /// Internal helper that shapes text and returns Glyph
    /// Delegates to shape_text_internal and converts the font reference.
    fn shape_text_for_font_ref(
        &self,
        font_ref: &FontRef,
        text: &str,
        script: Script,
        language: crate::text3::script::Language,
        direction: BidiDirection,
        style: &StyleProperties,
    ) -> Result<Vec<Glyph>, LayoutError> {
        // Use the common shaping implementation
        let shaped = shape_text_internal(self, text, script, language, direction, style)?;

        // Convert Glyph - now using font_hash and font_metrics instead of font reference
        let font_hash = font_ref.get_hash();
        let font_metrics = LayoutFontMetrics {
            ascent: self.font_metrics.ascent,
            descent: self.font_metrics.descent,
            line_gap: self.font_metrics.line_gap,
            units_per_em: self.font_metrics.units_per_em,
        };

        Ok(shaped
            .into_iter()
            .map(|g| Glyph {
                glyph_id: g.glyph_id,
                codepoint: g.codepoint,
                font_hash,
                font_metrics: font_metrics.clone(),
                style: g.style,
                source: g.source,
                logical_byte_index: g.logical_byte_index,
                logical_byte_len: g.logical_byte_len,
                content_index: g.content_index,
                cluster: g.cluster,
                advance: g.advance,
                kerning: g.kerning,
                offset: g.offset,
                vertical_advance: g.vertical_advance,
                vertical_origin_y: g.vertical_origin_y,
                vertical_bearing: g.vertical_bearing,
                orientation: g.orientation,
                script: g.script,
                bidi_level: g.bidi_level,
            })
            .collect())
    }

    fn get_hash(&self) -> u64 {
        self.hash
    }

    fn get_glyph_size(&self, glyph_id: u16, font_size_px: f32) -> Option<LogicalSize> {
        self.glyph_records_decoded.get(&glyph_id).map(|record| {
            let units_per_em = self.font_metrics.units_per_em as f32;
            let scale_factor = if units_per_em > 0.0 {
                font_size_px / units_per_em
            } else {
                0.01 // Avoid division by zero
            };

            // max_x, max_y, min_x, min_y in font units
            let bbox = &record.bounding_box;

            LogicalSize {
                width: (bbox.max_x - bbox.min_x) as f32 * scale_factor,
                height: (bbox.max_y - bbox.min_y) as f32 * scale_factor,
            }
        })
    }

    fn get_hyphen_glyph_and_advance(&self, font_size: f32) -> Option<(u16, f32)> {
        let glyph_id = self.lookup_glyph_index('-' as u32)?;
        let advance_units = self.get_horizontal_advance(glyph_id);
        let scale_factor = if self.font_metrics.units_per_em > 0 {
            font_size / (self.font_metrics.units_per_em as f32)
        } else {
            return None;
        };
        let scaled_advance = advance_units as f32 * scale_factor;
        Some((glyph_id, scaled_advance))
    }

    fn get_kashida_glyph_and_advance(&self, font_size: f32) -> Option<(u16, f32)> {
        // U+0640 is the Arabic Tatweel character, used for kashida justification.
        let glyph_id = self.lookup_glyph_index('\u{0640}' as u32)?;
        let advance_units = self.get_horizontal_advance(glyph_id);
        let scale_factor = if self.font_metrics.units_per_em > 0 {
            font_size / (self.font_metrics.units_per_em as f32)
        } else {
            return None;
        };
        let scaled_advance = advance_units as f32 * scale_factor;
        Some((glyph_id, scaled_advance))
    }
}

// Helper Functions

/// Builds a FeatureMask with the appropriate OpenType features for a given script.
/// This ensures proper text shaping for complex scripts like Arabic, Devanagari, etc.
///
/// The function includes:
/// - Common features for all scripts (ligatures, contextual alternates, etc.)
/// - Script-specific features (positional forms for Arabic, conjuncts for Indic, etc.)
///
/// This is designed to be stable and explicit - we control exactly which features
/// are enabled rather than relying on allsorts' defaults which may change.
fn build_feature_mask_for_script(script: Script) -> FeatureMask {
    use Script::*;

    // Start with common features that apply to most scripts
    let mut mask = FeatureMask::default(); // Includes: CALT, CCMP, CLIG, LIGA, LOCL, RLIG

    // Add script-specific features
    match script {
        // Arabic and related scripts - require positional forms
        Arabic => {
            mask |= FeatureMask::INIT; // Initial forms (at start of word)
            mask |= FeatureMask::MEDI; // Medial forms (middle of word)
            mask |= FeatureMask::FINA; // Final forms (end of word)
            mask |= FeatureMask::ISOL; // Isolated forms (standalone)
                                       // Note: RLIG (required ligatures) already in default for
                                       // lam-alef ligatures
        }

        // Indic scripts - require complex conjunct formation and reordering
        Devanagari | Bengali | Gujarati | Gurmukhi | Kannada | Malayalam | Oriya | Tamil
        | Telugu => {
            mask |= FeatureMask::NUKT; // Nukta forms
            mask |= FeatureMask::AKHN; // Akhand ligatures
            mask |= FeatureMask::RPHF; // Reph form
            mask |= FeatureMask::RKRF; // Rakar form
            mask |= FeatureMask::PREF; // Pre-base forms
            mask |= FeatureMask::BLWF; // Below-base forms
            mask |= FeatureMask::ABVF; // Above-base forms
            mask |= FeatureMask::HALF; // Half forms
            mask |= FeatureMask::PSTF; // Post-base forms
            mask |= FeatureMask::VATU; // Vattu variants
            mask |= FeatureMask::CJCT; // Conjunct forms
        }

        // Myanmar (Burmese) - has complex reordering
        Myanmar => {
            mask |= FeatureMask::PREF; // Pre-base forms
            mask |= FeatureMask::BLWF; // Below-base forms
            mask |= FeatureMask::PSTF; // Post-base forms
        }

        // Khmer - has complex reordering and stacking
        Khmer => {
            mask |= FeatureMask::PREF; // Pre-base forms
            mask |= FeatureMask::BLWF; // Below-base forms
            mask |= FeatureMask::ABVF; // Above-base forms
            mask |= FeatureMask::PSTF; // Post-base forms
        }

        // Thai - has tone marks and vowel reordering
        Thai => {
            // Thai mostly uses default features, but may have some special marks
            // The default mask is sufficient for most Thai fonts
        }

        // Hebrew - may have contextual forms but less complex than Arabic
        Hebrew => {
            // Hebrew fonts may use contextual alternates already in default
            // Some fonts have special features but they're rare
        }

        // Hangul (Korean) - has complex syllable composition
        Hangul => {
            // Note: Hangul jamo features (LJMO, VJMO, TJMO) are not available in allsorts'
            // FeatureMask Most modern Hangul fonts work correctly with the default
            // features as syllable composition is usually handled at a lower level
        }

        // Ethiopic - has syllabic script with some ligatures
        Ethiopic => {
            // Default features are usually sufficient
            // LIGA and CLIG already in default mask
        }

        // Latin, Greek, Cyrillic - standard features are sufficient
        Latin | Greek | Cyrillic => {
            // Default mask includes all needed features:
            // - LIGA: standard ligatures (fi, fl, etc.)
            // - CLIG: contextual ligatures
            // - CALT: contextual alternates
            // - CCMP: mark composition
        }

        // Georgian - uses standard features
        Georgian => {
            // Default features sufficient
        }

        // CJK scripts (Hiragana, Katakana, Mandarin/Hani)
        Hiragana | Katakana | Mandarin => {
            // CJK fonts may use vertical alternates, but those are controlled
            // by writing-mode, not GSUB features in the horizontal direction.
            // Default features are sufficient.
        }

        // Sinhala - Indic-derived but simpler
        Sinhala => {
            mask |= FeatureMask::AKHN; // Akhand ligatures
            mask |= FeatureMask::RPHF; // Reph form
            mask |= FeatureMask::VATU; // Vattu variants
        }
    }

    mask
}

/// Maps the layout engine's `Script` enum to an OpenType script tag `u32`.
fn to_opentype_script_tag(script: Script) -> u32 {
    use Script::*;
    // Tags from https://docs.microsoft.com/en-us/typography/opentype/spec/scripttags
    match script {
        Arabic => u32::from_be_bytes(*b"arab"),
        Bengali => u32::from_be_bytes(*b"beng"),
        Cyrillic => u32::from_be_bytes(*b"cyrl"),
        Devanagari => u32::from_be_bytes(*b"deva"),
        Ethiopic => u32::from_be_bytes(*b"ethi"),
        Georgian => u32::from_be_bytes(*b"geor"),
        Greek => u32::from_be_bytes(*b"grek"),
        Gujarati => u32::from_be_bytes(*b"gujr"),
        Gurmukhi => u32::from_be_bytes(*b"guru"),
        Hangul => u32::from_be_bytes(*b"hang"),
        Hebrew => u32::from_be_bytes(*b"hebr"),
        Hiragana => u32::from_be_bytes(*b"kana"),
        Kannada => u32::from_be_bytes(*b"knda"),
        Katakana => u32::from_be_bytes(*b"kana"),
        Khmer => u32::from_be_bytes(*b"khmr"),
        Latin => u32::from_be_bytes(*b"latn"),
        Malayalam => u32::from_be_bytes(*b"mlym"),
        Mandarin => u32::from_be_bytes(*b"hani"),
        Myanmar => u32::from_be_bytes(*b"mymr"),
        Oriya => u32::from_be_bytes(*b"orya"),
        Sinhala => u32::from_be_bytes(*b"sinh"),
        Tamil => u32::from_be_bytes(*b"taml"),
        Telugu => u32::from_be_bytes(*b"telu"),
        Thai => u32::from_be_bytes(*b"thai"),
    }
}

/// Parses a CSS-style font-feature-settings string like `"liga"`, `"liga=0"`, or `"ss01"`.
/// Returns an OpenType tag and a value.
fn parse_font_feature(feature_str: &str) -> Option<(u32, u32)> {
    let mut parts = feature_str.split('=');
    let tag_str = parts.next()?.trim();
    let value_str = parts.next().unwrap_or("1").trim(); // Default to 1 (on) if no value

    // OpenType feature tags must be 4 characters long.
    if tag_str.len() > 4 {
        return None;
    }
    // Pad with spaces if necessary
    let padded_tag_str = format!("{:<4}", tag_str);

    let tag = u32::from_be_bytes(padded_tag_str.as_bytes().try_into().ok()?);
    let value = value_str.parse::<u32>().ok()?;

    Some((tag, value))
}

/// A helper to add OpenType features based on CSS `font-variant-*` properties.
fn add_variant_features(style: &StyleProperties, features: &mut Vec<FeatureInfo>) {
    // Helper to add a feature that is simply "on".
    let mut add_on = |tag_str: &[u8; 4]| {
        features.push(FeatureInfo {
            feature_tag: u32::from_be_bytes(*tag_str),
            alternate: None,
        });
    };

    // Note on disabling features: The CSS properties `font-variant-ligatures: none` or
    // `no-common-ligatures` are meant to disable features that may be on by default for a
    // given script. The `allsorts` API for applying custom features is additive and does not
    // currently support disabling default features. This implementation only handles enabling
    // non-default features.

    // Ligatures
    match style.font_variant_ligatures {
        FontVariantLigatures::Discretionary => add_on(b"dlig"),
        FontVariantLigatures::Historical => add_on(b"hlig"),
        FontVariantLigatures::Contextual => add_on(b"calt"),
        _ => {} // Other cases are either default-on or require disabling.
    }

    // Caps
    match style.font_variant_caps {
        FontVariantCaps::SmallCaps => add_on(b"smcp"),
        FontVariantCaps::AllSmallCaps => {
            add_on(b"c2sc");
            add_on(b"smcp");
        }
        FontVariantCaps::PetiteCaps => add_on(b"pcap"),
        FontVariantCaps::AllPetiteCaps => {
            add_on(b"c2pc");
            add_on(b"pcap");
        }
        FontVariantCaps::Unicase => add_on(b"unic"),
        FontVariantCaps::TitlingCaps => add_on(b"titl"),
        FontVariantCaps::Normal => {}
    }

    // Numeric
    match style.font_variant_numeric {
        FontVariantNumeric::LiningNums => add_on(b"lnum"),
        FontVariantNumeric::OldstyleNums => add_on(b"onum"),
        FontVariantNumeric::ProportionalNums => add_on(b"pnum"),
        FontVariantNumeric::TabularNums => add_on(b"tnum"),
        FontVariantNumeric::DiagonalFractions => add_on(b"frac"),
        FontVariantNumeric::StackedFractions => add_on(b"afrc"),
        FontVariantNumeric::Ordinal => add_on(b"ordn"),
        FontVariantNumeric::SlashedZero => add_on(b"zero"),
        FontVariantNumeric::Normal => {}
    }
}

/// Maps the `hyphenation::Language` enum to an OpenType language tag `u32`.
#[cfg(feature = "text_layout_hyphenation")]
fn to_opentype_lang_tag(lang: hyphenation::Language) -> u32 {
    use hyphenation::Language::*;
    // A complete list of language tags can be found at:
    // https://docs.microsoft.com/en-us/typography/opentype/spec/languagetags
    let tag_bytes = match lang {
        Afrikaans => *b"AFK ",
        Albanian => *b"SQI ",
        Armenian => *b"HYE ",
        Assamese => *b"ASM ",
        Basque => *b"EUQ ",
        Belarusian => *b"BEL ",
        Bengali => *b"BEN ",
        Bulgarian => *b"BGR ",
        Catalan => *b"CAT ",
        Chinese => *b"ZHS ",
        Coptic => *b"COP ",
        Croatian => *b"HRV ",
        Czech => *b"CSY ",
        Danish => *b"DAN ",
        Dutch => *b"NLD ",
        EnglishGB => *b"ENG ",
        EnglishUS => *b"ENU ",
        Esperanto => *b"ESP ",
        Estonian => *b"ETI ",
        Ethiopic => *b"ETI ",
        Finnish => *b"FIN ",
        FinnishScholastic => *b"FIN ",
        French => *b"FRA ",
        Friulan => *b"FRL ",
        Galician => *b"GLC ",
        Georgian => *b"KAT ",
        German1901 => *b"DEU ",
        German1996 => *b"DEU ",
        GermanSwiss => *b"DES ",
        GreekAncient => *b"GRC ",
        GreekMono => *b"ELL ",
        GreekPoly => *b"ELL ",
        Gujarati => *b"GUJ ",
        Hindi => *b"HIN ",
        Hungarian => *b"HUN ",
        Icelandic => *b"ISL ",
        Indonesian => *b"IND ",
        Interlingua => *b"INA ",
        Irish => *b"IRI ",
        Italian => *b"ITA ",
        Kannada => *b"KAN ",
        Kurmanji => *b"KUR ",
        Latin => *b"LAT ",
        LatinClassic => *b"LAT ",
        LatinLiturgical => *b"LAT ",
        Latvian => *b"LVI ",
        Lithuanian => *b"LTH ",
        Macedonian => *b"MKD ",
        Malayalam => *b"MAL ",
        Marathi => *b"MAR ",
        Mongolian => *b"MNG ",
        NorwegianBokmal => *b"NOR ",
        NorwegianNynorsk => *b"NYN ",
        Occitan => *b"OCI ",
        Oriya => *b"ORI ",
        Pali => *b"PLI ",
        Panjabi => *b"PAN ",
        Piedmontese => *b"PMS ",
        Polish => *b"PLK ",
        Portuguese => *b"PTG ",
        Romanian => *b"ROM ",
        Romansh => *b"RMC ",
        Russian => *b"RUS ",
        Sanskrit => *b"SAN ",
        SerbianCyrillic => *b"SRB ",
        SerbocroatianCyrillic => *b"SHC ",
        SerbocroatianLatin => *b"SHL ",
        SlavonicChurch => *b"CSL ",
        Slovak => *b"SKY ",
        Slovenian => *b"SLV ",
        Spanish => *b"ESP ",
        Swedish => *b"SVE ",
        Tamil => *b"TAM ",
        Telugu => *b"TEL ",
        Thai => *b"THA ",
        Turkish => *b"TRK ",
        Turkmen => *b"TUK ",
        Ukrainian => *b"UKR ",
        Uppersorbian => *b"HSB ",
        Welsh => *b"CYM ",
    };
    u32::from_be_bytes(tag_bytes)
}

/// Internal shaping implementation - the single source of truth for text shaping.
/// Both FontRef and ParsedFont use this function.
fn shape_text_internal(
    parsed_font: &ParsedFont,
    text: &str,
    script: Script,
    language: crate::text3::script::Language,
    direction: BidiDirection,
    style: &StyleProperties,
) -> Result<Vec<Glyph>, LayoutError> {
    let script_tag = to_opentype_script_tag(script);
    #[cfg(feature = "text_layout_hyphenation")]
    let lang_tag = to_opentype_lang_tag(language);
    #[cfg(not(feature = "text_layout_hyphenation"))]
    let lang_tag = 0u32;

    let mut user_features: Vec<FeatureInfo> = style
        .font_features
        .iter()
        .filter_map(|s| parse_font_feature(s))
        .map(|(tag, value)| FeatureInfo {
            feature_tag: tag,
            alternate: if value > 1 {
                Some(value as usize)
            } else {
                None
            },
        })
        .collect();
    add_variant_features(style, &mut user_features);

    let opt_gdef = parsed_font.opt_gdef_table.as_ref().map(|v| &**v);

    let mut raw_glyphs: Vec<allsorts::gsub::RawGlyph<()>> = text
        .char_indices()
        .filter_map(|(cluster, ch)| {
            let glyph_index = parsed_font.lookup_glyph_index(ch as u32).unwrap_or(0);
            if cluster > u16::MAX as usize {
                None
            } else {
                Some(allsorts::gsub::RawGlyph {
                    unicodes: tinyvec::tiny_vec![[char; 1] => ch],
                    glyph_index,
                    liga_component_pos: cluster as u16,
                    glyph_origin: allsorts::gsub::GlyphOrigin::Char(ch),
                    flags: allsorts::gsub::RawGlyphFlags::empty(),
                    extra_data: (),
                    variation: None,
                })
            }
        })
        .collect();

    if let Some(gsub) = parsed_font.gsub_cache.as_ref() {
        let features = if user_features.is_empty() {
            Features::Mask(build_feature_mask_for_script(script))
        } else {
            Features::Custom(user_features.clone())
        };

        let dotted_circle_index = parsed_font
            .lookup_glyph_index(allsorts::DOTTED_CIRCLE as u32)
            .unwrap_or(0);
        gsub::apply(
            dotted_circle_index,
            gsub,
            opt_gdef,
            script_tag,
            Some(lang_tag),
            &features,
            None,
            parsed_font.num_glyphs(),
            &mut raw_glyphs,
        )
        .map_err(|e| LayoutError::ShapingError(e.to_string()))?;
    }

    let mut infos = gpos::Info::init_from_glyphs(opt_gdef, raw_glyphs);

    if let Some(gpos) = parsed_font.gpos_cache.as_ref() {
        let kern_table = parsed_font
            .opt_kern_table
            .as_ref()
            .map(|kt| kt.as_borrowed());
        let apply_kerning = kern_table.is_some();
        gpos::apply(
            gpos,
            opt_gdef,
            kern_table,
            apply_kerning,
            &Features::Custom(user_features),
            None,
            script_tag,
            Some(lang_tag),
            &mut infos,
        )
        .map_err(|e| LayoutError::ShapingError(e.to_string()))?;
    }

    let font_size = style.font_size_px;
    let scale_factor = if parsed_font.font_metrics.units_per_em > 0 {
        font_size / (parsed_font.font_metrics.units_per_em as f32)
    } else {
        0.01
    };

    let mut shaped_glyphs = Vec::new();
    for info in infos.iter() {
        let cluster = info.glyph.liga_component_pos as u32;
        let source_char = text
            .get(cluster as usize..)
            .and_then(|s| s.chars().next())
            .unwrap_or('\u{FFFD}');

        let base_advance = parsed_font.get_horizontal_advance(info.glyph.glyph_index);
        let advance = base_advance as f32 * scale_factor;
        let kerning = info.kerning as f32 * scale_factor;

        let (offset_x_units, offset_y_units) =
            if let allsorts::gpos::Placement::Distance(x, y) = info.placement {
                (x, y)
            } else {
                (0, 0)
            };
        let offset_x = offset_x_units as f32 * scale_factor;
        let offset_y = offset_y_units as f32 * scale_factor;

        let glyph = Glyph {
            glyph_id: info.glyph.glyph_index,
            codepoint: source_char,
            font_hash: parsed_font.get_hash(),
            font_metrics: LayoutFontMetrics {
                ascent: parsed_font.font_metrics.ascent,
                descent: parsed_font.font_metrics.descent,
                line_gap: parsed_font.font_metrics.line_gap,
                units_per_em: parsed_font.font_metrics.units_per_em,
            },
            style: Arc::new(style.clone()),
            source: GlyphSource::Char,
            logical_byte_index: cluster as usize,
            logical_byte_len: source_char.len_utf8(),
            content_index: 0,
            cluster,
            advance,
            kerning,
            offset: Point {
                x: offset_x,
                y: offset_y,
            },
            vertical_advance: 0.0,
            vertical_origin_y: 0.0,
            vertical_bearing: Point { x: 0.0, y: 0.0 },
            orientation: GlyphOrientation::Horizontal,
            script,
            bidi_level: BidiLevel::new(if direction.is_rtl() { 1 } else { 0 }),
        };
        shaped_glyphs.push(glyph);
    }

    Ok(shaped_glyphs)
}

/// Public helper function to shape text for ParsedFont, returning Glyph
/// This is used by the ParsedFontTrait implementation for ParsedFont
pub fn shape_text_for_parsed_font(
    parsed_font: &ParsedFont,
    text: &str,
    script: Script,
    language: crate::text3::script::Language,
    direction: BidiDirection,
    style: &StyleProperties,
) -> Result<Vec<Glyph>, LayoutError> {
    // Delegate to the single internal implementation
    shape_text_internal(parsed_font, text, script, language, direction, style)
}
