//! Built-in and embedded fonts.
//!
//! # Fonts in a PDF
//!
//! A page selects a font by name (`/F1`, `/Tf`) and then shows text with it. There are two kinds:
//!
//! - **Built-in fonts.** The 14 standard fonts (Times, Helvetica, Courier, Symbol, ZapfDingbats)
//!   are expected to exist in every viewer, so nothing is embedded. They are written with
//!   `/WinAnsiEncoding`, one byte per character, and cover Latin text only. See [`BuiltinFont`].
//! - **Embedded fonts.** The font file (TrueType or OpenType) is copied into the PDF as a font
//!   program, so the page looks the same everywhere and can use any script. printpdf writes
//!   them as Type0 composite fonts with `/Encoding /Identity-H`: the content stream does not
//!   contain characters but two-byte *codes*. A code is a CID (character identifier); for
//!   TrueType and name-keyed CFF fonts it is the glyph id, for CID-keyed CFF fonts it is the
//!   CID of the font's own charset (see [`cff_charset_gid_to_cid_map`]).
//!
//! Because the content stream holds glyph ids and no text, an embedded font needs two more
//! things so that viewers can measure and copy the text:
//!
//! - `/W`, the advance width of every code used, in 1/1000 em ([`get_normalized_widths_codes`]);
//! - `/ToUnicode`, a CMap stream that maps each code back to Unicode, which is what copy, paste
//!   and search use (see [`crate::cmap`]).
//!
//! Embedded fonts are *subset* on save: only the glyphs the document uses are kept and the glyph
//! ids are renumbered, because a full CJK font can be several megabytes. See
//! [`PdfSaveOptions::subset_fonts`](crate::PdfSaveOptions::subset_fonts).
//!
//! # How printpdf models fonts
//!
//! - [`ParsedFont`] is one parsed face. It always keeps the font file bytes it was parsed from,
//!   because embedding writes those bytes (after subsetting) into the PDF.
//! - [`PdfFont`] is a [`ParsedFont`] plus embedding settings ([`PrintpdfFontMeta`]). A document
//!   stores them in its resources, keyed by [`FontId`].
//! - [`PdfDocument::add_font`](crate::PdfDocument::add_font) registers a font and returns its
//!   [`FontId`].
//! - [`PdfFontHandle`](crate::PdfFontHandle) names the font of an
//!   [`Op::SetFont`](crate::Op::SetFont): a [`BuiltinFont`] or an external `FontId`.
//!
//! Without the `text_layout` feature, [`ParsedFont`] is a simpler struct with public fields that
//! is parsed with `allsorts` but cannot shape text or subset fonts. With it, [`ParsedFont`]
//! wraps azul-layout's face.
//!
//! # Examples
//!
//! Load a font file, register it and write text with it:
//!
//! ```
//! use printpdf::*;
//!
//! let bytes = std::fs::read("examples/assets/fonts/RobotoMedium.ttf").unwrap();
//! let font = ParsedFont::from_bytes(&bytes, 0, &mut Vec::new()).unwrap();
//!
//! let mut doc = PdfDocument::new("Fonts");
//! let font_id = doc.add_font(&font);
//!
//! let ops = vec![
//!     Op::StartTextSection,
//!     Op::SetTextCursor { pos: Point { x: Mm(10.0).into(), y: Mm(270.0).into() } },
//!     Op::SetFont { font: PdfFontHandle::External(font_id), size: Pt(16.0) },
//!     Op::ShowText { items: vec![TextItem::Text("Hello, fonts".to_string())] },
//!     Op::EndTextSection,
//! ];
//! doc.pages.push(PdfPage::new(Mm(210.0), Mm(297.0), ops));
//!
//! let pdf = doc.save(&PdfSaveOptions::default(), &mut Vec::new());
//! assert!(pdf.starts_with(b"%PDF"));
//! ```

use std::{
    collections::btree_map::BTreeMap,
    vec::Vec,
};

use serde_derive::{Deserialize, Serialize};

use crate::{
    FontId,
};

// Re-exported from azul-layout, which does the parsing and shaping.
#[cfg(feature = "text_layout")]
pub use azul_layout::{
    PdfFontMetrics as FontMetrics, FontParseWarning as PdfFontParseWarning, FontType, OwnedGlyph,
};

/// azul-layout's raw font face.
///
/// Prefer printpdf's [`ParsedFont`], which wraps this type and keeps the source bytes
/// attached; PDF embedding cannot work without them.
///
/// Requires the `text_layout` feature.
#[cfg(feature = "text_layout")]
pub use azul_layout::ParsedFont as AzulParsedFont;

#[cfg(feature = "text_layout")]
pub use self::parsed_font::ParsedFont;

#[cfg(feature = "text_layout")]
mod parsed_font {
    use std::{
        ops::{Deref, DerefMut},
        sync::Arc,
    };

    use super::{AzulParsedFont, PdfFontParseWarning};

    // Prefix of the `data:` URI a font is serialized as. printpdf has its own `Deserialize`
    // because azul's goes through `AzulParsedFont::from_bytes`, which does not keep the source
    // bytes, so a round-tripped font would not be embeddable.
    const FONT_B64_START: &str = "data:font/ttf;base64,";

    /// A parsed font face that keeps the font file bytes it was parsed from.
    ///
    /// Wraps [`AzulParsedFont`] and dereferences to it, so its lookup and metric methods
    /// (`lookup_glyph_index`, `num_glyphs`, `font_metrics`, ...) can be called directly.
    ///
    /// azul-layout's own `from_bytes` does not keep the source bytes, because layout and
    /// shaping never read them. PDF embedding does: the bytes are the `/FontFile2` stream and
    /// subsetting reads the font tables out of them. A face without them is written as an empty
    /// font file, so build faces with [`ParsedFont::from_bytes`] or
    /// [`ParsedFont::from_shared_bytes`] and check [`ParsedFont::has_source_bytes`] on faces
    /// that come from elsewhere.
    ///
    /// Serialized with serde as a `data:font/ttf;base64,...` string holding the source bytes.
    ///
    /// Requires the `text_layout` feature.
    ///
    /// # Examples
    ///
    /// ```
    /// use printpdf::ParsedFont;
    ///
    /// let bytes = std::fs::read("examples/assets/fonts/RobotoMedium.ttf").unwrap();
    /// let mut warnings = Vec::new();
    /// let font = ParsedFont::from_bytes(&bytes, 0, &mut warnings).unwrap();
    /// assert!(font.has_source_bytes());
    /// assert!(font.lookup_glyph_index('A' as u32).is_some());
    /// ```
    #[derive(Debug, Clone, PartialEq)]
    pub struct ParsedFont(AzulParsedFont);

    impl ParsedFont {
        /// Parses one face from font file bytes and keeps the bytes for embedding.
        ///
        /// `font_index` selects the face inside a TrueType/OpenType collection (`.ttc`,
        /// `.otc`); use `0` for a plain `.ttf` or `.otf`. Returns `None` when the bytes do not
        /// parse as a font; recoverable problems are pushed to `warnings`. A bare CID-keyed CFF
        /// table, as printpdf embeds it (see [`super::extract_cid_keyed_cff`]), is accepted too.
        pub fn from_bytes(
            bytes: &[u8],
            font_index: usize,
            warnings: &mut Vec<PdfFontParseWarning>,
        ) -> Option<Self> {
            // A bare CID-keyed CFF table has no head/hhea/maxp/hmtx, which azul's parser
            // requires, so it is wrapped in a synthetic sfnt. The face keeps the wrapper as its
            // source bytes, because azul indexes them by byte ranges computed against the
            // wrapped data. Embedding reads the CFF table back out of it unchanged.
            if let Some(wrapped) = crate::font::wrap_bare_cff_as_sfnt(bytes) {
                let inner = AzulParsedFont::from_bytes(&wrapped, 0, warnings)?;
                return Some(Self::attach_source_bytes(inner, &wrapped));
            }
            let inner = AzulParsedFont::from_bytes(bytes, font_index, warnings)?;
            Some(Self::attach_source_bytes(inner, bytes))
        }

        /// Like [`ParsedFont::from_bytes`] for bytes that are already shared in an `Arc`.
        ///
        /// The face keeps that allocation as its source bytes instead of copying it, and decodes
        /// glyph outlines from it lazily. The result is the same face as `from_bytes` gives for
        /// the same bytes. A bare CID-keyed CFF table is still wrapped in a synthetic sfnt,
        /// which is a copy.
        pub fn from_shared_bytes(
            bytes: Arc<[u8]>,
            font_index: usize,
            warnings: &mut Vec<PdfFontParseWarning>,
        ) -> Option<Self> {
            if let Some(wrapped) = crate::font::wrap_bare_cff_as_sfnt(&bytes) {
                let inner = AzulParsedFont::from_bytes(&wrapped, 0, warnings)?;
                return Some(Self::attach_source_bytes(inner, &wrapped));
            }
            let shared = Arc::new(rust_fontconfig::FontBytes::Owned(bytes));
            let inner = AzulParsedFont::from_bytes_shared(shared.clone(), font_index, warnings)?;
            if inner.source_bytes_for_subset().is_some() {
                return Some(Self(inner));
            }
            Some(Self(inner.with_source_bytes(shared)))
        }

        /// Adopts a face that azul-layout already parsed, for example one from the HTML layout
        /// font cache.
        ///
        /// Faces from the cache already carry their source bytes. A face that arrives without
        /// them is kept as it is: [`ParsedFont::has_source_bytes`] returns `false` and saving
        /// reports an error instead of embedding an empty font program.
        pub fn from_azul(inner: AzulParsedFont) -> Self {
            Self(inner)
        }

        /// Attaches `bytes` as the source bytes unless the face already carries some.
        fn attach_source_bytes(inner: AzulParsedFont, bytes: &[u8]) -> Self {
            if inner.source_bytes_for_subset().is_some() {
                return Self(inner);
            }
            Self(inner.with_source_bytes(Arc::new(rust_fontconfig::FontBytes::Owned(
                Arc::from(bytes.to_vec()),
            ))))
        }

        /// The font file bytes this face was parsed from, if they are still attached.
        ///
        /// Always `Some` for faces built by [`ParsedFont::from_bytes`].
        pub fn source_bytes(&self) -> Option<Arc<rust_fontconfig::FontBytes>> {
            self.0.source_bytes_for_subset()
        }

        /// Whether the source bytes are attached, which a face needs to be embedded in a PDF.
        pub fn has_source_bytes(&self) -> bool {
            self.source_bytes().is_some()
        }

        /// Borrows the underlying azul-layout face.
        pub fn as_azul(&self) -> &AzulParsedFont {
            &self.0
        }

        /// Unwraps to the underlying azul-layout face.
        pub fn into_inner(self) -> AzulParsedFont {
            self.0
        }
    }

    impl Deref for ParsedFont {
        type Target = AzulParsedFont;
        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }

