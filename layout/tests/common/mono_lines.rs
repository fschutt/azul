//! A paragraph set in Azul Mock Mono through the text pipeline's stages and
//! read back as glyph boxes, for tests of where lines may go: inside a
//! shape, around an exclusion, through a chain of frames.
//!
//! Every glyph of Azul Mock Mono is half an em wide with an ascent of 0.8 em
//! and a descent of 0.2 em, so at [`FONT_SIZE`] a glyph is exactly 10 px wide
//! and its line box exactly 20 px tall: a line's band and a glyph's box are
//! numbers a test can write down.

#![allow(dead_code)] // shared test helper: not every test uses every function

use std::{collections::HashMap, sync::Arc};

use azul_css::props::basic::FontRef;
use azul_layout::{
    font::parsed::ParsedFont,
    parsed_font_to_font_ref,
    text3::{
        cache::{
            create_logical_items, perform_fragment_layout, shape_bidi_runs,
            split_into_bidi_runs, AvailableSpace, BidiDirection, BreakCursor, FontChainKey,
            FontStack, InlineContent, LayoutFragment, LineHeight, LoadedFonts, OverflowInfo,
            ShapedItem,
            StyleProperties, StyledRun, TextShapingCache, UnicodeBidi, UnifiedConstraints,
            UnifiedLayout,
        },
        mock_fonts::MOCK_MONO_TTF,
    },
};
use azul_layout::{solver3::display_list::DisplayListItem, window::LayoutWindow};
use rust_fontconfig::{FcFontCache, FontBytes, FontFallbackChain, FontId};

/// The font size every paragraph is set in: 10 px glyphs, 20 px lines.
pub const FONT_SIZE: f32 = 20.0;

/// The width of every glyph at [`FONT_SIZE`].
pub const GLYPH: f32 = 10.0;

/// The height of every line box at [`FONT_SIZE`].
pub const LINE: f32 = 20.0;

/// Azul Mock Mono.
pub fn mono() -> FontRef {
    let shared = Arc::new(FontBytes::Owned(Arc::from(MOCK_MONO_TTF)));
    let parsed = ParsedFont::from_bytes(MOCK_MONO_TTF, 0, &mut Vec::new())
        .expect("Azul Mock Mono parses")
        .with_source_bytes(shared);
    parsed_font_to_font_ref(parsed)
}

/// The paragraph `text` in Azul Mock Mono at [`FONT_SIZE`].
pub fn paragraph(font: &FontRef, text: &str) -> Vec<InlineContent> {
    vec![InlineContent::Text(StyledRun {
        text: Arc::from(text),
        style: Arc::new(StyleProperties {
            font_stack: FontStack::Ref(font.clone()),
            font_size_px: FONT_SIZE,
            ..StyleProperties::default()
        }),
        logical_start_byte: 0,
        source_node_id: None,
    })]
}

/// The constraints of a left-to-right paragraph `width` wide whose lines are
/// [`LINE`] tall, the strut that of the font.
pub fn box_of_width(width: f32) -> UnifiedConstraints {
    UnifiedConstraints {
        available_width: AvailableSpace::Definite(width),
        line_height: LineHeight::Px(LINE),
        strut_ascent: FONT_SIZE * 0.8,
        strut_descent: FONT_SIZE * 0.2,
        strut_font_size: FONT_SIZE,
        direction: Some(BidiDirection::Ltr),
        ..UnifiedConstraints::default()
    }
}

/// `text` laid out in one fragment with `constraints`.
pub fn lay_out(text: &str, constraints: &UnifiedConstraints) -> UnifiedLayout {
    let font = mono();
    let content = paragraph(&font, text);
    let mut fonts: LoadedFonts<FontRef> = LoadedFonts::new();
    fonts.insert(FontId::new(), font);
    let logical = create_logical_items(&content, &[], &mut None);
    let runs = split_into_bidi_runs(&logical, BidiDirection::Ltr, UnicodeBidi::Normal, &mut None)
        .expect("bidi analysis");
    let chains: HashMap<FontChainKey, FontFallbackChain> = HashMap::new();
    let shaped = shape_bidi_runs(&runs, &chains, &FcFontCache::default(), &fonts, &mut None)
        .expect("shaping");
    let mut cursor = BreakCursor::for_constraints(&shaped, constraints);
    perform_fragment_layout(&mut cursor, &logical, constraints, &mut None, &fonts)
        .expect("line layout")
}

