//! The `text-decoration` shorthand and `text-decoration-style` reach the lines of the display
//! list (CSS Text Decoration 3, 2.1 - 2.4): `underline red 3px` is an underline of that colour
//! and thickness, `underline overline line-through` draws all three lines, `double` draws a line
//! as two lines as thick as the one, a thickness apart, and the shorthand resets the longhands
//! it does not name while a longhand after it wins. The style, like the colour and the
//! thickness, is not inherited: it is the style of the box that sets the line.
//!
//! The faces and the page are those of the text rig: `Azul Mock Mono` at 20 px, so that a line is
//! 20 x 0.08 = 1.6 px thick, an underline's top edge lies 2.4 px below its baseline, an overline's
//! 17 px above it and a line-through's 6 px above it.
//!
//! Not compiled by the author (house rule).

use azul_css::props::basic::ColorU;
use azul_layout::solver3::display_list::{DisplayList, DisplayListItem};

use crate::text_rig::{block, lay_out, painted, span, text, window, MONO};

const RED: ColorU = ColorU { r: 255, g: 0, b: 0, a: 255 };
const BLACK: ColorU = ColorU { r: 0, g: 0, b: 0, a: 255 };

/// One line of the display list.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Line {
    kind: &'static str,
    color: ColorU,
    y: f32,
    height: f32,
}

/// Every underline, overline and line-through of the list, in painting order.
fn lines(dl: &DisplayList) -> Vec<Line> {
    dl.items
        .iter()
        .filter_map(|item| {
            let (kind, bounds, color) = match item {
                DisplayListItem::Underline { bounds, color, .. } => ("underline", bounds, *color),
                DisplayListItem::Overline { bounds, color, .. } => ("overline", bounds, *color),
                DisplayListItem::Strikethrough { bounds, color, .. } => {
                    ("line-through", bounds, *color)
                }
                _ => return None,
            };
            let r = bounds.inner();
            Some(Line { kind, color, y: r.origin.y, height: r.size.height })
        })
        .collect()
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-2
}

/// The baseline of the first glyph and the lines of one block of `ab` under `css`.
fn under(css: &str) -> (f32, Vec<Line>) {
    let mut lw = window();
    let dl = lay_out(&mut lw, block(MONO, css, vec![text("ab")]), 400.0);
    (painted(&dl)[0].y, lines(&dl))
}

fn of_kind(found: &[Line], kind: &str) -> Vec<Line> {
    found.iter().copied().filter(|l| l.kind == kind).collect()
}

#[test]
fn the_shorthand_sets_the_colour_and_the_thickness_of_the_line() {
    let (base, found) = under("text-decoration: underline red 3px");
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].color, RED, "{found:?}");
    assert!(close(found[0].height, 3.0), "{found:?}");
    assert!(close(found[0].y - base, 2.4), "an underline grows down from its place: {found:?}");
    for value in [
        "red 3px underline",
        "3px underline red",
        "underline rgb(255, 0, 0) 3px",
        "underline 3px #f00",
    ] {
        let (_, found) = under(&format!("text-decoration: {value}"));
        assert_eq!(found.len(), 1, "{value}: {found:?}");
        assert_eq!(found[0].color, RED, "{value}: {found:?}");
        assert!(close(found[0].height, 3.0), "{value}: {found:?}");
    }
}

#[test]
fn the_shorthand_draws_every_line_it_names() {
    let (base, found) = under("text-decoration: underline overline line-through");
    assert_eq!(found.len(), 3, "{found:?}");
    let underlines = of_kind(&found, "underline");
    let overlines = of_kind(&found, "overline");
    let through = of_kind(&found, "line-through");
    assert_eq!((underlines.len(), overlines.len(), through.len()), (1, 1, 1), "{found:?}");
    assert!(close(underlines[0].y - base, 2.4), "{found:?}");
    assert!(close(overlines[0].y - base, -17.0), "{found:?}");
    assert!(close(through[0].y - base, -6.0), "{found:?}");
    // the longhand of the lines takes several as well
    let (_, found) = under("text-decoration-line: underline overline");
    let mut kinds: Vec<&str> = found.iter().map(|l| l.kind).collect();
    kinds.sort_unstable();
    assert_eq!(kinds, vec!["overline", "underline"], "{found:?}");
}

