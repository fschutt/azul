//! The picture of ONE reference frame of a window, shown by the shell as a layer of its own
//! (A10-06, the layer-backed present; `audit/A10.md`).
//!
//! # Why
//!
//! A map view pans and zooms by moving ONE frame: the picture's root. On a single window
//! buffer every such step is paid in pixels - a pan moves the view's clip in the frame and
//! presents all of it, a zoom step composites the kept picture through its matrix over the
//! whole view and presents that - although the picture itself did not change. A window
//! system that composites layers does the same step by moving or scaling a layer.
//!
//! # What the renderer does for it
//!
//! Nothing unless a backend asks ([`crate::cpu_backend::CpuBackend::promote_for_present`]).
//! When it does, each frame:
//!
//! 1. [`plan`] reads the lists as a walk without a damage rect would and finds the frame to
//!    promote ([`choose`]): the largest frame whose picture the walk can keep as a surface
//!    (`retained`), in no group, mask, rounded clip or text shadow, with nothing painted over
//!    it. For that frame it has the surface's key, how the surface lands on the frame
//!    ([`LayerMatrix`]), the clip it shows in, and the part of the surface that shows.
//! 2. The walk paints the promoted frame as an isolated group whatever its matrix
//!    (`raster::PromoteFrame`), keeps its surface, and does NOT composite it: the window's
//!    frame holds, under the layer, what is painted before the promoted frame.
//! 3. A change of the frame's own matrix, and a pan of the view that shows nothing but the
//!    frame, are no damage of the window's frame: they are the layer's new [`LayerMatrix`].
//!    What the layer shows of the surface that is not painted yet IS damage
//!    ([`damage_for_missing`]): the walk paints those parts of the surface.
//! 4. The backend reports the layer ([`PresentLayer`]); the shell shows the surface under
//!    the matrix and the clip, over the window's frame.
//!
//! # The invariant
//!
//! The window's frame with the layer's surface composited through [`PresentLayer::matrix`]
//! inside [`PresentLayer::clip`] (`blit_pixmap_affine_clipped`, what the walk does for every
//! group) is the frame the walk paints when it composites the promoted frame itself
//! (`PromoteFrame::composite`): every pixel. Tested below across a pan, a zoom step, a pan
//! during a zoom, a tile arriving and an edit.

use azul_core::geom::{LogicalPosition, LogicalRect, LogicalSize};
use azul_css::props::{basic::FontRef, style::box_shadow::StyleBoxShadow};

use super::{
    compositor::{is_identity_2d, mat3_is_affine, translation_2d, Mat3},
    pixmap::{intersect_clips, logical_rect_to_az_rect, AzRect},
    raster::{layer_candidate, text_shadow_reach, CpuRenderState, GroupGeometry, LayerCandidate},
    retained::{RetainedFrames, SurfaceKey},
};
use crate::{
    glyph_cache::GlyphCache,
    solver3::display_list::{DisplayList, DisplayListItem},
    text3::cache::FontManager,
};

/// How a layer's surface lands on the window's frame: `x' = xx x + xy y + tx` and
/// `y' = yx x + yy y + ty`, surface px in, device px of the frame out (both with the origin
/// at the top left and y down).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayerMatrix {
    pub xx: f64,
    pub xy: f64,
    pub yx: f64,
    pub yy: f64,
    pub tx: f64,
    pub ty: f64,
}

impl LayerMatrix {
    /// The affine part of a group's matrix (`raster::GroupGeometry::matrix`, row-major);
    /// `None` when it has a perspective part.
    #[must_use]
    pub fn of(m: &Mat3) -> Option<Self> {
        mat3_is_affine(m).then(|| Self {
            xx: m[0],
            xy: m[1],
            tx: m[2],
            yx: m[3],
            yy: m[4],
            ty: m[5],
        })
    }

    /// A scale (positive, per axis) and a move, nothing else: a surface that lands as an
    /// upright rectangle, which is all the first version of the layer present shows.
    #[must_use]
    pub fn is_upright(&self) -> bool {
        let finite = [self.xx, self.xy, self.yx, self.yy, self.tx, self.ty]
            .iter()
            .all(|v| v.is_finite());
        finite && self.xy.abs() < 1e-9 && self.yx.abs() < 1e-9 && self.xx > 1e-6 && self.yy > 1e-6
    }

    /// Where the surface point `(x, y)` lands.
    #[must_use]
    pub fn map(&self, x: f64, y: f64) -> (f64, f64) {
        (
            self.xx * x + self.xy * y + self.tx,
            self.yx * x + self.yy * y + self.ty,
        )
    }

    /// The box of the frame (device px, `(x0, y0, x1, y1)`, not rounded) that the box
    /// `[x0, y0, x1, y1]` of the surface lands in: the bounding box of its four corners.
    #[must_use]
    pub fn landing_box(&self, b: [i32; 4]) -> (f64, f64, f64, f64) {
        let (x0, y0, x1, y1) = (f64::from(b[0]), f64::from(b[1]), f64::from(b[2]), f64::from(b[3]));
        let corners = [self.map(x0, y0), self.map(x1, y0), self.map(x1, y1), self.map(x0, y1)];
        let mut out = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
        for (x, y) in corners {
            out = (out.0.min(x), out.1.min(y), out.2.max(x), out.3.max(y));
        }
        out
    }

    /// The box of the surface (surface px, outward to whole pixels, cut to a surface of
    /// `size`) that lands in the box `clip` of the frame. For an upright matrix only.
    ///
    /// Example: scale 1.5, move (-300, -200), clip (100, 76) .. (2940, 1876): x from
    /// `(100 + 300) / 1.5 = 266.67` to `(2940 + 300) / 1.5 = 2160`, y from `184` to `1384`:
    /// `[266, 184, 2160, 1384]`.
    #[must_use]
    #[allow(clippy::cast_possible_truncation)] // clamped to the surface before the cast
    pub fn shown_box(&self, clip: (i32, i32, i32, i32), size: (u32, u32)) -> [i32; 4] {
        let back_x = |v: i32| (f64::from(v) - self.tx) / self.xx;
        let back_y = |v: i32| (f64::from(v) - self.ty) / self.yy;
        let cut = |v: f64, hi: u32| v.clamp(0.0, f64::from(hi)) as i32;
        [
            cut(back_x(clip.0).floor(), size.0),
            cut(back_y(clip.1).floor(), size.1),
            cut(back_x(clip.2).ceil(), size.0),
            cut(back_y(clip.3).ceil(), size.1),
        ]
    }
}

