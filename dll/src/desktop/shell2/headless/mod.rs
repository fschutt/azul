//! Headless backend for testing and CPU-only rendering (`AZ_BACKEND=headless`).
//!
//! This backend implements the full `PlatformWindow` trait without
//! GPU / OpenGL. It behaves like a real platform window — DOM is laid out,
//! callbacks fire, timers tick — but rendering goes through a **CpuBackend**
//! instead of WebRender.
//!
//! ## CpuBackend
//!
//! `CpuBackend` has a similar *purpose* to the WebRender pipeline
//! (render-api, renderer, hit-tester) but is fully CPU-based and much
//! simpler. It is intentionally less efficient — the target use-case is
//! small, ancillary windows (Linux menu bars, tooltip popups) and headless
//! E2E tests, not high-framerate rendering.
//!
//! ```text
//! WebRender path:   DisplayList → WrRenderApi → Renderer (GPU) → swapBuffers
//! CpuBackend path:  DisplayList → cpurender   → Pixmap  (CPU)  → (no-op / PNG)
//! ```
//!
//! ## Headless Event Loop
//!
//! `HeadlessWindow::run()` blocks in an infinite loop just like every other
//! platform's `run()`. Instead of busy-waiting or `thread::sleep`, it
//! blocks on a **`Condvar`** that is signalled when:
//!
//! * An event is injected (via `inject_event` / debug server)
//! * A timer fires (the earliest timer deadline is used as `wait_timeout`)
//! * A background thread completes
//!
//! This means the headless loop consumes **zero CPU** when idle, just
//! like the native `WaitMessage()` / `XNextEvent()` / `NSEvent` loops
//! on real platforms.
//!
//! If nothing can wake the loop (no timers, no threads, no debug server)
//! a warning is printed to stderr and the loop blocks indefinitely
//! (the programme hangs). This is intentional — it is the same behaviour
//! you would get from a real window that nobody interacts with.
//!
//! ## Architecture
//!
//! ```text
//! HeadlessWindow
//! ├── common: CommonWindowState        (shared with all platforms)
//! ├── cpu_backend: CpuBackend          (replaces WebRender)
//! ├── event_queue: VecDeque<HeadlessEvent> (programmatic event injection)
//! └── pending_window_creates: Vec      (popup/dialog queue)
//! ```

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::{Arc, Condvar, Mutex};
use std::cell::RefCell;
use std::time::{Duration, Instant};

use azul_core::{
    geom::LogicalPosition,
    gl::OptionGlContextPtr,
    hit_test::DocumentId,
    icon::SharedIconProvider,
    refany::RefAny,
    resources::{AppConfig, AppTerminationBehavior, IdNamespace, ImageCache, RendererResources},
    window::{
        AcceleratorKey, FullScreenMode, RawWindowHandle, ScrollResult, TouchPoint, TouchPointVec,
        VirtualKeyCode, WindowFrame,
    },
};
use azul_layout::{
    window::{LayoutWindow, ScrollbarDragState},
    window_state::{FullWindowState, WindowCreateOptions},
};
use rust_fontconfig::FcFontCache;
use rust_fontconfig::registry::FcFontRegistry;

use crate::desktop::wr_translate2::{AsyncHitTester, WrRenderApi};
use crate::desktop::shell2::common::event::HitTestNode;

use crate::desktop::shell2::common::{
    accessibility::A11yActionQueue,
    debug_server::{self, LogCategory},
    event::{self, CommonWindowState, PlatformWindow},
    WindowError,
};
use crate::{impl_platform_window_getters, log_debug, log_error, log_info, log_trace, log_warn};

/// Events that can be injected into a HeadlessWindow for testing or
/// via the debug server.
#[derive(Debug, Clone)]
pub enum HeadlessEvent {
    /// Simulate window close
    Close,
    /// Simulate mouse move to position
    MouseMove { x: f32, y: f32 },
    /// Simulate mouse button press
    MouseDown { button: azul_core::events::MouseButton },
    /// Simulate mouse button release
    MouseUp { button: azul_core::events::MouseButton },
    /// Simulate key press
    KeyDown { virtual_keycode: VirtualKeyCode },
    /// Simulate key release
    KeyUp { virtual_keycode: VirtualKeyCode },
    /// Simulate text input
    TextInput { text: String },
    /// Simulate window resize
    Resize { width: f32, height: f32 },
    /// Simulate scroll wheel
    Scroll { delta_x: f32, delta_y: f32 },
    /// Simulate an OS file drag hovering the window at (x, y) (MWA-A4).
    /// Mirrors the desktop ingress: XdndPosition / draggingUpdated /
    /// IDropTarget::DragOver / wl_data_device.motion.
    FileHover { x: f32, y: f32, paths: Vec<String> },
    /// Simulate an OS file drop at (x, y) (MWA-A4). Mirrors XdndDrop /
    /// performDragOperation / IDropTarget::Drop / wl_data_device.drop.
    FileDrop { x: f32, y: f32, paths: Vec<String> },
    /// Simulate the OS file drag leaving the window without dropping
    /// (MWA-A4). Mirrors XdndLeave / draggingExited / DragLeave.
    FileHoverCancel,
}

/// MWA-A4: feed the gesture manager's input sessions exactly like the
/// desktop shells do (every OS mouse handler calls `record_input_sample`).
/// Headless previously only mutated `mouse_state`, so `detect_drag`,
/// `detect_double_click`, `detect_long_press`, swipes and node-DnD were
/// structurally invisible to headless E2E — the entire gesture surface was
/// untestable.
fn record_headless_input(
    window: &mut HeadlessWindow,
    is_button_down: bool,
    is_button_up: bool,
) {
    use crate::desktop::shell2::common::event::{
        BUTTON_STATE_LEFT, BUTTON_STATE_MIDDLE, BUTTON_STATE_NONE, BUTTON_STATE_RIGHT,
    };
    let ms = &window.common.current_window_state().mouse_state;
    let mut button_state = BUTTON_STATE_NONE;
    if ms.left_down {
        button_state |= BUTTON_STATE_LEFT;
    }
    if ms.right_down {
        button_state |= BUTTON_STATE_RIGHT;
    }
    if ms.middle_down {
        button_state |= BUTTON_STATE_MIDDLE;
    }
    let pos = ms
        .cursor_position
        .get_position()
        .unwrap_or(LogicalPosition { x: 0.0, y: 0.0 });
    window.record_input_sample(pos, button_state, is_button_down, is_button_up, None);
}

/// Outcome of a single `CpuBackend::render_frame` call.
///
/// MOVED to `azul_layout::window::FrameDamage` so that it can be stored on
/// `LayoutWindow` (and therefore be reachable from `CallbackInfo`, i.e. from an
/// E2E assertion). Re-exported here so that every existing
/// `crate::desktop::shell2::headless::FrameDamage` path keeps working.
pub use azul_layout::window::{FrameDamage, FrameReport};

// ---------------------------------------------------------------------------
// CpuBackend — replaces WebRender in headless / CPU-only windows
// ---------------------------------------------------------------------------

/// CPU-based rendering backend that replaces the WebRender pipeline.
///
/// In the GPU path every window holds a `WrRenderApi` (for submitting
/// display-lists, registering fonts/images), a `webrender::Renderer`
/// (for rasterising on the GPU) and an `AsyncHitTester` (for spatial
/// queries).  `CpuBackend` fills the same role with a much simpler,
/// fully CPU-based implementation:
///
/// | GPU path               | CpuBackend equivalent                       |
/// |------------------------|---------------------------------------------|
/// | `WrRenderApi`          | not needed – fonts/images stay in LayoutWindow |
/// | `webrender::Renderer`  | `cpurender::render()` (behind feature gate) |
/// | `AsyncHitTester`       | `CpuHitTester` (layout-based)               |
/// | `swapBuffers`          | no-op (or write PNG for screenshots)        |
///
/// The backend holds a retained-mode `CompositorState` for efficient
/// incremental re-rendering.  On resize, only the root layer pixbuf is
/// reallocated; scroll and damage use pixel-shift / partial re-render.
pub struct CpuBackend {
    /// CPU-based hit tester rebuilt after each layout pass.
    pub hit_tester: azul_layout::headless::CpuHitTester,
    /// Last rendered pixmap (if CPU rendering is enabled).
    /// `None` when rendering is disabled (layout-only mode).
    #[cfg(feature = "cpurender")]
    pub last_frame: Option<azul_layout::cpurender::AzulPixmap>,
    /// Retained compositor state with per-layer pixbufs.
    #[cfg(feature = "cpurender")]
    pub compositor: Option<azul_layout::cpurender::CompositorState>,
    /// Glyph cache — persists across frames for text rendering.
    #[cfg(feature = "cpurender")]
    pub glyph_cache: azul_layout::glyph_cache::GlyphCache,
    /// Previous display list for damage rect computation.
    #[cfg(feature = "cpurender")]
    pub previous_display_list: Option<std::sync::Arc<azul_layout::solver3::display_list::DisplayList>>,
    /// `LayoutCache::build_seq` at the last present — what
    /// `LayoutCache::pending_patch_damage` drains the patch log from, so two
    /// patched builds between presents both get repainted.
    pub last_consumed_build_seq: u64,
    /// PAINT damage of the most recent `render_frame` — the region actually
    /// re-rasterised (for scroll this is just the thin exposed strip). This is the
    /// "pixels repainted" metric. Recorded so the headless test harness can assert
    /// on it without re-running the diff. Not gated on `cpurender`.
    pub last_frame_damage: FrameDamage,
    /// PRESENT damage of the most recent `render_frame` — the region that visually
    /// CHANGED on screen and must be blitted/uploaded to the window/GPU. For a
    /// scroll this is the whole shifted clip (the pixels moved), which is LARGER
    /// than the paint damage (the strip). The render-vs-present split (DAMAGE_
    /// REGION_PLAN): small paint region, larger present region. Equals the paint
    /// damage when nothing was pixel-shifted.
    pub last_present_damage: FrameDamage,
    /// Arc ptr of the display list the round-3 patch SHIFT was last applied
    /// for. A buffers-held retry re-runs render_frame with the SAME list —
    /// re-applying the memmove would double-shift the retained pixels, so
    /// the hint is gated on ptr inequality (the retry then takes the plain
    /// diff, which damages the movers normally).
    pub last_patch_shift_dl: usize,
    /// #27 native backbuffer: when set, the NEXT `render_frame` draws
    /// DIRECTLY into this externally-owned pixmap (a platform backbuffer,
    /// e.g. the free Wayland shm slot) instead of the owned `last_frame`.
    /// Shell contract: the buffer (a) matches the frame's pixel dimensions,
    /// (b) already holds the previous frame's pixels (cross-slot catch-up),
    /// (c) outlives the call, and (d) the shell clears this field after the
    /// call (dangle guard — the mapping dies with the pool). Consumed by
    /// `render_frame`; `last_frame` stays `None` afterwards. Never set in
    /// headless/e2e runs.
    #[cfg(feature = "cpurender")]
    pub native_target: Option<azul_layout::cpurender::AzulPixmap>,
    /// True when the LAST `render_frame` drew into a `native_target` — the
    /// shell branches its present on this (the pixels are already in the
    /// platform buffer; there is nothing to copy).
    pub rendered_native: bool,
    /// #32: the armed native target is a POOL-ORDER (B,G,R,A) buffer — an
    /// ARGB8888 wl_shm slot whose commit swizzles the damage rects. In-target
    /// pixel moves (scroll shift, patch blit) must convert the moved block
    /// back to renderer order or the commit swizzle double-converts it
    /// (R/B-swapped scrolled content on the glass). Set by the shell at every
    /// arming; only read while `rendered_native` is true.
    pub native_target_pool_order: bool,
    /// Scroll offsets from the previous frame (scroll_id → (x,y)). Used to detect
    /// scroll-offset changes and damage the affected frame's viewport so its
    /// content re-renders at the new offset (#13 — the display list is unchanged
    /// on scroll, so the diff alone only catches the scrollbar).
    #[cfg(feature = "cpurender")]
    pub previous_scroll_offsets: azul_layout::cpurender::ScrollOffsetMap,
    /// Previous frame's `VirtualView` child-DOM display lists (keyed by child
    /// `DomId`). The parent display list's `VirtualView` item is unchanged when
    /// only the child re-renders (async tile writeback, etc.), so the parent-DL
    /// diff can't see it. Comparing child DLs frame-to-frame lets `render_frame`
    /// damage the VirtualView region when its content changed — otherwise the
    /// "nothing changed → skip" path freezes async VirtualView content. Without
    /// this, the MapWidget showed only the placeholder grid on backends (Wayland)
    /// that don't get spurious WM expose events to force a full repaint.
    #[cfg(feature = "cpurender")]
    pub previous_vview_dls: std::collections::BTreeMap<
        azul_core::dom::DomId,
        std::sync::Arc<azul_layout::solver3::display_list::DisplayList>,
    >,
    /// GPU-animated values of the previous frame (`key.id → value`), for the
    /// frame-to-frame GPU-value diff. Scrollbar thumb position/fade opacity
    /// and drag/CSS transforms live in the GPU value cache — display-list
    /// items only carry the KEYS, so the item diff can't see them change.
    #[cfg(feature = "cpurender")]
    pub previous_gpu_transforms:
        std::collections::HashMap<usize, azul_core::transform::ComputedTransform3D>,
    #[cfg(feature = "cpurender")]
    pub previous_gpu_opacities: std::collections::HashMap<usize, f32>,
    /// Where zombie exits painted LAST frame (logical px) — see the
    /// zombie-damage computation in `render_frame`.
    pub previous_zombie_rects: Vec<azul_core::geom::LogicalRect>,
}

/// #27 native-backbuffer master switch, shared by every platform shell:
/// `AZ_NATIVE_BACKBUFFER=0` forces the legacy owned-pixmap + copy present
/// (also the automatic fallback when a platform's buffer can't take the
/// renderer's RGBA byte order). NOTE: in native mode `CpuBackend.last_frame`
/// stays `None` — tools that read the retained frame (live screenshot dumps)
/// need `AZ_NATIVE_BACKBUFFER=0`.
/// #32: in-place R↔B swizzle over `rects` (x, y, w, h in buffer px) of a
/// tightly-packed 4-byte-per-pixel buffer. Converts the CPU renderer's
/// R,G,B,A byte order to ARGB8888's B,G,R,A where a compositor never
/// advertises ABGR8888 (KWin offers ABGR only at 10/16-bit depths). Touching
/// ONLY the damage rects is sound because writes ⊆ damage is pinned by the
/// damage-sound laws: every pixel written this frame is converted exactly
/// once, and retained pixels (converted at their own commit) are never
/// re-swizzled.
pub(crate) fn swizzle_rb_in_rects(
    buf: &mut [u8],
    stride_bytes: usize,
    buf_height: usize,
    rects: &[(i32, i32, i32, i32)],
) {
    let row_px = stride_bytes / 4;
    for &(x, y, w, h) in rects {
        if w <= 0 || h <= 0 {
            continue;
        }
        let x0 = x.max(0) as usize;
        let y0 = y.max(0) as usize;
        let x1 = (x.saturating_add(w) as usize).min(row_px);
        let y1 = (y.saturating_add(h) as usize).min(buf_height);
        for row in y0..y1 {
            let base = row * stride_bytes;
            for px in x0..x1 {
                let o = base + px * 4;
                if o + 4 <= buf.len() {
                    buf.swap(o, o + 2);
                }
            }
        }
    }
}

pub fn native_backbuffer_enabled() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| {
        std::env::var("AZ_NATIVE_BACKBUFFER")
            .map(|v| v != "0")
            .unwrap_or(true)
    })
}

impl Default for CpuBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl CpuBackend {
    pub fn new() -> Self {
        Self {
            hit_tester: azul_layout::headless::CpuHitTester::new(),
            #[cfg(feature = "cpurender")]
            last_frame: None,
            #[cfg(feature = "cpurender")]
            compositor: None,
            #[cfg(feature = "cpurender")]
            glyph_cache: azul_layout::glyph_cache::GlyphCache::new(),
            #[cfg(feature = "cpurender")]
            previous_display_list: None,
            last_consumed_build_seq: 0,
            last_frame_damage: FrameDamage::None,
            last_present_damage: FrameDamage::None,
            last_patch_shift_dl: 0,
            #[cfg(feature = "cpurender")]
            native_target: None,
            rendered_native: false,
            native_target_pool_order: false,
            #[cfg(feature = "cpurender")]
            previous_scroll_offsets: azul_layout::cpurender::ScrollOffsetMap::new(),
            #[cfg(feature = "cpurender")]
            previous_vview_dls: std::collections::BTreeMap::new(),
            #[cfg(feature = "cpurender")]
            previous_gpu_transforms: std::collections::HashMap::new(),
            previous_zombie_rects: Vec::new(),
            #[cfg(feature = "cpurender")]
            previous_gpu_opacities: std::collections::HashMap::new(),
        }
    }

    /// Render the current display list into `last_frame`.
    ///
    /// Uses damage-rect-based incremental rendering when possible:
    /// - Compares current display list against `previous_display_list`
    /// - If items match structurally, only repaints changed regions
    /// - On resize, uses grow-only buffer reuse for window expansion
    ///
    /// Returns the damage rects that were rendered (empty = full repaint).
    #[cfg(feature = "cpurender")]
    pub fn render_frame(
        &mut self,
        layout_window: &azul_layout::window::LayoutWindow,
        renderer_resources: &azul_core::resources::RendererResources,
        width: f32,
        height: f32,
        dpi_factor: f32,
    ) -> Vec<azul_core::geom::LogicalRect> {
        // Engine observability: frame duration + probe spans per present
        // (drop-guard covers all return paths).
        #[cfg(feature = "telemetry")]
        let _frame_pump = azul_layout::telemetry::FramePump::begin("present");

        use azul_core::dom::DomId;
        use azul_layout::cpurender;

        // #27: describes THIS call only — set at target acquisition below.
        self.rendered_native = false;

        // Every early return below must leave `last_frame_damage` /
        // `last_present_damage` describing THIS call ("nothing changed"), not
        // whatever the previous call recorded. The platform blit paths read
        // `last_present_damage` after every render_frame — with the fields
        // left stale, a call that rendered nothing (window minimised to 0×0,
        // no layout result yet, allocation failure) made the caller re-blit
        // the PREVIOUS frame's damage rects out of the retained pixmap.

        // Get the layout result from layout results
        let dom_id = DomId { inner: 0 };
        let result = match layout_window.layout_results.get(&dom_id) {
            Some(result) => result,
            None => {
                self.last_frame_damage = FrameDamage::None;
                self.last_present_damage = FrameDamage::None;
                return Vec::new();
            }
        };
        let display_list = &result.display_list;

        let pixel_w = (width * dpi_factor).ceil() as u32;
        let pixel_h = (height * dpi_factor).ceil() as u32;
        if pixel_w == 0 || pixel_h == 0 {
            self.last_frame_damage = FrameDamage::None;
            self.last_present_damage = FrameDamage::None;
            return Vec::new();
        }

        // Allocate or resize compositor
        let compositor = self.compositor.get_or_insert_with(|| {
            cpurender::CompositorState::new(pixel_w, pixel_h)
        });

        // Check if we need to resize the root layer
        let root = compositor.layers.get(&compositor.root_layer);
        let (old_pw, old_ph) = match root {
            Some(layer) => (layer.pixbuf.width(), layer.pixbuf.height()),
            None => (0, 0),
        };
        let needs_resize = old_pw != pixel_w || old_ph != pixel_h;

        let mut resize_damage = Vec::new();
        // A GROW preserves the previous frame: `resize_grow_only` copies the old
        // pixels into the top-left of the enlarged buffer (and `resize_reuse`
        // does the same for `last_frame` below), so the frame stays a valid base
        // for an incremental repaint and only the newly-exposed L is unknown.
        // A SHRINK throws the whole compositor away, so nothing may be reused.
        let mut resize_preserved_pixels = false;
        if needs_resize {
            let is_grow = pixel_w >= old_pw && pixel_h >= old_ph && old_pw > 0 && old_ph > 0;
            if is_grow {
                resize_preserved_pixels = true;
                // Grow-only: resize root layer pixbuf, keep old content
                if let Some(root_layer) = compositor.layers.get_mut(&compositor.root_layer) {
                    let _ = root_layer.pixbuf.resize_grow_only(pixel_w, pixel_h, 255, 255, 255, 255);
                    root_layer.bounds.size = azul_core::geom::LogicalSize {
                        width: pixel_w as f32, height: pixel_h as f32,
                    };
                }
                // Damage rects are LOGICAL everywhere downstream (the renderer
                // multiplies by dpi_factor) — convert the physical pixbuf dims
                // back to logical, or at dpi≠1 the exposed strips land at
                // dpi²-scaled positions (off the buffer entirely at dpi=2).
                resize_damage = cpurender::compute_resize_damage(
                    old_pw as f32 / dpi_factor,
                    old_ph as f32 / dpi_factor,
                    width,
                    height,
                );
            } else {
                // Shrink or first allocation: full recreate.
                //
                // A MIXED resize (wider but shorter) lands here too — `is_grow`
                // demands both axes. This branch stays a FULL repaint, and that
                // is a measured decision, not an oversight: it recreates the
                // compositor AND never calls `compute_resize_damage`, so letting
                // it reuse the previous frame under-paints. Measured with the
                // resize probe at 500x600 -> 700x400: 53200 changed pixels
                // uncovered by any damage rect, the first at (500, 134) — i.e.
                // the whole newly-exposed right strip, stale on a real screen. A
                // shrink also exposes nothing new, so a full repaint here costs
                // at most the NEW (smaller) buffer.
                *compositor = cpurender::CompositorState::new(pixel_w, pixel_h);
            }
        }

        // Real scroll offsets for this frame — needed by the damage diff below
        // (items inside scroll frames are stored at CONTENT coords; the diff
        // projects a changed item's old bounds through the offsets its pixels
        // were last painted at and its new bounds through the current offsets,
        // yielding viewport-space damage) and by the scroll-shift machinery
        // further down.
        let scroll_offsets = layout_window
            .scroll_manager
            .build_scroll_offset_map(dom_id, &result.scroll_id_to_node_id);

        // GPU-value diff: thumb position / fade opacity / drag & CSS
        // transforms change WITHOUT any display-list item changing (items
        // only carry the keys). Diff the cache values against last frame's
        // and damage the bound items — this is what lets ScrollBarStyled
        // compare as visually-equal (skip path reachable for scrollbar'd
        // windows) without freezing the thumb.
        let gpu_cache_early = layout_window.gpu_state_manager.get_cache(dom_id);
        let (gpu_transforms, gpu_opacities) =
            cpurender::extract_gpu_values(gpu_cache_early, dom_id);
        let gpu_damage = cpurender::gpu_value_damage(
            display_list,
            &self.previous_gpu_transforms,
            &self.previous_gpu_opacities,
            &gpu_transforms,
            &gpu_opacities,
        );
        let has_gpu_damage = !gpu_damage.rects.is_empty() || gpu_damage.needs_full;
        // Zombie exits repaint every tick with no display-list change — their
        // per-frame contribution is `previous ∪ current` painted rects
        // (restore the live frame where the exit was, paint it where it is;
        // the reap frame erases the leftovers the same way), which keeps the
        // incremental path alive for the whole exit.
        let zombies_active = layout_window.has_zombies();
        let zombie_rects = if zombies_active {
            layout_window.zombie_paint_rects()
        } else {
            Vec::new()
        };
        let zombie_damage: Vec<azul_core::geom::LogicalRect> = self
            .previous_zombie_rects
            .iter()
            .chain(zombie_rects.iter())
            .copied()
            .collect();
        // Values are painted this frame whichever path runs (incremental
        // repaints read the CURRENT cache; skip only happens when unchanged).
        self.previous_gpu_transforms = gpu_transforms;
        self.previous_gpu_opacities = gpu_opacities;

        // Can the pixels of the previous frame still be trusted? Yes when the
        // buffer did not change size at all, and yes on a GROW (the old pixels
        // were copied over verbatim). No on a shrink / first allocation.
        let can_reuse_previous_frame = !needs_resize || resize_preserved_pixels;

        // ROUND 3: the layout patch's presentation hint. Eligible when the
        // dominant delta is INTEGRAL in physical pixels (a fractional blit
        // would change every subpixel phase — those frames re-render), the
        // previous frame's pixels are trustworthy, and this display list has
        // not already been shifted (buffers-held retry).
        let dl_arc_ptr = std::sync::Arc::as_ptr(display_list) as usize;
        // Shared with the e2e harness on purpose: this logic used to live only
        // here, and layout/src/e2e/cpu_backend.rs had no notion of a translate
        // hint at all — zero mentions of TranslateHint, dominant_delta or the
        // already-shifted guard. So no e2e scenario could execute the blit path,
        // which is exactly where "typing does not repaint" lives.
        let patch_hint: Option<(cpurender::TranslateHint, Vec<azul_core::geom::LogicalRect>)> =
            cpurender::translate_hint_for_patch(
                layout_window.layout_cache.last_patch_move.as_ref(),
                dpi_factor,
                can_reuse_previous_frame,
                self.last_patch_shift_dl == dl_arc_ptr,
            );

        // Compute display list damage (incremental path)
        let mut patch_moved_union: Option<azul_core::geom::LogicalRect> = None;
        let dl_damage = match &self.previous_display_list {
            Some(old_dl) if can_reuse_previous_frame && !gpu_damage.needs_full => {
                cpurender::compute_display_list_damage_translated(
                    old_dl,
                    display_list,
                    &self.previous_scroll_offsets,
                    &scroll_offsets,
                    patch_hint.as_ref().map(|(h, _)| h),
                )
                .map(|(rects, moved)| {
                    patch_moved_union = moved;
                    rects
                })
            }
            _ => None, // first frame, shrink or ref-frame transform → full repaint
        };

        // VirtualView child-DOM damage. A child DOM (e.g. the MapWidget tile
        // grid) re-renders IN PLACE when async content arrives (a tile writeback
        // re-invokes the VirtualView), but the PARENT display list's VirtualView
        // item is byte-identical — so `compute_display_list_damage` above sees no
        // change and `render_frame` would take the "nothing changed → skip" path,
        // freezing the child content. Build the child DLs now and diff them
        // against last frame's; any that changed get their on-screen bounds
        // damaged below. This is why the map showed only the placeholder grid on
        // Wayland — which, unlike X11, gets no spurious WM expose/configure events
        // to force a full repaint and mask the bug.
        let vview_dls: std::collections::BTreeMap<DomId, std::sync::Arc<azul_layout::solver3::display_list::DisplayList>> =
            layout_window
                .layout_results
                .iter()
                .filter(|(id, _)| id.inner != dom_id.inner)
                .map(|(id, r)| (*id, r.display_list.clone()))
                .collect();
        let vview_damage = cpurender::compute_virtual_view_damage(
            display_list, &vview_dls, &self.previous_vview_dls,
        );
        let has_vview_damage = !vview_damage.is_empty();
        self.previous_vview_dls = vview_dls.clone();

        // #13/#14: scroll. The display list is UNCHANGED on scroll — content
        // items live at content coords and the scroll is applied at render time
        // via render_state.scroll_offsets — so the diff above only ever catches
        // the scrollbar, leaving the content frozen. Build the real scroll
        // offsets and, for any frame whose offset changed vs the previous frame,
        // record the (clip, delta) so we can MOVE the still-visible pixels and
        // repaint only the strip that scrolled into view (#14 thin-strip paint).
        // The actual pixel move + exposed-strip damage happens after `output` is
        // acquired (see `scroll_shift_region` below); here we only collect the
        // work, since the pixmap is not available yet. (`scroll_offsets` was
        // built above, before the display-list diff.)
        //
        // (scroll_id, clip, delta, new_offset) per frame whose offset changed.
        // LocalScrollId is a u64 alias.
        let mut scroll_shifts: Vec<(
            u64,
            azul_core::geom::LogicalRect,
            (f32, f32),
            (f32, f32),
        )> = Vec::new();
        for (scroll_id, offset) in &scroll_offsets {
            let prev = self
                .previous_scroll_offsets
                .get(scroll_id)
                .copied()
                .unwrap_or((0.0, 0.0));
            let delta = (offset.0 - prev.0, offset.1 - prev.1);
            // Threshold in PHYSICAL pixels: a delta that moves the content by
            // at least half a device pixel must repaint (at dpi=2 a 0.3-logical
            // wheel step is already a visible 0.6-device-px move).
            if (delta.0 * dpi_factor).abs() > 0.5 || (delta.1 * dpi_factor).abs() > 0.5 {
                for item in display_list.items.iter() {
                    if let azul_layout::solver3::display_list::DisplayListItem::PushScrollFrame {
                        clip_bounds,
                        scroll_id: sid,
                        ..
                    } = item
                    {
                        if sid == scroll_id {
                            scroll_shifts.push((*sid, *clip_bounds.inner(), delta, *offset));
                        }
                    }
                }
            }
        }
        let has_scroll = !scroll_shifts.is_empty();
        // Advance the scroll baseline ONLY for frames that actually get painted
        // at their new offset this call (shifted frames now; ALL frames on the
        // full-repaint path — finalised at the end of render_frame). Frames
        // whose sub-half-pixel delta was dropped keep their previous baseline,
        // so slow trackpad scrolling ACCUMULATES until it crosses a device
        // pixel instead of being silently swallowed frame after frame (content
        // frozen while the logical offset advances arbitrarily far).
        let shifted_ids: BTreeSet<u64> =
            scroll_shifts.iter().map(|(sid, ..)| *sid).collect();
        let next_scroll_baseline: azul_layout::cpurender::ScrollOffsetMap = scroll_offsets
            .iter()
            .map(|(id, off)| {
                if shifted_ids.contains(id) {
                    (*id, *off)
                } else {
                    (
                        *id,
                        self.previous_scroll_offsets
                            .get(id)
                            .copied()
                            .unwrap_or(*off),
                    )
                }
            })
            .collect();

        // Determine render path. Scroll strips are added AFTER the output pixmap
        // is acquired (the pixel move needs the buffer), so the incremental arm
        // starts with only display-list + resize damage.
        let mut all_damage: Vec<azul_core::geom::LogicalRect>;
        let is_incremental;

        // A PATCHED build may change the item count, which the old-vs-new
        // item diff reads as structural (None -> full). The patch recorded
        // its own precise damage at build time — and on a PATCHED build it is
        // AUTHORITATIVE, not a fallback: the item diff pairs items by index,
        // and a same-count splice (re-emitted node, translated neighbours)
        // mis-pairs old-vs-new items and under-damages (one stale rect where
        // the reflow moved three nodes). The diff stays the source of truth
        // only for unpatched incremental passes. Guarded to the same
        // conditions the diff itself ran under (a gpu needs_full / shrink /
        // first frame must stay a full repaint).
        let diff_path_ran = self.previous_display_list.is_some()
            && can_reuse_previous_frame
            && !gpu_damage.needs_full;
        if std::env::var_os("AZ_PATCH_DEBUG").is_some() {
            eprintln!(
                "[HLDMG-PRE] item_diff={:?} prev_is_same_arc={} prev_items={:?} new_items={}",
                dl_damage,
                self.previous_display_list
                    .as_ref()
                    .is_some_and(|p| std::sync::Arc::ptr_eq(p, display_list)),
                self.previous_display_list.as_ref().map(|p| p.items.len()),
                display_list.items.len(),
            );
        }
        // EVERY patched build since this backend last presented, not just
        // the last one: a css patch and the RefreshDom it returns are two
        // patched builds in one pass, each damaged relative to the layout
        // before it, and the second knows nothing about the rect the first
        // vacated. Replaying only the last one left a thumb behind on every
        // slider drag.
        let pending = layout_window
            .layout_cache
            .pending_patch_damage(self.last_consumed_build_seq);
        let dl_damage = if diff_path_ran && layout_window.layout_cache.last_build_was_patched {
            use azul_layout::solver3::cache::PendingPatchDamage as P;
            // On a PATCHED build the patch's own damage AUGMENTS the item
            // diff (union), and stands alone when the diff bails to None on
            // an item-count change. Never replace a Some(diff) wholesale:
            // unpatched-equal frames keep baseline damage exactly.
            match (dl_damage, pending) {
                // An EMPTY diff on a patched build means the splice produced a
                // byte-identical list (same-text re-shape) — the frame is IDLE
                // and must stay idle; painting patch rects here flips the
                // idle-skip and drifts the frame scheduling (scrollbar-fade
                // clock) off the baseline.
                (Some(d), P::Rects(_)) if d.is_empty() => Some(d),
                (Some(mut d), P::Rects(p)) => {
                    d.extend(p);
                    Some(d)
                }
                (None, P::Rects(p)) => Some(p),
                // A full emission went unpresented: the item diff is the
                // authority, and its bail is a full repaint.
                (d, P::FullBuildSincePresent) => d,
                (d, P::None) => d,
                // Fell behind the log: nothing to replay, repaint in full.
                (_, P::Unknown) => None,
            }
        } else {
            dl_damage
        };
        if std::env::var_os("AZ_PATCH_DEBUG").is_some() {
            eprintln!(
                "[HLDMG] dl_damage={:?} can_reuse={} patched={} patch_damage={:?} resize={:?}",
                dl_damage,
                can_reuse_previous_frame,
                layout_window.layout_cache.last_build_was_patched,
                layout_window.layout_cache.last_patch_damage,
                resize_damage,
            );
        }
        match dl_damage {
            Some(rects)
                if rects.is_empty()
                    && !needs_resize
                    && resize_damage.is_empty()
                    && !has_scroll
                    && !has_vview_damage
                    && !has_gpu_damage
                    && zombie_damage.is_empty()
                    // A pending patch MOVE with empty damage is NOT an idle
                    // frame — skipping would swallow the translation.
                    && patch_moved_union.is_none() =>
            {
                // `!needs_resize` is load-bearing now that a resize can reach
                // this match at all: skipping leaves `last_frame` at the OLD
                // dimensions while the compositor is already at the new ones, so
                // the host would publish (and present) a wrongly-sized buffer.
                // A frame whose backing store changed size is never "nothing".
                //
                // Nothing changed — skip rendering entirely. (`!has_vview_damage`
                // keeps us out of this branch when only a VirtualView child DOM
                // changed — that case must still re-composite, see below.)
                self.previous_display_list = Some(display_list.clone());
                self.last_consumed_build_seq = layout_window.layout_cache.build_seq;
                // Nothing painted: baseline keeps accumulating dropped
                // sub-pixel scroll deltas (see next_scroll_baseline above).
                self.previous_scroll_offsets = next_scroll_baseline;
                self.last_frame_damage = FrameDamage::None;
                self.last_present_damage = FrameDamage::None;
                // IDLE FRAME — the one safe place to do cache GC.
                //
                // `GlyphCache::gc()` frees the previous generation, which is
                // thousands of PathStorage/cell vectors. That must never land
                // on a keystroke, and it must NOT run every frame either:
                // the two-generation scheme depends on `prev` surviving long
                // enough for a rotated-out glyph to be promoted back, so
                // GC-ing after every present would collapse it to a single
                // generation and reintroduce the rebuild storm it exists to
                // prevent.
                //
                // An idle frame is the right compromise: nothing is being
                // painted, so the work is free in wall-clock terms, and by
                // definition the user has stopped typing — which is exactly
                // when a generation has gone cold.
                self.glyph_cache.gc();
                return Vec::new();
            }
            // The display-list diff plus, on a grow, the newly-exposed L. The
            // guard used to be `!needs_resize`, which meant a grow BUILT the
            // bounded repaint (`compute_resize_damage` + `resize_grow_only`
            // preserving the old pixels) and then threw it away: `dl_damage` was
            // forced to `None`, the match fell through to `_`, the buffer was
            // filled white and everything was repainted — `FrameDamage::Full`
            // for a window that only grew by a strip.
            Some(mut rects) if can_reuse_previous_frame => {
                // Incremental: changed items + (scroll strips added below)
                rects.extend(resize_damage);
                all_damage = rects;
                is_incremental = true;
            }
            _ => {
                // Full repaint (first frame, structural change, shrink). Scroll
                // offsets are applied fresh by the full render, so no pixel move.
                all_damage = resize_damage;
                is_incremental = false;
            }
        }

        // A VirtualView child DOM changed (async content) — damage its on-screen
        // region so the incremental path re-composites it. The full-repaint path
        // redraws everything anyway, so this only matters when incremental.
        if is_incremental && has_vview_damage {
            all_damage.extend(vview_damage);
        }

        // GPU-value changes (thumb move / fade tick) repaint their bound items.
        if is_incremental && !gpu_damage.rects.is_empty() {
            all_damage.extend(gpu_damage.rects.iter().copied());
        }
        if is_incremental && !zombie_damage.is_empty() {
            all_damage.extend(zombie_damage.iter().copied());
        }

        // #27 native backbuffer: a platform shell may hand the free shm slot
        // as the render target — the frame is then rasterised IN PLACE inside
        // the window's own buffer and no owned copy is retained. The shell
        // guarantees the buffer already holds the PREVIOUS frame (cross-slot
        // catch-up) and outlives this call; dimensions are re-checked here so
        // a configure race falls back to the owned path instead of clipping.
        let native = match self.native_target.take() {
            Some(ext) if ext.width() == pixel_w && ext.height() == pixel_h => Some(ext),
            Some(ext) => {
                log_error!(
                    LogCategory::Rendering,
                    "[native-bb] armed target {}x{} != frame {}x{} — owned-path fallback",
                    ext.width(),
                    ext.height(),
                    pixel_w,
                    pixel_h
                );
                None
            }
            None => None,
        };
        self.rendered_native = native.is_some();

        // Acquire output pixmap — reuse buffer for both grow and shrink
        let mut output = match native {
            Some(ext) => {
                // The platform buffer replaces the retained frame wholesale
                // (keeping an owned copy would defeat #27).
                self.last_frame = None;
                ext
            }
            None => match self.last_frame.take() {
                Some(p) if p.width() == pixel_w && p.height() == pixel_h => p,
                Some(mut p) => {
                    p.resize_reuse(pixel_w, pixel_h, 255, 255, 255, 255);
                    p
                }
                None => match cpurender::AzulPixmap::new(pixel_w, pixel_h) {
                    Some(mut p) => { p.fill(255, 255, 255, 255); p }
                    None => {
                        // Same contract as the early returns at the top: a call
                        // that produced no frame must not leave stale damage.
                        self.last_frame_damage = FrameDamage::None;
                        self.last_present_damage = FrameDamage::None;
                        return Vec::new();
                    }
                },
            },
        };

        // #14: thin-strip scroll. On the incremental path, MOVE the pixels that
        // are still visible inside each scrolled frame and repaint only the strip
        // that scrolled into view, instead of re-rasterising the whole viewport.
        // The move happens directly on `output`; the returned strips are added to
        // the damage set so `render_display_list_damaged` repaints just them. (On
        // the full-repaint path the whole frame is redrawn anyway, so no move.)
        // #20: the memmove is only correct when the scrolling content opaquely
        // covers the clip OR nothing is painted behind the frame — otherwise the
        // shift would drag static backdrop pixels. `scroll_fast_path_eligible`
        // proves the bug condition; when ineligible we full-repaint the clip (no
        // shift) so the static backdrop + re-offset content render correctly.
        // Regions that were pixel-SHIFTED: painted as a thin strip but the whole
        // clip changed on screen, so they belong to PRESENT damage (not paint).
        let mut present_extra: Vec<azul_core::geom::LogicalRect> = Vec::new();
        // ROUND 3: the layout-translation blit. The translated diff classified
        // the dominant movers as MOVES (no damage) — shift their pixels here
        // and repaint only the exceptions + exposed strips. Sign map: the
        // shifter takes SCROLL deltas (offset +d moves content by −d), so a
        // layout move BY +d passes delta −d anchored at offset (0,0).
        if is_incremental {
            if let (Some((hint, exceptions)), Some(moved)) =
                (patch_hint.as_ref(), patch_moved_union)
            {
                self.last_patch_shift_dl = dl_arc_ptr;
                let d = hint.delta;
                let _ = moved;
                // One blit PER MOVER RECT (never the union — the gaps
                // between movers are static backdrop that must not be
                // dragged). Clip per mover = old∪(old+delta) so both the
                // vacated source and the destination lie inside the
                // memmove region; exposed strips repaint per mover.
                let mover_rects = layout_window
                    .layout_cache
                    .last_patch_move
                    .as_ref()
                    .map(|m| m.mover_rects_old.clone())
                    .unwrap_or_default();
                if std::env::var_os("AZ_BLIT_DEBUG").is_some() {
                    eprintln!(
                        "[blit] delta={:?} movers={} exceptions={}",
                        d,
                        mover_rects.len(),
                        exceptions.len()
                    );
                    for m in &mover_rects {
                        eprintln!("[blit]   mover {:?}", m);
                    }
                    for e in exceptions.iter() {
                        eprintln!("[blit]   exception {:?}", e);
                    }
                }
                for mr in &mover_rects {
                    let dest = azul_core::geom::LogicalRect {
                        origin: azul_core::geom::LogicalPosition {
                            x: mr.origin.x + d.0,
                            y: mr.origin.y + d.1,
                        },
                        size: mr.size,
                    };
                    let x0 = mr.origin.x.min(dest.origin.x);
                    let y0 = mr.origin.y.min(dest.origin.y);
                    let x1 = (mr.origin.x + mr.size.width)
                        .max(dest.origin.x + dest.size.width);
                    let y1 = (mr.origin.y + mr.size.height)
                        .max(dest.origin.y + dest.size.height);
                    let clip = azul_core::geom::LogicalRect {
                        origin: azul_core::geom::LogicalPosition { x: x0, y: y0 },
                        size: azul_core::geom::LogicalSize {
                            width: x1 - x0,
                            height: y1 - y0,
                        },
                    };
                    let shift_exact = if self.rendered_native && self.native_target_pool_order {
                        cpurender::scroll_shift_region_exact_pool_order
                    } else {
                        cpurender::scroll_shift_region_exact
                    };
                    let strips = shift_exact(
                        &mut output,
                        &clip,
                        (-d.0, -d.1),
                        (0.0, 0.0),
                        dpi_factor,
                    );
                    // Inflate the vacated strips by 1px: LCD fringe of a run
                    // hugging the mover's edge hangs one device pixel OUTSIDE
                    // the mover rect, so the un-inflated vacated region leaves
                    // that column stale after the move (the full-repaint
                    // control clears it via the diff's own text inflation —
                    // the gate diverged by exactly that column). A wider strip
                    // that now cuts a destination run is already handled by
                    // the fringe-touching-runs-damaged-whole rule below.
                    let strips: Vec<azul_core::geom::LogicalRect> = strips
                        .iter()
                        .map(|r| azul_core::geom::LogicalRect {
                            origin: azul_core::geom::LogicalPosition {
                                x: r.origin.x - 1.0,
                                y: r.origin.y - 1.0,
                            },
                            size: azul_core::geom::LogicalSize {
                                width: r.size.width + 2.0,
                                height: r.size.height + 2.0,
                            },
                        })
                        .collect();
                    all_damage.extend(strips.iter().copied());
                    // LCD text is FIR-fringed: a run STARTING at the blit
                    // destination hangs 1px of fringe INTO the vacated
                    // strip. A strip-clipped repaint cannot reproduce that
                    // fringe (the colorimetric blend reads neighbours), so
                    // any text run whose 1px-inflated bounds touch a strip
                    // is damaged WHOLE — cleared and re-rendered exactly
                    // like the full-repaint control.
                    for strip in &strips {
                        for (idx, item) in display_list.items.iter().enumerate() {
                            let _ = idx;
                            if let azul_layout::solver3::display_list::DisplayListItem::Text {
                                ..
                            } = item
                            {
                                if let Some(b) = item.bounds() {
                                    let inflated = azul_core::geom::LogicalRect {
                                        origin: azul_core::geom::LogicalPosition {
                                            x: b.origin.x - 1.0,
                                            y: b.origin.y - 1.0,
                                        },
                                        size: azul_core::geom::LogicalSize {
                                            width: b.size.width + 2.0,
                                            height: b.size.height + 2.0,
                                        },
                                    };
                                    let ix = inflated.origin.x < strip.origin.x + strip.size.width
                                        && strip.origin.x < inflated.origin.x + inflated.size.width
                                        && inflated.origin.y < strip.origin.y + strip.size.height
                                        && strip.origin.y
                                            < inflated.origin.y + inflated.size.height;
                                    if ix {
                                        all_damage.push(inflated);
                                    }
                                }
                            }
                        }
                    }
                    present_extra.push(clip);
                }
                all_damage.extend(exceptions.iter().copied());
            }
            for (scroll_id, clip, delta, offset) in &scroll_shifts {
                // The pixels being dragged were composited at the PREVIOUS
                // offset — eligibility (opaque coverage) must hold there too,
                // or a backdrop fragment visible through a gap at the old
                // offset gets dragged into the kept region.
                let prev_offset = (offset.0 - delta.0, offset.1 - delta.1);
                if cpurender::scroll_fast_path_eligible(
                    display_list,
                    *scroll_id,
                    clip,
                    *offset,
                    prev_offset,
                ) {
                    let shift = if self.rendered_native && self.native_target_pool_order {
                        cpurender::scroll_shift_region_pool_order
                    } else {
                        cpurender::scroll_shift_region
                    };
                    let strips = shift(
                        &mut output,
                        clip,
                        *delta,
                        *offset,
                        dpi_factor,
                    );
                    all_damage.extend(strips);
                    // Items composited OVER the frame inside its clip (its own
                    // scrollbar, an open dropdown/tooltip) were just dragged by
                    // the memmove — repaint their clip intersection so no
                    // smeared copy survives.
                    all_damage.extend(cpurender::overlay_rects_after_frame(
                        display_list,
                        *scroll_id,
                        clip,
                    ));
                    // The shift moved the whole clip on screen → present it all.
                    present_extra.push(*clip);
                } else {
                    // Ineligible: repaint the whole clip with the new offset.
                    all_damage.push(*clip);
                }
            }
        }

        // Merge duplicates/overlaps accumulated from the independent damage
        // sources (DL diff, vview, strips, overlay-after-shift): the renderer
        // merges overlapping rects internally anyway, but the recorded
        // paint/present damage (and the pixel-count metric built on it) must
        // not double-count the same region.
        if is_incremental {
            cpurender::coalesce_damage_rects(&mut all_damage);
        }

        // Build render state from the GPU value cache (opacity/transform) + scroll
        // offsets — the SAME construction the real X11/Wayland CPU paths use, so
        // this render_frame is reusable by them. The headless harness has an empty
        // GPU cache, so this is equivalent to `new(scroll_offsets)` there.
        let gpu_cache = layout_window.gpu_state_manager.get_cache(dom_id);
        // `vview_dls` (the nested VirtualView child DOM display lists — e.g. the
        // MapWidget's tile grid) was built earlier for the child-DOM damage diff;
        // it's handed to the renderer here so the CPU `VirtualView` arm can
        // composite them. Without this the CPU backend only drew a placeholder.
        if std::env::var("AZ_MAP_DEBUG").is_ok() {
            let summary: std::vec::Vec<(usize, usize)> =
                vview_dls.iter().map(|(id, dl)| (id.inner, dl.items.len())).collect();
            let all_ids: std::vec::Vec<usize> =
                layout_window.layout_results.keys().map(|k| k.inner).collect();
            eprintln!(
                "[cpu-vview] render_frame: layout_results ids={:?}, vview_dls (id,items)={:?}",
                all_ids, summary
            );
            // Item-kind census of the ROOT display list being rendered + whether
            // the maps header's #2b2b2b background rect made it in.
            use azul_layout::solver3::display_list::DisplayListItem as I;
            let mut rects = 0; let mut texts = 0; let mut vviews = 0; let mut other = 0;
            let mut dark_rect = false;
            for it in display_list.items.iter() {
                match it {
                    I::Rect { color, .. } => {
                        rects += 1;
                        if color.r == 0x2b && color.g == 0x2b && color.b == 0x2b { dark_rect = true; }
                    }
                    I::Text { .. } | I::TextLayout { .. } => texts += 1,
                    I::VirtualView { .. } | I::VirtualViewPlaceholder { .. } => vviews += 1,
                    _ => other += 1,
                }
            }
            eprintln!(
                "[cpu-vview] ROOT DL census: total={} rects={} texts={} vviews={} other={} header_dark_rect={}",
                display_list.items.len(), rects, texts, vviews, other, dark_rect
            );
            // One-shot full item dump (first frame only): every Push/Pop with
            // bounds — the header is dropped by SOMETHING among these.
            use std::sync::atomic::{AtomicBool, Ordering as AOrd};
            static DUMPED_ITEMS: AtomicBool = AtomicBool::new(false);
            if !DUMPED_ITEMS.swap(true, AOrd::Relaxed) {
                for (i, it) in display_list.items.iter().enumerate() {
                    let desc = match it {
                        I::Rect { color, bounds, .. } => format!(
                            "Rect rgb({},{},{}) {:?}", color.r, color.g, color.b, bounds.inner()),
                        I::Text { .. } => "Text".to_string(),
                        I::TextLayout { .. } => "TextLayout".to_string(),
                        I::VirtualView { bounds, .. } => format!("VView {:?}", bounds.inner()),
                        I::VirtualViewPlaceholder { bounds, .. } =>
                            format!("VViewPh {:?}", bounds.inner()),
                        other => {
                            // Debug-print the variant; truncate to keep one line.
                            let s = format!("{:?}", other);
                            s.chars().take(110).collect::<String>()
                        }
                    };
                    eprintln!("[cpu-vview]   [{i:2}] {desc}");
                }
            }
        }
        // Incremental repaints must raster at the offsets the surrounding
        // (un-repainted) pixels are ALREADY at — the baseline. For shifted
        // frames baseline == current; for frames whose sub-pixel delta was
        // dropped it is the last-painted offset, so a band repainted for
        // unrelated damage stays aligned with the rest of the frame. The full
        // path repaints everything and uses the current offsets.
        let render_offsets = if is_incremental {
            &next_scroll_baseline
        } else {
            &scroll_offsets
        };
        let render_state =
            cpurender::CpuRenderState::from_gpu_cache(gpu_cache, dom_id, render_offsets)
                .with_system_style(layout_window.system_style.clone())
                .with_virtual_view_display_lists(vview_dls);

        if is_incremental && !all_damage.is_empty() {
            // Incremental: render only damaged regions
            let _ = cpurender::render_display_list_damaged(
                display_list, &mut output, dpi_factor,
                renderer_resources, &layout_window.font_manager,
                &mut self.glyph_cache, &render_state, &all_damage,
            );
            // Exits paint ON TOP of the restored live pixels; their current
            // rects are inside `all_damage` by construction.
            if zombies_active {
                layout_window.composite_zombies_cpu(
                    &mut output,
                    dpi_factor,
                    renderer_resources,
                    &mut self.glyph_cache,
                );
            }
        } else {
            // Full render
            output.fill(255, 255, 255, 255);
            compositor.allocate_layers_from_display_list(
                display_list,
                dpi_factor,
                &render_state.transforms,
                &render_state.opacities,
            );
            if let Err(e) = compositor.render_layers(
                display_list, dpi_factor, renderer_resources,
                &layout_window.font_manager, &mut self.glyph_cache,
                &render_state,
            ) {
                log_error!(
                    LogCategory::Rendering,
                    "[CpuBackend] render_layers error: {}",
                    e
                );
            }
            compositor.composite_frame(&mut output, dpi_factor);
            // The design doc's invariant: the rendered frame is B ∪ zombies.
            layout_window.composite_zombies_cpu(
                &mut output,
                dpi_factor,
                renderer_resources,
                &mut self.glyph_cache,
            );
        }

        // AZ_DUMP_FRAME_DIR=/tmp/frames dumps every rendered CPU frame as a
        // numbered PNG — splits "rendered wrong" from "presented wrong" when a
        // backend shows pixels that contradict the display list.
        if let Ok(dir) = std::env::var("AZ_DUMP_FRAME_DIR") {
            use std::sync::atomic::{AtomicUsize, Ordering};
            static FRAME_N: AtomicUsize = AtomicUsize::new(0);
            let n = FRAME_N.fetch_add(1, Ordering::Relaxed);
            if n < 40 {
                if let Ok(bytes) = output.encode_png() {
                    let _ = std::fs::create_dir_all(&dir);
                    let _ = std::fs::write(
                        format!("{}/frame_{:03}_{}.png", dir, n,
                            if is_incremental { "inc" } else { "full" }),
                        bytes,
                    );
                }
            }
        }

        self.previous_zombie_rects = zombie_rects;
        self.previous_display_list = Some(display_list.clone());
        self.last_consumed_build_seq = layout_window.layout_cache.build_seq;
        // Full render paints EVERY frame at its current offset → baseline is
        // the current offsets. Incremental: only shifted frames advanced.
        self.previous_scroll_offsets = if is_incremental {
            next_scroll_baseline
        } else {
            scroll_offsets.clone()
        };
        if output.is_external() {
            // #27: the pixels live in the platform's backbuffer and the shell
            // presents them from there. Retaining the borrowed wrapper would
            // dangle once the shm pool is recreated — keep nothing.
            self.last_frame = None;
        } else {
            self.last_frame = Some(output);
        }
        self.last_frame_damage = if is_incremental {
            FrameDamage::Rects(all_damage.clone())
        } else {
            FrameDamage::Full
        };
        // Present damage = paint damage ∪ the full clips that were pixel-shifted
        // (their content moved on screen even though only a strip was repainted).
        self.last_present_damage = if is_incremental {
            let mut present = all_damage.clone();
            present.extend(present_extra);
            FrameDamage::Rects(present)
        } else {
            FrameDamage::Full
        };
        all_damage
    }
}

