//! The history graphs and usage meters of the Performance and Networking
//! tabs, drawn as the old Task Manager drew them: green lines on black over a
//! green grid, the newest reading at the right edge; a column of lit and dark
//! green bars for "how much now".
//!
//! STEADY: a graph always spans [`SLOTS`] readings - a short history is
//! padded with zeros on the left, like a monitor that just started - so its
//! time axis never re-zooms while the history fills, and a rate graph's top
//! moves only in calm 1-2-5 steps ([`next_top`]).
//!
//! SMOOTH: the lines and the vertical grid sit in a STRIP. A reading draws
//! the strip with the newest reading at the right edge; until the next
//! reading the window's frame timer slides the strip left by the share of
//! the interval that has passed ([`offset`], `set_css_property` with a
//! `translateX`). When the next reading arrives the strip stands exactly one
//! step left - where the new drawing puts every old point - so the history
//! scrolls continuously instead of jumping a step a second. A grid line
//! belongs to a reading ([`grid_xs`]), so it travels with the lines.
//!
//! STABLE NODES: every drawing of a graph has the same nodes (the same
//! number of segments, grid lines and bars), only their styles change.
//!
//! Plain boxes only: a line segment is a thin box rotated about its centre.

use azul::{
    css::{CssProperty, PixelValue, StyleTransform},
    option::OptionString,
    prelude::*,
    str::String as AzString,
    vec::StyleTransformVec,
};

use crate::{ids, model::CHART_READINGS};

/// The readings a graph spans: the model's minute.
pub const SLOTS: usize = CHART_READINGS;

/// The graph's ground.
pub const BACKGROUND: &str = "#000000";
/// The grid.
pub const GRID: &str = "#008040";
/// The first line (CPU, memory, received).
pub const LINE: &str = "#00ff00";
/// The second line (sent).
pub const LINE_2: &str = "#ffff00";
/// A lit meter bar.
pub const METER_LIT: &str = "#00ff00";
/// A dark meter bar.
pub const METER_DARK: &str = "#005a00";

/// A grid cell, px (the old Task Manager's squares).
const CELL: f32 = 12.0;
/// A line's thickness, px.
const THICKNESS: f32 = 1.5;
/// A meter bar's height and the gap under it, px.
const BAR: f32 = 3.0;
const BAR_GAP: f32 = 1.0;
/// The meter's value line, px.
const METER_TEXT: f32 = 16.0;

// ==== Geometry (pure) ====

/// `values` (oldest first) as exactly `slots` readings: a short history is
/// padded with zeros on the left, a long one keeps its newest; a broken
/// reading (NaN, infinite) counts as nothing.
#[must_use]
pub fn padded(values: &[f64], slots: usize) -> Vec<f64> {
    let keep = values.len().min(slots);
    let mut out = vec![0.0; slots - keep];
    out.extend(
        values[values.len() - keep..]
            .iter()
            .map(|v| if v.is_finite() { *v } else { 0.0 }),
    );
    out
}

/// The px between two readings on a graph `width` px wide: `slots - 2`
/// steps span the width, so the oldest reading stands one step left of the
/// box and the line still covers the box while the strip slides by a step.
#[must_use]
#[allow(clippy::cast_precision_loss)] // a few dozen slots
pub fn step(width: f32, slots: usize) -> f32 {
    let steps = slots.saturating_sub(2).max(1) as f32;
    width.max(1.0) / steps
}

/// Where reading `i` (0 = the oldest of `slots`) stands in the strip.
#[must_use]
#[allow(clippy::cast_precision_loss)] // a few dozen slots
pub fn slot_x(i: usize, slots: usize, width: f32, step: f32) -> f32 {
    let from_newest = slots.saturating_sub(1).saturating_sub(i);
    width - from_newest as f32 * step
}

/// Where `value` stands on a graph `height` px high whose top is `top`: the
/// bottom for nothing, a pixel under the top edge for the top (so a full
/// line stays visible).
#[must_use]
#[allow(clippy::cast_possible_truncation)] // a share 0..=1
pub fn value_y(value: f64, top: f64, height: f32) -> f32 {
    let share = if top > 0.0 && value.is_finite() {
        (value / top).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let usable = (height - 2.0).max(0.0);
    height - 1.0 - share as f32 * usable
}

/// A line segment as a box: where it starts, how long it is, how far it is
/// turned (degrees, clockwise) about its centre.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Segment {
    pub left: f32,
    pub top: f32,
    pub length: f32,
    pub angle: f32,
}

