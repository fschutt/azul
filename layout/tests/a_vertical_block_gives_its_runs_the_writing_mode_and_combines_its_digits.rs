//! `writing-mode` and `text-combine-upright` reach the style of the text: a run of a vertical block
//! says so (the glyph runs the PDF writer is given carry the writing mode) and the digits a
//! `text-combine-upright` asks for are set side by side in one em of the vertical line
//! (tate-chu-yoko), in the display list and in the PDF.
//!
//! The style the shaper is given was built without either: every glyph run of a vertical document
//! said `horizontal-tb`, and `text-combine-upright: digits 2` combined nothing, because the one
//! trigger of a combined block is the style of the run - the CSS value only reached the
//! constraints of the line, where nothing read it.
//!
//! The face is `Azul Mock Mono` (half an em for every ASCII glyph).

use azul_layout::text3::cache::{ShapedItem, WritingMode};

use crate::text_rig::{block, laid_out, lay_out, pdf_runs, span, text, text_layouts, window, MONO};

/// The combined blocks the layout of the page holds, in order, each as its number of glyphs (a
/// block of two digits is two glyphs wide).
fn combined_blocks(dl: &azul_layout::solver3::display_list::DisplayList) -> Vec<usize> {
    text_layouts(dl)
        .iter()
        .flat_map(|layout| layout.items.iter())
        .filter_map(|positioned| match &positioned.item {
            ShapedItem::CombinedBlock { glyphs, .. } => Some(glyphs.len()),
            _ => None,
        })
        .collect()
}

#[test]
fn the_runs_of_a_vertical_block_say_their_writing_mode() {
    for (css, expected) in [
        ("writing-mode: vertical-rl", WritingMode::VerticalRl),
        ("writing-mode: vertical-lr", WritingMode::VerticalLr),
        ("writing-mode: horizontal-tb", WritingMode::HorizontalTb),
        ("", WritingMode::HorizontalTb),
    ] {
        let (lw, dl) = laid_out(MONO, css, "tate", 400.0);
        let runs = pdf_runs(&lw, &dl);
        assert!(!runs.is_empty(), "`{css}`: the text is given to the PDF writer");
        assert!(
            runs.iter().all(|run| run.writing_mode == expected),
            "`{css}`: {:?}",
            runs.iter().map(|run| run.writing_mode).collect::<Vec<_>>()
        );
    }
}

/// The writing mode is inherited: the text of a span in a vertical block is vertical.
#[test]
fn the_writing_mode_reaches_the_text_of_the_boxes_inside() {
    let mut lw = window();
    let dom = block(MONO, "writing-mode: vertical-rl", vec![span("", "tate"), text("yoko")]);
    let dl = lay_out(&mut lw, dom, 400.0);
    let runs = pdf_runs(&lw, &dl);
    assert!(!runs.is_empty());
    assert!(runs.iter().all(|run| run.writing_mode == WritingMode::VerticalRl));
}

#[test]
fn digits_two_combines_each_pair_of_digits() {
    let css = "writing-mode: vertical-rl; text-combine-upright: digits 2";
    let (lw, dl) = laid_out(MONO, css, "2026", 400.0);
    assert_eq!(combined_blocks(&dl), [2, 2], "two blocks of two digits");

    // The PDF writer is given the digits of each block, as the text they stand for.
    let runs = pdf_runs(&lw, &dl);
    let texts: Vec<String> = runs.iter().flat_map(|run| run.cluster_texts.clone()).collect();
    for piece in ["20", "26"] {
        assert!(
            texts.iter().any(|text| text == piece),
            "the PDF has the block {piece}: {texts:?}"
        );
    }
    let glyphs: usize = runs.iter().map(|run| run.glyphs.len()).sum();
    assert_eq!(glyphs, 4, "no digit is missing from the PDF");
}

#[test]
fn digits_leave_the_rest_of_the_text_alone() {
    let css = "writing-mode: vertical-rl; text-combine-upright: digits 2";
    let (_lw, dl) = laid_out(MONO, css, "no 2026 ok", 400.0);
    assert_eq!(combined_blocks(&dl), [2, 2]);
}

#[test]
fn all_combines_the_whole_text_of_the_box() {
    let (_lw, dl) = laid_out(MONO, "writing-mode: vertical-rl; text-combine-upright: all", "12", 400.0);
    assert_eq!(combined_blocks(&dl), [2], "one block of the two digits");
}

#[test]
fn none_and_the_initial_value_combine_nothing() {
    for css in [
        "writing-mode: vertical-rl; text-combine-upright: none",
        "writing-mode: vertical-rl",
    ] {
        let (_lw, dl) = laid_out(MONO, css, "2026", 400.0);
        assert!(combined_blocks(&dl).is_empty(), "`{css}`");
    }
}

/// The value is inherited, and a box that says `none` combines nothing.
#[test]
fn the_value_is_inherited_by_the_boxes_inside() {
    let mut lw = window();
    let dom = block(
        MONO,
        "writing-mode: vertical-rl; text-combine-upright: digits 2",
        vec![span("", "20"), span("text-combine-upright: none", "26")],
    );
    let dl = lay_out(&mut lw, dom, 400.0);
    assert_eq!(combined_blocks(&dl), [2], "the block of the first span only");
}

/// In horizontal text the property has no effect (CSS Writing Modes 3, 9.1).
#[test]
fn text_combine_upright_has_no_effect_in_horizontal_text() {
    let (_lw, dl) = laid_out(MONO, "text-combine-upright: digits 2", "2026", 400.0);
    assert!(combined_blocks(&dl).is_empty());
    let (_lw, dl) = laid_out(MONO, "text-combine-upright: all", "2026", 400.0);
    assert!(combined_blocks(&dl).is_empty());
}
