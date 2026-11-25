//! What a pen stroke LOOKS like, computed on this machine: closed or open, how many loops, how
//! straight, how long and how fast, whether it is an underline, a strike-through, a circle, a
//! scribble, a bracket, an arrow or a dot, which lines and words it covers or touches, whether it
//! sits in the margin. These are hints, not facts (a wavy underline is a scribble to a rule): they go
//! to the agent next to the SVG of the stroke, so it can infer what the reviewer meant without the
//! reviewer switching pens, and they are the offline default - a small rule table from a hint to a
//! meaning with a low confidence ([`RULES`]) - for a run without a model (the end-to-end tests, a
//! machine with no agent set up).
//!
//! The reviewer's own choice is never overridden: a stroke whose meaning the reviewer pinned is
//! left out of [`offline_intent`], and the agent is told so in its request.
//!
//! Everything is plain geometry on the cleaned, simplified path of [`crate::agent_stroke::Shape`]
//! (at most 96 points: the quadratic parts are cheap) and on the code of the page
//! ([`crate::agent_review::Context`]: a page of code is a grid of lines and columns).

use crate::{
    agent_limits::{Limits, Notes},
    agent_review::{Context, Intent, PageGrid},
    agent_stroke::{shape, Meaning, Shape, StrokeInput},
};

/// What a stroke may look like.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hint {
    /// A tap: no length to speak of.
    Dot,
    /// A straight, level line just under a line of text.
    Underline,
    /// A straight, level line through the middle of a line of text.
    StrikeThrough,
    /// A closed shape of about the same height and width: a lasso.
    Circle,
    /// To and fro over a small place.
    Scribble,
    /// A tall shape with two corners and its ends on one side: `[` or `{`.
    Bracket,
    /// A tall, straight line.
    VerticalBar,
    /// A line with a head.
    Arrow,
    /// It lies beside the text, not on it.
    Margin,
}

impl Hint {
    /// The word a prompt says.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Dot => "dot",
            Self::Underline => "underline",
            Self::StrikeThrough => "strike-through",
            Self::Circle => "circle",
            Self::Scribble => "scribble",
            Self::Bracket => "bracket",
            Self::VerticalBar => "vertical bar",
            Self::Arrow => "arrow",
            Self::Margin => "in the margin",
        }
    }
}

/// A word of the code a stroke is on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WordHit {
    pub line: usize,
    pub col: usize,
    pub text: String,
}

/// What a stroke looks like.
#[derive(Debug, Clone, PartialEq)]
pub struct Features {
    pub stroke: u64,
    pub closed: bool,
    /// Whole turns the path makes (a circle: 1).
    pub loops: u32,
    /// 0 to 1: how near the path is to a straight line between its ends.
    pub straightness: f32,
    pub length_px: f32,
    pub duration_ms: u32,
    /// Width over height of its box.
    pub aspect: f32,
    pub bbox: [f32; 4],
    pub mean_pressure: f32,
    pub peak_pressure: f32,
    /// How often it turns back on itself along its long side.
    pub reversals: u32,
    pub hints: Vec<Hint>,
    /// The lines of the page its box lies on (none: off the lines that were sent).
    pub lines: Option<(usize, usize)>,
    /// Words the stroke covers (under or through them, or round them).
    pub covered: Vec<WordHit>,
    /// Words it passes through without covering them.
    pub touched: Vec<WordHit>,
    pub in_margin: bool,
    /// It lies on the text.
    pub crosses_text: bool,
}

impl Features {
    #[must_use]
    pub fn has(&self, hint: Hint) -> bool {
        self.hints.contains(&hint)
    }
}

// ==== Geometry ====

/// The least a stroke must be to count as a dot or a shape, px.
const DOT_PX: f32 = 6.0;
const SHAPE_PX: f32 = 20.0;

fn dist(a: (f32, f32), b: (f32, f32)) -> f32 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

/// The turning angle at `b` of the path `a`, `b`, `c`, signed, radians in -pi to pi.
fn turn(a: (f32, f32), b: (f32, f32), c: (f32, f32)) -> f32 {
    let (ux, uy) = (b.0 - a.0, b.1 - a.1);
    let (vx, vy) = (c.0 - b.0, c.1 - b.1);
    (ux * vy - uy * vx).atan2(ux * vx + uy * vy)
}

