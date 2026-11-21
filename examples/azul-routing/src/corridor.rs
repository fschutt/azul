//! The tiles a trip will most likely read, fetched ahead of the search: the
//! zoom 14 tiles around both ends, and the tiles along the straight line
//! between them at the zoom the search reads at each place (fine near the
//! ends, coarse in the middle of a long trip).
//!
//! The search itself reads whatever tile it reaches, so the corridor is no
//! longer needed to find a way; it only lets the downloads run in parallel
//! instead of one after the other as the search reaches them.

use std::collections::BTreeSet;

use crate::{
    distance_m,
    tile::{around, tile_of, TileId},
};

/// How far apart the corridor looks along the line, metres: well under the
/// size of a zoom 14 tile, so none is skipped.
const STEP_M: f64 = 500.0;

/// The most places looked at along one line: a trip across a continent
/// takes longer steps, which only its coarse tiles see.
const MAX_STEPS: usize = 4_000;

/// The corridor of a trip from `from` to `to` (both `(lat, lon)`), nearest
/// to an end first: each end's zoom 14 tile and its eight neighbours (the
/// nearest road can be across a tile edge), and along the line the tile of
/// the zoom `zoom_at` names for each place - with its neighbours on zoom
/// 14, where the roads near the line matter. Each tile once.
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_precision_loss)]
pub fn corridor(from: (f64, f64), to: (f64, f64), zoom_at: &dyn Fn((f64, f64)) -> u8) -> Vec<TileId> {
    let mut tiles = BTreeSet::new();
    for end in [from, to] {
        tiles.extend(around(tile_of(end.0, end.1, 14), 1));
    }
    let steps = ((distance_m(from, to) / STEP_M).ceil() as usize).clamp(1, MAX_STEPS);
    for i in 0..=steps {
        let f = i as f64 / steps as f64;
        let at = (from.0 + (to.0 - from.0) * f, from.1 + (to.1 - from.1) * f);
        let z = zoom_at(at);
        let tile = tile_of(at.0, at.1, z);
        if z >= 14 {
            tiles.extend(around(tile, 1));
        } else {
            tiles.insert(tile);
        }
    }
    let mut tiles: Vec<TileId> = tiles.into_iter().collect();
    let nearest_end = |t: &TileId| {
        let c = t.centre();
        distance_m(c, from).min(distance_m(c, to))
    };
    tiles.sort_by(|a, b| nearest_end(a).total_cmp(&nearest_end(b)).then(b.z.cmp(&a.z)));
    tiles
}
