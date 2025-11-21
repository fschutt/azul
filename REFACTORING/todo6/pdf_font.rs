use core::fmt;
use std::{
    cmp::{max, min},
    collections::{btree_map::BTreeMap, BTreeSet},
    rc::Rc,
    vec::Vec,
};

use allsorts_subset_browser::{
    binary::{
        read::{ReadArray, ReadScope},
        write::WriteBinary,
    },
    cff::CFF,
    font::GlyphTableFlags,
    font_data::FontData,
    layout::{GDEFTable, LayoutCache, GPOS, GSUB},
    outline::{OutlineBuilder, OutlineSink},
    subset::SubsetProfile,
    tables::{
        cmap::{owned::CmapSubtable as OwnedCmapSubtable, CmapSubtable},
        glyf::{GlyfRecord, GlyfTable, Glyph},
        loca::{LocaOffsets, LocaTable},
        FontTableProvider, HeadTable, HheaTable, IndexToLocFormat, MaxpTable, NameTable,
        SfntVersion,
    },
};
use base64::Engine;
use lopdf::Object::{Array, Integer};
use serde_derive::{Deserialize, Serialize};
use time::error::Parse;

use crate::{
    cmap::ToUnicodeCMap, FontId, Op, PdfPage, PdfWarnMsg, ShapedText, TextItem, TextShapingOptions,
};

/// Builtin or external font
#[derive(Debug, Clone, PartialEq)]
pub enum Font {
    /// Represents one of the 14 built-in fonts (Arial, Helvetica, etc.)
    BuiltinFont(BuiltinFont),
    /// Represents a font loaded from an external file
    ExternalFont(Parse),
}

/// Standard built-in PDF fonts
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BuiltinFont {
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
}

impl Default for BuiltinFont {
    fn default() -> Self {
        Self::TimesRoman // HTML default is serif (Times New Roman)
    }
}

include!("../defaultfonts/mapping.rs");

impl BuiltinFont {
    pub fn check_if_matches(bytes: &[u8]) -> Option<Self> {
        let matching_based_on_len = match_len(bytes)?;
        // if the length is equal, check for equality
        if bytes == matching_based_on_len.get_subset_font().bytes.as_slice() {
            Some(matching_based_on_len)
        } else {
            None
        }
    }

    /// Returns a CSS font-family string appropriate for the built-in PDF font.
    /// For example, TimesRoman maps to "Times New Roman, Times, serif".
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

    /// Returns the CSS font-weight for the built-in font.
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

    /// Returns the CSS font-style for the built-in font.
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

    /// Returns the already-subsetted font (Win-1252 codepage)
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
                        Some((*old_gid, (*new_gid, *char)))
                    } else {
                        None
                    }
                })
                .collect(),
        }
    }

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

    pub fn from_id(s: &str) -> Option<Self> {
        use self::BuiltinFont::*;
        match s {
            "Times-Roman" => Some(TimesRoman),
            "Times-Bold" => Some(TimesBold),
            "Times-Italic" => Some(TimesItalic),
            "Times-BoldItalic" => Some(TimesBoldItalic),
            "Helvetica" => Some(Helvetica),
            "Helvetica-Bold" => Some(HelveticaBold),
            "Helvetica-Oblique" => Some(HelveticaOblique),
            "Helvetica-BoldOblique" => Some(HelveticaBoldOblique),
            "Courier" => Some(Courier),
            "Courier-Oblique" => Some(CourierOblique),
            "Courier-Bold" => Some(CourierBold),
            "Courier-BoldOblique" => Some(CourierBoldOblique),
            "Symbol" => Some(Symbol),
            "ZapfDingbats" => Some(ZapfDingbats),
            _ => None,
        }
    }

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

#[derive(Clone, Default)]
pub enum FontType {
    OpenTypeCFF(Vec<u8>),
    OpenTypeCFF2,
    #[default]
    TrueType,
}

#[derive(Clone, Default)]
pub struct ParsedFont {
    pub font_metrics: FontMetrics,
    pub num_glyphs: u16,
    pub hhea_table: Option<HheaTable>,
    pub hmtx_data: Vec<u8>,
    pub vmtx_data: Vec<u8>,
    pub maxp_table: Option<MaxpTable>,
    pub gsub_cache: Option<LayoutCache<GSUB>>,
    pub gpos_cache: Option<LayoutCache<GPOS>>,
    pub opt_gdef_table: Option<Rc<GDEFTable>>,
    pub glyph_records_decoded: BTreeMap<u16, OwnedGlyph>,
    pub space_width: Option<usize>,
    pub cmap_subtable: Option<OwnedCmapSubtable>,
    pub original_bytes: Vec<u8>,
    pub original_index: usize,
    pub font_type: FontType,
    pub font_name: Option<String>,
    pub index_to_cid: BTreeMap<u16, u16>,
}

impl ParsedFont {
    ///
    /// This function performs full text shaping and layout, including:
    ///
    /// - Breaking text into words and lines
    /// - Positioning glyphs with proper kerning
    /// - Handling line breaks and wrapping
    /// - Flowing text around "holes"
    /// - Aligning text horizontally
    ///
    /// # Arguments
    ///
    /// * `self` - The font to use for shaping
    /// Shape text using azul's text3 API (legacy method - use text3_integration module instead)
    ///
    /// # Arguments
    ///
    /// * `text` - The text to shape
    /// * `options` - Text shaping and layout options
    ///
    /// # Returns
    ///
    /// A `ShapedText` containing the fully laid out text (not yet positioned!)
    #[cfg(feature = "text_layout")]
    pub fn shape_text(
        &self,
        _text: &str,
        _options: &TextShapingOptions,
        _font_id: &FontId,
    ) -> ShapedText {
        unimplemented!("Old shape_text method removed. Use text3_integration module with azul's UnifiedLayout instead.")
    }
}

pub trait PrepFont {
    fn lgi(&self, codepoint: u32) -> Option<u32>;

    fn index_to_cid(&self, index: u16) -> Option<u16>;
}

impl PrepFont for ParsedFont {
    fn lgi(&self, codepoint: u32) -> Option<u32> {
        self.lookup_glyph_index(codepoint).map(Into::into)
    }

    fn index_to_cid(&self, index: u16) -> Option<u16> {
        self.index_to_cid.get(&index).copied()
    }
}

impl PartialEq for ParsedFont {
    fn eq(&self, other: &Self) -> bool {
        self.font_metrics == other.font_metrics
            && self.num_glyphs == other.num_glyphs
            && self.hhea_table == other.hhea_table
            && self.hmtx_data == other.hmtx_data
            && self.maxp_table == other.maxp_table
            && self.space_width == other.space_width
            && self.cmap_subtable == other.cmap_subtable
            && self.original_bytes.len() == other.original_bytes.len()
    }
}

impl fmt::Debug for ParsedFont {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ParsedFont")
            .field("font_metrics", &self.font_metrics)
            .field("num_glyphs", &self.num_glyphs)
            .field("hhea_table", &self.hhea_table)
            .field("hmtx_data", &self.hmtx_data)
            .field("maxp_table", &self.maxp_table)
            .field("glyph_records_decoded", &self.glyph_records_decoded)
            .field("space_width", &self.space_width)
            .field("cmap_subtable", &self.cmap_subtable)
            .finish()
    }
}

