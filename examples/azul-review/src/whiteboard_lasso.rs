//! The lasso: a pen stroke that goes round some code, and the lines of the page it goes round.
//! Pure geometry on the samples of a stroke - no window, no azul.
//!
//! A stroke is a lasso when it goes once round something: it closes (its end meets its start, or it
//! crosses its own beginning), turns about a full turn, encloses an area that is a good part of its
//! box, and is not a scribble. A quick slash across a word, an underline and a bar in the margin are
//! straight, so they are never a lasso: the two gestures are told apart by what the strokes ARE, not
//! by a guess at what was meant (the fixtures of `stroke_fixtures` hold both, and both suites run on
//! them).
//!
//! What the loop covers of the page ([`covered_lines`]): a line is covered when half of its text is
//! inside the loop; when no line is that much inside, the lines that hold the most characters wholly
//! inside it (a loop round three words is about their line, though it reaches over the lines around);
//! blank lines at the ends are no part of what it shows.

use crate::whiteboard_geom::{
    path_length, point_in_polygon, polygon_area, polygon_rect_overlap, segment_crossing, simplify, Pt, Rect,
};

/// What makes a stroke a lasso: every number is a limit of a hand, not of a screen.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LassoConfig {
    /// A stroke has at least this many samples (a touch has two).
    pub min_points: usize,
    /// The input is looked at as at most this many samples (an evenly thinned stroke).
    pub max_input: usize,
    /// The shortest and the longest line a lasso is drawn with, px.
    pub min_len: f32,
    pub max_len: f32,
    /// The smallest box a loop has, px (a line of code is 15 tall).
    pub min_w: f32,
    pub min_h: f32,
    /// The smallest area it encloses, px squared.
    pub min_area: f32,
    /// The stroke is straight when its ends are this much of its length apart or more.
    pub straight: f32,
    /// An end that stops short of the start by this much of the stroke's length or less still closes it.
    pub max_gap: f32,
    /// The loop is at least this much of the stroke (a tail before and after is fine, another loop is not).
    pub loop_share: f32,
    /// The area is at least this much of its box (a ring is 0.78, a thin zigzag 0.2).
    pub min_fill: f32,
    /// The loop turns at least this many degrees (a circle is 360).
    pub min_turn_deg: f32,
    /// A corner sharper than this is a reversal; more than `max_corners` of them is a scribble.
    pub corner_deg: f32,
    pub max_corners: usize,
    /// The tolerance of the outline kept, and of the corners looked for, px.
    pub fine_eps: f32,
    pub coarse_eps: f32,
    /// The points of the outline kept, and of the outline looked at for crossings.
    pub max_points: usize,
    pub max_checked: usize,
}

/// What the app uses unless it is told other numbers.
pub static LASSO: LassoConfig = LassoConfig {
    min_points: 6,
    max_input: 20_000,
    min_len: 60.0,
    max_len: 8_000.0,
    min_w: 30.0,
    min_h: 14.0,
    min_area: 350.0,
    straight: 0.85,
    max_gap: 0.15,
    loop_share: 0.6,
    min_fill: 0.45,
    min_turn_deg: 300.0,
    corner_deg: 120.0,
    max_corners: 1,
    fine_eps: 0.8,
    coarse_eps: 3.0,
    max_points: 128,
    max_checked: 400,
};

/// Why a stroke is no lasso.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotALasso {
    TooFew,
    TooShort,
    TooLong,
    TooSmall,
    /// A slash, an underline, a bar.
    Straight,
    /// A back and forth.
    Scribble,
    /// It does not close.
    Open,
    /// It closes on almost no area.
    Thin,
    /// It does not go round.
    NoTurn,
}

/// How the stroke closed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Closure {
    /// The end stopped short of the start by this many px.
    Gap(f32),
    /// The stroke crossed its own beginning.
    Crossing,
}

/// A loop recognised.
#[derive(Debug, Clone, PartialEq)]
pub struct Lasso {
    /// The outline, in the stroke's coordinates (closed by the last point to the first).
    pub polygon: Vec<Pt>,
    pub bounds: Rect,
    /// The area it encloses.
    pub area: f32,
    pub closure: Closure,
}

/// Whether the stroke through `points` is a lasso, with the numbers of [`LASSO`].
///
/// # Errors
/// Why it is not one.
pub fn recognize(points: &[Pt]) -> Result<Lasso, NotALasso> {
    recognize_with(points, &LASSO)
}

/// The sum of the signed turns at the corners of the closed outline `poly`, in degrees: 360 for a
/// loop that goes once round, 0 for a figure of eight.
fn total_turn_deg(poly: &[Pt]) -> f32 {
    let n = poly.len();
    if n < 3 {
        return 0.0;
    }
    let mut total = 0.0_f32;
    for i in 0..n {
        let (a, b, c) = (poly[i], poly[(i + 1) % n], poly[(i + 2) % n]);
        let (u, v) = (b - a, c - b);
        if u.len() > f32::EPSILON && v.len() > f32::EPSILON {
            total += u.cross(v).atan2(u.dot(v));
        }
    }
    total.to_degrees().abs()
}

