//! Ink rasterisation — the metaball field from AzPaint, retuned for a MARKER.
//!
//! Two differences from the paint app it borrows from:
//!
//! 1. **Translucent, not opaque.** A highlighter must let the glyphs show
//!    through, so the field is composited at a fixed low alpha rather than
//!    replacing the pixel.
//! 2. **Alpha does not accumulate along a stroke.** Real marker ink is
//!    absorbed by the paper: dragging back over the same spot does not get
//!    darker. Summing per-dab alpha (which is what a naive metaball sum does)
//!    made slow strokes blotchy and fast ones translucent, so coverage is
//!    computed as a MAX over the field rather than a sum.


use crate::model::{InkPoint, Semantic, Stroke};

/// Base half-width of a pen dab at full pressure, in logical px.
const PEN_RADIUS: f32 = 1.6;
/// Highlighter dabs are much wider — a marker nib, not a ballpoint.
const MARKER_RADIUS: f32 = 9.0;
/// Marker coverage never exceeds this, so code stays readable underneath.
const MARKER_ALPHA: f32 = 0.38;

/// Smooth falloff, 1.0 at the centre of a dab and 0.0 at its edge.
///
/// The quartic is deliberate: a linear falloff makes two overlapping dabs meet
/// in a visible crease, which is exactly the "weird edges on metaball merge"
/// artefact. This one has zero derivative at both ends, so merges are smooth.
fn kernel(q: f32) -> f32 {
    if q >= 1.0 {
        return 0.0;
    }
    let t = 1.0 - q * q;
    t * t
}

/// Rasterise all strokes for one page into an RGBA buffer.
///
/// Highlighter and pen are drawn in two passes, lowest layer first, so pen
/// commentary always reads on top of the region it refers to — the same
/// stacking the paper has by physical accident.
pub fn rasterize_page(strokes: &[&Stroke], w: u32, h: u32) -> Vec<u8> {
    let mut buf = vec![0u8; (w as usize) * (h as usize) * 4];
    for pass_highlighter in [true, false] {
        for s in strokes {
            if s.semantic.is_highlighter() != pass_highlighter {
                continue;
            }
            draw_stroke(&mut buf, w, h, s);
        }
    }
    buf
}

fn draw_stroke(buf: &mut [u8], w: u32, h: u32, stroke: &Stroke) {
    let color = stroke.semantic.color();
    let marker = stroke.semantic.is_highlighter();
    let base = if marker { MARKER_RADIUS } else { PEN_RADIUS };

    // Coverage for THIS stroke only, so overlap within one stroke maxes
    // instead of accumulating, while separate strokes still layer normally.
    let mut cov = vec![0f32; (w as usize) * (h as usize)];

    for pair in stroke.points.windows(2) {
        splat_segment(&mut cov, w, h, pair[0], pair[1], base, marker);
    }
    if stroke.points.len() == 1 {
        splat_dab(&mut cov, w, h, stroke.points[0], base, marker);
    }

    let max_a = if marker { MARKER_ALPHA } else { 1.0 };
    for i in 0..cov.len() {
        let a = (cov[i]).min(1.0) * max_a;
        if a <= 0.001 {
            continue;
        }
        let o = i * 4;
        let (dr, dg, db, da) = (
            buf[o] as f32 / 255.0,
            buf[o + 1] as f32 / 255.0,
            buf[o + 2] as f32 / 255.0,
            buf[o + 3] as f32 / 255.0,
        );
        let (sr, sg, sb) = (
            color.r as f32 / 255.0,
            color.g as f32 / 255.0,
            color.b as f32 / 255.0,
        );
        // Source-over.
        let out_a = a + da * (1.0 - a);
        if out_a <= 0.0 {
            continue;
        }
        buf[o] = (((sr * a + dr * da * (1.0 - a)) / out_a) * 255.0) as u8;
        buf[o + 1] = (((sg * a + dg * da * (1.0 - a)) / out_a) * 255.0) as u8;
        buf[o + 2] = (((sb * a + db * da * (1.0 - a)) / out_a) * 255.0) as u8;
        buf[o + 3] = (out_a * 255.0) as u8;
    }
}