/// The layer a frame reports to its shell: the kept surface of the promoted frame, where it
/// lands and what clips it. Everything is in device pixels of the window's frame (origin top
/// left, y down); the surface is RGBA8, premultiplied, top row first, tight rows, like the
/// frame.
#[derive(Debug, Clone, PartialEq)]
pub struct PresentLayer {
    /// Which picture the surface holds: another value for another list, frame, size, phase
    /// or set of values inside the frame (a hash of the surface's key).
    pub surface_id: u64,
    /// The surface's size.
    pub surface_size: (u32, u32),
    /// Surface px -> device px of the frame. Upright ([`LayerMatrix::is_upright`]).
    pub matrix: LayerMatrix,
    /// What the layer shows in, `(x0, y0, x1, y1)`, half-open.
    pub clip: (i32, i32, i32, i32),
    /// The stamp of the surface's pixels: another value whenever a pixel of it changed (a
    /// part painted, a repair), also under the same `surface_id`. A shell that holds a copy
    /// of the pixels copies them again when either differs from what it copied.
    pub generation: u64,
}

/// What [`plan`] found: the frame to promote this frame and what the backend needs to paint
/// with it.
#[derive(Debug, Clone, PartialEq)]
pub struct LayerPlan {
    /// `TransformKey::id` of the frame's push (`raster::PromoteFrame::transform_key`).
    pub(crate) transform_key: usize,
    /// The key of the frame's surface.
    pub(crate) key: SurfaceKey,
    /// Surface px: what shows under the clip with the taps of a composite (2 px): what is
    /// asked to be painted.
    pub(crate) needed: [i32; 4],
    /// Surface px: what shows under the clip ([`LayerMatrix::shown_box`]): what must be
    /// painted for the layer to be shown at all.
    pub(crate) shown: [i32; 4],
    pub(crate) matrix: LayerMatrix,
    /// Device px of the frame, half-open, inside the frame.
    pub(crate) clip: (i32, i32, i32, i32),
    /// The index, in the WINDOW's list, of the view the frame is shown through (directly, or
    /// through views inside it); `None` for a frame of the window's own list.
    pub(crate) view: Option<usize>,
    /// The frame is directly in that view's list and the list paints nothing else: a pan of
    /// the view (its content offset) moves no pixel of the window's frame. Otherwise a pan
    /// of the view repaints the view's box in the window's frame.
    pub(crate) alone: bool,
}

impl LayerPlan {
    /// The clip as a rect of the frame in logical px.
    pub(crate) fn clip_rect(&self, dpi_factor: f32) -> LogicalRect {
        device_box_to_logical(self.clip, dpi_factor)
    }

    /// What a frame that shows this plan's surface reports to the shell.
    pub(crate) fn layer(&self, generation: u64) -> PresentLayer {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.key.hash(&mut hasher);
        PresentLayer {
            surface_id: hasher.finish(),
            surface_size: self.key.size,
            matrix: self.matrix,
            clip: self.clip,
            generation,
        }
    }
}

/// A box of the frame in device px as a rect in logical px (what the damage lists hold).
pub(crate) fn device_box_to_logical(b: (i32, i32, i32, i32), dpi_factor: f32) -> LogicalRect {
    let dpi = if dpi_factor > 0.0 { dpi_factor } else { 1.0 };
    LogicalRect {
        origin: LogicalPosition {
            x: b.0 as f32 / dpi,
            y: b.1 as f32 / dpi,
        },
        size: LogicalSize {
            width: (b.2 - b.0).max(0) as f32 / dpi,
            height: (b.3 - b.1).max(0) as f32 / dpi,
        },
    }
}

/// How far around a part of the surface that is not painted the damage reaches, surface px:
/// the taps of a composite lie up to 2 px outside what shows, and a part that lies in that
/// rim alone lands outside the clip, where no pass can paint. Widened by this, it reaches
/// under the clip, and a pass paints its 64 px around what it lands on.
const MISSING_REACH_PX: i32 = 3;

/// The most parts a frame repaints one by one; more are repainted as their bounding box.
const MAX_MISSING_RECTS: usize = 8;

/// The rects of the window's frame (logical px) a walk has to repaint so that the parts
/// `missing` of a layer's surface (surface px, `RetainedFrames::missing`) are painted: each
/// part, widened by [`MISSING_REACH_PX`], where it lands, outward to whole device pixels,
/// inside the clip. A pass of the walk over such a rect finds the promoted frame, misses its
/// kept surface there, paints the part of the surface that lands in the rect (and 64 px
/// around it) and keeps it.
///
/// Example (the pan of the audit, scale factor 2): surface 3964 x 2936 px, landing at
/// (-484, -436) after three steps of 24 px, clip (100, 76) .. (2940, 1876). Painted: x up to
/// 3416; shown with its taps: x up to 3426. Missing `[3416, 510, 3426, 2314]`, widened
/// `[3413, 507, 3429, 2317]`, lands at x 2929 .. 2945, y 71 .. 1881, inside the clip
/// (2929, 76) .. (2940, 1876): 11 x 1800 device px = (1464.5, 38) 5.5 x 900 logical px.
/// The clip is 2840 x 1800 px: 258 times as much.
#[must_use]
#[allow(clippy::cast_possible_truncation)] // boxes inside the clip
pub(crate) fn damage_for_missing(
    missing: &[[i32; 4]],
    matrix: &LayerMatrix,
    clip: (i32, i32, i32, i32),
    dpi_factor: f32,
) -> Vec<LogicalRect> {
    let land = |b: &[i32; 4]| -> Option<(i32, i32, i32, i32)> {
        let widened = [
            b[0] - MISSING_REACH_PX,
            b[1] - MISSING_REACH_PX,
            b[2] + MISSING_REACH_PX,
            b[3] + MISSING_REACH_PX,
        ];
        let (x0, y0, x1, y1) = matrix.landing_box(widened);
        if ![x0, y0, x1, y1].iter().all(|v| v.is_finite()) {
            return Some(clip);
        }
        let cut = |v: f64, lo: i32, hi: i32| (v.clamp(-1.0e9, 1.0e9) as i32).clamp(lo, hi);
        let landed = (
            cut(x0.floor(), clip.0, clip.2),
            cut(y0.floor(), clip.1, clip.3),
            cut(x1.ceil(), clip.0, clip.2),
            cut(y1.ceil(), clip.1, clip.3),
        );
        (landed.2 > landed.0 && landed.3 > landed.1).then_some(landed)
    };
    let mut boxes: Vec<(i32, i32, i32, i32)> = missing.iter().filter_map(land).collect();
    if boxes.len() > MAX_MISSING_RECTS {
        let all = boxes.iter().fold(boxes[0], |a, b| {
            (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3))
        });
        boxes = vec![all];
    }
    boxes.into_iter().map(|b| device_box_to_logical(b, dpi_factor)).collect()
}

/// THE promotion rule, over the frames a scan found that could be layers: `(surface px,
/// painted over)` each, in list order. The largest surface is the one promoted (the first of
/// equals), and only if nothing is painted over it inside its clip: the first version has no
/// layer above the promoted one, so what is painted over the frame would be hidden by it.
/// `None`: nothing is promoted, the frame is presented as without the switch.
#[must_use]
pub(crate) fn choose(found: &[(u64, bool)]) -> Option<usize> {
    let mut best: Option<usize> = None;
    for (at, (px, _)) in found.iter().enumerate() {
        if best.is_none_or(|b| *px > found[b].0) {
            best = Some(at);
        }
    }
    best.filter(|b| !found[*b].1)
}

