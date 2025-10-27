//! A helper module to extract final, absolute glyph positions from a layout.
//! This is useful for renderers that work with simple lists of glyphs.

use std::sync::Arc;

use azul_core::{
    geom::{LogicalPosition, LogicalSize},
    ui_solver::GlyphInstance,
};
use azul_css::props::basic::ColorU;

use crate::text3::cache::{
    get_item_vertical_metrics, ParsedFontTrait, Point, PositionedItem, ShapedGlyph, ShapedItem,
    UnifiedLayout,
};

/// Represents a single glyph ready for rendering, with an absolute position on the baseline.
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct PositionedGlyph {
    pub glyph_id: u16,
    /// The absolute position of the glyph's origin on the baseline.
    pub position: Point,
    /// The advance width of the glyph, useful for caret placement.
    pub advance: f32,
}

#[derive(Debug, Clone)]
pub struct GlyphRun<T: ParsedFontTrait> {
    /// The glyphs in this run, with their positions relative to the start of the run.
    pub glyphs: Vec<GlyphInstance>,
    /// The color of the text in this glyph run.
    pub color: ColorU,
    /// The font used for this glyph run.
    pub font: T, // Changed from Arc<T> - T is already cheap to clone (e.g. FontRef)
    /// A hash of the font, useful for caching purposes.
    pub font_hash: u64,
    /// The font size in pixels.
    pub font_size_px: f32,
}

/// Same as `get_glyph_positions`, but returns a list of `GlyphRun`s instead of a flat list of
/// glyphs. This groups glyphs by their font and color, which can be more efficient for rendering.
pub fn get_glyph_runs<T: ParsedFontTrait>(layout: &UnifiedLayout<T>) -> Vec<GlyphRun<T>> {
    // Group glyphs by font and color
    let mut runs: Vec<GlyphRun<T>> = Vec::new();
    let mut current_run: Option<GlyphRun<T>> = None;

    for item in &layout.items {
        // We need the ascent of the item to find its baseline from its top-left position.
        let (item_ascent, _) = get_item_vertical_metrics(&item.item);
        let baseline_y = item.position.y + item_ascent;

        let mut process_glyphs =
            |positioned_glyphs: &[ShapedGlyph<T>],
             item_origin_x: f32,
             writing_mode: crate::text3::cache::WritingMode| {
                let mut pen_x = item_origin_x;

                for glyph in positioned_glyphs {
                    let glyph_color = glyph.style.color;
                    let font_hash = glyph.font.get_hash();
                    let font_size_px = glyph.style.font_size_px;

                    // Calculate absolute position: baseline position + GPOS offset
                    let absolute_position = LogicalPosition {
                        x: pen_x + glyph.offset.x,
                        y: baseline_y - glyph.offset.y, // Y-down: subtract positive offset
                    };

                    let instance = glyph.into_glyph_instance_at(writing_mode, absolute_position);

                    // Check if we can add to the current run
                    if let Some(run) = current_run.as_mut() {
                        if run.font_hash == font_hash
                            && run.color == glyph_color
                            && run.font_size_px == font_size_px
                        {
                            run.glyphs.push(instance);
                        } else {
                            // Different font, color, or size: finalize the current run and start a
                            // new one
                            runs.push(run.clone());
                            current_run = Some(GlyphRun {
                                glyphs: vec![instance],
                                color: glyph_color,
                                font: glyph.font.clone(),
                                font_hash,
                                font_size_px,
                            });
                        }
                    } else {
                        // Start a new run
                        current_run = Some(GlyphRun {
                            glyphs: vec![instance],
                            color: glyph_color,
                            font: glyph.font.clone(),
                            font_hash,
                            font_size_px,
                        });
                    }

                    // Advance the pen for the next glyph in the cluster/block.
                    // TODO: writing-mode support (vertical text) here
                    pen_x += glyph.advance;
                }
            };

        match &item.item {
            ShapedItem::Cluster(cluster) => {
                let writing_mode = cluster.style.writing_mode;
                process_glyphs(&cluster.glyphs, item.position.x, writing_mode);
            }
            // This is a rare case for tate-chu-yoko (mixed horizontal+vertical text)
            ShapedItem::CombinedBlock {
                glyphs,
                source,
                bounds,
                baseline_offset,
            } => {
                for g in glyphs {
                    let writing_mode = g.style.writing_mode;
                    process_glyphs(&[g.clone()], item.position.x, writing_mode);
                }
            }
            _ => {
                // Ignore non-text items like objects, breaks, etc.
            }
        }
    }

    if let Some(run) = current_run {
        runs.push(run);
    }

    runs
}

/// Transforms the final layout into a simple list of glyphs and their absolute positions.
///
/// This function iterates through all positioned items in a layout, filtering for text clusters
/// and combined text blocks. It calculates the absolute baseline position for each glyph within
/// these items and returns a flat vector of `PositionedGlyph` structs. This is useful for
/// rendering or for clients that need a lower-level representation of the text layout.
///
/// # Arguments
/// * `layout` - A reference to the final `UnifiedLayout` produced by the pipeline.
///
/// # Returns
/// A `Vec<PositionedGlyph>` containing all glyphs from the layout with their
/// absolute baseline positions.
pub fn get_glyph_positions<T: ParsedFontTrait>(layout: &UnifiedLayout<T>) -> Vec<PositionedGlyph> {
    let mut final_glyphs = Vec::new();

    for item in &layout.items {
        // We need the ascent of the item to find its baseline from its top-left position.
        let (item_ascent, _) = get_item_vertical_metrics(&item.item);
        let baseline_y = item.position.y + item_ascent;

        let mut process_glyphs = |positioned_glyphs: &[ShapedGlyph<T>], item_origin_x: f32| {
            let mut pen_x = item_origin_x;
            for glyph in positioned_glyphs {
                // The glyph's final position is its origin on the baseline.
                // GPOS y-offsets shift the glyph up or down relative to the baseline.
                // In a Y-down coordinate system, a positive GPOS offset (up) means subtracting from
                // Y.
                let glyph_pos = Point {
                    x: pen_x + glyph.offset.x,
                    y: baseline_y - glyph.offset.y,
                };

                final_glyphs.push(PositionedGlyph {
                    glyph_id: glyph.glyph_id,
                    position: glyph_pos,
                    advance: glyph.advance,
                });

                // Advance the pen for the next glyph in the cluster/block.
                pen_x += glyph.advance;
            }
        };

        match &item.item {
            ShapedItem::Cluster(cluster) => {
                process_glyphs(&cluster.glyphs, item.position.x);
            }
            ShapedItem::CombinedBlock { glyphs, .. } => {
                // This assumes horizontal layout for the combined block's glyphs.
                process_glyphs(glyphs, item.position.x);
            }
            _ => {
                // Ignore non-text items like objects, breaks, etc.
            }
        }
    }

    final_glyphs
}