/// The box that draws the line from `from` to `to`, `thickness` px thick: a
/// little longer than the gap, so two segments meet without a notch.
#[must_use]
pub fn segment(from: (f32, f32), to: (f32, f32), thickness: f32) -> Segment {
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    let length = (dx * dx + dy * dy).sqrt() + thickness;
    let angle = dy.atan2(dx).to_degrees();
    let (cx, cy) = ((from.0 + to.0) / 2.0, (from.1 + to.1) / 2.0);
    Segment {
        left: cx - length / 2.0,
        top: cy - thickness / 2.0,
        length,
        angle,
    }
}

/// The readings between two vertical grid lines: about a [`CELL`] apart.
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // a small count
pub fn grid_every(step: f32) -> usize {
    ((CELL / step.max(0.1)).round() as usize).max(1)
}

/// The x of every vertical grid line in the strip: on every `every`-th
/// reading counted from the first one ever (`readings` so far), so a line
/// travels with its reading and the next drawing puts it where the slide
/// left it. Always `slots / every + 1` lines (the oldest may stand left of
/// the box), so every drawing has as many as the one before.
#[must_use]
#[allow(clippy::cast_precision_loss, clippy::cast_possible_wrap)] // small counts
pub fn grid_xs(readings: u64, slots: usize, width: f32, step: f32, every: usize) -> Vec<f32> {
    let every = every.max(1);
    let newest = slots.saturating_sub(1) as i64;
    let first = newest - (readings % every as u64) as i64;
    (0..=slots / every)
        .map(|k| {
            let i = first - (k * every) as i64;
            width - (newest - i) as f32 * step
        })
        .collect()
}

/// How far the strip has slid (px, to the left): `elapsed_ms` after the
/// reading it shows, readings `gap_ms` apart, `step` px a reading. Never
/// more than a step: a late reading stops the slide, it does not run ahead.
#[must_use]
#[allow(clippy::cast_possible_truncation)] // a share 0..=1
pub fn offset(elapsed_ms: f64, gap_ms: f64, step: f32) -> f32 {
    if !(gap_ms > 0.0) || !elapsed_ms.is_finite() {
        return 0.0;
    }
    (elapsed_ms / gap_ms).clamp(0.0, 1.0) as f32 * step
}

/// The smallest 1-2-5 step (1, 2, 5, 10, 20, 50 ...) at or over `value`,
/// at least `floor` (and 1 for nothing at all).
#[must_use]
#[allow(clippy::cast_possible_truncation)] // a decimal exponent
pub fn nice_top(value: f64, floor: f64) -> f64 {
    let v = if value.is_finite() {
        value.max(floor)
    } else {
        floor
    };
    if !(v > 0.0) {
        return 1.0;
    }
    // `powi` multiplies: 10^3 is exactly 1000.
    let base = 10f64.powi(v.log10().floor() as i32);
    for m in [1.0, 2.0, 5.0, 10.0] {
        if m * base >= v {
            return m * base;
        }
    }
    10.0 * base
}

/// A rate graph's next top: up at once to the next nice step over the peak
/// (with a little room), down only when the peak fell under a third of the
/// top - the scale does not breathe with every reading.
#[must_use]
pub fn next_top(current: f64, peak: f64, floor: f64) -> f64 {
    let peak = if peak.is_finite() { peak.max(0.0) } else { 0.0 };
    let wanted = nice_top(peak * 1.1, floor);
    if !(current > 0.0) || wanted > current || peak < current / 3.0 {
        wanted
    } else {
        current
    }
}

// ==== The scroll (where the graphs stand between readings) ====

/// How many readings right of the graphs' right edge the newest reading
/// stands when it arrives.
pub const LAG: f64 = 1.25;

/// How far (in readings) the scroll may be off where it should stand when a
/// reading arrives before it starts over there.
pub const CATCH_UP: f64 = 1.5;

/// The graphs' scroll: which reading stands at their right edge - a reading
/// number with a fraction - moving on at a steady pace.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Scroll {
    /// The reading at the right edge at `at_ms`.
    pub position: f64,
    /// Readings per ms.
    pub rate: f64,
    /// When the scroll stood at `position`, ms on the app's clock.
    pub at_ms: f64,
}

