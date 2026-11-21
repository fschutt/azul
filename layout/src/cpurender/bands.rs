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
//!   group surfaces, a glyph cache with the text rasteriser and the counters of its thread (the
//!   counters are summed into the frame's when the band is done);
//! - shared behind a lock, for as long as the bands run: the frame's kept halos and blurred
//!   shadows (`GlyphCache::scratch`): every band's cache reaches them there, and the frame's
//!   cache takes them back when the bands are done.
//!
//! A band paints in the FRAME's coordinates, not in its own: every position the walk computes
//! is the one a single pass over the whole rect computes, so the pixels are the same ones (a
//! band is not a translated picture, whose positions would round differently). To do that
//! without two threads holding the frame, each band paints into a pixmap of its own that is as
//! large as the frame but only ever touched in the band's rows (it comes zeroed from the
//! allocator, so the rest costs no memory): the band's rows are copied in, the pass runs, and
//! the band's rect is copied back into the rows of the frame that belong to the band alone.
//!
//! Why the bands of a rect give the pixels of one pass over the rect: a pass writes inside its
//! clip only, and a pixel it writes depends on the items, on the positions (the same here), and
//! on the pixel's own row of the target (LCD text reads its horizontal neighbours; an opacity
//! group, a mask and a rounded clip blend pixel by pixel; blurs and halos are made in buffers of
//! their own from the items, and a group is composited from a surface of its own). Nothing reads
//! the rows above or below. The one thing that looks further is a text run's choice of path
//! (LCD or grayscale, pre-blended or blended) by the backdrop under the WHOLE run inside the
//! clip: a run cut by a seam whose backdrop is opaque or uniform on one side only can choose
//! differently on the two sides, as a damage rect cutting it does. A window cleared to an
//! opaque colour has an opaque backdrop everywhere outside opacity and filter groups; a window
//! cleared to anything else is painted in one pass.
//!
//! Memory: a band holds at most [`MAX_BAND_BYTES`] of the frame's rows, so a thread's scratch
//! is that plus what its pass allocates (group surfaces of the parts that land in the band).

use std::sync::{Arc, Mutex, OnceLock};

use azul_core::resources::RendererResources;
use azul_css::props::basic::FontRef;

use super::{
    pixmap::AzulPixmap,
    raster::{paint_damage_rect, CpuRenderState, SnappedRect},
};
use crate::{
    cpu_counters::Counter, glyph_cache::GlyphCache, solver3::display_list::DisplayList,
    text3::cache::FontManager,
};

/// Fewest pixels of damage one band is worth: a pass walks the list once whatever it paints,
/// and a thread costs tens of microseconds to start.
const MIN_BAND_PX: i64 = 200_000;

/// Fewest rows of a band: every seam paints the items that cross it twice.
const MIN_BAND_ROWS: i32 = 48;

/// Most bytes of the frame's rows one band copies (its thread's scratch): 8 MB is 819 rows of a
/// 2560 px frame, 409 of a 5120 px one.
pub(crate) const MAX_BAND_BYTES: usize = 8 * 1024 * 1024;

/// Bands per thread: threads take bands from a queue, so a band heavy with text does not leave
/// the other threads idle.
const BANDS_PER_THREAD: usize = 2;

/// The most threads a repaint uses unless it is told otherwise: the other cores belong to the
/// tile workers and to whatever else the machine runs.
pub const DEFAULT_MAX_PAINT_THREADS: usize = 4;