    impl DerefMut for ParsedFont {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.0
        }
    }

    impl From<AzulParsedFont> for ParsedFont {
        fn from(inner: AzulParsedFont) -> Self {
            Self::from_azul(inner)
        }
    }

    impl serde::Serialize for ParsedFont {
        fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            use base64::Engine;

            // Same wire format as azul, but an empty font is an error: a face that cannot
            // produce its bytes cannot be embedded.
            //
            // The retained source bytes are written verbatim. Rebuilding them with
            // `to_bytes(None)` walks a fixed TrueType table list, which fails for CFF faces
            // and for the subset fonts printpdf writes itself, so it is only the fallback.
            let encoded = match self.source_bytes() {
                Some(b) => base64::prelude::BASE64_STANDARD.encode(b.as_slice()),
                None => {
                    let bytes = self.0.to_bytes(None).map_err(|e| {
                        serde::ser::Error::custom(format!("font has no source bytes: {e}"))
                    })?;
                    base64::prelude::BASE64_STANDARD.encode(&bytes)
                }
            };
            let s = format!("{FONT_B64_START}{encoded}");
            s.serialize(serializer)
        }
    }

    impl<'de> serde::Deserialize<'de> for ParsedFont {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            use base64::Engine;

            let s = String::deserialize(deserializer)?;
            let b64 = s.strip_prefix(FONT_B64_START).ok_or_else(|| {
                serde::de::Error::custom(format!(
                    "font must be a {FONT_B64_START}… data URI, got {:.32?}",
                    s
                ))
            })?;
            let bytes = base64::prelude::BASE64_STANDARD
                .decode(b64)
                .map_err(serde::de::Error::custom)?;

            // Goes through printpdf's `from_bytes`, which re-attaches the source bytes.
            let mut warnings = Vec::new();
            ParsedFont::from_bytes(&bytes, 0, &mut warnings).ok_or_else(|| {
                serde::de::Error::custom(format!("font deserialization error: {warnings:?}"))
            })
        }
    }
}

// Without the `text_layout` feature these simplified types take the place of azul-layout's. The
// signatures match, so code that compiles with one configuration compiles with the other.
/// A parsed font face for builds without the `text_layout` feature.
///
/// Holds the font file bytes and the few tables embedding needs (character to glyph map,
/// advance widths, metrics). It cannot shape text or subset fonts. Build it with
/// [`ParsedFont::from_bytes`].
#[cfg(not(feature = "text_layout"))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParsedFont {
    /// The font file bytes the face was parsed from; embedded as is, since there is no subsetting.
    pub original_bytes: Vec<u8>,
    /// Index of the face inside a font collection (`0` for a plain font file).
    pub font_index: u32,
    /// The font's name, if known.
    pub font_name: Option<String>,
    /// Unicode codepoint to glyph id, from the font's `cmap` table.
    pub codepoint_to_glyph: BTreeMap<u32, u16>,
    /// Advance width of each glyph id, in font units (`units_per_em` per em).
    pub glyph_widths: BTreeMap<u16, u16>,
    /// Font units per em, typically 1000 or 2048.
    pub units_per_em: u16,
    /// Ascent and descent in font units.
    pub font_metrics: FontMetrics,
    /// Whether the outlines are TrueType (`glyf`) or CFF; decides how the font is embedded.
    pub font_type: FontType,
    /// Bounding box and slope metrics for the PDF font descriptor.
    pub pdf_font_metrics: PdfFontMetricsStub,
}

/// The metrics the PDF font descriptor needs, for builds without the `text_layout` feature.
///
/// Lengths are in font units (`units_per_em` per em).
#[cfg(not(feature = "text_layout"))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PdfFontMetricsStub {
    /// Font units per em.
    pub units_per_em: u16,
    /// Left edge of the font bounding box (`head.xMin`).
    pub x_min: i16,
    /// Bottom edge of the font bounding box (`head.yMin`).
    pub y_min: i16,
    /// Right edge of the font bounding box (`head.xMax`).
    pub x_max: i16,
    /// Top edge of the font bounding box (`head.yMax`).
    pub y_max: i16,
    /// Rise of the caret slope (`hhea`); the descriptor's `/ItalicAngle` is derived from it.
    pub caret_slope_rise: i16,
    /// Run of the caret slope (`hhea`).
    pub caret_slope_run: i16,
    /// `OS/2` `usWeightClass` (100 to 900), from which `/StemV` is estimated; 0 when the font has no `OS/2` table.
    pub us_weight_class: u16,
}

#[cfg(not(feature = "text_layout"))]
impl Default for PdfFontMetricsStub {
    fn default() -> Self {
        Self {
            units_per_em: 1000,
            x_min: 0,
            y_min: -200,
            x_max: 1000,
            y_max: 800,
            // Upright (rise 1, run 0), and no OS/2 table.
            caret_slope_rise: 1,
            caret_slope_run: 0,
            us_weight_class: 0,
        }
    }
}

#[cfg(not(feature = "text_layout"))]
impl ParsedFont {
    /// The font file bytes this face was parsed from, or `None` when there are none.
    ///
    /// Mirrors the `text_layout` API, where the result is the shared `Arc<FontBytes>`.
    pub fn source_bytes(&self) -> Option<Vec<u8>> {
        (!self.original_bytes.is_empty()).then(|| self.original_bytes.clone())
    }

    /// Whether the source bytes are present, which a face needs to be embedded in a PDF.
    pub fn has_source_bytes(&self) -> bool {
        !self.original_bytes.is_empty()
    }

    /// Parses one face from font file bytes.
    ///
    /// Same signature as with the `text_layout` feature. `index` selects the face inside a
    /// font collection (`0` for a plain font file). Returns `None` when the bytes do not
    /// parse as a font; a missing Unicode `cmap` or `hmtx` table is reported in `warnings`.
    /// Reads the character to glyph map, advance widths and metrics with `allsorts`.
    pub fn from_bytes(
        bytes: &[u8],
        index: usize,
        warnings: &mut Vec<PdfFontParseWarning>,
    ) -> Option<Self> {
        use allsorts::{
            binary::read::ReadScope,
            font_data::FontData,
            tables::{cmap::Cmap, FontTableProvider, HeadTable, HheaTable, HmtxTable, MaxpTable},
            tag,
        };

        let mut warn = |msg: &str| {
            warnings.push(PdfFontParseWarning {
                severity: FontParseSeverity::Warning,
                message: msg.to_string(),
            })
        };

        let font_file = ReadScope::new(bytes).read::<FontData<'_>>().ok()?;
        let provider = font_file.table_provider(index).ok()?;

        let head = provider
            .read_table_data(tag::HEAD)
            .ok()
            .and_then(|d| ReadScope::new(&d).read::<HeadTable>().ok())?;
        let maxp = provider
            .read_table_data(tag::MAXP)
            .ok()
            .and_then(|d| ReadScope::new(&d).read::<MaxpTable>().ok())?;
        let hhea = provider
            .read_table_data(tag::HHEA)
            .ok()
            .and_then(|d| ReadScope::new(&d).read::<HheaTable>().ok())?;

        // Character to glyph map; without it every character would be glyph 0.
        let mut codepoint_to_glyph = BTreeMap::new();
        let cmap_data = provider.read_table_data(tag::CMAP).ok();
        match cmap_data
            .as_deref()
            .and_then(|d| ReadScope::new(d).read::<Cmap<'_>>().ok())
            .and_then(|cmap| allsorts::font::read_cmap_subtable(&cmap).ok().flatten())
        {
            Some((_encoding, subtable)) => {
                let _ = subtable.mappings_fn(|cp, gid| {
                    codepoint_to_glyph.insert(cp, gid);
                });
            }
            None => warn("font has no usable Unicode cmap subtable; text will not map to glyphs"),
        }

        // Advance widths in font units; `/W` scales them to 1/1000 em when the PDF is written.
        let mut glyph_widths = BTreeMap::new();
        let hmtx_data = provider.read_table_data(tag::HMTX).ok();
        match hmtx_data.as_deref().and_then(|d| {
            ReadScope::new(d)
                .read_dep::<HmtxTable<'_>>((
                    usize::from(maxp.num_glyphs),
                    usize::from(hhea.num_h_metrics),
                ))
                .ok()
        }) {
            Some(hmtx) => {
                for gid in 0..maxp.num_glyphs {
                    if let Ok(advance) = hmtx.horizontal_advance(gid) {
                        glyph_widths.insert(gid, advance);
                    }
                }
            }
            None => warn("font has no hmtx table; glyph advances will be zero"),
        }

        // CFF outlines live in a `CFF ` table; TrueType outlines in `glyf`.
        let font_type = if provider.has_table(tag::CFF) {
            FontType::OpenTypeCFF(())
        } else {
            FontType::TrueType
        };

        // OS/2 usWeightClass drives the FontDescriptor's /StemV estimate. The table is
        // optional (0 means "absent", and /StemV falls back to a constant).
        let os2_data = provider.read_table_data(tag::OS_2).ok();
        let us_weight_class = os2_data
            .as_deref()
            .and_then(|d| {
                ReadScope::new(d)
                    .read_dep::<allsorts::tables::os2::Os2>(d.len())
                    .ok()
            })
            .map(|os2| os2.us_weight_class)
            .unwrap_or(0);

        Some(ParsedFont {
            original_bytes: bytes.to_vec(),
            font_index: index as u32,
            font_name: None,
            codepoint_to_glyph,
            glyph_widths,
            units_per_em: head.units_per_em,
            font_metrics: FontMetrics {
                ascent: hhea.ascender,
                descent: hhea.descender,
            },
            font_type,
            pdf_font_metrics: PdfFontMetricsStub {
                units_per_em: head.units_per_em,
                x_min: head.x_min,
                y_min: head.y_min,
                x_max: head.x_max,
                y_max: head.y_max,
                caret_slope_rise: hhea.caret_slope_rise,
                caret_slope_run: hhea.caret_slope_run,
                us_weight_class,
            },
        })
    }

    /// Creates a face from a font file and a hand-made glyph table, without parsing it.
    pub fn with_glyph_data(
        bytes: Vec<u8>,
        index: u32,
        font_name: Option<String>,
        codepoint_to_glyph: BTreeMap<u32, u16>,
        glyph_widths: BTreeMap<u16, u16>,
        units_per_em: u16,
        font_metrics: FontMetrics,
    ) -> Self {
        ParsedFont {
            original_bytes: bytes,
            font_index: index,
            font_name,
            codepoint_to_glyph,
            glyph_widths,
            units_per_em,
            font_metrics,
            font_type: FontType::TrueType,
            pdf_font_metrics: PdfFontMetricsStub { units_per_em, ..Default::default() },
        }
    }

    /// Maps a Unicode codepoint to a glyph id.
    pub fn set_codepoint_mapping(&mut self, codepoint: u32, gid: u16) {
        self.codepoint_to_glyph.insert(codepoint, gid);
    }

    /// Sets the advance width of a glyph id, in font units.
    pub fn set_glyph_width(&mut self, gid: u16, width: u16) {
        self.glyph_widths.insert(gid, width);
    }

    /// The advance width of a glyph id in font units, if known.
    pub fn get_glyph_width(&self, gid: u16) -> Option<u16> {
        self.glyph_widths.get(&gid).copied()
    }

    /// The glyph id of a Unicode codepoint, if the font has one.
    pub fn lookup_glyph_index(&self, codepoint: u32) -> Option<u16> {
        self.codepoint_to_glyph.get(&codepoint).copied()
    }

    /// Always `None`: finding the character of a glyph id needs the `text_layout` feature.
    pub fn get_glyph_primary_char(&self, _gid: u16) -> Option<char> {
        None
    }
}

/// Outline format of a font, for builds without the `text_layout` feature.
#[cfg(not(feature = "text_layout"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FontType {
    /// TrueType outlines (`glyf` table).
    TrueType,
    /// CFF outlines in an OpenType (`OTTO`) font.
    OpenTypeCFF(()),
}

/// How serious a font-parse diagnostic is.
#[cfg(not(feature = "text_layout"))]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FontParseSeverity {
    /// Informational.
    Info,
    /// The font parsed, but something is missing or off.
    Warning,
    /// The font could not be used.
    Error,
}

/// A problem found while parsing a font; stand-in for azul-layout's `FontParseWarning` in
/// builds without the `text_layout` feature.
#[cfg(not(feature = "text_layout"))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FontParseWarning {
    /// How serious the problem is.
    pub severity: FontParseSeverity,
    /// Human-readable description.
    pub message: String,
}

/// Alias matching the name azul-layout exports with the `text_layout` feature.
#[cfg(not(feature = "text_layout"))]
pub type PdfFontParseWarning = FontParseWarning;