/// Whether two half-open boxes share a pixel.
fn boxes_meet(a: (i32, i32, i32, i32), b: (i32, i32, i32, i32)) -> bool {
    a.0.max(b.0) < a.2.min(b.2) && a.1.max(b.1) < a.3.min(b.3)
}

/// An item that has bounds and paints nothing (`compositor::paints_nothing`, the same three).
fn paints_nothing(item: &DisplayListItem) -> bool {
    matches!(
        item,
        DisplayListItem::HitTestArea { .. }
            | DisplayListItem::TextLayout { .. }
            | DisplayListItem::WebView { .. }
    )
}

/// How deep views nest before the scan stops following them (as `compositor::MAX_VIEW_DEPTH`).
const MAX_VIEW_DEPTH: usize = 8;

/// A frame the scan found that could be a layer.
struct Found {
    candidate: LayerCandidate,
    /// Its clip inside the frame, device px.
    clip: (i32, i32, i32, i32),
    /// Something is painted over it inside its clip, after it in painter's order.
    overlaid: bool,
    /// See [`LayerPlan::view`] and [`LayerPlan::alone`].
    view: Option<usize>,
    alone: bool,
}

/// The scan of [`plan`]: the stacks a walk without a damage rect holds at each item, and no
/// pixel.
struct Scan<'a> {
    state: &'a CpuRenderState,
    retained: &'a RetainedFrames,
    glyph_cache: &'a GlyphCache,
    font_manager: &'a FontManager<FontRef>,
    dpi_factor: f32,
    frame_px: (u32, u32),
    found: Vec<Found>,
    /// A list the scan cannot follow (a frame that is never closed): nothing is promoted.
    gave_up: bool,
}