/// The threads the machine has (asked once: the answer reads `sysctl` or the cgroup files).
fn machine_threads() -> usize {
    // A hardware fact, computed once per process.
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
    // Process-wide configuration, read once from the environment (`AZ_CPU_THREADS`).
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

/// The bands the damage rect `[x0, y0, x1, y1]` (device px, half-open) of a frame
/// `frame_width` px wide is painted in on `threads` threads: rows of the rect, top to bottom,
/// each `[x0, y, x1, y']`. One band - the rect itself - when it is not worth more.
///
/// Example: the view of a 2560 x 1440 window, `[25, 19, 2535, 1421]` (2510 x 1402 px), on 4
/// threads: 8 bands (2 per thread; the pixels would allow 17, the rows 29) of
/// ceil(1402 / 8) = 176 rows, the last of 170: y = 19, 195, 371, 547, 723, 899, 1075, 1251.
/// A pan's strip `[2522, 19, 2535, 1421]` (18,226 px) is one band.
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_possible_wrap)] // a rect inside a pixmap
pub(crate) fn plan_bands(rect: [i32; 4], frame_width: u32, threads: usize) -> Vec<[i32; 4]> {
    let [x0, y0, x1, y1] = rect;
    let (w, h) = (i64::from(x1) - i64::from(x0), i64::from(y1) - i64::from(y0));
    if threads <= 1 || w <= 0 || h <= 0 {
        return vec![rect];
    }
    let by_px = (w * h / MIN_BAND_PX) as usize;
    let by_rows = (h / i64::from(MIN_BAND_ROWS)) as usize;
    let wanted = (threads * BANDS_PER_THREAD).min(by_px).min(by_rows);
    if wanted <= 1 {
        return vec![rect];
    }
    let row_bytes = (frame_width as usize * 4).max(1);
    let most_rows = (MAX_BAND_BYTES / row_bytes).max(MIN_BAND_ROWS as usize);
    let rows = (h as usize).div_ceil(wanted).min(most_rows).max(1) as i32;
    let mut bands = Vec::with_capacity(wanted);
    let mut y = y0;
    while y < y1 {
        let end = y.saturating_add(rows).min(y1);
        bands.push([x0, y, x1, end]);
        y = end;
    }
    bands
}

/// Paint the damage rect `sr` of `pixmap` (`raster::paint_damage_rect`): in bands on several
/// threads when it is large and the render state allows threads, else as one pass on this
/// thread. The pixels are the same either way (see the module).
///
/// Returns how many items were painted and culled, summed over the bands.
#[allow(clippy::too_many_arguments)]
pub(crate) fn paint_rect(
    display_list: &DisplayList,
    pixmap: &mut AzulPixmap,
    dpi_factor: f32,
    renderer_resources: &RendererResources,
    font_manager: &FontManager<FontRef>,
    glyph_cache: &mut GlyphCache,
    render_state: &CpuRenderState,
    sr: &SnappedRect,
    clear: Option<(u8, u8, u8, u8)>,
) -> Result<(usize, usize), String> {
    let threads = threads_for(render_state.paint_threads);
    // A target cleared to a colour that is not opaque has text that picks its path by what lies
    // under each run: one pass (see the module).
    let cleared_to = clear.map_or(render_state.clear_color[3], |c| c.3);
    let bands = if cleared_to == 255 {
        plan_bands([sr.x0, sr.y0, sr.x1, sr.y1], pixmap.width(), threads)
    } else {
        Vec::new()
    };
    if bands.len() <= 1 {
        return paint_damage_rect(
            display_list,
            pixmap,
            dpi_factor,
            renderer_resources,
            font_manager,
            glyph_cache,
            render_state,
            sr,
            clear,
        );
    }
    paint_bands(
        display_list,
        pixmap,
        dpi_factor,
        renderer_resources,
        font_manager,
        glyph_cache,
        render_state,
        &bands,
        threads,
        clear,
    )
}

