#![deny(missing_docs)]

//! `COLR` table parsing.
//!
//! <https://learn.microsoft.com/en-us/typography/opentype/spec/colr>

use std::cmp::Ordering;
use std::fmt;
use std::fmt::Write;
use std::str::FromStr;

use pathfinder_geometry::line_segment::LineSegment2F;
use pathfinder_geometry::rect::RectF;
use pathfinder_geometry::transform2d::Transform2F;
use pathfinder_geometry::vector::{vec2f, Vector2F};
use rustc_hash::FxHashSet;

use super::{F2Dot14, Fixed};
use crate::binary::{U24Be, U32Be};
use crate::outline::{OutlineBuilder, OutlineSink};
use crate::tables::cpal::{ColorRecord, Palette};
use crate::tables::variable_fonts::{DeltaSetIndexMap, ItemVariationStore};
use crate::SafeFrom;
use crate::{
    binary::{
        read::{ReadArray, ReadBinary, ReadBinaryDep, ReadCtxt, ReadFrom, ReadScope},
        U16Be,
    },
    error::ParseError,
};

/// `COLR` — Color Table.
pub struct ColrTable<'a> {
    /// COLR table version, 0 or 1 currently
    pub version: u16,
    // May be empty in COLRv1
    base_glyph_records: ReadArray<'a, BaseGlyph>,
    // May be empty in COLRv1
    layer_records: ReadArray<'a, Layer>,
    base_glyph_list: Option<BaseGlyphList<'a>>,
    layer_list: Option<LayerList<'a>>,
    clip_list: Option<ClipList<'a>>,
    /// Index map for variable COLR fonts
    pub var_index_map: Option<DeltaSetIndexMap<'a>>,
    /// Variation data for variable COLR fonts
    pub item_variation_store: Option<ItemVariationStore<'a>>,
}

/// A `COLR` table.
impl<'a, 'data> ColrTable<'data> {
    /// Lookup a color glyph in this table.
    pub fn lookup(&'a self, glyph_id: u16) -> Result<Option<ColrGlyph<'a, 'data>>, ParseError> {
        if self.version == 0 {
            self.v0_lookup(glyph_id)
        } else if self.version == 1 {
            // For applications that support COLR version 1, the application should search for a base
            // glyph ID first in the BaseGlyphList. Then, if not found, search in the baseGlyphRecords
            // array, if present.
            if let Some(list) = self.base_glyph_list.as_ref() {
                let Some(paint) = list.record(glyph_id)? else {
                    return self.v0_lookup(glyph_id);
                };
                return Ok(Some(ColrGlyph {
                    index: glyph_id,
                    table: self,
                    paint,
                }));
            } else {
                self.v0_lookup(glyph_id)
            }
        } else {
            return Err(ParseError::BadVersion);
        }
    }

    fn v0_lookup(&'a self, glyph_id: u16) -> Result<Option<ColrGlyph<'a, 'data>>, ParseError> {
        // NOTE(unwrap): Safe as search found an entry with the binary search
        let Some(base_glyph) = self
            .base_glyph_records
            .binary_search_by(|base| base.glyph_id.cmp(&glyph_id))
            .map(|index| self.base_glyph_records.get_item(index).unwrap())
            .ok()
        else {
            return Ok(None);
        };

        let table = PaintTable::Layers(PaintLayers {
            first_layer_index: base_glyph.first_layer_index,
            num_layers: base_glyph.num_layers,
        });
        let paint = Paint {
            addr: usize::from(glyph_id),
            table,
        };

        Ok(Some(ColrGlyph {
            index: glyph_id,
            table: self,
            paint,
        }))
    }

    /// Retrieve a layer from the layer list.
    pub fn layer(&self, index: u32) -> Result<Paint<'data>, ParseError> {
        let list = self.layer_list.as_ref().ok_or(ParseError::MissingValue)?;
        list.layer(index)
    }

    /// Retrieve a layer from the layer records.
    pub fn layer_record(&self, index: u16) -> Result<Layer, ParseError> {
        self.layer_records
            .get_item(usize::from(index))
            .ok_or(ParseError::BadIndex)
    }

    /// Retrieve a clip box from the clip list.
    pub fn clip_box(&self, index: u16) -> Result<Option<RectF>, ParseError> {
        let clip_box = self
            .clip_list
            .as_ref()
            .and_then(|list| list.clip_box(index).transpose())
            .transpose()?;

        if let Some(ClipBox {
            x_min,
            y_min,
            x_max,
            y_max,
            var_index_base: _,
        }) = clip_box
        {
            Ok(Some(RectF::from_points(
                vec2f(x_min.into(), y_min.into()),
                vec2f(x_max.into(), y_max.into()),
            )))
        } else {
            Ok(None)
        }
    }
}

/// A glyph from a `COLR` table.
pub struct ColrGlyph<'a, 'data> {
    /// The base glyph index of this COLR glyph.
    index: u16,
    table: &'a ColrTable<'data>,
    paint: Paint<'data>,
}

/// Trait used to traverse the paint tree of a `COLR` glyph.
///
/// A [Painter] implementation is passed to [ColrGlyph::visit] or
/// [Font::visit_colr_glyph][crate::Font::visit_colr_glyph] to traverse the paint tree
/// of a glyph.
pub trait Painter: OutlineSink {
    /// Type used to represent layers in the graphics context.
    type Layer;
    /// Error type returned from Painter methods.
    type Error;

    /// Fill the current path with the supplied color.
    fn fill(&mut self, color: Color) -> Result<(), Self::Error>;

    /// Fill the current path with a linear gradient.
    fn linear_gradient(
        &mut self,
        gradient: LinearGradient<'_>,
        palette: Palette<'_, '_>,
    ) -> Result<(), Self::Error>;

    /// Fill the current path with a radial gradient.
    fn radial_gradient(
        &mut self,
        gradient: RadialGradient<'_>,
        palette: Palette<'_, '_>,
    ) -> Result<(), Self::Error>;

    /// Fill the current path with a conic gradient.
    ///
    /// Corresponds to the PaintSweep `COLR` operator.
    fn conic_gradient(
        &mut self,
        gradient: ConicGradient<'_>,
        palette: Palette<'_, '_>,
    ) -> Result<(), Self::Error>;

    /// Establish a new clip region by intersecting the current clip region with the current path.
    fn clip(&mut self) -> Result<(), Self::Error>;

    /// Start a new path.
    fn new_path(&mut self) -> Result<(), Self::Error>;

    /// Start a new rendering layer.
    fn begin_layer(&mut self) -> Result<(), Self::Error>;

    /// End the current layer, returning it.
    fn end_layer(&mut self) -> Result<Self::Layer, Self::Error>;

    /// Compose two layers using the supplied mode.
    fn compose_layers(
        &mut self,
        backdrop: Self::Layer,
        source: Self::Layer,
        mode: CompositeMode,
    ) -> Result<(), Self::Error>;

    /// Save graphics context state.
    fn push_state(&mut self) -> Result<(), Self::Error>;

    /// Restore graphics context state previously saved with [push_state][Self::push_state].
    fn pop_state(&mut self) -> Result<(), Self::Error>;

    /// Apply the supplied affine transform to the graphics state.
    fn transform(&mut self, transform: Transform2F) -> Result<(), Self::Error>;

    /// Apply the supplied translation to the graphics state.
    fn translate(&mut self, dx: i16, dy: i16) -> Result<(), Self::Error>;

    /// Scale the graphics state by the supplied X-scale, and Y-scale values.
    ///
    /// If `center` is `Some` the scaling is performed around the supplied center point.
    fn scale(&mut self, sx: f32, sy: f32, center: Option<(i16, i16)>) -> Result<(), Self::Error>;

    /// Apply a rotating transformation to the graphics state.
    ///
    /// `angle` is in degrees. If `center` is `Some` the rotation is performed around the
    /// supplied center point.
    fn rotate(&mut self, angle: f32, center: Option<(i16, i16)>) -> Result<(), Self::Error>;

    /// Apply a skew transformation to the graphics state.
    ///
    /// `angle_x` and `angle_y` are in degrees. If `center` is `Some` the rotation is performed
    /// around the supplied center point.
    fn skew(
        &mut self,
        angle_x: f32,
        angle_y: f32,
        center: Option<(i16, i16)>,
    ) -> Result<(), Self::Error>;
}

struct PaintStack {
    stack: FxHashSet<usize>,
}

impl PaintStack {
    fn new() -> PaintStack {
        PaintStack {
            stack: FxHashSet::default(),
        }
    }
    fn push(&mut self, paint: &Paint<'_>) -> Result<(), ParseError> {
        if !self.stack.insert(paint.addr) {
            return Err(ParseError::LimitExceeded);
        }
        Ok(())
    }

    fn pop(&mut self, paint: &Paint<'_>) {
        self.stack.remove(&paint.addr);
    }
}

impl<'a, 'data> ColrGlyph<'a, 'data> {
    /// Read the clip box of this glyph from the clip list.
    ///
    /// If the `COLR` table does not supply a clip list or there is no clip box for this
    /// glyph, then `Ok(None)` is returned.
    pub fn clip_box(&self) -> Result<Option<RectF>, ParseError> {
        self.table.clip_box(self.index)
    }

    /// Traverse the paint tree of this glyph using the supplied `Painter`.
    ///
    /// Colors are supplied by `palette`, which can be obtained via
    /// [CpalTable::palette][super::cpal::CpalTable::palette].
    pub fn visit<P, G>(
        &self,
        painter: &mut P,
        glyphs: &mut G,
        palette: Palette<'a, 'data>,
    ) -> Result<(), P::Error>
    where
        P: Painter,
        P::Error: From<ParseError> + From<G::Error>,
        G: OutlineBuilder,
    {
        self.paint
            .visit(painter, glyphs, palette, self.table, &mut PaintStack::new())
    }
}

