//! Video-playback widget — a "dumb widget" identical in architecture to the
//! [`CameraWidget`](super::camera) / [`ScreenCaptureWidget`](super::screencap),
//! only the source differs (a video URL/file decoded via vk-video).
//! SUPER_PLAN_2 §4 P6, widget pivot.
//!
//! On `AfterMount` a background decode thread starts; its writeback uploads
//! each decoded frame into a stable GL-texture `ImageRef` + recomposites — no
//! relayout, no display-list rebuild. This tick uses a self-contained
//! **test-pattern** worker (shifting SMPTE-style colour bars, no platform
//! deps); the real vk-video decode + HTTP-range fetch worker swaps in later.
//!
//! (Thread/writeback/GL machinery duplicated from camera/screencap for now;
//! the shared core moves to a common module in the DRY pass.)

use alloc::vec::Vec;

use azul_core::animation::UpdateImageType;
use azul_core::callbacks::Update;
use azul_core::dom::{ComponentEventFilter, DatasetMergeCallbackType, Dom, EventFilter};
use azul_core::gl::gl::{RGBA, TEXTURE_2D, UNSIGNED_BYTE};
use azul_core::gl::{GlContextPtr, OptionU8VecRef, Texture, U8VecRef};
use azul_core::geom::PhysicalSizeU32;
use azul_core::refany::{OptionRefAny, RefAny};
use azul_core::resources::{ImageRef, RawImageFormat};
use azul_core::task::{ThreadId, ThreadReceiver};
use azul_core::video::VideoConfig;
use azul_css::props::basic::ColorU;

use crate::callbacks::{Callback, CallbackInfo, CallbackType};
use crate::thread::{
    Thread, ThreadCallback, ThreadReceiveMsg, ThreadSender, ThreadWriteBackMsg, WriteBackCallback,
};

/// Default decode size for the test pattern (the real decoder reports the
/// stream's actual size).
const DEFAULT_W: u32 = 1280;
const DEFAULT_H: u32 = 720;

/// One decoded frame, sent from the worker thread to [`video_writeback`].
#[derive(Clone)]
pub struct VideoFrame {
    /// Frame width in px.
    pub width: u32,
    /// Frame height in px.
    pub height: u32,
    /// Tightly-packed RGBA8 pixel bytes (`width * height * 4`).
    pub bytes: Vec<u8>,
}

/// Live state for one video widget, carried across relayout by
/// [`merge_video_state`].
pub struct VideoWidgetState {
    /// The requested playback configuration (source + autoplay/loop).
    pub config: VideoConfig,
    /// `true` once the decode thread has been started.
    pub started: bool,
    /// Most recent frame (cpurender fallback / debugging).
    pub latest_frame: Option<VideoFrame>,
    /// The stable external GL texture id once installed.
    pub gl_texture_id: Option<u32>,
}

/// A video-playback widget. `create(config).dom()` yields an `<img>` the
/// decode thread keeps fed.
#[repr(C)]
pub struct VideoWidget {
    /// Source URL + autoplay/loop + format.
    pub config: VideoConfig,
}

impl VideoWidget {
    /// Create a video widget for the given config.
    pub fn create(config: VideoConfig) -> Self {
        Self { config }
    }

    /// Build the widget's DOM: a single `<img>` node, fed by a background
    /// decode thread started on mount.
    pub fn dom(self) -> Dom {
        let state = VideoWidgetState {
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
            b"azul-video-placeholder".to_vec(),
        );

        Dom::create_image(placeholder)
            .with_dataset(OptionRefAny::Some(dataset.clone()))
            .with_merge_callback(merge_video_state as DatasetMergeCallbackType)
            .with_callback(
                EventFilter::Component(ComponentEventFilter::AfterMount),
                dataset,
                Callback::from(video_on_after_mount as CallbackType),
            )
    }
}

/// AfterMount: start the background decode thread exactly once.
extern "C" fn video_on_after_mount(mut data: RefAny, mut info: CallbackInfo) -> Update {
    {
        let mut s = match data.downcast_mut::<VideoWidgetState>() {
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
            ThreadCallback::new(video_test_worker),
        ),
    );
    Update::DoNothing
}

/// Background worker (test pattern): SMPTE-style colour bars scrolling
/// horizontally ~30×/s, until the widget unmounts. Replaced by the real
/// vk-video decode worker later.
extern "C" fn video_test_worker(_init: RefAny, mut sender: ThreadSender, _recv: ThreadReceiver) {
    const BARS: [[u8; 3]; 7] = [
        [235, 235, 235],
        [235, 235, 16],
        [16, 235, 235],
        [16, 235, 16],
        [235, 16, 235],
        [235, 16, 16],
        [16, 16, 235],
    ];
    let (w, h) = (DEFAULT_W as usize, DEFAULT_H as usize);
    let mut tick: u32 = 0;
    loop {
        let shift = (tick as usize / 4) % 7;
        let mut bytes = Vec::with_capacity(w * h * 4);
        for _y in 0..h {
            for x in 0..w {
                let c = BARS[((x * 7 / w) + shift) % 7];
                bytes.extend_from_slice(&[c[0], c[1], c[2], 255]);
            }
        }
        let frame = VideoFrame {
            width: w as u32,
            height: h as u32,
            bytes,
        };
        let sent = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::new(
            WriteBackCallback::new(video_writeback),
            RefAny::new(frame),
        )));
        if !sent {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(33));
        tick = tick.wrapping_add(2);
    }
}

/// Writeback (main thread): upload the decoded frame into the stable external
/// GL texture and recomposite — install once via `change_node_image`,
/// re-upload in place every frame after (no relayout, no DL rebuild).
/// cpurender (no GL) stores the frame. (GL is compile-verified only here.)
extern "C" fn video_writeback(
    mut writeback_data: RefAny,
    mut frame_data: RefAny,
    mut info: CallbackInfo,
) -> Update {
    let frame = match frame_data.downcast_ref::<VideoFrame>() {
        Some(f) => VideoFrame {
            width: f.width,
            height: f.height,
            bytes: f.bytes.clone(),
        },
        None => return Update::DoNothing,
    };

    let gl = match info.get_gl_context().into_option() {
        Some(g) => g,
        None => {
            if let Some(mut s) = writeback_data.downcast_mut::<VideoWidgetState>() {
                s.latest_frame = Some(frame);
            }
            return Update::DoNothing;
        }
    };

    let existing = writeback_data
        .downcast_ref::<VideoWidgetState>()
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
            if let Some(mut s) = writeback_data.downcast_mut::<VideoWidgetState>() {
                s.gl_texture_id = Some(id);
            }
        }
    }
    Update::DoNothing
}

/// Upload tightly-packed RGBA8 pixels into the GL texture `texture_id`.
fn upload_rgba(gl: &GlContextPtr, texture_id: u32, frame: &VideoFrame) {
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

/// Carry live state forward across relayout.
extern "C" fn merge_video_state(mut new_data: RefAny, mut old_data: RefAny) -> RefAny {
    {
        let new_guard = new_data.downcast_mut::<VideoWidgetState>();
        let old_guard = old_data.downcast_ref::<VideoWidgetState>();
        if let (Some(mut new_g), Some(old_g)) = (new_guard, old_guard) {
            new_g.started = old_g.started;
            new_g.latest_frame = old_g.latest_frame.clone();
            new_g.gl_texture_id = old_g.gl_texture_id;
        }
    }
    new_data
}