/// One band `[x0, y0, x1, y1]` of a frame of `frame` px, painted into `rows`: the frame's rows
/// `y0..y1`, all `frame.0` px of each, which this call alone holds.
///
/// Frame pixel `(x, y)` of the band is byte `((y - y0) * frame.0 + x) * 4` of `rows` and byte
/// `(y * frame.0 + x) * 4` of the band's own pixmap, which has the frame's size: the pass
/// paints there with the frame's coordinates. Example: band `[25, 195, 2535, 371]` of a
/// 2560 x 1440 frame: `rows` holds 176 x 10,240 bytes; its first byte is the frame's byte
/// 195 x 10,240 = 1,996,800, and the rect copied back starts 25 x 4 = 100 bytes into each row.
#[allow(clippy::too_many_arguments)]
#[allow(clippy::cast_sign_loss)] // a band inside the frame
fn paint_band(
    display_list: &DisplayList,
    rows: &mut [u8],
    frame: (u32, u32),
    band: [i32; 4],
    dpi_factor: f32,
    renderer_resources: &RendererResources,
    font_manager: &FontManager<FontRef>,
    glyph_cache: &mut GlyphCache,
    render_state: &CpuRenderState,
    clear: Option<(u8, u8, u8, u8)>,
) -> Result<(usize, usize), String> {
    let stride = frame.0 as usize * 4;
    let [x0, y0, x1, y1] = band;
    let (first, end) = (y0 as usize * stride, y1 as usize * stride);
    let mut own = AzulPixmap::new_transparent(frame.0, frame.1)
        .ok_or_else(|| "cannot create a band's pixmap".to_string())?;
    {
        let own_rows = own
            .data_mut()
            .get_mut(first..end)
            .filter(|own_rows| own_rows.len() == rows.len())
            .ok_or_else(|| "a band outside its frame".to_string())?;
        own_rows.copy_from_slice(rows);
    }
    let sr = SnappedRect::from_physical(x0, y0, x1, y1, dpi_factor);
    let counted = paint_damage_rect(
        display_list,
        &mut own,
        dpi_factor,
        renderer_resources,
        font_manager,
        glyph_cache,
        render_state,
        &sr,
        clear,
    )?;
    // Back into the frame: the band's rect, and nothing a pass may have written around it.
    let (left, right) = (x0 as usize * 4, x1 as usize * 4);
    let painted = own.data();
    for (r, row) in rows.chunks_exact_mut(stride).enumerate() {
        let at = first + r * stride;
        row[left..right].copy_from_slice(&painted[at + left..at + right]);
    }
    Ok(counted)
}

/// An empty glyph cache with the text settings of `like`: what stands in the window's place while
/// its cache is shared with the bands. (The window's own settings, not the defaults: a cache that
/// painted with the defaults would draw another text than one pass does.)
fn empty_cache_like(like: &GlyphCache) -> GlyphCache {
    let mut cache = GlyphCache::new();
    let _ = cache.set_text_aa(like.text_aa());
    let _ = cache.set_lcd_subpixel_order(like.lcd_subpixel_order());
    cache
}