impl<'data, 'a> Paint<'data> {
    fn visit<P, G>(
        &self,
        painter: &mut P,
        glyphs: &mut G,
        palette: Palette<'a, 'data>,
        colr: &'a ColrTable<'data>,
        stack: &mut PaintStack,
    ) -> Result<(), P::Error>
    where
        P: Painter + OutlineSink,
        P::Error: From<ParseError> + From<G::Error>,
        G: OutlineBuilder,
    {
        stack.push(self)?;
        match &self.table {
            PaintTable::Layers(PaintLayers {
                num_layers,
                first_layer_index,
            }) => {
                let range = *first_layer_index
                    ..first_layer_index
                        .checked_add(*num_layers)
                        .ok_or(ParseError::LimitExceeded)?;
                for index in range {
                    let layer = colr.layer_record(index)?;
                    painter.push_state()?;

                    // Apply the outline of the referenced glyph to the clip region
                    glyphs.visit(layer.glyph_id, None, painter)?;

                    // Take the intersection of clip regions
                    painter.clip()?;

                    // Draw the layer
                    let color = palette
                        .color(layer.palette_index)
                        .map(Color::from)
                        .unwrap_or_else(|| Color(0.0, 0.0, 0.0, 0.0));
                    painter.fill(color)?;

                    // Restore the previous clip region
                    painter.pop_state()?;
                }
            }
            PaintTable::ColrLayers(PaintColrLayers {
                num_layers,
                first_layer_index,
            }) => {
                let range = *first_layer_index
                    ..first_layer_index
                        .checked_add(u32::from(*num_layers))
                        .ok_or(ParseError::LimitExceeded)?;
                for index in range {
                    let layer = colr.layer(index)?;
                    layer.visit(painter, glyphs, palette, colr, stack)?;
                }
            }
            PaintTable::Solid(paint_solid) => {
                // Fall back on transparent black if color reference is invalid
                let color = paint_solid
                    .color(palette)
                    .unwrap_or(Color(0.0, 0.0, 0.0, 0.0));
                painter.fill(color)?;
            }
            PaintTable::LinearGradient(paint_linear_gradient) => {
                let color_line = paint_linear_gradient.color_line()?;
                let gradient = LinearGradient {
                    color_line,
                    start_point: (paint_linear_gradient.x0, paint_linear_gradient.y0),
                    end_point: (paint_linear_gradient.x1, paint_linear_gradient.y1),
                    rotation_point: (paint_linear_gradient.x2, paint_linear_gradient.y2),
                };
                painter.linear_gradient(gradient, palette)?;
            }
            PaintTable::RadialGradient(paint_radial_gradient) => {
                let color_line = paint_radial_gradient.color_line()?;
                let gradient = RadialGradient {
                    color_line,
                    start_circle: Circle {
                        x: paint_radial_gradient.x0,
                        y: paint_radial_gradient.y0,
                        radius: paint_radial_gradient.radius0,
                    },
                    end_circle: Circle {
                        x: paint_radial_gradient.x1,
                        y: paint_radial_gradient.y1,
                        radius: paint_radial_gradient.radius1,
                    },
                };
                painter.radial_gradient(gradient, palette)?;
            }
            PaintTable::SweepGradient(paint_sweep_gradient) => {
                let color_line = paint_sweep_gradient.color_line()?;
                let gradient = ConicGradient {
                    color_line,
                    center: (paint_sweep_gradient.center_x, paint_sweep_gradient.center_y),
                    start_angle: paint_sweep_gradient.start_angle.into(),
                    end_angle: paint_sweep_gradient.end_angle.into(),
                };
                painter.conic_gradient(gradient, palette)?;
            }
            PaintTable::Glyph(paint_glyph) => {
                let paint = paint_glyph.subpaint()?;
                painter.push_state()?;

                // Apply the outline of the referenced glyph to the clip region
                painter.new_path()?;
                glyphs.visit(paint_glyph.glyph_id, None, painter)?;

                // Take the intersection of clip regions
                painter.clip()?;

                // Visit the paint sub-table
                paint.visit(painter, glyphs, palette, colr, stack)?;

                // Restore the previous clip region
                painter.pop_state()?;
            }
            PaintTable::ColrGlyph(paint_colr_glyph) => {
                // TODO: This is essentially a sub-routine
                // Ideally it would be possible for painters to cache the rendering of a glyph by id
                // so it can be reused
                if let Some(glyph) = colr.lookup(paint_colr_glyph.glyph_id)? {
                    glyph.paint.visit(painter, glyphs, palette, colr, stack)?;
                }
            }
            PaintTable::Transform(paint_transform) => {
                let paint = paint_transform.subpaint()?;
                let t = &paint_transform.transform;
                let transform = Transform2F::row_major(
                    t.xx.into(),
                    t.yx.into(),
                    t.xy.into(),
                    t.yy.into(),
                    t.dx.into(),
                    t.dy.into(),
                );
                self.visit_transform(&paint, painter, glyphs, palette, colr, stack, |painter| {
                    painter.transform(transform)
                })?;
            }
            PaintTable::Translate(paint_translate) => {
                let paint = paint_translate.subpaint()?;
                self.visit_transform(&paint, painter, glyphs, palette, colr, stack, |painter| {
                    painter.translate(paint_translate.dx, paint_translate.dy)
                })?;
            }
            PaintTable::Scale(paint_scale) => {
                let PaintScale {
                    scale: (sx, sy),
                    center,
                    ..
                } = paint_scale;
                let paint = paint_scale.subpaint()?;
                self.visit_transform(&paint, painter, glyphs, palette, colr, stack, |painter| {
                    painter.scale(f32::from(*sx), f32::from(*sy), *center)
                })?;
            }
            PaintTable::Rotate(paint_rotate) => {
                let paint = paint_rotate.subpaint()?;
                self.visit_transform(&paint, painter, glyphs, palette, colr, stack, |painter| {
                    painter.rotate(raw_to_degrees(paint_rotate.angle), paint_rotate.center)
                })?;
            }
            PaintTable::Skew(paint_skew) => {
                let PaintSkew {
                    skew_angle: (sx, sy),
                    center,
                    ..
                } = paint_skew;
                let paint = paint_skew.subpaint()?;
                self.visit_transform(&paint, painter, glyphs, palette, colr, stack, |painter| {
                    painter.skew(raw_to_degrees(*sx), raw_to_degrees(*sy), *center)
                })?;
            }
            PaintTable::Composite(paint_composite) => {
                let paint_backdrop = paint_composite.backdrop()?;
                let paint_source = paint_composite.source()?;

                painter.begin_layer()?;
                paint_backdrop.visit(painter, glyphs, palette, colr, stack)?;
                let backdrop = painter.end_layer()?;
                painter.begin_layer()?;
                paint_source.visit(painter, glyphs, palette, colr, stack)?;
                let source = painter.end_layer()?;
                painter.compose_layers(backdrop, source, paint_composite.composite_mode)?;
            }
        }
        stack.pop(self);

        Ok(())
    }

    fn visit_transform<F, P, G>(
        &self,
        paint: &Paint<'data>,
        painter: &mut P,
        glyphs: &mut G,
        palette: Palette<'a, 'data>,
        colr: &'a ColrTable<'data>,
        stack: &mut PaintStack,
        f: F,
    ) -> Result<(), P::Error>
    where
        P: Painter + OutlineSink,
        P::Error: From<ParseError> + From<G::Error>,
        F: FnOnce(&mut P) -> Result<(), P::Error>,
        G: OutlineBuilder,
    {
        painter.push_state()?;
        f(painter)?;
        paint.visit(painter, glyphs, palette, colr, stack)?;
        painter.pop_state()?;
        Ok(())
    }
}

/// A [Painter] implementation that prints paint operators.
///
/// ### Example
///
/// ```
/// use allsorts::binary::read::ReadScope;
/// use allsorts::tables::colr::DebugVisitor;
/// use allsorts::tables::OpenTypeFont;
/// use allsorts::Font;
/// #
/// # pub fn read_fixture<P: AsRef<std::path::Path>>(path: P) -> Vec<u8> {
/// #     std::fs::read(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(path))
/// #         .expect("error reading file contents")
/// # }
///
/// let buffer = read_fixture(
///     "tests/fonts/colr/SixtyfourConvergence-Regular-VariableFont_BLED,SCAN,XELA,YELA.ttf",
/// );
/// let otf = ReadScope::new(&buffer).read::<OpenTypeFont<'_>>().unwrap();
/// let table_provider = otf.table_provider(0).expect("error reading font file");
/// let mut font = Font::new(table_provider).unwrap();
/// let mut painter = DebugVisitor;
/// let glyph_id = 47; // 'S'
/// match font.visit_colr_glyph(glyph_id, 0, &mut painter) {
///     Ok(()) => {}
///     Err(err) => panic!("error visiting COLR glyph: {}", err),
/// }
/// ```
pub struct DebugVisitor;

impl Painter for DebugVisitor {
    type Layer = ();
    type Error = Box<dyn std::error::Error>;

    fn fill(&mut self, color: Color) -> Result<(), Self::Error> {
        println!("fill {:?}", color);
        Ok(())
    }

    fn linear_gradient(
        &mut self,
        gradient: LinearGradient<'_>,
        _palette: Palette<'_, '_>,
    ) -> Result<(), Self::Error> {
        println!("linear_gradient {:?}", gradient);
        Ok(())
    }

    fn radial_gradient(
        &mut self,
        gradient: RadialGradient<'_>,
        _palette: Palette<'_, '_>,
    ) -> Result<(), Self::Error> {
        println!("radial_gradient {:?}", gradient);
        Ok(())
    }

    fn conic_gradient(
        &mut self,
        gradient: ConicGradient<'_>,
        _palette: Palette<'_, '_>,
    ) -> Result<(), Self::Error> {
        println!("conic_gradient {:?}", gradient);
        Ok(())
    }

    fn clip(&mut self) -> Result<(), Self::Error> {
        println!("clip");
        Ok(())
    }

    fn new_path(&mut self) -> Result<(), Self::Error> {
        println!("new_path");
        Ok(())
    }

    fn begin_layer(&mut self) -> Result<(), Self::Error> {
        println!("begin_layer");
        Ok(())
    }

    fn end_layer(&mut self) -> Result<Self::Layer, Self::Error> {
        println!("end_layer");
        Ok(())
    }

