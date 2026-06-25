//! Shared core for the "video-ish" widgets (camera / screencap / video).
//!
//! All three are identical in architecture (RefAny dataset + AfterMount
//! background capture/decode thread + writeback that uploads each frame into a
//! stable external GL texture + recomposites). Only the *config* and the
//! *worker* differ. This module holds the duplicated pieces - the [`VideoFrame`]
//! the worker produces and [`present_frame`], the GL writeback core - so each
//! widget is a thin config+worker wrapper and there's a single place for GL
//! fixes + the real platform workers (AVFoundation / ScreenCaptureKit /
//! vk-video) to plug in.
//!
//! NOTE: GL code - compile-verified here; the actual texture rendering must be
//! verified on a machine with a window + GPU.

use azul_core::resources::UpdateImageType;
use azul_core::callbacks::Update;
use azul_core::gl::gl::{RGBA, TEXTURE_2D, UNSIGNED_BYTE};
use azul_core::gl::{GlContextPtr, OptionU8VecRef, Texture, U8VecRef};
use azul_core::geom::PhysicalSizeU32;
use azul_core::refany::RefAny;
use azul_core::resources::ImageRef;
use azul_core::video::VideoFrame;
use azul_css::impl_option_inner; // brought into scope for impl_widget_callback!'s impl_option!
use azul_css::props::basic::ColorU;

use crate::callbacks::CallbackInfo;

/// User hook fired once per captured/decoded frame - the backreference
/// dependency-injection pattern (see `architecture.md`).
///
/// A capture widget's
/// private writeback invokes it with each [`VideoFrame`], so application code
/// can apply effects, save the frame into its own data model, or send it over
/// the network (azul-meet). Returns `Update` like any callback. Wired via
/// `CameraWidget::with_on_frame` / `ScreenCaptureWidget::with_on_frame` /
/// `VideoWidget::with_on_frame`.
pub type OnVideoFrameCallbackType = extern "C" fn(RefAny, CallbackInfo, VideoFrame) -> Update;
impl_widget_callback!(
    OnVideoFrame,
    OptionOnVideoFrame,
    OnVideoFrameCallback,
    OnVideoFrameCallbackType
);

// Host-invoker plumbing for managed-FFI bindings - see core/src/host_invoker.rs.
azul_core::impl_managed_callback! {
    wrapper:        OnVideoFrameCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: ON_VIDEO_FRAME_INVOKER,
    invoker_ty:     AzOnVideoFrameCallbackInvoker,
    thunk_fn:       az_on_video_frame_callback_thunk,
    setter_fn:      AzApp_setOnVideoFrameCallbackInvoker,
    from_handle_fn: AzOnVideoFrameCallback_createFromHostHandle,
    extra_args:     [ frame: VideoFrame ],
}

/// Invoke a capture widget's optional `on_frame` hook with `frame`, returning
/// the user's `Update` (`DoNothing` when no hook is set). Shared by all three
/// capture widgets' writebacks.
pub fn invoke_on_frame(
    hook: &OptionOnVideoFrame,
    info: &mut CallbackInfo,
    frame: &VideoFrame,
) -> Update {
    match hook {
        OptionOnVideoFrame::Some(h) => {
            (h.callback.cb)(h.refany.clone(), *info, frame.clone())
        }
        OptionOnVideoFrame::None => Update::DoNothing,
    }
}

/// Present `frame` for a video-ish widget and return the (stable) GL texture
/// id to store back in the widget's state.
///
/// - First frame (`current_id` is `None`): allocate a GL texture, upload, wrap
///   in an external-texture `ImageRef`, and install it on the widget's node
///   **once** via `change_node_image` (the node is found via
///   `get_node_id_of_root_dataset(dataset)`). Returns `Some(new_id)`.
/// - Every frame after: re-upload into the same texture id + recomposite
///   (`update_all_image_callbacks` -> `ShouldReRenderCurrentWindow`) - no
///   relayout, no display-list rebuild, since the external texture's wr key
///   (= the `ImageRef` data pointer) stays stable. Returns `current_id`.
/// - No GL context (cpurender): returns `current_id` unchanged (a CPU upload
///   path is a follow-up).
pub fn present_frame(
    info: &mut CallbackInfo,
    dataset: RefAny,
    current_id: Option<u32>,
    frame: &VideoFrame,
) -> Option<u32> {
    let Some(gl) = info.get_gl_context().into_option() else {
        return current_id;
    };

    if let Some(id) = current_id {
        upload_rgba(&gl, id, frame);
        info.update_all_image_callbacks();
        Some(id)
    } else {
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
        upload_rgba(&gl, id, frame);
        let image = ImageRef::new_gltexture(tex);
        if let Some(node) = info.get_node_id_of_root_dataset(dataset) {
            if let Some(nid) = node.node.into_crate_internal() {
                info.change_node_image(node.dom, nid, image, UpdateImageType::Content);
            }
        }
        Some(id)
    }
}