#[derive(Debug, Clone)]
pub struct SubsetFont {
    pub bytes: Vec<u8>,
    /// mapping (old glyph ID -> subset glyph ID + original char value)
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

impl ParsedFont {
    /// Returns the glyph IDs used in the PDF file
    pub(crate) fn get_used_glyph_ids(
        &self,
        font_id: &FontId,
        pages: &[PdfPage],
    ) -> BTreeMap<u16, char> {
        let codepoints = pages
            .iter()
            .flat_map(|p| {
                p.ops.iter().filter_map(|s| match s {
                    Op::WriteCodepoints { font, cp, .. } => {
                        if font == font_id {
                            Some(cp.clone())
                        } else {
                            None
                        }
                    }
                    Op::WriteCodepointsWithKerning { font, cpk, .. } => {
                        if font == font_id {
                            Some(cpk.iter().map(|s| (s.1, s.2)).collect())
                        } else {
                            None
                        }
                    }
                    _ => None,
                })
            })
            .flatten()
            .collect::<BTreeMap<_, _>>();

        let chars = pages
            .iter()
            .flat_map(|p| {
                p.ops.iter().filter_map(|s| match s {
                    Op::WriteText { font, items, .. } => {
                        if font_id == font {
                            Some(
                                items
                                    .iter()
                                    .flat_map(|s| match s {
                                        TextItem::Text(t) => Some(t.chars()),
                                        TextItem::Offset(_) => None,
                                    })
                                    .flatten(),
                            )
                        } else {
                            None
                        }
                    }
                    _ => None,
                })
            })
            .flatten()
            .map(|x| x)
            .collect::<BTreeSet<_>>();

        if codepoints.is_empty() && chars.is_empty() {
            return BTreeMap::new(); // font added, but never used
        }

        let resolved_chars = chars
            .iter()
            .filter_map(|c| self.lookup_glyph_index(*c as u32).map(|f| (f, *c)));

        let mut map = codepoints;
        map.extend(resolved_chars);

        if let Some(sp) = self.lookup_glyph_index(' ' as u32) {
            map.insert(sp, ' ');
        }

        map.insert(0, '\0');

        map
    }

    pub fn subset_simple(&self, chars: &BTreeSet<char>) -> Result<SubsetFont, String> {
        let scope = ReadScope::new(&self.original_bytes);
        let font_file = scope.read::<FontData<'_>>().map_err(|e| e.to_string())?;
        let provider = font_file
            .table_provider(self.original_index)
            .map_err(|e| e.to_string())?;

        let p = chars
            .iter()
            .filter_map(|s| self.lookup_glyph_index(*s as u32).map(|q| (q, *s)))
            .collect::<BTreeSet<_>>();

        let glyph_mapping = p
            .iter()
            .enumerate()
            .map(|(new_glyph_id, (original_glyph_id, ch))| {
                (*original_glyph_id, (new_glyph_id as u16, *ch))
            })
            .collect::<BTreeMap<_, _>>();

        let mut gids = p.iter().map(|s| s.0).collect::<Vec<_>>();
        gids.sort();
        gids.dedup();

        let bytes = allsorts_subset_browser::subset::subset(&provider, &gids, &SubsetProfile::Web)
            .map_err(|e| e.to_string())?;

        Ok(SubsetFont {
            bytes,
            glyph_mapping,
        })
    }

    /// Generates a new font file from the used glyph IDs
    pub fn subset(&self, glyph_ids: &BTreeMap<u16, char>) -> Result<SubsetFont, String> {
        let glyph_mapping = glyph_ids
            .iter()
            .enumerate()
            .map(|(new_glyph_id, (original_glyph_id, ch))| {
                (*original_glyph_id, (new_glyph_id as u16, *ch))
            })
            .collect();

        let scope = ReadScope::new(&self.original_bytes);

        let font_file = scope.read::<FontData<'_>>().map_err(|e| e.to_string())?;

        let provider = font_file
            .table_provider(self.original_index)
            .map_err(|e| e.to_string())?;

        // https://docs.rs/allsorts/latest/allsorts/subset/fn.subset.html
        // Glyph id 0, corresponding to the .notdef glyph is always present,
        // because it is returned from get_used_glyphs_ids.
        let ids: Vec<_> = glyph_ids.keys().copied().collect();

        let font = allsorts_subset_browser::subset::subset(&provider, &ids, &SubsetProfile::Web)
            .map_err(|e| e.to_string())?;

        Ok(SubsetFont {
            bytes: font,
            glyph_mapping,
        })
    }

    /// Replace this function in the ParsedFont implementation
    pub(crate) fn generate_cmap_string(
        &self,
        font_id: &FontId,
        glyph_ids: &[(u16, char)],
    ) -> String {
        // Convert the glyph_ids map to a ToUnicodeCMap structure
        let mappings = glyph_ids
            .iter()
            .map(|&(gid, unicode)| (gid as u32, vec![unicode as u32]))
            .collect();

        // Create the CMap and generate its string representation
        let cmap = ToUnicodeCMap { mappings };
        cmap.to_cmap_string(&font_id.0)
    }

    pub(crate) fn generate_gid_to_cid_map(&self, glyph_ids: &[(u16, char)]) -> Vec<(u16, u16)> {
        glyph_ids
            .iter()
            .filter_map(|(gid, _)| self.index_to_cid(*gid).map(|cid| (*gid, cid)))
            .collect()
    }

    pub(crate) fn get_normalized_widths_ttf(&self, glyph_ids: &[(u16, char)]) -> Vec<lopdf::Object> {
        let mut widths_list = Vec::new();
        let mut current_low_gid = 0;
        let mut current_high_gid = 0;
        let mut current_width_vec = Vec::new();

        // scale the font width so that it sort-of fits into an 1000 unit square
        let percentage_font_scaling = 1000.0 / (self.font_metrics.units_per_em as f32);

        for &(gid, _) in glyph_ids {
            let width = match self.get_glyph_width_internal(gid) {
                Some(s) => s,
                None => match self.get_space_width() {
                    Some(w) => w,
                    None => 0,
                },
            };

            if gid == current_high_gid {
                // subsequent GID
                current_width_vec.push(Integer((width as f32 * percentage_font_scaling) as i64));
                current_high_gid += 1;
            } else {
                // non-subsequent GID
                widths_list.push(Integer(current_low_gid as i64));
                widths_list.push(Array(std::mem::take(&mut current_width_vec)));

                current_width_vec.push(Integer((width as f32 * percentage_font_scaling) as i64));
                current_low_gid = gid;
                current_high_gid = gid + 1;
            }
        }

        // push the last widths, because the loop is delayed by one iteration
        widths_list.push(Integer(current_low_gid as i64));
        widths_list.push(Array(std::mem::take(&mut current_width_vec)));

        widths_list
    }

    pub(crate) fn get_normalized_widths_cff(
        &self,
        gid_to_cid_map: &[(u16, u16)],
    ) -> Vec<lopdf::Object> {
        let mut widths_list = Vec::new();

        // scale the font width so that it sort-of fits into an 1000 unit square
        let percentage_font_scaling = 1000.0 / (self.font_metrics.units_per_em as f32);

        for &(gid, cid) in gid_to_cid_map {
            let width = match self.get_glyph_width_internal(gid) {
                Some(s) => s,
                None => match self.get_space_width() {
                    Some(w) => w,
                    None => 0,
                },
            };

            let width = (width as f32 * percentage_font_scaling) as i64;

            widths_list.push(Integer(cid as i64));
            widths_list.push(Integer(cid as i64));
            widths_list.push(Integer(width));
        }

        widths_list
    }

