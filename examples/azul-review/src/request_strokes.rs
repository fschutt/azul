//! Which strokes an agent is told of for a conversation, and how each is handed over.
//!
//! What the reviewer drew while talking is half of what they meant: "this" is what the strokes mark. A request carries the
//! strokes that are about the conversation, as the samples the pen gave (x, y, pressure, the time of each, the tilt: the agent's
//! own module [`crate::agent_stroke`] cleans them and makes the SVG shape it reads). A stroke is about the conversation when
//!
//! * it is on the note's page and within [`NEAR_PX`] of the note's anchor (the distance is to the box of the stroke, so a long
//!   underline that passes under the anchor is near), or
//! * it was drawn while the words were said (from [`SLACK_MS`] before the first word to [`SLACK_MS`] after the last), wherever
//!   on the page it is: a circle drawn across the page while saying "this one".
//!
//! At most [`MAX_STROKES`] are told; when there are more the ones drawn during the words come first and then the nearest, and they
//! are handed over in the order they were drawn. Strokes are kept where the page was first drawn (lines an agent's suggestion put in
//! are not in them), which is the page the request's code is numbered by.
//!
//! Pure: a note and the strokes in, strokes out.

use crate::{
    agent_review::{PageGrid, WordRef},
    agent_stroke::{Meaning, Sample, StrokeInput},
    deletion,
    ink::{nib_width, Layer},
    model::{Note, Semantic, Stroke},
};

/// How far from the anchor a stroke may be and still be about the conversation, px.
pub const NEAR_PX: f32 = 160.0;
/// How long before the first word and after the last a stroke is still about what was said, ms.
pub const SLACK_MS: u64 = 4_000;
/// The most strokes a conversation tells (the request's own limit cuts further).
pub const MAX_STROKES: usize = 12;
/// The most words under the strokes a request lists.
pub const MAX_WORDS_UNDER: usize = 64;
/// How far apart along a stroke what is under it is looked at, px: under half a character, so that a fast stroke skips no word.
const STEP_PX: f32 = 3.0;
/// The most places looked at on one stroke.
const MAX_CHECKS: usize = 20_000;
/// The longest word listed, bytes.
const MAX_WORD_BYTES: usize = 128;

/// The meaning of a pen as the agent knows it.
#[must_use]
pub const fn meaning_of(semantic: Semantic) -> Meaning {
    match semantic {
        Semantic::Scope => Meaning::Scope,
        Semantic::Issue => Meaning::Issue,
        Semantic::Question => Meaning::Question,
        Semantic::Duplicate => Meaning::Duplicate,
        Semantic::Praise => Meaning::Praise,
    }
}

/// When the words of the conversation were said, Unix ms: from the first word of its first utterance to the last word of its last
/// (an utterance with no time, a typed note, says nothing).
fn spoken_window(note: &Note) -> Option<(u64, u64)> {
    let mut window: Option<(u64, u64)> = None;
    for said in note.talk.said.iter().filter(|s| s.started_unix_ms > 0) {
        let first = said.words.iter().map(|w| u64::from(w.start_ms)).min().unwrap_or(0);
        let last = said.words.iter().map(|w| u64::from(w.end_ms)).max().unwrap_or(0).max(first);
        let (from, to) = (said.started_unix_ms.saturating_add(first), said.started_unix_ms.saturating_add(last));
        window = Some(window.map_or((from, to), |(a, b)| (a.min(from), b.max(to))));
    }
    window
}

/// `[min x, min y, max x, max y]` of the points of the stroke that are numbers; none where there are none.
fn bbox(stroke: &Stroke) -> Option<[f32; 4]> {
    let mut found: Option<[f32; 4]> = None;
    for p in stroke.points.iter().filter(|p| p.x.is_finite() && p.y.is_finite()) {
        found = Some(match found {
            None => [p.x, p.y, p.x, p.y],
            Some([a, b, c, d]) => [a.min(p.x), b.min(p.y), c.max(p.x), d.max(p.y)],
        });
    }
    found
}

/// How far (`x`, `y`) is from the box (0 inside it).
fn distance_to_box(x: f32, y: f32, [x0, y0, x1, y1]: [f32; 4]) -> f32 {
    let dx = (x0 - x).max(x - x1).max(0.0);
    let dy = (y0 - y).max(y - y1).max(0.0);
    dx.hypot(dy)
}

/// Whether the stroke was drawn within the time `window` (Unix ms) widened by [`SLACK_MS`] on both sides.
fn drawn_during(stroke: &Stroke, window: Option<(u64, u64)>) -> bool {
    let Some((from, to)) = window else {
        return false;
    };
    if stroke.started_unix_ms == 0 {
        return false;
    }
    let began = stroke.started_unix_ms;
    let ended = began.saturating_add(u64::from(stroke.points.iter().map(|p| p.t_ms).max().unwrap_or(0)));
    began <= to.saturating_add(SLACK_MS) && ended >= from.saturating_sub(SLACK_MS)
}

