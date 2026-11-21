//! What a CPU frame did: counted, so a test can pin the work of a frame instead of its time.
//!
//! The rasteriser and the composite count into the [`FrameCounts`] of the glyph cache they paint
//! with (one relaxed load and store, no locked add) while a frame renders; the backend zeroes them
//! when the frame starts and takes them at its end, and the window keeps the last frame's and the
//! sum since the counters were reset (`FrameReport::cpu`). A zoom step that paints all the items of
//! a view again, a pan that draws every halo again, a surface that is allocated and dropped every
//! frame are numbers here, and a count does not depend on how loaded the machine is.
//!
//! A band of a repaint is painted on a thread of its own with a glyph cache of its own, so it
//! counts into that cache's counts; the frame's counters are the sum ([`FrameCounts::merge`]),
//! whichever thread painted what. Nothing here is process-wide: every window's backend has its own
//! cache and so its own counters, and two frames painted side by side never see each other's.

use core::sync::atomic::{AtomicU64, Ordering};

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

/// What one frame did, [`FrameCounts::take`]n at its end.
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
    pub const fn add(&mut self, other: &Self) {
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

/// The counters of the frame one painting thread is working on: a cell per [`Counter`].
///
/// Held by the glyph cache the thread paints with (`GlyphCache::counts`): the frame's own thread
/// counts into its window's cache, and each band of a repaint into the cache of the thread that
/// paints it. A cell is written by ONE thread at a time, the thread that paints with the cache, so
/// [`Self::add`] is a relaxed load and a relaxed store (a plain add on every target), not a locked
/// read-modify-write; the cells are atomics so that the cache stays `Sync`, which the window's
/// cache has to be while the bands of a repaint read it.
#[derive(Debug, Default)]
pub struct FrameCounts {
    cells: [AtomicU64; COUNTERS],
}

impl FrameCounts {
    /// Counters that all read zero.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Count `n` more of `counter`.
    #[inline]
    pub fn add(&self, counter: Counter, n: u64) {
        let cell = &self.cells[counter as usize];
        cell.store(cell.load(Ordering::Relaxed) + n, Ordering::Relaxed);
    }

    /// Zero every counter: the start of a frame.
    pub fn reset(&self) {
        for cell in &self.cells {
            cell.store(0, Ordering::Relaxed);
        }
    }

    /// What was counted, without zeroing it.
    #[must_use]
    pub fn peek(&self) -> CpuFrameCounters {
        self.read(|cell| cell.load(Ordering::Relaxed))
    }

    /// What was counted, zeroed: the end of a frame.
    #[must_use]
    pub fn take(&self) -> CpuFrameCounters {
        self.read(|cell| cell.swap(0, Ordering::Relaxed))
    }

    fn read(&self, get: impl Fn(&AtomicU64) -> u64) -> CpuFrameCounters {
        let get = |c: Counter| get(&self.cells[c as usize]);
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
    }

    /// Add `counted` - what another thread counted for this thread's frame, [`Self::take`]n
    /// there - to these counters. A band of a repaint is painted on a thread of its own and counts
    /// into the cache of that thread; the frame's counters are the sum, whichever thread painted
    /// what.
    pub fn merge(&self, counted: &CpuFrameCounters) {
        self.add(Counter::PxCleared, counted.px_cleared);
        self.add(Counter::PxBlitted, counted.px_blitted);
        self.add(Counter::ItemsWalked, counted.items_walked);
        self.add(Counter::ItemsPainted, counted.items_painted);
        self.add(Counter::TextRuns, counted.text_runs);
        self.add(Counter::Glyphs, counted.glyphs);
        self.add(Counter::HalosDrawn, counted.halos_drawn);
        self.add(Counter::GroupsOpened, counted.groups_opened);
        self.add(Counter::GroupPx, counted.group_px);
        self.add(Counter::RetainedHits, counted.retained_hits);
        self.add(Counter::ScanItems, counted.scan_items);
        self.add(Counter::Bands, counted.bands);
        self.add(Counter::BandThreads, counted.band_threads);
        self.add(Counter::GroupsEmpty, counted.groups_empty);
        self.add(Counter::GroupPxPainted, counted.group_px_painted);
        self.add(Counter::RetainedMisses, counted.retained_misses);
        self.add(Counter::RetainedInvalidations, counted.retained_invalidations);
        self.add(Counter::RetainedPatched, counted.retained_patched);
        self.add(Counter::CellsBuilt, counted.cells_built);
    }
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

/// What the scans of this thread did with the text halos: the numbers of the last
/// scan (`cpurender::within_halo_budget`, one per frame that registers its images), the
/// sums since the thread began and what is kept now. [`text_halo_stats`].
///
/// The frame recorder of an app reads it per frame: `created` and `pending` fall
/// to 0 a few frames after a view changed, `evicted` counts pictures of earlier
/// frames only, and `frame_halos` / `frame_box_bytes` say what one frame needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(C)]
pub struct TextHaloStats {
    /// The scans so far.
    pub scans: u64,
    /// The halo pictures drawn so far.
    pub created_total: u64,
    /// The halo pictures dropped so far.
    pub evicted_total: u64,
    /// The last scan: pictures drawn.
    pub created: u64,
    /// The last scan: pictures dropped to make room (never one the scan used).
    pub evicted: u64,
    /// The last scan: halos left for a later frame by the frame's time budget.
    pub pending: u64,
    /// The last scan: halos not drawn because the pictures the scan itself uses
    /// fill the cache (`MAX_HALO_BYTES`, `MAX_HALOS`).
    pub unkept: u64,
    /// The last scan: halos without a picture of their own that were painted with
    /// a stand-in (the picture of the same text at another size).
    pub stand_ins: u64,
    /// The last scan: small pictures drawn to stand in for halos that had none.
    pub previews: u64,
    /// The last scan: the kept pictures it used, each counted once ...
    pub frame_halos: u64,
    /// ... and the bytes they would hold as the whole boxes their glyphs can reach
    /// (what was kept before a picture was cut to its ink: a cache of 32 MB and
    /// 1,024 pictures held a frame only while these two stayed under that).
    pub frame_box_bytes: u64,
    /// The pictures kept now.
    pub kept: u64,
    /// The bytes they hold.
    pub kept_bytes: u64,
}

/// What the scans of this thread did with the text halos ([`TextHaloStats`]): the halo cache's
/// numbers with the CPU renderer (feature `cpurender`), all zero without it, which keeps no halos.
#[must_use]
pub fn text_halo_stats() -> TextHaloStats {
    #[cfg(feature = "cpurender")]
    {
        crate::cpurender::text_halo_stats()
    }
    #[cfg(not(feature = "cpurender"))]
    {
        TextHaloStats::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_are_taken_and_zeroed_per_owner() {
        let counts = FrameCounts::new();
        counts.add(Counter::ItemsWalked, 3);
        counts.add(Counter::ItemsWalked, 4);
        counts.add(Counter::HalosDrawn, 1);
        // Another owner's counts are not this one's: nothing is shared between two of them.
        let other = FrameCounts::new();
        std::thread::scope(|scope| {
            scope.spawn(|| other.add(Counter::ItemsWalked, 100));
        });
        assert_eq!(other.peek().items_walked, 100);
        let taken = counts.take();
        assert_eq!((taken.items_walked, taken.halos_drawn, taken.glyphs), (7, 1, 0));
        assert_eq!(counts.take(), CpuFrameCounters::default(), "taking zeroes");
    }

    /// A1-01: what a band thread counted reaches the frame's counters, every counter of it.
    /// Mutation: a counter left out of `merge` (or of `take`) fails the comparison with `add`.
    #[test]
    fn what_another_thread_counted_is_merged_into_this_threads_frame() {
        let frame = FrameCounts::new();
        frame.add(Counter::ItemsPainted, 5);
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
            let counts = FrameCounts::new();
            for (i, counter) in all.iter().copied().enumerate() {
                counts.add(counter, 10 + i as u64);
            }
            counts.take()
        })
        .join()
        .expect("a band thread");
        frame.merge(&band);
        let mut expected = CpuFrameCounters { items_painted: 5, ..Default::default() };
        expected.add(&band);
        let merged = frame.take();
        assert_eq!(merged, expected);
        assert_eq!(merged.items_painted, 5 + 13, "this thread's 5 and the band's 10 + 3");
        assert_eq!(merged.retained_invalidations, 26, "the seventeenth counter, 10 + 16");
        assert_eq!(merged.cells_built, 28, "the last counter, 10 + 18");
    }
}