/// How many corners of the open line `line` turn by more than `deg` degrees.
fn sharp_corners(line: &[Pt], deg: f32) -> usize {
    line.windows(3)
        .filter(|w| {
            let (u, v) = (w[1] - w[0], w[2] - w[1]);
            u.len() > f32::EPSILON && v.len() > f32::EPSILON && u.cross(v).atan2(u.dot(v)).to_degrees().abs() > deg
        })
        .count()
}

/// The stroke as at most `max` samples, evenly thinned, the last one kept.
fn thinned(points: &[Pt], max: usize) -> Vec<Pt> {
    if points.len() <= max {
        return points.to_vec();
    }
    let stride = points.len().div_ceil(max);
    let mut out: Vec<Pt> = points.iter().step_by(stride).copied().collect();
    if let Some(last) = points.last() {
        if out.last() != Some(last) {
            out.push(*last);
        }
    }
    out
}

/// Whether the stroke through `points` is a lasso, by the numbers of `cfg`.
///
/// # Errors
/// Why it is not one.
pub fn recognize_with(points: &[Pt], cfg: &LassoConfig) -> Result<Lasso, NotALasso> {
    let mut pts: Vec<Pt> = points.iter().copied().filter(|p| p.is_finite()).collect();
    pts.dedup();
    if pts.len() < cfg.min_points {
        return Err(NotALasso::TooFew);
    }
    let pts = thinned(&pts, cfg.max_input);
    let len = path_length(&pts);
    if len < cfg.min_len {
        return Err(NotALasso::TooShort);
    }
    if len > cfg.max_len {
        return Err(NotALasso::TooLong);
    }
    let (first, last) = (pts[0], pts[pts.len() - 1]);
    if first.dist(last) / len >= cfg.straight {
        return Err(NotALasso::Straight);
    }
    if sharp_corners(&simplify(&pts, cfg.coarse_eps), cfg.corner_deg) > cfg.max_corners {
        return Err(NotALasso::Scribble);
    }

    // The line without the jitter of the hand, thin enough to look for crossings in.
    let mut eps = cfg.fine_eps;
    let mut fine = simplify(&pts, eps);
    while fine.len() > cfg.max_checked {
        eps *= 1.6;
        fine = simplify(&pts, eps);
    }

    // Candidates, the best first: the loop between the stroke's crossing of itself with the most
    // area, then the whole stroke when its end meets its start.
    let mut candidates: Vec<(Vec<Pt>, Closure)> = Vec::new();
    let mut best: Option<(f32, Vec<Pt>)> = None;
    for i in 0..fine.len().saturating_sub(1) {
        for j in (i + 2)..fine.len().saturating_sub(1) {
            let Some(x) = segment_crossing(fine[i], fine[i + 1], fine[j], fine[j + 1]) else {
                continue;
            };
            let mut ring = vec![x];
            ring.extend_from_slice(&fine[i + 1..=j]);
            if path_length(&ring) + ring[ring.len() - 1].dist(x) < cfg.loop_share * len {
                continue;
            }
            let area = polygon_area(&ring).abs();
            if best.as_ref().is_none_or(|(a, _)| area > *a) {
                best = Some((area, ring));
            }
        }
    }
    if let Some((_, ring)) = best {
        candidates.push((ring, Closure::Crossing));
    }
    let gap = first.dist(last);
    if gap <= cfg.max_gap * len {
        candidates.push((fine.clone(), Closure::Gap(gap)));
    }
    if candidates.is_empty() {
        return Err(NotALasso::Open);
    }

    let mut why = NotALasso::Open;
    for (ring, closure) in candidates {
        match judged(ring, closure, cfg) {
            Ok(lasso) => return Ok(lasso),
            Err(e) => why = e,
        }
    }
    Err(why)
}

/// The candidate outline `ring`, as a lasso when it goes round a worthwhile area.
fn judged(ring: Vec<Pt>, closure: Closure, cfg: &LassoConfig) -> Result<Lasso, NotALasso> {
    let Some(bounds) = Rect::from_points(&ring) else {
        return Err(NotALasso::TooFew);
    };
    let area = polygon_area(&ring).abs();
    if bounds.w < cfg.min_w || bounds.h < cfg.min_h || area < cfg.min_area {
        return Err(NotALasso::TooSmall);
    }
    if area / (bounds.w * bounds.h) < cfg.min_fill {
        return Err(NotALasso::Thin);
    }
    if total_turn_deg(&ring) < cfg.min_turn_deg {
        return Err(NotALasso::NoTurn);
    }
    let polygon = if ring.len() > cfg.max_points {
        let step = ring.len() as f32 / cfg.max_points as f32;
        (0..cfg.max_points).map(|i| ring[((i as f32 * step) as usize).min(ring.len() - 1)]).collect()
    } else {
        ring
    };
    Ok(Lasso { polygon, bounds, area, closure })
}

