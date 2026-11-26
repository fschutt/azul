//! What the Android event loop does in a given lifecycle state.
//!
//! `android-activity` delivers ONE event per `poll_events` call, and the loop body runs after every
//! one of them. The first event of a launch is `Start`, then `Resume`, and only then `InitWindow`
//! with the surface and its size. The body used to lay out as soon as a regeneration was queued,
//! which is from the very first iteration on: the app's layout callback was first called at the
//! default window size and 96 dpi, a first layout nobody ever saw (a second on the emulator), and
//! the pixmap it rendered had the wrong size when the real one arrived. The state machine here is the
//! one place that says when the loop may do frame work.

/// What an iteration of the loop does for the frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameWork {
    /// Nothing: no surface, or nothing changed.
    Nothing,
    /// Run the app's layout callback and the solve, and paint.
    Regenerate,
    /// Paint the layout that exists (a scroll offset, a hover restyle): no app callback, no solve.
    Repaint,
}

/// The frame work of one iteration.
///
/// - No surface (before `InitWindow`, or after `TerminateWindow` while the activity is in the
///   background): nothing, and the requests stay queued. The window's size and density are what the
///   first pass has to see; a window nobody can see needs no layout.
/// - A queued regeneration wins over a queued repaint: it paints on its way through.
#[must_use]
pub const fn frame_work(has_surface: bool, regeneration_pending: bool, needs_rerender: bool) -> FrameWork {
    if !has_surface {
        FrameWork::Nothing
    } else if regeneration_pending {
        FrameWork::Regenerate
    } else if needs_rerender {
        FrameWork::Repaint
    } else {
        FrameWork::Nothing
    }
}

#[cfg(test)]
#[path = "lifecycle_test.rs"]
mod tests;
