//! What the thread that paints keeps from one draw to the next, owned by the glyph cache it paints
//! with.
//!
//! The rasteriser used to keep these in thread-locals (a counter per painting thread, the pooled
//! text rasteriser, the blurred shadows, the kept halos ...), with a hand-over dance when a repaint
//! was cut into bands: a band thread starts with no thread-locals of its own, so the frame's
//! thread moved its caches behind a lock, set them as "the frame's" on each band thread and took
//! them back. Now the state is a value: a [`PaintScratch`] is a field of every
//! [`GlyphCache`](crate::glyph_cache::GlyphCache), which is the one mutable object that travels
//! through every paint function already (one per window, and one per band of a repaint), and
//! the bands share what they share through [`SharedOrOwn`] handles that the cache hands out
//! ([`PaintScratch::share_with_bands`]) and takes back ([`PaintScratch::take_back_from_bands`]).
//! Nothing is process-wide, so two windows, two frames and two tests never see each other's
//! counters, shadows or halos.

use std::{
    collections::BTreeSet,
    sync::{atomic::AtomicBool, Arc, Mutex, PoisonError},
};

use agg_rust::{
    basics::FillingRule, rasterizer_scanline_aa::RasterizerScanlineAa, scanline_u::ScanlineU8,
};

use super::{pixmap::TintTable, raster::ShadowCache, text_halo::CpuHaloMemo};
use crate::cpu_counters::FrameCounts;

/// A store that the thread painting a frame owns, or that the bands of one repaint share behind a
/// lock.
///
/// Outside a repaint cut into bands the store is the cache's own and no lock is taken. While the
/// bands run, the frame's cache moves its store behind a lock ([`Self::share`]) that every band's
/// cache reaches it through; when they are done the frame's cache takes it back
/// ([`Self::unshare`]).
pub(crate) enum SharedOrOwn<T> {
    /// The store of this cache alone.
    Own(T),
    /// The store of the frame, shared with the bands of its repaint.
    Shared(Arc<Mutex<T>>),
}

impl<T: Default> Default for SharedOrOwn<T> {
    fn default() -> Self {
        Self::Own(T::default())
    }
}

impl<T: Default> SharedOrOwn<T> {
    /// `f` with the store to read, behind its lock when it is shared.
    pub(crate) fn peek<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        match self {
            Self::Own(own) => f(own),
            Self::Shared(shared) => f(&shared.lock().unwrap_or_else(PoisonError::into_inner)),
        }
    }

    /// `f` with the store, behind its lock when it is shared.
    pub(crate) fn with<R>(&mut self, f: impl FnOnce(&mut T) -> R) -> R {
        match self {
            Self::Own(own) => f(own),
            Self::Shared(shared) => {
                f(&mut shared.lock().unwrap_or_else(PoisonError::into_inner))
            }
        }
    }

    /// The store behind a lock that the bands of a repaint share: moved there now when it was
    /// this cache's own, the lock it already sits behind otherwise. This cache reaches it through
    /// the lock from here on, as the bands do.
    pub(crate) fn share(&mut self) -> Arc<Mutex<T>> {
        match self {
            Self::Shared(shared) => Arc::clone(shared),
            Self::Own(own) => {
                let shared = Arc::new(Mutex::new(core::mem::take(own)));
                *self = Self::Shared(Arc::clone(&shared));
                shared
            }
        }
    }

    /// The store back as this cache's own, with what the bands kept in it. Left behind its lock
    /// (the cache goes on reaching it there) if a band still holds the lock: nothing is lost, the
    /// next repaint's `share` finds it shared.
    pub(crate) fn unshare(&mut self) {
        if !matches!(self, Self::Shared(_)) {
            return;
        }
        let Self::Shared(shared) = core::mem::replace(self, Self::Own(T::default())) else {
            return;
        };
        *self = match Arc::try_unwrap(shared) {
            Ok(lock) => Self::Own(lock.into_inner().unwrap_or_else(PoisonError::into_inner)),
            Err(shared) => Self::Shared(shared),
        };
    }
}

/// The rasteriser and scanline a text run sweeps its glyph cells with, taken from a
/// [`TextRasterPool`] and given back to it.
pub(crate) struct TextRaster {
    ras: RasterizerScanlineAa,
    sl: ScanlineU8,
}

impl TextRaster {
    /// The rasteriser (empty, non-zero winding) and the scanline.
    pub(crate) fn parts(&mut self) -> (&mut RasterizerScanlineAa, &mut ScanlineU8) {
        self.ras.filling_rule(FillingRule::NonZero);
        (&mut self.ras, &mut self.sl)
    }
}

/// The text rasteriser a painting thread keeps between its text runs. Every run used to make its
/// own (the rasteriser's cell storage, the scanline's span and cover tables, grown from nothing)
/// and drop them after one sweep; a map draws thousands of runs a frame. A run takes the pair the
/// run before left ([`Self::take`]) and gives it back emptied ([`Self::give_back`]).
pub(crate) struct TextRasterPool {
    pair: Option<(RasterizerScanlineAa, ScanlineU8)>,
    /// How many rasterisers had to be made: a run that finds the pair the run before left makes
    /// none. What a test pins to say "the runs of a frame share one".
    made: usize,
    /// Whether a given-back pair is kept for the next run (it is, outside tests that compare with
    /// the time before the pool: "make one per run").
    pooling: bool,
}

impl Default for TextRasterPool {
    fn default() -> Self {
        Self { pair: None, made: 0, pooling: true }
    }
}

