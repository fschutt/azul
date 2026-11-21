//! Tiles: 256x256 straight-alpha RGBA8 blocks, shared and copied on write.

use std::{fmt, sync::Arc};

use super::geom::IRect;

/// The side of a tile in pixels.
pub const TILE: u32 = 256;
/// Pixels in one tile.
pub const TILE_PIXELS: usize = (TILE * TILE) as usize;

const CLEAR: [u8; 4] = [0, 0, 0, 0];

/// One tile: `TILE` x `TILE` pixels of straight (not premultiplied) RGBA8,
/// row by row. A tile at the right or bottom edge of a document stores the
/// full block; the pixels past the edge are unused.
#[derive(Clone, PartialEq, Eq)]
pub struct Tile {
    pub px: Vec<u8>,
}

impl fmt::Debug for Tile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Tile(transparent: {})", self.is_transparent())
    }
}

impl Tile {
    /// A tile of transparent pixels.
    #[must_use]
    pub fn transparent() -> Self {
        Self {
            px: vec![0; TILE_PIXELS * 4],
        }
    }

    /// A tile of one colour.
    #[must_use]
    pub fn filled(c: [u8; 4]) -> Self {
        Self {
            px: c.iter().copied().cycle().take(TILE_PIXELS * 4).collect(),
        }
    }

    /// Every pixel has alpha 0.
    #[must_use]
    pub fn is_transparent(&self) -> bool {
        self.px.chunks_exact(4).all(|p| p[3] == 0)
    }

    #[inline]
    #[must_use]
    pub const fn index(x: u32, y: u32) -> usize {
        ((y * TILE + x) * 4) as usize
    }

    #[inline]
    #[must_use]
    pub fn get(&self, x: u32, y: u32) -> [u8; 4] {
        let i = Self::index(x, y);
        [self.px[i], self.px[i + 1], self.px[i + 2], self.px[i + 3]]
    }

    #[inline]
    pub fn set(&mut self, x: u32, y: u32, c: [u8; 4]) {
        let i = Self::index(x, y);
        self.px[i..i + 4].copy_from_slice(&c);
    }
}

/// An image as a grid of shared tiles. A missing tile is transparent and
/// costs nothing; a tile is copied only when it is written while another
/// grid (an undo state, a stroke's base) still holds it.
#[derive(Clone)]
pub struct TileGrid {
    width: u32,
    height: u32,
    cols: u32,
    rows: u32,
    tiles: Vec<Option<Arc<Tile>>>,
}

impl fmt::Debug for TileGrid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "TileGrid({}x{}, {} of {} tiles)",
            self.width,
            self.height,
            self.tiles.iter().filter(|t| t.is_some()).count(),
            self.tiles.len()
        )
    }
}

impl TileGrid {
    /// A transparent `width` x `height` image (no tile allocated).
    #[must_use]
    pub fn new(width: u32, height: u32) -> Self {
        let cols = width.div_ceil(TILE);
        let rows = height.div_ceil(TILE);
        Self {
            width,
            height,
            cols,
            rows,
            tiles: vec![None; (cols * rows) as usize],
        }
    }

    /// An image of one colour: every tile is the SAME shared tile until it is
    /// written.
    #[must_use]
    pub fn filled(width: u32, height: u32, c: [u8; 4]) -> Self {
        let mut grid = Self::new(width, height);
        if c[3] != 0 {
            let tile = Arc::new(Tile::filled(c));
            for slot in &mut grid.tiles {
                *slot = Some(tile.clone());
            }
        }
        grid
    }

