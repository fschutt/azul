//! A line with a hole in the middle of its room is broken on each side of
//! the hole: no word crosses it.
//!
//! An exclusion or a hole in a `shape-inside` cuts a line's room into
//! segments. The line breaker fitted the words against the sum of the
//! segments' widths and then spread the line's clusters over the segments
//! one by one, so a word was torn in two across the hole, without a hyphen,
//! and the last segment overflowed. Each segment is now a line box of its
//! own for breaking, hyphenation, alignment and justification, filled in
//! reading order; a sliver too narrow for text holds none.
//!
//! Glyphs here are Azul Mock Mono at 20px: 10px wide, in 20px line boxes.

use azul_layout::text3::cache::{
    BidiDirection, Rect, ShapeBoundary, TextAlign, UnifiedConstraints,
};

use crate::{
    mono_lines::{box_of_width, glyph_boxes, inked_pens, lay_out, GlyphBox, GLYPH, LINE},
    table_markup::laid_out,
};

/// Ten words of ten letters each.
const WORDS: &str = "aaaaaaaaaa bbbbbbbbbb cccccccccc dddddddddd eeeeeeeeee ffffffffff gggggggggg \
                     hhhhhhhhhh iiiiiiiiii jjjjjjjjjj";

/// A 400px paragraph with the hole `x0..x1` from the top down to `bottom`.
fn with_hole(x0: f32, x1: f32, bottom: f32) -> UnifiedConstraints {
    let mut paragraph = box_of_width(400.0);
    paragraph.shape_exclusions = vec![ShapeBoundary::Rectangle(Rect {
        x: x0,
        y: 0.0,
        width: x1 - x0,
        height: bottom,
    })];
    paragraph
}

/// The words of `glyphs` as (first letter, left edge, right edge, line).
fn words(glyphs: &[GlyphBox]) -> Vec<(String, f32, f32, usize)> {
    let mut out: Vec<(String, f32, f32, usize)> = Vec::new();
    for g in glyphs {
        match out.last_mut() {
            Some((text, _, right, line))
                if *line == g.line && (g.left - *right).abs() < 0.01 && text.ends_with(&g.text) =>
            {
                text.push_str(&g.text);
                *right = g.right;
            }
            _ => out.push((g.text.clone(), g.left, g.right, g.line)),
        }
    }
    out
}

#[test]
fn no_word_crosses_the_hole() {
    let glyphs = glyph_boxes(&lay_out(WORDS, &with_hole(150.0, 250.0, 60.0)));
    for g in glyphs.iter().filter(|g| g.top < 60.0) {
        assert!(
            g.right <= 150.0 + 0.01 || g.left >= 250.0 - 0.01,
            "{:?} on line {} at {}..{} lies in the hole 150..250",
            g.text,
            g.line,
            g.left,
            g.right
        );
    }
    for (word, left, right, line) in words(&glyphs) {
        assert_eq!(
            word.len(),
            10,
            "line {line} holds the piece {word:?} at {left}..{right}: a word was torn"
        );
    }
    // Reading order: the first word left of the hole, the second right of it.
    let first_line: Vec<_> = words(&glyphs)
        .into_iter()
        .filter(|w| w.3 == 0)
        .collect();
    assert_eq!(
        first_line
            .iter()
            .map(|w| (w.0.as_str(), w.1))
            .collect::<Vec<_>>(),
        vec![("aaaaaaaaaa", 0.0), ("bbbbbbbbbb", 250.0)]
    );
}

#[test]
fn a_right_to_left_line_fills_the_segment_on_the_right_first() {
    let mut paragraph = with_hole(150.0, 250.0, 60.0);
    paragraph.direction = Some(BidiDirection::Rtl);
    let glyphs = glyph_boxes(&lay_out(WORDS, &paragraph));
    let first = words(&glyphs)
        .into_iter()
        .find(|w| w.0.starts_with('a'))
        .expect("the first word is laid out");
    assert!(
        first.1 >= 250.0 - 0.01 && first.3 == 0,
        "the first word starts the line on the right of the hole: {first:?}"
    );
}