/// Placeholder for azul-layout's decoded glyph type, which does not exist without `text_layout`.
#[cfg(not(feature = "text_layout"))]
pub type OwnedGlyph = ();

/// Vertical font metrics, for builds without the `text_layout` feature.
#[cfg(not(feature = "text_layout"))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FontMetrics {
    /// Distance from the baseline to the top of the line, in font units (`hhea` ascender).
    pub ascent: i16,
    /// Distance from the baseline to the bottom of the line, in font units; negative below the
    /// baseline (`hhea` descender).
    pub descent: i16,
}

/// A font program reduced to the glyphs a document uses.
#[derive(Debug, Clone)]
pub struct SubsetFont {
    /// The subset font file (an sfnt).
    pub bytes: Vec<u8>,
    /// Original glyph id to `(glyph id in the subset, text the glyph stands for)`. The text is
    /// several characters for a ligature.
    pub glyph_mapping: BTreeMap<u16, (u16, String)>,
}

/// PDF-specific settings of a font that do not belong in the layout face.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrintpdfFontMeta {
    /// Glyph id to CID map of the source PDF, when the font was read from one. The serializer
    /// does not read it.
    pub original_gid_to_cid: Option<BTreeMap<u16, u16>>,
    /// The `/ToUnicode` CMap text of the source PDF, when the font was read from one. The
    /// serializer does not read it; it generates a new CMap from the glyphs used.
    pub original_to_unicode_map: Option<String>,
    /// How the font is embedded.
    pub embedding_mode: FontEmbeddingMode,
    /// The font is subset on save only when this is `true`, [`Self::embedding_mode`] is
    /// [`FontEmbeddingMode::Subset`] and `PdfSaveOptions::subset_fonts` is set.
    pub requires_subsetting: bool,
}

/// How a font is embedded when the document is saved.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FontEmbeddingMode {
    /// Embed the whole font file.
    Full,
    /// Embed only the glyphs the document uses (default).
    Subset,
    /// Meant for system fonts that are not embedded. Saving does not act on it: the font is
    /// embedded in full, as with `Full`.
    Reference,
}

impl Default for PrintpdfFontMeta {
    fn default() -> Self {
        Self {
            original_gid_to_cid: None,
            original_to_unicode_map: None,
            embedding_mode: FontEmbeddingMode::Subset,
            requires_subsetting: true,
        }
    }
}

/// A font as stored in a document's resources: the face plus its PDF settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PdfFont {
    /// The parsed face, with its source bytes.
    pub parsed_font: ParsedFont,
    /// Embedding settings.
    pub meta: PrintpdfFontMeta,
}

impl PdfFont {
    /// Wraps a face with the default settings: subset on save.
    pub fn new(parsed_font: ParsedFont) -> Self {
        Self {
            parsed_font,
            meta: PrintpdfFontMeta::default(),
        }
    }

    /// Wraps a face with explicit settings.
    pub fn with_meta(parsed_font: ParsedFont, meta: PrintpdfFontMeta) -> Self {
        Self { parsed_font, meta }
    }
}

/// A font that is either one of the 14 built-in fonts or a parsed external one.
#[derive(Debug, Clone)]
pub enum Font {
    /// One of the 14 built-in fonts.
    BuiltinFont(BuiltinFont),
    /// A font loaded from a font file, with its PDF settings.
    ExternalFont(ParsedFont, PrintpdfFontMeta),
}

/// The 14 standard PDF fonts. Every viewer has them, so they are never embedded.
///
/// Serialized with serde as kebab-case names, for example `"times-roman"`.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[derive(Default)]
pub enum BuiltinFont {
    /// Times Roman (serif).
    #[default]
    TimesRoman,
    /// Times Bold.
    TimesBold,
    /// Times Italic.
    TimesItalic,
    /// Times Bold Italic.
    TimesBoldItalic,
    /// Helvetica (sans-serif).
    Helvetica,
    /// Helvetica Bold.
    HelveticaBold,
    /// Helvetica Oblique.
    HelveticaOblique,
    /// Helvetica Bold Oblique.
    HelveticaBoldOblique,
    /// Courier (monospace).
    Courier,
    /// Courier Oblique.
    CourierOblique,
    /// Courier Bold.
    CourierBold,
    /// Courier Bold Oblique.
    CourierBoldOblique,
    /// Symbol (Greek letters and mathematical symbols).
    Symbol,
    /// ZapfDingbats (ornaments and arrows).
    ZapfDingbats,
}


include!("../defaultfonts/mapping.rs");

impl BuiltinFont {
    /// The built-in font whose bundled subset font file is byte-identical to `bytes`, if any.
    pub fn check_if_matches(bytes: &[u8]) -> Option<Self> {
        let matching_based_on_len = match_len(bytes)?;
        // if the length is equal, check for equality
        if bytes == matching_based_on_len.get_subset_font().bytes.as_slice() {
            Some(matching_based_on_len)
        } else {
            None
        }
    }

    /// Parses the bundled subset font file of this built-in font, so that its glyphs can be
    /// measured and shaped. `None` if parsing fails.
    pub fn get_parsed_font(&self) -> Option<ParsedFont> {
        let subset = self.get_subset_font();
        ParsedFont::from_bytes(&subset.bytes, 0, &mut Vec::new())
    }

    /// A CSS `font-family` list for this font, for example `"Times New Roman, Times, serif"`
    /// for [`BuiltinFont::TimesRoman`].
    pub fn get_svg_font_family(&self) -> &'static str {
        match self {
            BuiltinFont::TimesRoman => "Times New Roman, Times, serif",
            BuiltinFont::TimesBold => "Times New Roman, Times, serif",
            BuiltinFont::TimesItalic => "Times New Roman, Times, serif",
            BuiltinFont::TimesBoldItalic => "Times New Roman, Times, serif",
            BuiltinFont::Helvetica => "Helvetica, Arial, sans-serif",
            BuiltinFont::HelveticaBold => "Helvetica, Arial, sans-serif",
            BuiltinFont::HelveticaOblique => "Helvetica, Arial, sans-serif",
            BuiltinFont::HelveticaBoldOblique => "Helvetica, Arial, sans-serif",
            BuiltinFont::Courier => "Courier New, Courier, monospace",
            BuiltinFont::CourierOblique => "Courier New, Courier, monospace",
            BuiltinFont::CourierBold => "Courier New, Courier, monospace",
            BuiltinFont::CourierBoldOblique => "Courier New, Courier, monospace",
            BuiltinFont::Symbol => "Symbol",
            BuiltinFont::ZapfDingbats => "Zapf Dingbats",
        }
    }

    /// The CSS `font-weight` of this font: `"bold"` or `"normal"`.
    pub fn get_font_weight(&self) -> &'static str {
        match self {
            BuiltinFont::TimesRoman
            | BuiltinFont::TimesItalic
            | BuiltinFont::Helvetica
            | BuiltinFont::HelveticaOblique
            | BuiltinFont::Courier
            | BuiltinFont::CourierOblique
            | BuiltinFont::Symbol
            | BuiltinFont::ZapfDingbats => "normal",
            BuiltinFont::TimesBold
            | BuiltinFont::TimesBoldItalic
            | BuiltinFont::HelveticaBold
            | BuiltinFont::HelveticaBoldOblique
            | BuiltinFont::CourierBold
            | BuiltinFont::CourierBoldOblique => "bold",
        }
    }

    /// The CSS `font-style` of this font: `"italic"` (also for oblique) or `"normal"`.
    pub fn get_font_style(&self) -> &'static str {
        match self {
            BuiltinFont::TimesItalic
            | BuiltinFont::TimesBoldItalic
            | BuiltinFont::HelveticaOblique
            | BuiltinFont::HelveticaBoldOblique
            | BuiltinFont::CourierOblique
            | BuiltinFont::CourierBoldOblique => "italic",
            _ => "normal",
        }
    }

    /// The bundled font file of this font, already subset to the WinAnsi (Windows-1252)
    /// characters, with the mapping from original to subset glyph ids.
    pub fn get_subset_font(&self) -> SubsetFont {
        use self::BuiltinFont::*;

        SubsetFont {
            bytes: match self {
                TimesRoman => crate::utils::uncompress(include_bytes!(
                    "../defaultfonts/Times-Roman.subset.ttf"
                )),
                TimesBold => crate::utils::uncompress(include_bytes!(
                    "../defaultfonts/Times-Bold.subset.ttf"
                )),
                TimesItalic => crate::utils::uncompress(include_bytes!(
                    "../defaultfonts/Times-Italic.subset.ttf"
                )),
                TimesBoldItalic => crate::utils::uncompress(include_bytes!(
                    "../defaultfonts/Times-BoldItalic.subset.ttf"
                )),
                Helvetica => {
                    crate::utils::uncompress(include_bytes!("../defaultfonts/Helvetica.subset.ttf"))
                }
                HelveticaBold => crate::utils::uncompress(include_bytes!(
                    "../defaultfonts/Helvetica-Bold.subset.ttf"
                )),
                HelveticaOblique => crate::utils::uncompress(include_bytes!(
                    "../defaultfonts/Helvetica-Oblique.subset.ttf"
                )),
                HelveticaBoldOblique => crate::utils::uncompress(include_bytes!(
                    "../defaultfonts/Helvetica-BoldOblique.subset.ttf"
                )),
                Courier => {
                    crate::utils::uncompress(include_bytes!("../defaultfonts/Courier.subset.ttf"))
                }
                CourierOblique => crate::utils::uncompress(include_bytes!(
                    "../defaultfonts/Courier-Oblique.subset.ttf"
                )),
                CourierBold => crate::utils::uncompress(include_bytes!(
                    "../defaultfonts/Courier-Bold.subset.ttf"
                )),
                CourierBoldOblique => crate::utils::uncompress(include_bytes!(
                    "../defaultfonts/Courier-BoldOblique.subset.ttf"
                )),
                Symbol => {
                    crate::utils::uncompress(include_bytes!("../defaultfonts/Symbol.subset.ttf"))
                }
                ZapfDingbats => crate::utils::uncompress(include_bytes!(
                    "../defaultfonts/ZapfDingbats.subset.ttf"
                )),
            },
            glyph_mapping: FONTS
                .iter()
                .filter_map(|(font_id, old_gid, new_gid, char)| {
                    if *font_id == self.get_num() {
                        Some((*old_gid, (*new_gid, char.to_string())))
                    } else {
                        None
                    }
                })
                .collect(),
        }
    }

    /// The resource name of this font in a content stream, `"F1"` to `"F14"`.
    pub fn get_pdf_id(&self) -> &'static str {
        use self::BuiltinFont::*;
        match self {
            TimesRoman => "F1",
            TimesBold => "F2",
            TimesItalic => "F3",
            TimesBoldItalic => "F4",
            Helvetica => "F5",
            HelveticaBold => "F6",
            HelveticaOblique => "F7",
            HelveticaBoldOblique => "F8",
            Courier => "F9",
            CourierOblique => "F10",
            CourierBold => "F11",
            CourierBoldOblique => "F12",
            Symbol => "F13",
            ZapfDingbats => "F14",
        }
    }

    /// The index of this font in declaration order, `0` to `13`.
    pub fn get_num(&self) -> usize {
        use self::BuiltinFont::*;
        match self {
            TimesRoman => 0,
            TimesBold => 1,
            TimesItalic => 2,
            TimesBoldItalic => 3,
            Helvetica => 4,
            HelveticaBold => 5,
            HelveticaOblique => 6,
            HelveticaBoldOblique => 7,
            Courier => 8,
            CourierOblique => 9,
            CourierBold => 10,
            CourierBoldOblique => 11,
            Symbol => 12,
            ZapfDingbats => 13,
        }
    }

    /// Looks a font up by its [`BuiltinFont::get_id`] name (`"Helvetica-Bold"`) or its
    /// [`BuiltinFont::get_pdf_id`] resource name (`"F6"`).
    pub fn from_id(s: &str) -> Option<Self> {
        use self::BuiltinFont::*;
        match s {
            "Times-Roman" | "F1" => Some(TimesRoman),
            "Times-Bold" | "F2" => Some(TimesBold),
            "Times-Italic" | "F3" => Some(TimesItalic),
            "Times-BoldItalic" | "F4" => Some(TimesBoldItalic),
            "Helvetica" | "F5" => Some(Helvetica),
            "Helvetica-Bold" | "F6" => Some(HelveticaBold),
            "Helvetica-Oblique" | "F7" => Some(HelveticaOblique),
            "Helvetica-BoldOblique" | "F8" => Some(HelveticaBoldOblique),
            "Courier" | "F9" => Some(Courier),
            "Courier-Oblique" | "F10" => Some(CourierOblique),
            "Courier-Bold" | "F11" => Some(CourierBold),
            "Courier-BoldOblique" | "F12" => Some(CourierBoldOblique),
            "Symbol" | "F13" => Some(Symbol),
            "ZapfDingbats" | "F14" => Some(ZapfDingbats),
            _ => None,
        }
    }

    /// The PDF base font name of this font, for example `"Helvetica-Bold"` (the `/BaseFont`).
    pub fn get_id(&self) -> &'static str {
        use self::BuiltinFont::*;
        match self {
            TimesRoman => "Times-Roman",
            TimesBold => "Times-Bold",
            TimesItalic => "Times-Italic",
            TimesBoldItalic => "Times-BoldItalic",
            Helvetica => "Helvetica",
            HelveticaBold => "Helvetica-Bold",
            HelveticaOblique => "Helvetica-Oblique",
            HelveticaBoldOblique => "Helvetica-BoldOblique",
            Courier => "Courier",
            CourierOblique => "Courier-Oblique",
            CourierBold => "Courier-Bold",
            CourierBoldOblique => "Courier-BoldOblique",
            Symbol => "Symbol",
            ZapfDingbats => "ZapfDingbats",
        }
    }

    /// All 14 built-in fonts, in declaration order.
    pub fn all_ids() -> [BuiltinFont; 14] {
        use self::BuiltinFont::*;
        [
            TimesRoman,
            TimesBold,
            TimesItalic,
            TimesBoldItalic,
            Helvetica,
            HelveticaBold,
            HelveticaOblique,
            HelveticaBoldOblique,
            Courier,
            CourierOblique,
            CourierBold,
            CourierBoldOblique,
            Symbol,
            ZapfDingbats,
        ]
    }
}

