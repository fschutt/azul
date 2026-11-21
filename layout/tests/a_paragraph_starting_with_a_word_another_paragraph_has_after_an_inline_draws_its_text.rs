//! A paragraph that starts with a word another paragraph has after an inline
//! element draws its text.
//!
//! Calmet's Dictionary p. 765 (pdfocr engine issue 9): the margin note
//! "ויגד Veiagged, He castrated." beside "Vaïagged, ויגד Nuntiavit." was
//! missing from the PDF, with no warning. The repro: `<p><i>x</i> ויגד</p>`
//! in one box, then `<p>ויגד Veia.</p>` in a box too narrow for its one
//! line. Every paragraph here is compared with the same paragraph laid out
//! alone, so the fonts of the machine do not decide the numbers.

use crate::table_markup::{glyphs_inside, laid_out};

const STYLE: &str = "* { margin: 0; padding: 0; } .region { position: absolute; font-family: \
                     Times, serif; font-size: 28.5pt; line-height: 28.5pt; text-align: left; }";

/// A page of absolutely placed regions: `(left pt, width pt, paragraph)`.
fn page(regions: &[(f32, f32, &str)]) -> String {
    let regions: String = regions
        .iter()
        .map(|(left, width, paragraph)| {
            format!(
                "<div class=\"region\" style=\"left: {left}pt; top: 20pt; width: \
                 {width}pt;\">{paragraph}</div>"
            )
        })
        .collect();
    format!(
        "<html><head><meta charset=\"utf-8\" /><style>{STYLE}</style></head><body><div \
         style=\"position: relative; width: 600pt; height: 300pt;\">{regions}</div></body></html>"
    )
}

/// How many glyphs `#id` paints on a page of `regions`.
fn glyph_count(regions: &[(f32, f32, &str)], id: &str) -> usize {
    glyphs_inside(&laid_out(&page(regions), 800.0, 600.0), id).len()
}

#[test]
fn a_wrapping_paragraph_that_starts_with_a_hebrew_word_another_has_after_an_inline_draws_its_text()
{
    let second = r#"<p id="second">ויגד Veia.</p>"#;
    let alone = glyph_count(&[(300.0, 88.0, second)], "second");
    assert!(
        alone >= 5,
        "harness: the paragraph alone paints its words: {alone} glyphs"
    );
    let after = glyph_count(
        &[
            (20.0, 175.0, r#"<p id="first"><i>x</i> ויגד</p>"#),
            (300.0, 88.0, second),
        ],
        "second",
    );
    assert_eq!(
        after, alone,
        "after a paragraph with the same Hebrew word behind an <i>, the paragraph paints \
         every glyph it paints alone"
    );
}

#[test]
fn a_paragraph_that_starts_with_a_word_another_has_after_an_inline_draws_its_text() {
    // The same shape in every script the Dictionary sets: the word first in
    // one paragraph, behind an inline element in another, in either order.
    for word in ["ויגד", "Veia", "λόγος"] {
        let first = format!(r#"<p id="first"><i>x</i> {word}</p>"#);
        let second = format!(r#"<p id="second">{word} Veia.</p>"#);
        for (name, regions) in [
            (
                "second",
                vec![
                    (20.0, 175.0, first.as_str()),
                    (300.0, 88.0, second.as_str()),
                ],
            ),
            (
                "first",
                vec![
                    (300.0, 88.0, second.as_str()),
                    (20.0, 175.0, first.as_str()),
                ],
            ),
        ] {
            let alone_regions: Vec<(f32, f32, &str)> = regions
                .iter()
                .copied()
                .filter(|(_, _, p)| p.contains(&format!("id=\"{name}\"")))
                .collect();
            let alone = glyph_count(&alone_regions, name);
            let together = glyph_count(&regions, name);
            assert_eq!(
                together, alone,
                "#{name} ({word}) paints as many glyphs beside the other paragraph as alone"
            );
        }
    }
}
