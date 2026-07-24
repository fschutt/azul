//! Default / concrete implementations of the text3 trait abstractions.
//!
//! This module bridges the generic text3 layout engine and the concrete
//! `FontRef` / `ParsedFont` types.  It provides:
//!
//! - `ParsedFontTrait` implementation for `FontRef`
//! - Font loading via `PathLoader`
//! - The core `shape_text_internal` shaping function

use std::{path::Path, sync::Arc};

use allsorts::{
    gpos,
    gsub::{self, Feature, FeatureInfo, FeatureMask, FeatureMaskExt},
};
use azul_core::geom::LogicalSize;
use azul_css::props::basic::FontRef;

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

/// Creates a `FontRef` from font bytes by parsing them into a `ParsedFont`.
///
/// This is a bridge function that:
///
/// 1. Parses the bytes into a `ParsedFont`
/// 2. Wraps it in a `FontRef` with proper reference counting
///
/// # Arguments
///
/// - `font_bytes` - The raw font file data
/// - `font_index` - Index of the font in a font collection (0 for single fonts)
/// - `parse_outlines` - Whether to parse glyph outlines (expensive, usually false for layout)
#[must_use] pub fn font_ref_from_bytes(
    font_bytes: &[u8],
    font_index: usize,
    parse_outlines: bool,
) -> Option<FontRef> {
    // Parse the font bytes into ParsedFont
    let mut warnings = Vec::new();
    let parsed_font = ParsedFont::from_bytes(font_bytes, font_index, &mut warnings)?;

    Some(crate::parsed_font_to_font_ref(parsed_font))
}

/// A `FontLoader` that parses font data from a byte slice.
///
/// It is designed to be used in conjunction with a mechanism that reads font files
/// from paths into memory. This loader simply handles the parsing aspect.
#[derive(Copy, Debug, Default, Clone)]
pub struct PathLoader;

impl PathLoader {
    /// Creates a new `PathLoader`.
    #[must_use] pub const fn new() -> Self {
        Self
    }

    /// Read a font from disk and parse via the lazy-LocaGlyf path.
    /// Convenience wrapper for callers that have a path but no
    /// `Arc<FontBytes>` yet — uses a heap read (`Owned`) since a
    /// loose path won't go through the fontconfig dedup cache.
    pub(crate) fn load_from_path(self, path: &Path, font_index: usize) -> Result<FontRef, LayoutError> {
        let font_bytes = std::fs::read(path).map_err(|_| {
            LayoutError::FontNotFound(FontSelector {
                family: path.to_string_lossy().into_owned(),
                weight: rust_fontconfig::FcWeight::Normal,
                style: crate::text3::cache::FontStyle::Normal,
                unicode_ranges: Vec::new(),
            })
        })?;
        let arc_owned = Arc::<[u8]>::from(font_bytes);
        let bytes = Arc::new(rust_fontconfig::FontBytes::Owned(arc_owned));
        self.load_font_shared(bytes, font_index)
    }

    /// Lazy-friendly loader: takes an `Arc<FontBytes>` (typically
    /// from [`rust_fontconfig::FcFontCache::get_font_bytes`]) and
    /// uses the [`ParsedFont::from_bytes_shared`] constructor so
    /// `LocaGlyf::load` is deferred until the first glyph decode.
    ///
    /// This is the only loader on the production path —
    /// `load_fonts_from_disk` calls this via the closure passed
    /// into `FontManager::load_missing_for_chains`. Fonts that
    /// never get rasterized (common — every face of a `.ttc` gets a
    /// `FontId`, but pages only hit a couple of them) skip their
    /// per-face loca+glyf materialisation entirely; with
    /// `FontBytes::Mmapped` the unread pages also never count
    /// toward RSS.
    /// # Errors
    ///
    /// Returns a `LayoutError` if the font cannot be loaded.
    pub fn load_font_shared(
        &self,
        font_bytes: Arc<rust_fontconfig::FontBytes>,
        font_index: usize,
    ) -> Result<FontRef, LayoutError> {
        let mut warnings = Vec::new();
        let parsed_font = ParsedFont::from_bytes_shared(font_bytes, font_index, &mut warnings)
            .ok_or_else(|| {
                LayoutError::ShapingError("Failed to parse font with allsorts".to_string())
            })?;
        Ok(crate::parsed_font_to_font_ref(parsed_font))
    }
}

impl FontManager<FontRef> {
    /// Evict the cached `LocaGlyf` for every face that hasn't had a
    /// `get_or_decode_glyph` call within the last `idle` duration.
    /// Only `LocaGlyfState::Deferred` faces (the production lazy
    /// path) can be evicted — they keep their source `Arc<[u8]>` so
    /// the next glyph access re-parses cheaply. `LocaGlyfState::Loaded`
    /// faces from the eager path stay put.
    ///
    /// Returns the number of faces evicted. Embedders can call this
    /// from a memory-pressure hook or on a timer; servo-shot
    /// exposes it via `--azul-evict-after-each` for measurement.
    #[allow(clippy::cast_possible_truncation)] // bounded layout/render numeric cast
    pub fn evict_unused(&self, idle: std::time::Duration) -> usize {
        use crate::font::parsed::ParsedFont;
        let Ok(parsed) = self.parsed_fonts.lock() else {
            return 0;
        };
        // We compare against the same monotonic clock the font's
        // `last_used` is sampled from. `last_used == 0` means
        // "never touched" -> eligible. Otherwise we only evict if
        // `now_nanos - last_used >= idle.as_nanos()`.
        let cutoff = idle.as_nanos() as u64;
        let now_nanos = crate::font::parsed::monotonic_now_nanos();
        let mut evicted = 0usize;
        for font_ref in parsed.values() {
            let font: &ParsedFont = crate::font_ref_to_parsed_font(font_ref);
            let last = font.last_used_nanos();
            // Untouched faces are eligible immediately. Touched
            // faces need to be `idle` past their last use.
            let stale = last == 0 || now_nanos.saturating_sub(last) >= cutoff;
            if stale && font.evict_loca_glyf() {
                evicted += 1;
            }
        }
        evicted
    }
}


// ParsedFontTrait Implementation for FontRef

// Implement ShallowClone for FontRef
impl ShallowClone for FontRef {
    fn shallow_clone(&self) -> Self {
        // FontRef::clone increments the reference count
        self.clone()
    }
}

// Use crate::font_ref_to_parsed_font instead of a local duplicate

impl ParsedFontTrait for FontRef {
    // +spec:block-formatting-context:21ec9a - bidi direction handled during text shaping for vertical writing modes
    fn shape_text(
        &self,
        text: &str,
        script: Script,
        language: crate::text3::script::Language,
        direction: BidiDirection,
        style: &StyleProperties,
    ) -> Result<Vec<Glyph>, LayoutError> {
        // Delegate to the inner ParsedFont's shape_text, passing self as font_ref
        let parsed = crate::font_ref_to_parsed_font(self);
        parsed.shape_text_for_font_ref(self, text, script, language, direction, style)
    }

    fn get_hash(&self) -> u64 {
        crate::font_ref_to_parsed_font(self).hash
    }

    fn resolve_font_hash(&self, hash: u64) -> Option<Self> {
        let parsed = crate::font_ref_to_parsed_font(self);
        if parsed.hash == hash {
            Some(self.clone())
        } else {
            parsed.get_variation_instance_by_hash(hash)
        }
    }

    fn get_glyph_size(&self, glyph_id: u16, font_size: f32) -> Option<LogicalSize> {
        crate::font_ref_to_parsed_font(self).get_glyph_size(glyph_id, font_size)
    }

    fn get_hyphen_glyph_and_advance(&self, font_size: f32) -> Option<(u16, f32)> {
        crate::font_ref_to_parsed_font(self).get_hyphen_glyph_and_advance(font_size)
    }

    fn get_kashida_glyph_and_advance(&self, font_size: f32) -> Option<(u16, f32)> {
        crate::font_ref_to_parsed_font(self).get_kashida_glyph_and_advance(font_size)
    }

    fn has_glyph(&self, codepoint: u32) -> bool {
        crate::font_ref_to_parsed_font(self).has_glyph(codepoint)
    }

    fn get_vertical_metrics(&self, glyph_id: u16) -> Option<VerticalMetrics> {
        crate::font_ref_to_parsed_font(self).get_vertical_metrics(glyph_id)
    }

    fn get_font_metrics(&self) -> LayoutFontMetrics {
        crate::font_ref_to_parsed_font(self).font_metrics
    }

    fn num_glyphs(&self) -> u16 {
        crate::font_ref_to_parsed_font(self).num_glyphs
    }

    fn get_space_width(&self) -> Option<usize> {
        crate::font_ref_to_parsed_font(self).get_space_width()
    }
}

/// Extension trait for `FontRef` to provide access to font bytes and metrics
///
/// This trait provides methods that require access to the inner `ParsedFont` data.
pub trait FontRefExt {
    /// Get the original font bytes. Returns an empty slice when the
    /// underlying `ParsedFont` was created without retaining its
    /// source bytes (the default since the lazy-font-loading refactor).
    /// Callers that need the bytes for PDF embedding must construct
    /// the `ParsedFont` via `ParsedFont::with_source_bytes`.
    fn get_bytes(&self) -> &[u8];
    /// Get the full font metrics (PDF-style metrics from HEAD, HHEA, OS/2 tables)
    fn get_full_font_metrics(&self) -> azul_css::props::basic::FontMetrics;
}

impl FontRefExt for FontRef {
    fn get_bytes(&self) -> &[u8] {
        crate::font_ref_to_parsed_font(self)
            .original_bytes
            .as_ref()
            .map_or(&[], |b| b.as_slice())
    }