/// Upload tightly-packed RGBA8 pixels into the GL texture `texture_id`.
#[allow(clippy::cast_possible_wrap)] // bounded graphics/coord/counter/fixed-point cast
pub fn upload_rgba(gl: &GlContextPtr, texture_id: u32, frame: &VideoFrame) {
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
        OptionU8VecRef::Some(U8VecRef::from(frame.bytes.as_ref())),
    );
}

/// A platform frame-capture backend (camera / screen), registered by the dll at
/// startup so the cross-platform capture widgets can pull **real** frames
/// instead of their built-in test pattern.
///
/// The dll provides one per OS (v4l2 on
/// Linux, `AVFoundation` on macOS, Media Foundation on Windows, `ScreenCaptureKit` /
/// `PipeWire` / DXGI for screens, ...). These are plain Rust fn pointers - the dll
/// links azul-layout statically, so registering + calling is a Rust-to-Rust
/// call, no `extern "C"`/trait-object dance.
#[derive(Debug, Clone, Copy)]
pub struct CaptureVTable {
    /// Open source `index` (camera device / display index) at the requested
    /// `width` x `height`. Returns an opaque handle, or `0` on failure (the
    /// worker then falls back to the test pattern).
    pub open: fn(index: u32, width: u32, height: u32) -> u64,
    /// Block for the next frame, writing tightly-packed RGBA8 into `out`
    /// (resized as needed). Returns the actual frame `(width, height)`, or
    /// `(0, 0)` on end-of-stream / error (the worker then stops + closes).
    pub read: fn(handle: u64, out: &mut Vec<u8>) -> (u32, u32),
    /// Close + free the source.
    pub close: fn(handle: u64),
}

static CAMERA_BACKEND: std::sync::OnceLock<CaptureVTable> = std::sync::OnceLock::new();
static SCREEN_BACKEND: std::sync::OnceLock<CaptureVTable> = std::sync::OnceLock::new();

/// Register the platform **camera** capture backend (called once by the dll at
/// startup; the first registration wins). Without it, `CameraWidget` shows its
/// test pattern.
pub fn register_camera_backend(vtable: CaptureVTable) {
    let _ = CAMERA_BACKEND.set(vtable);
}

/// Register the platform **screen** capture backend (for `ScreenCaptureWidget`).
pub fn register_screen_backend(vtable: CaptureVTable) {
    let _ = SCREEN_BACKEND.set(vtable);
}

/// The registered camera backend, if the dll provided one for this platform.
pub fn camera_backend() -> Option<CaptureVTable> {
    CAMERA_BACKEND.get().copied()
}

/// The registered screen-capture backend, if any.
pub fn screen_backend() -> Option<CaptureVTable> {
    SCREEN_BACKEND.get().copied()
}

/// A platform **audio**-capture backend (microphone), registered by the dll so
/// `MicrophoneWidget` can pull real samples instead of the test tone.
///
/// Like
/// [`CaptureVTable`] but yields interleaved `f32` audio rather than RGBA video.
#[derive(Debug, Clone, Copy)]
pub struct AudioCaptureVTable {
    /// Open the default mic at `sample_rate` x `channels`. Opaque handle, or
    /// `0` on failure.
    pub open: fn(sample_rate: u32, channels: u16) -> u64,
    /// Block for the next chunk, writing interleaved `f32` into `out` (resized).
    /// Returns the frame count (`out.len() / channels`), or `0` on error / EOF
    /// (the worker then stops + closes).
    pub read: fn(handle: u64, out: &mut Vec<f32>) -> u32,
    /// Close + free the source.
    pub close: fn(handle: u64),
}

static MIC_BACKEND: std::sync::OnceLock<AudioCaptureVTable> = std::sync::OnceLock::new();

/// Register the platform microphone-capture backend (called once by the dll).
pub fn register_mic_backend(vtable: AudioCaptureVTable) {
    let _ = MIC_BACKEND.set(vtable);
}

/// The registered mic-capture backend, if the dll provided one for this platform.
pub fn mic_backend() -> Option<AudioCaptureVTable> {
    MIC_BACKEND.get().copied()
}
