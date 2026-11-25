//! A pen stroke as the agent gets it: the samples (x, y, pressure, time, tilt) the pen gave, made
//! into an SVG shape - a centreline path in page coordinates, simplified to a few dozen points,
//! with the width the pen made at each kept point (from its pressure) and the time each was drawn
//! (ms since the stroke began). The shape is what the agent reads; nothing here draws.
//!
//! The input is whatever the window recorded, so it is cleaned first (a NaN, an infinity, a
//! coordinate of 1e30, a pressure outside 0 to 1, a time that runs backwards) and capped (a stroke
//! of a million samples is thinned): every cut leaves a note.

use crate::agent_limits::{Limits, Notes};

/// The five meanings a stroke can have (the pens 1 to 5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Meaning {
    Scope,
    Issue,
    Question,
    Duplicate,
    Praise,
}

impl Meaning {
    pub const ALL: [Self; 5] = [
        Self::Scope,
        Self::Issue,
        Self::Question,
        Self::Duplicate,
        Self::Praise,
    ];

    /// The word in a session file, in a prompt and in a reply.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Scope => "scope",
            Self::Issue => "issue",
            Self::Question => "question",
            Self::Duplicate => "duplicate",
            Self::Praise => "praise",
        }
    }

    #[must_use]
    pub fn from_key(key: &str) -> Option<Self> {
        let key = key.trim().to_ascii_lowercase();
        Self::ALL.into_iter().find(|m| m.key() == key)
    }

    /// What the meaning says, for a prompt.
    #[must_use]
    pub const fn gloss(self) -> &'static str {
        match self {
            Self::Scope => "marks the passage the reviewer is talking about",
            Self::Issue => "something here is wrong or should change",
            Self::Question => "the reviewer does not understand this or doubts it",
            Self::Duplicate => "this repeats code that exists elsewhere",
            Self::Praise => "this is good: say so, change nothing",
        }
    }
}

/// One sample of the pen.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sample {
    /// Page coordinates, px.
    pub x: f32,
    pub y: f32,
    /// 0 to 1.
    pub pressure: f32,
    /// Milliseconds since the stroke's first sample.
    pub dt_ms: u32,
    /// The pen's tilt in degrees, when the pointer has it.
    pub tilt: Option<(f32, f32)>,
}

/// A stroke as the window recorded it.
#[derive(Debug, Clone, PartialEq)]
pub struct StrokeInput {
    pub id: u64,
    pub page: usize,
    /// The meaning of the pen that drew it (none: no pen was chosen).
    pub meaning: Option<Meaning>,
    /// The reviewer chose the meaning on purpose (a key 1 to 5, a click): the agent must not
    /// override it.
    pub pinned: bool,
    /// The nib: `marker`, `pen` or `audio pen`.
    pub nib: String,
    /// The width of the nib at full pressure, px.
    pub nib_width: f32,
    /// When the stroke began, ms on the session's clock.
    pub t0_ms: u64,
    pub samples: Vec<Sample>,
}

/// A kept point of a stroke's path.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShapePoint {
    pub x: f32,
    pub y: f32,
    /// The width the pen made here, px.
    pub w: f32,
    /// Ms since the stroke began.
    pub t_ms: u32,
}

/// A stroke's cleaned, simplified shape.
#[derive(Debug, Clone, PartialEq)]
pub struct Shape {
    pub id: u64,
    pub points: Vec<ShapePoint>,
    /// `[min x, min y, max x, max y]` of the kept points.
    pub bbox: [f32; 4],
    /// Samples the stroke had that could be read.
    pub samples_in: usize,
    pub mean_pressure: f32,
    pub peak_pressure: f32,
    /// Ms from the first sample to the last.
    pub duration_ms: u32,
}

/// The coordinate a stroke may have at most, px (a page is a few thousand).
pub const MAX_COORD: f32 = 1.0e6;

