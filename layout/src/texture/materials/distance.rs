//! The distance to a shape's edge, for bevels that follow ANY outline.
//!
//! A bevel is a height made from the distance to the edge of the shape a
//! surface is cut to. For a rect with rounded corners that distance is
//! analytic (the shader's `sdRound`); for any other shape - a `clip-path`
//! circle, ellipse or polygon - the shape is drawn as a mask at device
//! pixels and the exact Euclidean distance of every pixel to the edge is
//! computed with two passes of Felzenszwalb and Huttenlocher's 1D distance
//! transform (columns, then rows), once for the inside and once for the
//! outside. The signed result is stored in one byte per pixel over
//! `+-range` logical px, exactly as the GPU reads it from a texture, so the
//! CPU and the GPU bevel the same shape.

use azul_css::{props::layout::shape::ClipPath, shape::CssShape};

/// A shape's signed distance field: negative inside, positive outside,
/// logical px, one byte per device pixel.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ShapeField {
    /// The field's size, device px (the surface's).
    pub width: usize,
    /// See [`Self::width`].
    pub height: usize,
    /// Device px per logical px, as bits (the field is only valid at this
    /// scale; `Hash` needs no float).
    dpr_bits: u32,
    /// The logical px the byte range spans each way: 0 is `-range`, 255
    /// `+range`, 127.5 the edge. Bits, as `dpr_bits`.
    range_bits: u32,
    /// The bytes, row by row from the top.
    pub data: Vec<u8>,
}

impl ShapeField {
    /// The field of the shape `inside(x, y)` answers for each device pixel
    /// (its centre) of a `width` x `height` mask at `dpr`, over a range of 8
    /// logical px.
    #[must_use]
    pub fn from_mask(
        width: usize,
        height: usize,
        dpr: f32,
        inside: impl Fn(usize, usize) -> bool,
    ) -> Self {
        Self::from_mask_with_range(width, height, dpr, 8.0, inside)
    }

    /// [`Self::from_mask`] over `range` logical px each way (a bevel or a
    /// pillow wider than 8 px needs the distance that far in).
    #[must_use]
    pub fn from_mask_with_range(
        width: usize,
        height: usize,
        dpr: f32,
        range: f32,
        inside: impl Fn(usize, usize) -> bool,
    ) -> Self {
        let dpr = if dpr.is_finite() && dpr > 0.0 { dpr } else { 1.0 };
        let range = range.max(1.0);
        let mut mask = vec![false; width * height];
        for y in 0..height {
            for x in 0..width {
                mask[y * width + x] = inside(x, y);
            }
        }
        let to_outside = squared_distances(&mask, width, height, true);
        let to_inside = squared_distances(&mask, width, height, false);
        let data = (0..width * height)
            .map(|i| {
                // The edge lies halfway between the last pixel inside and the
                // first outside: half a pixel short of the centres' distance.
                let device = if mask[i] {
                    -(to_outside[i].sqrt() - 0.5)
                } else {
                    to_inside[i].sqrt() - 0.5
                };
                let logical = device as f32 / dpr;
                (((logical / (2.0 * range)) + 0.5) * 255.0).round().clamp(0.0, 255.0) as u8
            })
            .collect();
        Self {
            width,
            height,
            dpr_bits: dpr.to_bits(),
            range_bits: range.to_bits(),
            data,
        }
    }

