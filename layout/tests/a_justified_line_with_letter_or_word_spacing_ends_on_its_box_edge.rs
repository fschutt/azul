//! A justified line ends on its box's edge whatever its letter- and
//! word-spacing.
//!
//! The pen adds the letter-spacing after every character and the
//! word-spacing after every space, so the line breaker measures them, and a
//! centred or right-aligned line is aligned by the width with them. The
//! justification took the room it had to fill from the bare advances: a line
//! with letter-spacing -0.02em ended 3 to 14 px short of a 300 pt column, one
//! with +0.05em up to 30 px past it (pdfocr engine issue 5).
//!
//! In Chrome a justified line's content fills its line box, the spacing after
//! its last letter included: with `letter-spacing: 5px` the last letter's
//! box ends on the edge and its glyph 5 px before it (a letter's box there
//! holds its advance and its spacing). `text-justify: inter-character` puts
//! the extra space between letters, never after the line's last one.

use azul_layout::text3::cache::{JustifyContent, Spacing, TextAlign};

use crate::calmet_lines::{calmet_ui, lay_out, lines, FONT_SIZE};

const TEXT: &str = "The Scripture calls them sometimes the Children of Anak, because they \
                    were descended from Anak the son of Arba, who built the city of Hebron; \
                    and they were of a gigantic stature, which made the Israelites afraid.";

const WIDTH: f32 = 300.0;

/// Every line but the last of the paragraph justified with `text_justify`,
/// `letter_spacing` and `word_spacing` (in em): where the pen is after its
/// last letter, the letter-spacing after it included.
fn line_ends(text_justify: JustifyContent, letter_spacing: f32, word_spacing: f32) -> Vec<f32> {
    let layout = lay_out(
        &calmet_ui(),
        TEXT,
        WIDTH,
        |style| {
            style.letter_spacing = Spacing::Em(letter_spacing);
            style.word_spacing = Spacing::Em(word_spacing);
        },
        |paragraph| {
            paragraph.text_align = TextAlign::Justify;
            paragraph.text_justify = text_justify;
        },
    );
    let lines = lines(&layout);
    assert!(
        lines.len() >= 4,
        "harness: the paragraph wraps over several lines: {:?}",
        lines.iter().map(|l| l.text()).collect::<Vec<_>>()
    );
    lines[..lines.len() - 1]
        .iter()
        .map(|line| line.end + letter_spacing * FONT_SIZE)
        .collect()
}

fn assert_on_the_edge(what: &str, ends: &[f32]) {
    for (index, end) in ends.iter().enumerate() {
        assert!(
            (end - WIDTH).abs() <= 0.5,
            "{what}: line {index} ends on the {WIDTH}px box's edge, not at {end:.2} (all: \
             {ends:?})"
        );
    }
}

#[test]
fn a_justified_line_without_spacing_ends_on_its_box_edge() {
    assert_on_the_edge(
        "no spacing",
        &line_ends(JustifyContent::InterWord, 0.0, 0.0),
    );
}

#[test]
fn a_justified_line_with_letter_spacing_ends_on_its_box_edge() {
    for letter_spacing in [-0.05, -0.02, 0.05] {
        assert_on_the_edge(
            &format!("letter-spacing {letter_spacing}em"),
            &line_ends(JustifyContent::InterWord, letter_spacing, 0.0),
        );
    }
}

#[test]
fn a_justified_line_with_word_spacing_ends_on_its_box_edge() {
    for word_spacing in [-0.1, 0.1] {
        assert_on_the_edge(
            &format!("word-spacing {word_spacing}em"),
            &line_ends(JustifyContent::InterWord, 0.0, word_spacing),
        );
    }
}

#[test]
fn a_line_justified_between_its_letters_ends_on_its_box_edge() {
    for letter_spacing in [0.0, 0.05] {
        assert_on_the_edge(
            &format!("inter-character, letter-spacing {letter_spacing}em"),
            &line_ends(JustifyContent::InterCharacter, letter_spacing, 0.0),
        );
    }
}
