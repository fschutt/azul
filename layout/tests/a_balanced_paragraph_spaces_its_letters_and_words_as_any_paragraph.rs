//! A balanced paragraph (`text-wrap: balance`) spaces its letters and words
//! as every paragraph does.
//!
//! The pen adds the letter-spacing after every character and the
//! word-spacing after every space, and the lines are chosen by the widths
//! with that spacing in them. Headless Chrome agrees: in a 300px box of
//! 20px Times with `letter-spacing: 5px`, the "T" of a balanced paragraph
//! has the box of a wrapped paragraph's (12.22 + 5 px), the paragraph takes
//! five lines instead of three, and no line is wider than its box.
//!
//! The balanced paragraph has a line breaker of its own (Knuth-Plass); it
//! measured and placed every item by its bare advance, so its letters stood
//! as close as without spacing and a justified balanced line with spacing
//! did not end on its box's edge.

use azul_layout::text3::cache::{
    JustifyContent, ShapedItem, Spacing, StyleProperties, TextAlign, TextWrap, UnifiedConstraints,
    UnifiedLayout,
};

use crate::calmet_lines::{calmet_ui, lay_out, lines};

const TEXT: &str = "The Scripture calls them sometimes the Children of Anak, because they \
                    were descended from Anak the son of Arba, who built the city of Hebron.";

const WIDTH: f32 = 300.0;

fn laid_out(
    wrap: TextWrap,
    style: impl FnOnce(&mut StyleProperties),
    align: TextAlign,
) -> UnifiedLayout {
    lay_out(
        &calmet_ui(),
        TEXT,
        WIDTH,
        style,
        |paragraph: &mut UnifiedConstraints| {
            paragraph.text_wrap = wrap;
            paragraph.text_align = align;
            // `text-justify: auto`, as the solver resolves it.
            paragraph.text_justify = JustifyContent::InterWord;
        },
    )
}

/// Where each cluster of the first line starts, as many as `count`.
fn first_line_starts(layout: &UnifiedLayout, count: usize) -> Vec<(String, f32)> {
    layout
        .items
        .iter()
        .filter(|item| item.line_index == 0)
        .filter_map(|item| match &item.item {
            ShapedItem::Cluster(c) => Some((c.text().to_string(), item.position.x)),
            _ => None,
        })
        .take(count)
        .collect()
}

/// "The Scripture" starts the first line of both paragraphs: its letters
/// stand where the wrapped paragraph's stand.
fn assert_spaced_as_wrapped(what: &str, style: impl Fn(&mut StyleProperties)) {
    let balanced = first_line_starts(&laid_out(TextWrap::Balance, &style, TextAlign::Left), 13);
    let wrapped = first_line_starts(&laid_out(TextWrap::Wrap, &style, TextAlign::Left), 13);
    assert_eq!(balanced.len(), 13, "harness: {balanced:?}");
    for (b, w) in balanced.iter().zip(&wrapped) {
        assert_eq!(b.0, w.0, "harness: both lines start with the same text");
        assert!(
            (b.1 - w.1).abs() <= 0.01,
            "{what}: the balanced line's {:?} starts at {:.2}, the wrapped line's at {:.2} \
             (balanced {balanced:?}, wrapped {wrapped:?})",
            b.0,
            b.1,
            w.1
        );
    }
}

#[test]
fn a_balanced_line_puts_the_letter_spacing_after_every_letter() {
    assert_spaced_as_wrapped("letter-spacing 5px", |style| {
        style.letter_spacing = Spacing::PxF(5.0);
    });
}

#[test]
fn a_balanced_line_puts_the_word_spacing_after_every_space() {
    assert_spaced_as_wrapped("word-spacing 8px", |style| {
        style.word_spacing = Spacing::PxF(8.0);
    });
}

#[test]
fn no_balanced_line_with_letter_spacing_is_wider_than_its_box() {
    let layout = laid_out(
        TextWrap::Balance,
        |style| style.letter_spacing = Spacing::PxF(5.0),
        TextAlign::Left,
    );
    let lines = lines(&layout);
    assert!(lines.len() >= 3, "harness: the paragraph wraps");
    for (index, line) in lines.iter().enumerate() {
        // The letter-spacing after the line's last letter is in its width.
        let width = line.end + 5.0;
        assert!(
            width <= WIDTH + 0.5,
            "line {index} ({:?}) is {width:.2} px wide in a {WIDTH} px box",
            line.text()
        );
    }
}

#[test]
fn a_justified_balanced_line_with_letter_or_word_spacing_ends_on_its_box_edge() {
    for (letter, word) in [(0.0, 0.0), (5.0, 0.0), (-1.0, 0.0), (0.0, 8.0)] {
        let layout = laid_out(
            TextWrap::Balance,
            |style| {
                style.letter_spacing = Spacing::PxF(letter);
                style.word_spacing = Spacing::PxF(word);
            },
            TextAlign::Justify,
        );
        let lines = lines(&layout);
        assert!(lines.len() >= 3, "harness: the paragraph wraps");
        for (index, line) in lines[..lines.len() - 1].iter().enumerate() {
            let end = line.end + letter;
            assert!(
                (end - WIDTH).abs() <= 0.5,
                "letter-spacing {letter}px, word-spacing {word}px: line {index} ({:?}) ends at \
                 {end:.2}, not on the {WIDTH} px box's edge",
                line.text()
            );
        }
    }
}
