use std::{path::Path, sync::Arc};

use allsorts::{
    gpos,
    gsub::{self, Features},
};
use azul_core::app_resources::Placement;
use rust_fontconfig::FcFontCache;

// Imports for the provided ParsedFont implementation
use crate::parsedfont::ParsedFont;
// Imports from the layout engine's module
use crate::text3::{
    script::estimate_script_and_language, BidiLevel, Color, Direction, FontLoaderTrait,
    FontManager, FontMetrics, FontRef, Glyph, GlyphSource, LayoutError, ParsedFontTrait, Point,
    Script, StyleProperties, TextCombineUpright, TextDecoration, TextOrientation, VerticalMetrics,
    WritingMode,
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
    ) -> Result<Vec<Glyph<Self>>, LayoutError> {
        let codepoints: Vec<u32> = text.chars().map(|c| c as u32).collect();

        // 1. Convert layout engine enums to OpenType tags for allsorts.
        let script_tag = to_opentype_script_tag(script);
        let lang_tag = to_opentype_lang_tag(language);

        // 2. Shape the text using the existing method on ParsedFont.
        let shaped_buffer = self.shape(&codepoints, script_tag, Some(lang_tag));

        // 3. Translate the allsorts output into the layout engine's `Glyph` format.

        // NOTE: `ParsedFontTrait` does not provide `StyleProperties`, which are needed
        // to correctly scale font metrics. We create a dummy style with a default
        // font size. A production-ready implementation would need the style context.
        let font_size = 16.0;
        let dummy_style = Arc::new(StyleProperties {
            font_ref: FontRef::invalid(),
            font_size_px: font_size,
            color: Color {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            },
            letter_spacing: 0.0,
            word_spacing: 0.0,
            line_height: font_size * 1.2,
            text_decoration: TextDecoration::default(),
            font_features: Vec::new(),
            writing_mode: WritingMode::HorizontalTb,
            text_orientation: TextOrientation::Mixed,
            text_combine_upright: None,
        });

        let scale_factor = if self.font_metrics.units_per_em > 0 {
            font_size / (self.font_metrics.units_per_em as f32)
        } else {
            0.01 // Avoid division by zero
        };

        let mut shaped_glyphs = Vec::new();
        let mut text_cursor = text.char_indices().peekable();

        for info in shaped_buffer.infos {
            // This logic is simplified. A full implementation needs to handle complex
            // scripts where character-to-glyph mapping is not 1-to-1. `allsorts`'s
            // `liga_component_pos` helps, but a robust solution requires tracking clusters.
            let (start_byte, source_char) = match text_cursor.next() {
                Some((i, c)) => (i, c),
                None => break, // Ran out of source characters
            };

            let advance = info.size.advance_x as f32 * scale_factor;
            let (offset_x, offset_y) = if let Placement::Distance(d) = info.placement {
                (d.x as f32 * scale_factor, d.y as f32 * scale_factor)
            } else {
                (0.0, 0.0)
            };

            let glyph = Glyph {
                glyph_id: info.glyph.glyph_index,
                codepoint: source_char,
                font: Arc::new(self.clone()),
                style: dummy_style.clone(),
                source: GlyphSource::Char,
                logical_byte_index: start_byte,
                logical_byte_len: source_char.len_utf8(),
                content_index: 0, // Set later by layout engine
                cluster: start_byte as u32,
                advance,
                offset: Point {
                    x: offset_x,
                    y: offset_y,
                },
                // Vertical metrics are not parsed in the provided `ParsedFont` code
                vertical_advance: 0.0,
                vertical_origin_y: 0.0,
                vertical_bearing: Point { x: 0.0, y: 0.0 },
                orientation: crate::text3::GlyphOrientation::Horizontal,
                script,
                bidi_level: BidiLevel::new(if direction.is_rtl() { 1 } else { 0 }),
            };
            shaped_glyphs.push(glyph);
        }

        Ok(shaped_glyphs)
    }

    fn get_hyphen_glyph_and_advance(&self, font_size: f32) -> (u16, f32) {
        let glyph_id = self.lookup_glyph_index('-' as u32).unwrap_or(0);
        let advance_units = self.get_horizontal_advance(glyph_id);
        let scale_factor = if self.font_metrics.units_per_em > 0 {
            font_size / (self.font_metrics.units_per_em as f32)
        } else {
            0.0
        };
        let scaled_advance = advance_units as f32 * scale_factor;
        (glyph_id, scaled_advance)
    }

    fn has_glyph(&self, codepoint: u32) -> bool {
        self.lookup_glyph_index(codepoint).is_some()
    }

    /// Returns vertical metrics for a glyph.
    /// NOTE: The provided `ParsedFont` implementation does not parse vertical layout
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