    fn get_full_font_metrics(&self) -> azul_css::props::basic::FontMetrics {
        use azul_css::{OptionI16, OptionU16, OptionU32};

        let parsed = crate::font_ref_to_parsed_font(self);
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
    /// Delegates to `shape_text_internal` and converts the font reference.
    fn shape_text_for_font_ref(
        &self,
        _font_ref: &FontRef,
        text: &str,
        script: Script,
        language: crate::text3::script::Language,
        direction: BidiDirection,
        style: &StyleProperties,
    ) -> Result<Vec<Glyph>, LayoutError> {
        // `shape_text_internal` already stamps each glyph with `font_hash`
        // and `font_metrics` derived from `self`, which is the same
        // `ParsedFont` backing `_font_ref`, so no per-glyph rewrite is needed.
        shape_text_internal(self, text, script, language, direction, style)
    }

    const fn get_hash(&self) -> u64 {
        self.hash
    }

    fn get_glyph_size(&self, glyph_id: u16, font_size_px: f32) -> Option<LogicalSize> {
        self.get_or_decode_glyph(glyph_id).map(|record| {
            let units_per_em = f32::from(self.font_metrics.units_per_em);
            let scale_factor = if units_per_em > 0.0 {
                font_size_px / units_per_em
            } else {
                FALLBACK_SCALE
            };

            // max_x, max_y, min_x, min_y in font units
            let bbox = &record.bounding_box;

            LogicalSize {
                width: f32::from(bbox.max_x - bbox.min_x) * scale_factor,
                height: f32::from(bbox.max_y - bbox.min_y) * scale_factor,
            }
        })
    }

    fn get_hyphen_glyph_and_advance(&self, font_size: f32) -> Option<(u16, f32)> {
        let glyph_id = self.lookup_glyph_index('-' as u32)?;
        let advance_units = self.get_horizontal_advance(glyph_id);
        let scale_factor = if self.font_metrics.units_per_em > 0 {
            font_size / f32::from(self.font_metrics.units_per_em)
        } else {
            return None;
        };
        let scaled_advance = f32::from(advance_units) * scale_factor;
        Some((glyph_id, scaled_advance))
    }

    fn get_kashida_glyph_and_advance(&self, font_size: f32) -> Option<(u16, f32)> {
        // U+0640 is the Arabic Tatweel character, used for kashida justification.
        let glyph_id = self.lookup_glyph_index('\u{0640}' as u32)?;
        let advance_units = self.get_horizontal_advance(glyph_id);
        let scale_factor = if self.font_metrics.units_per_em > 0 {
            font_size / f32::from(self.font_metrics.units_per_em)
        } else {
            return None;
        };
        let scaled_advance = f32::from(advance_units) * scale_factor;
        Some((glyph_id, scaled_advance))
    }
}

/// Fallback scale factor when `units_per_em` is zero (corrupt/broken font).
const FALLBACK_SCALE: f32 = 0.01;

// Helper Functions

/// Builds a `FeatureMask` with the appropriate OpenType features for a given script.
/// This ensures proper text shaping for complex scripts like Arabic, Devanagari, etc.
///
/// The function includes:
/// - Common features for all scripts (ligatures, contextual alternates, etc.)
/// - Script-specific features (positional forms for Arabic, conjuncts for Indic, etc.)
///
/// This is designed to be stable and explicit - we control exactly which features
/// are enabled rather than relying on allsorts' defaults which may change.
#[allow(clippy::match_same_arms)] // enum/value mapping/dispatch table: one arm per input variant (or cross-type bindings that can't merge)
fn build_feature_mask_for_script(script: Script) -> FeatureMask {
    use Script::{Arabic, Devanagari, Bengali, Gujarati, Gurmukhi, Kannada, Malayalam, Oriya, Tamil, Telugu, Myanmar, Khmer, Thai, Hebrew, Hangul, Ethiopic, Latin, Greek, Cyrillic, Georgian, Hiragana, Katakana, Mandarin, Sinhala};

    // Start with common features that apply to most scripts
    let mut mask = FeatureMask::default_mask(); // Includes: CALT, CCMP, CLIG, LIGA, LOCL, RLIG

    // Add script-specific features
    match script {
        // Arabic and related scripts - require positional forms
        Arabic => {
            mask |= Feature::INIT; // Initial forms (at start of word)
            mask |= Feature::MEDI; // Medial forms (middle of word)
            mask |= Feature::FINA; // Final forms (end of word)
            mask |= Feature::ISOL; // Isolated forms (standalone)
                                       // Note: RLIG (required ligatures) already in default for
                                       // lam-alef ligatures
        }

        // Indic scripts - require complex conjunct formation and reordering
        Devanagari | Bengali | Gujarati | Gurmukhi | Kannada | Malayalam | Oriya | Tamil
        | Telugu => {
            mask |= Feature::NUKT; // Nukta forms
            mask |= Feature::AKHN; // Akhand ligatures
            mask |= Feature::RPHF; // Reph form
            mask |= Feature::RKRF; // Rakar form
            mask |= Feature::PREF; // Pre-base forms
            mask |= Feature::BLWF; // Below-base forms
            mask |= Feature::ABVF; // Above-base forms
            mask |= Feature::HALF; // Half forms
            mask |= Feature::PSTF; // Post-base forms
            mask |= Feature::VATU; // Vattu variants
            mask |= Feature::CJCT; // Conjunct forms
        }

        // Myanmar (Burmese) - has complex reordering
        Myanmar => {
            mask |= Feature::PREF; // Pre-base forms
            mask |= Feature::BLWF; // Below-base forms
            mask |= Feature::PSTF; // Post-base forms
        }

        // Khmer - has complex reordering and stacking
        Khmer => {
            mask |= Feature::PREF; // Pre-base forms
            mask |= Feature::BLWF; // Below-base forms
            mask |= Feature::ABVF; // Above-base forms
            mask |= Feature::PSTF; // Post-base forms
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
            mask |= Feature::AKHN; // Akhand ligatures
            mask |= Feature::RPHF; // Reph form
            mask |= Feature::VATU; // Vattu variants
        }
    }

    mask
}

/// Maps the layout engine's `Script` enum to an OpenType script tag `u32`.
#[allow(clippy::match_same_arms)] // enum/value mapping/dispatch table: one arm per input variant (or cross-type bindings that can't merge)
const fn to_opentype_script_tag(script: Script) -> u32 {
    use Script::{Arabic, Bengali, Cyrillic, Devanagari, Ethiopic, Georgian, Greek, Gujarati, Gurmukhi, Hangul, Hebrew, Hiragana, Kannada, Katakana, Khmer, Latin, Malayalam, Mandarin, Myanmar, Oriya, Sinhala, Tamil, Telugu, Thai};
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
        // OpenType does not define a separate Hiragana script tag;
        // both Hiragana and Katakana intentionally use "kana".
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
    let padded_tag_str = format!("{tag_str:<4}");

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
#[allow(clippy::match_same_arms)] // enum/value mapping/dispatch table: one arm per input variant (or cross-type bindings that can't merge)
const fn to_opentype_lang_tag(lang: hyphenation::Language) -> u32 {
    use hyphenation::Language::{Afrikaans, Albanian, Armenian, Assamese, Basque, Belarusian, Bengali, Bulgarian, Catalan, Chinese, Coptic, Croatian, Czech, Danish, Dutch, EnglishGB, EnglishUS, Esperanto, Estonian, Ethiopic, Finnish, FinnishScholastic, French, Friulan, Galician, Georgian, German1901, German1996, GermanSwiss, GreekAncient, GreekMono, GreekPoly, Gujarati, Hindi, Hungarian, Icelandic, Indonesian, Interlingua, Irish, Italian, Kannada, Kurmanji, Latin, LatinClassic, LatinLiturgical, Latvian, Lithuanian, Macedonian, Malayalam, Marathi, Mongolian, NorwegianBokmal, NorwegianNynorsk, Occitan, Oriya, Pali, Panjabi, Piedmontese, Polish, Portuguese, Romanian, Romansh, Russian, Sanskrit, SerbianCyrillic, SerbocroatianCyrillic, SerbocroatianLatin, SlavonicChurch, Slovak, Slovenian, Spanish, Swedish, Tamil, Telugu, Thai, Turkish, Turkmen, Ukrainian, Uppersorbian, Welsh};
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
        Ethiopic => *b"ETH ",
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
/// Both `FontRef` and `ParsedFont` use this function.
#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss, clippy::cast_sign_loss)] // bounded layout/render numeric cast
#[allow(clippy::similar_names)] // domain-standard coordinate/geometry/short-lived names
#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
fn shape_text_internal(
    parsed_font: &ParsedFont,
    text: &str,
    script: Script,
    language: crate::text3::script::Language,
    direction: BidiDirection,
    style: &StyleProperties,
) -> Result<Vec<Glyph>, LayoutError> {
    // Variable fonts are converted to a static face on first use of a
    // coordinate tuple. Shaping, outline decoding, CPU rendering, and PDF
    // embedding then all operate on the same bytes and share the cached face.
    if let Some(instance) =
        parsed_font.get_or_create_variation_instance(&style.font_variations)
    {
        return shape_text_internal(
            crate::font_ref_to_parsed_font(&instance),
            text,
            script,
            language,
            direction,
            style,
        );
    }

    let script_tag = to_opentype_script_tag(script);
    #[cfg(feature = "text_layout_hyphenation")]
    let lang_tag = to_opentype_lang_tag(language);
    #[cfg(not(feature = "text_layout_hyphenation"))]
    let lang_tag = 0u32;

    // +spec:text-alignment-spacing:4357e6 - non-zero letter-spacing should disable optional ligatures; allsorts API is additive-only so default liga cannot be disabled here
    // +spec:text-alignment-spacing:24d624 - cursive script letter-spacing behavior is advisory (outside CSS scope per spec note)
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

    let opt_gdef = parsed_font.opt_gdef_table.as_deref();

    let mut raw_glyphs: Vec<gsub::RawGlyph<()>> = Vec::new();
    {
        let mut ci = 0usize;
        while ci < text.len() {
            let Some(ch) = text[ci..].chars().next() else {
                break;
            };
            let glyph_index = parsed_font.lookup_glyph_index(ch as u32).unwrap_or(0);
            // NOTE: `liga_component_pos` MUST be left at allsorts' managed default (0)
            // here. It is a GPOS ligature-COMPONENT index, and mark-to-mark /
            // mark-to-ligature attachment (gpos::forall_mark_mark_glyph_pairs,
            // gpos::markligpos) is gated on equality of this field between glyphs.
            // Overloading it with the source byte offset (as an earlier version did)
            // made every glyph carry a distinct value, silently disabling mkmk stacking
            // and mis-selecting ligature-component anchors. Source byte offsets are
            // instead reconstructed after shaping from each glyph's `unicodes` (see the
            // read-back loop below), which also removes the old u16 byte-offset cap that
            // dropped every glyph past byte 65535.
            raw_glyphs.push(gsub::RawGlyph {
                unicodes: tinyvec::tiny_vec![[char; 1] => ch],
                glyph_index,
                liga_component_pos: 0,
                glyph_origin: gsub::GlyphOrigin::Char(ch),
                flags: gsub::RawGlyphFlags::empty(),
                extra_data: (),
                variation: None,
            });
            ci += ch.len_utf8();
        }
    }

    if let Some(gsub) = parsed_font.gsub() {
        // Always start from the script's default feature mask (LIGA, CLIG, CALT,
        // CCMP, LOCL, RLIG + script-specific shaping features) and additively layer
        // any user-supplied features on top. gsub::apply applies the mask AND the
        // custom features, so these are NOT mutually exclusive. The previous code
        // replaced the mask with `empty()` whenever ANY font-feature/font-variant
        // was set, which silently disabled default ligatures/contextual-alternates
        // for Latin-family text (ScriptType::Default only re-adds CCMP|RLIG|LOCL).
        let feature_mask = build_feature_mask_for_script(script);
        let custom_features: &[FeatureInfo] = user_features.as_slice();

        let dotted_circle_index = parsed_font
            .lookup_glyph_index(allsorts::DOTTED_CIRCLE as u32)
            .unwrap_or(0);
        gsub::apply(
            dotted_circle_index,
            gsub,
            opt_gdef,
            script_tag,
            Some(lang_tag),
            feature_mask,
            custom_features,
            None,
            parsed_font.num_glyphs(),
            &mut raw_glyphs,
        )
        .map_err(|e| LayoutError::ShapingError(e.to_string()))?;
    }

    let mut infos = gpos::Info::init_from_glyphs(opt_gdef, raw_glyphs);

    if let Some(gpos) = parsed_font.gpos() {
        let kern_table = parsed_font
            .opt_kern_table
            .as_ref()
            .map(|kt| kt.as_borrowed());
        let apply_kerning = true; // Always enable GPOS kern feature (not just when legacy kern table exists)
        gpos::apply(
            gpos,
            opt_gdef,
            kern_table,
            apply_kerning,
            FeatureMask::empty(),
            &user_features,
            None,
            script_tag,
            Some(lang_tag),
            &mut infos,
        )
        .map_err(|e| LayoutError::ShapingError(e.to_string()))?;
    } else {
        // No GPOS table: apply the legacy `kern` table (and fallback mark
        // positioning) directly, so fonts that ship only a legacy `kern` table
        // still kern — matching CoreText/HarfBuzz behavior. Without this,
        // GPOS-less fonts got zero kerning.
        let kern_table = parsed_font
            .opt_kern_table
            .as_ref()
            .map(|kt| kt.as_borrowed());
        gpos::apply_fallback(kern_table, script_tag, &mut infos)
            .map_err(|e| LayoutError::ShapingError(e.to_string()))?;
    }

    let font_size = style.font_size_px;
    let scale_factor = if parsed_font.font_metrics.units_per_em > 0 {
        font_size / f32::from(parsed_font.font_metrics.units_per_em)
    } else {
        FALLBACK_SCALE
    };

    let font_hash = parsed_font.get_hash();
    let font_metrics = LayoutFontMetrics {
        ascent: parsed_font.font_metrics.ascent,
        descent: parsed_font.font_metrics.descent,
        line_gap: parsed_font.font_metrics.line_gap,
        units_per_em: parsed_font.font_metrics.units_per_em,
        x_height: parsed_font.font_metrics.x_height,
        cap_height: parsed_font.font_metrics.cap_height,
    };
    let style_arc = Arc::new(style.clone());
    let bidi_level = BidiLevel::new(u8::from(direction.is_rtl()));

    let mut shaped_glyphs = Vec::new();
    // Reconstruct source byte spans by walking the source text in logical order,
    // consuming each glyph's `unicodes`. This replaces the removed liga_component_pos
    // byte-offset overload. A ligature glyph carries ALL of its component chars in
    // `unicodes`, so its span covers every merged component (fixing the ligature
    // logical_byte_len that previously reported only the first component's length).
    // Multiple-substitution duplicates (MULTI_SUBST_DUP) and unicode-less inserted
    // glyphs share the current cursor position and do not advance it.
    let mut byte_cursor = 0usize;
    for info in &infos {
        let uni_len: usize = info.glyph.unicodes.iter().map(|c| c.len_utf8()).sum();
        let (byte_index, byte_len) = if info.glyph.multi_subst_dup() || uni_len == 0 {
            (byte_cursor.min(text.len()), 0)
        } else {
            let start = byte_cursor.min(text.len());
            byte_cursor = (byte_cursor + uni_len).min(text.len());
            (start, uni_len)
        };
        let cluster = byte_index as u32;
        let source_char = info
            .glyph
            .unicodes
            .first()
            .copied()
            .or_else(|| text.get(byte_index..).and_then(|s| s.chars().next()))
            .unwrap_or('\u{FFFD}');

        let base_advance = parsed_font.get_horizontal_advance(info.glyph.glyph_index);
        // Use hinted advance width when available (matches FreeType/Chrome behavior).
        // Hinting grid-fits at an INTEGER ppem, so rescale the result back to the exact
        // (fractional) font size to keep the advance on the SAME size basis as the GPOS
        // offsets and kerning below (which use the unrounded scale_factor). Otherwise a
        // run at e.g. 14.4px would pair a 14px-basis advance with 14.4px-basis positioning.
        let ppem = font_size.round().max(1.0) as u16;
        let advance = parsed_font
            .get_hinted_advance_px(info.glyph.glyph_index, ppem)
            .map_or_else(
                || f32::from(base_advance) * scale_factor,
                |hinted| hinted * font_size / f32::from(ppem),
            );
        let kerning = f32::from(info.kerning) * scale_factor;

        let (offset_x_units, offset_y_units) =
            if let gpos::Placement::Distance(x, y) = info.placement {
                (x, y)
            } else {
                (0, 0)
            };
        let offset_x = offset_x_units as f32 * scale_factor;
        let offset_y = offset_y_units as f32 * scale_factor;

        let vert = parsed_font.get_vertical_metrics(info.glyph.glyph_index);
        let glyph = Glyph {
            glyph_id: info.glyph.glyph_index,
            codepoint: source_char,
            font_hash,
            font_metrics,
            style: Arc::clone(&style_arc),
            source: GlyphSource::Char,
            logical_byte_index: byte_index,
            logical_byte_len: byte_len,
            content_index: 0,
            cluster,
            advance,
            kerning,
            offset: Point {
                x: offset_x,
                y: offset_y,
            },
            vertical_advance: vert.as_ref().map_or(0.0, |v| v.advance * font_size),
            vertical_origin_y: vert.as_ref().map_or(0.0, |v| v.origin_y * font_size),
            vertical_bearing: vert
                .map_or(Point { x: 0.0, y: 0.0 }, |v| Point { x: v.bearing_x * font_size, y: v.bearing_y * font_size }),
            orientation: GlyphOrientation::Horizontal,
            script,
            bidi_level,
        };
        shaped_glyphs.push(glyph);
    }

    Ok(shaped_glyphs)
}

/// Public helper function to shape text for `ParsedFont`, returning Glyph
/// This is used by the `ParsedFontTrait` implementation for `ParsedFont`
/// # Errors
///
/// Returns a `LayoutError` if the text cannot be shaped.
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

#[cfg(test)]
#[allow(clippy::float_cmp, clippy::cast_lossless, clippy::unreadable_literal)]
mod autotest_generated {
    use std::time::Duration;

    use rust_fontconfig::{FcFontCache, FontBytes, FontId};

    use super::*;
    use crate::text3::script::Language;

    /// Positive control: the built-in `Azul Mock Mono` TrueType face.
    const MOCK_MONO: &[u8] = crate::text3::mock_fonts::MOCK_MONO_TTF;

    /// Every `Script` variant, so the exhaustive mapping tables below can never
    /// silently miss one.
    const ALL_SCRIPTS: [Script; 24] = [
        Script::Arabic,
        Script::Bengali,
        Script::Cyrillic,
        Script::Devanagari,
        Script::Ethiopic,
        Script::Georgian,
        Script::Greek,
        Script::Gujarati,
        Script::Gurmukhi,
        Script::Hangul,
        Script::Hebrew,
        Script::Hiragana,
        Script::Kannada,
        Script::Katakana,
        Script::Khmer,
        Script::Latin,
        Script::Malayalam,
        Script::Mandarin,
        Script::Myanmar,
        Script::Oriya,
        Script::Sinhala,
        Script::Tamil,
        Script::Telugu,
        Script::Thai,
    ];

    /// Eager parse (`LocaGlyfState::Loaded`).
    fn mock() -> ParsedFont {
        let mut warnings = Vec::new();
        ParsedFont::from_bytes(MOCK_MONO, 0, &mut warnings).expect("Azul Mock Mono must parse")
    }

    /// Lazy parse (`LocaGlyfState::Deferred`) — the production path.
    fn mock_deferred() -> ParsedFont {
        let bytes = Arc::new(FontBytes::Owned(Arc::from(MOCK_MONO.to_vec())));
        let mut warnings = Vec::new();
        ParsedFont::from_bytes_shared(bytes, 0, &mut warnings)
            .expect("from_bytes_shared must parse the positive control")
    }

    fn style_at(font_size_px: f32) -> StyleProperties {
        StyleProperties {
            font_size_px,
            ..StyleProperties::default()
        }
    }

    fn shape(font: &ParsedFont, text: &str) -> Result<Vec<Glyph>, LayoutError> {
        shape_text_internal(
            font,
            text,
            Script::Latin,
            Language::EnglishUS,
            BidiDirection::Ltr,
            &style_at(16.0),
        )
    }

    /// Invariants that must hold for *any* shaping result, no matter how hostile
    /// the input: byte spans stay inside the source, land on char boundaries and
    /// never run backwards.
    fn assert_spans_are_sane(glyphs: &[Glyph], text: &str) {
        let mut prev_index = 0usize;
        for g in glyphs {
            assert!(
                g.logical_byte_index <= text.len(),
                "byte index {} escapes the {}-byte source",
                g.logical_byte_index,
                text.len()
            );
            let end = g.logical_byte_index + g.logical_byte_len;
            assert!(
                end <= text.len(),
                "span {}..{end} escapes the {}-byte source",
                g.logical_byte_index,
                text.len()
            );
            assert!(
                text.is_char_boundary(g.logical_byte_index) && text.is_char_boundary(end),
                "span {}..{end} splits a UTF-8 sequence",
                g.logical_byte_index
            );
            assert!(
                g.logical_byte_index >= prev_index,
                "byte cursor ran backwards: {} after {prev_index}",
                g.logical_byte_index
            );
            prev_index = g.logical_byte_index;
            assert_eq!(
                u64::from(g.cluster),
                g.logical_byte_index as u64,
                "cluster must mirror the logical byte index"
            );
        }
    }

    // -----------------------------------------------------------------
    // font_ref_from_bytes (parser)
    // -----------------------------------------------------------------

    #[test]
    fn font_ref_from_bytes_rejects_empty_and_malformed_input() {
        // empty / whitespace-only / invalid-UTF-8 / garbage: None, never a panic
        assert!(font_ref_from_bytes(b"", 0, false).is_none());
        assert!(font_ref_from_bytes(b"   \t\n", 0, false).is_none());
        assert!(font_ref_from_bytes(&[0xFF, 0xFE, 0x00], 0, false).is_none());
        assert!(font_ref_from_bytes(b"not a font at all, just prose", 0, false).is_none());
        // a 4-byte "sfnt-ish" header with nothing behind it
        assert!(font_ref_from_bytes(&[0x00, 0x01, 0x00, 0x00], 0, false).is_none());
        // truncated real font: header only, then a torso
        assert!(font_ref_from_bytes(&MOCK_MONO[..12], 0, false).is_none());
        assert!(font_ref_from_bytes(&MOCK_MONO[..64], 0, false).is_none());
        // leading junk in front of an otherwise valid font must not parse
        let mut prefixed = vec![0xABu8; 32];
        prefixed.extend_from_slice(MOCK_MONO);
        assert!(font_ref_from_bytes(&prefixed, 0, false).is_none());
    }

    #[test]
    fn font_ref_from_bytes_extremely_long_garbage_terminates() {
        // 1 MiB of NULs must be rejected without hanging or allocating wildly
        assert!(font_ref_from_bytes(&vec![0u8; 1_000_000], 0, false).is_none());
        // a "ttcf" collection header followed by a megabyte of noise
        let mut ttc_junk = b"ttcf".to_vec();
        ttc_junk.extend_from_slice(&vec![0xCDu8; 1_000_000]);
        assert!(font_ref_from_bytes(&ttc_junk, 0, false).is_none());
    }

    #[test]
    fn font_ref_from_bytes_valid_minimal_positive_control() {
        let font_ref =
            font_ref_from_bytes(MOCK_MONO, 0, false).expect("Azul Mock Mono must parse into a FontRef");
        assert!(font_ref.num_glyphs() > 0, "a real font has glyphs");
        assert_eq!(font_ref.get_hash(), mock().get_hash());
        assert!(font_ref.has_glyph('a' as u32));
    }

    #[test]
    fn font_ref_from_bytes_ignores_the_parse_outlines_flag() {
        // NOTE: `parse_outlines` is accepted but never forwarded to
        // `ParsedFont::from_bytes` — both settings must therefore produce
        // an identical face. This pins the current (no-op) behaviour.
        let with = font_ref_from_bytes(MOCK_MONO, 0, true).expect("parse with outlines");
        let without = font_ref_from_bytes(MOCK_MONO, 0, false).expect("parse without outlines");
        assert_eq!(with.get_hash(), without.get_hash());
        assert_eq!(with.num_glyphs(), without.num_glyphs());
        assert_eq!(with.get_space_width(), without.get_space_width());
    }

    #[test]
    fn font_ref_from_bytes_extreme_font_index_does_not_panic() {
        // Out-of-range collection indices must resolve to Some/None, never an
        // out-of-bounds index panic.
        for index in [0usize, 1, 255, usize::MAX / 2, usize::MAX] {
            let _ = font_ref_from_bytes(MOCK_MONO, index, false);
            let _ = font_ref_from_bytes(b"", index, false);
        }
    }

    // -----------------------------------------------------------------
    // PathLoader::new / load_from_path / load_font_shared
    // -----------------------------------------------------------------

    #[test]
    fn path_loader_new_is_a_zero_sized_stateless_handle() {
        assert_eq!(core::mem::size_of::<PathLoader>(), 0);
        let a = PathLoader::new();
        let b = PathLoader;
        // both handles behave identically: no hidden per-instance state
        assert!(a.load_from_path(Path::new("/nonexistent/azul/x.ttf"), 0).is_err());
        assert!(b.load_from_path(Path::new("/nonexistent/azul/x.ttf"), 0).is_err());
    }

    #[test]
    fn load_from_path_missing_empty_and_directory_paths_are_font_not_found() {
        let loader = PathLoader::new();
        for path in [
            "",
            "/nonexistent/definitely/not/a/font.ttf",
            "/dev/null",
            env!("CARGO_MANIFEST_DIR"), // a directory, not a file
        ] {
            match loader.load_from_path(Path::new(path), 0) {
                Err(LayoutError::FontNotFound(selector)) => {
                    assert_eq!(selector.family, path, "the failing path is reported back");
                    assert!(selector.unicode_ranges.is_empty());
                }
                // /dev/null reads as zero bytes on Linux -> the parse fails instead
                Err(LayoutError::ShapingError(_)) => {}
                other => panic!("{path:?} must not load a font: {other:?}"),
            }
        }
    }

    #[test]
    fn load_from_path_parses_a_real_font_and_survives_extreme_indices() {
        let loader = PathLoader::new();
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/fonts/test/azul-mock-mono.ttf"
        );
        let font_ref = loader
            .load_from_path(Path::new(path), 0)
            .expect("the positive control must load from disk");
        assert_eq!(font_ref.num_glyphs(), mock().num_glyphs());

        // 0 / MAX face index: either resolves or errors, but never panics
        for index in [0usize, 1, usize::MAX] {
            let _ = loader.load_from_path(Path::new(path), index);
        }
    }

    #[test]
    fn load_font_shared_rejects_empty_and_garbage_byte_blobs() {
        let loader = PathLoader::new();
        let cases: Vec<Vec<u8>> = vec![
            Vec::new(),
            b"   \t\n".to_vec(),
            vec![0xFF, 0xFE, 0x00],
            MOCK_MONO[..32].to_vec(),
            vec![0u8; 1_000_000],
        ];
        for bytes in cases {
            let shared = Arc::new(FontBytes::Owned(Arc::from(bytes)));
            match loader.load_font_shared(shared, 0) {
                Err(LayoutError::ShapingError(msg)) => assert!(!msg.is_empty()),
                other => panic!("garbage must not parse: {other:?}"),
            }
        }
    }

    #[test]
    fn load_font_shared_matches_the_eager_parse_and_tolerates_extreme_indices() {
        let loader = PathLoader::new();
        let shared = Arc::new(FontBytes::Owned(Arc::from(MOCK_MONO.to_vec())));
        let font_ref = loader
            .load_font_shared(Arc::clone(&shared), 0)
            .expect("the positive control must parse");
        let eager = mock();
        assert_eq!(font_ref.num_glyphs(), eager.num_glyphs());
        assert_eq!(font_ref.get_hash(), eager.get_hash());

        // font_index at the numeric extremes must not index out of bounds
        for index in [0usize, 1, usize::MAX] {
            let _ = loader.load_font_shared(Arc::clone(&shared), index);
        }
    }

    // -----------------------------------------------------------------
    // FontManager::evict_unused
    // -----------------------------------------------------------------

    #[test]
    fn evict_unused_on_an_empty_manager_is_zero_for_extreme_durations() {
        let manager: FontManager<FontRef> =
            FontManager::new(FcFontCache::default()).expect("an empty FontManager must build");
        // Duration::MAX truncates when cast to u64 nanos; with no faces cached
        // the result is 0 either way — the point is that it must not panic.
        for idle in [
            Duration::ZERO,
            Duration::from_nanos(1),
            Duration::from_secs(3600),
            Duration::MAX,
        ] {
            assert_eq!(manager.evict_unused(idle), 0);
        }
    }

    #[test]
    fn evict_unused_only_reclaims_stale_deferred_faces() {
        let manager: FontManager<FontRef> =
            FontManager::new(FcFontCache::default()).expect("an empty FontManager must build");

        let deferred = crate::parsed_font_to_font_ref(mock_deferred());
        let eager = crate::parsed_font_to_font_ref(mock());
        {
            let mut fonts = manager.parsed_fonts.lock().unwrap();
            fonts.insert(FontId::new(), deferred.clone());
            fonts.insert(FontId::new(), eager.clone());
        }

        // Nothing has decoded a glyph yet: the deferred face holds no LocaGlyf,
        // so there is nothing to release even though it is "never touched".
        assert_eq!(manager.evict_unused(Duration::ZERO), 0);

        // Touch the deferred face -> it materialises loca+glyf and stamps last_used.
        let parsed = crate::font_ref_to_parsed_font(&deferred);
        assert!(parsed.get_or_decode_glyph(1).is_some(), "gid 1 must decode");
        let _ = crate::font_ref_to_parsed_font(&eager).get_or_decode_glyph(1);

        // A face used microseconds ago is not idle for an hour.
        assert_eq!(manager.evict_unused(Duration::from_secs(3600)), 0);

        // With a zero idle window it is stale immediately. The eager face keeps
        // no source bytes, so it can never be evicted -> exactly one eviction.
        assert_eq!(manager.evict_unused(Duration::ZERO), 1);
        // Evicting twice is a no-op.
        assert_eq!(manager.evict_unused(Duration::ZERO), 0);

        // The evicted face still decodes: it re-parses from its retained bytes.
        assert!(parsed.get_or_decode_glyph(2).is_some());
    }

    // -----------------------------------------------------------------
    // shape_text_internal / shape_text_for_parsed_font / FontRef::shape_text
    // -----------------------------------------------------------------

    #[test]
    fn shape_text_empty_input_yields_no_glyphs() {
        let font = mock();
        let glyphs = shape(&font, "").expect("empty text is not an error");
        assert!(glyphs.is_empty());

        let via_public = shape_text_for_parsed_font(
            &font,
            "",
            Script::Latin,
            Language::EnglishUS,
            BidiDirection::Ltr,
            &style_at(16.0),
        )
        .expect("empty text is not an error");
        assert!(via_public.is_empty());

        let font_ref = crate::parsed_font_to_font_ref(mock());
        let via_ref = font_ref
            .shape_text(
                "",
                Script::Latin,
                Language::EnglishUS,
                BidiDirection::Ltr,
                &style_at(16.0),
            )
            .expect("empty text is not an error");
        assert!(via_ref.is_empty());
    }

    #[test]
    fn shape_text_valid_minimal_input_maps_bytes_one_to_one() {
        let font = mock();
        let glyphs = shape(&font, "abc").expect("plain ASCII must shape");
        assert_eq!(glyphs.len(), 3);
        assert_spans_are_sane(&glyphs, "abc");

        for (i, (g, ch)) in glyphs.iter().zip("abc".chars()).enumerate() {
            assert_eq!(g.codepoint, ch);
            assert_eq!(g.logical_byte_index, i);
            assert_eq!(g.logical_byte_len, 1);
            assert_eq!(g.glyph_id, font.lookup_glyph_index(ch as u32).unwrap_or(0));
            assert!(g.advance > 0.0, "a real glyph has a positive advance");
            assert!(g.advance.is_finite());
            assert_eq!(g.font_hash, font.get_hash());
            assert_eq!(g.script, Script::Latin);
        }
    }

    #[test]
    fn shape_text_whitespace_only_input_is_shaped_not_trimmed() {
        let font = mock();
        let text = " \t\n\r ";
        let glyphs = shape(&font, text).expect("whitespace must shape");
        assert!(!glyphs.is_empty(), "whitespace is not silently dropped");
        assert_spans_are_sane(&glyphs, text);
        for g in &glyphs {
            assert!(g.advance.is_finite());
        }
    }

    #[test]
    fn shape_text_unicode_and_missing_glyphs_keep_multibyte_spans_intact() {
        let font = mock();
        // emoji (almost certainly absent from a text face), combining marks, RTL,
        // CJK and an unassigned plane-15 codepoint
        for text in [
            "\u{1F600}",
            "e\u{0301}\u{0327}",
            "\u{0627}\u{0644}\u{0639}\u{0631}\u{0628}\u{064A}\u{0629}",
            "\u{4E2D}\u{6587}",
            "\u{FFFD}\u{FDD0}\u{F0000}",
            "a\u{200B}b\u{00AD}c",
        ] {
            let glyphs = shape(&font, text).unwrap_or_else(|e| panic!("{text:?} must shape: {e:?}"));
            assert!(!glyphs.is_empty(), "{text:?} produced no glyphs");
            assert_spans_are_sane(&glyphs, text);
        }

        // a missing glyph falls back to .notdef but must still carry the full
        // 4-byte span of the source character
        let emoji = "\u{1F600}";
        let glyphs = shape(&font, emoji).expect("emoji must shape");
        assert_eq!(glyphs.len(), 1);
        assert_eq!(glyphs[0].codepoint, '\u{1F600}');
        assert_eq!(glyphs[0].logical_byte_index, 0);
        assert_eq!(glyphs[0].logical_byte_len, 4, "the whole 4-byte char is covered");
        assert_eq!(
            glyphs[0].glyph_id,
            font.lookup_glyph_index('\u{1F600}' as u32).unwrap_or(0)
        );
    }

    #[test]
    fn shape_text_extremely_long_input_terminates() {
        let font = mock();
        let text = "a".repeat(10_000);
        let glyphs = shape(&font, &text).expect("a long run must shape");
        assert_eq!(glyphs.len(), 10_000);
        assert_spans_are_sane(&glyphs, &text);
        // the cursor must have walked the whole source, not stalled at 0
        assert_eq!(glyphs.last().unwrap().logical_byte_index, 9_999);
    }

    #[test]
    fn shape_text_deeply_nested_brackets_does_not_stack_overflow() {
        let font = mock();
        let text = format!("{}{}", "[".repeat(10_000), "]".repeat(10_000));
        let glyphs = shape(&font, &text).expect("nested brackets are just characters");
        assert_eq!(glyphs.len(), 20_000);
        assert_spans_are_sane(&glyphs, &text);
    }

    #[test]
    fn shape_text_boundary_numeric_text_is_shaped_verbatim() {
        let font = mock();
        let text = "0 -0 9223372036854775807 -9223372036854775808 NaN inf 1e309 0.0000001";
        let glyphs = shape(&font, text).expect("numeric-looking text is still text");
        assert_spans_are_sane(&glyphs, text);
        assert!(!glyphs.is_empty());
        for g in &glyphs {
            assert!(g.advance.is_finite(), "a 16px advance must stay finite");
        }
    }

    #[test]
    fn shape_text_nan_and_infinite_font_sizes_do_not_panic() {
        let font = mock();
        // ppem is `font_size.round().max(1.0) as u16` — an unchecked float->int
        // cast. NaN/inf must saturate, not trap.
        for size in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let glyphs = shape_text_internal(
                &font,
                "ab",
                Script::Latin,
                Language::EnglishUS,
                BidiDirection::Ltr,
                &style_at(size),
            )
            .unwrap_or_else(|e| panic!("font_size {size} must not fail shaping: {e:?}"));
            assert_eq!(glyphs.len(), 2);
            assert_spans_are_sane(&glyphs, "ab");
            for g in &glyphs {
                assert!(
                    !g.advance.is_finite(),
                    "a non-finite font size must not manufacture a finite advance ({size})"
                );
            }
        }

        // Finite-but-absurd sizes drive the float->u16 ppem cast to its
        // saturation points; they may overflow to ±inf but must not panic and
        // must not turn a non-NaN size into a NaN advance.
        for size in [f32::MAX, -f32::MAX, 1e30f32] {
            let glyphs = shape_text_internal(
                &font,
                "ab",
                Script::Latin,
                Language::EnglishUS,
                BidiDirection::Ltr,
                &style_at(size),
            )
            .unwrap_or_else(|e| panic!("font_size {size} must not fail shaping: {e:?}"));
            assert_eq!(glyphs.len(), 2);
            for g in &glyphs {
                assert!(!g.advance.is_nan(), "size {size} produced a NaN advance");
            }
        }
    }

    #[test]
    fn shape_text_zero_and_tiny_font_sizes_produce_zero_or_finite_advances() {
        let font = mock();
        for (size, expect_zero) in [(0.0f32, true), (-0.0f32, true), (f32::MIN_POSITIVE, false)] {
            let glyphs = shape_text_internal(
                &font,
                "ab",
                Script::Latin,
                Language::EnglishUS,
                BidiDirection::Ltr,
                &style_at(size),
            )
            .expect("degenerate font sizes must still shape");
            assert_eq!(glyphs.len(), 2);
            for g in &glyphs {
                assert!(g.advance.is_finite(), "size {size} produced {}", g.advance);
                if expect_zero {
                    assert_eq!(g.advance, 0.0, "a 0px font has zero-width glyphs");
                    assert_eq!(g.kerning, 0.0);
                }
            }
        }
    }

    #[test]
    fn shape_text_negative_font_size_mirrors_the_advance_sign() {
        let font = mock();
        let positive = shape_text_internal(
            &font,
            "a",
            Script::Latin,
            Language::EnglishUS,
            BidiDirection::Ltr,
            &style_at(16.0),
        )
        .expect("shape at +16px");
        let negative = shape_text_internal(
            &font,
            "a",
            Script::Latin,
            Language::EnglishUS,
            BidiDirection::Ltr,
            &style_at(-16.0),
        )
        .expect("a negative font size must not panic");
        assert_eq!(positive.len(), 1);
        assert_eq!(negative.len(), 1);
        assert_eq!(positive[0].glyph_id, negative[0].glyph_id);
        assert!(negative[0].advance.is_finite());
        assert!(
            negative[0].advance <= 0.0,
            "a negative size cannot yield a positive advance"
        );
    }

    #[test]
    fn shape_text_garbage_font_features_are_skipped_not_fatal() {
        let font = mock();
        let style = StyleProperties {
            font_features: vec![
                String::new(),
                "   ".to_string(),
                "waytoolongtag".to_string(),
                "liga=-1".to_string(),
                "liga=99999999999999999999".to_string(),
                "\u{1F600}".to_string(),
                "liga".to_string(),   // one good one, to prove the filter is per-item
                "ss01=2".to_string(),
            ],
            ..StyleProperties::default()
        };
        let glyphs = shape_text_internal(
            &font,
            "abc",
            Script::Latin,
            Language::EnglishUS,
            BidiDirection::Ltr,
            &style,
        )
        .expect("malformed feature strings must be dropped, not fatal");
        assert_eq!(glyphs.len(), 3);
        assert_spans_are_sane(&glyphs, "abc");
    }

    #[test]
    fn shape_text_direction_drives_the_bidi_level() {
        let font = mock();
        for (direction, expected) in [(BidiDirection::Ltr, 0u8), (BidiDirection::Rtl, 1u8)] {
            let glyphs = shape_text_internal(
                &font,
                "abc",
                Script::Latin,
                Language::EnglishUS,
                direction,
                &style_at(16.0),
            )
            .expect("both directions must shape");
            assert!(!glyphs.is_empty());
            for g in &glyphs {
                assert_eq!(g.bidi_level.level(), expected);
                assert_eq!(g.bidi_level.is_rtl(), direction.is_rtl());
            }
        }
    }

    #[test]
    fn shape_text_every_script_and_language_pairing_is_shapeable() {
        let font = mock();
        // A script tag that the font has no coverage for must degrade to
        // .notdef glyphs, never to an Err or a panic.
        for script in ALL_SCRIPTS {
            let glyphs = shape_text_internal(
                &font,
                "Hello \u{0e2a}\u{0e27}\u{0e31}\u{0e2a}\u{0e14}\u{0e35}",
                script,
                Language::EnglishUS,
                BidiDirection::Ltr,
                &style_at(16.0),
            )
            .unwrap_or_else(|e| panic!("{script:?} must shape: {e:?}"));
            assert!(!glyphs.is_empty(), "{script:?} produced no glyphs");
            for g in &glyphs {
                assert_eq!(g.script, script, "the requested script is stamped on the glyph");
            }
        }
    }

    #[test]
    fn shape_text_public_internal_and_font_ref_paths_agree() {
        // Round-trip / consistency: the three entry points are documented as
        // sharing one implementation, so they must produce identical glyphs.
        let font = mock();
        let font_ref = crate::parsed_font_to_font_ref(mock());
        let text = "Wafer fi\u{0301}x \u{1F600}";
        let style = style_at(13.5);

        let internal = shape_text_internal(
            &font,
            text,
            Script::Latin,
            Language::EnglishUS,
            BidiDirection::Ltr,
            &style,
        )
        .expect("internal shaping");
        let public = shape_text_for_parsed_font(
            &font,
            text,
            Script::Latin,
            Language::EnglishUS,
            BidiDirection::Ltr,
            &style,
        )
        .expect("public shaping");
        let via_ref = font_ref
            .shape_text(
                text,
                Script::Latin,
                Language::EnglishUS,
                BidiDirection::Ltr,
                &style,
            )
            .expect("FontRef shaping");
        let via_helper = font
            .shape_text_for_font_ref(
                &font_ref,
                text,
                Script::Latin,
                Language::EnglishUS,
                BidiDirection::Ltr,
                &style,
            )
            .expect("shape_text_for_font_ref");

        assert_eq!(internal.len(), public.len());
        assert_eq!(internal.len(), via_ref.len());
        assert_eq!(internal.len(), via_helper.len());
        for (((a, b), c), d) in internal
            .iter()
            .zip(public.iter())
            .zip(via_ref.iter())
            .zip(via_helper.iter())
        {
            for other in [b, c, d] {
                assert_eq!(a.glyph_id, other.glyph_id);
                assert_eq!(a.codepoint, other.codepoint);
                assert_eq!(a.advance, other.advance);
                assert_eq!(a.kerning, other.kerning);
                assert_eq!(a.logical_byte_index, other.logical_byte_index);
                assert_eq!(a.logical_byte_len, other.logical_byte_len);
                assert_eq!(a.font_hash, other.font_hash);
            }
        }
        assert_spans_are_sane(&internal, text);
    }

    // -----------------------------------------------------------------
    // ParsedFont::get_hash (getter)
    // -----------------------------------------------------------------

    #[test]
    fn get_hash_is_stable_and_shared_with_the_font_ref_view() {
        let a = mock();
        let b = mock();
        assert_eq!(a.get_hash(), b.get_hash(), "parsing is deterministic");
        assert_eq!(a.get_hash(), a.hash, "the getter reads the cached field");

        let font_ref = crate::parsed_font_to_font_ref(mock());
        assert_eq!(font_ref.get_hash(), a.get_hash());

        // the lazy constructor must not change identity
        assert_eq!(mock_deferred().get_hash(), a.get_hash());
    }

    // -----------------------------------------------------------------
    // ParsedFont::get_glyph_size (numeric)
    // -----------------------------------------------------------------

    #[test]
    fn get_glyph_size_out_of_range_glyph_ids_are_none() {
        let font = mock();
        assert!(font.get_glyph_size(u16::MAX, 16.0).is_none());
        assert!(font.get_glyph_size(font.num_glyphs, 16.0).is_none());
        // an in-range gid still decodes (positive control)
        let gid = font.lookup_glyph_index('a' as u32).expect("'a' must be mapped");
        assert!(gid < font.num_glyphs);
        assert!(font.get_glyph_size(gid, 16.0).is_some());
    }

    #[test]
    fn get_glyph_size_zero_negative_and_non_finite_font_sizes() {
        let font = mock();
        let gid = font.lookup_glyph_index('a' as u32).expect("'a' must be mapped");

        let zero = font.get_glyph_size(gid, 0.0).expect("gid decodes");
        assert_eq!(zero.width, 0.0);
        assert_eq!(zero.height, 0.0);

        let base = font.get_glyph_size(gid, 16.0).expect("gid decodes");
        assert!(base.width > 0.0 && base.height > 0.0);

        // scaling is linear in the font size
        let doubled = font.get_glyph_size(gid, 32.0).expect("gid decodes");
        assert!((doubled.width - 2.0 * base.width).abs() <= 1e-3 * base.width.max(1.0));

        let negative = font.get_glyph_size(gid, -16.0).expect("gid decodes");
        assert!(negative.width <= 0.0 && negative.width.is_finite());

        for size in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, f32::MAX, f32::MIN_POSITIVE] {
            let size_result = font
                .get_glyph_size(gid, size)
                .unwrap_or_else(|| panic!("gid must still decode at {size}"));
            assert!(
                !size_result.width.is_nan() || size.is_nan(),
                "only a NaN input may produce a NaN width"
            );
        }
    }