impl TextRasterPool {
    /// The pair left by the run before, or a new one.
    pub(crate) fn take(&mut self) -> TextRaster {
        let (ras, sl) = self.pair.take().unwrap_or_else(|| {
            self.made += 1;
            (RasterizerScanlineAa::new(), ScanlineU8::new())
        });
        TextRaster { ras, sl }
    }

    /// Hand a run's pair back, emptied, for the next run.
    pub(crate) fn give_back(&mut self, raster: TextRaster) {
        let TextRaster { mut ras, sl } = raster;
        ras.reset();
        if self.pooling {
            self.pair = Some((ras, sl));
        }
    }

    /// How many rasterisers this pool has had to make.
    pub(crate) const fn made(&self) -> usize {
        self.made
    }

    /// Tests: keep (or not) the pair a run gives back, to compare with the time before the pool.
    #[cfg(test)]
    pub(crate) fn set_pooling(&mut self, on: bool) {
        self.pooling = on;
        if !on {
            self.pair = None;
        }
    }
}

/// What a painting has already told the log, so that a broken frame does not say it again at
/// 60 Hz: one per window, shared by its glyph cache and by the caches of the bands painted for it.
#[derive(Debug, Default)]
pub(crate) struct SaidOnce {
    /// A `RenderImageCallback` image was painted as the flat grey placeholder.
    pub(crate) callback_placeholder: AtomicBool,
    /// The font hashes layout emitted that its `FontManager` could not resolve.
    pub(crate) unresolved_fonts: Mutex<BTreeSet<u64>>,
}

/// What the bands of one repaint share with the thread that paints its frame
/// ([`PaintScratch::share_with_bands`]).
pub(crate) struct BandShare {
    halos: Arc<Mutex<CpuHaloMemo>>,
    shadows: Arc<Mutex<ShadowCache>>,
    said: Arc<SaidOnce>,
}

/// What the thread that paints keeps from one draw to the next: see the module.
#[derive(Default)]
pub(crate) struct PaintScratch {
    /// What this thread's painting did, counted ([`FrameCounts`]).
    pub(crate) counts: FrameCounts,
    /// Whether this cache belongs to a thread that paints a band of a repaint: a composite inside
    /// a band does not start threads of its own.
    pub(crate) in_band: bool,
    /// The most threads a composite painted with this cache may use, as the window's settings
    /// say (`CpuRenderState::paint_threads`; `0` is the default, `bands::max_threads`).
    pub(crate) threads: usize,
    /// The rasteriser and scanline the text runs of this thread sweep their glyph cells with.
    pub(crate) text_raster: TextRasterPool,
    /// The colour [`blit_tinted_coverage`](super::pixmap::blit_tinted_coverage) tinted with last.
    pub(crate) tint: TintTable,
    /// The blurred box shadows kept: by what their pixels depend on, bounded in bytes.
    pub(crate) shadows: SharedOrOwn<ShadowCache>,
    /// The CPU renderer's kept text halos and turned text pictures.
    pub(crate) halos: SharedOrOwn<CpuHaloMemo>,
    /// What this window's painting has told the log.
    pub(crate) said: Arc<SaidOnce>,
}

impl PaintScratch {
    /// The scratch of a thread that paints a band of a repaint: its own counters, text
    /// rasteriser and tint table, and the frame's kept halos, shadows and log notes.
    pub(crate) fn for_band(share: &BandShare) -> Self {
        Self {
            counts: FrameCounts::new(),
            in_band: true,
            threads: 1,
            text_raster: TextRasterPool::default(),
            tint: TintTable::default(),
            shadows: SharedOrOwn::Shared(Arc::clone(&share.shadows)),
            halos: SharedOrOwn::Shared(Arc::clone(&share.halos)),
            said: Arc::clone(&share.said),
        }
    }

    /// The most threads a composite painted with this scratch may use: one inside a band of a
    /// repaint (the band's thread is the composite's), else up to what the window's settings
    /// say ([`Self::threads`], `0`: [`max_threads`](super::bands::max_threads)).
    pub(crate) fn composite_threads(&self) -> usize {
        if self.in_band {
            1
        } else {
            super::bands::threads_for(self.threads)
        }
    }

    /// Put what the bands of a repaint share behind locks, and hand it out: a band thread starts
    /// with nothing of its own, and one that blurred every shadow of its band or drew every halo
    /// again on every frame would lose what keeping them gains. Give back with
    /// [`Self::take_back_from_bands`] when the bands are done.
    pub(crate) fn share_with_bands(&mut self) -> BandShare {
        BandShare {
            halos: self.halos.share(),
            shadows: self.shadows.share(),
            said: Arc::clone(&self.said),
        }
    }

    /// Take what the bands shared back as this cache's own, with what they kept. The bands'
    /// caches must be gone (dropped or absorbed) by now; a store that a band still holds stays
    /// behind its lock.
    pub(crate) fn take_back_from_bands(&mut self) {
        self.halos.unshare();
        self.shadows.unshare();
    }

    /// Drop what is kept only to make the next frame cheaper: the blurred shadows and the halos
    /// (memory pressure, a window that is not shown). The counters and the log notes stay.
    pub(crate) fn release_kept(&mut self) {
        self.shadows.with(|shadows| *shadows = ShadowCache::default());
        self.halos.with(CpuHaloMemo::release);
    }
}

#[cfg(test)]
#[path = "scratch_test.rs"]
mod scratch_test;
