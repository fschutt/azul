//! `text-decoration-color` and `text-decoration-thickness` reach the lines of the display list: the
//! underline, the overline and the line-through of a box are painted in the colour and at the
//! thickness the box sets for them, and in the colour of the text and a thickness of the font size
//! where it sets none.
//!
//! The faces and the page are those of the text rig: `Azul Mock Mono` at 20 px, so that the
//! default thickness is 20 x 0.08 = 1.6 px and one em is 20 px.
//!
//! What is pinned, from the rules of CSS Text Decoration 3 and 4:
//! - neither property is inherited: a box that sets them and has no line paints none (2.1: a line
//!   is drawn with the properties of the box that sets it, the "decorating box");
//! - the lines of a box reach the text of the boxes inside it with the paint of the box that sets
//!   them: the colour is the colour of THAT box (`currentcolor` is its `color`), not the colour of
//!   the text the line crosses, and an `em` or a percentage is a part of ITS font size;
//! - every kind of line has its own paint: an underline of one box and a line-through of another
//!   are two lines of two paints on one run of text;
//! - a thickness of 0 paints no line; a hostile length is capped.

use azul_core::dom::Dom;
use azul_css::props::basic::ColorU;
use azul_layout::solver3::display_list::{DisplayList, DisplayListItem};

use crate::text_rig::{block, lay_out, laid_out, span, text, window, MONO};

const RED: ColorU = ColorU { r: 255, g: 0, b: 0, a: 255 };
const BLUE: ColorU = ColorU { r: 0, g: 0, b: 255, a: 255 };
const BLACK: ColorU = ColorU { r: 0, g: 0, b: 0, a: 255 };

/// One line of the display list.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Line {
    kind: &'static str,
    color: ColorU,
    thickness: f32,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

/// Every underline, overline and line-through of the list, in painting order.
fn lines(dl: &DisplayList) -> Vec<Line> {
    dl.items
        .iter()
        .filter_map(|item| {
            let (kind, bounds, color, thickness) = match item {
                DisplayListItem::Underline { bounds, color, thickness } => {
                    ("underline", bounds, *color, *thickness)
                }
                DisplayListItem::Overline { bounds, color, thickness } => {
                    ("overline", bounds, *color, *thickness)
                }
                DisplayListItem::Strikethrough { bounds, color, thickness } => {
                    ("line-through", bounds, *color, *thickness)
                }
                _ => return None,
            };
            let r = bounds.inner();
            Some(Line {
                kind,
                color,
                thickness,
                x: r.origin.x,
                y: r.origin.y,
                width: r.size.width,
                height: r.size.height,
            })
        })
        .collect()
}

/// The colour of every text run of the list, in painting order.
fn text_colours(dl: &DisplayList) -> Vec<ColorU> {
    dl.items
        .iter()
        .filter_map(|item| match item {
            DisplayListItem::Text { color, glyphs, .. } if !glyphs.is_empty() => Some(*color),
            _ => None,
        })
        .collect()
}

/// The lines of one block of `ab` under `css`.
fn lines_under(css: &str) -> Vec<Line> {
    lines(&laid_out(MONO, css, "ab", 400.0).1)
}

/// The one line of the kind `kind` that `css` gives a block of `ab`.
fn the_line(css: &str, kind: &str) -> Line {
    let found = lines_under(css);
    let of_kind: Vec<Line> = found.iter().copied().filter(|l| l.kind == kind).collect();
    assert_eq!(of_kind.len(), 1, "`{css}`: one {kind}: {found:?}");
    of_kind[0]
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-3
}

const KINDS: [(&str, &str); 3] = [
    ("underline", "text-decoration: underline"),
    ("overline", "text-decoration: overline"),
    ("line-through", "text-decoration: line-through"),
];

