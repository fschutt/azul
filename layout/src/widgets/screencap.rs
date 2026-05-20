//! Screen-capture widget — a "dumb widget" identical in architecture to the
//! [`CameraWidget`](super::camera), only the source differs (a display /
//! window instead of a camera). SUPER_PLAN_2 §4 P6, widget pivot.
//!
//! On `AfterMount` a background capture thread starts; its writeback uploads
//! each frame into a stable GL-texture `ImageRef` + recomposites
//! (`ShouldReRenderCurrentWindow`) — no relayout, no display-list rebuild, no
//! camera/screencap logic in core. This tick uses a self-contained
//! **test-pattern** worker (a moving horizontal band, no platform deps); the
//! real ScreenCaptureKit / MediaProjection / PipeWire worker swaps in later.
//!
//! (The thread→writeback→GL machinery is duplicated from `camera.rs` for now;
//! once the video widget lands too, the shared core moves to a common module.)

use alloc::vec::Vec;

use azul_core::animation::UpdateImageType;
use azul_core::callbacks::Update;
use azul_core::dom::{ComponentEventFilter, DatasetMergeCallbackType, Dom, EventFilter};
use azul_core::gl::gl::{RGBA, TEXTURE_2D, UNSIGNED_BYTE};
use azul_core::gl::{GlContextPtr, OptionU8VecRef, Texture, U8VecRef};
use azul_core::geom::PhysicalSizeU32;
use azul_core::refany::{OptionRefAny, RefAny};
use azul_core::resources::{ImageRef, RawImageFormat};
use azul_core::screencap::ScreenCaptureConfig;
use azul_core::task::{ThreadId, ThreadReceiver};
use azul_css::props::basic::ColorU;

use crate::callbacks::{Callback, CallbackInfo, CallbackType};
use crate::thread::{
    Thread, ThreadCallback, ThreadReceiveMsg, ThreadSender, ThreadWriteBackMsg, WriteBackCallback,
};

/// Default capture size for the test pattern (the real backend reports the
/// source's actual size).
const DEFAULT_W: u32 = 1280;
const DEFAULT_H: u32 = 720;

/// One captured frame, sent from the worker thread to [`screencap_writeback`].
#[derive(Clone)]
pub struct ScreenFrame {
    /// Frame width in px.
    pub width: u32,
    /// Frame height in px.
    pub height: u32,
    /// Tightly-packed RGBA8 pixel bytes (`width * height * 4`).
    pub bytes: Vec<u8>,
}

/// Live state for one screencap widget, carried across relayout by
/// [`merge_screencap_state`].
pub struct ScreenCaptureWidgetState {
    /// The requested capture configuration (the control POD).
    pub config: ScreenCaptureConfig,
    /// `true` once the capture thread has been started.
    pub started: bool,
    /// Most recent frame (cpurender fallback / debugging).
    pub latest_frame: Option<ScreenFrame>,
    /// The stable external GL texture id once installed.
    pub gl_texture_id: Option<u32>,
}

/// A screen-capture widget. `create(config).dom()` yields an `<img>` the
/// capture thread keeps fed.
#[repr(C)]
pub struct ScreenCaptureWidget {
    /// What to capture + fps + format.
    pub config: ScreenCaptureConfig,
}

impl ScreenCaptureWidget {
    /// Create a screencap widget for the given config.
    pub fn create(config: ScreenCaptureConfig) -> Self {
        Self { config }
    }

