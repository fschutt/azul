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
    get_item_vertical_metrics_approx, InlineBorderInfo, LoadedFonts, ParsedFontTrait, Point,
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
/// The font can be looked up later via `font_hash` if needed.
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

/// Groups glyphs into runs without requiring font references.
/// Use this when you only need glyph positions and don't need font references.
#[allow(clippy::float_cmp)] // intentional exact compare: change-detection / identity fast-path / cache-key match
#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
#[must_use] pub fn get_glyph_runs_simple(layout: &UnifiedLayout) -> Vec<SimpleGlyphRun> {
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
                    let glyph_border = glyph.style.border;
                    let font_hash = glyph.font_hash;
                    let font_size_px = glyph.style.font_size_px;
                    let text_decoration = glyph.style.text_decoration;

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
                                border: glyph_border,
                                font_hash,
                                font_size_px,
                                text_decoration,
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
                            border: glyph_border,
                            font_hash,
                            font_size_px,
                            text_decoration,
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
                // CombinedBlock (tate-chu-yoko) carries raw per-glyph advances/GPOS
                // offsets, NOT pre-accumulated pen positions. Feed the WHOLE slice to
                // `process_glyphs` in ONE call so the pen advances between glyphs;
                // calling it once per glyph reset pen_x each time and stacked every
                // glyph at the same x (mirrors get_glyph_positions). Use None for
                // source_node_id (tate-chu-yoko has no single source node).
                let writing_mode = glyphs
                    .first()
                    .map_or_else(crate::text3::cache::WritingMode::default, |g| {
                        g.style.writing_mode
                    });
                process_glyphs(glyphs, item.position.x, writing_mode, None);
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
                        for run in &mut runs[start + 1..end - 1] {
                            if let Some(ref mut b) = run.border {
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

/// A glyph run optimized for PDF rendering.
///
/// Groups glyphs by font, color, size, and style, while breaking at line boundaries.
/// This struct is used by the PDF renderer to efficiently render text with proper
/// styling, including inline background colors for `<span>` elements.
///
/// # Z-Order for Inline Backgrounds
///
/// The `background_color` field enables proper z-ordering of inline backgrounds:
/// - PDF renderers should iterate over all runs and render backgrounds FIRST
/// - Then iterate again and render all text SECOND
/// - This ensures backgrounds appear behind text, not on top of it
///
/// The display list (`paint_inline_content`) does NOT emit `push_rect()` for inline
/// backgrounds because that would cause double-rendering and z-order issues.
#[derive(Debug, Clone)]
pub struct PdfGlyphRun<T: ParsedFontTrait> {
    /// The glyphs in this run with their absolute positions
    pub glyphs: Vec<PdfPositionedGlyph>,
    /// The color of the text
    pub color: ColorU,
    /// Background color for inline elements (e.g., `<span style="background: yellow">`)
    ///
    /// This is rendered as a filled rectangle behind the text by the PDF renderer.
    /// The rectangle spans from ascent to descent and covers the full width of the run.
    pub background_color: Option<ColorU>,
    /// The font used for this run
    pub font: T,
    /// Font hash for identification
    pub font_hash: u64,
    /// Font size in pixels
    pub font_size_px: f32,
    /// Text decoration flags
    pub text_decoration: crate::text3::cache::TextDecoration,
    /// The line index this run belongs to (for breaking runs at line boundaries)
    pub line_index: usize,
    /// Text direction for this run
    pub direction: crate::text3::cache::BidiDirection,
    /// Writing mode for this run
    pub writing_mode: crate::text3::cache::WritingMode,
    /// The starting position (baseline) of this run - used for `SetTextMatrix`
    pub baseline_start: Point,
    /// Original cluster text for debugging/CID mapping
    pub cluster_texts: Vec<String>,
}

/// A glyph with its absolute position and cluster text for PDF rendering
#[derive(Debug, Clone)]
pub struct PdfPositionedGlyph {
    /// Glyph ID
    pub glyph_id: u16,
    /// Absolute position on the baseline (Y-down coordinate system)
    pub position: Point,
    /// The advance width of this glyph
    pub advance: f32,
    /// The Unicode character(s) this glyph represents (for PDF `ToUnicode` `CMap`)
    /// This is extracted from the cluster text using the glyph's `cluster_offset`
    pub unicode_codepoint: String,
}

/// Extract glyph runs optimized for PDF rendering.
/// This function:
/// - Groups consecutive glyphs by font, color, size, style, and line
/// - Breaks runs at line boundaries (different `line_index`)
/// - Preserves absolute positioning for each glyph (critical for RTL and complex scripts)
/// - Includes cluster text for proper CID/Unicode mapping
#[allow(clippy::float_cmp)] // intentional exact compare: change-detection / identity fast-path / cache-key match
#[must_use] pub fn get_glyph_runs_pdf<T: ParsedFontTrait>(
    layout: &UnifiedLayout,
    fonts: &LoadedFonts<T>,
) -> Vec<PdfGlyphRun<T>> {
    let mut runs: Vec<PdfGlyphRun<T>> = Vec::new();
    let mut current_run: Option<PdfGlyphRun<T>> = None;

    for positioned_item in &layout.items {
        // Only process text clusters
        let ShapedItem::Cluster(cluster) = &positioned_item.item else {
            continue; // Skip non-text items
        };

        if cluster.glyphs.is_empty() {
            continue;
        }

        // Calculate the baseline position for this cluster
        let (item_ascent, _) = get_item_vertical_metrics_approx(&positioned_item.item);
        let baseline_y = positioned_item.position.y + item_ascent;

        // Process each glyph in the cluster
        let mut pen_x = positioned_item.position.x;

        // For extracting the correct unicode codepoint per glyph, we need to track
        // which portion of the cluster text each glyph represents.
        // The cluster_offset in ShapedGlyph is the byte offset into cluster.text
        let cluster_text = &cluster.text;
        let cluster_glyphs_count = cluster.glyphs.len();

        for (glyph_idx, glyph) in cluster.glyphs.iter().enumerate() {
            let glyph_color = glyph.style.color;
            let glyph_background = glyph.style.background_color;
            let font_hash = glyph.font_hash;
            let font_size_px = glyph.style.font_size_px;
            let text_decoration = glyph.style.text_decoration;
            let line_index = positioned_item.line_index;
            let direction = cluster.direction;
            let writing_mode = cluster.style.writing_mode;

            // Look up the font from the fonts container
            let font = match fonts.get_by_hash(font_hash) {
                Some(f) => f.clone(),
                None => continue, // Skip glyphs with unknown fonts
            };

            // Calculate absolute glyph position on baseline
            let glyph_position = Point {
                x: pen_x + glyph.offset.x,
                y: baseline_y - glyph.offset.y, // Y-down: subtract positive GPOS offset
            };

            // Extract the unicode codepoint for this specific glyph
            // For simple 1:1 mappings, each glyph gets one character
            // For complex scripts (ligatures, etc.), we may need to assign
            // the whole cluster text to the first glyph, or split it appropriately
            let unicode_codepoint = if cluster_glyphs_count == 1 {
                // Simple case: one glyph represents the entire cluster
                cluster_text.clone()
            } else {
                // Multiple glyphs in cluster - try to extract the character at cluster_offset
                // cluster_offset is the byte offset into the cluster text
                let byte_offset = glyph.cluster_offset as usize;
                if byte_offset < cluster_text.len() {
                    // Get the character at this byte offset
                    cluster_text[byte_offset..]
                        .chars()
                        .next().map_or_else(|| cluster_text.clone(), |c| c.to_string())
                } else {
                    // Fallback: if offset is out of range, use the whole cluster for first glyph
                    // or empty for subsequent glyphs (they share the same codepoint)
                    if glyph_idx == 0 {
                        cluster_text.clone()
                    } else {
                        String::new()
                    }
                }
            };

            let pdf_glyph = PdfPositionedGlyph {
                glyph_id: glyph.glyph_id,
                position: glyph_position,
                advance: glyph.advance,
                unicode_codepoint,
            };

            // Font hash change = font change (shaping must break per spec).
            // Border/background change = margin/border/padding non-zero (shaping must break).
            // Text-decoration change = rendering-only break (shaping unaffected per spec).
            let should_break = current_run.as_ref().is_some_and(|run| run.font_hash != font_hash
                    || run.color != glyph_color
                    || run.background_color != glyph_background
                    || run.font_size_px != font_size_px
                    || run.text_decoration != text_decoration
                    || run.line_index != line_index
                    || run.direction != direction || run.writing_mode != writing_mode);

            if should_break {
                // Finalize the current run and start a new one
                if let Some(run) = current_run.take() {
                    runs.push(run);
                }
            }

            if let Some(run) = current_run.as_mut() {
                // Add to existing run
                run.glyphs.push(pdf_glyph);
                run.cluster_texts.push(cluster.text.clone());
            } else {
                // Start a new run
                current_run = Some(PdfGlyphRun {
                    glyphs: vec![pdf_glyph],
                    color: glyph_color,
                    background_color: glyph_background,
                    font: font.clone(),
                    font_hash,
                    font_size_px,
                    text_decoration,
                    line_index,
                    direction,
                    writing_mode,
                    baseline_start: Point {
                        x: pen_x,
                        y: baseline_y,
                    },
                    cluster_texts: vec![cluster.text.clone()],
                });
            }

            // Advance pen position - DON'T add kerning here because it's already
            // included in the positioned_item.position.x from the layout engine!
            // We only advance by the base advance to track our position within this cluster
            pen_x += glyph.advance + glyph.kerning;
        }
    }

    // Push the final run if any
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
///
/// - `layout` - A reference to the final `UnifiedLayout` produced by the pipeline.
///
/// # Returns
///
/// A `Vec<PositionedGlyph>` containing all glyphs from the layout with their
/// absolute baseline positions.
#[must_use] pub fn get_glyph_positions(layout: &UnifiedLayout) -> Vec<PositionedGlyph> {
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

/// Adversarial unit tests generated for `layout/src/text3/glyphs.rs`.
///
/// The three public entry points are pure transforms over a `UnifiedLayout`, so the
/// interesting failure modes are all in the pen arithmetic (NaN / ±inf / negative
/// kerning), the run-splitting predicates (which use `==` on `f32`), the UTF-8 slicing
/// in the PDF codepoint extraction, and the inline-box fragment post-pass.
///
/// Fixtures use `units_per_em == 0` metrics on purpose: that makes
/// `get_item_vertical_metrics_approx` skip the glyph, so ascent is exactly `0.0` and the
/// baseline lands exactly on `item.position.y`. Every asserted coordinate is therefore
/// exact rather than approximate.
///
/// Where a function has surprising-but-real behaviour, the test PINS it and says so
/// (see `pdf_cluster_offset_inside_multibyte_char_panics` and
/// `pdf_unknown_font_glyph_does_not_advance_the_pen`, both of which are genuine bugs).
///
/// Note: `Point`'s `PartialEq` is `round_eq` (rounds to `isize`), so all coordinate
/// assertions compare the raw `f32` fields instead of whole `Point`s.
#[cfg(test)]
#[allow(
    clippy::float_cmp,
    clippy::too_many_lines,
    clippy::unreadable_literal,
    clippy::similar_names,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation
)]
mod autotest_generated {
    use std::sync::Arc;

