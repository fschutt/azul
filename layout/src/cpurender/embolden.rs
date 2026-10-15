//! Synthetic bold: thickening a glyph outline for text that asks for a bold
//! weight its face does not have.
//!
//! Chrome draws such text with Skia's fake bold: the outline is stroked
//! while it is filled, with a stroke `size / 24` wide at 9px and below,
//! `size / 32` at 36px and above, and linear in between. Each side of a
//! stem therefore moves out by half that width, and the glyph's advance
//! stays as it is. Both of azul's rasterizers (the CPU one and `WebRender`'s
//! glyph rasterizer) thicken the outline here, before it is flattened, so
//! they draw the same stems.
//!
//! The outline is offset point by point, the way `FreeType`'s
//! `FT_Outline_EmboldenXY` does it: every point (control points too) moves
//! along the bisector of its two edges, far enough that both edges move out
//! by the strength. That is a mitred stroke, without stroking the path.

use agg_rust::{
    basics::{is_vertex, VertexD, PATH_CMD_MASK, PATH_CMD_MOVE_TO},
    path_storage::PathStorage,
};

/// Text size in px from which Skia's fake bold uses its thick ratio.
const THIN_SIZE: f32 = 9.0;
/// Text size in px from which Skia's fake bold uses its thin ratio.
const THICK_SIZE: f32 = 36.0;

/// The width of the stroke Skia adds around a fake-bold glyph at
/// `font_px`, in the same pixels as `font_px`. Each side of a stem moves
/// out by half of it.
#[must_use]
pub fn synthetic_bold_stroke_px(font_px: f32) -> f32 {
    if !font_px.is_finite() || font_px <= 0.0 {
        return 0.0;
    }
    let t = ((font_px - THIN_SIZE) / (THICK_SIZE - THIN_SIZE)).clamp(0.0, 1.0);
    let ratio = (1.0 / 24.0) + t * ((1.0 / 32.0) - (1.0 / 24.0));
    font_px * ratio
}

/// How far each side of a synthetic-bold glyph moves out at `font_px`.
#[must_use]
pub fn synthetic_bold_strength_px(font_px: f32) -> f32 {
    synthetic_bold_stroke_px(font_px) * 0.5
}

/// The contours of a path as half-open index ranges over its vertices: a
/// contour starts at a move-to and runs over every following vertex
/// command, up to the next move-to or the end-poly / stop that closes it.
fn contours(verts: &[VertexD]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    for (i, v) in verts.iter().enumerate() {
        let cmd = v.cmd & PATH_CMD_MASK;
        if cmd == PATH_CMD_MOVE_TO {
            if let Some(s) = start.take() {
                out.push((s, i));
            }
            start = Some(i);
        } else if !is_vertex(v.cmd) {
            if let Some(s) = start.take() {
                out.push((s, i));
            }
        }
    }
    if let Some(s) = start {
        out.push((s, verts.len()));
    }
    out.retain(|(s, e)| e - s >= 2);
    out
}

/// Twice the signed area of all contours together, control points counted
/// as polygon points. Its sign tells which side of an edge is outside: the
/// outer contours of a glyph all turn the same way and outweigh its holes.
fn signed_area2(verts: &[VertexD], contours: &[(usize, usize)]) -> f64 {
    let mut area = 0.0;
    for &(s, e) in contours {
        let n = e - s;
        for i in 0..n {
            let a = verts[s + i];
            let b = verts[s + (i + 1) % n];
            area += a.x * b.y - b.x * a.y;
        }
    }
    area
}

