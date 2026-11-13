//! A line too wide for its box overflows past its END edge, whatever its
//! `text-align` (SMALL6, via MAILENG6).
//!
//! CSS Text 3 section 7.1 (text-align): "If (after justification, if any)
//! the inline contents of a line box are too long to fit within it, then the
//! contents are start-aligned: any content that doesn't fit overflows the
//! line box's end edge." Chrome 154 does exactly that for `right`, `center`,
//! `end` and `justify` (probe: 23 unbreakable glyphs in a 100px box).
//! text3's `position_one_line` applied the (negative) remaining space of a
//! right- or center-aligned line as it was: the line started left of its box
//! and its START was cut off - AzCalculator's long results in the side panel.
//! In a right-to-left line the start edge is the right one: the content's
//! right edge stays on the box's, and the overflow goes left.
//!
//! `Azul Mock Mono` advances exactly 0.5em: twenty glyphs at 20px are 200px,
//! twice the 100px box. Not compiled by the author (house rule); RED before
//! the fix.

use crate::table_markup::{body, glyph_runs, near, rect};

/// Twenty glyphs, 200px: no break opportunity, twice as wide as the box.
const LONG: &str = "AAAAAAAAAAAAAAAAAAAA";

/// The glyph pens' x extent `(first left, last left + 10px advance)` of the
/// only text run.
fn run_extent(lw: &azul_layout::window::LayoutWindow) -> (f32, f32) {
    let runs = glyph_runs(lw);
    let run = runs.first().expect("the line paints its glyphs");
    assert_eq!(run.len(), 20, "every glyph of the line is painted: {run:?}");
    let left = run.iter().map(|g| g.0).fold(f32::INFINITY, f32::min);
    let right = run.iter().map(|g| g.0).fold(f32::NEG_INFINITY, f32::max) + 10.0;
    (left, right)
}

fn boxed(dir: &str, align: &str) -> azul_layout::window::LayoutWindow {
    body(&format!(
        "<div id=\"b\" dir=\"{dir}\" style=\"width: 100px; font-family: 'Azul Mock Mono'; \
         font-size: 20px; text-align: {align}\">{LONG}</div>"
    ))
}

#[test]
fn an_overflowing_ltr_line_starts_at_its_left_edge_whatever_its_alignment() {
    for align in ["left", "right", "center", "end", "justify", "start"] {
        let lw = boxed("ltr", align);
        let b = rect(&lw, "b");
        let (left, right) = run_extent(&lw);
        assert!(
            near(left, b.origin.x, 0.5),
            "text-align: {align}: the overflowing line starts at the box's left (start) edge \
             x {} and overflows past its right one, got glyphs from {left} to {right}",
            b.origin.x
        );
        assert!(
            near(right, b.origin.x + 200.0, 0.5),
            "text-align: {align}: the 200px line ends 100px past the box: {right}"
        );
    }
}

#[test]
fn an_overflowing_rtl_line_ends_at_its_right_edge_whatever_its_alignment() {
    for align in ["left", "right", "center", "end", "justify", "start"] {
        let lw = boxed("rtl", align);
        let b = rect(&lw, "b");
        let (left, right) = run_extent(&lw);
        assert!(
            near(right, b.origin.x + b.size.width, 0.5),
            "dir=rtl text-align: {align}: the overflowing line keeps its start on the box's \
             RIGHT edge x {} and overflows past its left one, got glyphs from {left} to {right}",
            b.origin.x + b.size.width
        );
    }
}

#[test]
fn a_line_that_fits_is_still_aligned_as_asked() {
    // Five glyphs, 50px: the alignment is untouched by the overflow rule.
    for (align, start) in [("left", 0.0), ("right", 50.0), ("center", 25.0)] {
        let lw = body(&format!(
            "<div id=\"b\" style=\"width: 100px; font-family: 'Azul Mock Mono'; \
             font-size: 20px; text-align: {align}\">AAAAA</div>"
        ));
        let b = rect(&lw, "b");
        let runs = glyph_runs(&lw);
        let left = runs[0].iter().map(|g| g.0).fold(f32::INFINITY, f32::min);
        assert!(
            near(left - b.origin.x, start, 0.5),
            "text-align: {align}: a 50px line in a 100px box starts at {start}, got {}",
            left - b.origin.x
        );
    }
}
