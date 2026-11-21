//! A paragraph laid out through a chain of frames reads as one paragraph:
//! the frames after the first go on with its base direction, its line
//! numbers and the inline boxes its lines are in, and every frame keeps its
//! own lines.
//!
//! Each fragment of a flow started its lines afresh. `unicode-bidi:
//! plaintext` took the base direction from what the fragment started with
//! instead of the paragraph's first strong character, `line-clamp` and the
//! line numbers counted again from zero, a span the break between two
//! frames cut drew its start edge again at the next frame's first line, and
//! two frames with one id kept only the last one's lines.
//!
//! Glyphs here are Azul Mock Mono at 20px: 10px wide, in 20px line boxes.

use std::{num::NonZeroUsize, sync::Arc};

use azul_core::dom::NodeId;
use azul_layout::text3::cache::{
    InlineBoxEdge, InlineContent, TextAlign, UnicodeBidi, UnifiedConstraints,
};

use crate::mono_lines::{box_of_width, flow, flow_content, glyph_boxes, mono, paragraph};

/// A frame `width` wide and `lines` lines tall.
fn frame(width: f32, lines: u8) -> UnifiedConstraints {
    UnifiedConstraints {
        available_height: Some(f32::from(lines) * 20.0),
        ..box_of_width(width)
    }
}

#[test]
fn a_later_frame_keeps_the_paragraphs_direction() {
    // `unicode-bidi: plaintext` with no direction of its own: the paragraph
    // starts in Hebrew, so it is right to left, and so is its second line,
    // which starts in Latin letters, in the next frame.
    let plaintext = |lines: u8| UnifiedConstraints {
        unicode_bidi: UnicodeBidi::Plaintext,
        direction: None,
        text_align: TextAlign::Start,
        ..frame(90.0, lines)
    };
    let (layouts, left_over) = flow(
        "\u{5d0}\u{5d0}\u{5d0}\u{5d0}\u{5d0}\u{5d0}\u{5d0}\u{5d0} bbbbbbbb",
        &[("a", plaintext(1)), ("b", plaintext(1))],
    );
    assert_eq!(left_over, 0, "the two frames hold the paragraph");
    let second = glyph_boxes(&layouts[1]);
    assert_eq!(second.len(), 8, "the second frame holds the Latin word: {second:?}");
    let right = second.iter().map(|g| g.right).fold(f32::MIN, f32::max);
    assert!(
        (right - 90.0).abs() < 0.01,
        "a right-to-left line starts at the frame's right edge, the word ends at {right}"
    );
}

/// Ten words of four letters: one word per line in a 60px wide frame.
const TEN_WORDS: &str = "aaaa bbbb cccc dddd eeee ffff gggg hhhh iiii jjjj";

#[test]
fn the_lines_of_a_later_frame_go_on_with_the_paragraphs_numbers() {
    let (layouts, _) = flow(TEN_WORDS, &[("a", frame(60.0, 2)), ("b", frame(60.0, 2))]);
    let first_of_second = layouts[1]
        .items
        .iter()
        .map(|item| item.line_index)
        .min()
        .expect("the second frame holds lines");
    assert_eq!(first_of_second, 2, "the second frame's first line is the paragraph's third");
}

#[test]
fn line_clamp_counts_the_lines_of_every_frame() {
    let clamped = |lines: u8| UnifiedConstraints {
        line_clamp: NonZeroUsize::new(3),
        ..frame(60.0, lines)
    };
    let (layouts, _) = flow(TEN_WORDS, &[("a", clamped(2)), ("b", clamped(2))]);
    let lines: Vec<usize> = layouts
        .iter()
        .map(|layout| {
            let mut indices: Vec<usize> = layout.items.iter().map(|item| item.line_index).collect();
            indices.sort_unstable();
            indices.dedup();
            indices.len()
        })
        .collect();
    assert_eq!(lines, vec![2, 1], "three lines in all: two in the first frame, one after");
}

#[test]
fn a_span_cut_between_two_frames_has_no_start_edge_in_the_second() {
    // The whole paragraph sits in one span with 20px of padding at its
    // start: the first frame's first line starts after it, the second
    // frame's first line goes on in the middle of the span.
    let font = mono();
    let mut content = paragraph(&font, TEN_WORDS);
    if let Some(InlineContent::Text(run)) = content.first_mut() {
        let mut style = (*run.style).clone();
        style.inline_boxes = Arc::from([InlineBoxEdge {
            node: NodeId::new(1),
            left: 20.0,
            right: 0.0,
        }]);
        run.style = Arc::new(style);
    }
    let (layouts, _) = flow_content(
        &font,
        &content,
        &[("a", frame(80.0, 2)), ("b", frame(80.0, 2))],
    );
    let first = glyph_boxes(&layouts[0]);
    assert!(
        first.first().is_some_and(|g| (g.left - 20.0).abs() < 0.01),
        "the span starts the paragraph after its padding: {first:?}"
    );
    let second = glyph_boxes(&layouts[1]);
    assert!(
        second.first().is_some_and(|g| g.left.abs() < 0.01),
        "the second frame goes on inside the span, with no padding: {second:?}"
    );
}

#[test]
fn two_frames_with_one_id_keep_their_lines_apart() {
    use azul_layout::text3::cache::{LayoutFragment, LoadedFonts, TextShapingCache};
    use rust_fontconfig::{FcFontCache, FontId};

    let font = mono();
    let content = paragraph(&font, TEN_WORDS);
    let mut fonts: LoadedFonts<azul_css::props::basic::FontRef> = LoadedFonts::new();
    fonts.insert(FontId::new(), font);
    let chain: Vec<LayoutFragment> = ["column", "column"]
        .iter()
        .map(|id| LayoutFragment {
            id: (*id).to_string(),
            constraints: frame(60.0, 2),
        })
        .collect();
    let flowed = TextShapingCache::new()
        .layout_flow(
            &content,
            &[],
            &chain,
            &std::collections::HashMap::new(),
            &FcFontCache::default(),
            &fonts,
            &mut None,
        )
        .expect("flow layout");
    assert_eq!(flowed.fragment_layouts.len(), 2, "both frames keep a layout");
    let lines: usize = flowed
        .fragment_layouts
        .values()
        .map(|layout| {
            let mut indices: Vec<usize> = layout.items.iter().map(|item| item.line_index).collect();
            indices.sort_unstable();
            indices.dedup();
            indices.len()
        })
        .sum();
    assert_eq!(lines, 4, "two lines in each frame");
}