/// `text` flowed through `fragments` in order: each fragment's layout, in
/// the order of the chain, and how many shaped items were left over.
pub fn flow(text: &str, fragments: &[(&str, UnifiedConstraints)]) -> (Vec<UnifiedLayout>, usize) {
    let font = mono();
    let content = paragraph(&font, text);
    let mut fonts: LoadedFonts<FontRef> = LoadedFonts::new();
    fonts.insert(FontId::new(), font);
    let chain: Vec<LayoutFragment> = fragments
        .iter()
        .map(|(id, constraints)| LayoutFragment {
            id: (*id).to_string(),
            constraints: constraints.clone(),
        })
        .collect();
    let mut cache = TextShapingCache::new();
    let flowed = cache
        .layout_flow(
            &content,
            &[],
            &chain,
            &HashMap::new(),
            &FcFontCache::default(),
            &fonts,
            &mut None,
        )
        .expect("flow layout");
    let layouts = fragments
        .iter()
        .map(|(id, _)| {
            flowed
                .fragment_layouts
                .get(*id)
                .map_or_else(
                    || UnifiedLayout {
                        items: Vec::new(),
                        overflow: OverflowInfo::default(),
                    },
                    |layout| (**layout).clone(),
                )
        })
        .collect();
    (layouts, flowed.remaining_items.len())
}

/// One glyph of a layout: its text, its line and its box.
#[derive(Debug, Clone, PartialEq)]
pub struct GlyphBox {
    pub text: String,
    pub line: usize,
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,
}

/// Every glyph of `layout` that draws something (spaces left out), in
/// layout order: its line box is [`LINE`] tall from the item's top.
pub fn glyph_boxes(layout: &UnifiedLayout) -> Vec<GlyphBox> {
    layout
        .items
        .iter()
        .filter_map(|item| match &item.item {
            ShapedItem::Cluster(c) if !c.text().trim().is_empty() => Some(GlyphBox {
                text: c.text().to_string(),
                line: item.line_index,
                left: item.position.x,
                right: item.position.x + c.advance,
                top: item.position.y,
                bottom: item.position.y + LINE,
            }),
            _ => None,
        })
        .collect()
}

/// The text of each line of `layout`, top to bottom, spaces included.
pub fn line_texts(layout: &UnifiedLayout) -> Vec<String> {
    let mut lines: Vec<(usize, f32, String)> = Vec::new();
    for item in &layout.items {
        if let ShapedItem::Cluster(c) = &item.item {
            lines.push((item.line_index, item.position.x, c.text().to_string()));
        }
    }
    lines.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)));
    let mut out: Vec<(usize, String)> = Vec::new();
    for (line, _, text) in lines {
        match out.last_mut() {
            Some((last, s)) if *last == line => s.push_str(&text),
            _ => out.push((line, text)),
        }
    }
    out.into_iter().map(|(_, s)| s).collect()
}

/// The letters `text` holds (what a layout must draw, spaces aside).
pub fn letters(text: &str) -> usize {
    text.chars().filter(|c| !c.is_whitespace()).count()
}

/// The glyph id of the space in Azul Mock Mono: a blank glyph, the others
/// follow from 0x21 on.
pub const SPACE_GLYPH: u32 = 1;

/// The pen (left end of the baseline) of every inked glyph `lw` draws.
pub fn inked_pens(lw: &LayoutWindow) -> Vec<(f32, f32)> {
    crate::table_markup::items(lw)
        .into_iter()
        .filter_map(|item| match item {
            DisplayListItem::Text { glyphs, .. } => Some(glyphs),
            _ => None,
        })
        .flatten()
        .filter(|g| g.index != SPACE_GLYPH)
        .map(|g| (g.point.x, g.point.y))
        .collect()
}