/// Moves every point of `verts` outward by `strength` path units along the
/// bisector of its two edges, so each edge of the outline moves out by
/// `strength` (holes shrink). Coordinates may be in any axis orientation:
/// which side is outside comes from the outline's own winding. Nothing
/// changes for a strength of zero or less, or an outline without area.
pub fn embolden_vertices(verts: &mut [VertexD], strength: f64) {
    if !strength.is_finite() || strength <= 0.0 {
        return;
    }
    let contours = contours(verts);
    let area = signed_area2(verts, &contours);
    if !area.is_finite() || area.abs() <= f64::MIN_POSITIVE {
        return;
    }
    // Outward normal of an edge with unit direction (dx, dy).
    let side = if area > 0.0 { 1.0 } else { -1.0 };
    let mut moved: Vec<(f64, f64)> = Vec::new();
    for (s, e) in contours {
        let n = e - s;
        let p = |i: usize| (verts[s + i].x, verts[s + i].y);
        let dist = |a: (f64, f64), b: (f64, f64)| (b.0 - a.0).hypot(b.1 - a.1);
        moved.clear();
        for i in 0..n {
            let here = p(i);
            // The nearest distinct neighbours: points sitting on top of each
            // other move together, by the shift of the corner they make.
            let prev = (1..n)
                .map(|k| p((i + n - k) % n))
                .find(|q| dist(*q, here) > 1e-9);
            let next = (1..n)
                .map(|k| p((i + k) % n))
                .find(|q| dist(here, *q) > 1e-9);
            let (Some(prev), Some(next)) = (prev, next) else {
                moved.push(here);
                continue;
            };
            let l_in = dist(prev, here);
            let l_out = dist(here, next);
            let din = ((here.0 - prev.0) / l_in, (here.1 - prev.1) / l_in);
            let dout = ((next.0 - here.0) / l_out, (next.1 - here.1) / l_out);
            let cos = din.0 * dout.0 + din.1 * dout.1;
            // A turn of more than about 160 degrees would mitre out to a
            // spike: such a point stays where it is, as in FreeType.
            if cos <= -0.9375 {
                moved.push(here);
                continue;
            }
            let d = 1.0 + cos;
            let bisector = (side * (din.1 + dout.1), -side * (din.0 + dout.0));
            // Positive at a concave corner, where a full mitre could run past
            // the next point of a short edge: the shift is then held to the
            // shorter edge.
            let q = -side * (din.0 * dout.1 - din.1 * dout.0);
            let l = l_in.min(l_out);
            let k = if strength * q <= l * d {
                strength / d
            } else {
                l / q
            };
            moved.push((here.0 + bisector.0 * k, here.1 + bisector.1 * k));
        }
        for (i, (x, y)) in moved.iter().enumerate() {
            verts[s + i].x = *x;
            verts[s + i].y = *y;
        }
    }
}

/// `path` with every contour moved out by `strength` path units
/// ([`embolden_vertices`]), as a new path with the same commands.
#[must_use]
pub fn emboldened_path(path: &PathStorage, strength: f64) -> PathStorage {
    let mut verts = path.vertices().to_vec();
    embolden_vertices(&mut verts, strength);
    let mut out = PathStorage::with_capacity(verts.len());
    for v in verts {
        out.add_vertex(v.x, v.y, v.cmd);
    }
    out
}

#[cfg(test)]
mod tests {
    use agg_rust::basics::{PATH_CMD_END_POLY, PATH_CMD_LINE_TO, PATH_FLAGS_CLOSE};

    use super::*;

    fn square(x0: f64, y0: f64, x1: f64, y1: f64, clockwise: bool) -> Vec<VertexD> {
        let mut pts = vec![(x0, y0), (x1, y0), (x1, y1), (x0, y1)];
        if clockwise {
            pts.reverse();
        }
        let mut v: Vec<VertexD> = pts
            .iter()
            .enumerate()
            .map(|(i, (x, y))| {
                let cmd = if i == 0 { PATH_CMD_MOVE_TO } else { PATH_CMD_LINE_TO };
                VertexD::new(*x, *y, cmd)
            })
            .collect();
        v.push(VertexD::new(0.0, 0.0, PATH_CMD_END_POLY | PATH_FLAGS_CLOSE));
        v
    }

    fn bounds(v: &[VertexD]) -> (f64, f64, f64, f64) {
        let pts = v.iter().filter(|v| is_vertex(v.cmd));
        pts.fold(
            (f64::MAX, f64::MAX, f64::MIN, f64::MIN),
            |(a, b, c, d), v| (a.min(v.x), b.min(v.y), c.max(v.x), d.max(v.y)),
        )
    }

