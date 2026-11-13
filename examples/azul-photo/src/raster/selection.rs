//! Selections: an 8-bit coverage mask over the document (255 = selected).
//!
//! Shapes rasterise anti-aliased (an ellipse and a lasso polygon cover their
//! exact area at 4 sub-rows per pixel); masks combine (replace, add, subtract,
//! intersect), invert, feather (a Gaussian on the mask) and come from the
//! magic wand (a flood fill with a colour tolerance).

use std::fmt;

use super::{filter::blur_plane, geom::IRect, tile::TileGrid};

/// One coverage byte per document pixel, row by row.
#[derive(Clone, PartialEq, Eq)]
pub struct Mask {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

impl fmt::Debug for Mask {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Mask({}x{}, bounds {:?})", self.width, self.height, self.bounds())
    }
}

/// How a new selection meets the current one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectMode {
    Replace,
    Add,
    Subtract,
    Intersect,
}

impl SelectMode {
    pub const ALL: [Self; 4] = [Self::Replace, Self::Add, Self::Subtract, Self::Intersect];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Replace => "New",
            Self::Add => "Add",
            Self::Subtract => "Subtract",
            Self::Intersect => "Intersect",
        }
    }
}

/// A selection shape in document pixels.
#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    /// Whole pixels.
    Rect(IRect),
    /// The ellipse inscribed in the rect.
    Ellipse(IRect),
    /// A closed polygon (the lasso), even-odd filled.
    Polygon(Vec<(f32, f32)>),
}

impl Shape {
    /// The pixels the shape can touch.
    #[must_use]
    pub fn bounds(&self) -> IRect {
        match self {
            Self::Rect(r) | Self::Ellipse(r) => *r,
            Self::Polygon(points) => {
                let mut x0 = f32::MAX;
                let mut y0 = f32::MAX;
                let mut x1 = f32::MIN;
                let mut y1 = f32::MIN;
                for (x, y) in points {
                    x0 = x0.min(*x);
                    y0 = y0.min(*y);
                    x1 = x1.max(*x);
                    y1 = y1.max(*y);
                }
                if points.is_empty() {
                    IRect::default()
                } else {
                    IRect::covering(x0, y0, x1, y1)
                }
            }
        }
    }
}

const SUB: usize = 4;

