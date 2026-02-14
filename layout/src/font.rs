#![cfg(feature = "font_loading")]

use azul_css::{AzString, U8Vec};
use rust_fontconfig::{FcFontCache, FontSource};

pub mod loading {
    #![cfg(feature = "std")]
    #![cfg(feature = "font_loading")]
    #![cfg_attr(not(feature = "std"), no_std)]

    use std::io::Error as IoError;

    use azul_css::{AzString, StringVec, U8Vec};
    use rust_fontconfig::FcFontCache;

    #[cfg(not(miri))]
    pub fn build_font_cache() -> FcFontCache {
        FcFontCache::build()
    }

    #[cfg(miri)]
    pub fn build_font_cache() -> FcFontCache {
        FcFontCache::default()
    }

    #[derive(Debug)]
    pub enum FontReloadError {
        Io(IoError, AzString),
        FontNotFound(AzString),
        FontLoadingNotActive(AzString),
    }

    impl Clone for FontReloadError {
        fn clone(&self) -> Self {
            use self::FontReloadError::*;
            match self {
                Io(err, path) => Io(IoError::new(err.kind(), "Io Error"), path.clone()),
                FontNotFound(id) => FontNotFound(id.clone()),
                FontLoadingNotActive(id) => FontLoadingNotActive(id.clone()),
            }
        }
    }

    azul_core::impl_display!(FontReloadError, {
        Io(err, path_buf) => format!("Could not load \"{}\" - IO error: {}", path_buf.as_str(), err),
        FontNotFound(id) => format!("Could not locate system font: \"{:?}\" found", id),
        FontLoadingNotActive(id) => format!("Could not load system font: \"{:?}\": crate was not compiled with --features=\"font_loading\"", id)
    });
}
pub mod mock {
    //! Mock font implementation for testing text layout.
    //!
    //! Provides a `MockFont` that simulates font behavior without requiring
    //! actual font files, useful for unit testing text layout functionality.

    use std::collections::BTreeMap;

    use crate::text3::cache::LayoutFontMetrics;

    /// A mock font implementation for testing text layout without real fonts.
    ///
    /// This allows testing text shaping, layout, and rendering code paths
    /// without needing to load actual TrueType/OpenType font files.
    #[derive(Debug, Clone)]
    pub struct MockFont {
        /// Font metrics (ascent, descent, etc.).
        pub font_metrics: LayoutFontMetrics,
        /// Width of the space character in font units.
        pub space_width: Option<usize>,
        /// Horizontal advance widths keyed by glyph ID.
        pub glyph_advances: BTreeMap<u16, u16>,
        /// Glyph bounding box sizes (width, height) keyed by glyph ID.
        pub glyph_sizes: BTreeMap<u16, (i32, i32)>,
        /// Unicode codepoint to glyph ID mapping.
        pub glyph_indices: BTreeMap<u32, u16>,
    }

    impl MockFont {
        /// Creates a new `MockFont` with the given font metrics.
        pub fn new(font_metrics: LayoutFontMetrics) -> Self {
            MockFont {
                font_metrics,
                space_width: Some(10),
                glyph_advances: BTreeMap::new(),
                glyph_sizes: BTreeMap::new(),
                glyph_indices: BTreeMap::new(),
            }
        }

        /// Sets the space character width.
        pub fn with_space_width(mut self, width: usize) -> Self {
            self.space_width = Some(width);
            self
        }

        /// Adds a horizontal advance value for a glyph.
        pub fn with_glyph_advance(mut self, glyph_index: u16, advance: u16) -> Self {
            self.glyph_advances.insert(glyph_index, advance);
            self
        }

        /// Adds a bounding box size for a glyph.
        pub fn with_glyph_size(mut self, glyph_index: u16, size: (i32, i32)) -> Self {
            self.glyph_sizes.insert(glyph_index, size);
            self
        }

        /// Adds a Unicode codepoint to glyph ID mapping.
        pub fn with_glyph_index(mut self, unicode: u32, index: u16) -> Self {
            self.glyph_indices.insert(unicode, index);
            self
        }
    }
}

pub mod parsed {
    use core::fmt;
    use std::{collections::BTreeMap, sync::Arc};

    use allsorts::{
        binary::read::ReadScope,
        font_data::FontData,
        layout::{GDEFTable, LayoutCache, LayoutCacheData, GPOS, GSUB},
        outline::{OutlineBuilder, OutlineSink},
        pathfinder_geometry::{line_segment::LineSegment2F, vector::Vector2F},
        subset::{subset as allsorts_subset, whole_font, CmapTarget, SubsetProfile},
        tables::{
            cmap::owned::CmapSubtable as OwnedCmapSubtable,
            glyf::{
                Glyph, GlyfVisitorContext, LocaGlyf, Point,
                VariableGlyfContext, VariableGlyfContextStore,
            },
            kern::owned::KernTable,
            FontTableProvider, HheaTable, MaxpTable,
        },
        tag,
    };
    use azul_core::resources::{
        GlyphOutline, GlyphOutlineOperation, OutlineCubicTo, OutlineLineTo, OutlineMoveTo,
        OutlineQuadTo, OwnedGlyphBoundingBox,
    };
    use azul_css::props::basic::FontMetrics as CssFontMetrics;

    // Mock font module for testing
    pub use crate::font::mock::MockFont;
    use crate::text3::cache::LayoutFontMetrics;

