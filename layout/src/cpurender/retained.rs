//! Group surfaces kept from frame to frame.
//!
//! A reference frame whose matrix is not a translation is painted into a surface of its own and
//! composited through the matrix (`raster::TransformGroup`). The picture in that surface does
//! not depend on the matrix: a zoom step, a turn, a slide that only writes a new matrix paints
//! the same picture again. Here the surface is kept, with the part of it that was painted, and
//! the next frame composites it instead of painting it.
//!
//! # When a kept surface is used
//!
//! A surface is the picture of ONE list (held, so it cannot change while the surface is kept)
//! from ONE frame's push to its pop, and a later walk composites it only when everything else
//! the picture depends on is the same ([`SurfaceKey`]):
//!
//! 1. the list: the same allocation. A list that is rebuilt or patched (a tile arriving, a
//!    label edited, a caret blinking) is another allocation, because the entry holds the old
//!    one; the entry is dropped at the next frame ([`RetainedFrames::begin_frame`]);
//! 2. the frame: the index of its push in that list;
//! 3. the surface's size in device pixels, and the scale factor;
//! 4. the sub-pixel PHASE of the surface: where the frame's extent begins within its first
//!    device pixel (in 1/1024 px). The content is painted at `(pos - offset) dpi - origin`,
//!    with `origin = floor((extent - offset) dpi)`: at an offset that differs by whole device
//!    pixels both terms move by the same whole number and the picture is the same one, landing
//!    that many pixels away (the composite places it with the live geometry). This is the
//!    claim the pixel shift of a scroll or a pan already makes (pixels painted at the old
//!    offset are moved next to a strip painted at the new one), no more: a pan of a scaled
//!    view composites the kept picture. At another phase the picture is another one;
//! 5. the live values bound INSIDE the frame (the matrices of the frames in it, the opacities
//!    of its groups, bound colours, scrollbar thumbs, the offsets of its scroll frames): a
//!    value written inside the frame changes the picture without changing the list;
//! 6. what text is painted with: the anti-aliasing settings and the fonts that are loaded;
//!    the system colours;
//! 7. the part the composite reads lies inside what was painted ([`Entry::valid`]): a zoom out
//!    that shows more of the surface than was painted is painted (and what it paints is kept).
//!
//! The frame's OWN matrix, the clip around it and the opacity of a group around it are not part
//! of the key: they are what may change. A frame that holds a view is never kept (its picture
//! depends on another list).
//!
//! # Bounds
//!
//! ONE surface of up to [`DEFAULT_MAX_BYTES`] (`AZ_CPU_RETAINED_MB`; `0` keeps nothing) is
//! always kept: the picture of the view being zoomed. A second one ([`MAX_ENTRIES`]) is kept
//! only where the two together are cheap, at most [`PAIR_MAX_BYTES`]. Sizes are the surfaces'
//! full sizes, although a page of a surface costs memory only once it is painted.
//!
//! The arithmetic: a map view of the whole window with the editor's 256 px of overscan on each
//! side is (w + 512) x (h + 512) logical px. At 2560 x 1440 and a scale factor of 1:
//! 3072 x 1952 px = 24.0 MB, two fit the pair bound. On a 1470 x 956 pt Retina panel (2940 x
//! 1912 px): 3964 x 2936 px = 46.6 MB: one is kept, a second would make 93 MB and is not. A
//! 4K panel at a scale factor of 2 (7680 x 4320 px): 8704 x 5344 px = 186 MB: one is kept.
//! A surface over the bound for one is not kept, its frame is painted on every step as before,
//! and the first time that happens it is said on stderr (a bound under a view's surface would
//! otherwise switch the feature off without a trace).
//!
//! # Opacity groups
//!
//! A fade is a write of one value, like a zoom step: the picture of an opacity group does not
//! depend on the group's opacity, only the blend at its pop does. The picture of a group that
//! is blended (an opacity under 1, or a blend mode) is kept the same way, under the same key
//! with the index of its `PushOpacity` as `push`, and a later walk blends the kept pixels at
//! the live opacity instead of painting the group (`raster::fade_candidate`). The surface is
//! the group's whole device box; a pass keeps and reads the part of it inside its clip. The
//! group's own opacity is not part of the key. A group at an opacity of exactly 1 with no
//! blend mode is never kept: its text may be painted in LCD stripes over what the group
//! painted under it, and at any other opacity it is grayscale - another picture.
//!
//! These pictures are bounded apart from the frames' surfaces, so that a fading toast does
//! not make the picture of the map it lies over leave the cache: at most
//! [`MAX_FADE_ENTRIES`] of them, [`FADE_MAX_BYTES`] together, the one used longest ago makes
//! room. A list that is replaced drops them (they are not repaired box by box).
//!
//! # Threads
//!
//! The cache is read while a frame's passes run and changed only between frames: every band of
//! a frame sees the same state, so what a band hits does not depend on thread timing. A pass
//! that paints a frame hands its surface to [`RetainedFrames::offer`]; the backend calls
//! [`RetainedFrames::commit`] when the frame is painted.

