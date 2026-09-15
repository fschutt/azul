use crate::model::{InkPoint, Semantic, Stroke};

const PEN_RADIUS: f32 = 1.6;
const MARKER_RADIUS: f32 = 9.0;
const MARKER_ALPHA: f32 = 0.38;

fn kernel(q: f32) -> f32 {
    if q >= 1.0 {
        return 0.0;
    }
    let t = 1.0 - q * q;
    t * t
}

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

fn splat_dab(cov: &mut [f32], w: u32, h: u32, p: InkPoint, base: f32, marker: bool) {
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
            let along = ox * ax + oy * ay;
            let across = -ox * ay + oy * ax;
            let q = ((along / elong).powi(2) + across.powi(2)).sqrt() / r.max(0.01);
            let v = kernel(q);
            if v > 0.0 {
                let i = yy as usize * w as usize + xx as usize;
                if v > cov[i] {
                    cov[i] = v;
                }
            }
        }
    }
}

pub fn point_from(x: f32, y: f32, pressure: f32, tilt_x: f32, tilt_y: f32) -> InkPoint {
    InkPoint {
        x,
        y,
        pressure: if pressure <= 0.0 {
            0.55
        } else {
            pressure.clamp(0.05, 1.0)
        },
        tilt_x,
        tilt_y,
    }
}

pub const fn eraser_semantic() -> Option<Semantic> {
    None
}
