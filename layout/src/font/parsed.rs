use core::fmt;
use std::{collections::BTreeMap, sync::Arc};

use allsorts::{
    binary::read::ReadScope,
    font_data::FontData,
    layout::{GDEFTable, LayoutCache, LayoutCacheData, GPOS, GSUB},
    subset::{subset as allsorts_subset, whole_font, CmapTarget, SubsetProfile},
    tables::{
        cmap::owned::CmapSubtable as OwnedCmapSubtable,
        glyf::{
            ComponentOffsets, CompositeGlyph, CompositeGlyphArgument, CompositeGlyphComponent,
            CompositeGlyphScale, EmptyGlyph, Glyph, Point, SimpleGlyph,
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

use crate::text3::cache::LayoutFontMetrics;

// Mock font module for testing
pub use crate::font::mock::MockFont;

/// Cached GSUB table for glyph substitution operations.
pub type GsubCache = Arc<LayoutCacheData<GSUB>>;
/// Cached GPOS table for glyph positioning operations.
pub type GposCache = Arc<LayoutCacheData<GPOS>>;
/// Glyph outline contours: outer Vec = contours, inner Vec = operations per contour.
pub type GlyphOutlineContours = Vec<Vec<GlyphOutlineOperation>>;

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
    pub pdf_font_metrics: FontMetrics,
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
pub struct FontMetrics {
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

impl Default for FontMetrics {
    fn default() -> Self {
        FontMetrics::zero()
    }
}

impl FontMetrics {
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
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<ParsedFont, D::Error> {
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
                glyf::{GlyfRecord, GlyfTable},
                loca::{LocaOffsets, LocaTable},
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

        let index_to_loc = head_table.index_to_loc_format;
        let num_glyphs = maxp_table.num_glyphs as usize;

        let loca_table = provider.table_data(tag::LOCA).ok();
        let loca_table = loca_table
            .as_ref()
            .and_then(|loca_data| {
                ReadScope::new(&loca_data.as_ref()?)
                    .read_dep::<LocaTable<'_>>((
                        num_glyphs.min(u16::MAX as usize) as u16,
                        index_to_loc,
                    ))
                    .ok()
            })
            .unwrap_or(LocaTable {
                offsets: LocaOffsets::Long(allsorts::binary::read::ReadArray::empty()),
            });

        let glyf_table = provider.table_data(tag::GLYF).ok();
        let mut glyf_table = glyf_table
            .as_ref()
            .and_then(|glyf_data| {
                ReadScope::new(&glyf_data.as_ref()?)
                    .read_dep::<GlyfTable<'_>>(&loca_table)
                    .ok()
            })
            .unwrap_or(GlyfTable::new(Vec::new()).unwrap());

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

        // Parse glyph outlines and metrics (always enabled for PDF generation)
        // For CFF fonts (no glyf table), we fall back to hmtx-only metrics
        let glyf_records_count = glyf_table.records().len();
        let use_glyf_parsing = glyf_records_count > 0;

        warnings.push(FontParseWarning::info(format!(
            "Font has {} glyf records, {} total glyphs, use_glyf_parsing={}",
            glyf_records_count, num_glyphs, use_glyf_parsing
        )));

        let glyph_records_decoded = if use_glyf_parsing {
            warnings.push(FontParseWarning::info(
                "Parsing glyph outlines from glyf table".to_string(),
            ));
            // Full parsing: outlines + metrics from TrueType glyf table
            // CRITICAL: Always call .parse() first to convert Present -> Parsed!
            glyf_table
                .records_mut()
                .into_iter()
                .enumerate()
                .filter_map(|(glyph_index, glyph_record)| {
                    if glyph_index > (u16::MAX as usize) {
                        return None;
                    }

                    // ALWAYS parse the glyph record first!
                    if let Err(_e) = glyph_record.parse() {
                        // If parsing fails, we can still try to get the advance width
                        let glyph_index = glyph_index as u16;
                        let horz_advance = allsorts::glyph_info::advance(
                            &maxp_table,
                            &hhea_table,
                            &hmtx_data,
                            glyph_index,
                        )
                        .unwrap_or_default();

                        // Return minimal glyph with just advance
                        return Some((
                            glyph_index,
                            OwnedGlyph {
                                horz_advance,
                                bounding_box: OwnedGlyphBoundingBox {
                                    min_x: 0,
                                    min_y: 0,
                                    max_x: horz_advance as i16,
                                    max_y: 0,
                                },
                                outline: Vec::new(),
                                unresolved_composite: Vec::new(),
                                phantom_points: None,
                            },
                        ));
                    }

                    let glyph_index = glyph_index as u16;
                    let horz_advance = allsorts::glyph_info::advance(
                        &maxp_table,
                        &hhea_table,
                        &hmtx_data,
                        glyph_index,
                    )
                    .unwrap_or_default();

                    // After parse(), record should be Parsed, not Present
                    match glyph_record {
                        GlyfRecord::Present { .. } => {
                            // This shouldn't happen after parse(), but handle it anyway
                            Some((
                                glyph_index,
                                OwnedGlyph {
                                    horz_advance,
                                    bounding_box: OwnedGlyphBoundingBox {
                                        min_x: 0,
                                        min_y: 0,
                                        max_x: horz_advance as i16,
                                        max_y: 0,
                                    },
                                    outline: Vec::new(),
                                    unresolved_composite: Vec::new(),
                                    phantom_points: None,
                                },
                            ))
                        }
                        GlyfRecord::Parsed(g) => {
                            OwnedGlyph::from_glyph_data(g, horz_advance).map(|g| (glyph_index, g))
                        }
                    }
                })
                .collect::<Vec<_>>()
                .into_iter()
                .collect::<BTreeMap<_, _>>()
        } else {
            // CFF fonts or fonts without glyf table: Parse metrics only from hmtx
            // This creates OwnedGlyph records with advance width but no outlines
            warnings.push(FontParseWarning::info(format!(
                "Using hmtx-only fallback for {} glyphs (CFF font or no glyf table)",
                num_glyphs
            )));
            (0..num_glyphs as usize)
                .filter_map(|glyph_index| {
                    if glyph_index > u16::MAX as usize {
                        return None;
                    }
                    let glyph_index_u16 = glyph_index as u16;
                    let horz_advance = allsorts::glyph_info::advance(
                        &maxp_table,
                        &hhea_table,
                        &hmtx_data,
                        glyph_index_u16,
                    )
                    .unwrap_or_default();

                    Some((
                        glyph_index_u16,
                        OwnedGlyph {
                            horz_advance,
                            bounding_box: OwnedGlyphBoundingBox {
                                min_x: 0,
                                min_y: 0,
                                max_x: horz_advance as i16,
                                max_y: 0,
                            },
                            outline: Vec::new(), // No outline data
                            unresolved_composite: Vec::new(),
                            phantom_points: None,
                        },
                    ))
                })
                .collect::<BTreeMap<_, _>>()
        };

        // Resolve composite glyphs in multiple passes
        let mut glyph_records_decoded = glyph_records_decoded;
        for _ in 0..6 {
            let composite_glyphs_to_resolve = glyph_records_decoded
                .iter()
                .filter(|s| !s.1.unresolved_composite.is_empty())
                .map(|(k, v)| (*k, v.clone()))
                .collect::<Vec<_>>();

            if composite_glyphs_to_resolve.is_empty() {
                break;
            }

            for (k, mut v) in composite_glyphs_to_resolve {
                resolved_glyph_components(&mut v, &glyph_records_decoded);
                glyph_records_decoded.insert(k, v);
            }
        }

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
                unresolved_composite: Vec::new(),
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
    ) -> FontMetrics {
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
        let base = FontMetrics {
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
            ..FontMetrics::zero()
        };

        // Add OS/2 metrics if available
        os2_table
            .map(|os2| FontMetrics {
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
    /// * `glyph_ids` - The glyph IDs to include in the subset (glyph 0/.notdef is always included)
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
    /// This should be called during text shaping when we know both the source text and resulting
    /// glyphs
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
    /// This is useful for PDF ToUnicode CMap generation which requires single character mappings
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

#[derive(Debug, Clone, PartialEq, PartialOrd)]
struct GlyphOutlineBuilder {
    operations: Vec<GlyphOutlineOperation>,
}

impl Default for GlyphOutlineBuilder {
    fn default() -> Self {
        GlyphOutlineBuilder {
            operations: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct OwnedGlyph {
    pub bounding_box: OwnedGlyphBoundingBox,
    pub horz_advance: u16,
    pub outline: Vec<GlyphOutline>,
    // unresolved outlines, later to be added
    pub unresolved_composite: Vec<CompositeGlyphComponent>,
    pub phantom_points: Option<[Point; 4]>,
}

impl OwnedGlyph {
    pub fn from_glyph_data(glyph: &Glyph, horz_advance: u16) -> Option<Self> {
        let bbox = glyph.bounding_box()?;
        Some(Self {
            bounding_box: OwnedGlyphBoundingBox {
                max_x: bbox.x_max,
                max_y: bbox.y_max,
                min_x: bbox.x_min,
                min_y: bbox.y_min,
            },
            horz_advance,
            phantom_points: glyph.phantom_points(),
            unresolved_composite: match glyph {
                Glyph::Empty(_) => Vec::new(),
                Glyph::Composite(c) => c.glyphs.clone(),
                Glyph::Simple(s) => Vec::new(),
            },
            outline: translate_glyph_outline(glyph)
                .unwrap_or_default()
                .into_iter()
                .map(|ol| GlyphOutline {
                    operations: ol.into(),
                })
                .collect(),
        })
    }
}

/// Converts a glyph to its outline contours.
fn translate_glyph_outline(glyph: &Glyph) -> Option<GlyphOutlineContours> {
    match glyph {
        Glyph::Empty(e) => translate_empty_glyph(e),
        Glyph::Simple(sg) => translate_simple_glyph(sg),
        Glyph::Composite(cg) => translate_composite_glyph(cg),
    }
}

/// Translates an empty glyph (uses phantom points for bounds).
fn translate_empty_glyph(glyph: &EmptyGlyph) -> Option<GlyphOutlineContours> {
    let f = glyph.phantom_points?;
    Some(vec![vec![
        GlyphOutlineOperation::MoveTo(OutlineMoveTo {
            x: f[0].0,
            y: f[0].1,
        }),
        GlyphOutlineOperation::LineTo(OutlineLineTo {
            x: f[1].0,
            y: f[1].1,
        }),
        GlyphOutlineOperation::LineTo(OutlineLineTo {
            x: f[2].0,
            y: f[2].1,
        }),
        GlyphOutlineOperation::LineTo(OutlineLineTo {
            x: f[3].0,
            y: f[3].1,
        }),
        GlyphOutlineOperation::ClosePath,
    ]])
}

/// Translates a simple glyph (TrueType outlines with quadratic curves).
fn translate_simple_glyph(glyph: &SimpleGlyph) -> Option<GlyphOutlineContours> {
    let mut outlines = Vec::new();

    // Process each contour
    for contour in glyph.contours() {
        let mut operations = Vec::new();
        let contour_len = contour.len();

        if contour_len == 0 {
            continue;
        }

        // Find first on-curve point (or use first point if none exist)
        let first_on_curve_idx = contour
            .iter()
            .position(|(flag, _)| flag.is_on_curve())
            .unwrap_or(0);

        let (first_flag, first_point) = contour[first_on_curve_idx];

        // Handle special case: all points are off-curve
        if !first_flag.is_on_curve() {
            // Create an implicit on-curve point between last and first
            let last_idx = contour_len - 1;
            let (_, last_point) = contour[last_idx];
            let implicit_x = (last_point.0 + first_point.0) / 2;
            let implicit_y = (last_point.1 + first_point.1) / 2;
            operations.push(GlyphOutlineOperation::MoveTo(OutlineMoveTo {
                x: implicit_x,
                y: implicit_y,
            }));
        } else {
            operations.push(GlyphOutlineOperation::MoveTo(OutlineMoveTo {
                x: first_point.0,
                y: first_point.1,
            }));
        }

        // Process remaining points
        let mut i = 0;
        while i < contour_len {
            let curr_idx = (first_on_curve_idx + 1 + i) % contour_len;
            let (curr_flag, curr_point) = contour[curr_idx];
            let next_idx = (curr_idx + 1) % contour_len;
            let (next_flag, next_point) = contour[next_idx];

            if curr_flag.is_on_curve() {
                // Current point is on-curve, add LineTo
                operations.push(GlyphOutlineOperation::LineTo(OutlineLineTo {
                    x: curr_point.0,
                    y: curr_point.1,
                }));
                i += 1;
            } else if next_flag.is_on_curve() {
                // Current off-curve, next on-curve: QuadraticCurveTo
                operations.push(GlyphOutlineOperation::QuadraticCurveTo(OutlineQuadTo {
                    ctrl_1_x: curr_point.0,
                    ctrl_1_y: curr_point.1,
                    end_x: next_point.0,
                    end_y: next_point.1,
                }));
                i += 2; // Skip both points
            } else {
                // Both off-curve, create implicit on-curve point
                let implicit_x = (curr_point.0 + next_point.0) / 2;
                let implicit_y = (curr_point.1 + next_point.1) / 2;

                operations.push(GlyphOutlineOperation::QuadraticCurveTo(OutlineQuadTo {
                    ctrl_1_x: curr_point.0,
                    ctrl_1_y: curr_point.1,
                    end_x: implicit_x,
                    end_y: implicit_y,
                }));
                i += 1; // Only advance by one point
            }
        }

        // Close the path
        operations.push(GlyphOutlineOperation::ClosePath);
        outlines.push(operations);
    }

    Some(outlines)
}

/// Translates a composite glyph (placeholder, resolved in second pass).
fn translate_composite_glyph(glyph: &CompositeGlyph) -> Option<GlyphOutlineContours> {
    // Composite glyphs will be resolved in a second pass
    // Return a placeholder based on bounding box for now
    let bbox = glyph.bounding_box;
    Some(vec![vec![
        GlyphOutlineOperation::MoveTo(OutlineMoveTo {
            x: bbox.x_min,
            y: bbox.y_min,
        }),
        GlyphOutlineOperation::LineTo(OutlineLineTo {
            x: bbox.x_max,
            y: bbox.y_min,
        }),
        GlyphOutlineOperation::LineTo(OutlineLineTo {
            x: bbox.x_max,
            y: bbox.y_max,
        }),
        GlyphOutlineOperation::LineTo(OutlineLineTo {
            x: bbox.x_min,
            y: bbox.y_max,
        }),
        GlyphOutlineOperation::ClosePath,
    ]])
}

// Additional function to resolve composite glyphs in a second pass
pub fn resolved_glyph_components(og: &mut OwnedGlyph, all_glyphs: &BTreeMap<u16, OwnedGlyph>) {
    // TODO: does not respect attachment points or anything like this
    // only checks whether we can resolve the glyph from the map
    let mut unresolved_composites = Vec::new();
    for i in og.unresolved_composite.iter() {
        let owned_glyph = match all_glyphs.get(&i.glyph_index) {
            Some(s) => s,
            None => {
                unresolved_composites.push(i.clone());
                continue;
            }
        };
        og.outline.extend_from_slice(&owned_glyph.outline);
    }

    og.unresolved_composite = unresolved_composites;
}

fn transform_component_outlines(
    outlines: &mut Vec<Vec<GlyphOutlineOperation>>,
    scale: Option<CompositeGlyphScale>,
    arg1: CompositeGlyphArgument,
    arg2: CompositeGlyphArgument,
    offset_type: ComponentOffsets,
) {
    // Extract offset values
    let (offset_x, offset_y) = match (arg1, arg2) {
        (CompositeGlyphArgument::I16(x), CompositeGlyphArgument::I16(y)) => (x, y),
        (CompositeGlyphArgument::U16(x), CompositeGlyphArgument::U16(y)) => (x as i16, y as i16),
        (CompositeGlyphArgument::I8(x), CompositeGlyphArgument::I8(y)) => {
            (i16::from(x), i16::from(y))
        }
        (CompositeGlyphArgument::U8(x), CompositeGlyphArgument::U8(y)) => {
            (i16::from(x), i16::from(y))
        }
        _ => (0, 0), // Mismatched types, use default
    };

    // Apply transformation to each outline
    for outline in outlines {
        for op in outline.as_mut_slice() {
            match op {
                GlyphOutlineOperation::MoveTo(point) => {
                    transform_point(point, offset_x, offset_y, scale, offset_type);
                }
                GlyphOutlineOperation::LineTo(point) => {
                    transform_point_lineto(point, offset_x, offset_y, scale, offset_type);
                }
                GlyphOutlineOperation::QuadraticCurveTo(curve) => {
                    transform_quad_point(curve, offset_x, offset_y, scale, offset_type);
                }
                GlyphOutlineOperation::CubicCurveTo(curve) => {
                    transform_cubic_point(curve, offset_x, offset_y, scale, offset_type);
                }
                GlyphOutlineOperation::ClosePath => {}
            }
        }
    }
}

fn transform_point(
    point: &mut OutlineMoveTo,
    offset_x: i16,
    offset_y: i16,
    scale: Option<CompositeGlyphScale>,
    offset_type: ComponentOffsets,
) {
    // Apply scale if present
    if let Some(scale_factor) = scale {
        match scale_factor {
            CompositeGlyphScale::Scale(s) => {
                let scale = f32::from(s);
                point.x = (point.x as f32 * scale) as i16;
                point.y = (point.y as f32 * scale) as i16;
            }
            CompositeGlyphScale::XY { x_scale, y_scale } => {
                point.x = (point.x as f32 * f32::from(x_scale)) as i16;
                point.y = (point.y as f32 * f32::from(y_scale)) as i16;
            }
            CompositeGlyphScale::Matrix(matrix) => {
                let new_x = (point.x as f32 * f32::from(matrix[0][0])
                    + point.y as f32 * f32::from(matrix[0][1])) as i16;
                let new_y = (point.x as f32 * f32::from(matrix[1][0])
                    + point.y as f32 * f32::from(matrix[1][1])) as i16;
                point.x = new_x;
                point.y = new_y;
            }
        }
    }

    // Apply offset based on offset type
    match offset_type {
        ComponentOffsets::Scaled => {
            // Offset is already scaled by the transform
            point.x += offset_x;
            point.y += offset_y;
        }
        ComponentOffsets::Unscaled => {
            // Offset should be applied after scaling
            point.x += offset_x;
            point.y += offset_y;
        }
    }
}

// Implement the same transform_point function for LineTo
fn transform_point_lineto(
    point: &mut OutlineLineTo,
    offset_x: i16,
    offset_y: i16,
    scale: Option<CompositeGlyphScale>,
    offset_type: ComponentOffsets,
) {
    // Same implementation as above, just with OutlineLineTo
    // Apply scale if present
    if let Some(scale_factor) = scale {
        match scale_factor {
            CompositeGlyphScale::Scale(s) => {
                let scale = f32::from(s);
                point.x = (point.x as f32 * scale) as i16;
                point.y = (point.y as f32 * scale) as i16;
            }
            CompositeGlyphScale::XY { x_scale, y_scale } => {
                point.x = (point.x as f32 * f32::from(x_scale)) as i16;
                point.y = (point.y as f32 * f32::from(y_scale)) as i16;
            }
            CompositeGlyphScale::Matrix(matrix) => {
                let new_x = (point.x as f32 * f32::from(matrix[0][0])
                    + point.y as f32 * f32::from(matrix[0][1])) as i16;
                let new_y = (point.x as f32 * f32::from(matrix[1][0])
                    + point.y as f32 * f32::from(matrix[1][1])) as i16;
                point.x = new_x;
                point.y = new_y;
            }
        }
    }

    // Apply offset based on offset type
    match offset_type {
        ComponentOffsets::Scaled => {
            // Offset is already scaled by the transform
            point.x += offset_x;
            point.y += offset_y;
        }
        ComponentOffsets::Unscaled => {
            // Offset should be applied after scaling
            point.x += offset_x;
            point.y += offset_y;
        }
    }
}

fn transform_quad_point(
    point: &mut OutlineQuadTo,
    offset_x: i16,
    offset_y: i16,
    scale: Option<CompositeGlyphScale>,
    offset_type: ComponentOffsets,
) {
    // Apply scale if present
    if let Some(scale_factor) = scale {
        match scale_factor {
            CompositeGlyphScale::Scale(s) => {
                let scale = f32::from(s);
                point.ctrl_1_x = (point.ctrl_1_x as f32 * scale) as i16;
                point.ctrl_1_y = (point.ctrl_1_y as f32 * scale) as i16;
                point.end_x = (point.end_x as f32 * scale) as i16;
                point.end_y = (point.end_y as f32 * scale) as i16;
            }
            CompositeGlyphScale::XY { x_scale, y_scale } => {
                point.ctrl_1_x = (point.ctrl_1_x as f32 * f32::from(x_scale)) as i16;
                point.ctrl_1_y = (point.ctrl_1_y as f32 * f32::from(y_scale)) as i16;
                point.end_x = (point.end_x as f32 * f32::from(x_scale)) as i16;
                point.end_y = (point.end_y as f32 * f32::from(y_scale)) as i16;
            }
            CompositeGlyphScale::Matrix(matrix) => {
                // Transform control point
                let new_ctrl_x = (point.ctrl_1_x as f32 * f32::from(matrix[0][0])
                    + point.ctrl_1_y as f32 * f32::from(matrix[0][1]))
                    as i16;
                let new_ctrl_y = (point.ctrl_1_x as f32 * f32::from(matrix[1][0])
                    + point.ctrl_1_y as f32 * f32::from(matrix[1][1]))
                    as i16;

                // Transform end point
                let new_end_x = (point.end_x as f32 * f32::from(matrix[0][0])
                    + point.end_y as f32 * f32::from(matrix[0][1]))
                    as i16;
                let new_end_y = (point.end_x as f32 * f32::from(matrix[1][0])
                    + point.end_y as f32 * f32::from(matrix[1][1]))
                    as i16;

                point.ctrl_1_x = new_ctrl_x;
                point.ctrl_1_y = new_ctrl_y;
                point.end_x = new_end_x;
                point.end_y = new_end_y;
            }
        }
    }

    // Apply offset based on offset type
    match offset_type {
        ComponentOffsets::Scaled => {
            point.ctrl_1_x += offset_x;
            point.ctrl_1_y += offset_y;
            point.end_x += offset_x;
            point.end_y += offset_y;
        }
        ComponentOffsets::Unscaled => {
            point.ctrl_1_x += offset_x;
            point.ctrl_1_y += offset_y;
            point.end_x += offset_x;
            point.end_y += offset_y;
        }
    }
}

fn transform_cubic_point(
    point: &mut OutlineCubicTo,
    offset_x: i16,
    offset_y: i16,
    scale: Option<CompositeGlyphScale>,
    offset_type: ComponentOffsets,
) {
    // Apply scale if present
    if let Some(scale_factor) = scale {
        match scale_factor {
            CompositeGlyphScale::Scale(s) => {
                let scale = f32::from(s);
                point.ctrl_1_x = (point.ctrl_1_x as f32 * scale) as i16;
                point.ctrl_1_y = (point.ctrl_1_y as f32 * scale) as i16;
                point.ctrl_2_x = (point.ctrl_2_x as f32 * scale) as i16;
                point.ctrl_2_y = (point.ctrl_2_y as f32 * scale) as i16;
                point.end_x = (point.end_x as f32 * scale) as i16;
                point.end_y = (point.end_y as f32 * scale) as i16;
            }
            CompositeGlyphScale::XY { x_scale, y_scale } => {
                point.ctrl_1_x = (point.ctrl_1_x as f32 * f32::from(x_scale)) as i16;
                point.ctrl_1_y = (point.ctrl_1_y as f32 * f32::from(y_scale)) as i16;
                point.ctrl_2_x = (point.ctrl_2_x as f32 * f32::from(x_scale)) as i16;
                point.ctrl_2_y = (point.ctrl_2_y as f32 * f32::from(y_scale)) as i16;
                point.end_x = (point.end_x as f32 * f32::from(x_scale)) as i16;
                point.end_y = (point.end_y as f32 * f32::from(y_scale)) as i16;
            }
            CompositeGlyphScale::Matrix(matrix) => {
                // Transform first control point
                let new_ctrl1_x = (point.ctrl_1_x as f32 * f32::from(matrix[0][0])
                    + point.ctrl_1_y as f32 * f32::from(matrix[0][1]))
                    as i16;
                let new_ctrl1_y = (point.ctrl_1_x as f32 * f32::from(matrix[1][0])
                    + point.ctrl_1_y as f32 * f32::from(matrix[1][1]))
                    as i16;

                // Transform second control point
                let new_ctrl2_x = (point.ctrl_2_x as f32 * f32::from(matrix[0][0])
                    + point.ctrl_2_y as f32 * f32::from(matrix[0][1]))
                    as i16;
                let new_ctrl2_y = (point.ctrl_2_x as f32 * f32::from(matrix[1][0])
                    + point.ctrl_2_y as f32 * f32::from(matrix[1][1]))
                    as i16;

                // Transform end point
                let new_end_x = (point.end_x as f32 * f32::from(matrix[0][0])
                    + point.end_y as f32 * f32::from(matrix[0][1]))
                    as i16;
                let new_end_y = (point.end_x as f32 * f32::from(matrix[1][0])
                    + point.end_y as f32 * f32::from(matrix[1][1]))
                    as i16;

                point.ctrl_1_x = new_ctrl1_x;
                point.ctrl_1_y = new_ctrl1_y;
                point.ctrl_2_x = new_ctrl2_x;
                point.ctrl_2_y = new_ctrl2_y;
                point.end_x = new_end_x;
                point.end_y = new_end_y;
            }
        }
    }

    // Apply offset based on offset type
    match offset_type {
        ComponentOffsets::Scaled => {
            point.ctrl_1_x += offset_x;
            point.ctrl_1_y += offset_y;
            point.ctrl_2_x += offset_x;
            point.ctrl_2_y += offset_y;
            point.end_x += offset_x;
            point.end_y += offset_y;
        }
        ComponentOffsets::Unscaled => {
            point.ctrl_1_x += offset_x;
            point.ctrl_1_y += offset_y;
            point.ctrl_2_x += offset_x;
            point.ctrl_2_y += offset_y;
            point.end_x += offset_x;
            point.end_y += offset_y;
        }
    }
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
        direction: crate::font_traits::Direction,
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

    fn get_vertical_metrics(&self, glyph_id: u16) -> Option<crate::text3::cache::VerticalMetrics> {
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