    /// Cached GSUB table for glyph substitution operations.
    pub type GsubCache = Arc<LayoutCacheData<GSUB>>;
    /// Cached GPOS table for glyph positioning operations.
    pub type GposCache = Arc<LayoutCacheData<GPOS>>;

    /// Adapter that collects allsorts outline commands into our `GlyphOutline` format.
    ///
    /// Implements `OutlineSink` so it can be passed to `GlyfVisitorContext::visit()`.
    /// This handles composite glyph resolution, transforms, and variable font
    /// deltas automatically via allsorts internals.
    struct GlyphOutlineCollector {
        contours: Vec<GlyphOutline>,
        current_contour: Vec<GlyphOutlineOperation>,
    }

    impl GlyphOutlineCollector {
        fn new() -> Self {
            Self {
                contours: Vec::new(),
                current_contour: Vec::new(),
            }
        }

        fn into_outlines(mut self) -> Vec<GlyphOutline> {
            if !self.current_contour.is_empty() {
                self.contours.push(GlyphOutline {
                    operations: std::mem::take(&mut self.current_contour).into(),
                });
            }
            self.contours
        }
    }

    impl OutlineSink for GlyphOutlineCollector {
        fn move_to(&mut self, to: Vector2F) {
            if !self.current_contour.is_empty() {
                self.contours.push(GlyphOutline {
                    operations: std::mem::take(&mut self.current_contour).into(),
                });
            }
            self.current_contour.push(GlyphOutlineOperation::MoveTo(OutlineMoveTo {
                x: to.x() as i16,
                y: to.y() as i16,
            }));
        }

        fn line_to(&mut self, to: Vector2F) {
            self.current_contour.push(GlyphOutlineOperation::LineTo(OutlineLineTo {
                x: to.x() as i16,
                y: to.y() as i16,
            }));
        }

        fn quadratic_curve_to(&mut self, ctrl: Vector2F, to: Vector2F) {
            self.current_contour.push(GlyphOutlineOperation::QuadraticCurveTo(
                OutlineQuadTo {
                    ctrl_1_x: ctrl.x() as i16,
                    ctrl_1_y: ctrl.y() as i16,
                    end_x: to.x() as i16,
                    end_y: to.y() as i16,
                },
            ));
        }

        fn cubic_curve_to(&mut self, ctrl: LineSegment2F, to: Vector2F) {
            self.current_contour.push(GlyphOutlineOperation::CubicCurveTo(
                OutlineCubicTo {
                    ctrl_1_x: ctrl.from_x() as i16,
                    ctrl_1_y: ctrl.from_y() as i16,
                    ctrl_2_x: ctrl.to_x() as i16,
                    ctrl_2_y: ctrl.to_y() as i16,
                    end_x: to.x() as i16,
                    end_y: to.y() as i16,
                },
            ));
        }

        fn close(&mut self) {
            self.current_contour.push(GlyphOutlineOperation::ClosePath);
            self.contours.push(GlyphOutline {
                operations: std::mem::take(&mut self.current_contour).into(),
            });
        }
    }

    /// Parsed font data with all required tables for text layout and PDF generation.
    ///
    /// This struct holds the parsed representation of a TrueType/OpenType font,
    /// including glyph outlines, metrics, and shaping tables. It's used for:
    /// - Text layout (via GSUB/GPOS tables)
    /// - Glyph rendering (via glyf/CFF outlines)
    /// - PDF font embedding (via font metrics and subsetting)
    #[derive(Clone)]
    pub struct ParsedFont {
        /// Hash of the font bytes for caching and equality checks.
        pub hash: u64,
        /// Layout-specific font metrics (ascent, descent, line gap).
        pub font_metrics: LayoutFontMetrics,
        /// PDF-specific detailed font metrics from HEAD, HHEA, OS/2 tables.
        pub pdf_font_metrics: PdfFontMetrics,
        /// Total number of glyphs in the font (from maxp table).
        pub num_glyphs: u16,
        /// Horizontal header table (hhea) containing global horizontal metrics.
        pub hhea_table: HheaTable,
        /// Raw horizontal metrics data (hmtx table bytes).
        pub hmtx_data: Vec<u8>,
        /// Raw vertical metrics data (vmtx table bytes, if present).
        pub vmtx_data: Vec<u8>,
        /// Maximum profile table (maxp) containing glyph count and memory hints.
        pub maxp_table: MaxpTable,
        /// Cached GSUB table for glyph substitution (ligatures, alternates).
        pub gsub_cache: Option<GsubCache>,
        /// Cached GPOS table for glyph positioning (kerning, mark placement).
        pub gpos_cache: Option<GposCache>,
        /// Glyph definition table (GDEF) for glyph classification.
        pub opt_gdef_table: Option<Arc<GDEFTable>>,
        /// Legacy kerning table (kern) for fonts without GPOS.
        pub opt_kern_table: Option<Arc<KernTable>>,
        /// Decoded glyph records with outlines and metrics, keyed by glyph ID.
        pub glyph_records_decoded: BTreeMap<u16, OwnedGlyph>,
        /// Cached width of the space character in font units.
        pub space_width: Option<usize>,
        /// Character-to-glyph mapping (cmap subtable).
        pub cmap_subtable: Option<OwnedCmapSubtable>,
        /// Mock font data for testing (replaces real font behavior).
        pub mock: Option<Box<MockFont>>,
        /// Reverse mapping: glyph_id -> cluster text (handles ligatures like "fi").
        pub reverse_glyph_cache: std::collections::BTreeMap<u16, String>,
        /// Original font bytes (needed for subsetting and reconstruction).
        pub original_bytes: Vec<u8>,
        /// Font index within collection (0 for single-font files).
        pub original_index: usize,
        /// GID to CID mapping for CFF fonts (required for PDF embedding).
        pub index_to_cid: BTreeMap<u16, u16>,
        /// Font type (TrueType outlines or OpenType CFF).
        pub font_type: FontType,
        /// PostScript font name from the NAME table.
        pub font_name: Option<String>,
    }