/// The most samples of one stroke that are read at all (a stroke of more is cut off there).
const MAX_READ: usize = 1_000_000;

/// How far a simplified path may leave the drawn one, px, before it is made coarser to fit.
const TOLERANCE_PX: f32 = 0.75;

/// How many times the tolerance is doubled to fit a stroke into the point cap, and how many times
/// the last step is halved back.
const MAX_ROUNDS: usize = 14;
const BISECTIONS: usize = 8;

/// The width at zero pressure as a part of the nib's, and the rest that pressure adds to it.
const WIDTH_BASE: f32 = 0.35;
const WIDTH_SPAN: f32 = 0.65;

/// How far the view box reaches past the strokes, px.
const VIEW_MARGIN: f32 = 8.0;

/// A number as the path writes it: one decimal at most, no `-0`, no `.0`.
#[must_use]
pub fn fmt_num(v: f32) -> String {
    if !v.is_finite() {
        return "0".to_string();
    }
    let r = (v * 10.0).round() / 10.0;
    if r == 0.0 {
        "0".to_string()
    } else if r.fract() == 0.0 {
        // checked: a whole number of at most seven digits
        format!("{}", r as i64)
    } else {
        format!("{r:.1}")
    }
}

/// How far `p` is from the segment `a` to `b` (from the point `a` when they are one).
fn distance_to_segment(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = dx * dx + dy * dy;
    let t = if len2 <= f32::EPSILON {
        0.0
    } else {
        (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / len2).clamp(0.0, 1.0)
    };
    (a.0 + t * dx - p.0).hypot(a.1 + t * dy - p.1)
}

/// The indices of the points of `points` that stay when the path is simplified with the
/// Ramer-Douglas-Peucker rule at `tolerance` px: the first and the last always.
#[must_use]
pub fn simplify_indices(points: &[(f32, f32)], tolerance: f32) -> Vec<usize> {
    let n = points.len();
    if n <= 2 {
        return (0..n).collect();
    }
    let mut keep = vec![false; n];
    keep[0] = true;
    keep[n - 1] = true;
    let mut stack = vec![(0, n - 1)];
    while let Some((a, b)) = stack.pop() {
        if b <= a + 1 {
            continue;
        }
        let mut worst = 0.0_f32;
        let mut at = a;
        for (i, p) in points.iter().enumerate().take(b).skip(a + 1) {
            let d = distance_to_segment(*p, points[a], points[b]);
            if d > worst {
                worst = d;
                at = i;
            }
        }
        if worst > tolerance {
            keep[at] = true;
            stack.push((a, at));
            stack.push((at, b));
        }
    }
    keep.iter()
        .enumerate()
        .filter(|(_, kept)| **kept)
        .map(|(i, _)| i)
        .collect()
}

/// `count` indices spread evenly over `0..n`, the first and the last among them.
fn spread(n: usize, count: usize) -> Vec<usize> {
    if count >= n {
        return (0..n).collect();
    }
    if count <= 1 {
        return vec![0];
    }
    (0..count).map(|k| k * (n - 1) / (count - 1)).collect()
}

