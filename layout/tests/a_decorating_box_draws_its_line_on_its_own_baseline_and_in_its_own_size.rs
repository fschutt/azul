//! The line a box sets is drawn in the size and on the baseline of THAT box, under all the text in
//! it, and the lines of the boxes inside it are drawn as well: one line per decorating box.
//!
//! CSS Text Decoration 3, 2.1 and 3: a decoration is propagated to the text of all in-flow
//! descendants, and the lines are drawn "with the position, thickness and style of the decorating
//! box", so that a smaller or a raised box inside does not move the line of the box around it:
//! `text-decoration: underline` on a block with a superscript in it is one continuous underline at
//! the baseline of the block, and a superscript that sets an underline of its own has a second
//! one, at its own baseline and in its own size. A line drew in the size and on the baseline of
//! the text it crossed, and the line of an inner box took the place of the line of the box around
//! it (WPT `css/css-text-decor/text-decoration-subelements-003.html`).
//!
//! The faces and the page are those of the text rig: `Azul Mock Mono` at 20 px. An underline's top
//! edge lies 0.12 em below its baseline, an overline's 0.85 em above it, a line-through's 0.3 em
//! above it, and a line is 0.08 em thick (at least 1 px): 2.4, -17, -6 and 1.6 px at 20 px, and 1.2,
//! -8.5, -3 and 1 px at 10 px.
//!
//! Not compiled by the author (house rule).

use azul_core::dom::Dom;
use azul_css::props::{basic::ColorU, style::text::StyleTextDecoration};
use azul_layout::{
    solver3::display_list::{DisplayList, DisplayListItem},
    text3::cache::{LineKind, LineStand, TextDecoration},
};

use crate::text_rig::{block, lay_out, painted, span, text, window, MONO};

const RED: ColorU = ColorU { r: 255, g: 0, b: 0, a: 255 };
const BLUE: ColorU = ColorU { r: 0, g: 0, b: 255, a: 255 };

/// One line of the display list.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Line {
    kind: &'static str,
    color: ColorU,
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
            let (kind, bounds, color) = match item {
                DisplayListItem::Underline { bounds, color, .. } => ("underline", bounds, *color),
                DisplayListItem::Overline { bounds, color, .. } => ("overline", bounds, *color),
                DisplayListItem::Strikethrough { bounds, color, .. } => {
                    ("line-through", bounds, *color)
                }
                _ => return None,
            };
            let r = bounds.inner();
            Some(Line {
                kind,
                color,
                x: r.origin.x,
                y: r.origin.y,
                width: r.size.width,
                height: r.size.height,
            })
        })
        .collect()
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-2
}

/// The kinds of line: the declaration, and where the top edge of the line of a 20 px box is under
/// (positive) or over (negative) its baseline when it is as thick as the font size suggests; and
/// the same at 10 px.
const KINDS: [(&str, &str, f32, f32); 3] = [
    ("underline", "text-decoration: underline", 2.4, 1.2),
    ("overline", "text-decoration: overline", -17.0, -8.5),
    ("line-through", "text-decoration: line-through", -6.0, -3.0),
];

/// The baselines of the glyphs `a`, `b`, `c`, `d` of a block of `ab` and a box of `cd`.
fn baselines(dl: &DisplayList) -> [f32; 4] {
    let glyphs = painted(dl);
    assert_eq!(glyphs.len(), 4, "the four letters are painted: {glyphs:?}");
    [glyphs[0].y, glyphs[1].y, glyphs[2].y, glyphs[3].y]
}

/// A box in a decorated block that is smaller than the block and stands on its baseline: the line
/// of the block under it is the line under the rest of the block - the same size, on the same
/// baseline - and not a line in the size of the smaller text.
#[test]
fn a_smaller_box_does_not_change_the_line_of_the_box_around_it() {
    for (kind, decoration, at20, _at10) in KINDS {
        let mut lw = window();
        let dom = block(MONO, decoration, vec![text("ab"), span("font-size: 10px", "cd")]);
        let dl = lay_out(&mut lw, dom, 400.0);
        let base = baselines(&dl);
        assert!(close(base[0], base[2]), "{kind}: both stand on one baseline: {base:?}");
        let found = lines(&dl);
        assert_eq!(found.len(), 2, "{kind}: one line under each run: {found:?}");
        for line in &found {
            assert_eq!(line.kind, kind);
            assert!(
                close(line.y - base[0], at20),
                "{kind}: the line is where the 20 px box puts it, {at20} from the baseline: \
                 {line:?} on {base:?}"
            );
            assert!(close(line.height, 1.6), "{kind}: as thick as the 20 px box's: {line:?}");
        }
        assert!(
            close(found[0].x + found[0].width, found[1].x),
            "{kind}: the two lines are one: {found:?}"
        );
    }
}