impl Scan<'_> {
    /// The whole frame as a box.
    fn frame_box(&self) -> (i32, i32, i32, i32) {
        (
            0,
            0,
            i32::try_from(self.frame_px.0).unwrap_or(i32::MAX),
            i32::try_from(self.frame_px.1).unwrap_or(i32::MAX),
        )
    }

    /// The pixels a paint under `clip` may write, inside the frame.
    fn clip_box(&self, clip: Option<AzRect>) -> (i32, i32, i32, i32) {
        let frame = self.frame_box();
        match clip.map(|c| c.pixel_box()) {
            None => frame,
            Some(c) => (
                c.0.clamp(frame.0, frame.2),
                c.1.clamp(frame.1, frame.3),
                c.2.clamp(frame.0, frame.2),
                c.3.clamp(frame.1, frame.3),
            ),
        }
    }

    /// Something paints the box `painted` of the frame now: every frame found before it that
    /// shows there has it painted over.
    fn note_painted(&mut self, painted: (i32, i32, i32, i32)) {
        for found in &mut self.found {
            if !found.overlaid && boxes_meet(found.clip, painted) {
                found.overlaid = true;
            }
        }
    }

    /// The box of the frame that `r` (list space, moved up and left by `offset`, widened by
    /// `reach` logical px) covers, outward, under `clip`.
    #[allow(clippy::cast_possible_truncation)] // clamped to the clip's box
    fn painted_box(
        &self,
        r: &LogicalRect,
        offset: (f32, f32),
        reach: f32,
        clip: Option<AzRect>,
    ) -> (i32, i32, i32, i32) {
        let c = self.clip_box(clip);
        let d = self.dpi_factor;
        let cut = |v: f32, lo: i32, hi: i32| {
            if v.is_finite() {
                (v.clamp(-1.0e9, 1.0e9) as i32).clamp(lo, hi)
            } else if v > 0.0 {
                hi
            } else {
                lo
            }
        };
        (
            cut(((r.origin.x - offset.0 - reach) * d).floor(), c.0, c.2),
            cut(((r.origin.y - offset.1 - reach) * d).floor(), c.1, c.3),
            cut(((r.origin.x + r.size.width - offset.0 + reach) * d).ceil(), c.0, c.2),
            cut(((r.origin.y + r.size.height - offset.1 + reach) * d).ceil(), c.1, c.3),
        )
    }

    /// Walks `list` as `raster::walk_range` does without a damage rect, keeping only the
    /// clip and the offset: `clip` and `offset` are the walk's at the list's first item,
    /// `grouped` whether a group, a mask, a rounded clip or a text shadow is open around the
    /// list, `through` the view of the window's list the list is shown through (`None` for
    /// the window's own list), `depth` how many views deep the list is (0 for the window's).
    #[allow(clippy::too_many_lines)]
    fn scan(
        &mut self,
        list: &DisplayList,
        clip: Option<AzRect>,
        offset: (f32, f32),
        grouped: bool,
        through: Option<usize>,
        depth: usize,
    ) {
        let index = list.frame_index();
        // The frame's render state, by its own lifetime (not through `self`).
        let state = self.state;
        // The clips open here (with: is it rounded?), innermost last; never empty.
        let mut clips: Vec<(Option<AzRect>, bool)> = vec![(clip, false)];
        // The walk's offsets: scroll frames, moved frames; never empty.
        let mut offsets: Vec<(f32, f32)> = vec![offset];
        // The frames open here that the scan walks into: did the frame push an offset?
        let mut frames: Vec<bool> = Vec::new();
        // Open opacity groups, filter groups, backdrop filters and image masks.
        let mut groups = 0usize;
        let mut rounded = 0usize;
        let mut shadows: Vec<StyleBoxShadow> = Vec::new();
        // Whether this list painted anything yet (see `LayerPlan::alone`).
        let mut list_painted = false;
        let moved = |r: &LogicalRect, by: (f32, f32)| LogicalRect {
            origin: LogicalPosition {
                x: r.origin.x - by.0,
                y: r.origin.y - by.1,
            },
            size: r.size,
        };
        let mut next = 0usize;
        while next < list.items.len() {
            if self.gave_up {
                return;
            }
            let at = next;
            next += 1;
            let item = &list.items[at];
            let now_clip = clips.last().and_then(|c| c.0);
            let now = offsets.last().copied().unwrap_or((0.0, 0.0));
            match item {
                // The `PushClip` arm of `render_single_item`, on the clips without a damage
                // rect (its `real_clip_stack`).
                DisplayListItem::PushClip { bounds, border_radius } => {
                    let new_clip = Some(
                        logical_rect_to_az_rect(&moved(bounds.inner(), now), self.dpi_factor)
                            .unwrap_or(AzRect::DENY_ALL),
                    );
                    let is_rounded = !border_radius.is_zero();
                    if is_rounded {
                        rounded += 1;
                    }
                    clips.push((intersect_clips(now_clip, new_clip), is_rounded));
                }
                DisplayListItem::PopClip => {
                    if clips.len() > 1 {
                        if let Some((_, true)) = clips.pop() {
                            rounded = rounded.saturating_sub(1);
                        }
                    }
                }
                DisplayListItem::PushScrollFrame { scroll_id, .. } => {
                    let by = state.scroll_offsets.get(scroll_id).copied().unwrap_or((0.0, 0.0));
                    offsets.push((now.0 + by.0, now.1 + by.1));
                }
                DisplayListItem::PopScrollFrame => {
                    if offsets.len() > 1 {
                        offsets.pop();
                    }
                }
                DisplayListItem::PushReferenceFrame {
                    transform_key,
                    initial_transform,
                    bounds,
                } => {
                    // The frame as the walk takes it: with the frames inside it where their
                    // live matrices put them.
                    let Some(mut frame) = index.frame_at(at) else {
                        self.gave_up = true;
                        return;
                    };
                    if !frame.self_contained {
                        self.gave_up = true;
                        return;
                    }
                    if !frame.leaf {
                        if let Some(extent) = index.extent_with(&list.items, at, &state.transforms) {
                            frame.extent = extent;
                        }
                    }
                    let m = state
                        .transforms
                        .get(&transform_key.id)
                        .map_or(&initial_transform.m, |t| &t.m);
                    let in_group = grouped || groups > 0 || rounded > 0 || !shadows.is_empty();
                    // A frame of text alone may be drawn through its matrix without a surface
                    // (`paint_turned_text_frame`): never a layer.
                    let candidate = if in_group || frame.text_only {
                        None
                    } else {
                        layer_candidate(
                            list,
                            item,
                            &frame,
                            index.inner_of(at),
                            self.frame_px,
                            self.dpi_factor,
                            now_clip,
                            now,
                            self.glyph_cache,
                            self.font_manager,
                            state,
                            self.retained,
                        )
                    };
                    if let Some(candidate) = candidate {
                        let own_clip = match candidate.clip {
                            None => self.frame_box(),
                            Some(c) => {
                                let f = self.frame_box();
                                (c.0.clamp(f.0, f.2), c.1.clamp(f.1, f.3), c.2.clamp(f.0, f.2), c.3.clamp(f.1, f.3))
                            }
                        };
                        // It paints, too: over every frame found before it that it lands on.
                        self.note_painted(own_clip);
                        self.found.push(Found {
                            candidate,
                            clip: own_clip,
                            overlaid: false,
                            view: through,
                            alone: depth == 1 && !list_painted,
                        });
                        list_painted = true;
                        next = frame.end + 1;
                        continue;
                    }
                    if is_identity_2d(m) {
                        frames.push(false);
                    } else if let Some((tx, ty)) = translation_2d(m) {
                        offsets.push((now.0 - tx, now.1 - ty));
                        frames.push(true);
                    } else {
                        // Any other matrix: a group of the walk. Where it can land is painted.
                        let g = GroupGeometry::of(
                            m,
                            &moved(bounds.inner(), now),
                            &moved(&frame.extent, now),
                            self.dpi_factor,
                            now_clip,
                        );
                        let under = self.clip_box(now_clip);
                        let lands = match LayerMatrix::of(&g.matrix) {
                            None => under,
                            Some(matrix) => {
                                let w = i32::try_from(g.width).unwrap_or(i32::MAX);
                                let h = i32::try_from(g.height).unwrap_or(i32::MAX);
                                let (x0, y0, x1, y1) = matrix.landing_box([0, 0, w, h]);
                                if [x0, y0, x1, y1].iter().all(|v| v.is_finite()) {
                                    #[allow(clippy::cast_possible_truncation)] // clamped
                                    let cut = |v: f64, lo: i32, hi: i32| (v.clamp(-1.0e9, 1.0e9) as i32).clamp(lo, hi);
                                    (
                                        cut(x0.floor(), under.0, under.2),
                                        cut(y0.floor(), under.1, under.3),
                                        cut(x1.ceil(), under.0, under.2),
                                        cut(y1.ceil(), under.1, under.3),
                                    )
                                } else {
                                    under
                                }
                            }
                        };
                        self.note_painted(lands);
                        list_painted = true;
                        next = frame.end + 1;
                    }
                }
                DisplayListItem::PopReferenceFrame => {
                    if frames.pop() == Some(true) && offsets.len() > 1 {
                        offsets.pop();
                    }
                }
                DisplayListItem::PushOpacity { .. }
                | DisplayListItem::PushFilter { .. }
                | DisplayListItem::PushBackdropFilter { .. }
                | DisplayListItem::PushImageMaskClip { .. } => groups += 1,
                DisplayListItem::PopOpacity
                | DisplayListItem::PopFilter
                | DisplayListItem::PopBackdropFilter
                | DisplayListItem::PopImageMaskClip => groups = groups.saturating_sub(1),
                DisplayListItem::PushTextShadow { shadow } => shadows.push(*shadow),
                DisplayListItem::PopTextShadow => {
                    shadows.pop();
                }
                DisplayListItem::PushStackingContext { .. } | DisplayListItem::PopStackingContext => {}
                // The `VirtualView` arm of `render_single_item`: the child's list under the
                // view's box, at the view's origin and content offset.
                DisplayListItem::VirtualView {
                    child_dom_id,
                    bounds,
                    content_offset,
                    ..
                } => {
                    let Some(child) = state.virtual_view_display_lists.get(child_dom_id) else {
                        continue;
                    };
                    let view_box = moved(bounds.inner(), now);
                    let view_clip =
                        intersect_clips(now_clip, logical_rect_to_az_rect(&view_box, self.dpi_factor));
                    if depth + 1 >= MAX_VIEW_DEPTH {
                        // Not followed: all of its box is painted.
                        let all = self.clip_box(view_clip);
                        self.note_painted(all);
                        list_painted = true;
                        continue;
                    }
                    let origin = bounds.inner().origin;
                    let child_offset = (
                        now.0 - origin.x - content_offset.x,
                        now.1 - origin.y - content_offset.y,
                    );
                    let in_group = grouped || groups > 0 || rounded > 0 || !shadows.is_empty();
                    let before = self.found.len();
                    let child = std::sync::Arc::clone(child);
                    self.scan(
                        &child,
                        view_clip,
                        child_offset,
                        in_group,
                        if depth == 0 { Some(at) } else { through },
                        depth + 1,
                    );
                    // What the view shows besides a frame found in it is this list's paint;
                    // a view that shows such a frame alone leaves this list as it was.
                    if self.found.len() == before || self.found[before..].iter().any(|f| !f.alone) {
                        list_painted = true;
                    }
                }
                _ => {
                    if item.is_state_management() || paints_nothing(item) {
                        continue;
                    }
                    list_painted = true;
                    if self.found.iter().all(|f| f.overlaid) {
                        continue;
                    }
                    // As the walk culls: the ink, or under a text shadow the box widened by
                    // the shadows' reach. An item without bounds can paint anywhere.
                    let bounds = if shadows.is_empty() {
                        item.visual_bounds().or_else(|| item.bounds())
                    } else {
                        item.bounds()
                    };
                    let painted = match bounds {
                        Some(b) => self.painted_box(&b, now, text_shadow_reach(&shadows), now_clip),
                        None => self.clip_box(now_clip),
                    };
                    self.note_painted(painted);
                }
            }
        }
    }
}