    fn compose_layers(
        &mut self,
        _backdrop: Self::Layer,
        _source: Self::Layer,
        mode: CompositeMode,
    ) -> Result<(), Self::Error> {
        println!("compose_layers {:?}", mode);
        Ok(())
    }

    fn push_state(&mut self) -> Result<(), Self::Error> {
        println!("push_state");
        Ok(())
    }

    fn pop_state(&mut self) -> Result<(), Self::Error> {
        println!("pop_state");
        Ok(())
    }

    fn transform(&mut self, t: Transform2F) -> Result<(), Self::Error> {
        println!("transform {:?}", t);
        Ok(())
    }

    fn translate(&mut self, dx: i16, dy: i16) -> Result<(), Self::Error> {
        println!("translate {}, {}", dx, dy);
        Ok(())
    }

    fn scale(&mut self, sx: f32, sy: f32, center: Option<(i16, i16)>) -> Result<(), Self::Error> {
        println!("scale, {}, {} @ {:?}", sx, sy, center);
        Ok(())
    }

    fn rotate(&mut self, angle: f32, center: Option<(i16, i16)>) -> Result<(), Self::Error> {
        println!("rotate, angle {} @ {:?}", angle, center);
        Ok(())
    }

    fn skew(
        &mut self,
        angle_x: f32,
        angle_y: f32,
        center: Option<(i16, i16)>,
    ) -> Result<(), Self::Error> {
        println!(
            "skew, angle_x {}, angle_y {} @ {:?}",
            angle_x, angle_y, center
        );
        Ok(())
    }
}

impl OutlineSink for DebugVisitor {
    fn move_to(&mut self, to: Vector2F) {
        println!("move_to {:?}", to);
    }

    fn line_to(&mut self, to: Vector2F) {
        println!("line_to {:?}", to);
    }

    fn quadratic_curve_to(&mut self, ctrl: Vector2F, to: Vector2F) {
        println!("quadratic_curve_to {:?}, {:?}", ctrl, to);
    }

    fn cubic_curve_to(&mut self, ctrl: LineSegment2F, to: Vector2F) {
        println!("cubic_curve_to {:?}, {:?}", ctrl, to);
    }

    fn close(&mut self) {
        println!("close");
    }
}

impl ReadBinary for ColrTable<'_> {
    type HostType<'a> = ColrTable<'a>;

    fn read<'a>(ctxt: &mut ReadCtxt<'a>) -> Result<Self::HostType<'a>, ParseError> {
        let colr_scope = ctxt.scope();
        let version = ctxt.read_u16be()?;
        // Number of BaseGlyph records; may be 0 in a version 1 table.
        let num_base_glyph_records = ctxt.read_u16be()?;
        // Offset to baseGlyphRecords array, from beginning of COLR table (may be NULL).
        let base_glyph_records_offset = ctxt.read_u32be()?;
        // Offset to layerRecords array, from beginning of COLR table (may be NULL).
        let layer_records_offset = ctxt.read_u32be()?;
        // Number of Layer records; may be 0 in a version 1 table.
        let num_layer_records = ctxt.read_u16be()?;

        let base_glyph_records = colr_scope
            .read_optional_array(base_glyph_records_offset, num_base_glyph_records)?
            .unwrap_or_else(ReadArray::empty);
        let layer_records = colr_scope
            .read_optional_array(layer_records_offset, num_layer_records)?
            .unwrap_or_else(ReadArray::empty);

        if version == 0 {
            Ok(ColrTable {
                version,
                base_glyph_records,
                layer_records,
                base_glyph_list: None,
                layer_list: None,
                clip_list: None,
                var_index_map: None,
                item_variation_store: None,
            })
        } else if version == 1 {
            // Offset to BaseGlyphList table, from beginning of COLR table.
            let base_glyph_list_offset = ctxt.read_u32be()?;
            // Offset to LayerList table, from beginning of COLR table (may be NULL).
            let layer_list_offset = ctxt.read_u32be()?;
            // Offset to ClipList table, from beginning of COLR table (may be NULL).
            let clip_list_offset = ctxt.read_u32be()?;
            // Offset to DeltaSetIndexMap table, from beginning of COLR table (may be NULL).
            let var_index_map_offset = ctxt.read_u32be()?;
            // Offset to ItemVariationStore, from beginning of COLR table (may be NULL).
            let item_variation_store_offset = ctxt.read_u32be()?;

            let base_glyph_list = (base_glyph_list_offset != 0)
                .then(|| {
                    colr_scope
                        .offset(usize::safe_from(base_glyph_list_offset))
                        .ctxt()
                        .read::<BaseGlyphList<'_>>()
                })
                .transpose()?;

            let layer_list = (layer_list_offset != 0)
                .then(|| {
                    colr_scope
                        .offset(usize::safe_from(layer_list_offset))
                        .ctxt()
                        .read::<LayerList<'_>>()
                })
                .transpose()?;

            let clip_list = (clip_list_offset != 0)
                .then(|| {
                    colr_scope
                        .offset(usize::safe_from(clip_list_offset))
                        .ctxt()
                        .read::<ClipList<'_>>()
                })
                .transpose()?;

            let var_index_map = (var_index_map_offset != 0)
                .then(|| {
                    colr_scope
                        .offset(usize::safe_from(var_index_map_offset))
                        .ctxt()
                        .read::<DeltaSetIndexMap<'_>>()
                })
                .transpose()?;

            let item_variation_store = (item_variation_store_offset != 0)
                .then(|| {
                    colr_scope
                        .offset(usize::safe_from(item_variation_store_offset))
                        .ctxt()
                        .read::<ItemVariationStore<'_>>()
                })
                .transpose()?;

            Ok(ColrTable {
                version,
                base_glyph_records,
                layer_records,
                base_glyph_list,
                layer_list,
                clip_list,
                var_index_map,
                item_variation_store,
            })
        } else {
            Err(ParseError::BadVersion)
        }
    }
}

/// BaseGlyph record.
#[derive(Debug, Copy, Clone)]
struct BaseGlyph {
    /// Glyph ID of the base glyph.
    glyph_id: u16,
    /// Index (base 0) into the layerRecords array.
    first_layer_index: u16,
    /// Number of color layers associated with this glyph.
    num_layers: u16,
}

/// Layer record.
#[derive(Debug, Clone, Copy)]
pub struct Layer {
    /// Glyph ID of the glyph used for a given layer.
    ///
    /// The glyphID in a Layer record must be less than the numGlyphs value in the `maxp` table.
    pub glyph_id: u16,
    /// Index (base 0) for a palette entry in the `CPAL` table.
    ///
    /// The paletteIndex value must be less than the numPaletteEntries value in the `CPAL` table. A
    /// paletteIndex value of 0xFFFF is a special case, indicating that the text foreground color
    /// (as determined by the application) is to be used.
    pub palette_index: u16,
}

impl ReadFrom for BaseGlyph {
    type ReadType = (U16Be, U16Be, U16Be);

    fn read_from((glyph_id, first_layer_index, num_layers): (u16, u16, u16)) -> Self {
        BaseGlyph {
            glyph_id,
            first_layer_index,
            num_layers,
        }
    }
}

impl ReadFrom for Layer {
    type ReadType = (U16Be, U16Be);

    fn read_from((glyph_id, palette_index): (u16, u16)) -> Self {
        Layer {
            glyph_id,
            palette_index,
        }
    }
}

#[derive(Debug)]
struct BaseGlyphList<'a> {
    scope: ReadScope<'a>,
    records: ReadArray<'a, BaseGlyphPaintRecord>,
}

impl<'a> BaseGlyphList<'a> {
    pub fn record(&self, glyph_id: u16) -> Result<Option<Paint<'a>>, ParseError> {
        let Some(record_index) = self
            .records
            .binary_search_by(|record| record.glyph_id.cmp(&glyph_id))
            .ok()
        else {
            return Ok(None);
        };
        // NOTE(unwrap): Safe as binary search found item at record_index
        let record = self.records.get_item(record_index).unwrap();
        self.scope
            .offset(usize::safe_from(record.paint_offset))
            .read::<Paint<'_>>()
            .map(Some)
    }
}

impl ReadBinary for BaseGlyphList<'_> {
    type HostType<'a> = BaseGlyphList<'a>;

    fn read<'a>(ctxt: &mut ReadCtxt<'a>) -> Result<Self::HostType<'a>, ParseError> {
        let scope = ctxt.scope();
        let num_base_glyph_paint_records = ctxt.read_u32be()?;
        let base_glyph_paint_records =
            ctxt.read_array(usize::safe_from(num_base_glyph_paint_records))?;

        Ok(BaseGlyphList {
            scope,
            records: base_glyph_paint_records,
        })
    }
}

#[derive(Debug, Copy, Clone)]
struct BaseGlyphPaintRecord {
    /// Glyph ID of the base glyph.
    glyph_id: u16,
    /// Offset to a Paint table, from beginning of BaseGlyphList table.
    paint_offset: u32,
}

impl ReadFrom for BaseGlyphPaintRecord {
    type ReadType = (U16Be, U32Be);

    fn read_from((glyph_id, paint_offset): (u16, u32)) -> Self {
        BaseGlyphPaintRecord {
            glyph_id,
            paint_offset,
        }
    }
}

#[derive(Debug)]
struct LayerList<'a> {
    scope: ReadScope<'a>,
    paint_offsets: ReadArray<'a, U32Be>,
}

impl<'a> LayerList<'a> {
    pub fn layer(&self, index: u32) -> Result<Paint<'a>, ParseError> {
        let offset = self
            .paint_offsets
            .get_item(usize::safe_from(index))
            .ok_or(ParseError::BadIndex)?;
        self.scope
            .offset(usize::safe_from(offset))
            .read::<Paint<'a>>()
    }
}

impl ReadBinary for LayerList<'_> {
    type HostType<'a> = LayerList<'a>;

    fn read<'a>(ctxt: &mut ReadCtxt<'a>) -> Result<Self::HostType<'a>, ParseError> {
        let scope = ctxt.scope();
        let num_layers = ctxt.read_u32be()?;
        let paint_offsets = ctxt.read_array(usize::safe_from(num_layers))?;

        Ok(LayerList {
            scope,
            paint_offsets,
        })
    }
}