#[test]
fn double_draws_two_lines_a_thickness_apart() {
    let (base, found) = under("text-decoration: underline double");
    let tops: Vec<f32> = found.iter().map(|l| l.y - base).collect();
    assert_eq!(found.len(), 2, "two underlines: {found:?}");
    assert!(close(tops[0], 2.4) && close(tops[1], 2.4 + 3.2), "the second below the first: {tops:?}");
    assert!(found.iter().all(|l| close(l.height, 1.6)), "each as thick as one: {found:?}");

    let (base, found) = under("text-decoration: overline double");
    let mut tops: Vec<f32> = found.iter().map(|l| l.y - base).collect();
    tops.sort_by(f32::total_cmp);
    assert_eq!(found.len(), 2, "{found:?}");
    assert!(close(tops[0], -17.0 - 3.2) && close(tops[1], -17.0), "the second above: {tops:?}");

    let (base, found) = under("text-decoration: line-through double");
    let mut tops: Vec<f32> = found.iter().map(|l| l.y - base).collect();
    tops.sort_by(f32::total_cmp);
    assert_eq!(found.len(), 2, "{found:?}");
    assert!(close(tops[0], -6.0 - 1.6) && close(tops[1], -6.0 + 1.6), "one on each side: {tops:?}");
}

#[test]
fn a_style_is_the_style_of_every_line_of_the_box() {
    let (_, found) = under("text-decoration: underline overline double red");
    assert_eq!(found.len(), 4, "two of each: {found:?}");
    assert_eq!(of_kind(&found, "underline").len(), 2, "{found:?}");
    assert_eq!(of_kind(&found, "overline").len(), 2, "{found:?}");
    assert!(found.iter().all(|l| l.color == RED), "{found:?}");
}

#[test]
fn the_thickness_of_a_double_line_is_the_thickness_of_each_of_its_lines() {
    let (base, found) = under("text-decoration: underline double 4px");
    assert_eq!(found.len(), 2, "{found:?}");
    assert!(found.iter().all(|l| close(l.height, 4.0)), "{found:?}");
    assert!(close(found[1].y - found[0].y, 8.0), "a thickness apart: {found:?} on {base}");
}

/// The shorthand sets all four: the ones it does not name are reset, so a colour set before it by
/// a longhand is gone, and a longhand after it wins.
#[test]
fn the_shorthand_resets_the_longhands_it_does_not_name() {
    let (_, found) = under("text-decoration-color: #ff0000; text-decoration: underline");
    assert_eq!(found[0].color, BLACK, "the colour is reset: {found:?}");
    let (_, found) = under("text-decoration-color: #ff0000; text-decoration: underline blue");
    assert_eq!(found[0].color, ColorU { r: 0, g: 0, b: 255, a: 255 }, "{found:?}");
    let (_, found) = under("text-decoration: underline; text-decoration-color: #ff0000");
    assert_eq!(found[0].color, RED, "a longhand after the shorthand wins: {found:?}");
    let (_, found) = under("text-decoration-thickness: 5px; text-decoration: underline");
    assert!(close(found[0].height, 1.6), "the thickness is reset: {found:?}");
    let (_, found) = under("text-decoration-style: double; text-decoration: underline");
    assert_eq!(found.len(), 1, "the style is reset: {found:?}");
    let (_, found) = under("text-decoration: underline double; text-decoration-style: solid");
    assert_eq!(found.len(), 1, "a longhand after the shorthand wins: {found:?}");
}

/// A line is drawn with the style of the box that sets it, and a box inside that sets none of its
/// own draws the lines of the box around it in THAT box's style.
#[test]
fn a_style_is_not_inherited_and_a_box_inside_draws_the_style_of_the_box_around_it() {
    let mut lw = window();
    let dom = block(
        MONO,
        "text-decoration-style: double",
        vec![span("text-decoration: underline", "ab")],
    );
    let found = lines(&lay_out(&mut lw, dom, 400.0));
    assert_eq!(found.len(), 1, "the span's own line is solid: {found:?}");

    let mut lw = window();
    let dom = block(
        MONO,
        "text-decoration: underline double",
        vec![text("ab"), span("font-size: 10px", "cd")],
    );
    let found = lines(&lay_out(&mut lw, dom, 400.0));
    assert_eq!(found.len(), 4, "two under each run, in the style of the block: {found:?}");
    let mut lw = window();
    let dom = block(
        MONO,
        "text-decoration: underline double",
        vec![
            text("ab"),
            span("text-decoration: underline; text-decoration-color: #ff0000", "cd"),
        ],
    );
    let found = lines(&lay_out(&mut lw, dom, 400.0));
    assert_eq!(
        found.len(),
        2 + 2 + 1,
        "under `cd` the block's two lines and the span's own, solid one: {found:?}"
    );
    assert_eq!(found.iter().filter(|l| l.color == RED).count(), 1, "{found:?}");
}
