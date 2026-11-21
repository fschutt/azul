//! A bounded memo of the SVG masks the display-list builder rasterises.
//!
//! (CHART7) Every build used to rasterise every clip mask and stroke mask again
//! - 2x-oversampled coverage per shape, per frame - for geometry that had not
//! changed. A mask is a pure function of its inputs (the path, the box it is
//! drawn into, the viewBox, the stroke width), so an unchanged shape gets its
//! earlier [`ImageRef`] back (the same image id, so a renderer's image cache
//! hits too).
//!
//! The memo is [`SvgMaskMemo`]: one per layout cache (`LayoutCache::svg_masks`),
//! so it lives as long as the document it serves, and no window shares masks
//! with another. A pass borrows it for the display-list build
//! (`PassMemo::svg_masks`) and gives it back. It is bounded by entries and by
//! mask bytes: the least recently used mask goes. (It was a thread-local, shared
//! by every window that built a list on the thread.)
//!
//! Kept out of the `display_list` module on purpose: the web backend's lift stubs
//! every symbol of that module, and this memo is created with every layout cache.

use std::hash::{Hash, Hasher};

use azul_core::{geom::LogicalRect, resources::ImageRef};

#[cfg(feature = "cpurender")]
use crate::image_memo::ImageMemo;

/// At most this many masks ...
const MAX_ENTRIES: usize = 256;
/// ... holding at most this many mask bytes (R8: one per pixel).
const MAX_BYTES: usize = 8 * 1024 * 1024;

/// Which rasteriser drew a mask.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MaskKind {
    /// `rasterize_svg_clip_to_r8`: the shape's fill region (non-zero).
    Clip,
    /// ... with `fill-rule="evenodd"`: overlapping rings cut holes.
    ClipEvenOdd,
    /// `rasterize_svg_stroke_to_r8`: its stroke.
    Stroke,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct Key {
    kind: MaskKind,
    path: u64,
    /// The paint rect's origin (only where the pixels depend on it) and
    /// size, as f32 bits.
    rect: [u32; 4],
    view_box: Option<[u32; 4]>,
    width: u32,
    /// The stroke's caps, joins and dashes (`StrokeOutline::memo_hash`);
    /// 0 for a clip.
    outline: u64,
}

/// The SVG masks of one layout cache, by what they are drawn from. Holds nothing in a build
/// without the `cpurender` feature, which draws no mask.
#[derive(Debug)]
pub struct SvgMaskMemo {
    #[cfg(feature = "cpurender")]
    memo: ImageMemo<Key, ImageRef>,
}

impl Default for SvgMaskMemo {
    fn default() -> Self {
        Self {
            #[cfg(feature = "cpurender")]
            memo: ImageMemo::new(MAX_ENTRIES, MAX_BYTES),
        }
    }
}

/// A copy of a cache starts with no masks: the pictures of the original stay
/// where they were drawn.
impl Clone for SvgMaskMemo {
    fn clone(&self) -> Self {
        Self::default()
    }
}

/// A 64-bit hash of every coordinate of `path`, by element.
fn path_hash(path: &azul_core::svg::SvgMultiPolygon) -> u64 {
    use azul_core::svg::SvgPathElement;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    let point = |p: &azul_css::props::basic::SvgPoint,
                 h: &mut std::collections::hash_map::DefaultHasher| {
        p.x.to_bits().hash(h);
        p.y.to_bits().hash(h);
    };
    for ring in path.rings.as_ref() {
        0xFFu8.hash(&mut h);
        for item in ring.items.as_ref() {
            match item {
                SvgPathElement::Line(l) => {
                    0u8.hash(&mut h);
                    point(&l.start, &mut h);
                    point(&l.end, &mut h);
                }
                SvgPathElement::QuadraticCurve(q) => {
                    1u8.hash(&mut h);
                    point(&q.start, &mut h);
                    point(&q.ctrl, &mut h);
                    point(&q.end, &mut h);
                }
                SvgPathElement::CubicCurve(c) => {
                    2u8.hash(&mut h);
                    point(&c.start, &mut h);
                    point(&c.ctrl_1, &mut h);
                    point(&c.ctrl_2, &mut h);
                    point(&c.end, &mut h);
                }
            }
        }
    }
    h.finish()
}

#[cfg(feature = "cpurender")]
impl SvgMaskMemo {
    /// The mask of `kind` for these inputs: the memo's, else `rasterise()`'s
    /// (remembered when it drew one).
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // a mask's pixel count
    pub fn get_or_rasterise(
        &mut self,
        kind: MaskKind,
        path: &azul_core::svg::SvgMultiPolygon,
        paint_rect: &LogicalRect,
        view_box: Option<(f32, f32, f32, f32)>,
        width: f32,
        outline: u64,
        rasterise: impl FnOnce() -> Option<ImageRef>,
    ) -> Option<ImageRef> {
        // A clip WITHOUT a viewBox is drawn in window coordinates (the paint
        // rect's origin is subtracted from the geometry); a stroke, and a
        // clip in a viewBox, only see the box's size.
        let origin = match (kind, view_box) {
            (MaskKind::Clip | MaskKind::ClipEvenOdd, None) => [
                paint_rect.origin.x.to_bits(),
                paint_rect.origin.y.to_bits(),
            ],
            _ => [0, 0],
        };
        let key = Key {
            kind,
            path: path_hash(path),
            rect: [
                origin[0],
                origin[1],
                paint_rect.size.width.to_bits(),
                paint_rect.size.height.to_bits(),
            ],
            view_box: view_box
                .map(|(x, y, w, h)| [x.to_bits(), y.to_bits(), w.to_bits(), h.to_bits()]),
            width: width.to_bits(),
            outline,
        };
        if let Some(hit) = self.memo.get(&key) {
            return Some(hit);
        }
        let image = rasterise()?;
        let size = image.get_size();
        let bytes = (size.width.max(0.0) * size.height.max(0.0)) as usize;
        self.memo.insert(key, image.clone(), bytes);
        Some(image)
    }
}