/// Whether `p` is inside the polygon `poly` (even-odd).
fn inside(p: (f32, f32), poly: &[(f32, f32)]) -> bool {
    let mut odd = false;
    let mut j = poly.len().saturating_sub(1);
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[j]);
        if (a.1 > p.1) != (b.1 > p.1) && p.0 < (b.0 - a.0) * (p.1 - a.1) / (b.1 - a.1) + a.0 {
            odd = !odd;
        }
        j = i;
    }
    odd
}

/// Whether the path touches the box `[left, top, right, bottom]`: an exact test of each of its
/// segments (Liang-Barsky), so a long segment costs no more than a short one.
fn path_in_box(points: &[(f32, f32)], bx: [f32; 4]) -> bool {
    let touches = |a: (f32, f32), b: (f32, f32)| -> bool {
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let (mut near, mut far) = (0.0_f32, 1.0_f32);
        for (p, q) in [
            (-dx, a.0 - bx[0]),
            (dx, bx[2] - a.0),
            (-dy, a.1 - bx[1]),
            (dy, bx[3] - a.1),
        ] {
            if p == 0.0 {
                if q < 0.0 {
                    return false;
                }
            } else {
                let at = q / p;
                if p < 0.0 {
                    if at > far {
                        return false;
                    }
                    near = near.max(at);
                } else {
                    if at < near {
                        return false;
                    }
                    far = far.min(at);
                }
            }
        }
        true
    };
    match points {
        [] => false,
        [only] => touches(*only, *only),
        _ => points.windows(2).any(|p| touches(p[0], p[1])),
    }
}

