/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! What this process' WebRender threads have done so far, counted, for the
//! host's frame report (azul: `GpuFrameCounters`, audit A4).
//!
//! The scene builder, the render backend, the glyph workers and the renderer
//! each add to a process-wide atomic (one relaxed add per event: a scene, a
//! frame, a hit tester, a batch of texture uploads); the host takes a
//! [`snapshot`] after it has drawn a frame and subtracts the one before
//! ([`Counters::since`]). Process-wide, because the threads belong to no
//! window: with several windows a frame's difference holds what all of them
//! did in between.

use std::sync::atomic::{AtomicU64, Ordering};

static SCENE_BUILDS: AtomicU64 = AtomicU64::new(0);
static SCENE_BUILD_NS: AtomicU64 = AtomicU64::new(0);
static FRAME_BUILDS: AtomicU64 = AtomicU64::new(0);
static FRAME_BUILD_NS: AtomicU64 = AtomicU64::new(0);
static HIT_TESTER_REBUILDS: AtomicU64 = AtomicU64::new(0);
static TEXTURE_UPLOADS: AtomicU64 = AtomicU64::new(0);
static TEXTURE_UPLOAD_PX: AtomicU64 = AtomicU64::new(0);

/// The counters at one moment (sums since the process started).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counters {
    /// Scenes built (`SceneBuilder::build`: every pipeline's list flattened).
    pub scene_builds: u64,
    /// The time those builds took, in nanoseconds (scene builder thread).
    pub scene_build_ns: u64,
    /// Frames built (`FrameBuilder::build`).
    pub frame_builds: u64,
    /// The time those builds took, glyph rasterisation they waited for
    /// included, in nanoseconds (render backend thread).
    pub frame_build_ns: u64,
    /// Hit testers built over the spatial tree (none for a scene without
    /// hit-test items).
    pub hit_tester_rebuilds: u64,
    /// Glyphs rasterised by the glyph rasteriser's workers.
    pub glyphs_rasterized: u64,
    /// Updates of the texture cache the renderer uploaded (images, glyphs).
    pub texture_uploads: u64,
    /// The pixels of those uploads.
    pub texture_upload_px: u64,
}

impl Counters {
    /// What happened between `earlier` and `self`.
    pub fn since(&self, earlier: &Counters) -> Counters {
        Counters {
            scene_builds: self.scene_builds.wrapping_sub(earlier.scene_builds),
            scene_build_ns: self.scene_build_ns.wrapping_sub(earlier.scene_build_ns),
            frame_builds: self.frame_builds.wrapping_sub(earlier.frame_builds),
            frame_build_ns: self.frame_build_ns.wrapping_sub(earlier.frame_build_ns),
            hit_tester_rebuilds: self
                .hit_tester_rebuilds
                .wrapping_sub(earlier.hit_tester_rebuilds),
            glyphs_rasterized: self
                .glyphs_rasterized
                .wrapping_sub(earlier.glyphs_rasterized),
            texture_uploads: self.texture_uploads.wrapping_sub(earlier.texture_uploads),
            texture_upload_px: self
                .texture_upload_px
                .wrapping_sub(earlier.texture_upload_px),
        }
    }
}

/// The counters now.
pub fn snapshot() -> Counters {
    Counters {
        scene_builds: SCENE_BUILDS.load(Ordering::Relaxed),
        scene_build_ns: SCENE_BUILD_NS.load(Ordering::Relaxed),
        frame_builds: FRAME_BUILDS.load(Ordering::Relaxed),
        frame_build_ns: FRAME_BUILD_NS.load(Ordering::Relaxed),
        hit_tester_rebuilds: HIT_TESTER_REBUILDS.load(Ordering::Relaxed),
        glyphs_rasterized: glyph_rasterizer::glyphs_rasterized(),
        texture_uploads: TEXTURE_UPLOADS.load(Ordering::Relaxed),
        texture_upload_px: TEXTURE_UPLOAD_PX.load(Ordering::Relaxed),
    }
}

/// A scene was built in `ns` nanoseconds.
pub(crate) fn scene_built(ns: u64) {
    SCENE_BUILDS.fetch_add(1, Ordering::Relaxed);
    SCENE_BUILD_NS.fetch_add(ns, Ordering::Relaxed);
}

/// A frame was built in `ns` nanoseconds.
pub(crate) fn frame_built(ns: u64) {
    FRAME_BUILDS.fetch_add(1, Ordering::Relaxed);
    FRAME_BUILD_NS.fetch_add(ns, Ordering::Relaxed);
}

/// A hit tester was built over the spatial tree.
pub(crate) fn hit_tester_rebuilt() {
    HIT_TESTER_REBUILDS.fetch_add(1, Ordering::Relaxed);
}

/// The renderer uploaded `count` updates of `px` pixels into its texture cache.
pub(crate) fn textures_uploaded(count: u64, px: u64) {
    TEXTURE_UPLOADS.fetch_add(count, Ordering::Relaxed);
    TEXTURE_UPLOAD_PX.fetch_add(px, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The counters only grow, and a difference is what was added in between
    /// (other tests of this process may add too: "at least").
    #[test]
    fn a_difference_of_two_snapshots_is_what_happened_in_between() {
        let before = snapshot();
        scene_built(1_500);
        frame_built(700);
        frame_built(300);
        hit_tester_rebuilt();
        textures_uploaded(3, 4_096);
        let during = snapshot().since(&before);
        assert!(during.scene_builds >= 1 && during.scene_build_ns >= 1_500);
        assert!(during.frame_builds >= 2 && during.frame_build_ns >= 1_000);
        assert!(during.hit_tester_rebuilds >= 1);
        assert!(during.texture_uploads >= 3 && during.texture_upload_px >= 4_096);
        assert_eq!(snapshot().since(&snapshot()).scene_builds, 0);
    }
}
