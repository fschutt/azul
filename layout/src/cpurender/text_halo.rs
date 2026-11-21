//! Text shadows drawn once and kept as pictures, for the GPU backend.
//!
//! `WebRender` draws a blurred text shadow as a picture of its own: an
//! offscreen surface, the glyphs drawn into it and two blur passes, on every
//! frame. A map draws a halo behind each of its labels and, on each frame of a
//! pan, every label again where it moved: a hundred and more blurred surfaces
//! per frame. Here the CPU renderer's text-shadow code draws a halo once, in
//! white, and it is kept by what its pixels depend on - the glyphs relative to
//! each other, their fonts and clips, the blur and the scale - and not by
//! where the text stands or by the shadow's colour. The compositor uploads it
//! as an image and paints it tinted with the shadow's colour, one textured
//! quad: a label that moves keeps its picture, and the GPU's halo is the CPU
//! renderer's halo.
//!
//! What is kept, and for how long ([`HaloCache`]): a frame's scan of the display
//! lists ([`within_halo_budget`]) never drops a picture it has used itself, so the
//! kept pictures follow what the frame shows, however many labels that is. A cache
//! of a fixed size smaller than one frame's labels dropped, for each halo it drew,
//! one the same frame showed, which the next frame drew again: the labels of a map
//! lost and regained their halos a few per frame, in turn, for as long as frames
//! were made (audit A5-01). Pictures of earlier frames go, the one used longest ago
//! first, when the bytes are needed; when the frame's own pictures fill the budget
//! the halos beyond it are not drawn at all (and no frame is owed for them).
//!
//! What a halo looks like while its picture is not drawn yet (a frame draws a
//! bounded number of new ones): the newest picture of the same text in the same
//! font at another size, stretched to where the new one will lie ([`TextHalo::stand_in`]).
//! A zoom that settles gives every label a new size and so a new picture; until
//! each is drawn the label keeps the halo it had, as the zoom's transform showed it.
//! A label that has no such picture (it is new in the view) gets a small one, the
//! text drawn at a quarter of its size ([`Scope::preview`]: a sixteenth of the
//! pixels and a fraction of the work, so a frame draws them for a whole view), which
//! stands in the same way: the halo is there from the label's first frame, soft,
//! and sharp a few frames later. This is for shadows with a SPREAD, which the
//! renderer has no shadow of its own for (it would paint the text without its
//! halo); a blurred shadow without one is the renderer's own until its picture is
//! drawn, as before.

use std::{
    cell::{Cell, RefCell},
    collections::{hash_map::DefaultHasher, HashMap},
    hash::{Hash, Hasher},
    sync::Arc,
    time::{Duration, Instant},
};

use azul_core::{
    geom::{LogicalPosition, LogicalRect},
    resources::{ImageRef, RawImage, RawImageData, RawImageFormat, RendererResources},
    ui_solver::GlyphInstance,
};
use azul_css::props::basic::{pixel::DEFAULT_FONT_SIZE, ColorU, FontRef};

use super::raster::{
    blurred_glyphs, shadow_blur_radius, shadow_spread_px, text_run_clip, BufferedRun, PixelBox,
};
use crate::{
    glyph_cache::GlyphCache,
    image_memo::ImageMemo,
    solver3::display_list::DisplayListItem,
    text3::cache::{FontHash, FontManager},
};

/// The halos kept hold at most this many bytes of pixels (RGBA: four per pixel) ...
///
/// A picture is the ink of its glyphs grown by the shadow, cut to what it paints
/// ([`cropped`]): a street name of a printed map (2.3 mm type, a 0.35 mm outline) at
/// 6 px per mm on a 2x display is about 184 x 38 px = 28 KB (65 KB as the whole box
/// its glyphs can reach, 223 x 73 px), a house number 5 KB. A window of 1,600 such
/// labels holds 45 MB, and a zoom that settles holds the old pictures (they stand in)
/// and the new ones for some frames. 96 MB is 1.2 % of an 8 GB machine; the renderer
/// holds the pictures a frame draws a second time, as textures.
const MAX_HALO_BYTES: usize = 96 * 1024 * 1024;
/// ... in at most this many pictures (each is an image of the renderer's).
const MAX_HALOS: usize = 16 * 1024;
/// The most pixels one halo may cover; a larger shadow (a page of shadowed
/// text) is left to the renderer's own blur.
const MAX_HALO_PIXELS: u64 = 1 << 21;

/// The CPU renderer keeps at most this many halos ([`CpuHalo`]) ...
const MAX_CPU_HALOS: usize = 4096;
/// ... holding at most this many bytes of coverage (one byte a pixel).
const MAX_CPU_HALO_BYTES: usize = 32 * 1024 * 1024;
/// The most pixels one kept CPU halo may cover. A larger shadow (a page of shadowed text) is
/// drawn as before, over the part that lands, so a repaint of a strip of it stays the strip's cost.
pub(super) const MAX_CPU_HALO_PIXELS: u64 = 1 << 17;

/// A text shadow's halo as the CPU renderer keeps it: the coverage of the glyphs grown by the
/// spread and blurred, in white, for the pixel box its glyphs reach. A label that moves by whole
/// pixels (a pan, a zoom's composite) draws the same halo, so it is tinted with the shadow's colour
/// and composited, not drawn again.
#[derive(Debug)]
pub(super) struct CpuHalo {
    /// The picture's size in device px.
    pub(super) width: u32,
    pub(super) height: u32,
    /// The picture's top-left corner from the run's anchor (its first glyph's pen, floored to a
    /// device pixel), in device px.
    pub(super) offset: (i32, i32),
    /// The coverage, one byte a pixel, row by row.
    pub(super) coverage: Vec<u8>,
}

thread_local! {
    /// The CPU renderer's halos by what their pixels depend on (`raster::kept_shadow_key`). `None`
    /// remembers a shadow that cannot be kept (too large), so it is not measured again.
    static CPU_HALOS: RefCell<ImageMemo<u64, Option<Arc<CpuHalo>>>> =
        RefCell::new(ImageMemo::new(MAX_CPU_HALOS, MAX_CPU_HALO_BYTES));
    /// Tests: keep no halo, so a shadow is drawn the way it was before halos were kept.
    #[cfg(test)]
    static CPU_HALOS_OFF: Cell<bool> = const { Cell::new(false) };
}

/// The CPU renderer's kept halos while the bands of a repaint run (`bands`): the memo of the
/// thread that paints the frame, moved behind a lock that its band threads share. A band thread
/// starts with no thread-locals of its own, and one that drew every halo of its band again
/// would lose what keeping them gains.
pub(super) type SharedCpuHalos = Arc<std::sync::Mutex<ImageMemo<u64, Option<Arc<CpuHalo>>>>>;

thread_local! {
    /// Set on a thread while it paints a band: the frame's shared halos ([`SharedCpuHalos`]),
    /// read and kept instead of this thread's own [`CPU_HALOS`].
    static FRAME_CPU_HALOS: RefCell<Option<SharedCpuHalos>> = const { RefCell::new(None) };
}

/// This thread's kept halos, moved out to be shared with the band threads of the frame it
/// paints; [`restore_cpu_halos`] moves them back.
pub(super) fn share_cpu_halos() -> SharedCpuHalos {
    let memo = CPU_HALOS.with(|halos| {
        core::mem::replace(
            &mut *halos.borrow_mut(),
            ImageMemo::new(MAX_CPU_HALOS, MAX_CPU_HALO_BYTES),
        )
    });
    Arc::new(std::sync::Mutex::new(memo))
}

/// Paint on this thread with the frame's shared halos (`Some`), or with its own again (`None`).
/// A band thread sets `None` before it ends, so that the frame's thread is the only holder
/// when it restores them.
pub(super) fn adopt_cpu_halos(shared: Option<SharedCpuHalos>) {
    FRAME_CPU_HALOS.with(|frame| *frame.borrow_mut() = shared);
}

/// Move the halos shared for a frame's bands back into this thread's own memo, with what the
/// bands kept. Left as they are (the thread then keeps an empty memo and draws halos again) if
/// a band thread still holds the store.
pub(super) fn restore_cpu_halos(shared: SharedCpuHalos) {
    adopt_cpu_halos(None);
    if let Ok(lock) = Arc::try_unwrap(shared) {
        let memo = lock.into_inner().unwrap_or_else(std::sync::PoisonError::into_inner);
        CPU_HALOS.with(|halos| *halos.borrow_mut() = memo);
    }
}

/// The frame's shared halos, while this thread paints a band of it.
fn frame_cpu_halos() -> Option<SharedCpuHalos> {
    FRAME_CPU_HALOS.with(|frame| frame.borrow().clone())
}

