//! The brush engine: brush, pencil, eraser and clone stamp.
//!
//! A stroke stamps dabs along the pointer path every `spacing x size`
//! pixels. Each dab adds to the stroke's own coverage mask
//! (`m += (1 - m) x flow x dab`), and the layer's pixels under the dab are
//! recomputed from the layer AS IT WAS when the stroke began, at
//! `opacity x m x selection`: so `flow` builds up within a stroke while one
//! stroke never paints past its `opacity` (Photoshop's model).
//!
//! The soft dab profile is azul's own (`RawImage::paint_dot`, the one brush
//! profile in azul, `brush_dab_coverage`): one stamp per radius, sampled
//! bilinearly at the dab's sub-pixel position. The pencil is a hard disc.

use std::collections::HashMap;

use azul::{
    image::{Brush, RawImage, RawImageData},
    prelude::ColorU,
};

use super::{
    blend,
    geom::IRect,
    layer::LayerId,
    selection::Mask,
    tile::{TileGrid, TILE, TILE_PIXELS},
};

/// What a stroke does to the layer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BrushTool {
    Brush,
    /// Hard pixels, no anti-aliasing.
    Pencil,
    /// Takes alpha away.
    Eraser,
    /// Paints the layer's own pixels from (`dx`, `dy`) away.
    Clone { dx: f32, dy: f32 },
}

impl BrushTool {
    /// The History panel's name for a stroke of this tool.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Brush => "Brush",
            Self::Pencil => "Pencil",
            Self::Eraser => "Eraser",
            Self::Clone { .. } => "Clone Stamp",
        }
    }
}

/// A stroke's settings, as the options bar shows them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BrushSettings {
    pub tool: BrushTool,
    /// Straight RGBA.
    pub color: [u8; 4],
    /// Diameter in document pixels.
    pub size: f32,
    /// 0 (soft) .. 1 (hard edge).
    pub hardness: f32,
    /// The most one stroke can paint, 0..1.
    pub opacity: f32,
    /// How much one dab paints, 0..1.
    pub flow: f32,
    /// Dab distance as a fraction of the diameter.
    pub spacing: f32,
    /// Pen pressure scales the diameter.
    pub pressure_size: bool,
    /// Pen pressure scales the flow.
    pub pressure_flow: bool,
}

impl Default for BrushSettings {
    fn default() -> Self {
        Self {
            tool: BrushTool::Brush,
            color: [0, 0, 0, 255],
            size: 24.0,
            hardness: 0.8,
            opacity: 1.0,
            flow: 1.0,
            spacing: 0.15,
            pressure_size: true,
            pressure_flow: false,
        }
    }
}

/// One pointer sample in document pixels; `pressure` 0..1 (a mouse is 1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StrokePoint {
    pub x: f32,
    pub y: f32,
    pub pressure: f32,
}

/// A dab's coverage, made once per radius by azul's brush.
#[derive(Clone, Debug)]
pub struct Stamp {
    side: usize,
    center: f32,
    coverage: Vec<f32>,
}

impl Stamp {
    /// The coverage of a dab of `radius` and `hardness`, painted by
    /// `RawImage::paint_dot` (its alpha channel is the coverage).
    #[must_use]
    pub fn new(radius: f32, hardness: f32) -> Self {
        let radius = radius.max(0.5);
        let side = (2.0 * radius).ceil() as usize + 3;
        let center = side as f32 / 2.0;
        let mut image =
            RawImage::create_rgba8(side as u32, side as u32, vec![0u8; side * side * 4].into(), true);
        let mut brush = Brush::create(
            ColorU {
                r: 255,
                g: 255,
                b: 255,
                a: 255,
            },
            radius,
        );
        brush.hardness = hardness.clamp(0.0, 1.0);
        brush.flow = 1.0;
        brush.spacing = 1.0;
        image.paint_dot(center, center, brush);
        let coverage = if let RawImageData::U8(ref bytes) = image.pixels {
            bytes
                .as_ref()
                .chunks_exact(4)
                .map(|p| f32::from(p[3]) / 255.0)
                .collect()
        } else {
            vec![0.0; side * side]
        };
        Self {
            side,
            center,
            coverage,
        }
    }

