//! Strokes for the tests of every gesture: a loop around a word, a quick slash through it, an underline,
//! a scribble. One set, shared: the whiteboard's lasso (`whiteboard_lasso`) and the slash that deletes
//! a word are told apart on the same pen movements, so a change to one that breaks the other fails in
//! both suites. Test support only (`#[cfg(test)]` in `lib.rs`); std only, no azul.
//!
//! The scene is one line of code on a page, in the page's own px (the ink layer's), laid out as the
//! pages are (the code from y 32, a line 15 px tall): line 12 spans y 197 to 212, and a word of ten
//! characters on it lies at x 300 to 366. Every fixture is a stroke a person could draw there, with the
//! time of each sample (ms from the pen going down) and a pressure, as the recording of a pen gives
//! them. The numbers are those of a quick hand: a slash takes 110 ms, a circle half a second.
//!
//! A fixture's [`Meaning`] is what the stroke IS as a shape, not what any recogniser says of it:
//! a recogniser's test is "it says what the fixture is".

/// One sample of a pen: where, when (ms after the pen went down) and how hard (0 to 1).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sample {
    pub x: f32,
    pub y: f32,
    pub t_ms: u32,
    pub pressure: f32,
}

/// What a fixture is, as a shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Meaning {
    /// A line that closes on itself (or crosses itself) around something: a loop, a ring.
    ClosedShape,
    /// A quick, short, nearly horizontal stroke across a word: a deletion.
    Slash,
    /// A stroke under a line of text.
    Underline,
    /// A back and forth over the same place.
    Scribble,
    /// An arc that does not close (a "C").
    OpenArc,
    /// A touch of the pen that hardly moves.
    Tap,
    /// A straight stroke that is neither a slash nor an underline (a bar in the margin).
    Line,
    /// Another mark: a tick, a figure of eight.
    Mark,
}

/// A named stroke.
#[derive(Debug, Clone)]
pub struct Fixture {
    pub name: &'static str,
    pub meaning: Meaning,
    pub samples: Vec<Sample>,
}

impl Fixture {
    /// The x, y of every sample.
    #[must_use]
    pub fn xy(&self) -> Vec<(f32, f32)> {
        xy(&self.samples)
    }
}

/// The x, y of every sample.
#[must_use]
pub fn xy(samples: &[Sample]) -> Vec<(f32, f32)> {
    samples.iter().map(|s| (s.x, s.y)).collect()
}

/// The word the loops go round: its centre.
pub const WORD_CENTRE: (f32, f32) = (333.0, 204.5);
/// The word's box: left, top, right, bottom.
pub const WORD_BOX: (f32, f32, f32, f32) = (300.0, 198.0, 366.0, 211.0);
/// The line the word is on: the number of the line (the first of the page is 1) and its top.
pub const WORD_LINE: (u32, f32) = (12, 197.0);

/// A deterministic hand tremor: the same numbers every run.
struct Tremor(u64);

impl Tremor {
    /// A number in -1 to 1.
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 33) as f32 / (1u64 << 31) as f32) * 2.0 - 1.0
    }
}

/// The pressure of a stroke at `t` (0 to 1 of its length): soft on, firmer, soft off.
fn pressure_at(t: f32) -> f32 {
    0.35 + 0.4 * (std::f32::consts::PI * t.clamp(0.0, 1.0)).sin()
}

/// Samples for `points`, spread over `duration_ms` evenly.
fn timed(points: &[(f32, f32)], duration_ms: u32) -> Vec<Sample> {
    let last = (points.len().max(2) - 1) as f32;
    points
        .iter()
        .enumerate()
        .map(|(i, &(x, y))| {
            let t = i as f32 / last;
            Sample { x, y, t_ms: (t * duration_ms as f32).round() as u32, pressure: pressure_at(t) }
        })
        .collect()
}

/// `n` points of an ellipse, from `start_deg` turning `turns` times clockwise on the screen.
fn ellipse(c: (f32, f32), rx: f32, ry: f32, start_deg: f32, turns: f32, n: usize, tremor: f32, seed: u64) -> Vec<(f32, f32)> {
    let mut hand = Tremor(seed);
    (0..n)
        .map(|i| {
            let a = (start_deg + 360.0 * turns * i as f32 / (n - 1) as f32).to_radians();
            (c.0 + rx * a.cos() + tremor * hand.next(), c.1 + ry * a.sin() + tremor * hand.next())
        })
        .collect()
}

/// `n` points of a rounded rectangle (a squircle, |x/a|^4 + |y/b|^4 = 1) from `start_deg`.
fn squircle(c: (f32, f32), a: f32, b: f32, start_deg: f32, turns: f32, n: usize) -> Vec<(f32, f32)> {
    (0..n)
        .map(|i| {
            let t = (start_deg + 360.0 * turns * i as f32 / (n - 1) as f32).to_radians();
            let (co, si) = (t.cos(), t.sin());
            (c.0 + a * co.signum() * co.abs().powf(0.5), c.1 + b * si.signum() * si.abs().powf(0.5))
        })
        .collect()
}