/// Nothing set: the colour of the text and the thickness the font size suggests, as ever.
#[test]
fn a_line_that_sets_nothing_has_the_colour_of_the_text_and_the_thickness_of_the_font() {
    for (kind, decoration) in KINDS {
        let line = the_line(&format!("color: #0000ff; {decoration}"), kind);
        assert_eq!(line.color, BLUE, "{kind}: the colour of the text");
        assert!(close(line.thickness, 1.6), "{kind}: 20 px x 0.08: {line:?}");
        assert!(close(line.height, line.thickness), "{kind}: the box is as thick as the line");
    }
}

#[test]
fn text_decoration_color_paints_the_line_and_not_the_text() {
    for (kind, decoration) in KINDS {
        let css = format!("color: #0000ff; {decoration}; text-decoration-color: #ff0000");
        let (_lw, dl) = laid_out(MONO, &css, "ab", 400.0);
        let found = lines(&dl);
        assert_eq!(found.len(), 1, "{kind}: {found:?}");
        assert_eq!(found[0].kind, kind);
        assert_eq!(found[0].color, RED, "{kind}: the line takes the colour it was given");
        assert_eq!(text_colours(&dl), vec![BLUE], "{kind}: the text keeps its own");
    }
}

#[test]
fn a_colour_may_be_translucent_a_name_or_a_function() {
    let line = the_line(
        "text-decoration: underline; text-decoration-color: rgba(0, 128, 0, 0.5)",
        "underline",
    );
    assert_eq!((line.color.r, line.color.g, line.color.b), (0, 128, 0), "{line:?}");
    assert!((120..=135).contains(&line.color.a), "half transparent: {}", line.color.a);
    let line = the_line("text-decoration: underline; text-decoration-color: red", "underline");
    assert_eq!(line.color, RED, "a colour name");
    let line = the_line("text-decoration: underline; text-decoration-color: #00f", "underline");
    assert_eq!(line.color, BLUE, "a short hex colour");
}

/// A transparent line is no line.
#[test]
fn a_transparent_line_is_not_painted() {
    let found = lines_under("text-decoration: underline; text-decoration-color: transparent");
    assert!(found.is_empty(), "{found:?}");
}

/// `currentcolor`, the initial value, is the colour of the box that sets the line.
#[test]
fn currentcolor_is_the_colour_of_the_box_and_the_property_may_be_reset_to_it() {
    let line = the_line(
        "color: #00ff00; text-decoration: underline; text-decoration-color: currentcolor",
        "underline",
    );
    assert_eq!(line.color, ColorU { r: 0, g: 255, b: 0, a: 255 }, "{line:?}");
    let line = the_line(
        "color: #00ff00; text-decoration: underline; text-decoration-color: CurrentColor",
        "underline",
    );
    assert_eq!(line.color, ColorU { r: 0, g: 255, b: 0, a: 255 }, "any case: {line:?}");
}

/// An `em` and a percentage are parts of the font size (20 px), a `pt` is 4/3 px.
#[test]
fn a_length_a_percentage_or_an_em_sets_the_thickness() {
    for (value, px) in [("4px", 4.0), ("0.25em", 5.0), ("50%", 10.0), ("3pt", 4.0), ("2.5px", 2.5)] {
        let line = the_line(
            &format!("text-decoration: underline; text-decoration-thickness: {value}"),
            "underline",
        );
        assert!(close(line.thickness, px), "{value}: {line:?}");
        assert!(close(line.height, px), "{value}: the box is as thick as the line: {line:?}");
    }
}

/// A thicker line grows from the top edge of the underline and the overline, and around the
/// middle of the line-through: the place the font puts a line is not moved by its thickness.
#[test]
fn a_thicker_line_keeps_its_place() {
    for (kind, decoration) in KINDS {
        let plain = the_line(decoration, kind);
        let thick = the_line(&format!("{decoration}; text-decoration-thickness: 5px"), kind);
        assert!(close(thick.thickness, 5.0), "{kind}: {thick:?}");
        let (plain_anchor, thick_anchor) = if kind == "line-through" {
            (plain.y + plain.height / 2.0, thick.y + thick.height / 2.0)
        } else {
            (plain.y, thick.y)
        };
        assert!(
            close(plain_anchor, thick_anchor),
            "{kind}: the line stays where it was: {plain:?} against {thick:?}"
        );
        assert!(close(plain.x, thick.x) && close(plain.width, thick.width), "{kind}: its extent");
    }
}

