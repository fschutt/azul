//! The state a painting thread keeps between its draws belongs to the glyph cache it paints
//! with: two caches never see each other's counters, shadows or halos, whichever thread they are
//! used on, and the bands of a repaint share the frame's stores for as long as they run.

use azul_core::{
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    resources::RendererResources,
};
use azul_css::props::basic::ColorU;

use super::*;
use crate::{
    cpu_counters::CpuFrameCounters,
    cpurender::{
        raster::empty_font_manager, render_display_list_damaged, AzulPixmap, CpuRenderState,
        ScrollOffsetMap,
    },
    glyph_cache::GlyphCache,
    solver3::display_list::{BorderRadius, DisplayList, DisplayListItem},
};

/// A list of `rects` opaque boxes side by side.
fn list_of(rects: usize) -> DisplayList {
    let items = (0..rects)
        .map(|i| DisplayListItem::Rect {
            bounds: LogicalRect::new(
                LogicalPosition::new(i as f32 * 12.0, 0.0),
                LogicalSize::new(10.0, 10.0),
            )
            .into(),
            color: ColorU { r: 200, g: 30, b: 30, a: 255 },
            border_radius: BorderRadius::default(),
        })
        .collect();
    DisplayList { items, ..Default::default() }
}

/// `list` painted once, all of it, into a fresh frame with `cache`.
fn paint(list: &DisplayList, cache: &mut GlyphCache) {
    let (resources, fonts) = (RendererResources::default(), empty_font_manager());
    let mut frame = AzulPixmap::new(200, 40).expect("a pixmap");
    frame.fill(255, 255, 255, 255);
    let state = CpuRenderState::new(ScrollOffsetMap::new());
    let whole = [LogicalRect::new(LogicalPosition::new(0.0, 0.0), LogicalSize::new(200.0, 40.0))];
    render_display_list_damaged(list, &mut frame, 1.0, &resources, &fonts, cache, &state, &whole)
        .expect("a frame");
}

/// Two windows' caches used one after the other on one thread: what each painted is its own.
/// (The counters were thread-locals: both windows' frames summed into the thread's.)
/// Mutation: counters kept by the thread instead of the cache make `a` and `b` equal.
#[test]
fn two_caches_on_one_thread_count_what_they_paint_apart() {
    let list = list_of(6);
    let (mut a, mut b) = (GlyphCache::new(), GlyphCache::new());
    paint(&list, &mut a);
    paint(&list, &mut b);
    paint(&list, &mut a);
    let (of_a, of_b) = (a.counts().take(), b.counts().take());
    assert!(of_b.items_painted >= 6, "premise: a frame paints its items: {of_b:?}");
    assert_eq!(of_a.items_painted, 2 * of_b.items_painted, "{of_a:?} against {of_b:?}");
    assert_eq!(of_a.px_cleared, 2 * of_b.px_cleared, "{of_a:?} against {of_b:?}");
    assert_eq!(a.counts().take(), CpuFrameCounters::default(), "taking zeroes");
}

/// Caches painted at the same time on several threads: each counts its own thread's frames and
/// nothing a neighbour does.
#[test]
fn caches_painted_side_by_side_count_apart() {
    let list = list_of(6);
    let one = {
        let mut cache = GlyphCache::new();
        paint(&list, &mut cache);
        cache.counts().take()
    };
    let counted: Vec<CpuFrameCounters> = std::thread::scope(|scope| {
        let handles: Vec<_> = (1..=4_usize)
            .map(|times| {
                let list = &list;
                scope.spawn(move || {
                    let mut cache = GlyphCache::new();
                    for _ in 0..times {
                        paint(list, &mut cache);
                    }
                    cache.counts().take()
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().expect("a painting thread")).collect()
    });
    for (i, counted) in counted.iter().enumerate() {
        assert_eq!(counted.items_painted, (i as u64 + 1) * one.items_painted, "thread {i}: {counted:?}");
    }
}

/// A store shared with the bands of a repaint is the one lock every holder reaches, and comes
/// back as the cache's own, with what the bands kept, once they are gone.
#[test]
fn a_store_shared_with_bands_comes_back_with_what_they_kept() {
    let mut store: SharedOrOwn<Vec<u32>> = SharedOrOwn::default();
    store.with(|kept| kept.push(1));
    let shared = store.share();
    let again = store.share();
    assert!(Arc::ptr_eq(&shared, &again), "a store that is shared hands out the lock it is behind");
    shared.lock().expect("the lock").push(2);
    store.with(|kept| kept.push(3));
    // A band that still holds the lock keeps the store behind it: nothing is lost.
    store.unshare();
    assert!(matches!(store, SharedOrOwn::Shared(_)), "held by a band: left behind its lock");
    drop((shared, again));
    store.unshare();
    assert!(matches!(store, SharedOrOwn::Own(_)), "no holder left: the cache's own again");
    assert_eq!(store.with(|kept| kept.clone()), vec![1, 2, 3]);
}

/// What the bands of a repaint share is the frame's own: its log notes and its stores, with the
/// band's thread marked as one; the frame's own cache is not.
#[test]
fn a_bands_scratch_is_marked_and_shares_the_frames_notes_and_stores() {
    let mut frame = PaintScratch::default();
    let share = frame.share_with_bands();
    let band = PaintScratch::for_band(&share);
    assert!(band.in_band && !frame.in_band);
    assert!(Arc::ptr_eq(&frame.said, &band.said), "a window says a thing once, however many bands");
    assert!(matches!((&band.shadows, &band.halos), (SharedOrOwn::Shared(_), SharedOrOwn::Shared(_))));
    assert_eq!(frame.composite_threads(), crate::cpurender::bands::max_threads());
    assert_eq!(band.composite_threads(), 1, "a composite inside a band starts no threads");
    drop((share, band));
    frame.take_back_from_bands();
    assert!(matches!((&frame.shadows, &frame.halos), (SharedOrOwn::Own(_), SharedOrOwn::Own(_))));
}

/// The text runs of a cache take the rasteriser the run before gave back; the pool made one.
#[test]
fn text_runs_take_the_rasteriser_the_run_before_gave_back() {
    let mut pool = TextRasterPool::default();
    for _ in 0..3 {
        let raster = pool.take();
        pool.give_back(raster);
    }
    assert_eq!(pool.made(), 1, "three runs, one rasteriser");
    pool.set_pooling(false);
    for _ in 0..2 {
        let raster = pool.take();
        pool.give_back(raster);
    }
    assert_eq!(pool.made(), 3, "without the pool every run makes its own");
}

/// Memory pressure lets go of what is only kept to make the next frame cheaper: the shadows and
/// the halos; what the frame counted stays.
#[test]
fn releasing_the_kept_pictures_leaves_the_counters() {
    let mut cache = GlyphCache::new();
    cache.scratch.shadows.with(|shadows| shadows.2 = 7);
    cache.counts().add(crate::cpu_counters::Counter::ItemsWalked, 5);
    cache.release_kept();
    assert_eq!(cache.scratch.shadows.with(|shadows| shadows.2), 0, "the shadows are gone");
    assert_eq!(cache.counts().peek().items_walked, 5, "the counters stay");
}