impl Font {
    /// The parsed face of an external font, `None` for a built-in one.
    pub fn get_parsed_font(&self) -> Option<&ParsedFont> {
        match self {
            Font::BuiltinFont(_) => None,
            Font::ExternalFont(parsed, _) => Some(parsed),
        }
    }

    /// The parsed face of an external font, mutably.
    pub fn get_parsed_font_mut(&mut self) -> Option<&mut ParsedFont> {
        match self {
            Font::BuiltinFont(_) => None,
            Font::ExternalFont(parsed, _) => Some(parsed),
        }
    }

    /// The PDF settings of an external font, `None` for a built-in one.
    pub fn get_font_meta(&self) -> Option<&PrintpdfFontMeta> {
        match self {
            Font::BuiltinFont(_) => None,
            Font::ExternalFont(_, meta) => Some(meta),
        }
    }

    /// The PDF settings of an external font, mutably.
    pub fn get_font_meta_mut(&mut self) -> Option<&mut PrintpdfFontMeta> {
        match self {
            Font::BuiltinFont(_) => None,
            Font::ExternalFont(_, meta) => Some(meta),
        }
    }
}

/// Reduces `font` to the glyphs in `glyph_ids` (glyph id to the text it stands for).
///
/// Glyph 0 (`.notdef`) is always kept and comes first; the other glyphs are renumbered in
/// ascending order of their original id, and [`SubsetFont::glyph_mapping`] records the new
/// ids. Fails when the face has no source bytes or the font tables cannot be read.
///
/// Requires the `text_layout` feature; without it the whole font is returned.
#[cfg(feature = "text_layout")]
pub fn subset_font(font: &ParsedFont, glyph_ids: &BTreeMap<u16, String>) -> Result<SubsetFont, String> {
    use allsorts::{binary::read::ReadScope, font_data::FontData, subset::CmapTarget};

    // Subsetting reads the font tables from the source bytes.
    let original_bytes = font
        .source_bytes()
        .ok_or_else(|| "ParsedFont has no source bytes to subset".to_string())?;
    let scope = ReadScope::new(original_bytes.as_slice());
    let font_file = scope.read::<FontData<'_>>().map_err(|e| e.to_string())?;
    let provider = font_file
        .table_provider(font.original_index)
        .map_err(|e| e.to_string())?;

    // Glyph 0 (.notdef) must come first and there must be no duplicates, which allsorts
    // requires of `subset()`. Without .notdef, allsorts renumbers silently and the first used
    // glyph lands in slot 0, which readers treat as a missing glyph. The keys of a `BTreeMap`
    // are already sorted and unique.
    let ids: Vec<u16> = std::iter::once(0)
        .chain(glyph_ids.keys().copied().filter(|gid| *gid != 0))
        .collect();

    // `SubsetProfile::Pdf` and a Unicode cmap. The subset cmap may be empty: rendering goes
    // through Identity-H glyph ids and text extraction through /ToUnicode and /ActualText,
    // and neither reads the font's own cmap.
    let bytes = allsorts::subset::subset(
        &provider,
        &ids,
        &allsorts::subset::SubsetProfile::Pdf,
        CmapTarget::Unicode,
    ).map_err(|e| e.to_string())?;

    // allsorts renumbers each requested glyph to its position in `ids` (the glyf and the CFF
    // subsetter both keep the input order), so the new glyph id is the index. Component glyphs
    // of composites are appended after the requested ids. The mapping must come from this
    // order and not from the subset's cmap: glyphs the shaper substituted (ligatures, alternate
    // forms) have no cmap entry of their own.
    let glyph_mapping: BTreeMap<u16, (u16, String)> = ids
        .iter()
        .enumerate()
        .filter_map(|(idx, &original_gid)| {
            glyph_ids.get(&original_gid).map(|ch| {
                // `ch` is the whole text of the glyph, several characters for a ligature, and
                // ToUnicode keeps all of them.
                (original_gid, (idx as u16, ch.clone()))
            })
        })
        .collect();

    Ok(SubsetFont {
        bytes,
        glyph_mapping,
    })
}

/// Returns the whole font file without subsetting, with an empty glyph mapping.
///
/// Without the `text_layout` feature fonts cannot be subset.
#[cfg(not(feature = "text_layout"))]
pub fn subset_font(font: &ParsedFont, _glyph_ids: &BTreeMap<u16, String>) -> Result<SubsetFont, String> {
    Ok(SubsetFont {
        bytes: font.original_bytes.clone(),
        glyph_mapping: BTreeMap::new(),
    })
}

/// Builds the `/ToUnicode` CMap text of an embedded font.
///
/// `glyph_ids` pairs each code used in the content stream with the text it stands for, and
/// `font_id` names the CMap. See [`crate::cmap::ToUnicodeCMap::to_cmap_string`].
pub fn generate_cmap_string(_font: &ParsedFont, font_id: &FontId, glyph_ids: &[(u16, String)]) -> String {
    let mappings = glyph_ids
        .iter()
        .map(|(gid, unicode)| {
            (*gid as u32, unicode.chars().map(|c| c as u32).collect())
        })
        .collect();

    let cmap = crate::cmap::ToUnicodeCMap { mappings };
    cmap.to_cmap_string(&font_id.0)
}

/// Repackages one face of a TrueType/OpenType collection (`ttcf`) as a standalone font file.
///
/// A font program in a PDF must be a single face, so a collection cannot be embedded as is.
/// Returns `None` when `bytes` is not a collection or the face at `index` cannot be extracted.
pub fn extract_collection_face(bytes: &[u8], index: usize) -> Option<Vec<u8>> {
    use allsorts::{
        binary::read::ReadScope, font_data::FontData, subset::whole_font,
        tables::FontTableProvider,
    };

    if bytes.get(..4) != Some(b"ttcf") {
        return None;
    }
    let font_file = ReadScope::new(bytes).read::<FontData<'_>>().ok()?;
    let provider = font_file.table_provider(index).ok()?;
    let tags = provider.table_tags()?;
    whole_font(&provider, &tags).ok()
}

/// The glyph id to CID map of the CFF charset of a font program, or `None` when the codes in
/// the content stream can stay glyph ids.
///
/// Under `/Encoding /Identity-H` the two-byte codes in the content stream are CIDs. How a
/// viewer turns a CID into a glyph depends on the descendant font:
///
/// - `CIDFontType2` (TrueType `glyf`): through `/CIDToGIDMap`, which printpdf leaves at
///   `/Identity`, so CID and glyph id are equal. `None`.
/// - `CIDFontType0`, name-keyed CFF: the CID is the glyph index. `None`.
/// - `CIDFontType0`, CID-keyed CFF: the viewer maps the CID to a glyph through the CFF
///   charset (ISO 32000-1, 9.7.4.2). The charset is not the identity in real fonts, and the
///   subsetter keeps the original CIDs, so the content stream, `/W` and `/ToUnicode` must all
///   be keyed by the CIDs this function returns.
///
/// `font_bytes` is a whole sfnt or a bare `CFF ` table; `index` selects the face of a
/// collection.
pub fn cff_charset_gid_to_cid_map(font_bytes: &[u8], index: usize) -> Option<BTreeMap<u16, u16>> {
    use allsorts::{
        binary::read::ReadScope, cff::CFF, font_data::FontData, tables::FontTableProvider, tag,
    };

    // Accept both a whole sfnt (take its `CFF ` table) and a bare `CFF ` table, which is
    // what is embedded for a CID-keyed CFF and read back on parse.
    let cff_data: Vec<u8> = (|| -> Option<Vec<u8>> {
        let font_file = ReadScope::new(font_bytes).read::<FontData<'_>>().ok()?;
        let provider = font_file.table_provider(index).ok()?;
        Some(provider.read_table_data(tag::CFF).ok()?.into_owned())
    })()
    .unwrap_or_else(|| font_bytes.to_vec());
    let cff = ReadScope::new(&cff_data).read::<CFF<'_>>().ok()?;
    let font = cff.fonts.first()?;
    if !font.is_cid_keyed() {
        return None;
    }
    let num_glyphs = font.char_strings_index.len() as u16;
    Some(
        (0..num_glyphs)
            .filter_map(|gid| font.charset.id_for_glyph(gid).map(|cid| (gid, cid)))
            .collect(),
    )
}

/// The bare `CFF ` table of an `OTTO` font, for embedding as `/FontFile3` with
/// `/Subtype /CIDFontType0C`; `None` unless that CFF is CID-keyed (TrueType faces and
/// name-keyed CFF fonts keep their usual wrappers, where code and glyph id agree in every
/// viewer).
///
/// The wrapper decides how FreeType-based viewers resolve the charset CIDs that are written
/// to the content stream (see [`cff_charset_gid_to_cid_map`]). For a whole `OTTO` font,
/// PDFium and poppler use the codes as glyph indices and never read the charset, while
/// Acrobat and Preview resolve them through it, so no code assignment renders the same in
/// both. For a bare CID-keyed CFF all viewers resolve through the charset.
///
/// The returned bytes are the table [`cff_charset_gid_to_cid_map`] reads the charset from,
/// so the codes stay consistent with the embedded program.
pub fn extract_cid_keyed_cff(font_bytes: &[u8], index: usize) -> Option<Vec<u8>> {
    use allsorts::{
        binary::read::ReadScope, cff::CFF, font_data::FontData, tables::FontTableProvider, tag,
    };

    if !font_bytes.starts_with(b"OTTO") {
        return None;
    }
    let font_file = ReadScope::new(font_bytes).read::<FontData<'_>>().ok()?;
    let provider = font_file.table_provider(index).ok()?;
    let cff_data = provider.read_table_data(tag::CFF).ok()?;
    let cff = ReadScope::new(&cff_data).read::<CFF<'_>>().ok()?;
    if !cff.fonts.first()?.is_cid_keyed() {
        return None;
    }
    Some(cff_data.into_owned())
}