impl Mask {
    /// Nothing selected.
    #[must_use]
    pub fn empty(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            data: vec![0; (width as usize) * (height as usize)],
        }
    }

    /// Everything selected.
    #[must_use]
    pub fn full(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            data: vec![255; (width as usize) * (height as usize)],
        }
    }

    /// The coverage at (`x`, `y`); 0 outside.
    #[inline]
    #[must_use]
    pub fn get(&self, x: u32, y: u32) -> u8 {
        if x >= self.width || y >= self.height {
            return 0;
        }
        self.data[(y * self.width + x) as usize]
    }

    /// The coverage at (`x`, `y`) in 0..1.
    #[inline]
    #[must_use]
    pub fn coverage(&self, x: u32, y: u32) -> f32 {
        f32::from(self.get(x, y)) / 255.0
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.data.iter().all(|v| *v == 0)
    }

    /// The smallest rect holding every selected pixel.
    #[must_use]
    pub fn bounds(&self) -> Option<IRect> {
        let w = self.width as usize;
        let mut x0 = usize::MAX;
        let mut y0 = usize::MAX;
        let mut x1 = 0;
        let mut y1 = 0;
        for (y, row) in self.data.chunks_exact(w.max(1)).enumerate() {
            if let Some(first) = row.iter().position(|v| *v > 0) {
                let last = row.iter().rposition(|v| *v > 0).unwrap_or(first);
                x0 = x0.min(first);
                x1 = x1.max(last + 1);
                y0 = y0.min(y);
                y1 = y + 1;
            }
        }
        (x0 != usize::MAX).then(|| IRect::new(x0 as i32, y0 as i32, (x1 - x0) as i32, (y1 - y0) as i32))
    }

    /// The shape as a mask.
    #[must_use]
    pub fn from_shape(width: u32, height: u32, shape: &Shape) -> Self {
        let mut mask = Self::empty(width, height);
        match shape {
            Shape::Rect(r) => {
                if let Some(r) = r.intersect(&IRect::new(0, 0, width as i32, height as i32)) {
                    for y in r.y..r.bottom() {
                        let row = (y as usize) * (width as usize);
                        mask.data[row + r.x as usize..row + r.right() as usize].fill(255);
                    }
                }
            }
            Shape::Ellipse(r) => {
                if r.is_empty() {
                    return mask;
                }
                let rx = r.w as f32 / 2.0;
                let ry = r.h as f32 / 2.0;
                let cx = r.x as f32 + rx;
                let cy = r.y as f32 + ry;
                // An ellipse is a polygon with enough sides for sub-pixel accuracy.
                let n = ((rx + ry) * 2.0).clamp(32.0, 4096.0) as usize;
                let points: Vec<(f32, f32)> = (0..n)
                    .map(|i| {
                        let t = i as f32 / n as f32 * std::f32::consts::TAU;
                        (cx + rx * t.cos(), cy + ry * t.sin())
                    })
                    .collect();
                mask.fill_polygon(&points);
            }
            Shape::Polygon(points) => mask.fill_polygon(points),
        }
        mask
    }

    /// Rasterise an even-odd polygon: `SUB` sub-rows per pixel row, exact
    /// horizontal coverage per span.
    fn fill_polygon(&mut self, points: &[(f32, f32)]) {
        if points.len() < 3 {
            return;
        }
        let (w, h) = (self.width as usize, self.height as usize);
        let b = Shape::Polygon(points.to_vec())
            .bounds()
            .intersect(&IRect::new(0, 0, w as i32, h as i32));
        let Some(b) = b else {
            return;
        };
        let edges: Vec<((f32, f32), (f32, f32))> = points
            .iter()
            .zip(points.iter().cycle().skip(1))
            .map(|(a, b)| (*a, *b))
            .filter(|(a, b)| (a.1 - b.1).abs() > f32::EPSILON)
            .collect();
        let mut acc = vec![0.0f32; w];
        let mut xs: Vec<f32> = Vec::new();
        for y in b.y..b.bottom() {
            acc[b.x as usize..b.right() as usize].fill(0.0);
            for s in 0..SUB {
                let sy = y as f32 + (s as f32 + 0.5) / SUB as f32;
                xs.clear();
                for ((x0, y0), (x1, y1)) in &edges {
                    let (lo, hi) = if y0 < y1 { (*y0, *y1) } else { (*y1, *y0) };
                    if sy >= lo && sy < hi {
                        xs.push(x0 + (sy - y0) * (x1 - x0) / (y1 - y0));
                    }
                }
                xs.sort_by(f32::total_cmp);
                for pair in xs.chunks_exact(2) {
                    let xa = pair[0].clamp(0.0, w as f32);
                    let xb = pair[1].clamp(0.0, w as f32);
                    add_span(&mut acc, xa, xb, 1.0 / SUB as f32);
                }
            }
            let row = (y as usize) * w;
            for x in b.x as usize..b.right() as usize {
                self.data[row + x] = (acc[x].clamp(0.0, 1.0) * 255.0).round() as u8;
            }
        }
    }

    /// This selection met with `other` in `mode`.
    #[must_use]
    pub fn combine(&self, other: &Self, mode: SelectMode) -> Self {
        let mut out = self.clone();
        match mode {
            SelectMode::Replace => return other.clone(),
            SelectMode::Add => {
                for (a, b) in out.data.iter_mut().zip(&other.data) {
                    *a = (*a).max(*b);
                }
            }
            SelectMode::Subtract => {
                for (a, b) in out.data.iter_mut().zip(&other.data) {
                    *a = ((u32::from(*a) * u32::from(255 - *b) + 127) / 255) as u8;
                }
            }
            SelectMode::Intersect => {
                for (a, b) in out.data.iter_mut().zip(&other.data) {
                    *a = (*a).min(*b);
                }
            }
        }
        out
    }

    /// Everything that was not selected.
    #[must_use]
    pub fn invert(&self) -> Self {
        Self {
            width: self.width,
            height: self.height,
            data: self.data.iter().map(|v| 255 - v).collect(),
        }
    }

    /// The selection with a soft edge `radius` pixels wide (a Gaussian with
    /// sigma = radius / 2).
    #[must_use]
    pub fn feather(&self, radius: f32) -> Self {
        if radius <= 0.0 {
            return self.clone();
        }
        let mut plane: Vec<f32> = self.data.iter().map(|v| f32::from(*v)).collect();
        blur_plane(&mut plane, self.width as usize, self.height as usize, radius / 2.0);
        Self {
            width: self.width,
            height: self.height,
            data: plane.iter().map(|v| v.round().clamp(0.0, 255.0) as u8).collect(),
        }
    }

    /// The magic wand at (`x`, `y`): every pixel whose channels all differ
    /// from the clicked one by at most `tolerance`; with `contiguous`, only
    /// those connected to it (4-neighbourhood).
    #[must_use]
    pub fn magic_wand(grid: &TileGrid, x: u32, y: u32, tolerance: u8, contiguous: bool) -> Self {
        let (w, h) = (grid.width(), grid.height());
        let mut mask = Self::empty(w, h);
        if x >= w || y >= h {
            return mask;
        }
        let rgba = grid.to_rgba();
        let seed = grid.pixel(x, y);
        let matches = |i: usize| -> bool {
            let p = &rgba[i * 4..i * 4 + 4];
            p.iter()
                .zip(seed.iter())
                .all(|(a, b)| a.abs_diff(*b) <= tolerance)
        };
        if !contiguous {
            for i in 0..(w as usize * h as usize) {
                if matches(i) {
                    mask.data[i] = 255;
                }
            }
            return mask;
        }
        let w = w as usize;
        let h = h as usize;
        let mut stack = vec![(y as usize) * w + x as usize];
        mask.data[stack[0]] = 255;
        while let Some(i) = stack.pop() {
            let (px, py) = (i % w, i / w);
            let mut visit = |j: usize, mask: &mut Self| {
                if mask.data[j] == 0 && matches(j) {
                    mask.data[j] = 255;
                    stack.push(j);
                }
            };
            if px > 0 {
                visit(i - 1, &mut mask);
            }
            if px + 1 < w {
                visit(i + 1, &mut mask);
            }
            if py > 0 {
                visit(i - w, &mut mask);
            }
            if py + 1 < h {
                visit(i + w, &mut mask);
            }
        }
        mask
    }

    /// The mask cut to `r` and moved so `r`'s corner is the origin (crop).
    #[must_use]
    pub fn crop(&self, r: IRect) -> Self {
        let mut out = Self::empty(r.w.max(0) as u32, r.h.max(0) as u32);
        for y in 0..out.height {
            for x in 0..out.width {
                let sx = x as i32 + r.x;
                let sy = y as i32 + r.y;
                if sx >= 0 && sy >= 0 {
                    out.data[(y * out.width + x) as usize] = self.get(sx as u32, sy as u32);
                }
            }
        }
        out
    }
}

/// Add `weight` x the covered fraction of each pixel in `[xa, xb)`.
fn add_span(acc: &mut [f32], xa: f32, xb: f32, weight: f32) {
    if xb <= xa {
        return;
    }
    let first = xa.floor() as usize;
    let last = (xb.ceil() as usize).min(acc.len());
    for (px, cell) in acc.iter_mut().enumerate().take(last).skip(first) {
        let lo = (px as f32).max(xa);
        let hi = (px as f32 + 1.0).min(xb);
        if hi > lo {
            *cell += (hi - lo) * weight;
        }
    }
}