/// A word of a line of code, with its byte column.
fn words_of(line: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    for (i, c) in line.char_indices() {
        let word = c.is_alphanumeric() || c == '_';
        match (word, start) {
            (true, None) => start = Some(i),
            (false, Some(s)) => {
                out.push((s, &line[s..i]));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        out.push((s, &line[s..]));
    }
    out
}

/// Where a line of the page is: the top of its box, px.
fn line_top(grid: &PageGrid, line: usize) -> f32 {
    grid.origin_y + (line as f32 - grid.first_line as f32) * grid.line_height
}

/// The most lines and columns of a page that are read (the request builder sends far fewer: this
/// is the guard of a caller that does not).
const MAX_LINES: usize = 2_000;
const MAX_COLS: usize = 4_000;

/// The first `MAX_COLS` characters of a line.
fn visible(line: &str) -> &str {
    match line.char_indices().nth(MAX_COLS) {
        Some((end, _)) => &line[..end],
        None => line,
    }
}

/// How many characters a line occupies (columns, by characters).
fn width_chars(line: &str) -> usize {
    visible(line).chars().count()
}

// ==== The analysis ====

/// What `stroke` (whose cleaned shape is `shape`) looks like on the page of `ctx`.
#[must_use]
pub fn analyze(stroke: &StrokeInput, shape: &Shape, ctx: &Context) -> Features {
    let pts: Vec<(f32, f32)> = shape.points.iter().map(|p| (p.x, p.y)).collect();
    let [x0, y0, x1, y1] = if pts.is_empty() { [0.0; 4] } else { shape.bbox };
    let (w, h) = (x1 - x0, y1 - y0);
    let diag = w.hypot(h);
    let length: f32 = pts.windows(2).map(|p| dist(p[0], p[1])).sum();
    let chord = match (pts.first(), pts.last()) {
        (Some(a), Some(b)) => dist(*a, *b),
        _ => 0.0,
    };
    let straightness = if length <= f32::EPSILON {
        1.0
    } else {
        (chord / length).clamp(0.0, 1.0)
    };
    let aspect = (w / h.max(0.001)).clamp(0.001, 999.0);

    let turns: Vec<f32> = pts.windows(3).map(|t| turn(t[0], t[1], t[2])).collect();
    let signed: f32 = turns.iter().sum();
    let loops = ((signed.abs() / std::f32::consts::TAU) + 0.25).floor() as u32;
    let closed = diag >= DOT_PX * 2.0 && chord <= 0.35 * diag && length >= 2.0 * diag;

    // how often it turns back along its long side
    let reversals = {
        let along = |f: fn((f32, f32)) -> f32| {
            let mut last = 0.0_f32;
            let mut count = 0;
            for pair in pts.windows(2) {
                let d = f(pair[1]) - f(pair[0]);
                if d.abs() < 2.0 {
                    continue;
                }
                if last != 0.0 && (d > 0.0) != (last > 0.0) {
                    count += 1;
                }
                last = d;
            }
            count
        };
        if w >= h {
            along(|p| p.0)
        } else {
            along(|p| p.1)
        }
    };

    let angle = pts
        .first()
        .zip(pts.last())
        .map_or(0.0, |(a, b)| (b.1 - a.1).atan2(b.0 - a.0).to_degrees());
    let level = angle.abs() <= 12.0 || angle.abs() >= 168.0;

    let grid = ctx.grid;
    let mut hints = Vec::new();
    let mut covered: Vec<WordHit> = Vec::new();
    let mut touched: Vec<WordHit> = Vec::new();
    let mut lines = None;
    let mut in_margin = false;
    let mut crosses_text = false;

    if pts.is_empty() {
        return Features {
            stroke: stroke.id,
            closed: false,
            loops: 0,
            straightness: 1.0,
            length_px: 0.0,
            duration_ms: shape.duration_ms,
            aspect: 1.0,
            bbox: [0.0; 4],
            mean_pressure: shape.mean_pressure,
            peak_pressure: shape.peak_pressure,
            reversals: 0,
            hints,
            lines,
            covered,
            touched,
            in_margin,
            crosses_text,
        };
    }

    if diag < DOT_PX && length < DOT_PX * 2.0 {
        hints.push(Hint::Dot);
    }
    if closed && loops >= 1 && (0.3..=3.3).contains(&aspect) && diag >= SHAPE_PX {
        hints.push(Hint::Circle);
    }
    if !closed && reversals >= 4 && length >= 3.0 * w.max(h) && diag >= DOT_PX {
        hints.push(Hint::Scribble);
    }
    let sharp = turns
        .iter()
        .filter(|t| (0.96..=2.2).contains(&t.abs()))
        .count();
    if aspect <= 0.6 && h >= SHAPE_PX && !closed && sharp >= 2 {
        let mid_x = (x0 + x1) / 2.0;
        let ends = [pts[0].0, pts[pts.len() - 1].0];
        if ends.iter().all(|e| *e > mid_x + 3.0) || ends.iter().all(|e| *e < mid_x - 3.0) {
            hints.push(Hint::Bracket);
        }
    }
    if aspect <= 0.25 && straightness >= 0.9 && h >= SHAPE_PX && !hints.contains(&Hint::Bracket) {
        hints.push(Hint::VerticalBar);
    }
    // a head: a straight shaft, then at least two sharp turns in a short tail
    if pts.len() >= 5 && !closed {
        let tail_turns = turns
            .iter()
            .rev()
            .take(3)
            .filter(|t| t.abs() >= 1.75)
            .count();
        let tail_len: f32 = pts.windows(2).rev().take(3).map(|p| dist(p[0], p[1])).sum();
        // the shaft ends where the head begins: three segments before the end
        let shaft = dist(pts[0], pts[pts.len() - 4]);
        if tail_turns >= 2 && tail_len <= 0.45 * length && shaft >= 20.0 {
            hints.push(Hint::Arrow);
        }
    }

    // where it is on the page
    if let Some(grid) = grid {
        let lh = grid.line_height.max(1.0);
        let cw = grid.char_width.max(1.0);
        let usable = ctx.lines.len().min(MAX_LINES);
        let last_line = ctx.first_line + usable.saturating_sub(1);
        let line_at =
            |y: f32| -> i64 { grid.first_line as i64 + ((y - grid.origin_y) / lh).floor() as i64 };
        let (first, last) = (line_at(y0), line_at(y1));
        let on_page = |l: i64| l >= ctx.first_line as i64 && l <= last_line as i64;
        if usable > 0 && (on_page(first) || on_page(last)) {
            lines = Some((
                first.max(ctx.first_line as i64) as usize,
                last.min(last_line as i64) as usize,
            ));
        }
        let text_right = grid.origin_x
            + ctx
                .lines
                .iter()
                .take(usable)
                .map(|l| width_chars(l))
                .max()
                .unwrap_or(0) as f32
                * cw;
        let centre_x = (x0 + x1) / 2.0;
        in_margin = centre_x < grid.origin_x - 2.0 || centre_x > text_right + 8.0;

        let word_box = |line: usize, col: usize, text: &str| -> [f32; 4] {
            let top = line_top(&grid, line);
            let left = grid.origin_x
                + ctx
                    .lines
                    .get(line - ctx.first_line)
                    .map_or(0, |l| visible(l)[..col].chars().count()) as f32
                    * cw;
            [left, top, left + text.chars().count() as f32 * cw, top + lh]
        };
        let all_words = |line: usize| -> Vec<WordHit> {
            line.checked_sub(ctx.first_line)
                .filter(|i| *i < usable)
                .and_then(|i| ctx.lines.get(i))
                .map(|text| {
                    words_of(visible(text))
                        .into_iter()
                        .map(|(col, w)| WordHit {
                            line,
                            col,
                            text: w.to_string(),
                        })
                        .collect()
                })
                .unwrap_or_default()
        };

        // a level line under or through a line of text
        if straightness >= 0.85
            && level
            && length >= 3.0 * cw
            && h <= lh * 0.4
            && !hints.contains(&Hint::Dot)
        {
            let mean_y = pts.iter().map(|p| p.1).sum::<f32>() / pts.len() as f32;
            let rel = (mean_y - grid.origin_y) / lh;
            let f = rel - rel.floor();
            let idx = grid.first_line as i64 + rel.floor() as i64;
            let (kind, line) = if f >= 0.72 {
                (Some(Hint::Underline), idx)
            } else if f <= 0.12 {
                (Some(Hint::Underline), idx - 1)
            } else if (0.3..=0.62).contains(&f) {
                (Some(Hint::StrikeThrough), idx)
            } else {
                (None, idx)
            };
            if let (Some(kind), true) = (kind, on_page(line)) {
                hints.push(kind);
                if let Ok(line) = usize::try_from(line) {
                    for hit in all_words(line) {
                        let b = word_box(hit.line, hit.col, &hit.text);
                        let overlap = (x1.min(b[2]) - x0.max(b[0])).max(0.0);
                        if overlap >= 0.5 * (b[2] - b[0]) {
                            covered.push(hit);
                        }
                    }
                }
            }
        }
        // a closed shape covers the words whose middle it holds; any path touches the words it crosses
        if let Some((a, b)) = lines {
            for line in a..=b {
                for hit in all_words(line) {
                    let bx = word_box(hit.line, hit.col, &hit.text);
                    // a word beside the stroke's box is neither (and most words are)
                    if bx[2] < x0
                        || bx[0] > x1
                        || bx[3] < y0
                        || bx[1] > y1
                        || covered.contains(&hit)
                    {
                        continue;
                    }
                    let middle = ((bx[0] + bx[2]) / 2.0, (bx[1] + bx[3]) / 2.0);
                    if closed && inside(middle, &pts) {
                        covered.push(hit);
                    } else if path_in_box(&pts, bx) {
                        touched.push(hit);
                    }
                }
            }
        }
        crosses_text = !covered.is_empty() || !touched.is_empty();
        if in_margin {
            hints.push(Hint::Margin);
        }
    }

    Features {
        stroke: stroke.id,
        closed,
        loops,
        straightness,
        length_px: length,
        duration_ms: shape.duration_ms,
        aspect,
        bbox: [x0, y0, x1, y1],
        mean_pressure: shape.mean_pressure,
        peak_pressure: shape.peak_pressure,
        reversals,
        hints,
        lines,
        covered,
        touched,
        in_margin,
        crosses_text,
    }
}

// ==== In words ====

fn quoted(words: &[WordHit]) -> String {
    words
        .iter()
        .take(6)
        .map(|w| format!("\"{}\"", w.text))
        .collect::<Vec<_>>()
        .join(", ")
}

/// One line of a prompt about `f`.
#[must_use]
pub fn describe(f: &Features) -> String {
    let mut parts = vec![
        if f.closed {
            "closed".to_string()
        } else {
            "open".to_string()
        },
        if f.straightness >= 0.9 {
            format!("straight ({:.2})", f.straightness)
        } else if f.straightness >= 0.5 {
            format!("curved ({:.2})", f.straightness)
        } else {
            format!("winding ({:.2})", f.straightness)
        },
        format!(
            "{:.0} px long in {:.1} s",
            f.length_px,
            f64::from(f.duration_ms) / 1000.0
        ),
    ];
    if f.loops > 0 {
        parts.push(format!(
            "{} loop{}",
            f.loops,
            if f.loops == 1 { "" } else { "s" }
        ));
    }
    if f.reversals > 0 {
        parts.push(format!("turns back {} times", f.reversals));
    }
    let hints: Vec<&str> = f.hints.iter().map(|h| h.word()).collect();
    parts.push(if hints.is_empty() {
        "no special shape".to_string()
    } else {
        format!("looks like: {}", hints.join(", "))
    });
    if !f.covered.is_empty() {
        parts.push(format!("covers {}", quoted(&f.covered)));
    }
    if !f.touched.is_empty() {
        parts.push(format!("passes through {}", quoted(&f.touched)));
    }
    if let Some((a, b)) = f.lines {
        parts.push(if a == b {
            format!("on line {a}")
        } else {
            format!("on lines {a} to {b}")
        });
    }
    parts.push(format!(
        "pressure mean {:.2}, peak {:.2}",
        f.mean_pressure, f.peak_pressure
    ));
    format!("- stroke {}: {}", f.stroke, parts.join("; "))
}

// ==== The offline default ====

/// A row of the rule table: a stroke that looks like `hint` probably means `meaning`.
#[derive(Debug)]
pub struct Rule {
    pub hint: Hint,
    pub meaning: Meaning,
    /// 0 to 1, and low: it is a guess from a shape.
    pub confidence: f32,
}

/// The offline default: what a shape most often means. A guess, never above a half.
pub static RULES: [Rule; 7] = [
    Rule {
        hint: Hint::Underline,
        meaning: Meaning::Scope,
        confidence: 0.45,
    },
    Rule {
        hint: Hint::Circle,
        meaning: Meaning::Scope,
        confidence: 0.40,
    },
    Rule {
        hint: Hint::Bracket,
        meaning: Meaning::Scope,
        confidence: 0.40,
    },
    Rule {
        hint: Hint::StrikeThrough,
        meaning: Meaning::Issue,
        confidence: 0.35,
    },
    Rule {
        hint: Hint::Scribble,
        meaning: Meaning::Issue,
        confidence: 0.35,
    },
    Rule {
        hint: Hint::Arrow,
        meaning: Meaning::Scope,
        confidence: 0.30,
    },
    Rule {
        hint: Hint::Dot,
        meaning: Meaning::Question,
        confidence: 0.20,
    },
];

/// What the rule table guesses for the strokes the reviewer did not choose a meaning for: the best
/// rule among their hints, or nothing when none fits.
#[must_use]
pub fn offline_intent(features: &[Features], pinned: &[u64]) -> Option<Intent> {
    features
        .iter()
        .filter(|f| !pinned.contains(&f.stroke))
        .flat_map(|f| {
            RULES
                .iter()
                .filter(move |r| f.has(r.hint))
                .map(move |r| (r, f.stroke))
        })
        .max_by(|a, b| a.0.confidence.total_cmp(&b.0.confidence))
        .map(|(rule, stroke)| Intent {
            meaning: Some(rule.meaning),
            confidence: rule.confidence,
            rationale: format!("a rule of shape: stroke {stroke}, {}", rule.hint.word()),
        })
}

/// The offline default for `strokes` on the page of `ctx`, in one call (what the window uses when no
/// agent is set up, and what the mock answers with): the shapes of the newest strokes
/// (`limits.max_strokes`, as a request sends them) through [`offline_intent`], without the strokes
/// whose meaning the reviewer chose on purpose (a pinned stroke that has one).
#[must_use]
pub fn offline_intent_for(
    strokes: &[StrokeInput],
    ctx: &Context,
    limits: &Limits,
) -> Option<Intent> {
    let newest = &strokes[strokes.len().saturating_sub(limits.max_strokes)..];
    let mut notes = Notes::new();
    let features: Vec<Features> = newest
        .iter()
        .map(|s| analyze(s, &shape(s, limits, &mut notes), ctx))
        .collect();
    let chosen: Vec<u64> = newest
        .iter()
        .filter(|s| s.pinned && s.meaning.is_some())
        .map(|s| s.id)
        .collect();
    offline_intent(&features, &chosen)
}

#[cfg(test)]
#[path = "stroke_features_test.rs"]
mod stroke_features_test;