/// `n` points on the line from `a` to `b`, each `bow` px off it by the sine of its place (0 is straight).
fn line(a: (f32, f32), b: (f32, f32), n: usize, bow: f32) -> Vec<(f32, f32)> {
    (0..n)
        .map(|i| {
            let t = i as f32 / (n - 1) as f32;
            (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t + bow * (std::f32::consts::PI * t).sin())
        })
        .collect()
}

/// A loop round the word with a small gap where the end meets the start: 0.97 of a turn.
#[must_use]
pub fn closed_loop() -> Fixture {
    Fixture {
        name: "closed loop",
        meaning: Meaning::ClosedShape,
        samples: timed(&ellipse(WORD_CENTRE, 55.0, 24.0, -100.0, 0.97, 44, 0.0, 1), 520),
    }
}

/// A circle that goes past its start and crosses its own beginning: 1.18 turns.
#[must_use]
pub fn circle_with_overlap() -> Fixture {
    Fixture {
        name: "circle with overlap",
        meaning: Meaning::ClosedShape,
        samples: timed(&ellipse(WORD_CENTRE, 55.0, 24.0, -100.0, 1.18, 52, 0.0, 2), 600),
    }
}

/// A rounded rectangle round the word, the end stopping a few px short of the start.
#[must_use]
pub fn rounded_rectangle_loop() -> Fixture {
    Fixture {
        name: "rounded rectangle loop",
        meaning: Meaning::ClosedShape,
        samples: timed(&squircle(WORD_CENTRE, 62.0, 26.0, 200.0, 0.975, 64), 700),
    }
}

/// The closed loop drawn with a shaky hand: 2 px of tremor on every sample.
#[must_use]
pub fn wobbly_loop() -> Fixture {
    Fixture {
        name: "wobbly loop",
        meaning: Meaning::ClosedShape,
        samples: timed(&ellipse(WORD_CENTRE, 55.0, 24.0, -100.0, 0.98, 70, 2.0, 3), 560),
    }
}

/// A loop round a block of six lines (lines 18 to 23): far larger than a word.
#[must_use]
pub fn loop_around_a_block() -> Fixture {
    Fixture {
        name: "loop around a block",
        meaning: Meaning::ClosedShape,
        samples: timed(&ellipse((400.0, 297.5), 170.0, 62.0, 170.0, 0.98, 90, 0.8, 4), 900),
    }
}

/// A loop round the whole of line 12, long and thin: 420 px by 26.
#[must_use]
pub fn loop_around_a_line() -> Fixture {
    Fixture {
        name: "loop around a line",
        meaning: Meaning::ClosedShape,
        samples: timed(&ellipse((400.0, 204.5), 210.0, 13.0, 180.0, 0.98, 90, 0.5, 5), 800),
    }
}

/// A "C": 0.55 of a turn, nowhere near closing.
#[must_use]
pub fn open_c() -> Fixture {
    Fixture {
        name: "open c",
        meaning: Meaning::OpenArc,
        samples: timed(&ellipse(WORD_CENTRE, 55.0, 24.0, 130.0, 0.55, 30, 0.0, 6), 380),
    }
}

/// A quick slash through the word, left to right, a little uphill: 76 px in 110 ms.
#[must_use]
pub fn slash() -> Fixture {
    Fixture { name: "slash", meaning: Meaning::Slash, samples: timed(&line((296.0, 206.5), (372.0, 202.5), 10, 0.0), 110) }
}

/// A slash through a long word, left to right: 160 px in 220 ms, with a little bow.
#[must_use]
pub fn slash_through_a_long_word() -> Fixture {
    Fixture {
        name: "slash through a long word",
        meaning: Meaning::Slash,
        samples: timed(&line((300.0, 205.5), (460.0, 200.5), 16, 2.0), 220),
    }
}

/// An underline under the word: 66 px, sagging 2 px, in 240 ms.
#[must_use]
pub fn underline() -> Fixture {
    Fixture {
        name: "underline",
        meaning: Meaning::Underline,
        samples: timed(&line((298.0, 219.5), (368.0, 220.5), 20, 2.0), 240),
    }
}

