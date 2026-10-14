//! Which tiles a trip needs (ROUTING.md, steps 2 and 4): a corridor along
//! the straight line from start to destination, fine near the ends and
//! coarse in the middle, widened on every attempt that found no way.

use std::collections::BTreeSet;

use crate::{
    distance_m,
    tile::{around, line_tiles, tile_of, TileId},
};

/// A trip shorter than this is searched on zoom 14 all along.
const SHORT_TRIP_M: f64 = 15_000.0;
/// A trip shorter than this (and not short) is searched on zoom 11 in the
/// middle; a longer one on zoom 8.
const MEDIUM_TRIP_M: f64 = 100_000.0;

/// How many times [`corridor`] widens: attempts `0..ATTEMPTS`.
pub const ATTEMPTS: usize = 3;

/// The corridor of one attempt: how far around the line each level looks.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Widths {
    /// Zoom 14 covers the line this far from each end, in metres...
    fine_m: f64,
    /// ...and this many tiles to each side of it.
    fine_buffer: i64,
    /// Zoom 11 covers this far from each end on a long trip.
    middle_m: f64,
    /// Zoom 11's tiles to each side of the line.
    middle_buffer: i64,
    /// Zoom 8's tiles to each side of the line.
    coarse_buffer: i64,
}

/// The widths of attempt `attempt` (0, 1, 2): each one looks further from the
/// line, for the bridge outside the first buffer or the way around a lake.
fn widths(attempt: usize) -> Widths {
    match attempt {
        0 => Widths {
            fine_m: 3_000.0,
            fine_buffer: 1,
            middle_m: 30_000.0,
            middle_buffer: 1,
            coarse_buffer: 1,
        },
        1 => Widths {
            fine_m: 6_000.0,
            fine_buffer: 3,
            middle_m: 50_000.0,
            middle_buffer: 2,
            coarse_buffer: 1,
        },
        _ => Widths {
            fine_m: 12_000.0,
            fine_buffer: 8,
            middle_m: 80_000.0,
            middle_buffer: 4,
            coarse_buffer: 2,
        },
    }
}

/// The tiles attempt `attempt` (from 0) of a trip from `from` to `to` (both
/// `(lat, lon)`) reads, nearest to an end first. Every attempt holds the
/// tiles of the attempts before it.
///
/// Always: the zoom 14 tile of each end and its eight neighbours (the
/// nearest road can be across a tile edge). A short trip covers the whole
/// line on zoom 14; a medium one covers it on zoom 11 with zoom 14 near the
/// ends; a long one covers it on zoom 8, with zoom 11 and then zoom 14
/// nearer the ends.
#[must_use]
pub fn corridor(from: (f64, f64), to: (f64, f64), attempt: usize) -> Vec<TileId> {
    let w = widths(attempt);
    let length = distance_m(from, to);
    let mut tiles = BTreeSet::new();
    for end in [from, to] {
        tiles.extend(around(tile_of(end.0, end.1, 14), 1));
    }
    // The line from `end` towards the other end, `metres` long at most.
    let toward = |end: (f64, f64), other: (f64, f64), metres: f64| {
        let f = (metres / length.max(1.0)).min(1.0);
        (end.0 + (other.0 - end.0) * f, end.1 + (other.1 - end.1) * f)
    };
    let mut cover = |a: (f64, f64), b: (f64, f64), z: u8, buffer: i64| {
        for tile in line_tiles(a, b, z) {
            tiles.extend(around(tile, buffer));
        }
    };
    if length < SHORT_TRIP_M {
        cover(from, to, 14, w.fine_buffer);
    } else {
        for (end, other) in [(from, to), (to, from)] {
            cover(end, toward(end, other, w.fine_m), 14, w.fine_buffer);
        }
        if length < MEDIUM_TRIP_M {
            cover(from, to, 11, w.middle_buffer);
        } else {
            for (end, other) in [(from, to), (to, from)] {
                cover(end, toward(end, other, w.middle_m), 11, w.middle_buffer);
            }
            cover(from, to, 8, w.coarse_buffer);
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