#[derive(Debug)]
struct ClipList<'a> {
    scope: ReadScope<'a>,
    /// Clip records. Sorted by startGlyphID.
    clips: ReadArray<'a, Clip>,
}

impl ClipList<'_> {
    fn clip_box(&self, glyph_id: u16) -> Result<Option<ClipBox>, ParseError> {
        let clip_index = self
            .clips
            .binary_search_by(|clip| {
                if clip.contains(glyph_id) {
                    Ordering::Equal
                } else if glyph_id < clip.start_glyph_id {
                    Ordering::Greater
                } else {
                    Ordering::Less
                }
            })
            .ok();

        let clip = match clip_index {
            // NOTE(unwrap): Safe as binary search found item at index
            Some(index) => self.clips.get_item(index).unwrap(),
            None => return Ok(None),
        };

        self.scope
            .offset(usize::safe_from(clip.clip_box_offset))
            .read::<ClipBox>()
            .map(Some)
    }
}

impl ReadBinary for ClipList<'_> {
    type HostType<'a> = ClipList<'a>;

    fn read<'a>(ctxt: &mut ReadCtxt<'a>) -> Result<Self::HostType<'a>, ParseError> {
        let scope = ctxt.scope();
        let format = ctxt.read_u8()?;
        ctxt.check_version(format == 1)?;
        let num_clips = ctxt.read_u32be()?;
        let clips = ctxt.read_array(usize::safe_from(num_clips))?;

        Ok(ClipList { scope, clips })
    }
}

/// Clip record.
#[derive(Debug, Clone, Copy)]
struct Clip {
    /// First glyph ID in the range.
    start_glyph_id: u16,
    /// Last glyph ID in the range.
    end_glyph_id: u16,
    /// Offset to a ClipBox table, from beginning of ClipList table.
    clip_box_offset: u32, // This is read from a 24-bit value
}

impl Clip {
    fn contains(&self, glyph_id: u16) -> bool {
        (self.start_glyph_id..=self.end_glyph_id).contains(&glyph_id)
    }
}

impl ReadFrom for Clip {
    type ReadType = (U16Be, U16Be, U24Be);

    fn read_from((start_glyph_id, end_glyph_id, clip_box_offset): (u16, u16, u32)) -> Self {
        Clip {
            start_glyph_id,
            end_glyph_id,
            clip_box_offset,
        }
    }
}

/// Clip box for COLR glyph.
#[derive(Debug, Clone, Copy)]
pub struct ClipBox {
    /// Minimum x of clip box.
    ///
    /// For variation, use varIndexBase + 0.
    pub x_min: i16,
    /// Minimum y of clip box.
    ///
    /// For variation, use varIndexBase + 1.
    pub y_min: i16,
    /// Maximum x of clip box.
    ///
    /// For variation, use varIndexBase + 2.
    pub x_max: i16,
    /// Maximum y of clip box.
    ///
    /// For variation, use varIndexBase + 3.
    pub y_max: i16,
    /// Base index into DeltaSetIndexMap.
    pub var_index_base: Option<u32>,
}

impl ClipBox {
    /// Obtain the width of this clip box.
    pub fn width(&self) -> i16 {
        self.x_max - self.x_min
    }

    /// Obtain the height of this clip box.
    pub fn height(&self) -> i16 {
        self.y_max - self.y_min
    }
}

impl ReadBinary for ClipBox {
    type HostType<'a> = ClipBox;

    fn read<'a>(ctxt: &mut ReadCtxt<'a>) -> Result<Self::HostType<'a>, ParseError> {
        let format = ctxt.read_u8()?;
        ctxt.check_version(format == 1 || format == 2)?;
        let x_min = ctxt.read_i16be()?;
        let y_min = ctxt.read_i16be()?;
        let x_max = ctxt.read_i16be()?;
        let y_max = ctxt.read_i16be()?;
        let var_index_base = (format == 2).then(|| ctxt.read_u32be()).transpose()?;

        Ok(ClipBox {
            x_min,
            y_min,
            x_max,
            y_max,
            var_index_base,
        })
    }
}

/// A possibly variable color stop in a gradient.
pub struct ColorStop {
    /// Position on a color line.
    ///
    /// For variation, use varIndexBase + 0.
    pub stop_offset: F2Dot14,
    /// Index for a `CPAL` palette entry.
    pub palette_index: u16,
    /// Alpha value.
    ///
    /// For variation, use varIndexBase + 1.
    pub alpha: F2Dot14,
    /// Base index into DeltaSetIndexMap if the color line is variable.
    pub var_index_base: Option<u32>,
}

impl ColorStop {
    /// The offset of this color stop along the color line.
    pub fn offset(&self) -> f32 {
        f32::from(self.stop_offset)
    }

    /// The color of this color-stop, according to the supplied palette.
    ///
    /// Obtain a [Palette] using the [palette method on CpalTable][super::cpal::CpalTable::palette].
    pub fn color(&self, palette: Palette<'_, '_>) -> Option<Color> {
        // The alpha value in the COLR structure is multiplied into the alpha value given in the
        // CPAL color entry. If the palette entry index is 0xFFFF, the alpha value in the COLR
        // structure is multiplied into the alpha value of the text foreground color.
        let color = palette.color(self.palette_index)?;
        let color = Color::new_with_alpha(color, self.alpha);
        Some(color)
    }
}

impl From<StaticColorStop> for ColorStop {
    fn from(
        StaticColorStop {
            stop_offset,
            palette_index,
            alpha,
        }: StaticColorStop,
    ) -> Self {
        ColorStop {
            stop_offset,
            palette_index,
            alpha,
            var_index_base: None,
        }
    }
}

impl From<VarColorStop> for ColorStop {
    fn from(
        VarColorStop {
            stop_offset,
            palette_index,
            alpha,
            var_index_base,
        }: VarColorStop,
    ) -> Self {
        ColorStop {
            stop_offset,
            palette_index,
            alpha,
            var_index_base: Some(var_index_base),
        }
    }
}

/// A non-variable gradient color stop.
#[derive(Debug, Clone, Copy)]
pub struct StaticColorStop {
    /// Position on a color line.
    pub stop_offset: F2Dot14,
    /// Index for a `CPAL` palette entry.
    pub palette_index: u16,
    /// Alpha value.
    pub alpha: F2Dot14,
}

impl ReadFrom for StaticColorStop {
    type ReadType = (F2Dot14, U16Be, F2Dot14);

    fn read_from((stop_offset, palette_index, alpha): (F2Dot14, u16, F2Dot14)) -> Self {
        StaticColorStop {
            stop_offset,
            palette_index,
            alpha,
        }
    }
}

/// A variable gradient color stop.
#[derive(Debug, Clone, Copy)]
pub struct VarColorStop {
    /// Position on a color line.
    ///
    /// For variation, use varIndexBase + 0.
    pub stop_offset: F2Dot14,
    /// Index for a `CPAL` palette entry.
    pub palette_index: u16,
    /// Alpha value.
    ///
    /// For variation, use varIndexBase + 1.
    pub alpha: F2Dot14,
    /// Base index into DeltaSetIndexMap.
    pub var_index_base: u32,
}

impl ReadFrom for VarColorStop {
    type ReadType = (F2Dot14, U16Be, F2Dot14, U32Be);

    fn read_from(
        (stop_offset, palette_index, alpha, var_index_base): (F2Dot14, u16, F2Dot14, u32),
    ) -> Self {
        VarColorStop {
            stop_offset,
            palette_index,
            alpha,
            var_index_base,
        }
    }
}

/// A gradient color line.
#[derive(Debug)]
pub enum ColorLine<'a> {
    /// A non-variable color line.
    Static(StaticColorLine<'a>),
    /// A variable color line.
    Variable(VarColorLine<'a>),
}

impl<'a> ColorLine<'a> {
    /// The extend mode of this color line.
    pub fn extend(&self) -> Extend {
        match self {
            ColorLine::Static(line) => line.extend,
            ColorLine::Variable(line) => line.extend,
        }
    }

    /// Iterator over the stops of this color line.
    pub fn color_stops<'b>(&'b self) -> ColorStopIter<'b, 'a> {
        ColorStopIter {
            line: self,
            index: 0,
        }
    }
}

impl<'a> From<StaticColorLine<'a>> for ColorLine<'a> {
    fn from(line: StaticColorLine<'a>) -> Self {
        ColorLine::Static(line)
    }
}

impl<'a> From<VarColorLine<'a>> for ColorLine<'a> {
    fn from(line: VarColorLine<'a>) -> Self {
        ColorLine::Variable(line)
    }
}

/// Color line iterator.
///
/// Returned from [ColorLine::color_stops]
#[derive(Copy, Clone)]
pub struct ColorStopIter<'a, 'data> {
    line: &'a ColorLine<'data>,
    index: usize,
}

impl ColorStopIter<'_, '_> {
    /// Retrieve a specific color stop on the color line.
    ///
    /// None if `index` is >= color stops length.
    pub fn get_item(&self, index: usize) -> Option<ColorStop> {
        match self.line {
            ColorLine::Static(line) => line.color_stops.get_item(index).map(ColorStop::from),
            ColorLine::Variable(line) => line.color_stops.get_item(index).map(ColorStop::from),
        }
    }
}

impl Iterator for ColorStopIter<'_, '_> {
    type Item = ColorStop;

    fn next(&mut self) -> Option<Self::Item> {
        let stop = self.get_item(self.index);
        self.index = self.index.saturating_add(1);
        stop
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = match self.line {
            ColorLine::Static(line) => line.color_stops().len() - self.index,
            ColorLine::Variable(line) => line.color_stops().len() - self.index,
        };
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for ColorStopIter<'_, '_> {}

/// A non-variable gradient color line.
#[derive(Debug, Clone)]
pub struct StaticColorLine<'a> {
    /// An Extend enum value.
    extend: Extend,
    /// ColorStop records.
    color_stops: ReadArray<'a, StaticColorStop>,
}

impl<'a> StaticColorLine<'a> {
    /// The extend mode of this color line.
    pub fn extend(&self) -> Extend {
        self.extend
    }

