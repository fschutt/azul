use std::{path::Path, sync::Arc};

use allsorts::{
    gpos,
    gsub::{self, FeatureInfo, FeatureMask, Features},
};
use azul_core::app_resources::Placement;
use rust_fontconfig::FcFontCache;

// Imports from the layout engine's module
use crate::text3::{
    cache::{
        BidiLevel, Direction, FontLoaderTrait, FontManager, FontMetrics, FontRef, Glyph,
        GlyphOrientation, GlyphSource, LayoutError, ParsedFontTrait, Point, StyleProperties,
        TextCombineUpright, TextDecoration, TextOrientation, VerticalMetrics, WritingMode,
    },
    script::{estimate_script_and_language, Script},
};
// Imports for the provided ParsedFont implementation
use crate::{
    parsedfont::ParsedFont,
    text3::cache::{FontVariantCaps, FontVariantLigatures, FontVariantNumeric},
};

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
    pub fn load_from_path(
        &self,
        path: &Path,
        font_index: usize,
    ) -> Result<Arc<ParsedFont>, LayoutError> {
        println!("[PathLoader] Accessing font file at: {:?}", path);
        let font_bytes = std::fs::read(path).map_err(|e| {
            LayoutError::FontNotFound(FontRef {
                family: path.to_string_lossy().into_owned(),
                ..FontRef::invalid()
            })
        })?;
        self.load_font(&font_bytes, font_index)
    }
}

impl FontManager<ParsedFont, PathLoader> {
    pub fn new(fc_cache: FcFontCache) -> Result<Self, LayoutError> {
        FontManager::with_loader(fc_cache, Arc::new(PathLoader::new()))
    }
}

impl FontLoaderTrait<ParsedFont> for PathLoader {
    /// Loads a font from a byte slice.
    ///
    /// This implementation is designed to work with `FontManager<ParsedFont, PathLoader>`.
    /// It parses the byte slice into a `ParsedFont` instance and returns it as the
    /// generic type `T` required by the `FontManager`.
    fn load_font(
        &self,
        font_bytes: &[u8],
        font_index: usize,
    ) -> Result<Arc<ParsedFont>, LayoutError> {
        println!(
            "[PathLoader] Parsing font from byte stream (font index: {})",
            font_index
        );

        // Parse the font bytes using the provided ParsedFont implementation.
        // We disable parsing glyph outlines for performance in the layout stage.
        let parsed_font =
            ParsedFont::from_bytes(font_bytes, font_index, false).ok_or_else(|| {
                LayoutError::ShapingError("Failed to parse font with allsorts".to_string())
            })?;

        Ok(Arc::new(parsed_font))
    }
}

// --- ParsedFontTrait Implementation ---