    /*
    /// Returns the maximum height in UNSCALED units of the used glyph IDs
    pub(crate) fn get_max_height(&self, glyph_ids: &BTreeMap<u16, char>) -> i64 {
        let mut max_height = 0;
        for (glyph_id, _) in glyph_ids.iter() {
            if let Some((_, glyph_height)) = self.get_glyph_size(*glyph_id) {
                max_height = max_height.max(glyph_height as i64);
            }
        }
        max_height
    }

    /// Returns the total width in UNSCALED units of the used glyph IDs
    pub(crate) fn get_total_width(&self, glyph_ids: &BTreeMap<u16, char>) -> usize {
        glyph_ids
            .keys()
            .filter_map(|s| self.get_glyph_width_internal(*s))
            .sum()
    }
    */
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C, u8)]
pub enum GlyphOutlineOperation {
    MoveTo(OutlineMoveTo),
    LineTo(OutlineLineTo),
    QuadraticCurveTo(OutlineQuadTo),
    CubicCurveTo(OutlineCubicTo),
    ClosePath,
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct OutlineMoveTo {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct OutlineLineTo {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct OutlineQuadTo {
    pub ctrl_1_x: f32,
    pub ctrl_1_y: f32,
    pub end_x: f32,
    pub end_y: f32,
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct OutlineCubicTo {
    pub ctrl_1_x: f32,
    pub ctrl_1_y: f32,
    pub ctrl_2_x: f32,
    pub ctrl_2_y: f32,
    pub end_x: f32,
    pub end_y: f32,
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct GlyphOutline {
    pub operations: Vec<GlyphOutlineOperation>,
}

/*
impl ttf_parser::OutlineBuilder for GlyphOutlineBuilder {
    fn move_to(&mut self, x: f32, y: f32) { self.operations.push(GlyphOutlineOperation::MoveTo(OutlineMoveTo { x, y })); }
    fn line_to(&mut self, x: f32, y: f32) { self.operations.push(GlyphOutlineOperation::LineTo(OutlineLineTo { x, y })); }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) { self.operations.push(GlyphOutlineOperation::QuadraticCurveTo(OutlineQuadTo { ctrl_1_x: x1, ctrl_1_y: y1, end_x: x, end_y: y })); }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) { self.operations.push(GlyphOutlineOperation::CubicCurveTo(OutlineCubicTo { ctrl_1_x: x1, ctrl_1_y: y1, ctrl_2_x: x2, ctrl_2_y: y2, end_x: x, end_y: y })); }
    fn close(&mut self) { self.operations.push(GlyphOutlineOperation::ClosePath); }
}
*/

#[derive(Debug, Clone)]
#[repr(C)]
pub struct OwnedGlyphBoundingBox {
    pub max_x: i16,
    pub max_y: i16,
    pub min_x: i16,
    pub min_y: i16,
}

impl OwnedGlyphBoundingBox {
    fn new() -> Self {
        Self {
            max_x: 0,
            max_y: 0,
            min_x: 0,
            min_y: 0,
        }
    }

    fn update(&mut self, x: f32, y: f32) {
        self.max_x = max(self.max_x, x as i16);
        self.max_y = max(self.max_y, y as i16);
        self.min_x = min(self.min_x, x as i16);
        self.min_y = min(self.min_y, y as i16);
    }
}

#[derive(Debug, Clone)]
pub struct OwnedGlyph {
    pub bounding_box: OwnedGlyphBoundingBox,
    pub horz_advance: u16,
    pub outline: Option<GlyphOutline>,
}

impl OwnedGlyph {
    fn new() -> Self {
        Self {
            bounding_box: OwnedGlyphBoundingBox::new(),
            horz_advance: 0,
            outline: None,
        }
    }

    fn from_glyph_data(glyph: &Glyph<'_>, horz_advance: u16) -> Option<Self> {
        let bbox = glyph.bounding_box()?;
        Some(Self {
            bounding_box: OwnedGlyphBoundingBox {
                max_x: bbox.x_max,
                max_y: bbox.y_max,
                min_x: bbox.x_min,
                min_y: bbox.y_min,
            },
            horz_advance,
            outline: None,
        })
    }
}

impl OutlineSink for OwnedGlyph {
    fn move_to(&mut self, to: allsorts_subset_browser::pathfinder_geometry::vector::Vector2F) {
        let op = GlyphOutlineOperation::MoveTo(OutlineMoveTo {
            x: to.x(),
            y: to.y(),
        });

        self.outline = match self.outline.clone() {
            Some(mut outline) => {
                outline.operations.push(op);
                Some(outline)
            }
            None => Some(GlyphOutline {
                operations: vec![op],
            }),
        };

        self.bounding_box.update(to.x(), to.y());
    }

    fn line_to(&mut self, to: allsorts_subset_browser::pathfinder_geometry::vector::Vector2F) {
        let op = GlyphOutlineOperation::LineTo(OutlineLineTo {
            x: to.x(),
            y: to.y(),
        });

        self.outline = match self.outline.clone() {
            Some(mut outline) => {
                outline.operations.push(op);
                Some(outline)
            }
            None => Some(GlyphOutline {
                operations: vec![op],
            }),
        };

        self.bounding_box.update(to.x(), to.y());
    }

    fn cubic_curve_to(
        &mut self,
        ctrl: allsorts_subset_browser::pathfinder_geometry::line_segment::LineSegment2F,
        to: allsorts_subset_browser::pathfinder_geometry::vector::Vector2F,
    ) {
        let op = GlyphOutlineOperation::CubicCurveTo(OutlineCubicTo {
            ctrl_1_x: ctrl.min_x(),
            ctrl_1_y: ctrl.min_y(),
            ctrl_2_x: ctrl.max_x(),
            ctrl_2_y: ctrl.max_y(),
            end_x: to.x(),
            end_y: to.y(),
        });
        self.outline = match self.outline.clone() {
            Some(mut outline) => {
                outline.operations.push(op);
                Some(outline)
            }
            None => Some(GlyphOutline {
                operations: vec![op],
            }),
        };

        self.bounding_box.update(to.x(), to.y());
    }

    fn quadratic_curve_to(
        &mut self,
        ctrl: allsorts_subset_browser::pathfinder_geometry::vector::Vector2F,
        to: allsorts_subset_browser::pathfinder_geometry::vector::Vector2F,
    ) {
        let op = GlyphOutlineOperation::QuadraticCurveTo(OutlineQuadTo {
            ctrl_1_x: ctrl.x(),
            ctrl_1_y: ctrl.y(),
            end_x: to.x(),
            end_y: to.y(),
        });

        self.outline = match self.outline.clone() {
            Some(mut outline) => {
                outline.operations.push(op);
                Some(outline)
            }
            None => Some(GlyphOutline {
                operations: vec![op],
            }),
        };

        self.bounding_box.update(to.x(), to.y());
    }

    fn close(&mut self) {}
}

impl ParsedFont {
    pub fn from_bytes(
        font_bytes: &[u8],
        font_index: usize,
        warnings: &mut Vec<PdfWarnMsg>,
    ) -> Option<Self> {
        use allsorts_subset_browser::tag;

        let scope = ReadScope::new(font_bytes);
        let font_file = match scope.read::<FontData<'_>>() {
            Ok(ff) => {
                warnings.push(PdfWarnMsg::info(
                    0,
                    0,
                    "Successfully read font data".to_string(),
                ));
                ff
            }
            Err(e) => {
                warnings.push(PdfWarnMsg::warning(
                    0,
                    0,
                    format!("Failed to read font data: {}", e),
                ));
                return None;
            }
        };

        let provider = match font_file.table_provider(font_index) {
            Ok(p) => {
                warnings.push(PdfWarnMsg::info(
                    0,
                    0,
                    format!("Successfully loaded font at index {}", font_index),
                ));
                p
            }
            Err(e) => {
                warnings.push(PdfWarnMsg::warning(
                    0,
                    0,
                    format!(
                        "Failed to get table provider for font index {}: {}",
                        font_index, e
                    ),
                ));
                return None;
            }
        };

        let font = match allsorts_subset_browser::font::Font::new(provider) {
            Ok(font) => font,
            Err(err) => {
                warnings.push(PdfWarnMsg::error(
                    0,
                    0,
                    format!("Error parsing font: {:?}", err),
                ));
                return None;
            }
        };
        let provider = &font.font_table_provider;

        let font_name = provider.table_data(tag::NAME).ok().and_then(|name_data| {
            let result = ReadScope::new(&name_data?)
                .read::<NameTable>()
                .ok()
                .and_then(|name_table| name_table.string_for_id(NameTable::POSTSCRIPT_NAME));

            result
        });

        let head_table = provider.table_data(tag::HEAD).ok().and_then(|head_data| {
            let result = ReadScope::new(&head_data?).read::<HeadTable>().ok();
            if result.is_some() {
                warnings.push(PdfWarnMsg::info(
                    0,
                    0,
                    "Successfully read HEAD table".to_string(),
                ));
            } else {
                warnings.push(PdfWarnMsg::warning(
                    0,
                    0,
                    "Failed to parse HEAD table".to_string(),
                ));
            }
            result
        });

        let maxp_table = provider
            .table_data(tag::MAXP)
            .ok()
            .and_then(|maxp_data| {
                let result = ReadScope::new(&maxp_data?).read::<MaxpTable>().ok();
                if let Some(ref table) = result {
                    warnings.push(PdfWarnMsg::info(
                        0,
                        0,
                        format!("MAXP table: {} glyphs", table.num_glyphs),
                    ));
                } else {
                    warnings.push(PdfWarnMsg::warning(
                        0,
                        0,
                        "Failed to parse MAXP table".to_string(),
                    ));
                }
                result
            })
            .unwrap_or(MaxpTable {
                num_glyphs: 0,
                version1_sub_table: None,
            });

        let index_to_loc = head_table
            .map(|s| s.index_to_loc_format)
            .unwrap_or(IndexToLocFormat::Long);
        let num_glyphs = maxp_table.num_glyphs as usize;
        warnings.push(PdfWarnMsg::info(
            0,
            0,
            format!("Font has {} glyphs", num_glyphs),
        ));

        let loca_table = provider.table_data(tag::LOCA).ok();
        let loca_table = loca_table
            .as_ref()
            .and_then(|loca_data| {
                let result = ReadScope::new(loca_data.as_ref()?)
                    .read_dep::<LocaTable<'_>>((num_glyphs, index_to_loc))
                    .ok();
                if result.is_some() {
                    warnings.push(PdfWarnMsg::info(
                        0,
                        0,
                        "Successfully read LOCA table".to_string(),
                    ));
                } else {
                    warnings.push(PdfWarnMsg::warning(
                        0,
                        0,
                        "Failed to parse LOCA table".to_string(),
                    ));
                }
                result
            })
            .unwrap_or(LocaTable {
                offsets: LocaOffsets::Long(ReadArray::empty()),
            });

        let second_scope = ReadScope::new(font_bytes);
        let second_font_file = match second_scope.read::<FontData<'_>>() {
            Ok(ff) => ff,
            Err(e) => {
                warnings.push(PdfWarnMsg::warning(
                    0,
                    0,
                    format!("Failed to read font data (second pass): {}", e),
                ));
                return None;
            }
        };

        let second_provider = match second_font_file.table_provider(font_index) {
            Ok(p) => p,
            Err(e) => {
                warnings.push(PdfWarnMsg::warning(
                    0,
                    0,
                    format!("Failed to get table provider (second pass): {}", e),
                ));
                return None;
            }
        };

        let font_data_impl = match allsorts_subset_browser::font::Font::new(second_provider) {
            Ok(fdi) => {
                warnings.push(PdfWarnMsg::info(
                    0,
                    0,
                    "Successfully created allsorts Font".to_string(),
                ));
                fdi
            }
            Err(e) => {
                warnings.push(PdfWarnMsg::warning(
                    0,
                    0,
                    format!("Failed to create allsorts Font: {}", e),
                ));
                return None;
            }
        };

        // Set placeholder values for GSUB/GPOS/GDEF which we're not using here
        let gsub_cache = None;
        let gpos_cache = None;
        let opt_gdef_table = None;
        let num_glyphs = font_data_impl.num_glyphs();

        let cmap_subtable = ReadScope::new(font_data_impl.cmap_subtable_data());
        let cmap_subtable = cmap_subtable.read::<CmapSubtable<'_>>().ok().and_then(|s| {
            let result = s.to_owned();
            if result.is_some() {
                warnings.push(PdfWarnMsg::info(
                    0,
                    0,
                    "Successfully parsed cmap subtable".to_string(),
                ));
            } else {
                warnings.push(PdfWarnMsg::warning(
                    0,
                    0,
                    "Failed to convert cmap subtable to owned version".to_string(),
                ));
            }
            result
        });

        if cmap_subtable.is_none() {
            warnings.push(PdfWarnMsg::warning(
                0,
                0,
                "Warning: no cmap subtable found in font".to_string(),
            ));
        }

        let hmtx_data = provider
            .table_data(tag::HMTX)
            .ok()
            .and_then(|s| {
                let result = Some(s?.into_owned());
                if result.is_some() {
                    warnings.push(PdfWarnMsg::info(
                        0,
                        0,
                        "Successfully read HMTX data".to_string(),
                    ));
                } else {
                    warnings.push(PdfWarnMsg::warning(
                        0,
                        0,
                        "Failed to read HMTX data".to_string(),
                    ));
                }
                result
            })
            .unwrap_or_default();

        let vmtx_data = provider
            .table_data(tag::VMTX)
            .ok()
            .and_then(|s| {
                let result = Some(s?.into_owned());
                if result.is_some() {
                    warnings.push(PdfWarnMsg::info(
                        0,
                        0,
                        "Successfully read VMTX data".to_string(),
                    ));
                } else {
                    warnings.push(PdfWarnMsg::warning(
                        0,
                        0,
                        "VMTX data not found or error reading it".to_string(),
                    ));
                }
                result
            })
            .unwrap_or_default();

        let hhea_table = provider
            .table_data(tag::HHEA)
            .ok()
            .and_then(|hhea_data| {
                let result = ReadScope::new(&hhea_data?).read::<HheaTable>().ok();
                if result.is_some() {
                    warnings.push(PdfWarnMsg::info(
                        0,
                        0,
                        "Successfully read HHEA table".to_string(),
                    ));
                } else {
                    warnings.push(PdfWarnMsg::warning(
                        0,
                        0,
                        "Failed to parse HHEA table".to_string(),
                    ));
                }
                result
            })
            .unwrap_or(HheaTable {
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
            });

        let font_metrics = FontMetrics::from_bytes(font_bytes, font_index);
        warnings.push(PdfWarnMsg::info(
            0,
            0,
            format!("Font metrics: units_per_em={}", font_metrics.units_per_em),
        ));

        let mut index_to_cid: BTreeMap<u16, u16> = BTreeMap::new();

        let font_type;
        let glyph_records_decoded = if font.glyph_table_flags.contains(GlyphTableFlags::CFF)
            && provider.sfnt_version() == tag::OTTO
        {
            let cff_table = provider.table_data(tag::CFF).ok();
            let mut cff = cff_table
                .as_ref()
                .and_then(|cff_data| {
                    let result = ReadScope::new(cff_data.as_ref()?).read_dep::<CFF>(()).ok();
                    if result.is_some() {
                        warnings.push(PdfWarnMsg::info(
                            0,
                            0,
                            "Successfully read `CFF ` table".to_string(),
                        ));
                    } else {
                        warnings.push(PdfWarnMsg::warning(
                            0,
                            0,
                            "Failed to parse `CFF ` table".to_string(),
                        ));
                        return None;
                    }
                    result
                })
                .unwrap();

            let mut decoded: Vec<(u16, OwnedGlyph)> = vec![];

            let glyph_count = font.maxp_table.num_glyphs;

            for glyph_index in 0..glyph_count {
                let mut owned_glyph = OwnedGlyph::new();

                if let Err(e) = cff.visit(glyph_index, &mut owned_glyph) {
                    warnings.push(PdfWarnMsg::warning(
                        0,
                        0,
                        format!("Failed to parse glyph {}: {}", glyph_index, e),
                    ));
                    return None;
                }
                let horz_advance = match allsorts_subset_browser::glyph_info::advance(
                    &maxp_table,
                    &hhea_table,
                    &hmtx_data,
                    glyph_index,
                ) {
                    Ok(adv) => adv,
                    Err(e) => {
                        warnings.push(PdfWarnMsg::warning(
                            0,
                            0,
                            format!("Error getting advance for glyph {}: {}", glyph_index, e),
                        ));
                        0
                    }
                };
                owned_glyph.horz_advance = horz_advance;

                decoded.push((glyph_index, owned_glyph));

                if let Some(cid) = cff.fonts[0 as usize].charset.id_for_glyph(glyph_index) {
                    index_to_cid.insert(glyph_index, cid);
                }
            }

            warnings.push(PdfWarnMsg::info(
                0,
                0,
                format!("Successfully decoded {} glyphs from CFF font", glyph_count),
            ));

            let mut buf = allsorts_subset_browser::binary::write::WriteBuffer::new();

            allsorts_subset_browser::cff::CFF::write(&mut buf, &cff).unwrap();

            font_type = FontType::OpenTypeCFF(buf.into_inner());
            decoded.into_iter().collect()
        } else if font.glyph_table_flags.contains(GlyphTableFlags::CFF2)
            && provider.sfnt_version() == tag::OTTO
        {
            warnings.push(PdfWarnMsg::error(
                0,
                0,
                format!("CFF2 font file is not supported yet"),
            ));

            return None;
        } else if font.glyph_table_flags.contains(GlyphTableFlags::GLYF) {
            // Process glyph records with detailed warnings
            let glyf_table = provider.table_data(tag::GLYF).ok();
            let mut glyf_table = glyf_table
                .as_ref()
                .and_then(|glyf_data| {
                    let result = ReadScope::new(glyf_data.as_ref()?)
                        .read_dep::<GlyfTable<'_>>(&loca_table)
                        .ok();
                    if result.is_some() {
                        warnings.push(PdfWarnMsg::info(
                            0,
                            0,
                            "Successfully read GLYF table".to_string(),
                        ));
                    } else {
                        warnings.push(PdfWarnMsg::warning(
                            0,
                            0,
                            "Failed to parse GLYF table".to_string(),
                        ));
                    }
                    result
                })
                .unwrap_or(GlyfTable::new(Vec::new()).unwrap());

            let mut glyph_count = 0;
            let decoded = glyf_table
                .records_mut()
                .iter_mut()
                .enumerate()
                .filter_map(|(glyph_index, glyph_record)| {
                    if glyph_index > (u16::MAX as usize) {
                        warnings.push(PdfWarnMsg::warning(
                            0,
                            0,
                            format!("Skipping glyph {} - exceeds u16::MAX", glyph_index),
                        ));
                        return None;
                    }

                    if let Err(e) = glyph_record.parse() {
                        warnings.push(PdfWarnMsg::warning(
                            0,
                            0,
                            format!("Failed to parse glyph {}: {}", glyph_index, e),
                        ));
                        return None;
                    }

                    let glyph_index = glyph_index as u16;
                    // Create identity map for compatibility with CFF fonts
                    index_to_cid.insert(glyph_index, glyph_index);
                    let horz_advance = match allsorts_subset_browser::glyph_info::advance(
                        &maxp_table,
                        &hhea_table,
                        &hmtx_data,
                        glyph_index,
                    ) {
                        Ok(adv) => adv,
                        Err(e) => {
                            warnings.push(PdfWarnMsg::warning(
                                0,
                                0,
                                format!("Error getting advance for glyph {}: {}", glyph_index, e),
                            ));
                            0
                        }
                    };

                    match glyph_record {
                        GlyfRecord::Present { .. } => None,
                        GlyfRecord::Parsed(g) => match OwnedGlyph::from_glyph_data(g, horz_advance)
                        {
                            Some(owned) => {
                                glyph_count += 1;
                                Some((glyph_index, owned))
                            }
                            None => {
                                warnings.push(PdfWarnMsg::warning(
                                    0,
                                    0,
                                    format!(
                                        "Failed to convert glyph {} to OwnedGlyph from GLYF font",
                                        glyph_index
                                    ),
                                ));
                                None
                            }
                        },
                    }
                })
                .collect::<Vec<_>>();

            warnings.push(PdfWarnMsg::info(
                0,
                0,
                format!("Successfully decoded {} glyphs", glyph_count),
            ));

            font_type = FontType::TrueType;

            decoded.into_iter().collect()
        } else {
            warnings.push(PdfWarnMsg::error(
                0,
                0,
                format!("Unsupported font file format"),
            ));
            return None;
        };

        let mut font = ParsedFont {
            font_metrics,
            num_glyphs,
            hhea_table: Some(hhea_table),
            hmtx_data,
            vmtx_data,
            maxp_table: Some(maxp_table),
            gsub_cache,
            gpos_cache,
            opt_gdef_table,
            cmap_subtable,
            glyph_records_decoded,
            original_bytes: font_bytes.to_vec(),
            original_index: font_index,
            space_width: None,
            font_type,
            font_name,
            index_to_cid,
        };

        let space_width = font.get_space_width_internal();
        if space_width.is_some() {
            warnings.push(PdfWarnMsg::info(
                0,
                0,
                format!("Font space width: {}", space_width.unwrap()),
            ));
        } else {
            warnings.push(PdfWarnMsg::info(
                0,
                0,
                "Font does not have a space character width".to_string(),
            ));
        }
        font.space_width = space_width;

        warnings.push(PdfWarnMsg::info(
            0,
            0,
            "Font parsing completed successfully".to_string(),
        ));
        Some(font)
    }

    // returns the space width in unscaled units
    fn get_space_width_internal(&self) -> Option<usize> {
        let glyph_index = self.lookup_glyph_index(' ' as u32)?;
        self.get_glyph_width_internal(glyph_index)
    }

    // returns the glyph width in unscaled units
    fn get_glyph_width_internal(&self, glyph_index: u16) -> Option<usize> {
        let maxp_table = self.maxp_table.as_ref()?;
        let hhea_table = self.hhea_table.as_ref()?;

        // note: pass in vmtx_data for vertical writing here
        allsorts_subset_browser::glyph_info::advance(
            &maxp_table,
            &hhea_table,
            &self.hmtx_data,
            glyph_index,
        )
        .ok()
        .map(|s| s as usize)
    }

    /// Returns the width of the space " " character (unscaled units)
    #[inline]
    pub const fn get_space_width(&self) -> Option<usize> {
        self.space_width
    }

    /// Get the horizontal advance of a glyph index (unscaled units)
    pub fn get_horizontal_advance(&self, glyph_index: u16) -> u16 {
        self.glyph_records_decoded
            .get(&glyph_index)
            .map(|gi| gi.horz_advance)
            .unwrap_or_default()
    }

    // get the x and y size of a glyph (unscaled units)
    pub fn get_glyph_size(&self, glyph_index: u16) -> Option<(i32, i32)> {
        let g = self.glyph_records_decoded.get(&glyph_index)?;
        let glyph_width = g.horz_advance as i32;
        let glyph_height = g.bounding_box.max_y as i32 - g.bounding_box.min_y as i32; // height
        Some((glyph_width, glyph_height))
    }

    pub fn lookup_glyph_index(&self, c: u32) -> Option<u16> {
        match self.cmap_subtable.as_ref()?.map_glyph(c) {
            Ok(c) => c,
            _ => None,
        }
    }
}

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
    pub panose: [u8; 10],
    pub ul_unicode_range1: u32,
    pub ul_unicode_range2: u32,
    pub ul_unicode_range3: u32,
    pub ul_unicode_range4: u32,
    pub ach_vend_id: u32,
    pub fs_selection: u16,
    pub us_first_char_index: u16,
    pub us_last_char_index: u16,