/// The halo kept under `key`: `Some(None)` for a shadow that is not kept, `None` for one not seen.
pub(super) fn kept_cpu_halo(key: u64) -> Option<Option<Arc<CpuHalo>>> {
    if let Some(shared) = frame_cpu_halos() {
        return shared.lock().ok().and_then(|mut halos| halos.get(&key));
    }
    CPU_HALOS.with(|halos| halos.borrow_mut().get(&key))
}

/// Keep `halo` under `key`.
pub(super) fn keep_cpu_halo(key: u64, halo: Option<Arc<CpuHalo>>) {
    let bytes = halo.as_ref().map_or(0, |h| h.coverage.len());
    if let Some(shared) = frame_cpu_halos() {
        if let Ok(mut halos) = shared.lock() {
            halos.insert(key, halo, bytes);
        }
        return;
    }
    CPU_HALOS.with(|halos| halos.borrow_mut().insert(key, halo, bytes));
}

/// Whether this thread keeps the CPU renderer's halos (always, outside tests).
pub(super) fn cpu_halos_are_kept() -> bool {
    #[cfg(test)]
    {
        !CPU_HALOS_OFF.with(Cell::get)
    }
    #[cfg(not(test))]
    {
        true
    }
}

/// Tests: keep (or not) the CPU renderer's halos on this thread.
#[cfg(test)]
pub(super) fn keep_cpu_halos(on: bool) {
    CPU_HALOS_OFF.with(|c| c.set(!on));
}

thread_local! {
    /// How many halo pictures this thread has DRAWN (not found kept): what a
    /// test reads to say "this frame drew no halo". Per thread because the
    /// halos are kept per thread ([`HALOS`]) and the tests run in parallel.
    static DRAWN: Cell<usize> = const { Cell::new(0) };
}

/// How many halos [`text_halos`] has drawn on this thread.
#[must_use]
pub fn halos_drawn() -> usize {
    DRAWN.with(Cell::get)
}

thread_local! {
    /// The budget of the frame being prepared on this thread
    /// ([`within_halo_budget`]): how long it may draw, how long it has drawn,
    /// how many halos it has drawn, how many it had to leave.
    static BUDGET: Cell<Option<Budget>> = const { Cell::new(None) };
}

#[derive(Copy, Clone)]
struct Budget {
    /// Halos drawn however long they take: a frame always makes progress.
    free: usize,
    /// How long the frame may spend drawing halos after those.
    time: Duration,
    /// How long it has spent drawing them.
    spent: Duration,
    drawn: usize,
    deferred: usize,
    /// The small stand-in pictures it has drawn ([`Scope::preview`]), and how
    /// long they took.
    previews: usize,
    previews_spent: Duration,
}

/// Small stand-in pictures a frame draws however long they take ...
const PREVIEWS_ANYWAY: usize = 64;
/// ... and how long it goes on drawing them after those. One is a sixteenth of
/// its halo's pixels with a quarter of the spread: some fifty in the time of one
/// halo.
const PREVIEW_TIME: Duration = Duration::from_millis(6);

/// Runs `f` - a scan of display lists for text halos ([`text_halos`] with fonts)
/// - with a BUDGET: the first `free` halos it has to draw are drawn, the rest
/// only while the halos drawn so far took less than `time`, and the others are
/// left for a later frame (nothing is kept for them: they are found missing,
/// and the shadow they stand for is painted with a stand-in or by the renderer's
/// own blur, until a later scan draws the picture). Returns what `f` returned and
/// how many halos it left.
///
/// The time is the time spent DRAWING, not the time since the scan began: a scan
/// of ten thousand labels that are all kept takes its time too, and a budget
/// counted from its start left such a frame the `free` halos only.
///
/// A halo is a CPU raster of the label's glyphs, a disk dilation and a blur: 25 new
/// labels of a map that panned were 5.0 s on the UI thread in one frame (1.2 s
/// for 80 in another, 9.0 s over a session), and nothing in the frame could
/// run before they were done. A frame that finds more halos than it may draw
/// draws the rest as the map settles.
///
/// A call is one SCAN of the kept halos ([`HaloCache::begin_scan`]): the pictures
/// it uses are not dropped before the next call, whatever else it draws.
pub fn within_halo_budget<R>(free: usize, time: Duration, f: impl FnOnce() -> R) -> (R, usize) {
    HALOS.with(|halos| halos.borrow_mut().begin_scan());
    let outer = BUDGET.with(|b| {
        b.replace(Some(Budget {
            free,
            time,
            spent: Duration::ZERO,
            drawn: 0,
            deferred: 0,
            previews: 0,
            previews_spent: Duration::ZERO,
        }))
    });
    let result = f();
    let budget = BUDGET.with(|b| b.replace(outer));
    let left = budget.map_or(0, |b| b.deferred);
    // The halos' glyphs: a scan that drew none, after scans that drew, is where
    // a view's halos are done - the glyph cache gives up the generation it only
    // keeps to hand glyphs back (`GlyphCache::gc`; it was never called here, and
    // the cache held both generations for the life of the thread, audit A9 #30).
    let drew = budget.is_some_and(|b| b.drawn + b.previews > 0);
    if drew {
        GLYPHS_USED.with(|used| used.set(true));
    } else if left == 0 && GLYPHS_USED.with(|used| used.replace(false)) {
        GLYPHS.with(|glyph_cache| glyph_cache.borrow_mut().gc());
    }
    log_scan();
    (result, left)
}

/// Whether the budget of this thread lets one more halo be drawn.
fn budget_left() -> bool {
    BUDGET.with(|cell| {
        cell.get()
            .is_none_or(|budget| budget.drawn < budget.free || budget.spent < budget.time)
    })
}

/// Counts a halo the budget of this thread let be drawn (`drawn`) or not.
fn count_in_budget(drawn: bool) {
    BUDGET.with(|cell| {
        if let Some(mut budget) = cell.get() {
            if drawn {
                budget.drawn += 1;
            } else {
                budget.deferred += 1;
            }
            cell.set(Some(budget));
        }
    });
}

/// Counts `time` as spent drawing halos by the budget of this thread.
fn spend(time: Duration) {
    BUDGET.with(|cell| {
        if let Some(mut budget) = cell.get() {
            budget.spent += time;
            cell.set(Some(budget));
        }
    });
}

/// Whether a halo whose box holds `box_bytes` is drawn now: the frame's budget
/// lets it, and there is room to keep it ([`HaloCache::make_room`]). Counted
/// either way: a halo the budget leaves is `pending` (a later frame draws it, and
/// is owed), one there is no room for is `unkept` (no frame is owed: the next
/// one would find the same).
fn admit(box_bytes: usize) -> bool {
    if !budget_left() {
        count_in_budget(false);
        HALOS.with(|halos| halos.borrow_mut().stats.pending += 1);
        return false;
    }
    let room = HALOS.with(|halos| {
        let mut halos = halos.borrow_mut();
        let room = halos.make_room(box_bytes);
        if !room {
            halos.stats.unkept += 1;
        }
        room
    });
    if !room {
        return false;
    }
    count_in_budget(true);
    DRAWN.with(|n| n.set(n.get() + 1));
    true
}

/// Whether a small stand-in picture whose box holds `box_bytes` is drawn now:
/// the frame has drawn fewer than [`PREVIEWS_ANYWAY`] or spent less than
/// [`PREVIEW_TIME`] on them, and there is room to keep it.
fn admit_preview(box_bytes: usize) -> bool {
    let within = BUDGET.with(|cell| {
        cell.get().is_none_or(|budget| {
            budget.previews < PREVIEWS_ANYWAY || budget.previews_spent < PREVIEW_TIME
        })
    });
    if !within || !HALOS.with(|halos| halos.borrow_mut().make_room(box_bytes)) {
        return false;
    }
    BUDGET.with(|cell| {
        if let Some(mut budget) = cell.get() {
            budget.previews += 1;
            cell.set(Some(budget));
        }
    });
    // A translation made before it does not paint it: the lists go again.
    DRAWN.with(|n| n.set(n.get() + 1));
    true
}

/// Counts `time` as spent drawing small stand-in pictures by this thread's budget.
fn spend_on_previews(time: Duration) {
    BUDGET.with(|cell| {
        if let Some(mut budget) = cell.get() {
            budget.previews_spent += time;
            cell.set(Some(budget));
        }
    });
}

/// What the scans of this thread did with the text halos: the numbers of the last
/// scan ([`within_halo_budget`], one per frame that registers its images), the
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

/// What the scans of this thread did with the text halos ([`TextHaloStats`]).
#[must_use]
pub fn text_halo_stats() -> TextHaloStats {
    HALOS.with(|halos| {
        let halos = halos.borrow();
        let mut stats = halos.stats;
        stats.kept = halos.entries.len() as u64;
        stats.kept_bytes = halos.bytes as u64;
        stats
    })
}