impl ParsedFontTrait for ParsedFont {
    /// Shapes a text string using the `allsorts` shaping engine.
    fn shape_text(
        &self,
        text: &str,
        script: Script,
        language: hyphenation::Language,
        direction: Direction,
        style: &StyleProperties,
    ) -> Result<Vec<Glyph<Self>>, LayoutError> {
        // 1. Convert layout engine enums to OpenType tags for allsorts.
        let script_tag = to_opentype_script_tag(script);
        let lang_tag = to_opentype_lang_tag(language);

        // 2. Build a list of user-specified features.
        // For now, these are only passed to the GPOS stage. GSUB uses a default set.
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

        // 3. Perform shaping using the full allsorts pipeline.
        let gdef = self.opt_gdef_table.as_ref().ok_or_else(|| {
            LayoutError::ShapingError("GDEF table not found, needed for shaping.".to_string())
        })?;

        // 3a. Map text to a `RawGlyph` buffer. We use `liga_component_pos` as a temporary
        // store for the cluster ID (the original byte index of the character).
        let mut raw_glyphs: Vec<allsorts::gsub::RawGlyph<()>> = text
            .char_indices()
            .filter_map(|(cluster, ch)| {
                let glyph_index = self.lookup_glyph_index(ch as u32).unwrap_or(0);
                if cluster > u16::MAX as usize {
                    // This is a limitation of using liga_component_pos to store the cluster ID.
                    // The text needs to be shaped in smaller chunks.
                    None
                } else {
                    Some(allsorts::gsub::RawGlyph {
                        unicodes: tinyvec::tiny_vec![[char; 1] => ch],
                        glyph_index,
                        liga_component_pos: cluster as u16, // Store cluster, may truncate
                        glyph_origin: allsorts::gsub::GlyphOrigin::Char(ch),
                        flags: allsorts::gsub::RawGlyphFlags::empty(),
                        extra_data: (),
                        variation: None,
                    })
                }
            })
            .collect();

        // 3b. Apply GSUB substitutions with a default feature set for the script.
        if let Some(gsub) = self.gsub_cache.as_ref() {
            let features = Features::Custom(user_features.clone());
            let dotted_circle_index = self
                .lookup_glyph_index(allsorts::DOTTED_CIRCLE as u32)
                .unwrap_or(0);
            gsub::apply(
                dotted_circle_index,
                gsub,
                Some(gdef),
                script_tag,
                Some(lang_tag),
                &features,
                None, // No variations tuple for now
                self.num_glyphs(),
                &mut raw_glyphs,
            )
            .map_err(|e| LayoutError::ShapingError(e.to_string()))?;
        }

        // 3c. Convert the `RawGlyph` buffer to a `gpos::Info` buffer for positioning.
        // The cluster ID we stored in `liga_component_pos` is preserved inside `info.glyph`.
        let mut infos = gpos::Info::init_from_glyphs(Some(gdef), raw_glyphs);

        // 3d. Apply GPOS positioning.
        if let Some(gpos) = self.gpos_cache.as_ref() {
            let kern_table = self
                .opt_kern_table
                .as_ref()
                .map(|kt| allsorts::tables::kern::KernTable::from_owned(&**kt));
            let apply_kerning = kern_table.is_some();
            // The modern `gpos::apply` takes a GlyphDirection enum and an iterator of features.
            gpos::apply(
                gpos,
                Some(gdef),
                kern_table,
                apply_kerning,
                &Features::Custom(user_features),
                None,
                script_tag,
                Some(lang_tag), // Note: &Vec can be used to create an iterator
                &mut infos,
            )
            .map_err(|e| LayoutError::ShapingError(e.to_string()))?;
        }

        // 4. Translate the allsorts output into the layout engine's `Glyph` format.
        let font_size = style.font_size_px;
        let scale_factor = if self.font_metrics.units_per_em > 0 {
            font_size / (self.font_metrics.units_per_em as f32)
        } else {
            0.01 // Avoid division by zero
        };

        let mut shaped_glyphs = Vec::new();
        for info in infos.iter() {
            // Retrieve the cluster ID from the field we used to store it.
            let cluster = info.glyph.liga_component_pos as u32;

            let source_char = text
                .get(cluster as usize..)
                .and_then(|s| s.chars().next())
                .unwrap_or('\u{FFFD}');

            let base_advance = self.get_horizontal_advance(info.glyph.glyph_index);
            // Ensure both operands are i32 before adding
            let advance = (base_advance as i32 + info.kerning as i32) as f32 * scale_factor;

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
                font: Arc::new(self.clone()),
                style: Arc::new(style.clone()),
                source: GlyphSource::Char,
                logical_byte_index: cluster as usize,
                logical_byte_len: source_char.len_utf8(),
                content_index: 0,
                cluster,
                advance,
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

    fn num_glyphs(&self) -> u16 {
        self.num_glyphs
    }

    fn has_glyph(&self, codepoint: u32) -> bool {
        self.lookup_glyph_index(codepoint).is_some()
    }

    /// Returns vertical metrics for a glyph.
    ///
    /// TODO: The provided `ParsedFont` implementation does not parse vertical layout
    /// tables (`vhea`, `vmtx`), so this method will always return `None`.
    fn get_vertical_metrics(&self, glyph_id: u16) -> Option<VerticalMetrics> {
        // To implement this, one would need to parse the `vhea` and `vmtx` tables
        // from the font file and store them in the `ParsedFont` struct.
        None
    }

    /// Translates the font-specific metrics into the layout engine's generic `FontMetrics`.
    fn get_font_metrics(&self) -> FontMetrics {
        // The `descender` value in OpenType is typically negative.
        let descender = if self.font_metrics.descender > 0 {
            -self.font_metrics.descender
        } else {
            self.font_metrics.descender
        };

        FontMetrics {
            ascent: self.font_metrics.ascender as f32,
            descent: descender as f32,
            line_gap: self.font_metrics.line_gap as f32,
            units_per_em: self.font_metrics.units_per_em,
        }
    }
}

// --- Helper Functions ---

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

impl Direction {
    fn is_rtl(&self) -> bool {
        matches!(self, Direction::Rtl)
    }
}