/// A stroke as the agent gets it: the nib, the meaning, whether the reviewer chose it, and every sample.
fn handed_over(stroke: &Stroke) -> StrokeInput {
    let marker = Layer::of(stroke.semantic) == Layer::Marker;
    StrokeInput {
        id: stroke.id,
        page: stroke.page,
        meaning: Some(meaning_of(stroke.semantic)),
        // the window starts with the marker: only another meaning is a choice the agent must not override
        pinned: stroke.semantic != Semantic::Scope,
        nib: if marker { "marker" } else { "pen" }.to_string(),
        nib_width: nib_width(stroke.semantic),
        t0_ms: stroke.started_unix_ms,
        samples: stroke
            .points
            .iter()
            .map(|p| Sample {
                x: p.x,
                y: p.y,
                pressure: p.pressure,
                dt_ms: p.t_ms,
                tilt: (p.tilt_x != 0.0 || p.tilt_y != 0.0).then_some((p.tilt_x, p.tilt_y)),
            })
            .collect(),
    }
}

/// The strokes of the open file that an agent is told of for `note`, in the order they were drawn.
#[must_use]
pub fn pick(note: &Note, strokes: &[Stroke]) -> Vec<StrokeInput> {
    let window = spoken_window(note);
    // (drawn during the words, distance from the anchor, the stroke)
    let mut about: Vec<(bool, f32, &Stroke)> = strokes
        .iter()
        .filter(|s| s.page == note.page)
        .filter_map(|s| {
            let near = bbox(s).map(|b| distance_to_box(note.x, note.y, b)).filter(|d| *d <= NEAR_PX);
            let during = drawn_during(s, window);
            (near.is_some() || during).then(|| (during, near.unwrap_or(f32::MAX), s))
        })
        .collect();
    // the ones drawn during the words first, then the nearest; a tie goes to the one drawn first
    about.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.total_cmp(&b.1)).then(a.2.id.cmp(&b.2.id)));
    about.truncate(MAX_STROKES);
    about.sort_by_key(|(_, _, s)| s.id);
    about.into_iter().map(|(_, _, s)| handed_over(s)).collect()
}

/// A word is something with a letter, a digit or `_` in it: the signs between words (`{`, `->`, `;`) are not what a stroke is on.
fn is_word(text: &str) -> bool {
    text.chars().any(|c| c.is_alphanumeric() || c == '_')
}

/// `text` as at most [`MAX_WORD_BYTES`] bytes, cut at a letter.
fn cut(text: &str) -> &str {
    let mut end = text.len().min(MAX_WORD_BYTES);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// The words of the code the strokes are on, each with its line, its byte in the line and the stroke that is on it: the words
/// the path of a stroke passes over (looked at every few px along it, so that a fast stroke skips none), in the order it passes
/// them, once for each stroke. `lines` are the lines of the page as `grid` draws them. Signs with no letter in them are not
/// words; at most [`MAX_WORDS_UNDER`] are listed.
#[must_use]
pub fn words_under(strokes: &[StrokeInput], grid: &PageGrid, lines: &[String]) -> Vec<WordRef> {
    let numbers = [grid.origin_x, grid.origin_y, grid.line_height, grid.char_width];
    if !numbers.iter().all(|n| n.is_finite()) || grid.line_height <= 0.0 || grid.char_width <= 0.0 || lines.is_empty() {
        return Vec::new();
    }
    let tokens: Vec<Vec<deletion::Token>> = lines.iter().map(|l| deletion::tokens(l)).collect();
    let mut out: Vec<WordRef> = Vec::new();
    for stroke in strokes {
        // (row, the word's byte) already listed for this stroke
        let mut seen: Vec<(usize, usize)> = Vec::new();
        let mut look = |x: f32, y: f32, out: &mut Vec<WordRef>| {
            let row = ((y - grid.origin_y) / grid.line_height).floor();
            if row < 0.0 || row >= lines.len() as f32 {
                return;
            }
            let row = row as usize;
            let col = (x - grid.origin_x) / grid.char_width;
            if col < 0.0 {
                return;
            }
            for token in &tokens[row] {
                if col >= token.col as f32 && col < (token.col + token.cols) as f32 {
                    let text = &lines[row][token.start..token.end];
                    if is_word(text) && !seen.contains(&(row, token.start)) {
                        seen.push((row, token.start));
                        out.push(WordRef { stroke: stroke.id, line: grid.first_line + row, col: token.start, text: cut(text).to_string() });
                    }
                    return;
                }
            }
        };
        let points: Vec<(f32, f32)> = stroke.samples.iter().map(|s| (s.x, s.y)).collect();
        let mut checks = 0;
        for (i, &(x, y)) in points.iter().enumerate() {
            if !x.is_finite() || !y.is_finite() {
                continue;
            }
            look(x, y, &mut out);
            // along the segment to the next sample, when that is a number too
            let Some(&(nx, ny)) = points.get(i + 1).filter(|(a, b)| a.is_finite() && b.is_finite()) else {
                continue;
            };
            let steps = ((nx - x).hypot(ny - y) / STEP_PX).ceil().max(1.0).min(10_000.0) as usize;
            for k in 1..steps {
                checks += 1;
                if checks > MAX_CHECKS {
                    break;
                }
                let t = k as f32 / steps as f32;
                look(x + (nx - x) * t, y + (ny - y) * t, &mut out);
            }
            if out.len() >= MAX_WORDS_UNDER {
                break;
            }
        }
        out.truncate(MAX_WORDS_UNDER);
        if out.len() >= MAX_WORDS_UNDER {
            break;
        }
    }
    out
}

#[cfg(test)]
#[path = "request_strokes_test.rs"]
mod request_strokes_test;
