//! A helper module to extract final, absolute glyph positions from a layout.
//! This is useful for renderers that work with simple lists of glyphs.

use azul_core::{
    dom::NodeId,
    geom::LogicalPosition,
    ui_solver::GlyphInstance,
};
use azul_css::props::basic::ColorU;
use azul_css::props::style::StyleBackgroundContent;

use crate::text3::cache::{
    get_item_vertical_metrics_approx, InlineBorderInfo, Point,
    ShapedGlyph, ShapedItem, UnifiedLayout,
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

/// A simple glyph run without font reference - used when fonts aren't available.
/// The font can be looked up later via font_hash if needed.
#[derive(Debug, Clone)]
pub struct SimpleGlyphRun {
    /// The glyphs in this run, with their positions relative to the start of the run.
    pub glyphs: Vec<GlyphInstance>,
    /// The color of the text in this glyph run.
    pub color: ColorU,
    /// Background color for this run (rendered behind text)
    pub background_color: Option<ColorU>,
    /// Full background content layers (for gradients, images, etc.)
    pub background_content: Vec<StyleBackgroundContent>,
    /// Border information for inline elements
    pub border: Option<InlineBorderInfo>,
    /// A hash of the font, useful for caching purposes.
    pub font_hash: u64,
    /// The font size in pixels.
    pub font_size_px: f32,
    /// Text decoration (underline, strikethrough, overline)
    pub text_decoration: crate::text3::cache::TextDecoration,
    /// Whether this is an IME composition preview (should be rendered with special styling)
    pub is_ime_preview: bool,
    /// The source DOM node that generated this text run (for hit-testing)
    pub source_node_id: Option<NodeId>,
}

/// Simple version of get_glyph_runs that doesn't require fonts.
/// Use this when you only need glyph positions and don't need font references.
pub fn get_glyph_runs_simple(layout: &UnifiedLayout) -> Vec<SimpleGlyphRun> {
    let mut runs: Vec<SimpleGlyphRun> = Vec::new();
    let mut current_run: Option<SimpleGlyphRun> = None;

    for item in &layout.items {
        let (item_ascent, _) = get_item_vertical_metrics_approx(&item.item);
        let baseline_y = item.position.y + item_ascent;

        let mut process_glyphs =
            |positioned_glyphs: &[ShapedGlyph],
             item_origin_x: f32,
             writing_mode: crate::text3::cache::WritingMode,
             source_node_id: Option<NodeId>| {
                let mut pen_x = item_origin_x;

                for glyph in positioned_glyphs {
                    let glyph_color = glyph.style.color;
                    let glyph_background = glyph.style.background_color;
                    let glyph_background_content = glyph.style.background_content.clone();
                    let glyph_border = glyph.style.border.clone();
                    let font_hash = glyph.font_hash;
                    let font_size_px = glyph.style.font_size_px;
                    let text_decoration = glyph.style.text_decoration.clone();

                    let absolute_position = LogicalPosition {
                        x: pen_x + glyph.offset.x,
                        y: baseline_y - glyph.offset.y,
                    };

                    let instance =
                        glyph.into_glyph_instance_at_simple(writing_mode, absolute_position);

                    if let Some(run) = current_run.as_mut() {
                        // changes (font, color, border, size). Per spec, text-decoration
                        // changes do not affect shaping (shaping is done upstream in
                        // default.rs), but we still break rendering runs for correct drawing.
                        // Border/margin/padding changes break both shaping and rendering runs.
                        if run.font_hash == font_hash
                            && run.color == glyph_color
                            && run.background_color == glyph_background
                            && run.background_content == glyph_background_content
                            && run.border == glyph_border
                            && run.font_size_px == font_size_px
                            && run.text_decoration == text_decoration
                            && run.source_node_id == source_node_id
                        {
                            run.glyphs.push(instance);
                        } else {
                            runs.push(run.clone());
                            current_run = Some(SimpleGlyphRun {
                                glyphs: vec![instance],
                                color: glyph_color,
                                background_color: glyph_background,
                                background_content: glyph_background_content.clone(),
                                border: glyph_border.clone(),
                                font_hash,
                                font_size_px,
                                text_decoration: text_decoration.clone(),
                                is_ime_preview: false,
                                source_node_id,
                            });
                        }
                    } else {
                        current_run = Some(SimpleGlyphRun {
                            glyphs: vec![instance],
                            color: glyph_color,
                            background_color: glyph_background,
                            background_content: glyph_background_content.clone(),
                            border: glyph_border.clone(),
                            font_hash,
                            font_size_px,
                            text_decoration: text_decoration.clone(),
                            is_ime_preview: false,
                            source_node_id,
                        });
                    }

                    pen_x += glyph.advance + glyph.kerning;
                }
            };

        match &item.item {
            ShapedItem::Cluster(cluster) => {
                let writing_mode = cluster.style.writing_mode;
                process_glyphs(&cluster.glyphs, item.position.x, writing_mode, cluster.source_node_id);
            }
            ShapedItem::CombinedBlock { glyphs, .. } => {
                for g in glyphs {
                    let writing_mode = g.style.writing_mode;
                    // CombinedBlock is for tate-chu-yoko, use None for source_node_id
                    process_glyphs(&[g.clone()], item.position.x, writing_mode, None);
                }
            }
            _ => {}
        }
    }

    if let Some(run) = current_run {
        runs.push(run);
    }

    // +spec:box-model:6c62d3 - suppress margins/borders/padding at inline box split points
    // CSS 2.2 §9.4.2: When an inline box is split across lines, margins, borders,
    // and padding have no visible effect at the split points.
    // Post-process: for runs from the same source_node_id that have borders,
    // mark intermediate fragments so left_inset()/right_inset() suppress edges.
    if runs.len() > 1 {
        let mut i = 0;
        while i < runs.len() {
            if let Some(node_id) = runs[i].source_node_id {
                if runs[i].border.is_some() {
                    let start = i;
                    let mut end = i + 1;
                    while end < runs.len()
                        && runs[end].source_node_id == Some(node_id)
                        && runs[end].border.is_some()
                    {
                        end += 1;
                    }
                    if end - start > 1 {
                        if let Some(ref mut b) = runs[start].border {
                            b.is_last_fragment = false;
                        }
                        for j in (start + 1)..(end - 1) {
                            if let Some(ref mut b) = runs[j].border {
                                b.is_first_fragment = false;
                                b.is_last_fragment = false;
                            }
                        }
                        if let Some(ref mut b) = runs[end - 1].border {
                            b.is_first_fragment = false;
                        }
                    }
                    i = end;
                    continue;
                }
            }
            i += 1;
        }
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
///
/// - `layout` - A reference to the final `UnifiedLayout` produced by the pipeline.
///
/// # Returns
///
/// A `Vec<PositionedGlyph>` containing all glyphs from the layout with their
/// absolute baseline positions.
pub fn get_glyph_positions(layout: &UnifiedLayout) -> Vec<PositionedGlyph> {
    let mut final_glyphs = Vec::new();

    for item in &layout.items {
        let (item_ascent, _) = get_item_vertical_metrics_approx(&item.item);
        let baseline_y = item.position.y + item_ascent;

        let mut process_glyphs = |positioned_glyphs: &[ShapedGlyph], item_origin_x: f32| {
            let mut pen_x = item_origin_x;
            for glyph in positioned_glyphs {
                // The glyph's final position is its origin on the baseline.
                // GPOS y-offsets shift the glyph up or down relative to the baseline.
                // In a Y-down coordinate system, a positive GPOS offset (up) means
                // subtracting from Y.
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
                pen_x += glyph.advance + glyph.kerning;
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