    // os/2 version 0 table
    pub s_typo_ascender: Option<i16>,
    pub s_typo_descender: Option<i16>,
    pub s_typo_line_gap: Option<i16>,
    pub us_win_ascent: Option<u16>,
    pub us_win_descent: Option<u16>,

    // os/2 version 1 table
    pub ul_code_page_range1: Option<u32>,
    pub ul_code_page_range2: Option<u32>,

    // os/2 version 2 table
    pub sx_height: Option<i16>,
    pub s_cap_height: Option<i16>,
    pub us_default_char: Option<u16>,
    pub us_break_char: Option<u16>,
    pub us_max_context: Option<u16>,

    // os/2 version 3 table
    pub us_lower_optical_point_size: Option<u16>,
    pub us_upper_optical_point_size: Option<u16>,
}

impl Default for FontMetrics {
    fn default() -> Self {
        FontMetrics::zero()
    }
}

impl FontMetrics {
    /// Only for testing, zero-sized font, will always return 0 for every metric (`units_per_em =
    /// 1000`)
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
            us_weight_class: 0,
            us_width_class: 0,
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
            panose: [0; 10],
            ul_unicode_range1: 0,
            ul_unicode_range2: 0,
            ul_unicode_range3: 0,
            ul_unicode_range4: 0,
            ach_vend_id: 0,
            fs_selection: 0,
            us_first_char_index: 0,
            us_last_char_index: 0,
            s_typo_ascender: None,
            s_typo_descender: None,
            s_typo_line_gap: None,
            us_win_ascent: None,
            us_win_descent: None,
            ul_code_page_range1: None,
            ul_code_page_range2: None,
            sx_height: None,
            s_cap_height: None,
            us_default_char: None,
            us_break_char: None,
            us_max_context: None,
            us_lower_optical_point_size: None,
            us_upper_optical_point_size: None,
        }
    }

    /// Parses `FontMetrics` from a font
    pub fn from_bytes(font_bytes: &[u8], font_index: usize) -> Self {
        #[derive(Default)]
        struct Os2Info {
            x_avg_char_width: i16,
            us_weight_class: u16,
            us_width_class: u16,
            fs_type: u16,
            y_subscript_x_size: i16,
            y_subscript_y_size: i16,
            y_subscript_x_offset: i16,
            y_subscript_y_offset: i16,
            y_superscript_x_size: i16,
            y_superscript_y_size: i16,
            y_superscript_x_offset: i16,
            y_superscript_y_offset: i16,
            y_strikeout_size: i16,
            y_strikeout_position: i16,
            s_family_class: i16,
            panose: [u8; 10],
            ul_unicode_range1: u32,
            ul_unicode_range2: u32,
            ul_unicode_range3: u32,
            ul_unicode_range4: u32,
            ach_vend_id: u32,
            fs_selection: u16,
            us_first_char_index: u16,
            us_last_char_index: u16,
            s_typo_ascender: Option<i16>,
            s_typo_descender: Option<i16>,
            s_typo_line_gap: Option<i16>,
            us_win_ascent: Option<u16>,
            us_win_descent: Option<u16>,
            ul_code_page_range1: Option<u32>,
            ul_code_page_range2: Option<u32>,
            sx_height: Option<i16>,
            s_cap_height: Option<i16>,
            us_default_char: Option<u16>,
            us_break_char: Option<u16>,
            us_max_context: Option<u16>,
            us_lower_optical_point_size: Option<u16>,
            us_upper_optical_point_size: Option<u16>,
        }

        let scope = ReadScope::new(font_bytes);
        let font_file = match scope.read::<FontData<'_>>() {
            Ok(o) => o,
            Err(_) => return FontMetrics::default(),
        };
        let provider = match font_file.table_provider(font_index) {
            Ok(o) => o,
            Err(_) => return FontMetrics::default(),
        };
        let font = match allsorts_subset_browser::font::Font::new(provider).ok() {
            Some(s) => s,
            _ => return FontMetrics::default(),
        };

        // read the HHEA table to get the metrics for horizontal layout
        let hhea_table = &font.hhea_table;
        let head_table = match font.head_table().ok() {
            Some(Some(s)) => s,
            _ => return FontMetrics::default(),
        };

        let os2_table = match font.os2_table().ok() {
            Some(Some(s)) => Os2Info {
                x_avg_char_width: s.x_avg_char_width,
                us_weight_class: s.us_weight_class,
                us_width_class: s.us_width_class,
                fs_type: s.fs_type,
                y_subscript_x_size: s.y_subscript_x_size,
                y_subscript_y_size: s.y_subscript_y_size,
                y_subscript_x_offset: s.y_subscript_x_offset,
                y_subscript_y_offset: s.y_subscript_y_offset,
                y_superscript_x_size: s.y_superscript_x_size,
                y_superscript_y_size: s.y_superscript_y_size,
                y_superscript_x_offset: s.y_superscript_x_offset,
                y_superscript_y_offset: s.y_superscript_y_offset,
                y_strikeout_size: s.y_strikeout_size,
                y_strikeout_position: s.y_strikeout_position,
                s_family_class: s.s_family_class,
                panose: s.panose,
                ul_unicode_range1: s.ul_unicode_range1,
                ul_unicode_range2: s.ul_unicode_range2,
                ul_unicode_range3: s.ul_unicode_range3,
                ul_unicode_range4: s.ul_unicode_range4,
                ach_vend_id: s.ach_vend_id,
                fs_selection: s.fs_selection.bits(),
                us_first_char_index: s.us_first_char_index,
                us_last_char_index: s.us_last_char_index,

                s_typo_ascender: s.version0.as_ref().map(|q| q.s_typo_ascender),
                s_typo_descender: s.version0.as_ref().map(|q| q.s_typo_descender),
                s_typo_line_gap: s.version0.as_ref().map(|q| q.s_typo_line_gap),
                us_win_ascent: s.version0.as_ref().map(|q| q.us_win_ascent),
                us_win_descent: s.version0.as_ref().map(|q| q.us_win_descent),

                ul_code_page_range1: s.version1.as_ref().map(|q| q.ul_code_page_range1),
                ul_code_page_range2: s.version1.as_ref().map(|q| q.ul_code_page_range2),

                sx_height: s.version2to4.as_ref().map(|q| q.sx_height),
                s_cap_height: s.version2to4.as_ref().map(|q| q.s_cap_height),
                us_default_char: s.version2to4.as_ref().map(|q| q.us_default_char),
                us_break_char: s.version2to4.as_ref().map(|q| q.us_break_char),
                us_max_context: s.version2to4.as_ref().map(|q| q.us_max_context),

                us_lower_optical_point_size: s
                    .version5
                    .as_ref()
                    .map(|q| q.us_lower_optical_point_size),
                us_upper_optical_point_size: s
                    .version5
                    .as_ref()
                    .map(|q| q.us_upper_optical_point_size),
            },
            _ => Os2Info::default(),
        };

        FontMetrics {
            // head table
            units_per_em: if head_table.units_per_em == 0 {
                1000_u16
            } else {
                head_table.units_per_em
            },
            font_flags: head_table.flags,
            x_min: head_table.x_min,
            y_min: head_table.y_min,
            x_max: head_table.x_max,
            y_max: head_table.y_max,

            // hhea table
            ascender: hhea_table.ascender,
            descender: hhea_table.descender,
            line_gap: hhea_table.line_gap,
            advance_width_max: hhea_table.advance_width_max,
            min_left_side_bearing: hhea_table.min_left_side_bearing,
            min_right_side_bearing: hhea_table.min_right_side_bearing,
            x_max_extent: hhea_table.x_max_extent,
            caret_slope_rise: hhea_table.caret_slope_rise,
            caret_slope_run: hhea_table.caret_slope_run,
            caret_offset: hhea_table.caret_offset,
            num_h_metrics: hhea_table.num_h_metrics,

            // os/2 table
            x_avg_char_width: os2_table.x_avg_char_width,
            us_weight_class: os2_table.us_weight_class,
            us_width_class: os2_table.us_width_class,
            fs_type: os2_table.fs_type,
            y_subscript_x_size: os2_table.y_subscript_x_size,
            y_subscript_y_size: os2_table.y_subscript_y_size,
            y_subscript_x_offset: os2_table.y_subscript_x_offset,
            y_subscript_y_offset: os2_table.y_subscript_y_offset,
            y_superscript_x_size: os2_table.y_superscript_x_size,
            y_superscript_y_size: os2_table.y_superscript_y_size,
            y_superscript_x_offset: os2_table.y_superscript_x_offset,
            y_superscript_y_offset: os2_table.y_superscript_y_offset,
            y_strikeout_size: os2_table.y_strikeout_size,
            y_strikeout_position: os2_table.y_strikeout_position,
            s_family_class: os2_table.s_family_class,
            panose: os2_table.panose,
            ul_unicode_range1: os2_table.ul_unicode_range1,
            ul_unicode_range2: os2_table.ul_unicode_range2,
            ul_unicode_range3: os2_table.ul_unicode_range3,
            ul_unicode_range4: os2_table.ul_unicode_range4,
            ach_vend_id: os2_table.ach_vend_id,
            fs_selection: os2_table.fs_selection,
            us_first_char_index: os2_table.us_first_char_index,
            us_last_char_index: os2_table.us_last_char_index,
            s_typo_ascender: os2_table.s_typo_ascender,
            s_typo_descender: os2_table.s_typo_descender,
            s_typo_line_gap: os2_table.s_typo_line_gap,
            us_win_ascent: os2_table.us_win_ascent,
            us_win_descent: os2_table.us_win_descent,
            ul_code_page_range1: os2_table.ul_code_page_range1,
            ul_code_page_range2: os2_table.ul_code_page_range2,
            sx_height: os2_table.sx_height,
            s_cap_height: os2_table.s_cap_height,
            us_default_char: os2_table.us_default_char,
            us_break_char: os2_table.us_break_char,
            us_max_context: os2_table.us_max_context,
            us_lower_optical_point_size: os2_table.us_lower_optical_point_size,
            us_upper_optical_point_size: os2_table.us_upper_optical_point_size,
        }
    }

    /// If set, use `OS/2.sTypoAscender - OS/2.sTypoDescender + OS/2.sTypoLineGap` to calculate the
    /// height
    ///
    /// See [`USE_TYPO_METRICS`](https://docs.microsoft.com/en-us/typography/opentype/spec/os2#fss)
    pub fn use_typo_metrics(&self) -> bool {
        self.fs_selection & (1 << 7) != 0
    }

    pub fn get_ascender_unscaled(&self) -> i16 {
        let use_typo = if !self.use_typo_metrics() {
            None
        } else {
            self.s_typo_ascender
        };
        match use_typo {
            Some(s) => s,
            None => self.ascender,
        }
    }

    /// NOTE: descender is NEGATIVE
    pub fn get_descender_unscaled(&self) -> i16 {
        let use_typo = if !self.use_typo_metrics() {
            None
        } else {
            self.s_typo_descender
        };
        match use_typo {
            Some(s) => s,
            None => self.descender,
        }
    }

    pub fn get_line_gap_unscaled(&self) -> i16 {
        let use_typo = if !self.use_typo_metrics() {
            None
        } else {
            self.s_typo_line_gap
        };
        match use_typo {
            Some(s) => s,
            None => self.line_gap,
        }
    }

    pub fn get_ascender(&self, target_font_size: f32) -> f32 {
        self.get_ascender_unscaled() as f32 / self.units_per_em as f32 * target_font_size
    }
    pub fn get_descender(&self, target_font_size: f32) -> f32 {
        self.get_descender_unscaled() as f32 / self.units_per_em as f32 * target_font_size
    }
    pub fn get_line_gap(&self, target_font_size: f32) -> f32 {
        self.get_line_gap_unscaled() as f32 / self.units_per_em as f32 * target_font_size
    }

    pub fn get_x_min(&self, target_font_size: f32) -> f32 {
        self.x_min as f32 / self.units_per_em as f32 * target_font_size
    }
    pub fn get_y_min(&self, target_font_size: f32) -> f32 {
        self.y_min as f32 / self.units_per_em as f32 * target_font_size
    }
    pub fn get_x_max(&self, target_font_size: f32) -> f32 {
        self.x_max as f32 / self.units_per_em as f32 * target_font_size
    }
    pub fn get_y_max(&self, target_font_size: f32) -> f32 {
        self.y_max as f32 / self.units_per_em as f32 * target_font_size
    }
    pub fn get_advance_width_max(&self, target_font_size: f32) -> f32 {
        self.advance_width_max as f32 / self.units_per_em as f32 * target_font_size
    }
    pub fn get_min_left_side_bearing(&self, target_font_size: f32) -> f32 {
        self.min_left_side_bearing as f32 / self.units_per_em as f32 * target_font_size
    }
    pub fn get_min_right_side_bearing(&self, target_font_size: f32) -> f32 {
        self.min_right_side_bearing as f32 / self.units_per_em as f32 * target_font_size
    }
    pub fn get_x_max_extent(&self, target_font_size: f32) -> f32 {
        self.x_max_extent as f32 / self.units_per_em as f32 * target_font_size
    }
    pub fn get_x_avg_char_width(&self, target_font_size: f32) -> f32 {
        self.x_avg_char_width as f32 / self.units_per_em as f32 * target_font_size
    }
    pub fn get_y_subscript_x_size(&self, target_font_size: f32) -> f32 {
        self.y_subscript_x_size as f32 / self.units_per_em as f32 * target_font_size
    }
    pub fn get_y_subscript_y_size(&self, target_font_size: f32) -> f32 {
        self.y_subscript_y_size as f32 / self.units_per_em as f32 * target_font_size
    }
    pub fn get_y_subscript_x_offset(&self, target_font_size: f32) -> f32 {
        self.y_subscript_x_offset as f32 / self.units_per_em as f32 * target_font_size
    }
    pub fn get_y_subscript_y_offset(&self, target_font_size: f32) -> f32 {
        self.y_subscript_y_offset as f32 / self.units_per_em as f32 * target_font_size
    }
    pub fn get_y_superscript_x_size(&self, target_font_size: f32) -> f32 {
        self.y_superscript_x_size as f32 / self.units_per_em as f32 * target_font_size
    }
    pub fn get_y_superscript_y_size(&self, target_font_size: f32) -> f32 {
        self.y_superscript_y_size as f32 / self.units_per_em as f32 * target_font_size
    }
    pub fn get_y_superscript_x_offset(&self, target_font_size: f32) -> f32 {
        self.y_superscript_x_offset as f32 / self.units_per_em as f32 * target_font_size
    }
    pub fn get_y_superscript_y_offset(&self, target_font_size: f32) -> f32 {
        self.y_superscript_y_offset as f32 / self.units_per_em as f32 * target_font_size
    }
    pub fn get_y_strikeout_size(&self, target_font_size: f32) -> f32 {
        self.y_strikeout_size as f32 / self.units_per_em as f32 * target_font_size
    }
    pub fn get_y_strikeout_position(&self, target_font_size: f32) -> f32 {
        self.y_strikeout_position as f32 / self.units_per_em as f32 * target_font_size
    }

    pub fn get_s_typo_ascender(&self, target_font_size: f32) -> Option<f32> {
        self.s_typo_ascender
            .map(|s| s as f32 / self.units_per_em as f32 * target_font_size)
    }
    pub fn get_s_typo_descender(&self, target_font_size: f32) -> Option<f32> {
        self.s_typo_descender
            .map(|s| s as f32 / self.units_per_em as f32 * target_font_size)
    }
    pub fn get_s_typo_line_gap(&self, target_font_size: f32) -> Option<f32> {
        self.s_typo_line_gap
            .map(|s| s as f32 / self.units_per_em as f32 * target_font_size)
    }
    pub fn get_us_win_ascent(&self, target_font_size: f32) -> Option<f32> {
        self.us_win_ascent
            .map(|s| s as f32 / self.units_per_em as f32 * target_font_size)
    }
    pub fn get_us_win_descent(&self, target_font_size: f32) -> Option<f32> {
        self.us_win_descent
            .map(|s| s as f32 / self.units_per_em as f32 * target_font_size)
    }
    pub fn get_sx_height(&self, target_font_size: f32) -> Option<f32> {
        self.sx_height
            .map(|s| s as f32 / self.units_per_em as f32 * target_font_size)
    }
    pub fn get_s_cap_height(&self, target_font_size: f32) -> Option<f32> {
        self.s_cap_height
            .map(|s| s as f32 / self.units_per_em as f32 * target_font_size)
    }
}