use std::sync::{Arc, Mutex};

use super::pixmap::AzulPixmap;
use crate::solver3::display_list::DisplayList;

/// The most surfaces kept at once.
pub const MAX_ENTRIES: usize = 2;

/// The most bytes ONE kept surface may hold unless `AZ_CPU_RETAINED_MB` says otherwise.
pub const DEFAULT_MAX_BYTES: usize = 192 * 1024 * 1024;

/// The most bytes two kept surfaces may hold together: a second surface is kept only when it
/// is cheap (two views of a 2560 x 1440 window at a scale factor of 1; not two on a Retina
/// panel).
pub const PAIR_MAX_BYTES: usize = 48 * 1024 * 1024;

/// The most pictures of opacity groups kept at once (see the module, "Opacity groups").
pub const MAX_FADE_ENTRIES: usize = 8;

/// The most bytes the kept pictures of opacity groups hold together: one group as large as a
/// 2940 x 1912 px window is 22.5 MB.
pub const FADE_MAX_BYTES: usize = 24 * 1024 * 1024;

/// The fewest pixels a surface must have to be kept: a label's surface is painted faster than
/// it is looked up, and there are a thousand of them.
pub(crate) const MIN_SURFACE_PX: u64 = 256 * 256;

/// The most painted boxes remembered for one surface.
const MAX_VALID_BOXES: usize = 32;

/// What the picture in a kept surface depends on, besides the items of the list it holds: see
/// the module (rules 1 to 6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct SurfaceKey {
    /// The list's allocation (`Arc::as_ptr`), which the entry holds.
    pub(crate) list: usize,
    /// The index of the frame's `PushReferenceFrame` in the list.
    pub(crate) push: usize,
    /// The surface's size, device px.
    pub(crate) size: (u32, u32),
    /// The scale factor's bits.
    pub(crate) dpi_bits: u32,
    /// Where the frame's extent begins within its first device pixel, in 1/1024 px (0 ..=
    /// 1024, not wrapped: a phase just under a whole pixel is another origin than one just over).
    pub(crate) phase: (i32, i32),
    /// A hash of the live values bound inside the frame.
    pub(crate) values: u64,
    /// A hash of the text settings, the loaded fonts and the system colours.
    pub(crate) settings: u64,
}

/// One kept surface.
pub(crate) struct Entry {
    pub(crate) key: SurfaceKey,
    /// The list the picture was painted from: held, so that no patch can change it in place
    /// and no other list can take its address while the surface is kept.
    pub(crate) list: Arc<DisplayList>,
    pub(crate) surface: Arc<AzulPixmap>,
    /// The boxes of the surface that hold the picture, `[x0, y0, x1, y1]` in surface px.
    pub(crate) valid: Vec<[i32; 4]>,
    /// Where the picture was painted from ([`Placement`]).
    pub(crate) placement: Placement,
    /// The picture of an opacity group, not of a reference frame: bounded apart, and dropped
    /// (not repaired) when its list is replaced.
    pub(crate) fade: bool,
    last_used: u64,
    /// (A10-06) The clock when the surface's pixels last changed: a commit that made it or
    /// merged a painted part into it, a repair ([`RetainedFrames::put_back`]). What a
    /// presenter that shows the surface itself compares ([`RetainedFrames::kept`]).
    painted_at: u64,
}

/// Where a kept picture was painted from, for a repaint of a part of it when its list is
/// replaced (`raster::refresh_retained_surfaces`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Placement {
    /// The walk's offset inside the group when the picture was painted: the item at list
    /// position `p` is at surface pixel `(p - offset) dpi`.
    pub(crate) offset: (f32, f32),
    /// Which list of the window the frame is in: a view's DOM id, `usize::MAX` for the
    /// window's own list.
    pub(crate) slot: usize,
}

impl Entry {
    fn bytes(&self) -> usize {
        self.surface.data().len()
    }
}

/// A part of a frame's picture that a pass painted, on its way into the cache.
struct Painted {
    key: SurfaceKey,
    list: Arc<DisplayList>,
    surface: AzulPixmap,
    part: [i32; 4],
    placement: Placement,
    fade: bool,
}

#[derive(Default)]
struct Inner {
    entries: Vec<Entry>,
    outbox: Vec<Painted>,
    clock: u64,
    max_bytes: usize,
}

/// The group surfaces a backend keeps between its frames. Shared with the frame's passes
/// through `CpuRenderState::retained`.
pub struct RetainedFrames {
    inner: Mutex<Inner>,
}

impl core::fmt::Debug for RetainedFrames {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "RetainedFrames({} surfaces, {} B)", self.len(), self.bytes())
    }
}