/// The shape of `stroke`: cleaned, thinned to `limits.max_samples_per_stroke`, simplified to at most
/// `limits.max_svg_points` points, each with its width and time.
#[must_use]
pub fn shape(stroke: &StrokeInput, limits: &Limits, notes: &mut Notes) -> Shape {
    let id = stroke.id;
    let mut clean: Vec<Sample> = Vec::with_capacity(stroke.samples.len().min(MAX_READ));
    let (mut unreadable, mut clamped, mut t) = (0_usize, false, 0_u32);
    for s in stroke.samples.iter().take(MAX_READ) {
        if !s.x.is_finite() || !s.y.is_finite() {
            unreadable += 1;
            continue;
        }
        let (x, y) = (
            s.x.clamp(-MAX_COORD, MAX_COORD),
            s.y.clamp(-MAX_COORD, MAX_COORD),
        );
        clamped |= x != s.x || y != s.y;
        let pressure = if s.pressure.is_finite() {
            s.pressure.clamp(0.0, 1.0)
        } else {
            0.5
        };
        t = t.max(s.dt_ms);
        clean.push(Sample {
            x,
            y,
            pressure,
            dt_ms: t,
            tilt: s.tilt,
        });
    }
    if stroke.samples.len() > MAX_READ {
        notes.push(format!(
            "stroke {id}: only the first {MAX_READ} of {} samples were read",
            stroke.samples.len()
        ));
    }
    if unreadable > 0 {
        notes.push(format!(
            "stroke {id}: {unreadable} unreadable samples were dropped"
        ));
    }
    if clamped {
        notes.push(format!(
            "stroke {id}: a coordinate beyond \u{b1}{MAX_COORD} px was held to the limit"
        ));
    }
    let empty = Shape {
        id,
        points: Vec::new(),
        bbox: [0.0; 4],
        samples_in: 0,
        mean_pressure: 0.0,
        peak_pressure: 0.0,
        duration_ms: 0,
    };
    let (Some(first), Some(last)) = (clean.first().copied(), clean.last().copied()) else {
        notes.push(format!("stroke {id} has no readable sample"));
        return empty;
    };
    let samples_in = clean.len();
    let mean_pressure = clean.iter().map(|s| s.pressure).sum::<f32>() / samples_in as f32;
    let peak_pressure = clean.iter().map(|s| s.pressure).fold(0.0_f32, f32::max);
    let duration_ms = last.dt_ms.saturating_sub(first.dt_ms);

    // thinned evenly (the last sample stays) when there are more than the cap
    let cap = limits.max_samples_per_stroke.max(2);
    let thin: Vec<Sample> = if samples_in > cap {
        let step = samples_in.div_ceil(cap);
        let mut thin: Vec<Sample> = clean.iter().step_by(step).copied().collect();
        if thin.last().map(|s| s.dt_ms) != Some(last.dt_ms) || thin.len() < 2 {
            thin.push(last);
        }
        notes.push(format!(
            "stroke {id}: {samples_in} samples were thinned to {}",
            thin.len()
        ));
        thin
    } else {
        clean
    };

    // simplified; when that leaves more than the cap, the finest tolerance that fits is searched:
    // doubled until it fits, then halved back between the last two (a coarse step alone can jump from
    // far over the cap to a handful of points)
    let points: Vec<(f32, f32)> = thin.iter().map(|s| (s.x, s.y)).collect();
    let max_points = limits.max_svg_points.max(2);
    let mut low = TOLERANCE_PX;
    let mut kept = simplify_indices(&points, low);
    if kept.len() > max_points {
        let mut high = low;
        let mut fits = Vec::new();
        for _ in 0..MAX_ROUNDS {
            high *= 2.0;
            fits = simplify_indices(&points, high);
            if fits.len() <= max_points {
                break;
            }
            low = high;
        }
        if fits.len() <= max_points {
            for _ in 0..BISECTIONS {
                let middle = (low + high) / 2.0;
                let k = simplify_indices(&points, middle);
                if k.len() <= max_points {
                    high = middle;
                    fits = k;
                } else {
                    low = middle;
                }
            }
            kept = fits;
        } else {
            kept = spread(thin.len(), max_points);
        }
    }
    if kept.len() < thin.len() && thin.len() > max_points {
        notes.push(format!(
            "stroke {id}: its path was simplified to {} points",
            kept.len()
        ));
    }

    let nib = if stroke.nib_width.is_finite() && stroke.nib_width > 0.0 {
        stroke.nib_width
    } else {
        2.0
    };
    let shape_points: Vec<ShapePoint> = kept
        .iter()
        .filter_map(|&i| thin.get(i))
        .map(|s| ShapePoint {
            x: s.x,
            y: s.y,
            w: nib * (WIDTH_BASE + WIDTH_SPAN * s.pressure),
            t_ms: s.dt_ms,
        })
        .collect();
    let mut bbox = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
    for p in &shape_points {
        bbox = [
            bbox[0].min(p.x),
            bbox[1].min(p.y),
            bbox[2].max(p.x),
            bbox[3].max(p.y),
        ];
    }
    Shape {
        id,
        points: shape_points,
        bbox,
        samples_in,
        mean_pressure,
        peak_pressure,
        duration_ms,
    }
}

