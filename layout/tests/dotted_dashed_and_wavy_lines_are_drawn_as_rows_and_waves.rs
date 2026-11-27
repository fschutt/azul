//! `text-decoration-style` `dotted`, `dashed` and `wavy` (CSS Text Decoration 3, 2.3) reach the
//! display list as what every renderer draws already: a dotted line is a row of squares a
//! thickness wide with a gap as wide, a dashed line a row of dashes three thicknesses long with a
//! gap of two, and a wavy line one stroked path - a wave a thickness high either way and four
//! thicknesses long, round at its ends - that the CPU rasteriser strokes, the PDF writer writes
//! as a path and the GPU renderer paints through its mask. A build without the rasteriser draws
//! the wave solid.
//!
//! The faces and the page are those of the text rig: `Azul Mock Mono` at 20 px, so that a line is
//! 20 x 0.08 = 1.6 px thick and an underline's top edge lies 2.4 px below its baseline; two
//! letters are 20 px wide.
//!
//! Not compiled by the author (house rule).

use azul_css::props::basic::ColorU;
use azul_layout::solver3::display_list::{DisplayList, DisplayListItem};

use crate::text_rig::{block, lay_out, painted, text, window, MONO};

const RED: ColorU = ColorU { r: 255, g: 0, b: 0, a: 255 };

/// One rectangle of a line of the display list.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Piece {
    kind: &'static str,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

fn pieces(dl: &DisplayList) -> Vec<Piece> {
    dl.items
        .iter()
        .filter_map(|item| {
            let (kind, bounds) = match item {
                DisplayListItem::Underline { bounds, .. } => ("underline", bounds),
                DisplayListItem::Overline { bounds, .. } => ("overline", bounds),
                DisplayListItem::Strikethrough { bounds, .. } => ("line-through", bounds),
                _ => return None,
            };
            let r = bounds.inner();
            Some(Piece { kind, x: r.origin.x, y: r.origin.y, width: r.size.width, height: r.size.height })
        })
        .collect()
}

/// The stroked paths of the list: the points of each, its colour and its width.
fn strokes(dl: &DisplayList) -> Vec<(Vec<(f32, f32)>, ColorU, f32)> {
    dl.items
        .iter()
        .filter_map(|item| {
            let DisplayListItem::StrokedPath { path, color, width, .. } = item else {
                return None;
            };
            let mut points = Vec::new();
            for ring in path.rings.as_ref() {
                for element in ring.items.as_ref() {
                    if let azul_core::svg::SvgPathElement::Line(line) = element {
                        if points.is_empty() {
                            points.push((line.start.x, line.start.y));
                        }
                        points.push((line.end.x, line.end.y));
                    }
                }
            }
            Some((points, *color, *width))
        })
        .collect()
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-2
}

/// The display list and the baseline of one block of `ab` under `css`.
fn under(css: &str) -> (DisplayList, f32) {
    let mut lw = window();
    let dl = lay_out(&mut lw, block(MONO, css, vec![text("ab")]), 400.0);
    let base = painted(&dl)[0].y;
    (dl, base)
}

#[test]
fn a_dotted_line_is_a_row_of_squares_with_a_gap_as_wide() {
    let (dl, base) = under("text-decoration: underline dotted");
    let found = pieces(&dl);
    // 20 px of text in dots of 1.6 px every 3.2 px: 7 dots (the last one is cut at the end)
    assert_eq!(found.len(), 7, "{found:?}");
    for (i, piece) in found.iter().enumerate() {
        assert_eq!(piece.kind, "underline");
        assert!(close(piece.x, 3.2 * i as f32), "{i}: every two thicknesses: {piece:?}");
        assert!(close(piece.y - base, 2.4) && close(piece.height, 1.6), "{piece:?}");
        assert!(piece.width <= 1.6 + 1e-3, "{piece:?}");
    }
    assert!(found.iter().take(6).all(|p| close(p.width, 1.6)), "squares: {found:?}");
    assert!(
        close(found[6].x + found[6].width, 20.0),
        "the last one stops where the line ends: {found:?}"
    );
}