/// The same for a bigger box: the line of the block under it is not the size of the bigger text.
#[test]
fn a_bigger_box_does_not_change_the_line_of_the_box_around_it() {
    for (kind, decoration, at20, _at10) in KINDS {
        let mut lw = window();
        let dom = block(MONO, decoration, vec![text("ab"), span("font-size: 40px", "cd")]);
        let dl = lay_out(&mut lw, dom, 400.0);
        let base = baselines(&dl);
        let found = lines(&dl);
        assert_eq!(found.len(), 2, "{kind}: one line under each run: {found:?}");
        for line in &found {
            assert!(close(line.y - base[0], at20), "{kind}: {line:?} on {base:?}");
            assert!(close(line.height, 1.6), "{kind}: {line:?}");
        }
    }
}

/// A raised box (a superscript) with no line of its own: the line of the block under it stays on
/// the baseline of the block, below the raised text, and does not rise with it.
#[test]
fn a_raised_box_does_not_move_the_line_of_the_box_around_it() {
    for (kind, decoration, at20, _at10) in KINDS {
        let mut lw = window();
        let dom = block(
            MONO,
            decoration,
            vec![text("ab"), span("font-size: 10px; vertical-align: super", "cd")],
        );
        let dl = lay_out(&mut lw, dom, 400.0);
        let base = baselines(&dl);
        assert!(base[2] < base[0] - 5.0, "{kind}: the box is raised: {base:?}");
        let found = lines(&dl);
        assert_eq!(found.len(), 2, "{kind}: one line under each run: {found:?}");
        for line in &found {
            assert!(
                close(line.y - base[0], at20),
                "{kind}: on the baseline of the block, not of the raised text: {line:?} on \
                 {base:?}"
            );
            assert!(close(line.height, 1.6), "{kind}: {line:?}");
        }
    }
}

/// A raised box that sets a line of its own, in a block that sets one: the text of the box has two
/// lines - the block's, on the baseline of the block and in its size, and the box's own, on its
/// raised baseline and in its smaller size. This is the page of the WPT test.
#[test]
fn a_raised_box_with_a_line_of_its_own_has_a_second_line() {
    let mut lw = window();
    let dom = block(
        MONO,
        "text-decoration: underline",
        vec![
            text("ab"),
            span("font-size: 10px; vertical-align: super; text-decoration: underline", "cd"),
        ],
    );
    let dl = lay_out(&mut lw, dom, 400.0);
    let base = baselines(&dl);
    assert!(base[2] < base[0] - 5.0, "the box is raised: {base:?}");
    let found = lines(&dl);
    assert_eq!(found.len(), 3, "one under `ab`, two under `cd`: {found:?}");
    assert!(close(found[0].y - base[0], 2.4) && close(found[0].height, 1.6), "{found:?}");
    // the line of the block continues under the raised box
    assert!(close(found[1].y - base[0], 2.4), "the block's line, on the block's baseline: {found:?}");
    assert!(close(found[1].height, 1.6), "in the block's size: {found:?}");
    assert!(close(found[0].x + found[0].width, found[1].x), "continuous: {found:?}");
    // and the box has its own, raised and smaller
    assert!(close(found[2].y - base[2], 1.2), "the box's own line on its own baseline: {found:?}");
    assert!(close(found[2].height, 1.0), "in the box's own size: {found:?}");
    assert!(close(found[1].x, found[2].x) && close(found[1].width, found[2].width), "{found:?}");
}

