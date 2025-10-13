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
use azul_core::app_resources::{
    GlyphOutline, GlyphOutlineOperation, OutlineCubicTo, OutlineLineTo, OutlineMoveTo,
    OutlineQuadTo, OwnedGlyphBoundingBox, ShapedTextBufferUnsized,
};
use azul_css::props::basic::FontMetrics;
use mock::MockFont;

#[cfg(feature = "text_layout")]
use crate::text2::FontImpl;

pub type GsubCache = Arc<LayoutCacheData<GSUB>>;
pub type GposCache = Arc<LayoutCacheData<GPOS>>;

#[derive(Clone)]
pub struct ParsedFont {
    /// A hash of the font, useful for caching purposes
    pub hash: u64,
    pub font_metrics: FontMetrics,
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

#[cfg(feature = "text_layout")]
impl FontImpl for ParsedFont {
    fn get_space_width(&self) -> Option<usize> {
        self.get_space_width()
    }

    fn get_horizontal_advance(&self, glyph_index: u16) -> u16 {
        self.get_horizontal_advance(glyph_index)
    }

    fn get_glyph_size(&self, glyph_index: u16) -> Option<(i32, i32)> {
        self.get_glyph_size(glyph_index)
    }

    fn shape(&self, text: &[u32], script: u32, lang: Option<u32>) -> ShapedTextBufferUnsized {
        self.shape(text, script, lang)
    }

    fn lookup_glyph_index(&self, c: u32) -> Option<u16> {
        self.lookup_glyph_index(c)
    }

    fn get_font_metrics(&self) -> &azul_css::props::basic::FontMetrics {
        &self.font_metrics
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

    use azul_core::app_resources::{
        Advance, GlyphInfo, GlyphOrigin, Placement, RawGlyph, ShapedTextBufferUnsized,
    };
    use azul_css::props::basic::FontMetrics;

    #[cfg(feature = "text_layout")]
    use super::FontImpl;

    /// A mock font implementation for testing text layout functionality without requiring real
    /// fonts
    #[derive(Debug, Clone)]
    pub struct MockFont {
        pub font_metrics: FontMetrics,
        pub space_width: Option<usize>,
        pub glyph_advances: BTreeMap<u16, u16>,
        pub glyph_sizes: BTreeMap<u16, (i32, i32)>,
        pub glyph_indices: BTreeMap<u32, u16>,
    }

    impl MockFont {
        /// Create a new MockFont with the given font metrics
        pub fn new(font_metrics: FontMetrics) -> Self {
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

    #[cfg(feature = "text_layout")]
    impl FontImpl for MockFont {
        fn get_space_width(&self) -> Option<usize> {
            self.space_width
        }

        fn get_horizontal_advance(&self, glyph_index: u16) -> u16 {
            self.glyph_advances.get(&glyph_index).copied().unwrap_or(0)
        }

        fn get_glyph_size(&self, glyph_index: u16) -> Option<(i32, i32)> {
            self.glyph_sizes.get(&glyph_index).copied()
        }

        fn shape(&self, text: &[u32], _script: u32, _lang: Option<u32>) -> ShapedTextBufferUnsized {
            // Simple implementation for testing
            let mut infos = Vec::new();

            for &ch in text {
                if let Some(glyph_index) = self.lookup_glyph_index(ch) {
                    let adv_x = self.get_horizontal_advance(glyph_index);
                    let (size_x, size_y) = self.get_glyph_size(glyph_index).unwrap_or((0, 0));

                    let glyph = RawGlyph {
                        unicode_codepoint: Some(ch).into(),
                        glyph_index,
                        liga_component_pos: 0,
                        glyph_origin: GlyphOrigin::Char(char::from_u32(ch).unwrap_or('\u{FFFD}')),
                        small_caps: false,
                        multi_subst_dup: false,
                        is_vert_alt: false,
                        fake_bold: false,
                        fake_italic: false,
                        variation: None.into(),
                    };

                    let advance = Advance {
                        advance_x: adv_x,
                        size_x,
                        size_y,
                        kerning: 0,
                    };

                    let info = GlyphInfo {
                        glyph,
                        size: advance,
                        kerning: 0,
                        placement: Placement::None,
                    };

                    infos.push(info);
                }
            }

            ShapedTextBufferUnsized { infos }
        }

        fn lookup_glyph_index(&self, c: u32) -> Option<u16> {
            self.glyph_indices.get(&c).copied()
        }

        fn get_font_metrics(&self) -> &FontMetrics {
            &self.font_metrics
        }
    }
}
