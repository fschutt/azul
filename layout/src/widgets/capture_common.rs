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

use azul_core::animation::UpdateImageType;
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
/// dependency-injection pattern (see `architecture.md`). A capture widget's
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
            (h.callback.cb)(h.refany.clone(), info.clone(), frame.clone())
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
    let gl = match info.get_gl_context().into_option() {
        Some(g) => g,
        None => return current_id,
    };

    match current_id {
        Some(id) => {
            upload_rgba(&gl, id, frame);
            info.update_all_image_callbacks();
            Some(id)
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
}

/// Upload tightly-packed RGBA8 pixels into the GL texture `texture_id`.
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