/// `AZ_HALO_STATS=1`: one line on stderr for every scan that drew, dropped, left or
/// stood in for a halo - what a run on a real window shows without a recorder.
fn log_scan() {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if !*ON.get_or_init(|| std::env::var("AZ_HALO_STATS").is_ok_and(|v| !v.is_empty() && v != "0")) {
        return;
    }
    let s = text_halo_stats();
    if s.created + s.evicted + s.pending + s.unkept + s.stand_ins + s.previews == 0 {
        return;
    }
    #[allow(clippy::cast_precision_loss)] // bytes as megabytes, for a log line
    let mb = |bytes: u64| bytes as f64 / (1024.0 * 1024.0);
    eprintln!(
        "[azul halos] scan {}: created {} evicted {} pending {} unkept {} stand-ins {} \
         previews {} | the frame uses {} pictures ({:.1} MB as boxes) | kept {} ({:.1} MB)",
        s.scans,
        s.created,
        s.evicted,
        s.pending,
        s.unkept,
        s.stand_ins,
        s.previews,
        s.frame_halos,
        mb(s.frame_box_bytes),
        s.kept,
        mb(s.kept_bytes),
    );
}

/// A text shadow drawn as a picture, where its `PushTextShadow` stands.
#[derive(Debug, Clone, PartialEq)]
pub struct TextHalo {
    /// The index of the shadow's `PushTextShadow` in the display list.
    pub push: usize,
    /// The halo in premultiplied white; tinted with `color` it is the shadow.
    pub image: ImageRef,
    /// The shadow's colour.
    pub color: ColorU,
    /// The top-left corner of where it is painted, in device px of the window
    /// (before any scrolling), the shadow's offset included.
    pub origin: (f32, f32),
    /// The size of where it is painted, in device px: the picture's own
    /// ([`Self::width`], [`Self::height`]: one texel per pixel), or for a
    /// stand-in the size its own picture will have, about.
    pub size: (f32, f32),
    /// The picture's width in device px.
    pub width: u32,
    /// Its height in device px.
    pub height: u32,
    /// Whether the picture is another halo's standing in: the same text in the
    /// same fonts drawn at another size (an earlier zoom's, or the small one
    /// drawn for the purpose), stretched over where this one's own picture will
    /// lie once a frame has drawn it.
    pub stand_in: bool,
}

/// The text shadows of `items` that are drawn as pictures at `dpi`: blurred
/// or spread shadows over nothing but text (decorations or anything else under a
/// shadow leave it to the renderer's own). With `fonts` a halo that is not
/// kept yet is drawn and kept; without, only kept ones are found. The GPU
/// backend draws them while it collects a frame's images, before it builds
/// the frame's display list, so the second call finds every one it can use.
///
/// A SPREAD shadow whose own picture is not drawn yet (the frame's budget left it,
/// or there is no room to keep it) is painted with a stand-in
/// ([`TextHalo::stand_in`]): a kept picture of the same text at another size, or
/// a small one drawn now (with `fonts`). It is not in the result when there is
/// neither, and a shadow without a spread never is before its own picture.
#[must_use]
pub fn text_halos(
    items: &[DisplayListItem],
    dpi: f32,
    fonts: Option<(&RendererResources, &FontManager<FontRef>)>,
) -> Vec<TextHalo> {
    // NaN too: no positive scale, no halo.
    if dpi.partial_cmp(&0.0) != Some(core::cmp::Ordering::Greater) {
        return Vec::new();
    }
    // The scan that draws counts what it finds; the one that only looks (the
    // translation of the same lists, right after) does not count it twice.
    let counted = fonts.is_some();
    let mut geometry = Vec::new();
    scopes(items)
        .into_iter()
        .filter_map(|scope| {
            let sought = scope.sought(dpi, &mut geometry);
            let own = HALOS.with(|halos| {
                let mut halos = halos.borrow_mut();
                let id = halos.find(sought.text, &geometry)?;
                halos.use_own(id, counted)
            });
            let own = match (own, fonts) {
                // Kept: its picture, or the note that it cannot be one.
                (Some(kept), _) => kept,
                (None, Some((renderer_resources, font_manager))) => {
                    match scope.draw(&sought, dpi, renderer_resources, font_manager, admit, spend) {
                        Drawn::Picture { kept, box_bytes } => {
                            HALOS.with(|halos| {
                                halos.borrow_mut().insert(
                                    &sought,
                                    &geometry,
                                    Some(kept.clone()),
                                    box_bytes,
                                    false,
                                );
                            });
                            Some(kept)
                        }
                        Drawn::NotAPicture => {
                            // Remembered, so it is not tried on every frame.
                            HALOS.with(|halos| {
                                let mut halos = halos.borrow_mut();
                                if halos.make_room(0) {
                                    halos.insert(&sought, &geometry, None, 0, false);
                                }
                            });
                            None
                        }
                        // Over the frame's budget, or no room: not drawn, not
                        // kept - found missing, and drawn by a later scan.
                        Drawn::Left => None,
                    }
                }
                (None, None) => None,
            };
            let (kept, stand_in) = match own {
                Some(kept) => (kept, false),
                // Without a spread the renderer's own shadow is the halo until
                // the picture is drawn.
                None if scope.spread <= 0.0 => return None,
                None => {
                    // Stretched more than eight times either way a picture is
                    // a smear: such a halo waits for its own.
                    let fits = |kept: &Kept| {
                        let k = sought.scale / kept.scale;
                        kept.scale > 0.0 && (0.125..=8.0).contains(&k)
                    };
                    let stood = HALOS
                        .with(|halos| halos.borrow_mut().stand_in(sought.shape, counted, fits));
                    let kept = match (stood, fonts) {
                        (Some(kept), _) => kept,
                        (None, Some((renderer_resources, font_manager))) => {
                            let small =
                                scope.preview(&sought, dpi, renderer_resources, font_manager)?;
                            HALOS.with(|halos| halos.borrow_mut().stats.stand_ins += 1);
                            small
                        }
                        (None, None) => return None,
                    };
                    (kept, true)
                }
            };
            let (origin, size) =
                placed(&kept, sought.anchor, stand_in.then(|| sought.scale / kept.scale));
            Some(TextHalo {
                push: scope.push,
                image: kept.image,
                color: scope.color,
                origin: (origin.0 + scope.offset.0 * dpi, origin.1 + scope.offset.1 * dpi),
                size,
                width: kept.width,
                height: kept.height,
                stand_in,
            })
        })
        .collect()
}

/// Where a kept picture is painted for a shadow whose first glyph's pen is on
/// `anchor`: its top-left corner and its size, in device px. `stretch`: the
/// picture stands in for the same text at `stretch` times the size it was drawn
/// at - laid out again at another size, a text keeps its first glyph's pen as
/// the point everything is measured from, so the picture is scaled about it.
#[allow(clippy::cast_precision_loss)] // device px
fn placed(kept: &Kept, anchor: (i32, i32), stretch: Option<f32>) -> ((f32, f32), (f32, f32)) {
    match stretch {
        Some(k) => (
            (
                anchor.0 as f32 + kept.offset.0 as f32 * k,
                anchor.1 as f32 + kept.offset.1 as f32 * k,
            ),
            (kept.width as f32 * k, kept.height as f32 * k),
        ),
        None => (
            ((anchor.0 + kept.offset.0) as f32, (anchor.1 + kept.offset.1) as f32),
            (kept.width as f32, kept.height as f32),
        ),
    }
}

/// A halo as it is kept: the picture and where it lies from its scope's
/// anchor, in device px.
#[derive(Debug, Clone)]
struct Kept {
    /// The halo in premultiplied white.
    image: ImageRef,
    /// The picture's top-left corner from the scope's anchor.
    offset: (i32, i32),
    /// The picture's size in device px.
    width: u32,
    height: u32,
    /// The size it was drawn at: device px per em of its first run. What a
    /// picture that stands in for the same text at another size is stretched by.
    scale: f32,
}

/// What [`Scope::draw`] made of a shadow that had no picture.
enum Drawn {
    /// Its picture, and the bytes of the whole box its glyphs can reach.
    Picture { kept: Kept, box_bytes: usize },
    /// It cannot be a picture: nothing of it paints, or it is too large.
    NotAPicture,
    /// Not drawn now ([`admit`]).
    Left,
}

/// One kept halo.
#[derive(Debug)]
struct Entry {
    /// The picture; `None` remembers a shadow that cannot be one (too large, or
    /// nothing of it paints), so it is not tried every frame.
    kept: Option<Kept>,
    /// The bytes of the picture's pixels.
    bytes: usize,
    /// The bytes of the whole box its glyphs can reach.
    box_bytes: usize,
    /// What it is the halo of: [`Sought::text`], the geometry found with it, and
    /// [`Sought::shape`].
    text: u64,
    geometry: Vec<i32>,
    shape: u64,
    /// The scan that last used it as a shadow's own picture: that scan does not
    /// drop it.
    used_in: u64,
    /// The cache's clock when it was last used at all, as a stand-in too: the
    /// entry used longest ago is dropped first.
    last_used: u64,
}