    /// Distinguishes TrueType fonts from OpenType CFF fonts.
    ///
    /// This affects how glyph outlines are extracted and how the font
    /// is embedded in PDF documents.
    #[derive(Debug, Clone, PartialEq)]
    pub enum FontType {
        /// TrueType font with quadratic Bézier outlines in glyf table.
        TrueType,
        /// OpenType font with cubic Bézier outlines in CFF table.
        /// Contains the serialized CFF data for PDF embedding.
        OpenTypeCFF(Vec<u8>),
    }

    /// PDF-specific font metrics from HEAD, HHEA, and OS/2 tables.
    ///
    /// These metrics are used for PDF font descriptors and accurate
    /// text positioning in generated PDF documents.
    #[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
    #[repr(C)]
    pub struct PdfFontMetrics {
        // -- HEAD table fields --
        /// Font units per em-square (typically 1000 or 2048).
        pub units_per_em: u16,
        /// Font flags (italic, bold, fixed-pitch, etc.).
        pub font_flags: u16,
        /// Minimum x-coordinate across all glyphs.
        pub x_min: i16,
        /// Minimum y-coordinate across all glyphs.
        pub y_min: i16,
        /// Maximum x-coordinate across all glyphs.
        pub x_max: i16,
        /// Maximum y-coordinate across all glyphs.
        pub y_max: i16,

        // -- HHEA table fields --
        /// Typographic ascender (distance above baseline).
        pub ascender: i16,
        /// Typographic descender (distance below baseline, usually negative).
        pub descender: i16,
        /// Recommended line gap between lines of text.
        pub line_gap: i16,
        /// Maximum horizontal advance width across all glyphs.
        pub advance_width_max: u16,
        /// Caret slope rise for italic angle calculation.
        pub caret_slope_rise: i16,
        /// Caret slope run for italic angle calculation.
        pub caret_slope_run: i16,

        // -- OS/2 table fields (0 if table not present) --
        /// Average width of lowercase letters.
        pub x_avg_char_width: i16,
        /// Visual weight class (100-900, 400=normal, 700=bold).
        pub us_weight_class: u16,
        /// Visual width class (1-9, 5=normal).
        pub us_width_class: u16,
        /// Thickness of strikeout stroke in font units.
        pub y_strikeout_size: i16,
        /// Vertical position of strikeout stroke.
        pub y_strikeout_position: i16,
    }

    impl Default for PdfFontMetrics {
        fn default() -> Self {
            PdfFontMetrics::zero()
        }
    }

    impl PdfFontMetrics {
        pub const fn zero() -> Self {
            PdfFontMetrics {
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
                caret_slope_rise: 0,
                caret_slope_run: 0,
                x_avg_char_width: 0,
                us_weight_class: 0,
                us_width_class: 0,
                y_strikeout_size: 0,
                y_strikeout_position: 0,
            }
        }
    }

    /// Result of font subsetting operation.
    ///
    /// Contains the subsetted font bytes and a mapping from original
    /// glyph IDs to new glyph IDs in the subset.
    #[derive(Debug, Clone)]
    pub struct SubsetFont {
        /// The subsetted font file bytes (smaller than original).
        pub bytes: Vec<u8>,
        /// Mapping: original glyph ID -> (new subset glyph ID, source character).
        pub glyph_mapping: BTreeMap<u16, (u16, char)>,
    }

    impl SubsetFont {
        /// Return the changed text so that when rendering with the subset font (instead of the
        /// original) the renderer will end up at the same glyph IDs as if we used the original text
        /// on the original font
        pub fn subset_text(&self, text: &str) -> String {
            text.chars()
                .filter_map(|c| {
                    self.glyph_mapping.values().find_map(|(ngid, ch)| {
                        if *ch == c {
                            char::from_u32(*ngid as u32)
                        } else {
                            None
                        }
                    })
                })
                .collect()
        }
    }

    impl PartialEq for ParsedFont {
        fn eq(&self, other: &Self) -> bool {
            self.hash == other.hash
        }
    }

    impl Eq for ParsedFont {}

    const FONT_B64_START: &str = "data:font/ttf;base64,";