/// The `bands` of one damage rect (rows of it, top to bottom, adjacent) painted on up to
/// `threads` threads, this one included.
#[allow(clippy::too_many_arguments)]
#[allow(clippy::cast_sign_loss)] // bands inside the frame
fn paint_bands(
    display_list: &DisplayList,
    pixmap: &mut AzulPixmap,
    dpi_factor: f32,
    renderer_resources: &RendererResources,
    font_manager: &FontManager<FontRef>,
    glyph_cache: &mut GlyphCache,
    render_state: &CpuRenderState,
    bands: &[[i32; 4]],
    threads: usize,
    clear: Option<(u8, u8, u8, u8)>,
) -> Result<(usize, usize), String> {
    let frame = (pixmap.width(), pixmap.height());
    let stride = frame.0 as usize * 4;
    let (Some(top), Some(bottom)) = (bands.first(), bands.last()) else {
        return Ok((0, 0));
    };
    // Each band gets the rows of the frame that are its own: disjoint slices, no lock on a pixel.
    let mut rest: &mut [u8] = pixmap
        .data_mut()
        .get_mut(top[1] as usize * stride..bottom[3] as usize * stride)
        .ok_or_else(|| "bands outside their frame".to_string())?;
    let mut jobs: Vec<([i32; 4], &mut [u8])> = Vec::with_capacity(bands.len());
    for band in bands {
        let bytes = (band[3] - band[1]) as usize * stride;
        if bytes > rest.len() {
            return Err("bands outside their frame".to_string());
        }
        let (rows, tail) = core::mem::take(&mut rest).split_at_mut(bytes);
        jobs.push((*band, rows));
        rest = tail;
    }
    let workers = threads.min(jobs.len()).max(1);
    let jobs = Mutex::new(jobs);

    // What the bands share with this thread's frame: its kept halos and box shadows, behind a
    // lock, and the window's glyph cache, behind an `Arc` and without one: every band reads
    // it (`GlyphCache::cached_cells`) and none writes it while the bands run - this thread
    // included, which holds an empty cache in its place until they are done. What a band
    // lacks it builds into a cache of its own, empty at the start of every repaint; the
    // window's cache takes those back below (`GlyphCache::absorb`), so no thread keeps a
    // second copy of the window's cells from frame to frame.
    let share = glyph_cache.scratch.share_with_bands();
    let in_the_windows_place = empty_cache_like(glyph_cache);
    let window_glyphs = Arc::new(core::mem::replace(glyph_cache, in_the_windows_place));
    let mut own_cache = GlyphCache::for_band(&window_glyphs, &share);
    let mut thread_caches: Vec<GlyphCache> =
        (1..workers).map(|_| GlyphCache::for_band(&window_glyphs, &share)).collect();
    // The caches hold what the bands share now; the frame's thread must be the only other
    // holder of it when it takes it back.
    drop(share);
    #[cfg(test)]
    let test_flags = super::raster::walk_test_flags();

    // A thread paints bands until the queue is empty. Which thread paints which band does not
    // change a pixel: a band's pass depends on the band alone.
    let work = |cache: &mut GlyphCache| -> Result<(usize, usize), String> {
        let mut counted = (0usize, 0usize);
        loop {
            let job = jobs
                .lock()
                .map_err(|_| "the band queue is poisoned".to_string())?
                .pop();
            let Some((band, rows)) = job else {
                return Ok(counted);
            };
            let (painted, skipped) = paint_band(
                display_list,
                rows,
                frame,
                band,
                dpi_factor,
                renderer_resources,
                font_manager,
                cache,
                render_state,
                clear,
            )?;
            counted.0 += painted;
            counted.1 += skipped;
        }
    };

    let mut outcomes: Vec<Result<(usize, usize), String>> = Vec::new();
    let own_outcome = std::thread::scope(|scope| {
        let handles: Vec<_> = thread_caches
            .iter_mut()
            .map(|cache| {
                let work = &work;
                scope.spawn(move || {
                    #[cfg(test)]
                    super::raster::set_walk_test_flags(test_flags);
                    work(cache)
                })
            })
            .collect();
        // This thread paints bands, too; its counts go into the frame's with the others'.
        let own = work(&mut own_cache);
        for handle in handles {
            outcomes.push(
                handle
                    .join()
                    .unwrap_or_else(|_| Err("a band thread panicked".to_string())),
            );
        }
        own
    });
    // The window's cache back in its place: every band's cache lets go of it first. Were one
    // held, the window would go on with an empty cache: a frame of builds, never another pixel.
    own_cache.release_window();
    for cache in &mut thread_caches {
        cache.release_window();
    }
    *glyph_cache = Arc::try_unwrap(window_glyphs)
        .unwrap_or_else(|_| empty_cache_like(&own_cache));
    // Then what the bands counted and built, in the order of the workers: the frame's counters
    // are the sum whichever thread painted what, and the window's cache holds the cells the
    // bands made. A band's cache is gone when it is absorbed, which leaves the window's cache
    // the only holder of the halos and shadows the bands shared: it takes them back.
    for cache in core::iter::once(own_cache).chain(thread_caches) {
        glyph_cache.counts().merge(&cache.counts().take());
        glyph_cache.absorb(cache);
    }
    glyph_cache.scratch.take_back_from_bands();

    glyph_cache.counts().add(Counter::Bands, bands.len() as u64);
    glyph_cache.counts().add(Counter::BandThreads, (workers - 1) as u64);
    let mut counted = own_outcome?;
    let mut failed = None;
    for result in outcomes {
        match result {
            Ok((painted, skipped)) => {
                counted.0 += painted;
                counted.1 += skipped;
            }
            Err(e) => failed = Some(e),
        }
    }
    failed.map_or(Ok(counted), Err)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A1-06: the bands of a rect are its rows, top to bottom, adjacent, covering it exactly.
    /// The hand-computed example of `plan_bands`. Mutation: `div_ceil` -> `/` leaves a ninth
    /// band of 2 rows (175 x 8 = 1400); dropping `.min(y1)` runs the last band past the rect.
    #[test]
    fn a_large_rect_is_cut_into_adjacent_bands_of_rows_and_a_small_one_is_not() {
        let view = [25, 19, 2535, 1421];
        let bands = plan_bands(view, 2560, 4);
        let tops: Vec<i32> = bands.iter().map(|b| b[1]).collect();
        assert_eq!(tops, vec![19, 195, 371, 547, 723, 899, 1075, 1251]);
        assert_eq!(bands.last().map(|b| b[3]), Some(1421));
        for pair in bands.windows(2) {
            assert_eq!(pair[0][3], pair[1][1], "adjacent: no row twice, none left out");
        }
        assert!(bands.iter().all(|b| b[0] == 25 && b[2] == 2535), "every band is as wide as the rect");
        // One thread, a pan's strip, a small box: one band, the rect itself.
        assert_eq!(plan_bands(view, 2560, 1), vec![view]);
        assert_eq!(plan_bands([2522, 19, 2535, 1421], 2560, 4), vec![[2522, 19, 2535, 1421]]);
        assert_eq!(plan_bands([0, 0, 400, 300], 2560, 4), vec![[0, 0, 400, 300]]);
        assert_eq!(plan_bands([10, 10, 10, 50], 2560, 4), vec![[10, 10, 10, 50]], "an empty rect");
        // 800 x 600 = 480,000 px: two bands' worth of pixels, whatever the threads.
        assert_eq!(plan_bands([0, 0, 800, 600], 800, 8), vec![[0, 0, 800, 300], [0, 300, 800, 600]]);
        // A frame so wide that 8 MB hold few rows: 16,384 px x 4 = 65,536 bytes a row, 128 rows.
        let wide = plan_bands([0, 0, 16_384, 2048], 16_384, 2);
        assert_eq!(wide.len(), 16, "2,048 rows in bands of 128, not 4 of 512");
        assert!(wide.iter().all(|b| (b[3] - b[1]) as usize * 16_384 * 4 <= MAX_BAND_BYTES));
    }

    /// A1-06 (a): a band's glyph cache is empty and has the window's text settings.
    /// Mutation: the settings left out of `GlyphCache::for_band` give a band the default (LCD).
    #[test]
    fn a_bands_glyph_cache_is_empty_and_has_the_windows_settings() {
        let grayscale = super::super::raster::TextAaSettings::grayscale();
        let order = crate::glyph_cache::LcdSubpixelOrder::Bgr;
        let mut window = GlyphCache::new();
        let _ = window.set_text_aa(grayscale);
        let _ = window.set_lcd_subpixel_order(order);
        let share = window.scratch.share_with_bands();
        let window = Arc::new(window);
        let cache = GlyphCache::for_band(&window, &share);
        assert!(cache.text_aa() == grayscale);
        assert!(cache.lcd_subpixel_order() == order);
        assert_eq!((cache.paths_len(), cache.cells_len(), cache.cells_built()), (0, 0, 0));
        assert!(cache.scratch.in_band, "a band's thread does not start threads for a composite");
        assert!(!window.scratch.in_band, "the window's cache is not a band's");
    }

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