    /// The color stops of this color line.
    pub fn color_stops(&self) -> &ReadArray<'a, StaticColorStop> {
        &self.color_stops
    }
}

impl ReadBinary for StaticColorLine<'_> {
    type HostType<'a> = StaticColorLine<'a>;

    fn read<'a>(ctxt: &mut ReadCtxt<'a>) -> Result<Self::HostType<'a>, ParseError> {
        let extend = ctxt.read_u8()?;
        // If a ColorLine in a font has an unrecognized extend value,
        // applications should use EXTEND_PAD by default.
        let extend = Extend::try_from(extend).unwrap_or(Extend::Pad);
        let num_stops = ctxt.read_u16be()?;
        let color_stops = ctxt.read_array(usize::from(num_stops))?;
        Ok(StaticColorLine {
            extend,
            color_stops,
        })
    }
}

/// A variable gradient color line.
#[derive(Debug, Clone)]
pub struct VarColorLine<'a> {
    /// An Extend enum value.
    extend: Extend,
    /// Allows for variations.
    color_stops: ReadArray<'a, VarColorStop>,
}

impl<'a> VarColorLine<'a> {
    /// The extend mode of this color line.
    pub fn extend(&self) -> Extend {
        self.extend
    }

    /// The color stops of this color line.
    pub fn color_stops(&self) -> &ReadArray<'a, VarColorStop> {
        &self.color_stops
    }
}

impl ReadBinary for VarColorLine<'_> {
    type HostType<'a> = VarColorLine<'a>;

    fn read<'a>(ctxt: &mut ReadCtxt<'a>) -> Result<Self::HostType<'a>, ParseError> {
        let extend = ctxt.read_u8()?;
        // If a ColorLine in a font has an unrecognized extend value,
        // applications should use EXTEND_PAD by default.
        let extend = Extend::try_from(extend).unwrap_or(Extend::Pad);
        let num_stops = ctxt.read_u16be()?;
        let color_stops = ctxt.read_array(usize::from(num_stops))?;
        Ok(VarColorLine {
            extend,
            color_stops,
        })
    }
}

/// Gradient extend mode.
///
/// This defines how a gradient is extended if its color stops do not
/// cover the full 0 to 1.0 range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Extend {
    /// Use nearest color stop.
    Pad,
    /// Repeat from farthest color stop.
    Repeat,
    /// Mirror color line from nearest end.
    Reflect,
}

impl TryFrom<u8> for Extend {
    type Error = ParseError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Extend::Pad),
            1 => Ok(Extend::Repeat),
            2 => Ok(Extend::Reflect),
            _ => Err(ParseError::BadValue),
        }
    }
}

/// A COLR Paint table.
///
/// This describes a particular graphical operation.
#[derive(Debug)]
pub struct Paint<'a> {
    addr: usize,
    table: PaintTable<'a>,
}

#[derive(Debug)]
enum PaintTable<'a> {
    /// V0 layers
    Layers(PaintLayers),
    ColrLayers(PaintColrLayers),
    Solid(PaintSolid),
    LinearGradient(PaintLinearGradient<'a>),
    RadialGradient(PaintRadialGradient<'a>),
    SweepGradient(PaintSweepGradient<'a>),
    Glyph(PaintGlyph<'a>),
    ColrGlyph(PaintColrGlyph),
    Transform(PaintTransform<'a>),
    Translate(PaintTranslate<'a>),
    Scale(PaintScale<'a>),
    Rotate(PaintRotate<'a>),
    Skew(PaintSkew<'a>),
    Composite(PaintComposite<'a>),
}

macro_rules! subpaint {
    ($t:ty) => {
        impl<'data> $t {
            fn subpaint(&self) -> Result<Paint<'data>, ParseError> {
                self.scope
                    .offset(usize::safe_from(self.paint_offset))
                    .ctxt()
                    .read::<Paint<'_>>()
            }
        }
    };
}

subpaint!(PaintGlyph<'data>);
subpaint!(PaintTransform<'data>);
subpaint!(PaintTranslate<'data>);
subpaint!(PaintScale<'data>);
subpaint!(PaintRotate<'data>);
subpaint!(PaintSkew<'data>);

#[derive(Debug)]
struct PaintLayers {
    /// Index (base 0) into the layerRecords array.
    first_layer_index: u16,
    /// Number of color layers associated with this glyph.
    num_layers: u16,
}

#[derive(Debug)]
struct PaintColrLayers {
    /// Number of offsets to paint tables to read from LayerList.
    num_layers: u8,
    /// Index (base 0) into the LayerList.
    first_layer_index: u32,
}

#[derive(Debug)]
struct PaintSolid {
    /// Index for a CPAL palette entry.
    palette_index: u16,
    /// Alpha value.
    alpha: F2Dot14,
    /// Base index into DeltaSetIndexMap.
    _var_index_base: Option<u32>,
}

trait Gradient {
    fn scope(&self) -> ReadScope<'_>;

    fn color_line_offset(&self) -> u32;

    fn var_index_base(&self) -> Option<u32>;

    fn color_line(&self) -> Result<ColorLine<'_>, ParseError> {
        let mut ctxt = self
            .scope()
            .offset(usize::safe_from(self.color_line_offset()))
            .ctxt();

        if self.var_index_base().is_some() {
            ctxt.read::<VarColorLine<'_>>().map(ColorLine::from)
        } else {
            ctxt.read::<StaticColorLine<'_>>().map(ColorLine::from)
        }
    }
}

#[derive(Debug)]
struct PaintLinearGradient<'a> {
    scope: ReadScope<'a>,
    /// Offset to ColorLine table, from beginning of PaintLinearGradient table.
    color_line_offset: u32, // Offset24,
    /// Start point (p₀) x coordinate.
    x0: i16,
    /// Start point (p₀) y coordinate.
    y0: i16,
    /// End point (p₁) x coordinate.
    x1: i16,
    /// End point (p₁) y coordinate.
    y1: i16,
    /// Rotation point (p₂) x coordinate.
    x2: i16,
    /// Rotation point (p₂) y coordinate.
    y2: i16,
    /// Base index into DeltaSetIndexMap.
    var_index_base: Option<u32>,
}

/// A linear gradient, possibly rotated about a point.
#[derive(Debug)]
pub struct LinearGradient<'a> {
    /// The color line of the gradient
    pub color_line: ColorLine<'a>,
    /// Start point (p₀)
    pub start_point: (i16, i16),
    /// End point (p₁)
    pub end_point: (i16, i16),
    /// Rotation point (p₂)
    pub rotation_point: (i16, i16),
}

#[derive(Debug)]
struct PaintRadialGradient<'a> {
    scope: ReadScope<'a>,
    /// Offset to VarColorLine table, from beginning of PaintVarRadialGradient table.
    color_line_offset: u32, // Offset24,
    /// Start circle center x coordinate.
    ///
    /// For variation, use varIndexBase + 0.
    x0: i16,
    /// Start circle center y coordinate.
    ///
    /// For variation, use varIndexBase + 1.
    y0: i16,
    /// Start circle radius.
    ///
    /// For variation, use varIndexBase + 2.
    radius0: u16,
    /// End circle center x coordinate.
    ///
    /// For variation, use varIndexBase + 3.
    x1: i16,
    /// End circle center y coordinate.
    ///
    /// For variation, use varIndexBase + 4.
    y1: i16,
    /// End circle radius.
    ///
    /// For variation, use varIndexBase + 5.
    radius1: u16,
    /// Base index into DeltaSetIndexMap.
    var_index_base: Option<u32>,
}

/// A gradient of colors along a cylinder defined by two circles.
#[derive(Debug)]
pub struct RadialGradient<'a> {
    /// The color line of the gradient
    pub color_line: ColorLine<'a>,
    /// Start circle
    pub start_circle: Circle,
    /// End circle
    pub end_circle: Circle,
}

/// A constituent circle of a radial gradient.
#[derive(Debug, Copy, Clone)]
pub struct Circle {
    /// The center X coordinate of the circle
    pub x: i16,
    /// The center Y coordinate of the circle
    pub y: i16,
    /// The radius of the circle
    pub radius: u16,
}

#[derive(Debug)]
struct PaintSweepGradient<'a> {
    scope: ReadScope<'a>,
    /// Offset to VarColorLine table, from beginning of PaintVarSweepGradient table.
    color_line_offset: u32, // Offset24,
    /// Center x coordinate.
    ///
    /// For variation, use varIndexBase + 0.
    center_x: i16,
    /// Center y coordinate.
    ///
    /// For variation, use varIndexBase + 1.
    center_y: i16,
    /// Start of the angular range of the gradient: add 1.0 and multiply by 180° to retrieve counter-clockwise degrees.
    ///
    /// For variation, use varIndexBase + 2.
    start_angle: F2Dot14,
    /// End of the angular range of the gradient: add 1.0 and multiply by 180° to retrieve counter-clockwise degrees.
    ///
    /// For variation, use varIndexBase + 3.
    end_angle: F2Dot14,
    /// Base index into DeltaSetIndexMap.
    var_index_base: Option<u32>,
}

/// A conic or sweep gradient that provides a gradation of colors that sweep around a center point.
#[derive(Debug)]
pub struct ConicGradient<'a> {
    /// The color line of the gradient
    pub color_line: ColorLine<'a>,
    /// The center point of the sweep
    pub center: (i16, i16),
    /// The starting angle of the sweep (raw value)
    ///
    /// **Note:** This is the raw value from the font. Add 1.0 and multiply by 180 to get
    /// the angle in degrees.
    pub start_angle: f32,
    /// The ending angle of the sweep (raw value)
    ///
    /// **Note:** This is the raw value from the font. Add 1.0 and multiply by 180 to get
    /// the angle in degrees.
    pub end_angle: f32,
}

#[derive(Debug)]
struct PaintGlyph<'a> {
    scope: ReadScope<'a>,
    /// Offset to a Paint table, from beginning of PaintGlyph table.
    paint_offset: u32, // Offset24,
    /// Glyph ID for the source outline.
    glyph_id: u16,
}

#[derive(Debug)]
struct PaintColrGlyph {
    /// Glyph ID for a BaseGlyphList base glyph.
    glyph_id: u16,
}

#[derive(Debug)]
struct PaintTransform<'a> {
    scope: ReadScope<'a>,
    /// Offset to a Paint table, from beginning of PaintGlyph table.
    paint_offset: u32, // Offset24,
    /// Offset to an Affine2x3 table, from beginning of PaintTransform table.
    transform: Affine2x3,
}

#[derive(Debug)]
struct Affine2x3 {
    /// x-component of transformed x-basis vector.
    ///
    /// For variation, use varIndexBase + 0.
    xx: Fixed,
    /// y-component of transformed x-basis vector.
    ///
    /// For variation, use varIndexBase + 1.
    yx: Fixed,
    /// x-component of transformed y-basis vector.
    ///
    /// For variation, use varIndexBase + 2.
    xy: Fixed,
    /// y-component of transformed y-basis vector.
    ///
    /// For variation, use varIndexBase + 3.
    yy: Fixed,
    /// Translation in x direction.
    ///
    /// For variation, use varIndexBase + 4.
    dx: Fixed,
    /// Translation in y direction.
    ///
    /// For variation, use varIndexBase + 5.
    dy: Fixed,
    /// Base index into DeltaSetIndexMap.
    _var_index_base: Option<u32>,
}

#[derive(Debug)]
struct PaintTranslate<'a> {
    scope: ReadScope<'a>,
    /// Offset to a Paint subtable, from beginning of PaintVarTranslate table.
    paint_offset: u32, // Offset24,
    /// Translation in x direction.
    ///
    /// For variation, use varIndexBase + 0.
    dx: i16,
    /// Translation in y direction.
    ///
    /// For variation, use varIndexBase + 1.
    dy: i16,
    /// Base index into DeltaSetIndexMap.
    _var_index_base: Option<u32>,
}

#[derive(Debug)]
struct PaintScale<'a> {
    scope: ReadScope<'a>,
    /// Offset to a Paint subtable, from beginning of PaintVarScale table.
    paint_offset: u32, // Offset24,
    /// Scale factor in (x, y) directions.
    ///
    /// For variation, use varIndexBase + 0 for x, varIndexBase + 1 for y.
    scale: (F2Dot14, F2Dot14),
    /// Coordinates for the center of scaling (x, y).
    ///
    /// For variation, use varIndexBase + 2 for x, varIndexBase + 3 for y.
    center: Option<(i16, i16)>,
    /// Base index into DeltaSetIndexMap.
    _var_index_base: Option<u32>,
}

#[derive(Debug)]
struct PaintRotate<'a> {
    scope: ReadScope<'a>,
    /// Offset to a Paint subtable, from beginning of PaintVarRotate table.
    paint_offset: u32, // Offset24,
    /// Rotation angle, 180° in counter-clockwise degrees per 1.0 of value.
    ///
    /// For variation, use varIndexBase + 0.
    angle: F2Dot14,
    /// Coordinates for the center of rotation (x, y).
    ///
    /// For variation, use varIndexBase + 1 for x and varIndexBase + 2 for y.
    center: Option<(i16, i16)>,
    /// Base index into DeltaSetIndexMap.
    _var_index_base: Option<u32>,
}

#[derive(Debug)]
struct PaintSkew<'a> {
    scope: ReadScope<'a>,
    /// Offset to a Paint subtable, from beginning of PaintVarSkew table.
    paint_offset: u32, // Offset24,
    /// Angle of skew (x, y)
    ///
    /// 180° in counter-clockwise degrees per 1.0 of value.
    ///
    /// For variation, use varIndexBase + 0 for x-axis and varIndexBase + 1 for y-axis.
    skew_angle: (F2Dot14, F2Dot14),
    /// Coordinates for the center of rotation (x, y).
    ///
    /// For variation, use varIndexBase + 2 for x and varIndexBase + 3 for y.
    center: Option<(i16, i16)>,
    /// Base index into DeltaSetIndexMap.
    _var_index_base: Option<u32>,
}