    #[test]
    fn get_glyph_size_zero_units_per_em_uses_the_constant_fallback_scale() {
        assert_eq!(FALLBACK_SCALE, 0.01);
        let mut font = mock();
        let gid = font.lookup_glyph_index('a' as u32).expect("'a' must be mapped");
        font.font_metrics.units_per_em = 0; // corrupt/broken font

        // With upem == 0 the scale is the constant FALLBACK_SCALE, so the size no
        // longer depends on the requested font size at all.
        let small = font.get_glyph_size(gid, 16.0).expect("gid decodes");
        let huge = font.get_glyph_size(gid, 1000.0).expect("gid decodes");
        assert!(small.width > 0.0 && small.height > 0.0, "no divide-by-zero NaN");
        assert_eq!(small.width, huge.width);
        assert_eq!(small.height, huge.height);
    }

    // -----------------------------------------------------------------
    // ParsedFont::get_hyphen_glyph_and_advance / get_kashida_glyph_and_advance
    // -----------------------------------------------------------------

    #[test]
    fn get_hyphen_glyph_and_advance_follows_the_cmap_and_scales_linearly() {
        let font = mock();
        let expected_gid = font
            .lookup_glyph_index('-' as u32)
            .expect("the positive control has a hyphen");

        let (gid, zero_advance) = font.get_hyphen_glyph_and_advance(0.0).expect("hyphen at 0px");
        assert_eq!(gid, expected_gid);
        assert_eq!(zero_advance, 0.0, "a 0px font gives a 0px advance");

        let (_, a16) = font.get_hyphen_glyph_and_advance(16.0).expect("hyphen at 16px");
        let (_, a32) = font.get_hyphen_glyph_and_advance(32.0).expect("hyphen at 32px");
        assert!(a16 > 0.0 && a16.is_finite());
        assert!((a32 - 2.0 * a16).abs() <= 1e-3 * a16, "advance is linear in font size");

        let (_, negative) = font.get_hyphen_glyph_and_advance(-16.0).expect("hyphen at -16px");
        assert!(negative.is_finite() && negative <= 0.0);
    }