/// A page's lines, where they are and how long they are: what [`covered_lines`] measures a loop on.
#[derive(Debug, Clone, Copy)]
pub struct PageLines<'a> {
    /// The number (from 1) of the page's first line in its file.
    pub first_number: u32,
    /// The characters of each line of the page.
    pub chars: &'a [usize],
    /// The y of the top of the first line, in the page's px.
    pub top: f32,
    pub line_h: f32,
    /// The x of the first character, and how wide each one is (monospace).
    pub text_x: f32,
    pub char_w: f32,
    /// The widest the text is shown: what is further right is clipped.
    pub max_w: f32,
}

/// A line is covered when this much of its text is inside the loop.
pub const COVER_MIN: f32 = 0.5;
/// With no line that much inside, the lines that have characters wholly inside it are the answer: those with at
/// least this much of the most any line has.
pub const CORE_SHARE: f32 = 0.5;
/// With no character wholly inside either, the line with most inside is the answer when it has at least this much.
pub const BEST_MIN: f32 = 0.08;
/// A blank line has the width of this many characters (its place is still a place).
pub const BLANK_CHARS: usize = 4;

/// The lines a loop covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineRange {
    /// The first and the last line, counted from 1 in the file.
    pub first: u32,
    pub last: u32,
    /// The line with most of it inside.
    pub best: u32,
}

/// The characters of line `i` whose boxes lie wholly inside `polygon`.
fn wholly_inside(polygon: &[Pt], page: &PageLines<'_>, i: usize) -> usize {
    let shown = ((page.max_w / page.char_w).floor() as usize).min(page.chars[i]);
    let y0 = page.top + i as f32 * page.line_h + 0.5;
    let y1 = y0 + page.line_h - 1.0;
    (0..shown)
        .filter(|&k| {
            let x0 = page.text_x + k as f32 * page.char_w + 0.5;
            let x1 = x0 + page.char_w - 1.0;
            [(x0, y0), (x1, y0), (x0, y1), (x1, y1)].iter().all(|&(x, y)| point_in_polygon(polygon, Pt::new(x, y)))
        })
        .count()
}

/// The lines of `page` the loop `polygon` covers, `None` when it covers none (it is in the margin,
/// or under the last line): the lines with half of their text inside it; else the lines holding the
/// most characters that are wholly inside it (a loop round a few words is about their line, even when
/// it reaches over the lines around); else the line with most inside. Blank lines at the ends are
/// left out.
#[must_use]
pub fn covered_lines(polygon: &[Pt], page: &PageLines<'_>) -> Option<LineRange> {
    if polygon.len() < 3 || page.char_w <= 0.0 || page.line_h <= 0.0 {
        return None;
    }
    let fractions: Vec<f32> = page
        .chars
        .iter()
        .enumerate()
        .map(|(i, &n)| {
            let w = (n.max(BLANK_CHARS) as f32 * page.char_w).min(page.max_w);
            let r = Rect::new(page.text_x, page.top + i as f32 * page.line_h, w, page.line_h);
            polygon_rect_overlap(polygon, &r) / r.area()
        })
        .collect();
    let best = (0..fractions.len()).max_by(|&a, &b| fractions[a].total_cmp(&fractions[b]))?;
    let mut covered: Vec<usize> = (0..fractions.len()).filter(|&i| fractions[i] >= COVER_MIN).collect();
    if covered.is_empty() {
        let inside: Vec<usize> = (0..fractions.len())
            .map(|i| if fractions[i] > 0.0 { wholly_inside(polygon, page, i) } else { 0 })
            .collect();
        let most = inside.iter().copied().max().unwrap_or(0);
        if most > 0 {
            covered = (0..inside.len()).filter(|&i| inside[i] as f32 >= CORE_SHARE * most as f32 && inside[i] > 0).collect();
        } else if fractions[best] >= BEST_MIN {
            covered.push(best);
        } else {
            return None;
        }
    }
    // The ends of what is shown are lines of code, not the blank lines round a block.
    let text = |i: &usize| page.chars[*i] > 0;
    let first = covered.iter().copied().find(text).unwrap_or(covered[0]);
    let last = covered.iter().rev().copied().find(text).unwrap_or(covered[covered.len() - 1]);
    let number = |i: usize| page.first_number + u32::try_from(i).unwrap_or(0);
    Some(LineRange { first: number(first), last: number(last), best: number(best.clamp(first, last)) })
}

#[cfg(test)]
#[path = "whiteboard_lasso_test.rs"]
mod whiteboard_lasso_test;
