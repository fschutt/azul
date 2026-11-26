//! The loop's frame work by lifecycle state. No device.

use super::*;

/// The launch: `Start` and `Resume` arrive before `InitWindow`, with the first regeneration already
/// queued by the window's creation. Nothing is laid out until the surface and its size are known.
///
/// Mutation: a loop that regenerates whenever a regeneration is pending (what the shell did: the
/// app's layout callback ran at 640 x 480 and 96 dpi before the surface existed).
#[test]
fn nothing_is_laid_out_before_the_surface_exists() {
    assert_eq!(frame_work(false, true, false), FrameWork::Nothing);
    assert_eq!(frame_work(false, true, true), FrameWork::Nothing);
}

/// With the surface the queued request is run, in the order that wastes least.
#[test]
fn a_queued_regeneration_runs_once_the_surface_exists_and_wins_over_a_repaint() {
    assert_eq!(frame_work(true, true, false), FrameWork::Regenerate);
    assert_eq!(frame_work(true, true, true), FrameWork::Regenerate);
    assert_eq!(frame_work(true, false, true), FrameWork::Repaint);
    assert_eq!(frame_work(true, false, false), FrameWork::Nothing);
}

/// In the background (the surface is gone) a repaint request waits too: there is nothing to paint to.
#[test]
fn a_repaint_waits_in_the_background() {
    assert_eq!(frame_work(false, false, true), FrameWork::Nothing);
}

/// `onTrimMemory` levels: nothing for a mere moderate warning, caches from running-low on, the allocator as
/// well once the UI is hidden (20) and in the background levels (40, 60, 80). `am send-trim-memory
/// RUNNING_CRITICAL` is 15.
///
/// Mutation: every level releases everything (a moderate warning flushing the glyph cache of a busy app); no
/// level at all (the shell ignored `onTrimMemory`: only `onLowMemory` reached native code).
#[test]
fn trim_memory_levels_release_by_how_bad_it_is() {
    assert_eq!(trim_plan(5), TrimPlan { release_caches: false, purge_allocator: false });
    assert_eq!(trim_plan(10), TrimPlan { release_caches: true, purge_allocator: false });
    assert_eq!(trim_plan(15), TrimPlan { release_caches: true, purge_allocator: false });
    for level in [20, 40, 60, 80] {
        assert_eq!(trim_plan(level), TrimPlan { release_caches: true, purge_allocator: true }, "level {level}");
    }
    assert_eq!(trim_plan(0), TrimPlan { release_caches: false, purge_allocator: false });
    assert_eq!(trim_plan(-1), TrimPlan { release_caches: false, purge_allocator: false });
}
