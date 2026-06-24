//! Screen-capture widget — a "dumb widget" identical in architecture to the
//! [`CameraWidget`](super::camera), only the source differs (a display /
//! window). SUPER_PLAN_2 §4 P6, widget pivot.
//!
//! `ScreenCaptureWidget::create(config).dom()` → an `<img>` a background
//! capture thread keeps fed; each frame goes through
//! [`super::capture_common::present_frame`] (GL-texture install-once /
//! re-upload + recomposite). The shared core lives in `capture_common`; this
//! widget is its config + worker. Test-pattern worker (a moving band) stands
//! in for the real ScreenCaptureKit / MediaProjection / PipeWire worker.

use alloc::vec::Vec;

use azul_core::callbacks::Update;
use azul_core::dom::{ComponentEventFilter, DatasetMergeCallbackType, Dom, EventFilter};
use azul_core::refany::{OptionRefAny, RefAny};
use azul_core::resources::{ImageRef, RawImageFormat};
use azul_core::screencap::ScreenCaptureConfig;
use azul_core::task::{ThreadId, ThreadReceiver};

use azul_core::video::VideoFrame;

use super::capture_common::{
    invoke_on_frame, present_frame, screen_backend, OnVideoFrame, OnVideoFrameCallback,
    OptionOnVideoFrame,
};
use crate::callbacks::{Callback, CallbackInfo, CallbackType};
use crate::thread::{
    Thread, ThreadCallback, ThreadReceiveMsg, ThreadSender, ThreadWriteBackMsg, WriteBackCallback,
};

/// Default capture size for the test pattern (the real backend reports the
/// source's actual size).
const DEFAULT_W: u32 = 1280;
const DEFAULT_H: u32 = 720;

/// Live state for one screencap widget, carried across relayout by
/// [`merge_screencap_state`].
pub struct ScreenCaptureWidgetState {
    /// The requested capture configuration (the control POD).
    pub config: ScreenCaptureConfig,
    /// `true` once the capture thread has been started.
    pub started: bool,
    /// The stable external GL texture id once installed.
    pub gl_texture_id: Option<u32>,
    /// Optional user hook invoked with each captured frame (effects / save /
    /// send). Re-set on every fresh build (see [`merge_screencap_state`]).
    pub on_frame: OptionOnVideoFrame,
}

/// A screen-capture widget. `create(config).dom()` yields an `<img>` the
/// capture thread keeps fed.
#[repr(C)]
pub struct ScreenCaptureWidget {
    /// What to capture + fps + format.
    pub config: ScreenCaptureConfig,
    /// Optional per-frame user hook (effects / save / send - azul-meet).
    pub on_frame: OptionOnVideoFrame,
}

impl ScreenCaptureWidget {
    /// Create a screencap widget for the given config.
    #[must_use] pub const fn create(config: ScreenCaptureConfig) -> Self {
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
        let state = ScreenCaptureWidgetState {
            config: self.config,
            started: false,
            gl_texture_id: None,
            on_frame: self.on_frame,
        };
        let dataset = RefAny::new(state);

        let placeholder = ImageRef::null_image(
            DEFAULT_W as usize,
            DEFAULT_H as usize,
            RawImageFormat::BGRA8,
            b"azul-screencap-placeholder".to_vec(),
        );

        Dom::create_image(placeholder)
            .with_dataset(OptionRefAny::Some(dataset.clone()))
            .with_merge_callback(merge_screencap_state as DatasetMergeCallbackType)
            .with_callback(
                EventFilter::Component(ComponentEventFilter::AfterMount),
                dataset,
                Callback::from(screencap_on_after_mount as CallbackType),
            )
    }
}

/// `AfterMount`: start the background capture thread exactly once.
extern "C" fn screencap_on_after_mount(mut data: RefAny, mut info: CallbackInfo) -> Update {
    {
        let Some(mut s) = data.downcast_mut::<ScreenCaptureWidgetState>() else {
            return Update::DoNothing;
        };
        if s.started {
            return Update::DoNothing;
        }
        s.started = true;
    }
    info.add_thread(
        ThreadId::unique(),
        Thread::create(
            RefAny::new(()),
            data.clone(),
            ThreadCallback::new(screencap_worker),
        ),
    );
    Update::DoNothing
}

/// Background worker (test pattern): a downward-moving white band on dark grey,
/// ~30×/s. Replaced by the real `ScreenCaptureKit` / `MediaProjection` worker.
extern "C" fn screencap_worker(_init: RefAny, mut sender: ThreadSender, _recv: ThreadReceiver) {
    // Real platform capture if the dll registered a screen backend
    // (ScreenCaptureKit / X11 / DXGI; Wayland stays a dummy); else the test pattern.
    if let Some(backend) = screen_backend() {
        let handle = (backend.open)(0, DEFAULT_W, DEFAULT_H);
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
                    WriteBackCallback::new(screencap_writeback),
                    RefAny::new(frame),
                ))) {
                    break;
                }
            }
            (backend.close)(handle);
            return;
        }
    }

    let (w, h) = (DEFAULT_W as usize, DEFAULT_H as usize);
    let mut tick: u32 = 0;
    loop {
        let band = (tick as usize) % h;
        let mut bytes = Vec::with_capacity(w * h * 4);
        for y in 0..h {
            let v = if y.abs_diff(band) < 8 { 235u8 } else { 28u8 };
            for _ in 0..w {
                bytes.extend_from_slice(&[v, v, v, 255]);
            }
        }
        let frame = VideoFrame {
            width: u32::try_from(w).unwrap_or(0),
            height: u32::try_from(h).unwrap_or(0),
            bytes: bytes.into(),
        };
        let sent = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::new(
            WriteBackCallback::new(screencap_writeback),
            RefAny::new(frame),
        )));
        if !sent {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(33));
        tick = tick.wrapping_add(12);
    }
}

/// Writeback (main thread): hand the frame to the shared GL presenter and
/// store the (stable) texture id.
extern "C" fn screencap_writeback(
    mut writeback_data: RefAny,
    mut frame_data: RefAny,
    mut info: CallbackInfo,
) -> Update {
    let (current, hook) = writeback_data.downcast_ref::<ScreenCaptureWidgetState>().map_or_else(|| (None, OptionOnVideoFrame::None), |s| (s.gl_texture_id, s.on_frame.clone()));
    let mut user_update = Update::DoNothing;
    let new_id = match frame_data.downcast_ref::<VideoFrame>() {
        Some(frame) => {
            let id = present_frame(&mut info, writeback_data.clone(), current, &frame);
            user_update = invoke_on_frame(&hook, &mut info, &frame);
            id
        }
        None => return Update::DoNothing,
    };
    if let Some(mut s) = writeback_data.downcast_mut::<ScreenCaptureWidgetState>() {
        s.gl_texture_id = new_id;
    }
    user_update
}

/// Carry live state forward across relayout.
extern "C" fn merge_screencap_state(mut new_data: RefAny, mut old_data: RefAny) -> RefAny {
    {
        let new_guard = new_data.downcast_mut::<ScreenCaptureWidgetState>();
        let old_guard = old_data.downcast_ref::<ScreenCaptureWidgetState>();
        if let (Some(mut new_g), Some(old_g)) = (new_guard, old_guard) {
            new_g.started = old_g.started;
            new_g.gl_texture_id = old_g.gl_texture_id;
        }
    }
    new_data
}