// ---------------------------------------------------------------------------
// HeadlessWindow
// ---------------------------------------------------------------------------

/// Shared wake-up state for the condvar-based event loop.
///
/// The `Condvar` is signalled whenever new work is available (event
/// injected, timer registered, thread completed).  This lets the
/// blocking loop sleep with zero CPU usage when idle.
struct WakeState {
    /// `true` when the loop should re-check for work.
    woken: bool,
}

/// Headless / CPU-only window implementing the full `PlatformWindow` trait.
///
/// Behaves identically to platform windows for layout, callbacks, and state
/// management.  Instead of a GPU context it holds a [`CpuBackend`] for
/// hit-testing and optional CPU rendering.
pub struct HeadlessWindow {
    /// Common window state (layout, resources, etc.) — shared with all platforms.
    pub common: CommonWindowState,
    /// CPU rendering backend (replaces WebRender).
    pub cpu_backend: CpuBackend,
    /// Whether the window is "open".
    is_open: bool,
    /// Event queue for programmatic event injection.
    event_queue: VecDeque<HeadlessEvent>,
    /// Thread poll timer running flag.
    thread_poll_timer_running: bool,
    /// Pending window creation requests (for popup menus, dialogs, etc.).
    pub pending_window_creates: Vec<WindowCreateOptions>,
    /// Config snapshot (needed for spawning sub-windows).
    config: AppConfig,
    /// Icon provider (shared across all windows).
    icon_provider: SharedIconProvider,
    /// Font registry (needed for spawning sub-windows).
    font_registry: Option<Arc<FcFontRegistry>>,
    /// `WindowCreateOptions::create_callback`, deferred until `run()`.
    ///
    /// Every OS shell fires this after the window exists and before the
    /// first layout; headless dropped it, so an app installing its startup
    /// timer there idled forever under `AZ_BACKEND=headless`.
    create_callback: azul_layout::callbacks::OptionCallback,
    /// Condvar + mutex used to block the event loop until work arrives.
    wake_condvar: Arc<Condvar>,
    wake_mutex: Arc<Mutex<WakeState>>,
    /// Accessibility actions waiting to be applied.
    ///
    /// Named to match the four desktop backends' `accessibility_adapter`
    /// because it fills the same slot in the frame loop: headless has no OS
    /// assistive technology to talk to, so instead of an `accesskit` adapter
    /// this is a plain injectable queue. That is not a lesser thing here — it
    /// is the ONLY way a11y behaviour is testable at all, since headless is
    /// the backend the E2E corpus runs on. Fed by
    /// [`HeadlessWindow::inject_accessibility_action`], drained by
    /// [`HeadlessWindow::process_accessibility_actions`].
    pub accessibility_adapter: A11yActionQueue,
}

/// Timer poll interval — how often the loop re-checks when timers are
/// active.  16 ms = 60 Hz, matches the Linux select() timeout used
/// by the X11 backend.
const TIMER_POLL_MS: u64 = 16;

impl HeadlessWindow {
    /// Create a new headless window with the given options.
    ///
    /// This constructor mirrors the real platform window constructors:
    /// 1. Creates `LayoutWindow` with font cache
    /// 2. Initialises `CommonWindowState`
    /// 3. Sets up the `CpuBackend`
    ///
    /// No system resources (window handle, GL context) are allocated.
    pub fn new(
        options: WindowCreateOptions,
        app_data: Arc<RefCell<RefAny>>,
        undo_manager: event::SharedUndoManager,
        config: AppConfig,
        icon_provider: SharedIconProvider,
        fc_cache: Arc<FcFontCache>,
        font_registry: Option<Arc<FcFontRegistry>>,
    ) -> Result<Self, WindowError> {
        // Extract create_callback before consuming options (same as every
        // platform shell) — invoked in run() ahead of the initial layout.
        let create_callback = options.create_callback.clone();
        let full_window_state = options.window_state;

        // Create layout window — same as real platforms
        let mut layout_window = LayoutWindow::new(fc_cache.as_ref().clone())
            .map_err(|e| WindowError::PlatformError(format!("Layout init failed: {:?}", e)))?;
        // Headless = the e2e test driver: deterministic, no caret / selection
        // tween (a screenshot must never catch geometry mid-glide).
        layout_window.system_animations_override =
            Some(azul_core::resources::SystemAnimations::disabled());
        layout_window.current_window_state = full_window_state.clone();
        layout_window.routes = config.routes.clone();

        let wake_condvar = Arc::new(Condvar::new());
        let wake_mutex = Arc::new(Mutex::new(WakeState { woken: false }));

        let mut common = CommonWindowState::new(
            full_window_state,
            fc_cache,
            Arc::new(crate::desktop::app::discover_system_style()),
            app_data,
            undo_manager,
        );
        common.layout_window = Some(layout_window);
        common.cpu_hit_tester = Some(azul_layout::headless::CpuHitTester::new());

        Ok(Self {
            common,
            cpu_backend: CpuBackend::new(),
            is_open: true,
            event_queue: VecDeque::new(),
            thread_poll_timer_running: false,
            pending_window_creates: Vec::new(),
            config,
            icon_provider,
            font_registry,
            create_callback,
            wake_condvar,
            wake_mutex,
            accessibility_adapter: A11yActionQueue::new(),
        })
    }

    // === Lifecycle ===

    /// Poll the next event from the queue.
    pub fn poll_event(&mut self) -> Option<HeadlessEvent> {
        self.event_queue.pop_front()
    }

    /// Check if the window is still "open".
    pub fn is_open(&self) -> bool {
        self.is_open
    }

    /// Close the window.
    pub fn close(&mut self) {
        // WebRender's Renderer must be deinit()'d, not dropped — texture
        // deletion has to happen inside a frame. Never doing so crashed debug
        // builds on close and leaked GPU resources in release.
        self.common.deinit_renderer();
        self.is_open = false;
    }

    /// Drop every live `Thread` so its destructor runs BEFORE the process ends.
    ///
    /// `Thread::drop` sends `TerminateThread`, waits out the grace period and
    /// joins the worker. `std::process::exit` runs no destructors at all, so
    /// without this the workers are simply abandoned — which is what
    /// ThreadSanitizer reports as `thread leak ... in pthread_create`.
    ///
    /// Clearing the map is enough: the `Thread` values own the handles, so
    /// dropping them performs the terminate-and-join. Doing this from the
    /// window rather than the loop keeps it correct for every exit path that
    /// ends the process instead of unwinding.
    fn shutdown_threads(&mut self) {
        let Some(lw) = self.get_layout_window_mut() else {
            return;
        };
        if lw.threads.is_empty() {
            return;
        }
        log_info!(
            LogCategory::EventLoop,
            "[Headless] terminating {} background thread(s) before exit",
            lw.threads.len(),
        );
        lw.threads.clear();
    }

    // === Layout ===

    /// Regenerate layout and rebuild CPU hit-tester.
    ///
    /// This is the HeadlessWindow equivalent of `MacOSWindow::regenerate_layout()` /
    /// `WinWindow::regenerate_layout()` etc. It calls the shared
    /// `common::layout::regenerate_layout()` (which no longer requires WebRender
    /// types) and then rebuilds the `CpuHitTester` from the new layout results.
    pub fn regenerate_layout_inner(
        &mut self,
    ) -> Result<crate::desktop::shell2::common::layout::LayoutRegenerateResult, String> {
        // Consume the reason tag BEFORE borrowing the layout window: this is
        // the regeneration this window asked for, and the tag travels with
        // the request (see CommonWindowState::request_regeneration).
        let relayout_reason = self.common.take_relayout_reason();

        let borrows = self.common.layout_borrows();
        let layout_window = borrows.layout_window.ok_or("No layout window")?;

        // Collect debug messages if debug server is enabled
        let debug_enabled = crate::desktop::shell2::common::debug_server::is_debug_enabled();
        let mut debug_messages = if debug_enabled {
            Some(Vec::new())
        } else {
            None
        };

        // Call unified regenerate_layout from common module
        let result = crate::desktop::shell2::common::layout::regenerate_layout(
            layout_window,
            borrows.app_data,
            borrows.current_window_state,
            borrows.renderer_resources,
            borrows.gl_context_ptr,
            borrows.fc_cache,
            &self.font_registry,
            borrows.system_style,
            &self.icon_provider,
            &mut debug_messages,
            relayout_reason,
        )?;

        // Forward layout debug messages to the debug server's log queue
        if let Some(msgs) = debug_messages {
            for msg in msgs {
                crate::desktop::shell2::common::debug_server::log(
                    crate::desktop::shell2::common::debug_server::LogLevel::Debug,
                    crate::desktop::shell2::common::debug_server::LogCategory::Layout,
                    msg.message.as_str().to_string(),
                    None,
                );
            }
        }

        // Rebuild CPU hit-tester from new layout results
        if let Some(lw) = self.common.layout_window.as_ref() {
            self.cpu_backend.hit_tester.rebuild_from_layout_with_gpu(&lw.layout_results, Some(&lw.gpu_state_manager));
        }

        // Also rebuild the SHARED hit-tester that the common event-dispatch path
        // reads (perform_hit_test → update_hit_test_at). `cpu_backend.hit_tester`
        // above only feeds the headless render/screenshot path; pointer events
        // (real or synthetic via the debug server) resolve their target node
        // through `common.cpu_hit_tester`. Without this rebuild that tester stays
        // empty, so every click hit-tests to nothing and widget callbacks (e.g. a
        // button's on_click) never fire — clicks silently do nothing in headless.
        self.common.rebuild_cpu_hit_tester();

        // Drain any lifecycle events produced by reconciliation (Mount/Unmount/
        // Update/Resize) and dispatch them through the normal callback pipeline.
        // Doing this inside regenerate_layout keeps the headless test harness
        // self-contained: callers do not have to remember to pump lifecycle
        // events separately to see `.with_callback(EventFilter::Component(_))`
        // fire.

        // CPU-render the frame (retained compositor handles efficient resize)
        #[cfg(feature = "cpurender")]
        {
            let ws = self.common.current_window_state();
            let width = ws.size.dimensions.width;
            let height = ws.size.dimensions.height;
            let dpi = ws.size.dpi as f32 / 96.0;
            // MWA-C-gpu_state/MWA-D: deliberately NO per-frame scrollbar
            // fade refresh here (unlike the interactive backends) — the
            // audit rated the headless relayout-only cache update adequate
            // for snapshot rendering, and the wall-clock fade advancing
            // BETWEEN two renders broke the fast-scroll-vs-full-render
            // pixel-identity golden tests (the two frames must share cache
            // state to be comparable). Content preparation (image callbacks
            // through the chokepoint + journal clock) still runs: a canvas
            // is content, not animation — without it every callback image
            // rendered as the announced grey placeholder on headless.
            //
            // The thumb TRANSFORMS are refreshed all the same: they read no
            // clock (pure function of layout + the current scroll offsets),
            // so they cannot desynchronise two renders, and without them a
            // scroll that changes nothing else leaves every thumb parked at
            // the position of the last full layout — mis-painted, and
            // undamaged, because the display-list items compare equal and
            // only the GPU value diff can raise the bar's rect.
            if let Some(lw) = self.common.layout_window.as_mut() {
                lw.prepare_frame_content();
                lw.refresh_scrollbar_transforms();
            }
            if let Some(lw) = self.common.layout_window.as_ref() {
                self.cpu_backend.render_frame(
                    lw,
                    &self.common.renderer_resources,
                    width,
                    height,
                    dpi,
                );
            }
            // Publish the damage of the frame we just rendered onto the
            // LayoutWindow, where `CallbackInfo::get_layout_window()` — and
            // therefore an E2E assertion — can actually see it. Without this the
            // damage machinery is invisible from outside the engine.
            let paint = self.cpu_backend.last_frame_damage.clone();
            let present = self.cpu_backend.last_present_damage.clone();
            if let Some(lw) = self.common.layout_window.as_mut() {
                lw.record_frame(paint, present);
            }
        }

        // Deliberately NO request_regeneration here. This ran at the end of
        // every rendered frame ("mark that frame needs regeneration"), which
        // turned the headless loop into a perpetual full-DOM-rebuild cycle:
        // each frame re-invoked the user's layout() on the next tick, so every
        // runtime CSS patch (a gallery panel toggled open, a combobox list
        // shown) was silently reverted one frame later, incremental paths were
        // never exercised in E2E, and "did this idle frame do any work?" was
        // unanswerable. A frame is a RESPONSE to a request, never a producer
        // of one — new frames come from real requests (events, timers,
        // request_repaint, request_regeneration by callbacks).

        Ok(result)
    }

    /// Service one owed frame according to the tier a pass reported plus the
    /// pending regeneration / relayout-only requests — the same contract the
    /// four desktop loops implement (X11 `render_and_present`, wayland
    /// `generate_frame_if_needed`, windows `WM_PAINT`, macOS
    /// `build_atomic_txn`): relayout-only is tested FIRST and both flags are
    /// consumed, and the full `regenerate_layout()` — which re-invokes the
    /// user's `layout()` and therefore DISCARDS runtime CSS patches — runs
    /// ONLY when a DOM rebuild was actually requested. Headless used to map
    /// every redraw signal to `regenerate_layout()`, so a `set_css_property`
    /// patch (the gallery panel toggling open) survived exactly one frame.
    fn service_frame(&mut self, tier: azul_core::events::ProcessEventResult) {
        use azul_core::events::ProcessEventResult as R;

        // Mirror the desktop event-arm routing: a regenerate-tier result marks
        // the DOM rebuild; an incremental-relayout result means the chokepoint
        // ALREADY re-ran layout on the existing StyledDom, so the frame takes
        // the relayout-only path (raise-time guard: never downgrade a pending
        // rebuild).
        if tier >= R::ShouldRegenerateDomCurrentWindow {
            self.common
                .request_regeneration(azul_core::callbacks::RelayoutReason::RefreshDom);
        } else if tier == R::ShouldIncrementalRelayout {
            self.common.request_relayout_only();
        }

        let relayout_only = self.common.take_relayout_only();
        // The resize fast path folds into headless's existing arms: a full
        // regeneration (boundary crossed) lays out at the new size, and BOTH
        // other arms below call relayout_only(), which re-lays-out the
        // existing StyledDom at the current (new) size. Consuming the latch
        // here keeps it from leaking into a later frame.
        let _resize_relayout = self.common.take_resize_relayout();
        let regen_requested = self.common.take_regeneration();

        let (res, what) = if relayout_only {
            (self.relayout_only(), "relayout")
        } else if regen_requested {
            (self.regenerate_layout().map(|_| ()), "regeneration")
        } else {
            // Pure repaint (request_repaint, a paint-only change): render from
            // the existing DOM. relayout_only() re-lays-out the EXISTING
            // StyledDom and renders — it never re-invokes the user's layout(),
            // so runtime patches survive.
            (self.relayout_only(), "repaint")
        };
        if let Err(e) = res {
            log_error!(
                LogCategory::Layout,
                "[Headless] Frame service ({}) failed: {}",
                what,
                e
            );
        }
    }

    /// Re-run layout on the EXISTING (already mutated) `StyledDom` and re-render —
    /// the `ShouldIncrementalRelayout` path every other backend implements
    /// (macOS `apply_incremental_relayout_result`, windows/wayland
    /// `request_relayout_only`), and which headless was missing entirely.
    ///
    /// Headless used to answer *every* redraw signal with the full
    /// `regenerate_layout()`. For an in-place DOM mutation that is not just the
    /// slow path, it is the WRONG path: `regenerate_layout` short-circuits on
    /// `is_layout_equivalent(old, new)`, and after an in-place mutation "old" and
    /// "new" are the same DOM — so layout was skipped and the frame kept the
    /// pre-mutation shaped text and geometry forever (the stale screen).
    pub fn relayout_only(&mut self) -> Result<(), String> {
        let debug_enabled = crate::desktop::shell2::common::debug_server::is_debug_enabled();
        let mut debug_messages = if debug_enabled { Some(Vec::new()) } else { None };

        // The common method owns the finalize tail (the CPU hit-tester
        // rebuild) and the trait wrapper delivers the lifecycle events the
        // pass produced — see `PlatformWindow::incremental_relayout_dispatching`.
        self.incremental_relayout_dispatching(
            crate::desktop::shell2::common::event::IncrementalRelayout::Restyle,
            &mut debug_messages,
        )?;

        if let Some(msgs) = debug_messages {
            for msg in msgs {
                crate::desktop::shell2::common::debug_server::log(
                    crate::desktop::shell2::common::debug_server::LogLevel::Debug,
                    crate::desktop::shell2::common::debug_server::LogCategory::Layout,
                    msg.message.as_str().to_string(),
                    None,
                );
            }
        }

        // Same finalize tail as regenerate_layout: the backend's own
        // hit-tester (common's was rebuilt inside `incremental_relayout`),
        // CPU frame, damage.
        if let Some(lw) = self.common.layout_window.as_ref() {
            self.cpu_backend.hit_tester.rebuild_from_layout_with_gpu(&lw.layout_results, Some(&lw.gpu_state_manager));
        }

        #[cfg(feature = "cpurender")]
        {
            let ws = self.common.current_window_state();
            let width = ws.size.dimensions.width;
            let height = ws.size.dimensions.height;
            let dpi = ws.size.dpi as f32 / 96.0;
            // Content preparation + clockless thumb transforms — see the
            // fade-refresh note above.
            if let Some(lw) = self.common.layout_window.as_mut() {
                lw.prepare_frame_content();
                lw.refresh_scrollbar_transforms();
            }
            if let Some(lw) = self.common.layout_window.as_ref() {
                self.cpu_backend.render_frame(
                    lw,
                    &self.common.renderer_resources,
                    width,
                    height,
                    dpi,
                );
            }
            let paint = self.cpu_backend.last_frame_damage.clone();
            let present = self.cpu_backend.last_present_damage.clone();
            if let Some(lw) = self.common.layout_window.as_mut() {
                lw.record_frame(paint, present);
            }
        }

        // Same as regenerate_layout_inner above: a completed frame must not
        // re-arm regeneration (see the comment there).
        Ok(())
    }

    // === Event injection (for tests / debug server) ===

    /// Inject an event into the queue for the next poll cycle.
    ///
    /// Wakes the blocking event loop if it is sleeping on the condvar.
    pub fn inject_event(&mut self, event: HeadlessEvent) {
        self.event_queue.push_back(event);
        self.wake();
    }

    /// Inject multiple events at once.
    pub fn inject_events(&mut self, events: impl IntoIterator<Item = HeadlessEvent>) {
        self.event_queue.extend(events);
        self.wake();
    }

    /// Queue an accessibility action as if a screen reader had requested it.
    ///
    /// This is headless's substitute for AT-SPI `do_action` / UIA `Invoke` /
    /// `NSAccessibility` press: the four desktop backends receive an
    /// `accesskit::ActionRequest` on a bus and decode it to exactly this
    /// triple, then hand it to `process_accessibility_actions()`. There is no
    /// bus here, so the triple is the ingress.
    ///
    /// It exists so accessibility behaviour is *observable* — before it, no
    /// backend could be driven through `LayoutWindow::process_accessibility_action`
    /// from a test, which is why "screen-reader activation invokes no callback"
    /// could ship and stay shipped.
    ///
    /// The action is applied on the next loop iteration (or on the next
    /// explicit `process_accessibility_actions()` call), NOT synchronously —
    /// same as every real adapter, whose actions arrive off-loop and are
    /// drained by the frame pump.
    pub fn inject_accessibility_action(
        &mut self,
        dom_id: azul_core::dom::DomId,
        node_id: azul_core::dom::NodeId,
        action: azul_core::dom::AccessibilityAction,
    ) {
        self.accessibility_adapter.push(dom_id, node_id, action);
        self.wake();
    }

    /// Tell this window the system light/dark preference changed.
    ///
    /// headless has no compositor to hear it from, so — exactly like
    /// [`HeadlessWindow::inject_accessibility_action`] — the injection IS the
    /// ingress. Without it no test could drive a theme switch on any backend,
    /// and that is why "a runtime dark-mode toggle does nothing" survived on six
    /// of the seven backends: the one backend CI can run had no way to express
    /// the event.
    ///
    /// The real backends' shape is `windows/mod.rs`'s `WM_SETTINGCHANGE |
    /// WM_THEMECHANGED` arm: re-read the system style, update the window state,
    /// pump the events that fall out, then request a regeneration tagged
    /// [`RelayoutReason::ThemeChange`]. This does the same, minus the
    /// re-discovery — the caller supplies the theme, since there is no system
    /// setting here to read.
    ///
    /// Returns `false` if the theme was already the requested one, in which case
    /// nothing is dispatched and no frame is requested. A no-op switch should
    /// not cost a relayout, and a test asserting "N relayouts" should not have to
    /// know whether the theme happened to differ.
    pub fn set_system_theme(&mut self, theme: azul_core::window::WindowTheme) -> bool {
        if self.common.current_window_state().theme == theme {
            return false;
        }

        // previous_window_state is what the diff pipeline compares against to
        // decide that a ThemeChanged event fired; without this snapshot the
        // event is never determined and the callbacks never run.
        self.snapshot_window_state_baseline("headless.set_system_theme");
        self.common.update_unsynced_state(|ws| ws.theme = theme);

        // Same shape as the HeadlessEvent arms in `run()`: pump the events the
        // state change implies and let the result speak; there is no window
        // handle here to route a result to.
        let _ = self.process_window_events(0);

        self.common
            .request_regeneration(azul_core::callbacks::RelayoutReason::ThemeChange);
        self.wake();
        true
    }

    /// Drain queued accessibility actions, apply them, and dispatch the
    /// callbacks they map to.
    ///
    /// Mirrors `Win32Window::process_accessibility_actions` /
    /// `X11Window::process_accessibility_actions` exactly: poll the action
    /// source, route each through `LayoutWindow::process_accessibility_action`,
    /// mark the display list dirty for a non-empty affected set, dispatch the
    /// mapped callbacks and honour the `Update` they return. The shared body
    /// lives in `PlatformWindow::dispatch_accessibility_actions` so all seven
    /// backends cannot drift apart.
    #[cfg(feature = "a11y")]
    pub fn process_accessibility_actions(&mut self) {
        let mut actions = Vec::new();
        while let Some(action) = self.accessibility_adapter.poll_action() {
            actions.push(action);
        }
        if actions.is_empty() {
            return;
        }
        self.dispatch_accessibility_actions(actions);
        // UNCONDITIONAL, exactly like the four desktop backends. Half the
        // actions (Focus, Blur, ScrollUp/Down/Left/Right, SetScrollOffset,
        // ScrollIntoView, SetTextSelection) change manager state and return an
        // EMPTY affected-node map, because they map to no callback. Gating the
        // redraw on that map would mean a screen reader could move focus or
        // scroll the view and the window would never repaint it.
        self.request_redraw();
    }

    /// Simulate a window resize. Updates `current_window_state.size` to the
    /// new logical dimensions and tags the next `regenerate_layout()` call
    /// with `RelayoutReason::Resize` so the user's `LayoutCallback` sees
    /// the size change via `info.relayout_reason()` plus the live
    /// `info.window_size`. The next `regenerate_layout()` call will
    /// re-invoke `layout()` exactly the way the real platform handlers do.
    pub fn simulate_resize(&mut self, width: f32, height: f32) {
        use azul_core::geom::LogicalSize;
        // Contract shape (snapshot → mutate → pass): the size diff dispatches
        // `WindowResize` exactly as a native configure does, and no live delta
        // is left behind for the validation check to flag.
        self.snapshot_window_state_baseline("headless.simulate_resize");
        self.common
            .update_window_state(event::WindowStateSource::Os, |ws| {
                ws.size.dimensions = LogicalSize { width, height };
            });
        self.common.request_regeneration(azul_core::callbacks::RelayoutReason::Resize);
        let _ = self.process_window_events(0);
    }

    /// Read the queued reason for the next `regenerate_layout()` call.
    /// Useful for asserting in tests that an event handler tagged the
    /// upcoming relayout correctly.
    pub fn pending_relayout_reason(&self) -> azul_core::callbacks::RelayoutReason {
        self.common.regeneration_reason()
    }

    /// Convert a `KeyDown` virtual keycode into the locale-independent character
    /// fallback (delegating to [`VirtualKeyCode::get_lowercase`]) and, if a
    /// character is available, queue a synthetic `TextInput` event for the next
    /// poll cycle.
    ///
    /// This mirrors what platform IME paths do when no locale-specific composer
    /// is active: latin keys still produce a typed character without going
    /// through a full input-method round-trip.
    pub fn synthesize_character_input(&mut self, vk: VirtualKeyCode) -> Option<char> {
        let c = vk.get_lowercase()?;
        self.inject_event(HeadlessEvent::TextInput { text: c.to_string() });
        Some(c)
    }

    /// Replace the active touch point list. Updates `num_touches` to match.
    pub fn inject_touch_points(&mut self, points: impl IntoIterator<Item = TouchPoint>) {
        // Contract shape (snapshot → mutate → pass): the touch_state diff is
        // what dispatches TouchStart/Move/End, same as the native shells.
        self.snapshot_window_state_baseline("headless.inject_touch_points");
        let vec: TouchPointVec = points.into_iter().collect::<Vec<_>>().into();
        let touch_state = self.common.touch_state_mut();
        touch_state.num_touches = vec.len();
        touch_state.touch_points = vec;
        let _ = self.process_window_events(0);
        self.wake();
    }