/// `auto` is the initial value. `from-font` asks the face for its own thickness; the mock face has
/// none to give, so it is `auto` too.
#[test]
fn auto_and_from_font_are_the_thickness_of_the_font_size() {
    for value in ["auto", "from-font"] {
        let line = the_line(
            &format!("text-decoration: underline; text-decoration-thickness: {value}"),
            "underline",
        );
        assert!(close(line.thickness, 1.6), "{value}: {line:?}");
    }
}

/// A thickness of nothing is no line; one below zero is no value.
#[test]
fn a_thickness_of_zero_is_no_line_and_a_negative_one_is_no_value() {
    let found = lines_under("text-decoration: underline; text-decoration-thickness: 0px");
    assert!(found.is_empty(), "a line 0 px thick is not painted: {found:?}");
    for bad in ["-2px", "thick", "auto auto", "1px 2px", ""] {
        let line = the_line(
            &format!("text-decoration: underline; text-decoration-thickness: {bad}"),
            "underline",
        );
        assert!(close(line.thickness, 1.6), "`{bad}` is ignored: {line:?}");
    }
    for bad in ["notacolor", "#12", "red blue"] {
        let line = the_line(
            &format!("color: #0000ff; text-decoration: underline; text-decoration-color: {bad}"),
            "underline",
        );
        assert_eq!(line.color, BLUE, "`{bad}` is ignored: {line:?}");
    }
}

/// A box that sets the two and has no line of its own paints none, and a line of a box inside
/// takes none of them: they are not inherited.
#[test]
fn colour_and_thickness_are_not_inherited() {
    let mut lw = window();
    let dom = block(
        MONO,
        "text-decoration-color: #ff0000; text-decoration-thickness: 6px",
        vec![span("text-decoration: underline", "ab")],
    );
    let found = lines(&lay_out(&mut lw, dom, 400.0));
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].color, BLACK, "the colour of the text, not the box's: {found:?}");
    assert!(close(found[0].thickness, 1.6), "not the box's: {found:?}");
}

/// The lines of a box are painted over the text of the boxes inside it as the box sets them.
#[test]
fn a_box_hands_its_colour_and_its_thickness_to_the_text_inside() {
    let mut lw = window();
    let dom = block(
        MONO,
        "text-decoration: underline; text-decoration-color: #ff0000; text-decoration-thickness: 3px",
        vec![text("ab"), span("color: #0000ff", "cd")],
    );
    let dl = lay_out(&mut lw, dom, 400.0);
    let found = lines(&dl);
    assert_eq!(found.len(), 2, "one underline under each run: {found:?}");
    for line in &found {
        assert_eq!(line.color, RED, "{found:?}");
        assert!(close(line.thickness, 3.0), "{found:?}");
    }
    assert_eq!(text_colours(&dl), vec![BLACK, BLUE], "the text keeps its colours");
}

/// CSS Text Decoration 3, 2.1: the line is the decorating box's, drawn in ITS `currentcolor`:
/// the underline under a blue span inside a red block is red, as the line under the rest of it.
#[test]
fn a_line_without_a_colour_is_in_the_colour_of_the_box_that_sets_it() {
    let mut lw = window();
    let dom = block(
        MONO,
        "text-decoration: underline; color: #ff0000",
        vec![text("ab"), span("color: #0000ff", "cd")],
    );
    let dl = lay_out(&mut lw, dom, 400.0);
    let found = lines(&dl);
    assert_eq!(found.len(), 2, "{found:?}");
    assert_eq!(found[0].color, RED, "under the text of the box: {found:?}");
    assert_eq!(found[1].color, RED, "under the blue span, in the colour of the box: {found:?}");
    assert_eq!(text_colours(&dl), vec![RED, BLUE], "the text keeps its colours");
}