    #[test]
    fn get_kashida_glyph_and_advance_presence_matches_the_cmap() {
        let font = mock();
        // U+0640 ARABIC TATWEEL: present or not, the two views must agree.
        let has_tatweel = font.has_glyph(0x0640);
        let result = font.get_kashida_glyph_and_advance(16.0);
        assert_eq!(
            result.is_some(),
            has_tatweel,
            "kashida availability must track the cmap"
        );
        if let Some((gid, advance)) = result {
            assert_eq!(Some(gid), font.lookup_glyph_index(0x0640));
            assert!(advance.is_finite() && advance >= 0.0);
            let (_, doubled) = font.get_kashida_glyph_and_advance(32.0).expect("still mapped");
            assert!((doubled - 2.0 * advance).abs() <= 1e-3 * advance.max(1.0));
        }
    }

    #[test]
    fn hyphen_and_kashida_survive_nan_inf_and_extreme_font_sizes() {
        let font = mock();
        let hyphen_gid = font
            .lookup_glyph_index('-' as u32)
            .expect("the positive control has a hyphen");
        assert!(
            font.get_horizontal_advance(hyphen_gid) > 0,
            "the hyphen must have a non-zero advance for this test to mean anything"
        );

        // NaN / ±inf in -> non-finite out, and never a panic.
        for size in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let (gid, advance) = font
                .get_hyphen_glyph_and_advance(size)
                .expect("the glyph id does not depend on the font size");
            assert_eq!(gid, hyphen_gid);
            assert!(!advance.is_finite(), "{size} produced a finite advance");
            let _ = font.get_kashida_glyph_and_advance(size);
        }