/// The frame of a window to show as a layer this frame, or `None`.
///
/// `state` is the render state the frame is painted with (its offsets, its live values, the
/// window's list as `root_display_list`, the views' lists), `frame_px` the frame's size in
/// device px. The lists are read as a walk without a damage rect reads them (`Scan::scan`),
/// so the clip found for a frame is the clip all the passes of a repaint composite it under
/// together, and its key is the key each of them looks its surface up with.
///
/// Promoted is the frame [`choose`] names, if its surface lands upright
/// ([`LayerMatrix::is_upright`]) and shows somewhere.
#[must_use]
pub(crate) fn plan(
    state: &CpuRenderState,
    frame_px: (u32, u32),
    dpi_factor: f32,
    glyph_cache: &GlyphCache,
    font_manager: &FontManager<FontRef>,
) -> Option<LayerPlan> {
    let root = state.root_display_list.as_ref()?;
    let retained = state.retained.as_deref()?;
    if !(dpi_factor > 0.0) || frame_px.0 == 0 || frame_px.1 == 0 {
        return None;
    }
    let mut scan = Scan {
        state,
        retained,
        glyph_cache,
        font_manager,
        dpi_factor,
        frame_px,
        found: Vec::new(),
        gave_up: false,
    };
    scan.scan(root, None, (0.0, 0.0), false, None, 0);
    if scan.gave_up {
        return None;
    }
    let sizes: Vec<(u64, bool)> = scan
        .found
        .iter()
        .map(|f| {
            let (w, h) = f.candidate.key.size;
            (u64::from(w) * u64::from(h), f.overlaid)
        })
        .collect();
    let found = &scan.found[choose(&sizes)?];
    let matrix = LayerMatrix::of(&found.candidate.matrix).filter(LayerMatrix::is_upright)?;
    if found.clip.2 <= found.clip.0 || found.clip.3 <= found.clip.1 {
        return None;
    }
    Some(LayerPlan {
        transform_key: found.candidate.transform_key,
        key: found.candidate.key,
        needed: found.candidate.needed,
        shown: matrix.shown_box(found.clip, found.candidate.key.size),
        matrix,
        clip: found.clip,
        view: found.view,
        alone: found.alone,
    })
}