/// `text` escaped for an XML attribute value.
#[must_use]
pub fn xml_attr(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            other => out.push(other),
        }
    }
    out
}

/// The numbers `f` makes of the points, separated by a space.
fn joined(points: &[ShapePoint], f: impl Fn(&ShapePoint) -> String) -> String {
    points.iter().map(f).collect::<Vec<_>>().join(" ")
}

/// One stroke's `<path>`.
fn path_element(stroke: &StrokeInput, shape: &Shape) -> String {
    let mut d = String::new();
    for (i, p) in shape.points.iter().enumerate() {
        d.push_str(if i == 0 { "M" } else { " L" });
        d.push_str(&format!("{} {}", fmt_num(p.x), fmt_num(p.y)));
    }
    if shape.points.len() == 1 {
        // a dot: a path of no length, drawn round
        let p = &shape.points[0];
        d.push_str(&format!(" L{} {}", fmt_num(p.x), fmt_num(p.y)));
    }
    format!(
        "<path id=\"s{id}\" d=\"{d}\" fill=\"none\" stroke-linecap=\"round\" data-nib=\"{nib}\" data-meaning=\"{meaning}\" \
         data-pinned=\"{pinned}\" data-t0-ms=\"{t0}\" data-t=\"{t}\" data-w=\"{w}\" data-pressure-mean=\"{pm}\" \
         data-pressure-peak=\"{pp}\"/>",
        id = stroke.id,
        nib = xml_attr(&stroke.nib),
        meaning = stroke.meaning.map_or("unknown", Meaning::key),
        pinned = stroke.pinned,
        t0 = stroke.t0_ms,
        t = joined(&shape.points, |p| p.t_ms.to_string()),
        w = joined(&shape.points, |p| fmt_num(p.w)),
        pm = fmt_num(shape.mean_pressure),
        pp = fmt_num(shape.peak_pressure),
    )
}

/// The `<svg>` of the strokes: one `<path>` each, in page coordinates, with the data the agent
/// reads (`data-t`, `data-w`, the pressure, the meaning, whether it was pinned); empty when no
/// stroke has a point.
#[must_use]
pub fn svg_document(items: &[(&StrokeInput, Shape)]) -> String {
    let drawn: Vec<&(&StrokeInput, Shape)> = items
        .iter()
        .filter(|(_, shape)| !shape.points.is_empty())
        .collect();
    if drawn.is_empty() {
        return String::new();
    }
    let mut bbox = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
    for (_, shape) in &drawn {
        bbox = [
            bbox[0].min(shape.bbox[0]),
            bbox[1].min(shape.bbox[1]),
            bbox[2].max(shape.bbox[2]),
            bbox[3].max(shape.bbox[3]),
        ];
    }
    let (x, y) = (bbox[0] - VIEW_MARGIN, bbox[1] - VIEW_MARGIN);
    let (w, h) = (
        bbox[2] - bbox[0] + 2.0 * VIEW_MARGIN,
        bbox[3] - bbox[1] + 2.0 * VIEW_MARGIN,
    );
    let mut out = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"{} {} {} {}\">\n",
        fmt_num(x),
        fmt_num(y),
        fmt_num(w),
        fmt_num(h)
    );
    for (stroke, shape) in drawn {
        out.push_str(&path_element(stroke, shape));
        out.push('\n');
    }
    out.push_str("</svg>");
    out
}

#[cfg(test)]
#[path = "agent_stroke_test.rs"]
mod agent_stroke_test;
