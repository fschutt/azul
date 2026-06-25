//! Camera-preview widget - a "dumb widget" (like [`MapWidget`](super::map))
//! that owns a background capture thread + a GL-texture `ImageRef`, with **no**
//! camera-specific logic in the core framework (SUPER_PLAN_2 §4 P6, widget
//! pivot - see the MASTER PLAN in `MOBILE_SESSION_LOG.md`).
//!
//! `CameraWidget::create(config).dom()` -> a static `<img>` whose pixels a
//! background thread keeps fed. On `AfterMount` the capture thread starts
//! (`CallbackInfo::add_thread`); each frame goes through
//! [`super::capture_common::present_frame`], which uploads it into a stable
//! external GL texture + recomposites - no relayout, no display-list rebuild.
//! The shared thread/writeback/GL core lives in `capture_common`; this widget
//! is just its config + worker.
//!
//! This tick uses a self-contained **test-pattern** worker (colour cycle, no
//! platform deps); the real AVFoundation/Camera2 worker (dll-side) swaps in
//! later.

use alloc::vec::Vec;

use azul_core::callbacks::Update;
use azul_core::camera::CameraConfig;
use azul_core::dom::{ComponentEventFilter, DatasetMergeCallbackType, Dom, EventFilter};
use azul_core::refany::{OptionRefAny, RefAny};
use azul_core::resources::{ImageRef, RawImageFormat};
use azul_core::task::{ThreadId, ThreadReceiver};

use azul_core::video::VideoFrame;

use super::capture_common::{
    camera_backend, invoke_on_frame, present_frame, OnVideoFrame, OnVideoFrameCallback,
    OptionOnVideoFrame,
};
use crate::callbacks::{Callback, CallbackInfo, CallbackType};
use crate::thread::{
    Thread, ThreadCallback, ThreadReceiveMsg, ThreadSender, ThreadWriteBackMsg, WriteBackCallback,
};

/// Init data handed to the capture worker thread.
struct CameraThreadInit {
    width: u32,
    height: u32,
}

/// Live state for one camera widget, carried across relayout by
/// [`merge_camera_state`].
#[derive(Debug)]
pub struct CameraWidgetState {
    /// The requested capture configuration (the control POD).
    pub config: CameraConfig,
    /// `true` once the capture thread has been started.
    pub started: bool,
    /// The stable external GL texture id once the first frame installed it.
    pub gl_texture_id: Option<u32>,
    /// Optional user hook invoked with each captured frame (effects / save /
    /// send). Re-set on every fresh build (see [`merge_camera_state`]).
    pub on_frame: OptionOnVideoFrame,
}

/// A camera-preview widget. `create(config).dom()` yields an `<img>` the
/// capture thread keeps fed.
#[repr(C)]
#[derive(Debug)]
pub struct CameraWidget {
    /// Requested capture config (camera facing, resolution, fps, format).
    pub config: CameraConfig,
    /// Optional per-frame user hook (effects / save / send - azul-meet).
    pub on_frame: OptionOnVideoFrame,
}

impl CameraWidget {
    /// Create a camera widget for the given capture config.
    #[must_use] pub const fn create(config: CameraConfig) -> Self {
        Self {
            config,
            on_frame: OptionOnVideoFrame::None,
        }
    }

    /// Set a hook invoked with every captured frame - for live effects, saving
    /// frames into your data model, or sending them over the network
    /// (azul-meet). The backreference DI pattern (see `architecture.md`).
    pub fn set_on_frame<C: Into<OnVideoFrameCallback>>(&mut self, data: RefAny, on_frame: C) {
        self.on_frame = Some(OnVideoFrame {
            refany: data,
            callback: on_frame.into(),
        })
        .into();
    }

    /// Builder form of [`set_on_frame`](Self::set_on_frame).
    #[must_use]
    pub fn with_on_frame<C: Into<OnVideoFrameCallback>>(
        mut self,
        data: RefAny,
        on_frame: C,
    ) -> Self {
        self.set_on_frame(data, on_frame);
        self
    }

    /// Build the widget's DOM: a single `<img>` node, fed by a background
    /// capture thread started on mount.
    #[must_use] pub fn dom(self) -> Dom {
        let state = CameraWidgetState {
            config: self.config,
            started: false,
            gl_texture_id: None,
            on_frame: self.on_frame,
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
            .with_merge_callback(azul_core::dom::DatasetMergeCallback::from_ptr(merge_camera_state))
            .with_callback(
                EventFilter::Component(ComponentEventFilter::AfterMount),
                dataset,
                Callback::from_ptr(camera_on_after_mount),
            )
    }
}

