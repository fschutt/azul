//! Web Mercator tiles: the XYZ scheme of the map's vector tiles (origin at
//! the top-left, `x` east, `y` south), at the routing levels 8, 11 and 14.
//!
//! Zoom 14 holds every road, zoom 11 the arterial roads and up, zoom 8 the
//! highways (motorway, trunk, primary). These are Valhalla's three
//! hierarchy levels by road class; only the grid is the map's, so a router
//! can read fine tiles near the ends of a trip and coarse ones between.

use std::f64::consts::PI;

/// The routing levels, coarse to fine: zoom 8 holds motorway, trunk and
/// primary roads, zoom 11 secondary and tertiary roads and up, zoom 14
/// every road.
pub const LEVELS: [u8; 3] = [8, 11, 14];

/// One tile: its zoom level, column and row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TileId {
    /// The zoom level: 8, 11 or 14 for routing tiles.
    pub z: u8,
    /// The column, from the antimeridian eastwards.
    pub x: u32,
    /// The row, from the north southwards.
    pub y: u32,
}

impl TileId {
    /// The tile `z/x/y`.
    #[must_use]
    pub const fn new(z: u8, x: u32, y: u32) -> Self {
        Self { z, x, y }
    }

    /// `14/8800/5373`, the tile's place in a URL or a cache folder.
    #[must_use]
    pub fn path(&self) -> String {
        format!("{}/{}/{}", self.z, self.x, self.y)
    }

    /// The tile's middle as `(lat, lon)`.
    #[must_use]
    pub fn centre(&self) -> (f64, f64) {
        let n = f64::from(1u32 << self.z);
        latlon_at(f64::from(self.x) + 0.5, f64::from(self.y) + 0.5, n)
    }

    /// The tile's corners as `(south, west, north, east)` in degrees.
    #[must_use]
    pub fn bounds(&self) -> (f64, f64, f64, f64) {
        let n = f64::from(1u32 << self.z);
        let (north, west) = latlon_at(f64::from(self.x), f64::from(self.y), n);
        let (south, east) = latlon_at(f64::from(self.x) + 1.0, f64::from(self.y) + 1.0, n);
        (south, west, north, east)
    }
}

/// The coarsest zoom a road of Valhalla classification `class` is in:
/// 8 for motorway, trunk and primary (0 to 2), 11 for secondary and
/// tertiary (3, 4), 14 for the rest. A road is in every finer zoom too.
#[must_use]
pub const fn zoom_of_class(class: u8) -> u8 {
    match class {
        0..=2 => 8,
        3 | 4 => 11,
        _ => 14,
    }
}

/// Where `(lat, lon)` lies in the tiles of level `z`, as fractional tile
/// coordinates `(x, y)`.
#[must_use]
pub fn tile_xy(lat: f64, lon: f64, z: u8) -> (f64, f64) {
    let n = f64::from(1u32 << z);
    let lat = lat.clamp(-85.0511, 85.0511).to_radians();
    let x = (lon + 180.0) / 360.0 * n;
    let y = (1.0 - (lat.tan() + 1.0 / lat.cos()).ln() / PI) / 2.0 * n;
    (x, y)
}

/// The place at fractional tile coordinates `(x, y)` of a level with `n`
/// tiles per side, as `(lat, lon)`.
#[must_use]
pub fn latlon_at(x: f64, y: f64, n: f64) -> (f64, f64) {
    let lon = x / n * 360.0 - 180.0;
    let lat = (PI * (1.0 - 2.0 * y / n)).sinh().atan().to_degrees();
    (lat, lon)
}

/// The tile of level `z` holding `(lat, lon)`.
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn tile_of(lat: f64, lon: f64, z: u8) -> TileId {
    let n = 1u32 << z;
    let (x, y) = tile_xy(lat, lon, z);
    let clamp = |v: f64| (v.floor().max(0.0) as u32).min(n - 1);
    TileId::new(z, clamp(x), clamp(y))
}

/// The tiles of level `z` within `radius` tiles of `tile` (itself
/// included), clamped to the world's rows and wrapped across the
/// antimeridian.
#[must_use]
#[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
pub fn around(tile: TileId, radius: i64) -> Vec<TileId> {
    let n = i64::from(1u32 << tile.z);
    let mut out = Vec::new();
    for dy in -radius..=radius {
        let y = i64::from(tile.y) + dy;
        if y < 0 || y >= n {
            continue;
        }
        for dx in -radius..=radius {
            let x = (i64::from(tile.x) + dx).rem_euclid(n);
            out.push(TileId::new(tile.z, x as u32, y as u32));
        }
    }
    out
}

/// Every tile of level `z` the straight line from `a` to `b` (both
/// `(lat, lon)`) touches, in order from `a` (a supercover walk through the
/// grid).
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn line_tiles(a: (f64, f64), b: (f64, f64), z: u8) -> Vec<TileId> {
    let n = 1u32 << z;
    let (x0, y0) = tile_xy(a.0, a.1, z);
    let (x1, y1) = tile_xy(b.0, b.1, z);
    let (mut cx, mut cy) = (x0.floor() as i64, y0.floor() as i64);
    let (ex, ey) = (x1.floor() as i64, y1.floor() as i64);
    let (dx, dy) = (x1 - x0, y1 - y0);
    let step_x: i64 = if dx > 0.0 { 1 } else { -1 };
    let step_y: i64 = if dy > 0.0 { 1 } else { -1 };
    // How far along the line (0..1) the next column / row boundary is, and
    // how far one whole tile is.
    let next = |c: i64, step: i64, origin: f64, delta: f64| {
        if delta == 0.0 {
            f64::INFINITY
        } else {
            let edge = if step > 0 { c as f64 + 1.0 } else { c as f64 };
            (edge - origin) / delta
        }
    };
    let mut t_x = next(cx, step_x, x0, dx);
    let mut t_y = next(cy, step_y, y0, dy);
    let d_x = if dx == 0.0 { f64::INFINITY } else { 1.0 / dx.abs() };
    let d_y = if dy == 0.0 { f64::INFINITY } else { 1.0 / dy.abs() };
    let push = |out: &mut Vec<TileId>, x: i64, y: i64| {
        if y >= 0 && y < i64::from(n) {
            out.push(TileId::new(z, x.rem_euclid(i64::from(n)) as u32, y as u32));
        }
    };
    let mut out = Vec::new();
    push(&mut out, cx, cy);
    let limit = (ex - cx).abs() + (ey - cy).abs() + 2;
    for _ in 0..limit {
        if cx == ex && cy == ey {
            break;
        }
        if (t_x - t_y).abs() < 1e-12 {
            // Through a corner: both neighbours are touched.
            push(&mut out, cx + step_x, cy);
            push(&mut out, cx, cy + step_y);
            cx += step_x;
            cy += step_y;
            t_x += d_x;
            t_y += d_y;
        } else if t_x < t_y {
            cx += step_x;
            t_x += d_x;
        } else {
            cy += step_y;
            t_y += d_y;
        }
        push(&mut out, cx, cy);
    }
    out
}
