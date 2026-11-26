//! The picture of a cutout: pixels put together on the CPU, and the plan of what is drawn at which
//! scale. The pages' code is drawn by azul's CPU renderer (`whiteboard_render`, on a Thread); what
//! this file does is everything round it, with no window and no azul: which part of the page is
//! drawn and at what scale ([`plan`]), the layers in the order the page shows them ([`compose`]:
//! paper, the marker's ink, the code, the pen's ink), and the cut along the lasso with a soft edge
//! ([`Raster::mask_polygon`]).
//!
//! A [`Raster`] holds colours multiplied by their alpha (what the CPU renderer produces and the engine
//! shows); a PNG holds them straight, so [`Raster::to_straight`] and [`Raster::from_straight`] are the
//! way in and out of one.

use std::sync::Arc;

use crate::{
    whiteboard::{CutoutImage, Limits, Notice},
    whiteboard_geom::{polygon_area, Pt, Rect, Size},
};

/// How far round the lasso's box the picture reaches, in board px: the edge of the cut is soft, and the
/// ink of the lasso itself is half outside its outline.
pub const PAD: f32 = 3.0;
/// The scales a picture is drawn at: a screen's, never less than half, never more than four times.
pub const MIN_SCALE: f32 = 0.5;
pub const MAX_SCALE: f32 = 4.0;
/// How soft the cut's edge is, device px.
pub const FEATHER: f32 = 1.0;
/// How many rows of samples a pixel of the cut is measured on.
const SUB_ROWS: usize = 8;

/// A picture: `w` x `h` pixels of four bytes (red, green, blue, alpha), the colours multiplied by the
/// alpha, rows from the top.
#[derive(Debug, Clone, PartialEq)]
pub struct Raster {
    pub w: u32,
    pub h: u32,
    pub px: Vec<u8>,
}

fn mul255(c: u8, a: u8) -> u8 {
    ((u16::from(c) * u16::from(a) + 127) / 255) as u8
}

impl Raster {
    /// A clear picture.
    #[must_use]
    pub fn new(w: u32, h: u32) -> Self {
        Self { w, h, px: vec![0; w as usize * h as usize * 4] }
    }

    /// A picture of one colour (`rgba` straight, as a person says a colour).
    #[must_use]
    pub fn filled(w: u32, h: u32, rgba: [u8; 4]) -> Self {
        let one = [mul255(rgba[0], rgba[3]), mul255(rgba[1], rgba[3]), mul255(rgba[2], rgba[3]), rgba[3]];
        let mut px = Vec::with_capacity(w as usize * h as usize * 4);
        for _ in 0..(w as usize * h as usize) {
            px.extend_from_slice(&one);
        }
        Self { w, h, px }
    }

    /// The pictures `px` makes when its colours are already multiplied by their alpha (`None` when the
    /// bytes are not `w x h` pixels, or there are none).
    #[must_use]
    pub fn from_premultiplied(w: u32, h: u32, px: Vec<u8>) -> Option<Self> {
        (w > 0 && h > 0 && px.len() as u64 == u64::from(w) * u64::from(h) * 4).then_some(Self { w, h, px })
    }

    /// The picture `px` makes when its colours are straight (a PNG's), multiplied here.
    #[must_use]
    pub fn from_straight(w: u32, h: u32, mut px: Vec<u8>) -> Option<Self> {
        if w == 0 || h == 0 || px.len() as u64 != u64::from(w) * u64::from(h) * 4 {
            return None;
        }
        for p in px.chunks_exact_mut(4) {
            let a = p[3];
            p[0] = mul255(p[0], a);
            p[1] = mul255(p[1], a);
            p[2] = mul255(p[2], a);
        }
        Some(Self { w, h, px })
    }

    /// The bytes with the colours straight (for a PNG): divided by the alpha again.
    #[must_use]
    pub fn to_straight(&self) -> Vec<u8> {
        let mut out = self.px.clone();
        for p in out.chunks_exact_mut(4) {
            let a = u32::from(p[3]);
            if a == 0 {
                p[0] = 0;
                p[1] = 0;
                p[2] = 0;
            } else if a < 255 {
                for c in &mut p[..3] {
                    *c = ((u32::from(*c) * 255 + a / 2) / a).min(255) as u8;
                }
            }
        }
        out
    }

