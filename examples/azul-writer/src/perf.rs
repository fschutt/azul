//! Per-frame phase timing for the layout callback.
//!
//! WHY: `AZWRITER_FRAME_LOG=1` reports that a whole `layout()` call took
//! 167 ms, which is useless on its own — the callback clones the app state,
//! re-splits the paginated document, and rebuilds four sub-trees, and any
//! one of them could own that number. This records a duration per named
//! phase so a slow frame names its own culprit.
//!
//! Cost when disabled: one `Option` check per phase and nothing recorded.
//! The env var is read once into a `OnceLock`, not per phase.
//!
//! Usage:
//! - `AZWRITER_FRAME_LOG=1`   — breakdown for frames over the 8 ms budget
//! - `AZWRITER_FRAME_LOG=all` — breakdown for EVERY frame (call counting)

use std::cell::RefCell;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

pub use crate::args::FrameLog as Mode;

static M: OnceLock<Mode> = OnceLock::new();

/// Store what `--frame-log` asked for. Called once from `start`, before any
/// frame exists; a second call is ignored rather than racing the first.
pub fn init_frame_log(mode: Mode) {
    let _ = M.set(mode);
}

pub fn mode() -> Mode {
    M.get().copied().unwrap_or(Mode::Off)
}

thread_local! {
    static PHASES: RefCell<Vec<(&'static str, Duration)>> = const { RefCell::new(Vec::new()) };
}

/// Scoped timer: records `name` -> elapsed when dropped. Nested phases are
/// recorded independently, so an outer phase's time INCLUDES its inner ones
/// (the report marks nesting by insertion order, not by subtraction).
pub struct Phase {
    name: &'static str,
    start: Option<Instant>,
}

impl Phase {
    #[must_use]
    pub fn start(name: &'static str) -> Self {
        Self {
            name,
            start: (mode() != Mode::Off).then(Instant::now),
        }
    }
}

impl Drop for Phase {
    fn drop(&mut self) {
        if let Some(t) = self.start {
            let d = t.elapsed();
            PHASES.with(|p| p.borrow_mut().push((self.name, d)));
        }
    }
}

/// Drain the recorded phases (called by the frame timer when it reports).
pub fn take_phases() -> Vec<(&'static str, Duration)> {
    PHASES.with(|p| std::mem::take(&mut *p.borrow_mut()))
}

/// Monotonic count of completed layout() calls, so a report can say WHICH
/// frame was slow — "the first two" and "one in every ten" are different
/// bugs with the same duration.
pub fn next_frame_number() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    N.fetch_add(1, Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The recorder must be inert when the env var is unset — a diagnostic
    /// that allocates on every frame in the shipping configuration is a
    /// perf bug of its own.
    ///
    /// NEGATIVE CONTROL: making `Phase::start` unconditionally record
    /// (`start: Some(Instant::now())`) makes this fail — verified.
    #[test]
    fn phases_record_nothing_when_the_env_var_is_unset() {
        if mode() != Mode::Off {
            // The harness runs with the var set; the assertion below only
            // means anything in the default configuration.
            return;
        }
        let _ = take_phases();
        {
            let _p = Phase::start("test");
        }
        assert!(
            take_phases().is_empty(),
            "a disabled Phase must not push a record"
        );
    }
}
