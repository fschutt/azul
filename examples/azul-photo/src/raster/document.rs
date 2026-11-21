//! The document and its composite.
//!
//! [`Document`]: size, layer tree, selection. Cloning one is cheap (shared
//! tiles), which is what the undo history keeps.
//!
//! [`Composite`]: the flattened image as tiles, with one dirty flag per tile.
//! [`Composite::update`] recomposites only the dirty tiles, split across
//! worker threads, and answers the rect that changed.

use std::{collections::HashMap, sync::Arc};

use super::{
    adjust::Prepared,
    blend,
    filter::workers,
    geom::IRect,
    layer::{self, Layer, LayerContent, LayerId},
    selection::Mask,
    tile::{Tile, TileGrid, TILE_PIXELS},
};

/// A photo: its size, its layers (bottom first) and its selection.
#[derive(Clone, Debug)]
pub struct Document {
    pub width: u32,
    pub height: u32,
    pub layers: Vec<Layer>,
    /// `None` = nothing selected (edits act on the whole layer).
    pub selection: Option<Arc<Mask>>,
    /// The next layer id to hand out.
    pub next_id: LayerId,
}

impl Document {
    /// An empty document (no layers).
    #[must_use]
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            layers: Vec::new(),
            selection: None,
            next_id: 1,
        }
    }

    /// A document with one "Background" layer of `color`.
    #[must_use]
    pub fn with_background(width: u32, height: u32, color: [u8; 4]) -> Self {
        let mut doc = Self::new(width, height);
        let id = doc.mint_id();
        doc.layers.push(Layer::raster(
            id,
            "Background",
            TileGrid::filled(width, height, color),
        ));
        doc
    }

    /// A document with one layer of these pixels (an opened photo).
    #[must_use]
    pub fn from_rgba(width: u32, height: u32, rgba: Vec<u8>, name: &str) -> Self {
        let mut doc = Self::new(width, height);
        let id = doc.mint_id();
        doc.layers.push(Layer::raster(
            id,
            name,
            TileGrid::from_rgba(width, height, &rgba),
        ));
        doc
    }

    /// A fresh layer id.
    pub fn mint_id(&mut self) -> LayerId {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    #[must_use]
    pub const fn bounds(&self) -> IRect {
        IRect::new(0, 0, self.width as i32, self.height as i32)
    }

    #[must_use]
    pub fn layer(&self, id: LayerId) -> Option<&Layer> {
        layer::find(&self.layers, id)
    }

    /// Every adjustment layer, ready for compositing.
    #[must_use]
    pub fn prepare_adjustments(&self) -> HashMap<LayerId, Prepared> {
        fn walk(list: &[Layer], out: &mut HashMap<LayerId, Prepared>) {
            for l in list {
                match &l.content {
                    LayerContent::Adjustment(a) => {
                        out.insert(l.id, a.prepare());
                    }
                    LayerContent::Group(children) => walk(children, out),
                    LayerContent::Raster(_) => {}
                }
            }
        }
        let mut out = HashMap::new();
        walk(&self.layers, &mut out);
        out
    }

    /// The flattened pixels of tile (`tx`, `ty`).
    #[must_use]
    pub fn composite_tile(&self, tx: u32, ty: u32) -> Tile {
        self.composite_tile_with(tx, ty, &self.prepare_adjustments())
    }

    fn composite_tile_with(&self, tx: u32, ty: u32, prepared: &HashMap<LayerId, Prepared>) -> Tile {
        let mut acc = vec![[0.0f32; 4]; TILE_PIXELS];
        composite_list(&self.layers, tx, ty, prepared, &mut acc);
        let mut tile = Tile::transparent();
        for (i, p) in acc.iter().enumerate() {
            if p[3] > 0.0 {
                tile.px[i * 4..i * 4 + 4].copy_from_slice(&blend::premul_to_rgba8(*p));
            }
        }
        tile
    }

    /// The whole flattened image.
    #[must_use]
    pub fn flatten(&self) -> TileGrid {
        let mut composite = Composite::new(self.width, self.height);
        composite.update(self);
        composite.grid
    }
}