/// Whether two geometries ([`Scope::sought`]) are the same text laid out at two
/// places: every value equal to within a step. A glyph's place from the first
/// glyph's pen is the difference of two window positions, and that difference
/// depends on where the text stands by the rounding of the two (an advance of
/// 4.6953125 px is 300.5 steps from a pen at 20.0 and 300.49 steps from one at
/// 2047.9): hashed, one label in twenty-five lost its halo at every pan that laid
/// the labels out again.
fn close(a: &[i32], b: &[i32]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(x, y)| (i64::from(*x) - i64::from(*y)).abs() <= 1)
}

/// The halos kept on this thread.
///
/// A halo is found by what its pixels depend on: the text (`by_text`: fonts,
/// sizes, glyphs, blur, spread and scale, exactly) and the geometry (clips and
/// glyph positions, to within a step: [`close`]). Bounded by entries and by
/// bytes, with one rule above both: a SCAN - one frame's look at its display
/// lists, [`within_halo_budget`] - never drops an entry it has used as a
/// shadow's own picture. So what a frame shows is kept for as long as frames
/// show it, and the cache thrashes at no size: when the pictures of the scan
/// itself fill it, a new one is not drawn ([`Self::make_room`] says no) and its
/// shadow keeps what it had, for this frame and the next.
#[derive(Debug)]
struct HaloCache {
    entries: HashMap<u64, Entry>,
    /// The ids of the entries of each text.
    by_text: HashMap<u64, Vec<u64>>,
    /// The newest picture of each shape ([`Sought::shape`]): what stands in for
    /// a halo of that shape that has no picture of its own yet.
    by_shape: HashMap<u64, u64>,
    next_id: u64,
    /// The sum of every entry's `bytes`.
    bytes: usize,
    /// Ticks once per use; orders the entries by their last use.
    clock: u64,
    /// The scan under way (or the last one).
    scan: u64,
    /// The scan in which [`Self::make_room`] last found nothing to drop: until
    /// the next scan nothing can be (entries are only added and used in a scan),
    /// and ten thousand halos that do not fit do not each look through the cache.
    nothing_to_drop_in: Option<u64>,
    /// The entries the scan under way may drop, the one used longest ago LAST
    /// (it is popped first), as they were at `victims_at` on the clock: sorted
    /// once when room is first needed, not searched for each entry dropped (a
    /// zoom that settles in a full cache drops as many pictures as it draws:
    /// a search of every entry for each was the entries squared, audit A9 H14).
    victims: Vec<u64>,
    victims_at: u64,
    /// How often that list was made (a test counts it).
    victim_lists: u64,
    max_entries: usize,
    max_bytes: usize,
    stats: TextHaloStats,
}

impl HaloCache {
    fn new(max_entries: usize, max_bytes: usize) -> Self {
        Self {
            entries: HashMap::new(),
            by_text: HashMap::new(),
            by_shape: HashMap::new(),
            next_id: 0,
            bytes: 0,
            clock: 0,
            // From 1: an entry that no scan has used as a shadow's own picture
            // says 0.
            scan: 1,
            nothing_to_drop_in: None,
            victims: Vec::new(),
            victims_at: 0,
            victim_lists: 0,
            max_entries,
            max_bytes,
            stats: TextHaloStats::default(),
        }
    }

    /// A new scan begins: what the last one used may be dropped again, and the
    /// numbers of a scan start at zero.
    fn begin_scan(&mut self) {
        self.scan += 1;
        self.victims.clear();
        self.stats = TextHaloStats {
            scans: self.stats.scans + 1,
            created_total: self.stats.created_total,
            evicted_total: self.stats.evicted_total,
            ..TextHaloStats::default()
        };
    }

    /// The entry of `text` with this `geometry`, to within a step.
    fn find(&self, text: u64, geometry: &[i32]) -> Option<u64> {
        self.by_text.get(&text)?.iter().copied().find(|id| {
            self.entries
                .get(id)
                .is_some_and(|entry| close(&entry.geometry, geometry))
        })
    }

    /// Entry `id` used as a shadow's own picture: what it keeps (`None`: the
    /// shadow cannot be a picture), not dropped by this scan from now on.
    /// `counted`: its first use in a scan counts in the scan's numbers.
    fn use_own(&mut self, id: u64, counted: bool) -> Option<Option<Kept>> {
        self.clock += 1;
        let entry = self.entries.get_mut(&id)?;
        entry.last_used = self.clock;
        if entry.used_in != self.scan {
            entry.used_in = self.scan;
            if counted {
                self.stats.frame_halos += 1;
                self.stats.frame_box_bytes += entry.box_bytes as u64;
            }
        }
        Some(entry.kept.clone())
    }

    /// The picture that stands in for a halo of `shape` without one of its own:
    /// the newest picture of that shape, if it `fits`. A use, but not one that keeps the entry
    /// from being dropped: a stand-in that held its place would leave no room
    /// for the pictures that replace it.
    fn stand_in(
        &mut self,
        shape: u64,
        counted: bool,
        fits: impl FnOnce(&Kept) -> bool,
    ) -> Option<Kept> {
        let id = *self.by_shape.get(&shape)?;
        let entry = self.entries.get_mut(&id)?;
        let kept = entry.kept.as_ref().filter(|kept| fits(*kept))?.clone();
        self.clock += 1;
        entry.last_used = self.clock;
        if counted {
            self.stats.stand_ins += 1;
        }
        Some(kept)
    }

    /// Makes room for one more entry of `bytes`: drops entries this scan has not
    /// used as a shadow's own picture, the one used longest ago first, until both
    /// bounds hold. `false` when they cannot: the scan's own pictures fill the
    /// cache (or `bytes` alone is more than it holds); what was dropped on the
    /// way was of earlier frames.
    fn make_room(&mut self, bytes: usize) -> bool {
        if bytes > self.max_bytes {
            return false;
        }
        while self.entries.len() >= self.max_entries || self.bytes + bytes > self.max_bytes {
            let Some(oldest) = self.next_victim() else {
                return false;
            };
            self.remove(oldest);
            self.stats.evicted += 1;
            self.stats.evicted_total += 1;
        }
        true
    }

    /// The entry to drop next: of those this scan has not used as a shadow's own
    /// picture, the one used longest ago. `None` when there is none.
    fn next_victim(&mut self) -> Option<u64> {
        let scan = self.scan;
        if self.nothing_to_drop_in == Some(scan) {
            return None;
        }
        loop {
            while let Some(id) = self.victims.pop() {
                // Still kept, not used as an own picture since the list was made,
                // and not used at all since (a stand-in that this frame paints
                // goes after the pictures nothing uses: it is in the next list).
                let droppable = self
                    .entries
                    .get(&id)
                    .is_some_and(|entry| entry.used_in != scan && entry.last_used <= self.victims_at);
                if droppable {
                    return Some(id);
                }
            }
            let mut victims: Vec<(u64, u64)> = self
                .entries
                .iter()
                .filter(|(_, entry)| entry.used_in != scan)
                .map(|(id, entry)| (entry.last_used, *id))
                .collect();
            if victims.is_empty() {
                self.nothing_to_drop_in = Some(scan);
                return None;
            }
            // The newest first: the one used longest ago is popped first.
            victims.sort_unstable_by(|a, b| b.0.cmp(&a.0));
            self.victims = victims.into_iter().map(|(_, id)| id).collect();
            self.victims_at = self.clock;
            self.victim_lists += 1;
        }
    }

    /// The small picture kept for `text` and `geometry` (a [`Scope::preview`]
    /// drawn before), made the stand-in of `shape` again: `Some(None)` when what
    /// is kept is the note that there is no such picture, `None` when nothing is.
    fn kept_preview(&mut self, text: u64, geometry: &[i32], shape: u64) -> Option<Option<Kept>> {
        let id = self.find(text, geometry)?;
        self.clock += 1;
        let entry = self.entries.get_mut(&id)?;
        entry.last_used = self.clock;
        if entry.kept.is_some() {
            self.by_shape.insert(shape, id);
        }
        Some(entry.kept.clone())
    }

