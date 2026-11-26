//! The frame tap: the pictures a window shows, handed to an app as they change.
//!
//! An app that records its window, streams it or looks at it registers a callback
//! (`CallbackInfo::set_frame_tap`). The CPU frame (`CpuBackend::render_frame`, the one every shell
//! paints and presents: macOS, X11, Wayland, Windows, iOS, Android and the headless runner) calls it for
//! every frame that changed a pixel, with what the frame is and what changed in it, and never for a frame
//! that changed nothing. A window that is drawn by the GPU has no CPU frame and so no tap.
//!
//! The tap hears the frame the window shows, in device pixels, at the window's own scale: no permission
//! of the system is needed for it, no other window or desktop is in it, and a frame is exactly the bytes
//! that were painted. The pixels are borrowed for the call, not copied: a tap that wants them later
//! copies what it needs (the rows of the damaged rectangles are all that changed) and returns; it runs on
//! the window's thread, inside the frame, so what it does is part of the frame's time.

use alloc::{sync::Arc, vec::Vec};
use core::{
    fmt,
    sync::atomic::{AtomicU64, Ordering},
};

use azul_core::{
    geom::LogicalRect,
    gl::{I32VecRef, U8VecRef},
    refany::RefAny,
};

use crate::window::FrameDamage;

/// The function a frame tap calls: the data the app registered, and the frame.
pub type FrameTapCallbackType = extern "C" fn(RefAny, FrameTapInfo);

/// A frame tap's function, as the functions of the C interface take it. (A plain function: a language
/// that registers its callables through a host handle has no way to tap a frame yet.)
#[repr(C)]
pub struct FrameTapCallback {
    /// The function called with each frame.
    pub cb: FrameTapCallbackType,
}

azul_core::impl_callback_simple!(FrameTapCallback);

impl From<FrameTapCallbackType> for FrameTapCallback {
    fn from(cb: FrameTapCallbackType) -> Self {
        Self { cb }
    }
}

impl FrameTapCallback {
    /// A function (of Rust or of C) as a tap's callback.
    #[must_use]
    pub const fn create(cb: FrameTapCallbackType) -> Self {
        Self { cb }
    }
}

/// What a window's frame tap hears of one frame.
#[repr(C)]
#[derive(Clone)]
pub struct FrameTapInfo {
    /// The frame's width in device pixels.
    pub width: u32,
    /// The frame's height in device pixels.
    pub height: u32,
    /// Bytes from the start of one row to the start of the next: `width * 4`, or more where the renderer
    /// paints into a buffer of the platform's whose rows are padded.
    pub stride: usize,
    /// Device pixels to a logical one (2.0 on a Retina display).
    pub scale: f32,
    /// The whole frame changed (the first frame, a resize, a change of scale or of the colour of the
    /// canvas): `damage` is empty then.
    pub full: bool,
    /// What changed, when `full` is not set: rectangles in device pixels, `x, y, width, height` one
    /// after the other, each rounded out to whole pixels and inside the frame. The pixels outside them
    /// are the ones the tap heard before.
    pub damage: I32VecRef,
    /// The frame: `stride * height` bytes of RGBA, 8 bits a channel, top row first, as the renderer keeps
    /// it (alpha premultiplied, 255 everywhere in a window with a solid background). Borrowed: valid
    /// until the tap returns. What lies past `width * 4` in a row is not the frame's.
    pub pixels: U8VecRef,
}

impl fmt::Debug for FrameTapInfo {
    // (Not the derived one: that would print every byte of the frame.)
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FrameTapInfo {{ {}x{} px at {}x, ", self.width, self.height, self.scale)?;
        if self.full {
            write!(f, "the whole frame")?;
        } else {
            write!(f, "{} rects", self.damage.len / 4)?;
        }
        write!(f, ", {} bytes }}", self.pixels.len)
    }
}

/// A window's frame tap: the function and the data it is called with.
///
/// The tap also remembers the size of the last frame it was given, so that the first frame it hears
/// (and the first of another size) is the whole frame, whatever the renderer repainted: it has
/// nothing yet to lay a rectangle over. The copies of a tap are one registration and share that.
#[derive(Debug, Clone)]
pub struct FrameTap {
    /// What the app registered with the tap: the callback is called with a handle on it.
    pub data: RefAny,
    /// The function that hears the frames.
    pub callback: FrameTapCallback,
    /// The size of the last frame told, `width << 32 | height`; 0 before the first.
    told: Arc<AtomicU64>,
}

/// A tap's data is what the handler that registered it was given, often a clone of a node's dataset: a rebuild that
/// merges the dataset keeps one allocation and drops the other, and a tap that still held a clone of the dropped one
/// would tell the frames to a copy the app never reads.
impl azul_core::refany::RefAnyHolder for FrameTap {
    fn for_each_refany(&mut self, visit: &mut dyn FnMut(&mut RefAny)) {
        visit(&mut self.data);
    }
}

impl FrameTap {
    /// A tap that calls `callback` with a handle on `data`.
    #[must_use]
    pub fn new<C: Into<FrameTapCallback>>(data: RefAny, callback: C) -> Self {
        Self { data, callback: callback.into(), told: Arc::new(AtomicU64::new(0)) }
    }