/// Wraps a bare `CFF ` table (as embedded by [`extract_cid_keyed_cff`]) in a minimal `OTTO`
/// font file, so it can be parsed: azul's parser needs the `head`, `hhea`, `maxp` and `hmtx`
/// tables a bare CFF does not have. Used only to rebuild an in-memory face when reading back
/// such an embedding; the bytes written to a PDF stay bare, because a wrapper would hide the
/// CID-keyed CFF from FreeType.
///
/// `None` if `bytes` already looks like an sfnt or does not parse as CFF.
#[cfg(feature = "text_layout")]
pub(crate) fn wrap_bare_cff_as_sfnt(bytes: &[u8]) -> Option<Vec<u8>> {
    use allsorts::{binary::read::ReadScope, cff::CFF};

    let looks_like_sfnt = matches!(bytes.get(..4), Some(magic) if magic == b"OTTO"
        || magic == [0, 1, 0, 0]
        || magic == *b"true"
        || magic == *b"typ1"
        || magic == *b"ttcf"
        || magic == *b"wOFF"
        || magic == *b"wOF2");
    if looks_like_sfnt {
        return None;
    }

    let cff = ReadScope::new(bytes).read::<CFF<'_>>().ok()?;
    let font = cff.fonts.first()?;
    let num_glyphs = u16::try_from(font.char_strings_index.len()).ok()?;
    let widths: Vec<u16> = (0..num_glyphs)
        .map(|gid| cff_charstring_width(&cff, font, gid))
        .collect();
    let units_per_em = cff_units_per_em(font);

    let unicode_to_gid = cff_name_cmap(&cff, font, num_glyphs);

    Some(build_minimal_sfnt(bytes, num_glyphs, &widths, units_per_em, &unicode_to_gid))
}

/// Character to glyph id (Basic Multilingual Plane only) for a name-keyed CFF font, from its glyph
/// names through the Adobe Glyph List. Empty for a CID-keyed font, whose glyphs have no names.
/// A bare CFF has no `cmap`; this is the one [`wrap_bare_cff_as_sfnt`] writes.
#[cfg(feature = "text_layout")]
fn cff_name_cmap(
    cff: &allsorts::cff::CFF<'_>,
    font: &allsorts::cff::Font<'_>,
    num_glyphs: u16,
) -> std::collections::BTreeMap<u16, u16> {
    let mut map = std::collections::BTreeMap::new();
    if matches!(font.data, allsorts::cff::CFFVariant::CID(_)) {
        return map;
    }
    for gid in 1..num_glyphs {
        let Some(sid) = font.charset.id_for_glyph(gid) else {
            continue;
        };
        let Ok(name) = cff.read_string(sid) else {
            continue;
        };
        let Some(text) = crate::encoding::glyph_name_to_string(name) else {
            continue;
        };
        let mut chars = text.chars();
        if let (Some(c), None) = (chars.next(), chars.next()) {
            if let Ok(unicode) = u16::try_from(u32::from(c)) {
                map.entry(unicode).or_insert(gid);
            }
        }
    }
    map
}

/// The glyph of each one-byte code of a simple font whose program has no Unicode `cmap` subtable,
/// read from its symbol (3,0) or Macintosh (1,0) subtable, for the codes 0 to 255 (glyph 0 where
/// the font has none).
///
/// The codes of such a font are the keys of its own glyph map, and a lookup by character finds
/// the wrong glyph or none (a font that Quartz subsets has codes of its own). `None` for a
/// program with a Unicode subtable, which maps characters, and for bytes that have no `cmap` that
/// can be read.
#[cfg(feature = "text_layout")]
pub(crate) fn code_to_glyph_without_unicode(
    font_bytes: &[u8],
    font_index: usize,
) -> Option<std::collections::BTreeMap<u16, u16>> {
    use allsorts::{
        binary::read::ReadScope,
        font::{find_good_cmap_subtable, Encoding},
        font_data::FontData,
        tables::{
            cmap::{Cmap, CmapSubtable},
            FontTableProvider,
        },
        tag,
    };

    let font_file = ReadScope::new(font_bytes).read::<FontData<'_>>().ok()?;
    let provider = font_file.table_provider(font_index).ok()?;
    let data = provider.read_table_data(tag::CMAP).ok()?;
    let cmap = ReadScope::new(&data).read::<Cmap<'_>>().ok()?;
    let (encoding, record) = find_good_cmap_subtable(&cmap)?;
    if !matches!(encoding, Encoding::Symbol | Encoding::AppleRoman) {
        return None;
    }
    let subtable = cmap
        .scope
        .offset(usize::try_from(record.offset).ok()?)
        .read::<CmapSubtable<'_>>()
        .ok()?;
    let mut glyphs = std::collections::BTreeMap::new();
    for code in 0u16..=255 {
        let mut glyph = subtable.map_glyph(u32::from(code)).ok().flatten();
        if glyph.is_none() && encoding == Encoding::Symbol {
            // A symbol font puts its codes at 0xF000 (ISO 32000-1, 9.6.6.4).
            glyph = subtable.map_glyph(0xF000 + u32::from(code)).ok().flatten();
        }
        glyphs.insert(code, glyph.unwrap_or(0));
    }
    Some(glyphs)
}

/// The glyph name that the built-in encoding of a bare name-keyed CFF font gives to each code,
/// for the codes where the font's own encoding is a custom one. Empty for the standard encoding
/// (which [`crate::encoding::BaseEncoding::Standard`] describes), for a CID-keyed font and for
/// bytes that are not a bare CFF.
#[cfg(feature = "text_layout")]
pub(crate) fn cff_builtin_encoding(bytes: &[u8]) -> std::collections::BTreeMap<u8, String> {
    use allsorts::binary::read::ReadScope;
    use allsorts::cff::{CFFVariant, CustomEncoding, Encoding, CFF};

    let mut names = std::collections::BTreeMap::new();
    let Ok(cff) = ReadScope::new(bytes).read::<CFF<'_>>() else {
        return names;
    };
    let Some(font) = cff.fonts.first() else {
        return names;
    };
    let CFFVariant::Type1(t1) = &font.data else {
        return names;
    };
    let Encoding::Custom(custom) = &t1.encoding else {
        return names;
    };
    let mut assign = |code: u8, gid: u16| {
        if let Some(sid) = font.charset.id_for_glyph(gid) {
            if let Ok(name) = cff.read_string(sid) {
                names.insert(code, name.to_string());
            }
        }
    };
    match custom {
        CustomEncoding::Format0 { codes } => {
            for (index, code) in codes.iter().enumerate() {
                if let Ok(gid) = u16::try_from(index + 1) {
                    assign(code, gid);
                }
            }
        }
        CustomEncoding::Format1 { ranges } => {
            let mut gid: u16 = 1;
            for range in ranges.iter() {
                for step in 0..=range.n_left {
                    if let Some(code) = range.first.checked_add(step) {
                        assign(code, gid);
                    }
                    gid = gid.saturating_add(1);
                }
            }
        }
    }
    names
}

/// Units per em implied by the CFF top dict `FontMatrix`, `round(1/m[0])`; 1000 for the
/// default `0.001` matrix. The synthesized `head`, `hhea` and `hmtx` must use the same space
/// as the charstring coordinates and the widths scanned from them. Clamped to the legal
/// 16 to 16384 range of `head`.
#[cfg(feature = "text_layout")]
fn cff_units_per_em(font: &allsorts::cff::Font<'_>) -> u16 {
    use allsorts::cff::{Operand, Operator};

    let scale = font
        .top_dict
        .get_with_default(Operator::FontMatrix)
        .and_then(|ops| ops.first())
        .and_then(|op| match op {
            Operand::Integer(i) => Some(f64::from(*i)),
            Operand::Offset(i) => Some(f64::from(*i)),
            Operand::Real(r) => f64::try_from(r).ok(),
        });
    match scale {
        Some(m0) if m0.is_finite() && m0 > 0.0 => (1.0 / m0).round().clamp(16.0, 16384.0) as u16,
        _ => 1000,
    }
}

/// A glyph's advance width read from its Type2 charstring: `defaultWidthX` or
/// `nominalWidthX` plus the optional leading width delta before the first stack-clearing
/// operator (Adobe TN5177, 16). Used by [`wrap_bare_cff_as_sfnt`] to synthesize an `hmtx`,
/// since a bare CFF has none.
#[cfg(feature = "text_layout")]
fn cff_charstring_width(
    cff: &allsorts::cff::CFF<'_>,
    font: &allsorts::cff::Font<'_>,
    gid: u16,
) -> u16 {
    use allsorts::cff::{CFFVariant, Operator};

    let charstring = font.char_strings_index.read_object(usize::from(gid));

    let (nominal_width, default_width, local_subrs) = match &font.data {
        CFFVariant::CID(cid) => {
            let fd = cid.fd_select.font_dict_index(gid).unwrap_or(0) as usize;
            let pd = cid.private_dicts.get(fd);
            (
                pd.and_then(|d| d.get_i32(Operator::NominalWidthX))
                    .and_then(Result::ok)
                    .unwrap_or(0),
                pd.and_then(|d| d.get_i32(Operator::DefaultWidthX))
                    .and_then(Result::ok)
                    .unwrap_or(0),
                cid.local_subr_indices.get(fd).and_then(|s| s.as_ref()),
            )
        }
        CFFVariant::Type1(t1) => (
            t1.private_dict
                .get_i32(Operator::NominalWidthX)
                .and_then(Result::ok)
                .unwrap_or(0),
            t1.private_dict
                .get_i32(Operator::DefaultWidthX)
                .and_then(Result::ok)
                .unwrap_or(0),
            t1.local_subr_index.as_ref(),
        ),
    };

    let Some(charstring) = charstring else {
        return default_width.max(0) as u16;
    };

    let mut stack: Vec<f64> = Vec::new();
    let mut steps = CFF_WIDTH_SCAN_STEPS;
    let width = match cff_charstring_width_delta(
        charstring,
        local_subrs,
        &cff.global_subr_index,
        &mut stack,
        &mut steps,
        0,
    ) {
        WidthScan::Decided(Some(delta)) => nominal_width as f64 + delta,
        WidthScan::Decided(None) | WidthScan::Returned => default_width as f64,
    };
    width.round().clamp(0.0, u16::MAX as f64) as u16
}

/// The standard CFF local/global subroutine index bias (Adobe TN5177 §16).
#[cfg(feature = "text_layout")]
fn cff_subr_bias(count: usize) -> i32 {
    if count < 1240 {
        107
    } else if count < 33900 {
        1131
    } else {
        32768
    }
}

/// Where [`cff_charstring_width_delta`] stopped in one charstring or subroutine.
#[cfg(feature = "text_layout")]
enum WidthScan {
    /// At the first stack-clearing operator: the leading width delta, if the operand count
    /// proves one is there. `Decided(None)` also covers charstrings the scan cannot follow;
    /// both mean `defaultWidthX`.
    Decided(Option<f64>),
    /// A subroutine ended (`return`, or ran out of bytes) before any stack-clearing operator,
    /// so the caller keeps scanning after its call.
    ///
    /// Kept apart from `Decided(None)`: a subroutine that reached the first `hstemhm` with an
    /// even operand count has decided "no width", and scanning on in the caller would read the
    /// next hint group's operand count instead.
    Returned,
}

