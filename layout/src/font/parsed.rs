use core::fmt;
use std::{collections::BTreeMap, sync::Arc};

use allsorts::{
    layout::{GDEFTable, LayoutCache, LayoutCacheData, GPOS, GSUB},
    tables::{
        cmap::owned::CmapSubtable as OwnedCmapSubtable,
        glyf::{
            ComponentOffsets, CompositeGlyph, CompositeGlyphArgument, CompositeGlyphComponent,
            CompositeGlyphScale, EmptyGlyph, Glyph, Point, SimpleGlyph,
        },
        kern::owned::KernTable,
        HheaTable, MaxpTable,
    },
};
use azul_core::resources::{
    GlyphOutline, GlyphOutlineOperation, OutlineCubicTo, OutlineLineTo, OutlineMoveTo,
    OutlineQuadTo, OwnedGlyphBoundingBox,
};
use azul_css::props::basic::FontMetrics;
use mock::MockFont;

use crate::text3::cache::LayoutFontMetrics;

pub type GsubCache = Arc<LayoutCacheData<GSUB>>;
pub type GposCache = Arc<LayoutCacheData<GPOS>>;

#[derive(Clone)]
pub struct ParsedFont {
    /// A hash of the font, useful for caching purposes
    pub hash: u64,
    /// Layout-specific font metrics (simplified from full FontMetrics)
    pub font_metrics: LayoutFontMetrics,
    pub num_glyphs: u16,
    pub hhea_table: HheaTable,
    pub hmtx_data: Vec<u8>,
    pub maxp_table: MaxpTable,
    pub gsub_cache: Option<GsubCache>,
    pub gpos_cache: Option<GposCache>,
    pub opt_gdef_table: Option<Arc<GDEFTable>>,
    pub opt_kern_table: Option<Arc<KernTable>>,
    pub glyph_records_decoded: BTreeMap<u16, OwnedGlyph>,
    pub space_width: Option<usize>,
    pub cmap_subtable: Option<OwnedCmapSubtable>,
    pub mock: Option<Box<MockFont>>,
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

// NOTE: FontImpl trait removed with text2 - text3 uses ParsedFontTrait
// #[cfg(feature = "text_layout")]
// impl FontImpl for ParsedFont { ... }

impl ParsedFont {
    /// Parse a font from bytes using allsorts
    ///
    /// # Arguments
    /// * `font_bytes` - The font file data
    /// * `font_index` - Index of the font in a font collection (0 for single fonts)
    /// * `parse_outlines` - Whether to parse and cache glyph outlines (expensive, skip for
    ///   layout-only)
    ///
    /// # Returns
    /// `Some(ParsedFont)` if parsing succeeds, `None` otherwise
    pub fn from_bytes(font_bytes: &[u8], font_index: usize, parse_outlines: bool) -> Option<Self> {
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
        let font_file = scope.read::<FontData<'_>>().ok()?;
        let provider = font_file.table_provider(font_index).ok()?;

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

        // Parse glyph outlines and metrics (required for rendering and layout)
        let glyph_records_decoded = if parse_outlines {
            // Full parsing: outlines + metrics
            glyf_table
                .records_mut()
                .into_iter()
                .enumerate()
                .filter_map(|(glyph_index, glyph_record)| {
                    if glyph_index > (u16::MAX as usize) {
                        return None;
                    }
                    glyph_record.parse().ok()?;
                    let glyph_index = glyph_index as u16;
                    let horz_advance = allsorts::glyph_info::advance(
                        &maxp_table,
                        &hhea_table,
                        &hmtx_data,
                        glyph_index,
                    )
                    .unwrap_or_default();
                    match glyph_record {
                        GlyfRecord::Present { .. } => None,
                        GlyfRecord::Parsed(g) => {
                            OwnedGlyph::from_glyph_data(g, horz_advance).map(|g| (glyph_index, g))
                        }
                    }
                })
                .collect::<Vec<_>>()
                .into_iter()
                .collect::<BTreeMap<_, _>>()
        } else {
            // Fallback: Parse metrics only (for layout without rendering)
            // This creates minimal OwnedGlyph records with only advance width
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
                    
                    Some((glyph_index_u16, OwnedGlyph {
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
                    }))
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

        let cmap_subtable = ReadScope::new(font_data_impl.cmap_subtable_data());
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
            num_glyphs,
            hhea_table,
            hmtx_data,
            maxp_table,
            gsub_cache,
            gpos_cache,
            opt_gdef_table,
            opt_kern_table,
            cmap_subtable,
            glyph_records_decoded,
            space_width: None,
            mock: None,
        };

        // Calculate space width
        let space_width = font.get_space_width_internal();
        font.space_width = space_width;

        Some(font)
    }

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
        self.cmap_subtable
            .as_ref()?
            .map_glyph(codepoint)
            .ok()
            .flatten()
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