    /// The coverage at (`ox`, `oy`) from the dab's centre.
    #[must_use]
    pub fn sample(&self, ox: f32, oy: f32) -> f32 {
        let u = ox + self.center - 0.5;
        let v = oy + self.center - 0.5;
        let x0 = u.floor();
        let y0 = v.floor();
        let (tx, ty) = (u - x0, v - y0);
        let (x0, y0) = (x0 as isize, y0 as isize);
        let side = self.side as isize;
        let at = |x: isize, y: isize| -> f32 {
            if x < 0 || y < 0 || x >= side || y >= side {
                0.0
            } else {
                self.coverage[(y * side + x) as usize]
            }
        };
        let top = at(x0, y0) + (at(x0 + 1, y0) - at(x0, y0)) * tx;
        let bottom = at(x0, y0 + 1) + (at(x0 + 1, y0 + 1) - at(x0, y0 + 1)) * tx;
        top + (bottom - top) * ty
    }
}

/// A stroke in progress on one raster layer.
#[derive(Debug)]
pub struct Stroke {
    pub settings: BrushSettings,
    pub layer: LayerId,
    /// The layer as it was when the stroke began (shared tiles: free).
    base: TileGrid,
    /// The stroke's coverage per tile, 0..1.
    mask: HashMap<(u32, u32), Vec<f32>>,
    last: Option<StrokePoint>,
    /// Path length since the last dab.
    carry: f32,
    stamps: HashMap<(u32, u32), Stamp>,
    /// Everything this stroke changed so far.
    pub dirty: IRect,
}

impl Stroke {
    #[must_use]
    pub fn begin(settings: BrushSettings, layer: LayerId, base: TileGrid) -> Self {
        Self {
            settings,
            layer,
            base,
            mask: HashMap::new(),
            last: None,
            carry: 0.0,
            stamps: HashMap::new(),
            dirty: IRect::default(),
        }
    }

    fn diameter(&self, p: &StrokePoint) -> f32 {
        let s = &self.settings;
        let d = if s.pressure_size {
            s.size * p.pressure.clamp(0.0, 1.0)
        } else {
            s.size
        };
        d.max(1.0)
    }

    /// Continue the stroke to `p`: dabs along the way into `grid` (the
    /// layer), confined to `selection`. Returns the rect it changed.
    pub fn add_point(&mut self, p: StrokePoint, grid: &mut TileGrid, selection: Option<&Mask>) -> IRect {
        if !(p.x.is_finite() && p.y.is_finite()) {
            return IRect::default();
        }
        let mut changed = IRect::default();
        match self.last {
            None => {
                changed = self.dab(p, grid, selection);
                self.carry = 0.0;
            }
            Some(prev) => {
                let (dx, dy) = (p.x - prev.x, p.y - prev.y);
                let len = dx.hypot(dy);
                let step = (self.diameter(&p) * self.settings.spacing).max(0.5);
                let mut at = step - self.carry;
                let mut placed = 0usize;
                while at <= len && placed < 100_000 {
                    let t = if len > 0.0 { at / len } else { 1.0 };
                    let q = StrokePoint {
                        x: prev.x + dx * t,
                        y: prev.y + dy * t,
                        pressure: prev.pressure + (p.pressure - prev.pressure) * t,
                    };
                    changed = changed.union(&self.dab(q, grid, selection));
                    at += step;
                    placed += 1;
                }
                self.carry = len - (at - step);
            }
        }
        self.last = Some(p);
        self.dirty = self.dirty.union(&changed);
        changed
    }