    use azul_core::geom::LogicalSize;
    use rust_fontconfig::FontId;

    use super::*;
    use crate::text3::{
        cache::{
            BidiDirection, BreakType, ClearType, ContentIndex, Glyph, GlyphKind,
            GraphemeClusterId, InlineBreak, InlineContent, InlineSpace, LayoutError,
            LayoutFontMetrics, OverflowInfo, PositionedItem, Rect, ShallowClone, ShapedCluster,
            StyleProperties, TextDecoration, VerticalMetrics, WritingMode,
        },
        script::{Language, Script},
    };

    const FONT_A: u64 = 0xAAAA_AAAA;
    const FONT_B: u64 = 0xBBBB_BBBB;

    // ---------------------------------------------------------------------
    // Fixtures
    // ---------------------------------------------------------------------

    /// `units_per_em == 0` => `get_item_vertical_metrics_approx` returns `(0.0, 0.0)`,
    /// so `baseline_y == item.position.y` and every coordinate below is exact.
    fn zero_metrics() -> LayoutFontMetrics {
        LayoutFontMetrics {
            ascent: 0.0,
            descent: 0.0,
            line_gap: 0.0,
            units_per_em: 0,
            x_height: None,
            cap_height: None,
        }
    }

    fn style() -> Arc<StyleProperties> {
        Arc::new(StyleProperties::default())
    }