#[derive(Debug)]
struct PaintComposite<'a> {
    scope: ReadScope<'a>,
    /// Offset to a source Paint table, from beginning of PaintComposite table.
    source_paint_offset: u32, // Offset24,
    /// A CompositeMode enumeration value.
    composite_mode: CompositeMode,
    /// Offset to a backdrop Paint table, from beginning of PaintComposite table.
    backdrop_paint_offset: u32, // Offset24,
}

/// Mode to use for compositing paint format.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum CompositeMode {
    // Porter-Duff modes
    /// Clear
    Clear,
    /// Source (“Copy” in Composition & Blending Level 1)
    Src,
    /// Destination
    Dest,
    /// Source Over
    SrcOver,
    /// Destination Over
    DestOver,
    /// Source In
    SrcIn,
    /// Destination In
    DestIn,
    /// Source Out
    SrcOut,
    /// Destination Out
    DestOut,
    /// Source Atop
    SrcAtop,
    /// Destination Atop
    DestAtop,
    /// XOR
    Xor,
    /// Plus (“Lighter” in Composition & Blending Level 1)
    Plus,
    // Separable color blend modes:
    /// screen
    Screen,
    /// overlay
    Overlay,
    /// darken
    Darken,
    /// lighten
    Lighten,
    /// color-dodge
    ColorDodge,
    /// color-burn
    ColorBurn,
    /// hard-light
    HardLight,
    /// soft-light
    SoftLight,
    /// difference
    Difference,
    /// exclusion
    Exclusion,
    /// multiply
    Multiply,
    // Non-separable color blend modes:
    /// hue
    HslHue,
    /// saturation
    HslSaturation,
    /// color
    HslColor,
    /// luminosity
    HslLuminosity,
}

impl TryFrom<u8> for CompositeMode {
    type Error = ParseError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(CompositeMode::Clear),
            1 => Ok(CompositeMode::Src),
            2 => Ok(CompositeMode::Dest),
            3 => Ok(CompositeMode::SrcOver),
            4 => Ok(CompositeMode::DestOver),
            5 => Ok(CompositeMode::SrcIn),
            6 => Ok(CompositeMode::DestIn),
            7 => Ok(CompositeMode::SrcOut),
            8 => Ok(CompositeMode::DestOut),
            9 => Ok(CompositeMode::SrcAtop),
            10 => Ok(CompositeMode::DestAtop),
            11 => Ok(CompositeMode::Xor),
            12 => Ok(CompositeMode::Plus),
            13 => Ok(CompositeMode::Screen),
            14 => Ok(CompositeMode::Overlay),
            15 => Ok(CompositeMode::Darken),
            16 => Ok(CompositeMode::Lighten),
            17 => Ok(CompositeMode::ColorDodge),
            18 => Ok(CompositeMode::ColorBurn),
            19 => Ok(CompositeMode::HardLight),
            20 => Ok(CompositeMode::SoftLight),
            21 => Ok(CompositeMode::Difference),
            22 => Ok(CompositeMode::Exclusion),
            23 => Ok(CompositeMode::Multiply),
            24 => Ok(CompositeMode::HslHue),
            25 => Ok(CompositeMode::HslSaturation),
            26 => Ok(CompositeMode::HslColor),
            27 => Ok(CompositeMode::HslLuminosity),
            _ => Err(ParseError::BadValue),
        }
    }
}

