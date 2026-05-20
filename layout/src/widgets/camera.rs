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
//! This tick (widget.2) wires the **background thread + writeback plumbing**
//! with a self-contained **test-pattern** worker (no platform deps): the
//! worker emits a colour-cycling frame ~30×/s; the writeback stores the latest
//! frame in the dataset. The GL-texture upload + recomposite is widget.3; the
//! real AVFoundation/Camera2 worker (passed in dll-side, like map's
//! `dom_with_fetch`) is widget.4.

use alloc::vec::Vec;

use azul_core::callbacks::Update;
use azul_core::camera::CameraConfig;
use azul_core::dom::{ComponentEventFilter, DatasetMergeCallbackType, Dom, EventFilter};
use azul_core::refany::{OptionRefAny, RefAny};
use azul_core::resources::{ImageRef, RawImageFormat};
use azul_core::task::{ThreadId, ThreadReceiver};

use crate::callbacks::{Callback, CallbackInfo, CallbackType};
use crate::thread::{
    Thread, ThreadCallback, ThreadReceiveMsg, ThreadSender, ThreadWriteBackMsg, WriteBackCallback,
};

/// One captured frame, sent from the worker thread to [`camera_writeback`].
/// Tightly-packed pixels in the config's `output_format` (BGRA8 for now).
#[derive(Clone)]
pub struct CameraFrame {
    /// Frame width in px.
    pub width: u32,
    /// Frame height in px.
    pub height: u32,
    /// Tightly-packed pixel bytes (`width * height * 4` for BGRA8).
    pub bytes: Vec<u8>,
}

/// Init data handed to the capture worker thread.
struct CameraThreadInit {
    width: u32,
    height: u32,
}

/// Live state for one camera widget, owned by the node's dataset `RefAny` and
/// carried across relayout by [`merge_camera_state`].
pub struct CameraWidgetState {
    /// The requested capture configuration (the control POD).
    pub config: CameraConfig,
    /// `true` once the capture thread has been started, so a relayout re-mount
    /// doesn't spawn a second one. (Later: the GL-texture handle lives here.)
    pub started: bool,
    /// Most recent frame the worker delivered (widget.3 uploads it to the GL
    /// texture; for now it's just stored to prove the thread→writeback loop).
    pub latest_frame: Option<CameraFrame>,
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
            latest_frame: None,
        };
        let dataset = RefAny::new(state);

        let (w, h) = frame_dims(&self.config);
        let placeholder = ImageRef::null_image(
            w as usize,
            h as usize,
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

/// Frame dimensions for a config (0 → a sane default).
fn frame_dims(config: &CameraConfig) -> (u32, u32) {
    let w = if config.width > 0 { config.width } else { 640 };
    let h = if config.height > 0 { config.height } else { 480 };
    (w, h)
}

/// AfterMount: start the background capture thread exactly once.
extern "C" fn camera_on_after_mount(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let dims = {
        let mut s = match data.downcast_mut::<CameraWidgetState>() {
            Some(s) => s,
            None => return Update::DoNothing,
        };
        if s.started {
            return Update::DoNothing;
        }
        s.started = true;
        frame_dims(&s.config)
    };

    let init = RefAny::new(CameraThreadInit {
        width: dims.0,
        height: dims.1,
    });
    // writeback_data = the same dataset the widget renders from, so the
    // writeback's `latest_frame` write is seen by the (future) GL upload.
    info.add_thread(
        ThreadId::unique(),
        Thread::create(
            init,
            data.clone(),
            ThreadCallback::new(test_pattern_worker),
        ),
    );
    Update::DoNothing
}

/// Background worker (test pattern): emits a colour-cycling solid frame
/// ~30×/s until the widget unmounts (`sender.send` returns `false` once the
/// receiver is dropped). The real AVFoundation/Camera2 capture loop replaces
/// this in widget.4.
extern "C" fn test_pattern_worker(mut init: RefAny, mut sender: ThreadSender, _recv: ThreadReceiver) {
    let (w, h) = init
        .downcast_ref::<CameraThreadInit>()
        .map(|i| (i.width, i.height))
        .unwrap_or((640, 480));
    let px = (w as usize) * (h as usize);
    let mut tick: u32 = 0;
    loop {
        let color = [
            (tick % 256) as u8,
            (tick.wrapping_mul(2) % 256) as u8,
            (tick.wrapping_mul(3) % 256) as u8,
            255u8,
        ];
        let mut bytes = Vec::with_capacity(px * 4);
        for _ in 0..px {
            bytes.extend_from_slice(&color);
        }
        let frame = CameraFrame {
            width: w,
            height: h,
            bytes,
        };
        let sent = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::new(
            WriteBackCallback::new(camera_writeback),
            RefAny::new(frame),
        )));
        if !sent {
            break; // window/widget gone — stop capturing.
        }
        std::thread::sleep(std::time::Duration::from_millis(33));
        tick = tick.wrapping_add(8);
    }
}

/// Writeback (main thread): store the latest frame in the widget's dataset.
/// widget.3 swaps this for a GL-texture upload + `ShouldReRenderCurrentWindow`
/// recomposite (the no-relayout path); for now it proves the loop.
extern "C" fn camera_writeback(
    mut writeback_data: RefAny,
    mut frame_data: RefAny,
    _info: CallbackInfo,
) -> Update {
    if let Some(frame) = frame_data.downcast_ref::<CameraFrame>() {
        if let Some(mut s) = writeback_data.downcast_mut::<CameraWidgetState>() {
            s.latest_frame = Some(frame.clone());
        }
    }
    Update::DoNothing
}

/// Carry the live state forward across relayout: the freshly-built state from
/// `dom()` keeps its (possibly user-updated) config, but inherits the running
/// thread / latest frame / `started` flag from the previous frame's state.
extern "C" fn merge_camera_state(mut new_data: RefAny, mut old_data: RefAny) -> RefAny {
    {
        let new_guard = new_data.downcast_mut::<CameraWidgetState>();
        let old_guard = old_data.downcast_ref::<CameraWidgetState>();
        if let (Some(mut new_g), Some(old_g)) = (new_guard, old_guard) {
            new_g.started = old_g.started;
            new_g.latest_frame = old_g.latest_frame.clone();
        }
    }
    new_data
}