/// Walk a segment in sub-dab steps so a fast stroke is still continuous.
///
/// Stepping by a fraction of the radius rather than a fixed pixel count is
/// what keeps a quick flick from turning into a dotted line — the pointer only
/// samples so often, and the gap between samples is arbitrary.
fn splat_segment(
    cov: &mut [f32],
    w: u32,
    h: u32,
    a: InkPoint,
    b: InkPoint,
    base: f32,
    marker: bool,
) {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let dist = (dx * dx + dy * dy).sqrt();
    let step = (base * 0.35).max(0.75);
    let n = ((dist / step).ceil() as usize).max(1);
    for i in 0..=n {
        let t = i as f32 / n as f32;
        let p = InkPoint {
            x: a.x + dx * t,
            y: a.y + dy * t,
            pressure: a.pressure + (b.pressure - a.pressure) * t,
            tilt_x: a.tilt_x + (b.tilt_x - a.tilt_x) * t,
            tilt_y: a.tilt_y + (b.tilt_y - a.tilt_y) * t,
        };
        splat_dab(cov, w, h, p, base, marker);
    }
}

/// One dab. Tilt elongates it along the tilt direction, so a tilted pen paints
/// a directional, stretched blob the way a real nib does.
fn splat_dab(cov: &mut [f32], w: u32, h: u32, p: InkPoint, base: f32, marker: bool) {
    // A marker keeps a constant nib width; a pen thins with lighter pressure.
    let r = if marker {
        base
    } else {
        base * (0.35 + 0.65 * p.pressure.clamp(0.0, 1.0))
    };
    let tilt_mag = (p.tilt_x * p.tilt_x + p.tilt_y * p.tilt_y).sqrt().min(1.0);
    let elong = 1.0 + tilt_mag * 1.6;
    let (ax, ay) = if tilt_mag > 0.01 {
        (p.tilt_x / tilt_mag, p.tilt_y / tilt_mag)
    } else {
        (1.0, 0.0)
    };

    let reach = (r * elong).ceil() as i32 + 1;
    let cx = p.x;
    let cy = p.y;
    for yy in (cy as i32 - reach)..=(cy as i32 + reach) {
        if yy < 0 || yy >= h as i32 {
            continue;
        }
        for xx in (cx as i32 - reach)..=(cx as i32 + reach) {
            if xx < 0 || xx >= w as i32 {
                continue;
            }
            let ox = xx as f32 + 0.5 - cx;
            let oy = yy as f32 + 0.5 - cy;
            // Project into the dab's own frame: along the tilt axis it is
            // `elong` times longer, across it unchanged.
            let along = ox * ax + oy * ay;
            let across = -ox * ay + oy * ax;
            let q = ((along / elong).powi(2) + across.powi(2)).sqrt() / r.max(0.01);
            let v = kernel(q);
            if v > 0.0 {
                let i = yy as usize * w as usize + xx as usize;
                // MAX, not +=: marker ink does not darken on re-traverse.
                if v > cov[i] {
                    cov[i] = v;
                }
            }
        }
    }
}

/// Convert a pen sample into an ink point, defaulting sanely for a finger or
/// mouse (no pressure, no tilt) so touch still draws.
pub fn point_from(x: f32, y: f32, pressure: f32, tilt_x: f32, tilt_y: f32) -> InkPoint {
    InkPoint {
        x,
        y,
        pressure: if pressure <= 0.0 { 0.55 } else { pressure.clamp(0.05, 1.0) },
        tilt_x,
        tilt_y,
    }
}

/// Semantic of the eraser end of a stylus — flipping the pen erases, which is
/// the gesture everyone already knows.
pub const fn eraser_semantic() -> Option<Semantic> {
    None
}