/// Six passes back and forth over the word, going down a little with each: striking it out.
#[must_use]
pub fn scribble() -> Fixture {
    let mut hand = Tremor(7);
    let mut points = Vec::new();
    for k in 0..7 {
        let (x, y) = (if k % 2 == 0 { 298.0 } else { 368.0 }, 199.0 + 2.0 * k as f32);
        points.push((x + hand.next(), y + hand.next()));
    }
    // Between the turns, the points of the passes.
    let mut dense = Vec::new();
    for w in points.windows(2) {
        for i in 0..6 {
            let t = i as f32 / 6.0;
            dense.push((w[0].0 + (w[1].0 - w[0].0) * t, w[0].1 + (w[1].1 - w[0].1) * t));
        }
    }
    dense.push(*points.last().unwrap_or(&(0.0, 0.0)));
    Fixture { name: "scribble", meaning: Meaning::Scribble, samples: timed(&dense, 420) }
}

/// A scribble of crossing diagonals over the word, 40 px tall: the crossings make triangles.
#[must_use]
pub fn crossing_scribble() -> Fixture {
    let corners = [
        (300.0, 187.5),
        (366.0, 227.5),
        (300.0, 227.5),
        (366.0, 187.5),
        (300.0, 189.5),
        (366.0, 229.5),
        (300.0, 229.5),
        (366.0, 189.5),
    ];
    let mut dense = Vec::new();
    for w in corners.windows(2) {
        for i in 0..8 {
            let t = i as f32 / 8.0;
            dense.push((w[0].0 + (w[1].0 - w[0].0) * t, w[0].1 + (w[1].1 - w[0].1) * t));
        }
    }
    dense.push(*corners.last().unwrap_or(&(0.0, 0.0)));
    Fixture { name: "crossing scribble", meaning: Meaning::Scribble, samples: timed(&dense, 650) }
}

/// The pen touched down and lifted: two samples a pixel apart.
#[must_use]
pub fn tap() -> Fixture {
    Fixture {
        name: "tap",
        meaning: Meaning::Tap,
        samples: vec![
            Sample { x: 333.0, y: 204.5, t_ms: 0, pressure: 0.5 },
            Sample { x: 333.6, y: 204.9, t_ms: 60, pressure: 0.5 },
        ],
    }
}

/// A vertical bar in the margin, 40 px long.
#[must_use]
pub fn vertical_bar() -> Fixture {
    Fixture { name: "vertical bar", meaning: Meaning::Line, samples: timed(&line((280.0, 187.5), (281.0, 227.5), 14, 0.0), 260) }
}

/// A tick: a short stroke down, a long one up.
#[must_use]
pub fn check_mark() -> Fixture {
    let mut pts = line((300.0, 202.5), (312.0, 217.5), 8, 0.0);
    pts.extend(line((312.0, 217.5), (350.0, 185.5), 14, 0.0).into_iter().skip(1));
    Fixture { name: "check mark", meaning: Meaning::Mark, samples: timed(&pts, 330) }
}

/// A figure of eight: two loops of opposite ways round.
#[must_use]
pub fn figure_eight() -> Fixture {
    let pts: Vec<(f32, f32)> = (0..60)
        .map(|i| {
            let a = (i as f32 / 59.0 * 360.0).to_radians();
            (333.0 + 55.0 * a.sin(), 204.5 + 24.0 * (2.0 * a).sin())
        })
        .collect();
    Fixture { name: "figure of eight", meaning: Meaning::Mark, samples: timed(&pts, 640) }
}

/// Every fixture.
#[must_use]
pub fn all() -> Vec<Fixture> {
    vec![
        closed_loop(),
        circle_with_overlap(),
        rounded_rectangle_loop(),
        wobbly_loop(),
        loop_around_a_block(),
        loop_around_a_line(),
        open_c(),
        slash(),
        slash_through_a_long_word(),
        underline(),
        scribble(),
        crossing_scribble(),
        tap(),
        vertical_bar(),
        check_mark(),
        figure_eight(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_fixture_has_time_running_forward_and_a_pressure_in_range() {
        for f in all() {
            assert!(f.samples.len() >= 2, "{}", f.name);
            assert!(f.samples.windows(2).all(|w| w[1].t_ms >= w[0].t_ms), "{}: time runs forward", f.name);
            assert!(f.samples.iter().all(|s| (0.0..=1.0).contains(&s.pressure)), "{}", f.name);
            assert!(f.samples.iter().all(|s| s.x.is_finite() && s.y.is_finite()), "{}", f.name);
        }
    }

    #[test]
    fn the_fixtures_are_the_same_strokes_every_run() {
        assert_eq!(wobbly_loop().samples, wobbly_loop().samples);
        assert_eq!(scribble().samples, scribble().samples);
    }

    #[test]
    fn a_slash_is_quick_and_short_and_a_loop_is_neither() {
        let dur = |f: &Fixture| f.samples.last().map_or(0, |s| s.t_ms);
        assert!(dur(&slash()) <= 150 && dur(&slash_through_a_long_word()) <= 250);
        assert!(dur(&closed_loop()) >= 400 && dur(&circle_with_overlap()) >= 500);
    }
}
