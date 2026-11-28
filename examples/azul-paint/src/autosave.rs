//! The drawing survives the app being ended.
//!
//! A phone ends a background process without another word, so the strokes are written when the app leaves the
//! foreground (the lifecycle handler's `Paused`, [`crate::on_lifecycle`]) and read back when the window comes up. The
//! file is plain text, one `stroke` line (colour, eraser, number of points) and one line per point; a file that does not
//! parse is no drawing, never a crash: the canvas starts empty, as it did before.

use crate::{ColorU, Stroke, StrokePoint};

/// First line of the file: the format and its version.
const HEADER: &str = "azpaint-autosave 1";

/// The most strokes and the most points of one stroke a file may name, and the biggest file read: a corrupt or hostile
/// count must not make the app allocate its way to the end of the memory.
pub(crate) const MAX_STROKES: usize = 100_000;
pub(crate) const MAX_POINTS_PER_STROKE: usize = 1_000_000;
pub(crate) const MAX_BYTES: usize = 64 * 1024 * 1024;

/// The key of the autosave in the data tree: `paint/autosave.txt`.
pub(crate) fn key() -> String {
    azul_appkit::data::app_key(crate::ABOUT.app_folder, "autosave.txt")
}

/// The strokes as the file holds them.
pub(crate) fn encode(strokes: &[Stroke]) -> String {
    let mut out = String::from(HEADER);
    out.push('\n');
    for stroke in strokes {
        let c = stroke.color;
        out.push_str(&format!(
            "stroke {} {} {} {} {} {}\n",
            c.r,
            c.g,
            c.b,
            c.a,
            u8::from(stroke.is_eraser),
            stroke.points.len()
        ));
        for p in &stroke.points {
            out.push_str(&format!(
                "{} {} {} {} {} {}\n",
                p.x, p.y, p.pressure, p.tilt_x, p.tilt_y, p.barrel_roll_rad
            ));
        }
    }
    out
}

/// The strokes a file holds, or `None` for a file that is not one (another header, a count that is too big, a point
/// that is cut off or not a number).
pub(crate) fn decode(text: &str) -> Option<Vec<Stroke>> {
    if text.len() > MAX_BYTES {
        return None;
    }
    let mut lines = text.lines();
    if lines.next()?.trim() != HEADER {
        return None;
    }
    let mut strokes = Vec::new();
    while let Some(head) = lines.next() {
        if head.trim().is_empty() {
            continue;
        }
        let mut fields = head.split_whitespace();
        if fields.next()? != "stroke" {
            return None;
        }
        let r: u8 = fields.next()?.parse().ok()?;
        let g: u8 = fields.next()?.parse().ok()?;
        let b: u8 = fields.next()?.parse().ok()?;
        let a: u8 = fields.next()?.parse().ok()?;
        let eraser: u8 = fields.next()?.parse().ok()?;
        let count: usize = fields.next()?.parse().ok()?;
        if count > MAX_POINTS_PER_STROKE || strokes.len() >= MAX_STROKES {
            return None;
        }
        let mut points = Vec::with_capacity(count.min(4096));
        for _ in 0..count {
            let mut numbers = lines.next()?.split_whitespace().map(|n| n.parse::<f32>().ok());
            let mut next = || numbers.next().flatten().filter(|v| v.is_finite());
            points.push(StrokePoint {
                x: next()?,
                y: next()?,
                pressure: next()?,
                tilt_x: next()?,
                tilt_y: next()?,
                barrel_roll_rad: next()?,
            });
        }
        strokes.push(Stroke { points, color: ColorU { r, g, b, a }, is_eraser: eraser == 1 });
    }
    Some(strokes)
}
