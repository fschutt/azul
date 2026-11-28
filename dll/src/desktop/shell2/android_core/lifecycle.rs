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

/// `ComponentCallbacks2.TRIM_MEMORY_RUNNING_LOW`: the app is in the foreground and the system runs low.
pub const TRIM_MEMORY_RUNNING_LOW: i32 = 10;
/// `ComponentCallbacks2.TRIM_MEMORY_UI_HIDDEN`: the app's UI is no longer visible.
pub const TRIM_MEMORY_UI_HIDDEN: i32 = 20;

/// What to give back for an `onTrimMemory(level)` (NativeActivity forwards only `onLowMemory` to native
/// code: `am send-trim-memory RUNNING_CRITICAL`, the UI going hidden and the system's background levels
/// never reached the shell; `AzulActivity.onTrimMemory` now queues them).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrimPlan {
    /// What the CPU backend keeps only to make frames cheaper (kept group surfaces, halos, the layered
    /// compositor) and the glyph cache's idle entries.
    pub release_caches: bool,
    /// The allocator's free arenas go back to the system.
    pub purge_allocator: bool,
}

/// The plan for `level` (the `TRIM_MEMORY_*` of `ComponentCallbacks2`, 5 .. 80):
///
/// - below `RUNNING_LOW` (5, `RUNNING_MODERATE`): nothing, the system only notes that it is a little
///   short;
/// - from `RUNNING_LOW` (10) on: the caches go (the next frames make again what they need);
/// - from `UI_HIDDEN` (20) on (the app is no longer visible, then the background levels 40, 60, 80): the
///   allocator purges as well.
#[must_use]
pub const fn trim_plan(level: i32) -> TrimPlan {
    TrimPlan {
        release_caches: level >= TRIM_MEMORY_RUNNING_LOW,
        purge_allocator: level >= TRIM_MEMORY_UI_HIDDEN,
    }
}

/// What the app is told of its coming and going (the app's lifecycle handler, `LifecycleEvent`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tell {
    /// The app left the screen: save what is not saved (the process may be killed from here on, without another word).
    Paused,
    /// The app is back on the screen after a pause.
    Resumed,
    /// The system wants memory back (`onLowMemory`, a trim from `TRIM_MEMORY_RUNNING_LOW` on).
    LowMemory,
    /// The activity is destroyed: the last word.
    WillTerminate,
}

/// What of the activity's coming and going the app has been told, so that what it hears holds in pairs: `Paused` once
/// when it leaves (`onPause`; a surface lost without one tells it too, since the loop has seen `TerminateWindow` before
/// `Pause`), `Resumed` only after a `Paused` (the `Resume` of the launch is no return), a second `Pause` says nothing new,
/// `WillTerminate` once. `LowMemory` is said at every trim: what to drop is the app's to decide.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AppPhase {
    paused: bool,
    terminating: bool,
}

impl AppPhase {
    /// `onPause` reached the loop (`MainEvent::Pause`).
    pub fn on_pause(&mut self) -> Option<Tell> {
        if self.paused || self.terminating {
            return None;
        }
        self.paused = true;
        Some(Tell::Paused)
    }

    /// The surface went (`MainEvent::TerminateWindow`): the app is off the screen even if `Pause` did not come first.
    pub fn on_surface_lost(&mut self) -> Option<Tell> {
        self.on_pause()
    }

    /// `onResume` reached the loop (`MainEvent::Resume`).
    pub fn on_resume(&mut self) -> Option<Tell> {
        if !self.paused || self.terminating {
            return None;
        }
        self.paused = false;
        Some(Tell::Resumed)
    }

    /// `onLowMemory` (`MainEvent::LowMemory`).
    #[must_use]
    pub const fn on_low_memory(&self) -> Option<Tell> {
        if self.terminating {
            None
        } else {
            Some(Tell::LowMemory)
        }
    }

    /// `onTrimMemory(level)`: told from the running-low level on (the plan's `release_caches`).
    #[must_use]
    pub const fn on_trim(&self, level: i32) -> Option<Tell> {
        if self.terminating || level < TRIM_MEMORY_RUNNING_LOW {
            None
        } else {
            Some(Tell::LowMemory)
        }
    }

    /// `onDestroy` (`MainEvent::Destroy`): the app that never heard `Paused` hears it first, then the last word.
    pub fn on_destroy(&mut self) -> [Option<Tell>; 2] {
        if self.terminating {
            return [None, None];
        }
        let paused = self.on_pause();
        self.terminating = true;
        [paused, Some(Tell::WillTerminate)]
    }
}

#[cfg(test)]
#[path = "lifecycle_test.rs"]
mod tests;