#[cfg(test)]
mod test {
    use std::collections::BTreeMap;

    use crate::*;

    const WIN_1252: &[char; 214] = &[
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

    #[test]
    fn subset_test() {
        let charmap = WIN_1252.iter().copied().collect();
        let mut target_map = vec![];

        let mut tm2 = BTreeMap::new();
        for (name, bytes) in FONTS {
            let font = ParsedFont::from_bytes(bytes, 0, &mut Vec::new()).unwrap();
            let subset = font.subset_simple(&charmap).unwrap();
            tm2.insert(name.clone(), subset.bytes.len());
            let _ = std::fs::write(
                format!(
                    "{}/defaultfonts/{}.subset.ttf",
                    env!("CARGO_MANIFEST_DIR"),
                    name.get_id()
                ),
                crate::utils::compress(&subset.bytes),
            );
            for (old_gid, (new_gid, char)) in subset.glyph_mapping.iter() {
                target_map.push(format!(
                    "    ({}, {old_gid}, {new_gid}, '{c}'),",
                    name.get_num(),
                    c = if *char == '\'' {
                        "\\'".to_string()
                    } else if *char == '\\' {
                        "\\\\".to_string()
                    } else {
                        char.to_string()
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

// Azul text3 integration - modern text shaping using azul's layout engine
#[cfg(feature = "text_layout")]
pub mod text3_integration {
    use super::*;
    use azul_layout::text3::{
        cache::{
            LayoutError, ParsedFontTrait,
            UnifiedLayout,
        },
        glyphs::{get_glyph_runs, GlyphRun},
    };

    /// Shape text using azul's text3 API
    /// 
    /// This function provides a modern interface for text shaping and layout
    /// using azul's unified text layout engine.
    pub fn shape_text_with_azul<T: ParsedFontTrait>(
        _text: &str,
        _font: &T,
        _font_size_px: f32,
        _max_width: Option<f32>,
    ) -> Result<UnifiedLayout<T>, LayoutError> {
        todo!("Implement text shaping with azul text3 API")
    }

    /// Extract glyph runs from a UnifiedLayout for PDF rendering
    pub fn extract_glyph_runs<T: ParsedFontTrait>(
        layout: &UnifiedLayout<T>,
    ) -> Vec<GlyphRun<T>> {
        get_glyph_runs(layout)
    }
}