    fn at(&self, x: u32, y: u32) -> usize {
        (y as usize * self.w as usize + x as usize) * 4
    }

    /// The pixel at (`x`, `y`) (clear outside the picture).
    #[must_use]
    pub fn get(&self, x: u32, y: u32) -> [u8; 4] {
        if x >= self.w || y >= self.h {
            return [0; 4];
        }
        let i = self.at(x, y);
        [self.px[i], self.px[i + 1], self.px[i + 2], self.px[i + 3]]
    }

    /// Puts `src` over this picture with its top-left corner at (`dx`, `dy`), clipped to it.
    pub fn over(&mut self, src: &Self, dx: i32, dy: i32) {
        for sy in 0..src.h {
            let y = i64::from(sy) + i64::from(dy);
            if y < 0 || y >= i64::from(self.h) {
                continue;
            }
            for sx in 0..src.w {
                let x = i64::from(sx) + i64::from(dx);
                if x < 0 || x >= i64::from(self.w) {
                    continue;
                }
                let s = src.at(sx, sy);
                let sa = src.px[s + 3];
                if sa == 0 {
                    continue;
                }
                let d = self.at(x as u32, y as u32);
                let keep = 255 - sa;
                for k in 0..3 {
                    self.px[d + k] = src.px[s + k].saturating_add(mul255(self.px[d + k], keep));
                }
                self.px[d + 3] = sa.saturating_add(mul255(self.px[d + 3], keep));
            }
        }
    }

    /// The part at (`x`, `y`), `w` x `h`, clipped to the picture (an empty picture when none of it is in).
    #[must_use]
    pub fn cropped(&self, x: u32, y: u32, w: u32, h: u32) -> Self {
        if x >= self.w || y >= self.h {
            return Self { w: 0, h: 0, px: Vec::new() };
        }
        let (w, h) = (w.min(self.w - x), h.min(self.h - y));
        let mut px = Vec::with_capacity(w as usize * h as usize * 4);
        for row in 0..h {
            let start = self.at(x, y + row);
            px.extend_from_slice(&self.px[start..start + w as usize * 4]);
        }
        Self { w, h, px }
    }

    /// The box of what is not clear: (`x`, `y`, `w`, `h`), `None` for a clear picture.
    #[must_use]
    pub fn alpha_box(&self) -> Option<(u32, u32, u32, u32)> {
        let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
        for y in 0..self.h {
            for x in 0..self.w {
                if self.px[self.at(x, y) + 3] != 0 {
                    x0 = x0.min(x);
                    y0 = y0.min(y);
                    x1 = x1.max(x);
                    y1 = y1.max(y);
                }
            }
        }
        (x0 != u32::MAX).then(|| (x0, y0, x1 - x0 + 1, y1 - y0 + 1))
    }

    /// Cuts the picture along `polygon` (in its pixels): what is outside goes clear, the edge is soft
    /// by `feather` px. No outline leaves it as it is.
    pub fn mask_polygon(&mut self, polygon: &[Pt], feather: f32) {
        if polygon.len() < 3 || self.w == 0 || self.h == 0 {
            return;
        }
        let (w, h) = (self.w as usize, self.h as usize);
        let mut coverage = coverage_of(polygon, w, h);
        let radius = if feather.is_finite() { feather.round().max(0.0) as usize } else { 0 };
        if radius > 0 {
            blur(&mut coverage, w, h, radius);
        }
        for (p, c) in self.px.chunks_exact_mut(4).zip(&coverage) {
            let c = (c.clamp(0.0, 1.0) * 255.0).round() as u8;
            for v in p.iter_mut() {
                *v = mul255(*v, c);
            }
        }
    }

    /// The picture as what a board keeps: its pixels, the scale it was drawn at and its PNG.
    #[must_use]
    pub fn into_image(self, scale: f32, png: Vec<u8>) -> CutoutImage {
        CutoutImage { width: self.w, height: self.h, scale, rgba: self.px, png }
    }