    /// An image from straight RGBA8 rows (`width * height * 4` bytes; a
    /// shorter buffer leaves the rest transparent). Fully transparent tiles
    /// are not allocated.
    #[must_use]
    pub fn from_rgba(width: u32, height: u32, rgba: &[u8]) -> Self {
        let mut grid = Self::new(width, height);
        grid.write_rect(IRect::new(0, 0, width as i32, height as i32), rgba);
        grid.prune();
        grid
    }

    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }

    #[must_use]
    pub const fn height(&self) -> u32 {
        self.height
    }

    #[must_use]
    pub const fn cols(&self) -> u32 {
        self.cols
    }

    #[must_use]
    pub const fn rows(&self) -> u32 {
        self.rows
    }

    /// The whole image as a rect.
    #[must_use]
    pub const fn bounds(&self) -> IRect {
        IRect::new(0, 0, self.width as i32, self.height as i32)
    }

    #[inline]
    fn slot(&self, tx: u32, ty: u32) -> usize {
        (ty * self.cols + tx) as usize
    }

    /// The tile at (`tx`, `ty`), `None` when it is transparent (or outside).
    #[must_use]
    pub fn tile(&self, tx: u32, ty: u32) -> Option<&Arc<Tile>> {
        if tx >= self.cols || ty >= self.rows {
            return None;
        }
        self.tiles[self.slot(tx, ty)].as_ref()
    }

    /// The tile at (`tx`, `ty`) for writing: allocated when missing, copied
    /// when shared.
    pub fn tile_mut(&mut self, tx: u32, ty: u32) -> &mut Tile {
        let i = self.slot(tx, ty);
        let slot = &mut self.tiles[i];
        if slot.is_none() {
            *slot = Some(Arc::new(Tile::transparent()));
        }
        match slot {
            Some(arc) => Arc::make_mut(arc),
            None => unreachable!("allocated above"),
        }
    }

    /// Replace the tile at (`tx`, `ty`).
    pub fn set_tile(&mut self, tx: u32, ty: u32, tile: Option<Arc<Tile>>) {
        if tx < self.cols && ty < self.rows {
            let i = self.slot(tx, ty);
            self.tiles[i] = tile;
        }
    }

    /// The image pixels the tile (`tx`, `ty`) covers.
    #[must_use]
    pub fn tile_rect(&self, tx: u32, ty: u32) -> IRect {
        let x = (tx * TILE) as i32;
        let y = (ty * TILE) as i32;
        IRect::new(
            x,
            y,
            (TILE as i32).min(self.width as i32 - x),
            (TILE as i32).min(self.height as i32 - y),
        )
    }

    /// The tiles a rect touches (clipped to the image).
    #[must_use]
    pub fn tiles_in(&self, r: &IRect) -> Vec<(u32, u32)> {
        let Some(r) = r.intersect(&self.bounds()) else {
            return Vec::new();
        };
        let t = TILE as i32;
        let mut out = Vec::new();
        for ty in (r.y / t)..=((r.bottom() - 1) / t) {
            for tx in (r.x / t)..=((r.right() - 1) / t) {
                out.push((tx as u32, ty as u32));
            }
        }
        out
    }

    /// Every tile that is allocated (holds pixels), in row order.
    #[must_use]
    pub fn non_empty_tiles(&self) -> Vec<(u32, u32)> {
        let mut out = Vec::new();
        for ty in 0..self.rows {
            for tx in 0..self.cols {
                if self.tiles[self.slot(tx, ty)].is_some() {
                    out.push((tx, ty));
                }
            }
        }
        out
    }

    /// Whether this grid and `other` hold the very same tile at (`tx`, `ty`).
    #[must_use]
    pub fn shares_tile_with(&self, other: &Self, tx: u32, ty: u32) -> bool {
        match (self.tile(tx, ty), other.tile(tx, ty)) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }

    /// Drop the tiles that turned fully transparent.
    pub fn prune(&mut self) {
        for slot in &mut self.tiles {
            if slot.as_ref().is_some_and(|t| t.is_transparent()) {
                *slot = None;
            }
        }
    }

    /// The pixel at (`x`, `y`); transparent outside the image.
    #[must_use]
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        if x >= self.width || y >= self.height {
            return CLEAR;
        }
        match self.tile(x / TILE, y / TILE) {
            Some(t) => t.get(x % TILE, y % TILE),
            None => CLEAR,
        }
    }

    /// Write one pixel (ignored outside the image).
    pub fn set_pixel(&mut self, x: u32, y: u32, c: [u8; 4]) {
        if x >= self.width || y >= self.height {
            return;
        }
        if c[3] == 0 && self.tile(x / TILE, y / TILE).is_none() {
            return;
        }
        self.tile_mut(x / TILE, y / TILE).set(x % TILE, y % TILE, c);
    }

    /// The pixels of `r` as RGBA8 rows (`r.w * r.h * 4` bytes); transparent
    /// where `r` leaves the image.
    #[must_use]
    pub fn read_rect(&self, r: IRect) -> Vec<u8> {
        if r.is_empty() {
            return Vec::new();
        }
        let mut out = vec![0u8; (r.w as usize) * (r.h as usize) * 4];
        let Some(inside) = r.intersect(&self.bounds()) else {
            return out;
        };
        for (tx, ty) in self.tiles_in(&inside) {
            let Some(tile) = self.tile(tx, ty) else {
                continue;
            };
            let Some(part) = self.tile_rect(tx, ty).intersect(&inside) else {
                continue;
            };
            let lx = (part.x - (tx * TILE) as i32) as u32;
            for row in 0..part.h {
                let ly = (part.y + row - (ty * TILE) as i32) as u32;
                let src = Tile::index(lx, ly);
                let dst = (((part.y + row - r.y) * r.w + (part.x - r.x)) * 4) as usize;
                let n = (part.w * 4) as usize;
                out[dst..dst + n].copy_from_slice(&tile.px[src..src + n]);
            }
        }
        out
    }

    /// Write RGBA8 rows into `r` (the part inside the image). A transparent
    /// source does not allocate a missing tile.
    pub fn write_rect(&mut self, r: IRect, rgba: &[u8]) {
        let Some(inside) = r.intersect(&self.bounds()) else {
            return;
        };
        for (tx, ty) in self.tiles_in(&inside) {
            let Some(part) = self.tile_rect(tx, ty).intersect(&inside) else {
                continue;
            };
            let row_bytes = (part.w * 4) as usize;
            let src_at = |row: i32| (((part.y + row - r.y) * r.w + (part.x - r.x)) * 4) as usize;
            if self.tile(tx, ty).is_none() {
                let all_clear = (0..part.h).all(|row| {
                    let s = src_at(row);
                    rgba.get(s..s + row_bytes)
                        .is_none_or(|px| px.chunks_exact(4).all(|p| p[3] == 0))
                });
                if all_clear {
                    continue;
                }
            }
            let lx = (part.x - (tx * TILE) as i32) as u32;
            let tile = self.tile_mut(tx, ty);
            for row in 0..part.h {
                let ly = (part.y + row - (ty * TILE) as i32) as u32;
                let dst = Tile::index(lx, ly);
                let s = src_at(row);
                match rgba.get(s..s + row_bytes) {
                    Some(src) => tile.px[dst..dst + row_bytes].copy_from_slice(src),
                    None => tile.px[dst..dst + row_bytes].fill(0),
                }
            }
        }
    }

    /// The whole image as RGBA8 rows.
    #[must_use]
    pub fn to_rgba(&self) -> Vec<u8> {
        self.read_rect(self.bounds())
    }
}