    /// Build the widget's DOM: a single `<img>` node, fed by a background
    /// capture thread started on mount.
    pub fn dom(self) -> Dom {
        let state = ScreenCaptureWidgetState {
            config: self.config,
            started: false,
            latest_frame: None,
            gl_texture_id: None,
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

/// AfterMount: start the background capture thread exactly once.
extern "C" fn screencap_on_after_mount(mut data: RefAny, mut info: CallbackInfo) -> Update {
    {
        let mut s = match data.downcast_mut::<ScreenCaptureWidgetState>() {
            Some(s) => s,
            None => return Update::DoNothing,
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
            ThreadCallback::new(screencap_test_worker),
        ),
    );
    Update::DoNothing
}

/// Background worker (test pattern): a downward-moving white band on dark grey,
/// ~30×/s, until the widget unmounts. Replaced by the real ScreenCaptureKit /
/// MediaProjection worker later.
extern "C" fn screencap_test_worker(_init: RefAny, mut sender: ThreadSender, _recv: ThreadReceiver) {
    let (w, h) = (DEFAULT_W as usize, DEFAULT_H as usize);
    let mut tick: u32 = 0;
    loop {
        let band = (tick as usize) % h;
        let mut bytes = Vec::with_capacity(w * h * 4);
        for y in 0..h {
            let on_band = y.abs_diff(band) < 8;
            let v = if on_band { 235u8 } else { 28u8 };
            for _ in 0..w {
                bytes.extend_from_slice(&[v, v, v, 255]);
            }
        }
        let frame = ScreenFrame {
            width: w as u32,
            height: h as u32,
            bytes,
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

/// Writeback (main thread): upload the new frame into the stable external GL
/// texture and recomposite — install once via `change_node_image`, re-upload
/// in place every frame after (no relayout, no DL rebuild). cpurender (no GL)
/// stores the frame. (GL is compile-verified only here.)
extern "C" fn screencap_writeback(
    mut writeback_data: RefAny,
    mut frame_data: RefAny,
    mut info: CallbackInfo,
) -> Update {
    let frame = match frame_data.downcast_ref::<ScreenFrame>() {
        Some(f) => ScreenFrame {
            width: f.width,
            height: f.height,
            bytes: f.bytes.clone(),
        },
        None => return Update::DoNothing,
    };

    let gl = match info.get_gl_context().into_option() {
        Some(g) => g,
        None => {
            if let Some(mut s) = writeback_data.downcast_mut::<ScreenCaptureWidgetState>() {
                s.latest_frame = Some(frame);
            }
            return Update::DoNothing;
        }
    };

    let existing = writeback_data
        .downcast_ref::<ScreenCaptureWidgetState>()
        .and_then(|s| s.gl_texture_id);

    match existing {
        Some(id) => {
            upload_rgba(&gl, id, &frame);
            info.update_all_image_callbacks();
        }
        None => {
            let tex = Texture::allocate_rgba8(
                gl.clone(),
                PhysicalSizeU32 {
                    width: frame.width,
                    height: frame.height,
                },
                ColorU {
                    r: 0,
                    g: 0,
                    b: 0,
                    a: 0,
                },
            );
            let id = tex.texture_id;
            upload_rgba(&gl, id, &frame);
            let image = ImageRef::new_gltexture(tex);
            if let Some(node) = info.get_node_id_of_root_dataset(writeback_data.clone()) {
                if let Some(nid) = node.node.into_crate_internal() {
                    info.change_node_image(node.dom, nid, image, UpdateImageType::Content);
                }
            }
            if let Some(mut s) = writeback_data.downcast_mut::<ScreenCaptureWidgetState>() {
                s.gl_texture_id = Some(id);
            }
        }
    }
    Update::DoNothing
}

/// Upload tightly-packed RGBA8 pixels into the GL texture `texture_id`.
fn upload_rgba(gl: &GlContextPtr, texture_id: u32, frame: &ScreenFrame) {
    gl.bind_texture(TEXTURE_2D, texture_id);
    gl.tex_image_2d(
        TEXTURE_2D,
        0,
        RGBA as i32,
        frame.width as i32,
        frame.height as i32,
        0,
        RGBA,
        UNSIGNED_BYTE,
        OptionU8VecRef::Some(U8VecRef::from(frame.bytes.as_slice())),
    );
}

/// Carry live state forward across relayout (config from the fresh build,
/// thread / texture from the previous frame).
extern "C" fn merge_screencap_state(mut new_data: RefAny, mut old_data: RefAny) -> RefAny {
    {
        let new_guard = new_data.downcast_mut::<ScreenCaptureWidgetState>();
        let old_guard = old_data.downcast_ref::<ScreenCaptureWidgetState>();
        if let (Some(mut new_g), Some(old_g)) = (new_guard, old_guard) {
            new_g.started = old_g.started;
            new_g.latest_frame = old_g.latest_frame.clone();
            new_g.gl_texture_id = old_g.gl_texture_id;
        }
    }
    new_data
}