    /// The same, shared.
    #[must_use]
    pub fn into_shared_image(self, scale: f32, png: Vec<u8>) -> Arc<CutoutImage> {
        Arc::new(self.into_image(scale, png))
    }
}

/// How much of each pixel the polygon covers, 0 to 1 (even-odd, `SUB_ROWS` samples down and exact
/// across).
fn coverage_of(polygon: &[Pt], w: usize, h: usize) -> Vec<f32> {
    let mut cov = vec![0.0_f32; w * h];
    let mut xs: Vec<f32> = Vec::new();
    let n = polygon.len();
    for row in 0..h {
        for s in 0..SUB_ROWS {
            let y = row as f32 + (s as f32 + 0.5) / SUB_ROWS as f32;
            xs.clear();
            for i in 0..n {
                let (a, b) = (polygon[i], polygon[(i + 1) % n]);
                if (a.y <= y) != (b.y <= y) {
                    xs.push(a.x + (y - a.y) * (b.x - a.x) / (b.y - a.y));
                }
            }
            xs.sort_by(f32::total_cmp);
            for pair in xs.chunks_exact(2) {
                let (x0, x1) = (pair[0].clamp(0.0, w as f32), pair[1].clamp(0.0, w as f32));
                if x1 <= x0 {
                    continue;
                }
                for xi in (x0.floor() as usize)..(x1.ceil() as usize).min(w) {
                    let (left, right) = ((xi as f32).max(x0), ((xi + 1) as f32).min(x1));
                    cov[row * w + xi] += (right - left).max(0.0) / SUB_ROWS as f32;
                }
            }
        }
    }
    cov
}

/// A box blur of `radius` px in both directions (the edge of the picture is repeated: a cut that leaves
/// the picture stays whole at its edge).
fn blur(v: &mut [f32], w: usize, h: usize, radius: usize) {
    let r = radius as isize;
    let width = (2 * radius + 1) as f32;
    let mut tmp = vec![0.0_f32; v.len()];
    for y in 0..h {
        for x in 0..w {
            let sum: f32 = (-r..=r).map(|k| v[y * w + (x as isize + k).clamp(0, w as isize - 1) as usize]).sum();
            tmp[y * w + x] = sum / width;
        }
    }
    for y in 0..h {
        for x in 0..w {
            let sum: f32 = (-r..=r).map(|k| tmp[(y as isize + k).clamp(0, h as isize - 1) as usize * w + x]).sum();
            v[y * w + x] = sum / width;
        }
    }
}

/// What is drawn for a cutout, where and at which scale.
#[derive(Debug, Clone, PartialEq)]
pub struct CapturePlan {
    /// Device pixels per board px that the picture is drawn at (the screen's, or less for a large cutout).
    pub scale: f32,
    /// The part of the page that is drawn (the lasso's box and [`PAD`] round it, on the page, on the grid of
    /// device pixels: the text falls on the pixels it falls on in the window).
    pub region: Rect,
    /// The picture's size in pixels.
    pub px_w: u32,
    pub px_h: u32,
    /// The lasso in the picture's pixels.
    pub outline: Vec<Pt>,
    /// The picture's size in board px (what the cutout is shown at).
    pub logical: Size,
    /// What the person is told when the scale had to be lowered.
    pub notice: Option<Notice>,
}

/// Rounds `v` down to a multiple of `1 / scale`, so it falls on a device pixel.
fn grid_floor(v: f32, scale: f32) -> f32 {
    (v * scale).floor() / scale
}

fn grid_ceil(v: f32, scale: f32) -> f32 {
    (v * scale).ceil() / scale
}