/// The most operators and operands the width scan of one glyph reads, over the charstring and
/// every subroutine it calls. Subroutines may call other subroutines several times each, so
/// the scan is a tree of calls that the length of the font does not bound.
#[cfg(feature = "text_layout")]
const CFF_WIDTH_SCAN_STEPS: u32 = 10_000;

/// `Decided` with the first operand as the width when `present`, else without a width. The
/// operand counts that say a width is there all leave at least one operand on the stack.
#[cfg(feature = "text_layout")]
fn leading_width(stack: &[f64], present: bool) -> WidthScan {
    WidthScan::Decided(if present { stack.first().copied() } else { None })
}

/// Scan a Type2 charstring (following `callsubr`/`callgsubr`, depth-limited
/// like allsorts' own interpreter) up to its first stack-clearing operator,
/// returning the leading width delta if the operand count proves one is
/// present. `stack` is threaded through subroutine calls so operands a
/// subroutine leaves behind are visible to its caller, matching how a real
/// Type2 interpreter treats `callsubr`/`callgsubr` as inlining. `steps` is what
/// is left of [`CFF_WIDTH_SCAN_STEPS`]; the scan is `Decided(None)` when it runs
/// out.
#[cfg(feature = "text_layout")]
fn cff_charstring_width_delta(
    charstring: &[u8],
    local_subrs: Option<&allsorts::cff::MaybeOwnedIndex<'_>>,
    global_subrs: &allsorts::cff::MaybeOwnedIndex<'_>,
    stack: &mut Vec<f64>,
    steps: &mut u32,
    depth: u8,
) -> WidthScan {
    use WidthScan::{Decided, Returned};

    if depth > 10 {
        return Decided(None);
    }
    let mut bytes = charstring.iter().copied();
    while let Some(b0) = bytes.next() {
        let Some(left) = steps.checked_sub(1) else {
            return Decided(None);
        };
        *steps = left;
        match b0 {
            1 | 3 | 18 | 19 | 20 | 23 => {
                // hstem/vstem/hstemhm/hintmask/cntrmask/vstemhm: width present iff
                // the operand count is odd.
                return leading_width(stack, stack.len() % 2 == 1);
            }
            21 => return leading_width(stack, stack.len() > 2), // rmoveto: 2 args
            22 | 4 => return leading_width(stack, stack.len() > 1), // h/vmoveto: 1 arg
            14 => return leading_width(stack, stack.len() == 1 || stack.len() == 5), // endchar
            10 | 29 => {
                // callsubr / callgsubr
                let Some(idx) = stack.pop() else {
                    return Decided(None);
                };
                let (subrs, bias) = if b0 == 10 {
                    let Some(subrs) = local_subrs else {
                        return Decided(None);
                    };
                    (subrs, cff_subr_bias(subrs.len()))
                } else {
                    (global_subrs, cff_subr_bias(global_subrs.len()))
                };
                // `idx` is any number the charstring computes.
                let Some(subr_idx) = (idx as i32).checked_add(bias) else {
                    return Decided(None);
                };
                let Some(sub_bytes) = usize::try_from(subr_idx)
                    .ok()
                    .and_then(|si| subrs.read_object(si))
                else {
                    return Decided(None);
                };
                match cff_charstring_width_delta(
                    sub_bytes,
                    local_subrs,
                    global_subrs,
                    stack,
                    steps,
                    depth.saturating_add(1),
                ) {
                    Returned => {}
                    decided => return decided,
                }
            }
            11 => return Returned, // return: no stack-clearing op seen in this subroutine
            12 => {
                // Two-byte escape operators. The arithmetic ones may legally appear before the
                // first stack-clearing operator and change the operand count, so they are
                // replayed. Stateful, random and path operators make the width undecidable
                // from a static scan, which falls back to defaultWidthX.
                let Some(op) = bytes.next() else {
                    return Decided(None);
                };
                if !cff_apply_escape_op(op, stack) {
                    return Decided(None);
                }
            }
            28 => {
                let (Some(hi), Some(lo)) = (bytes.next(), bytes.next()) else {
                    return Decided(None);
                };
                stack.push(f64::from(i16::from_be_bytes([hi, lo])));
            }
            32..=246 => stack.push(f64::from(b0) - 139.0),
            247..=250 => {
                let Some(b1) = bytes.next() else {
                    return Decided(None);
                };
                stack.push((f64::from(b0) - 247.0) * 256.0 + f64::from(b1) + 108.0);
            }
            251..=254 => {
                let Some(b1) = bytes.next() else {
                    return Decided(None);
                };
                stack.push(-(f64::from(b0) - 251.0) * 256.0 - f64::from(b1) - 108.0);
            }
            255 => {
                let (Some(b1), Some(b2), Some(b3), Some(b4)) =
                    (bytes.next(), bytes.next(), bytes.next(), bytes.next())
                else {
                    return Decided(None);
                };
                let bits = i32::from_be_bytes([b1, b2, b3, b4]);
                stack.push(f64::from(bits) / 65536.0);
            }
            _ => return Decided(None),
        }
    }
    Returned
}

/// Replay one two-byte (`12 x`) Type2 operator's stack effect for the width
/// scanner. `true` = effect applied; `false` = the width is statically
/// undecidable past this operator (unknown/stateful/path op, stack underflow,
/// or a domain error like division by zero) and the caller must fall back to
/// `defaultWidthX`.
#[cfg(feature = "text_layout")]
fn cff_apply_escape_op(op: u8, stack: &mut Vec<f64>) -> bool {
    fn un(stack: &mut Vec<f64>, f: impl Fn(f64) -> Option<f64>) -> bool {
        let Some(a) = stack.pop() else { return false };
        match f(a) {
            Some(v) => {
                stack.push(v);
                true
            }
            None => false,
        }
    }
    fn bin(stack: &mut Vec<f64>, f: impl Fn(f64, f64) -> Option<f64>) -> bool {
        let Some(b) = stack.pop() else { return false };
        let Some(a) = stack.pop() else { return false };
        match f(a, b) {
            Some(v) => {
                stack.push(v);
                true
            }
            None => false,
        }
    }
    fn truth(v: bool) -> Option<f64> {
        Some(if v { 1.0 } else { 0.0 })
    }

    match op {
        3 => bin(stack, |a, b| truth(a != 0.0 && b != 0.0)), // and
        4 => bin(stack, |a, b| truth(a != 0.0 || b != 0.0)), // or
        5 => un(stack, |a| truth(a == 0.0)),                 // not
        9 => un(stack, |a| Some(a.abs())),                   // abs
        10 => bin(stack, |a, b| Some(a + b)),                // add
        11 => bin(stack, |a, b| Some(a - b)),                // sub
        12 => bin(stack, |a, b| (b != 0.0).then(|| a / b)),  // div
        14 => un(stack, |a| Some(-a)),                       // neg
        15 => bin(stack, |a, b| truth(a == b)),              // eq
        18 => stack.pop().is_some(),                         // drop
        22 => {
            // ifelse: s1 s2 v1 v2 -> (v1 <= v2 ? s1 : s2)
            let (Some(v2), Some(v1), Some(s2), Some(s1)) =
                (stack.pop(), stack.pop(), stack.pop(), stack.pop())
            else {
                return false;
            };
            stack.push(if v1 <= v2 { s1 } else { s2 });
            true
        }
        24 => bin(stack, |a, b| Some(a * b)), // mul
        26 => un(stack, |a| (a >= 0.0).then(|| a.sqrt())), // sqrt
        27 => match stack.last().copied() {
            // dup
            Some(a) => {
                stack.push(a);
                true
            }
            None => false,
        },
        28 => {
            // exch
            let [.., below, top] = stack.as_mut_slice() else {
                return false;
            };
            std::mem::swap(below, top);
            true
        }
        29 => {
            // index: n < 0 duplicates the top element (Adobe TN5177 §4.4)
            let Some(n) = stack.pop() else { return false };
            let idx = if n < 0.0 {
                stack.len().checked_sub(1)
            } else {
                // `n` is any number the charstring computes, so `1 + n` can overflow.
                1usize
                    .checked_add(n as usize)
                    .and_then(|from_top| stack.len().checked_sub(from_top))
            };
            match idx.and_then(|i| stack.get(i)).copied() {
                Some(v) => {
                    stack.push(v);
                    true
                }
                None => false,
            }
        }
        30 => {
            // roll: rotate the top n elements by j (positive j -> toward the top)
            let (Some(j), Some(n)) = (stack.pop(), stack.pop()) else {
                return false;
            };
            if n < 0.0 || n as usize > stack.len() {
                return false;
            }
            let n = n as usize;
            if n > 0 {
                let Some(top) = stack
                    .len()
                    .checked_sub(n)
                    .and_then(|start| stack.get_mut(start..))
                else {
                    return false;
                };
                // `shift` is below `n`, the length of `top`.
                let shift = (j as i64).rem_euclid(n as i64) as usize;
                top.rotate_right(shift);
            }
            true
        }
        // put(20)/get(21) touch the transient array, random(23) is
        // non-deterministic, 34..=37 are flex path ops (illegal before the
        // first moveto anyway), everything else is unknown.
        _ => false,
    }
}