#[test]
fn a_dashed_line_is_a_row_of_dashes_three_thicknesses_long() {
    let (dl, base) = under("text-decoration: underline dashed");
    let found = pieces(&dl);
    // dashes of 4.8 px every 8 px along 20 px: three, the last cut at 4 px
    assert_eq!(found.len(), 3, "{found:?}");
    for (i, piece) in found.iter().enumerate() {
        assert!(close(piece.x, 8.0 * i as f32), "{i}: {piece:?}");
        assert!(close(piece.y - base, 2.4) && close(piece.height, 1.6), "{piece:?}");
    }
    assert!(close(found[0].width, 4.8) && close(found[1].width, 4.8), "{found:?}");
    assert!(close(found[2].width, 4.0), "cut where the line ends: {found:?}");
}

#[test]
fn the_other_kinds_of_line_are_rows_as_well() {
    for (decoration, kind, from_baseline) in [
        ("overline", "overline", -17.0),
        ("line-through", "line-through", -6.0),
    ] {
        let (dl, base) = under(&format!("text-decoration: {decoration} dotted"));
        let found = pieces(&dl);
        assert_eq!(found.len(), 7, "{decoration}: {found:?}");
        assert!(found.iter().all(|p| p.kind == kind), "{decoration}: {found:?}");
        assert!(found.iter().all(|p| close(p.y - base, from_baseline)), "{decoration}: {found:?}");
    }
}

/// A thicker row has bigger and wider-spaced pieces: they are parts of the thickness.
#[test]
fn the_pieces_are_parts_of_the_thickness() {
    let (dl, _) = under("text-decoration: underline dotted 4px");
    let found = pieces(&dl);
    assert_eq!(found.len(), 3, "20 px: dots of 4 px every 8 px: {found:?}");
    assert!(found.iter().take(2).all(|p| close(p.width, 4.0) && close(p.height, 4.0)), "{found:?}");
}

#[test]
fn a_wavy_line_is_one_stroked_wave() {
    let (dl, base) = under("text-decoration: underline wavy red 2px");
    if !cfg!(feature = "cpurender") {
        assert_eq!(pieces(&dl).len(), 1, "no rasteriser: a solid line");
        return;
    }
    assert!(pieces(&dl).is_empty(), "no rectangles: {:?}", pieces(&dl));
    let found = strokes(&dl);
    assert_eq!(found.len(), 1, "one path: {found:?}");
    let (points, color, width) = &found[0];
    assert_eq!(*color, RED);
    assert!(close(*width, 2.0), "as thick as the line: {width}");
    assert!(points.len() > 8, "a wave has many points: {}", points.len());
    // from the start to the end of the 20 px of text
    assert!(close(points[0].0, 0.0) && close(points[points.len() - 1].0, 20.0), "{points:?}");
    // a thickness (2 px) high either way, around the line's place: the top of an underline, then
    // the amplitude and half the thickness
    let centre = base + 2.4 + 2.0 + 1.0;
    let (low, high) = points
        .iter()
        .fold((f32::MAX, f32::MIN), |(low, high), p| (low.min(p.1), high.max(p.1)));
    assert!(close(low, centre - 2.0) && close(high, centre + 2.0), "{low} {high} around {centre}");
    // four thicknesses long: a crest at every 8 px, 2 px into the line, and a trough 6 px in
    let crest = points.iter().find(|p| close(p.0, 2.0)).copied();
    let trough = points.iter().find(|p| close(p.0, 6.0)).copied();
    assert!(crest.is_some_and(|p| close(p.1, centre + 2.0)), "crest: {crest:?} around {centre}");
    assert!(trough.is_some_and(|p| close(p.1, centre - 2.0)), "trough: {trough:?} around {centre}");
}

/// Two boxes that draw the same dotted line in one place and one colour draw it once.
#[test]
fn the_same_dotted_line_of_two_boxes_is_drawn_once() {
    let mut lw = window();
    let dom = block(
        MONO,
        "text-decoration: underline dotted",
        vec![text("a"), azul_core::dom::Dom::create_span()
            .with_css("text-decoration: underline dotted")
            .with_child(text("b"))],
    );
    let dl = lay_out(&mut lw, dom, 400.0);
    let found = pieces(&dl);
    // 10 px of text each: 4 dots (the last cut at 10 px)
    assert_eq!(found.len(), 8, "{found:?}");
}