    /// Set the desired fullscreen-transition style on the current window state
    /// flags. The next request to enter or leave fullscreen will honor this
    /// value (slow vs. fast on macOS).
    ///
    /// On platforms that do not distinguish slow/fast transitions this is a
    /// no-op for animation purposes but still recorded on the window state for
    /// observation.
    pub fn set_fullscreen_mode(&mut self, mode: FullScreenMode) {
        self.common
            .update_window_state(event::WindowStateSource::App, |ws| {
                let flags = &mut ws.flags;
                flags.fullscreen_mode = mode;
                // Fold the request into the current frame state so headless callers
                // can observe the transition without a real OS event loop.
                flags.frame = match mode {
                    FullScreenMode::SlowFullScreen | FullScreenMode::FastFullScreen => {
                        WindowFrame::Fullscreen
                    }
                    FullScreenMode::SlowWindowed | FullScreenMode::FastWindowed => {
                        WindowFrame::Normal
                    }
                };
            });
    }

    /// Returns `true` if every entry of `chord` is currently active in the
    /// window's keyboard state. Use to evaluate registered accelerator
    /// shortcuts (e.g. `[Ctrl, Key(VirtualKeyCode::S)]`) on each key event.
    pub fn matches_accelerator(&self, chord: &[AcceleratorKey]) -> bool {
        self.common
            .current_window_state()
            .keyboard_state
            .matches_accelerator(chord)
    }

    /// Drive a synthetic scroll delta through [`process_system_scroll`] and
    /// return the [`ScrollResult`] for assertion in tests.
    pub fn process_system_scroll(
        &mut self,
        delta: LogicalPosition,
        hit_scrollbar: bool,
    ) -> ScrollResult {
        azul_core::window::process_system_scroll(delta, hit_scrollbar)
    }

    /// Ask for another frame.
    ///
    /// The headless analogue of `XSendEvent(Expose)` / `InvalidateRect` /
    /// `setNeedsDisplay`: there is no compositor that could handle a
    /// repaint-only update in the CPU path (see the note in `run()`), so a
    /// redraw request is a full regeneration request plus a loop wake-up.
    pub fn request_redraw(&mut self) {
        self.common
            .request_regeneration(azul_core::callbacks::RelayoutReason::RefreshDom);
        self.wake();
    }

    /// Signal the condvar so the blocking loop wakes up.
    fn wake(&self) {
        if let Ok(mut guard) = self.wake_mutex.lock() {
            guard.woken = true;
            self.wake_condvar.notify_one();
        }
    }

    /// Check if any timers are currently active.
    pub fn has_active_timers(&self) -> bool {
        self.common.layout_window.as_ref()
            .map_or(false, |lw| !lw.timers.is_empty())
    }

    /// Get the number of pending window creation requests.
    pub fn pending_window_count(&self) -> usize {
        self.pending_window_creates.len()
    }

    /// Fire `WindowCreateOptions::create_callback` exactly once, before the
    /// first layout.
    ///
    /// X11/Wayland/Windows/macOS all invoke it after the window exists and
    /// BEFORE the first layout; headless is documented as behavior-parity
    /// ("callbacks fire, timers tick"), so it must too — an app that installs
    /// its startup timer here otherwise idles until killed under
    /// `AZ_BACKEND=headless`. Public so the e2e harness (which drives frames
    /// manually instead of calling the blocking `run()`) can fire it at the
    /// same point in the lifecycle.
    pub fn invoke_create_callback(&mut self) {
        // take, not clone: fires exactly once per window lifetime.
        let taken = core::mem::replace(
            &mut self.create_callback,
            azul_layout::callbacks::OptionCallback::None,
        );
        let Some(mut callback) = taken.into_option() else {
            return;
        };

        let app_data = self.common.app_data.clone();
        let borrows = self.prepare_callback_invocation();
        let mut app_data_ref = app_data.borrow_mut();

        let (changes, _update) = borrows.layout_window.invoke_single_callback(
            &mut callback,
            &mut *app_data_ref,
            &borrows.window_handle,
            borrows.gl_context_ptr,
            borrows.system_style.clone(),
            &azul_layout::callbacks::ExternalSystemCallbacks::rust_internal(),
            borrows.previous_window_state,
            borrows.current_window_state,
            borrows.renderer_resources,
        );

        drop(app_data_ref);
        use crate::desktop::shell2::common::event::PlatformWindow;
        for change in &changes {
            let r = self.apply_user_change(change);
            if r != azul_core::events::ProcessEventResult::DoNothing {
                self.common
                    .request_regeneration(azul_core::callbacks::RelayoutReason::RefreshDom);
            }
        }
    }

    // === Blocking event loop ===

    /// Run the headless event loop — **blocks** until the window closes.
    ///
    /// This is the HeadlessWindow equivalent of `NSApplication.run()` / the
    /// Win32 `GetMessage` loop / the X11 `XNextEvent` loop.
    ///
    /// The loop uses a `Condvar` for zero-CPU blocking:
    /// * When timers are active it uses `wait_timeout` (16 ms / 60 Hz)
    ///   so timers get ticked even without external events.
    /// * When no timers are active it calls `wait` (indefinite) — the
    ///   thread is parked until `inject_event()`, `start_timer()`, or
    ///   another caller invokes `wake()`.
    /// * If nothing can ever wake the loop (no timers, no threads, no
    ///   debug server) a one-time warning is printed to stderr and the
    ///   loop blocks forever — identical to a desktop window nobody
    ///   interacts with.
    pub fn run(mut self) -> Result<(), WindowError> {
        let debug_enabled = debug_server::is_debug_enabled();
        let start = Instant::now();

        log_info!(
            LogCategory::EventLoop,
            "[Headless] Entering condvar-based blocking event loop (debug={})",
            debug_enabled,
        );

        // -- Invoke create_callback (behavior parity with the OS shells) --
        self.invoke_create_callback();

        // -- Perform initial layout (same as every platform) --
        log_debug!(
            LogCategory::Layout,
            "[Headless] Performing initial layout"
        );
        if let Err(e) = self.regenerate_layout() {
            log_warn!(
                LogCategory::Layout,
                "[Headless] WARNING: Initial layout failed: {}",
                e
            );
        }

        // -- Optional one-shot PNG snapshot --
        // `AZ_HEADLESS_SNAPSHOT_PATH=/tmp/out.png` writes the very
        // first rendered frame as PNG, then closes the window so the
        // process exits with code 0. Enables CI golden-image testing
        // without a full E2E harness: build the app, run with the env
        // var set, diff against a checked-in reference.
        #[cfg(feature = "cpurender")]
        if let Ok(path) = std::env::var("AZ_HEADLESS_SNAPSHOT_PATH") {
            if let Some(ref pixmap) = self.cpu_backend.last_frame {
                match pixmap.encode_png() {
                    Ok(bytes) => match std::fs::write(&path, &bytes) {
                        Ok(()) => log_info!(
                            LogCategory::Rendering,
                            "[Headless] AZ_HEADLESS_SNAPSHOT_PATH: wrote {} bytes to {}",
                            bytes.len(),
                            path,
                        ),
                        Err(e) => log_error!(
                            LogCategory::Rendering,
                            "[Headless] write({}): {}",
                            path,
                            e
                        ),
                    },
                    Err(e) => log_error!(
                        LogCategory::Rendering,
                        "[Headless] encode_png: {}",
                        e
                    ),
                }
            } else {
                log_warn!(
                    LogCategory::Rendering,
                    "[Headless] AZ_HEADLESS_SNAPSHOT_PATH set but no last_frame after initial layout — \
                     ensure the app's layout callback returns a non-empty DOM",
                );
            }
            // Exit cleanly so CI/test scripts get a deterministic
            // process termination after the snapshot is written.
            self.close();
        }

        // -- Exit once a frame has actually rendered --
        // `AZ_EXIT_SUCCESS_AFTER_FRAME_RENDER=1` closes the window as soon as
        // one frame exists, so the process exits 0 having PROVEN it rendered.
        //
        // This comment used to claim "windows, macos, x11 and wayland have
        // honoured this for a while". Only WINDOWS did. macos, x11 and wayland
        // never had it, so there was no non-interactive way to ask a real
        // compositor whether it rendered — the process just kept running and a
        // harness could only time out, which reads the same as a hang. wayland
        // has it now (see linux/wayland/mod.rs, after the surface_committed
        // guard); x11 and macos still do not — #56.
        //
        // headless did not honour it either, and that silently defeated the ASan gate. The CI
        // step sets AZ_BACKEND=headless plus this variable and then runs
        // `timeout 30 ./hello-world-asan || [ $? -eq 124 ]` — so the process
        // ALWAYS ran to the 30s wall, was killed with rc 124, and the `|| [ $? -eq
        // 124 ]` converted that into success. An app that rendered nothing, hung
        // on first layout, or deadlocked was indistinguishable from the healthy
        // path: all rc 124, all green. The gate still caught an ASan abort (rc
        // 134), but it had never once verified a frame rendered, which is
        // precisely what its own env var claims to check.
        //
        // Deliberately AFTER the snapshot block so both can be set together: the
        // snapshot writes the frame, this exits on it.
        if std::env::var("AZ_EXIT_SUCCESS_AFTER_FRAME_RENDER").is_ok() {
            #[cfg(feature = "cpurender")]
            let rendered = self.cpu_backend.last_frame.is_some();
            #[cfg(not(feature = "cpurender"))]
            let rendered = false;

            if rendered {
                log_info!(
                    LogCategory::Rendering,
                    "[Headless] AZ_EXIT_SUCCESS_AFTER_FRAME_RENDER: a frame rendered, exiting 0",
                );
                self.close();
            } else {
                // Do NOT close. Exiting 0 here would re-create the false green
                // this exists to remove — the caller must be able to tell "no
                // frame" apart from "frame rendered". Staying up means the
                // caller's timeout fires, which is now a real failure signal.
                log_warn!(
                    LogCategory::Rendering,
                    "[Headless] AZ_EXIT_SUCCESS_AFTER_FRAME_RENDER set but NO frame has rendered — \
                     not exiting, so the caller's timeout reports this as the failure it is",
                );
            }
        }

        // -- child windows (sub-HeadlessWindows for menus, dialogs) --
        let mut children: Vec<HeadlessWindow> = Vec::new();
        let mut warned_no_wake_sources = false;

        while self.is_open() {
            // ── Phase 1: Process injected events ─────────────────
            let mut events_need_redraw = false;
            // The strongest ProcessEventResult of this drain — decides whether
            // the frame below may rebuild the DOM or must keep it (see
            // service_frame).
            let mut events_result = azul_core::events::ProcessEventResult::DoNothing;
            while let Some(event) = self.poll_event() {
                match event {
                    HeadlessEvent::Close => {
                        self.close();
                    }
                    HeadlessEvent::FileHover { x, y, paths } => {
                        // MWA-A4: same ingress the OS backends perform —
                        // position + hit test + hovered-file into the manager,
                        // then an event pass (dispatches HoveredFile).
                        use azul_core::window::CursorPosition;
                        self.snapshot_window_state_baseline("headless.run.file_hover");
                        let pos = LogicalPosition { x, y };
                        self.common.mouse_state_mut().cursor_position =
                            CursorPosition::InWindow(pos);
                        self.update_hit_test_at(pos);
                        if let Some(lw) = self.common.layout_window.as_mut() {
                            // MWA-B7: full multi-file list, like the OS shells.
                            lw.file_drop_manager
                                .set_hovered_files(paths.into_iter().map(Into::into).collect());
                        }
                        let r = self.process_window_events(0);
                        events_result = events_result.max(r);
                        if !matches!(r, azul_core::events::ProcessEventResult::DoNothing) {
                            events_need_redraw = true;
                        }
                    }
                    HeadlessEvent::FileDrop { x, y, paths } => {
                        use azul_core::window::CursorPosition;
                        self.snapshot_window_state_baseline("headless.run.file_drop");
                        let pos = LogicalPosition { x, y };
                        self.common.mouse_state_mut().cursor_position =
                            CursorPosition::InWindow(pos);
                        self.update_hit_test_at(pos);
                        if let Some(lw) = self.common.layout_window.as_mut() {
                            lw.file_drop_manager
                                .set_dropped_files(paths.into_iter().map(Into::into).collect());
                        }
                        let r = self.process_window_events(0);
                        events_result = events_result.max(r);
                        if !matches!(r, azul_core::events::ProcessEventResult::DoNothing) {
                            events_need_redraw = true;
                        }
                        // Post-pass cleanup, mirroring the OS backends: the
                        // drop is a one-shot; hover state ends with it.
                        if let Some(lw) = self.common.layout_window.as_mut() {
                            lw.file_drop_manager.set_dropped_file(None);
                            lw.file_drop_manager.set_hovered_file(None);
                            lw.file_drop_manager.clear_hover_cancelled();
                        }
                    }
                    HeadlessEvent::FileHoverCancel => {
                        self.snapshot_window_state_baseline("headless.run.file_hover_cancel");
                        if let Some(lw) = self.common.layout_window.as_mut() {
                            // Some→None flags the cancel; the pass dispatches
                            // HoveredFileCancelled, then we clear the flag.
                            lw.file_drop_manager.set_hovered_file(None);
                        }
                        let r = self.process_window_events(0);
                        events_result = events_result.max(r);
                        if !matches!(r, azul_core::events::ProcessEventResult::DoNothing) {
                            events_need_redraw = true;
                        }
                        if let Some(lw) = self.common.layout_window.as_mut() {
                            lw.file_drop_manager.clear_hover_cancelled();
                        }
                    }
                    HeadlessEvent::MouseMove { x, y } => {
                        use azul_core::window::CursorPosition;
                        self.snapshot_window_state_baseline("headless.run.mouse_move");
                        let pos = LogicalPosition { x, y };
                        self.common.mouse_state_mut().cursor_position =
                            CursorPosition::InWindow(pos);
                        // MWA-C-scroll: active scrollbar thumb drag (desktop
                        // pattern) — scrollbar interaction was untestable in
                        // E2E because headless never routed it.
                        if self.common.scrollbar_drag_state.is_some() {
                            let r = PlatformWindow::handle_scrollbar_drag(&mut self, pos);
                            events_result = events_result.max(r);
                            if !matches!(r, azul_core::events::ProcessEventResult::DoNothing) {
                                events_need_redraw = true;
                            }
                            // SANCTIONED SWALLOW: the thumb drag consumed this
                            // motion; the cursor delta must not surface as a
                            // MouseMove event later. Same exception as the
                            // desktop shells.
                            PlatformWindow::discard_input_delta(
                                &mut self,
                                "headless.mouse_move.scrollbar_drag",
                            );
                        } else {
                            self.update_hit_test_at(pos);
                            record_headless_input(&mut self, false, false); // MWA-A4
                            let r = self.process_window_events(0);
                            events_result = events_result.max(r);
                            if !matches!(r, azul_core::events::ProcessEventResult::DoNothing) {
                                events_need_redraw = true;
                            }
                        }
                    }
                    HeadlessEvent::MouseDown { button } => {
                        self.snapshot_window_state_baseline("headless.run.mouse_down");
                        // MWA-C-scroll: scrollbar hit first (desktop pattern).
                        let sb_hit = if matches!(button, azul_core::events::MouseButton::Left) {
                            self.common
                                .current_window_state()
                                .mouse_state
                                .cursor_position
                                .get_position()
                                .and_then(|p| {
                                    PlatformWindow::perform_scrollbar_hit_test(&self, p)
                                        .map(|h| (h, p))
                                })
                        } else {
                            None
                        };
                        if let Some((hit, p)) = sb_hit {
                            self.common.mouse_state_mut().left_down = true;
                            let r = PlatformWindow::handle_scrollbar_click(&mut self, hit, p);
                            events_result = events_result.max(r);
                            if !matches!(r, azul_core::events::ProcessEventResult::DoNothing) {
                                events_need_redraw = true;
                            }
                            // SANCTIONED SWALLOW: the scrollbar consumed this
                            // press; the left_down delta must not surface as a
                            // MouseDown event later. Same exception as the
                            // motion arm above.
                            PlatformWindow::discard_input_delta(
                                &mut self,
                                "headless.mouse_down.scrollbar_click",
                            );
                        } else {
                        match button {
                            azul_core::events::MouseButton::Left => {
                                self.common.mouse_state_mut().left_down = true;
                            }
                            azul_core::events::MouseButton::Right => {
                                self.common.mouse_state_mut().right_down = true;
                            }
                            azul_core::events::MouseButton::Middle => {
                                self.common.mouse_state_mut().middle_down = true;
                            }
                            _ => {}
                        }
                        record_headless_input(&mut self, true, false); // MWA-A4
                        let r = self.process_window_events(0);
                        events_result = events_result.max(r);
                        if !matches!(r, azul_core::events::ProcessEventResult::DoNothing) {
                            events_need_redraw = true;
                        }
                        }
                    }
                    HeadlessEvent::MouseUp { button } => {
                        self.snapshot_window_state_baseline("headless.run.mouse_up");
                        // MWA-C-scroll: a release ends any scrollbar drag.
                        if self.common.scrollbar_drag_state.is_some() {
                            self.common.scrollbar_drag_state = None;
                            events_need_redraw = true;
                        }
                        match button {
                            azul_core::events::MouseButton::Left => {
                                self.common.mouse_state_mut().left_down = false;
                            }
                            azul_core::events::MouseButton::Right => {
                                self.common.mouse_state_mut().right_down = false;
                            }
                            azul_core::events::MouseButton::Middle => {
                                self.common.mouse_state_mut().middle_down = false;
                            }
                            _ => {}
                        }
                        record_headless_input(&mut self, false, true); // MWA-A4
                        let r = self.process_window_events(0);
                        events_result = events_result.max(r);
                        if !matches!(r, azul_core::events::ProcessEventResult::DoNothing) {
                            events_need_redraw = true;
                        }
                    }
                    HeadlessEvent::KeyDown { virtual_keycode } => {
                        self.snapshot_window_state_baseline("headless.run.key_down");
                        self.common.keyboard_state_mut().current_virtual_keycode =
                            azul_core::window::OptionVirtualKeyCode::Some(virtual_keycode);
                        self.common.keyboard_state_mut()
                            .pressed_virtual_keycodes.insert_hm_item(virtual_keycode);
                        let r = self.process_window_events(0);
                        events_result = events_result.max(r);
                        if !matches!(r, azul_core::events::ProcessEventResult::DoNothing) {
                            events_need_redraw = true;
                        }
                    }
                    HeadlessEvent::KeyUp { virtual_keycode } => {
                        self.snapshot_window_state_baseline("headless.run.key_up");
                        self.common.keyboard_state_mut().current_virtual_keycode =
                            azul_core::window::OptionVirtualKeyCode::None;
                        self.common.keyboard_state_mut()
                            .pressed_virtual_keycodes.remove_hm_item(&virtual_keycode);
                        let r = self.process_window_events(0);
                        events_result = events_result.max(r);
                        if !matches!(r, azul_core::events::ProcessEventResult::DoNothing) {
                            events_need_redraw = true;
                        }
                    }
                    HeadlessEvent::TextInput { text } => {
                        // Drive the SAME canonical text pipeline the debug
                        // server and platform IME paths use: record the input
                        // against the focused/editable node, dispatch the
                        // synthetic Input events, apply the changeset. This
                        // arm used to be an empty stub, which silently
                        // swallowed injected text (and made
                        // `synthesize_character_input` a no-op end to end).
                        self.snapshot_window_state_baseline("headless.run.text_input");
                        let r = self.apply_user_change(
                            &azul_layout::callbacks::CallbackChange::CreateTextInput {
                                text: text.clone().into(),
                            },
                        );
                        events_result = events_result.max(r);
                        if !matches!(r, azul_core::events::ProcessEventResult::DoNothing) {
                            events_need_redraw = true;
                        }
                    }
                    HeadlessEvent::Resize { width, height } => {
                        self.snapshot_window_state_baseline("headless.run.resize");
                        self.common
                            .update_window_state(event::WindowStateSource::Os, |ws| {
                                ws.size.dimensions.width = width;
                                ws.size.dimensions.height = height;
                            });
                        // Tag the upcoming regenerate_layout with the REAL
                        // reason, same as `simulate_resize()` — the two
                        // headless resize entry points used to disagree
                        // (this one left the implicit RefreshDom), so the
                        // user's LayoutCallback saw a phantom non-resize
                        // relayout depending on which API drove the resize.
                        self.common
                            .request_regeneration(azul_core::callbacks::RelayoutReason::Resize);
                        // Same shape as the ten sibling arms: run the pass so
                        // the size diff dispatches `WindowResize` — the one
                        // backend CI runs used to be the one backend that
                        // never fired it (the F4 class), and the un-passed
                        // delta tripped the AZ_VALIDATE assertion at the next
                        // `process_timers_and_threads()`.
                        let r = self.process_window_events(0);
                        events_result = events_result.max(r);
                        if !matches!(r, azul_core::events::ProcessEventResult::DoNothing) {
                            events_need_redraw = true;
                        }
                        events_need_redraw = true;
                    }
                    HeadlessEvent::Scroll { delta_x, delta_y } => {
                        // Drive the SAME physics-timer scroll path the desktop
                        // backends use: record_scroll_from_hit_test queues the
                        // delta against the scroll node under the pointer and
                        // the SCROLL_MOMENTUM_TIMER applies it over time.
                        // delta_x/delta_y are RAW input deltas, same as a platform
                        // wheel/axis event — the direction sign (natural-scroll
                        // flag) is applied centrally in ScrollManager, not here. A
                        // prior MouseMove must have left the hover hit-test over a
                        // scrollable node — otherwise this is a no-op (just like
                        // wheeling over a non-scrollable area on the desktop).
                        let queue = if let Some(lw) = self.common.layout_window.as_mut() {
                            let now = azul_core::task::Instant::from(std::time::Instant::now());
                            match lw.scroll_manager.record_scroll_from_hit_test(
                                delta_x,
                                delta_y,
                                azul_layout::managers::scroll_state::ScrollInputSource::WheelDiscrete,
                                // e2e harness scrolls must stay deterministic
                                // (velocity model, no wall-clock glide).
                                azul_layout::managers::scroll_state::ScrollInputDevice::TestDriver,
                                &lw.hover_manager,
                                &azul_layout::managers::hover::InputPointId::Mouse,
                                now,
                            ) {
                                Some((_, _, true)) => Some(lw.scroll_manager.get_input_queue()),
                                _ => None,
                            }
                        } else {
                            None
                        };

                        // Start the momentum timer only on the first pending
                        // input (subsequent deltas are picked up by the running
                        // timer via the shared ScrollInputQueue).
                        if let Some(queue) = queue {
                            let physics_state =
                                azul_layout::scroll_timer::ScrollPhysicsState::new(
                                    queue,
                                    self.common.system_style.scroll_physics.clone(),
                                );
                            let interval_ms =
                                self.common.system_style.scroll_physics.timer_interval_ms;
                            let timer = azul_layout::timer::Timer::create(
                                azul_core::refany::RefAny::new(physics_state),
                                azul_layout::scroll_timer::scroll_physics_timer_callback
                                    as azul_layout::timer::TimerCallbackType,
                                azul_layout::callbacks::ExternalSystemCallbacks::rust_internal()
                                    .get_system_time_fn,
                            )
                            .with_interval(azul_core::task::Duration::System(
                                azul_core::task::SystemTimeDiff::from_millis(interval_ms as u64),
                            ));
                            self.start_timer(
                                azul_core::task::SCROLL_MOMENTUM_TIMER_ID.id,
                                timer,
                            );
                        }
                    }
                }
            }
            // MWA-C-virtual_view: drain queued VirtualView re-invocations
            // FIRST so their queue-time reasons (EdgeScrolled/DomRecreated)
            // reach the user callback — headless previously relied solely on
            // the full regenerate below, which resets invocation flags and
            // re-invokes everything as InitialRender (queue never drained,
            // reasons untestable in E2E).
            // One drain for every backend (re-invoke in place + CPU hit-tester
            // rebuild). A non-empty queue owes a frame even if a view declined
            // to rebuild, as before.
            let had_virtual_view_updates = self
                .common
                .layout_window
                .as_ref()
                .is_some_and(|lw| !lw.pending_virtual_view_updates.is_empty());
            self.common.drain_virtual_view_updates();
            if had_virtual_view_updates {
                events_need_redraw = true;
            }

            if events_need_redraw {
                self.service_frame(events_result);
            }

            // ── Phase 1b: Apply queued accessibility actions ─────
            // The same slot `run.rs` gives the four desktop backends: actions
            // arrive off-loop (there, from an accesskit bus; here, from
            // `inject_accessibility_action`) and are drained by the frame pump
            // after input and before timers. Without this call the queue would
            // fill and nothing would ever read it — which is exactly the state
            // headless a11y was in.
            #[cfg(feature = "a11y")]
            self.process_accessibility_actions();

            // ── Phase 2: Tick timers and threads ─────────────────
            // Use the shared PlatformWindow trait method to invoke
            // expired timer callbacks and poll background threads.
            let needs_redraw = self.process_timers_and_threads();

            // In the CPU-only path there is no GPU compositor that can
            // handle scroll-offset-only or repaint-only updates.  Every
            // visual change (including scroll) requires a full display
            // list rebuild, so we re-render on any redraw signal — but
            // the relayout-only request decides WHICH pass runs: an in-place DOM
            // mutation (debug-server DOM ops, restyle, runtime text edit) must
            // re-run layout on the EXISTING StyledDom. Sending it through the
            // full `regenerate_layout()` is not a slower way to get the same
            // answer: that path bails out on `is_layout_equivalent(old, new)`,
            // which after an in-place mutation compares the DOM with itself,
            // reports "unchanged", and skips layout — leaving the old shaped
            // text and geometry on screen forever.
            if needs_redraw {
                // process_timers_and_threads already routed the tier: it
                // raised the regeneration request only for real RefreshDom
                // returns and relayout-only for in-place mutations. Passing
                // ShouldReRenderCurrentWindow here just says "a frame is
                // owed"; service_frame consumes the flags to pick the pass.
                self.service_frame(
                    azul_core::events::ProcessEventResult::ShouldReRenderCurrentWindow,
                );
            }

            // ── Phase 2b: Honour `flags.close_requested` ─────────
            // `CallbackChange::CloseWindow` — the cross-platform "quit" API a
            // callback or timer uses — does not close anything itself: it sets
            // `flags.close_requested` and relies on the shell's loop to consume
            // it. Every desktop backend does (the Linux run loop's
            // `close_requested() → close()` check, Windows' WM_PAINT/WndProc
            // checks, macOS's sync_window_state) — headless did NOT, so an app
            // whose exit path is `window.close()` from a callback kept its loop
            // alive forever: the flag was set, `DoNothing` came back, and
            // `while self.is_open()` never terminated. With an active timer the
            // loop even kept polling at 60 Hz, which is exactly the
            // "self-test never exits after the last window closes" hang.
            // Checked here — after events (Phase 1), a11y actions (Phase 1b)
            // and timers/threads (Phase 2), the three places a callback can
            // run — so a close requested anywhere this iteration exits before
            // the condvar wait instead of after a wake that may never come.
            if self.common.current_window_state().flags.close_requested {
                log_info!(
                    LogCategory::EventLoop,
                    "[Headless] close_requested by callback — closing window"
                );
                self.close();
            }

            // ── Phase 3: Spawn sub-HeadlessWindows for pending creates ─
            while let Some(pending_create) = self.pending_window_creates.pop() {
                log_debug!(
                    LogCategory::Window,
                    "[Headless] Spawning sub-HeadlessWindow (type: {:?})",
                    pending_create.window_state.flags.window_type
                );
                match HeadlessWindow::new(
                    pending_create,
                    self.common.app_data.clone(),
                    self.common.undo_manager.clone(),
                    self.config.clone(),
                    self.icon_provider.clone(),
                    self.common.fc_cache.clone(),
                    self.font_registry.clone(),
                ) {
                    Ok(child) => children.push(child),
                    Err(e) => {
                        log_error!(
                            LogCategory::Window,
                            "[Headless] Failed to create sub-HeadlessWindow: {:?}",
                            e
                        );
                    }
                }
            }

            // ── Phase 4: Pump child windows ──────────────────────
            children.retain_mut(|child| {
                while let Some(ev) = child.poll_event() {
                    if let HeadlessEvent::Close = ev { child.close(); }
                }
                // Same close_requested contract as the parent window above: a
                // callback that closes a child popup/dialog sets the flag and
                // the loop must consume it.
                if child.common.current_window_state().flags.close_requested {
                    child.close();
                }
                child.pending_window_creates.clear();
                child.is_open()
            });

            // ── Phase 5: Condvar-based wait ──────────────────────
            let has_timers = self.common.layout_window.as_ref()
                .map_or(false, |lw| !lw.timers.is_empty());
            let has_wake_sources = has_timers
                || self.thread_poll_timer_running
                || debug_enabled
                || !children.is_empty();

            if !has_wake_sources && !warned_no_wake_sources {
                warned_no_wake_sources = true;
                eprintln!(
                    "[azul] HeadlessWindow: no timers, threads, or debug server active. \
                     The event loop will block indefinitely on a condvar \
                     (same as a desktop window nobody interacts with). \
                     Set AZ_DEBUG=1 to enable the debug server, or \
                     inject events via inject_event()."
                );
            }

            // Lock, then wait — but only if no wake is already pending.
            let mut guard = self.wake_mutex.lock().unwrap();

            // Threads count as a wake source for the TIMED wait, not just for
            // the no-wake-sources warning above: a background `Thread`'s
            // completion is only ever OBSERVED by polling (`run_all_threads`
            // inside `process_timers_and_threads`) — the worker has no handle
            // to this condvar, so it cannot signal it. With threads in flight
            // and no timers armed, the old `has_timers`-only split blocked
            // indefinitely: the fetch worker finished, its writeback never
            // ran, and the window froze — the headless twin of the X11/Wayland
            // "threads have no fd in the poll set" 16 ms tick, which is
            // exactly how those backends solve the same problem.
            if guard.woken {
                // A wake arrived DURING this iteration's processing — e.g. a
                // timer callback injected an event after Phase 1 had already
                // drained the queue, or requested a redraw after Phase 2. The
                // old sequence cleared `woken` unconditionally and then
                // waited, erasing that wake: the queued work sat there until
                // the next unrelated wake (or forever, with no timer armed).
                // Consume the flag and loop again WITHOUT waiting, so the
                // work the wake announced is serviced now.
                guard.woken = false;
            } else if has_timers || self.thread_poll_timer_running {
                // Timers or threads active → poll at 60 Hz
                let _r = self.wake_condvar.wait_timeout_while(
                    guard,
                    Duration::from_millis(TIMER_POLL_MS),
                    |ws| !ws.woken,
                );
            } else {
                // No timers → block indefinitely until woken
                let _r = self.wake_condvar.wait_while(
                    guard,
                    |ws| !ws.woken,
                );
            }
        }

        log_info!(
            LogCategory::EventLoop,
            "[Headless] Event loop finished (elapsed: {:.1}s)",
            start.elapsed().as_secs_f64()
        );

        // Handle termination behaviour (same as every platform run())
        match self.config.termination_behavior {
            AppTerminationBehavior::EndProcess => {
                // `process::exit` does NOT run destructors, so every live
                // `Thread` would skip its `Drop` — the one that sends
                // TerminateThread, waits out the grace period and JOINS the
                // worker. The threads simply vanish with the process, and
                // ThreadSanitizer reports exactly that:
                //
                //   SUMMARY: ThreadSanitizer: thread leak in pthread_create
                //
                // The async example makes it visible because it spawns one
                // worker per visible map tile and the harness exits after a
                // single frame, with all of them still in flight. Dropping the
                // registry here runs those destructors while the process is
                // still alive.
                self.shutdown_threads();
                std::process::exit(0);
            }
            AppTerminationBehavior::ReturnToMain => { /* return normally */ }
            AppTerminationBehavior::RunForever => { /* all windows closed */ }
        }

        Ok(())
    }
}

// === PlatformWindow Trait Implementation ===

impl PlatformWindow for HeadlessWindow {
    fn regenerate_layout_once(
        &mut self,
    ) -> Result<crate::desktop::shell2::common::layout::LayoutRegenerateResult, String> {
        // The single pass. The bounded lifecycle loop lives in the trait
        // default `regenerate_layout`, which is what frame paths call.
        self.regenerate_layout_inner()
    }

    // 28 getter/setter methods generated by macro — identical to all other platforms
    impl_platform_window_getters!(common);

    fn get_raw_window_handle(&self) -> RawWindowHandle {
        RawWindowHandle::Unsupported
    }

    fn prepare_callback_invocation(&mut self) -> event::InvokeSingleCallbackBorrows<'_> {
        let borrows = self.common.layout_borrows();

        event::InvokeSingleCallbackBorrows {
            layout_window: borrows
                .layout_window
                .expect("Layout window must exist for callback invocation"),
            window_handle: RawWindowHandle::Unsupported,
            gl_context_ptr: borrows.gl_context_ptr,
            fc_cache_clone: (**borrows.fc_cache).clone(),
            system_style: borrows.system_style.clone(),
            previous_window_state: borrows.previous_window_state,
            current_window_state: borrows.current_window_state,
            renderer_resources: borrows.renderer_resources,
        }
    }

    // Timer Management — condvar wakes the loop when timers change

    fn start_timer(&mut self, timer_id: usize, timer: azul_layout::timer::Timer) {
        if let Some(layout_window) = self.common.layout_window.as_mut() {
            layout_window
                .timers
                .insert(azul_core::task::TimerId { id: timer_id }, timer);
        }
        self.wake(); // transition condvar from indefinite to timed wait
    }

    fn stop_timer(&mut self, timer_id: usize) {
        if let Some(layout_window) = self.common.layout_window.as_mut() {
            layout_window
                .timers
                .remove(&azul_core::task::TimerId { id: timer_id });
        }
    }

    fn start_thread_poll_timer(&mut self) {
        self.thread_poll_timer_running = true;
    }

    fn stop_thread_poll_timer(&mut self) {
        self.thread_poll_timer_running = false;
    }

    fn add_threads(
        &mut self,
        threads: BTreeMap<azul_core::task::ThreadId, azul_layout::thread::Thread>,
    ) {
        if let Some(lw) = self.common.layout_window.as_mut() {
            for (id, thread) in threads {
                lw.threads.insert(id, thread);
            }
        }
        if !self.thread_poll_timer_running {
            self.start_thread_poll_timer();
        }
    }

    fn remove_threads(
        &mut self,
        thread_ids: &BTreeSet<azul_core::task::ThreadId>,
    ) {
        if let Some(lw) = self.common.layout_window.as_mut() {
            for id in thread_ids {
                lw.threads.remove(id);
            }
            if lw.threads.is_empty() {
                self.stop_thread_poll_timer();
            }
        }
    }

    fn queue_window_create(&mut self, options: WindowCreateOptions) {
        self.pending_window_creates.push(options);
    }

    fn show_menu_from_callback(
        &mut self,
        _menu: &azul_core::menu::Menu,
        _position: LogicalPosition,
    ) {
        // TODO: could create a sub-HeadlessWindow with the menu content
    }

    fn show_tooltip_from_callback(
        &mut self,
        _text: &str,
        _position: LogicalPosition,
    ) {
        // No-op — no visual surface to show a tooltip on
    }

    fn hide_tooltip_from_callback(&mut self) {
        // No-op
    }

    fn sync_window_state(&mut self) {
        // No native window to synchronise, so there is no OS-sync baseline to
        // diff either (`os_synced_state` stays `None`).
    }
}

