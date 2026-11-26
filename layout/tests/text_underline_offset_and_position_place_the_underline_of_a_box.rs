//! `text-underline-offset` and `text-underline-position` place the underline of the display list:
//! the offset moves it down (up for a negative one) from the place the font size suggests, `under`
//! puts it below the descenders, and neither moves an overline or a line-through.
//!
//! The faces and the page are those of the text rig: `Azul Mock Mono` at 20 px, so that one em is
//! 20 px, the underline of a plain box sits 2.4 px below the baseline and the em box ends 4 px
//! below it.
//!
//! What is pinned, from CSS Text Decoration 3 and 4:
//! - both properties are inherited: a box inside one that sets them is placed by them, and `auto`
//!   ends them;
//! - the place is the one of the box that sets the line: an `em` or a percentage is a part of ITS
//!   font size, whatever the font size of the text under the line;
//! - `from-font` asks the face for the place of its underline; the mock face names none, so it is
//!   `auto`; `left` and `right` are for vertical text and change nothing in a horizontal line;
//! - a hostile length is capped.

use crate::text_rig::{block, laid_out, lay_out, span, text, window, MONO};
use azul_layout::solver3::display_list::{DisplayList, DisplayListItem};

/// The top edge of every underline of the list, in painting order.
fn underlines(dl: &DisplayList) -> Vec<f32> {
    dl.items
        .iter()
        .filter_map(|item| match item {
            DisplayListItem::Underline { bounds, .. } => Some(bounds.inner().origin.y),
            _ => None,
        })
        .collect()
}

/// The top edge of the lines of the kind `kind` of the list.
fn tops(dl: &DisplayList, kind: &str) -> Vec<f32> {
    dl.items
        .iter()
        .filter_map(|item| match (kind, item) {
            ("overline", DisplayListItem::Overline { bounds, .. })
            | ("line-through", DisplayListItem::Strikethrough { bounds, .. }) => {
                Some(bounds.inner().origin.y)
            }
            _ => None,
        })
        .collect()
}

/// The one underline `css` gives a block of `ab`: where its top edge is.
fn underline_top(css: &str) -> f32 {
    let found = underlines(&laid_out(MONO, css, "ab", 400.0).1);
    assert_eq!(found.len(), 1, "`{css}`: one underline: {found:?}");
    found[0]
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-3
}

/// Where the underline of a plain box is.
fn plain() -> f32 {
    underline_top("text-decoration: underline")
}

#[test]
fn an_offset_moves_the_underline_from_its_place() {
    let plain = plain();
    for (value, moved) in [
        ("3px", 3.0),
        ("-2px", -2.0),
        ("0.25em", 5.0),
        ("50%", 10.0),
        ("0", 0.0),
        ("auto", 0.0),
        ("3pt", 4.0),
        ("-0.1em", -2.0),
    ] {
        let top = underline_top(&format!(
            "text-decoration: underline; text-underline-offset: {value}"
        ));
        assert!(close(top, plain + moved), "{value}: {top} against {plain} + {moved}");
    }
}

/// The offset is the underline's: an overline and a line-through stay where they were.
#[test]
fn the_offset_moves_no_other_line() {
    for kind in ["overline", "line-through"] {
        let decoration = format!("text-decoration: {kind}");
        let plain = tops(&laid_out(MONO, &decoration, "ab", 400.0).1, kind);
        let moved = tops(
            &laid_out(MONO, &format!("{decoration}; text-underline-offset: 9px"), "ab", 400.0).1,
            kind,
        );
        assert_eq!(plain.len(), 1, "{kind}: {plain:?}");
        assert!(close(plain[0], moved[0]), "{kind}: {plain:?} against {moved:?}");
        let under = format!("{decoration}; text-underline-position: under");
        let moved = tops(&laid_out(MONO, &under, "ab", 400.0).1, kind);
        assert!(close(plain[0], moved[0]), "{kind} under: {plain:?} against {moved:?}");
    }
}

/// `under` puts the top of the underline where the em box ends, 4 px below the baseline, where the
/// plain underline starts at 2.4: 1.6 px lower. The offset is counted from there.
#[test]
fn under_puts_the_underline_below_the_descenders() {
    let plain = plain();
    for (value, moved) in [("under", 1.6), ("UNDER", 1.6), ("under left", 1.6), ("right under", 1.6)] {
        let top = underline_top(&format!(
            "text-decoration: underline; text-underline-position: {value}"
        ));
        assert!(close(top, plain + moved), "{value}: {top} against {plain} + {moved}");
    }
    let top = underline_top(
        "text-decoration: underline; text-underline-position: under; text-underline-offset: 3px",
    );
    assert!(close(top, plain + 1.6 + 3.0), "the offset is counted from the place: {top}");
}

