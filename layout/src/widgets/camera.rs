//! Camera-preview widget — a "dumb widget" (like [`MapWidget`](super::map))
//! that owns a background capture thread + a GL-texture `ImageRef`, with **no**
//! camera-specific logic in the core framework (SUPER_PLAN_2 §4 P6, widget
//! pivot — see the MASTER PLAN in `MOBILE_SESSION_LOG.md`).
//!
//! Design:
//! - `CameraWidget::create(config).dom()` → a static `<img>` (Image node)
//!   holding a stable GL-texture `ImageRef`, plus a [`CameraWidgetState`]
//!   `RefAny` dataset carried across relayout by [`merge_camera_state`].
//! - On `AfterMount`, a background capture thread is started
//!   (`CallbackInfo::add_thread`, like the map-tile fetch). It captures frames;
//!   its writeback uploads each into the GL texture and triggers a recomposite
//!   (`ShouldReRenderCurrentWindow`) — **no relayout, no display-list rebuild,
//!   no RenderImageCallback**, because WebRender re-reads the external texture
//!   each composite (wr ImageKey == ImageRef data pointer, so the key is stable).
//! - The [`CameraConfig`] control POD (front/back, zoom, …) is mutated by user
//!   callbacks to switch cameras without re-initialising permissions (the
//!   thread persists via the merge callback).
//!
//! This tick lands the **scaffold**: the widget structure, a placeholder image,
//! and the AfterMount/merge wiring. The capture thread, GL-texture upload, and
//! the AVFoundation/Camera2 backend are follow-up ticks.

use azul_core::callbacks::Update;
use azul_core::camera::CameraConfig;
use azul_core::dom::{ComponentEventFilter, DatasetMergeCallbackType, Dom, EventFilter};
use azul_core::refany::{OptionRefAny, RefAny};
use azul_core::resources::{ImageRef, RawImageFormat};

use crate::callbacks::{Callback, CallbackInfo, CallbackType};

/// Live state for one camera widget, owned by the node's dataset `RefAny` and
/// carried across relayout by [`merge_camera_state`].
pub struct CameraWidgetState {
    /// The requested capture configuration (the control POD).
    pub config: CameraConfig,
    /// `true` once the capture thread has been started, so a relayout re-mount
    /// doesn't spawn a second one. (Later: the GL-texture handle + thread id
    /// live here too.)
    pub started: bool,
}

/// A camera-preview widget. `create(config).dom()` yields an `<img>` the
/// capture thread keeps fed.
#[repr(C)]
pub struct CameraWidget {
    /// Requested capture config (camera facing, resolution, fps, format).
    pub config: CameraConfig,
}

impl CameraWidget {
    /// Create a camera widget for the given capture config.
    pub fn create(config: CameraConfig) -> Self {
        Self { config }
    }

    /// Build the widget's DOM: a single `<img>` node, fed by a background
    /// capture thread started on mount.
    pub fn dom(self) -> Dom {
        let state = CameraWidgetState {
            config: self.config,
            started: false,
        };
        let dataset = RefAny::new(state);

        // Placeholder texture until the capture thread installs the live GL
        // texture. Sized from the config (0 → a sane default).
        let w = if self.config.width > 0 {
            self.config.width as usize
        } else {
            640
        };
        let h = if self.config.height > 0 {
            self.config.height as usize
        } else {
            480
        };
        let placeholder = ImageRef::null_image(
            w,
            h,
            RawImageFormat::BGRA8,
            b"azul-camera-placeholder".to_vec(),
        );

        Dom::create_image(placeholder)
            .with_dataset(OptionRefAny::Some(dataset.clone()))
            .with_merge_callback(merge_camera_state as DatasetMergeCallbackType)
            .with_callback(
                EventFilter::Component(ComponentEventFilter::AfterMount),
                dataset,
                Callback::from(camera_on_after_mount as CallbackType),
            )
    }
}

/// AfterMount: start the background capture thread exactly once. (The
/// `add_thread` call + GL-texture upload land in the next tick; for now this
/// flips the guard so the mount wiring is exercised.)
extern "C" fn camera_on_after_mount(mut data: RefAny, _info: CallbackInfo) -> Update {
    if let Some(mut s) = data.downcast_mut::<CameraWidgetState>() {
        if s.started {
            return Update::DoNothing;
        }
        s.started = true;
        // TODO(next tick): start the capture thread via `info.add_thread(...)`;
        // its writeback uploads frames to the GL texture + recomposites.
    }
    Update::DoNothing
}

/// Carry the live state forward across relayout: the freshly-built state from
/// `dom()` keeps its (possibly user-updated) config, but inherits the running
/// thread / texture / `started` flag from the previous frame's state.
extern "C" fn merge_camera_state(mut new_data: RefAny, mut old_data: RefAny) -> RefAny {
    {
        let new_guard = new_data.downcast_mut::<CameraWidgetState>();
        let old_guard = old_data.downcast_ref::<CameraWidgetState>();
        if let (Some(mut new_g), Some(old_g)) = (new_guard, old_guard) {
            new_g.started = old_g.started;
        }
    }
    new_data
}