    /// One dab at `q`.
    fn dab(&mut self, q: StrokePoint, grid: &mut TileGrid, selection: Option<&Mask>) -> IRect {
        let s = self.settings;
        let radius = self.diameter(&q) / 2.0;
        let flow = if s.pressure_flow {
            s.flow * q.pressure.clamp(0.0, 1.0)
        } else {
            s.flow
        }
        .clamp(0.0, 1.0);
        let reach = IRect::covering(q.x - radius - 1.0, q.y - radius - 1.0, q.x + radius + 1.0, q.y + radius + 1.0);
        let Some(bounds) = reach.intersect(&grid.bounds()) else {
            return IRect::default();
        };

        // The dab's coverage over its bounds.
        let pencil = s.tool == BrushTool::Pencil;
        let mut cover = vec![0.0f32; (bounds.w * bounds.h) as usize];
        {
            let stamp = if pencil {
                None
            } else {
                let key = ((radius * 4.0).round() as u32, (s.hardness.clamp(0.0, 1.0) * 100.0).round() as u32);
                Some(
                    &*self
                        .stamps
                        .entry(key)
                        .or_insert_with(|| Stamp::new(key.0 as f32 / 4.0, key.1 as f32 / 100.0)),
                )
            };
            for y in 0..bounds.h {
                for x in 0..bounds.w {
                    let ox = (bounds.x + x) as f32 + 0.5 - q.x;
                    let oy = (bounds.y + y) as f32 + 0.5 - q.y;
                    cover[(y * bounds.w + x) as usize] = match stamp {
                        Some(stamp) => stamp.sample(ox, oy),
                        None => {
                            if ox * ox + oy * oy <= radius * radius {
                                1.0
                            } else {
                                0.0
                            }
                        }
                    };
                }
            }
        }

        let Self { base, mask, .. } = self;
        for (tx, ty) in grid.tiles_in(&bounds) {
            let Some(part) = grid.tile_rect(tx, ty).intersect(&bounds) else {
                continue;
            };
            let tile_mask = mask
                .entry((tx, ty))
                .or_insert_with(|| vec![0.0; TILE_PIXELS]);
            let base_tile = base.tile(tx, ty);
            let ox = (tx * TILE) as i32;
            let oy = (ty * TILE) as i32;
            for y in part.y..part.bottom() {
                for x in part.x..part.right() {
                    let c = cover[((y - bounds.y) * bounds.w + (x - bounds.x)) as usize];
                    if c <= 0.0 {
                        continue;
                    }
                    let (lx, ly) = ((x - ox) as u32, (y - oy) as u32);
                    let mi = (ly * TILE + lx) as usize;
                    let m = &mut tile_mask[mi];
                    *m += (1.0 - *m) * flow * c;
                    let sel = selection.map_or(1.0, |sm| sm.coverage(x as u32, y as u32));
                    let a = s.opacity.clamp(0.0, 1.0) * *m * sel;
                    let under = base_tile.as_ref().map_or([0; 4], |t| t.get(lx, ly));
                    let out = match s.tool {
                        BrushTool::Brush | BrushTool::Pencil => blend::paint_over(under, s.color, a),
                        BrushTool::Eraser => {
                            let alpha = (f32::from(under[3]) * (1.0 - a)).round() as u8;
                            if alpha == 0 {
                                [0, 0, 0, 0]
                            } else {
                                [under[0], under[1], under[2], alpha]
                            }
                        }
                        BrushTool::Clone { dx, dy } => {
                            let sx = (x as f32 + 0.5 + dx).floor();
                            let sy = (y as f32 + 0.5 + dy).floor();
                            let src = if sx >= 0.0 && sy >= 0.0 {
                                base.pixel(sx as u32, sy as u32)
                            } else {
                                [0; 4]
                            };
                            blend::paint_over(under, src, a)
                        }
                    };
                    grid.set_pixel(x as u32, y as u32, out);
                }
            }
        }
        bounds
    }
}