/// `from-font`: the mock face names no place for its underline, so it is `auto`. `left` and
/// `right` are for the line of vertical text and move nothing here.
#[test]
fn the_other_positions_leave_a_horizontal_underline_where_it_was() {
    let plain = plain();
    for value in ["auto", "from-font", "left", "right", "from-font left"] {
        let top = underline_top(&format!(
            "text-decoration: underline; text-underline-position: {value}"
        ));
        assert!(close(top, plain), "{value}: {top} against {plain}");
    }
}

/// What is no value is ignored: the underline stays where it was.
#[test]
fn what_is_no_value_is_ignored() {
    let plain = plain();
    for bad in ["thick", "1px 2px", "", "auto auto"] {
        let top =
            underline_top(&format!("text-decoration: underline; text-underline-offset: {bad}"));
        assert!(close(top, plain), "offset `{bad}`: {top} against {plain}");
    }
    for bad in ["under under", "left right", "from-font under", "under from-font", "above", ""] {
        let top =
            underline_top(&format!("text-decoration: underline; text-underline-position: {bad}"));
        assert!(close(top, plain), "position `{bad}`: {top} against {plain}");
    }
}

/// Both are inherited: a box inside one that sets them is placed by them, and `auto` ends them.
#[test]
fn both_are_inherited_and_auto_ends_them() {
    let plain = plain();
    let under = |css: &str, inner: &str| {
        let mut lw = window();
        let dom = block(MONO, css, vec![span(inner, "ab")]);
        let found = underlines(&lay_out(&mut lw, dom, 400.0));
        assert_eq!(found.len(), 1, "{found:?}");
        found[0]
    };
    let offset = under("text-underline-offset: 6px", "text-decoration: underline");
    assert!(close(offset, plain + 6.0), "the span takes the offset of its block: {offset}");
    let reset =
        under("text-underline-offset: 6px", "text-decoration: underline; text-underline-offset: auto");
    assert!(close(reset, plain), "`auto` of the span ends it: {reset}");
    let moved = under(
        "text-underline-offset: 6px",
        "text-decoration: underline; text-underline-offset: 2px",
    );
    assert!(close(moved, plain + 2.0), "the span's own: {moved}");
    let position = under("text-underline-position: under", "text-decoration: underline");
    assert!(close(position, plain + 1.6), "the span takes the position of its block: {position}");
    let reset = under(
        "text-underline-position: under",
        "text-decoration: underline; text-underline-position: auto",
    );
    assert!(close(reset, plain), "`auto` of the span ends it: {reset}");
}

/// The place of a line is the one of the box that sets it: an `em` is a part of ITS font size,
/// whatever the font size of the text the line crosses.
#[test]
fn an_em_is_a_part_of_the_font_size_of_the_box_that_sets_the_line() {
    let run = |css: &str| {
        let mut lw = window();
        let dom = block(MONO, css, vec![text("ab"), span("font-size: 40px", "cd")]);
        underlines(&lay_out(&mut lw, dom, 400.0))
    };
    let plain = run("text-decoration: underline");
    let moved = run("text-decoration: underline; text-underline-offset: 0.25em");
    assert_eq!((plain.len(), moved.len()), (2, 2), "{plain:?} {moved:?}");
    for (before, after) in plain.iter().zip(&moved) {
        assert!(close(*after, before + 5.0), "a quarter of the box's 20 px: {plain:?} {moved:?}");
    }
}

/// Under a CSS `zoom` a length is scaled with the box.
#[test]
fn a_zoom_scales_the_offset_with_the_box() {
    let plain = underline_top("zoom: 2; text-decoration: underline");
    let top = underline_top("zoom: 2; text-decoration: underline; text-underline-offset: 3px");
    assert!(close(top, plain + 6.0), "{top} against {plain} + 6");
    let top = underline_top("zoom: 2; text-decoration: underline; text-underline-offset: 0.25em");
    assert!(close(top, plain + 10.0), "a quarter of the zoomed 40 px: {top} against {plain} + 10");
}

/// A hostile length is capped at 1,000 px either way, with a diagnostic.
#[test]
fn a_hostile_offset_is_capped_with_a_diagnostic() {
    let _lock = azul_core::diagnostics::test_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    azul_core::diagnostics::clear();
    let plain = plain();
    for (value, moved) in [
        ("1000000px", 1000.0),
        ("1e9px", 1000.0),
        ("99999999999999999999999999px", 1000.0),
        ("-1e9px", -1000.0),
        ("1e39em", 1000.0),
    ] {
        let top = underline_top(&format!(
            "text-decoration: underline; text-underline-offset: {value}"
        ));
        assert!(top.is_finite() && close(top, plain + moved), "{value}: {top}");
    }
    assert!(
        azul_core::diagnostics::any_contains("text-underline-offset"),
        "the cap is reported: {:?}",
        azul_core::diagnostics::recorded()
    );
}