#[test]
fn each_segment_is_justified_to_its_own_edges() {
    let mut paragraph = with_hole(150.0, 250.0, 60.0);
    paragraph.text_align = TextAlign::Justify;
    // Words of four letters: three of them and their two spaces take 140px
    // of a segment's 150, and justification spreads the other 10.
    let text = ["aaaa"; 30].join(" ");
    let glyphs = glyph_boxes(&lay_out(&text, &paragraph));
    let last_line = glyphs.iter().map(|g| g.line).max().unwrap_or(0);
    for line in 0..2 {
        let on_line: Vec<_> = glyphs.iter().filter(|g| g.line == line).collect();
        let left_end = on_line
            .iter()
            .filter(|g| g.right <= 150.01)
            .map(|g| g.right)
            .fold(0.0_f32, f32::max);
        let right_end = on_line
            .iter()
            .filter(|g| g.left >= 249.99)
            .map(|g| g.right)
            .fold(0.0_f32, f32::max);
        assert!(line < last_line, "the text runs past the hole");
        assert!(
            (left_end - 150.0).abs() < 0.01 && (right_end - 400.0).abs() < 0.01,
            "line {line}: the left segment ends at {left_end}, the right one at {right_end}"
        );
    }
}

#[test]
fn a_sliver_beside_a_hole_holds_no_text() {
    // The hole leaves 15px on its left: less than an em of the 20px font.
    let glyphs = glyph_boxes(&lay_out(WORDS, &with_hole(15.0, 200.0, 60.0)));
    for g in glyphs.iter().filter(|g| g.top < 60.0) {
        assert!(
            g.left >= 200.0 - 0.01,
            "{:?} on line {} at {}..{} sits in the 15px sliver or the hole",
            g.text,
            g.line,
            g.left,
            g.right
        );
    }
}

#[test]
fn a_frame_with_a_hole_keeps_its_words_whole() {
    // R2 of the fragmentainer audit as a frame: a 400 x 100 frame with a
    // 100px hole in the middle of its top 60px.
    let html = format!(
        "<html><head><style>body {{ margin: 0 }} .f {{ position: absolute; left: 20px; top: \
         20px; width: 400px; height: 100px; font-family: 'Azul Mock Mono'; font-size: 20px; \
         line-height: 20px; shape-inside: path(evenodd, 'M0 0H400V100H0Z M150 0H250V60H150Z') \
         }}</style></head><body><div class=\"f\">{WORDS}</div></body></html>"
    );
    let pens = inked_pens(&laid_out(&html, 800.0, 600.0));
    assert!(!pens.is_empty(), "the frame holds text");
    let mut rows: Vec<(f32, Vec<f32>)> = Vec::new();
    for &(x, baseline) in &pens {
        let (left, top) = (x - 20.0, baseline - 16.0 - 20.0);
        if top + LINE <= 60.0 + 0.01 {
            assert!(
                left + GLYPH <= 150.0 + 0.01 || left >= 250.0 - 0.01,
                "a glyph at {left} on the line at {top} lies in the hole"
            );
        }
        match rows.iter_mut().find(|(t, _)| (t - top).abs() < 0.01) {
            Some((_, xs)) => xs.push(left),
            None => rows.push((top, vec![left])),
        }
    }
    for (top, mut xs) in rows {
        xs.sort_by(f32::total_cmp);
        // Every run of touching glyphs is a whole word of ten letters.
        let mut run = 1;
        for pair in xs.windows(2) {
            if (pair[1] - pair[0] - GLYPH).abs() < 0.01 {
                run += 1;
            } else {
                assert_eq!(run, 10, "a torn word on the line at {top}: {xs:?}");
                run = 1;
            }
        }
        assert_eq!(run, 10, "a torn word on the line at {top}: {xs:?}");
    }
}