impl Default for RetainedFrames {
    fn default() -> Self {
        Self::new()
    }
}

/// `AZ_CPU_RETAINED_MB` as a byte count: unset, empty or not a number is `None`.
fn parse_megabytes(v: Option<&str>) -> Option<usize> {
    v.and_then(|s| s.trim().parse::<usize>().ok())
        .map(|mb| mb.saturating_mul(1024 * 1024))
}

/// Does the union of `boxes` cover `needed`? (What of `needed` no box covers is empty.)
fn covers(boxes: &[[i32; 4]], needed: [i32; 4]) -> bool {
    let mut left = vec![needed];
    for b in boxes {
        left = left
            .into_iter()
            .flat_map(|piece| super::raster::box_minus(piece, *b))
            .collect();
        if left.is_empty() {
            return true;
        }
    }
    left.iter().all(|p| p[2] <= p[0] || p[3] <= p[1])
}

/// (A10-06) What of `needed` no box of `boxes` covers, as boxes (none of them empty): empty
/// exactly when [`covers`] is true.
fn uncovered(boxes: &[[i32; 4]], needed: [i32; 4]) -> Vec<[i32; 4]> {
    let mut left = vec![needed];
    for b in boxes {
        left = left
            .into_iter()
            .flat_map(|piece| super::raster::box_minus(piece, *b))
            .collect();
        if left.is_empty() {
            break;
        }
    }
    left.retain(|p| p[2] > p[0] && p[3] > p[1]);
    left
}

/// Copy the box `part` of `from` into `to`; both have the same size.
#[allow(clippy::cast_sign_loss)] // a box inside the surface
fn copy_part(from: &AzulPixmap, to: &mut AzulPixmap, part: [i32; 4]) {
    if from.width() != to.width() || from.height() != to.height() {
        return;
    }
    let (w, h) = (from.width() as i32, from.height() as i32);
    let [x0, y0, x1, y1] = [part[0].clamp(0, w), part[1].clamp(0, h), part[2].clamp(0, w), part[3].clamp(0, h)];
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    let stride = w as usize * 4;
    let (left, right) = (x0 as usize * 4, x1 as usize * 4);
    let (src, dst) = (from.data(), to.data_mut());
    for y in y0 as usize..y1 as usize {
        let row = y * stride;
        dst[row + left..row + right].copy_from_slice(&src[row + left..row + right]);
    }
}

impl RetainedFrames {
    /// A cache that keeps nothing yet, with the bound of the environment
    /// (`AZ_CPU_RETAINED_MB`) or [`DEFAULT_MAX_BYTES`].
    #[must_use]
    pub fn new() -> Self {
        Self::with_max_bytes(
            parse_megabytes(std::env::var("AZ_CPU_RETAINED_MB").ok().as_deref())
                .unwrap_or(DEFAULT_MAX_BYTES),
        )
    }

    /// A cache whose surfaces hold at most `max_bytes` together.
    #[must_use]
    pub fn with_max_bytes(max_bytes: usize) -> Self {
        Self {
            inner: Mutex::new(Inner { max_bytes, ..Inner::default() }),
        }
    }

    /// Bytes the kept surfaces hold (their full size).
    #[must_use]
    pub fn bytes(&self) -> usize {
        self.inner
            .lock()
            .map_or(0, |inner| inner.entries.iter().map(Entry::bytes).sum())
    }

