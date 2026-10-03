//! Text beside an italic or bold box keeps a font (E-INLINE).
//!
//! TABLE_A (scripts/TABLE_A_2026_10_01.md, "engine findings") saw a table
//! of inline-blocks separated by spaces measure the SUM of its boxes as its
//! min-content (`words(10, 100)` = 1000px) and rewrote its two width-cap
//! tests to prose (56b105f60) without a test for the bug itself
//! (DEDUP_WIDGETS_API F24). TABLES (wave 5) bisected it on the prebuilt
//! engine at 2e92c759b against Chrome 154 (scratchpad probe, AzPaint's
//! debug server):
//!
//! | markup (the document's only text in brackets)                  | azul | Chrome |
//! |-----------------------------------------------------------------|------|--------|
//! | `width: 1px` table, `<i ib 100/> <i ib 100/> <i ib 100/>`        | 306  | 106    |
//! | the same with `<span>` boxes                                    | 106  | 106    |
//! | `<span style="font-style: italic">` boxes                       | 306  | 106    |
//! | `<i style="font-style: normal">` boxes                          | 106  | 106    |
//! | `<i>` boxes plus a `<p>x</p>` elsewhere                         | 106  | 106    |
//! | `<i>` boxes plus a bold (or an italic) `<p>x</p>`               | 306  | 106    |
//! | `<p><b>bold</b> tail</p>`                                       | "bold" | "bold tail" |
//! | `<p><span>plain</span> tail</p>`                                | "plain tail" | same |
//!
//! So it is neither the table nor the inline intrinsic sizing: the spaces
//! (and " tail") are dropped - not shaped, no width, no break opportunity -
//! when the element beside them has another `font-style` / `font-weight`
//! and no other text in the document needs their regular face. The
//! display list has no text item for them at all; the measurement then
//! sees the boxes glued together. That points at the font chains resolved
//! for the document (`solver3::getters::collect_font_stacks_from_styled_dom`
//! / `resolve_font_chains_fast`) or the shaping lookup of the run's chain
//! (`text3::cache`), not at layout.
//!
//! ROOT CAUSE (MAILENG6, wave 6): `collect_font_stacks_from_styled_dom` read
//! each text node's weight and style through `NodeId::from_usize(i)` - the
//! 1-based FFI decoder - on a plain 0-based index, i.e. from the node BEFORE
//! the text node in document order. A text that follows an element of
//! another weight or style (the `<b>` itself, or the text inside it) asked
//! for that element's face; its own face was never collected, never loaded,
//! and the run shaped to nothing. Box sizes are fixed, so the numbers do not
//! depend on the machine's fonts.

use crate::table_markup::{body, glyph_runs, near, rect, rects_of_color};

/// An italic (`<i>`, UA `font-style: italic`) inline-block of `w` px,
/// painted in `rgb`.
fn italic_box(w: u32, (r, g, b): (u8, u8, u8)) -> String {
    format!(
        "<i style=\"display: inline-block; width: {w}px; height: 10px; \
         background: rgb({r}, {g}, {b})\"></i>"
    )
}

#[test]
fn spaces_between_italic_inline_blocks_are_break_opportunities() {
    let boxes = (0..3)
        .map(|_| italic_box(100, (200, 0, 0)))
        .collect::<Vec<_>>()
        .join(" ");
    let lw = body(&format!(
        "<table id=\"t\" style=\"width: 1px; border-spacing: 0\"><tr>\
         <td style=\"padding: 0\">{boxes}</td></tr></table>"
    ));
    let t = rect(&lw, "t");
    assert!(
        near(t.size.width, 100.0, 0.5),
        "the table's minimum is ONE box - the spaces between them are soft wrap \
         opportunities (CSS Text 3 5.1): {} (the sum of the boxes is 300)",
        t.size.width
    );
}

#[test]
fn spaces_between_italic_inline_blocks_take_their_width() {
    let lw = body(&format!(
        "<div>{} {}</div>",
        italic_box(100, (200, 0, 0)),
        italic_box(100, (0, 0, 200))
    ));
    let first = rects_of_color(&lw, (200, 0, 0));
    let second = rects_of_color(&lw, (0, 0, 200));
    assert_eq!(
        (first.len(), second.len()),
        (1, 1),
        "both boxes paint once: {first:?} {second:?}"
    );
    let gap = second[0].origin.x - (first[0].origin.x + first[0].size.width);
    assert!(
        gap >= 2.0,
        "a space separates the two boxes on their line: gap {gap} ({first:?} / {second:?})"
    );
}

#[test]
fn text_after_a_bold_element_is_painted() {
    let lw = body("<p><b>bold</b> tail</p>");
    let glyphs: usize = glyph_runs(&lw).iter().map(Vec::len).sum();
    assert!(
        glyphs >= "boldtail".len(),
        "\"bold\" AND \" tail\" are painted: {glyphs} glyphs"
    );
}