    fn styled(f: impl FnOnce(&mut StyleProperties)) -> Arc<StyleProperties> {
        let mut s = StyleProperties::default();
        f(&mut s);
        Arc::new(s)
    }

    fn rgba(r: u8, g: u8, b: u8, a: u8) -> ColorU {
        ColorU { r, g, b, a }
    }

    /// A glyph with the standard degenerate metrics, sitting at font `FONT_A`.
    fn glyph(glyph_id: u16, advance: f32, st: &Arc<StyleProperties>) -> ShapedGlyph {
        ShapedGlyph {
            kind: GlyphKind::Character,
            glyph_id,
            cluster_offset: 0,
            advance,
            kerning: 0.0,
            offset: Point { x: 0.0, y: 0.0 },
            vertical_advance: 0.0,
            vertical_offset: Point { x: 0.0, y: 0.0 },
            script: Script::Latin,
            style: st.clone(),
            font_hash: FONT_A,
            font_metrics: zero_metrics(),
        }
    }

    /// Returns the `ShapedCluster` (not the `ShapedItem`) so tests can override
    /// `direction` / `style` / `text` before wrapping it with [`item`].
    fn cluster(text: &str, glyphs: Vec<ShapedGlyph>, st: &Arc<StyleProperties>) -> ShapedCluster {
        ShapedCluster {
            text: text.to_string(),
            source_cluster_id: GraphemeClusterId {
                source_run: 0,
                start_byte_in_run: 0,
            },
            source_content_index: ContentIndex {
                run_index: 0,
                item_index: 0,
            },
            source_node_id: None,
            glyphs: glyphs.into_iter().collect(),
            advance: 0.0,
            direction: BidiDirection::Ltr,
            style: st.clone(),
            marker_position_outside: None,
            is_first_fragment: true,
            is_last_fragment: true,
        }
    }

    fn item(c: ShapedCluster) -> ShapedItem {
        ShapedItem::Cluster(c)
    }

    fn combined(glyphs: Vec<ShapedGlyph>) -> ShapedItem {
        ShapedItem::CombinedBlock {
            source: ContentIndex {
                run_index: 0,
                item_index: 0,
            },
            glyphs: glyphs.into_iter().collect(),
            // height 0 keeps the fallback ascent (`0.8 * height`) at exactly 0.
            bounds: Rect {
                x: 0.0,
                y: 0.0,
                width: 0.0,
                height: 0.0,
            },
            baseline_offset: 0.0,
        }
    }

    fn tab() -> ShapedItem {
        ShapedItem::Tab {
            source: ContentIndex {
                run_index: 0,
                item_index: 0,
            },
            bounds: Rect {
                x: 0.0,
                y: 0.0,
                width: 8.0,
                height: 0.0,
            },
        }
    }

    fn hard_break() -> ShapedItem {
        ShapedItem::Break {
            source: ContentIndex {
                run_index: 0,
                item_index: 0,
            },
            break_info: InlineBreak {
                break_type: BreakType::Hard,
                clear: ClearType::None,
                content_index: 0,
            },
        }
    }

    fn object() -> ShapedItem {
        ShapedItem::Object {
            source: ContentIndex {
                run_index: 0,
                item_index: 0,
            },
            bounds: Rect {
                x: 0.0,
                y: 0.0,
                width: 10.0,
                height: 10.0,
            },
            baseline_offset: 0.0,
            content: InlineContent::Space(InlineSpace {
                width: 10.0,
                is_breaking: false,
                is_stretchy: false,
            }),
        }
    }

    fn at(i: ShapedItem, x: f32, y: f32, line_index: usize) -> PositionedItem {
        PositionedItem {
            item: i,
            position: Point { x, y },
            line_index,
        }
    }

    fn layout(items: Vec<PositionedItem>) -> UnifiedLayout {
        UnifiedLayout {
            items,
            overflow: OverflowInfo::default(),
        }
    }

    fn border() -> InlineBorderInfo {
        InlineBorderInfo {
            left: 2.0,
            right: 2.0,
            ..InlineBorderInfo::default()
        }
    }

    /// A minimal in-memory `ParsedFontTrait` so `LoadedFonts` can be populated
    /// without touching the filesystem or fontconfig.
    #[derive(Debug, Clone)]
    struct TestFont {
        hash: u64,
    }

    impl ShallowClone for TestFont {
        fn shallow_clone(&self) -> Self {
            self.clone()
        }
    }

    impl ParsedFontTrait for TestFont {
        fn shape_text(
            &self,
            _text: &str,
            _script: Script,
            _language: Language,
            _direction: BidiDirection,
            _style: &StyleProperties,
        ) -> Result<Vec<Glyph>, LayoutError> {
            Ok(Vec::new())
        }
        fn get_hash(&self) -> u64 {
            self.hash
        }
        fn get_glyph_size(&self, _glyph_id: u16, font_size: f32) -> Option<LogicalSize> {
            Some(LogicalSize {
                width: font_size,
                height: font_size,
            })
        }
        fn get_hyphen_glyph_and_advance(&self, font_size: f32) -> Option<(u16, f32)> {
            Some((1, font_size * 0.3))
        }
        fn get_kashida_glyph_and_advance(&self, font_size: f32) -> Option<(u16, f32)> {
            Some((2, font_size * 0.2))
        }
        fn has_glyph(&self, _codepoint: u32) -> bool {
            true
        }
        fn get_vertical_metrics(&self, _glyph_id: u16) -> Option<VerticalMetrics> {
            None
        }
        fn get_font_metrics(&self) -> LayoutFontMetrics {
            zero_metrics()
        }
        fn num_glyphs(&self) -> u16 {
            10
        }
        fn get_space_width(&self) -> Option<usize> {
            Some(500)
        }
    }

    fn fonts_with(hashes: &[u64]) -> LoadedFonts<TestFont> {
        let mut fonts = LoadedFonts::new();
        for &h in hashes {
            fonts.insert(FontId::new(), TestFont { hash: h });
        }
        fonts
    }

    fn no_fonts() -> LoadedFonts<TestFont> {
        LoadedFonts::new()
    }

    // =====================================================================
    // get_glyph_positions
    // =====================================================================

    #[test]
    fn positions_empty_layout_yields_no_glyphs() {
        assert!(get_glyph_positions(&layout(Vec::new())).is_empty());
    }