/// Two lines of one kind are painted in the order of the boxes, the outer one first, each in the
/// colour of the box that sets it.
#[test]
fn the_line_of_the_inner_box_is_painted_over_the_line_of_the_outer_one() {
    let mut lw = window();
    let dom = block(
        MONO,
        "text-decoration: underline; text-decoration-color: #ff0000",
        vec![
            text("ab"),
            span(
                "font-size: 10px; vertical-align: super; text-decoration: underline; \
                 text-decoration-color: #0000ff",
                "cd",
            ),
        ],
    );
    let found = lines(&lay_out(&mut lw, dom, 400.0));
    assert_eq!(found.len(), 3, "{found:?}");
    assert_eq!(found[0].color, RED, "the block's line under `ab`: {found:?}");
    assert_eq!(found[1].color, RED, "the block's line under `cd`, first: {found:?}");
    assert_eq!(found[2].color, BLUE, "the box's own line under `cd`, over it: {found:?}");
}

/// The kinds do not take each other's place: a line-through of the block and an underline of the
/// box inside it are two lines under the box, each on the baseline and in the size of the box
/// that sets it.
#[test]
fn a_line_of_another_kind_is_a_line_of_its_own() {
    let mut lw = window();
    let dom = block(
        MONO,
        "text-decoration: line-through",
        vec![
            text("ab"),
            span("font-size: 10px; vertical-align: super; text-decoration: underline", "cd"),
        ],
    );
    let dl = lay_out(&mut lw, dom, 400.0);
    let base = baselines(&dl);
    let found = lines(&dl);
    let through: Vec<_> = found.iter().filter(|l| l.kind == "line-through").collect();
    let under: Vec<_> = found.iter().filter(|l| l.kind == "underline").collect();
    assert_eq!(through.len(), 2, "the block's line-through under both runs: {found:?}");
    assert_eq!(under.len(), 1, "the box's underline under its run: {found:?}");
    assert!(
        close(through[1].y - base[0], -6.0) && close(through[1].height, 1.6),
        "the line-through under `cd` is the block's: {found:?}"
    );
    assert!(
        close(under[0].y - base[2], 1.2) && close(under[0].height, 1.0),
        "the underline is the box's own: {found:?}"
    );
}

/// One line is one line: a box that sets the same line as the box around it, on the same baseline
/// and in the same size, paints it once.
#[test]
fn the_same_line_of_two_boxes_on_one_baseline_is_painted_once() {
    let mut lw = window();
    let dom = block(
        MONO,
        "text-decoration: underline",
        vec![
            text("ab"),
            Dom::create_span()
                .with_css("text-decoration: underline")
                .with_child(text("cd")),
        ],
    );
    let found = lines(&lay_out(&mut lw, dom, 400.0));
    assert_eq!(found.len(), 2, "one line under each run, none twice: {found:?}");
}

/// A box that is itself raised and sets a line: the line is on ITS baseline under all the text in
/// it, a smaller box inside it that stands on the baseline of the raised box included.
#[test]
fn the_line_of_a_raised_box_goes_with_the_text_that_stands_on_its_baseline() {
    let mut lw = window();
    let dom = block(
        MONO,
        "",
        vec![
            text("xy"),
            Dom::create_span()
                .with_css("vertical-align: super; text-decoration: underline")
                .with_child(text("ab"))
                .with_child(span("font-size: 10px", "cd")),
        ],
    );
    let dl = lay_out(&mut lw, dom, 400.0);
    let glyphs = painted(&dl);
    assert_eq!(glyphs.len(), 6, "{glyphs:?}");
    let (line_base, raised_base) = (glyphs[0].y, glyphs[2].y);
    assert!(raised_base < line_base - 5.0, "the box is raised: {glyphs:?}");
    assert!(close(glyphs[4].y, raised_base), "the smaller box stands on it: {glyphs:?}");
    let found = lines(&dl);
    assert_eq!(found.len(), 2, "one line under each run of the raised box: {found:?}");
    for line in &found {
        assert!(
            close(line.y - raised_base, 2.4) && close(line.height, 1.6),
            "the line of the raised box, on its baseline, in its size: {line:?} on {glyphs:?}"
        );
    }
}

/// The lines of a box of a font size of 20 px that sets `css`.
fn of(css: StyleTextDecoration) -> TextDecoration {
    TextDecoration::from_css(css).sized(20.0)
}