    /// Get vertical metrics for a glyph (for vertical text layout)
    /// Returns None because vertical layout tables (vhea, vmtx) are not parsed yet
    pub fn get_vertical_metrics(
        &self,
        _glyph_id: u16,
    ) -> Option<crate::text3::cache::VerticalMetrics> {
        // TODO: Parse vhea and vmtx tables to support vertical text layout
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

fn translate_glyph_outline(glyph: &Glyph) -> Option<Vec<Vec<GlyphOutlineOperation>>> {
    match glyph {
        Glyph::Empty(e) => translate_empty_glyph(e),
        Glyph::Simple(sg) => translate_simple_glyph(sg),
        Glyph::Composite(cg) => translate_composite_glyph(cg),
    }
}

fn translate_empty_glyph(glyph: &EmptyGlyph) -> Option<Vec<Vec<GlyphOutlineOperation>>> {
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

fn translate_simple_glyph(glyph: &SimpleGlyph) -> Option<Vec<Vec<GlyphOutlineOperation>>> {
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

fn translate_composite_glyph(glyph: &CompositeGlyph) -> Option<Vec<Vec<GlyphOutlineOperation>>> {
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

pub mod mock {
    use alloc::collections::btree_map::BTreeMap;

    use azul_core::glyph::{Advance, GlyphInfo, GlyphOrigin, Placement, RawGlyph};

    use crate::text3::cache::LayoutFontMetrics;

    // NOTE: FontImpl removed with text2
    // #[cfg(feature = "text_layout")]
    // use super::FontImpl;

    /// A mock font implementation for testing text layout functionality without requiring real
    /// fonts
    #[derive(Debug, Clone)]
    pub struct MockFont {
        pub font_metrics: LayoutFontMetrics,
        pub space_width: Option<usize>,
        pub glyph_advances: BTreeMap<u16, u16>,
        pub glyph_sizes: BTreeMap<u16, (i32, i32)>,
        pub glyph_indices: BTreeMap<u32, u16>,
    }

    impl MockFont {
        /// Create a new MockFont with the given font metrics
        pub fn new(font_metrics: LayoutFontMetrics) -> Self {
            MockFont {
                font_metrics,
                space_width: Some(10), // Default space width
                glyph_advances: BTreeMap::new(),
                glyph_sizes: BTreeMap::new(),
                glyph_indices: BTreeMap::new(),
            }
        }

        /// Set the space width
        pub fn with_space_width(mut self, width: usize) -> Self {
            self.space_width = Some(width);
            self
        }

        /// Add a glyph advance value
        pub fn with_glyph_advance(mut self, glyph_index: u16, advance: u16) -> Self {
            self.glyph_advances.insert(glyph_index, advance);
            self
        }

        /// Add a glyph size
        pub fn with_glyph_size(mut self, glyph_index: u16, size: (i32, i32)) -> Self {
            self.glyph_sizes.insert(glyph_index, size);
            self
        }

        /// Add a Unicode code point to glyph index mapping
        pub fn with_glyph_index(mut self, unicode: u32, index: u16) -> Self {
            self.glyph_indices.insert(unicode, index);
            self
        }
    }
}