impl Scroll {
    /// The scroll after reading `newest` arrived at `now_ms` (`previous`:
    /// the scroll so far, `None` before the first), readings `gap_ms` apart.
    #[must_use]
    pub fn after_reading(_previous: Option<Self>, now_ms: f64, newest: f64, gap_ms: f64) -> Self {
        Self {
            position: newest,
            rate: if gap_ms > 0.0 { 1.0 / gap_ms } else { 0.0 },
            at_ms: now_ms,
        }
    }

    /// The reading at the right edge at `now_ms`, `newest` being the newest
    /// reading drawn.
    #[must_use]
    pub fn position_at(&self, now_ms: f64, newest: f64) -> f64 {
        (self.position + (now_ms - self.at_ms).max(0.0) * self.rate).min(newest + 1.0)
    }

    /// How many readings right of the right edge the newest reading drawn,
    /// `newest`, stands at `now_ms`: a drawing's strip is shifted right by
    /// this many steps.
    #[must_use]
    pub fn lag(&self, now_ms: f64, newest: f64) -> f64 {
        newest - self.position_at(now_ms, newest)
    }
}

/// How many of a meter's `bars` are lit for `share` (0..=1) of the range.
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_precision_loss)]
pub fn lit_bars(share: f64, bars: usize) -> usize {
    let share = if share.is_finite() {
        share.clamp(0.0, 1.0)
    } else {
        0.0
    };
    ((share * bars as f64).round() as usize).min(bars)
}

// ==== The strips (what the frame timer slides) ====

/// The marker of strip `index` (the graphs of the tab shown, in drawing
/// order).
#[must_use]
pub fn strip_marker(index: usize) -> AzString {
    AzString::from(format!("{}strip-{index}", ids::PREFIX))
}

/// The strip's slide as a CSS property: `translateX(-px)`.
#[must_use]
pub fn slide(px: f32) -> CssProperty {
    CssProperty::transform(StyleTransformVec::from_vec(vec![StyleTransform::TranslateX(
        PixelValue::px(-px),
    )]))
}

/// The steps of the graphs drawn so far (strip `i` slides by `steps[i]` a
/// reading): what the frame timer needs to know about a drawing.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Strips {
    pub steps: Vec<f32>,
}

// ==== The DOM ====

/// One line of a graph: its readings (oldest first) and its colour.
pub struct Line<'a> {
    pub values: &'a [f64],
    pub color: &'static str,
}