    /// Keeps `kept` for what `sought` and `geometry` name. As a shadow's own
    /// picture it is used by this scan; as a `preview` it is a stand-in, which
    /// no scan holds on to. The caller made room for it ([`Self::make_room`],
    /// for at least its bytes).
    fn insert(
        &mut self,
        sought: &Sought,
        geometry: &[i32],
        kept: Option<Kept>,
        box_bytes: usize,
        preview: bool,
    ) {
        let bytes = kept
            .as_ref()
            .map_or(0, |kept| kept.width as usize * kept.height as usize * 4);
        let id = self.next_id;
        self.next_id += 1;
        self.clock += 1;
        if kept.is_some() {
            self.by_shape.insert(sought.shape, id);
            if preview {
                self.stats.previews += 1;
            } else {
                self.stats.created += 1;
                self.stats.created_total += 1;
                self.stats.frame_halos += 1;
                self.stats.frame_box_bytes += box_bytes as u64;
            }
        }
        self.by_text.entry(sought.text).or_default().push(id);
        self.bytes += bytes;
        self.entries.insert(
            id,
            Entry {
                kept,
                bytes,
                box_bytes,
                text: sought.text,
                geometry: geometry.to_vec(),
                shape: sought.shape,
                // No scan is numbered 0 ([`Self::new`]).
                used_in: if preview { 0 } else { self.scan },
                last_used: self.clock,
            },
        );
    }

    /// Drops entry `id`.
    fn remove(&mut self, id: u64) {
        let Some(entry) = self.entries.remove(&id) else {
            return;
        };
        self.bytes -= entry.bytes;
        let emptied = self.by_text.get_mut(&entry.text).is_some_and(|ids| {
            ids.retain(|other| *other != id);
            ids.is_empty()
        });
        if emptied {
            self.by_text.remove(&entry.text);
        }
        if self.by_shape.get(&entry.shape) == Some(&id) {
            self.by_shape.remove(&entry.shape);
        }
    }
}

thread_local! {
    /// The halos drawn so far.
    static HALOS: RefCell<HaloCache> = RefCell::new(HaloCache::new(MAX_HALOS, MAX_HALO_BYTES));
    /// The glyph rasteriser's cache for the halos' glyphs.
    static GLYPHS: RefCell<GlyphCache> = RefCell::new(GlyphCache::new());
    /// Whether halos were drawn since [`GLYPHS`] last gave up its older
    /// generation ([`within_halo_budget`]).
    static GLYPHS_USED: Cell<bool> = const { Cell::new(false) };
}

/// One glyph run under a text shadow.
struct Run<'a> {
    /// Its glyphs, at their pen positions in CSS px of the window.
    glyphs: &'a [GlyphInstance],
    /// The font they are drawn in, and its size.
    font_hash: FontHash,
    font_size_px: f32,
    /// The run's clip rect, CSS px of the window.
    clip_rect: LogicalRect,
    /// Where its glyphs can paint: their ink, padded, inside the clip rect.
    ink: LogicalRect,
}

/// A blurred text shadow over nothing but text.
struct Scope<'a> {
    /// The index of its `PushTextShadow`.
    push: usize,
    /// The shadow's colour; the halo is drawn without it.
    color: ColorU,
    /// The shadow's offset, CSS px.
    offset: (f32, f32),
    /// Its blur radius, CSS px.
    blur: f32,
    /// Its spread, CSS px: how far the glyphs grow before the blur (a solid
    /// halo when there is no blur). `WebRender`'s own shadow has none, so a
    /// spread shadow is always a picture.
    spread: f32,
    /// The text under the shadow, in list order; never empty.
    runs: Vec<Run<'a>>,
}

/// A shadow length in CSS px, resolved as the CPU renderer resolves it.
fn shadow_px(value: &azul_css::props::basic::pixel::PixelValue) -> f32 {
    value.to_pixels_internal(0.0, DEFAULT_FONT_SIZE, DEFAULT_FONT_SIZE)
}

/// The shadows of `items` a halo can stand for: blurred or spread, over text
/// alone. A shadow with neither is not one: `WebRender` draws it as a plain
/// copy of the text, without a surface.
fn scopes(items: &[DisplayListItem]) -> Vec<Scope<'_>> {
    let mut out = Vec::new();
    for (push, item) in items.iter().enumerate() {
        let DisplayListItem::PushTextShadow { shadow } = item else {
            continue;
        };
        let blur = shadow_px(&shadow.blur_radius.inner).max(0.0);
        let spread = shadow_px(&shadow.spread_radius.inner).max(0.0);
        if (blur <= 0.0 && spread <= 0.0) || shadow.color.a == 0 {
            continue;
        }
        let mut runs = Vec::new();
        let mut closed = false;
        for inner in &items[push + 1..] {
            match inner {
                DisplayListItem::Text {
                    glyphs,
                    font_hash,
                    font_size_px,
                    clip_rect,
                    ..
                } => {
                    if let (false, Some(ink)) = (glyphs.is_empty(), inner.visual_bounds()) {
                        runs.push(Run {
                            glyphs,
                            font_hash: *font_hash,
                            font_size_px: *font_size_px,
                            clip_rect: *clip_rect.inner(),
                            ink,
                        });
                    }
                }
                // They paint nothing on either backend: the text's layout
                // (kept for PDF and accessibility) and its hit area.
                DisplayListItem::TextLayout { .. } | DisplayListItem::HitTestArea { .. } => {}
                DisplayListItem::PopTextShadow => {
                    closed = true;
                    break;
                }
                _ => break,
            }
        }
        if closed && !runs.is_empty() {
            out.push(Scope {
                push,
                color: shadow.color,
                offset: (
                    shadow_px(&shadow.offset_x.inner),
                    shadow_px(&shadow.offset_y.inner),
                ),
                blur,
                spread,
                runs,
            });
        }
    }
    out
}

/// What a scope's halo is found by ([`Scope::sought`]), besides its geometry.
#[derive(Debug, Clone, Copy)]
struct Sought {
    /// What its pixels depend on that is counted exactly: the blur, the spread,
    /// the scale, and each run's font, size and glyphs.
    text: u64,
    /// The fonts and glyphs alone, with the blur and the spread as parts of the
    /// first run's size: the same for the text laid out at another size, which
    /// is what a picture can stand in for ([`HaloCache::stand_in`]).
    shape: u64,
    /// The first glyph's pen on the nearest device pixel: what its picture is
    /// drawn from.
    anchor: (i32, i32),
    /// Device px per em of its first run.
    scale: f32,
}