impl ReadBinary for Paint<'_> {
    type HostType<'a> = Paint<'a>;

    fn read<'a>(ctxt: &mut ReadCtxt<'a>) -> Result<Self::HostType<'a>, ParseError> {
        // Paint tables can introduce cycles because they can refer to each other in order to
        // reduce duplication. When rendering a paint table if any of its ancestors are encountered
        // again then a cycle is introduced. We need to detect this to prevent an infinite loop.
        //
        // Identifying a paint table as being the "same" poses some challenges. It's not equality
        // that matters since it would be ok for two equivalent paint tables to be encountered that
        // contain the same data _if_ they were stored separately. Additionally, the offsets used
        // to refer to paint tables are relative to different base positions so these can't be used
        // either.
        //
        // With the assumption that the paint tables are all being read from the same slice of data
        // in memory the offsets will end up resolving to addresses in that slice. An offset that
        // resolves to the same address as an existing paint table in the stack indicates that it's
        // pointing to the same paint table and thus a cycle.
        //
        // So, we track the address that each paint table originated from.
        let addr = ctxt.scope().data().as_ptr() as usize;

        // Peek the format to determine paint type to read
        let format = ctxt.scope().ctxt().read_u8()?;
        let table = match format {
            1 => PaintTable::ColrLayers(ctxt.read::<PaintColrLayers>()?),
            2 | 3 => PaintTable::Solid(ctxt.read::<PaintSolid>()?),
            4 | 5 => PaintTable::LinearGradient(ctxt.read::<PaintLinearGradient<'_>>()?),
            6 | 7 => PaintTable::RadialGradient(ctxt.read::<PaintRadialGradient<'_>>()?),
            8 | 9 => PaintTable::SweepGradient(ctxt.read::<PaintSweepGradient<'_>>()?),
            10 => PaintTable::Glyph(ctxt.read::<PaintGlyph<'_>>()?),
            11 => PaintTable::ColrGlyph(ctxt.read::<PaintColrGlyph>()?),
            12 | 13 => PaintTable::Transform(ctxt.read::<PaintTransform<'_>>()?),
            14 | 15 => PaintTable::Translate(ctxt.read::<PaintTranslate<'_>>()?),
            16 | 17 | 18 | 19 | 20 | 21 | 22 | 23 => {
                PaintTable::Scale(ctxt.read::<PaintScale<'_>>()?)
            }
            24 | 25 | 26 | 27 => PaintTable::Rotate(ctxt.read::<PaintRotate<'_>>()?),
            28 | 29 | 30 | 31 => PaintTable::Skew(ctxt.read::<PaintSkew<'_>>()?),
            32 => PaintTable::Composite(ctxt.read::<PaintComposite<'_>>()?),
            _ => return Err(ParseError::BadValue),
        };

        Ok(Paint { addr, table })
    }
}

impl ReadBinary for PaintColrLayers {
    type HostType<'a> = PaintColrLayers;

    fn read<'a>(ctxt: &mut ReadCtxt<'a>) -> Result<Self::HostType<'a>, ParseError> {
        let format = ctxt.read_u8()?;
        ctxt.check_version(format == 1)?;
        let num_layers = ctxt.read_u8()?;
        let first_layer_index = ctxt.read_u32be()?;

        Ok(PaintColrLayers {
            num_layers,
            first_layer_index,
        })
    }
}

impl PaintSolid {
    fn color(&self, palette: Palette<'_, '_>) -> Option<Color> {
        // The alpha value in the COLR structure is multiplied into the alpha value given in the
        // CPAL color entry. If the palette entry index is 0xFFFF, the alpha value in the COLR
        // structure is multiplied into the alpha value of the text foreground color.
        let color = palette.color(self.palette_index)?;
        Some(Color::new_with_alpha(color, self.alpha))
    }
}

impl ReadBinary for PaintSolid {
    type HostType<'a> = PaintSolid;

    fn read<'a>(ctxt: &mut ReadCtxt<'a>) -> Result<Self::HostType<'a>, ParseError> {
        let format = ctxt.read_u8()?;
        let palette_index = ctxt.read_u16be()?;
        let alpha = ctxt.read::<F2Dot14>()?;
        let var_index_base = match format {
            2 => None,
            3 => ctxt.read_u32be().map(Some)?,
            _ => return Err(ParseError::BadValue),
        };

        Ok(PaintSolid {
            palette_index,
            alpha,
            _var_index_base: var_index_base,
        })
    }
}

macro_rules! gradient {
    ($t:ty) => {
        impl Gradient for $t {
            fn scope(&self) -> ReadScope<'_> {
                self.scope
            }

            fn color_line_offset(&self) -> u32 {
                self.color_line_offset
            }

            fn var_index_base(&self) -> Option<u32> {
                self.var_index_base
            }
        }
    };
}

gradient!(PaintLinearGradient<'_>);
gradient!(PaintRadialGradient<'_>);
gradient!(PaintSweepGradient<'_>);

impl ReadBinary for PaintLinearGradient<'_> {
    type HostType<'a> = PaintLinearGradient<'a>;

    fn read<'a>(ctxt: &mut ReadCtxt<'a>) -> Result<Self::HostType<'a>, ParseError> {
        let scope = ctxt.scope();
        let format = ctxt.read_u8()?;
        let color_line_offset = ctxt.read::<U24Be>()?;
        let x0 = ctxt.read_i16be()?;
        let y0 = ctxt.read_i16be()?;
        let x1 = ctxt.read_i16be()?;
        let y1 = ctxt.read_i16be()?;
        let x2 = ctxt.read_i16be()?;
        let y2 = ctxt.read_i16be()?;
        let var_index_base = match format {
            4 => None,
            5 => ctxt.read_u32be().map(Some)?,
            _ => return Err(ParseError::BadValue),
        };

        Ok(PaintLinearGradient {
            scope,
            color_line_offset,
            x0,
            y0,
            x1,
            y1,
            x2,
            y2,
            var_index_base,
        })
    }
}

impl ReadBinary for PaintRadialGradient<'_> {
    type HostType<'a> = PaintRadialGradient<'a>;

    fn read<'a>(ctxt: &mut ReadCtxt<'a>) -> Result<Self::HostType<'a>, ParseError> {
        let scope = ctxt.scope();
        let format = ctxt.read_u8()?;
        let color_line_offset = ctxt.read::<U24Be>()?;
        let x0 = ctxt.read_i16be()?;
        let y0 = ctxt.read_i16be()?;
        let radius0 = ctxt.read_u16be()?;
        let x1 = ctxt.read_i16be()?;
        let y1 = ctxt.read_i16be()?;
        let radius1 = ctxt.read_u16be()?;
        let var_index_base = match format {
            6 => None,
            7 => ctxt.read_u32be().map(Some)?,
            _ => return Err(ParseError::BadValue),
        };

        Ok(PaintRadialGradient {
            scope,
            color_line_offset,
            x0,
            y0,
            radius0,
            x1,
            y1,
            radius1,
            var_index_base,
        })
    }
}

impl ReadBinary for PaintSweepGradient<'_> {
    type HostType<'a> = PaintSweepGradient<'a>;

    fn read<'a>(ctxt: &mut ReadCtxt<'a>) -> Result<Self::HostType<'a>, ParseError> {
        let scope = ctxt.scope();
        let format = ctxt.read_u8()?;
        let color_line_offset = ctxt.read::<U24Be>()?;
        let center_x = ctxt.read_i16be()?;
        let center_y = ctxt.read_i16be()?;
        let start_angle = ctxt.read::<F2Dot14>()?;
        let end_angle = ctxt.read::<F2Dot14>()?;
        let var_index_base = match format {
            8 => None,
            9 => ctxt.read_u32be().map(Some)?,
            _ => return Err(ParseError::BadValue),
        };

        Ok(PaintSweepGradient {
            scope,
            color_line_offset,
            center_x,
            center_y,
            start_angle,
            end_angle,
            var_index_base,
        })
    }
}

impl ReadBinary for PaintGlyph<'_> {
    type HostType<'a> = PaintGlyph<'a>;

    fn read<'a>(ctxt: &mut ReadCtxt<'a>) -> Result<Self::HostType<'a>, ParseError> {
        let scope = ctxt.scope();
        let format = ctxt.read_u8()?;
        ctxt.check_version(format == 10)?;
        let paint_offset = ctxt.read::<U24Be>()?;
        let glyph_id = ctxt.read_u16be()?;

        Ok(PaintGlyph {
            scope,
            paint_offset,
            glyph_id,
        })
    }
}

impl ReadBinary for PaintColrGlyph {
    type HostType<'a> = PaintColrGlyph;

    fn read<'a>(ctxt: &mut ReadCtxt<'a>) -> Result<Self::HostType<'a>, ParseError> {
        let format = ctxt.read_u8()?;
        ctxt.check_version(format == 11)?;
        let glyph_id = ctxt.read_u16be()?;

        Ok(PaintColrGlyph { glyph_id })
    }
}

impl ReadBinary for PaintTransform<'_> {
    type HostType<'a> = PaintTransform<'a>;

    fn read<'a>(ctxt: &mut ReadCtxt<'a>) -> Result<Self::HostType<'a>, ParseError> {
        let scope = ctxt.scope();
        let format = ctxt.read_u8()?;
        let paint_offset = ctxt.read::<U24Be>()?;
        let transform_offset = ctxt.read::<U24Be>().map(SafeFrom::safe_from)?;
        let variable = match format {
            12 => false,
            13 => true,
            _ => return Err(ParseError::BadValue),
        };
        let transform = scope
            .offset(transform_offset)
            .ctxt()
            .read_dep::<Affine2x3>(variable)?;

        Ok(PaintTransform {
            scope,
            paint_offset,
            transform,
        })
    }
}

impl ReadBinaryDep for Affine2x3 {
    type Args<'a> = bool;
    type HostType<'a> = Affine2x3;

    fn read_dep<'a>(
        ctxt: &mut ReadCtxt<'a>,
        variable: bool,
    ) -> Result<Self::HostType<'a>, ParseError> {
        let xx = ctxt.read::<Fixed>()?;
        let yx = ctxt.read::<Fixed>()?;
        let xy = ctxt.read::<Fixed>()?;
        let yy = ctxt.read::<Fixed>()?;
        let dx = ctxt.read::<Fixed>()?;
        let dy = ctxt.read::<Fixed>()?;
        let var_index_base = variable.then(|| ctxt.read_u32be()).transpose()?;

        Ok(Affine2x3 {
            xx,
            yx,
            xy,
            yy,
            dx,
            dy,
            _var_index_base: var_index_base,
        })
    }
}

impl ReadBinary for PaintTranslate<'_> {
    type HostType<'a> = PaintTranslate<'a>;

    fn read<'a>(ctxt: &mut ReadCtxt<'a>) -> Result<Self::HostType<'a>, ParseError> {
        let scope = ctxt.scope();
        let format = ctxt.read_u8()?;
        let paint_offset = ctxt.read::<U24Be>()?;
        let dx = ctxt.read_i16be()?;
        let dy = ctxt.read_i16be()?;
        let var_index_base = match format {
            14 => None,
            15 => ctxt.read_u32be().map(Some)?,
            _ => return Err(ParseError::BadValue),
        };

        Ok(PaintTranslate {
            scope,
            paint_offset,
            dx,
            dy,
            _var_index_base: var_index_base,
        })
    }
}

impl ReadBinary for PaintScale<'_> {
    type HostType<'a> = PaintScale<'a>;

    fn read<'a>(ctxt: &mut ReadCtxt<'a>) -> Result<Self::HostType<'a>, ParseError> {
        let scope = ctxt.scope();
        let format = ctxt.read_u8()?;
        let paint_offset = ctxt.read::<U24Be>()?;
        let (scale, center, var_index_base) = match format {
            // PaintScale and PaintVarScale
            16 | 17 => {
                let scale_x = ctxt.read::<F2Dot14>()?;
                let scale_y = ctxt.read::<F2Dot14>()?;
                let var_index_base = (format == 17).then(|| ctxt.read_u32be()).transpose()?;
                ((scale_x, scale_y), None, var_index_base)
            }
            // PaintScaleAroundCenter and PaintVarScaleAroundCenter
            18 | 19 => {
                let scale_x = ctxt.read::<F2Dot14>()?;
                let scale_y = ctxt.read::<F2Dot14>()?;
                let center_x = ctxt.read_i16be()?;
                let center_y = ctxt.read_i16be()?;
                let var_index_base = (format == 19).then(|| ctxt.read_u32be()).transpose()?;
                (
                    (scale_x, scale_y),
                    Some((center_x, center_y)),
                    var_index_base,
                )
            }
            // PaintScaleUniform and PaintVarScaleUniform
            20 | 21 => {
                let scale = ctxt.read::<F2Dot14>()?;
                let var_index_base = (format == 21).then(|| ctxt.read_u32be()).transpose()?;
                ((scale, scale), None, var_index_base)
            }
            // PaintScaleUniformAroundCenter and PaintVarScaleUniformAroundCenter
            22 | 23 => {
                let scale = ctxt.read::<F2Dot14>()?;
                let center_x = ctxt.read_i16be()?;
                let center_y = ctxt.read_i16be()?;
                let var_index_base = (format == 23).then(|| ctxt.read_u32be()).transpose()?;
                ((scale, scale), Some((center_x, center_y)), var_index_base)
            }
            _ => return Err(ParseError::BadValue),
        };

        Ok(PaintScale {
            scope,
            paint_offset,
            scale,
            center,
            _var_index_base: var_index_base,
        })
    }
}