/// The same through a span: the line of a span is drawn in the colour of the span, under the
/// text of the boxes inside it.
#[test]
fn a_span_paints_its_line_in_its_own_colour_under_the_boxes_inside_it() {
    let mut lw = window();
    let dom = block(
        MONO,
        "color: #000000",
        vec![Dom::create_span()
            .with_css("text-decoration: underline; color: #ff0000")
            .with_child(text("ab"))
            .with_child(span("color: #0000ff", "cd"))],
    );
    let dl = lay_out(&mut lw, dom, 400.0);
    let found = lines(&dl);
    assert_eq!(found.len(), 2, "{found:?}");
    assert!(found.iter().all(|l| l.color == RED), "the span's colour under both runs: {found:?}");
}

/// Every kind of line has its own paint: an underline of the outer box and a line-through of the
/// box inside are two lines of two colours under one run.
#[test]
fn each_kind_of_line_has_the_paint_of_the_box_that_sets_it() {
    let mut lw = window();
    let dom = block(
        MONO,
        "text-decoration: underline; text-decoration-color: #ff0000",
        vec![
            text("ab"),
            span("text-decoration: line-through; text-decoration-color: #0000ff", "cd"),
        ],
    );
    let found = lines(&lay_out(&mut lw, dom, 400.0));
    let under: Vec<_> = found.iter().filter(|l| l.kind == "underline").collect();
    let through: Vec<_> = found.iter().filter(|l| l.kind == "line-through").collect();
    assert_eq!(under.len(), 2, "the underline of the block under both runs: {found:?}");
    assert!(under.iter().all(|l| l.color == RED), "{found:?}");
    assert_eq!(through.len(), 1, "the line-through of the span under its run: {found:?}");
    assert_eq!(through[0].color, BLUE, "{found:?}");
}

/// An `em` is a part of the font size of the box that sets the line, not of the text under it.
#[test]
fn an_em_is_a_part_of_the_font_size_of_the_box_that_sets_the_line() {
    let mut lw = window();
    let dom = block(
        MONO,
        "text-decoration: underline; text-decoration-thickness: 0.25em",
        vec![text("ab"), span("font-size: 40px", "cd")],
    );
    let found = lines(&lay_out(&mut lw, dom, 400.0));
    assert_eq!(found.len(), 2, "{found:?}");
    for line in &found {
        assert!(close(line.thickness, 5.0), "a quarter of the box's 20 px: {found:?}");
    }
}

/// Under a CSS `zoom` a length is scaled with the box: 3 px at zoom 2 are 6.
#[test]
fn a_zoom_scales_the_thickness_with_the_box() {
    let plain = the_line("text-decoration: underline", "underline");
    let zoomed = the_line("zoom: 2; text-decoration: underline", "underline");
    assert!(close(zoomed.thickness, plain.thickness * 2.0), "{plain:?} {zoomed:?}");
    let zoomed = the_line(
        "zoom: 2; text-decoration: underline; text-decoration-thickness: 3px",
        "underline",
    );
    assert!(close(zoomed.thickness, 6.0), "{zoomed:?}");
    let zoomed = the_line(
        "zoom: 2; text-decoration: underline; text-decoration-thickness: 0.25em",
        "underline",
    );
    assert!(close(zoomed.thickness, 10.0), "a quarter of the zoomed 40 px: {zoomed:?}");
}

/// A hostile length is capped at a thickness no text needs, with a diagnostic, and the layout
/// goes on.
#[test]
fn a_hostile_thickness_is_capped_with_a_diagnostic() {
    let _lock = azul_core::diagnostics::test_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    azul_core::diagnostics::clear();
    for value in ["1000000px", "1e9px", "99999999999999999999999999px", "1e39em", "100000000%"] {
        let line = the_line(
            &format!("text-decoration: underline; text-decoration-thickness: {value}"),
            "underline",
        );
        assert!(
            line.thickness.is_finite() && line.thickness <= 1000.0 && line.thickness >= 100.0,
            "{value}: {line:?}"
        );
    }
    assert!(
        azul_core::diagnostics::any_contains("text-decoration-thickness"),
        "the cap is reported: {:?}",
        azul_core::diagnostics::recorded()
    );
}