impl Scope<'_> {
    /// The first glyph's pen position: what everything is measured from.
    fn first(&self) -> LogicalPosition {
        self.runs[0].glyphs[0].point
    }

    /// What the halo is found by ([`Sought`]), and into `geometry` (cleared
    /// first) the runs' clips and glyph positions from the first glyph's pen in
    /// 1/64 px - text that moves keeps them, to within a step ([`close`]).
    #[allow(clippy::cast_possible_truncation)] // device px and 1/64 px steps
    fn sought(&self, dpi: f32, geometry: &mut Vec<i32>) -> Sought {
        let first = self.first();
        let anchor = ((first.x * dpi).round() as i32, (first.y * dpi).round() as i32);
        let step = |v: f32| (v * 64.0).round() as i32;
        let mut text = DefaultHasher::new();
        let mut shape = DefaultHasher::new();
        self.blur.to_bits().hash(&mut text);
        self.spread.to_bits().hash(&mut text);
        dpi.to_bits().hash(&mut text);
        self.runs.len().hash(&mut text);
        self.runs.len().hash(&mut shape);
        geometry.clear();
        for run in &self.runs {
            // The whole face: one thickened for a bold it has no cut for draws
            // a wider halo than the face as it is.
            run.font_hash.hash(&mut text);
            run.font_hash.hash(&mut shape);
            run.font_size_px.to_bits().hash(&mut text);
            for rect in [run.clip_rect, run.ink] {
                geometry.push(step(rect.origin.x - first.x));
                geometry.push(step(rect.origin.y - first.y));
                geometry.push(step(rect.size.width));
                geometry.push(step(rect.size.height));
            }
            run.glyphs.len().hash(&mut text);
            run.glyphs.len().hash(&mut shape);
            for glyph in run.glyphs {
                glyph.index.hash(&mut text);
                glyph.index.hash(&mut shape);
                geometry.push(step(glyph.point.x - first.x));
                geometry.push(step(glyph.point.y - first.y));
            }
        }
        // A zoom scales the type and its shadow alike: as parts of the type's
        // size the shadow is the same before and after, to a thirty-second.
        let em = self.runs[0].font_size_px;
        let part = |v: f32| if em > 0.0 { (v / em * 32.0).round() as i64 } else { 0 };
        part(self.blur).hash(&mut shape);
        part(self.spread).hash(&mut shape);
        Sought {
            text: text.finish(),
            shape: shape.finish(),
            anchor,
            scale: em * dpi,
        }
    }

    /// A small picture of the halo to stand in for it until a frame draws the
    /// real one: the text at a quarter of its size (half, for small type; none for
    /// type under 10 px, where a quarter is a smudge), kept like any halo - it IS
    /// the halo of this text at that scale - and found as the stand-in of its
    /// shape. A sixteenth of the pixels, a quarter of the spread: the dilation,
    /// which is what a halo costs (ink pixels x the disk's taps), is some 1/250 of
    /// the real one's. Painted four times its size it is soft, and it is there in
    /// the frame the label first shows.
    fn preview(
        &self,
        sought: &Sought,
        dpi: f32,
        renderer_resources: &RendererResources,
        font_manager: &FontManager<FontRef>,
    ) -> Option<Kept> {
        let divisor = if sought.scale >= 20.0 {
            4.0
        } else if sought.scale >= 10.0 {
            2.0
        } else {
            return None;
        };
        let small_dpi = dpi / divisor;
        let mut geometry = Vec::new();
        let small = self.sought(small_dpi, &mut geometry);
        let kept = HALOS
            .with(|halos| halos.borrow_mut().kept_preview(small.text, &geometry, small.shape));
        if let Some(kept) = kept {
            return kept;
        }
        let drawn = self.draw(
            &small,
            small_dpi,
            renderer_resources,
            font_manager,
            admit_preview,
            spend_on_previews,
        );
        match drawn {
            Drawn::Picture { kept, box_bytes } => {
                HALOS.with(|halos| {
                    halos
                        .borrow_mut()
                        .insert(&small, &geometry, Some(kept.clone()), box_bytes, true);
                });
                Some(kept)
            }
            Drawn::NotAPicture => {
                HALOS.with(|halos| {
                    let mut halos = halos.borrow_mut();
                    if halos.make_room(0) {
                        halos.insert(&small, &geometry, None, 0, true);
                    }
                });
                None
            }
            Drawn::Left => None,
        }
    }

    /// The halo drawn with the first glyph on `sought.anchor`: the runs moved by
    /// less than a device pixel, so the picture fits the text wherever it
    /// stands. `admit` is asked, with the bytes of the box the glyphs can reach,
    /// once it is known that there is a picture to draw and before it is drawn;
    /// `spent` is told how long drawing it took.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_precision_loss)]
    fn draw(
        &self,
        sought: &Sought,
        dpi: f32,
        renderer_resources: &RendererResources,
        font_manager: &FontManager<FontRef>,
        admit: impl FnOnce(usize) -> bool,
        spent: impl FnOnce(Duration),
    ) -> Drawn {
        let anchor = sought.anchor;
        let first = self.first();
        let shift = (anchor.0 as f32 / dpi - first.x, anchor.1 as f32 / dpi - first.y);
        let moved = |r: LogicalRect| {
            LogicalRect::new(
                LogicalPosition::new(r.origin.x + shift.0, r.origin.y + shift.1),
                r.size,
            )
        };
        // Each run's clip rect, and where it paints: its ink inside that rect.
        let clips: Vec<_> = self
            .runs
            .iter()
            .map(|run| {
                let clip_rect = moved(run.clip_rect);
                let paints = text_run_clip(&moved(run.ink), None, dpi)
                    .and_then(|ink| text_run_clip(&clip_rect, Some(ink), dpi));
                (clip_rect, paints)
            })
            .collect();
        let mut reach: Option<(f32, f32, f32, f32)> = None;
        for (_, paints) in &clips {
            let Some(paints) = paints else {
                continue;
            };
            let (x0, y0, x1, y1) = (paints.x, paints.y, paints.x + paints.width, paints.y + paints.height);
            reach = Some(match reach {
                Some((a, b, c, d)) => (a.min(x0), b.min(y0), c.max(x1), d.max(y1)),
                None => (x0, y0, x1, y1),
            });
        }
        let Some((x0, y0, x1, y1)) = reach else {
            return Drawn::NotAPicture;
        };
        let radius = shadow_blur_radius(self.blur, dpi);
        let spread = shadow_spread_px(self.spread, dpi);
        let r = radius as i32 + spread.ceil() as i32;
        let buffer = PixelBox {
            x: x0.floor() as i32 - r,
            y: y0.floor() as i32 - r,
            width: (x1.ceil() as i32 - x0.floor() as i32 + 2 * r).max(0) as u32,
            height: (y1.ceil() as i32 - y0.floor() as i32 + 2 * r).max(0) as u32,
        };
        if buffer.width == 0
            || buffer.height == 0
            || u64::from(buffer.width) * u64::from(buffer.height) > MAX_HALO_PIXELS
        {
            return Drawn::NotAPicture;
        }
        let box_bytes = buffer.width as usize * buffer.height as usize * 4;
        if !admit(box_bytes) {
            return Drawn::Left;
        }
        let started = Instant::now();
        let glyphs: Vec<Vec<GlyphInstance>> = self
            .runs
            .iter()
            .map(|run| {
                run.glyphs
                    .iter()
                    .map(|g| {
                        let mut g = *g;
                        g.point.x += shift.0;
                        g.point.y += shift.1;
                        g
                    })
                    .collect()
            })
            .collect();
        let mut runs = Vec::with_capacity(self.runs.len());
        for ((run, glyphs), (clip_rect, paints)) in self.runs.iter().zip(&glyphs).zip(clips) {
            let Some(paints) = paints else {
                continue;
            };
            runs.push(BufferedRun {
                glyphs,
                font_hash: run.font_hash,
                font_size_px: run.font_size_px,
                clip_rect,
                clip: paints,
            });
        }
        let pixels = GLYPHS.with(|glyph_cache| {
            blurred_glyphs(
                &runs,
                ColorU::WHITE,
                buffer,
                radius,
                spread,
                renderer_resources,
                font_manager,
                dpi,
                &mut glyph_cache.borrow_mut(),
                (0.0, 0.0),
            )
        });
        let kept = pixels.and_then(|pixels| cropped(pixels.data(), buffer, anchor, sought.scale));
        spent(started.elapsed());
        kept.map_or(Drawn::NotAPicture, |kept| Drawn::Picture { kept, box_bytes })
    }
}

/// The part of a picture that paints: the columns `x0..x1` and rows `y0..y1` of
/// `data` (premultiplied RGBA, `width` x `height`) outside which every pixel is
/// clear. `None` for a picture that paints nothing.
fn ink_box(data: &[u8], width: usize, height: usize) -> Option<(usize, usize, usize, usize)> {
    let (mut x0, mut y0, mut x1, mut y1) = (width, height, 0, 0);
    for y in 0..height {
        let row = data.get(y * width * 4..(y + 1) * width * 4)?;
        let mut span: Option<(usize, usize)> = None;
        for (x, pixel) in row.chunks_exact(4).enumerate() {
            if pixel[3] != 0 {
                span = Some((span.map_or(x, |(first, _)| first), x));
            }
        }
        if let Some((first, last)) = span {
            x0 = x0.min(first);
            x1 = x1.max(last + 1);
            y0 = y0.min(y);
            y1 = y + 1;
        }
    }
    (x1 > x0 && y1 > y0).then_some((x0, y0, x1, y1))
}

/// The halo drawn into `buffer` (`data`: its premultiplied RGBA pixels) as it is
/// kept: cut to what it paints, with one clear pixel left all round where the box
/// has one (a picture painted turned or stretched is sampled between its texels;
/// its rim is clear, so nothing of it is cut hard). The box is the ink of the
/// runs padded by an em and more each way (`DisplayListItem::visual_bounds`):
/// kept whole, a street name's halo was 65 KB of which 28 KB paint, a house
/// number's 23 KB of which 5 KB do. `None` when nothing of it paints.
#[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)] // a box of at most 2^21 px
fn cropped(data: &[u8], buffer: PixelBox, anchor: (i32, i32), scale: f32) -> Option<Kept> {
    let (width, height) = (buffer.width as usize, buffer.height as usize);
    let (x0, y0, x1, y1) = ink_box(data, width, height)?;
    let (x0, y0) = (x0.saturating_sub(1), y0.saturating_sub(1));
    let (x1, y1) = ((x1 + 1).min(width), (y1 + 1).min(height));
    let mut pixels = Vec::with_capacity((x1 - x0) * (y1 - y0) * 4);
    for y in y0..y1 {
        pixels.extend_from_slice(data.get((y * width + x0) * 4..(y * width + x1) * 4)?);
    }
    let image = ImageRef::new_rawimage(RawImage {
        pixels: RawImageData::U8(pixels.into()),
        width: x1 - x0,
        height: y1 - y0,
        premultiplied_alpha: true,
        data_format: RawImageFormat::RGBA8,
        tag: Vec::new().into(),
    })?;
    Some(Kept {
        image,
        offset: (buffer.x + x0 as i32 - anchor.0, buffer.y + y0 as i32 - anchor.1),
        width: (x1 - x0) as u32,
        height: (y1 - y0) as u32,
        scale,
    })
}

/// Guards of audit A5 (`audit/A5.md`): the kept halos of the GPU backend.
#[cfg(test)]
mod audit_a5_tests {
    use azul_core::geom::LogicalSize;
    use azul_css::props::{
        basic::pixel::{PixelValue, PixelValueNoPercent},
        style::box_shadow::StyleBoxShadow,
    };

    use super::*;

