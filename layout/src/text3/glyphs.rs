//! A helper module to extract final, absolute glyph positions from a layout.
//! This is useful for renderers that work with simple lists of glyphs.

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