    /// How many surfaces are kept.
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.lock().map_or(0, |inner| inner.entries.len())
    }

    /// Whether no surface is kept.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Drop every kept surface.
    pub fn clear(&self) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.entries.clear();
            inner.outbox.clear();
        }
    }

    /// The start of a frame: `current` are the lists the frame paints (their `Arc::as_ptr`).
    /// A surface of any other list is dropped - its list was rebuilt, patched or removed -
    /// which also lets go of that list. Returns how many were dropped.
    pub fn begin_frame(&self, current: &[usize]) -> usize {
        let Ok(mut inner) = self.inner.lock() else {
            return 0;
        };
        inner.outbox.clear();
        inner.clock += 1;
        let before = inner.entries.len();
        inner.entries.retain(|entry| current.contains(&entry.key.list));
        let dropped = before - inner.entries.len();
        crate::cpu_counters::count(
            crate::cpu_counters::Counter::RetainedInvalidations,
            dropped as u64,
        );
        dropped
    }

    /// The kept surface of `key` when the box `needed` of it (surface px, what a composite
    /// reads) was painted: the composite may read it. `None` is a miss: the frame is painted.
    pub(crate) fn lookup(&self, key: &SurfaceKey, needed: [i32; 4]) -> Option<Arc<AzulPixmap>> {
        let mut inner = self.inner.lock().ok()?;
        // The FRAME's stamp ([`Self::begin_frame`] advances the clock), not one per lookup:
        // band threads look up in no fixed order, and a stamp per lookup made "used longest
        // ago" - which surface makes room at the next commit - depend on which band came
        // last (A2-cpu-R07: no pixel, but the hit and miss counters of later frames). Two
        // surfaces used by one frame tie, and the tie falls to the first entry, whose order
        // `commit` fixes.
        let clock = inner.clock;
        let entry = inner.entries.iter_mut().find(|entry| entry.key == *key)?;
        if !covers(&entry.valid, needed) {
            return None;
        }
        entry.last_used = clock;
        Some(Arc::clone(&entry.surface))
    }

    /// (A10-06) The parts of the box `needed` (surface px) of the surface kept for `key` that
    /// are NOT painted, as boxes: empty when [`Self::lookup`] of the same box would hit.
    /// `None` when no surface is kept for `key`. Not a use: nothing is stamped.
    pub(crate) fn missing(&self, key: &SurfaceKey, needed: [i32; 4]) -> Option<Vec<[i32; 4]>> {
        let inner = self.inner.lock().ok()?;
        let entry = inner.entries.iter().find(|entry| entry.key == *key)?;
        Some(uncovered(&entry.valid, needed))
    }

    /// (A10-06) The surface kept for `key` and the stamp of its pixels (another value
    /// whenever they changed), for a presenter that shows the surface itself. A use, stamped
    /// with its frame like a lookup. The caller lets go of the surface before the next
    /// [`Self::commit`]: a part painted then is merged into a surface nobody else holds, and
    /// replaces one that somebody does.
    pub(crate) fn kept(&self, key: &SurfaceKey) -> Option<(Arc<AzulPixmap>, u64)> {
        let mut inner = self.inner.lock().ok()?;
        let clock = inner.clock;
        let entry = inner.entries.iter_mut().find(|entry| entry.key == *key)?;
        entry.last_used = clock;
        Some((Arc::clone(&entry.surface), entry.painted_at))
    }

    /// Whether a surface of `bytes` bytes can be kept at all.
    pub(crate) fn can_keep(&self, bytes: usize) -> bool {
        let max_bytes = self.inner.lock().map_or(0, |inner| inner.max_bytes);
        if bytes > max_bytes && max_bytes > 0 {
            static SAID: std::sync::Once = std::sync::Once::new();
            SAID.call_once(|| {
                eprintln!(
                    "[cpurender] a group surface of {} MB is over the bound of {} MB \
                     (AZ_CPU_RETAINED_MB): it is not kept and its frame is painted on every step",
                    bytes >> 20,
                    max_bytes >> 20,
                );
            });
        }
        bytes > 0 && bytes <= max_bytes
    }

    /// A pass painted the box `part` of the surface of `key` (the whole `surface` otherwise
    /// transparent or painted by nobody): kept from the next [`Self::commit`] on. `list` is the
    /// list the frame is in.
    pub(crate) fn offer(
        &self,
        key: SurfaceKey,
        list: Arc<DisplayList>,
        surface: AzulPixmap,
        part: [i32; 4],
        placement: Placement,
    ) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.outbox.push(Painted { key, list, surface, part, placement, fade: false });
        }
    }

    /// Whether the picture of an opacity group of `bytes` bytes can be kept at all.
    pub(crate) fn can_keep_fade(&self, bytes: usize) -> bool {
        let max_bytes = self.inner.lock().map_or(0, |inner| inner.max_bytes);
        bytes > 0 && bytes <= FADE_MAX_BYTES.min(max_bytes)
    }

    /// [`Self::offer`] for the picture of an opacity group: `surface` is the group's whole
    /// device box, of which a pass painted the box `part` (the pixels the group painted on
    /// transparent, before its pop blended them into what was under them).
    pub(crate) fn offer_fade(
        &self,
        key: SurfaceKey,
        list: Arc<DisplayList>,
        surface: AzulPixmap,
        part: [i32; 4],
        placement: Placement,
    ) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.outbox.push(Painted { key, list, surface, part, placement, fade: true });
        }
    }

    /// The kept surfaces of lists that are not among `current` any more, taken out of the
    /// cache: the caller repairs each for the list that replaced its own and puts it back
    /// ([`Self::put_back`]), or drops it.
    pub(crate) fn take_stale(&self, current: &[usize]) -> Vec<Entry> {
        let Ok(mut inner) = self.inner.lock() else {
            return Vec::new();
        };
        let (kept, stale): (Vec<Entry>, Vec<Entry>) = core::mem::take(&mut inner.entries)
            .into_iter()
            .partition(|entry| current.contains(&entry.key.list));
        inner.entries = kept;
        stale
    }

    /// Keep `entry` again (after [`Self::take_stale`]), unless a surface of its frame is kept.
    pub(crate) fn put_back(&self, mut entry: Entry) {
        if let Ok(mut inner) = self.inner.lock() {
            let taken = inner
                .entries
                .iter()
                .any(|e| e.key.list == entry.key.list && e.key.push == entry.key.push);
            // (Only a frame's surface is repaired and put back; the pictures of opacity
            // groups are counted apart.)
            if !taken && inner.entries.iter().filter(|e| !e.fade).count() < MAX_ENTRIES {
                // (A10-06) A repaired surface holds other pixels than it did.
                inner.clock += 1;
                entry.painted_at = inner.clock;
                inner.entries.push(entry);
            }
        }
    }

    /// The end of a frame: what its passes painted becomes what the next frame may composite.
    /// The parts of one surface painted by several bands are put together; a surface whose key
    /// changed (a value inside the frame, its size, its phase) replaces the one kept for the
    /// same frame; the surfaces used longest ago make room.
    pub fn commit(&self) {
        let Ok(mut inner) = self.inner.lock() else {
            return;
        };
        let mut parts = core::mem::take(&mut inner.outbox);
        // In a fixed order, whichever thread offered what first.
        parts.sort_by_key(|p| (p.key.list, p.key.push, p.part));
        for painted in parts {
            inner.clock += 1;
            let clock = inner.clock;
            // The same picture, more of it: into the kept surface.
            if let Some(entry) = inner.entries.iter_mut().find(|e| e.key == painted.key) {
                if let Some(kept) = Arc::get_mut(&mut entry.surface) {
                    copy_part(&painted.surface, kept, painted.part);
                    entry.valid.push(painted.part);
                    if entry.valid.len() > MAX_VALID_BOXES {
                        entry.valid.remove(0);
                    }
                    entry.last_used = clock;
                    entry.painted_at = clock;
                    continue;
                }
            }
            // Another picture of the same frame, or a frame not kept yet: this surface is it.
            let before = inner.entries.len();
            inner.entries.retain(|e| {
                !(e.key.list == painted.key.list && e.key.push == painted.key.push)
            });
            crate::cpu_counters::count(
                crate::cpu_counters::Counter::RetainedInvalidations,
                (before - inner.entries.len()) as u64,
            );
            let bytes = painted.surface.data().len();
            if bytes == 0 || bytes > inner.max_bytes {
                continue;
            }
            // Room, among the surfaces of the same kind. Frames: one surface is always
            // kept; another stays beside it only while the two are cheap together. Opacity
            // groups: a few small ones, under a bound of their own. The surfaces used
            // longest ago go.
            let fade = painted.fade;
            let (most_entries, most_together) = if fade {
                (MAX_FADE_ENTRIES, FADE_MAX_BYTES.min(inner.max_bytes))
            } else {
                (MAX_ENTRIES, PAIR_MAX_BYTES.min(inner.max_bytes))
            };
            if fade && bytes > most_together {
                continue;
            }
            loop {
                let (count, held) = inner
                    .entries
                    .iter()
                    .filter(|e| e.fade == fade)
                    .fold((0usize, 0usize), |(n, b), e| (n + 1, b + e.bytes()));
                if (count == 0 && !fade) || (count < most_entries && held + bytes <= most_together) {
                    break;
                }
                let Some(oldest) = inner
                    .entries
                    .iter()
                    .enumerate()
                    .filter(|(_, e)| e.fade == fade)
                    .min_by_key(|(_, e)| e.last_used)
                    .map(|(at, _)| at)
                else {
                    break;
                };
                inner.entries.remove(oldest);
            }
            inner.entries.push(Entry {
                key: painted.key,
                list: painted.list,
                surface: Arc::new(painted.surface),
                valid: vec![painted.part],
                placement: painted.placement,
                fade,
                last_used: clock,
                painted_at: clock,
            });
        }
    }

    /// The lists the kept surfaces hold (their `Arc::as_ptr`), for tests and memory probes.
    #[must_use]
    pub fn held_lists(&self) -> Vec<usize> {
        self.inner.lock().map_or_else(
            |_| Vec::new(),
            |inner| inner.entries.iter().map(|e| Arc::as_ptr(&e.list) as usize).collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(list: &Arc<DisplayList>, push: usize, values: u64) -> SurfaceKey {
        SurfaceKey {
            list: Arc::as_ptr(list) as usize,
            push,
            size: (40, 30),
            dpi_bits: 1.0_f32.to_bits(),
            phase: (0, 0),
            values,
            settings: 7,
        }
    }

    /// A 40 x 30 surface whose box `part` is filled with `value`.
    fn surface(part: [i32; 4], value: u8) -> AzulPixmap {
        let mut s = AzulPixmap::new_transparent(40, 30).expect("a surface");
        s.fill_rect(part[0], part[1], part[2] - part[0], part[3] - part[1], value, value, value, 255);
        s
    }

    const PLACED: Placement = Placement { offset: (0.0, 0.0), slot: 1 };

    fn at(s: &AzulPixmap, x: usize, y: usize) -> u8 {
        s.data()[(y * 40 + x) * 4]
    }

    /// A1-08 (b), rule 7: a composite may read a kept surface only inside what was painted,
    /// and the parts two bands painted are one picture.
    /// Mutations: `covers` answering true for a box that sticks out by one pixel composites
    /// transparent pixels (the hole a zoom out would show); a commit that keeps the last part
    /// only loses the first band's rows.
    #[test]
    fn a_kept_surface_serves_what_was_painted_of_it_and_no_more() {
        let cache = RetainedFrames::with_max_bytes(1 << 20);
        let list = Arc::new(DisplayList::default());
        let k = key(&list, 3, 11);
        let current = [Arc::as_ptr(&list) as usize];
        assert_eq!(cache.begin_frame(&current), 0);
        // Two bands paint the top and the bottom of the surface, with two rows in common.
        cache.offer(k, Arc::clone(&list), surface([0, 0, 40, 16], 100), [0, 0, 40, 16], PLACED);
        cache.offer(k, Arc::clone(&list), surface([0, 14, 40, 30], 100), [0, 14, 40, 30], PLACED);
        assert!(cache.lookup(&k, [0, 0, 40, 30]).is_none(), "nothing is kept before the frame ends");
        cache.commit();
        assert_eq!((cache.len(), cache.bytes()), (1, 40 * 30 * 4));
        let kept = cache.lookup(&k, [0, 0, 40, 30]).expect("both parts together cover the surface");
        assert_eq!((at(&kept, 5, 2), at(&kept, 5, 15), at(&kept, 5, 29)), (100, 100, 100));
        drop(kept);
        // A smaller picture of another frame: only its part is served.
        let other = key(&list, 9, 11);
        cache.offer(other, Arc::clone(&list), surface([10, 10, 20, 20], 50), [10, 10, 20, 20], PLACED);
        cache.commit();
        assert!(cache.lookup(&other, [10, 10, 20, 20]).is_some());
        assert!(cache.lookup(&other, [12, 12, 18, 18]).is_some(), "a part of what was painted");
        assert!(cache.lookup(&other, [10, 10, 21, 20]).is_none(), "one column more than was painted");
        assert!(cache.lookup(&other, [9, 10, 20, 20]).is_none());
        assert!(cache.lookup(&other, [10, 10, 20, 20 + 1]).is_none());
    }

    /// A1-08 (b), rules 1 to 6: another key is another picture. A value inside the frame that
    /// changed replaces the surface kept for the frame; a list that is not painted any more is
    /// dropped at the next frame and let go of.
    /// Mutations: a lookup by `(list, push)` alone composites the picture of the old values;
    /// `begin_frame` keeping the surfaces of lists that are gone holds every old list of a view.
    #[test]
    fn a_kept_surface_is_of_one_list_one_frame_and_one_set_of_values() {
        let cache = RetainedFrames::with_max_bytes(1 << 20);
        let list = Arc::new(DisplayList::default());
        let whole = [0, 0, 40, 30];
        let k = key(&list, 3, 11);
        cache.offer(k, Arc::clone(&list), surface(whole, 100), whole, PLACED);
        cache.commit();
        assert!(cache.lookup(&k, whole).is_some());
        for other in [
            SurfaceKey { values: 12, ..k },
            SurfaceKey { push: 4, ..k },
            SurfaceKey { size: (40, 31), ..k },
            SurfaceKey { dpi_bits: 2.0_f32.to_bits(), ..k },
            SurfaceKey { phase: (512, 0), ..k },
            SurfaceKey { settings: 8, ..k },
            SurfaceKey { list: k.list + 8, ..k },
        ] {
            assert!(cache.lookup(&other, whole).is_none(), "{other:?} is another picture");
        }
        // The frame painted with another value inside it: its surface replaces the kept one.
        let moved = SurfaceKey { values: 12, ..k };
        cache.offer(moved, Arc::clone(&list), surface(whole, 200), whole, PLACED);
        cache.commit();
        assert_eq!(cache.len(), 1, "one surface a frame");
        assert!(cache.lookup(&k, whole).is_none(), "the picture of the old value is gone");
        assert_eq!(cache.lookup(&moved, whole).map(|s| at(&s, 1, 1)), Some(200));
        // The list is held while its surface is kept, and let go of when it is not painted.
        assert_eq!(Arc::strong_count(&list), 2);
        let rebuilt = Arc::new(DisplayList::default());
        assert_eq!(cache.begin_frame(&[Arc::as_ptr(&rebuilt) as usize]), 1);
        assert_eq!((cache.len(), Arc::strong_count(&list)), (0, 1));
        assert!(cache.lookup(&moved, whole).is_none());
    }

    /// A1-08 (b), the bound: at most two surfaces and at most the byte bound; the one used
    /// longest ago makes room; a surface larger than the bound is not kept.
    #[test]
    fn the_kept_surfaces_are_bounded_in_number_and_in_bytes() {
        let whole = [0, 0, 40, 30];
        let list = Arc::new(DisplayList::default());
        let cache = RetainedFrames::with_max_bytes(1 << 20);
        let keys: Vec<SurfaceKey> = (0..3).map(|push| key(&list, push, 1)).collect();
        for k in &keys[..2] {
            cache.offer(*k, Arc::clone(&list), surface(whole, 9), whole, PLACED);
        }
        cache.commit();
        assert_eq!(cache.len(), 2);
        // The next frame (a use is stamped with its frame, A2-cpu-R07).
        assert_eq!(cache.begin_frame(&[Arc::as_ptr(&list) as usize]), 0);
        assert!(cache.lookup(&keys[0], whole).is_some(), "used now: the other one is the oldest");
        cache.offer(keys[2], Arc::clone(&list), surface(whole, 9), whole, PLACED);
        cache.commit();
        assert_eq!(cache.len(), MAX_ENTRIES);
        assert!(cache.lookup(&keys[1], whole).is_none(), "the one used longest ago made room");
        assert!(cache.lookup(&keys[0], whole).is_some() && cache.lookup(&keys[2], whole).is_some());
        // 4,800 bytes a surface: a bound of 6,000 holds one, a bound of 4,000 none.
        let one = RetainedFrames::with_max_bytes(6000);
        for k in &keys[..2] {
            one.offer(*k, Arc::clone(&list), surface(whole, 9), whole, PLACED);
        }
        one.commit();
        assert_eq!((one.len(), one.bytes()), (1, 4800));
        let none = RetainedFrames::with_max_bytes(4000);
        assert!(!none.can_keep(4800) && one.can_keep(4800) && !one.can_keep(0));
        none.offer(keys[0], Arc::clone(&list), surface(whole, 9), whole, PLACED);
        none.commit();
        assert!(none.is_empty(), "a surface larger than the bound is not kept");
        assert_eq!(parse_megabytes(Some("0")), Some(0), "AZ_CPU_RETAINED_MB=0 keeps nothing");
        assert_eq!(parse_megabytes(Some("64")), Some(64 << 20));
        assert_eq!(parse_megabytes(None), None);
    }

    /// A1-28 (A9): the bound at the user's display. A view of the whole 1470 x 956 pt Retina
    /// window with 256 px of overscan on each side is 3964 x 2936 px = 46.6 MB: it is kept,
    /// and a second one replaces it (93 MB together is not cheap). Two views of a 2560 x 1440
    /// window at a scale factor of 1 (3072 x 1952 px = 24.0 MB each) are both kept.
    /// Mutation: a bound of twice the frame's bytes (2 x 22.5 MB) keeps no view of the window.
    #[test]
    fn one_view_of_the_users_display_is_always_kept_and_a_second_only_where_it_is_cheap() {
        let list = Arc::new(DisplayList::default());
        let keyed = |push: usize, size: (u32, u32)| SurfaceKey { size, ..key(&list, push, 1) };
        let offer = |cache: &RetainedFrames, push: usize, size: (u32, u32)| {
            let surface = AzulPixmap::new_transparent(size.0, size.1).expect("a surface");
            assert!(cache.can_keep(surface.data().len()), "{size:?} is under the bound for one");
            cache.offer(keyed(push, size), Arc::clone(&list), surface, [0, 0, 1, 1], PLACED);
            cache.commit();
        };
        let retina = ((1470 + 512) * 2, (956 + 512) * 2);
        assert_eq!(retina, (3964, 2936));
        let cache = RetainedFrames::with_max_bytes(DEFAULT_MAX_BYTES);
        offer(&cache, 1, retina);
        assert_eq!((cache.len(), cache.bytes()), (1, 3964 * 2936 * 4));
        offer(&cache, 2, retina);
        assert_eq!(cache.len(), 1, "two would be 93 MB");
        assert!(cache.lookup(&keyed(2, retina), [0, 0, 1, 1]).is_some(), "the newer one is kept");
        let desktop = (2560 + 512, 1440 + 512);
        let cache = RetainedFrames::with_max_bytes(DEFAULT_MAX_BYTES);
        offer(&cache, 1, desktop);
        offer(&cache, 2, desktop);
        assert_eq!((cache.len(), cache.bytes()), (2, 2 * 3072 * 1952 * 4), "48 MB: both");
        assert!(cache.bytes() <= PAIR_MAX_BYTES);
        // 4K at a scale factor of 2: one fits; a surface over the bound for one does not.
        assert!(cache.can_keep((7680 + 1024) * (4320 + 1024) * 4));
        assert!(!cache.can_keep(DEFAULT_MAX_BYTES + 1));
    }

    /// A2-cpu-R07: which surface makes room does not depend on the order in which the bands
    /// of a frame looked up. Two surfaces are kept (frame 1); frame 2 uses both - once in the
    /// order A, B and once in the order B, A - and paints a third: in both orders the same one
    /// makes room, the first entry (A), because a use is stamped with its FRAME.
    /// Mutation: the clock advanced per lookup makes room with B when A was looked up last.
    #[test]
    fn which_surface_makes_room_does_not_depend_on_the_order_of_a_frames_lookups() {
        let whole = [0, 0, 40, 30];
        let list = Arc::new(DisplayList::default());
        let current = [Arc::as_ptr(&list) as usize];
        let keys: Vec<SurfaceKey> = (0..3).map(|push| key(&list, push, 1)).collect();
        let kept_after = |order: [usize; 2]| {
            let cache = RetainedFrames::with_max_bytes(1 << 20);
            cache.begin_frame(&current);
            for k in &keys[..2] {
                cache.offer(*k, Arc::clone(&list), surface(whole, 9), whole, PLACED);
            }
            cache.commit();
            cache.begin_frame(&current);
            for at in order {
                assert!(cache.lookup(&keys[at], whole).is_some());
            }
            cache.offer(keys[2], Arc::clone(&list), surface(whole, 9), whole, PLACED);
            cache.commit();
            [0, 1, 2].map(|at| cache.lookup(&keys[at], whole).is_some())
        };
        assert_eq!(kept_after([0, 1]), kept_after([1, 0]));
        assert_eq!(kept_after([1, 0]), [false, true, true], "the first entry makes room");
    }

    /// A1-26: the pictures of opacity groups are bounded apart from the frames' surfaces. A
    /// frame's surface is kept (4,800 bytes); ten opacity groups are offered after it: the
    /// frame's surface is still there, and of the groups the last [`MAX_FADE_ENTRIES`] (the
    /// first two made room). Under a byte bound of 10,000 for the whole cache two groups'
    /// pictures fit (9,600 bytes) and the third makes the first leave; a group's picture
    /// larger than the bound is not kept; `AZ_CPU_RETAINED_MB=0` keeps none.
    /// Mutation: one room loop over every entry lets the first group's picture make the
    /// frame's surface leave (a fading toast drops the picture of the map under it).
    #[test]
    fn the_pictures_of_opacity_groups_are_bounded_apart_from_the_frames_surfaces() {
        let whole = [0, 0, 40, 30];
        let list = Arc::new(DisplayList::default());
        let current = [Arc::as_ptr(&list) as usize];
        let cache = RetainedFrames::with_max_bytes(1 << 20);
        cache.begin_frame(&current);
        let frame = key(&list, 0, 1);
        cache.offer(frame, Arc::clone(&list), surface(whole, 9), whole, PLACED);
        cache.commit();
        let fades: Vec<SurfaceKey> = (1..=10).map(|push| key(&list, push, 1)).collect();
        for k in &fades {
            cache.begin_frame(&current);
            assert!(cache.can_keep_fade(4800));
            cache.offer_fade(*k, Arc::clone(&list), surface(whole, 5), whole, PLACED);
            cache.commit();
        }
        assert_eq!(cache.len(), 1 + MAX_FADE_ENTRIES);
        assert!(cache.lookup(&frame, whole).is_some(), "the frame's surface did not make room for a group");
        let kept: Vec<bool> = fades.iter().map(|k| cache.lookup(k, whole).is_some()).collect();
        assert_eq!(kept, [false, false, true, true, true, true, true, true, true, true]);
        // The same key offered again with another part: into the kept picture.
        cache.offer_fade(fades[9], Arc::clone(&list), surface(whole, 5), [0, 0, 1, 1], PLACED);
        cache.commit();
        assert_eq!(cache.len(), 1 + MAX_FADE_ENTRIES);
        // Bytes: 10,000 for everything holds two pictures of 4,800.
        let small = RetainedFrames::with_max_bytes(10_000);
        for k in &fades[..3] {
            small.offer_fade(*k, Arc::clone(&list), surface(whole, 5), whole, PLACED);
            small.commit();
        }
        assert_eq!((small.len(), small.bytes()), (2, 9600));
        assert!(small.lookup(&fades[0], whole).is_none() && small.lookup(&fades[2], whole).is_some());
        assert!(!small.can_keep_fade(10_001) && !small.can_keep_fade(0));
        assert!(!RetainedFrames::with_max_bytes(1 << 30).can_keep_fade(FADE_MAX_BYTES + 1));
        let none = RetainedFrames::with_max_bytes(0);
        assert!(!none.can_keep_fade(4800));
        none.offer_fade(fades[0], Arc::clone(&list), surface(whole, 5), whole, PLACED);
        none.commit();
        assert!(none.is_empty());
        // A frame's surface that is put back after a repair does not count the groups' pictures.
        let stale = cache.take_stale(&[]);
        assert_eq!(stale.len(), 1 + MAX_FADE_ENTRIES);
        for entry in stale {
            if !entry.fade {
                cache.put_back(entry);
            }
        }
        assert_eq!(cache.len(), 1);
    }
}