/// Assembles a minimal `OTTO` font around a bare `CFF ` table, synthesizing the `head`,
/// `hhea`, `maxp`, `hmtx` and `cmap` tables it lacks (see [`wrap_bare_cff_as_sfnt`]).
#[cfg(feature = "text_layout")]
fn build_minimal_sfnt(
    cff_bytes: &[u8],
    num_glyphs: u16,
    widths: &[u16],
    units_per_em: u16,
    unicode_to_gid: &std::collections::BTreeMap<u16, u16>,
) -> Vec<u8> {
    let mut head = Vec::with_capacity(54);
    head.extend_from_slice(&1u16.to_be_bytes()); // majorVersion
    head.extend_from_slice(&0u16.to_be_bytes()); // minorVersion
    head.extend_from_slice(&0x0001_0000u32.to_be_bytes()); // fontRevision
    head.extend_from_slice(&0u32.to_be_bytes()); // checkSumAdjustment
    head.extend_from_slice(&0x5F0F_3CF5u32.to_be_bytes()); // magicNumber
    head.extend_from_slice(&0u16.to_be_bytes()); // flags
    head.extend_from_slice(&units_per_em.to_be_bytes());
    head.extend_from_slice(&0i64.to_be_bytes()); // created
    head.extend_from_slice(&0i64.to_be_bytes()); // modified
    head.extend_from_slice(&0i16.to_be_bytes()); // xMin
    head.extend_from_slice(&0i16.to_be_bytes()); // yMin
    head.extend_from_slice(&0i16.to_be_bytes()); // xMax
    head.extend_from_slice(&0i16.to_be_bytes()); // yMax
    head.extend_from_slice(&0u16.to_be_bytes()); // macStyle
    head.extend_from_slice(&0u16.to_be_bytes()); // lowestRecPPEM
    head.extend_from_slice(&2i16.to_be_bytes()); // fontDirectionHint
    head.extend_from_slice(&0i16.to_be_bytes()); // indexToLocFormat
    head.extend_from_slice(&0i16.to_be_bytes()); // glyphDataFormat

    let advance_width_max = widths.iter().copied().max().unwrap_or(0);
    // Computed in an `i32`: `units_per_em` is up to 16384, and its multiples overflow an `i16`.
    // The results are 80% and 20% of it, which fit.
    let em = i32::from(units_per_em);
    let mut hhea = Vec::with_capacity(36);
    hhea.extend_from_slice(&1u16.to_be_bytes()); // majorVersion
    hhea.extend_from_slice(&0u16.to_be_bytes()); // minorVersion
    hhea.extend_from_slice(&((em * 8 / 10) as i16).to_be_bytes()); // ascender
    hhea.extend_from_slice(&((-em * 2 / 10) as i16).to_be_bytes()); // descender
    hhea.extend_from_slice(&0i16.to_be_bytes()); // lineGap
    hhea.extend_from_slice(&advance_width_max.to_be_bytes());
    hhea.extend_from_slice(&0i16.to_be_bytes()); // minLeftSideBearing
    hhea.extend_from_slice(&0i16.to_be_bytes()); // minRightSideBearing
    hhea.extend_from_slice(&0i16.to_be_bytes()); // xMaxExtent
    hhea.extend_from_slice(&1i16.to_be_bytes()); // caretSlopeRise
    hhea.extend_from_slice(&0i16.to_be_bytes()); // caretSlopeRun
    hhea.extend_from_slice(&0i16.to_be_bytes()); // caretOffset
    hhea.extend_from_slice(&0i16.to_be_bytes()); // reserved x4
    hhea.extend_from_slice(&0i16.to_be_bytes());
    hhea.extend_from_slice(&0i16.to_be_bytes());
    hhea.extend_from_slice(&0i16.to_be_bytes());
    hhea.extend_from_slice(&0i16.to_be_bytes()); // metricDataFormat
    hhea.extend_from_slice(&num_glyphs.to_be_bytes()); // numberOfHMetrics

    let mut maxp = Vec::with_capacity(6);
    maxp.extend_from_slice(&0x0000_5000u32.to_be_bytes()); // version 0.5 (CFF)
    maxp.extend_from_slice(&num_glyphs.to_be_bytes());

    let mut hmtx = Vec::with_capacity(widths.len() * 4);
    for &w in widths {
        hmtx.extend_from_slice(&w.to_be_bytes());
        hmtx.extend_from_slice(&0i16.to_be_bytes()); // lsb
    }

    // allsorts requires a `cmap` with a recognised Windows/Unicode subtable. A bare CFF has no
    // character mapping of its own; the one written here comes from the glyph names. Without
    // any, it is an empty format 6 table: every lookup returns `None`, where a zero-filled
    // format 0 table would return a bogus `Some(0)`.
    let cmap_subtable = if unicode_to_gid.is_empty() {
        let mut t = Vec::with_capacity(10);
        t.extend_from_slice(&6u16.to_be_bytes()); // format 6
        t.extend_from_slice(&10u16.to_be_bytes()); // length
        t.extend_from_slice(&0u16.to_be_bytes()); // language
        t.extend_from_slice(&0u16.to_be_bytes()); // firstCode
        t.extend_from_slice(&0u16.to_be_bytes()); // entryCount
        t
    } else {
        cmap_format4(unicode_to_gid)
    };

    let mut cmap = Vec::with_capacity(12 + cmap_subtable.len());
    cmap.extend_from_slice(&0u16.to_be_bytes()); // version
    cmap.extend_from_slice(&1u16.to_be_bytes()); // numTables
    cmap.extend_from_slice(&3u16.to_be_bytes()); // platformID: Windows
    cmap.extend_from_slice(&1u16.to_be_bytes()); // encodingID: Unicode BMP
    cmap.extend_from_slice(&12u32.to_be_bytes()); // subtable offset
    cmap.extend_from_slice(&cmap_subtable);

    build_sfnt(&[
        (*b"CFF ", cff_bytes),
        (*b"cmap", &cmap),
        (*b"head", &head),
        (*b"hhea", &hhea),
        (*b"hmtx", &hmtx),
        (*b"maxp", &maxp),
    ])
}

/// A `cmap` subtable of format 4 for the BMP characters in `map`, one segment for each run of
/// consecutive characters with consecutive glyph ids.
#[cfg(feature = "text_layout")]
fn cmap_format4(map: &std::collections::BTreeMap<u16, u16>) -> Vec<u8> {
    // (start, end, delta) of each segment
    let mut segments: Vec<(u16, u16, u16)> = Vec::new();
    for (&code, &gid) in map {
        let delta = gid.wrapping_sub(code);
        match segments.last_mut() {
            Some((_, end, d)) if end.checked_add(1) == Some(code) && *d == delta => *end = code,
            _ => segments.push((code, code, delta)),
        }
    }
    // The subtable length is a `u16`: 8000 segments are 64016 bytes.
    segments.truncate(8000);
    if segments.last().map(|s| s.1) != Some(0xFFFF) {
        segments.push((0xFFFF, 0xFFFF, 1));
    }
    let seg_count = segments.len() as u16;
    let mut entry_selector = 0u16;
    while 1u32 << (entry_selector + 1) <= u32::from(seg_count) {
        entry_selector += 1;
    }
    let search_range = 2 * (1u16 << entry_selector);
    let length = 16 + 8 * usize::from(seg_count);

    let mut t = Vec::with_capacity(length);
    t.extend_from_slice(&4u16.to_be_bytes()); // format
    t.extend_from_slice(&(length as u16).to_be_bytes());
    t.extend_from_slice(&0u16.to_be_bytes()); // language
    t.extend_from_slice(&(seg_count * 2).to_be_bytes());
    t.extend_from_slice(&search_range.to_be_bytes());
    t.extend_from_slice(&entry_selector.to_be_bytes());
    t.extend_from_slice(&(seg_count * 2 - search_range).to_be_bytes());
    for s in &segments {
        t.extend_from_slice(&s.1.to_be_bytes()); // endCode
    }
    t.extend_from_slice(&0u16.to_be_bytes()); // reservedPad
    for s in &segments {
        t.extend_from_slice(&s.0.to_be_bytes()); // startCode
    }
    for s in &segments {
        t.extend_from_slice(&s.2.to_be_bytes()); // idDelta
    }
    for _ in &segments {
        t.extend_from_slice(&0u16.to_be_bytes()); // idRangeOffset
    }
    t
}

/// Assembles an `OTTO` font from `(tag, data)` tables given in ascending tag order, as the
/// table directory requires, and computes each table's checksum.
#[cfg(feature = "text_layout")]
fn build_sfnt(tables: &[([u8; 4], &[u8])]) -> Vec<u8> {
    fn checksum(data: &[u8]) -> u32 {
        let mut sum: u32 = 0;
        for chunk in data.chunks(4) {
            // The last chunk may be shorter than four bytes; it is padded with zeros.
            let mut word = [0u8; 4];
            for (slot, byte) in word.iter_mut().zip(chunk) {
                *slot = *byte;
            }
            sum = sum.wrapping_add(u32::from_be_bytes(word));
        }
        sum
    }
    fn padded_len(len: usize) -> usize {
        (len + 3) & !3
    }

    let num_tables = tables.len() as u16;
    let mut search_range_pow2 = 1u16;
    let mut entry_selector = 0u16;
    while search_range_pow2 * 2 <= num_tables {
        search_range_pow2 *= 2;
        entry_selector += 1;
    }
    let search_range = search_range_pow2 * 16;
    let range_shift = num_tables * 16 - search_range;

    let mut offset = 12 + 16 * tables.len();
    let mut directory = Vec::with_capacity(16 * tables.len());
    let mut data_section = Vec::new();
    for (tag, data) in tables {
        directory.extend_from_slice(tag);
        directory.extend_from_slice(&checksum(data).to_be_bytes());
        directory.extend_from_slice(&(offset as u32).to_be_bytes());
        directory.extend_from_slice(&(data.len() as u32).to_be_bytes());

        data_section.extend_from_slice(data);
        data_section.resize(data_section.len() + (padded_len(data.len()) - data.len()), 0);
        offset += padded_len(data.len());
    }

    let mut out = Vec::with_capacity(offset);
    out.extend_from_slice(b"OTTO");
    out.extend_from_slice(&num_tables.to_be_bytes());
    out.extend_from_slice(&search_range.to_be_bytes());
    out.extend_from_slice(&entry_selector.to_be_bytes());
    out.extend_from_slice(&range_shift.to_be_bytes());
    out.extend_from_slice(&directory);
    out.extend_from_slice(&data_section);
    out
}

#[cfg(feature = "text_layout")]
fn get_glyph_width(font: &ParsedFont, gid: u16) -> Option<u16> {
    font.get_or_decode_glyph(gid).map(|g| g.horz_advance)
}

/// The factor from font units to the 1/1000 em of `/W`: 1000 divided by the units per em, or 1
/// for a font that has none.
#[cfg(feature = "text_layout")]
fn width_scale(font: &ParsedFont) -> f32 {
    match font.pdf_font_metrics.units_per_em {
        0 => 1.0,
        units_per_em => 1000.0 / f32::from(units_per_em),
    }
}

/// Builds the `/W` array of a CID font where the code is the glyph id.
///
/// `glyph_ids` must be sorted ascending by glyph id; the text is not used. Widths are in
/// 1/1000 em. See [`get_normalized_widths_codes`] for the format and for fonts where the code
/// differs from the glyph id.
///
/// Requires the `text_layout` feature.
#[cfg(feature = "text_layout")]
pub fn get_normalized_widths_ttf(font: &ParsedFont, glyph_ids: &[(u16, String)]) -> Vec<lopdf::Object> {
    let mut widths_list = Vec::new();
    let mut current_low_gid = 0u16;
    let mut current_high_gid = 0u16;
    let mut current_width_vec = Vec::new();

    let percentage_font_scaling = width_scale(font);

    for (gid, _) in glyph_ids {
        let glyph_width = get_glyph_width(font, *gid)
            .map(|w| (w as f32 * percentage_font_scaling) as i64)
            .unwrap_or(0);

        if current_width_vec.is_empty() {
            current_low_gid = *gid;
            current_high_gid = *gid;
            current_width_vec.push(glyph_width);
        } else if current_high_gid.checked_add(1) == Some(*gid) {
            current_high_gid = *gid;
            current_width_vec.push(glyph_width);
        } else {
            widths_list.push(lopdf::Object::Integer(current_low_gid as i64));
            widths_list.push(lopdf::Object::Array(
                current_width_vec.iter().map(|w| lopdf::Object::Integer(*w)).collect(),
            ));
            current_low_gid = *gid;
            current_high_gid = *gid;
            current_width_vec = vec![glyph_width];
        }
    }

    if !current_width_vec.is_empty() {
        widths_list.push(lopdf::Object::Integer(current_low_gid as i64));
        widths_list.push(lopdf::Object::Array(
            current_width_vec.iter().map(|w| lopdf::Object::Integer(*w)).collect(),
        ));
    }

    widths_list
}

/// Builds the `/W` array of a CID font from `(code, gid)` pairs: the code is what the content
/// stream shows, the glyph id selects the glyph whose width is written for it.
///
/// Widths are the glyph's advance scaled to 1/1000 em and truncated; a glyph whose width
/// cannot be read gets 0. Runs of consecutive codes are written as `c [w1 w2 ...]`, so
/// `entries` must be sorted ascending by code. Under `Identity-H` the code is the CID: the
/// glyph id for TrueType and name-keyed CFF fonts, the charset CID for CID-keyed CFF fonts
/// (see [`cff_charset_gid_to_cid_map`]).
///
/// Requires the `text_layout` feature.
#[cfg(feature = "text_layout")]
pub fn get_normalized_widths_codes(
    font: &ParsedFont,
    entries: &[(u16, u16)],
) -> Vec<lopdf::Object> {
    let percentage_font_scaling = width_scale(font);

    let mut widths_list = Vec::new();
    let mut current_low_code = 0u16;
    let mut current_high_code = 0u16;
    let mut current_width_vec: Vec<i64> = Vec::new();

    for &(code, gid) in entries {
        let glyph_width = get_glyph_width(font, gid)
            .map(|w| (w as f32 * percentage_font_scaling) as i64)
            .unwrap_or(0);

        if current_width_vec.is_empty() {
            current_low_code = code;
            current_high_code = code;
            current_width_vec.push(glyph_width);
        } else if current_high_code.checked_add(1) == Some(code) {
            current_high_code = code;
            current_width_vec.push(glyph_width);
        } else {
            widths_list.push(lopdf::Object::Integer(current_low_code as i64));
            widths_list.push(lopdf::Object::Array(
                current_width_vec.iter().map(|w| lopdf::Object::Integer(*w)).collect(),
            ));
            current_low_code = code;
            current_high_code = code;
            current_width_vec = vec![glyph_width];
        }
    }

    if !current_width_vec.is_empty() {
        widths_list.push(lopdf::Object::Integer(current_low_code as i64));
        widths_list.push(lopdf::Object::Array(
            current_width_vec.iter().map(|w| lopdf::Object::Integer(*w)).collect(),
        ));
    }

    widths_list
}