    /// Would the next frame, `width` x `height` pixels, be the first this tap hears of that size? Then
    /// it is told whole, and the renderer has to make a frame even if the window changed nothing.
    #[must_use]
    pub fn needs_a_whole_frame(&self, width: u32, height: u32) -> bool {
        self.told.load(Ordering::Relaxed) != frame_size(width, height)
    }
}

/// A frame's size as the one number a tap remembers it by (never 0 for a frame that has pixels).
fn frame_size(width: u32, height: u32) -> u64 {
    (u64::from(width) << 32) | u64::from(height)
}

/// `damage` as a tap hears it: rectangles of the frame in device pixels, `x, y, width, height` one
/// after the other. Each is scaled by `scale`, rounded out to whole pixels (a pixel that is only partly
/// damaged is damaged) and cut to the `width` x `height` frame; one with nothing inside the frame (or no
/// size, or no number for a size) is left out.
#[must_use]
// The frame's sides are far below the 2^24 where an f32 stops counting whole numbers, and a rectangle
// is cut to them before it becomes a whole number again.
#[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
pub fn device_rects(damage: &[LogicalRect], scale: f32, width: u32, height: u32) -> Vec<i32> {
    let (frame_w, frame_h) = (width as f32, height as f32);
    let mut out = Vec::with_capacity(damage.len() * 4);
    for rect in damage {
        let x0 = (rect.origin.x * scale).floor().clamp(0.0, frame_w);
        let y0 = (rect.origin.y * scale).floor().clamp(0.0, frame_h);
        let x1 = ((rect.origin.x + rect.size.width) * scale).ceil().clamp(0.0, frame_w);
        let y1 = ((rect.origin.y + rect.size.height) * scale).ceil().clamp(0.0, frame_h);
        // (Written so that a number that is not one fails it too.)
        if !(x1 > x0 && y1 > y0) {
            continue;
        }
        out.extend_from_slice(&[x0 as i32, y0 as i32, (x1 - x0) as i32, (y1 - y0) as i32]);
    }
    out
}

/// Hands a finished frame to `tap`: once, with the frame's size, what `damage` says changed and the
/// pixels of the frame. `stride` is the bytes from one row of `pixels` to the next: more than
/// `width * 4` when the renderer paints into a buffer of the platform's whose rows are padded.
/// The first frame the tap hears of a size is told whole, whatever `damage` says (it names what the
/// renderer repainted, and the tap has no picture of that size yet). Nothing is called when nothing
/// changed (`FrameDamage::None`, or damage that lies outside the frame), or when `pixels` is shorter than
/// `stride * height` or `stride` shorter than a row.
pub fn deliver(
    tap: &FrameTap,
    pixels: &[u8],
    width: u32,
    height: u32,
    stride: usize,
    scale: f32,
    damage: &FrameDamage,
) {
    if matches!(damage, FrameDamage::None) {
        return;
    }
    let bytes = stride.saturating_mul(height as usize);
    if stride < width as usize * 4 || pixels.len() < bytes {
        return;
    }
    // The first frame this tap hears of a size is told whole: the buffer holds all of it, correct,
    // however the renderer painted it, and the tap has no picture of that size to lay rectangles on.
    let size = frame_size(width, height);
    let first_of_its_size = tap.told.swap(size, Ordering::Relaxed) != size;
    let rects;
    let (full, changed): (bool, &[i32]) = match damage {
        FrameDamage::Rects(list) if !first_of_its_size => {
            rects = device_rects(list, scale, width, height);
            if rects.is_empty() {
                return;
            }
            (false, rects.as_slice())
        }
        _ => (true, &[]),
    };
    let info = FrameTapInfo {
        width,
        height,
        stride,
        scale,
        full,
        damage: I32VecRef::from(changed),
        pixels: U8VecRef::from(&pixels[..bytes]),
    };
    // direct-cb-call: `FrameTapCallback` is a plain function with no context of a binding's to hand on (a language
    // that keeps its callables behind a host handle cannot tap a frame yet), so it has no `invoke`.
    (tap.callback.cb)(tap.data.clone(), info);
}

#[cfg(test)]
#[path = "frame_tap_test.rs"]
mod frame_tap_test;