    #[test]
    fn positions_ignore_non_text_items() {
        // Object / Tab / Break carry no glyphs and must be skipped, not panicked on.
        let l = layout(vec![
            at(object(), 0.0, 0.0, 0),
            at(tab(), 10.0, 0.0, 0),
            at(hard_break(), 20.0, 0.0, 0),
        ]);
        assert!(get_glyph_positions(&l).is_empty());
    }

    #[test]
    fn positions_cluster_without_glyphs_yields_nothing() {
        let st = style();
        let l = layout(vec![at(item(cluster("abc", Vec::new(), &st)), 0.0, 0.0, 0)]);
        assert!(get_glyph_positions(&l).is_empty());
    }

    #[test]
    fn positions_pen_advances_by_advance_plus_kerning() {
        let st = style();
        let mut g0 = glyph(1, 10.0, &st);
        g0.kerning = 2.0;
        let mut g1 = glyph(2, 10.0, &st);
        g1.kerning = 2.0;
        let g2 = glyph(3, 10.0, &st);

        let l = layout(vec![at(
            item(cluster("abc", vec![g0, g1, g2], &st)),
            100.0,
            50.0,
            0,
        )]);
        let out = get_glyph_positions(&l);

        assert_eq!(out.len(), 3);
        assert_eq!(out[0].position.x, 100.0);
        assert_eq!(out[1].position.x, 112.0, "advance 10 + kerning 2");
        assert_eq!(out[2].position.x, 124.0);
        // upem == 0 => ascent == 0 => baseline == item.position.y, exactly.
        for g in &out {
            assert_eq!(g.position.y, 50.0);
        }
    }

    #[test]
    fn positions_negative_kerning_walks_the_pen_backwards() {
        // Kerning is unbounded below; a tighter-than-advance kern makes x decrease.
        let st = style();
        let mut g = glyph(1, 10.0, &st);
        g.kerning = -12.0;
        let l = layout(vec![at(
            item(cluster("aaa", vec![g.clone(), g.clone(), g], &st)),
            0.0,
            0.0,
            0,
        )]);
        let out = get_glyph_positions(&l);

        assert_eq!(out[0].position.x, 0.0);
        assert_eq!(out[1].position.x, -2.0);
        assert_eq!(out[2].position.x, -4.0);
    }

    #[test]
    fn positions_gpos_y_offset_is_subtracted_in_y_down_space() {
        let st = style();
        let mut g = glyph(1, 10.0, &st);
        g.offset = Point { x: 3.0, y: 7.0 };
        let l = layout(vec![at(item(cluster("a", vec![g], &st)), 0.0, 100.0, 0)]);
        let out = get_glyph_positions(&l);

        assert_eq!(out[0].position.x, 3.0, "x offset is added");
        assert_eq!(
            out[0].position.y, 93.0,
            "positive GPOS y (up) subtracts in Y-down space"
        );
    }

    #[test]
    fn positions_combined_block_glyphs_do_not_stack_at_one_x() {
        // Regression: CombinedBlock carries raw advances, so the pen must accumulate
        // across the whole slice instead of resetting per glyph.
        let st = style();
        let l = layout(vec![at(
            combined(vec![glyph(1, 10.0, &st), glyph(2, 10.0, &st)]),
            100.0,
            0.0,
            0,
        )]);
        let out = get_glyph_positions(&l);

        assert_eq!(out.len(), 2);
        assert_eq!(out[0].position.x, 100.0);
        assert_eq!(out[1].position.x, 110.0);
    }

    #[test]
    fn positions_round_trip_glyph_id_and_advance_verbatim() {
        let st = style();
        let mut g = glyph(u16::MAX, f32::MIN_POSITIVE, &st);
        g.advance = f32::MIN_POSITIVE;
        let l = layout(vec![at(item(cluster("\u{10FFFF}", vec![g], &st)), 0.0, 0.0, 0)]);
        let out = get_glyph_positions(&l);

        assert_eq!(out[0].glyph_id, u16::MAX, "glyph id is copied, not truncated");
        assert_eq!(out[0].advance, f32::MIN_POSITIVE, "advance is copied verbatim");
    }

    #[test]
    fn positions_nan_advance_poisons_later_glyphs_but_does_not_panic() {
        let st = style();
        let l = layout(vec![at(
            item(cluster(
                "ab",
                vec![glyph(1, f32::NAN, &st), glyph(2, 10.0, &st)],
                &st,
            )),
            0.0,
            0.0,
            0,
        )]);
        let out = get_glyph_positions(&l);

        assert_eq!(out.len(), 2);
        assert_eq!(out[0].position.x, 0.0, "first glyph is placed before the NaN advance");
        assert!(
            out[1].position.x.is_nan(),
            "NaN advance propagates into the pen — pinned, not fixed"
        );
    }

    #[test]
    fn positions_huge_advances_saturate_to_infinity_without_panicking() {
        let st = style();
        let l = layout(vec![at(
            item(cluster(
                "abc",
                vec![
                    glyph(1, f32::MAX, &st),
                    glyph(2, f32::MAX, &st),
                    glyph(3, 1.0, &st),
                ],
                &st,
            )),
            0.0,
            0.0,
            0,
        )]);
        let out = get_glyph_positions(&l);

        assert_eq!(out[0].position.x, 0.0);
        assert_eq!(out[1].position.x, f32::MAX);
        assert!(
            out[2].position.x.is_infinite() && out[2].position.x.is_sign_positive(),
            "f32 saturates on overflow instead of wrapping/panicking"
        );
    }

    #[test]
    fn positions_ten_thousand_glyphs_do_not_overflow() {
        let st = style();
        let glyphs: Vec<ShapedGlyph> = (0..10_000u32)
            .map(|i| glyph((i % 65536) as u16, 1.0, &st))
            .collect();
        let l = layout(vec![at(item(cluster("x", glyphs, &st)), 0.0, 0.0, 0)]);
        let out = get_glyph_positions(&l);

        assert_eq!(out.len(), 10_000);
        assert_eq!(out[9_999].position.x, 9_999.0, "integral f32 accumulation is exact here");
    }

    #[test]
    fn positions_pen_resets_at_each_item_origin() {
        let st = style();
        let l = layout(vec![
            at(item(cluster("a", vec![glyph(1, 10.0, &st)], &st)), 0.0, 0.0, 0),
            at(item(cluster("b", vec![glyph(2, 10.0, &st)], &st)), 200.0, 30.0, 1),
        ]);
        let out = get_glyph_positions(&l);

        assert_eq!(out[1].position.x, 200.0, "pen restarts from the item origin");
        assert_eq!(out[1].position.y, 30.0);
    }