/// What a frame that promotes `plan` adds to its damage besides what the lists' diffs found,
/// and whether it may be shown as a layer at all:
///
/// - `covered`: nothing of the surface that shows is missing. The layer shows the kept
///   surface as it is.
/// - otherwise the rects ([`damage_for_missing`]) whose passes paint the missing parts.
///
/// `missing` is `RetainedFrames::missing(key, needed)`: `None` when no surface is kept for
/// the key (all of what shows is missing).
#[must_use]
pub(crate) fn damage_of_plan(
    plan: &LayerPlan,
    missing: Option<Vec<[i32; 4]>>,
    dpi_factor: f32,
) -> Vec<LogicalRect> {
    let missing = missing.unwrap_or_else(|| vec![plan.needed]);
    damage_for_missing(&missing, &plan.matrix, plan.clip, dpi_factor)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use azul_css::props::basic::ColorU;

    use super::*;
    use crate::{
        cpurender::{
            blit_pixmap_affine_clipped, mat3_affine_part, refresh_retained_surfaces,
            render_display_list_damaged, AzulPixmap, PromoteFrame, ScrollOffsetMap,
        },
        solver3::display_list::{BorderRadius, WindowLogicalRect},
    };

    fn rect(x: f32, y: f32, w: f32, h: f32) -> LogicalRect {
        LogicalRect {
            origin: LogicalPosition { x, y },
            size: LogicalSize { width: w, height: h },
        }
    }

    fn wrect(x: f32, y: f32, w: f32, h: f32) -> WindowLogicalRect {
        rect(x, y, w, h).into()
    }

    fn fill(x: f32, y: f32, w: f32, h: f32, rgb: [u8; 3]) -> DisplayListItem {
        DisplayListItem::Rect {
            bounds: wrect(x, y, w, h),
            color: ColorU { r: rgb[0], g: rgb[1], b: rgb[2], a: 255 },
            border_radius: BorderRadius::default(),
        }
    }

    fn list(items: Vec<DisplayListItem>) -> Arc<DisplayList> {
        Arc::new(DisplayList { items, ..Default::default() })
    }

    /// A row-vector 4x4 that scales by `k` and then moves by `(tx, ty)` logical px.
    fn scale_then_move(k: f32, tx: f32, ty: f32) -> [[f32; 4]; 4] {
        [
            [k, 0.0, 0.0, 0.0],
            [0.0, k, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [tx, ty, 0.0, 1.0],
        ]
    }

    const VIEW: usize = 1;
    const PICTURE: usize = 7;

    /// The window of the tests, 1000 x 700 px: a sidebar (x 0 .. 100), a ground beside it, and
    /// a view at (100, 50) 800 x 600 whose child is [`picture`]; `over` paints a panel over
    /// the view (after it in the window's list). The view is the third item of the list.
    fn host(content_offset: (f32, f32), over: bool) -> Arc<DisplayList> {
        let mut items = vec![
            fill(0.0, 0.0, 100.0, 700.0, [40, 44, 52]),
            fill(100.0, 0.0, 900.0, 700.0, [250, 250, 250]),
            DisplayListItem::VirtualView {
                child_dom_id: azul_core::dom::DomId { inner: VIEW },
                bounds: wrect(100.0, 50.0, 800.0, 600.0),
                clip_rect: wrect(100.0, 50.0, 800.0, 600.0),
                content_offset: LogicalPosition { x: content_offset.0, y: content_offset.1 },
            },
        ];
        if over {
            items.push(fill(700.0, 300.0, 250.0, 100.0, [200, 30, 30]));
        }
        list(items)
    }

    /// The view's list: a picture of 1120 x 920 px, 160 px larger than the view on each side
    /// (the view's content offset is (-160, -160) at rest): its root frame (key 7) around a
    /// ground, a grid of 28 x 23 cells and a "label" frame moved by its own matrix (key 9).
    /// `edited` paints the cell at (404, 364) in another colour: another list, the same
    /// items but one (a tile that arrived, a label that was edited).
    fn picture(edited: bool) -> Arc<DisplayList> {
        let mut items = vec![DisplayListItem::PushReferenceFrame {
            transform_key: azul_core::resources::TransformKey { id: PICTURE },
            initial_transform: azul_core::transform::ComputedTransform3D { m: scale_then_move(1.0, 0.0, 0.0) },
            bounds: wrect(0.0, 0.0, 1120.0, 920.0),
        }];
        items.push(fill(0.0, 0.0, 1120.0, 920.0, [236, 233, 225]));
        for i in 0..28u8 {
            for j in 0..23u8 {
                let rgb = if edited && i == 10 && j == 9 { [255, 0, 255] } else { [i * 9, j * 11, 60 + i * 4 + j * 3] };
                items.push(fill(f32::from(i) * 40.0 + 4.0, f32::from(j) * 40.0 + 4.0, 32.0, 32.0, rgb));
            }
        }
        items.push(DisplayListItem::PushReferenceFrame {
            transform_key: azul_core::resources::TransformKey { id: 9 },
            initial_transform: azul_core::transform::ComputedTransform3D { m: scale_then_move(1.0, 3.0, 0.0) },
            bounds: wrect(500.0, 450.0, 60.0, 14.0),
        });
        items.push(fill(500.0, 450.0, 60.0, 14.0, [20, 20, 20]));
        items.push(DisplayListItem::PopReferenceFrame);
        items.push(DisplayListItem::PopReferenceFrame);
        list(items)
    }

    struct Scene {
        host: Arc<DisplayList>,
        child: Arc<DisplayList>,
    }

    const REST: (f32, f32) = (-160.0, -160.0);
    const FRAME: (u32, u32) = (1000, 700);
    const WHOLE: (f32, f32, f32, f32) = (0.0, 0.0, 1000.0, 700.0);
    const VIEW_BOX: (f32, f32, f32, f32) = (100.0, 50.0, 800.0, 600.0);

    /// The render state of a frame of `scene` with the picture's root scaled by `k` and moved
    /// by `t`, the kept surfaces in `retained`, and `promote`.
    fn state(
        scene: &Scene,
        (k, t): (f32, (f32, f32)),
        retained: &Arc<RetainedFrames>,
        promote: Option<PromoteFrame>,
        threads: usize,
    ) -> CpuRenderState {
        let mut state = CpuRenderState::new(ScrollOffsetMap::new())
            .with_virtual_view_display_lists(
                [(azul_core::dom::DomId { inner: VIEW }, Arc::clone(&scene.child))].into_iter().collect(),
            )
            .with_root_display_list(Arc::clone(&scene.host))
            .with_retained_frames(Arc::clone(retained))
            .with_paint_threads(threads);
        state.transforms.insert(
            PICTURE,
            azul_core::transform::ComputedTransform3D { m: scale_then_move(k, t.0, t.1) },
        );
        state.promote = promote;
        state
    }

    struct Deps {
        rr: azul_core::resources::RendererResources,
        fm: FontManager<FontRef>,
        gc: GlyphCache,
    }

    fn deps() -> Deps {
        Deps {
            rr: azul_core::resources::RendererResources::default(),
            fm: super::super::raster::empty_font_manager(),
            gc: GlyphCache::new(),
        }
    }

    /// One repaint of `damage` of the frame `pm` as the backend does it: the kept surfaces of
    /// replaced lists repaired, the passes, the commit.
    fn repaint(scene: &Scene, state: &CpuRenderState, pm: &mut AzulPixmap, damage: &[LogicalRect], d: &mut Deps) {
        let retained = state.retained.as_ref().expect("a cache");
        refresh_retained_surfaces(state, &d.rr, &d.fm, &mut d.gc, 1.0).unwrap();
        retained.begin_frame(&[Arc::as_ptr(&scene.host) as usize, Arc::as_ptr(&scene.child) as usize]);
        render_display_list_damaged(&scene.host, pm, 1.0, &d.rr, &d.fm, &mut d.gc, state, damage).unwrap();
        retained.commit();
    }

    fn white() -> AzulPixmap {
        let mut pm = AzulPixmap::new(FRAME.0, FRAME.1).unwrap();
        pm.fill(255, 255, 255, 255);
        pm
    }

    fn differing(a: &AzulPixmap, b: &AzulPixmap) -> usize {
        a.data().chunks_exact(4).zip(b.data().chunks_exact(4)).filter(|(p, q)| p != q).count()
    }

    /// The frame of `scene` at `zoom` painted afresh with the promoted frame COMPOSITED by the
    /// walk (a new cache, one pass over the whole frame): what the window has to show.
    fn reference(scene: &Scene, zoom: (f32, (f32, f32))) -> AzulPixmap {
        let retained = Arc::new(RetainedFrames::with_max_bytes(64 << 20));
        let promote = Some(PromoteFrame { transform_key: PICTURE, composite: true });
        let st = state(scene, zoom, &retained, promote, 1);
        let mut pm = white();
        repaint(scene, &st, &mut pm, &[rect(WHOLE.0, WHOLE.1, WHOLE.2, WHOLE.3)], &mut deps());
        pm
    }

    /// What the shell shows: `below` with the layer's surface through its matrix inside its
    /// clip, composited as the walk composites a group.
    fn shown(below: &AzulPixmap, plan: &LayerPlan, retained: &RetainedFrames) -> AzulPixmap {
        let (surface, _) = retained.kept(&plan.key).expect("the layer's surface is kept");
        let m = plan.matrix;
        let mat3: Mat3 = [m.xx, m.xy, m.tx, m.yx, m.yy, m.ty, 0.0, 0.0, 1.0];
        let mut out = AzulPixmap::new(below.width(), below.height()).unwrap();
        out.data_mut().copy_from_slice(below.data());
        blit_pixmap_affine_clipped(&surface, &mut out, &mat3_affine_part(&mat3), 1.0, Some(plan.clip));
        out
    }

    /// One frame as `CpuBackend::render_frame` makes it in layer mode: the plan, the damage
    /// (`extra` from the lists' diffs plus the parts of the surface that show and are not
    /// painted), the repaint without the composite. Returns the plan and the damage.
    fn layer_frame(
        scene: &Scene,
        zoom: (f32, (f32, f32)),
        retained: &Arc<RetainedFrames>,
        pm: &mut AzulPixmap,
        extra: &[LogicalRect],
        threads: usize,
    ) -> (LayerPlan, Vec<LogicalRect>) {
        let mut d = deps();
        let probe = state(scene, zoom, retained, None, threads);
        refresh_retained_surfaces(&probe, &d.rr, &d.fm, &mut d.gc, 1.0).unwrap();
        let found = plan(&probe, FRAME, 1.0, &d.gc, &d.fm).expect("the picture is promoted");
        let mut damage = extra.to_vec();
        damage.extend(damage_of_plan(&found, retained.missing(&found.key, found.needed), 1.0));
        let promote = Some(PromoteFrame { transform_key: found.transform_key, composite: false });
        let st = state(scene, zoom, retained, promote, threads);
        repaint(scene, &st, pm, &damage, &mut d);
        assert_eq!(
            retained.missing(&found.key, found.shown),
            Some(Vec::new()),
            "what shows of the surface is painted after the frame"
        );
        (found, damage)
    }

    /// A10-06, the geometry of a layer. The handover's example: a view clip (100, 76) ..
    /// (2940, 1876) px at a scale factor of 2, the picture scaled by 1.5 and moved by
    /// (-300, -200) px. A surface point lands at `1.5 p + (-300, -200)`: (0, 0) at
    /// (-300, -200), (2000, 1000) at (2700, 1300). What shows of a 3964 x 2936 surface is
    /// `(clip - move) / 1.5`: x 266.67 .. 2160, y 184 .. 1384, outward: [266, 184, 2160, 1384].
    /// Mutations: `m[1]` and `m[3]` swapped in `of` turns a shear into its transpose (the
    /// sheared assert); `floor` -> `ceil` on the near edge of `shown_box` loses column 266.
    #[test]
    fn a_layers_matrix_takes_a_surface_pixel_to_the_frame() {
        let m = LayerMatrix::of(&[1.5, 0.0, -300.0, 0.0, 1.5, -200.0, 0.0, 0.0, 1.0]).unwrap();
        assert!(m.is_upright());
        assert_eq!(m.map(0.0, 0.0), (-300.0, -200.0));
        assert_eq!(m.map(2000.0, 1000.0), (2700.0, 1300.0));
        assert_eq!(m.landing_box([0, 0, 100, 100]), (-300.0, -200.0, -150.0, -50.0));
        assert_eq!(m.shown_box((100, 76, 2940, 1876), (3964, 2936)), [266, 184, 2160, 1384]);
        // Cut to the surface: a clip that shows past the surface's far edge.
        assert_eq!(m.shown_box((100, 76, 2940, 1876), (1000, 1000)), [266, 184, 1000, 1000]);
        // A shear: x' = x + 0.5 y, y' = y.
        let sheared = LayerMatrix::of(&[1.0, 0.5, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]).unwrap();
        assert_eq!(sheared.map(0.0, 10.0), (5.0, 10.0));
        assert!(!sheared.is_upright(), "a sheared or turned surface is not shown as a layer");
        let mirrored = LayerMatrix::of(&[-1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]).unwrap();
        assert!(!mirrored.is_upright());
        assert!(LayerMatrix::of(&[1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.001, 0.0, 1.0]).is_none(), "perspective");
    }

    /// A10-06, the promotion rule: the largest frame, and nothing when something is painted
    /// over it. Mutations: `>` -> `>=` promotes the later of two equals; dropping the filter
    /// promotes a frame under a popup, whose layer would hide the popup.
    #[test]
    fn the_largest_frame_is_promoted_unless_something_is_painted_over_it() {
        assert_eq!(choose(&[]), None);
        assert_eq!(choose(&[(100, false)]), Some(0));
        assert_eq!(choose(&[(100, false), (900, false), (400, false)]), Some(1));
        assert_eq!(choose(&[(900, false), (900, false)]), Some(0), "the first of equals");
        assert_eq!(choose(&[(100, false), (900, true)]), None, "not the next largest: one rule, no flapping");
        assert_eq!(choose(&[(100, true), (900, false)]), Some(1));
    }

    /// A10-06, the count: a pan step in layer mode repaints (and presents) the strip of the
    /// surface that came into view, not the view's clip. The doc example of
    /// [`damage_for_missing`]: 11 x 1800 device px = 79,200 bytes of the window's frame,
    /// against the clip's 2840 x 1800 px = 20,448,000 bytes.
    /// Mutations: without the widening by 3 px the rim of the taps lands outside the clip and
    /// yields no rect at all; landing y through `xx` and `tx` misplaces the rect.
    #[test]
    fn a_pan_step_in_layer_mode_repaints_the_strip_not_the_clip() {
        let clip = (100, 76, 2940, 1876);
        let m = LayerMatrix { xx: 1.0, xy: 0.0, yx: 0.0, yy: 1.0, tx: -484.0, ty: -436.0 };
        let rects = damage_for_missing(&[[3416, 510, 3426, 2314]], &m, clip, 2.0);
        assert_eq!(rects, vec![rect(1464.5, 38.0, 5.5, 900.0)]);
        let bytes: f32 = rects.iter().map(|r| r.size.width * 2.0 * r.size.height * 2.0 * 4.0).sum();
        assert_eq!(bytes, 79_200.0);
        assert_eq!((clip.2 - clip.0) * (clip.3 - clip.1) * 4, 20_448_000);
        // Nothing missing: nothing repainted, nothing presented.
        assert!(damage_for_missing(&[], &m, clip, 2.0).is_empty());
        // The rim of the taps alone (2 px past what shows at the far edge): still a rect
        // inside the clip, so that a pass can paint it.
        let rim = damage_for_missing(&[[3424, 510, 3426, 2314]], &m, clip, 2.0);
        assert_eq!(rim, vec![rect(1468.5, 38.0, 1.5, 900.0)]);
        // More parts than a frame repaints one by one: their bounding box.
        let many: Vec<[i32; 4]> = (0..12).map(|i| [600 + i * 100, 600, 610 + i * 100, 610]).collect();
        assert_eq!(damage_for_missing(&many, &m, clip, 2.0).len(), 1);
    }

    /// A10-06, what the plan finds in the scene: the picture's root, under the view's box,
    /// landing at the view's origin plus the content offset. The view is at (100, 50) and its
    /// content offset (-160, -160): the surface's pixel (0, 0) lands at (100 - 160, 50 - 160)
    /// = (-60, -110), and (160, 160) at the view's corner. The view shows
    /// [160, 160, 960, 760] of the surface. The view's list paints nothing else, so a pan of
    /// the view moves no pixel of the window's frame (`view`, `alone`).
    /// Mutations: `- content_offset` -> `+` in the scan's view arm lands the surface at
    /// (260, 210); a clip taken from a damage rect instead of the view's box is another clip
    /// every frame; an offset that is not the walk's makes another key, and the surface the
    /// walk kept is never found (the invariant test below).
    #[test]
    fn the_plan_of_a_window_with_a_map_view_is_the_views_picture() {
        let retained = Arc::new(RetainedFrames::with_max_bytes(64 << 20));
        let d = deps();
        let s = Scene { host: host(REST, false), child: picture(false) };
        let found = plan(&state(&s, (1.0, (0.0, 0.0)), &retained, None, 1), FRAME, 1.0, &d.gc, &d.fm)
            .expect("the picture is promoted");
        assert_eq!(found.transform_key, PICTURE);
        assert_eq!(found.key.size, (1120, 920));
        assert_eq!(found.clip, (100, 50, 900, 650));
        assert_eq!(found.matrix, LayerMatrix { xx: 1.0, xy: 0.0, yx: 0.0, yy: 1.0, tx: -60.0, ty: -110.0 });
        assert_eq!(found.shown, [160, 160, 960, 760]);
        assert_eq!(found.needed, [158, 158, 962, 762]);
        assert_eq!((found.view, found.alone), (Some(2), true), "the view is the third item of the window's list");
        assert_eq!(found.clip_rect(1.0), rect(VIEW_BOX.0, VIEW_BOX.1, VIEW_BOX.2, VIEW_BOX.3));
        // Scaled by 1.5 about the picture's corner and moved by (-40, -30): (0, 0) lands at
        // (-60 - 40, -110 - 30), and the view shows ((100 + 100) / 1.5, (50 + 140) / 1.5) ..
        // ((900 + 100) / 1.5, (650 + 140) / 1.5) = (133.3, 126.7) .. (666.7, 526.7).
        let zoomed = plan(&state(&s, (1.5, (-40.0, -30.0)), &retained, None, 1), FRAME, 1.0, &d.gc, &d.fm)
            .expect("the picture is promoted");
        assert_eq!(zoomed.matrix, LayerMatrix { xx: 1.5, xy: 0.0, yx: 0.0, yy: 1.5, tx: -100.0, ty: -140.0 });
        assert_eq!(zoomed.shown, [133, 126, 667, 527]);
        assert_eq!(zoomed.key, found.key, "the picture does not depend on the frame's own matrix");
        // A panel painted over the view: nothing is promoted.
        let covered = Scene { host: host(REST, true), child: Arc::clone(&s.child) };
        assert!(plan(&state(&covered, (1.0, (0.0, 0.0)), &retained, None, 1), FRAME, 1.0, &d.gc, &d.fm).is_none());
        // A surface that cannot be kept (the cache keeps nothing): nothing is promoted.
        let none = Arc::new(RetainedFrames::with_max_bytes(0));
        assert!(plan(&state(&s, (1.0, (0.0, 0.0)), &none, None, 1), FRAME, 1.0, &d.gc, &d.fm).is_none());
    }

    /// A10-06, THE invariant: the window's frame painted without the promoted frame, with
    /// the layer's surface composited through the plan's matrix inside the plan's clip, is
    /// the frame the walk paints when it composites the promoted frame itself. Every pixel,
    /// at every step of: the first frame, a pan inside what is painted (no pixel painted), a
    /// pan past it (a strip), a zoom step and a pan during the zoom (no pixel painted), a
    /// zoom out past what is painted, a tile arriving, an edit back, and back at rest; on one
    /// thread and in bands.
    ///
    /// The numbers (surface px; a pass paints 64 px around what lands, a layer needs 2):
    /// step 1 paints [96, 96, 1024, 824]. Step 2 (the view pans by (12, -5)) needs
    /// [170, 153, 974, 757]: painted. Step 3 (pans to an offset of (-260, -155)) needs x up to
    /// 1062: [1024, 153, 1062, 757] is missing, lands at x 861 .. 905 of the frame (widened by
    /// 3), is cut to the clip: the damage is (861, 50) 39 x 600, and its pass paints
    /// [957, 91, 1120, 819]. Steps 4 and 5 (scale 1.25) need [158, 150, 802, 634] and
    /// [177, 143, 822, 628]: painted.
    /// Step 6 (scale 0.75, landing at (0, -60)) needs [131, 144, 1120, 920]: the rows from 824
    /// are missing.
    ///
    /// Mutations: a hit that composites (`!candidate.layer` dropped) or a kept pop that does
    /// (`lands = false` dropped) paints the picture into the window's frame (assert 2); a
    /// plan whose clip, matrix or key is not the walk's shifts the picture, or never finds
    /// the surface (assert 1, and the one in `layer_frame`); a frame that does not damage
    /// what is missing of the surface shows transparent columns after the pan (assert 1).
    #[test]
    fn a_frame_and_its_layer_are_the_frame_painted_with_the_group_composited() {
        // (content offset, zoom, the edited list?, what a list diff damaged, pixels painted?)
        type Step = ((f32, f32), (f32, (f32, f32)), bool, Option<(f32, f32, f32, f32)>, bool);
        let steps: [(&str, Step); 9] = [
            ("the first frame", (REST, (1.0, (0.0, 0.0)), false, Some(WHOLE), true)),
            ("a pan inside what is painted", ((-172.0, -155.0), (1.0, (0.0, 0.0)), false, None, false)),
            ("a pan past what is painted", ((-260.0, -155.0), (1.0, (0.0, 0.0)), false, None, true)),
            ("a zoom step", (REST, (1.25, (-40.0, -30.0)), false, None, false)),
            ("a pan during the zoom", (REST, (1.25, (-64.0, -22.0)), false, None, false)),
            ("a zoom out past what is painted", (REST, (0.75, (60.0, 50.0)), false, None, true)),
            // A list diff damages the view where the lists differ; here, coarsely, all of it.
            ("a tile arriving", (REST, (0.75, (60.0, 50.0)), true, Some(VIEW_BOX), true)),
            ("an edit back", (REST, (0.75, (60.0, 50.0)), false, Some(VIEW_BOX), true)),
            ("back at rest", (REST, (1.0, (0.0, 0.0)), false, None, false)),
        ];
        let (plain, edited_list) = (picture(false), picture(true));
        for threads in [1usize, 4] {
            let retained = Arc::new(RetainedFrames::with_max_bytes(64 << 20));
            let mut below = white();
            let mut stamp = 0u64;
            for (name, (offset, zoom, edited, diff, paints)) in steps {
                let what = format!("{threads} thread(s), {name}");
                // The view's list is the same list from frame to frame unless it changed.
                let child = if edited { &edited_list } else { &plain };
                let s = Scene { host: host(offset, false), child: Arc::clone(child) };
                let extra: Vec<LogicalRect> = diff.iter().map(|d| rect(d.0, d.1, d.2, d.3)).collect();
                let (found, damage) = layer_frame(&s, zoom, &retained, &mut below, &extra, threads);
                // 1. frame + layer == the frame with the group composited by the walk.
                let composed = shown(&below, &found, &retained);
                let wanted = reference(&s, zoom);
                assert_eq!(differing(&composed, &wanted), 0, "{what}: frame and layer are the frame");
                // 2. the window's frame under the layer holds no pixel of the picture: it is
                //    the frame of the same window with an empty view.
                let empty = Scene { host: Arc::clone(&s.host), child: list(Vec::new()) };
                assert_eq!(
                    differing(&below, &reference(&empty, zoom)),
                    0,
                    "{what}: nothing of the picture is in the window's frame"
                );
                // 3. the stamp of the surface's pixels moves exactly when a pixel was painted
                //    (a repair of a replaced list counts as one).
                let (_, now) = retained.kept(&found.key).expect("kept");
                assert_eq!(now != stamp, paints, "{what}: stamp {stamp} -> {now}, damage {damage:?}");
                if !paints {
                    assert!(damage.is_empty(), "{what}: a move of the layer damages nothing: {damage:?}");
                }
                if name == "a pan past what is painted" {
                    // One rect per part that is missing (the bands of the first frame each
                    // painted a part); together they are the strip.
                    let strip = damage.iter().fold((f32::MAX, f32::MAX, f32::MIN, f32::MIN), |b, r| {
                        (
                            b.0.min(r.origin.x),
                            b.1.min(r.origin.y),
                            b.2.max(r.origin.x + r.size.width),
                            b.3.max(r.origin.y + r.size.height),
                        )
                    });
                    assert_eq!(strip, (861.0, 50.0, 900.0, 650.0), "{what}: the strip, 39 x 600 px");
                }
                stamp = now;
            }
            assert_eq!(retained.len(), 1, "one surface for the one frame");
        }
    }
}