/// What the lines of a run are made of: the lines of the boxes around it come first - they are
/// painted first - and the line of the run's own box last.
#[test]
fn the_lines_of_the_boxes_around_a_run_come_before_its_own() {
    let own = of(StyleTextDecoration::LineThrough);
    let around = of(StyleTextDecoration::Underline);
    let both = own.within(Some(0.0), &around, Some(0.0));
    let kinds: Vec<LineKind> = both.lines().iter().map(|line| line.kind).collect();
    assert_eq!(kinds, vec![LineKind::Underline, LineKind::Strikethrough], "{both:?}");
    assert!(matches!(both.lines()[0].stand, LineStand::Around { .. }), "{both:?}");
    assert_eq!(both.lines()[1].stand, LineStand::Run, "{both:?}");
}

/// The line a box sets reaches the text of the box itself and, set as it is by the boxes around,
/// the same text again: one line.
#[test]
fn the_same_line_of_the_box_and_of_the_boxes_around_is_one_line() {
    let line = of(StyleTextDecoration::Underline);
    let both = line.within(Some(0.0), &line, Some(0.0));
    assert_eq!(both.lines().len(), 1, "{both:?}");
    assert_eq!(both.lines()[0].stand, LineStand::Run, "{both:?}");
    // another size is another line
    let bigger = TextDecoration::from_css(StyleTextDecoration::Underline).sized(40.0);
    assert_eq!(line.within(Some(0.0), &bigger, Some(0.0)).lines().len(), 2);
}

/// A run raised by 8 px over its line: the baseline of a box around it, which stands on the
/// baseline of the line, is 8 px below the baseline of the run.
#[test]
fn a_raised_run_has_the_line_of_the_box_around_it_below_its_own_baseline() {
    let own = of(StyleTextDecoration::Underline).sized(10.0);
    let around = of(StyleTextDecoration::Underline);
    let both = own.within(Some(8.0), &around, Some(0.0));
    assert_eq!(both.lines().len(), 2, "{both:?}");
    match both.lines()[0].stand {
        LineStand::Around { raise, drop } => {
            assert_eq!(raise.map(|up| up.get()), Some(0.0), "{both:?}");
            assert!((drop.get() - 8.0).abs() < 1e-3, "{both:?}");
        }
        LineStand::Run => panic!("the line of the box around is the first: {both:?}"),
    }
    assert!((both.lines()[0].font_size.get() - 20.0).abs() < 1e-3, "in the size of that box");
    // a box that is raised as the run is, is on the baseline of the run
    let level = own.within(Some(8.0), &around, Some(8.0));
    match level.lines()[0].stand {
        LineStand::Around { drop, .. } => assert!(drop.get().abs() < 1e-6, "{level:?}"),
        LineStand::Run => panic!("{level:?}"),
    }
}

/// `top`, `bottom`, `middle`, `text-top` and `text-bottom` put a box where only the line layout
/// knows: the line of a box around stands on the baseline of the run then.
#[test]
fn an_alignment_that_is_no_plain_raise_leaves_the_line_on_the_baseline_of_the_run() {
    let own = of(StyleTextDecoration::Underline).sized(10.0);
    let around = of(StyleTextDecoration::Underline);
    for (run, boxed) in [(None, Some(0.0)), (Some(5.0), None), (None, None)] {
        let both = own.within(run, &around, boxed);
        match both.lines()[0].stand {
            LineStand::Around { drop, .. } => assert!(drop.get().abs() < 1e-6, "{both:?}"),
            LineStand::Run => panic!("{both:?}"),
        }
    }
}

/// The editor's underline and line-through are lines no box sets: switched on, they are plain
/// lines; switched off, every line of the kind is gone.
#[test]
fn a_kind_of_line_is_switched_on_and_off() {
    let none = TextDecoration::default();
    assert!(!none.has_lines());
    let on = none.with_kind(LineKind::Underline, true);
    assert!(on.underline() && !on.strikethrough() && !on.overline(), "{on:?}");
    assert_eq!(on.with_kind(LineKind::Underline, true), on, "already on");
    assert_eq!(on.with_kind(LineKind::Underline, false), none, "off again");
    let both = on.with_kind(LineKind::Strikethrough, true);
    assert!(both.underline() && both.strikethrough(), "{both:?}");
    assert_eq!(both.with_kind(LineKind::Strikethrough, false), on, "{both:?}");
}