/// Prefix of the `data:` URI a font is serialized as; the base64 of its source bytes follows.
pub const FONT_B64_START: &str = "data:font/ttf;base64,";

#[cfg(all(test, feature = "text_layout"))]
mod test {
    use std::collections::BTreeMap;

    use crate::*;

    pub const WIN_1252: &[char; 214] = &[
        '!', '"', '#', '$', '%', '&', '\'', '(', ')', '*', '+', ',', '-', '.', '/', '0', '1', '2',
        '3', '4', '5', '6', '7', '8', '9', ':', ';', '<', '=', '>', '?', '@', 'A', 'B', 'C', 'D',
        'E', 'F', 'G', 'H', 'I', 'J', 'K', 'L', 'M', 'N', 'O', 'P', 'Q', 'R', 'S', 'T', 'U', 'V',
        'W', 'X', 'Y', 'Z', '[', '\\', ']', '^', '_', '`', 'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h',
        'i', 'j', 'k', 'l', 'm', 'n', 'o', 'p', 'q', 'r', 's', 't', 'u', 'v', 'w', 'x', 'y', 'z',
        '{', '|', '}', '~', '€', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', 'Ž', '‘',
        '’', '“', '•', '–', '—', '˜', '™', 'š', '›', 'œ', 'ž', 'Ÿ', '¡', '¢', '£', '¤', '¥', '¦',
        '§', '¨', '©', 'ª', '«', '¬', '®', '¯', '°', '±', '²', '³', '´', 'µ', '¶', '·', '¸', '¹',
        'º', '»', '¼', '½', '¾', '¿', 'À', 'Á', 'Â', 'Ã', 'Ä', 'Å', 'Æ', 'Ç', 'È', 'É', 'Ê', 'Ë',
        'Ì', 'Í', 'Î', 'Ï', 'Ð', 'Ñ', 'Ò', 'Ó', 'Ô', 'Õ', 'Ö', '×', 'Ø', 'Ù', 'Ú', 'Û', 'Ü', 'Ý',
        'Þ', 'ß', 'à', 'á', 'â', 'ã', 'ä', 'å', 'æ', 'ç', 'è', 'é', 'ê', 'ë', 'ì', 'í', 'î', 'ï',
        'ð', 'ñ', 'ò', 'ó', 'ô', 'õ', 'ö', '÷', 'ø', 'ù', 'ú', 'û', 'ü', 'ý', 'þ', 'ÿ',
    ];

    const FONTS: &[(BuiltinFont, &[u8])] = &[
        (
            BuiltinFont::Courier,
            include_bytes!("../examples/assets/fonts/Courier.ttf"),
        ),
        (
            BuiltinFont::CourierOblique,
            include_bytes!("../examples/assets/fonts/Courier-Oblique.ttf"),
        ),
        (
            BuiltinFont::CourierBold,
            include_bytes!("../examples/assets/fonts/Courier-Bold.ttf"),
        ),
        (
            BuiltinFont::CourierBoldOblique,
            include_bytes!("../examples/assets/fonts/Courier-BoldOblique.ttf"),
        ),
        (
            BuiltinFont::Helvetica,
            include_bytes!("../examples/assets/fonts/Helvetica.ttf"),
        ),
        (
            BuiltinFont::HelveticaBold,
            include_bytes!("../examples/assets/fonts/Helvetica-Bold.ttf"),
        ),
        (
            BuiltinFont::HelveticaOblique,
            include_bytes!("../examples/assets/fonts/Helvetica-Oblique.ttf"),
        ),
        (
            BuiltinFont::HelveticaBoldOblique,
            include_bytes!("../examples/assets/fonts/Helvetica-BoldOblique.ttf"),
        ),
        (
            BuiltinFont::Symbol,
            include_bytes!("../examples/assets/fonts/PDFASymbol.woff2"),
        ),
        (
            BuiltinFont::TimesRoman,
            include_bytes!("../examples/assets/fonts/Times.ttf"),
        ),
        (
            BuiltinFont::TimesBold,
            include_bytes!("../examples/assets/fonts/Times-Bold.ttf"),
        ),
        (
            BuiltinFont::TimesItalic,
            include_bytes!("../examples/assets/fonts/Times-Oblique.ttf"),
        ),
        (
            BuiltinFont::TimesBoldItalic,
            include_bytes!("../examples/assets/fonts/Times-BoldOblique.ttf"),
        ),
        (
            BuiltinFont::ZapfDingbats,
            include_bytes!("../examples/assets/fonts/ZapfDingbats.ttf"),
        ),
    ];

    // Regenerates `defaultfonts/*.subset.ttf` and the `FONTS` table in `defaultfonts/mapping.rs`
    // from the fonts in `examples/assets/fonts`. Not a unit test: run it manually with --ignored.
    #[test]
    #[ignore = "regenerates bundled subset fonts + FONTS table; run manually with --ignored"]
    fn subset_test() {
        use std::collections::BTreeSet;
        
        let charmap: BTreeSet<char> = WIN_1252.iter().copied().collect();
        let mut target_map = vec![];

        let mut tm2 = BTreeMap::new();
        for (name, bytes) in FONTS {
            let mut warnings = Vec::new();
            let font = ParsedFont::from_bytes(bytes, 0, &mut warnings).unwrap();
            // Convert charmap to Vec<(u16, char)> format for subset()
            let glyph_ids: Vec<(u16, char)> = charmap.iter()
                .filter_map(|&ch| font.lookup_glyph_index(ch as u32).map(|gid| (gid, ch)))
                .collect();
            let (subset_bytes, glyph_mapping) = font.subset(&glyph_ids, azul_layout::CmapTarget::Unicode).unwrap();
            let glyph_mapping = glyph_mapping
                .into_iter()
                .map(|(k, (g, c))| (k, (g, c.to_string())))
                .collect();
            let subset = crate::font::SubsetFont { bytes: subset_bytes, glyph_mapping };
            tm2.insert(*name, subset.bytes.len());
            let _ = std::fs::write(
                format!(
                    "{}/defaultfonts/{}.subset.ttf",
                    env!("CARGO_MANIFEST_DIR"),
                    name.get_id()
                ),
                crate::utils::compress(&subset.bytes),
            );
            for (old_gid, (new_gid, char)) in subset.glyph_mapping.iter() {
                // glyph_mapping values are Strings (ligatures carry several characters); the
                // generated FONTS table holds one character per built-in glyph, which is always
                // the case for the WIN_1252 charmap.
                let ch = char.chars().next().unwrap_or('\u{FFFD}');
                target_map.push(format!(
                    "    ({}, {old_gid}, {new_gid}, '{c}'),",
                    name.get_num(),
                    c = if ch == '\'' {
                        "\\'".to_string()
                    } else if ch == '\\' {
                        "\\\\".to_string()
                    } else {
                        ch.to_string()
                    }
                ));
            }
        }

        let mut tm = vec![format!(
            "const FONTS: &[(usize, u16, u16, char);{}] = &[",
            target_map.len()
        )];
        tm.append(&mut target_map);
        tm.push("];".to_string());

        tm.push("fn match_len(bytes: &[u8]) -> Option<BuiltinFont> {".to_string());
        tm.push("match bytes.len() {".to_string());
        for (f, b) in tm2.iter() {
            tm.push(format!("{b} => Some(BuiltinFont::{f:?}),"));
        }
        tm.push("_ => None,".to_string());
        tm.push("}".to_string());
        tm.push("}".to_string());

        let _ = std::fs::write(
            format!("{}/defaultfonts/mapping.rs", env!("CARGO_MANIFEST_DIR")),
            tm.join("\r\n"),
        );
    }
}


#[cfg(all(test, feature = "text_layout"))]
mod bare_cff_wrap_test {
    use super::{extract_cid_keyed_cff, wrap_bare_cff_as_sfnt};

    const MOCK_CFF_CID: &[u8] = include_bytes!("../tests/assets/fonts/mock/mock_cff_cid.otf");
    const MOCK_CFF_CID_FM: &[u8] =
        include_bytes!("../tests/assets/fonts/mock/mock_cff_cid_fm.otf");

    /// (offset, length) of `tag` in a raw sfnt's table directory.
    fn table(sfnt: &[u8], tag: &[u8; 4]) -> (usize, usize) {
        let num_tables = u16::from_be_bytes([sfnt[4], sfnt[5]]) as usize;
        for rec in 0..num_tables {
            let off = 12 + 16 * rec;
            if &sfnt[off..off + 4] == tag {
                let start =
                    u32::from_be_bytes(sfnt[off + 8..off + 12].try_into().unwrap()) as usize;
                let len =
                    u32::from_be_bytes(sfnt[off + 12..off + 16].try_into().unwrap()) as usize;
                return (start, len);
            }
        }
        panic!("table {} missing", String::from_utf8_lossy(tag));
    }

    fn wrap(font: &[u8]) -> Vec<u8> {
        let bare = extract_cid_keyed_cff(font, 0).expect("mock font must be CID-keyed CFF");
        wrap_bare_cff_as_sfnt(&bare).expect("bare CFF must wrap")
    }

    fn upm(sfnt: &[u8]) -> u16 {
        let (off, _) = table(sfnt, b"head");
        u16::from_be_bytes([sfnt[off + 18], sfnt[off + 19]])
    }

    fn advance(sfnt: &[u8], gid: usize) -> u16 {
        let (off, len) = table(sfnt, b"hmtx");
        assert!(4 * gid + 2 <= len, "gid {gid} beyond hmtx");
        u16::from_be_bytes([sfnt[off + 4 * gid], sfnt[off + 4 * gid + 1]])
    }

    #[test]
    fn synthetic_sfnt_upm_follows_font_matrix() {
        // default 0.001 FontMatrix -> 1000/em; explicit 0.0005 -> 2000/em.
        // A hardcoded 1000 would halve every metric of the _fm mock on re-parse.
        assert_eq!(upm(&wrap(MOCK_CFF_CID)), 1000);
        assert_eq!(upm(&wrap(MOCK_CFF_CID_FM)), 2000);
    }

    #[test]
    fn width_scanner_evaluates_escape_arithmetic() {
        let w = wrap(MOCK_CFF_CID_FM);
        // gid 3 = B: the charstring pushes its width operand as `350 350 add`
        // (scripts/gen_mock_fonts.py); a scanner that bails on the 12-xx escape
        // falls back to defaultWidthX = 0 here.
        assert_eq!(advance(&w, 3), 700);
        // plain-number widths around it, in the 2000-unit em
        assert_eq!(advance(&w, 0), 1000); // .notdef
        assert_eq!(advance(&w, 1), 500); // space
        assert_eq!(advance(&w, 2), 600); // A
        assert_eq!(advance(&w, 11), 1500); // J
    }
}