    /// A halo's picture of `width` x `height` px, drawn at `scale`.
    fn picture(width: u32, height: u32, scale: f32) -> Kept {
        Kept {
            image: ImageRef::null_image(
                width as usize,
                height as usize,
                RawImageFormat::RGBA8,
                Vec::new(),
            ),
            offset: (-6, -22),
            width,
            height,
            scale,
        }
    }

    /// What label `n` of a view is found by: a text and a shape of its own.
    fn label(n: u64) -> Sought {
        Sought {
            text: n,
            shape: 1_000_000 + n,
            anchor: (0, 0),
            scale: 24.0,
        }
    }

    const GEOMETRY: [i32; 3] = [0, 300, 1836];
    /// The bytes of the 4 x 4 px pictures the view's labels get here.
    const BYTES: usize = 4 * 4 * 4;

    /// One frame of a view of `labels` halos as `text_halos` runs it: the scan
    /// that draws (a kept halo is used, a missing one drawn while the frame may
    /// draw - `budget` of them - and there is room), then the translation of the
    /// same lists, which only looks. Returns the labels the translation finds a
    /// picture for: the labels this frame paints with their halo.
    fn frame(cache: &mut HaloCache, labels: u64, budget: usize) -> Vec<u64> {
        cache.begin_scan();
        let mut drawn = 0;
        for n in 0..labels {
            let sought = label(n);
            if let Some(id) = cache.find(sought.text, &GEOMETRY) {
                let _ = cache.use_own(id, true);
            } else if drawn < budget && cache.make_room(BYTES) {
                drawn += 1;
                cache.insert(&sought, &GEOMETRY, Some(picture(4, 4, 24.0)), BYTES, false);
            }
        }
        (0..labels)
            .filter(|n| cache.find(label(*n).text, &GEOMETRY).is_some())
            .collect()
    }

    /// A5-01. The Tuebingen view names 1,567 halos and the cache held 1,024 (on a
    /// 2x display fewer: 32 MB of 65 KB boxes are 500). Each frame that drew the
    /// halos it lacked dropped as many of the ones it had just used - the entries
    /// used longest ago were the first labels of the same scan - and the next frame
    /// drew those again: frame by frame a few labels lost their halo and others got
    /// theirs back, for as long as frames were made. A scan drops nothing it has
    /// used: the labels that have a halo keep it, frame after frame, and an
    /// unchanged view comes to rest - no picture drawn, none dropped.
    ///
    /// Mutation: without `.filter(|(_, entry)| entry.used_in != scan)` in
    /// `HaloCache::make_room` the frame after the cache is full paints fewer labels
    /// with a halo than the one before it, and this fails.
    #[test]
    fn a_view_of_more_halos_than_the_cache_holds_keeps_the_halos_it_has() {
        const LABELS: u64 = 1567;
        const HELD: usize = 1024;
        let mut cache = HaloCache::new(HELD, usize::MAX);
        let mut before: Vec<u64> = Vec::new();
        for n in 0..200 {
            let now = frame(&mut cache, LABELS, 12);
            // The labels are painted in list order: the ones with a halo are the
            // first of the list, and stay the first.
            assert!(now.starts_with(&before), "frame {n}: a label that had its halo lost it");
            assert_eq!(cache.stats.evicted, 0, "frame {n}: a picture the view shows was dropped");
            assert_eq!(now.len(), (12 * (n + 1)).min(HELD), "frame {n}: twelve more a frame");
            before = now;
        }
        assert_eq!(before.len(), HELD, "the cache is full of the view's own halos");
        // At rest: the frames that follow draw nothing and drop nothing.
        let now = frame(&mut cache, LABELS, 12);
        assert_eq!(now, before);
        assert_eq!((cache.stats.created, cache.stats.evicted), (0, 0));
        assert_eq!(cache.stats.frame_halos, HELD as u64);
        assert_eq!(cache.stats.evicted_total, 0);
    }

    /// A5-01. Pictures no frame shows any more make room, the one used longest
    /// ago first; the pictures of the scan under way never do.
    #[test]
    fn pictures_of_earlier_frames_make_room_oldest_first() {
        let mut cache = HaloCache::new(3, usize::MAX);
        cache.begin_scan();
        for n in 0..3 {
            assert!(cache.make_room(BYTES));
            cache.insert(&label(n), &GEOMETRY, Some(picture(4, 4, 24.0)), BYTES, false);
        }
        assert!(!cache.make_room(BYTES), "the scan uses all three: there is no room for a fourth");
        assert_eq!(cache.entries.len(), 3, "and nothing was dropped for it");

        // The next frame shows labels 2 and 1 (in that order) and a new one.
        cache.begin_scan();
        for n in [2, 1] {
            let id = cache.find(label(n).text, &GEOMETRY).expect("kept");
            assert!(cache.use_own(id, true).is_some());
        }
        assert!(cache.make_room(BYTES), "label 0 is of an earlier frame");
        cache.insert(&label(3), &GEOMETRY, Some(picture(4, 4, 24.0)), BYTES, false);
        assert!(cache.find(label(0).text, &GEOMETRY).is_none(), "label 0 made room");
        assert_eq!((cache.stats.evicted, cache.stats.evicted_total), (1, 1));
        assert!(!cache.make_room(BYTES), "1, 2 and 3 are this frame's");

        // A frame that shows none of them: the one used longest ago goes first.
        cache.begin_scan();
        assert!(cache.make_room(BYTES));
        cache.insert(&label(4), &GEOMETRY, Some(picture(4, 4, 24.0)), BYTES, false);
        assert!(cache.find(label(2).text, &GEOMETRY).is_none(), "2 was used before 1 and 3");
        assert!(cache.find(label(1).text, &GEOMETRY).is_some());
        assert!(cache.find(label(3).text, &GEOMETRY).is_some());
        assert_eq!(cache.bytes, 3 * BYTES);
    }

    /// A5-01. The bytes bound as the entries bound: the scan's own pictures stay,
    /// and a picture larger than the whole budget is never kept.
    #[test]
    fn the_bytes_of_a_scans_own_pictures_are_not_given_up() {
        let mut cache = HaloCache::new(usize::MAX, 3 * BYTES);
        cache.begin_scan();
        for n in 0..3 {
            assert!(cache.make_room(BYTES));
            cache.insert(&label(n), &GEOMETRY, Some(picture(4, 4, 24.0)), BYTES, false);
        }
        assert!(!cache.make_room(1), "not one byte is free, and the scan uses them all");
        assert!(!cache.make_room(4 * BYTES), "more than the cache holds");
        assert_eq!(cache.bytes, 3 * BYTES);
        cache.begin_scan();
        assert!(cache.make_room(2 * BYTES), "an earlier frame's pictures make room");
        assert_eq!(cache.bytes, BYTES);
    }

    /// A5-06 (A9 H14). A zoom that settles in a full cache draws a view of new
    /// pictures and drops as many old ones. The entries to drop are sorted once
    /// for the scan, not searched for the oldest once per picture dropped (n
    /// searches of n entries): one list for 256 pictures, and one for 1,024.
    ///
    /// Mutation: `self.victims = Vec::new()` after a victim is returned in
    /// `next_victim` (a list per dropped entry) fails this.
    #[test]
    fn dropping_a_view_of_old_pictures_sorts_the_cache_once() {
        for n in [256u64, 1024] {
            let mut cache = HaloCache::new(n as usize, usize::MAX);
            cache.begin_scan();
            for old in 0..n {
                assert!(cache.make_room(BYTES));
                cache.insert(&label(old), &GEOMETRY, Some(picture(4, 4, 24.0)), BYTES, false);
            }
            cache.begin_scan();
            let lists = cache.victim_lists;
            for new in n..2 * n {
                assert!(cache.make_room(BYTES), "{new}: an old picture makes room");
                cache.insert(&label(new), &GEOMETRY, Some(picture(4, 4, 24.0)), BYTES, false);
            }
            assert_eq!(cache.stats.evicted, n, "every old picture went");
            assert_eq!(cache.victim_lists - lists, 1, "{n} pictures dropped from one list");
            assert_eq!(cache.entries.len() as u64, n);
            assert_eq!(cache.bytes, n as usize * BYTES);
            // The oldest went first: the last one dropped was the newest old one.
            assert!(cache.find(label(n).text, &GEOMETRY).is_some());
            assert!(!cache.make_room(BYTES), "the rest is this scan's own");
        }
    }