    impl serde::Serialize for ParsedFont {
        fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            use base64::Engine;
            let s = format!(
                "{FONT_B64_START}{}",
                base64::prelude::BASE64_STANDARD.encode(&self.to_bytes(None).unwrap_or_default())
            );
            s.serialize(serializer)
        }
    }

    impl<'de> serde::Deserialize<'de> for ParsedFont {
        fn deserialize<D: serde::Deserializer<'de>>(
            deserializer: D,
        ) -> Result<ParsedFont, D::Error> {
            use base64::Engine;
            let s = String::deserialize(deserializer)?;
            let b64 = if s.starts_with(FONT_B64_START) {
                let b = &s[FONT_B64_START.len()..];
                base64::prelude::BASE64_STANDARD.decode(&b).ok()
            } else {
                None
            };

            let mut warnings = Vec::new();
            ParsedFont::from_bytes(&b64.unwrap_or_default(), 0, &mut warnings).ok_or_else(|| {
                serde::de::Error::custom(format!("Font deserialization error: {warnings:?}"))
            })
        }
    }

    impl fmt::Debug for ParsedFont {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("ParsedFont")
                .field("hash", &self.hash)
                .field("font_metrics", &self.font_metrics)
                .field("num_glyphs", &self.num_glyphs)
                .field("hhea_table", &self.hhea_table)
                .field(
                    "hmtx_data",
                    &format_args!("<{} bytes>", self.hmtx_data.len()),
                )
                .field("maxp_table", &self.maxp_table)
                .field(
                    "glyph_records_decoded",
                    &format_args!("{} entries", self.glyph_records_decoded.len()),
                )
                .field("space_width", &self.space_width)
                .field("cmap_subtable", &self.cmap_subtable)
                .finish()
        }
    }

    /// Warning or error message generated during font parsing.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct FontParseWarning {
        /// Severity level of this warning.
        pub severity: FontParseWarningSeverity,
        /// Human-readable description of the issue.
        pub message: String,
    }

    /// Severity level for font parsing warnings.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum FontParseWarningSeverity {
        /// Informational message (not an error).
        Info,
        /// Warning that may affect font rendering.
        Warning,
        /// Error that prevents proper font usage.
        Error,
    }

    impl FontParseWarning {
        /// Creates an info-level message.
        pub fn info(message: String) -> Self {
            Self {
                severity: FontParseWarningSeverity::Info,
                message,
            }
        }

        /// Creates a warning-level message.
        pub fn warning(message: String) -> Self {
            Self {
                severity: FontParseWarningSeverity::Warning,
                message,
            }
        }

        /// Creates an error-level message.
        pub fn error(message: String) -> Self {
            Self {
                severity: FontParseWarningSeverity::Error,
                message,
            }
        }
    }

    impl ParsedFont {
        /// Parse a font from bytes using allsorts
        ///
        /// # Arguments
        /// * `font_bytes` - The font file data
        /// * `font_index` - Index of the font in a font collection (0 for single fonts)
        /// * `warnings` - Optional vector to collect parsing warnings
        ///
        /// # Returns
        /// `Some(ParsedFont)` if parsing succeeds, `None` otherwise
        ///
        /// Note: Outlines are always parsed (parse_outlines = true)
        pub fn from_bytes(
            font_bytes: &[u8],
            font_index: usize,
            warnings: &mut Vec<FontParseWarning>,
        ) -> Option<Self> {
            use std::{
                collections::hash_map::DefaultHasher,
                hash::{Hash, Hasher},
            };

            use allsorts::{
                binary::read::ReadScope,
                font_data::FontData,
                tables::{
                    cmap::{owned::CmapSubtable as OwnedCmapSubtable, CmapSubtable},
                    FontTableProvider, HeadTable, HheaTable, MaxpTable,
                },
                tag,
            };

            let scope = ReadScope::new(font_bytes);
            let font_file = match scope.read::<FontData<'_>>() {
                Ok(ff) => {
                    warnings.push(FontParseWarning::info(
                        "Successfully read font data".to_string(),
                    ));
                    ff
                }
                Err(e) => {
                    warnings.push(FontParseWarning::error(format!(
                        "Failed to read font data: {}",
                        e
                    )));
                    return None;
                }
            };
            let provider = match font_file.table_provider(font_index) {
                Ok(p) => {
                    warnings.push(FontParseWarning::info(format!(
                        "Successfully loaded font at index {}",
                        font_index
                    )));
                    p
                }
                Err(e) => {
                    warnings.push(FontParseWarning::error(format!(
                        "Failed to get table provider for font index {}: {}",
                        font_index, e
                    )));
                    return None;
                }
            };

            // Extract font name from NAME table early (before provider is moved)
            let font_name = provider.table_data(tag::NAME).ok().and_then(|name_data| {
                ReadScope::new(&name_data?)
                    .read::<allsorts::tables::NameTable>()
                    .ok()
                    .and_then(|name_table| {
                        name_table.string_for_id(allsorts::tables::NameTable::POSTSCRIPT_NAME)
                    })
            });

            let head_table = provider
                .table_data(tag::HEAD)
                .ok()
                .and_then(|head_data| ReadScope::new(&head_data?).read::<HeadTable>().ok())?;

            let maxp_table = provider
                .table_data(tag::MAXP)
                .ok()
                .and_then(|maxp_data| ReadScope::new(&maxp_data?).read::<MaxpTable>().ok())
                .unwrap_or(MaxpTable {
                    num_glyphs: 0,
                    version1_sub_table: None,
                });

            let num_glyphs = maxp_table.num_glyphs as usize;

            let hmtx_data = provider
                .table_data(tag::HMTX)
                .ok()
                .and_then(|s| Some(s?.to_vec()))
                .unwrap_or_default();

            let vmtx_data = provider
                .table_data(tag::VMTX)
                .ok()
                .and_then(|s| Some(s?.to_vec()))
                .unwrap_or_default();

            let hhea_table = provider
                .table_data(tag::HHEA)
                .ok()
                .and_then(|hhea_data| ReadScope::new(&hhea_data?).read::<HheaTable>().ok())
                .unwrap_or(unsafe { std::mem::zeroed() });

            // Build layout-specific font metrics
            let font_metrics = LayoutFontMetrics {
                units_per_em: if head_table.units_per_em == 0 {
                    1000
                } else {
                    head_table.units_per_em
                },
                ascent: hhea_table.ascender as f32,
                descent: hhea_table.descender as f32,
                line_gap: hhea_table.line_gap as f32,
            };

            // Build PDF-specific font metrics
            let pdf_font_metrics =
                Self::parse_pdf_font_metrics(font_bytes, font_index, &head_table, &hhea_table);

            // Use allsorts LocaGlyf + GlyfVisitorContext for outline extraction.
            // This correctly handles composite glyphs (recursive resolution, transforms)
            // and variable font deltas (gvar) automatically via allsorts internals.
            let has_glyf = provider.has_table(tag::GLYF) && provider.has_table(tag::LOCA);

            let glyph_records_decoded: BTreeMap<u16, OwnedGlyph> = if has_glyf {
                warnings.push(FontParseWarning::info(
                    "Parsing glyph outlines via allsorts OutlineBuilder (composite-safe)".to_string(),
                ));

                // Load LocaGlyf for the visitor
                match LocaGlyf::load(&provider) {
                    Ok(mut loca_glyf) => {
                        // Optionally set up variable font context for gvar deltas
                        let var_store = VariableGlyfContextStore::read(&provider).ok();
                        let var_context = var_store.as_ref()
                            .and_then(|store| VariableGlyfContext::new(store).ok());

                        let mut visitor = GlyfVisitorContext::new(
                            &mut loca_glyf,
                            var_context,
                        );

                        let mut map = BTreeMap::new();
                        for glyph_index in 0..num_glyphs.min(u16::MAX as usize) {
                            let gid = glyph_index as u16;
                            let horz_advance = allsorts::glyph_info::advance(
                                &maxp_table, &hhea_table, &hmtx_data, gid,
                            ).unwrap_or_default();

                            // Visit the glyph outline via allsorts (handles composites + transforms)
                            let mut collector = GlyphOutlineCollector::new();
                            // Use default variation instance (no user tuple)
                            let visit_result = visitor.visit(gid, None, &mut collector);

                            let outlines = match visit_result {
                                Ok(()) => collector.into_outlines(),
                                Err(_) => Vec::new(),
                            };

                            // Get bounding box from the collected outlines
                            let (min_x, min_y, max_x, max_y) = compute_outline_bbox(&outlines);

                            map.insert(gid, OwnedGlyph {
                                horz_advance,
                                bounding_box: OwnedGlyphBoundingBox {
                                    min_x, min_y, max_x, max_y,
                                },
                                outline: outlines,
                                phantom_points: None,
                            });
                        }
                        map
                    }
                    Err(e) => {
                        warnings.push(FontParseWarning::warning(format!(
                            "Failed to load LocaGlyf: {} — falling back to hmtx-only", e
                        )));
                        // Fall back to hmtx-only metrics
                        (0..num_glyphs.min(u16::MAX as usize))
                            .map(|glyph_index| {
                                let gid = glyph_index as u16;
                                let horz_advance = allsorts::glyph_info::advance(
                                    &maxp_table, &hhea_table, &hmtx_data, gid,
                                ).unwrap_or_default();
                                (gid, OwnedGlyph {
                                    horz_advance,
                                    bounding_box: OwnedGlyphBoundingBox {
                                        min_x: 0, min_y: 0,
                                        max_x: horz_advance as i16, max_y: 0,
                                    },
                                    outline: Vec::new(),
                                    phantom_points: None,
                                })
                            })
                            .collect()
                    }
                }
            } else {
                // CFF fonts or fonts without glyf table: Parse metrics only from hmtx
                warnings.push(FontParseWarning::info(format!(
                    "Using hmtx-only fallback for {} glyphs (CFF font or no glyf table)",
                    num_glyphs
                )));
                (0..num_glyphs.min(u16::MAX as usize))
                    .map(|glyph_index| {
                        let gid = glyph_index as u16;
                        let horz_advance = allsorts::glyph_info::advance(
                            &maxp_table, &hhea_table, &hmtx_data, gid,
                        ).unwrap_or_default();

                        (gid, OwnedGlyph {
                            horz_advance,
                            bounding_box: OwnedGlyphBoundingBox {
                                min_x: 0, min_y: 0,
                                max_x: horz_advance as i16, max_y: 0,
                            },
                            outline: Vec::new(),
                            phantom_points: None,
                        })
                    })
                    .collect::<BTreeMap<_, _>>()
            };

            let mut font_data_impl = allsorts::font::Font::new(provider).ok()?;

            // Required for font layout: gsub_cache, gpos_cache and gdef_table
            let gsub_cache = font_data_impl.gsub_cache().ok().and_then(|s| s);
            let gpos_cache = font_data_impl.gpos_cache().ok().and_then(|s| s);
            let opt_gdef_table = font_data_impl.gdef_table().ok().and_then(|o| o);
            let num_glyphs = font_data_impl.num_glyphs();

            let opt_kern_table = font_data_impl
                .kern_table()
                .ok()
                .and_then(|s| Some(s?.to_owned()));

            let cmap_data = font_data_impl.cmap_subtable_data();
            let cmap_subtable = ReadScope::new(cmap_data);
            let cmap_subtable = cmap_subtable
                .read::<CmapSubtable<'_>>()
                .ok()
                .and_then(|s| s.to_owned());

            // Calculate hash of font data
            let mut hasher = DefaultHasher::new();
            font_bytes.hash(&mut hasher);
            font_index.hash(&mut hasher);
            let hash = hasher.finish();

            let mut font = ParsedFont {
                hash,
                font_metrics,
                pdf_font_metrics,
                num_glyphs,
                hhea_table,
                hmtx_data,
                vmtx_data,
                maxp_table,
                gsub_cache,
                gpos_cache,
                opt_gdef_table,
                opt_kern_table,
                cmap_subtable,
                glyph_records_decoded,
                space_width: None,
                mock: None,
                reverse_glyph_cache: BTreeMap::new(),
                original_bytes: font_bytes.to_vec(),
                original_index: font_index,
                index_to_cid: BTreeMap::new(), // Will be filled for CFF fonts
                font_type: FontType::TrueType, // Default, will be updated if CFF
                font_name,
            };

            // Calculate space width
            let space_width = font.get_space_width_internal();

            // Ensure space glyph is in glyph_records_decoded
            // Space glyphs often don't have outlines, so they may not be loaded by default
            let _ = (|| {
                let space_gid = font.lookup_glyph_index(' ' as u32)?;
                if font.glyph_records_decoded.contains_key(&space_gid) {
                    return None; // Already exists
                }
                let space_width_val = space_width?;
                let space_record = OwnedGlyph {
                    bounding_box: OwnedGlyphBoundingBox {
                        max_x: 0,
                        max_y: 0,
                        min_x: 0,
                        min_y: 0,
                    },
                    horz_advance: space_width_val as u16,
                    outline: Vec::new(),
                    phantom_points: None,
                };
                font.glyph_records_decoded.insert(space_gid, space_record);
                Some(())
            })();

            font.space_width = space_width;

            Some(font)
        }

        /// Parse PDF-specific font metrics from HEAD, HHEA, and OS/2 tables
        fn parse_pdf_font_metrics(
            font_bytes: &[u8],
            font_index: usize,
            head_table: &allsorts::tables::HeadTable,
            hhea_table: &allsorts::tables::HheaTable,
        ) -> PdfFontMetrics {
            use allsorts::{
                binary::read::ReadScope,
                font_data::FontData,
                tables::{os2::Os2, FontTableProvider},
                tag,
            };

            let scope = ReadScope::new(font_bytes);
            let font_file = scope.read::<FontData<'_>>().ok();
            let provider = font_file
                .as_ref()
                .and_then(|ff| ff.table_provider(font_index).ok());

            let os2_table = provider
                .as_ref()
                .and_then(|p| p.table_data(tag::OS_2).ok())
                .and_then(|os2_data| {
                    let data = os2_data?;
                    let scope = ReadScope::new(&data);
                    scope.read_dep::<Os2>(data.len()).ok()
                });

            // Base metrics from HEAD and HHEA (always present)
            let base = PdfFontMetrics {
                units_per_em: head_table.units_per_em,
                font_flags: head_table.flags,
                x_min: head_table.x_min,
                y_min: head_table.y_min,
                x_max: head_table.x_max,
                y_max: head_table.y_max,
                ascender: hhea_table.ascender,
                descender: hhea_table.descender,
                line_gap: hhea_table.line_gap,
                advance_width_max: hhea_table.advance_width_max,
                caret_slope_rise: hhea_table.caret_slope_rise,
                caret_slope_run: hhea_table.caret_slope_run,
                ..PdfFontMetrics::zero()
            };

            // Add OS/2 metrics if available
            os2_table
                .map(|os2| PdfFontMetrics {
                    x_avg_char_width: os2.x_avg_char_width,
                    us_weight_class: os2.us_weight_class,
                    us_width_class: os2.us_width_class,
                    y_strikeout_size: os2.y_strikeout_size,
                    y_strikeout_position: os2.y_strikeout_position,
                    ..base
                })
                .unwrap_or(base)
        }

        /// Returns the width of the space character in font units.
        ///
        /// This is used internally for text layout calculations.
        /// Returns `None` if the font has no space glyph or its width cannot be determined.
        fn get_space_width_internal(&self) -> Option<usize> {
            if let Some(mock) = self.mock.as_ref() {
                return mock.space_width;
            }
            let glyph_index = self.lookup_glyph_index(' ' as u32)?;

            allsorts::glyph_info::advance(
                &self.maxp_table,
                &self.hhea_table,
                &self.hmtx_data,
                glyph_index,
            )
            .ok()
            .map(|s| s as usize)
        }

        /// Look up the glyph index for a Unicode codepoint
        pub fn lookup_glyph_index(&self, codepoint: u32) -> Option<u16> {
            let cmap = self.cmap_subtable.as_ref()?;
            cmap.map_glyph(codepoint).ok().flatten()
        }

        /// Get the horizontal advance width for a glyph in font units
        pub fn get_horizontal_advance(&self, glyph_index: u16) -> u16 {
            if let Some(mock) = self.mock.as_ref() {
                return mock.glyph_advances.get(&glyph_index).copied().unwrap_or(0);
            }
            self.glyph_records_decoded
                .get(&glyph_index)
                .map(|gi| gi.horz_advance)
                .unwrap_or_default()
        }

        /// Get the number of glyphs in this font
        pub fn num_glyphs(&self) -> u16 {
            self.num_glyphs
        }

        /// Check if this font has a glyph for the given codepoint
        pub fn has_glyph(&self, codepoint: u32) -> bool {
            self.lookup_glyph_index(codepoint).is_some()
        }

        /// Get vertical metrics for a glyph (for vertical text layout).
        ///
        /// Currently always returns `None` because vertical layout tables
        /// (vhea, vmtx) are not parsed. Vertical text layout is not yet supported.
        pub fn get_vertical_metrics(
            &self,
            _glyph_id: u16,
        ) -> Option<crate::text3::cache::VerticalMetrics> {
            // Vertical text layout requires parsing vhea and vmtx tables
            None
        }

        /// Get layout-specific font metrics
        pub fn get_font_metrics(&self) -> crate::text3::cache::LayoutFontMetrics {
            // Ensure descent is positive (OpenType may have negative descent)
            let descent = if self.font_metrics.descent > 0.0 {
                self.font_metrics.descent
            } else {
                -self.font_metrics.descent
            };

            crate::text3::cache::LayoutFontMetrics {
                ascent: self.font_metrics.ascent,
                descent,
                line_gap: self.font_metrics.line_gap,
                units_per_em: self.font_metrics.units_per_em,
            }
        }

        /// Convert the ParsedFont back to bytes using allsorts::whole_font
        /// This reconstructs the entire font from the parsed data
        ///
        /// # Arguments
        /// * `tags` - Optional list of specific table tags to include (None = all tables)
        pub fn to_bytes(&self, tags: Option<&[u32]>) -> Result<Vec<u8>, String> {
            let scope = ReadScope::new(&self.original_bytes);
            let font_file = scope.read::<FontData<'_>>().map_err(|e| e.to_string())?;
            let provider = font_file
                .table_provider(self.original_index)
                .map_err(|e| e.to_string())?;

            let tags_to_use = tags.unwrap_or(&[
                tag::CMAP,
                tag::HEAD,
                tag::HHEA,
                tag::HMTX,
                tag::MAXP,
                tag::NAME,
                tag::OS_2,
                tag::POST,
                tag::GLYF,
                tag::LOCA,
            ]);

            whole_font(&provider, tags_to_use).map_err(|e| e.to_string())
        }

        /// Create a subset font containing only the specified glyph IDs
        /// Returns the subset font bytes and a mapping from old to new glyph IDs
        ///
        /// # Arguments
        /// * `glyph_ids` - The glyph IDs to include in the subset (glyph 0/.notdef is always
        ///   included)
        /// * `cmap_target` - Target cmap format (Unicode for web, MacRoman for compatibility)
        ///
        /// # Returns
        /// A tuple of (subset_font_bytes, glyph_mapping) where glyph_mapping maps
        /// original_glyph_id -> (new_glyph_id, original_char)
        pub fn subset(
            &self,
            glyph_ids: &[(u16, char)],
            cmap_target: CmapTarget,
        ) -> Result<(Vec<u8>, BTreeMap<u16, (u16, char)>), String> {
            let scope = ReadScope::new(&self.original_bytes);
            let font_file = scope.read::<FontData<'_>>().map_err(|e| e.to_string())?;
            let provider = font_file
                .table_provider(self.original_index)
                .map_err(|e| e.to_string())?;

            // Build glyph mapping: original_id -> (new_id, char)
            let glyph_mapping: BTreeMap<u16, (u16, char)> = glyph_ids
                .iter()
                .enumerate()
                .map(|(new_id, &(original_id, ch))| (original_id, (new_id as u16, ch)))
                .collect();

            // Extract just the glyph IDs for subsetting
            let ids: Vec<u16> = glyph_ids.iter().map(|(id, _)| *id).collect();

            // Use PDF profile for embedding fonts in PDFs
            let font_bytes = allsorts_subset(&provider, &ids, &SubsetProfile::Pdf, cmap_target)
                .map_err(|e| format!("Subset error: {:?}", e))?;

            Ok((font_bytes, glyph_mapping))
        }

        /// Get the width of a glyph in font units (internal, unscaled)
        pub fn get_glyph_width_internal(&self, glyph_index: u16) -> Option<usize> {
            allsorts::glyph_info::advance(
                &self.maxp_table,
                &self.hhea_table,
                &self.hmtx_data,
                glyph_index,
            )
            .ok()
            .map(|s| s as usize)
        }

        /// Get the width of the space character (unscaled font units)
        #[inline]
        pub const fn get_space_width(&self) -> Option<usize> {
            self.space_width
        }

        /// Add glyph-to-text mapping to reverse cache
        /// This should be called during text shaping when we know both the source text and
        /// resulting glyphs
        pub fn cache_glyph_mapping(&mut self, glyph_id: u16, cluster_text: &str) {
            self.reverse_glyph_cache
                .insert(glyph_id, cluster_text.to_string());
        }

        /// Get the cluster text that produced a specific glyph ID
        /// Returns the original text that was shaped into this glyph (handles ligatures correctly)
        pub fn get_glyph_cluster_text(&self, glyph_id: u16) -> Option<&str> {
            self.reverse_glyph_cache.get(&glyph_id).map(|s| s.as_str())
        }

        /// Get the first character from the cluster text for a glyph ID
        /// This is useful for PDF ToUnicode CMap generation which requires single character
        /// mappings
        pub fn get_glyph_primary_char(&self, glyph_id: u16) -> Option<char> {
            self.reverse_glyph_cache
                .get(&glyph_id)
                .and_then(|text| text.chars().next())
        }

        /// Clear the reverse glyph cache (useful for memory management)
        pub fn clear_glyph_cache(&mut self) {
            self.reverse_glyph_cache.clear();
        }

        /// Get the bounding box size of a glyph (unscaled units) - for PDF
        /// Returns (width, height) in font units
        pub fn get_glyph_bbox_size(&self, glyph_index: u16) -> Option<(i32, i32)> {
            let g = self.glyph_records_decoded.get(&glyph_index)?;
            let glyph_width = g.horz_advance as i32;
            let glyph_height = g.bounding_box.max_y as i32 - g.bounding_box.min_y as i32;
            Some((glyph_width, glyph_height))
        }
    }

    /// Compute the bounding box from collected glyph outlines.
    fn compute_outline_bbox(outlines: &[GlyphOutline]) -> (i16, i16, i16, i16) {
        let mut min_x = i16::MAX;
        let mut min_y = i16::MAX;
        let mut max_x = i16::MIN;
        let mut max_y = i16::MIN;
        let mut has_points = false;

        for outline in outlines {
            for op in outline.operations.as_slice() {
                let points: &[(i16, i16)] = match op {
                    GlyphOutlineOperation::MoveTo(m) => &[(m.x, m.y)],
                    GlyphOutlineOperation::LineTo(l) => &[(l.x, l.y)],
                    GlyphOutlineOperation::QuadraticCurveTo(q) => {
                        // Check both control and end point for bbox
                        min_x = min_x.min(q.ctrl_1_x).min(q.end_x);
                        min_y = min_y.min(q.ctrl_1_y).min(q.end_y);
                        max_x = max_x.max(q.ctrl_1_x).max(q.end_x);
                        max_y = max_y.max(q.ctrl_1_y).max(q.end_y);
                        has_points = true;
                        continue;
                    }
                    GlyphOutlineOperation::CubicCurveTo(c) => {
                        min_x = min_x.min(c.ctrl_1_x).min(c.ctrl_2_x).min(c.end_x);
                        min_y = min_y.min(c.ctrl_1_y).min(c.ctrl_2_y).min(c.end_y);
                        max_x = max_x.max(c.ctrl_1_x).max(c.ctrl_2_x).max(c.end_x);
                        max_y = max_y.max(c.ctrl_1_y).max(c.ctrl_2_y).max(c.end_y);
                        has_points = true;
                        continue;
                    }
                    GlyphOutlineOperation::ClosePath => continue,
                };
                for &(x, y) in points {
                    min_x = min_x.min(x);
                    min_y = min_y.min(y);
                    max_x = max_x.max(x);
                    max_y = max_y.max(y);
                    has_points = true;
                }
            }
        }

        if has_points {
            (min_x, min_y, max_x, max_y)
        } else {
            (0, 0, 0, 0)
        }
    }

    #[derive(Debug, Clone)]
    pub struct OwnedGlyph {
        pub bounding_box: OwnedGlyphBoundingBox,
        pub horz_advance: u16,
        pub outline: Vec<GlyphOutline>,
        pub phantom_points: Option<[Point; 4]>,
    }

    // --- ParsedFontTrait Implementation for ParsedFont ---

    impl crate::text3::cache::ShallowClone for ParsedFont {
        fn shallow_clone(&self) -> Self {
            self.clone() // ParsedFont::clone uses Arc internally, so it's shallow
        }
    }

    impl crate::text3::cache::ParsedFontTrait for ParsedFont {
        fn shape_text(
            &self,
            text: &str,
            script: crate::font_traits::Script,
            language: crate::font_traits::Language,
            direction: crate::font_traits::BidiDirection,
            style: &crate::font_traits::StyleProperties,
        ) -> Result<Vec<crate::font_traits::Glyph>, crate::font_traits::LayoutError> {
            // Call the existing shape_text_for_parsed_font method (defined in default.rs)
            crate::text3::default::shape_text_for_parsed_font(
                self, text, script, language, direction, style,
            )
        }

        fn get_hash(&self) -> u64 {
            self.hash
        }

        fn get_glyph_size(
            &self,
            glyph_id: u16,
            font_size_px: f32,
        ) -> Option<azul_core::geom::LogicalSize> {
            self.glyph_records_decoded.get(&glyph_id).map(|record| {
                let units_per_em = self.font_metrics.units_per_em as f32;
                let scale_factor = if units_per_em > 0.0 {
                    font_size_px / units_per_em
                } else {
                    0.01
                };
                let bbox = &record.bounding_box;
                azul_core::geom::LogicalSize {
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

        fn has_glyph(&self, codepoint: u32) -> bool {
            self.lookup_glyph_index(codepoint).is_some()
        }

        fn get_vertical_metrics(
            &self,
            glyph_id: u16,
        ) -> Option<crate::text3::cache::VerticalMetrics> {
            // Default implementation - can be enhanced later
            None
        }

        fn get_font_metrics(&self) -> crate::text3::cache::LayoutFontMetrics {
            self.font_metrics.clone()
        }

        fn num_glyphs(&self) -> u16 {
            self.num_glyphs
        }
    }
}
