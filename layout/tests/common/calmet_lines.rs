//! A paragraph set in Calmet UI through the text pipeline's stages, as the
//! solver runs them (logical items, bidi, shaping, line breaking and
//! positioning), and read back line by line.
//!
//! Calmet UI is the font of these tests because it is in the tree (no system
//! font decides a number), has "æ", and ligates "fi", "ff" and "ffi" into one
//! cluster each.

use std::{collections::HashMap, sync::Arc};

use azul_css::props::basic::FontRef;
use azul_layout::{
    font::parsed::ParsedFont,
    parsed_font_to_font_ref,
    text3::cache::{
        create_logical_items, get_item_measure, perform_fragment_layout, reorder_logical_items,
        shape_visual_items, AvailableSpace, BidiDirection, BreakCursor, FontChainKey, FontStack,
        GlyphKind, InlineContent, LoadedFonts, ShapedCluster, ShapedItem, StyleProperties,
        StyledRun, UnicodeBidi, UnifiedConstraints, UnifiedLayout,
    },
};
use rust_fontconfig::{FcFontCache, FontBytes, FontFallbackChain, FontId};

/// The font size every paragraph is set in.
pub const FONT_SIZE: f32 = 20.0;

/// Calmet UI's regular face.
pub fn calmet_ui() -> FontRef {
    let bytes = azul_fonts_calmet::faces()[0].clone();
    let source = Arc::new(FontBytes::Owned(Arc::from(bytes.as_slice())));
    let parsed = ParsedFont::from_bytes(&bytes, 0, &mut Vec::new())
        .expect("Calmet UI parses")
        .with_source_bytes(source);
    parsed_font_to_font_ref(parsed)
}

/// `text` in `font` at [`FONT_SIZE`] in a box `width` px wide, left to
/// right; `style` and `constraints` set what a test varies (spacing,
/// alignment, hyphenation).
pub fn lay_out(
    font: &FontRef,
    text: &str,
    width: f32,
    style: impl FnOnce(&mut StyleProperties),
    constraints: impl FnOnce(&mut UnifiedConstraints),
) -> UnifiedLayout {
    let mut run_style = StyleProperties {
        font_stack: FontStack::Ref(font.clone()),
        font_size_px: FONT_SIZE,
        ..StyleProperties::default()
    };
    style(&mut run_style);
    let content = vec![InlineContent::Text(StyledRun {
        text: Arc::from(text),
        style: Arc::new(run_style),
        logical_start_byte: 0,
        source_node_id: None,
    })];
    let mut paragraph = UnifiedConstraints {
        available_width: AvailableSpace::Definite(width),
        ..UnifiedConstraints::default()
    };
    constraints(&mut paragraph);
    let mut fonts: LoadedFonts<FontRef> = LoadedFonts::new();
    fonts.insert(FontId::new(), font.clone());
    let logical = create_logical_items(&content, &[], &mut None);
    let visual =
        reorder_logical_items(&logical, BidiDirection::Ltr, UnicodeBidi::Normal, &mut None)
            .expect("bidi reorder");
    let chains: HashMap<FontChainKey, FontFallbackChain> = HashMap::new();
    let shaped = shape_visual_items(&visual, &chains, &FcFontCache::default(), &fonts, &mut None)
        .expect("shaped");
    // The cursor carries the paragraph's breaking properties, as in the solver.
    let mut cursor = BreakCursor::with_word_break(&shaped, paragraph.word_break);
    cursor.hyphens = paragraph.hyphenation;
    cursor.line_break = paragraph.line_break;
    perform_fragment_layout(&mut cursor, &logical, &paragraph, &mut None, &fonts).expect("laid out")
}

/// A hyphen the line breaker put at the end of a line (not one of the text).
pub fn is_inserted_hyphen(c: &ShapedCluster) -> bool {
    c.glyphs.iter().any(|g| matches!(g.kind, GlyphKind::Hyphen))
}

/// One line of a layout.
pub struct Line<'a> {
    /// The line's clusters in visual order.
    pub clusters: Vec<&'a ShapedCluster>,
    /// Where the line's last cluster ends: its x plus its advance and
    /// kerning, without the spacing the pen adds after it.
    pub end: f32,
}

impl Line<'_> {
    /// The line's text, inserted hyphens included.
    pub fn text(&self) -> String {
        self.clusters.iter().map(|c| c.text()).collect()
    }
}

/// The lines of `layout`, top to bottom.
pub fn lines(layout: &UnifiedLayout) -> Vec<Line<'_>> {
    let mut out: Vec<Line<'_>> = Vec::new();
    for item in &layout.items {
        let ShapedItem::Cluster(c) = &item.item else {
            continue;
        };
        while out.len() <= item.line_index {
            out.push(Line {
                clusters: Vec::new(),
                end: f32::MIN,
            });
        }
        let line = &mut out[item.line_index];
        line.clusters.push(c);
        line.end = line
            .end
            .max(item.position.x + get_item_measure(&item.item, false));
    }
    out
}
