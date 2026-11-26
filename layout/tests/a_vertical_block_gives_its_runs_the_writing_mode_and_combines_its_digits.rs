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
//! The face is `Azul Mock Mono` (half an em for every ASCII glyph). The PDF writer's side is read
//! from a document laid out the way an export lays it out (`text_rig::export_page`): the display
//! list of a window has no `TextLayout` items.

use azul_layout::text3::cache::{ShapedItem, WritingMode};

use crate::text_rig::{export_page, laid_out, painted, MONO};

/// The combined blocks the layout of an exported page holds, in order, each as its number of
/// glyphs (a block of two digits is two glyphs wide).
fn combined_blocks(exported: &crate::text_rig::Exported) -> Vec<usize> {
    exported
        .layouts()
        .iter()
        .flat_map(|layout| layout.items.iter().cloned())
        .filter_map(|positioned| match positioned.item {
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
        let runs = export_page(MONO, css, "tate").pdf_runs();
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
    let runs = export_page(
        MONO,
        "writing-mode: vertical-rl",
        "<span>tate</span>yoko",
    )
    .pdf_runs();
    assert!(!runs.is_empty());
    assert!(runs.iter().all(|run| run.writing_mode == WritingMode::VerticalRl));
}

#[test]
fn digits_two_combines_each_pair_of_digits() {
    let css = "writing-mode: vertical-rl; text-combine-upright: digits 2";
    let exported = export_page(MONO, css, "2026");
    assert_eq!(combined_blocks(&exported), [2, 2], "two blocks of two digits");

    // The PDF writer is given the digits of each block, as the text they stand for.
    let runs = exported.pdf_runs();
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
    assert_eq!(combined_blocks(&export_page(MONO, css, "no 2026 ok")), [2, 2]);
}

#[test]
fn all_combines_the_whole_text_of_the_box() {
    let css = "writing-mode: vertical-rl; text-combine-upright: all";
    assert_eq!(combined_blocks(&export_page(MONO, css, "12")), [2], "one block of the two digits");
}

#[test]
fn none_and_the_initial_value_combine_nothing() {
    for css in [
        "writing-mode: vertical-rl; text-combine-upright: none",
        "writing-mode: vertical-rl",
    ] {
        assert!(combined_blocks(&export_page(MONO, css, "2026")).is_empty(), "`{css}`");
    }
}

/// The value is inherited, and a box that says `none` combines nothing.
#[test]
fn the_value_is_inherited_by_the_boxes_inside() {
    let exported = export_page(
        MONO,
        "writing-mode: vertical-rl; text-combine-upright: digits 2",
        "<span>20</span><span style=\"text-combine-upright: none\">26</span>",
    );
    assert_eq!(combined_blocks(&exported), [2], "the block of the first span only");
}

/// In horizontal text the property has no effect (CSS Writing Modes 3, 9.1).
#[test]
fn text_combine_upright_has_no_effect_in_horizontal_text() {
    for css in ["text-combine-upright: digits 2", "text-combine-upright: all"] {
        assert!(combined_blocks(&export_page(MONO, css, "2026")).is_empty(), "`{css}`");
    }
}

/// What the renderers are given: in vertical text the digits of a block stand side by side - on
/// one baseline across the line - where the same digits uncombined follow each other down it.
#[test]
fn the_digits_of_a_block_share_a_baseline_in_the_display_list() {
    let across_the_line = |css: &str| {
        let (_lw, dl) = laid_out(MONO, css, "2026", 400.0);
        let glyphs = painted(&dl);
        assert_eq!(glyphs.len(), 4, "`{css}`: four digits");
        let mut positions: Vec<(i32, i32)> =
            glyphs.iter().map(|g| (g.x.round() as i32, g.y.round() as i32)).collect();
        positions.sort_unstable();
        positions
    };
    let combined = across_the_line("writing-mode: vertical-rl; text-combine-upright: digits 2");
    let plain = across_the_line("writing-mode: vertical-rl");
    assert_ne!(combined, plain, "the combined digits are placed as blocks: {combined:?} / {plain:?}");
}