/// The plan for the loop `outline` (in the page's px) on a page of `page`, for a screen of `dpr` device
/// px per px. When the picture would have more pixels than `limits` allow, it is drawn at a lower scale
/// (the cutout keeps its size, only its sharpness is less) and the plan says so.
///
/// # Errors
/// There is no outline, or no scale of at least [`MIN_SCALE`] makes the picture fit.
pub fn plan(outline: &[Pt], dpr: f32, page: Size, limits: &Limits) -> Result<CapturePlan, Notice> {
    let Some(bounds) = Rect::from_points(outline) else {
        return Err(Notice::PictureInvalid);
    };
    if outline.len() < 3 || polygon_area(outline).abs() <= 0.0 {
        return Err(Notice::PictureInvalid);
    }
    let wanted = if dpr.is_finite() { dpr.clamp(MIN_SCALE, MAX_SCALE) } else { 1.0 };
    let padded = bounds.inflate(PAD);
    let on_page = Rect::new(0.0, 0.0, page.w.max(1.0), page.h.max(1.0));
    let (x0, y0) = (padded.x.max(on_page.x), padded.y.max(on_page.y));
    let (x1, y1) = (padded.right().min(on_page.right()), padded.bottom().min(on_page.bottom()));
    let drawn = |scale: f32| -> (Rect, u32, u32) {
        let region = Rect::from_corners(
            Pt::new(grid_floor(x0, scale), grid_floor(y0, scale)),
            Pt::new(grid_ceil(x1, scale), grid_ceil(y1, scale)),
        );
        let w = (region.w * scale).round().max(1.0) as u32;
        let h = (region.h * scale).round().max(1.0) as u32;
        (region, w, h)
    };
    let mut scale = wanted;
    let (mut region, mut px_w, mut px_h) = drawn(scale);
    let max = limits.max_image_px;
    if u64::from(px_w) * u64::from(px_h) > max {
        // The scale at which the picture has as many pixels as may be, rounded down to a twentieth.
        let k = (max as f32 / (px_w as f32 * px_h as f32)).sqrt();
        scale = ((scale * k * 20.0).floor() / 20.0).max(0.05);
        (region, px_w, px_h) = drawn(scale);
        while scale >= MIN_SCALE && u64::from(px_w) * u64::from(px_h) > max {
            scale -= 0.05;
            (region, px_w, px_h) = drawn(scale);
        }
        if scale < MIN_SCALE {
            return Err(Notice::PictureTooBig { px: u64::from(px_w) * u64::from(px_h), limit_px: max });
        }
    }
    let notice = (scale < wanted - 1e-4).then(|| Notice::ScaleLowered {
        wanted_pct: (wanted * 100.0).round() as u32,
        used_pct: (scale * 100.0).round() as u32,
    });
    let outline_px = outline.iter().map(|p| Pt::new((p.x - region.x) * scale, (p.y - region.y) * scale)).collect();
    Ok(CapturePlan {
        scale,
        region,
        px_w,
        px_h,
        outline: outline_px,
        logical: Size::new(px_w as f32 / scale, px_h as f32 / scale),
        notice,
    })
}

/// The cutout's picture from its layers, each `plan.px_w` x `plan.px_h`: the paper, the marker's ink
/// under the code, the code (drawn with a clear ground), the pen's ink over it, then cut along the
/// lasso with a soft edge.
///
/// # Errors
/// A layer is not the size of the plan.
pub fn compose(
    plan: &CapturePlan,
    paper: [u8; 4],
    marker: Option<&Raster>,
    code: &Raster,
    pen: Option<&Raster>,
) -> Result<Raster, String> {
    for (name, layer) in [("marker", marker), ("code", Some(code)), ("pen", pen)] {
        if let Some(l) = layer {
            if (l.w, l.h) != (plan.px_w, plan.px_h) {
                return Err(format!(
                    "the {name} layer is {} x {} pixels, the picture {} x {}",
                    l.w, l.h, plan.px_w, plan.px_h
                ));
            }
        }
    }
    let mut picture = Raster::filled(plan.px_w, plan.px_h, paper);
    if let Some(m) = marker {
        picture.over(m, 0, 0);
    }
    picture.over(code, 0, 0);
    if let Some(p) = pen {
        picture.over(p, 0, 0);
    }
    picture.mask_polygon(&plan.outline, FEATHER);
    Ok(picture)
}

#[cfg(test)]
#[path = "whiteboard_cutout_test.rs"]
mod whiteboard_cutout_test;
