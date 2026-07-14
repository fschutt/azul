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
/// - No GL context (cpurender): installs the frame as a **raw RGBA
///   `ImageRef`** on the node every frame (same per-frame `new_rawimage` the
///   `VideoWidget` CPU path uses) - heavier than the GL re-upload (new image
///   resource per frame) but the tile shows live frames on both renderers.
pub fn present_frame(
    info: &mut CallbackInfo,
    dataset: RefAny,
    current_id: Option<u32>,
    frame: &VideoFrame,
) -> Option<u32> {
    use azul_core::resources::{RawImage, RawImageData, RawImageFormat};

    let Some(gl) = info.get_gl_context().into_option() else {
        // CPU renderer: swap the node's image content for this frame.
        if let Some(img) = ImageRef::new_rawimage(RawImage {
            pixels: RawImageData::U8(frame.bytes.clone()),
            width: frame.width as usize,
            height: frame.height as usize,
            premultiplied_alpha: false,
            data_format: RawImageFormat::RGBA8,
            tag: b"azul-capture-frame".to_vec().into(),
        }) {
            if let Some(node) = info.get_node_id_of_root_dataset(dataset) {
                if let Some(nid) = node.node.into_crate_internal() {
                    info.change_node_image(node.dom, nid, img, UpdateImageType::Content);
                }
            }
        }
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

#[cfg(test)]
#[allow(clippy::too_many_lines)] // table-driven cases; splitting them hides the case list
mod autotest_generated {
    use std::{
        collections::{BTreeMap, HashMap},
        panic::{catch_unwind, AssertUnwindSafe},
        rc::Rc,
        sync::{Arc, Mutex, PoisonError},
    };

    use azul_core::{
        dom::{Dom, DomId, DomNodeId, NodeId, NodeType},
        geom::{LogicalRect, OptionLogicalPosition},
        gl::{GenericGlContext, OptionGlContextPtr, GLvoid},
        hit_test::ScrollPosition,
        refany::OptionRefAny,
        resources::{DecodedImage, RendererResources},
        styled_dom::{NodeHierarchyItemId, StyledDom},
        window::{MonitorVec, RawWindowHandle, RendererType},
    };
    use azul_css::system::SystemStyle;
    use rust_fontconfig::FcFontCache;

    use super::*;
    #[cfg(feature = "icu")]
    use crate::icu::IcuLocalizerHandle;
    use crate::{
        callbacks::{CallbackChange, CallbackInfoRefData, ExternalSystemCallbacks},
        solver3::{display_list::DisplayList, layout_tree::LayoutTree},
        window::{DomLayoutResult, LayoutWindow},
        window_state::FullWindowState,
    };

    // ------------------------------------------------------------------
    // Fake GL drivers
    //
    // Every field of `GenericGlContext` is a `*mut c_void` entry point, and
    // gl-context-loader null-checks each one before transmuting + calling it
    // (returning a default instead). So an all-zero context is a SAFE no-op
    // "driver never loaded" GL, and a context with only the three entry points
    // this module actually uses filled in is a safe *recording* driver: we can
    // observe exactly which GL calls `upload_rgba` / `present_frame` emit, with
    // which arguments, entirely off-GPU.
    // ------------------------------------------------------------------

    /// The texture name the recording driver hands out from `glGenTextures`.
    const RECORDED_TEXTURE_ID: u32 = 42;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum GlCall {
        GenTextures {
            n: i32,
        },
        BindTexture {
            target: u32,
            texture: u32,
        },
        TexImage2d {
            target: u32,
            level: i32,
            internal_format: i32,
            width: i32,
            height: i32,
            border: i32,
            format: u32,
            ty: u32,
            /// `false` = the `NULL` pixel pointer `Texture::allocate_rgba8` uses,
            /// `true` = a real pixel upload (what `upload_rgba` does).
            has_pixels: bool,
        },
    }

    static GL_LOG: Mutex<Vec<GlCall>> = Mutex::new(Vec::new());
    /// Serializes the tests that use the (process-global) recording driver.
    static GL_SERIAL: Mutex<()> = Mutex::new(());

    fn gl_log_push(call: GlCall) {
        GL_LOG
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(call);
    }

    extern "system" fn rec_gen_textures(n: i32, out: *mut u32) {
        gl_log_push(GlCall::GenTextures { n });
        // The caller (gl-context-loader) always passes a `Vec<GLuint>` of len `n`.
        for i in 0..n.max(0) {
            // SAFETY: `out` addresses `n` writable `GLuint`s (a `vec![0; n]`).
            unsafe { out.add(i as usize).write(RECORDED_TEXTURE_ID + i as u32) };
        }
    }

    extern "system" fn rec_bind_texture(target: u32, texture: u32) {
        gl_log_push(GlCall::BindTexture { target, texture });
    }

    #[allow(clippy::too_many_arguments)] // must mirror glTexImage2D exactly
    extern "system" fn rec_tex_image_2d(
        target: u32,
        level: i32,
        internal_format: i32,
        width: i32,
        height: i32,
        border: i32,
        format: u32,
        ty: u32,
        pixels: *const GLvoid,
    ) {
        gl_log_push(GlCall::TexImage2d {
            target,
            level,
            internal_format,
            width,
            height,
            border,
            format,
            ty,
            has_pixels: !pixels.is_null(),
        });
    }

    /// A GL context whose entry points are all `NULL` (driver never loaded).
    fn null_gl_context() -> GlContextPtr {
        // SAFETY: every field of `GenericGlContext` is a raw pointer, for which
        // the all-zero (NULL) bit pattern is valid.
        let ctx: GenericGlContext = unsafe { core::mem::zeroed() };
        GlContextPtr::new(RendererType::Software, Rc::new(ctx))
    }

    /// A GL context that records the calls this module makes (and nothing else:
    /// `glTexParameteri` / `glGetIntegerv` / `glDeleteTextures` stay NULL, i.e.
    /// safe no-ops).
    fn recording_gl_context() -> GlContextPtr {
        // SAFETY: as above — NULL is a valid value for every field; the three we
        // overwrite get fn pointers with exactly the signatures gl-context-loader
        // transmutes them back to.
        let mut ctx: GenericGlContext = unsafe { core::mem::zeroed() };
        ctx.glGenTextures = rec_gen_textures as *const () as *mut azul_core::gl::c_void;
        ctx.glBindTexture = rec_bind_texture as *const () as *mut azul_core::gl::c_void;
        ctx.glTexImage2D = rec_tex_image_2d as *const () as *mut azul_core::gl::c_void;
        GlContextPtr::new(RendererType::Software, Rc::new(ctx))
    }

    /// Runs `f` against the recording driver and returns the GL calls it made.
    fn with_recorded_gl<R>(f: impl FnOnce(GlContextPtr) -> R) -> (R, Vec<GlCall>) {
        let _serial = GL_SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
        GL_LOG
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
        let out = f(recording_gl_context());
        let log = GL_LOG
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        (out, log)
    }

    // ------------------------------------------------------------------
    // CallbackInfo harness (mirrors the other widget test modules)
    // ------------------------------------------------------------------

    /// A `DomLayoutResult` with an *empty* layout tree: the code under test only
    /// walks `styled_dom.node_data`, so no real layout (and no font) is needed.
    fn layout_result(styled_dom: StyledDom) -> DomLayoutResult {
        DomLayoutResult {
            styled_dom,
            layout_tree: LayoutTree {
                nodes: Vec::new(),
                warm: Vec::new(),
                cold: Vec::new(),
                root: 0,
                dom_to_layout: BTreeMap::new(),
                children_arena: Vec::new(),
                children_offsets: Vec::new(),
                subtree_needs_intrinsic: Vec::new(),
            },
            calculated_positions: Vec::new(),
            viewport: LogicalRect::zero(),
            display_list: DisplayList::default(),
            scroll_ids: HashMap::new(),
            scroll_id_to_node_id: HashMap::new(),
        }
    }

    /// Invokes `f` with a `CallbackInfo` over a window holding `styled` (or no
    /// layout results at all, when `styled` is `None`) and the given GL context.
    /// Returns `f`'s value plus every `CallbackChange` the callback recorded.
    fn with_callback_info<R>(
        styled: Option<StyledDom>,
        gl_context: OptionGlContextPtr,
        f: impl FnOnce(&mut CallbackInfo) -> R,
    ) -> (R, Vec<CallbackChange>) {
        let mut layout_window =
            LayoutWindow::new(FcFontCache::default()).expect("LayoutWindow::new failed");
        if let Some(sd) = styled {
            layout_window
                .layout_results
                .insert(DomId::ROOT_ID, layout_result(sd));
        }

        let renderer_resources = RendererResources::default();
        let previous_window_state: Option<FullWindowState> = None;
        let current_window_state = FullWindowState::default();
        let scroll_states: BTreeMap<DomId, BTreeMap<NodeHierarchyItemId, ScrollPosition>> =
            BTreeMap::new();
        let window_handle = RawWindowHandle::Unsupported;
        let system_callbacks = ExternalSystemCallbacks::rust_internal();

        let ref_data = CallbackInfoRefData {
            layout_window: &layout_window,
            renderer_resources: &renderer_resources,
            previous_window_state: &previous_window_state,
            current_window_state: &current_window_state,
            gl_context: &gl_context,
            current_scroll_manager: &scroll_states,
            current_window_handle: &window_handle,
            system_callbacks: &system_callbacks,
            system_style: Arc::new(SystemStyle::default()),
            monitors: Arc::new(Mutex::new(MonitorVec::from_const_slice(&[]))),
            #[cfg(feature = "icu")]
            icu_localizer: IcuLocalizerHandle::default(),
            ctx: OptionRefAny::None,
        };

        let changes: Arc<Mutex<Vec<CallbackChange>>> = Arc::new(Mutex::new(Vec::new()));

        let mut info = CallbackInfo::new(
            &ref_data,
            &changes,
            DomNodeId {
                dom: DomId::ROOT_ID,
                node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(0))),
            },
            OptionLogicalPosition::None,
            OptionLogicalPosition::None,
        );

        let out = f(&mut info);
        let recorded = core::mem::take(&mut *changes.lock().expect("change log poisoned"));
        (out, recorded)
    }

    // ------------------------------------------------------------------
    // Fixtures
    // ------------------------------------------------------------------

    /// The dataset type a capture widget stores on its node.
    #[derive(Debug, Default)]
    struct CamState {
        _texture_id: Option<u32>,
    }

    /// A *different* dataset type, to prove the node lookup is type-scoped.
    #[derive(Debug, Default)]
    struct OtherState {
        _unused: u8,
    }

    /// A `div`, carrying `ds` as its dataset when there is one.
    fn div_with(ds: Option<RefAny>) -> Dom {
        let d = Dom::create_node(NodeType::Div);
        match ds {
            Some(r) => d.with_dataset(OptionRefAny::Some(r)),
            None => d,
        }
    }

    /// `body(0) -> div(1) -> div(2)`, where a `Some(ds)` gives that div a dataset.
    fn dom_with_datasets(first: Option<RefAny>, second: Option<RefAny>) -> StyledDom {
        let dom = Dom::create_node(NodeType::Body)
            .with_child(div_with(first))
            .with_child(div_with(second));
        let styled = StyledDom::create_from_dom(dom);
        assert_eq!(
            styled.node_hierarchy.as_ref().len(),
            3,
            "fixture must flatten to exactly body + 2 divs"
        );
        styled
    }

    /// A `width` x `height` RGBA8 frame with a deterministic (tightly-packed) ramp.
    /// Only ever called with tiny dimensions — `width * height * 4` is allocated.
    fn frame(width: u32, height: u32) -> VideoFrame {
        let len = (width as usize) * (height as usize) * 4;
        let bytes: Vec<u8> = (0..len).map(|i| (i % 251) as u8).collect();
        VideoFrame::new(width, height, bytes.into())
    }

    /// A frame whose *declared* dimensions need not match its byte count.
    fn frame_raw(width: u32, height: u32, bytes: Vec<u8>) -> VideoFrame {
        VideoFrame::new(width, height, bytes.into())
    }

    /// Every image installed on a node, as `(dom, node index, image, update type)`.
    fn image_installs(
        changes: &[CallbackChange],
    ) -> Vec<(DomId, usize, &ImageRef, UpdateImageType)> {
        changes
            .iter()
            .filter_map(|c| match c {
                CallbackChange::ChangeNodeImage {
                    dom_id,
                    node_id,
                    image,
                    update_type,
                } => Some((*dom_id, node_id.index(), image, *update_type)),
                _ => None,
            })
            .collect()
    }

    /// How many "recomposite, don't relayout" requests the callback made.
    fn recomposites(changes: &[CallbackChange]) -> usize {
        changes
            .iter()
            .filter(|c| matches!(c, CallbackChange::UpdateAllImageCallbacks))
            .count()
    }

    // ==================================================================
    // invoke_on_frame
    // ==================================================================

    /// Payload of the `on_frame` hook: records every frame it is handed.
    #[derive(Debug)]
    struct HookLog {
        seen: Vec<(u32, u32, usize, Option<u8>)>,
        reply: Update,
    }

    extern "C" fn hook_record(mut data: RefAny, _: CallbackInfo, frame: VideoFrame) -> Update {
        let mut reply = Update::DoNothing;
        if let Some(mut log) = data.downcast_mut::<HookLog>() {
            let bytes = frame.bytes.as_ref();
            log.seen
                .push((frame.width, frame.height, bytes.len(), bytes.first().copied()));
            reply = log.reply;
        }
        reply
    }

    /// A hook that writes through the `CallbackInfo` it was handed (by value).
    extern "C" fn hook_recomposite(_: RefAny, mut info: CallbackInfo, _: VideoFrame) -> Update {
        info.update_all_image_callbacks();
        Update::RefreshDomAllWindows
    }

    fn hook(cb: OnVideoFrameCallbackType, data: RefAny) -> OptionOnVideoFrame {
        OptionOnVideoFrame::Some(OnVideoFrame {
            refany: data,
            callback: cb.into(),
        })
    }

    fn hook_seen(data: &mut RefAny) -> Vec<(u32, u32, usize, Option<u8>)> {
        data.downcast_ref::<HookLog>()
            .expect("payload must still be a HookLog")
            .seen
            .clone()
    }

    #[test]
    fn invoke_on_frame_without_a_hook_is_do_nothing_and_touches_nothing() {
        let (update, changes) = with_callback_info(None, OptionGlContextPtr::None, |info| {
            invoke_on_frame(&OptionOnVideoFrame::None, info, &frame(2, 2))
        });
        assert_eq!(
            update,
            Update::DoNothing,
            "an unset on_frame hook must be a no-op"
        );
        assert!(
            changes.is_empty(),
            "an unset hook must not record any change, got {changes:?}"
        );
    }

    #[test]
    fn invoke_on_frame_returns_the_hooks_update_verbatim() {
        for reply in [
            Update::DoNothing,
            Update::RefreshDom,
            Update::RefreshDomAllWindows,
        ] {
            let data = RefAny::new(HookLog {
                seen: Vec::new(),
                reply,
            });
            let h = hook(hook_record, data);
            let (update, _) = with_callback_info(None, OptionGlContextPtr::None, |info| {
                invoke_on_frame(&h, info, &frame(1, 1))
            });
            assert_eq!(
                update, reply,
                "invoke_on_frame must return the user's Update unchanged"
            );
        }
    }

    #[test]
    fn invoke_on_frame_forwards_every_frame_into_the_hooks_shared_refany() {
        let mut data = RefAny::new(HookLog {
            seen: Vec::new(),
            reply: Update::RefreshDom,
        });
        let h = hook(hook_record, data.clone());

        // The hook is handed a *clone* of its RefAny on every invocation — the
        // backreference DI pattern only works if that clone shares the payload.
        with_callback_info(None, OptionGlContextPtr::None, |info| {
            for (w, hgt) in [(1_u32, 1_u32), (2, 3), (4, 4)] {
                invoke_on_frame(&h, info, &frame(w, hgt));
            }
        });

        assert_eq!(
            hook_seen(&mut data),
            vec![
                (1, 1, 4, Some(0)),
                (2, 3, 24, Some(0)),
                (4, 4, 64, Some(0)),
            ],
            "every frame must reach the hook, in order, with its bytes intact"
        );
    }

    #[test]
    fn invoke_on_frame_forwards_degenerate_frames_unvalidated_and_without_panicking() {
        let mut data = RefAny::new(HookLog {
            seen: Vec::new(),
            reply: Update::DoNothing,
        });
        let h = hook(hook_record, data.clone());

        with_callback_info(None, OptionGlContextPtr::None, |info| {
            // 0x0, dimensions that disagree with the byte count, and dimensions
            // whose tight-packing size (w*h*4) overflows usize. `invoke_on_frame`
            // must hand all of them to the hook as-is: it is a pure forwarder and
            // must never multiply the dimensions out.
            invoke_on_frame(&h, info, &frame_raw(0, 0, Vec::new()));
            invoke_on_frame(&h, info, &frame_raw(9, 9, vec![7, 8, 9]));
            invoke_on_frame(&h, info, &frame_raw(u32::MAX, u32::MAX, Vec::new()));
            invoke_on_frame(&h, info, &frame_raw(u32::MAX, 1, vec![255]));
        });

        assert_eq!(
            hook_seen(&mut data),
            vec![
                (0, 0, 0, None),
                (9, 9, 3, Some(7)),
                (u32::MAX, u32::MAX, 0, None),
                (u32::MAX, 1, 1, Some(255)),
            ],
            "invoke_on_frame must forward frames verbatim, without validating them"
        );
    }

    #[test]
    fn invoke_on_frame_hook_writes_through_the_shared_callback_info() {
        // `invoke_on_frame` passes `*info` (CallbackInfo is Copy) — the copy must
        // still write into the *caller's* transaction container.
        let h = hook(hook_recomposite, RefAny::new(OtherState::default()));
        let (update, changes) = with_callback_info(None, OptionGlContextPtr::None, |info| {
            invoke_on_frame(&h, info, &frame(1, 1))
        });

        assert_eq!(update, Update::RefreshDomAllWindows);
        assert_eq!(
            recomposites(&changes),
            1,
            "a change made by the hook must be visible to the widget's writeback"
        );
    }

    // ==================================================================
    // upload_rgba
    // ==================================================================

    #[test]
    fn upload_rgba_forwards_the_texture_id_and_the_rgba8_constants() {
        for id in [0_u32, 1, 7, u32::MAX] {
            let ((), log) = with_recorded_gl(|gl| upload_rgba(&gl, id, &frame(2, 2)));
            assert_eq!(
                log,
                vec![
                    GlCall::BindTexture {
                        target: TEXTURE_2D,
                        texture: id,
                    },
                    GlCall::TexImage2d {
                        target: TEXTURE_2D,
                        level: 0,
                        internal_format: RGBA as i32,
                        width: 2,
                        height: 2,
                        border: 0,
                        format: RGBA,
                        ty: UNSIGNED_BYTE,
                        has_pixels: true,
                    },
                ],
                "upload_rgba must bind exactly texture {id} and upload tightly-packed RGBA8"
            );
        }
    }

    #[test]
    fn upload_rgba_zero_sized_frame_is_forwarded_as_a_0x0_upload() {
        let ((), log) = with_recorded_gl(|gl| upload_rgba(&gl, 3, &frame_raw(0, 0, Vec::new())));
        assert_eq!(
            log,
            vec![
                GlCall::BindTexture {
                    target: TEXTURE_2D,
                    texture: 3,
                },
                GlCall::TexImage2d {
                    target: TEXTURE_2D,
                    level: 0,
                    internal_format: RGBA as i32,
                    width: 0,
                    height: 0,
                    border: 0,
                    format: RGBA,
                    ty: UNSIGNED_BYTE,
                    has_pixels: true,
                },
            ],
            "a 0x0 frame must still be a well-formed (if empty) glTexImage2D, not a panic"
        );
    }

    #[test]
    fn upload_rgba_dimensions_above_i32_max_wrap_to_negative_glsizei() {
        // glTexImage2D takes GLsizei (= i32), so a u32 dimension > i32::MAX is a
        // lossy cast. Assert the *exact* wrapped value: GL then rejects the call
        // with GL_INVALID_VALUE (the frame is dropped) — the cast must never be a
        // debug-mode overflow panic or UB.
        let cases: [(u32, u32, i32, i32); 4] = [
            (i32::MAX as u32, 1, i32::MAX, 1),
            (i32::MAX as u32 + 1, 1, i32::MIN, 1),
            (u32::MAX, u32::MAX, -1, -1),
            (u32::MAX - 1, 2, -2, 2),
        ];

        for (w, h, want_w, want_h) in cases {
            // Empty byte buffer: the huge dimensions must never be multiplied out
            // (that would be a several-exabyte allocation), only cast.
            let ((), log) = with_recorded_gl(|gl| upload_rgba(&gl, 1, &frame_raw(w, h, Vec::new())));
            let tex = log
                .iter()
                .find_map(|c| match c {
                    GlCall::TexImage2d { width, height, .. } => Some((*width, *height)),
                    _ => None,
                })
                .expect("upload_rgba must always call glTexImage2D");
            assert_eq!(
                tex,
                (want_w, want_h),
                "{w}x{h} must cast to GLsizei {want_w}x{want_h}"
            );
        }
    }

    #[test]
    fn upload_rgba_against_an_unloaded_driver_is_a_silent_no_op() {
        // is_gl_usable() == false (all entry points NULL): the loader must swallow
        // every call rather than jumping through a NULL function pointer.
        let gl = null_gl_context();
        upload_rgba(&gl, 0, &frame(2, 2));
        upload_rgba(&gl, u32::MAX, &frame_raw(u32::MAX, u32::MAX, Vec::new()));
        upload_rgba(&gl, 1, &frame_raw(0, 0, Vec::new()));
    }

    // ==================================================================
    // present_frame — CPU (no GL context)
    // ==================================================================

    #[test]
    fn present_frame_without_gl_installs_a_raw_image_on_the_dataset_node() {
        let ds = RefAny::new(CamState::default());
        let styled = dom_with_datasets(Some(ds.clone()), None);

        let (id, changes) = with_callback_info(Some(styled), OptionGlContextPtr::None, |info| {
            present_frame(info, ds.clone(), None, &frame(4, 4))
        });

        // The CPU path never allocates a GL texture, so it must hand back the id it
        // was given (None) rather than inventing one.
        assert_eq!(id, None, "the cpurender path must not invent a texture id");

        let installs = image_installs(&changes);
        assert_eq!(installs.len(), 1, "exactly one image install per frame");
        let (dom_id, node_idx, image, update_type) = installs[0];
        assert_eq!(dom_id, DomId::ROOT_ID);
        assert_eq!(node_idx, 1, "the image must land on the dataset's node");
        assert_eq!(update_type, UpdateImageType::Content);
        match image.get_data() {
            DecodedImage::Raw((descriptor, _)) => {
                assert_eq!(
                    (descriptor.width, descriptor.height),
                    (4, 4),
                    "the installed image must keep the frame's dimensions"
                );
            }
            other => panic!("cpurender must install a raw image, got {other:?}"),
        }
        assert_eq!(
            recomposites(&changes),
            0,
            "the CPU path swaps the node's image instead of recompositing a texture"
        );
    }

    #[test]
    fn present_frame_without_gl_returns_the_current_id_verbatim() {
        for current in [None, Some(0_u32), Some(1), Some(u32::MAX)] {
            let ds = RefAny::new(CamState::default());
            let styled = dom_with_datasets(Some(ds.clone()), None);
            let (id, changes) =
                with_callback_info(Some(styled), OptionGlContextPtr::None, |info| {
                    present_frame(info, ds.clone(), current, &frame(2, 2))
                });
            assert_eq!(
                id, current,
                "the cpurender path must round-trip current_id ({current:?}) untouched"
            );
            assert_eq!(
                image_installs(&changes).len(),
                1,
                "the CPU path re-installs the image on *every* frame"
            );
        }
    }

    #[test]
    fn present_frame_without_gl_and_without_a_matching_dataset_installs_nothing() {
        // Node carries `OtherState`, the widget looks for `CamState`.
        let node_ds = RefAny::new(OtherState::default());
        let styled = dom_with_datasets(Some(node_ds), None);
        let search = RefAny::new(CamState::default());

        let (id, changes) = with_callback_info(Some(styled), OptionGlContextPtr::None, |info| {
            present_frame(info, search.clone(), Some(9), &frame(2, 2))
        });

        assert_eq!(id, Some(9), "a failed node lookup must not lose the id");
        assert!(
            changes.is_empty(),
            "no node owns the dataset, so nothing may be installed: {changes:?}"
        );
    }

    #[test]
    fn present_frame_without_gl_and_without_any_layout_result_installs_nothing() {
        let ds = RefAny::new(CamState::default());
        let (id, changes) = with_callback_info(None, OptionGlContextPtr::None, |info| {
            present_frame(info, ds.clone(), Some(3), &frame(2, 2))
        });
        assert_eq!(id, Some(3));
        assert!(
            changes.is_empty(),
            "an empty window must not be written to: {changes:?}"
        );
    }

    #[test]
    fn present_frame_without_gl_rejects_a_frame_whose_byte_count_disagrees_with_its_size() {
        // A backend that lies about the frame size (or a short read) must not be
        // able to install a bogus image — RawImage validates len == w*h*4.
        for (w, h, bytes) in [
            (4_u32, 4_u32, vec![0_u8; 3]),        // far too short
            (4, 4, vec![0_u8; 63]),               // one byte short
            (4, 4, vec![0_u8; 65]),               // one byte long
            (2, 2, Vec::new()),                   // no pixels at all
        ] {
            let ds = RefAny::new(CamState::default());
            let styled = dom_with_datasets(Some(ds.clone()), None);
            let (id, changes) =
                with_callback_info(Some(styled), OptionGlContextPtr::None, |info| {
                    present_frame(info, ds.clone(), Some(5), &frame_raw(w, h, bytes.clone()))
                });

            assert_eq!(id, Some(5), "a rejected frame must not disturb the id");
            assert!(
                changes.is_empty(),
                "a {w}x{h} frame with {} bytes must be rejected, not installed: {changes:?}",
                bytes.len()
            );
        }
    }

    #[test]
    fn present_frame_without_gl_installs_a_degenerate_image_for_a_0x0_frame() {
        // 0*0*4 == 0 == len(bytes), so a 0x0 frame passes RawImage's length check
        // and IS installed (as a 0x0 image). Pin the behaviour: it must at least
        // not panic and must not corrupt the returned id.
        let ds = RefAny::new(CamState::default());
        let styled = dom_with_datasets(Some(ds.clone()), None);
        let (id, changes) = with_callback_info(Some(styled), OptionGlContextPtr::None, |info| {
            present_frame(info, ds.clone(), Some(2), &frame_raw(0, 0, Vec::new()))
        });

        assert_eq!(id, Some(2));
        let installs = image_installs(&changes);
        assert_eq!(installs.len(), 1);
        match installs[0].2.get_data() {
            DecodedImage::Raw((descriptor, _)) => {
                assert_eq!((descriptor.width, descriptor.height), (0, 0));
            }
            other => panic!("expected a raw image, got {other:?}"),
        }
    }

    #[test]
    fn present_frame_without_gl_survives_dimensions_whose_byte_count_overflows_usize() {
        // ADVERSARIAL: a backend reporting 2^31 x 2^31 makes the CPU path compute
        // `width * height * 4` in usize inside `RawImage::into_loaded_image_source`
        // -> 2^64, which overflows.
        //
        // Today that is an arithmetic-overflow PANIC in a debug build (and a
        // silent wrap to 0 in release, which then *accepts* the empty byte buffer
        // as a valid 2^31 x 2^31 image). Neither is a graceful rejection — see the
        // autotest report. What must hold in *both* modes is the one invariant we
        // can still assert: the caller's texture id is never corrupted, and no GL
        // work is attempted.
        let ds = RefAny::new(CamState::default());
        let styled = dom_with_datasets(Some(ds.clone()), None);

        let (result, _changes) = with_callback_info(Some(styled), OptionGlContextPtr::None, |info| {
            catch_unwind(AssertUnwindSafe(|| {
                present_frame(
                    info,
                    ds.clone(),
                    Some(11),
                    &frame_raw(1_u32 << 31, 1_u32 << 31, Vec::new()),
                )
            }))
        });

        match result {
            Ok(id) => assert_eq!(
                id,
                Some(11),
                "the cpurender path must always hand back current_id"
            ),
            Err(_) => eprintln!(
                "NOTE: present_frame panicked (usize overflow of width*height*4) for a \
                 2^31 x 2^31 frame — a malformed capture backend can take the process down"
            ),
        }
    }

    #[test]
    fn present_frame_installs_exactly_one_image_when_two_nodes_share_a_dataset_type() {
        // Two capture widgets of the same state type in one DOM: the lookup scores
        // candidates by RefAny instance id, so *which* node wins is an internal
        // detail — but it must pick exactly ONE, and it must be a node that
        // actually owns a dataset (never the body at index 0, never both).
        let styled = dom_with_datasets(
            Some(RefAny::new(CamState::default())),
            Some(RefAny::new(CamState::default())),
        );
        let search = RefAny::new(CamState::default());

        let (id, changes) = with_callback_info(Some(styled), OptionGlContextPtr::None, |info| {
            present_frame(info, search.clone(), Some(4), &frame(2, 2))
        });

        assert_eq!(id, Some(4));
        let installs = image_installs(&changes);
        assert_eq!(
            installs.len(),
            1,
            "a frame must never be installed on two nodes at once: {changes:?}"
        );
        assert!(
            installs[0].1 == 1 || installs[0].1 == 2,
            "the image must land on a node that owns a dataset, not on node {}",
            installs[0].1
        );
    }

    #[test]
    fn present_frame_matches_datasets_by_type_id_not_by_identity() {
        // FOOTGUN: the lookup compares *type ids*, so a completely unrelated
        // RefAny of the same type finds the node. Two capture widgets sharing a
        // state type would therefore fight over one node.
        let node_ds = RefAny::new(CamState::default());
        let styled = dom_with_datasets(Some(node_ds), None);

        let unrelated = RefAny::new(CamState::default()); // a different allocation
        let (id, changes) = with_callback_info(Some(styled), OptionGlContextPtr::None, |info| {
            present_frame(info, unrelated.clone(), None, &frame(2, 2))
        });

        assert_eq!(id, None);
        assert_eq!(
            image_installs(&changes).len(),
            1,
            "an unrelated RefAny of the same type still resolves to the node"
        );
    }

    // ==================================================================
    // present_frame — GL
    // ==================================================================

    #[test]
    fn present_frame_with_gl_first_frame_allocates_uploads_and_installs_once() {
        let ds = RefAny::new(CamState::default());
        let styled = dom_with_datasets(Some(ds.clone()), None);

        let ((id, changes), log) = with_recorded_gl(|gl| {
            with_callback_info(Some(styled), OptionGlContextPtr::Some(gl), |info| {
                present_frame(info, ds.clone(), None, &frame(4, 4))
            })
        });

        assert_eq!(
            id,
            Some(RECORDED_TEXTURE_ID),
            "the first frame must hand back the texture name the driver allocated"
        );

        // Installed exactly once, as an external GL texture, on the dataset's node.
        let installs = image_installs(&changes);
        assert_eq!(installs.len(), 1, "the node's image is installed ONCE");
        assert_eq!(installs[0].1, 1);
        assert_eq!(installs[0].3, UpdateImageType::Content);
        match installs[0].2.get_data() {
            DecodedImage::Gl(texture) => {
                assert_eq!(texture.texture_id, RECORDED_TEXTURE_ID);
                assert_eq!(
                    (texture.size.width, texture.size.height),
                    (4, 4),
                    "the external texture must be sized like the frame"
                );
            }
            other => panic!("the GL path must install an external texture, got {other:?}"),
        }
        assert_eq!(
            recomposites(&changes),
            0,
            "the install itself rebuilds the display list; no extra recomposite needed"
        );

        // One texture allocated, and the pixels really were uploaded into it.
        assert_eq!(
            log.iter()
                .filter(|c| matches!(c, GlCall::GenTextures { .. }))
                .count(),
            1,
            "exactly one texture may be allocated for the first frame"
        );
        assert_eq!(
            log.last(),
            Some(&GlCall::TexImage2d {
                target: TEXTURE_2D,
                level: 0,
                internal_format: RGBA as i32,
                width: 4,
                height: 4,
                border: 0,
                format: RGBA,
                ty: UNSIGNED_BYTE,
                has_pixels: true,
            }),
            "the last GL call must be the pixel upload, not the empty allocation"
        );
        assert!(
            log.contains(&GlCall::BindTexture {
                target: TEXTURE_2D,
                texture: RECORDED_TEXTURE_ID,
            }),
            "the upload must target the freshly allocated texture: {log:?}"
        );
    }

    #[test]
    fn present_frame_with_gl_later_frames_reupload_into_the_same_texture() {
        let ds = RefAny::new(CamState::default());
        let styled = dom_with_datasets(Some(ds.clone()), None);

        let ((id, changes), log) = with_recorded_gl(|gl| {
            with_callback_info(Some(styled), OptionGlContextPtr::Some(gl), |info| {
                present_frame(info, ds.clone(), Some(RECORDED_TEXTURE_ID), &frame(4, 4))
            })
        });

        assert_eq!(id, Some(RECORDED_TEXTURE_ID), "the texture id must stay stable");
        assert!(
            image_installs(&changes).is_empty(),
            "steady-state frames must NOT re-install the node's image (that would \
             rebuild the display list every frame): {changes:?}"
        );
        assert_eq!(
            recomposites(&changes),
            1,
            "a steady-state frame recomposites exactly once"
        );
        assert_eq!(
            log,
            vec![
                GlCall::BindTexture {
                    target: TEXTURE_2D,
                    texture: RECORDED_TEXTURE_ID,
                },
                GlCall::TexImage2d {
                    target: TEXTURE_2D,
                    level: 0,
                    internal_format: RGBA as i32,
                    width: 4,
                    height: 4,
                    border: 0,
                    format: RGBA,
                    ty: UNSIGNED_BYTE,
                    has_pixels: true,
                },
            ],
            "a steady-state frame must be exactly one re-upload — no new texture"
        );
    }

    #[test]
    fn present_frame_with_gl_round_trips_extreme_texture_ids() {
        for current in [Some(0_u32), Some(u32::MAX)] {
            let ds = RefAny::new(CamState::default());
            let styled = dom_with_datasets(Some(ds.clone()), None);

            let ((id, changes), log) = with_recorded_gl(|gl| {
                with_callback_info(Some(styled), OptionGlContextPtr::Some(gl), |info| {
                    present_frame(info, ds.clone(), current, &frame(1, 1))
                })
            });

            assert_eq!(
                id, current,
                "a stored texture id must survive the writeback unchanged"
            );
            assert_eq!(recomposites(&changes), 1);
            assert!(
                log.contains(&GlCall::BindTexture {
                    target: TEXTURE_2D,
                    texture: current.expect("current is Some"),
                }),
                "the id must be forwarded to glBindTexture verbatim: {log:?}"
            );
        }
    }

    #[test]
    fn present_frame_with_gl_first_frame_without_a_matching_node_still_returns_the_id() {
        // The node lookup fails (no dataset of that type), so the freshly created
        // ImageRef — and with it the GL texture — is dropped again, yet the id is
        // still handed back and will be re-uploaded into on every later frame.
        let styled = dom_with_datasets(Some(RefAny::new(OtherState::default())), None);
        let search = RefAny::new(CamState::default());

        let ((id, changes), log) = with_recorded_gl(|gl| {
            with_callback_info(Some(styled), OptionGlContextPtr::Some(gl), |info| {
                present_frame(info, search.clone(), None, &frame(2, 2))
            })
        });

        assert_eq!(id, Some(RECORDED_TEXTURE_ID));
        assert!(
            changes.is_empty(),
            "nothing may be installed when no node owns the dataset: {changes:?}"
        );
        assert_eq!(
            log.iter()
                .filter(|c| matches!(c, GlCall::GenTextures { .. }))
                .count(),
            1,
            "a texture is allocated even though it can never be shown"
        );
    }

    // ==================================================================
    // Backend registries (CaptureVTable / AudioCaptureVTable)
    //
    // The three registries are process-global `OnceLock`s, so each is exercised
    // by exactly ONE test (registering from two tests would race). Each backend
    // fn body is deliberately distinct so the linker cannot fold them onto one
    // address and make the identity assertions vacuous.
    // ==================================================================

    fn open_a(index: u32, width: u32, height: u32) -> u64 {
        u64::from(index) + u64::from(width) * 3 + u64::from(height)
    }
    fn read_a(handle: u64, out: &mut Vec<u8>) -> (u32, u32) {
        out.clear();
        out.extend_from_slice(&[1, 2, 3, 4]);
        (handle as u32, 1)
    }
    fn close_a(_handle: u64) {}

    fn open_b(index: u32, width: u32, height: u32) -> u64 {
        u64::from(index) * 7 + u64::from(width) + u64::from(height) * 11
    }
    fn read_b(_handle: u64, out: &mut Vec<u8>) -> (u32, u32) {
        out.push(9);
        (0, 0)
    }
    fn close_b(_handle: u64) {
        // distinct body: the linker must not fold this onto close_a
        let _ = core::hint::black_box(1_u8);
    }

    fn vtable_a() -> CaptureVTable {
        CaptureVTable {
            open: open_a,
            read: read_a,
            close: close_a,
        }
    }
    fn vtable_b() -> CaptureVTable {
        CaptureVTable {
            open: open_b,
            read: read_b,
            close: close_b,
        }
    }

    fn same_vtable(a: CaptureVTable, b: CaptureVTable) -> bool {
        a.open as usize == b.open as usize
            && a.read as usize == b.read as usize
            && a.close as usize == b.close as usize
    }

    #[test]
    fn register_camera_backend_is_first_wins_and_never_overwritten() {
        let before = camera_backend();

        register_camera_backend(vtable_a());
        let first = camera_backend().expect("a backend is registered after the first call");

        // A second registration must be silently ignored, not panic and not swap
        // the vtable out from under a running capture worker.
        register_camera_backend(vtable_b());
        register_camera_backend(vtable_b());
        let after = camera_backend().expect("the backend must still be there");

        assert!(
            same_vtable(first, after),
            "the first registration must win; a later one must not replace it"
        );
        if let Some(pre) = before {
            assert!(
                same_vtable(pre, after),
                "a backend registered before this test must not have been replaced"
            );
        } else {
            assert!(
                same_vtable(vtable_a(), after),
                "camera_backend() must hand back exactly the vtable that was registered"
            );
            // The registered fn pointers must actually be callable through the vtable.
            assert_eq!((after.open)(1, 2, 3), open_a(1, 2, 3));
            assert_eq!((after.open)(u32::MAX, u32::MAX, u32::MAX), open_a(u32::MAX, u32::MAX, u32::MAX));
            let mut buf = vec![0_u8; 8];
            assert_eq!((after.read)(u64::from(u32::MAX), &mut buf), (u32::MAX, 1));
            assert_eq!(buf, vec![1, 2, 3, 4], "read must be able to resize `out`");
            (after.close)(0);
            (after.close)(u64::MAX);
        }
    }

    #[test]
    fn register_screen_backend_is_independent_of_the_camera_backend() {
        let before = screen_backend();
        register_screen_backend(vtable_b());
        let after = screen_backend().expect("a screen backend is registered");

        if let Some(pre) = before {
            assert!(same_vtable(pre, after), "first registration wins");
        } else {
            assert!(
                same_vtable(vtable_b(), after),
                "the screen registry must hand back the screen vtable"
            );
            // Registering into the screen slot must not have leaked into the
            // camera slot (they are separate OnceLocks).
            if let Some(cam) = camera_backend() {
                assert!(
                    !same_vtable(cam, vtable_b()),
                    "the camera registry must not pick up the screen vtable"
                );
            }
            // `(0, 0)` is the documented end-of-stream signal.
            let mut buf = Vec::new();
            assert_eq!((after.read)(0, &mut buf), (0, 0));
        }
    }

    fn mic_open(sample_rate: u32, channels: u16) -> u64 {
        u64::from(sample_rate) * 2 + u64::from(channels)
    }
    fn mic_read(handle: u64, out: &mut Vec<f32>) -> u32 {
        out.clear();
        // NaN / inf / subnormal samples must survive the vtable boundary untouched.
        out.extend_from_slice(&[f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.0]);
        (handle % 3) as u32
    }
    fn mic_close(_handle: u64) {}

    fn mic_open_other(sample_rate: u32, channels: u16) -> u64 {
        u64::from(sample_rate) ^ u64::from(channels)
    }
    fn mic_read_other(_handle: u64, out: &mut Vec<f32>) -> u32 {
        out.push(1.0);
        0
    }
    fn mic_close_other(_handle: u64) {
        let _ = core::hint::black_box(2_u8);
    }

    #[test]
    fn register_mic_backend_is_first_wins_and_passes_f32_samples_through() {
        let before = mic_backend();

        register_mic_backend(AudioCaptureVTable {
            open: mic_open,
            read: mic_read,
            close: mic_close,
        });
        register_mic_backend(AudioCaptureVTable {
            open: mic_open_other,
            read: mic_read_other,
            close: mic_close_other,
        });

        let vt = mic_backend().expect("a mic backend is registered");

        if before.is_none() {
            assert_eq!(
                vt.open as usize, mic_open as usize,
                "the first mic registration must win"
            );

            // Boundary sample rates / channel counts must go through untouched.
            assert_eq!((vt.open)(0, 0), 0);
            assert_eq!((vt.open)(u32::MAX, u16::MAX), mic_open(u32::MAX, u16::MAX));

            let mut samples = Vec::new();
            let frames = (vt.read)(4, &mut samples);
            assert_eq!(frames, 1, "the frame count must be the vtable's, verbatim");
            assert_eq!(samples.len(), 4);
            assert!(samples[0].is_nan(), "a NaN sample must not be normalised");
            assert_eq!(samples[1], f32::INFINITY);
            assert_eq!(samples[2], f32::NEG_INFINITY);
            assert!(
                samples[3] == 0.0 && samples[3].is_sign_negative(),
                "-0.0 must keep its sign bit"
            );

            // `0` is the documented EOF/error return.
            assert_eq!((vt.read)(3, &mut samples), 0);
            (vt.close)(u64::MAX);
        }
    }
}