    /// The field of `clip_path` for a `width` x `height` (logical px) box at
    /// `dpr`, reaching `range` logical px each way. `None` for no clip path
    /// and for a path (`path(..)`), which is not drawn here.
    #[must_use]
    pub fn from_clip_path_with_range(
        clip_path: &ClipPath,
        width: f32,
        height: f32,
        dpr: f32,
        range: f32,
    ) -> Option<Self> {
        let ClipPath::Shape(shape) = clip_path else {
            return None;
        };
        let (w, h) = (
            (width * dpr).round().max(1.0) as usize,
            (height * dpr).round().max(1.0) as usize,
        );
        let at = |x: usize, y: usize| ((x as f32 + 0.5) / dpr, (y as f32 + 0.5) / dpr);
        Some(match shape {
            CssShape::Circle(c) => Self::from_mask_with_range(w, h, dpr, range, |x, y| {
                let (px, py) = at(x, y);
                (px - c.center.x).hypot(py - c.center.y) < c.radius
            }),
            CssShape::Ellipse(e) => Self::from_mask_with_range(w, h, dpr, range, |x, y| {
                let (px, py) = at(x, y);
                let (dx, dy) = (
                    (px - e.center.x) / e.radius_x.max(1e-3),
                    (py - e.center.y) / e.radius_y.max(1e-3),
                );
                dx * dx + dy * dy < 1.0
            }),
            CssShape::Polygon(polygon) => {
                let points: Vec<(f32, f32)> =
                    polygon.points.as_ref().iter().map(|p| (p.x, p.y)).collect();
                Self::from_mask_with_range(w, h, dpr, range, |x, y| {
                    let (px, py) = at(x, y);
                    inside_polygon(&points, px, py)
                })
            }
            CssShape::Inset(inset) => {
                let r = match inset.border_radius {
                    azul_css::corety::OptionF32::Some(r) => r.max(0.0),
                    azul_css::corety::OptionF32::None => 0.0,
                };
                let (x0, y0) = (inset.inset_left, inset.inset_top);
                let (x1, y1) = (width - inset.inset_right, height - inset.inset_bottom);
                Self::from_mask_with_range(w, h, dpr, range, |x, y| {
                    let (px, py) = at(x, y);
                    if px < x0 || px >= x1 || py < y0 || py >= y1 {
                        return false;
                    }
                    // Inside the corner squares, inside the corner's circle.
                    let cx = px.clamp(x0 + r, (x1 - r).max(x0 + r));
                    let cy = py.clamp(y0 + r, (y1 - r).max(y0 + r));
                    (px - cx).hypot(py - cy) <= r
                })
            }
            CssShape::Path(_) => return None,
        })
    }

    /// [`Self::from_clip_path_with_range`] over 8 logical px.
    #[must_use]
    pub fn from_clip_path(clip_path: &ClipPath, width: f32, height: f32, dpr: f32) -> Option<Self> {
        Self::from_clip_path_with_range(clip_path, width, height, dpr, 8.0)
    }

    /// Device px per logical px the field was made at.
    #[must_use]
    pub const fn dpr(&self) -> f32 {
        f32::from_bits(self.dpr_bits)
    }

    /// The logical px the byte range spans each way.
    #[must_use]
    pub const fn range(&self) -> f32 {
        f32::from_bits(self.range_bits)
    }

    /// The signed distance at logical point (`px`, `py`) of the box, logical
    /// px: the bytes bilinearly blended at texel centres, clamped to the
    /// edge - what a GPU's linear texture filter returns.
    #[must_use]
    pub fn distance_at(&self, px: f32, py: f32) -> f32 {
        if self.width == 0 || self.height == 0 {
            return self.range();
        }
        let dpr = self.dpr();
        let u = px * dpr - 0.5;
        let v = py * dpr - 0.5;
        let (fx, fy) = (u.floor(), v.floor());
        let (tx, ty) = (u - fx, v - fy);
        let clamp_x = |i: f32| (i.max(0.0) as usize).min(self.width - 1);
        let clamp_y = |i: f32| (i.max(0.0) as usize).min(self.height - 1);
        let (x0, x1) = (clamp_x(fx), clamp_x(fx + 1.0));
        let (y0, y1) = (clamp_y(fy), clamp_y(fy + 1.0));
        let at = |x: usize, y: usize| f32::from(self.data[y * self.width + x]) / 255.0;
        let top = at(x0, y0) * (1.0 - tx) + at(x1, y0) * tx;
        let bottom = at(x0, y1) * (1.0 - tx) + at(x1, y1) * tx;
        let q = top * (1.0 - ty) + bottom * ty;
        (q - 0.5) * 2.0 * self.range()
    }
}