/// Composite `layers` (bottom first) over the premultiplied `acc` of one tile.
fn composite_list(
    layers: &[Layer],
    tx: u32,
    ty: u32,
    prepared: &HashMap<LayerId, Prepared>,
    acc: &mut [[f32; 4]],
) {
    for l in layers {
        if !l.visible || l.opacity <= 0.0 {
            continue;
        }
        match &l.content {
            LayerContent::Raster(grid) => {
                let Some(tile) = grid.tile(tx, ty) else {
                    continue;
                };
                for (i, dst) in acc.iter_mut().enumerate() {
                    let p = [tile.px[i * 4], tile.px[i * 4 + 1], tile.px[i * 4 + 2], tile.px[i * 4 + 3]];
                    if p[3] == 0 {
                        continue;
                    }
                    blend::blend_into(dst, blend::to_f32(p), l.opacity, l.blend);
                }
            }
            LayerContent::Adjustment(_) => {
                let Some(adj) = prepared.get(&l.id) else {
                    continue;
                };
                let t = l.opacity.clamp(0.0, 1.0);
                for dst in acc.iter_mut() {
                    let a = dst[3];
                    if a <= 0.0 {
                        continue;
                    }
                    let c = [dst[0] / a, dst[1] / a, dst[2] / a];
                    let m = adj.apply(c);
                    for k in 0..3 {
                        dst[k] = (c[k] + (m[k] - c[k]) * t) * a;
                    }
                }
            }
            LayerContent::Group(children) => {
                let mut group = vec![[0.0f32; 4]; acc.len()];
                composite_list(children, tx, ty, prepared, &mut group);
                for (dst, g) in acc.iter_mut().zip(&group) {
                    let a = g[3];
                    if a <= 0.0 {
                        continue;
                    }
                    let straight = [g[0] / a, g[1] / a, g[2] / a, a];
                    blend::blend_into(dst, straight, l.opacity, l.blend);
                }
            }
        }
    }
}

/// The flattened image, kept up to date tile by tile.
pub struct Composite {
    grid: TileGrid,
    dirty: Vec<bool>,
}

impl Composite {
    /// A composite of a `width` x `height` document, every tile dirty.
    #[must_use]
    pub fn new(width: u32, height: u32) -> Self {
        let grid = TileGrid::new(width, height);
        let n = (grid.cols() * grid.rows()) as usize;
        Self {
            grid,
            dirty: vec![true; n],
        }
    }

    /// The flattened image (as of the last [`Self::update`]).
    #[must_use]
    pub const fn grid(&self) -> &TileGrid {
        &self.grid
    }

    /// The tiles of `r` need compositing again.
    pub fn mark(&mut self, r: &IRect) {
        let cols = self.grid.cols();
        for (tx, ty) in self.grid.tiles_in(r) {
            self.dirty[(ty * cols + tx) as usize] = true;
        }
    }

    /// Every tile needs compositing again.
    pub fn mark_all(&mut self) {
        self.dirty.fill(true);
    }

    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.dirty.iter().any(|d| *d)
    }

    /// Composite the dirty tiles of `doc` on worker threads. Returns the
    /// rect they cover, `None` when nothing was dirty. A document of another
    /// size starts over (everything dirty).
    pub fn update(&mut self, doc: &Document) -> Option<IRect> {
        if (self.grid.width(), self.grid.height()) != (doc.width, doc.height) {
            *self = Self::new(doc.width, doc.height);
        }
        let cols = self.grid.cols();
        let list: Vec<(u32, u32)> = self
            .dirty
            .iter()
            .enumerate()
            .filter(|(_, d)| **d)
            .map(|(i, _)| (i as u32 % cols.max(1), i as u32 / cols.max(1)))
            .collect();
        if list.is_empty() {
            return None;
        }
        let prepared = doc.prepare_adjustments();
        let chunk = list.len().div_ceil(workers().min(list.len()).max(1));
        let results: Vec<((u32, u32), Tile)> = std::thread::scope(|s| {
            let prepared = &prepared;
            let handles: Vec<_> = list
                .chunks(chunk)
                .map(|part| {
                    s.spawn(move || {
                        part.iter()
                            .map(|&(tx, ty)| ((tx, ty), doc.composite_tile_with(tx, ty, prepared)))
                            .collect::<Vec<_>>()
                    })
                })
                .collect();
            handles
                .into_iter()
                .flat_map(|h| h.join().unwrap_or_default())
                .collect()
        });
        let mut rect = IRect::default();
        for ((tx, ty), tile) in results {
            rect = rect.union(&self.grid.tile_rect(tx, ty));
            let tile = (!tile.is_transparent()).then(|| Arc::new(tile));
            self.grid.set_tile(tx, ty, tile);
            self.dirty[(ty * cols + tx) as usize] = false;
        }
        Some(rect)
    }
}
