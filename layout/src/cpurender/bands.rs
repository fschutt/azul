//! How a repaint is cut into walks that run side by side.
//!
//! A repaint is a set of disjoint damage rects, and each rect is painted by a pass of its own
//! (`raster::paint_damage_rect`): the rect is cleared and the list is walked once, clipped to
//! it, with stacks of its own. A pass writes no pixel outside its rect and what it writes inside
//! does not depend on what lies outside, so the passes are independent. This module cuts a large
//! rect into BANDS of rows - each a rect, painted by the same pass - and runs the bands on a few
//! threads.
//!
//! What is shared and what is not:
//!
//! - shared, behind `&`, read-only while the bands run: the display list (and its
//!   `FrameIndex`), the views' lists, the resources, the fonts, the render state with the GPU
//!   values, the kept group surfaces (`retained`);
//! - per band, behind `&mut` or owned by its thread: the rows it writes, its walk's stacks and
//!   group surfaces, a glyph cache, the text rasteriser of its thread, its counters (summed
//!   into the frame's when the band is done).
//!
//! A band paints in the FRAME's coordinates, not in its own: every position the walk computes
//! is the one a single pass over the whole rect computes, so the pixels are the same ones (a
//! band is not a translated picture, whose positions would round differently).

use std::sync::OnceLock;

/// The most threads a repaint uses unless it is told otherwise: the other cores belong to the
/// tile workers and to whatever else the machine runs.
pub const DEFAULT_MAX_PAINT_THREADS: usize = 4;

/// The threads the machine has (asked once: the answer reads `sysctl` or the cgroup files).
fn machine_threads() -> usize {
    static THREADS: OnceLock<usize> = OnceLock::new();
    *THREADS.get_or_init(|| std::thread::available_parallelism().map_or(1, usize::from))
}

/// `AZ_CPU_THREADS` as a thread count: unset, empty, zero or not a number is `None`.
fn parse_threads(v: Option<&str>) -> Option<usize> {
    v.and_then(|s| s.trim().parse::<usize>().ok()).filter(|n| *n > 0)
}

/// The most threads one repaint (and one composite) may use on this machine: `AZ_CPU_THREADS`
/// when it is set (`1` paints everything on the calling thread), else
/// [`DEFAULT_MAX_PAINT_THREADS`]; never more than the machine has.
#[must_use]
pub fn max_threads() -> usize {
    static CONFIGURED: OnceLock<Option<usize>> = OnceLock::new();
    let configured =
        *CONFIGURED.get_or_init(|| parse_threads(std::env::var("AZ_CPU_THREADS").ok().as_deref()));
    configured
        .unwrap_or(DEFAULT_MAX_PAINT_THREADS)
        .min(machine_threads())
        .max(1)
}

/// The threads a repaint asked for with `requested` (`CpuRenderState::paint_threads`) may use:
/// `0` is [`max_threads`], any other number is taken as it is (a test asks for bands on a
/// machine with one core, too).
#[must_use]
pub(crate) fn threads_for(requested: usize) -> usize {
    if requested == 0 {
        max_threads()
    } else {
        requested
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_thread_count_of_the_environment_is_a_positive_number_or_nothing() {
        assert_eq!(parse_threads(None), None);
        assert_eq!(parse_threads(Some("")), None);
        assert_eq!(parse_threads(Some("0")), None);
        assert_eq!(parse_threads(Some("four")), None);
        assert_eq!(parse_threads(Some("1")), Some(1));
        assert_eq!(parse_threads(Some(" 3 ")), Some(3));
        assert!(max_threads() >= 1);
        assert_eq!(threads_for(7), 7, "a count that was asked for is taken as it is");
        assert_eq!(threads_for(0), max_threads());
    }
}