    // =====================================================================
    // get_glyph_runs_simple
    // =====================================================================

    #[test]
    fn simple_empty_layout_yields_no_runs() {
        assert!(get_glyph_runs_simple(&layout(Vec::new())).is_empty());
    }

    #[test]
    fn simple_non_text_items_yield_no_runs() {
        let l = layout(vec![
            at(object(), 0.0, 0.0, 0),
            at(tab(), 0.0, 0.0, 0),
            at(hard_break(), 0.0, 0.0, 0),
        ]);
        assert!(get_glyph_runs_simple(&l).is_empty());
    }

    #[test]
    fn simple_uniform_glyphs_merge_into_one_run_across_items() {
        // The current run is NOT flushed at item boundaries — only on a style change.
        let st = style();
        let l = layout(vec![
            at(
                item(cluster("ab", vec![glyph(1, 10.0, &st), glyph(2, 10.0, &st)], &st)),
                0.0,
                0.0,
                0,
            ),
            at(
                item(cluster("cd", vec![glyph(3, 10.0, &st), glyph(4, 10.0, &st)], &st)),
                50.0,
                0.0,
                1,
            ),
        ]);
        let runs = get_glyph_runs_simple(&l);

        assert_eq!(runs.len(), 1, "identical style => one run, even across lines");
        assert_eq!(runs[0].glyphs.len(), 4);
        assert_eq!(runs[0].glyphs[2].point.x, 50.0, "pen still restarts per item");
    }