    /// The display list of a label of two glyphs, `advance` apart, whose first pen
    /// is at `x`, `y`, in 12 px type under a spread shadow.
    fn label_items(x: f32, y: f32, advance: f32) -> Vec<DisplayListItem> {
        let glyph = |index: u32, dx: f32| GlyphInstance {
            index,
            point: LogicalPosition::new(x + dx, y),
            size: LogicalSize::new(0.0, 0.0),
        };
        let clip = LogicalRect::new(
            LogicalPosition::new(x - 32.0, y - 48.0),
            LogicalSize::new(256.0, 96.0),
        );
        vec![
            DisplayListItem::PushTextShadow {
                shadow: StyleBoxShadow {
                    spread_radius: PixelValueNoPercent {
                        inner: PixelValue::px(2.0),
                    },
                    color: ColorU::WHITE,
                    ..StyleBoxShadow::default()
                },
            },
            DisplayListItem::Text {
                glyphs: vec![glyph(40, 0.0), glyph(41, advance)],
                font_hash: FontHash::from_hash(7),
                font_size_px: 12.0,
                color: ColorU::BLACK,
                clip_rect: clip.into(),
                line_box: clip.into(),
                source_node_index: None,
            },
            DisplayListItem::PopTextShadow,
        ]
    }

    /// A5-03. A label laid out at another place is the same halo. The second
    /// glyph's place from the first is the difference of two window positions:
    /// with an advance of 4.6953125 px it is 4.6953125 from a pen at 20.0
    /// (300.5 steps of 1/64 px, rounded 301) and 2052.59521484375 -
    /// 2047.9000244140625 = 4.6951904296875 from a pen at 2047.9 (300.49 steps,
    /// rounded 300). The positions were hashed: the label panned by 2027.9 px had
    /// another key, no halo, and was drawn again.
    ///
    /// Mutation: `<= 1` -> `== 0` in `close` fails this.
    #[test]
    fn a_label_laid_out_at_another_place_finds_its_halo() {
        let near = label_items(20.0, 100.0, 4.695_312_5);
        let far = label_items(2047.9, 613.3, 4.695_312_5);
        let (mut here, mut there) = (Vec::new(), Vec::new());
        let a = scopes(&near)[0].sought(2.0, &mut here);
        let b = scopes(&far)[0].sought(2.0, &mut there);
        assert_eq!((a.text, a.shape), (b.text, b.shape), "the same glyphs in the same font");
        // The hand-computed example: the second glyph's x, after the two rects
        // (four values each) and the first glyph's x and y.
        assert_eq!((here[10], there[10]), (301, 300), "premise: the rounding differs");
        assert!(close(&here, &there), "{here:?} / {there:?}");

        let mut cache = HaloCache::new(8, usize::MAX);
        cache.begin_scan();
        cache.insert(&a, &here, Some(picture(40, 30, 24.0)), 40 * 30 * 4, false);
        assert!(cache.find(b.text, &there).is_some(), "the moved label finds the kept halo");

        // Another advance is another text: two steps apart is not the same.
        let mut wider = Vec::new();
        let c = scopes(&label_items(20.0, 100.0, 4.75))[0].sought(2.0, &mut wider);
        assert_eq!(c.text, a.text);
        assert!(!close(&here, &wider));
        assert!(cache.find(c.text, &wider).is_none());
    }

    /// A5-02. A zoom that settles lays every label out at a new size: a new text
    /// (the size is part of it) of the same shape. Until its own picture is drawn
    /// the label is painted with the old one, stretched about the first glyph's
    /// pen. Hand-computed: a picture of 100 x 30 px whose corner is 6 px left of
    /// and 22 px above the pen, drawn at 24 px per em; the label at 36 px per em
    /// (a zoom by 1.5) with its pen on (300, 200) paints it from (300 - 9, 200 -
    /// 33) = (291, 167), 150 x 45 px. Its own picture is painted where it lies.
    #[test]
    fn a_stand_in_is_stretched_about_the_first_glyphs_pen() {
        let old = picture(100, 30, 24.0);
        assert_eq!(
            placed(&old, (300, 200), Some(36.0 / 24.0)),
            ((291.0, 167.0), (150.0, 45.0))
        );
        assert_eq!(placed(&old, (300, 200), None), ((294.0, 178.0), (100.0, 30.0)));
    }

    /// A5-02. The stand-in of a shape is its newest picture; the size is no part
    /// of the shape, the fonts and the glyphs are. Standing in does not keep a
    /// picture from making room: the pictures that replace the stand-ins need it.
    #[test]
    fn the_newest_picture_of_a_shape_stands_in_and_holds_no_room() {
        let near = label_items(20.0, 100.0, 6.0);
        let mut geometry = Vec::new();
        let small = scopes(&near)[0].sought(1.0, &mut geometry);
        let large = scopes(&near)[0].sought(2.0, &mut geometry);
        assert_ne!(small.text, large.text, "another scale is another picture");
        assert_eq!(small.shape, large.shape, "of the same shape");
        assert_eq!((small.scale, large.scale), (12.0, 24.0));

        let mut cache = HaloCache::new(2, usize::MAX);
        cache.begin_scan();
        assert!(cache.stand_in(small.shape, true, |_| true).is_none(), "nothing kept yet");
        cache.insert(&small, &geometry, Some(picture(50, 15, small.scale)), 3000, false);
        let stood = cache.stand_in(large.shape, true, |_| true).expect("the small one stands in");
        assert_eq!((stood.width, stood.scale), (50, 12.0));
        assert_eq!(cache.stats.stand_ins, 1);
        assert!(
            cache.stand_in(large.shape, true, |kept| kept.scale > 12.0).is_none(),
            "a picture that does not fit does not stand in"
        );

        // The next frame: the label is drawn at the large size while the small
        // picture stands in, then the cache needs the room.
        cache.begin_scan();
        assert!(cache.stand_in(large.shape, true, |_| true).is_some());
        cache.insert(&large, &geometry, Some(picture(100, 30, large.scale)), 12000, false);
        let newest = cache.stand_in(small.shape, false, |_| true).expect("a picture of the shape");
        assert_eq!(newest.scale, 24.0, "the newest one");
        assert!(cache.make_room(1), "the stand-in was not this frame's own picture");
        assert_eq!(cache.entries.len(), 1);
        assert!(cache.find(large.text, &geometry).is_some(), "the frame's own picture stays");
    }

    /// A5-04. A picture is kept cut to what it paints, one clear pixel left all
    /// round where the box has one. A box of 8 x 6 px with ink in columns 3..=4
    /// of rows 2..=3 is kept as columns 2..6 of rows 1..5: 4 x 4 px, its corner
    /// (2, 1) further from the anchor than the box's.
    #[test]
    fn a_picture_is_cut_to_its_ink() {
        let (width, height) = (8usize, 6usize);
        let mut data = vec![0u8; width * height * 4];
        for (x, y) in [(3, 2), (4, 2), (3, 3), (4, 3)] {
            data[(y * width + x) * 4..(y * width + x) * 4 + 4].copy_from_slice(&[200; 4]);
        }
        assert_eq!(ink_box(&data, width, height), Some((3, 2, 5, 4)));
        let buffer = PixelBox {
            x: 100,
            y: 50,
            width: 8,
            height: 6,
        };
        let kept = cropped(&data, buffer, (104, 53), 24.0).expect("it paints");
        assert_eq!((kept.width, kept.height), (4, 4));
        assert_eq!(kept.offset, (100 + 2 - 104, 50 + 1 - 53));
        let bytes = kept.image.get_bytes().expect("a picture in memory");
        assert_eq!(bytes.len(), 4 * 4 * 4);
        // Row 1 of the cut is row 2 of the box: clear, ink, ink, clear.
        let alpha: Vec<u8> = bytes[16..32].chunks_exact(4).map(|p| p[3]).collect();
        assert_eq!(alpha, [0, 200, 200, 0]);

        assert_eq!(ink_box(&vec![0u8; 8 * 6 * 4], 8, 6), None, "nothing paints");
        assert!(cropped(&vec![0u8; 8 * 6 * 4], buffer, (104, 53), 24.0).is_none());
        // Ink in the box's corner: there is no pixel to leave clear beyond it.
        let mut corner = vec![0u8; 8 * 6 * 4];
        corner[..4].copy_from_slice(&[255; 4]);
        let kept = cropped(&corner, buffer, (104, 53), 24.0).expect("it paints");
        assert_eq!((kept.width, kept.height, kept.offset), (2, 2, (-4, -3)));
    }

    /// The budget of a frame is time spent drawing, not time since its scan
    /// began, and a halo that is left counts as left, not as drawn.
    #[test]
    fn the_budget_counts_what_is_drawn_and_what_is_left() {
        let ((), left) = within_halo_budget(2, Duration::from_millis(5), || {
            assert!(admit(BYTES) && admit(BYTES), "two as of right");
            assert!(admit(BYTES), "no time spent yet: a third");
            spend(Duration::from_millis(5));
            assert!(!admit(BYTES), "the time is spent");
            assert!(!admit(BYTES));
        });
        assert_eq!(left, 2);
        let stats = text_halo_stats();
        assert_eq!((stats.pending, stats.unkept, stats.scans), (2, 0, 1));
        assert!(admit(BYTES), "no budget: every halo is drawn");
    }
}