/// Frame dimensions for a config (0 -> a sane default).
const fn frame_dims(config: &CameraConfig) -> (u32, u32) {
    let w = if config.width > 0 { config.width } else { 640 };
    let h = if config.height > 0 { config.height } else { 480 };
    (w, h)
}

/// `AfterMount`: start the background capture thread exactly once.
extern "C" fn camera_on_after_mount(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let dims = {
        let Some(mut s) = data.downcast_mut::<CameraWidgetState>() else {
            return Update::DoNothing;
        };
        if s.started {
            return Update::DoNothing;
        }
        s.started = true;
        frame_dims(&s.config)
    };

    info.add_thread(
        ThreadId::unique(),
        Thread::create(
            RefAny::new(CameraThreadInit {
                width: dims.0,
                height: dims.1,
            }),
            data.clone(),
            ThreadCallback::new(camera_worker),
        ),
    );
    Update::DoNothing
}

/// Background worker (test pattern): a colour-cycling solid frame ~30x/s until
/// the widget unmounts. The real AVFoundation/Camera2 capture loop replaces it.
#[allow(clippy::cast_possible_truncation)] // bounded graphics/coord/counter/fixed-point cast
extern "C" fn camera_worker(mut init: RefAny, mut sender: ThreadSender, _recv: ThreadReceiver) {
    let (w, h) = init
        .downcast_ref::<CameraThreadInit>()
        .map_or((640, 480), |i| (i.width, i.height));

    // Real platform capture if the dll registered a camera backend (v4l2 /
    // AVFoundation / Media Foundation); otherwise the colour-cycle test pattern.
    if let Some(backend) = camera_backend() {
        let handle = (backend.open)(0, w, h);
        if handle != 0 {
            let mut buf: alloc::vec::Vec<u8> = alloc::vec::Vec::new();
            loop {
                let (fw, fh) = (backend.read)(handle, &mut buf);
                if fw == 0 || fh == 0 {
                    break;
                }
                let frame = VideoFrame {
                    width: fw,
                    height: fh,
                    bytes: buf.clone().into(),
                };
                if !sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::new(
                    WriteBackCallback::new(camera_writeback),
                    RefAny::new(frame),
                ))) {
                    break;
                }
            }
            (backend.close)(handle);
            return;
        }
    }

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
        let frame = VideoFrame {
            width: w,
            height: h,
            bytes: bytes.into(),
        };
        let sent = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::new(
            WriteBackCallback::new(camera_writeback),
            RefAny::new(frame),
        )));
        if !sent {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(33));
        tick = tick.wrapping_add(8);
    }
}

/// Writeback (main thread): hand the frame to the shared GL presenter and
/// store the (stable) texture id back in the widget's state.
extern "C" fn camera_writeback(
    mut writeback_data: RefAny,
    mut frame_data: RefAny,
    mut info: CallbackInfo,
) -> Update {
    let (current, hook) = writeback_data.downcast_ref::<CameraWidgetState>().map_or_else(|| (None, OptionOnVideoFrame::None), |s| (s.gl_texture_id, s.on_frame.clone()));
    let mut user_update = Update::DoNothing;
    let new_id = match frame_data.downcast_ref::<VideoFrame>() {
        Some(frame) => {
            let id = present_frame(&mut info, writeback_data.clone(), current, &frame);
            user_update = invoke_on_frame(&hook, &mut info, &frame);
            id
        }
        None => return Update::DoNothing,
    };
    if let Some(mut s) = writeback_data.downcast_mut::<CameraWidgetState>() {
        s.gl_texture_id = new_id;
    }
    user_update
}

/// Carry live state forward across relayout (config from the fresh build,
/// thread / texture from the previous frame).
extern "C" fn merge_camera_state(mut new_data: RefAny, mut old_data: RefAny) -> RefAny {
    {
        let new_guard = new_data.downcast_mut::<CameraWidgetState>();
        let old_guard = old_data.downcast_ref::<CameraWidgetState>();
        if let (Some(mut new_g), Some(old_g)) = (new_guard, old_guard) {
            new_g.started = old_g.started;
            new_g.gl_texture_id = old_g.gl_texture_id;
        }
    }
    new_data
}