    #[test]
    fn simple_color_change_splits_the_run() {
        let a = styled(|s| s.color = rgba(255, 0, 0, 255));
        let b = styled(|s| s.color = rgba(0, 0, 255, 255));
        let l = layout(vec![at(
            item(cluster("ab", vec![glyph(1, 10.0, &a), glyph(2, 10.0, &b)], &a)),
            0.0,
            0.0,
            0,
        )]);
        let runs = get_glyph_runs_simple(&l);

        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].color, rgba(255, 0, 0, 255));
        assert_eq!(runs[1].color, rgba(0, 0, 255, 255));
    }

    #[test]
    fn simple_font_hash_change_splits_the_run() {
        let st = style();
        let mut g1 = glyph(2, 10.0, &st);
        g1.font_hash = FONT_B;
        let l = layout(vec![at(
            item(cluster("ab", vec![glyph(1, 10.0, &st), g1], &st)),
            0.0,
            0.0,
            0,
        )]);
        let runs = get_glyph_runs_simple(&l);

        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].font_hash, FONT_A);
        assert_eq!(runs[1].font_hash, FONT_B);
    }

    #[test]
    fn simple_font_size_and_decoration_changes_split_the_run() {
        let a = style();
        let b = styled(|s| s.font_size_px = 24.0);
        let c = styled(|s| {
            s.font_size_px = 24.0;
            s.text_decoration = TextDecoration {
                underline: true,
                strikethrough: false,
                overline: false,
            };
        });
        let l = layout(vec![at(
            item(cluster(
                "abc",
                vec![glyph(1, 10.0, &a), glyph(2, 10.0, &b), glyph(3, 10.0, &c)],
                &a,
            )),
            0.0,
            0.0,
            0,
        )]);
        let runs = get_glyph_runs_simple(&l);

        assert_eq!(runs.len(), 3, "size change and decoration change both break");
        assert_eq!(runs[0].font_size_px, 16.0);
        assert_eq!(runs[1].font_size_px, 24.0);
        assert!(runs[2].text_decoration.underline);
    }

    #[test]
    fn simple_background_content_change_splits_the_run() {
        let a = style();
        let b = styled(|s| {
            s.background_content = vec![StyleBackgroundContent::Color(rgba(1, 2, 3, 4))];
        });
        let l = layout(vec![at(
            item(cluster("ab", vec![glyph(1, 10.0, &a), glyph(2, 10.0, &b)], &a)),
            0.0,
            0.0,
            0,
        )]);
        let runs = get_glyph_runs_simple(&l);

        assert_eq!(runs.len(), 2);
        assert!(runs[0].background_content.is_empty());
        assert_eq!(runs[1].background_content.len(), 1);
    }

    #[test]
    fn simple_source_node_id_change_splits_the_run() {
        let st = style();
        let mut c0 = cluster("a", vec![glyph(1, 10.0, &st)], &st);
        c0.source_node_id = Some(NodeId::new(3));
        let mut c1 = cluster("b", vec![glyph(2, 10.0, &st)], &st);
        c1.source_node_id = Some(NodeId::new(4));

        let l = layout(vec![
            at(item(c0), 0.0, 0.0, 0),
            at(item(c1), 10.0, 0.0, 0),
        ]);
        let runs = get_glyph_runs_simple(&l);

        assert_eq!(runs.len(), 2, "hit-testing identity must not be merged away");
        assert_eq!(runs[0].source_node_id, Some(NodeId::new(3)));
        assert_eq!(runs[1].source_node_id, Some(NodeId::new(4)));
    }

    #[test]
    fn simple_nan_font_size_forces_one_run_per_glyph() {
        // The run predicate compares font sizes with `==`. NaN != NaN, so a NaN
        // font-size defeats run coalescing entirely: N glyphs => N runs.
        let nan = styled(|s| s.font_size_px = f32::NAN);
        let l = layout(vec![at(
            item(cluster(
                "abc",
                vec![
                    glyph(1, 10.0, &nan),
                    glyph(2, 10.0, &nan),
                    glyph(3, 10.0, &nan),
                ],
                &nan,
            )),
            0.0,
            0.0,
            0,
        )]);
        let runs = get_glyph_runs_simple(&l);

        assert_eq!(runs.len(), 3, "NaN never compares equal => no coalescing");
        assert!(runs.iter().all(|r| r.glyphs.len() == 1));
    }

    #[test]
    fn simple_combined_block_with_no_glyphs_is_a_no_op() {
        // `glyphs.first()` is None => the default writing mode is used and nothing
        // is emitted. Must not panic on the empty slice.
        let l = layout(vec![at(combined(Vec::new()), 0.0, 0.0, 0)]);
        assert!(get_glyph_runs_simple(&l).is_empty());
    }

    #[test]
    fn simple_never_marks_runs_as_ime_preview() {
        let st = style();
        let l = layout(vec![at(item(cluster("a", vec![glyph(1, 10.0, &st)], &st)), 0.0, 0.0, 0)]);
        let runs = get_glyph_runs_simple(&l);
        assert!(!runs[0].is_ime_preview, "this path never sets the IME flag");
    }

    #[test]
    fn simple_run_glyphs_agree_with_get_glyph_positions() {
        // Cross-function invariant: both walk Cluster + CombinedBlock with the same
        // pen arithmetic, so the flattened runs must match the positions 1:1.
        let st = style();
        let l = layout(vec![
            at(
                item(cluster("ab", vec![glyph(1, 10.0, &st), glyph(2, 7.5, &st)], &st)),
                12.0,
                40.0,
                0,
            ),
            at(tab(), 30.0, 40.0, 0),
            at(
                combined(vec![glyph(3, 5.0, &st), glyph(4, 5.0, &st)]),
                60.0,
                80.0,
                1,
            ),
        ]);

        let positions = get_glyph_positions(&l);
        let flat: Vec<GlyphInstance> = get_glyph_runs_simple(&l)
            .into_iter()
            .flat_map(|r| r.glyphs)
            .collect();

        assert_eq!(flat.len(), positions.len(), "same glyph count");
        assert_eq!(flat.len(), 4, "the Tab contributes nothing");
        for (inst, pos) in flat.iter().zip(positions.iter()) {
            assert_eq!(inst.index, u32::from(pos.glyph_id));
            assert_eq!(inst.point.x, pos.position.x);
            assert_eq!(inst.point.y, pos.position.y);
        }
    }

    // --- inline-box split post-pass (CSS 2.2 §9.4.2) ----------------------

    /// n same-node bordered clusters whose only difference is colour => n runs.
    fn bordered_fragments(n: usize) -> UnifiedLayout {
        let node = NodeId::new(7);
        let items = (0..n)
            .map(|i| {
                let st = styled(|s| {
                    s.border = Some(border());
                    s.color = rgba(u8::try_from(i % 256).unwrap_or(0), 0, 0, 255);
                });
                let mut c = cluster("x", vec![glyph(1, 10.0, &st)], &st);
                c.source_node_id = Some(node);
                at(item(c), i as f32 * 10.0, 0.0, i)
            })
            .collect();
        layout(items)
    }

    #[test]
    fn simple_two_fragment_split_suppresses_the_inner_edges() {
        let runs = get_glyph_runs_simple(&bordered_fragments(2));
        assert_eq!(runs.len(), 2);

        let first = runs[0].border.expect("border survives run splitting");
        let last = runs[1].border.expect("border survives run splitting");
        assert!(first.is_first_fragment && !first.is_last_fragment);
        assert!(!last.is_first_fragment && last.is_last_fragment);
    }

    #[test]
    fn simple_three_fragment_split_strips_both_edges_from_the_middle() {
        let runs = get_glyph_runs_simple(&bordered_fragments(3));
        assert_eq!(runs.len(), 3);

        let mid = runs[1].border.expect("border survives run splitting");
        assert!(
            !mid.is_first_fragment && !mid.is_last_fragment,
            "an intermediate fragment draws neither the start nor the end edge"
        );
        assert!(!runs[0].border.unwrap().is_last_fragment);
        assert!(!runs[2].border.unwrap().is_first_fragment);
    }

    #[test]
    fn simple_fragment_scan_terminates_on_a_long_run_chain() {
        // The post-pass advances `i = end` and `continue`s; guard against a
        // non-advancing scan by driving 64 consecutive same-node fragments.
        let runs = get_glyph_runs_simple(&bordered_fragments(64));
        assert_eq!(runs.len(), 64);
        assert!(!runs[0].border.unwrap().is_last_fragment);
        assert!(!runs[63].border.unwrap().is_first_fragment);
        for r in &runs[1..63] {
            let b = r.border.unwrap();
            assert!(!b.is_first_fragment && !b.is_last_fragment);
        }
    }

    #[test]
    fn simple_border_without_source_node_id_is_left_untouched() {
        // No node id => the fragments cannot be proven to belong to one inline box,
        // so both edges stay drawn.
        let a = styled(|s| {
            s.border = Some(border());
            s.color = rgba(255, 0, 0, 255);
        });
        let b = styled(|s| {
            s.border = Some(border());
            s.color = rgba(0, 255, 0, 255);
        });
        let l = layout(vec![at(
            item(cluster("ab", vec![glyph(1, 10.0, &a), glyph(2, 10.0, &b)], &a)),
            0.0,
            0.0,
            0,
        )]);
        let runs = get_glyph_runs_simple(&l);

        assert_eq!(runs.len(), 2);
        for r in &runs {
            let bd = r.border.unwrap();
            assert!(bd.is_first_fragment && bd.is_last_fragment);
        }
    }

    #[test]
    fn simple_single_bordered_run_keeps_both_edges() {
        let runs = get_glyph_runs_simple(&bordered_fragments(1));
        assert_eq!(runs.len(), 1);
        let b = runs[0].border.unwrap();
        assert!(
            b.is_first_fragment && b.is_last_fragment,
            "an unsplit inline box draws both edges"
        );
    }

    // =====================================================================
    // get_glyph_runs_pdf
    // =====================================================================

    #[test]
    fn pdf_empty_layout_yields_no_runs() {
        let runs = get_glyph_runs_pdf(&layout(Vec::new()), &fonts_with(&[FONT_A]));
        assert!(runs.is_empty());
    }

    #[test]
    fn pdf_glyphs_with_unknown_fonts_are_dropped() {
        let st = style();
        let l = layout(vec![at(
            item(cluster("ab", vec![glyph(1, 10.0, &st), glyph(2, 10.0, &st)], &st)),
            0.0,
            0.0,
            0,
        )]);
        assert!(
            get_glyph_runs_pdf(&l, &no_fonts()).is_empty(),
            "no font => no run (glyphs are skipped, not defaulted)"
        );
    }

    #[test]
    fn pdf_ignores_combined_blocks_unlike_get_glyph_positions() {
        // Tate-chu-yoko blocks are silently dropped by the PDF path.
        let st = style();
        let l = layout(vec![at(
            combined(vec![glyph(1, 10.0, &st), glyph(2, 10.0, &st)]),
            0.0,
            0.0,
            0,
        )]);

        assert_eq!(get_glyph_positions(&l).len(), 2);
        assert!(
            get_glyph_runs_pdf(&l, &fonts_with(&[FONT_A])).is_empty(),
            "CombinedBlock never reaches the PDF run builder"
        );
    }

    #[test]
    fn pdf_empty_and_non_text_items_are_skipped() {
        let st = style();
        let l = layout(vec![
            at(item(cluster("", Vec::new(), &st)), 0.0, 0.0, 0),
            at(tab(), 0.0, 0.0, 0),
            at(hard_break(), 0.0, 0.0, 0),
            at(object(), 0.0, 0.0, 0),
            at(item(cluster("a", vec![glyph(1, 10.0, &st)], &st)), 5.0, 0.0, 0),
        ]);
        let runs = get_glyph_runs_pdf(&l, &fonts_with(&[FONT_A]));

        assert_eq!(runs.len(), 1, "only the one real cluster survives");
        assert_eq!(runs[0].glyphs.len(), 1);
    }

    #[test]
    fn pdf_single_glyph_cluster_maps_the_whole_cluster_text() {
        let st = style();
        let l = layout(vec![at(
            item(cluster("\u{1F600}", vec![glyph(1, 10.0, &st)], &st)),
            0.0,
            0.0,
            0,
        )]);
        let runs = get_glyph_runs_pdf(&l, &fonts_with(&[FONT_A]));

        assert_eq!(
            runs[0].glyphs[0].unicode_codepoint, "\u{1F600}",
            "a 1:1 cluster carries its full (astral) text for ToUnicode"
        );
    }

    #[test]
    fn pdf_multi_glyph_cluster_offset_past_the_end_falls_back() {
        // byte_offset >= len => whole text for glyph 0, empty for the rest.
        let st = style();
        let mut g0 = glyph(1, 10.0, &st);
        g0.cluster_offset = 99;
        let mut g1 = glyph(2, 10.0, &st);
        g1.cluster_offset = 99;
        let l = layout(vec![at(item(cluster("ab", vec![g0, g1], &st)), 0.0, 0.0, 0)]);
        let runs = get_glyph_runs_pdf(&l, &fonts_with(&[FONT_A]));

        assert_eq!(runs[0].glyphs[0].unicode_codepoint, "ab");
        assert_eq!(runs[0].glyphs[1].unicode_codepoint, "");
    }

    #[test]
    fn pdf_cluster_offset_u32_max_does_not_overflow() {
        let st = style();
        let mut g0 = glyph(1, 10.0, &st);
        g0.cluster_offset = u32::MAX;
        let mut g1 = glyph(2, 10.0, &st);
        g1.cluster_offset = u32::MAX;
        let l = layout(vec![at(item(cluster("ab", vec![g0, g1], &st)), 0.0, 0.0, 0)]);
        let runs = get_glyph_runs_pdf(&l, &fonts_with(&[FONT_A]));

        // `u32::MAX as usize` is not < 2, so both take the out-of-range fallback.
        assert_eq!(runs[0].glyphs[0].unicode_codepoint, "ab");
        assert_eq!(runs[0].glyphs[1].unicode_codepoint, "");
    }

    #[test]
    fn pdf_empty_cluster_text_with_several_glyphs_yields_empty_codepoints() {
        let st = style();
        let l = layout(vec![at(
            item(cluster("", vec![glyph(1, 10.0, &st), glyph(2, 10.0, &st)], &st)),
            0.0,
            0.0,
            0,
        )]);
        let runs = get_glyph_runs_pdf(&l, &fonts_with(&[FONT_A]));

        assert_eq!(runs[0].glyphs.len(), 2);
        assert!(runs[0]
            .glyphs
            .iter()
            .all(|g| g.unicode_codepoint.is_empty()));
    }

    /// BUG (pinned): the codepoint extractor slices `cluster_text[byte_offset..]`
    /// after only checking `byte_offset < len`, never that the offset is a UTF-8
    /// char boundary. A multi-glyph cluster over a multi-byte character (any
    /// decomposed/combining sequence where the shaper reports a mid-char offset)
    /// therefore panics inside `get_glyph_runs_pdf` instead of degrading.
    /// The correct behaviour would be `is_char_boundary()` + fallback.
    #[test]
    #[should_panic(expected = "char boundary")]
    fn pdf_cluster_offset_inside_multibyte_char_panics() {
        let st = style();
        let g0 = glyph(1, 10.0, &st); // cluster_offset 0 — fine
        let mut g1 = glyph(2, 10.0, &st);
        g1.cluster_offset = 1; // 1 < 2, but it is inside 'é'
        let l = layout(vec![at(item(cluster("é", vec![g0, g1], &st)), 0.0, 0.0, 0)]);

        let _ = get_glyph_runs_pdf(&l, &fonts_with(&[FONT_A]));
    }

    /// BUG (pinned): the `continue` that drops an unknown-font glyph happens BEFORE
    /// `pen_x += advance + kerning`, so every glyph after a dropped one is rendered
    /// one advance too far to the left. A font that fails to load silently shifts
    /// the rest of the cluster instead of just omitting a glyph.
    #[test]
    fn pdf_unknown_font_glyph_does_not_advance_the_pen() {
        let st = style();
        let mut g0 = glyph(1, 40.0, &st);
        g0.font_hash = FONT_B; // not in LoadedFonts
        let g1 = glyph(2, 10.0, &st); // FONT_A

        let l = layout(vec![at(item(cluster("ab", vec![g0, g1], &st)), 100.0, 0.0, 0)]);
        let runs = get_glyph_runs_pdf(&l, &fonts_with(&[FONT_A]));

        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].glyphs.len(), 1, "the unknown-font glyph is dropped");
        assert_eq!(
            runs[0].glyphs[0].position.x, 100.0,
            "pinned: the surviving glyph sits at the origin — the dropped glyph's \
             40px advance was never applied, so it renders 40px too far left"
        );
    }

    #[test]
    fn pdf_line_index_change_breaks_the_run() {
        let st = style();
        let l = layout(vec![
            at(item(cluster("a", vec![glyph(1, 10.0, &st)], &st)), 0.0, 0.0, 0),
            at(item(cluster("b", vec![glyph(2, 10.0, &st)], &st)), 0.0, 20.0, 1),
        ]);
        let runs = get_glyph_runs_pdf(&l, &fonts_with(&[FONT_A]));

        assert_eq!(runs.len(), 2, "runs must not straddle a line boundary");
        assert_eq!(runs[0].line_index, 0);
        assert_eq!(runs[1].line_index, 1);
    }

    #[test]
    fn pdf_direction_change_breaks_the_run() {
        let st = style();
        let ltr = cluster("a", vec![glyph(1, 10.0, &st)], &st);
        let mut rtl = cluster("\u{05D0}", vec![glyph(2, 10.0, &st)], &st);
        rtl.direction = BidiDirection::Rtl;

        let l = layout(vec![
            at(item(ltr), 0.0, 0.0, 0),
            at(item(rtl), 10.0, 0.0, 0),
        ]);
        let runs = get_glyph_runs_pdf(&l, &fonts_with(&[FONT_A]));

        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].direction, BidiDirection::Ltr);
        assert_eq!(runs[1].direction, BidiDirection::Rtl);
    }

    #[test]
    fn pdf_writing_mode_change_breaks_the_run() {
        // writing_mode is read from the CLUSTER style while size/colour come from the
        // GLYPH style — this pins that the cluster-level property still breaks runs.
        let st = style();
        let vertical = styled(|s| s.writing_mode = WritingMode::VerticalRl);

        let horiz = cluster("a", vec![glyph(1, 10.0, &st)], &st);
        let mut vert = cluster("b", vec![glyph(2, 10.0, &st)], &st);
        vert.style = vertical;

        let l = layout(vec![
            at(item(horiz), 0.0, 0.0, 0),
            at(item(vert), 10.0, 0.0, 0),
        ]);
        let runs = get_glyph_runs_pdf(&l, &fonts_with(&[FONT_A]));

        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].writing_mode, WritingMode::HorizontalTb);
        assert_eq!(runs[1].writing_mode, WritingMode::VerticalRl);
    }

    #[test]
    fn pdf_background_color_change_breaks_the_run() {
        let plain = style();
        let highlighted = styled(|s| s.background_color = Some(rgba(255, 255, 0, 255)));
        let l = layout(vec![at(
            item(cluster(
                "ab",
                vec![glyph(1, 10.0, &plain), glyph(2, 10.0, &highlighted)],
                &plain,
            )),
            0.0,
            0.0,
            0,
        )]);
        let runs = get_glyph_runs_pdf(&l, &fonts_with(&[FONT_A]));

        assert_eq!(runs.len(), 2, "inline <span> background must get its own run");
        assert_eq!(runs[0].background_color, None);
        assert_eq!(runs[1].background_color, Some(rgba(255, 255, 0, 255)));
    }

    #[test]
    fn pdf_nan_font_size_breaks_every_glyph_into_its_own_run() {
        // `run.font_size_px != font_size_px` is always TRUE for NaN, so coalescing
        // is defeated — the mirror image of `simple_nan_font_size_forces_one_run_per_glyph`.
        let nan = styled(|s| s.font_size_px = f32::NAN);
        let l = layout(vec![at(
            item(cluster(
                "abc",
                vec![
                    glyph(1, 10.0, &nan),
                    glyph(2, 10.0, &nan),
                    glyph(3, 10.0, &nan),
                ],
                &nan,
            )),
            0.0,
            0.0,
            0,
        )]);
        let runs = get_glyph_runs_pdf(&l, &fonts_with(&[FONT_A]));

        assert_eq!(runs.len(), 3);
    }

    #[test]
    fn pdf_cluster_texts_are_parallel_to_glyphs() {
        // Every run must be able to map glyph i back to its cluster text.
        let st = style();
        let l = layout(vec![
            at(
                item(cluster("ab", vec![glyph(1, 10.0, &st), glyph(2, 10.0, &st)], &st)),
                0.0,
                0.0,
                0,
            ),
            at(item(cluster("c", vec![glyph(3, 10.0, &st)], &st)), 20.0, 0.0, 0),
        ]);
        let runs = get_glyph_runs_pdf(&l, &fonts_with(&[FONT_A]));

        assert_eq!(runs.len(), 1, "same style + same line => one run");
        assert_eq!(
            runs[0].cluster_texts.len(),
            runs[0].glyphs.len(),
            "cluster_texts is per-glyph, not per-cluster"
        );
        assert_eq!(runs[0].cluster_texts, vec!["ab", "ab", "c"]);
    }

    #[test]
    fn pdf_baseline_start_is_the_pen_before_the_gpos_offset() {
        let st = style();
        let mut g = glyph(1, 10.0, &st);
        g.offset = Point { x: 5.0, y: 2.0 };
        let l = layout(vec![at(item(cluster("a", vec![g], &st)), 100.0, 20.0, 0)]);
        let runs = get_glyph_runs_pdf(&l, &fonts_with(&[FONT_A]));

        assert_eq!(runs[0].baseline_start.x, 100.0, "text matrix origin excludes GPOS x");
        assert_eq!(runs[0].baseline_start.y, 20.0, "baseline == item y (ascent 0)");
        assert_eq!(runs[0].glyphs[0].position.x, 105.0, "the glyph itself carries GPOS x");
        assert_eq!(runs[0].glyphs[0].position.y, 18.0, "GPOS y is subtracted (Y-down)");
    }

    #[test]
    fn pdf_pen_accumulates_advance_plus_kerning_within_a_cluster() {
        let st = style();
        let mut g0 = glyph(1, 10.0, &st);
        g0.kerning = -3.0;
        let g1 = glyph(2, 10.0, &st);
        let l = layout(vec![at(item(cluster("ab", vec![g0, g1], &st)), 0.0, 0.0, 0)]);
        let runs = get_glyph_runs_pdf(&l, &fonts_with(&[FONT_A]));

        assert_eq!(runs[0].glyphs[0].position.x, 0.0);
        assert_eq!(runs[0].glyphs[1].position.x, 7.0, "advance 10 + kerning -3");
    }

    #[test]
    fn pdf_infinite_advance_does_not_panic() {
        let st = style();
        let l = layout(vec![at(
            item(cluster(
                "ab",
                vec![glyph(1, f32::INFINITY, &st), glyph(2, 10.0, &st)],
                &st,
            )),
            0.0,
            0.0,
            0,
        )]);
        let runs = get_glyph_runs_pdf(&l, &fonts_with(&[FONT_A]));

        assert_eq!(runs[0].glyphs.len(), 2);
        assert!(runs[0].glyphs[1].position.x.is_infinite());
        assert!(runs[0].glyphs[0].advance.is_infinite(), "advance is copied verbatim");
    }
}