    #[test]
    fn skias_fake_bold_stroke_is_a_24th_of_small_text_and_a_32nd_of_large_text() {
        assert!((synthetic_bold_stroke_px(9.0) - 9.0 / 24.0).abs() < 1e-6);
        assert!((synthetic_bold_stroke_px(6.0) - 6.0 / 24.0).abs() < 1e-6);
        assert!((synthetic_bold_stroke_px(36.0) - 36.0 / 32.0).abs() < 1e-6);
        assert!((synthetic_bold_stroke_px(72.0) - 72.0 / 32.0).abs() < 1e-6);
        let mid = synthetic_bold_stroke_px(22.5);
        assert!(mid > 22.5 / 32.0 && mid < 22.5 / 24.0, "linear in between: {mid}");
        assert_eq!(synthetic_bold_stroke_px(0.0), 0.0);
        assert_eq!(synthetic_bold_stroke_px(f32::NAN), 0.0);
        let half = synthetic_bold_strength_px(24.0) * 2.0 - synthetic_bold_stroke_px(24.0);
        assert!(half.abs() < 1e-6);
    }

    #[test]
    fn a_square_grows_by_the_strength_on_every_side_whichever_way_it_turns() {
        for clockwise in [false, true] {
            let mut v = square(0.0, 0.0, 10.0, 10.0, clockwise);
            embolden_vertices(&mut v, 1.0);
            let (x0, y0, x1, y1) = bounds(&v);
            for (got, want) in [(x0, -1.0), (y0, -1.0), (x1, 11.0), (y1, 11.0)] {
                assert!((got - want).abs() < 1e-9, "clockwise {clockwise}: {got} != {want}");
            }
        }
    }

    #[test]
    fn a_hole_shrinks_while_its_outline_grows() {
        // An outer square one way round, a hole the other way: an 'o'.
        let mut v = square(0.0, 0.0, 10.0, 10.0, false);
        v.extend(square(3.0, 3.0, 7.0, 7.0, true));
        embolden_vertices(&mut v, 0.5);
        let (outer, hole) = v.split_at(5);
        let (x0, _, x1, _) = bounds(outer);
        let (hx0, _, hx1, _) = bounds(hole);
        assert!((x0 + 0.5).abs() < 1e-9 && (x1 - 10.5).abs() < 1e-9);
        assert!((hx0 - 3.5).abs() < 1e-9 && (hx1 - 6.5).abs() < 1e-9);
    }

    #[test]
    fn nothing_moves_for_no_strength_or_an_outline_without_area() {
        let mut v = square(0.0, 0.0, 10.0, 10.0, false);
        let before = v.clone();
        embolden_vertices(&mut v, 0.0);
        assert_eq!(v, before);
        embolden_vertices(&mut v, f64::NAN);
        assert_eq!(v, before);
        let mut line = vec![
            VertexD::new(0.0, 0.0, PATH_CMD_MOVE_TO),
            VertexD::new(5.0, 0.0, PATH_CMD_LINE_TO),
        ];
        let line_before = line.clone();
        embolden_vertices(&mut line, 1.0);
        assert_eq!(line, line_before);
    }

    #[test]
    fn a_point_repeated_on_top_of_itself_moves_with_its_corner() {
        let mut v = square(0.0, 0.0, 10.0, 10.0, false);
        // Repeat the second corner.
        v.insert(2, VertexD::new(10.0, 0.0, PATH_CMD_LINE_TO));
        embolden_vertices(&mut v, 1.0);
        assert!((v[1].x - v[2].x).abs() < 1e-9 && (v[1].y - v[2].y).abs() < 1e-9);
        assert!((v[1].x - 11.0).abs() < 1e-9 && (v[1].y + 1.0).abs() < 1e-9);
    }

    #[test]
    fn the_rebuilt_path_keeps_its_commands() {
        let mut path = PathStorage::new();
        path.move_to(0.0, 0.0);
        path.line_to(10.0, 0.0);
        path.curve3(10.0, 10.0, 0.0, 10.0);
        path.close_polygon(0);
        let bold = emboldened_path(&path, 1.0);
        let a: Vec<u32> = path.vertices().iter().map(|v| v.cmd).collect();
        let b: Vec<u32> = bold.vertices().iter().map(|v| v.cmd).collect();
        assert_eq!(a, b);
    }
}