// =========================================================================
// Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_stub() -> HeadlessWindow {
        use azul_core::icon::{IconProviderHandle, SharedIconProvider};
        let fc_cache = Arc::new(FcFontCache::default());
        let app_data = Arc::new(RefCell::new(RefAny::new(())));
        let icon_provider = SharedIconProvider::from_handle(IconProviderHandle::default());
        HeadlessWindow::new(
            WindowCreateOptions::default(),
            app_data,
            event::SharedUndoManager::new(),
            AppConfig::default(),
            icon_provider,
            fc_cache,
            None,
        ).unwrap()
    }

    #[test]
    fn test_stub_window_creation() {
        let window = make_stub();
        assert!(window.is_open());
    }

    // =====================================================================
    // Damage harness — pure-Rust (no X11) simulation of the repaint path.
    //
    // Builds a HeadlessWindow with a controlled layout callback, drives state
    // changes, and captures the calculated FrameDamage + the rendered
    // display-list text. Uses println! to trace the architecture (run with
    // `cargo test -p azul-dll damage_ -- --nocapture`).
    // =====================================================================

    use azul_core::callbacks::{LayoutCallback, LayoutCallbackInfo};
    use azul_core::refany::OptionRefAny;
    use azul_core::dom::Dom;
    use azul_core::geom::LogicalSize;
    use azul_layout::solver3::display_list::DisplayListItem;

    /// Minimal app state the harness layout callback reads.
    #[derive(Debug, Clone)]
    struct UiState {
        label: String,
    }

    /// Layout callback: `<body><div>{label}</div></body>`. The text content is
    /// driven entirely by UiState, so a label change is a pure text-content
    /// change at a stable DOM position — the cross-window stale-text repro,
    /// headless.
    extern "C" fn harness_layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        let label = data
            .downcast_ref::<UiState>()
            .map(|s| s.label.clone())
            .unwrap_or_default();
        Dom::create_body()
            .with_child(Dom::create_div().with_child(Dom::create_text_do_not_use_without_block_level_wrapper(label.as_str())))
    }

    /// One embedded font for the whole harness. Using a bundled font instead of
    /// `FcFontCache::build()` makes glyph metrics DETERMINISTIC (tests never
    /// depend on which fonts the host has installed) and does ZERO disk access —
    /// the system-font scan was both a flakiness source and a contributor to the
    /// build-machine lockup (every test forking a full font enumeration).
    const HARNESS_FONT: &[u8] =
        include_bytes!("../../../../../assets/fonts/InstrumentSerif-Regular.ttf");

    /// Parse [`HARNESS_FONT`] and insert it straight into the window's
    /// `FontManager` so text shapes without any system-font scan.
    ///
    /// Why inject the parsed font rather than register it in the `FcFontCache`
    /// (the obvious approach)? Because a single in-memory font CANNOT serve text
    /// through an otherwise-empty fontconfig cache (the trail, #15):
    /// - generic families ("serif", azul's default) are EXPANDED to a hardcoded
    ///   OS list ("DejaVu Serif", …) and the generic is dropped, so a custom font
    ///   is never matched by the generic name (the `web/eventloop.rs` "serif
    ///   sans-serif monospace" trick silently does nothing);
    /// - the Unicode-fallback path skips every codepoint < U+0400 (it assumes the
    ///   CSS fallbacks' own glyphs cover Latin — i.e. that real system fonts
    ///   exist), so ASCII resolves no fallback in an empty cache.
    ///
    /// The shaper's last resort, however, is a direct glyph probe over the
    /// LOADED fonts (`split_text_by_font_coverage`'s `.or_else` →
    /// `font.has_glyph`). With an empty `FcFontCache` every char misses
    /// fontconfig and falls through to that probe — so a font present in the
    /// `FontManager` is used by real cmap coverage, no font-family needed on the
    /// DOM. We insert with interior mutability (`insert_font(&self, …)`), so this
    /// runs before the test's first `regenerate_layout`.
    ///
    /// (The underlying gap — a bundled in-memory font can't serve generic
    /// families / Latin via the cache — is a real rust-fontconfig footgun that
    /// also breaks the web/wasm fallback; flagged for an upstream fix.)
    fn inject_harness_font(window: &HeadlessWindow) {
        use azul_layout::text3::default::font_ref_from_bytes;
        let font_ref = match font_ref_from_bytes(HARNESS_FONT, 0, false) {
            Some(f) => f,
            None => return,
        };
        if let Some(lw) = window.common.layout_window.as_ref() {
            lw.font_manager
                .insert_font(rust_fontconfig::FontId::new(), font_ref);
        }
    }

    fn make_window_with(
        state: &Arc<RefCell<RefAny>>,
        cb: azul_core::callbacks::LayoutCallbackType,
    ) -> HeadlessWindow {
        make_window_sized(state, cb, 400.0, 300.0)
    }

    /// [`make_window_with`] at an explicit viewport.
    ///
    /// The 400x300 default is PHONE-sized: a widget with a responsive layout
    /// (the ribbon collapses its tab strip into one mobile tab button)
    /// renders its small-screen variant there, and a test aiming at desktop
    /// chrome finds nodes that were never laid out.
    fn make_window_sized(
        state: &Arc<RefCell<RefAny>>,
        cb: azul_core::callbacks::LayoutCallbackType,
        width: f32,
        height: f32,
    ) -> HeadlessWindow {
        use azul_core::icon::{IconProviderHandle, SharedIconProvider};
        // Empty cache → NO system-font scan / disk access. The deterministic
        // embedded font is injected into the FontManager below (see
        // `inject_harness_font` for why the cache route doesn't work).
        let fc_cache = Arc::new(FcFontCache::default());
        let icon_provider = SharedIconProvider::from_handle(IconProviderHandle::default());
        let mut opts = WindowCreateOptions::default();
        opts.window_state.layout_callback = LayoutCallback {
            cb,
            ctx: OptionRefAny::None,
        };
        opts.window_state.size.dimensions = LogicalSize::new(width, height);
        let window = HeadlessWindow::new(
            opts,
            state.clone(),
            event::SharedUndoManager::new(),
            AppConfig::default(),
            icon_provider,
            fc_cache,
            None,
        )
        .unwrap();
        inject_harness_font(&window);
        window
    }

    fn make_harness_window(state: &Arc<RefCell<RefAny>>) -> HeadlessWindow {
        make_window_with(state, harness_layout)
    }

    /// Total area of a FrameDamage (None for Full = unbounded, 0.0 for None).
    fn damage_area(d: &FrameDamage) -> Option<f32> {
        match d {
            FrameDamage::None => Some(0.0),
            FrameDamage::Full => None,
            FrameDamage::Rects(rs) => {
                Some(rs.iter().map(|r| r.size.width * r.size.height).sum())
            }
        }
    }

    /// State + layout for a non-text colored box (isolates the damage system
    /// from text-shaping generation bugs).
    #[derive(Debug, Clone)]
    struct BoxState {
        red: bool,
    }

    extern "C" fn harness_layout_box(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        use azul_css::props::layout::dimensions::{LayoutHeight, LayoutWidth};
        use azul_css::props::property::CssProperty;
        use azul_css::dynamic_selector::CssPropertyWithConditions;
        use azul_css::props::basic::color::ColorU;
        use azul_css::props::style::background::{StyleBackgroundContent, StyleBackgroundContentVec};

        let red = data.downcast_ref::<BoxState>().map(|s| s.red).unwrap_or(false);
        let color = if red {
            ColorU { r: 255, g: 0, b: 0, a: 255 }
        } else {
            ColorU { r: 0, g: 0, b: 255, a: 255 }
        };
        let bg: StyleBackgroundContentVec = vec![StyleBackgroundContent::Color(color)].into();
        Dom::create_body().with_child(
            Dom::create_div().with_css_props(
                vec![
                    CssPropertyWithConditions::simple(CssProperty::width(LayoutWidth::px(100.0))),
                    CssPropertyWithConditions::simple(CssProperty::height(LayoutHeight::px(50.0))),
                    CssPropertyWithConditions::simple(CssProperty::background_content(bg)),
                ]
                .into(),
            ),
        )
    }

    /// A ribbon-shaped row: LEFT-anchored group, a flex spacer, then two
    /// RIGHT-anchored groups (they move by the FULL width delta on resize),
    /// with a centered label underneath (moves by HALF the delta) — the
    /// mover-diversity of the live miniword ribbon, in miniature.
    extern "C" fn harness_layout_ribbon(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        use azul_css::dynamic_selector::CssPropertyWithConditions as C;
        use azul_css::props::basic::color::ColorU;
        use azul_css::props::layout::dimensions::{LayoutHeight, LayoutWidth};
        use azul_css::props::layout::flex::{LayoutFlexDirection, LayoutFlexGrow};
        use azul_css::props::property::CssProperty;
        use azul_css::props::style::background::{
            StyleBackgroundContent, StyleBackgroundContentVec,
        };
        use azul_css::props::style::text::StyleTextAlign;

        let label = data
            .downcast_ref::<UiState>()
            .map(|s| s.label.clone())
            .unwrap_or_default();
        let bg = |r: u8, g: u8, b: u8| -> StyleBackgroundContentVec {
            vec![StyleBackgroundContent::Color(ColorU { r, g, b, a: 255 })].into()
        };
        let group = |color: StyleBackgroundContentVec, text: &str| {
            Dom::create_div()
                .with_css_props(
                    vec![
                        C::simple(CssProperty::width(LayoutWidth::px(90.0))),
                        C::simple(CssProperty::height(LayoutHeight::px(40.0))),
                        C::simple(CssProperty::background_content(color)),
                    ]
                    .into(),
                )
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(text))
        };
        let spacer = Dom::create_div().with_css_props(
            vec![C::simple(CssProperty::flex_grow(LayoutFlexGrow::const_new(1)))].into(),
        );
        let row = Dom::create_div()
            .with_css_props(
                vec![
                    C::simple(CssProperty::flex_direction(LayoutFlexDirection::Row)),
                    C::simple(CssProperty::height(LayoutHeight::px(44.0))),
                ]
                .into(),
            )
            .with_child(group(bg(200, 60, 60), "Clip"))
            .with_child(spacer)
            .with_child(group(bg(60, 160, 60), "Font"))
            .with_child(group(bg(60, 60, 200), "Layout"));
        let centered = Dom::create_div()
            .with_css_props(
                vec![C::simple(CssProperty::text_align(StyleTextAlign::Center))].into(),
            )
            .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(label.as_str()));
        Dom::create_body().with_child(row).with_child(centered)
    }

    /// TASK #19 REPRO ATTEMPT: incremental resizes of a multi-mover row must
    /// stay pixel-identical to a fresh render at every step. The live
    /// symptom ("LAYOUT tab flapping, groups merging") is stale bands after
    /// drag steps; if the defect is render-side (damage/blit), this goes red
    /// with first-diff coordinates. If this stays green, the defect is in
    /// the wayland PRESENT path (slots/timing), not the renderer.
    #[test]
    fn ribbon_row_stays_pixel_true_across_incremental_resizes() {
        use azul_core::geom::LogicalSize;
        let state = Arc::new(RefCell::new(RefAny::new(UiState {
            label: "centered caption".to_string(),
        })));
        let mut window = make_window_sized(&state, harness_layout_ribbon, 800.0, 240.0);
        window.regenerate_layout().expect("initial layout");
        window.regenerate_layout().expect("settle");

        let mut prev_w = 800.0f32;
        // Odd step: fractional centered deltas + non-integral mover deltas.
        for w in [793.0f32, 786.0, 779.0, 772.0, 765.0] {
            let full = window.common.request_regeneration_for_resize(
                LogicalSize::new(prev_w, 240.0),
                LogicalSize::new(w, 240.0),
            );
            window.common.update_window_state(event::WindowStateSource::Os, |ws| {
                ws.size.dimensions = LogicalSize::new(w, 240.0);
            });
            if full {
                window.regenerate_layout().expect("regen");
            } else {
                let _ = window.common.take_relayout_only();
                window.relayout_only().expect("relayout");
            }
            let incr = window
                .cpu_backend
                .last_frame
                .as_ref()
                .expect("incremental frame")
                .clone_pixmap();

            // Fresh window at the same size = ground truth.
            let fresh_state = Arc::new(RefCell::new(RefAny::new(UiState {
                label: "centered caption".to_string(),
            })));
            let mut fresh = make_window_sized(&fresh_state, harness_layout_ribbon, w, 240.0);
            fresh.regenerate_layout().expect("fresh layout");
            let full_frame = fresh
                .cpu_backend
                .last_frame
                .as_ref()
                .expect("fresh frame")
                .clone_pixmap();

            assert_eq!(incr.width(), full_frame.width(), "width at {w}");
            let (a, b) = (incr.data(), full_frame.data());
            let mut diffs = 0usize;
            let mut first: Option<(u32, u32)> = None;
            for i in (0..a.len().min(b.len())).step_by(4) {
                if a[i] != b[i] || a[i + 1] != b[i + 1] || a[i + 2] != b[i + 2] {
                    diffs += 1;
                    if first.is_none() {
                        let px = (i / 4) as u32;
                        first = Some((px % incr.width(), px / incr.width()));
                    }
                }
            }
            assert_eq!(
                diffs, 0,
                "step to {w}: incremental frame differs from fresh in {diffs} px,                  first at {first:?} — stale band (the live flapping/merge symptom)"
            );
            prev_w = w;
        }
    }

    /// #19 with the REAL ribbon widget (USER ask: "test the ribbon again
    /// with the new damage-rect op" — this is the headless equivalent of
    /// `assert_damage_sound`'s covers-changes law, applied at every drag
    /// step): sweep the width in small odd steps across the group-tiling
    /// thresholds; the incrementally presented frame must be bit-identical
    /// to a fresh full render at every width. The earlier law used a
    /// synthetic row (the "fixture can't express it" gap) — this one builds
    /// `azul_layout::widgets::ribbon::Ribbon` itself: tabs + labeled groups
    /// + the small-button columns whose group labels visually "merged" live.
    #[test]
    fn real_ribbon_resize_sweep_matches_fresh_at_every_step() {
        use azul_core::geom::LogicalSize;
        use azul_layout::widgets::ribbon::{
            Ribbon, RibbonButton, RibbonColumn, RibbonGroup, RibbonItem, RibbonTab,
            RibbonTabVec,
        };

        fn tabs() -> RibbonTabVec {
            let col = |a: (&str, &str), b: (&str, &str)| {
                RibbonItem::Column(
                    RibbonColumn::new()
                        .with_item(RibbonItem::SmallButton(RibbonButton::new(a.0.into(), a.1.into())))
                        .with_item(RibbonItem::SmallButton(RibbonButton::new(b.0.into(), b.1.into()))),
                )
            };
            let home = RibbonTab::new("HOME".into())
                .with_group(
                    RibbonGroup::new("Clipboard".into())
                        .with_item(RibbonItem::LargeButton(RibbonButton::new("content_paste".into(), "Paste".into())))
                        .with_item(col(("content_cut", "Cut"), ("content_copy", "Copy"))),
                )
                .with_group(
                    RibbonGroup::new("Font".into())
                        .with_item(col(("format_bold", "Bold"), ("format_italic", "Italic")))
                        .with_item(col(
                            ("format_underlined", "Underline"),
                            ("format_color_text", "Color"),
                        )),
                )
                .with_group(RibbonGroup::new("Styles".into()).with_item(RibbonItem::LargeButton(
                    RibbonButton::new("style".into(), "Styles".into()),
                )));
            RibbonTabVec::from_vec(vec![
                home,
                RibbonTab::new("INSERT".into()).with_group(
                    RibbonGroup::new("Preview".into()).with_item(RibbonItem::LargeButton(
                        RibbonButton::new("layers".into(), "Insert".into()),
                    )),
                ),
                RibbonTab::new("LAYOUT".into()).with_group(
                    RibbonGroup::new("Preview".into()).with_item(RibbonItem::LargeButton(
                        RibbonButton::new("layers".into(), "Layout".into()),
                    )),
                ),
            ])
        }
        extern "C" fn layout_real_ribbon(_data: RefAny, _info: LayoutCallbackInfo) -> Dom {
            Dom::create_body().with_child(tabs_dom())
        }
        fn tabs_dom() -> Dom {
            Ribbon::new(tabs()).with_active_tab(0).dom()
        }

        let state = Arc::new(RefCell::new(RefAny::new(0usize)));
        let mut window = make_window_sized(&state, layout_real_ribbon, 900.0, 260.0);
        window.regenerate_layout().expect("initial layout");
        window.regenerate_layout().expect("settle");

        let mut prev_w = 900.0f32;
        // Odd 13px steps: fractional group re-tiling + label re-centering at
        // every step, crossing the widths where groups compress.
        let mut w = 887.0f32;
        while w >= 614.0 {
            let full = window.common.request_regeneration_for_resize(
                LogicalSize::new(prev_w, 260.0),
                LogicalSize::new(w, 260.0),
            );
            window.common.update_window_state(event::WindowStateSource::Os, |ws| {
                ws.size.dimensions = LogicalSize::new(w, 260.0);
            });
            if full {
                window.regenerate_layout().expect("regen");
            } else {
                let _ = window.common.take_relayout_only();
                window.relayout_only().expect("relayout");
            }
            let incr = window
                .cpu_backend
                .last_frame
                .as_ref()
                .expect("incremental frame")
                .clone_pixmap();

            let fresh_state = Arc::new(RefCell::new(RefAny::new(0usize)));
            let mut fresh = make_window_sized(&fresh_state, layout_real_ribbon, w, 260.0);
            fresh.regenerate_layout().expect("fresh layout");
            let full_frame = fresh
                .cpu_backend
                .last_frame
                .as_ref()
                .expect("fresh frame")
                .clone_pixmap();

            assert_eq!(incr.width(), full_frame.width(), "width at {w}");
            let (a, b) = (incr.data(), full_frame.data());
            let mut diffs = 0usize;
            let mut first: Option<(u32, u32)> = None;
            for i in (0..a.len().min(b.len())).step_by(4) {
                if a[i] != b[i] || a[i + 1] != b[i + 1] || a[i + 2] != b[i + 2] {
                    diffs += 1;
                    if first.is_none() {
                        let px = (i / 4) as u32;
                        first = Some((px % incr.width(), px / incr.width()));
                    }
                }
            }
            assert_eq!(
                diffs, 0,
                "REAL ribbon diverges from fresh render at width {w}: {diffs} px, first {first:?} \
                 — the live 'merge' class (patched-DL under-damage)"
            );
            prev_w = w;
            w -= 13.0;
        }
    }

    /// `make_window_sized` with the REAL system font cache instead of the
    /// injected deterministic harness font — for laws that must reproduce
    /// live glyph metrics ("Liberation Sans" fractional advances; the
    /// fit-test epsilon is metric-dependent).
    fn make_window_sized_real_fonts(
        state: &Arc<RefCell<RefAny>>,
        cb: azul_core::callbacks::LayoutCallbackType,
        width: f32,
        height: f32,
    ) -> HeadlessWindow {
        use azul_core::icon::{IconProviderHandle, SharedIconProvider};
        let fc_cache = Arc::new(FcFontCache::build());
        let icon_provider = SharedIconProvider::from_handle(IconProviderHandle::default());
        let mut opts = WindowCreateOptions::default();
        opts.window_state.layout_callback = LayoutCallback {
            cb,
            ctx: OptionRefAny::None,
        };
        opts.window_state.size.dimensions = LogicalSize::new(width, height);
        HeadlessWindow::new(
            opts,
            state.clone(),
            event::SharedUndoManager::new(),
            AppConfig::default(),
            icon_provider,
            fc_cache,
            None,
        )
        .unwrap()
    }

    /// USER 2026-08-12 live report: resizing the azwriter window narrower
    /// made "PAGE LAYOUT" — the only tab label with internal whitespace —
    /// suddenly break onto a second line, and the group captions below
    /// visibly de-centered. Fresh layout at every integer width 600..=1400
    /// is clean (layout/tests/ribbon_tab_whitespace.rs), so this law walks
    /// the INCREMENTAL resize path with azwriter parity (nine tabs,
    /// desktop-only DOM, pinned "Liberation Sans"): 1px steps down and back
    /// up, on the integer AND the half-pixel lattice. At every step, on the
    /// live window's own layout: every tab label keeps one-line height and
    /// every group caption stays centered on its footer.
    #[test]
    fn azwriter_ribbon_resize_sweep_keeps_tabs_one_line_and_captions_centered() {
        use azul_core::dom::{DomId, DomNodeId, NodeId, NodeType};
        use azul_core::geom::LogicalSize;
        use azul_layout::widgets::ribbon::{
            Ribbon, RibbonAppButton, RibbonButton, RibbonColumn, RibbonGroup, RibbonItem,
            RibbonTab, RibbonTabVec,
        };

        const TAB_LABELS: &[&str] = &[
            "HOME", "INSERT", "DESIGN", "PAGE LAYOUT", "REFERENCES", "MAILINGS", "REVIEW", "VIEW",
        ];
        const CAPTIONS: &[&str] = &["Clipboard", "Font", "Paragraph", "Styles"];

        extern "C" fn layout_azwriter_ribbon(_data: RefAny, _info: LayoutCallbackInfo) -> Dom {
            use azul_css::{
                dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
                props::{
                    basic::font::{StyleFontFamily, StyleFontFamilyVec},
                    property::CssProperty,
                },
            };
            let col = |labels: [&str; 3]| {
                RibbonItem::Column(labels.into_iter().fold(RibbonColumn::new(), |c, l| {
                    c.with_item(RibbonItem::SmallButton(RibbonButton::new(
                        "content_cut".into(),
                        l.into(),
                    )))
                }))
            };
            let home = RibbonTab::new("HOME".into())
                .with_group(
                    RibbonGroup::new("Clipboard".into())
                        .with_item(col(["Cut", "Copy", "Format Painter"])),
                )
                .with_group(
                    RibbonGroup::new("Font".into()).with_item(col(["Grow", "Shrink", "Clear"])),
                )
                .with_group(
                    RibbonGroup::new("Paragraph".into())
                        .with_item(col(["Bullets", "Numbering", "Sort"])),
                )
                .with_group(RibbonGroup::new("Styles".into()).with_item(
                    RibbonItem::LargeButton(RibbonButton::new("style".into(), "Styles".into())),
                ));
            let mut tabs = vec![home];
            for label in
                ["INSERT", "DESIGN", "PAGE LAYOUT", "REFERENCES", "MAILINGS", "REVIEW", "VIEW"]
            {
                tabs.push(RibbonTab::new(label.into()).with_group(
                    RibbonGroup::new("Preview".into()).with_item(RibbonItem::LargeButton(
                        RibbonButton::new("layers".into(), label.into()),
                    )),
                ));
            }
            let mut ribbon = Ribbon::new(RibbonTabVec::from_vec(tabs))
                .with_app_button(RibbonAppButton::new("FILE".into()));
            let mut v = ribbon.style.container_style.as_ref().to_vec();
            v.push(CssPropertyWithConditions::simple(CssProperty::const_font_family(
                StyleFontFamilyVec::from_vec(vec![StyleFontFamily::System(
                    "Liberation Sans".into(),
                )]),
            )));
            ribbon.style.container_style = CssPropertyWithConditionsVec::from_vec(v);
            Dom::create_body().with_child(ribbon.dom_desktop())
        }

        fn lw_of(window: &HeadlessWindow) -> &azul_layout::window::LayoutWindow {
            window.common.layout_window.as_ref().expect("layout window")
        }
        // Labels are `<p>`-wrapped text: the text node has no rect of its
        // own; wrap + centering are read off the text3 UnifiedLayout of the
        // wrapping `<p>` and its box rect.
        fn find_text_node(window: &HeadlessWindow, needle: &str) -> Option<usize> {
            let lw = lw_of(window);
            let result = lw.layout_results.get(&DomId::ROOT_ID)?;
            let node_data = result.styled_dom.node_data.as_container();
            (0..node_data.len()).find(|i| {
                matches!(
                    node_data[NodeId::new(*i)].get_node_type(),
                    NodeType::Text(s) if s.as_ref().as_str() == needle
                )
            })
        }
        fn parent_of(window: &HeadlessWindow, child: usize) -> Option<NodeId> {
            let lw = lw_of(window);
            let result = lw.layout_results.get(&DomId::ROOT_ID)?;
            let hier = result.styled_dom.node_hierarchy.as_container();
            hier[NodeId::new(child)].parent_id()
        }
        fn rect_of(window: &HeadlessWindow, i: NodeId) -> Option<azul_core::geom::LogicalRect> {
            let id = DomNodeId {
                dom: DomId::ROOT_ID,
                node: azul_core::styled_dom::NodeHierarchyItemId::from_crate_internal(Some(i)),
            };
            lw_of(window).get_node_layout_rect(id)
        }


        let state = Arc::new(RefCell::new(RefAny::new(0usize)));
        let mut window = make_window_sized_real_fonts(&state, layout_azwriter_ribbon, 1000.0, 300.0);
        window.regenerate_layout().expect("initial layout");
        window.regenerate_layout().expect("settle");

        // Box-level detectors (static screen text retains no UnifiedLayout —
        // §3.2 drops it): a wrapped label doubles its `<p>` height; a
        // de-centered caption is a `<p>` that failed to grow onto its footer.
        // Both detectors are NC-proven in layout/tests/ribbon_tab_whitespace.rs.
        let base_h: Vec<f32> = TAB_LABELS
            .iter()
            .map(|l| {
                let i = find_text_node(&window, l).unwrap_or_else(|| panic!("tab {l} @1000"));
                let p = parent_of(&window, i).expect("tab <p>");
                rect_of(&window, p).expect("tab <p> rect").size.height
            })
            .collect();

        let check = |window: &HeadlessWindow, w: f32| {
            for (k, l) in TAB_LABELS.iter().enumerate() {
                let i = find_text_node(window, l)
                    .unwrap_or_else(|| panic!("tab '{l}' vanished at w={w}"));
                let p = parent_of(window, i).expect("tab <p>");
                let r = rect_of(window, p)
                    .unwrap_or_else(|| panic!("tab '{l}' has no box at w={w}"));
                assert!(
                    r.size.height <= base_h[k] * 1.5,
                    "tab '{l}' WRAPPED at w={w}: <p> height {:.2} vs one-line {:.2} — the live \
                     PAGE-LAYOUT symptom (resize-path line break diverges from fresh layout)",
                    r.size.height,
                    base_h[k]
                );
            }
            for l in CAPTIONS {
                let i = find_text_node(window, l)
                    .unwrap_or_else(|| panic!("caption '{l}' vanished at w={w}"));
                let p = parent_of(window, i).expect("caption <p>");
                let pr = rect_of(window, p).expect("caption <p> rect");
                let f = parent_of(window, p.index()).expect("caption footer");
                let fr = rect_of(window, f).expect("footer rect");
                let err = (pr.origin.x + pr.size.width / 2.0)
                    - (fr.origin.x + fr.size.width / 2.0);
                assert!(
                    err.abs() <= 1.0,
                    "caption '{l}' OFF-CENTER by {err:.2}px at w={w} — the live de-centering \
                     symptom"
                );
            }
        };
        check(&window, 1000.0);

        // Four lattices: integer down/up, half-pixel down/up. ~1600 resize
        // steps total, every one through the production resize decision.
        let phases: [(f32, f32, f32); 4] = [
            (999.0, 600.0, -1.0),
            (601.0, 1000.0, 1.0),
            (999.5, 600.5, -1.0),
            (601.5, 999.5, 1.0),
        ];
        let mut prev_w = 1000.0f32;
        for (start, end, step) in phases {
            let mut w = start;
            loop {
                if (step < 0.0 && w < end) || (step > 0.0 && w > end) {
                    break;
                }
                let full = window.common.request_regeneration_for_resize(
                    LogicalSize::new(prev_w, 300.0),
                    LogicalSize::new(w, 300.0),
                );
                window.common.update_window_state(event::WindowStateSource::Os, |ws| {
                    ws.size.dimensions = LogicalSize::new(w, 300.0);
                });
                if full {
                    window.regenerate_layout().expect("regen");
                } else {
                    let _ = window.common.take_relayout_only();
                    window.relayout_only().expect("relayout");
                }
                check(&window, w);
                prev_w = w;
                w += step;
            }
        }
    }

    /// #32: the commit swizzle converts R↔B EXACTLY inside the given rects
    /// and leaves every other pixel untouched. The pattern has R != B at
    /// every pixel, so a no-op swizzle fails the inside assertions and an
    /// overreaching swizzle fails the outside assertions — both failure
    /// directions are expressible by this fixture.
    #[test]
    fn commit_swizzle_converts_rects_exactly_and_nothing_else() {
        let (w, h) = (8usize, 6usize);
        let stride = w * 4;
        let mut buf = vec![0u8; stride * h];
        for y in 0..h {
            for x in 0..w {
                let o = y * stride + x * 4;
                buf[o] = (10 + x) as u8; // R
                buf[o + 1] = (100 + y) as u8; // G
                buf[o + 2] = (200 - x) as u8; // B (never equals R)
                buf[o + 3] = 255;
            }
        }
        let orig = buf.clone();
        // Two interior rects + one deliberately out-of-bounds rect (clamped).
        let rects = [(1, 1, 3, 2), (5, 0, 2, 4), (-2, 4, 4, 50)];
        swizzle_rb_in_rects(&mut buf, stride, h, &rects);
        let inside = |x: usize, y: usize| {
            rects.iter().any(|&(rx, ry, rw, rh)| {
                (x as i32) >= rx
                    && (x as i32) < rx + rw
                    && (y as i32) >= ry
                    && (y as i32) < ry + rh
            })
        };
        for y in 0..h {
            for x in 0..w {
                let o = y * stride + x * 4;
                if inside(x, y) {
                    assert_eq!(buf[o], orig[o + 2], "R<-B at {x},{y}");
                    assert_eq!(buf[o + 2], orig[o], "B<-R at {x},{y}");
                } else {
                    assert_eq!(&buf[o..o + 4], &orig[o..o + 4], "untouched outside at {x},{y}");
                }
                assert_eq!(buf[o + 1], orig[o + 1], "G untouched at {x},{y}");
                assert_eq!(buf[o + 3], orig[o + 3], "A untouched at {x},{y}");
            }
        }
    }

    /// State for the harvested-breakpoint pin: counts `layout()` invocations.
    #[derive(Debug, Clone)]
    struct BreakpointState {
        layouts: usize,
    }

    /// The ribbon pattern in miniature: an inline conditional property that
    /// flips the box color at viewport width <= 720 (a threshold that was
    /// NOT on the old hardcoded CSS_BREAKPOINTS guess list).
    extern "C" fn harness_layout_breakpoint(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        use azul_css::dynamic_selector::{
            CssPropertyWithConditions, DynamicSelector, MinMaxRange,
        };
        use azul_css::props::basic::color::ColorU;
        use azul_css::props::layout::dimensions::{LayoutHeight, LayoutWidth};
        use azul_css::props::property::CssProperty;
        use azul_css::props::style::background::{
            StyleBackgroundContent, StyleBackgroundContentVec,
        };

        if let Some(mut st) = data.downcast_mut::<BreakpointState>() {
            st.layouts += 1;
        }
        let blue: StyleBackgroundContentVec =
            vec![StyleBackgroundContent::Color(ColorU { r: 0, g: 0, b: 255, a: 255 })].into();
        let red: StyleBackgroundContentVec =
            vec![StyleBackgroundContent::Color(ColorU { r: 255, g: 0, b: 0, a: 255 })].into();
        let cond_mobile: azul_css::dynamic_selector::DynamicSelectorVec =
            vec![DynamicSelector::ViewportWidth(MinMaxRange { min: f32::NAN, max: 720.0 })]
                .into();
        Dom::create_body().with_child(Dom::create_div().with_css_props(
            vec![
                CssPropertyWithConditions::simple(CssProperty::width(LayoutWidth::px(100.0))),
                CssPropertyWithConditions::simple(CssProperty::height(LayoutHeight::px(50.0))),
                CssPropertyWithConditions::simple(CssProperty::background_content(blue)),
                CssPropertyWithConditions::with_conditions(
                    CssProperty::background_content(red),
                    cond_mobile,
                ),
            ]
            .into(),
        ))
    }

    /// The first non-white Rect color in DOM 0's display list.
    fn first_box_color(window: &HeadlessWindow) -> Option<(u8, u8, u8)> {
        use azul_core::dom::DomId;
        let lw = window.common.layout_window.as_ref()?;
        let dl = &lw.layout_results.get(&DomId { inner: 0 })?.display_list;
        dl.items.iter().find_map(|it| match it {
            DisplayListItem::Rect { color, .. }
                if !(color.r > 240 && color.g > 240 && color.b > 240) =>
            {
                Some((color.r, color.g, color.b))
            }
            _ => None,
        })
    }

    /// THE HARVESTED-BREAKPOINT PIN (task #20). Shrinking across an inline
    /// conditional threshold (the ribbon's 720px pattern) must re-invoke
    /// `layout()` and flip the style; crossing a threshold that exists only
    /// on the OLD hardcoded guess list (768) must NOT — that list fired a
    /// ~66ms full regeneration on every drag across 640/768/1024/...
    #[test]
    fn resize_regenerates_exactly_at_harvested_breakpoints() {
        use azul_core::geom::LogicalSize;
        let state = Arc::new(RefCell::new(RefAny::new(BreakpointState { layouts: 0 })));
        let mut window = make_window_sized(&state, harness_layout_breakpoint, 800.0, 600.0);
        window.regenerate_layout().expect("initial layout");
        window.regenerate_layout().expect("settle");
        let base = state
            .borrow_mut()
            .downcast_ref::<BreakpointState>()
            .map(|s| s.layouts)
            .unwrap_or(0);
        assert!(base > 0, "the layout callback must have run");
        assert_eq!(first_box_color(&window), Some((0, 0, 255)), "desktop = blue");

        // 800 -> 750 crosses the old guess-list's 768 but NO threshold of
        // THIS dom: stays incremental, layout() NOT re-invoked.
        let full = window
            .common
            .request_regeneration_for_resize(
                LogicalSize::new(800.0, 600.0),
                LogicalSize::new(750.0, 600.0),
            );
        window.common.update_window_state(event::WindowStateSource::Os, |ws| {
            ws.size.dimensions = LogicalSize::new(750.0, 600.0);
        });
        assert!(
            !full,
            "768 is not a threshold of this DOM — the hardcoded guess list must not fire"
        );
        // Mirror the shells' dispatch: not-full -> the INCREMENTAL entry
        // (relayout_only), exactly what wayland's resize arm calls.
        let _ = window.common.take_relayout_only();
        window.relayout_only().expect("relayout at 750");
        let after_750 = state
            .borrow_mut()
            .downcast_ref::<BreakpointState>()
            .map(|s| s.layouts)
            .unwrap_or(0);
        assert_eq!(after_750, base, "no layout() re-invoke for a non-crossing resize");
        assert_eq!(first_box_color(&window), Some((0, 0, 255)), "still blue at 750");

        // 750 -> 600 crosses the harvested 720: full regeneration + flip.
        let full = window
            .common
            .request_regeneration_for_resize(
                LogicalSize::new(750.0, 600.0),
                LogicalSize::new(600.0, 600.0),
            );
        window.common.update_window_state(event::WindowStateSource::Os, |ws| {
            ws.size.dimensions = LogicalSize::new(600.0, 600.0);
        });
        window.regenerate_layout().expect("regen at 600");
        assert!(full, "crossing the harvested 720 must regenerate (shrink side!)");
        let after_600 = state
            .borrow_mut()
            .downcast_ref::<BreakpointState>()
            .map(|s| s.layouts)
            .unwrap_or(0);
        assert!(after_600 > after_750, "layout() must re-run across the breakpoint");
        assert_eq!(
            first_box_color(&window),
            Some((255, 0, 0)),
            "mobile style must apply after shrinking across 720"
        );

        // And back up: 600 -> 800 crosses 720 the other way.
        let full = window
            .common
            .request_regeneration_for_resize(
                LogicalSize::new(600.0, 600.0),
                LogicalSize::new(800.0, 600.0),
            );
        window.common.update_window_state(event::WindowStateSource::Os, |ws| {
            ws.size.dimensions = LogicalSize::new(800.0, 600.0);
        });
        window.regenerate_layout().expect("regen back at 800");
        assert!(full, "growing back across 720 regenerates too");
        assert_eq!(first_box_color(&window), Some((0, 0, 255)), "desktop again");
    }

    fn set_box_red(state: &Arc<RefCell<RefAny>>, red: bool) {
        let mut g = state.borrow_mut();
        let r: &mut RefAny = &mut g;
        let mut opt = r.downcast_mut::<BoxState>();
        if let Some(s) = opt.as_mut() {
            s.red = red;
        }
    }

    /// Glyph count of every Text item in the current display list (DOM 0).
    fn text_glyph_counts(window: &HeadlessWindow) -> Vec<usize> {
        use azul_core::dom::DomId;
        let lw = match window.common.layout_window.as_ref() {
            Some(lw) => lw,
            None => return Vec::new(),
        };
        let dl = match lw.layout_results.get(&DomId { inner: 0 }) {
            Some(r) => &r.display_list,
            None => return Vec::new(),
        };
        dl.items
            .iter()
            .filter_map(|it| match it {
                DisplayListItem::Text { glyphs, .. } => Some(glyphs.len()),
                _ => None,
            })
            .collect()
    }

    fn set_label(state: &Arc<RefCell<RefAny>>, new_label: &str) {
        let mut g = state.borrow_mut();
        let r: &mut RefAny = &mut g;
        let mut opt = r.downcast_mut::<UiState>();
        if let Some(s) = opt.as_mut() {
            s.label = new_label.to_string();
        }
    }

    /// Damage must survive the cases that BREAK a naive item mapping.
    ///
    /// The randomised text sequence covers glyphs moving inside a stable
    /// box. These are the shapes that move the BOX, and each one breaks a
    /// different naive assumption:
    ///
    ///  * **box-shadow** — visual bounds deliberately EXCEED the item box
    ///    (offset + blur + spread), so damage taken from the box alone
    ///    leaves the shadow's fringe stale.
    ///  * **shrinking** — the vacated area is outside the NEW bounds, so
    ///    damage must include the OLD bounds or the old pixels survive.
    ///  * **anonymous layout boxes** — the div holds inline text, so the
    ///    layout tree carries boxes the DOM never had, and resizing moves
    ///    them; any NodeId -> item mapping has to cope with items that have
    ///    no DOM identity.
    ///
    /// Each step is compared pixel-for-pixel against a full repaint, the
    /// same ground truth `damage_survives_a_randomised_edit_sequence` uses.
    ///
    /// THE ITEM COUNT IS HELD CONSTANT ON PURPOSE. `compute_display_list_
    /// damage` returns `None` — full repaint — the moment the item count
    /// changes, so a fixture that adds or removes a shadow or a child never
    /// takes the incremental path at all. The first version of this test did
    /// exactly that and passed with damage forcibly cleared, i.e. it proved
    /// nothing. Everything here varies SIZE only, so the frame stays
    /// incremental and the assertion has something to catch.
    #[test]
    fn damage_survives_shadow_shrink_and_anonymous_boxes() {
        #[derive(Debug, Clone)]
        struct ShapeState {
            variant: usize,
        }

        extern "C" fn layout_shapes(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
            use azul_css::dynamic_selector::CssPropertyWithConditions as P;
            use azul_css::props::basic::color::ColorU;
            use azul_css::props::basic::{PixelValue, PixelValueNoPercent};
            use azul_css::props::layout::dimensions::{LayoutHeight, LayoutWidth};
            use azul_css::props::property::CssProperty;
            use azul_css::props::style::background::{
                StyleBackgroundContent, StyleBackgroundContentVec,
            };
            use azul_css::props::style::box_shadow::{BoxShadowClipMode, StyleBoxShadow};

            let v = data.downcast_ref::<ShapeState>().map(|s| s.variant).unwrap_or(0);
            let bg: StyleBackgroundContentVec =
                vec![StyleBackgroundContent::Color(ColorU { r: 0, g: 0, b: 200, a: 255 })].into();

            // Variant drives size, shadow presence and child count together,
            // so consecutive steps exercise several shapes at once.
            let (w, h) = match v % 4 {
                0 => (180.0, 90.0),
                1 => (60.0, 30.0),   // shrink: vacates a large area
                2 => (200.0, 100.0), // grow back
                _ => (120.0, 60.0),
            };
            let mut props = vec![
                P::simple(CssProperty::width(LayoutWidth::px(w))),
                P::simple(CssProperty::height(LayoutHeight::px(h))),
                P::simple(CssProperty::background_content(bg)),
            ];
            {
                let shadow = StyleBoxShadow {
                    offset_x: PixelValueNoPercent { inner: PixelValue::const_px(6) },
                    offset_y: PixelValueNoPercent { inner: PixelValue::const_px(6) },
                    blur_radius: PixelValueNoPercent { inner: PixelValue::const_px(8) },
                    spread_radius: PixelValueNoPercent { inner: PixelValue::const_px(4) },
                    clip_mode: BoxShadowClipMode::default(),
                    color: ColorU { r: 0, g: 0, b: 0, a: 200 },
                };
                props.push(P::simple(CssProperty::box_shadow_left(shadow.clone())));
                props.push(P::simple(CssProperty::box_shadow_right(shadow.clone())));
                props.push(P::simple(CssProperty::box_shadow_top(shadow.clone())));
                props.push(P::simple(CssProperty::box_shadow_bottom(shadow)));
            }

            // A constant inline child: it forces the anonymous layout boxes
            // an inline formatting context creates (which have no DOM node),
            // while keeping the display-list item COUNT stable so the frame
            // stays on the incremental path — see the note on the test.
            let div = Dom::create_div()
                .with_css_props(props.into())
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper("one two three"));
            Dom::create_body().with_child(div)
        }

        let state = Arc::new(RefCell::new(RefAny::new(ShapeState { variant: 0 })));
        let mut window = make_window_with(&state, layout_shapes);
        window.regenerate_layout().expect("initial layout");
        window.regenerate_layout().expect("settle");

        for step in 1..12usize {
            if let Ok(mut b) = state.try_borrow_mut() {
                if let Some(mut s) = b.downcast_mut::<ShapeState>() {
                    s.variant = step;
                }
            }
            window.regenerate_layout().expect("incremental relayout");
            let incremental = window
                .cpu_backend
                .last_frame
                .as_ref()
                .expect("frame")
                .clone_pixmap();
            let damage = window.cpu_backend.last_frame_damage.clone();

            let mut fresh = make_window_with(&state, layout_shapes);
            fresh.regenerate_layout().expect("fresh layout");
            let full = fresh.cpu_backend.last_frame.as_ref().expect("fresh").clone_pixmap();

            let (a, b) = (incremental.data(), full.data());
            let mut diffs = 0usize;
            let mut first: Option<(u32, u32)> = None;
            let w = incremental.width();
            for i in (0..a.len().min(b.len())).step_by(4) {
                if a[i] != b[i] || a[i + 1] != b[i + 1] || a[i + 2] != b[i + 2] {
                    diffs += 1;
                    if first.is_none() {
                        let px = (i / 4) as u32;
                        first = Some((px % w, px / w));
                    }
                }
            }
            assert_eq!(
                diffs, 0,
                "variant {step} left {diffs} stale pixels, first at {first:?}. \
                 Damage did not cover a shadow fringe, a vacated area, or an \
                 anonymous-box change. damage = {damage:?}"
            );
        }
    }

    /// #27 native backbuffer: `render_frame` with an armed `native_target`
    /// draws EXACTLY the pixels the owned path produces — first full frame
    /// and every incremental follow-up — while retaining nothing: the frame
    /// lives ONLY in the external buffer (`last_frame` stays `None`, the
    /// target is consumed). The external buffer stands in for the Wayland shm
    /// slot; a single buffer trivially satisfies the shell's catch-up
    /// contract (it always holds frame N−1), so what this pins is the engine
    /// half: incremental rendering on top of an externally-owned base is
    /// bit-equal to rendering on the retained pixmap.
    #[test]
    fn native_target_render_matches_owned_and_retains_nothing() {
        #[derive(Debug, Clone)]
        struct NbState {
            variant: usize,
        }

        extern "C" fn layout_nb(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
            use azul_css::dynamic_selector::CssPropertyWithConditions as P;
            use azul_css::props::basic::color::ColorU;
            use azul_css::props::layout::dimensions::{LayoutHeight, LayoutWidth};
            use azul_css::props::property::CssProperty;
            use azul_css::props::style::background::{
                StyleBackgroundContent, StyleBackgroundContentVec,
            };

            let v = data.downcast_ref::<NbState>().map(|s| s.variant).unwrap_or(0);
            let bg: StyleBackgroundContentVec =
                vec![StyleBackgroundContent::Color(ColorU { r: 200, g: 30, b: 30, a: 255 })]
                    .into();
            // Sizes change per step, item count stays stable → the frames
            // after the first take the INCREMENTAL path (asserted below).
            let (w, h) = match v % 3 {
                0 => (180.0, 90.0),
                1 => (70.0, 40.0),
                _ => (220.0, 120.0),
            };
            let div = Dom::create_div()
                .with_css_props(
                    vec![
                        P::simple(CssProperty::width(LayoutWidth::px(w))),
                        P::simple(CssProperty::height(LayoutHeight::px(h))),
                        P::simple(CssProperty::background_content(bg)),
                    ]
                    .into(),
                )
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper("native backbuffer"));
            Dom::create_body().with_child(div)
        }

        let state = Arc::new(RefCell::new(RefAny::new(NbState { variant: 0 })));
        let mut nat = make_window_with(&state, layout_nb);
        let mut own = make_window_with(&state, layout_nb);
        nat.regenerate_layout().expect("nat initial");
        own.regenerate_layout().expect("own initial");

        // The "slot": a heap buffer standing in for the shm mapping, seeded
        // with frame 1 so it starts caught-up.
        let seed = nat
            .cpu_backend
            .last_frame
            .as_ref()
            .expect("frame 1")
            .clone_pixmap();
        let (pw, ph) = (seed.width(), seed.height());
        let mut slot = seed.data().to_vec();
        drop(seed);

        let mut saw_incremental = false;
        for step in 1..6usize {
            if let Ok(mut b) = state.try_borrow_mut() {
                if let Some(mut s) = b.downcast_mut::<NbState>() {
                    s.variant = step;
                }
            }
            // Arm: this frame renders DIRECTLY into the external buffer.
            nat.cpu_backend.native_target = unsafe {
                azul_layout::cpurender::AzulPixmap::from_external(slot.as_mut_ptr(), pw, ph)
            };
            nat.regenerate_layout().expect("native incremental");
            assert!(
                nat.cpu_backend.rendered_native,
                "step {step}: armed target was not rendered into"
            );
            assert!(
                nat.cpu_backend.last_frame.is_none(),
                "step {step}: native mode retained an owned frame"
            );
            assert!(
                nat.cpu_backend.native_target.is_none(),
                "step {step}: target not consumed"
            );
            let native_damage = nat.cpu_backend.last_frame_damage.clone();
            if matches!(native_damage, FrameDamage::Rects(_)) {
                saw_incremental = true;
            }

            own.regenerate_layout().expect("owned incremental");
            let reference = own
                .cpu_backend
                .last_frame
                .as_ref()
                .expect("owned frame")
                .clone_pixmap();
            let b = reference.data();
            assert_eq!(slot.len(), b.len(), "step {step}: size mismatch");
            let mut diffs = 0usize;
            let mut first: Option<(u32, u32)> = None;
            for i in (0..slot.len()).step_by(4) {
                if slot[i] != b[i] || slot[i + 1] != b[i + 1] || slot[i + 2] != b[i + 2] {
                    diffs += 1;
                    if first.is_none() {
                        let px = (i / 4) as u32;
                        first = Some((px % pw, px / pw));
                    }
                }
            }
            assert_eq!(
                diffs, 0,
                "step {step}: external buffer diverges from the owned render at {diffs} px, \
                 first {first:?} — native damage {native_damage:?}"
            );
        }
        assert!(
            saw_incremental,
            "every step took the full-repaint path — the external-base \
             incremental law was never exercised"
        );
    }

    /// #28: `LayoutWindow::query_pagination` — the speculative query must
    /// (a) SHARE the window's shaped-text entries by pointer (fork law:
    /// cloning the cache maps bumps Arcs, copies no shaped data — the
    /// memory/speed claim), and (b) agree EXACTLY with the ground-truth
    /// fresh-cache pagination the app side computes today (the correctness
    /// claim: forked caches change nothing about the result).
    #[test]
    fn query_pagination_matches_fresh_and_shares_shaping() {
        fn doc_dom() -> Dom {
            let mut body = Dom::create_body();
            for i in 0..14usize {
                body = body.with_child(Dom::create_text_do_not_use_without_block_level_wrapper(match i % 3 {
                    0 => "alpha beta gamma delta epsilon zeta eta theta iota kappa",
                    1 => "the quick brown fox jumps over the lazy dog again and again",
                    _ => "lorem ipsum dolor sit amet consectetur adipiscing elit sed do",
                }));
            }
            body
        }
        extern "C" fn layout_doc(_data: RefAny, _info: LayoutCallbackInfo) -> Dom {
            doc_dom()
        }

        let state = Arc::new(RefCell::new(RefAny::new(0usize)));
        let mut window = make_window_with(&state, layout_doc);
        window.regenerate_layout().expect("layout");
        let lw = window.common.layout_window.as_ref().expect("layout window");

        // (a) fork law: every shaped entry is shared, not copied.
        let keys = lw.text_cache.per_item_keys();
        assert!(!keys.is_empty(), "screen layout shaped nothing — fixture broken");
        let fork = lw.text_cache.fork_shared();
        for k in &keys {
            assert!(
                lw.text_cache.per_item_entry_ptr_eq(&fork, *k),
                "shaped entry {k} was copied, not shared"
            );
        }

        // (b) equivalence with the fresh-cache ground truth.
        use azul_layout::solver3::pagination::FakePageConfig;
        let page_size = azul_core::geom::LogicalSize::new(320.0, 180.0);
        let img_resources = azul_core::resources::ImageCache::default();

        let doc = azul_core::styled_dom::StyledDom::create_from_dom(doc_dom());
        let q = lw
            .query_pagination(&doc, page_size, FakePageConfig::new(), &img_resources)
            .expect("query_pagination");

        let doc2 = azul_core::styled_dom::StyledDom::create_from_dom(doc_dom());
        let mut oracle_cache = azul_layout::Solver3LayoutCache::default();
        let mut oracle_text = azul_layout::TextLayoutCache::new();
        let mut fm = lw.font_manager.clone_shared();
        let loader = azul_layout::text3::default::PathLoader::new();
        let font_loader = |bytes, index| loader.load_font_shared(bytes, index);
        let o = azul_layout::solver3::paged_layout::compute_document_pagination(
            &mut oracle_cache,
            &mut oracle_text,
            azul_layout::paged::FragmentationContext::new_paged(page_size),
            &doc2,
            azul_core::geom::LogicalRect {
                origin: azul_core::geom::LogicalPosition::zero(),
                size: page_size,
            },
            &mut fm,
            &std::collections::BTreeMap::new(),
            &mut None,
            None,
            &azul_core::resources::RendererResources::default(),
            azul_core::resources::IdNamespace(0),
            azul_core::dom::DomId::ROOT_ID,
            font_loader,
            FakePageConfig::new(),
            &img_resources,
            azul_core::task::GetSystemTimeCallback {
                cb: azul_core::task::get_system_time_libstd,
            },
        )
        .expect("oracle pagination");

        assert!(
            o.page_count >= 2,
            "fixture fits one page (page_count {}) — the law never bites",
            o.page_count
        );
        assert_eq!(q.page_count, o.page_count, "page counts diverge");
        assert_eq!(q.breaks, o.breaks, "break positions diverge");
        assert_eq!(
            q.total_content_height.to_bits(),
            o.total_content_height.to_bits(),
            "content height diverges ({} vs {})",
            q.total_content_height,
            o.total_content_height
        );
    }

    /// Cache GC must run on IDLE frames and not while the user is typing.
    ///
    /// `GlyphCache::gc()` frees the previous generation — thousands of
    /// vectors. Two ways to get this wrong, and this pins both:
    ///
    ///  * never calling it: the cold generation is held until a rotation
    ///    overwrites it, so memory is retained long after the text changed.
    ///  * calling it every present: `prev` never survives long enough for a
    ///    rotated-out glyph to be promoted back, collapsing the two
    ///    generations into one and reintroducing the rebuild storm the
    ///    generational scheme exists to prevent.
    #[test]
    fn glyph_cache_gc_runs_on_idle_frames_only() {
        let state = Arc::new(RefCell::new(RefAny::new(UiState {
            label: "alpha beta gamma".to_string(),
        })));
        let mut window = make_harness_window(&state);
        window.regenerate_layout().expect("initial layout");

        // A frame that CHANGES something must not GC — the caches have to
        // stay warm across an edit.
        set_label(&state, "delta epsilon zeta");
        window.regenerate_layout().expect("changed frame");
        let after_change = window.cpu_backend.glyph_cache.paths_len();
        assert!(
            after_change > 0,
            "the changed frame must have populated the glyph path cache, or \
             this test cannot tell GC from an empty cache"
        );
        assert_ne!(
            window.cpu_backend.last_frame_damage,
            FrameDamage::None,
            "a text change must damage something; if it does not, the idle \
             branch below is being reached for the wrong reason"
        );

        // Force a rotation so there IS a previous generation to collect.
        // Without this the idle frame has nothing to free and the assertion
        // below would pass whether or not GC is wired up — which is exactly
        // how the first version of this test managed to be worthless.
        for i in 0..(8192u64 + 1) {
            let _ = window
                .cpu_backend
                .glyph_cache
                .get_or_build_cells(i, 0, 16, 0.0, 0.0, 1.0, false, 1.0);
        }
        assert!(
            window.cpu_backend.glyph_cache.prev_generation_len() > 0,
            "the fixture failed to rotate a generation, so the GC assertion \
             below would be vacuous"
        );

        // Now an IDLE frame: same content, nothing to paint.
        window.regenerate_layout().expect("idle frame");
        assert_eq!(
            window.cpu_backend.glyph_cache.prev_generation_len(),
            0,
            "an idle frame must collect the previous generation — that is the \
             whole point of doing GC here rather than on a keystroke"
        );
        assert_eq!(
            window.cpu_backend.last_frame_damage,
            FrameDamage::None,
            "an unchanged relayout must be idle, or the GC hook never runs"
        );
        // GC only frees the PREVIOUS generation, so the live entries stay.
        // What must hold is that the cache is still usable and bounded.
        let after_idle = window.cpu_backend.glyph_cache.paths_len();
        assert!(
            after_idle <= after_change,
            "an idle frame must not GROW the glyph cache (was {after_change}, \
             now {after_idle})"
        );

        // And the cache must still serve: another edit reuses it rather than
        // starting from nothing.
        set_label(&state, "delta epsilon zeta eta");
        window.regenerate_layout().expect("second edit");
        assert!(
            window.cpu_backend.glyph_cache.paths_len() > 0,
            "the glyph cache must survive an idle-frame GC — if it is empty \
             here, GC threw away the LIVE generation, not the cold one"
        );
    }

    /// STRESS the damage diff with a long, varied edit sequence.
    ///
    /// `tight_text_damage_leaves_no_stale_pixels` proves one scripted edit
    /// is safe. That is not enough to trust incremental painting: the
    /// failure mode is a damage rect that misses something, and whether it
    /// does depends on the SHAPE of the edit — append, insert in the middle,
    /// delete, replace, shrink to empty, grow across a line break. Each
    /// moves a different set of glyphs.
    ///
    /// This runs 40 such edits from a fixed seed and, after every one,
    /// compares the incrementally-painted frame against a full repaint of
    /// the same content. Any rect the diff failed to emit shows up as a
    /// differing pixel at the step that caused it, so a regression names
    /// its own repro.
    ///
    /// This is the guard the display-list PATCHING work needs before it
    /// lands: patching items in place instead of regenerating them can
    /// desync the list from the layout, and a desync is invisible to every
    /// timing test but shows up here immediately.
    #[test]
    fn damage_survives_a_randomised_edit_sequence() {
        const WORDS: [&str; 6] = ["alpha", "beta", "gamma", "delta", "epsilon", "zeta"];

        let state = Arc::new(RefCell::new(RefAny::new(UiState {
            label: "alpha beta gamma".to_string(),
        })));
        let mut window = make_harness_window(&state);
        window.regenerate_layout().expect("initial layout");
        window.regenerate_layout().expect("settle");

        // Deterministic LCG — a fixed seed means a failure reproduces
        // exactly, and there is no rand dependency in this crate.
        let mut seed: u64 = 0x2545_F491_4F6C_DD1D;
        let mut next = move || {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (seed >> 33) as usize
        };

        let mut text = String::from("alpha beta gamma");
        for step in 0..40 {
            match next() % 5 {
                0 => text.push_str(WORDS[next() % WORDS.len()]),      // append
                1 => {
                    // insert in the middle
                    let at = if text.is_empty() { 0 } else { next() % text.len() };
                    let at = (0..=at).rev().find(|i| text.is_char_boundary(*i)).unwrap_or(0);
                    text.insert_str(at, WORDS[next() % WORDS.len()]);
                }
                2 => {
                    // delete a chunk
                    if !text.is_empty() {
                        let a = next() % text.len();
                        let b = (a + 1 + next() % 5).min(text.len());
                        let a = (0..=a).rev().find(|i| text.is_char_boundary(*i)).unwrap_or(0);
                        let b = (b..=text.len()).find(|i| text.is_char_boundary(*i)).unwrap_or(text.len());
                        text.replace_range(a..b, "");
                    }
                }
                3 => text = WORDS[next() % WORDS.len()].to_string(),  // replace all
                _ => text.push(' '),                                  // whitespace only
            }

            set_label(&state, &text);
            window.regenerate_layout().expect("incremental relayout");
            let incremental = window
                .cpu_backend
                .last_frame
                .as_ref()
                .expect("frame")
                .clone_pixmap();
            let damage = window.cpu_backend.last_frame_damage.clone();

            // Ground truth: the same content painted from scratch.
            let mut fresh = make_harness_window(&state);
            fresh.regenerate_layout().expect("fresh layout");
            let full = fresh.cpu_backend.last_frame.as_ref().expect("fresh").clone_pixmap();

            let (a, b) = (incremental.data(), full.data());
            let mut diffs = 0usize;
            let mut first: Option<(u32, u32)> = None;
            let w = incremental.width();
            for i in (0..a.len().min(b.len())).step_by(4) {
                if a[i] != b[i] || a[i + 1] != b[i + 1] || a[i + 2] != b[i + 2] {
                    diffs += 1;
                    if first.is_none() {
                        let px = (i / 4) as u32;
                        first = Some((px % w, px / w));
                    }
                }
            }
            assert_eq!(
                diffs, 0,
                "step {step} (text = {text:?}) left {diffs} stale pixels, first at \
                 {first:?}. The damage rects did not cover everything that \
                 changed. damage = {damage:?}"
            );
        }
    }

    /// MEASUREMENT: how tight is the damage for a one-character edit?
    ///
    /// The interactive budget is spent here. A `DisplayListItem::Text`
    /// reports its `clip_rect` as its bounds (`display_list.rs` `bounds()`),
    /// and the emission site sets that to the node's whole viewport-sized
    /// content box — so damage for a text change can never be finer than the
    /// enclosing text node, no matter how little of it changed.
    ///
    /// This prints the ratio for a paragraph wide enough to wrap to several
    /// lines, and pins the CURRENT behaviour so the next change to
    /// glyph-run splitting or `visual_bounds()` shows up as a number rather
    /// than a surprise. It deliberately does NOT assert a tight bound yet —
    /// asserting one now would just encode the defect as expected.
    /// NEGATIVE CONTROL for the tightened text damage bounds.
    ///
    /// `DisplayListItem::Text::visual_bounds()` now reports the glyph ink
    /// extent instead of the node's whole clip box, and glyph runs break at
    /// line boundaries. Both make damage rects SMALLER, and a damage rect
    /// that is too small leaves stale pixels on screen — a worse defect
    /// than the coarseness it replaces, and one no timing test can see.
    ///
    /// This edits text and then compares the incrementally-painted frame
    /// against a FULL repaint of the same content, pixel for pixel. Any
    /// region the damage rects failed to cover shows up as a difference.
    #[test]
    fn tight_text_damage_leaves_no_stale_pixels() {
        let long = "the quick brown fox jumps over the lazy dog and keeps on \
                    running past the end of the first line and onto a second";
        let state = Arc::new(RefCell::new(RefAny::new(UiState {
            label: long.to_string(),
        })));
        let mut window = make_harness_window(&state);
        window.regenerate_layout().expect("initial layout");
        window.regenerate_layout().expect("settle");

        // Edit, rendered INCREMENTALLY through the damage path.
        set_label(&state, &format!("{long} tail"));
        window.regenerate_layout().expect("incremental relayout");
        let incremental = window
            .cpu_backend
            .last_frame
            .as_ref()
            .expect("frame")
            .clone_pixmap();
        let damage = window.cpu_backend.last_frame_damage.clone();

        // Same content, but painted from scratch: drop the retained frame so
        // the next render cannot reuse anything.
        let mut fresh_window = make_harness_window(&state);
        fresh_window.regenerate_layout().expect("fresh layout");
        let full = fresh_window
            .cpu_backend
            .last_frame
            .as_ref()
            .expect("fresh frame")
            .clone_pixmap();

        assert_eq!(incremental.width(), full.width());
        assert_eq!(incremental.height(), full.height());

        let (a, b) = (incremental.data(), full.data());
        let mut diffs = 0usize;
        let mut first: Option<(u32, u32)> = None;
        let w = incremental.width();
        for i in (0..a.len().min(b.len())).step_by(4) {
            // Anti-aliasing is deterministic here (same glyphs, same
            // positions, same cache), so require an exact match on RGB.
            if a[i] != b[i] || a[i + 1] != b[i + 1] || a[i + 2] != b[i + 2] {
                diffs += 1;
                if first.is_none() {
                    let px = (i / 4) as u32;
                    first = Some((px % w, px / w));
                }
            }
        }
        println!(
            "[harness] stale-pixel check: {diffs} differing px, first at {first:?}, \
             damage = {damage:?}"
        );
        assert_eq!(
            diffs, 0,
            "the incrementally-painted frame differs from a full repaint of the \
             same content in {diffs} pixels (first at {first:?}). The damage \
             rects did not cover everything that changed, so those pixels are \
             STALE on a real screen. damage = {damage:?}"
        );
    }

    #[test]
    fn damage_one_char_edit_reports_its_granularity() {
        let long = "the quick brown fox jumps over the lazy dog and keeps on                     running past the end of the first line and onto a second";
        let state = Arc::new(RefCell::new(RefAny::new(UiState {
            label: long.to_string(),
        })));
        let mut window = make_harness_window(&state);
        window.regenerate_layout().expect("initial layout");
        window.regenerate_layout().expect("settle");

        // Append ONE character. Everything before it is untouched.
        set_label(&state, &format!("{long}X"));
        window.regenerate_layout().expect("relayout");
        let damage = window.cpu_backend.last_frame_damage.clone();
        println!(
            "[harness] glyph runs after edit = {:?}",
            text_glyph_counts(&window)
        );

        let damaged_area: f32 = match &damage {
            FrameDamage::Full => 400.0 * 300.0,
            FrameDamage::None => 0.0,
            FrameDamage::Rects(rs) => rs.iter().map(|r| r.size.width * r.size.height).sum(),
        };
        // The window is 400x300; the text node spans most of its width.
        let window_area = 400.0 * 300.0;
        println!(
            "[harness] one-char edit damage = {damage:?}\n\
             [harness]   damaged {damaged_area:.0} px2 of {window_area:.0} px2 window              = {:.1}%",
            damaged_area / window_area * 100.0
        );

        assert!(
            damaged_area > 0.0,
            "a text change must damage SOMETHING, or the edit never reaches the \
             screen"
        );
        assert!(
            !matches!(damage, FrameDamage::Full),
            "a one-character edit must not escalate to a FULL-window repaint — \
             that is the worst case and means damage tracking gave up entirely"
        );
    }

    #[test]
    fn damage_text_change_repro() {
        let state = Arc::new(RefCell::new(RefAny::new(UiState {
            label: "AAA".to_string(),
        })));
        let mut window = make_harness_window(&state);

        // Initial layout — establishes the baseline display list.
        window.regenerate_layout().expect("initial layout");
        let before = text_glyph_counts(&window);
        println!(
            "[harness] initial   : text_glyph_counts={:?} damage={:?}",
            before, window.cpu_backend.last_frame_damage
        );

        // Pure text-content change: "AAA" (3) -> "BBBBBBBB" (8).
        set_label(&state, "BBBBBBBB");
        window.regenerate_layout().expect("relayout after change");
        let after = text_glyph_counts(&window);
        let damage = window.cpu_backend.last_frame_damage.clone();
        println!(
            "[harness] post-change: text_glyph_counts={:?} damage={:?}",
            after, damage
        );

        // Baseline sanity: text shaped at all.
        assert_eq!(
            before,
            vec![3],
            "baseline: expected an initial 3-glyph run (\"AAA\"), got {:?} \
             (no fonts? text not shaping?)",
            before
        );

        // HONEST ASSERTION — reproduces the stale-text bug (#11). The display
        // list MUST reflect the new 8-char label. It currently stays at 3
        // glyphs ("AAA"), so this FAILS until #11 is fixed (do NOT weaken it).
        assert_eq!(
            after,
            vec![8],
            "STALE-TEXT BUG (#11): after changing the label to \"BBBBBBBB\" (8 chars) \
             the display list should contain an 8-glyph text run, but it still has {:?} \
             — the text change never reached the display list. Damage was {:?}, so the \
             diff/regen ran but produced STALE content (display-list generation bug, \
             not a damage bug).",
            after,
            damage
        );
    }

    /// App-side state observed by the create_callback parity test.
    #[derive(Debug, Clone)]
    struct CreateState {
        fired: usize,
    }

    extern "C" fn create_cb_installs_timer(
        mut data: RefAny,
        mut info: azul_layout::callbacks::CallbackInfo,
    ) -> azul_core::callbacks::Update {
        use azul_core::callbacks::Update;
        if let Some(mut s) = data.downcast_mut::<CreateState>() {
            s.fired += 1;
        }
        // The miniword symptom: the startup timer installed here never
        // ticked because the callback never ran under AZ_BACKEND=headless.
        info.add_timer(
            azul_core::task::TimerId { id: 777 },
            azul_layout::timer::Timer::default(),
        );
        Update::DoNothing
    }

    #[test]
    fn create_callback_fires_once_with_app_data_and_installs_timers() {
        use azul_core::icon::{IconProviderHandle, SharedIconProvider};
        let state = Arc::new(RefCell::new(RefAny::new(CreateState { fired: 0 })));

        let fc_cache = Arc::new(FcFontCache::default());
        let icon_provider = SharedIconProvider::from_handle(IconProviderHandle::default());
        let mut opts = WindowCreateOptions::default();
        opts.window_state.layout_callback = LayoutCallback {
            cb: harness_layout,
            ctx: OptionRefAny::None,
        };
        opts.window_state.size.dimensions = LogicalSize::new(400.0, 300.0);
        opts.create_callback = azul_layout::callbacks::OptionCallback::Some(
            azul_layout::callbacks::Callback {
                cb: create_cb_installs_timer,
                ctx: OptionRefAny::None,
            },
        );
        let mut window = HeadlessWindow::new(
            opts,
            state.clone(),
            event::SharedUndoManager::new(),
            AppConfig::default(),
            icon_provider,
            fc_cache,
            None,
        )
        .unwrap();
        inject_harness_font(&window);

        // Negative control: nothing fired at construction time.
        assert!(
            !window.has_active_timers(),
            "no timer may exist before invoke_create_callback"
        );
        assert_eq!(
            state.borrow_mut().downcast_ref::<CreateState>().unwrap().fired,
            0
        );

        window.invoke_create_callback();

        assert_eq!(
            state.borrow_mut().downcast_ref::<CreateState>().unwrap().fired,
            1,
            "create_callback must run with the APP data (parity with OS shells)"
        );
        assert!(
            window.has_active_timers(),
            "the timer installed by create_callback must land in the \
             layout window (AddTimer change applied)"
        );

        // Exactly once per window lifetime — a second lifecycle pass must
        // not re-fire it (OS shells consume the callback at window creation).
        window.invoke_create_callback();
        assert_eq!(
            state.borrow_mut().downcast_ref::<CreateState>().unwrap().fired,
            1,
            "create_callback fired more than once"
        );
    }

    #[test]
    fn damage_noop_relayout_is_clean() {
        let state = Arc::new(RefCell::new(RefAny::new(UiState {
            label: "Hello world".to_string(),
        })));
        let mut window = make_harness_window(&state);

        window.regenerate_layout().expect("initial layout");
        // Relayout AGAIN with the SAME state — nothing changed at all.
        window.regenerate_layout().expect("no-op relayout");
        let damage = window.cpu_backend.last_frame_damage.clone();
        println!("[harness] no-op relayout: damage={:?}", damage);

        // HONEST ASSERTION: relaying out an unchanged DOM must produce NO
        // damage. Anything else is a false-positive (e.g. text re-shaping to
        // glyphs at sub-pixel-different positions each pass), which makes the
        // incremental path repaint the whole frame every time.
        assert_eq!(
            damage,
            FrameDamage::None,
            "NO-OP relayout produced {:?} — an unchanged DOM must yield \
             FrameDamage::None; false-positive damage every frame defeats \
             incremental rendering.",
            damage
        );
    }

    #[test]
    fn damage_box_paint_change_is_local() {
        let state = Arc::new(RefCell::new(RefAny::new(BoxState { red: false })));
        let mut window = make_window_with(&state, harness_layout_box);
        window.regenerate_layout().expect("initial layout");

        // Recolor the 100x50 box blue -> red. Pure paint change, no reflow.
        set_box_red(&state, true);
        window.regenerate_layout().expect("recolor");
        let damage = window.cpu_backend.last_frame_damage.clone();
        println!("[harness] box recolor: damage={:?}", damage);

        // HONEST: recoloring a 100x50 box must damage roughly the box, NOT the
        // whole 400x300 window. This isolates the damage system from text
        // generation — if THIS passes, the damage machinery is sound and the
        // earlier failures are text-specific.
        let window_area = 400.0 * 300.0;
        match damage_area(&damage) {
            Some(a) if a > 0.0 => assert!(
                a < window_area * 0.5,
                "box recolor damage area {} should be ~box-sized (~5000), not \
                 near-full-window {} — damage={:?}",
                a, window_area, damage
            ),
            other => panic!(
                "box recolor should produce bounded incremental damage, got \
                 area={:?} damage={:?}",
                other, damage
            ),
        }
    }

    #[test]
    fn damage_box_noop_clean() {
        let state = Arc::new(RefCell::new(RefAny::new(BoxState { red: false })));
        let mut window = make_window_with(&state, harness_layout_box);
        window.regenerate_layout().expect("initial layout");
        window.regenerate_layout().expect("no-op relayout");
        let damage = window.cpu_backend.last_frame_damage.clone();
        println!("[harness] box no-op: damage={:?}", damage);

        // HONEST + diagnostic: a static colored box (no text) relaid out with
        // no change must be FrameDamage::None. If this is None but the TEXT
        // no-op test reports damage, the false-positive is text-shaping
        // specific (non-deterministic glyphs); if this also reports damage,
        // the false-positive is general.
        assert_eq!(
            damage,
            FrameDamage::None,
            "no-op relayout of a static box must be FrameDamage::None, got {:?}",
            damage
        );
    }

    #[test]
    fn perf_noop_relayout_under_budget() {
        let state = Arc::new(RefCell::new(RefAny::new(BoxState { red: false })));
        let mut window = make_window_with(&state, harness_layout_box);
        window.regenerate_layout().expect("initial layout");

        // Measure in BATCHES and judge the FASTEST one: a wall-clock average
        // under a parallel test run measures the scheduler, not the layout
        // engine (this test flaked at ~3ms/relayout with the suite saturating
        // every core while the isolated cost was a steady ~1ms). The batch
        // MINIMUM is load-immune — a real caching regression raises the
        // minimum too, so the budget still bites.
        let batches: u32 = 5;
        let per_batch: u32 = 40;
        let mut best = std::time::Duration::MAX;
        for _ in 0..batches {
            let start = std::time::Instant::now();
            for _ in 0..per_batch {
                window.regenerate_layout().expect("no-op relayout");
            }
            best = best.min(start.elapsed() / per_batch);
        }
        println!(
            "[perf] {} x {} no-op relayouts: fastest batch per={:?}",
            batches, per_batch, best
        );

        // PERF BUDGET: a no-op relayout of a trivial DOM should be cheap
        // (cache hits, no re-render). 2ms is very generous; if nothing caches
        // and every frame fully re-lays-out + re-renders, this blows past it.
        // A slow UI — especially scrolling at this cost per frame — is unusable.
        assert!(
            best < std::time::Duration::from_millis(2),
            "no-op relayout too slow: {:?}/relayout (budget 2ms, best of {} batches) — \
             incremental caching is not working; this is unusable for scrolling",
            best,
            batches
        );
    }

    // --- Reflow / structural tests via a stacked grid of colored boxes ---

    /// A vertical stack of colored boxes; each entry is (width, height). Lets
    /// tests drive size reflow, sibling shifts, and structural add/remove with
    /// one layout callback.
    #[derive(Debug, Clone)]
    struct GridState {
        boxes: Vec<(f32, f32)>,
        /// Index of a box to paint a distinct colour (for local-paint tests).
        highlight: Option<usize>,
    }

    extern "C" fn harness_layout_grid(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        use azul_css::props::layout::dimensions::{LayoutHeight, LayoutWidth};
        use azul_css::props::property::CssProperty;
        use azul_css::dynamic_selector::CssPropertyWithConditions;
        use azul_css::props::basic::color::ColorU;
        use azul_css::props::style::background::{StyleBackgroundContent, StyleBackgroundContentVec};

        let (boxes, highlight) = data
            .downcast_ref::<GridState>()
            .map(|s| (s.boxes.clone(), s.highlight))
            .unwrap_or_default();
        let mut body = Dom::create_body();
        for (i, (w, h)) in boxes.iter().enumerate() {
            let color = if Some(i) == highlight {
                ColorU { r: 30, g: 220, b: 30, a: 255 } // highlighted box
            } else if i % 2 == 0 {
                ColorU { r: 220, g: 30, b: 30, a: 255 }
            } else {
                ColorU { r: 30, g: 30, b: 220, a: 255 }
            };
            let bg: StyleBackgroundContentVec = vec![StyleBackgroundContent::Color(color)].into();
            body = body.with_child(Dom::create_div().with_css_props(
                vec![
                    CssPropertyWithConditions::simple(CssProperty::width(LayoutWidth::px(*w))),
                    CssPropertyWithConditions::simple(CssProperty::height(LayoutHeight::px(*h))),
                    CssPropertyWithConditions::simple(CssProperty::background_content(bg)),
                ]
                .into(),
            ));
        }
        body
    }

    fn set_grid(state: &Arc<RefCell<RefAny>>, boxes: Vec<(f32, f32)>) {
        let mut g = state.borrow_mut();
        let r: &mut RefAny = &mut g;
        let mut opt = r.downcast_mut::<GridState>();
        if let Some(s) = opt.as_mut() {
            s.boxes = boxes;
        }
    }

    fn set_highlight(state: &Arc<RefCell<RefAny>>, highlight: Option<usize>) {
        let mut g = state.borrow_mut();
        let r: &mut RefAny = &mut g;
        let mut opt = r.downcast_mut::<GridState>();
        if let Some(s) = opt.as_mut() {
            s.highlight = highlight;
        }
    }

    /// Max bottom-edge Y across the damage (Full = +inf, None = 0).
    fn damage_max_y(d: &FrameDamage) -> f32 {
        match d {
            FrameDamage::None => 0.0,
            FrameDamage::Full => f32::INFINITY,
            FrameDamage::Rects(rs) => rs
                .iter()
                .map(|r| r.origin.y + r.size.height)
                .fold(0.0f32, f32::max),
        }
    }

    #[test]
    fn damage_box_size_reflow() {
        let state = Arc::new(RefCell::new(RefAny::new(GridState {
            boxes: vec![(100.0, 50.0)],
            highlight: None,
        })));
        let mut window = make_window_with(&state, harness_layout_grid);
        window.regenerate_layout().expect("initial layout");

        // Widen the box 100 -> 200 (same height). Pure size reflow.
        set_grid(&state, vec![(200.0, 50.0)]);
        window.regenerate_layout().expect("reflow");
        let damage = window.cpu_backend.last_frame_damage.clone();
        println!("[harness] size reflow: damage={:?}", damage);

        // HONEST: widening must damage the box region (old∪new ⊇ the 200x50
        // box), bounded — not the whole 400x300 window, not empty.
        let window_area = 400.0 * 300.0;
        match damage_area(&damage) {
            Some(a) if a > 0.0 => assert!(
                a < window_area * 0.5,
                "size reflow damage area {} should be box-sized (~10000), not \
                 near-full-window {} — damage={:?}",
                a, window_area, damage
            ),
            other => panic!(
                "size reflow should produce bounded incremental damage, got \
                 area={:?} damage={:?}",
                other, damage
            ),
        }
    }

    #[test]
    fn damage_reflow_shifts_sibling() {
        let state = Arc::new(RefCell::new(RefAny::new(GridState {
            boxes: vec![(100.0, 50.0), (100.0, 50.0)],
            highlight: None,
        })));
        let mut window = make_window_with(&state, harness_layout_grid);
        window.regenerate_layout().expect("initial layout");

        // Grow box1's height 50 -> 100. box2 (below it) shifts DOWN by 50.
        set_grid(&state, vec![(100.0, 100.0), (100.0, 50.0)]);
        window.regenerate_layout().expect("reflow");
        let damage = window.cpu_backend.last_frame_damage.clone();
        println!("[harness] sibling shift: damage={:?}", damage);

        // HONEST: box2 moved from y≈58..108 to y≈108..158. The damage MUST reach
        // box2's new bottom (~158) — otherwise box2 leaves a ghost at its old
        // position / never paints at its new one. If damage stops at the grown
        // box1 (~108), that's the bug.
        let max_y = damage_max_y(&damage);
        assert!(
            max_y >= 140.0,
            "reflow-shift damage must reach the shifted sibling (bottom ~158), \
             got max_y={} damage={:?} — box2 would ghost/not repaint",
            max_y, damage
        );
    }

    #[test]
    fn damage_structural_add_covers_new_node() {
        let state = Arc::new(RefCell::new(RefAny::new(GridState {
            boxes: vec![(100.0, 50.0)],
            highlight: None,
        })));
        let mut window = make_window_with(&state, harness_layout_grid);
        window.regenerate_layout().expect("initial layout");

        // Add a second box below the first (structural change).
        set_grid(&state, vec![(100.0, 50.0), (100.0, 50.0)]);
        window.regenerate_layout().expect("add box");
        let damage = window.cpu_backend.last_frame_damage.clone();
        println!("[harness] structural add: damage={:?}", damage);

        // HONEST: a structural change (item count differs) can't be diffed
        // item-by-item, so a conservative FULL repaint is correct (precise
        // layout-level damage is a #10 goal). Either Full, or rects that at
        // least reach the new box (~y 58..108). NOT None — the new box must
        // paint.
        match &damage {
            FrameDamage::Full => {}
            FrameDamage::Rects(_) => {
                let max_y = damage_max_y(&damage);
                assert!(
                    max_y >= 90.0,
                    "structural add must damage the new box (~y 108), got \
                     max_y={} damage={:?}",
                    max_y, damage
                );
            }
            FrameDamage::None => panic!(
                "structural add produced NO damage — the new box would never paint"
            ),
        }
    }

    // --- Event-driven harness: drive a HeadlessEvent through the same per-event
    // path run() uses, relayout if it requested a redraw, and return the damage
    // produced this step (None if the event caused no visual change). ---
    // --- One physical release = ONE Hover(MouseUp) invocation ------------
    //
    // The regression this pins: `process_window_events` derives events from
    // the previous->current window-state diff but never CONSUMED the delta at
    // the end of a pass. Any later pass with no state change of its own (a
    // redraw tick, a wait_frame pump, the regeneration pass after RefreshDom)
    // re-detected the SAME down->up transition and re-invoked the callback -
    // every toggle widget self-cancelled (the ribbon gallery "More" panel
    // could never open). The wrapper now advances previous_window_state after
    // every completed pass.

    #[derive(Debug, Clone)]
    struct ClickCounterState {
        hits: Arc<core::sync::atomic::AtomicUsize>,
    }

    extern "C" fn count_mouse_up(
        mut refany: RefAny,
        _info: azul_layout::callbacks::CallbackInfo,
    ) -> azul_core::callbacks::Update {
        if let Some(s) = refany.downcast_ref::<ClickCounterState>() {
            s.hits.fetch_add(1, core::sync::atomic::Ordering::SeqCst);
        }
        azul_core::callbacks::Update::DoNothing
    }

    extern "C" fn click_counter_layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        use azul_core::callbacks::{CoreCallback, CoreCallbackData};
        use azul_core::events::{EventFilter, HoverEventFilter};
        use azul_css::dynamic_selector::CssPropertyWithConditions;
        use azul_css::props::layout::dimensions::{LayoutHeight, LayoutWidth};
        use azul_css::props::property::CssProperty;

        let hits = data
            .downcast_ref::<ClickCounterState>()
            .map(|s| s.hits.clone())
            .unwrap_or_default();
        Dom::create_body().with_child(
            Dom::create_div()
                .with_css_props(
                    vec![
                        CssPropertyWithConditions::simple(CssProperty::width(
                            LayoutWidth::px(400.0),
                        )),
                        CssPropertyWithConditions::simple(CssProperty::height(
                            LayoutHeight::px(300.0),
                        )),
                    ]
                    .into(),
                )
                .with_callbacks(
                    vec![CoreCallbackData {
                        event: EventFilter::Hover(HoverEventFilter::MouseUp),
                        callback: CoreCallback {
                            cb: count_mouse_up as usize,
                            ctx: azul_core::refany::OptionRefAny::None,
                        },
                        refany: RefAny::new(ClickCounterState { hits }),
                    }]
                    .into(),
                ),
        )
    }

    #[test]
    fn a_mouse_release_invokes_its_hover_callback_exactly_once() {
        use azul_core::events::MouseButton;

        let hits = Arc::new(core::sync::atomic::AtomicUsize::new(0));
        let state = Arc::new(RefCell::new(RefAny::new(ClickCounterState {
            hits: hits.clone(),
        })));
        let mut window = make_window_with(&state, click_counter_layout);
        window.regenerate_layout().expect("initial layout");

        step(&mut window, HeadlessEvent::MouseMove { x: 200.0, y: 150.0 });
        step(&mut window, HeadlessEvent::MouseDown { button: MouseButton::Left });
        step(&mut window, HeadlessEvent::MouseUp { button: MouseButton::Left });
        assert_eq!(
            hits.load(core::sync::atomic::Ordering::SeqCst),
            1,
            "one release fires the Hover(MouseUp) callback once"
        );

        // The regression trigger: run extra passes WITHOUT any state change -
        // exactly what a redraw tick or a wait_frame pump does. The consumed
        // delta must not re-fire the callback.
        for _ in 0..3 {
            use crate::desktop::shell2::common::event::PlatformWindow;
            let _ = window.process_window_events(0);
        }
        assert_eq!(
            hits.load(core::sync::atomic::Ordering::SeqCst),
            1,
            "a pass without a state change must not re-dispatch the release"
        );

        // A genuine second click still works (the delta is consumed, not lost).
        step(&mut window, HeadlessEvent::MouseDown { button: MouseButton::Left });
        step(&mut window, HeadlessEvent::MouseUp { button: MouseButton::Left });
        assert_eq!(
            hits.load(core::sync::atomic::Ordering::SeqCst),
            2,
            "the second click fires exactly once more"
        );
    }

    // --- Slider drag -----------------------------------------------------
    //
    // REPORTED: "dragging the slider leaves old thumbs on the track". The
    // widget slides its thumb with `set_css_property(thumb, margin-left)` on
    // every pointer move, and the AzWidgets demo's `on_value_change` bumps a
    // RENDERED interactions counter and returns `RefreshDom` — so each move
    // is an in-place relayout followed by a full DOM regeneration, through
    // the same `headless::CpuBackend` every desktop shell presents with.

    #[derive(Debug, Clone)]
    struct SliderUiState {
        slider_value: f32,
        interactions: usize,
    }

    extern "C" fn slider_demo_on_change(
        mut data: RefAny,
        _info: azul_layout::callbacks::CallbackInfo,
        _state: azul_layout::widgets::slider::SliderState,
    ) -> azul_core::callbacks::Update {
        if let Some(mut s) = data.downcast_mut::<SliderUiState>() {
            // Exactly what examples/azul-widgets does: count the callback,
            // do NOT store the value, refresh.
            s.interactions += 1;
        }
        azul_core::callbacks::Update::RefreshDom
    }

    extern "C" fn slider_demo_layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        use azul_core::refany::OptionRefAny;
        use azul_css::dynamic_selector::CssPropertyWithConditions as C;
        use azul_css::props::layout::spacing::{
            LayoutPaddingBottom, LayoutPaddingLeft, LayoutPaddingRight, LayoutPaddingTop,
        };
        use azul_css::props::property::CssProperty;
        use azul_layout::widgets::slider::{Slider, SliderOnValueChangeCallback};

        let (value, interactions) = data
            .downcast_ref::<SliderUiState>()
            .map(|s| (s.slider_value, s.interactions))
            .unwrap_or((0.0, 0));
        let caption = format!("callbacks fired so far: {interactions}");
        Dom::create_body()
            .with_css_props(
                vec![
                    C::simple(CssProperty::const_padding_top(LayoutPaddingTop::const_px(20))),
                    C::simple(CssProperty::const_padding_right(LayoutPaddingRight::const_px(20))),
                    C::simple(CssProperty::const_padding_bottom(LayoutPaddingBottom::const_px(20))),
                    C::simple(CssProperty::const_padding_left(LayoutPaddingLeft::const_px(20))),
                ]
                .into(),
            )
            .with_child(
                Dom::create_div().with_child(
                    Dom::create_text_do_not_use_without_block_level_wrapper(caption.as_str()),
                ),
            )
            .with_child(
                Slider::create(value, 0.0, 100.0)
                    .with_on_value_change(
                        data.clone(),
                        SliderOnValueChangeCallback {
                            cb: slider_demo_on_change,
                            ctx: OptionRefAny::None,
                        },
                    )
                    .dom(),
            )
    }

    /// The slider's live `dragging` flag, read from the callback state on the
    /// CURRENT DOM's track node — i.e. whatever the reconciler left there
    /// after the app's last `RefreshDom`. `None` when no track node exists.
    fn slider_dragging(window: &HeadlessWindow) -> Option<bool> {
        use azul_core::dom::{DomId, IdOrClass};
        use azul_layout::widgets::slider::SliderStateWrapper;

        let lw = window.common.layout_window.as_ref()?;
        let dom = lw.layout_results.get(&DomId { inner: 0 })?;
        for data in dom.styled_dom.node_data.as_container().internal.iter() {
            let is_track = data.get_ids_and_classes().iter().any(|c| match c {
                IdOrClass::Class(s) => s.as_str() == "__azul-native-slider",
                IdOrClass::Id(_) => false,
            });
            if !is_track {
                continue;
            }
            let mut state = data.callbacks.as_ref().first()?.refany.clone();
            let w = state.downcast_ref::<SliderStateWrapper>()?;
            return Some(w.dragging);
        }
        None
    }

    /// Pixel-diff the window's INCREMENTALLY presented frame against a full
    /// repaint of the SAME display list by a fresh backend (no retained
    /// pixels, nothing to blit or skip). Returns (differing px, first diff).
    fn incremental_vs_full(window: &mut HeadlessWindow) -> (usize, Option<(u32, u32)>) {
        let incremental = window
            .cpu_backend
            .last_frame
            .as_ref()
            .expect("incremental frame")
            .clone_pixmap();
        let ws = window.common.current_window_state();
        let (w, h, dpi) = (
            ws.size.dimensions.width,
            ws.size.dimensions.height,
            ws.size.dpi as f32 / 96.0,
        );
        let mut fresh = CpuBackend::new();
        let lw = window.common.layout_window.as_ref().expect("layout window");
        fresh.render_frame(lw, &window.common.renderer_resources, w, h, dpi);
        let full = fresh.last_frame.as_ref().expect("full frame").clone_pixmap();
        assert_eq!(incremental.width(), full.width());
        assert_eq!(incremental.height(), full.height());
        let (a, b) = (incremental.data(), full.data());
        let mut diffs = 0usize;
        let mut first: Option<(u32, u32)> = None;
        for i in (0..a.len().min(b.len())).step_by(4) {
            if a[i] != b[i] || a[i + 1] != b[i + 1] || a[i + 2] != b[i + 2] {
                diffs += 1;
                if first.is_none() {
                    let px = (i / 4) as u32;
                    first = Some((px % incremental.width(), px / incremental.width()));
                }
            }
        }
        (diffs, first)
    }

    #[test]
    fn dragging_the_slider_leaves_no_thumb_behind() {
        use azul_core::events::MouseButton;

        let state = Arc::new(RefCell::new(RefAny::new(SliderUiState {
            slider_value: 40.0,
            interactions: 0,
        })));
        let mut window = make_window_sized(&state, slider_demo_layout, 400.0, 160.0);
        window.regenerate_layout().expect("initial layout");
        window.regenerate_layout().expect("settle");

        let thumb = rects_by_class(&window, "__azul-native-slider-thumb");
        assert_eq!(thumb.len(), 1, "one thumb: {thumb:?}");
        let thumb0 = thumb[0];
        let track = rects_by_class(&window, "__azul-native-slider");
        assert_eq!(track.len(), 1, "one track: {track:?}");
        let track = track[0];
        let y = thumb0.origin.y + thumb0.size.height / 2.0;
        let x0 = thumb0.origin.x + thumb0.size.width / 2.0;
        println!("[slider] track={track:?} thumb={thumb0:?}");

        step(&mut window, HeadlessEvent::MouseMove { x: x0, y });
        let press_damage = step(&mut window, HeadlessEvent::MouseDown { button: MouseButton::Left });
        println!(
            "[slider] press: dragging={:?} damage={press_damage:?} thumb={:?}",
            slider_dragging(&window),
            rects_by_class(&window, "__azul-native-slider-thumb")
        );
        let (d, f) = incremental_vs_full(&mut window);
        assert_eq!(d, 0, "after the press: {d} stale px, first at {f:?}");

        // A drag: the cursor walks right across the rail in steps, like a
        // real pointer does, and every presented frame must match a full
        // repaint of what layout says is on screen.
        let mut prev_thumb = thumb0;
        for (i, dx) in [24.0f32, 48.0, 72.0, 96.0].iter().enumerate() {
            let x = x0 + dx;
            let damage = step(&mut window, HeadlessEvent::MouseMove { x, y });
            let now = rects_by_class(&window, "__azul-native-slider-thumb");
            let fired = state
                .borrow_mut()
                .downcast_ref::<SliderUiState>()
                .map(|s| s.interactions)
                .unwrap_or(0);
            println!(
                "[slider] move {i}: cursor x={x} thumb={now:?} (was {prev_thumb:?}) \
                 callbacks={fired} damage={damage:?} dragging={:?}",
                slider_dragging(&window)
            );
            let (diffs, first) = incremental_vs_full(&mut window);
            assert_eq!(
                diffs, 0,
                "drag step {i} (cursor x={x}): the presented frame differs from a full \
                 repaint of the same display list in {diffs} px, first at {first:?} — a \
                 thumb ghost / stale pixels on a real screen. thumb now {now:?}, before \
                 {prev_thumb:?}, damage {damage:?}"
            );
            if let Some(r) = now.first() {
                prev_thumb = *r;
            }
        }
        // The drag must FOLLOW the pointer across the app's RefreshDom
        // rebuilds, not die after the press. The widget maps the cursor's
        // fraction of the track onto a travel of (track − thumb): for the
        // last in-track cursor x that is where the thumb must sit. Before
        // the slider carried its `dragging` flag across a rebuild (and
        // before a bubbled MouseLeave from the thumb stopped ending the
        // drag), it followed for exactly one move in any app that refreshes
        // on change.
        let last_x = x0 + 96.0;
        let fraction = ((last_x - track.origin.x) / track.size.width).clamp(0.0, 1.0);
        let expected_x = track.origin.x + (fraction * (track.size.width - thumb0.size.width)).round();
        assert!(
            (prev_thumb.origin.x - expected_x).abs() <= 1.0,
            "the thumb did not follow the drag: at {prev_thumb:?}, expected x≈{expected_x} for \
             cursor x={last_x} (started at {thumb0:?})"
        );
        assert_eq!(slider_dragging(&window), Some(true), "still dragging inside the track");

        // Leaving the TRACK ends the drag (the widget's rule); the thumb
        // stays where the last in-track move put it, and the frame is still
        // exact.
        let outside_x = track.origin.x + track.size.width + 2.0;
        step(&mut window, HeadlessEvent::MouseMove { x: outside_x, y });
        let (diffs, first) = incremental_vs_full(&mut window);
        assert_eq!(diffs, 0, "after leaving the track: {diffs} stale px, first at {first:?}");
        assert_eq!(
            slider_dragging(&window),
            Some(false),
            "leaving the track ends the drag"
        );
        let parked = rects_by_class(&window, "__azul-native-slider-thumb");
        assert_eq!(parked.first().map(|r| r.origin.x), Some(prev_thumb.origin.x));

        step(&mut window, HeadlessEvent::MouseUp { button: MouseButton::Left });
        assert_eq!(slider_dragging(&window), Some(false), "released");
    }

    // --- A text selection survives a relayout ---------------------------
    //
    // REPORTED (demo test 2026-08-21): selecting from the heading into the
    // subtitle "flickers" the subtitle but nothing stays selected. The
    // selection was computed correctly; only the DISPLAY-LIST-ONLY rebuild
    // painted `SelectionRect`s, while the layout path handed
    // `layout_document` an empty selection map — so every relayout (a
    // resize, a restyle, an app's RefreshDom, a css patch) erased the band
    // until the next drag move repainted it, and the first relayout after the
    // release erased it for good. The class: two builders of the same
    // display list fed different inputs. This drags a selection across two
    // paragraphs, forces the LAYOUT path with a resize, and expects the band
    // to still be there.

    extern "C" fn selection_layout(_data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        Dom::create_body()
            .with_child(
                Dom::create_p_with_text("Azul Widget Showcase")
                    .with_ids_and_classes(vec![azul_core::dom::IdOrClass::Class("h".into())].into())
                    .with_css("font-size: 24px; margin: 10px;"),
            )
            .with_child(
                Dom::create_p_with_text("Every built-in widget (callbacks fired so far: 0)")
                    .with_ids_and_classes(vec![azul_core::dom::IdOrClass::Class("sub".into())].into())
                    .with_css("font-size: 14px; margin: 10px;"),
            )
    }

    fn selection_rect_count(window: &HeadlessWindow) -> usize {
        use azul_core::dom::DomId;
        let Some(lw) = window.common.layout_window.as_ref() else { return 0 };
        let Some(lr) = lw.layout_results.get(&DomId { inner: 0 }) else { return 0 };
        lr.display_list
            .items
            .iter()
            .filter(|it| matches!(it, DisplayListItem::SelectionRect { .. }))
            .count()
    }

    #[test]
    fn a_text_selection_survives_a_relayout() {
        use azul_core::events::MouseButton;
        use crate::desktop::shell2::common::event::PlatformWindow;

        let state = Arc::new(RefCell::new(RefAny::new(())));
        let mut window = make_window_sized(&state, selection_layout, 500.0, 200.0);
        window.regenerate_layout().expect("initial layout");
        window.regenerate_layout().expect("settle");

        let heading = rects_by_class(&window, "h");
        let subtitle = rects_by_class(&window, "sub");
        assert_eq!(heading.len(), 1, "{heading:?}");
        assert_eq!(subtitle.len(), 1, "{subtitle:?}");
        let (h, sub) = (heading[0], subtitle[0]);

        // Press inside the heading's first glyphs, drag into the subtitle.
        let start = (h.origin.x + 6.0, h.origin.y + h.size.height / 2.0);
        let end = (sub.origin.x + sub.size.width * 0.5, sub.origin.y + sub.size.height / 2.0);
        step(&mut window, HeadlessEvent::MouseMove { x: start.0, y: start.1 });
        step(&mut window, HeadlessEvent::MouseDown { button: MouseButton::Left });
        for i in 1..=6 {
            let t = i as f32 / 6.0;
            step(
                &mut window,
                HeadlessEvent::MouseMove {
                    x: start.0 + (end.0 - start.0) * t,
                    y: start.1 + (end.1 - start.1) * t,
                },
            );
        }
        step(&mut window, HeadlessEvent::MouseUp { button: MouseButton::Left });

        let cross_block = window
            .common
            .layout_window
            .as_ref()
            .and_then(|lw| lw.text_edit_manager.get_cross_block_selection().cloned());
        assert!(
            cross_block.is_some(),
            "dragging from the heading into the subtitle must produce a cross-block selection"
        );
        let before = selection_rect_count(&window);
        assert!(before > 0, "the selection must be painted after the drag");

        // Force the LAYOUT path: a resize is a relayout with the same DOM.
        window.snapshot_window_state_baseline("headless.test.selection_resize");
        window
            .common
            .update_window_state(event::WindowStateSource::Os, |ws| {
                ws.size.dimensions.width = 560.0;
            });
        window
            .common
            .request_regeneration(azul_core::callbacks::RelayoutReason::Resize);
        let _ = window.process_window_events(0);
        window.regenerate_layout().expect("relayout after resize");

        let after = selection_rect_count(&window);
        assert!(
            after > 0,
            "the selection band vanished on relayout ({before} SelectionRect(s) before, {after} after): \
             the layout path must paint the live selection, not an empty map"
        );
        assert!(
            window
                .common
                .layout_window
                .as_ref()
                .and_then(|lw| lw.text_edit_manager.get_cross_block_selection())
                .is_some(),
            "the selection state itself must survive the relayout"
        );
    }

    // --- A relayout-only pass keeps the hit-tester in sync ----------------
    //
    // REPORTED (demo test 2026-08-21): AzWidgets' scroll area stopped reacting
    // to the wheel after a window resize and AzMap's "+" went dead, both
    // "healing" on the next widget click. The class: a RELAYOUT-ONLY pass
    // (the coalesced resize fast path, a restyle, a runtime edit) re-runs
    // layout on the existing StyledDom, but the CPU hit-tester is a CACHE of
    // that layout and was rebuilt only by the full `regenerate_layout()`. On
    // macOS and X11 nothing else rebuilt it, so input over a node that had
    // moved went to whatever used to be there. `CommonWindowState::
    // incremental_relayout` now owns that finalize tail (and the bare layout
    // function is private to `common`); this drives the resize path through
    // it and asks the hit-tester where a right-aligned box went.

    extern "C" fn right_aligned_layout(_data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        Dom::create_body()
            .with_css(
                "display: flex; flex-direction: row; justify-content: flex-end; \
                 width: 100%; height: 100%;",
            )
            .with_child(
                Dom::create_div()
                    .with_ids_and_classes(
                        vec![azul_core::dom::IdOrClass::Class("target".into())].into(),
                    )
                    .with_css(
                        "width: 40px; height: 40px; flex-grow: 0; flex-shrink: 0; \
                         background: red;",
                    ),
            )
    }

    /// Does the window's CPU hit-tester report a `.target` node at (x, y)?
    fn cpu_hit_tester_hits_class(window: &HeadlessWindow, class: &str, x: f32, y: f32) -> bool {
        use azul_core::dom::IdOrClass;
        use azul_core::geom::LogicalPosition;

        let Some(ht) = window.common.cpu_hit_tester.as_ref() else {
            panic!("the headless window must own a CPU hit-tester");
        };
        let Some(lw) = window.common.layout_window.as_ref() else {
            return false;
        };
        ht.hit_test(LogicalPosition::new(x, y))
            .into_iter()
            .any(|(dom, node)| {
                lw.layout_results
                    .get(&dom)
                    .and_then(|lr| {
                        lr.styled_dom
                            .node_data
                            .as_container()
                            .internal
                            .get(node.index())
                            .map(|data| {
                                data.get_ids_and_classes().iter().any(|c| match c {
                                    IdOrClass::Class(s) => s.as_str() == class,
                                    IdOrClass::Id(_) => false,
                                })
                            })
                    })
                    .unwrap_or(false)
            })
    }

    #[test]
    fn a_relayout_only_pass_rebuilds_the_hit_tester() {
        use crate::desktop::shell2::common::event::{IncrementalRelayout, PlatformWindow};

        let state = Arc::new(RefCell::new(RefAny::new(())));
        let mut window = make_window_sized(&state, right_aligned_layout, 300.0, 100.0);
        window.regenerate_layout().expect("initial layout");
        window.regenerate_layout().expect("settle");

        let before = rects_by_class(&window, "target");
        assert_eq!(before.len(), 1, "{before:?}");
        assert!(
            (before[0].origin.x - 260.0).abs() < 1.0,
            "a 40 px box right-aligned in a 300 px window starts at x = 260: {before:?}"
        );
        assert!(
            cpu_hit_tester_hits_class(&window, "target", 280.0, 20.0),
            "after the full layout the hit-tester must find the box at its position"
        );

        // Widen the window through the RELAYOUT-ONLY path: the coalesced
        // resize fast path, no DOM rebuild — exactly what a live window does
        // on every drag of the window edge.
        window.snapshot_window_state_baseline("headless.test.relayout_only_hit_tester");
        window
            .common
            .update_window_state(event::WindowStateSource::Os, |ws| {
                ws.size.dimensions.width = 500.0;
            });
        let mut debug_messages = None;
        window
            .common
            .incremental_relayout(IncrementalRelayout::Resize, &mut debug_messages)
            .expect("resize fast path");

        let after = rects_by_class(&window, "target");
        assert_eq!(after.len(), 1, "{after:?}");
        assert!(
            (after[0].origin.x - 460.0).abs() < 1.0,
            "layout itself must have moved the box to x = 460: {after:?}"
        );
        assert!(
            cpu_hit_tester_hits_class(&window, "target", 480.0, 20.0),
            "STALE HIT-TESTER: layout moved the box to x = 460 but the hit-tester still \
             answers for the 300 px window — the relayout-only path skipped the rebuild"
        );
        assert!(
            !cpu_hit_tester_hits_class(&window, "target", 280.0, 20.0),
            "the box's OLD position must no longer hit it"
        );

        // The restyle flavour takes the same tail.
        window
            .common
            .update_window_state(event::WindowStateSource::Os, |ws| {
                ws.size.dimensions.width = 400.0;
            });
        window
            .common
            .incremental_relayout(IncrementalRelayout::Restyle, &mut debug_messages)
            .expect("restyle relayout");
        assert!(
            cpu_hit_tester_hits_class(&window, "target", 380.0, 20.0),
            "the restyle relayout must rebuild the hit-tester too"
        );
    }

    // --- An unchanged RefreshDom still re-renders VirtualViews -------------
    //
    // REPORTED (AzMap "+" analysis, 2026-08-22): a RefreshDom whose only
    // change lives inside a dataset or a VirtualView's refany rebuilds an
    // IDENTICAL DOM, so regenerate_layout takes an unchanged exit — and the
    // view that renders that data was never re-invoked. The full path
    // re-invokes every view (reset_all_invocation_flags); the two unchanged
    // exits re-invoked none. The class: "the DOM did not change" is not
    // "the data did not change". Both exits now queue every view, and the
    // one frame-path drain re-invokes them in place AND rebuilds the CPU
    // hit-tester (macOS and headless used to skip that rebuild).

    /// What the VirtualView renders from: a model the app mutates in place.
    struct VvCounter {
        value: u32,
    }

    /// The app state: holds the SAME RefAny across builds, the shape that
    /// fingerprints equal (a map's tile cache, a virtual list's rows).
    struct VvAppState {
        content: RefAny,
    }

    static VV_INVOCATIONS: std::sync::atomic::AtomicUsize =
        std::sync::atomic::AtomicUsize::new(0);

    extern "C" fn counter_view_render(
        data: RefAny,
        info: azul_core::callbacks::VirtualViewCallbackInfo,
    ) -> azul_core::callbacks::VirtualViewReturn {
        use azul_core::geom::{LogicalPosition, LogicalRect};
        VV_INVOCATIONS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let mut data = data;
        let value = data
            .downcast_ref::<VvCounter>()
            .map(|c| c.value)
            .unwrap_or(u32::MAX);
        let size = info.get_bounds().get_logical_size();
        let rect = LogicalRect::new(LogicalPosition::zero(), size);
        azul_core::callbacks::VirtualViewReturn {
            dom: azul_core::dom::OptionDom::Some(
                Dom::create_div()
                    .with_child(Dom::create_p_with_text(format!("value {value}").as_str())),
            ),
            materialized: rect,
            virtual_rect: rect,
        }
    }

    extern "C" fn counter_view_layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        let content = data
            .downcast_ref::<VvAppState>()
            .map(|s| s.content.clone())
            .expect("app state");
        Dom::create_body().with_child(
            Dom::create_virtual_view(
                content,
                azul_core::callbacks::VirtualViewCallback::create(counter_view_render),
            )
            .with_css("width: 200px; height: 100px;"),
        )
    }

    /// Every text node in every NESTED dom (a VirtualView's content), debug-formatted.
    fn nested_dom_texts(window: &HeadlessWindow) -> Vec<String> {
        use azul_core::dom::NodeType;
        let Some(lw) = window.common.layout_window.as_ref() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for (dom_id, lr) in &lw.layout_results {
            if dom_id.inner == 0 {
                continue;
            }
            for nd in lr.styled_dom.node_data.as_container().internal.iter() {
                if let NodeType::Text(_) = nd.get_node_type() {
                    out.push(format!("{:?}", nd.get_node_type()));
                }
            }
        }
        out
    }

    fn pending_virtual_view_updates(window: &HeadlessWindow) -> usize {
        window
            .common
            .layout_window
            .as_ref()
            .map_or(0, |lw| lw.pending_virtual_view_updates.values().map(|m| m.len()).sum())
    }

    #[test]
    fn an_unchanged_refresh_dom_still_reinvokes_virtual_views() {
        use crate::desktop::shell2::common::event::PlatformWindow;
        use crate::desktop::shell2::common::layout::LayoutRegenerateResult;
        use azul_core::geom::LogicalPosition;
        use std::sync::atomic::Ordering;

        let state = Arc::new(RefCell::new(RefAny::new(VvAppState {
            content: RefAny::new(VvCounter { value: 0 }),
        })));
        let mut window = make_window_sized(&state, counter_view_layout, 300.0, 200.0);
        window.regenerate_layout().expect("initial layout");
        window.regenerate_layout().expect("settle");
        // Settle the queue the identical second build just raised, so the
        // count below starts from a drained state.
        window.common.drain_virtual_view_updates();
        assert!(
            nested_dom_texts(&window).iter().any(|t| t.contains("value 0")),
            "the view must have rendered the initial model: {:?}",
            nested_dom_texts(&window)
        );
        let invocations_before = VV_INVOCATIONS.load(Ordering::SeqCst);
        assert!(invocations_before >= 1);

        // The app mutates its model IN PLACE and asks for a RefreshDom. The
        // rebuilt DOM is identical (same RefAny, same callback, same CSS).
        {
            let mut g = state.borrow_mut();
            let r: &mut RefAny = &mut g;
            let mut app = r.downcast_mut::<VvAppState>().expect("app state");
            let mut counter = app.content.downcast_mut::<VvCounter>().expect("counter");
            counter.value = 1;
        }
        window
            .common
            .request_regeneration(azul_core::callbacks::RelayoutReason::RefreshDom);
        let result = window.regenerate_layout().expect("refresh");
        assert!(
            matches!(result, LayoutRegenerateResult::LayoutUnchanged),
            "this test exercises the UNCHANGED exit — an identical rebuild must take it"
        );
        assert!(
            pending_virtual_view_updates(&window) > 0,
            "an unchanged RefreshDom must queue the VirtualViews for a re-invoke: the DOM \
             did not change, the data behind the view did"
        );

        // The frame path drains the queue before it paints.
        assert!(
            window.common.drain_virtual_view_updates(),
            "the drain must report that a view was rebuilt"
        );
        assert_eq!(pending_virtual_view_updates(&window), 0, "the drain empties the queue");
        assert!(
            VV_INVOCATIONS.load(Ordering::SeqCst) > invocations_before,
            "the view's callback must run again after the RefreshDom"
        );
        let texts = nested_dom_texts(&window);
        assert!(
            texts.iter().any(|t| t.contains("value 1")),
            "the view must now render the mutated model, not last frame's: {texts:?}"
        );

        // And the hit-tester knows the REBUILT child DOM (fresh NodeIds): a
        // point inside the view resolves to a node of a nested dom.
        let ht = window
            .common
            .cpu_hit_tester
            .as_ref()
            .expect("headless owns a CPU hit-tester");
        let hits = ht.hit_test(LogicalPosition::new(100.0, 50.0));
        assert!(
            hits.iter().any(|(dom, _)| dom.inner != 0),
            "after the drain the hit-tester must index the view's rebuilt content: {hits:?}"
        );
    }

    // --- A native pinch reaches the callbacks of its own pass --------------
    //
    // REPORTED (AzMap, 2026-08-21): a trackpad pinch over the map did nothing.
    // The class: per-pass input that callbacks read LIVE from a manager
    // (the wheel delta via get_scroll_delta, the injected native gesture via
    // get_pinch) must be cleared AFTER dispatch. The native gesture was
    // cleared with the other manager flags during determination, so the
    // PinchIn/PinchOut event it produced was dispatched to a callback that
    // read `None`. The clear now sits next to the wheel delta's, after
    // dispatch; this injects a magnify the way macOS does and checks what
    // the callback saw — and that it does not re-fire on the next pass.

    #[derive(Debug, Clone)]
    struct PinchLog {
        /// (callbacks invoked, callbacks that saw a pinch, last scale × 1000)
        seen: Arc<core::sync::atomic::AtomicUsize>,
        invoked: Arc<core::sync::atomic::AtomicUsize>,
        scale_milli: Arc<core::sync::atomic::AtomicUsize>,
    }

    extern "C" fn log_pinch(
        mut refany: RefAny,
        info: azul_layout::callbacks::CallbackInfo,
    ) -> azul_core::callbacks::Update {
        use core::sync::atomic::Ordering;
        if let Some(log) = refany.downcast_ref::<PinchLog>() {
            log.invoked.fetch_add(1, Ordering::SeqCst);
            if let Some(p) = info.get_pinch().into_option() {
                log.seen.fetch_add(1, Ordering::SeqCst);
                log.scale_milli
                    .store((p.scale * 1000.0).round() as usize, Ordering::SeqCst);
            }
        }
        azul_core::callbacks::Update::DoNothing
    }

    extern "C" fn pinch_layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        use azul_core::callbacks::{CoreCallback, CoreCallbackData};
        use azul_core::events::{EventFilter, HoverEventFilter};
        let log = data
            .downcast_ref::<PinchLog>()
            .map(|l| l.clone())
            .expect("pinch log");
        Dom::create_body().with_child(
            Dom::create_div()
                .with_css("width: 300px; height: 200px;")
                .with_callbacks(
                    vec![
                        CoreCallbackData {
                            event: EventFilter::Hover(HoverEventFilter::PinchOut),
                            callback: CoreCallback {
                                cb: log_pinch as usize,
                                ctx: azul_core::refany::OptionRefAny::None,
                            },
                            refany: RefAny::new(log.clone()),
                        },
                        CoreCallbackData {
                            event: EventFilter::Hover(HoverEventFilter::PinchIn),
                            callback: CoreCallback {
                                cb: log_pinch as usize,
                                ctx: azul_core::refany::OptionRefAny::None,
                            },
                            refany: RefAny::new(log),
                        },
                    ]
                    .into(),
                ),
        )
    }

    #[test]
    fn a_native_pinch_is_visible_to_the_callbacks_of_its_own_pass() {
        use azul_layout::managers::gesture::{DetectedPinch, NativeGestureEvent};
        use core::sync::atomic::{AtomicUsize, Ordering};
        use crate::desktop::shell2::common::event::PlatformWindow;

        let log = PinchLog {
            seen: Arc::new(AtomicUsize::new(0)),
            invoked: Arc::new(AtomicUsize::new(0)),
            scale_milli: Arc::new(AtomicUsize::new(0)),
        };
        let state = Arc::new(RefCell::new(RefAny::new(log.clone())));
        let mut window = make_window_sized(&state, pinch_layout, 400.0, 300.0);
        window.regenerate_layout().expect("initial layout");

        // Hover the box (PinchIn/PinchOut target the hovered node), then inject
        // a magnify exactly like macOS's magnify handler does, and run a pass.
        step(&mut window, HeadlessEvent::MouseMove { x: 150.0, y: 100.0 });
        window
            .common
            .layout_window
            .as_mut()
            .expect("layout window")
            .gesture_drag_manager
            .inject_native_gesture(NativeGestureEvent::Pinch(DetectedPinch {
                scale: 1.5,
                center: azul_core::geom::LogicalPosition::new(150.0, 100.0),
                initial_distance: 100.0,
                current_distance: 150.0,
                duration_ms: 0,
            }));
        window.snapshot_window_state_baseline("headless.test.magnify");
        let _ = window.process_window_events(0);

        assert_eq!(
            log.invoked.load(Ordering::SeqCst),
            1,
            "the injected magnify must dispatch exactly one PinchOut callback"
        );
        assert_eq!(
            log.seen.load(Ordering::SeqCst),
            1,
            "the callback must be able to READ the pinch it was dispatched for: \
             clearing the native gesture before dispatch hands it `None`"
        );
        assert_eq!(log.scale_milli.load(Ordering::SeqCst), 1500);

        // The gesture is consumed by its pass: a later pass with nothing new
        // must not re-fire it.
        window.snapshot_window_state_baseline("headless.test.magnify_idle");
        let _ = window.process_window_events(0);
        assert_eq!(
            log.invoked.load(Ordering::SeqCst),
            1,
            "an ended pinch must not re-fire on the next pass"
        );
    }

    // --- NodeResized fires when a node's box changes ----------------------
    //
    // REPORTED (AzPaint): "do we have a working 'node was resized' event?"
    // `ComponentEventFilter::NodeResized` is public API (the video widget
    // resizes its decoder target on it) and had NEVER fired in a running
    // app: its only emitter compared layout maps production passed EMPTY,
    // ran before the new tree was solved, and was not reached by a window
    // resize at all. The class: a lifecycle event derived from layout must
    // be derived from the SOLVED layout, after every pass — and the relayout
    // paths must deliver it. This resizes through the fast path, through a
    // no-op restyle, and through a full rebuild.

    #[derive(Debug, Clone)]
    struct ResizeLog {
        hits: Arc<core::sync::atomic::AtomicUsize>,
        last_width: Arc<core::sync::atomic::AtomicUsize>,
    }

    extern "C" fn on_node_resized(
        mut refany: RefAny,
        info: azul_layout::callbacks::CallbackInfo,
    ) -> azul_core::callbacks::Update {
        use core::sync::atomic::Ordering;
        if let Some(log) = refany.downcast_ref::<ResizeLog>() {
            log.hits.fetch_add(1, Ordering::SeqCst);
            if let Some(size) = info.get_node_size(info.get_hit_node()) {
                log.last_width.store(size.width.round() as usize, Ordering::SeqCst);
            }
        }
        azul_core::callbacks::Update::DoNothing
    }

    extern "C" fn node_resized_layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        use azul_core::callbacks::{CoreCallback, CoreCallbackData};
        use azul_core::dom::{ComponentEventFilter, EventFilter};
        let log = data
            .downcast_ref::<ResizeLog>()
            .map(|l| l.clone())
            .expect("resize log");
        Dom::create_body()
            .with_css("display: flex; flex-direction: row; width: 100%; height: 100%;")
            .with_child(
                Dom::create_div().with_css("width: 100px; height: 50px; flex-grow: 0; flex-shrink: 0;"),
            )
            .with_child(
                Dom::create_div()
                    .with_css("flex-grow: 1; height: 50px;")
                    .with_callbacks(
                        vec![CoreCallbackData {
                            event: EventFilter::Component(ComponentEventFilter::NodeResized),
                            callback: CoreCallback {
                                cb: on_node_resized as usize,
                                ctx: azul_core::refany::OptionRefAny::None,
                            },
                            refany: RefAny::new(log),
                        }]
                        .into(),
                    ),
            )
    }

    #[test]
    fn node_resized_fires_after_a_relayout() {
        use crate::desktop::shell2::common::event::{IncrementalRelayout, PlatformWindow};
        use core::sync::atomic::{AtomicUsize, Ordering};

        let log = ResizeLog {
            hits: Arc::new(AtomicUsize::new(0)),
            last_width: Arc::new(AtomicUsize::new(0)),
        };
        let state = Arc::new(RefCell::new(RefAny::new(log.clone())));
        let mut window = make_window_sized(&state, node_resized_layout, 400.0, 100.0);
        window.regenerate_layout().expect("initial layout");
        window.regenerate_layout().expect("settle");
        assert_eq!(log.hits.load(Ordering::SeqCst), 0, "a mount is not a resize");

        // Widen through the RESIZE FAST PATH (no rebuild): 400 → 600 px grows
        // the flex child from 300 to 500 px.
        window.snapshot_window_state_baseline("headless.test.node_resized");
        window
            .common
            .update_window_state(event::WindowStateSource::Os, |ws| {
                ws.size.dimensions.width = 600.0;
            });
        let mut debug_messages = None;
        window
            .incremental_relayout_dispatching(IncrementalRelayout::Resize, &mut debug_messages)
            .expect("resize fast path");
        assert_eq!(
            log.hits.load(Ordering::SeqCst),
            1,
            "NodeResized must fire once for the child whose box grew (and not for \
             the fixed-width sibling)"
        );
        assert_eq!(log.last_width.load(Ordering::SeqCst), 500, "the callback sees the NEW size");

        // A relayout that changes nothing is not a resize.
        window
            .incremental_relayout_dispatching(IncrementalRelayout::Restyle, &mut debug_messages)
            .expect("restyle");
        assert_eq!(log.hits.load(Ordering::SeqCst), 1, "an unchanged box must not re-fire");

        // A FULL regeneration at yet another size fires too: the node keeps
        // its identity across the rebuild, so its baseline follows it.
        window
            .common
            .update_window_state(event::WindowStateSource::Os, |ws| {
                ws.size.dimensions.width = 500.0;
            });
        window
            .common
            .request_regeneration(azul_core::callbacks::RelayoutReason::Resize);
        window.regenerate_layout().expect("full regeneration");
        assert_eq!(log.hits.load(Ordering::SeqCst), 2, "the full path delivers NodeResized as well");
        assert_eq!(log.last_width.load(Ordering::SeqCst), 400);
    }

    // --- Indicator marks are centred ------------------------------------
    //
    // REPORTED (demo test 2026-08-21): "CheckBox not centered" — the 8 px mark
    // sat in the top-left of the box. The class: an indicator widget (check
    // box, radio dot, switch knob) positions its mark with its CONTAINER's
    // layout, and a container that is not a centring flex box parks the mark
    // at its content origin. This lays the indicator widgets out for real
    // and measures, so a container style edit that drops the centring fails
    // here rather than in a screenshot.

    extern "C" fn indicator_layout(_data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        use azul_layout::widgets::{check_box::CheckBox, radio_group::RadioGroup, switch::Switch};
        let options: azul_css::StringVec =
            vec![azul_css::AzString::from("a"), azul_css::AzString::from("b")].into();
        Dom::create_body()
            .with_child(CheckBox::create(true).dom())
            .with_child(RadioGroup::create(options).dom())
            .with_child(Switch::create(true).dom())
    }

    #[test]
    fn indicator_marks_are_centred_in_their_boxes() {
        fn centre(r: &azul_core::geom::LogicalRect) -> (f32, f32) {
            (r.origin.x + r.size.width / 2.0, r.origin.y + r.size.height / 2.0)
        }
        let state = Arc::new(RefCell::new(RefAny::new(())));
        let mut window = make_window_sized(&state, indicator_layout, 400.0, 300.0);
        window.regenerate_layout().expect("initial layout");
        window.regenerate_layout().expect("settle");

        // (widget, container class, mark class, horizontally centred too?)
        // A switch knob sits at one END of its track by design; only its
        // vertical centring is a layout invariant.
        for (widget, container, mark, both_axes) in [
            ("CheckBox", "__azul-native-checkbox-container", "__azul-native-checkbox-content", true),
            ("RadioGroup", "__azul-native-radio-group-circle", "__azul-native-radio-group-dot", true),
            ("Switch", "__azul-native-switch", "__azul-native-switch-knob", false),
        ] {
            let boxes = rects_by_class(&window, container);
            let marks = rects_by_class(&window, mark);
            assert!(
                !boxes.is_empty() && boxes.len() == marks.len(),
                "{widget}: {} boxes vs {} marks ({boxes:?} / {marks:?})",
                boxes.len(),
                marks.len()
            );
            for (b, m) in boxes.iter().zip(marks.iter()) {
                let (bx, by) = centre(b);
                let (mx, my) = centre(m);
                assert!(
                    (by - my).abs() <= 0.5,
                    "{widget}: mark {m:?} is not vertically centred in its box {b:?}"
                );
                if both_axes {
                    assert!(
                        (bx - mx).abs() <= 0.5,
                        "{widget}: mark {m:?} is not horizontally centred in its box {b:?}"
                    );
                }
            }
        }
    }

    // --- Ribbon tab switching -------------------------------------------
    //
    // REPORTED: "clicking on various tabs causes repaint / damage rect
    // issues, i.e. it's as if the damage rects have a NodeID issue and take
    // the damage rect from a different node (anonymous nodes?)".
    //
    // A tab click is the hardest case a real app hands the incremental
    // path: the callback returns RefreshDom, the content band is REPLACED
    // by a different tab's groups (a different node COUNT, so NodeIds
    // shift), and two tab headers swap their active/inactive style.
    //
    // The harness puts a large, deliberately UNCHANGING document block
    // below the ribbon as a control surface.

    #[derive(Debug, Clone)]
    struct RibbonUiState {
        active_tab: usize,
        /// Give every tab header its own border colour (`RibbonTab::style`).
        /// Off for the damage tests so the strip stays uniform; on for the
        /// test that pins what the aid exposed.
        bordered: bool,
    }

    /// Desktop viewport: at 400x300 the ribbon renders its MOBILE variant,
    /// which has no tab strip to click.
    const RIBBON_W: f32 = 1000.0;
    const RIBBON_H: f32 = 600.0;

    extern "C" fn on_ribbon_tab(
        mut refany: RefAny,
        _info: azul_layout::callbacks::CallbackInfo,
        idx: usize,
    ) -> azul_core::callbacks::Update {
        if let Some(mut s) = refany.downcast_mut::<RibbonUiState>() {
            s.active_tab = idx;
        }
        azul_core::callbacks::Update::RefreshDom
    }

    extern "C" fn ribbon_layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        use azul_css::dynamic_selector::CssPropertyWithConditions;
        use azul_css::props::basic::color::ColorU;
        use azul_css::props::layout::dimensions::{LayoutHeight, LayoutWidth};
        use azul_css::props::property::CssProperty;
        use azul_css::props::style::background::{
            StyleBackgroundContent, StyleBackgroundContentVec,
        };
        use azul_css::AzString;
        use azul_css::dynamic_selector::CssPropertyWithConditionsVec;
        use azul_layout::widgets::ribbon::{
            Ribbon, RibbonButton, RibbonGroup, RibbonItem, RibbonTab, RibbonTabVec,
        };

        use azul_css::props::style::border::{
            BorderStyle, LayoutBorderBottomWidth, LayoutBorderLeftWidth,
            LayoutBorderRightWidth, LayoutBorderTopWidth, StyleBorderBottomColor,
            StyleBorderBottomStyle, StyleBorderLeftColor, StyleBorderLeftStyle,
            StyleBorderRightColor, StyleBorderRightStyle, StyleBorderTopColor,
            StyleBorderTopStyle,
        };

        let (active, bordered) = data
            .downcast_ref::<RibbonUiState>()
            .map(|s| (s.active_tab, s.bordered))
            .unwrap_or((0, false));

        let tab_border = |r: u8, g: u8, b: u8| {
            if !bordered {
                return CssPropertyWithConditionsVec::from_vec(Vec::new());
            }
            let c = ColorU { r, g, b, a: 255 };
            CssPropertyWithConditionsVec::from_vec(vec![
                CssPropertyWithConditions::simple(CssProperty::border_top_style(
                    StyleBorderTopStyle { inner: BorderStyle::Solid },
                )),
                CssPropertyWithConditions::simple(CssProperty::border_right_style(
                    StyleBorderRightStyle { inner: BorderStyle::Solid },
                )),
                CssPropertyWithConditions::simple(CssProperty::border_bottom_style(
                    StyleBorderBottomStyle { inner: BorderStyle::Solid },
                )),
                CssPropertyWithConditions::simple(CssProperty::border_left_style(
                    StyleBorderLeftStyle { inner: BorderStyle::Solid },
                )),
                CssPropertyWithConditions::simple(CssProperty::border_top_width(
                    LayoutBorderTopWidth::const_px(2),
                )),
                CssPropertyWithConditions::simple(CssProperty::border_right_width(
                    LayoutBorderRightWidth::const_px(2),
                )),
                CssPropertyWithConditions::simple(CssProperty::border_bottom_width(
                    LayoutBorderBottomWidth::const_px(2),
                )),
                CssPropertyWithConditions::simple(CssProperty::border_left_width(
                    LayoutBorderLeftWidth::const_px(2),
                )),
                CssPropertyWithConditions::simple(CssProperty::border_top_color(
                    StyleBorderTopColor { inner: c },
                )),
                CssPropertyWithConditions::simple(CssProperty::border_right_color(
                    StyleBorderRightColor { inner: c },
                )),
                CssPropertyWithConditions::simple(CssProperty::border_bottom_color(
                    StyleBorderBottomColor { inner: c },
                )),
                CssPropertyWithConditions::simple(CssProperty::border_left_color(
                    StyleBorderLeftColor { inner: c },
                )),
            ])
        };

        // Different group counts and label lengths per tab, so the replaced
        // subtree differs in node count AND painted extent - a rect carried
        // over from the previous tab cannot happen to be the right size.
        let tab = |label: &str, groups: usize, btns: usize| {
            let mut t = RibbonTab::new(AzString::from(label));
            for g in 0..groups {
                let mut grp = RibbonGroup::new(AzString::from(format!("{label}-G{g}")));
                for b in 0..btns {
                    grp.add_item(RibbonItem::LargeButton(RibbonButton::new(
                        AzString::from(""),
                        AzString::from(format!("{label}{g}{b}")),
                    )));
                }
                t.add_group(grp);
            }
            t
        };

        // Each tab gets a DIFFERENT border colour (RibbonTab::style, the
        // per-tab hook). This is the debug aid the reported artifact needed:
        // with the strip painted in one colour, a rect attributed to the
        // wrong tab is invisible, and telling two tabs apart in a frame dump
        // meant reading their labels.
        let ribbon = Ribbon::new(RibbonTabVec::from_vec(vec![
            tab("HOME", 3, 2).with_style(tab_border(255, 0, 0)),
            tab("INSERT", 1, 4).with_style(tab_border(0, 160, 0)),
            tab("DESIGN", 2, 1).with_style(tab_border(0, 0, 255)),
        ]))
        .with_active_tab(active)
        .with_on_tab_click(
            data.clone(),
            on_ribbon_tab as azul_layout::widgets::ribbon::RibbonOnTabClickCallbackType,
        );

        let doc = Dom::create_div().with_css_props(
            vec![
                CssPropertyWithConditions::simple(CssProperty::width(LayoutWidth::px(RIBBON_W))),
                CssPropertyWithConditions::simple(CssProperty::height(LayoutHeight::px(RIBBON_H))),
                CssPropertyWithConditions::simple(CssProperty::background_content(
                    StyleBackgroundContentVec::from(vec![StyleBackgroundContent::Color(
                        ColorU { r: 0, g: 128, b: 0, a: 255 },
                    )]),
                )),
            ]
            .into(),
        );

        Dom::create_body().with_child(ribbon.dom()).with_child(doc)
    }

    /// On-screen rects of every laid-out node carrying `class`, left to right.
    fn rects_by_class(
        window: &HeadlessWindow,
        class: &str,
    ) -> Vec<azul_core::geom::LogicalRect> {
        use azul_core::dom::{DomId, DomNodeId, IdOrClass};
        use azul_core::geom::LogicalRect;
        use azul_core::styled_dom::NodeHierarchyItemId;

        let Some(lw) = window.common.layout_window.as_ref() else {
            return Vec::new();
        };
        let Some(dom) = lw.layout_results.get(&DomId { inner: 0 }) else {
            return Vec::new();
        };
        let mut found: Vec<LogicalRect> = Vec::new();
        for (node_id, data) in
            dom.styled_dom.node_data.as_container().internal.iter().enumerate()
        {
            let matches = data.get_ids_and_classes().iter().any(|c| match c {
                IdOrClass::Class(s) => s.as_str() == class,
                IdOrClass::Id(_) => false,
            });
            if !matches {
                continue;
            }
            let dnid = DomNodeId {
                dom: DomId { inner: 0 },
                node: NodeHierarchyItemId::from_crate_internal(Some(
                    azul_core::dom::NodeId::new(node_id),
                )),
            };
            // A tab header has a MouseUp callback, so it has a tag and a
            // HitTestArea; the ribbon root does not, so fall back to the
            // layout box. A node in the DOM but NOT in the layout (the
            // mobile variant on a desktop viewport) yields neither.
            let bounds = lw.get_node_hit_test_bounds(dnid).or_else(|| {
                Some(LogicalRect {
                    origin: lw.get_node_position(dnid)?,
                    size: lw.get_node_size(dnid)?,
                })
            });
            if let Some(r) = bounds {
                found.push(r);
            }
        }
        found.sort_by(|a, b| a.origin.x.total_cmp(&b.origin.x));
        found
    }

    fn tab_header_centre(window: &HeadlessWindow, idx: usize) -> Option<(f32, f32)> {
        let r = *rects_by_class(window, "__azul-native-ribbon-tab").get(idx)?;
        Some((
            r.origin.x + r.size.width / 2.0,
            r.origin.y + r.size.height / 2.0,
        ))
    }

    /// Bottom edge of the ribbon as LAID OUT; below it is the document area.
    /// Measured, not hard-coded: a constant that drifts past the real edge
    /// turns the control surface into a no-op.
    fn ribbon_bottom(window: &HeadlessWindow) -> f32 {
        rects_by_class(window, "__azul-native-ribbon")
            .iter()
            .map(|r| r.origin.y + r.size.height)
            .fold(0.0_f32, f32::max)
    }

    /// The app-side tab index (`RefAny::downcast_ref` needs `&mut self`).
    fn active_tab_of(state: &Arc<RefCell<RefAny>>) -> Option<usize> {
        let mut g = state.borrow_mut();
        let r: &mut RefAny = &mut g;
        r.downcast_ref::<RibbonUiState>().map(|s| s.active_tab)
    }

    fn click_at(window: &mut HeadlessWindow, x: f32, y: f32) -> FrameDamage {
        use azul_core::events::MouseButton;
        step(window, HeadlessEvent::MouseMove { x, y });
        step(window, HeadlessEvent::MouseDown { button: MouseButton::Left });
        step(window, HeadlessEvent::MouseUp { button: MouseButton::Left })
    }

    /// Text items of a window's display list, as debug strings.
    fn dl_texts(window: &HeadlessWindow) -> Vec<String> {
        let Some(lw) = window.common.layout_window.as_ref() else {
            return Vec::new();
        };
        let Some(r) = lw.layout_results.get(&azul_core::dom::DomId { inner: 0 }) else {
            return Vec::new();
        };
        r.display_list
            .items
            .iter()
            .filter_map(|i| match i {
                DisplayListItem::Text { .. } => Some(format!("{i:?}")),
                _ => None,
            })
            .collect()
    }

    /// Build the pair the tab-switch tests compare: a window that reached
    /// `active_tab = 1` by CLICKING, and a fresh one that started there.
    fn switched_and_fresh() -> (HeadlessWindow, HeadlessWindow, f32, f32, FrameDamage) {
        let state = Arc::new(RefCell::new(RefAny::new(RibbonUiState { active_tab: 0, bordered: false })));
        let mut window = make_window_sized(&state, ribbon_layout, RIBBON_W, RIBBON_H);
        window.regenerate_layout().expect("initial layout");
        window.regenerate_layout().expect("settle");

        let Some((x, y)) = tab_header_centre(&window, 1) else {
            panic!("no ribbon tab headers in the layout - the harness built the wrong DOM");
        };
        let damage = click_at(&mut window, x, y);
        window.regenerate_layout().expect("post-click relayout");
        assert_eq!(
            active_tab_of(&state),
            Some(1),
            "the click at ({x}, {y}) did not reach the tab header, so every \
             comparison below would pass for the wrong reason"
        );

        // The same state painted from scratch. The cursor has to sit where
        // it sits in the window under test: the tab under the pointer draws
        // in its hover style, so a fresh window with the mouse elsewhere is
        // a DIFFERENT picture and the comparison would report hover as
        // staleness.
        let fresh_state = Arc::new(RefCell::new(RefAny::new(RibbonUiState { active_tab: 1, bordered: false })));
        let mut fresh = make_window_sized(&fresh_state, ribbon_layout, RIBBON_W, RIBBON_H);
        fresh.regenerate_layout().expect("fresh layout");
        step(&mut fresh, HeadlessEvent::MouseMove { x, y });
        fresh.regenerate_layout().expect("fresh layout under the cursor");

        (window, fresh, x, y, damage)
    }

    /// A tab click must not leave the layout tree describing a DOM that no
    /// longer exists.
    ///
    /// This is the regression for the reconciliation identity bug: switching
    /// from a 3-group tab to a 1-group tab SHRINKS the DOM, reconciliation
    /// falls back to positional matching for the children whose ids moved,
    /// and `clone_node_from_old` used to carry the OLD node's `dom_node_id`
    /// into the new tree. `assert_dom_ids_are_in_range` (solver3::cache)
    /// fires on the resulting tree; before the fix this panicked with
    /// "index out of bounds: the len is 37 but the index is 52" inside
    /// `compute_counters`.
    ///
    /// NEGATIVE CONTROL: reverting `clone_node_from_old` to copy
    /// `old_node.dom_node_id` makes this panic - verified.
    #[test]
    fn switching_ribbon_tabs_keeps_the_layout_tree_addressing_the_live_dom() {
        let (window, _fresh, _x, _y, _damage) = switched_and_fresh();

        // Switch on to a tab that GROWS the tree again, then back: both
        // directions of the size change go through the positional fallback.
        let state = Arc::new(RefCell::new(RefAny::new(RibbonUiState { active_tab: 1, bordered: false })));
        let mut w2 = make_window_sized(&state, ribbon_layout, RIBBON_W, RIBBON_H);
        w2.regenerate_layout().expect("layout");
        for target in [0usize, 2, 1, 0] {
            let Some((x, y)) = tab_header_centre(&w2, target) else {
                panic!("tab {target} has no header");
            };
            click_at(&mut w2, x, y);
            w2.regenerate_layout().expect("relayout after tab switch");
            assert_eq!(active_tab_of(&state), Some(target));
        }

        // The frames exist and are the right size - a tree that survived the
        // assertion must still paint.
        for (name, w) in [("clicked", &window), ("cycled", &w2)] {
            let pm = w
                .cpu_backend
                .last_frame
                .as_ref()
                .unwrap_or_else(|| panic!("{name} window produced no frame"))
                .clone_pixmap();
            assert_eq!((pm.width(), pm.height()), (RIBBON_W as u32, RIBBON_H as u32));
        }
    }

    /// FIXED (was: KNOWN GAP) - a deactivated tab used to keep the text
    /// colour it resolved while it was active.
    ///
    /// After clicking tab 1, tab 0's label painted the accent ACTIVE colour
    /// where a fresh render paints #444444. The cascade was right in both
    /// windows; the stale value was baked into the cached glyph runs
    /// (`StyleProperties::layout_hash` deliberately excludes colour, so a
    /// colour-only change never re-lays the IFC owner). The `<p>` label
    /// wrapper widened the hole from "sometimes" to "always": the IFC owner
    /// became the un-restyled `<p>`, so nothing ever re-laid it. FIX: the
    /// display-list builder re-resolves each run's colour from the CURRENT
    /// cascade via `CompactGlyphRun::source_node_id` at build time
    /// (solver3/display_list.rs, the live_color read before push_text_run);
    /// the baked colour only serves runs without a source node.
    #[test]
    fn clicking_a_ribbon_tab_re_resolves_the_deactivated_tabs_text_colour() {
        let (window, fresh, x, y, _damage) = switched_and_fresh();
        let (a, b) = (dl_texts(&window), dl_texts(&fresh));
        assert_eq!(a.len(), b.len(), "same state must emit the same Text items");
        for (i, (ta, tb)) in a.iter().zip(b.iter()).enumerate() {
            assert_eq!(
                ta, tb,
                "Text[{i}] differs after a tab click at ({x}, {y}); the clicked \
                 window painted a colour it resolved before the switch"
            );
        }
    }

    /// KNOWN GAP - a tab click repaints the whole window.
    ///
    /// `compute_display_list_damage` (cpurender::compositor) returns None -
    /// meaning FULL damage - as soon as the two lists differ in item COUNT,
    /// which every tab switch does. The rects are therefore not wrong, they
    /// are absent; this test pins the behaviour the queued display-list
    /// patching work (NodeId -> item mapping) is meant to deliver.
    #[test]
    #[ignore = "known gap: a structural change falls back to FULL damage"]
    fn ribbon_tab_click_does_not_damage_the_document_below() {
        let state = Arc::new(RefCell::new(RefAny::new(RibbonUiState { active_tab: 0, bordered: false })));
        let mut window = make_window_sized(&state, ribbon_layout, RIBBON_W, RIBBON_H);
        window.regenerate_layout().expect("initial layout");
        window.regenerate_layout().expect("settle");

        let band = ribbon_bottom(&window);
        assert!(
            band > 0.0 && band < RIBBON_H,
            "the ribbon must occupy a band at the top; measured bottom = {band}"
        );

        let Some((x, y)) = tab_header_centre(&window, 1) else {
            panic!("no ribbon tab headers in the layout");
        };
        click_at(&mut window, x, y);
        window.regenerate_layout().expect("post-click relayout");
        let damage = window.cpu_backend.last_frame_damage.clone();
        assert_eq!(active_tab_of(&state), Some(1), "the click missed the tab");

        match &damage {
            FrameDamage::Full => panic!(
                "a ribbon tab click reported FULL damage. Only the tab strip and \
                 the group band changed."
            ),
            FrameDamage::None => panic!(
                "a ribbon tab click reported NO damage, but the visible tab \
                 changed - the new frame would never reach the screen"
            ),
            FrameDamage::Rects(rects) => {
                let intruders: Vec<_> = rects
                    .iter()
                    .filter(|r| r.origin.y + r.size.height > band)
                    .collect();
                assert!(
                    intruders.is_empty(),
                    "a tab click damaged {} rect(s) reaching below the ribbon's \
                     own bottom edge (y = {band}), into the document area that \
                     did not change: {intruders:?}",
                    intruders.len()
                );
            }
        }
    }

    /// Two nodes with the SAME text must not share one shaped entry's
    /// colour or its identity.
    ///
    /// `shape_visual_items_with_per_item_cache` keys on `layout_hash`, which
    /// excludes paint-only properties by design so a recolour can reuse the
    /// shaping. The cached clusters, though, carry a whole
    /// `Arc<StyleProperties>` per glyph AND a `source_node_id`, and the
    /// display list reads its text colour and its `source_node_index` —
    /// which is what damage attributes rects by — straight out of them. Two
    /// labels reading "Label" at the same size therefore collided: the
    /// second took the first's colour and reported the first's node.
    ///
    /// NEGATIVE CONTROL: restoring the plain
    /// `shaped.extend(cached.clusters.iter().cloned())` makes both
    /// assertions fail — verified.
    extern "C" fn twin_label_layout(_data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        use azul_css::dynamic_selector::CssPropertyWithConditions;
        use azul_css::props::basic::color::ColorU;
        use azul_css::props::property::CssProperty;
        use azul_css::props::style::text::StyleTextColor;

        let label = |r: u8, g: u8, b: u8| {
            Dom::create_div()
                .with_css_props(
                    vec![CssPropertyWithConditions::simple(CssProperty::text_color(
                        StyleTextColor { inner: ColorU { r, g, b, a: 255 } },
                    ))]
                    .into(),
                )
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper("Label"))
        };
        Dom::create_body()
            .with_child(label(255, 0, 0))
            .with_child(label(0, 0, 255))
    }

    #[test]
    fn two_nodes_with_the_same_text_keep_their_own_colour_and_identity() {
        let state = Arc::new(RefCell::new(RefAny::new(())));
        let mut window = make_window_sized(&state, twin_label_layout, 400.0, 300.0);
        window.regenerate_layout().expect("layout");

        let texts = dl_texts(&window);
        assert_eq!(texts.len(), 2, "one Text item per label: {texts:?}");

        let red = texts.iter().filter(|t| t.contains("r: 255, g: 0, b: 0")).count();
        let blue = texts.iter().filter(|t| t.contains("r: 0, g: 0, b: 255")).count();
        assert_eq!(
            (red, blue),
            (1, 1),
            "each label keeps its own colour; the shaping cache is keyed on \
             layout_hash, which excludes colour, so a shared entry would paint \
             both in whichever colour shaped first. items = {texts:?}"
        );

        let mut sources: Vec<&str> = texts
            .iter()
            .filter_map(|t| t.split("source_node_index: ").nth(1))
            .collect();
        sources.sort_unstable();
        sources.dedup();
        assert_eq!(
            sources.len(),
            2,
            "the two labels must report DIFFERENT source nodes - damage \
             attributes rects by source_node_index, so a shared one repaints \
             the wrong node. items = {texts:?}"
        );
    }

    /// Border items of the display list, as debug strings.
    fn dl_borders(window: &HeadlessWindow) -> Vec<String> {
        let Some(lw) = window.common.layout_window.as_ref() else {
            return Vec::new();
        };
        let Some(r) = lw.layout_results.get(&azul_core::dom::DomId { inner: 0 }) else {
            return Vec::new();
        };
        r.display_list
            .items
            .iter()
            .filter_map(|i| match i {
                DisplayListItem::Border { .. } => Some(format!("{i:?}")),
                _ => None,
            })
            .collect()
    }

    /// `RibbonTab::style` reaches the rendered tab header.
    ///
    /// `RibbonStyle` describes the tab STRIP, so before this there was no
    /// way to tint one tab: every header shared `tab_style` /
    /// `tab_active_style`. The per-tab hook is what makes a frame dump
    /// legible when a rect is attributed to the wrong tab.
    ///
    /// NEGATIVE CONTROL: dropping the `tab.style` append in
    /// `Ribbon::dom()` leaves all three colours absent — verified.
    #[test]
    fn a_per_tab_style_reaches_that_tabs_header_and_no_other() {
        let state = Arc::new(RefCell::new(RefAny::new(RibbonUiState {
            active_tab: 0,
            bordered: true,
        })));
        let mut window = make_window_sized(&state, ribbon_layout, RIBBON_W, RIBBON_H);
        window.regenerate_layout().expect("layout");

        let borders = dl_borders(&window);
        for (name, needle) in [
            ("HOME red", "#ff0000ff"),
            ("INSERT green", "#00a000ff"),
            ("DESIGN blue", "#0000ffff"),
        ] {
            let n = borders.iter().filter(|b| b.contains(needle)).count();
            assert_eq!(
                n, 1,
                "expected exactly one border carrying the {name} tab colour, \
                 found {n}. Per-tab style is not reaching the header (or is \
                 reaching more than one)."
            );
        }
    }

    /// A tab switch must not move the tabs that did not change.
    ///
    /// Clicking tab 1 used to leave tab 2 — the tab whose state never
    /// changed — drawing its label 1.5px lower than a fresh render of the
    /// same state (baseline 34.6 against 33.1), with an IDENTICAL header
    /// box of 66x26 @ (126, 16). Only the CONTENT moved.
    ///
    /// Root cause: `clone_node_from_old` carried the node's taffy
    /// measurement cache. A clone is taken because the node's own data is
    /// unchanged, but that says nothing about its surroundings — and a
    /// clone is taken precisely when a sibling changed enough to re-lay the
    /// parent out. Tab 2 was answering with a size measured while tab 0 was
    /// still active.
    ///
    /// Found by the per-tab border aid (`RibbonTab::style`), which is why
    /// this runs with `bordered: true`: without borders the stale
    /// measurement happens to agree.
    ///
    /// NEGATIVE CONTROL: restoring the cache on the clone (dropping
    /// `new_node.taffy_cache.clear()`) makes this fail — run and seen.
    #[test]
    fn switching_tabs_does_not_shift_the_other_tabs_text() {
        let state = Arc::new(RefCell::new(RefAny::new(RibbonUiState {
            active_tab: 0,
            bordered: true,
        })));
        let mut window = make_window_sized(&state, ribbon_layout, RIBBON_W, RIBBON_H);
        window.regenerate_layout().expect("initial layout");
        window.regenerate_layout().expect("settle");
        let Some((x, y)) = tab_header_centre(&window, 1) else {
            panic!("no ribbon tab headers");
        };
        click_at(&mut window, x, y);
        window.regenerate_layout().expect("post-click relayout");

        let fresh_state = Arc::new(RefCell::new(RefAny::new(RibbonUiState {
            active_tab: 1,
            bordered: true,
        })));
        let mut fresh = make_window_sized(&fresh_state, ribbon_layout, RIBBON_W, RIBBON_H);
        fresh.regenerate_layout().expect("fresh layout");
        step(&mut fresh, HeadlessEvent::MouseMove { x, y });
        fresh.regenerate_layout().expect("fresh layout under the cursor");

        assert_eq!(
            dl_texts(&window),
            dl_texts(&fresh),
            "a tab switch must leave the untouched tabs where a fresh render \
             puts them"
        );
    }

    fn step(window: &mut HeadlessWindow, event: HeadlessEvent) -> FrameDamage {
        use azul_core::events::{MouseButton, ProcessEventResult};
        use azul_core::window::CursorPosition;
        use crate::desktop::shell2::common::event::PlatformWindow;

        window.snapshot_window_state_baseline("headless.test.step");
        let mut needs_redraw = false;
        match event {
            HeadlessEvent::MouseMove { x, y } => {
                let pos = LogicalPosition { x, y };
                window.common.mouse_state_mut().cursor_position =
                    CursorPosition::InWindow(pos);
                // MWA-C-scroll: active scrollbar thumb drag (desktop pattern).
                if window.common.scrollbar_drag_state.is_some() {
                    needs_redraw = !matches!(
                        PlatformWindow::handle_scrollbar_drag(window, pos),
                        ProcessEventResult::DoNothing
                    );
                    // SANCTIONED SWALLOW: mirrors `run()`'s MouseMove arm — the
                    // thumb drag consumed this motion.
                    PlatformWindow::discard_input_delta(
                        window,
                        "headless.test.step.scrollbar_drag",
                    );
                } else {
                    window.update_hit_test_at(pos);
                    record_headless_input(window, false, false); // MWA-A4
                    needs_redraw = !matches!(
                        window.process_window_events(0),
                        ProcessEventResult::DoNothing
                    );
                }
            }
            HeadlessEvent::MouseDown { button } => {
                // MWA-C-scroll: scrollbar hit first (desktop pattern) —
                // thumb drags / track jumps were untestable in E2E.
                let sb_hit = if matches!(button, MouseButton::Left) {
                    window
                        .common
                        .current_window_state()
                        .mouse_state
                        .cursor_position
                        .get_position()
                        .and_then(|p| {
                            PlatformWindow::perform_scrollbar_hit_test(window, p).map(|h| (h, p))
                        })
                } else {
                    None
                };
                if let Some((hit, p)) = sb_hit {
                    window.common.mouse_state_mut().left_down = true;
                    needs_redraw = !matches!(
                        PlatformWindow::handle_scrollbar_click(window, hit, p),
                        ProcessEventResult::DoNothing
                    );
                    // SANCTIONED SWALLOW: mirrors `run()`'s MouseDown arm — the
                    // scrollbar consumed this press.
                    PlatformWindow::discard_input_delta(
                        window,
                        "headless.test.step.scrollbar_click",
                    );
                } else {
                    match button {
                        MouseButton::Left => window.common.mouse_state_mut().left_down = true,
                        MouseButton::Right => window.common.mouse_state_mut().right_down = true,
                        MouseButton::Middle => window.common.mouse_state_mut().middle_down = true,
                        _ => {}
                    }
                    record_headless_input(window, true, false); // MWA-A4
                    needs_redraw = !matches!(
                        window.process_window_events(0),
                        ProcessEventResult::DoNothing
                    );
                }
            }
            HeadlessEvent::MouseUp { button } => {
                // MWA-C-scroll: a release ends any scrollbar drag.
                let ended_scrollbar_drag = window.common.scrollbar_drag_state.is_some();
                if ended_scrollbar_drag {
                    window.common.scrollbar_drag_state = None;
                }
                match button {
                    MouseButton::Left => window.common.mouse_state_mut().left_down = false,
                    MouseButton::Right => window.common.mouse_state_mut().right_down = false,
                    MouseButton::Middle => window.common.mouse_state_mut().middle_down = false,
                    _ => {}
                }
                record_headless_input(window, false, true); // MWA-A4
                let pass_changed = !matches!(
                    window.process_window_events(0),
                    ProcessEventResult::DoNothing
                );
                needs_redraw = ended_scrollbar_drag || pass_changed;
            }
            HeadlessEvent::KeyDown { virtual_keycode } => {
                window.common.keyboard_state_mut().current_virtual_keycode =
                    azul_core::window::OptionVirtualKeyCode::Some(virtual_keycode);
                window.common.keyboard_state_mut()
                    .pressed_virtual_keycodes.insert_hm_item(virtual_keycode);
                needs_redraw = !matches!(
                    window.process_window_events(0),
                    ProcessEventResult::DoNothing
                );
            }
            HeadlessEvent::KeyUp { virtual_keycode } => {
                window.common.keyboard_state_mut().current_virtual_keycode =
                    azul_core::window::OptionVirtualKeyCode::None;
                window.common.keyboard_state_mut()
                    .pressed_virtual_keycodes.remove_hm_item(&virtual_keycode);
                needs_redraw = !matches!(
                    window.process_window_events(0),
                    ProcessEventResult::DoNothing
                );
            }
            _ => {}
        }
        if needs_redraw {
            let _ = window.regenerate_layout();
            window.cpu_backend.last_frame_damage.clone()
        } else {
            FrameDamage::None
        }
    }

    #[test]
    fn damage_mouse_move_no_change_is_clean() {
        let state = Arc::new(RefCell::new(RefAny::new(GridState {
            boxes: vec![(200.0, 100.0)],
            highlight: None,
        })));
        let mut window = make_window_with(&state, harness_layout_grid);
        window.regenerate_layout().expect("initial layout");

        // Move the mouse over a static colored box (no :hover rule, no callback).
        let d1 = step(&mut window, HeadlessEvent::MouseMove { x: 50.0, y: 50.0 });
        let d2 = step(&mut window, HeadlessEvent::MouseMove { x: 90.0, y: 70.0 });
        println!("[harness] mouse moves: d1={:?} d2={:?}", d1, d2);

        // HONEST: moving the mouse over static content with no hover styling and
        // no callbacks must NOT repaint. Otherwise every pointer move repaints
        // the frame — unusable (esp. with the cursor moving constantly).
        assert_eq!(
            d1, FrameDamage::None,
            "mouse move over static content produced damage {:?}", d1
        );
        assert_eq!(
            d2, FrameDamage::None,
            "second mouse move over static content produced damage {:?}", d2
        );
    }

    #[test]
    fn damage_single_paint_in_large_grid_is_local() {
        let n = 30usize;
        let boxes: Vec<(f32, f32)> = (0..n).map(|_| (100.0, 20.0)).collect();
        let state = Arc::new(RefCell::new(RefAny::new(GridState {
            boxes,
            highlight: None,
        })));
        let mut window = make_window_with(&state, harness_layout_grid);
        window.regenerate_layout().expect("initial layout");

        // Recolor exactly ONE box (index 15) in a 30-box grid.
        set_highlight(&state, Some(15));
        window.regenerate_layout().expect("highlight one box");
        let damage = window.cpu_backend.last_frame_damage.clone();
        println!("[harness] single paint in {}-box grid: damage={:?}", n, damage);

        // HONEST + perf-critical: changing ONE box's colour must damage ~one box
        // (100x20 = 2000 px²), NOT the whole grid (600px tall) or the window.
        // Over-damaging on every small change makes a large UI unusable — this is
        // the core "damage must be incremental at scale" invariant.
        let window_area = 400.0 * 300.0;
        match damage_area(&damage) {
            Some(a) if a > 0.0 => assert!(
                a < window_area * 0.2,
                "single-box recolor in a {}-box grid damaged area {} — should be \
                 ~one box (~2000 px²), not the whole grid/window. Damage is not \
                 incremental at scale. damage={:?}",
                n, a, damage
            ),
            other => panic!(
                "single-box recolor should produce small local damage, got \
                 area={:?} damage={:?}",
                other, damage
            ),
        }
    }

    // --- Scroll: the make-or-break perf case (see DAMAGE_REGION_PLAN.md §0.6) ---

    #[derive(Debug, Clone)]
    struct ScrollTestState {
        n_items: usize,
    }

    /// A 200x100 `overflow:scroll` container holding `n_items` 30px-tall rows
    /// (so n_items > ~3 overflows and makes it scrollable).
    extern "C" fn harness_layout_scroll(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        use azul_css::props::layout::dimensions::{LayoutHeight, LayoutWidth};
        use azul_css::props::layout::overflow::LayoutOverflow;
        use azul_css::props::property::CssProperty;
        use azul_css::dynamic_selector::CssPropertyWithConditions;
        use azul_css::props::basic::color::ColorU;
        use azul_css::props::style::background::{StyleBackgroundContent, StyleBackgroundContentVec};

        let n = data.downcast_ref::<ScrollTestState>().map(|s| s.n_items).unwrap_or(0);
        let mut container = Dom::create_div().with_css_props(
            vec![
                CssPropertyWithConditions::simple(CssProperty::width(LayoutWidth::px(200.0))),
                CssPropertyWithConditions::simple(CssProperty::height(LayoutHeight::px(100.0))),
                CssPropertyWithConditions::simple(CssProperty::overflow_y(LayoutOverflow::Scroll)),
            ]
            .into(),
        );
        for i in 0..n {
            let color = if i % 2 == 0 {
                ColorU { r: 200, g: 60, b: 60, a: 255 }
            } else {
                ColorU { r: 60, g: 60, b: 200, a: 255 }
            };
            let bg: StyleBackgroundContentVec = vec![StyleBackgroundContent::Color(color)].into();
            container = container.with_child(Dom::create_div().with_css_props(
                vec![
                    CssPropertyWithConditions::simple(CssProperty::width(LayoutWidth::px(180.0))),
                    CssPropertyWithConditions::simple(CssProperty::height(LayoutHeight::px(30.0))),
                    CssPropertyWithConditions::simple(CssProperty::background_content(bg)),
                ]
                .into(),
            ));
        }
        Dom::create_body().with_child(container)
    }

    /// A 200x100 `overflow:scroll` container (BOTH axes) holding `n_items` rows
    /// that are WIDER than the viewport (400px) and 30px tall — so the frame is
    /// scrollable diagonally (mobile pan). Rows alternate colour every 30px so a
    /// vertical scroll is visible at a fixed pixel.
    extern "C" fn harness_layout_scroll_2d(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        use azul_css::props::layout::dimensions::{LayoutHeight, LayoutWidth};
        use azul_css::props::layout::overflow::LayoutOverflow;
        use azul_css::props::property::CssProperty;
        use azul_css::dynamic_selector::CssPropertyWithConditions;
        use azul_css::props::basic::color::ColorU;
        use azul_css::props::style::background::{StyleBackgroundContent, StyleBackgroundContentVec};

        let n = data.downcast_ref::<ScrollTestState>().map(|s| s.n_items).unwrap_or(0);
        let mut container = Dom::create_div().with_css_props(
            vec![
                CssPropertyWithConditions::simple(CssProperty::width(LayoutWidth::px(200.0))),
                CssPropertyWithConditions::simple(CssProperty::height(LayoutHeight::px(100.0))),
                CssPropertyWithConditions::simple(CssProperty::overflow_x(LayoutOverflow::Scroll)),
                CssPropertyWithConditions::simple(CssProperty::overflow_y(LayoutOverflow::Scroll)),
            ]
            .into(),
        );
        for i in 0..n {
            let color = if i % 2 == 0 {
                ColorU { r: 200, g: 60, b: 60, a: 255 }
            } else {
                ColorU { r: 60, g: 60, b: 200, a: 255 }
            };
            let bg: StyleBackgroundContentVec = vec![StyleBackgroundContent::Color(color)].into();
            container = container.with_child(Dom::create_div().with_css_props(
                vec![
                    CssPropertyWithConditions::simple(CssProperty::width(LayoutWidth::px(400.0))),
                    CssPropertyWithConditions::simple(CssProperty::height(LayoutHeight::px(30.0))),
                    CssPropertyWithConditions::simple(CssProperty::background_content(bg)),
                ]
                .into(),
            ));
        }
        Dom::create_body().with_child(container)
    }

    /// Grid harness variant with an opaque dark BODY BACKGROUND. The bg rect
    /// spans the whole window, so it intersects every damage rect — exactly
    /// the ingredient that triggered the union-clip overpaint bug (an item
    /// intersecting several disjoint damage rects repainted across their
    /// whole union, erasing the untouched content in between).
    extern "C" fn harness_layout_grid_on_bg(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        use azul_css::props::layout::dimensions::{LayoutHeight, LayoutWidth};
        use azul_css::props::property::CssProperty;
        use azul_css::dynamic_selector::CssPropertyWithConditions;
        use azul_css::props::basic::color::ColorU;
        use azul_css::props::style::background::{StyleBackgroundContent, StyleBackgroundContentVec};

        let (boxes, highlight) = data
            .downcast_ref::<GridState>()
            .map(|s| (s.boxes.clone(), s.highlight))
            .unwrap_or_default();
        let body_bg: StyleBackgroundContentVec =
            vec![StyleBackgroundContent::Color(ColorU { r: 40, g: 40, b: 40, a: 255 })].into();
        let mut body = Dom::create_body().with_css_props(
            vec![CssPropertyWithConditions::simple(CssProperty::background_content(body_bg))]
                .into(),
        );
        for (i, (w, h)) in boxes.iter().enumerate() {
            let color = if Some(i) == highlight {
                ColorU { r: 30, g: 220, b: 30, a: 255 }
            } else if i % 2 == 0 {
                ColorU { r: 220, g: 30, b: 30, a: 255 }
            } else {
                ColorU { r: 30, g: 30, b: 220, a: 255 }
            };
            let bg: StyleBackgroundContentVec = vec![StyleBackgroundContent::Color(color)].into();
            body = body.with_child(Dom::create_div().with_css_props(
                vec![
                    CssPropertyWithConditions::simple(CssProperty::width(LayoutWidth::px(*w))),
                    CssPropertyWithConditions::simple(CssProperty::height(LayoutHeight::px(*h))),
                    CssPropertyWithConditions::simple(CssProperty::background_content(bg)),
                ]
                .into(),
            ));
        }
        body
    }

    /// REGRESSION (union-clip overpaint): two boxes far apart change color in
    /// one frame → two DISJOINT damage rects. The full-window background
    /// intersects BOTH. The old union-clip renderer repainted the background
    /// across the whole union while the unchanged boxes in between were
    /// filtered out → they were ERASED to background color on the first
    /// incremental frame. Per-rect passes must leave them untouched.
    #[test]
    #[cfg(feature = "cpurender")]
    fn damage_disjoint_rects_do_not_erase_content_between() {
        let state = Arc::new(RefCell::new(RefAny::new(GridState {
            boxes: vec![(100.0, 20.0); 5],
            highlight: Some(0),
        })));
        let mut window = make_window_with(&state, harness_layout_grid_on_bg);
        window.regenerate_layout().expect("initial layout");

        // Box centers: body content starts at (8, 8); box i spans y 8+i*20.
        let box2_px = (58u32, 58u32); // center of box 2 (unchanged, red)
        let before = sample_px(&window, box2_px.0, box2_px.1).expect("sample before");
        assert_eq!(
            before,
            [220, 30, 30, 255],
            "box2 should start red (harness sanity)"
        );

        // Flip the highlight from box0 to box4: box0 green→red AND box4
        // blue→green — two changed items at opposite ends, disjoint rects.
        set_highlight(&state, Some(4));
        window.regenerate_layout().expect("incremental relayout");
        let damage = window.cpu_backend.last_frame_damage.clone();
        println!("[harness] disjoint-change damage = {:?}", damage);

        let after = sample_px(&window, box2_px.0, box2_px.1).expect("sample after");
        assert_eq!(
            after,
            [220, 30, 30, 255],
            "box2 (unchanged, BETWEEN the two damage rects) was overwritten — \
             an item intersecting several disjoint damage rects must not \
             repaint across their union (it erases skipped neighbours); \
             damage={:?}",
            damage
        );
        // And the actually-changed boxes must have their new colors.
        let box0 = sample_px(&window, 58, 18).expect("box0");
        let box4 = sample_px(&window, 58, 98).expect("box4");
        assert_eq!(box0, [220, 30, 30, 255], "box0 should now be red");
        assert_eq!(box4, [30, 220, 30, 255], "box4 should now be green");
    }

    /// Scroll harness variant where one row's color is state-driven, so a test
    /// can change content INSIDE an already-scrolled frame.
    #[derive(Debug, Clone)]
    struct ScrollHighlightState {
        n_items: usize,
        highlight: Option<usize>,
    }

    extern "C" fn harness_layout_scroll_highlight(
        mut data: RefAny,
        _info: LayoutCallbackInfo,
    ) -> Dom {
        use azul_css::props::layout::dimensions::{LayoutHeight, LayoutWidth};
        use azul_css::props::layout::overflow::LayoutOverflow;
        use azul_css::props::property::CssProperty;
        use azul_css::dynamic_selector::CssPropertyWithConditions;
        use azul_css::props::basic::color::ColorU;
        use azul_css::props::style::background::{StyleBackgroundContent, StyleBackgroundContentVec};

        let (n, highlight) = data
            .downcast_ref::<ScrollHighlightState>()
            .map(|s| (s.n_items, s.highlight))
            .unwrap_or((0, None));
        let mut container = Dom::create_div().with_css_props(
            vec![
                CssPropertyWithConditions::simple(CssProperty::width(LayoutWidth::px(200.0))),
                CssPropertyWithConditions::simple(CssProperty::height(LayoutHeight::px(100.0))),
                CssPropertyWithConditions::simple(CssProperty::overflow_y(LayoutOverflow::Scroll)),
            ]
            .into(),
        );
        for i in 0..n {
            let color = if Some(i) == highlight {
                ColorU { r: 30, g: 220, b: 30, a: 255 }
            } else if i % 2 == 0 {
                ColorU { r: 200, g: 60, b: 60, a: 255 }
            } else {
                ColorU { r: 60, g: 60, b: 200, a: 255 }
            };
            let bg: StyleBackgroundContentVec = vec![StyleBackgroundContent::Color(color)].into();
            container = container.with_child(Dom::create_div().with_css_props(
                vec![
                    CssPropertyWithConditions::simple(CssProperty::width(LayoutWidth::px(180.0))),
                    CssPropertyWithConditions::simple(CssProperty::height(LayoutHeight::px(30.0))),
                    CssPropertyWithConditions::simple(CssProperty::background_content(bg)),
                ]
                .into(),
            ));
        }
        Dom::create_body().with_child(container)
    }

    fn set_scroll_highlight(state: &Arc<RefCell<RefAny>>, highlight: Option<usize>) {
        let mut g = state.borrow_mut();
        let r: &mut RefAny = &mut g;
        let mut opt = r.downcast_mut::<ScrollHighlightState>();
        if let Some(s) = opt.as_mut() {
            s.highlight = highlight;
        }
    }

    /// Scroll the (single) scroll frame of `window` to vertical offset `dy`.
    #[cfg(feature = "cpurender")]
    fn scroll_frame_to(window: &mut HeadlessWindow, dy: f32) {
        use azul_core::dom::DomId;
        use azul_core::geom::{LogicalPosition, LogicalRect, LogicalSize};
        use azul_core::hit_test::ScrollPosition;

        let node_id = window
            .common
            .layout_window
            .as_ref()
            .and_then(|lw| lw.layout_cache.scroll_id_to_node_id.values().next().copied())
            .expect("no scroll frame registered");
        let sp = ScrollPosition {
            parent_rect: LogicalRect {
                origin: LogicalPosition::new(8.0, 8.0),
                size: LogicalSize::new(200.0, 100.0),
            },
            children_rect: LogicalRect {
                origin: LogicalPosition::new(0.0, dy),
                size: LogicalSize::new(200.0, 600.0),
            },
        };
        window
            .common
            .layout_window
            .as_mut()
            .unwrap()
            .set_scroll_position(DomId { inner: 0 }, node_id, sp);
    }

    /// REGRESSION (content-space damage in scrolled frames): change a row's
    /// color while the frame is scrolled. The damage diff used to emit the
    /// item's CONTENT-space bounds, so the repaint landed a scroll-offset too
    /// low and the changed row stayed visually stale on screen.
    #[test]
    #[cfg(feature = "cpurender")]
    fn damage_change_inside_scrolled_frame_repaints_at_viewport_position() {
        let state = Arc::new(RefCell::new(RefAny::new(ScrollHighlightState {
            n_items: 20,
            highlight: None,
        })));
        let mut window = make_window_with(&state, harness_layout_scroll_highlight);
        window.regenerate_layout().expect("initial layout");

        // Scroll down 30px and render (row 1's content span y 30..60 is now
        // on screen at viewport y 8..38; parent content starts at y=8).
        scroll_frame_to(&mut window, 30.0);
        window.regenerate_layout().expect("scroll relayout");
        println!(
            "[harness] post-scroll damage = {:?} px(50,20)={:?} px(50,75)={:?}",
            window.cpu_backend.last_frame_damage,
            sample_px(&window, 50, 20),
            sample_px(&window, 50, 75),
        );

        let probe = (50u32, 20u32); // inside row 1's on-screen span (content y=42)
        let before = sample_px(&window, probe.0, probe.1).expect("sample before");
        assert_eq!(before, [60, 60, 200, 255], "row1 starts blue (sanity)");

        // Change row 1 to green while scrolled.
        set_scroll_highlight(&state, Some(1));
        window.regenerate_layout().expect("highlight relayout");
        let damage = window.cpu_backend.last_frame_damage.clone();
        println!("[harness] scrolled-change damage = {:?}", damage);

        let after = sample_px(&window, probe.0, probe.1).expect("sample after");
        assert_eq!(
            after,
            [30, 220, 30, 255],
            "row 1 changed color while the frame was scrolled but its ON-SCREEN \
             pixels did not update — the damage diff must project item bounds \
             through the scroll offset (content-space damage repaints the wrong \
             band); damage={:?}",
            damage
        );
    }

    /// REGRESSION (swallowed sub-pixel scrolling): high-resolution trackpads
    /// deliver deltas well under a device pixel per frame. The scroll baseline
    /// used to advance every frame even when the delta was dropped as
    /// sub-threshold, so the deficit never accumulated — slow scrolling froze
    /// the content entirely. The baseline must stay at the last PAINTED offset
    /// so tiny deltas accumulate until they cross a device pixel.
    #[test]
    #[cfg(feature = "cpurender")]
    fn damage_subpixel_scroll_accumulates() {
        let state = Arc::new(RefCell::new(RefAny::new(ScrollTestState { n_items: 20 })));
        let mut window = make_window_with(&state, harness_layout_scroll);
        window.regenerate_layout().expect("initial layout");

        // Three 0.2px scroll steps. Each individual delta is sub-threshold;
        // cumulatively they cross half a device pixel at 0.6.
        scroll_frame_to(&mut window, 0.2);
        window.regenerate_layout().expect("step 1");
        let d1 = window.cpu_backend.last_frame_damage.clone();
        scroll_frame_to(&mut window, 0.4);
        window.regenerate_layout().expect("step 2");
        let d2 = window.cpu_backend.last_frame_damage.clone();
        scroll_frame_to(&mut window, 0.6);
        window.regenerate_layout().expect("step 3");
        let d3 = window.cpu_backend.last_frame_damage.clone();

        println!("[harness] subpixel damage steps = {:?} / {:?} / {:?}", d1, d2, d3);
        // A sub-device-pixel scroll must repaint NOTHING — not the content
        // (the frame builder's half-device-pixel threshold drops the shift)
        // and not the scrollbar either (`quantize_thumb_offset` rounds the
        // thumb's GPU value, so a ~0.03 px thumb move is not a value change).
        //
        // This used to be asserted on "the content area, x < 200" — a filter
        // that silently meant "everything but the scrollbar" on macOS ONLY,
        // where the overlay bar hangs off the container's right edge at
        // x=200..208. Windows and Linux reserve a 12 px gutter INSIDE the
        // container, putting the bar at x=196..208, so the filter classified
        // the bar as content and the law read as a content repaint that never
        // happened. Neither platform needs an exemption now, so the honest
        // assertion is the strong one: no damage at all.
        let damaged = |d: &FrameDamage| -> bool { *d != FrameDamage::None };
        assert!(
            !damaged(&d1),
            "0.2px scroll must not repaint ANYTHING: the content shift is \
             below the half-device-pixel threshold and the thumb moves ~0.03px, \
             which quantises to no move at all; got {:?}",
            d1
        );
        assert!(
            !damaged(&d2),
            "0.4px cumulative must not repaint anything yet; got {:?}",
            d2
        );
        assert!(
            damaged(&d3),
            "0.6px CUMULATIVE scroll crossed half a device pixel and must \
             repaint content — if the damage is empty the baseline advanced on \
             skipped frames and slow trackpad scrolling is swallowed forever; \
             got {:?}",
            d3
        );
    }

    /// REGRESSION (idle skip with scrollbars): a no-op relayout of a window
    /// WITH a scrollbar must reach `FrameDamage::None`. ScrollBarStyled used
    /// to fall into `is_visually_equal`'s `_ => false` catch-all, so every
    /// scrollbar'd window re-damaged its bar every frame — the skip path was
    /// unreachable and idle windows re-rendered + re-presented forever (the
    /// thumb position now flows through the GPU-value damage channel instead).
    /// Scrolling must MOVE the pixels it already has, not repaint them.
    ///
    /// `render_frame` memmoves the still-visible part of a scrolled frame
    /// inside the retained pixmap (`scroll_shift_region`) and repaints only
    /// the strip that scrolled into view — but ONLY when
    /// `scroll_fast_path_eligible` says the content opaquely covers the clip
    /// at both the old and new offsets. When it says no, the else-branch
    /// pushes the WHOLE clip as damage and every scrolled pixel is
    /// re-rasterized.
    ///
    /// Nothing pinned which of those two happens, so the fast path could
    /// silently stop firing — turning every pan into a full repaint of the
    /// scrolled area — without a single test noticing. This asserts the
    /// painted area stays far below the clip area.
    #[test]
    fn damage_scroll_takes_the_memmove_fast_path() {
        use azul_core::geom::{LogicalPosition, LogicalRect, LogicalSize};

        let state = Arc::new(RefCell::new(RefAny::new(ScrollTestState { n_items: 20 })));
        let mut window = make_window_with(&state, harness_layout_scroll);
        window.regenerate_layout().expect("initial layout");
        // Second render so the next one is incremental (the first frame is a
        // full repaint by definition, and the fast path only runs then).
        window.regenerate_layout().expect("settle");

        // The scroll clip from `harness_layout_scroll` / `scroll_frame_to`.
        let clip = LogicalRect {
            origin: LogicalPosition::new(8.0, 8.0),
            size: LogicalSize::new(200.0, 100.0),
        };
        let clip_area = clip.size.width * clip.size.height;

        scroll_frame_to(&mut window, 30.0);
        window.regenerate_layout().expect("scroll relayout");
        let damage = window.cpu_backend.last_frame_damage.clone();
        println!("[harness] scroll-30 PAINT damage = {damage:?}");

        // Area actually re-rasterized inside the scroll clip.
        let painted: f32 = match &damage {
            FrameDamage::Full => clip_area,
            FrameDamage::None => 0.0,
            FrameDamage::Rects(rs) => rs
                .iter()
                .filter_map(|r| {
                    let x0 = r.origin.x.max(clip.origin.x);
                    let y0 = r.origin.y.max(clip.origin.y);
                    let x1 = (r.origin.x + r.size.width).min(clip.origin.x + clip.size.width);
                    let y1 = (r.origin.y + r.size.height).min(clip.origin.y + clip.size.height);
                    if x1 > x0 && y1 > y0 { Some((x1 - x0) * (y1 - y0)) } else { None }
                })
                .sum(),
        };

        // A 30px scroll of a 100px-tall clip exposes a 30px strip, so ~30% of
        // the clip plus the scrollbar column. The threshold is deliberately
        // loose — the point is to catch "the whole clip got repainted"
        // (100%), which is what an ineligible fast path produces.
        let ratio = painted / clip_area;
        println!("[harness] painted {painted:.0}px2 of clip {clip_area:.0}px2 = {ratio:.2}");
        assert!(
            ratio < 0.6,
            "scrolling 30px of a 100px clip repainted {:.0}% of it. The \
             memmove fast path did not fire, so every pan re-rasterizes the \
             whole scrolled area instead of shifting the pixels it already \
             has. damage = {damage:?}",
            ratio * 100.0
        );
        assert!(
            painted > 0.0,
            "scrolling repainted NOTHING — the newly exposed strip must still \
             be painted, or scrolled-in content is stale pixels"
        );
    }

    #[test]
    #[cfg(feature = "cpurender")]
    fn damage_idle_scrollbar_window_skips() {
        let state = Arc::new(RefCell::new(RefAny::new(ScrollTestState { n_items: 20 })));
        let mut window = make_window_with(&state, harness_layout_scroll);
        window.regenerate_layout().expect("initial layout");
        // Second render, nothing changed at all.
        window.regenerate_layout().expect("no-op relayout");
        let damage = window.cpu_backend.last_frame_damage.clone();
        assert_eq!(
            damage,
            FrameDamage::None,
            "an idle window with a scrollbar must skip (FrameDamage::None);              non-None means the scrollbar (or another item) produces false              per-frame damage and idle windows burn CPU forever"
        );
        // And scrolling must still damage the bar (thumb moved → GPU value
        // diff) — the equality arm must not have frozen the thumb.
        scroll_frame_to(&mut window, 30.0);
        window.regenerate_layout().expect("scroll relayout");
        let damage = window.cpu_backend.last_frame_damage.clone();
        // The bar's own rect, read off the display list rather than
        // re-derived from per-OS constants: the platform decides the
        // scrollbar width (8px overlay where the UA sheet says `thin`, 12px
        // where it says `auto`) AND whether it reserves a gutter, so the only
        // honest source for "where is the bar" is where layout put it. A
        // hard-coded x also made this law vacuous on overlay platforms, where
        // the bar sits inside the scroll clip and the exposed-strip damage
        // covers those columns whether or not the thumb was damaged at all.
        use azul_core::dom::ScrollbarOrientation;
        let bar = {
            let lw = window.common.layout_window.as_ref().expect("layout window");
            let r = lw
                .layout_results
                .get(&azul_core::dom::DomId::ROOT_ID)
                .expect("root layout result");
            r.display_list
                .items
                .iter()
                .find_map(|it| match it {
                    DisplayListItem::ScrollBarStyled { info }
                        if info.orientation == ScrollbarOrientation::Vertical =>
                    {
                        Some(info.bounds.0)
                    }
                    _ => None,
                })
                .expect("the scrollable container must paint a vertical scrollbar")
        };
        match &damage {
            FrameDamage::Rects(rs) => {
                // The thumb moved (GPU value cache), so the bar's rect must be
                // covered by the damage — by a single rect, since that is how
                // the value diff raises it.
                let covers = |r: &azul_core::geom::LogicalRect| {
                    r.origin.x <= bar.origin.x + 0.5
                        && r.origin.y <= bar.origin.y + 0.5
                        && r.origin.x + r.size.width >= bar.origin.x + bar.size.width - 0.5
                        && r.origin.y + r.size.height >= bar.origin.y + bar.size.height - 0.5
                };
                assert!(
                    rs.iter().any(covers),
                    "scroll must damage the scrollbar {bar:?} (the thumb moved via \
                     the GPU value cache, and the display-list items compare equal, \
                     so nothing else can raise it); got {rs:?}"
                );
            }
            other => panic!("scroll should be incremental, got {:?}", other),
        }
    }

    /// Sample the RGBA of the last rendered frame at physical pixel (x, y).
    #[cfg(feature = "cpurender")]
    fn sample_px(window: &HeadlessWindow, x: u32, y: u32) -> Option<[u8; 4]> {
        let pm = window.cpu_backend.last_frame.as_ref()?;
        let (w, h) = (pm.width(), pm.height());
        if x >= w || y >= h {
            return None;
        }
        let d = pm.data();
        let i = ((y * w + x) * 4) as usize;
        if i + 4 > d.len() {
            return None;
        }
        Some([d[i], d[i + 1], d[i + 2], d[i + 3]])
    }

    /// Write the window's last frame to `/tmp/<name>.png` for visual inspection.
    /// Best-effort: silently does nothing if there's no frame / encode fails.
    #[cfg(feature = "cpurender")]
    fn save_frame_png(window: &HeadlessWindow, name: &str) {
        if let Some(pm) = window.cpu_backend.last_frame.as_ref() {
            if let Ok(bytes) = pm.encode_png() {
                let _ = std::fs::write(format!("/tmp/{}.png", name), bytes);
            }
        }
    }

    /// Count pixels (and the max per-channel delta) that differ between two
    /// pixmaps. (usize::MAX, 255) if the dimensions differ.
    #[cfg(feature = "cpurender")]
    fn pixmap_diff(
        pa: &azul_layout::cpurender::AzulPixmap,
        pb: &azul_layout::cpurender::AzulPixmap,
    ) -> (usize, u8) {
        if pa.width() != pb.width() || pa.height() != pb.height() {
            return (usize::MAX, 255);
        }
        let (da, db) = (pa.data(), pb.data());
        let mut diff_px = 0usize;
        let mut max_d = 0u8;
        for (ca, cb) in da.chunks_exact(4).zip(db.chunks_exact(4)) {
            let d = (0..4)
                .map(|k| (ca[k] as i16 - cb[k] as i16).unsigned_abs() as u8)
                .max()
                .unwrap_or(0);
            if d > 0 {
                diff_px += 1;
                max_d = max_d.max(d);
            }
        }
        (diff_px, max_d)
    }

    /// Render the window's CURRENT state as a full, offset-aware frame using the
    /// offset-applying rasteriser (`render_display_list_damaged` over the whole
    /// viewport) — the trustworthy "what it should look like" reference, independent
    /// of the incremental and compositor paths.
    #[cfg(feature = "cpurender")]
    fn offset_aware_reference(w: &mut HeadlessWindow) -> azul_layout::cpurender::AzulPixmap {
        use azul_core::dom::DomId;
        use azul_core::geom::{LogicalPosition, LogicalRect, LogicalSize};
        let (pw, ph) = w
            .cpu_backend
            .last_frame
            .as_ref()
            .map(|p| (p.width(), p.height()))
            .unwrap_or((1, 1));
        let dpi = {
            let ws = w.common.current_window_state();
            ws.size.dpi as f32 / 96.0
        };
        let mut reference = azul_layout::cpurender::AzulPixmap::new(pw, ph).expect("ref pixmap");
        reference.fill(255, 255, 255, 255);
        let lw = w.common.layout_window.as_ref().unwrap();
        let dom = DomId { inner: 0 };
        let result = lw.layout_results.get(&dom).unwrap();
        let offsets = lw
            .scroll_manager
            .build_scroll_offset_map(dom, &result.scroll_id_to_node_id);
        // The GPU value cache is part of the frame's state, not decoration:
        // scrollbar thumb positions and fade opacities live ONLY there (the
        // display list carries the keys). A reference built without them
        // paints every thumb at its display-list-baked initial position, so
        // it would call a correctly-moved thumb a diff.
        let (gpu_transforms, gpu_opacities) = azul_layout::cpurender::extract_gpu_values(
            lw.gpu_state_manager.get_cache(dom),
            dom,
        );
        let mut rs = azul_layout::cpurender::CpuRenderState::new(offsets)
            .with_system_style(lw.system_style.clone());
        rs.transforms = gpu_transforms;
        rs.opacities = gpu_opacities;
        let full_clip = LogicalRect {
            origin: LogicalPosition::new(0.0, 0.0),
            size: LogicalSize::new(pw as f32 / dpi, ph as f32 / dpi),
        };
        let _ = azul_layout::cpurender::render_display_list_damaged(
            &result.display_list,
            &mut reference,
            dpi,
            &w.common.renderer_resources,
            &lw.font_manager,
            &mut w.cpu_backend.glyph_cache,
            &rs,
            &[full_clip],
        );
        reference
    }

    /// Render a scrolled state TWO ways — the incremental fast path (memmove +
    /// strip) and a forced full re-render at the same offset — and assert they're
    /// pixel-identical. This is the rigorous proof that the scroll-shift fast path
    /// is correct. Saves `/tmp/<tag>_fast.png` and `/tmp/<tag>_full.png` for the
    /// human to eyeball, and returns the fast-path damage.
    #[cfg(feature = "cpurender")]
    fn assert_fast_matches_full_scroll(
        cb: azul_core::callbacks::LayoutCallbackType,
        dx: f32,
        dy: f32,
        tag: &str,
    ) -> FrameDamage {
        use azul_core::dom::DomId;
        use azul_core::geom::{LogicalPosition, LogicalRect, LogicalSize};
        use azul_core::hit_test::ScrollPosition;

        let state = Arc::new(RefCell::new(RefAny::new(ScrollTestState { n_items: 100 })));
        let mut w = make_window_with(&state, cb);
        w.regenerate_layout().expect("initial layout");
        let node = w
            .common
            .layout_window
            .as_ref()
            .and_then(|lw| lw.layout_cache.scroll_id_to_node_id.values().next().copied())
            .expect("scroll frame should exist");
        let sp = ScrollPosition {
            parent_rect: LogicalRect {
                origin: LogicalPosition::new(8.0, 8.0),
                size: LogicalSize::new(200.0, 100.0),
            },
            children_rect: LogicalRect {
                origin: LogicalPosition::new(dx, dy),
                size: LogicalSize::new(400.0, 3000.0),
            },
        };
        w.common
            .layout_window
            .as_mut()
            .unwrap()
            .set_scroll_position(DomId { inner: 0 }, node, sp);
        // Incremental fast path.
        w.regenerate_layout().expect("scroll (fast)");
        let damage = w.cpu_backend.last_frame_damage.clone();
        save_frame_png(&w, &format!("{}_fast", tag));
        let fast = w
            .cpu_backend
            .last_frame
            .as_ref()
            .map(|p| p.clone_pixmap())
            .expect("fast frame");

        // Correct reference: a FULL offset-aware render of the whole viewport via
        // the offset-applying rasteriser.
        let full = offset_aware_reference(&mut w);
        if let Ok(bytes) = full.encode_png() {
            let _ = std::fs::write(format!("/tmp/{}_full.png", tag), bytes);
        }

        let (diff_px, max_d) = pixmap_diff(&fast, &full);
        println!(
            "[harness] {tag}: fast-vs-full diff_px={diff_px} max_delta={max_d} (PNGs in /tmp/{tag}_*.png)"
        );
        assert_eq!(
            diff_px, 0,
            "{tag}: fast-path scroll is NOT pixel-identical to a full re-render \
             ({diff_px} px differ, max channel delta {max_d}). The memmove produced \
             a wrong frame — see /tmp/{tag}_fast.png vs /tmp/{tag}_full.png",
        );
        damage
    }

    #[test]
    fn scroll_moves_content_not_just_scrollbar() {
        use azul_core::dom::DomId;
        use azul_core::geom::{LogicalPosition, LogicalRect, LogicalSize};
        use azul_core::hit_test::ScrollPosition;

        let state = Arc::new(RefCell::new(RefAny::new(ScrollTestState { n_items: 20 })));
        let mut window = make_window_with(&state, harness_layout_scroll);
        window.regenerate_layout().expect("initial layout");
        #[cfg(feature = "cpurender")]
        let before_px = sample_px(&window, 50, 20);

        // Find the scroll frame's node (overflow:scroll should register one).
        let (n_scroll_nodes, scroll_node) = window
            .common
            .layout_window
            .as_ref()
            .map(|lw| {
                (
                    lw.layout_cache.scroll_id_to_node_id.len(),
                    lw.layout_cache.scroll_id_to_node_id.values().next().copied(),
                )
            })
            .unwrap_or((0, None));
        println!("[harness] scroll frames registered = {}", n_scroll_nodes);
        let node_id = match scroll_node {
            Some(n) => n,
            None => panic!(
                "overflow:scroll created NO scroll frame (scroll_id_to_node_id empty) \
                 — content {}px in a 100px container should be scrollable",
                20 * 30
            ),
        };

        // Scroll down by 30px (one row).
        let sp = ScrollPosition {
            parent_rect: LogicalRect {
                origin: LogicalPosition::new(8.0, 8.0),
                size: LogicalSize::new(200.0, 100.0),
            },
            children_rect: LogicalRect {
                origin: LogicalPosition::new(0.0, 30.0),
                size: LogicalSize::new(200.0, 600.0),
            },
        };
        window
            .common
            .layout_window
            .as_mut()
            .unwrap()
            .set_scroll_position(DomId { inner: 0 }, node_id, sp);
        window.regenerate_layout().expect("scroll relayout");
        let damage = window.cpu_backend.last_frame_damage.clone();
        println!("[harness] scroll damage = {:?}", damage);

        // HONEST: scrolling must move the CONTENT, not just the scrollbar. The
        // rows alternate colour every 30px, so a 30px scroll swaps the colour at
        // a fixed viewport pixel. If it's unchanged, the content is FROZEN on
        // scroll — only the scrollbar moved (damage was scrollbar-only). The
        // scroll_layer shift is dead code and content items don't shift in the DL
        // (§0.6). A weak "damage != None / bounded" assertion would FAKE-PASS on
        // the scrollbar alone — so we check the rendered pixels directly.
        #[cfg(feature = "cpurender")]
        {
            let after_px = sample_px(&window, 50, 20);
            println!(
                "[harness] content px @ (50,20): before={:?} after={:?}",
                before_px, after_px
            );
            assert!(
                before_px.is_some() && after_px.is_some(),
                "no rendered pixmap to sample (before={:?} after={:?})",
                before_px, after_px
            );
            assert_ne!(
                before_px, after_px,
                "scroll did NOT change the content at (50,20) — content is FROZEN on \
                 scroll; only the scrollbar moved (damage={:?}). scroll_layer is dead \
                 code (§0.6) and content items don't shift in the display list.",
                damage
            );
        }
    }

    #[test]
    #[cfg(feature = "cpurender")]
    fn scroll_present_damage_larger_than_paint_damage() {
        // The render-vs-present split: scrolling PAINTS a thin strip but PRESENTS
        // the whole clip (the pixels moved on screen). Paint damage must stay a
        // strip; present damage must cover the full clip.
        use azul_core::dom::DomId;
        use azul_core::geom::{LogicalPosition, LogicalRect, LogicalSize};
        use azul_core::hit_test::ScrollPosition;

        let state = Arc::new(RefCell::new(RefAny::new(ScrollTestState { n_items: 100 })));
        let mut window = make_window_with(&state, harness_layout_scroll);
        window.regenerate_layout().expect("initial layout");
        let node_id = window
            .common
            .layout_window
            .as_ref()
            .and_then(|lw| lw.layout_cache.scroll_id_to_node_id.values().next().copied())
            .expect("scroll frame should exist");
        let sp = ScrollPosition {
            parent_rect: LogicalRect {
                origin: LogicalPosition::new(8.0, 8.0),
                size: LogicalSize::new(200.0, 100.0),
            },
            children_rect: LogicalRect {
                origin: LogicalPosition::new(0.0, 30.0),
                size: LogicalSize::new(400.0, 3000.0),
            },
        };
        window
            .common
            .layout_window
            .as_mut()
            .unwrap()
            .set_scroll_position(DomId { inner: 0 }, node_id, sp);
        window.regenerate_layout().expect("scroll");

        let paint = damage_area(&window.cpu_backend.last_frame_damage);
        let present = damage_area(&window.cpu_backend.last_present_damage);
        println!("[harness] paint={:?} present={:?}", paint, present);
        let (paint, present) = (paint.expect("paint finite"), present.expect("present finite"));
        // Paint stays a strip; present covers the ~188x100 clip; present > paint.
        assert!(paint <= 10_000.0, "paint damage should be a strip, got {paint}px");
        assert!(
            present >= 18_000.0,
            "present damage should cover the full ~188x100 clip, got {present}px"
        );
        assert!(present > paint, "present ({present}) must exceed paint ({paint})");
    }

    #[test]
    fn scroll_repaint_pixels_is_strip() {
        use azul_core::dom::DomId;
        use azul_core::geom::{LogicalPosition, LogicalRect, LogicalSize};
        use azul_core::hit_test::ScrollPosition;

        let state = Arc::new(RefCell::new(RefAny::new(ScrollTestState { n_items: 100 })));
        let mut window = make_window_with(&state, harness_layout_scroll);
        window.regenerate_layout().expect("initial layout");
        let node_id = window
            .common
            .layout_window
            .as_ref()
            .and_then(|lw| lw.layout_cache.scroll_id_to_node_id.values().next().copied())
            .expect("scroll frame should exist");

        // Scroll down 30px (one row) in the 200x100 viewport.
        let sp = ScrollPosition {
            parent_rect: LogicalRect {
                origin: LogicalPosition::new(8.0, 8.0),
                size: LogicalSize::new(200.0, 100.0),
            },
            children_rect: LogicalRect {
                origin: LogicalPosition::new(0.0, 30.0),
                size: LogicalSize::new(200.0, 3000.0),
            },
        };
        window
            .common
            .layout_window
            .as_mut()
            .unwrap()
            .set_scroll_position(DomId { inner: 0 }, node_id, sp);
        window.regenerate_layout().expect("scroll relayout");
        let damage = window.cpu_backend.last_frame_damage.clone();
        let pixels = damage_area(&damage);
        println!(
            "[harness] scroll repaint pixels = {:?} damage = {:?}",
            pixels, damage
        );

        // HONEST perf metric — count pixels REPAINTED, not m×n (the whole
        // viewport). Scrolling a 200x100 viewport by 30px should repaint ~a 30px
        // content strip + the scrollbar (~30*188 + 12*100 ≈ 6.8k px), NOT the full
        // viewport (~188*100 + 12*100 ≈ 20k px). Wall-time is noisy and dominated
        // by relayout (which real scroll skips); the repainted-pixel count is the
        // deterministic signal. Currently a full-viewport re-render (scroll_layer
        // pixel-shift unwired) → FAILS here until #14 cuts the paint to a strip.
        match pixels {
            Some(px) => assert!(
                px <= 10_000.0,
                "scroll repainted {} px — should be a ~30px strip + scrollbar (~6.8k \
                 px), not the full viewport (~20k px = m×n). Wire scroll_layer \
                 pixel-shift (#14). damage={:?}",
                px, damage
            ),
            None => panic!(
                "scroll produced Full damage — worse than full-viewport. damage={:?}",
                damage
            ),
        }
    }

    #[test]
    fn scroll_diagonal_pan_two_strips() {
        // #16 mobile pan: a DIAGONAL scroll (both axes in one frame) must repaint
        // an L-shape (a bottom strip + a right strip), not the whole viewport and
        // not fall back to a full-clip repaint. Exercises the single-pass 2-D
        // shift end-to-end through render_frame.
        use azul_core::dom::DomId;
        use azul_core::geom::{LogicalPosition, LogicalRect, LogicalSize};
        use azul_core::hit_test::ScrollPosition;

        let state = Arc::new(RefCell::new(RefAny::new(ScrollTestState { n_items: 100 })));
        let mut window = make_window_with(&state, harness_layout_scroll_2d);
        window.regenerate_layout().expect("initial layout");
        #[cfg(feature = "cpurender")]
        let before_px = sample_px(&window, 50, 20);
        let node_id = window
            .common
            .layout_window
            .as_ref()
            .and_then(|lw| lw.layout_cache.scroll_id_to_node_id.values().next().copied())
            .expect("2-axis scroll frame should exist");

        // Pan down-right: 20px right + 30px down (one row).
        let sp = ScrollPosition {
            parent_rect: LogicalRect {
                origin: LogicalPosition::new(8.0, 8.0),
                size: LogicalSize::new(200.0, 100.0),
            },
            children_rect: LogicalRect {
                origin: LogicalPosition::new(20.0, 30.0),
                size: LogicalSize::new(400.0, 3000.0),
            },
        };
        window
            .common
            .layout_window
            .as_mut()
            .unwrap()
            .set_scroll_position(DomId { inner: 0 }, node_id, sp);
        window.regenerate_layout().expect("scroll relayout");
        let damage = window.cpu_backend.last_frame_damage.clone();
        let pixels = damage_area(&damage);
        println!("[harness] diagonal pan pixels = {:?} damage = {:?}", pixels, damage);

        // Perf: the L-shape (two thin strips + scrollbars) must stay well under a
        // full re-render. A diagonal that fell back to a full-clip repaint (the
        // pre-#16 behaviour) would land near the ~17-20k full viewport.
        match pixels {
            Some(px) => assert!(
                px > 0.0 && px <= 12_000.0,
                "diagonal pan repainted {} px — expected a thin L-shape (two strips \
                 + scrollbars), not a full-clip repaint. damage={:?}",
                px, damage
            ),
            None => panic!("diagonal pan produced Full damage. damage={:?}", damage),
        }

        // The damage must contain at least TWO content strips (one per axis) — a
        // single strip would mean only one axis actually scrolled.
        if let FrameDamage::Rects(rs) = &damage {
            let content_strips = rs
                .iter()
                .filter(|r| r.size.width > 20.0 && r.size.height > 20.0)
                .count();
            assert!(
                content_strips >= 2,
                "diagonal pan must expose TWO content strips (bottom + right), got \
                 {} sizeable rects in {:?}",
                content_strips, damage
            );
        }

        // Correctness: content actually moved (the row colour at a fixed pixel
        // flips on the 30px vertical component of the pan).
        #[cfg(feature = "cpurender")]
        {
            let after_px = sample_px(&window, 50, 20);
            assert!(before_px.is_some() && after_px.is_some());
            assert_ne!(
                before_px, after_px,
                "diagonal pan did not move content at (50,20) — before={:?} after={:?}",
                before_px, after_px
            );
        }
    }

    // #21: PNG visual tests. The fast path (memmove + strip repaint) must produce
    // a frame byte-identical to a full re-render at the same offset. These render
    // both ways, assert pixel-equality, and drop PNGs in /tmp for eyeballing.
    #[test]
    #[cfg(feature = "cpurender")]
    fn png_scroll_vertical_fast_matches_full_render() {
        let damage = assert_fast_matches_full_scroll(harness_layout_scroll, 0.0, 30.0, "scroll_vert");
        // It really took the fast path (a strip), not a full clip repaint.
        match damage_area(&damage) {
            Some(px) => assert!(
                px <= 10_000.0,
                "vertical scroll should be a thin strip via the fast path, got {px}px {:?}",
                damage
            ),
            None => panic!("vertical scroll produced Full damage: {:?}", damage),
        }
    }

    #[test]
    #[cfg(feature = "cpurender")]
    fn png_scroll_diagonal_fast_matches_full_render() {
        let damage =
            assert_fast_matches_full_scroll(harness_layout_scroll_2d, 20.0, 30.0, "scroll_diag");
        match damage_area(&damage) {
            Some(px) => assert!(
                px <= 12_000.0,
                "diagonal pan should be two strips via the fast path, got {px}px {:?}",
                damage
            ),
            None => panic!("diagonal pan produced Full damage: {:?}", damage),
        }
    }

    #[test]
    #[cfg(feature = "cpurender")]
    fn png_scroll_compositor_full_render_applies_offset() {
        // #18 fix: the COMPOSITOR full-render path (render_layers) must apply scroll
        // offsets. It used to render with an empty offset map → a full repaint while
        // scrolled drew content at offset 0. Force the compositor full path at a
        // 30px scroll and assert it matches the offset-aware reference.
        use azul_core::dom::DomId;
        use azul_core::geom::{LogicalPosition, LogicalRect, LogicalSize};
        use azul_core::hit_test::ScrollPosition;

        let state = Arc::new(RefCell::new(RefAny::new(ScrollTestState { n_items: 100 })));
        let mut w = make_window_with(&state, harness_layout_scroll);
        w.regenerate_layout().expect("initial");
        let node = w
            .common
            .layout_window
            .as_ref()
            .and_then(|lw| lw.layout_cache.scroll_id_to_node_id.values().next().copied())
            .expect("scroll frame");
        let sp = ScrollPosition {
            parent_rect: LogicalRect {
                origin: LogicalPosition::new(8.0, 8.0),
                size: LogicalSize::new(200.0, 100.0),
            },
            children_rect: LogicalRect {
                origin: LogicalPosition::new(0.0, 30.0),
                size: LogicalSize::new(400.0, 3000.0),
            },
        };
        w.common
            .layout_window
            .as_mut()
            .unwrap()
            .set_scroll_position(DomId { inner: 0 }, node, sp);
        w.regenerate_layout().expect("scroll (incremental)");
        // Force the FULL (compositor) path for the next frame at the same offset.
        w.cpu_backend.previous_display_list = None;
        w.regenerate_layout().expect("compositor full");
        save_frame_png(&w, "scroll_compositor_full");
        let comp = w
            .cpu_backend
            .last_frame
            .as_ref()
            .map(|p| p.clone_pixmap())
            .expect("compositor frame");
        let reference = offset_aware_reference(&mut w);
        let (diff_px, max_d) = pixmap_diff(&comp, &reference);
        println!(
            "[harness] compositor-full vs offset-aware reference: diff_px={diff_px} max={max_d}"
        );
        // Allow a tiny tolerance for AA/compositing path differences; the pre-fix
        // bug was a whole-viewport mismatch (~18k px, full row phase wrong).
        assert!(
            diff_px < 200,
            "compositor full-render does not apply the scroll offset (diff {diff_px}px, \
             max delta {max_d}) — see /tmp/scroll_compositor_full.png",
        );
    }

    #[test]
    fn test_stub_window_close() {
        let mut window = make_stub();
        window.close();
        assert!(!window.is_open());
    }

    #[test]
    fn test_stub_event_injection() {
        let mut window = make_stub();

        assert!(window.poll_event().is_none());

        window.inject_event(HeadlessEvent::MouseMove { x: 100.0, y: 200.0 });
        window.inject_event(HeadlessEvent::Close);

        assert!(matches!(window.poll_event(), Some(HeadlessEvent::MouseMove { .. })));
        assert!(matches!(window.poll_event(), Some(HeadlessEvent::Close)));
        assert!(window.poll_event().is_none());
    }

    #[test]
    fn test_stub_timer_management() {
        let mut window = make_stub();
        assert!(!window.has_active_timers());

        let get_time = azul_core::task::GetSystemTimeCallback {
            cb: azul_core::task::get_system_time_libstd,
        };
        let timer = azul_layout::timer::Timer::create(
            RefAny::new(()),
            test_timer_callback as azul_layout::timer::TimerCallbackType,
            get_time,
        );
        window.start_timer(1, timer);
        assert!(window.has_active_timers());

        window.stop_timer(1);
        assert!(!window.has_active_timers());
    }

    #[test]
    fn test_stub_window_create_queue() {
        let mut window = make_stub();
        assert_eq!(window.pending_window_count(), 0);

        window.queue_window_create(WindowCreateOptions::default());
        assert_eq!(window.pending_window_count(), 1);
    }

    #[test]
    fn test_cpu_backend_creation() {
        let backend = CpuBackend::new();
        let results = backend.hit_tester.hit_test(
            azul_core::geom::LogicalPosition { x: 0.0, y: 0.0 },
        );
        assert!(results.is_empty());
    }

    extern "C" fn test_timer_callback(
        _data: RefAny,
        _info: azul_layout::timer::TimerCallbackInfo,
    ) -> azul_core::callbacks::TimerCallbackReturn {
        azul_core::callbacks::TimerCallbackReturn::terminate_unchanged()
    }
}