/// A history graph `width` x `height` px: the black box, its fixed
/// horizontal grid, and the strip with the vertical grid and the `lines`
/// scaled to `top`. `readings` is how many readings arrived so far (where
/// the grid lines fall). Registers its strip in `strips`.
#[must_use]
pub fn graph(
    strips: &mut Strips,
    lines: &[Line<'_>],
    top: f64,
    readings: u64,
    width: f32,
    height: f32,
) -> Dom {
    let w = width.max(16.0).floor();
    let h = height.max(16.0).floor();
    let step = step(w, SLOTS);
    let index = strips.steps.len();
    strips.steps.push(step);

    let mut strip = Dom::create_div()
        .with_class(ids::GRAPH_STRIP)
        .with_marker(OptionString::Some(strip_marker(index)))
        .with_css(format!(
            "position: absolute; left: 0px; top: 0px; width: {w}px; height: {h}px; \
             transform: translateX(0px);"
        ));
    for x in grid_xs(readings, SLOTS, w, step, grid_every(step)) {
        strip.add_child(Dom::create_div().with_css(format!(
            "position: absolute; left: {x:.2}px; top: 0px; width: 1px; height: {h}px; \
             background-color: {GRID};"
        )));
    }
    for line in lines {
        let values = padded(line.values, SLOTS);
        let points: Vec<(f32, f32)> = values
            .iter()
            .enumerate()
            .map(|(i, v)| (slot_x(i, SLOTS, w, step), value_y(*v, top, h)))
            .collect();
        for pair in points.windows(2) {
            let s = segment(pair[0], pair[1], THICKNESS);
            strip.add_child(Dom::create_div().with_css(format!(
                "position: absolute; left: {:.2}px; top: {:.2}px; width: {:.2}px; height: \
                 {THICKNESS}px; background-color: {}; transform: rotate({:.2}deg);",
                s.left, s.top, s.length, line.color, s.angle
            )));
        }
    }

    let mut graph = Dom::create_div().with_class(ids::GRAPH).with_css(format!(
        "position: relative; width: {w}px; height: {h}px; flex-shrink: 0; overflow: hidden; \
         background-color: {BACKGROUND};"
    ));
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // a small count
    let rows = ((h / CELL).round() as usize).max(2);
    for j in 1..rows {
        #[allow(clippy::cast_precision_loss)] // a small count
        let y = (j as f32 * h / rows as f32).floor();
        graph.add_child(Dom::create_div().with_css(format!(
            "position: absolute; left: 0px; top: {y}px; width: {w}px; height: 1px; \
             background-color: {GRID};"
        )));
    }
    graph.with_child(strip)
}

/// The usage meter `width` x `height` px: lit bars for `share` (0..=1) of
/// the range in pairs, the value `text` under them - on black, in green.
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // a small count
pub fn meter(share: f64, text: &str, width: f32, height: f32) -> Dom {
    let w = width.max(40.0).floor();
    let h = height.max(40.0).floor();
    let bars_h = (h - METER_TEXT - 12.0).max(BAR + BAR_GAP);
    let bars = ((bars_h / (BAR + BAR_GAP)).floor() as usize).max(1);
    let lit = lit_bars(share, bars);
    let half = ((w - 24.0) / 2.0).floor().max(8.0);
    let mut column = Dom::create_div().with_css(format!(
        "display: flex; flex-direction: column; align-items: center; height: {bars_h}px; \
         justify-content: flex-end;"
    ));
    for k in 0..bars {
        // Drawn top down: the bar `k` from the top is lit when it is among
        // the `lit` lowest.
        let color = if bars - k <= lit {
            METER_LIT
        } else {
            METER_DARK
        };
        column.add_child(
            Dom::create_div()
                .with_css(format!(
                    "display: flex; flex-direction: row; height: {BAR}px; margin-bottom: \
                     {BAR_GAP}px; flex-shrink: 0;"
                ))
                .with_child(Dom::create_div().with_css(format!(
                    "width: {half}px; height: {BAR}px; margin-right: 2px; background-color: \
                     {color};"
                )))
                .with_child(Dom::create_div().with_css(format!(
                    "width: {half}px; height: {BAR}px; background-color: {color};"
                ))),
        );
    }
    Dom::create_div()
        .with_class(ids::METER)
        .with_css(format!(
            "display: flex; flex-direction: column; align-items: center; width: {w}px; \
             height: {h}px; flex-shrink: 0; padding-top: 6px; box-sizing: border-box; \
             background-color: {BACKGROUND};"
        ))
        .with_child(column)
        .with_child(Dom::create_p_with_text(text).with_css(format!(
            "margin: 4px 0px 0px 0px; height: {METER_TEXT}px; font-size: 12px; color: \
             {METER_LIT}; text-align: center;"
        )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_short_history_is_padded_on_the_left_and_a_long_one_keeps_its_newest() {
        assert_eq!(padded(&[5.0, 6.0], 4), vec![0.0, 0.0, 5.0, 6.0]);
        assert_eq!(padded(&[1.0, 2.0, 3.0, 4.0, 5.0], 3), vec![3.0, 4.0, 5.0]);
        assert_eq!(padded(&[f64::NAN, 2.0], 2), vec![0.0, 2.0]);
        assert_eq!(padded(&[], 2), vec![0.0, 0.0]);
    }

    #[test]
    fn the_newest_reading_stands_at_the_right_edge_and_the_oldest_a_step_left_of_the_box() {
        let w = 580.0;
        let s = step(w, 60);
        assert!((s - 10.0).abs() < 0.001, "58 steps span the width");
        assert!((slot_x(59, 60, w, s) - 580.0).abs() < 0.001);
        assert!((slot_x(1, 60, w, s) - 0.0).abs() < 0.001);
        assert!((slot_x(0, 60, w, s) + 10.0).abs() < 0.001);
    }

    #[test]
    fn a_value_stands_at_its_share_of_the_height() {
        assert!((value_y(0.0, 100.0, 102.0) - 101.0).abs() < 0.001);
        assert!((value_y(100.0, 100.0, 102.0) - 1.0).abs() < 0.001);
        assert!((value_y(50.0, 100.0, 102.0) - 51.0).abs() < 0.001);
        // Over the top: at the top; no top or a broken value: at the bottom.
        assert!((value_y(250.0, 100.0, 102.0) - 1.0).abs() < 0.001);
        assert!((value_y(5.0, 0.0, 102.0) - 101.0).abs() < 0.001);
        assert!((value_y(f64::NAN, 100.0, 102.0) - 101.0).abs() < 0.001);
    }

    #[test]
    fn a_segment_is_a_box_turned_about_its_centre() {
        let flat = segment((0.0, 10.0), (10.0, 10.0), 2.0);
        assert!((flat.angle).abs() < 0.001);
        assert!((flat.length - 12.0).abs() < 0.001, "the gap plus the thickness");
        assert!((flat.left + 1.0).abs() < 0.001);
        assert!((flat.top - 9.0).abs() < 0.001);
        // Down and to the right is clockwise (the screen's y grows down).
        let down = segment((0.0, 0.0), (10.0, 10.0), 2.0);
        assert!((down.angle - 45.0).abs() < 0.001);
        let up = segment((0.0, 10.0), (10.0, 0.0), 2.0);
        assert!((up.angle + 45.0).abs() < 0.001);
    }

    #[test]
    fn a_grid_line_travels_with_its_reading_and_every_drawing_has_as_many() {
        let (w, s, every) = (580.0, 10.0, 3);
        let a = grid_xs(30, 60, w, s, every);
        let b = grid_xs(31, 60, w, s, every);
        assert_eq!(a.len(), 60 / every + 1);
        assert_eq!(a.len(), b.len());
        // Reading 30 is on a line and newest: at the right edge. One reading
        // later it stands a step further left.
        assert!((a[0] - 580.0).abs() < 0.001);
        assert!((b[0] - 570.0).abs() < 0.001);
        assert!((a[1] - 550.0).abs() < 0.001, "every third reading");
        assert_eq!(grid_every(10.0), 1);
        assert_eq!(grid_every(3.0), 4);
    }

    #[test]
    fn the_strip_slides_by_the_share_of_the_interval_and_never_past_a_step() {
        assert!((offset(0.0, 1000.0, 10.0)).abs() < 0.001);
        assert!((offset(500.0, 1000.0, 10.0) - 5.0).abs() < 0.001);
        assert!((offset(1500.0, 1000.0, 10.0) - 10.0).abs() < 0.001, "a late reading");
        assert!((offset(500.0, 0.0, 10.0)).abs() < 0.001, "paused");
        assert!((offset(-5.0, 1000.0, 10.0)).abs() < 0.001);
    }

    #[test]
    fn a_rate_scale_moves_in_calm_steps() {
        assert_eq!(nice_top(1.0, 0.0), 1.0);
        assert_eq!(nice_top(1.5, 0.0), 2.0);
        assert_eq!(nice_top(3000.0, 0.0), 5000.0);
        assert_eq!(nice_top(5000.0, 0.0), 5000.0);
        assert_eq!(nice_top(12_000.0, 0.0), 20_000.0);
        assert_eq!(nice_top(10.0, 1024.0), 2000.0, "never under the floor");
        assert_eq!(nice_top(f64::NAN, 0.0), 1.0);
        // Up at once over the peak...
        assert_eq!(next_top(10_000.0, 30_000.0, 1024.0), 50_000.0);
        // ...but a smaller peak keeps the scale until it falls under a third.
        assert_eq!(next_top(50_000.0, 20_000.0, 1024.0), 50_000.0);
        assert_eq!(next_top(50_000.0, 10_000.0, 1024.0), 20_000.0);
        assert_eq!(next_top(0.0, 0.0, 1024.0), 2000.0, "the first reading");
    }

    // ---- a new reading scrolls the graph on, it does not jolt it ----

    /// The scroll after readings 1..=`n`, exactly a second apart (reading `k`
    /// at `k` s).
    fn steady(n: u32) -> Scroll {
        let mut scroll = None;
        for k in 1..=n {
            scroll = Some(Scroll::after_reading(
                scroll,
                f64::from(k) * 1000.0,
                f64::from(k),
                1000.0,
            ));
        }
        scroll.expect("one reading at least")
    }

    #[test]
    #[allow(clippy::cast_possible_truncation)] // a few steps
    fn a_reading_moves_no_point_of_the_graph() {
        let w = 560.0_f32;
        let st = step(w, SLOTS);
        // The 11th reading arrives early, 960 ms after the 10th.
        let tenth = steady(10);
        let at = 10_960.0;
        let eleventh = Scroll::after_reading(Some(tenth), at, 11.0, 1000.0);
        // Reading 10: the newest slot of the 10th drawing, one slot left in
        // the 11th - each drawing at its strip's shift.
        let before = slot_x(SLOTS - 1, SLOTS, w, st) + tenth.lag(at, 10.0) as f32 * st;
        let after = slot_x(SLOTS - 2, SLOTS, w, st) + eleventh.lag(at, 11.0) as f32 * st;
        assert!(
            (before - after).abs() < 0.01,
            "reading 10 jumped from {before} px to {after} px when the 11th arrived"
        );
    }

    #[test]
    fn the_newest_reading_comes_in_from_the_right_edge_at_a_steady_pace() {
        let tenth = steady(10);
        let arrived = 10_000.0;
        // When it arrives, the newest reading - and the line to it - stands
        // right of the box: nothing pops up at the edge.
        let first = tenth.lag(arrived, 10.0);
        assert!(first >= 1.0, "the newest reading stands {first} readings right of the edge");
        // It comes in at a steady pace: equal times, equal ways.
        let (a, b) = (tenth.lag(arrived + 400.0, 10.0), tenth.lag(arrived + 800.0, 10.0));
        assert!(first > a && a > b);
        assert!(((first - a) - (a - b)).abs() < 1e-9);
    }

    #[test]
    fn a_late_reading_stops_the_scroll_with_the_newest_reading_at_the_edge() {
        let tenth = steady(10);
        // The 11th reading is late: the scroll waits with the 10th at the
        // box's right edge - it never runs past it into an empty strip.
        assert!(tenth.lag(12_500.0, 10.0).abs() < 1e-9);
        assert!(tenth.lag(60_000.0, 10.0).abs() < 1e-9);
        // When the 11th arrives the scroll goes on from there.
        let eleventh = Scroll::after_reading(Some(tenth), 12_500.0, 11.0, 1000.0);
        assert!((eleventh.position - 10.0).abs() < 1e-9);
        assert!(eleventh.rate > 0.0);
    }

    #[test]
    fn an_early_reading_speeds_the_scroll_up_instead_of_jumping() {
        let tenth = steady(10);
        // 200 ms early: the scroll goes on from where it stands, a little
        // faster, and stands where it should when the next one is due.
        let eleventh = Scroll::after_reading(Some(tenth), 10_800.0, 11.0, 1000.0);
        assert!((eleventh.position - tenth.position_at(10_800.0, 10.0)).abs() < 1e-9);
        assert!(eleventh.rate > 1.0 / 1000.0);
        assert!((eleventh.position_at(11_800.0, 11.0) - (12.0 - LAG)).abs() < 1e-6);
    }

    #[test]
    #[allow(clippy::cast_possible_truncation)] // a few steps
    fn the_strip_covers_the_box_wherever_the_scroll_stands() {
        let w = 560.0_f32;
        let st = step(w, SLOTS);
        for lag in [0.0, LAG, LAG + CATCH_UP] {
            let shift = lag as f32 * st;
            let oldest = slot_x(0, SLOTS, w, st) + shift;
            let newest = slot_x(SLOTS - 1, SLOTS, w, st) + shift;
            assert!(oldest <= 0.0, "lag {lag}: the oldest reading drawn stands at {oldest} px");
            assert!(newest >= w - 0.001, "lag {lag}: the newest stands at {newest} px");
        }
    }

    #[test]
    fn a_meter_lights_its_share_of_the_bars() {
        assert_eq!(lit_bars(0.0, 20), 0);
        assert_eq!(lit_bars(0.5, 20), 10);
        assert_eq!(lit_bars(1.0, 20), 20);
        assert_eq!(lit_bars(3.0, 20), 20);
        assert_eq!(lit_bars(f64::NAN, 20), 0);
    }

    #[test]
    fn every_strip_has_its_marker_and_its_step() {
        let mut strips = Strips::default();
        let history = [10.0, 20.0, 30.0];
        let _a = graph(
            &mut strips,
            &[Line {
                values: &history,
                color: LINE,
            }],
            100.0,
            3,
            580.0,
            100.0,
        );
        let _b = graph(&mut strips, &[], 100.0, 3, 290.0, 100.0);
        assert_eq!(strips.steps.len(), 2);
        assert!((strips.steps[0] - 10.0).abs() < 0.001);
        assert!((strips.steps[1] - 5.0).abs() < 0.001);
        assert_eq!(strip_marker(1).as_str(), "__azmonitor_strip-1");
    }
}