/// The even-odd rule: whether (`px`, `py`) lies inside the polygon.
fn inside_polygon(points: &[(f32, f32)], px: f32, py: f32) -> bool {
    let mut inside = false;
    let n = points.len();
    if n < 3 {
        return false;
    }
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = points[i];
        let (xj, yj) = points[j];
        if (yi > py) != (yj > py) && px < (xj - xi) * (py - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// For every pixel, the squared distance (device px) to the nearest pixel
/// centre on the OTHER side: with `from_inside`, from each inside pixel to
/// the nearest outside one; otherwise from each outside pixel to the
/// nearest inside one. Zero on the far side itself.
fn squared_distances(mask: &[bool], width: usize, height: usize, from_inside: bool) -> Vec<f64> {
    const FAR: f64 = 1e12;
    let mut f: Vec<f64> = mask
        .iter()
        .map(|&inside| if inside == from_inside { FAR } else { 0.0 })
        .collect();
    let longest = width.max(height);
    let mut line = LineTransform::new(longest);
    let mut column = vec![0.0; height];
    for x in 0..width {
        for y in 0..height {
            column[y] = f[y * width + x];
        }
        line.run(&mut column);
        for y in 0..height {
            f[y * width + x] = column[y];
        }
    }
    for y in 0..height {
        line.run(&mut f[y * width..(y + 1) * width]);
    }
    f
}

/// Felzenszwalb and Huttenlocher's one-dimensional squared distance
/// transform: the lower envelope of the parabolas rooted at each sample,
/// in linear time. The buffers are kept between lines.
struct LineTransform {
    d: Vec<f64>,
    v: Vec<usize>,
    z: Vec<f64>,
}

impl LineTransform {
    fn new(longest: usize) -> Self {
        Self {
            d: vec![0.0; longest],
            v: vec![0; longest],
            z: vec![0.0; longest + 1],
        }
    }

    /// Replace `f` with its distance transform.
    // The lower envelope's walk (Felzenszwalb-Huttenlocher) in the paper's
    // letters: `z` holds finite parabola crossings and ends in FAR, so the
    // float loop ends.
    #[allow(clippy::while_float, clippy::many_single_char_names)]
    fn run(&mut self, f: &mut [f64]) {
        const FAR: f64 = 1e12;
        let n = f.len();
        if n == 0 {
            return;
        }
        let (d, v, z) = (&mut self.d, &mut self.v, &mut self.z);
        let mut k = 0_usize;
        v[0] = 0;
        z[0] = -FAR;
        z[1] = FAR;
        for q in 1..n {
            let qf = q as f64;
            let mut s;
            loop {
                let r = v[k];
                let rf = r as f64;
                s = (f[q] + qf * qf - (f[r] + rf * rf)) / (2.0 * qf - 2.0 * rf);
                if s <= z[k] && k > 0 {
                    k -= 1;
                } else {
                    break;
                }
            }
            // `k` may be 0 with `s <= z[0]` only when z[0] is -FAR: never.
            k += 1;
            v[k] = q;
            z[k] = s;
            z[k + 1] = FAR;
        }
        k = 0;
        for q in 0..n {
            let qf = q as f64;
            while z[k + 1] < qf {
                k += 1;
            }
            let r = v[k] as f64;
            d[q] = (qf - r) * (qf - r) + f[v[k]];
        }
        f.copy_from_slice(&d[..n]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_line_transform_is_the_brute_force_minimum() {
        let mut line = LineTransform::new(16);
        let f0 = [1e12, 1e12, 0.0, 1e12, 1e12, 1e12, 1e12, 0.0, 1e12, 4.0, 1e12, 1e12];
        let mut f = f0;
        line.run(&mut f);
        for q in 0..f0.len() {
            let brute = (0..f0.len())
                .map(|p| (q as f64 - p as f64).powi(2) + f0[p])
                .fold(f64::MAX, f64::min);
            assert!((f[q] - brute).abs() < 1e-6, "at {q}: {} vs {brute}", f[q]);
        }
    }

    #[test]
    fn a_square_s_field_is_negative_inside_and_grows_outward() {
        let field = ShapeField::from_mask(40, 40, 1.0, |x, y| (10..30).contains(&x) && (10..30).contains(&y));
        assert!(field.distance_at(20.0, 20.0) < -7.5, "deep inside, as far as the range reaches");
        assert!(field.distance_at(2.0, 20.0) > 7.0, "far outside");
        let edge = field.distance_at(10.0, 20.0);
        assert!(edge.abs() < 0.6, "on the edge: {edge}");
    }
}
