//! Group surfaces kept from frame to frame.
//!
//! A reference frame whose matrix is not a translation is painted into a surface of its own and
//! composited through the matrix (`raster::TransformGroup`). The picture in that surface does
//! not depend on the matrix: a zoom step, a turn, a slide that only writes a new matrix paints
//! the same picture again. Here the surface is kept, with the part of it that was painted, and
//! the next frame composites it instead of painting it.
//!
//! The cache is read-only while a frame's passes run (every band of a frame sees the same
//! state, so what a band hits does not depend on thread timing); what the passes painted is
//! handed over when they are done.

use std::sync::Mutex;

/// The group surfaces a backend keeps between its frames. Shared with the frame's passes
/// through `CpuRenderState::retained`.
#[derive(Debug, Default)]
pub struct RetainedFrames {
    inner: Mutex<Inner>,
}

#[derive(Debug, Default)]
struct Inner {
    /// Bytes of pixels the kept surfaces hold.
    bytes: usize,
}

impl RetainedFrames {
    /// A cache that keeps nothing yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Bytes of pixels the kept surfaces hold.
    #[must_use]
    pub fn bytes(&self) -> usize {
        self.inner.lock().map_or(0, |inner| inner.bytes)
    }
}
