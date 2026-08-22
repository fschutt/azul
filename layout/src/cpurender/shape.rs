//! A window's SHAPE from its rendered alpha: the rectangles that cover every
//! pixel the frame painted with alpha above a threshold.
//!
//! What the windowing layer hands to `XShape` (`ShapeBounding` +
//! `ShapeInput`), `wl_surface.set_input_region`, or `SetWindowRgn`: the OS
//! treats everything outside as not-the-window (clicks fall through, X11 and
//! Windows also stop drawing it). macOS needs none of this - a non-opaque
//! window hit-tests by its alpha on its own.
//!
//! Rows are scanned for runs of opaque-enough pixels; consecutive rows with
//! identical runs are merged into taller rectangles, so a rounded-corner
//! popup costs a handful of rects per corner row and one big one for the
//! body, not one per pixel.

use super::AzulPixmap;

/// One rectangle of the shape, in PHYSICAL (buffer) pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ShapeRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// The default alpha threshold: a pixel the content touched at all belongs
/// to the window (anti-aliased edges stay clickable; a shadow at 2% does
/// not make the window rectangular again).
pub const SHAPE_ALPHA_THRESHOLD: u8 = 8;

/// The rectangles covering every pixel of `pixmap` with alpha >= `threshold`.
/// Empty for a fully transparent frame - the caller decides whether an
/// empty shape means "no window" or "keep the last shape".
#[must_use]
pub fn alpha_shape_rects(pixmap: &AzulPixmap, threshold: u8) -> Vec<ShapeRect> {
    alpha_shape_rects_raw(pixmap.data(), pixmap.width(), pixmap.height(), threshold)
}

/// [`alpha_shape_rects`] over raw RGBA8 (premultiplied or not - only the
/// alpha byte is read), `width * height * 4` bytes.
#[must_use]
pub fn alpha_shape_rects_raw(rgba: &[u8], width: u32, height: u32, threshold: u8) -> Vec<ShapeRect> {
    let w = width as usize;
    let h = height as usize;
    if w == 0 || h == 0 || rgba.len() < w * h * 4 {
        return Vec::new();
    }
    let mut out: Vec<ShapeRect> = Vec::new();
    // The runs of the previous row, as (x, width) pairs, and where in `out`
    // they start - a row whose runs repeat the previous row's extends them.
    let mut prev_runs: Vec<(u32, u32)> = Vec::new();
    let mut prev_start = 0usize;
    let mut runs: Vec<(u32, u32)> = Vec::new();
    for y in 0..h {
        runs.clear();
        let row = &rgba[y * w * 4..(y + 1) * w * 4];
        let mut x = 0usize;
        while x < w {
            if row[x * 4 + 3] < threshold {
                x += 1;
                continue;
            }
            let start = x;
            while x < w && row[x * 4 + 3] >= threshold {
                x += 1;
            }
            #[allow(clippy::cast_possible_truncation)] // within `width`
            runs.push((start as u32, (x - start) as u32));
        }
        if !runs.is_empty() && runs == prev_runs {
            for r in &mut out[prev_start..] {
                r.height += 1;
            }
            continue;
        }
        prev_start = out.len();
        #[allow(clippy::cast_possible_truncation)] // within `height`
        out.extend(runs.iter().map(|&(x, width)| ShapeRect { x, y: y as u32, width, height: 1 }));
        core::mem::swap(&mut prev_runs, &mut runs);
        if out.len() == prev_start {
            // An empty row breaks vertical merging.
            prev_runs.clear();
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(w: u32, h: u32, opaque: impl Fn(u32, u32) -> bool) -> Vec<u8> {
        let mut v = vec![0u8; (w * h * 4) as usize];
        for y in 0..h {
            for x in 0..w {
                if opaque(x, y) {
                    v[((y * w + x) * 4 + 3) as usize] = 255;
                }
            }
        }
        v
    }

    #[test]
    fn a_full_frame_is_one_rect_and_an_empty_one_none() {
        let full = frame(5, 3, |_, _| true);
        assert_eq!(
            alpha_shape_rects_raw(&full, 5, 3, SHAPE_ALPHA_THRESHOLD),
            vec![ShapeRect { x: 0, y: 0, width: 5, height: 3 }]
        );
        let empty = frame(5, 3, |_, _| false);
        assert!(alpha_shape_rects_raw(&empty, 5, 3, SHAPE_ALPHA_THRESHOLD).is_empty());
        assert!(alpha_shape_rects_raw(&[], 0, 0, 1).is_empty());
    }

    #[test]
    fn rounded_corners_become_one_rect_per_distinct_row_shape() {
        // A 6x4 frame with the top corners cut: row 0 spans 1..5, rows 1-3 full.
        let f = frame(6, 4, |x, y| y > 0 || (1..5).contains(&x));
        assert_eq!(
            alpha_shape_rects_raw(&f, 6, 4, SHAPE_ALPHA_THRESHOLD),
            vec![
                ShapeRect { x: 1, y: 0, width: 4, height: 1 },
                ShapeRect { x: 0, y: 1, width: 6, height: 3 },
            ]
        );
    }

    #[test]
    fn holes_split_runs_and_gaps_break_vertical_merging() {
        // Two columns with a gap, a fully transparent row, then one column.
        let f = frame(5, 3, |x, y| match y {
            0 => x < 2 || x == 4,
            1 => false,
            _ => x < 2,
        });
        assert_eq!(
            alpha_shape_rects_raw(&f, 5, 3, SHAPE_ALPHA_THRESHOLD),
            vec![
                ShapeRect { x: 0, y: 0, width: 2, height: 1 },
                ShapeRect { x: 4, y: 0, width: 1, height: 1 },
                ShapeRect { x: 0, y: 2, width: 2, height: 1 },
            ]
        );
    }

    #[test]
    fn the_threshold_keeps_antialiased_edges_and_drops_faint_shadow() {
        let mut f = frame(3, 1, |_, _| false);
        f[3] = 2; // faint
        f[7] = 8; // edge
        f[11] = 255;
        assert_eq!(
            alpha_shape_rects_raw(&f, 3, 1, SHAPE_ALPHA_THRESHOLD),
            vec![ShapeRect { x: 1, y: 0, width: 2, height: 1 }]
        );
    }
}