impl ReadBinary for PaintRotate<'_> {
    type HostType<'a> = PaintRotate<'a>;

    fn read<'a>(ctxt: &mut ReadCtxt<'a>) -> Result<Self::HostType<'a>, ParseError> {
        let scope = ctxt.scope();
        let format = ctxt.read_u8()?;
        let paint_offset = ctxt.read::<U24Be>()?;
        let angle = ctxt.read::<F2Dot14>()?;
        let (center, var_index_base) = match format {
            // PaintRotate
            24 => (None, None),
            // PaintVarRotate
            25 => {
                let var_index_base = ctxt.read_u32be()?;
                (None, Some(var_index_base))
            }
            // PaintRotateAroundCenter and PaintVarRotateAroundCenter
            26 | 27 => {
                let center_x = ctxt.read_i16be()?;
                let center_y = ctxt.read_i16be()?;
                let var_index_base = (format == 27).then(|| ctxt.read_u32be()).transpose()?;
                (Some((center_x, center_y)), var_index_base)
            }
            _ => return Err(ParseError::BadValue),
        };

        Ok(PaintRotate {
            scope,
            paint_offset,
            angle,
            center,
            _var_index_base: var_index_base,
        })
    }
}

impl ReadBinary for PaintSkew<'_> {
    type HostType<'a> = PaintSkew<'a>;

    fn read<'a>(ctxt: &mut ReadCtxt<'a>) -> Result<Self::HostType<'a>, ParseError> {
        let scope = ctxt.scope();
        let format = ctxt.read_u8()?;
        let paint_offset = ctxt.read::<U24Be>()?;
        let x_skew_angle = ctxt.read::<F2Dot14>()?;
        let y_skew_angle = ctxt.read::<F2Dot14>()?;
        let (center, var_index_base) = match format {
            // PaintSkew
            28 => (None, None),
            // PaintVarSkew
            29 => {
                let var_index_base = ctxt.read_u32be()?;
                (None, Some(var_index_base))
            }
            // PaintSkewAroundCenter and PaintVarSkewAroundCenter
            30 | 31 => {
                let center_x = ctxt.read_i16be()?;
                let center_y = ctxt.read_i16be()?;
                let var_index_base = (format == 31).then(|| ctxt.read_u32be()).transpose()?;
                (Some((center_x, center_y)), var_index_base)
            }
            _ => return Err(ParseError::BadValue),
        };

        Ok(PaintSkew {
            scope,
            paint_offset,
            skew_angle: (x_skew_angle, y_skew_angle),
            center,
            _var_index_base: var_index_base,
        })
    }
}

impl<'data> PaintComposite<'data> {
    fn backdrop(&self) -> Result<Paint<'data>, ParseError> {
        self.scope
            .offset(usize::safe_from(self.backdrop_paint_offset))
            .ctxt()
            .read::<Paint<'data>>()
    }

    fn source(&self) -> Result<Paint<'data>, ParseError> {
        self.scope
            .offset(usize::safe_from(self.source_paint_offset))
            .ctxt()
            .read::<Paint<'data>>()
    }
}

impl ReadBinary for PaintComposite<'_> {
    type HostType<'a> = PaintComposite<'a>;

    fn read<'a>(ctxt: &mut ReadCtxt<'a>) -> Result<Self::HostType<'a>, ParseError> {
        let scope = ctxt.scope();
        let format = ctxt.read_u8()?;
        ctxt.check_version(format == 32)?;
        let source_paint_offset = ctxt.read::<U24Be>()?;
        let composite_mode = ctxt.read::<CompositeMode>()?;
        let backdrop_paint_offset = ctxt.read::<U24Be>()?;

        Ok(PaintComposite {
            scope,
            source_paint_offset,
            composite_mode,
            backdrop_paint_offset,
        })
    }
}

impl ReadBinary for CompositeMode {
    type HostType<'a> = CompositeMode;

    fn read<'a>(ctxt: &mut ReadCtxt<'a>) -> Result<Self::HostType<'a>, ParseError> {
        ctxt.read_u8()
            .map_err(ParseError::from)
            .and_then(TryFrom::try_from)
    }
}

/// An RGBA color
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Color(pub f32, pub f32, pub f32, pub f32);

impl Color {
    /// Create a new Color from the supplied [ColorRecord] and alpha value
    pub fn new_with_alpha(color: ColorRecord, alpha: F2Dot14) -> Self {
        // "The alpha indicated in this record is multiplied with the alpha component of the CPAL
        // entry (converted to float—divide by 255)."
        //
        // "Values for alpha outside the range [0., 1.] (inclusive) are reserved; values outside
        // this range must be clamped."
        let alpha = ((f32::from(color.alpha) / 255.0) * f32::from(alpha)).clamp(0.0, 1.0);
        Color(
            f32::from(color.red) / 255.0,
            f32::from(color.green) / 255.0,
            f32::from(color.blue) / 255.0,
            alpha,
        )
    }
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let r = (255.0 * self.0).round() as u8;
        let g = (255.0 * self.1).round() as u8;
        let b = (255.0 * self.2).round() as u8;
        let a = (255.0 * self.3).round() as u8;

        f.write_char('#')?;
        write!(f, "{:02x}", r)?;
        write!(f, "{:02x}", g)?;
        write!(f, "{:02x}", b)?;
        write!(f, "{:02x}", a)
    }
}

impl From<ColorRecord> for Color {
    fn from(color: ColorRecord) -> Self {
        Color(
            f32::from(color.red) / 255.0,
            f32::from(color.green) / 255.0,
            f32::from(color.blue) / 255.0,
            f32::from(color.alpha) / 255.0,
        )
    }
}

impl FromStr for Color {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if !s.starts_with('#') || s.len() != 9 || s.chars().skip(1).any(|c| !c.is_ascii_hexdigit())
        {
            return Err(ParseError::BadValue);
        }

        // NOTE(unwrap): Safe as we have verified all chars are ASCII hex digits
        let r = u8::from_str_radix(&s[1..3], 16).unwrap();
        let g = u8::from_str_radix(&s[3..5], 16).unwrap();
        let b = u8::from_str_radix(&s[5..7], 16).unwrap();
        let a = u8::from_str_radix(&s[7..9], 16).unwrap();
        Ok(Color(
            f32::from(r) / 255.0,
            f32::from(g) / 255.0,
            f32::from(b) / 255.0,
            f32::from(a) / 255.0,
        ))
    }
}

impl fmt::Debug for ColrTable<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let ColrTable {
            version,
            base_glyph_records,
            layer_records,
            base_glyph_list,
            layer_list,
            clip_list,
            var_index_map,
            item_variation_store,
        } = self;
        f.debug_struct("ColrTable")
            .field("version", version)
            .field("base_glyph_records", base_glyph_records)
            .field("layer_records", layer_records)
            .field("base_glyph_list", base_glyph_list)
            .field("layer_list", layer_list)
            .field("clip_list", clip_list)
            .field("var_index_map", var_index_map)
            .field(
                "item_variation_store",
                if item_variation_store.is_some() {
                    &"Some(_)"
                } else {
                    &"None"
                },
            )
            .finish()
    }
}

/// Convert the raw angle value in a paint format to degrees.
fn raw_to_degrees(angle: F2Dot14) -> f32 {
    f32::from(angle) * 180.
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        tables::{FontTableProvider, OpenTypeFont},
        tag,
        tests::read_fixture,
        Font,
    };

    #[test]
    fn test_read_colr_v1_variable() {
        let buffer = read_fixture(
            "tests/fonts/colr/SixtyfourConvergence-Regular-VariableFont_BLED,SCAN,XELA,YELA.ttf",
        );
        let otf = ReadScope::new(&buffer).read::<OpenTypeFont<'_>>().unwrap();
        let table_provider = otf.table_provider(0).expect("error reading font file");

        let colr_data = table_provider
            .read_table_data(tag::COLR)
            .expect("unable to read COLR data");
        let colr = ReadScope::new(&colr_data)
            .read::<ColrTable<'_>>()
            .expect("unable to parse COLR table");

        assert!(colr.base_glyph_records.is_empty());
        assert!(colr.layer_records.is_empty());
        assert!(colr.layer_list.is_none());
        assert!(colr.clip_list.is_none());
        assert_eq!(colr.var_index_map.as_ref().map(|map| map.len()), Some(8));
        assert_eq!(
            colr.base_glyph_list.as_ref().map(|list| list.records.len()),
            Some(481)
        );

        assert!(colr.lookup(1).unwrap().is_some());
    }

    #[test]
    #[cfg(feature = "prince")]
    fn test_visit_colr_nabla_glyph() {
        let buffer =
            read_fixture("../../../tests/data/fonts/colr/Nabla-Regular-VariableFont_EDPT,EHLT.ttf");
        let otf = ReadScope::new(&buffer).read::<OpenTypeFont<'_>>().unwrap();
        let table_provider = otf.table_provider(0).expect("error reading font file");
        let mut font = Font::new(table_provider).unwrap();
        let mut painter = DebugVisitor;

        let glyph_id = 62; // 'N'
        match font.visit_colr_glyph(glyph_id, 0, &mut painter) {
            Ok(()) => {}
            Err(err) => panic!("error visiting COLR glyph: {}", err),
        }
    }
}
