//! What a CPU frame did: counted, so a test can pin the work of a frame instead of its time.
//!
//! The rasteriser and the composite count into thread-local cells (one increment, no atomics) while
//! a frame renders; the backend takes them at the end of the frame ([`take`]) and the window keeps the
//! last frame's and the sum since the counters were reset (`FrameReport::cpu`). A zoom step that
//! paints all the items of a view again, a pan that draws every halo again, a surface that is
//! allocated and dropped every frame are numbers here, and a count does not depend on how loaded
//! the machine is.

use core::cell::Cell;

/// One counter of [`CpuFrameCounters`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Counter {
    /// Pixels of the frame cleared before the damage was painted.
    PxCleared,
    /// Pixels written by composites: a layer, a group, an image, a kept halo.
    PxBlitted,
    /// Display-list items the rasteriser walked (the windows' and the views').
    ItemsWalked,
    /// Of those, the items that drew: they passed every cull test.
    ItemsPainted,
    /// Text runs rasterised.
    TextRuns,
    /// Glyphs rasterised.
    Glyphs,
    /// Text halos (shadows) drawn.
    HalosDrawn,
    /// Isolated groups opened: a reference frame painted into a surface of its own.
    GroupsOpened,
    /// Pixels of those surfaces.
    GroupPx,
    /// Groups served from a retained surface instead of being painted again.
    RetainedHits,
    /// Items visited by the scans of a whole list: a frame's extent, the value damage, the cover
    /// of a shift.
    ScanItems,
}

const COUNTERS: usize = 11;

thread_local! {
    static COUNTS: [Cell<u64>; COUNTERS] = const { [const { Cell::new(0) }; COUNTERS] };
}

/// What one frame did, [`take`]n at its end.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CpuFrameCounters {
    /// See [`Counter::PxCleared`].
    pub px_cleared: u64,
    /// See [`Counter::PxBlitted`].
    pub px_blitted: u64,
    /// See [`Counter::ItemsWalked`].
    pub items_walked: u64,
    /// See [`Counter::ItemsPainted`].
    pub items_painted: u64,
    /// See [`Counter::TextRuns`].
    pub text_runs: u64,
    /// See [`Counter::Glyphs`].
    pub glyphs: u64,
    /// See [`Counter::HalosDrawn`].
    pub halos_drawn: u64,
    /// See [`Counter::GroupsOpened`].
    pub groups_opened: u64,
    /// See [`Counter::GroupPx`].
    pub group_px: u64,
    /// See [`Counter::RetainedHits`].
    pub retained_hits: u64,
    /// See [`Counter::ScanItems`].
    pub scan_items: u64,
}

impl CpuFrameCounters {
    /// `self` plus `other`, counter by counter.
    pub fn add(&mut self, other: &Self) {
        self.px_cleared += other.px_cleared;
        self.px_blitted += other.px_blitted;
        self.items_walked += other.items_walked;
        self.items_painted += other.items_painted;
        self.text_runs += other.text_runs;
        self.glyphs += other.glyphs;
        self.halos_drawn += other.halos_drawn;
        self.groups_opened += other.groups_opened;
        self.group_px += other.group_px;
        self.retained_hits += other.retained_hits;
        self.scan_items += other.scan_items;
    }
}

/// Count `n` more of `counter` on this thread.
#[inline]
pub fn count(counter: Counter, n: u64) {
    COUNTS.with(|cells| {
        let cell = &cells[counter as usize];
        cell.set(cell.get() + n);
    });
}

/// Zero this thread's counters: the start of a frame.
pub fn reset() {
    COUNTS.with(|cells| cells.iter().for_each(|c| c.set(0)));
}

/// This thread's counters, zeroed: the end of a frame.
#[must_use]
pub fn take() -> CpuFrameCounters {
    COUNTS.with(|cells| {
        let get = |c: Counter| cells[c as usize].replace(0);
        CpuFrameCounters {
            px_cleared: get(Counter::PxCleared),
            px_blitted: get(Counter::PxBlitted),
            items_walked: get(Counter::ItemsWalked),
            items_painted: get(Counter::ItemsPainted),
            text_runs: get(Counter::TextRuns),
            glyphs: get(Counter::Glyphs),
            halos_drawn: get(Counter::HalosDrawn),
            groups_opened: get(Counter::GroupsOpened),
            group_px: get(Counter::GroupPx),
            retained_hits: get(Counter::RetainedHits),
            scan_items: get(Counter::ScanItems),
        }
    })
}

/// What a window keeps of its CPU frames: the last frame's counters and their sum since the
/// counters were reset (`LayoutWindow::request_frame_report_reset`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CpuCounterState {
    /// The reset generation the sum belongs to.
    pub generation: u64,
    /// The last frame's counters.
    pub last: CpuFrameCounters,
    /// Every frame's since the last reset.
    pub since_reset: CpuFrameCounters,
}

/// A frame being rendered on this thread: counted from [`Self::begin`] to its drop, where its
/// counters go to the window.
pub struct FrameScope<'a> {
    window: &'a crate::window::LayoutWindow,
}

impl<'a> FrameScope<'a> {
    /// Zero this thread's counters and count the frame of `window`.
    #[must_use]
    pub fn begin(window: &'a crate::window::LayoutWindow) -> Self {
        reset();
        Self { window }
    }
}

impl Drop for FrameScope<'_> {
    fn drop(&mut self) {
        self.window.record_cpu_counters(take());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_are_taken_and_zeroed_per_thread() {
        reset();
        count(Counter::ItemsWalked, 3);
        count(Counter::ItemsWalked, 4);
        count(Counter::HalosDrawn, 1);
        std::thread::spawn(|| count(Counter::ItemsWalked, 100)).join().expect("a thread");
        let taken = take();
        assert_eq!((taken.items_walked, taken.halos_drawn, taken.glyphs), (7, 1, 0));
        assert_eq!(take(), CpuFrameCounters::default(), "taking zeroes");
    }
}
