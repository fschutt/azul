//! A fragment of a flow holds the lines that end inside it; the next line
//! goes on in the next fragment.
//!
//! CSS Fragmentation 3: a line box is monolithic, it fits a fragmentainer
//! or goes to the next one. The line breaker tested only whether a line
//! STARTED above the fragment's bottom, so the last line of a frame or a
//! column could hang below it by almost a whole line.
//!
//! Glyphs here are Azul Mock Mono at 20px, in 20px line boxes.

use azul_layout::text3::cache::UnifiedConstraints;

use crate::mono_lines::{box_of_width, flow, line_texts, LINE};

/// Ten words of four letters: one word per line in a 60px wide frame.
const TEN_WORDS: &str = "aaaa bbbb cccc dddd eeee ffff gggg hhhh iiii jjjj";

/// A frame `height` px tall and 60px wide.
fn frame(height: f32) -> UnifiedConstraints {
    UnifiedConstraints {
        available_height: Some(height),
        ..box_of_width(60.0)
    }
}

#[test]
fn a_frame_one_and_a_half_lines_tall_holds_one_line() {
    let (layouts, left_over) = flow(TEN_WORDS, &[("a", frame(30.0)), ("b", frame(200.0))]);
    let a = line_texts(&layouts[0]);
    assert_eq!(
        a,
        vec!["aaaa".to_string()],
        "a second line would end at 40px, below the 30px frame"
    );
    let bottom = layouts[0]
        .items
        .iter()
        .map(|item| item.position.y + LINE)
        .fold(0.0_f32, f32::max);
    assert!(bottom <= 30.0, "the frame's lines end at {bottom}px");
    assert_eq!(
        line_texts(&layouts[1]).first().map(String::as_str),
        Some("bbbb"),
        "the next frame goes on with the second line"
    );
    assert_eq!(left_over, 0, "the second frame holds the rest");
}

#[test]
fn a_frame_exactly_two_lines_tall_holds_two_lines() {
    let (layouts, _) = flow(TEN_WORDS, &[("a", frame(40.0)), ("b", frame(200.0))]);
    assert_eq!(line_texts(&layouts[0]), vec!["aaaa", "bbbb"]);
    assert_eq!(
        line_texts(&layouts[1]).first().map(String::as_str),
        Some("cccc")
    );
}
