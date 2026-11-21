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
//! 4. the walk's offset at the frame's push (the scroll frames and the view offset around it),
//!    bit for bit: the content is painted at `pos - offset`, and at another offset - also one
//!    that differs by whole device pixels - every position is another subtraction, which may
//!    round another way. A pan of a scaled view therefore paints its strip (a small part of
//!    the surface) and is kept as that; it is not composited from the picture of the offset
//!    before;
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
//! At most [`MAX_ENTRIES`] surfaces holding at most [`DEFAULT_MAX_BYTES`] together
//! (`AZ_CPU_RETAINED_MB`; `0` keeps nothing), counted by their full size although a page of a
//! surface costs memory only once it is painted. A surface that alone is larger than the bound
//! is not kept: its frame is painted on every step, as before. The surface of a 2560 x 1440
//! map view with its overscan is 23 MB at a scale factor of 1 and 92.5 MB at 2.
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

/// The most bytes the kept surfaces hold together unless `AZ_CPU_RETAINED_MB` says otherwise.
pub const DEFAULT_MAX_BYTES: usize = 96 * 1024 * 1024;

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
    /// The walk's offset at the frame's push (logical px), bit for bit.
    pub(crate) offset: (u32, u32),
    /// A hash of the live values bound inside the frame.
    pub(crate) values: u64,
    /// A hash of the text settings, the loaded fonts and the system colours.
    pub(crate) settings: u64,
}

/// One kept surface.
struct Entry {
    key: SurfaceKey,
    /// The list the picture was painted from: held, so that no patch can change it in place
    /// and no other list can take its address while the surface is kept.
    list: Arc<DisplayList>,
    surface: Arc<AzulPixmap>,
    /// The boxes of the surface that hold the picture, `[x0, y0, x1, y1]` in surface px.
    valid: Vec<[i32; 4]>,
    last_used: u64,
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
        inner.clock += 1;
        let clock = inner.clock;
        let entry = inner.entries.iter_mut().find(|entry| entry.key == *key)?;
        if !covers(&entry.valid, needed) {
            return None;
        }
        entry.last_used = clock;
        Some(Arc::clone(&entry.surface))
    }

    /// Whether a surface of `bytes` bytes can be kept at all.
    pub(crate) fn can_keep(&self, bytes: usize) -> bool {
        self.inner
            .lock()
            .is_ok_and(|inner| bytes > 0 && bytes <= inner.max_bytes)
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
    ) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.outbox.push(Painted { key, list, surface, part });
        }
    }

    /// The end of a frame: what its passes painted becomes what the next frame may composite.
    /// The parts of one surface painted by several bands are put together; a surface whose key
    /// changed (a value inside the frame, its size, the walk's offset) replaces the one kept
    /// for the same frame; the surfaces used longest ago make room.
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
            // Room: the surfaces used longest ago go.
            loop {
                let held: usize = inner.entries.iter().map(Entry::bytes).sum();
                if inner.entries.len() < MAX_ENTRIES && held + bytes <= inner.max_bytes {
                    break;
                }
                let Some(oldest) = inner
                    .entries
                    .iter()
                    .enumerate()
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
                last_used: clock,
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
            offset: (0, 0),
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
        cache.offer(k, Arc::clone(&list), surface([0, 0, 40, 16], 100), [0, 0, 40, 16]);
        cache.offer(k, Arc::clone(&list), surface([0, 14, 40, 30], 100), [0, 14, 40, 30]);
        assert!(cache.lookup(&k, [0, 0, 40, 30]).is_none(), "nothing is kept before the frame ends");
        cache.commit();
        assert_eq!((cache.len(), cache.bytes()), (1, 40 * 30 * 4));
        let kept = cache.lookup(&k, [0, 0, 40, 30]).expect("both parts together cover the surface");
        assert_eq!((at(&kept, 5, 2), at(&kept, 5, 15), at(&kept, 5, 29)), (100, 100, 100));
        drop(kept);
        // A smaller picture of another frame: only its part is served.
        let other = key(&list, 9, 11);
        cache.offer(other, Arc::clone(&list), surface([10, 10, 20, 20], 50), [10, 10, 20, 20]);
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
        cache.offer(k, Arc::clone(&list), surface(whole, 100), whole);
        cache.commit();
        assert!(cache.lookup(&k, whole).is_some());
        for other in [
            SurfaceKey { values: 12, ..k },
            SurfaceKey { push: 4, ..k },
            SurfaceKey { size: (40, 31), ..k },
            SurfaceKey { dpi_bits: 2.0_f32.to_bits(), ..k },
            SurfaceKey { offset: (12.0_f32.to_bits(), 0), ..k },
            SurfaceKey { settings: 8, ..k },
            SurfaceKey { list: k.list + 8, ..k },
        ] {
            assert!(cache.lookup(&other, whole).is_none(), "{other:?} is another picture");
        }
        // The frame painted with another value inside it: its surface replaces the kept one.
        let moved = SurfaceKey { values: 12, ..k };
        cache.offer(moved, Arc::clone(&list), surface(whole, 200), whole);
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
            cache.offer(*k, Arc::clone(&list), surface(whole, 9), whole);
        }
        cache.commit();
        assert_eq!(cache.len(), 2);
        assert!(cache.lookup(&keys[0], whole).is_some(), "used now: the other one is the oldest");
        cache.offer(keys[2], Arc::clone(&list), surface(whole, 9), whole);
        cache.commit();
        assert_eq!(cache.len(), MAX_ENTRIES);
        assert!(cache.lookup(&keys[1], whole).is_none(), "the one used longest ago made room");
        assert!(cache.lookup(&keys[0], whole).is_some() && cache.lookup(&keys[2], whole).is_some());
        // 4,800 bytes a surface: a bound of 6,000 holds one, a bound of 4,000 none.
        let one = RetainedFrames::with_max_bytes(6000);
        for k in &keys[..2] {
            one.offer(*k, Arc::clone(&list), surface(whole, 9), whole);
        }
        one.commit();
        assert_eq!((one.len(), one.bytes()), (1, 4800));
        let none = RetainedFrames::with_max_bytes(4000);
        assert!(!none.can_keep(4800) && one.can_keep(4800) && !one.can_keep(0));
        none.offer(keys[0], Arc::clone(&list), surface(whole, 9), whole);
        none.commit();
        assert!(none.is_empty(), "a surface larger than the bound is not kept");
        assert_eq!(parse_megabytes(Some("0")), Some(0), "AZ_CPU_RETAINED_MB=0 keeps nothing");
        assert_eq!(parse_megabytes(Some("64")), Some(64 << 20));
        assert_eq!(parse_megabytes(None), None);
    }
}
