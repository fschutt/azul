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
    /// Passes of the damaged walk that were bands of a larger damage rect (0 when every rect was
    /// painted by one pass).
    Bands,
    /// Threads started to paint bands.
    BandThreads,
    /// Isolated groups whose box lands nowhere in the pixels below: no surface was made for them
    /// (they are not in [`Counter::GroupsOpened`]).
    GroupsEmpty,
    /// Pixels of the group surfaces a walk could paint: the part of each that can land.
    GroupPxPainted,
    /// Groups that could have been served from a retained surface and were painted instead.
    RetainedMisses,
    /// Retained surfaces dropped because what they show changed.
    RetainedInvalidations,
    /// Retained surfaces kept through a change of their list: only the boxes of the items
    /// that differ were painted again.
    RetainedPatched,
    /// Glyph cell sets built (`GlyphCache::cells_built`): a glyph, size and sub-pixel step
    /// rasterised because no cache the painting thread could ask held it - the window's, or in
    /// a band the window's and the band's own. A repaint of text drawn before counts none.
    CellsBuilt,
}

const COUNTERS: usize = 19;

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
    /// See [`Counter::Bands`].
    pub bands: u64,
    /// See [`Counter::BandThreads`].
    pub band_threads: u64,
    /// See [`Counter::GroupsEmpty`].
    pub groups_empty: u64,
    /// See [`Counter::GroupPxPainted`].
    pub group_px_painted: u64,
    /// See [`Counter::RetainedMisses`].
    pub retained_misses: u64,
    /// See [`Counter::RetainedInvalidations`].
    pub retained_invalidations: u64,
    /// See [`Counter::RetainedPatched`].
    pub retained_patched: u64,
    /// See [`Counter::CellsBuilt`].
    pub cells_built: u64,
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
        self.bands += other.bands;
        self.band_threads += other.band_threads;
        self.groups_empty += other.groups_empty;
        self.group_px_painted += other.group_px_painted;
        self.retained_misses += other.retained_misses;
        self.retained_invalidations += other.retained_invalidations;
        self.retained_patched += other.retained_patched;
        self.cells_built += other.cells_built;
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
            bands: get(Counter::Bands),
            band_threads: get(Counter::BandThreads),
            groups_empty: get(Counter::GroupsEmpty),
            group_px_painted: get(Counter::GroupPxPainted),
            retained_misses: get(Counter::RetainedMisses),
            retained_invalidations: get(Counter::RetainedInvalidations),
            retained_patched: get(Counter::RetainedPatched),
            cells_built: get(Counter::CellsBuilt),
        }
    })
}

/// Add `counted` - what another thread counted for this thread's frame, [`take`]n there - to
/// this thread's counters. A band of a repaint is painted on a thread of its own and counts
/// into that thread's cells; the frame's counters are the sum, whichever thread painted what.
pub fn merge(counted: &CpuFrameCounters) {
    count(Counter::PxCleared, counted.px_cleared);
    count(Counter::PxBlitted, counted.px_blitted);
    count(Counter::ItemsWalked, counted.items_walked);
    count(Counter::ItemsPainted, counted.items_painted);
    count(Counter::TextRuns, counted.text_runs);
    count(Counter::Glyphs, counted.glyphs);
    count(Counter::HalosDrawn, counted.halos_drawn);
    count(Counter::GroupsOpened, counted.groups_opened);
    count(Counter::GroupPx, counted.group_px);
    count(Counter::RetainedHits, counted.retained_hits);
    count(Counter::ScanItems, counted.scan_items);
    count(Counter::Bands, counted.bands);
    count(Counter::BandThreads, counted.band_threads);
    count(Counter::GroupsEmpty, counted.groups_empty);
    count(Counter::GroupPxPainted, counted.group_px_painted);
    count(Counter::RetainedMisses, counted.retained_misses);
    count(Counter::RetainedInvalidations, counted.retained_invalidations);
    count(Counter::RetainedPatched, counted.retained_patched);
    count(Counter::CellsBuilt, counted.cells_built);
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

    /// A1-01: what a band thread counted reaches the frame's counters, every counter of it.
    /// Mutation: a counter left out of `merge` (or of `take`) fails the comparison with `add`.
    #[test]
    fn what_another_thread_counted_is_merged_into_this_threads_frame() {
        reset();
        count(Counter::ItemsPainted, 5);
        let all = [
            Counter::PxCleared,
            Counter::PxBlitted,
            Counter::ItemsWalked,
            Counter::ItemsPainted,
            Counter::TextRuns,
            Counter::Glyphs,
            Counter::HalosDrawn,
            Counter::GroupsOpened,
            Counter::GroupPx,
            Counter::RetainedHits,
            Counter::ScanItems,
            Counter::Bands,
            Counter::BandThreads,
            Counter::GroupsEmpty,
            Counter::GroupPxPainted,
            Counter::RetainedMisses,
            Counter::RetainedInvalidations,
            Counter::RetainedPatched,
            Counter::CellsBuilt,
        ];
        assert_eq!(all.len(), COUNTERS, "harness: every counter is listed");
        let band = std::thread::spawn(move || {
            for (i, counter) in all.iter().copied().enumerate() {
                count(counter, 10 + i as u64);
            }
            take()
        })
        .join()
        .expect("a band thread");
        merge(&band);
        let mut expected = CpuFrameCounters { items_painted: 5, ..Default::default() };
        expected.add(&band);
        let merged = take();
        assert_eq!(merged, expected);
        assert_eq!(merged.items_painted, 5 + 13, "this thread's 5 and the band's 10 + 3");
        assert_eq!(merged.retained_invalidations, 26, "the seventeenth counter, 10 + 16");
        assert_eq!(merged.cells_built, 28, "the last counter, 10 + 18");
    }
}