        // The numeric extremes may overflow to ±inf, but a non-NaN size must
        // never produce a NaN advance and must never panic.
        for size in [f32::MAX, -f32::MAX, f32::MIN_POSITIVE, -f32::MIN_POSITIVE] {
            let (gid, advance) = font
                .get_hyphen_glyph_and_advance(size)
                .expect("the glyph id does not depend on the font size");
            assert_eq!(gid, hyphen_gid);
            assert!(!advance.is_nan(), "size {size} produced a NaN advance");
            let _ = font.get_kashida_glyph_and_advance(size);
        }
    }

    #[test]
    fn hyphen_and_kashida_return_none_when_units_per_em_is_zero() {
        let mut font = mock();
        font.font_metrics.units_per_em = 0;
        // A zero upem would divide by zero: both getters must bail out instead.
        assert!(font.get_hyphen_glyph_and_advance(16.0).is_none());
        assert!(font.get_kashida_glyph_and_advance(16.0).is_none());
        assert!(font.get_hyphen_glyph_and_advance(f32::NAN).is_none());
    }

    // -----------------------------------------------------------------
    // build_feature_mask_for_script (other)
    // -----------------------------------------------------------------

    #[test]
    fn build_feature_mask_always_contains_the_default_mask() {
        let default_bits = FeatureMask::default_mask().bits();
        for script in ALL_SCRIPTS {
            let mask = build_feature_mask_for_script(script);
            assert_eq!(
                mask.bits() & default_bits,
                default_bits,
                "{script:?} dropped a default feature"
            );
            assert!(
                mask.contains(Feature::LIGA) && mask.contains(Feature::CCMP),
                "{script:?} must keep LIGA/CCMP"
            );
        }
    }

    #[test]
    fn build_feature_mask_adds_the_script_specific_features() {
        // Arabic needs positional forms, or cursive joining silently breaks.
        let arabic = build_feature_mask_for_script(Script::Arabic);
        for feature in [Feature::INIT, Feature::MEDI, Feature::FINA, Feature::ISOL] {
            assert!(arabic.contains(feature), "Arabic is missing a positional form");
        }

        // Indic needs conjunct formation.
        for script in [
            Script::Devanagari,
            Script::Bengali,
            Script::Gujarati,
            Script::Gurmukhi,
            Script::Kannada,
            Script::Malayalam,
            Script::Oriya,
            Script::Tamil,
            Script::Telugu,
        ] {
            let mask = build_feature_mask_for_script(script);
            for feature in [Feature::CJCT, Feature::HALF, Feature::RPHF, Feature::NUKT] {
                assert!(mask.contains(feature), "{script:?} is missing an Indic feature");
            }
        }

        // Sinhala is Indic-derived but explicitly simpler: no conjunct feature.
        let sinhala = build_feature_mask_for_script(Script::Sinhala);
        assert!(sinhala.contains(Feature::AKHN) && sinhala.contains(Feature::RPHF));
        assert!(!sinhala.contains(Feature::CJCT));

        // Myanmar/Khmer get pre/below/post-base forms.
        assert!(build_feature_mask_for_script(Script::Myanmar).contains(Feature::PSTF));
        assert!(build_feature_mask_for_script(Script::Khmer).contains(Feature::ABVF));

        // Simple scripts must be exactly the default mask — no accidental extras.
        for script in [Script::Latin, Script::Greek, Script::Cyrillic, Script::Georgian] {
            assert_eq!(
                build_feature_mask_for_script(script).bits(),
                FeatureMask::default_mask().bits(),
                "{script:?} must not add script-specific features"
            );
        }
    }

    // -----------------------------------------------------------------
    // to_opentype_script_tag (other)
    // -----------------------------------------------------------------

    #[test]
    fn script_tags_are_four_printable_ascii_bytes() {
        for script in ALL_SCRIPTS {
            let tag = to_opentype_script_tag(script);
            let bytes = tag.to_be_bytes();
            assert_eq!(bytes.len(), 4);
            for b in bytes {
                assert!(
                    b.is_ascii_lowercase() || b.is_ascii_digit(),
                    "{script:?} -> {tag:#010x} is not a lowercase OpenType tag"
                );
            }
            assert_ne!(tag, 0, "{script:?} must not map to the null tag");
        }
    }

    #[test]
    fn script_tags_match_the_opentype_registry_and_alias_kana() {
        assert_eq!(to_opentype_script_tag(Script::Latin), u32::from_be_bytes(*b"latn"));
        assert_eq!(to_opentype_script_tag(Script::Arabic), u32::from_be_bytes(*b"arab"));
        assert_eq!(to_opentype_script_tag(Script::Mandarin), u32::from_be_bytes(*b"hani"));
        assert_eq!(to_opentype_script_tag(Script::Devanagari), u32::from_be_bytes(*b"deva"));

        // Hiragana and Katakana intentionally share "kana" (documented).
        assert_eq!(
            to_opentype_script_tag(Script::Hiragana),
            to_opentype_script_tag(Script::Katakana)
        );
        assert_eq!(to_opentype_script_tag(Script::Hiragana), u32::from_be_bytes(*b"kana"));

        // Every other pair must be distinct: 24 scripts, 23 distinct tags.
        let mut tags: Vec<u32> = ALL_SCRIPTS.iter().map(|s| to_opentype_script_tag(*s)).collect();
        tags.sort_unstable();
        tags.dedup();
        assert_eq!(tags.len(), 23, "only the kana pair may collide");
    }

    // -----------------------------------------------------------------
    // parse_font_feature (parser)
    // -----------------------------------------------------------------

    #[test]
    fn parse_font_feature_valid_minimal_inputs() {
        assert_eq!(
            parse_font_feature("liga"),
            Some((u32::from_be_bytes(*b"liga"), 1)),
            "a bare tag defaults to value 1"
        );
        assert_eq!(parse_font_feature("liga=0"), Some((u32::from_be_bytes(*b"liga"), 0)));
        assert_eq!(parse_font_feature("ss01"), Some((u32::from_be_bytes(*b"ss01"), 1)));
        assert_eq!(parse_font_feature("smcp=2"), Some((u32::from_be_bytes(*b"smcp"), 2)));
        // u32::MAX is the largest accepted value
        assert_eq!(
            parse_font_feature("ss01=4294967295"),
            Some((u32::from_be_bytes(*b"ss01"), u32::MAX))
        );
    }

    #[test]
    fn parse_font_feature_pads_short_tags_with_spaces() {
        assert_eq!(parse_font_feature("aa"), Some((u32::from_be_bytes(*b"aa  "), 1)));
        assert_eq!(parse_font_feature("a"), Some((u32::from_be_bytes(*b"a   "), 1)));
        assert_eq!(parse_font_feature("abc=3"), Some((u32::from_be_bytes(*b"abc "), 3)));
    }

    #[test]
    fn parse_font_feature_trims_surrounding_whitespace() {
        assert_eq!(parse_font_feature("  liga  "), Some((u32::from_be_bytes(*b"liga"), 1)));
        assert_eq!(parse_font_feature("\tliga\n=\t2 "), Some((u32::from_be_bytes(*b"liga"), 2)));
    }

    #[test]
    fn parse_font_feature_empty_and_whitespace_only_yield_the_all_space_tag() {
        // Documents current behaviour: an empty/blank tag is NOT rejected — it is
        // padded to the four-space tag (0x20202020), which no font can match.
        let space_tag = u32::from_be_bytes(*b"    ");
        assert_eq!(parse_font_feature(""), Some((space_tag, 1)));
        assert_eq!(parse_font_feature("   "), Some((space_tag, 1)));
        assert_eq!(parse_font_feature("\t\n"), Some((space_tag, 1)));
        // ...but an empty *value* is still rejected.
        assert_eq!(parse_font_feature("="), None);
        assert_eq!(parse_font_feature("liga="), None);
        assert_eq!(parse_font_feature("liga=  "), None);
    }

    #[test]
    fn parse_font_feature_rejects_over_long_tags_and_junk() {
        assert_eq!(parse_font_feature("toolongtag"), None);
        assert_eq!(parse_font_feature("lig a"), None); // 5 bytes after trim
        assert_eq!(parse_font_feature("liga;garbage"), None);
        assert_eq!(parse_font_feature("valid;garbage=1"), None);
        // a megabyte-long tag must be rejected by the length check, not parsed
        assert_eq!(parse_font_feature(&"x".repeat(1_000_000)), None);
        assert_eq!(parse_font_feature(&format!("{}=1", "x".repeat(1_000_000))), None);
    }

    #[test]
    fn parse_font_feature_rejects_boundary_and_non_numeric_values() {
        for bad in [
            "liga=-1",
            "liga=-0",
            "liga=1.5",
            "liga=NaN",
            "liga=inf",
            "liga=0x1",
            "liga=4294967296",          // u32::MAX + 1
            "liga=9223372036854775807", // i64::MAX
            "liga=99999999999999999999999999",
            "liga= 1 2",
        ] {
            assert_eq!(parse_font_feature(bad), None, "{bad:?} must be rejected");
        }
        // `u32::from_str` accepts a leading '+', so this one is (surprisingly) valid
        assert_eq!(parse_font_feature("liga=+1"), Some((u32::from_be_bytes(*b"liga"), 1)));
        // a trailing extra '=' segment is ignored: only the first value is read
        assert_eq!(parse_font_feature("liga=1=2"), Some((u32::from_be_bytes(*b"liga"), 1)));
    }

    #[test]
    fn parse_font_feature_unicode_input_does_not_panic() {
        // Multibyte tags pad by *chars* but the tag must be exactly 4 *bytes*,
        // so every one of these must fall out as None rather than slicing a
        // char boundary or panicking on the array conversion.
        for input in [
            "\u{1F600}",         // 4 bytes, 1 char
            "\u{1F600}\u{1F600}",
            "é",
            "ß=1",
            "e\u{0301}",         // combining acute
            "\u{202E}liga",      // RTL override
            "\u{0000}\u{0001}",
        ] {
            let _ = parse_font_feature(input); // must not panic
        }
        assert_eq!(parse_font_feature("\u{1F600}"), None);
        assert_eq!(parse_font_feature("é"), None);
    }

    // -----------------------------------------------------------------
    // add_variant_features (other)
    // -----------------------------------------------------------------

    #[test]
    fn add_variant_features_maps_css_variants_to_opentype_tags() {
        let tags = |style: &StyleProperties| -> Vec<u32> {
            let mut features = Vec::new();
            add_variant_features(style, &mut features);
            assert!(
                features.iter().all(|f| f.alternate.is_none()),
                "variant features are on/off, never alternates"
            );
            features.iter().map(|f| f.feature_tag).collect()
        };

        // the default style adds nothing
        assert!(tags(&StyleProperties::default()).is_empty());

        let small_caps = StyleProperties {
            font_variant_caps: FontVariantCaps::SmallCaps,
            ..StyleProperties::default()
        };
        assert_eq!(tags(&small_caps), vec![u32::from_be_bytes(*b"smcp")]);

        let all_small = StyleProperties {
            font_variant_caps: FontVariantCaps::AllSmallCaps,
            ..StyleProperties::default()
        };
        assert_eq!(
            tags(&all_small),
            vec![u32::from_be_bytes(*b"c2sc"), u32::from_be_bytes(*b"smcp")]
        );

        let combined = StyleProperties {
            font_variant_ligatures: FontVariantLigatures::Discretionary,
            font_variant_numeric: FontVariantNumeric::TabularNums,
            font_variant_caps: FontVariantCaps::TitlingCaps,
            ..StyleProperties::default()
        };
        assert_eq!(
            tags(&combined),
            vec![
                u32::from_be_bytes(*b"dlig"),
                u32::from_be_bytes(*b"titl"),
                u32::from_be_bytes(*b"tnum"),
            ],
            "ligature, caps and numeric features are all emitted, in that order"
        );
    }

    #[test]
    fn add_variant_features_is_additive_and_never_panics_for_any_variant() {
        let ligatures = [
            FontVariantLigatures::Normal,
            FontVariantLigatures::None,
            FontVariantLigatures::Common,
            FontVariantLigatures::NoCommon,
            FontVariantLigatures::Discretionary,
            FontVariantLigatures::NoDiscretionary,
            FontVariantLigatures::Historical,
            FontVariantLigatures::NoHistorical,
            FontVariantLigatures::Contextual,
            FontVariantLigatures::NoContextual,
        ];
        let caps = [
            FontVariantCaps::Normal,
            FontVariantCaps::SmallCaps,
            FontVariantCaps::AllSmallCaps,
            FontVariantCaps::PetiteCaps,
            FontVariantCaps::AllPetiteCaps,
            FontVariantCaps::Unicase,
            FontVariantCaps::TitlingCaps,
        ];
        let numeric = [
            FontVariantNumeric::Normal,
            FontVariantNumeric::LiningNums,
            FontVariantNumeric::OldstyleNums,
            FontVariantNumeric::ProportionalNums,
            FontVariantNumeric::TabularNums,
            FontVariantNumeric::DiagonalFractions,
            FontVariantNumeric::StackedFractions,
            FontVariantNumeric::Ordinal,
            FontVariantNumeric::SlashedZero,
        ];

        // a pre-existing feature must survive: the helper appends, never clears
        let sentinel = FeatureInfo {
            feature_tag: u32::from_be_bytes(*b"kern"),
            alternate: Some(7),
        };
        for l in ligatures {
            for c in caps {
                for n in numeric {
                    let style = StyleProperties {
                        font_variant_ligatures: l,
                        font_variant_caps: c,
                        font_variant_numeric: n,
                        ..StyleProperties::default()
                    };
                    let mut features = vec![sentinel];
                    add_variant_features(&style, &mut features);
                    assert_eq!(features[0].feature_tag, sentinel.feature_tag);
                    assert_eq!(features[0].alternate, Some(7));
                    // at most 2 (caps) + 1 (ligature) + 1 (numeric) new tags
                    assert!(features.len() <= 5, "{l:?}/{c:?}/{n:?} emitted too many features");
                    for f in &features[1..] {
                        assert!(f.feature_tag.to_be_bytes().iter().all(u8::is_ascii_graphic));
                    }
                }
            }
        }
    }

    // -----------------------------------------------------------------
    // to_opentype_lang_tag (other, feature-gated)
    // -----------------------------------------------------------------

    #[cfg(feature = "text_layout_hyphenation")]
    #[test]
    fn lang_tags_are_four_byte_uppercase_padded_tags() {
        use hyphenation::Language as HL;

        // A representative spread across the mapping table, including the two
        // arms that intentionally share a tag.
        let sample = [
            HL::EnglishUS,
            HL::EnglishGB,
            HL::German1901,
            HL::German1996,
            HL::French,
            HL::Russian,
            HL::Finnish,
            HL::FinnishScholastic,
            HL::Latin,
            HL::LatinClassic,
            HL::Welsh,
            HL::Thai,
        ];
        for lang in sample {
            let tag = to_opentype_lang_tag(lang);
            let bytes = tag.to_be_bytes();
            for b in bytes {
                assert!(
                    b.is_ascii_uppercase() || b == b' ',
                    "{lang:?} -> {tag:#010x} is not an uppercase, space-padded tag"
                );
            }
            assert_ne!(tag, 0);
        }

        assert_eq!(to_opentype_lang_tag(HL::EnglishUS), u32::from_be_bytes(*b"ENU "));
        assert_eq!(to_opentype_lang_tag(HL::EnglishGB), u32::from_be_bytes(*b"ENG "));
        assert_eq!(to_opentype_lang_tag(HL::German1996), u32::from_be_bytes(*b"DEU "));
        assert_eq!(to_opentype_lang_tag(HL::French), u32::from_be_bytes(*b"FRA "));
        assert_eq!(to_opentype_lang_tag(HL::Russian), u32::from_be_bytes(*b"RUS "));
        // documented aliases: both German orthographies and both Finnish variants
        assert_eq!(
            to_opentype_lang_tag(HL::German1901),
            to_opentype_lang_tag(HL::German1996)
        );
        assert_eq!(
            to_opentype_lang_tag(HL::Finnish),
            to_opentype_lang_tag(HL::FinnishScholastic)
        );
    }

    // -----------------------------------------------------------------
    // FontRef trait surface: delegation invariants
    // -----------------------------------------------------------------

    #[test]
    fn font_ref_trait_getters_delegate_to_the_inner_parsed_font() {
        let parsed = mock();
        let font_ref = crate::parsed_font_to_font_ref(mock());

        assert_eq!(font_ref.num_glyphs(), parsed.num_glyphs);
        assert_eq!(font_ref.get_space_width(), parsed.get_space_width());
        assert_eq!(font_ref.get_font_metrics().units_per_em, parsed.font_metrics.units_per_em);
        assert!(font_ref.has_glyph('a' as u32) == parsed.has_glyph('a' as u32));
        assert!(!font_ref.has_glyph(0x0011_0000), "an invalid scalar value has no glyph");

        let gid = parsed.lookup_glyph_index('a' as u32).expect("'a' must be mapped");
        let via_ref = font_ref.get_glyph_size(gid, 16.0).expect("gid decodes");
        let via_parsed = parsed.get_glyph_size(gid, 16.0).expect("gid decodes");
        assert_eq!(via_ref.width, via_parsed.width);
        assert_eq!(via_ref.height, via_parsed.height);

        assert_eq!(
            font_ref.get_hyphen_glyph_and_advance(16.0).map(|(g, _)| g),
            parsed.get_hyphen_glyph_and_advance(16.0).map(|(g, _)| g)
        );
        assert_eq!(
            font_ref.get_kashida_glyph_and_advance(16.0).map(|(g, _)| g),
            parsed.get_kashida_glyph_and_advance(16.0).map(|(g, _)| g)
        );

        // shallow_clone shares the same underlying face
        let cloned = font_ref.shallow_clone();
        assert_eq!(cloned.get_hash(), font_ref.get_hash());
    }
}
