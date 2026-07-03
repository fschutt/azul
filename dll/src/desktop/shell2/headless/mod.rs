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
    let ms = &window.common.current_window_state.mouse_state;
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

/// Outcome of a single `CpuBackend::render_frame` call — the seed of the
/// unified `DamageRegion` type described in `DAMAGE_REGION_PLAN.md`.
///
/// `render_frame` historically returned `Vec<LogicalRect>`, where an empty vec
/// was ambiguous: it meant *both* "nothing changed, render skipped" AND "full
/// repaint (first frame / structural change), no incremental rects". This enum
/// disambiguates so the headless damage harness (and later the platform
/// presenters) can tell a no-op from a full repaint.
#[derive(Debug, Clone, PartialEq)]
pub enum FrameDamage {
    /// Nothing changed; render was skipped, the previous frame is still valid.
    None,
    /// Incremental repaint of exactly these logical rects.
    Rects(Vec<azul_core::geom::LogicalRect>),
    /// Full repaint (first frame, structural change, or shrink-resize).
    Full,
}

impl FrameDamage {
    /// Convert this damage record into physical-pixel present rects for a
    /// `buf_w`×`buf_h` buffer at `dpi_factor` — the ONE conversion every
    /// platform presenter should use to hand damage to its compositor
    /// (`XPutImage` sub-rects / `wl_surface_damage` / partial `StretchDIBits`
    /// / `setNeedsDisplayInRect:`).
    ///
    /// - `None` → returns `None`: the previous frame is still on screen and
    ///   valid — present nothing. Callers must STILL present in full when the
    ///   OS asked for a re-present (Expose / WM_PAINT-from-uncover / drawRect)
    ///   — pass `force_full = true` there.
    /// - `Rects` → `Some(rects)` as `(x, y, w, h)` physical px, rounded
    ///   OUTWARD (floor origin / ceil far edge — truncation would under-cover
    ///   fractional edges and leave 1px stale seams), clamped to the buffer.
    ///   More than 16 rects collapses to one full-buffer rect (bounded cost,
    ///   per DAMAGE_REGION_PLAN §3).
    /// - `Full` → one full-buffer rect.
    ///
    /// "Present must never silently be empty when a present is required" —
    /// when in doubt, callers should treat errors/unknowns as `Full`.
    #[must_use]
    pub fn to_present_rects_physical(
        &self,
        dpi_factor: f32,
        buf_w: u32,
        buf_h: u32,
        force_full: bool,
    ) -> Option<Vec<(u32, u32, u32, u32)>> {
        const MAX_PRESENT_RECTS: usize = 16;
        if buf_w == 0 || buf_h == 0 {
            return None;
        }
        let full = || Some(vec![(0u32, 0u32, buf_w, buf_h)]);
        if force_full {
            // OS-driven expose: the on-screen content may be stale/undefined
            // regardless of what we last painted — push the whole retained
            // frame.
            return full();
        }
        match self {
            FrameDamage::None => None,
            FrameDamage::Full => full(),
            FrameDamage::Rects(rects) => {
                if rects.is_empty() {
                    return None;
                }
                if rects.len() > MAX_PRESENT_RECTS {
                    return full();
                }
                let mut out = Vec::with_capacity(rects.len());
                for r in rects {
                    let x0 =
                        ((r.origin.x * dpi_factor).floor() as i64).clamp(0, i64::from(buf_w));
                    let y0 =
                        ((r.origin.y * dpi_factor).floor() as i64).clamp(0, i64::from(buf_h));
                    let x1 = (((r.origin.x + r.size.width) * dpi_factor).ceil() as i64)
                        .clamp(0, i64::from(buf_w));
                    let y1 = (((r.origin.y + r.size.height) * dpi_factor).ceil() as i64)
                        .clamp(0, i64::from(buf_h));
                    if x1 > x0 && y1 > y0 {
                        #[allow(clippy::cast_sign_loss)] // clamped to [0, buf] above
                        out.push((x0 as u32, y0 as u32, (x1 - x0) as u32, (y1 - y0) as u32));
                    }
                }
                if out.is_empty() { None } else { Some(out) }
            }
        }
    }
}

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
    pub previous_display_list: Option<azul_layout::solver3::display_list::DisplayList>,
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
            last_frame_damage: FrameDamage::None,
            last_present_damage: FrameDamage::None,
            #[cfg(feature = "cpurender")]
            previous_scroll_offsets: azul_layout::cpurender::ScrollOffsetMap::new(),
            #[cfg(feature = "cpurender")]
            previous_vview_dls: std::collections::BTreeMap::new(),
            #[cfg(feature = "cpurender")]
            previous_gpu_transforms: std::collections::HashMap::new(),
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
        use azul_core::dom::DomId;
        use azul_layout::cpurender;

        // Get the layout result from layout results
        let dom_id = DomId { inner: 0 };
        let result = match layout_window.layout_results.get(&dom_id) {
            Some(result) => result,
            None => return Vec::new(),
        };
        let display_list = &result.display_list;

        let pixel_w = (width * dpi_factor).ceil() as u32;
        let pixel_h = (height * dpi_factor).ceil() as u32;
        if pixel_w == 0 || pixel_h == 0 {
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
        if needs_resize {
            let is_grow = pixel_w >= old_pw && pixel_h >= old_ph && old_pw > 0 && old_ph > 0;
            if is_grow {
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
                // Shrink or first allocation: full recreate
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
            .build_scroll_offset_map(dom_id, &result.scroll_ids);

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
        // Values are painted this frame whichever path runs (incremental
        // repaints read the CURRENT cache; skip only happens when unchanged).
        self.previous_gpu_transforms = gpu_transforms;
        self.previous_gpu_opacities = gpu_opacities;

        // Compute display list damage (incremental path)
        let dl_damage = match &self.previous_display_list {
            Some(old_dl) if !needs_resize && !gpu_damage.needs_full => {
                cpurender::compute_display_list_damage(
                    old_dl,
                    display_list,
                    &self.previous_scroll_offsets,
                    &scroll_offsets,
                )
            }
            _ => None, // first frame, resize or ref-frame transform → full repaint
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
                .map(|(id, r)| (*id, std::sync::Arc::new(r.display_list.clone())))
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

        match dl_damage {
            Some(rects)
                if rects.is_empty()
                    && resize_damage.is_empty()
                    && !has_scroll
                    && !has_vview_damage
                    && !has_gpu_damage =>
            {
                // Nothing changed — skip rendering entirely. (`!has_vview_damage`
                // keeps us out of this branch when only a VirtualView child DOM
                // changed — that case must still re-composite, see below.)
                self.previous_display_list = Some(display_list.clone());
                // Nothing painted: baseline keeps accumulating dropped
                // sub-pixel scroll deltas (see next_scroll_baseline above).
                self.previous_scroll_offsets = next_scroll_baseline;
                self.last_frame_damage = FrameDamage::None;
                self.last_present_damage = FrameDamage::None;
                return Vec::new();
            }
            Some(mut rects) if !needs_resize => {
                // Incremental: changed items + (scroll strips added below)
                rects.extend(resize_damage);
                all_damage = rects;
                is_incremental = true;
            }
            _ => {
                // Full repaint (first frame, structural change, resize). Scroll
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

        // Acquire output pixmap — reuse buffer for both grow and shrink
        let mut output = match self.last_frame.take() {
            Some(p) if p.width() == pixel_w && p.height() == pixel_h => p,
            Some(mut p) => {
                p.resize_reuse(pixel_w, pixel_h, 255, 255, 255, 255);
                p
            }
            None => match cpurender::AzulPixmap::new(pixel_w, pixel_h) {
                Some(mut p) => { p.fill(255, 255, 255, 255); p }
                None => return Vec::new(),
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
        if is_incremental {
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
                    let strips = cpurender::scroll_shift_region(
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
                .with_virtual_view_display_lists(vview_dls)
                .with_image_callback_results(layout_window.cpu_image_callback_results.clone());

        if is_incremental && !all_damage.is_empty() {
            // Incremental: render only damaged regions
            let _ = cpurender::render_display_list_damaged(
                display_list, &mut output, dpi_factor,
                renderer_resources, Some(&layout_window.font_manager),
                &mut self.glyph_cache, &render_state, &all_damage,
            );
        } else {
            // Full render
            output.fill(255, 255, 255, 255);
            compositor.allocate_layers_from_display_list(display_list, dpi_factor);
            if let Err(e) = compositor.render_layers(
                display_list, dpi_factor, renderer_resources,
                Some(&layout_window.font_manager), &mut self.glyph_cache,
                &render_state,
            ) {
                log_error!(
                    LogCategory::Rendering,
                    "[CpuBackend] render_layers error: {}",
                    e
                );
            }
            compositor.composite_frame(&mut output, dpi_factor);
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

        self.previous_display_list = Some(display_list.clone());
        // Full render paints EVERY frame at its current offset → baseline is
        // the current offsets. Incremental: only shifted frames advanced.
        self.previous_scroll_offsets = if is_incremental {
            next_scroll_baseline
        } else {
            scroll_offsets.clone()
        };
        self.last_frame = Some(output);
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
    /// Condvar + mutex used to block the event loop until work arrives.
    wake_condvar: Arc<Condvar>,
    wake_mutex: Arc<Mutex<WakeState>>,
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
        let full_window_state = options.window_state;

        // Create layout window — same as real platforms
        let mut layout_window = LayoutWindow::new(fc_cache.as_ref().clone())
            .map_err(|e| WindowError::PlatformError(format!("Layout init failed: {:?}", e)))?;
        layout_window.current_window_state = full_window_state.clone();
        layout_window.routes = config.routes.clone();

        let wake_condvar = Arc::new(Condvar::new());
        let wake_mutex = Arc::new(Mutex::new(WakeState { woken: false }));

        Ok(Self {
            common: CommonWindowState {
                layout_window: Some(layout_window),
                current_window_state: full_window_state,
                previous_window_state: None,
                image_cache: ImageCache::default(),
                renderer_resources: RendererResources::default(),
                fc_cache,
                gl_context_ptr: OptionGlContextPtr::None,
                system_style: Arc::new(crate::desktop::app::discover_system_style()),
                app_data,
                undo_manager,
                scrollbar_drag_state: None,
                hit_tester: None,
                cpu_hit_tester: Some(azul_layout::headless::CpuHitTester::new()),
                last_hovered_node: None,
                document_id: None,
                id_namespace: None,
                render_api: None,
                renderer: None,
                frame_needs_regeneration: true,
                frame_relayout_only: false,
                next_relayout_reason: azul_core::callbacks::RelayoutReason::Initial,
                display_list_initialized: false,
                display_list_dirty: false,
                a11y_dirty: true,
            },
            cpu_backend: CpuBackend::new(),
            is_open: true,
            event_queue: VecDeque::new(),
            thread_poll_timer_running: false,
            pending_window_creates: Vec::new(),
            config,
            icon_provider,
            font_registry,
            wake_condvar,
            wake_mutex,
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
        self.is_open = false;
    }

    // === Layout ===

    /// Regenerate layout and rebuild CPU hit-tester.
    ///
    /// This is the HeadlessWindow equivalent of `MacOSWindow::regenerate_layout()` /
    /// `WinWindow::regenerate_layout()` etc. It calls the shared
    /// `common::layout::regenerate_layout()` (which no longer requires WebRender
    /// types) and then rebuilds the `CpuHitTester` from the new layout results.
    pub fn regenerate_layout(
        &mut self,
    ) -> Result<crate::desktop::shell2::common::layout::LayoutRegenerateResult, String> {
        let layout_window = self.common.layout_window.as_mut().ok_or("No layout window")?;

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
            &self.common.app_data,
            &self.common.current_window_state,
            &mut self.common.renderer_resources,
            &self.common.image_cache,
            &self.common.gl_context_ptr,
            &self.common.fc_cache,
            &self.font_registry,
            &self.common.system_style,
            &self.icon_provider,
            &mut debug_messages,
            self.common.next_relayout_reason,
        )?;
        // Reset the reason now that it has been consumed. Subsequent
        // untagged regen calls (RefAny mutation -> Update::RefreshDom) will
        // see the implicit `RefreshDom` reason.
        self.common.next_relayout_reason =
            azul_core::callbacks::RelayoutReason::RefreshDom;

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
            self.cpu_backend.hit_tester.rebuild_from_layout(&lw.layout_results);
        }

        // Also rebuild the SHARED hit-tester that the common event-dispatch path
        // reads (perform_hit_test → update_hit_test_at). `cpu_backend.hit_tester`
        // above only feeds the headless render/screenshot path; pointer events
        // (real or synthetic via the debug server) resolve their target node
        // through `common.cpu_hit_tester`. Without this rebuild that tester stays
        // empty, so every click hit-tests to nothing and widget callbacks (e.g. a
        // button's on_click) never fire — clicks silently do nothing in headless.
        if let Some(ref mut cpu_ht) = self.common.cpu_hit_tester {
            if let Some(lw) = self.common.layout_window.as_ref() {
                cpu_ht.rebuild_from_layout(&lw.layout_results);
            }
        }

        // Drain any lifecycle events produced by reconciliation (Mount/Unmount/
        // Update/Resize) and dispatch them through the normal callback pipeline.
        // Doing this inside regenerate_layout keeps the headless test harness
        // self-contained: callers do not have to remember to pump lifecycle
        // events separately to see `.with_callback(EventFilter::Component(_))`
        // fire.
        self.dispatch_pending_lifecycle_events();

        // CPU-render the frame (retained compositor handles efficient resize)
        #[cfg(feature = "cpurender")]
        {
            let ws = &self.common.current_window_state;
            let width = ws.size.dimensions.width;
            let height = ws.size.dimensions.height;
            let dpi = ws.size.dpi as f32 / 96.0;
            // MWA-C-gpu_state: per-frame scrollbar thumb/fade cache refresh
            // (WR builders do this every frame; the CPU path refreshed only
            // on full relayout).
            if let Some(lw) = self.common.layout_window.as_mut() {
                lw.refresh_scrollbar_gpu_cache_for_cpu_frame();
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
        }

        // Mark that frame needs regeneration
        self.common.frame_needs_regeneration = true;

        Ok(result)
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

    /// Simulate a window resize. Updates `current_window_state.size` to the
    /// new logical dimensions and tags the next `regenerate_layout()` call
    /// with `RelayoutReason::Resize` so the user's `LayoutCallback` sees
    /// the size change via `info.relayout_reason()` plus the live
    /// `info.window_size`. The next `regenerate_layout()` call will
    /// re-invoke `layout()` exactly the way the real platform handlers do.
    pub fn simulate_resize(&mut self, width: f32, height: f32) {
        use azul_core::geom::LogicalSize;
        self.common.current_window_state.size.dimensions = LogicalSize { width, height };
        self.common.next_relayout_reason =
            azul_core::callbacks::RelayoutReason::Resize;
    }

    /// Read the queued reason for the next `regenerate_layout()` call.
    /// Useful for asserting in tests that an event handler tagged the
    /// upcoming relayout correctly.
    pub fn pending_relayout_reason(&self) -> azul_core::callbacks::RelayoutReason {
        self.common.next_relayout_reason
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
        let vec: TouchPointVec = points.into_iter().collect::<Vec<_>>().into();
        let touch_state = &mut self.common.current_window_state.touch_state;
        touch_state.num_touches = vec.len();
        touch_state.touch_points = vec;
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
        let flags = &mut self.common.current_window_state.flags;
        flags.fullscreen_mode = mode;
        // Fold the request into the current frame state so headless callers
        // can observe the transition without a real OS event loop.
        flags.frame = match mode {
            FullScreenMode::SlowFullScreen | FullScreenMode::FastFullScreen => {
                WindowFrame::Fullscreen
            }
            FullScreenMode::SlowWindowed | FullScreenMode::FastWindowed => WindowFrame::Normal,
        };
    }

    /// Returns `true` if every entry of `chord` is currently active in the
    /// window's keyboard state. Use to evaluate registered accelerator
    /// shortcuts (e.g. `[Ctrl, Key(VirtualKeyCode::S)]`) on each key event.
    pub fn matches_accelerator(&self, chord: &[AcceleratorKey]) -> bool {
        self.common
            .current_window_state
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

        // -- child windows (sub-HeadlessWindows for menus, dialogs) --
        let mut children: Vec<HeadlessWindow> = Vec::new();
        let mut warned_no_wake_sources = false;

        while self.is_open() {
            // ── Phase 1: Process injected events ─────────────────
            let mut events_need_redraw = false;
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
                        self.common.previous_window_state =
                            Some(self.common.current_window_state.clone());
                        let pos = LogicalPosition { x, y };
                        self.common.current_window_state.mouse_state.cursor_position =
                            CursorPosition::InWindow(pos);
                        self.update_hit_test_at(pos);
                        if let Some(lw) = self.common.layout_window.as_mut() {
                            // MWA-B7: full multi-file list, like the OS shells.
                            lw.file_drop_manager
                                .set_hovered_files(paths.into_iter().map(Into::into).collect());
                        }
                        let r = self.process_window_events(0);
                        if !matches!(r, azul_core::events::ProcessEventResult::DoNothing) {
                            events_need_redraw = true;
                        }
                    }
                    HeadlessEvent::FileDrop { x, y, paths } => {
                        use azul_core::window::CursorPosition;
                        self.common.previous_window_state =
                            Some(self.common.current_window_state.clone());
                        let pos = LogicalPosition { x, y };
                        self.common.current_window_state.mouse_state.cursor_position =
                            CursorPosition::InWindow(pos);
                        self.update_hit_test_at(pos);
                        if let Some(lw) = self.common.layout_window.as_mut() {
                            lw.file_drop_manager
                                .set_dropped_files(paths.into_iter().map(Into::into).collect());
                        }
                        let r = self.process_window_events(0);
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
                        self.common.previous_window_state =
                            Some(self.common.current_window_state.clone());
                        if let Some(lw) = self.common.layout_window.as_mut() {
                            // Some→None flags the cancel; the pass dispatches
                            // HoveredFileCancelled, then we clear the flag.
                            lw.file_drop_manager.set_hovered_file(None);
                        }
                        let r = self.process_window_events(0);
                        if !matches!(r, azul_core::events::ProcessEventResult::DoNothing) {
                            events_need_redraw = true;
                        }
                        if let Some(lw) = self.common.layout_window.as_mut() {
                            lw.file_drop_manager.clear_hover_cancelled();
                        }
                    }
                    HeadlessEvent::MouseMove { x, y } => {
                        use azul_core::window::CursorPosition;
                        self.common.previous_window_state =
                            Some(self.common.current_window_state.clone());
                        let pos = LogicalPosition { x, y };
                        self.common.current_window_state.mouse_state.cursor_position =
                            CursorPosition::InWindow(pos);
                        // MWA-C-scroll: active scrollbar thumb drag (desktop
                        // pattern) — scrollbar interaction was untestable in
                        // E2E because headless never routed it.
                        if self.common.scrollbar_drag_state.is_some() {
                            let r = PlatformWindow::handle_scrollbar_drag(&mut self, pos);
                            if !matches!(r, azul_core::events::ProcessEventResult::DoNothing) {
                                events_need_redraw = true;
                            }
                        } else {
                            self.update_hit_test_at(pos);
                            record_headless_input(&mut self, false, false); // MWA-A4
                            let r = self.process_window_events(0);
                            if !matches!(r, azul_core::events::ProcessEventResult::DoNothing) {
                                events_need_redraw = true;
                            }
                        }
                    }
                    HeadlessEvent::MouseDown { button } => {
                        self.common.previous_window_state =
                            Some(self.common.current_window_state.clone());
                        // MWA-C-scroll: scrollbar hit first (desktop pattern).
                        let sb_hit = if matches!(button, azul_core::events::MouseButton::Left) {
                            self.common
                                .current_window_state
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
                            self.common.current_window_state.mouse_state.left_down = true;
                            let r = PlatformWindow::handle_scrollbar_click(&mut self, hit, p);
                            if !matches!(r, azul_core::events::ProcessEventResult::DoNothing) {
                                events_need_redraw = true;
                            }
                        } else {
                        match button {
                            azul_core::events::MouseButton::Left => {
                                self.common.current_window_state.mouse_state.left_down = true;
                            }
                            azul_core::events::MouseButton::Right => {
                                self.common.current_window_state.mouse_state.right_down = true;
                            }
                            azul_core::events::MouseButton::Middle => {
                                self.common.current_window_state.mouse_state.middle_down = true;
                            }
                            _ => {}
                        }
                        record_headless_input(&mut self, true, false); // MWA-A4
                        let r = self.process_window_events(0);
                        if !matches!(r, azul_core::events::ProcessEventResult::DoNothing) {
                            events_need_redraw = true;
                        }
                        }
                    }
                    HeadlessEvent::MouseUp { button } => {
                        self.common.previous_window_state =
                            Some(self.common.current_window_state.clone());
                        // MWA-C-scroll: a release ends any scrollbar drag.
                        if self.common.scrollbar_drag_state.is_some() {
                            self.common.scrollbar_drag_state = None;
                            events_need_redraw = true;
                        }
                        match button {
                            azul_core::events::MouseButton::Left => {
                                self.common.current_window_state.mouse_state.left_down = false;
                            }
                            azul_core::events::MouseButton::Right => {
                                self.common.current_window_state.mouse_state.right_down = false;
                            }
                            azul_core::events::MouseButton::Middle => {
                                self.common.current_window_state.mouse_state.middle_down = false;
                            }
                            _ => {}
                        }
                        record_headless_input(&mut self, false, true); // MWA-A4
                        let r = self.process_window_events(0);
                        if !matches!(r, azul_core::events::ProcessEventResult::DoNothing) {
                            events_need_redraw = true;
                        }
                    }
                    HeadlessEvent::KeyDown { virtual_keycode } => {
                        self.common.previous_window_state =
                            Some(self.common.current_window_state.clone());
                        self.common.current_window_state.keyboard_state.current_virtual_keycode =
                            azul_core::window::OptionVirtualKeyCode::Some(virtual_keycode);
                        self.common.current_window_state.keyboard_state
                            .pressed_virtual_keycodes.insert_hm_item(virtual_keycode);
                        let r = self.process_window_events(0);
                        if !matches!(r, azul_core::events::ProcessEventResult::DoNothing) {
                            events_need_redraw = true;
                        }
                    }
                    HeadlessEvent::KeyUp { virtual_keycode } => {
                        self.common.previous_window_state =
                            Some(self.common.current_window_state.clone());
                        self.common.current_window_state.keyboard_state.current_virtual_keycode =
                            azul_core::window::OptionVirtualKeyCode::None;
                        self.common.current_window_state.keyboard_state
                            .pressed_virtual_keycodes.remove_hm_item(&virtual_keycode);
                        let r = self.process_window_events(0);
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
                        self.common.previous_window_state =
                            Some(self.common.current_window_state.clone());
                        let r = self.apply_user_change(
                            &azul_layout::callbacks::CallbackChange::CreateTextInput {
                                text: text.clone().into(),
                            },
                        );
                        if !matches!(r, azul_core::events::ProcessEventResult::DoNothing) {
                            events_need_redraw = true;
                        }
                    }
                    HeadlessEvent::Resize { width, height } => {
                        self.common.previous_window_state =
                            Some(self.common.current_window_state.clone());
                        self.common.current_window_state.size.dimensions.width = width;
                        self.common.current_window_state.size.dimensions.height = height;
                        // Tag the upcoming regenerate_layout with the REAL
                        // reason, same as `simulate_resize()` — the two
                        // headless resize entry points used to disagree
                        // (this one left the implicit RefreshDom), so the
                        // user's LayoutCallback saw a phantom non-resize
                        // relayout depending on which API drove the resize.
                        self.common.next_relayout_reason =
                            azul_core::callbacks::RelayoutReason::Resize;
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
            if let Some(lw) = self.common.layout_window.as_mut() {
                if !lw.pending_virtual_view_updates.is_empty() {
                    let system_callbacks =
                        azul_layout::callbacks::ExternalSystemCallbacks::rust_internal();
                    let current_window_state = lw.current_window_state.clone();
                    let renderer_resources = std::mem::take(&mut lw.renderer_resources);
                    let _ = lw.process_pending_virtual_view_updates(
                        &current_window_state,
                        &renderer_resources,
                        &system_callbacks,
                    );
                    lw.renderer_resources = renderer_resources;
                    events_need_redraw = true;
                }
            }

            if events_need_redraw {
                if let Err(e) = self.regenerate_layout() {
                    log_error!(
                        LogCategory::Layout,
                        "[Headless] Layout regeneration after event failed: {}",
                        e
                    );
                }
            }

            // ── Phase 2: Tick timers and threads ─────────────────
            // Use the shared PlatformWindow trait method to invoke
            // expired timer callbacks and poll background threads.
            let needs_redraw = self.process_timers_and_threads();

            // In the CPU-only path there is no GPU compositor that can
            // handle scroll-offset-only or repaint-only updates.  Every
            // visual change (including scroll) requires a full display
            // list rebuild, so we regenerate layout on any redraw signal.
            if needs_redraw {
                if let Err(e) = self.regenerate_layout() {
                    log_error!(
                        LogCategory::Layout,
                        "[Headless] Layout regeneration failed: {}",
                        e
                    );
                }
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

            // Lock, clear `woken`, then wait.
            let mut guard = self.wake_mutex.lock().unwrap();
            guard.woken = false;

            if has_timers {
                // Timers active → poll at 60 Hz
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
    // 28 getter/setter methods generated by macro — identical to all other platforms
    impl_platform_window_getters!(common);

    fn get_raw_window_handle(&self) -> RawWindowHandle {
        RawWindowHandle::Unsupported
    }

    fn prepare_callback_invocation(&mut self) -> event::InvokeSingleCallbackBorrows<'_> {
        let layout_window = self
            .common
            .layout_window
            .as_mut()
            .expect("Layout window must exist for callback invocation");

        event::InvokeSingleCallbackBorrows {
            layout_window,
            window_handle: RawWindowHandle::Unsupported,
            gl_context_ptr: &self.common.gl_context_ptr,
            image_cache: &mut self.common.image_cache,
            fc_cache_clone: (*self.common.fc_cache).clone(),
            system_style: self.common.system_style.clone(),
            previous_window_state: &self.common.previous_window_state,
            current_window_state: &self.common.current_window_state,
            renderer_resources: &mut self.common.renderer_resources,
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
        // No native window to synchronise
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
            .with_child(Dom::create_div().with_child(Dom::create_text(label.as_str())))
    }

    /// One embedded font for the whole harness. Using a bundled font instead of
    /// `FcFontCache::build()` makes glyph metrics DETERMINISTIC (tests never
    /// depend on which fonts the host has installed) and does ZERO disk access —
    /// the system-font scan was both a flakiness source and a contributor to the
    /// build-machine lockup (every test forking a full font enumeration).
    const HARNESS_FONT: &[u8] =
        include_bytes!("../../../../../doc/fonts/InstrumentSerif-Regular.ttf");

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
        opts.window_state.size.dimensions = LogicalSize::new(400.0, 300.0);
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

        let n: u32 = 200;
        let start = std::time::Instant::now();
        for _ in 0..n {
            window.regenerate_layout().expect("no-op relayout");
        }
        let elapsed = start.elapsed();
        let per = elapsed / n;
        println!(
            "[perf] {} no-op relayouts: total={:?} per={:?}",
            n, elapsed, per
        );

        // PERF BUDGET: a no-op relayout of a trivial DOM should be cheap
        // (cache hits, no re-render). 2ms is very generous; if nothing caches
        // and every frame fully re-lays-out + re-renders, this blows past it.
        // A slow UI — especially scrolling at this cost per frame — is unusable.
        assert!(
            per < std::time::Duration::from_millis(2),
            "no-op relayout too slow: {:?}/relayout (budget 2ms) — incremental \
             caching is not working; this is unusable for scrolling",
            per
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
    fn step(window: &mut HeadlessWindow, event: HeadlessEvent) -> FrameDamage {
        use azul_core::events::{MouseButton, ProcessEventResult};
        use azul_core::window::CursorPosition;
        use crate::desktop::shell2::common::event::PlatformWindow;

        window.common.previous_window_state =
            Some(window.common.current_window_state.clone());
        let mut needs_redraw = false;
        match event {
            HeadlessEvent::MouseMove { x, y } => {
                let pos = LogicalPosition { x, y };
                window.common.current_window_state.mouse_state.cursor_position =
                    CursorPosition::InWindow(pos);
                // MWA-C-scroll: active scrollbar thumb drag (desktop pattern).
                if window.common.scrollbar_drag_state.is_some() {
                    needs_redraw = !matches!(
                        PlatformWindow::handle_scrollbar_drag(window, pos),
                        ProcessEventResult::DoNothing
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
                        .current_window_state
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
                    window.common.current_window_state.mouse_state.left_down = true;
                    needs_redraw = !matches!(
                        PlatformWindow::handle_scrollbar_click(window, hit, p),
                        ProcessEventResult::DoNothing
                    );
                } else {
                    match button {
                        MouseButton::Left => window.common.current_window_state.mouse_state.left_down = true,
                        MouseButton::Right => window.common.current_window_state.mouse_state.right_down = true,
                        MouseButton::Middle => window.common.current_window_state.mouse_state.middle_down = true,
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
                    MouseButton::Left => window.common.current_window_state.mouse_state.left_down = false,
                    MouseButton::Right => window.common.current_window_state.mouse_state.right_down = false,
                    MouseButton::Middle => window.common.current_window_state.mouse_state.middle_down = false,
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
                window.common.current_window_state.keyboard_state.current_virtual_keycode =
                    azul_core::window::OptionVirtualKeyCode::Some(virtual_keycode);
                window.common.current_window_state.keyboard_state
                    .pressed_virtual_keycodes.insert_hm_item(virtual_keycode);
                needs_redraw = !matches!(
                    window.process_window_events(0),
                    ProcessEventResult::DoNothing
                );
            }
            HeadlessEvent::KeyUp { virtual_keycode } => {
                window.common.current_window_state.keyboard_state.current_virtual_keycode =
                    azul_core::window::OptionVirtualKeyCode::None;
                window.common.current_window_state.keyboard_state
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
        // The scrollbar redamages every frame (ScrollBarStyled has no
        // is_visually_equal arm — known coarseness, tracked separately), so
        // "no repaint" is asserted on the CONTENT area (x < 200) only.
        let content_damage = |d: &FrameDamage| -> Vec<azul_core::geom::LogicalRect> {
            match d {
                FrameDamage::Rects(rs) => {
                    rs.iter().filter(|r| r.origin.x < 200.0).copied().collect()
                }
                FrameDamage::Full => vec![azul_core::geom::LogicalRect {
                    origin: azul_core::geom::LogicalPosition { x: 0.0, y: 0.0 },
                    size: azul_core::geom::LogicalSize { width: 1.0, height: 1.0 },
                }],
                FrameDamage::None => Vec::new(),
            }
        };
        assert!(
            content_damage(&d1).is_empty(),
            "0.2px scroll must not repaint CONTENT (sub-device-pixel); got {:?}",
            d1
        );
        assert!(
            content_damage(&d2).is_empty(),
            "0.4px cumulative must not repaint CONTENT yet; got {:?}",
            d2
        );
        assert!(
            !content_damage(&d3).is_empty(),
            "0.6px CUMULATIVE scroll crossed half a device pixel and must \
             repaint content — if the content damage is empty the baseline \
             advanced on skipped frames and slow trackpad scrolling is \
             swallowed forever; got {:?}",
            d3
        );
    }

    /// REGRESSION (idle skip with scrollbars): a no-op relayout of a window
    /// WITH a scrollbar must reach `FrameDamage::None`. ScrollBarStyled used
    /// to fall into `is_visually_equal`'s `_ => false` catch-all, so every
    /// scrollbar'd window re-damaged its bar every frame — the skip path was
    /// unreachable and idle windows re-rendered + re-presented forever (the
    /// thumb position now flows through the GPU-value damage channel instead).
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
        match &damage {
            FrameDamage::Rects(rs) => {
                assert!(
                    rs.iter().any(|r| r.origin.x >= 190.0),
                    "scroll must damage the scrollbar region (thumb moved via                      GPU value cache); got {:?}",
                    rs
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
            let ws = &w.common.current_window_state;
            ws.size.dpi as f32 / 96.0
        };
        let mut reference = azul_layout::cpurender::AzulPixmap::new(pw, ph).expect("ref pixmap");
        reference.fill(255, 255, 255, 255);
        let lw = w.common.layout_window.as_ref().unwrap();
        let dom = DomId { inner: 0 };
        let result = lw.layout_results.get(&dom).unwrap();
        let offsets = lw.scroll_manager.build_scroll_offset_map(dom, &result.scroll_ids);
        let rs = azul_layout::cpurender::CpuRenderState::new(offsets)
            .with_system_style(lw.system_style.clone());
        let full_clip = LogicalRect {
            origin: LogicalPosition::new(0.0, 0.0),
            size: LogicalSize::new(pw as f32 / dpi, ph as f32 / dpi),
        };
        let _ = azul_layout::cpurender::render_display_list_damaged(
            &result.display_list,
            &mut reference,
            dpi,
            &w.common.renderer_resources,
            Some(&lw.font_manager),
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
