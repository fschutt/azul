//! The CPU frame: what every CPU-drawn window of every shell paints and presents.
//!
//! `CpuBackend` has a similar *purpose* to the WebRender pipeline (render-api, renderer,
//! hit-tester) but is fully CPU-based and much simpler. It holds the retained compositor, the
//! previous frame's lists, offsets and GPU values (everything the frame-to-frame damage diff
//! needs) and renders the layout window's lists into a pixmap, repainting only what changed.
//!
//! ONE implementation. The headless window, the E2E runner and every desktop shell (macOS, X11,
//! Wayland, Windows, iOS, Android) call this `render_frame`: it used to be written twice, in the
//! dll and (without the platform parts) in the E2E runner, and every damage rule had to be put
//! into both and tested in both.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use azul_core::resources::RendererResources;

use crate::{
    window::{FrameDamage, LayoutWindow},
    window_state::FullWindowState,
};

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
/// May the next frame be painted INCREMENTALLY - only the damage rects
/// re-rastered, every other pixel left standing in the OUTPUT buffer?
///
/// Only while that buffer still holds the PREVIOUS frame:
/// - a resize that GREW the buffer preserved it (`resize_preserved_pixels`);
///   a shrink threw it away,
/// - a different canvas colour invalidates every reused pixel,
/// - `force_full_repaint` is the shell's own "something changed outside the
///   display list",
/// - and `target_holds_previous_frame` is the one the PLATFORM BACKBUFFER
///   path gets wrong. `CpuBackend::native_target` is documented to already
///   hold frame N-1, but a buffer the platform just (re)created holds
///   nothing: a `CreateDIBSection` DIB is zeroed, a never-filled `wl_shm`
///   slot is zeroed, a resized macOS view framebuffer is white-filled.
///   Rastering damage strips into any of those presents the fill everywhere
///   the diff found nothing.
///
/// Pure so the law can be tested without a window.
pub fn frame_may_reuse_previous_pixels(
    needs_resize: bool,
    resize_preserved_pixels: bool,
    clear_color_changed: bool,
    force_full_repaint: bool,
    target_holds_previous_frame: bool,
) -> bool {
    target_holds_previous_frame
        && (!needs_resize || resize_preserved_pixels)
        && !clear_color_changed
        && !force_full_repaint
}

pub struct CpuBackend {
    /// CPU-based hit tester rebuilt after each layout pass.
    pub hit_tester: crate::headless::CpuHitTester,
    /// The window's background material is `Transparent`: frames are
    /// cleared to transparent black instead of opaque white, so whatever
    /// the content leaves at alpha 0 shows the desktop through (the backend
    /// presents the alpha). Set by the backend from the window state before
    /// every `render_frame`.
    pub transparent: bool,
    /// Whether an unstyled window follows the SYSTEM background colour.
    ///
    /// True for a real window on a desktop: an app that sets no
    /// `background_color` should get the system window background of the mode
    /// the window shows (`common::window_clear_color`), so a dark system theme
    /// - or a dark app pin - does not produce a white sheet. False for offscreen
    /// rendering, where there is no desktop to follow and the output has to be
    /// reproducible byte for byte — the headless renderer backs screenshots,
    /// PDF export and the reference images the scroll tests diff against, and
    /// those must not change colour with whatever machine runs them.
    pub follow_system_background: bool,
    /// The canvas colour the PREVIOUS frame was cleared to, or `None` before
    /// the first frame.
    ///
    /// A frame is normally repainted incrementally: only the rectangles the
    /// display-list diff reports are redrawn and everything else is reused
    /// from the last frame. That reuse is only sound while the canvas UNDER
    /// the content stays the same colour — switching the desktop between light
    /// and dark repaints every widget that changed, and leaves the old
    /// backdrop standing everywhere the diff found nothing, so the window ends
    /// up half light and half dark until something else forces a full repaint.
    pub last_clear_color: Option<[u8; 4]>,
    /// The scale factor the PREVIOUS frame was painted at, or `None` before the first frame: a
    /// frame at another factor is repainted in full, like one on another canvas colour.
    pub last_dpi_factor: Option<f32>,
    /// Repaint the next frame in full (for changes outside the display list); consumed by `render_frame`.
    pub force_full_repaint: bool,
    /// Implied by `transparent`: after every frame the window's shape (the
    /// rectangles of opaque-enough pixels, physical px) is computed into
    /// `last_shape` for the backend to hand to the OS, so clicks on fully
    /// transparent pixels fall through like they do on macOS.
    pub shape_from_alpha: bool,
    /// The shape of the last rendered frame when `shape_from_alpha` is on
    /// (`None` otherwise, or before the first frame). Physical pixels.
    pub last_shape: Option<Vec<crate::cpurender::ShapeRect>>,
    /// The shape the backend last handed to the OS, so an unchanged frame
    /// shape does not re-issue the (not free) OS call.
    pub applied_shape: Option<Vec<crate::cpurender::ShapeRect>>,
    /// Last rendered pixmap (if CPU rendering is enabled).
    /// `None` when rendering is disabled (layout-only mode).
    pub last_frame: Option<crate::cpurender::AzulPixmap>,
    /// Retained compositor state with per-layer pixbufs.
    pub compositor: Option<crate::cpurender::CompositorState>,
    /// Glyph cache — persists across frames for text rendering.
    pub glyph_cache: crate::glyph_cache::GlyphCache,
    /// Previous display list for damage rect computation.
    pub previous_display_list:
        Option<Arc<crate::solver3::display_list::DisplayList>>,
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
    /// The pixels the most recent `render_frame` MOVED inside the frame instead of painting
    /// them (a scroll, a pan of a view), in the order it moved them
    /// (`cpurender::PresentMove`: `new[p] = old[p - (dx, dy)]` for every device pixel `p` of
    /// `dst`). A presenter that can move the pixels it already shows applies these and THEN
    /// presents `last_present_damage_after_moves`, which wins where it overlaps a `dst`; one
    /// that cannot ignores both and presents `last_present_damage`, which covers the moved
    /// clips whole. Empty for a full repaint and for a frame that moved nothing.
    pub last_present_moves: Vec<crate::cpurender::PresentMove>,
    /// What a presenter that applied `last_present_moves` still has to present: the paint
    /// damage (the strips that came into view, what is painted over the moved content, every
    /// other change of the frame) and the boxes of moves that are not reported as moves (the
    /// layout translation blit). A pan of a 2510 x 1402 view by 12 px is 18,226 px here and
    /// 3.5 M px in `last_present_damage`.
    pub last_present_damage_after_moves: FrameDamage,
    /// Arc ptr of the display list the round-3 patch SHIFT was last applied
    /// for. A buffers-held retry re-runs render_frame with the SAME list —
    /// re-applying the memmove would double-shift the retained pixels, so
    /// the hint is gated on ptr inequality (the retry then takes the plain
    /// diff, which damages the movers normally).
    pub last_patch_shift_dl: usize,
    /// The layout pass (`LayoutCache::layout_seq`) the frame on screen shows:
    /// the layout move hint is blitted only from that pass
    /// (`LayoutCache::last_patch_move_from_seq`).
    pub presented_layout_seq: Option<u64>,
    /// #27 native backbuffer: when set, the NEXT `render_frame` draws
    /// DIRECTLY into this externally-owned pixmap (a platform backbuffer,
    /// e.g. the free Wayland shm slot) instead of the owned `last_frame`.
    /// Shell contract: the buffer (a) matches the frame's pixel dimensions,
    /// (b) already holds the previous frame's pixels (cross-slot catch-up),
    /// (c) outlives the call, and (d) the shell clears this field after the
    /// call (dangle guard — the mapping dies with the pool). Consumed by
    /// `render_frame`; `last_frame` stays `None` afterwards. Never set in
    /// headless/e2e runs.
    pub native_target: Option<crate::cpurender::AzulPixmap>,
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
    /// #27: the shell's answer to "does the armed `native_target` ALREADY
    /// hold the previous frame?" - clause (b) of the `native_target`
    /// contract, which until now nothing enforced. Set at EVERY arming, next
    /// to `native_target` itself; only read while a target is armed.
    ///
    /// False means the platform just handed over a buffer it created or
    /// refilled this frame (a re-created Windows DIB section, a never-filled
    /// `wl_shm` slot, a white-filled macOS view framebuffer), and the frame
    /// must be repainted in FULL - see `frame_may_reuse_previous_pixels`.
    pub native_target_holds_previous_frame: bool,
    /// Pixels of row padding the armed `native_target` carries past the
    /// frame's width: the target is `frame width + this` pixels wide because
    /// an `AzulPixmap`'s width IS its row pitch. A Wayland `wl_shm` slot pads
    /// every row to 256 bytes (the pitch a compositor's GPU samples in place,
    /// `linux/wayland/shm.rs`); the frame is drawn into columns
    /// `0..frame width` and the padding is never presented. The Wayland shell
    /// sets it at every arming; it stays 0 on every shell whose buffer rows
    /// are tight. Only read while a target is armed.
    pub native_target_row_padding_px: u32,
    /// Scroll offsets from the previous frame (scroll_id → (x,y)). Used to detect
    /// scroll-offset changes and damage the affected frame's viewport so its
    /// content re-renders at the new offset (#13 — the display list is unchanged
    /// on scroll, so the diff alone only catches the scrollbar).
    pub previous_scroll_offsets: crate::cpurender::ScrollOffsetMap,
    /// Previous frame's `VirtualView` child-DOM display lists (keyed by child
    /// `DomId`). The parent display list's `VirtualView` item is unchanged when
    /// only the child re-renders (async tile writeback, etc.), so the parent-DL
    /// diff can't see it. Comparing child DLs frame-to-frame lets `render_frame`
    /// damage the VirtualView region when its content changed — otherwise the
    /// "nothing changed → skip" path freezes async VirtualView content. Without
    /// this, the MapWidget showed only the placeholder grid on backends (Wayland)
    /// that don't get spurious WM expose events to force a full repaint.
    pub previous_vview_dls: BTreeMap<
        azul_core::dom::DomId,
        Arc<crate::solver3::display_list::DisplayList>,
    >,
    /// GPU-animated values of the previous frame (`key.id → value`: transforms,
    /// opacities and bound background colours), for the frame-to-frame GPU-value
    /// diff. Scrollbar thumb position/fade opacity and drag/CSS transforms live in
    /// the GPU value cache — display-list items only carry the KEYS, so the item
    /// diff can't see them change. The values of EVERY DOM of the window: a node
    /// inside a `VirtualView` binds its key in the view's own cache, and a write
    /// of the value alone moves it there without a new list.
    pub previous_gpu_values: crate::cpurender::WindowGpuValues,
    /// Where zombie exits painted LAST frame (logical px) — see the
    /// zombie-damage computation in `render_frame`.
    pub previous_zombie_rects: Vec<azul_core::geom::LogicalRect>,
    /// The group surfaces kept between frames (`cpurender::retained`): the picture of a turned
    /// or scaled reference frame whose content did not change is composited again instead of
    /// painted again. Handed to every incremental frame's render state.
    pub retained_frames: Arc<crate::cpurender::RetainedFrames>,
}


impl Default for CpuBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl CpuBackend {
    /// The backend of a window of the app `config` describes: its text is
    /// anti-aliased as the app says (`AppConfig::text_antialiasing`, which
    /// `AppConfig::create` read from the environment once). `new()` is the
    /// backend of a window that belongs to no app - a test, a tool - and
    /// keeps the platform's default.
    pub fn for_config(config: &azul_core::resources::AppConfig) -> Self {
        #[allow(unused_mut)]
        let mut backend = Self::new();
        backend
            .glyph_cache
            .set_text_aa(config.text_antialiasing.into());
        backend
    }

    pub fn new() -> Self {
        Self {
            hit_tester: crate::headless::CpuHitTester::new(),
            transparent: false,
            follow_system_background: true,
            last_clear_color: None,
            last_dpi_factor: None,
            shape_from_alpha: false,
            last_shape: None,
            applied_shape: None,
            last_frame: None,
            compositor: None,
            glyph_cache: crate::glyph_cache::GlyphCache::new(),
            force_full_repaint: false,
            previous_display_list: None,
            last_consumed_build_seq: 0,
            last_frame_damage: FrameDamage::None,
            last_present_damage: FrameDamage::None,
            last_present_moves: Vec::new(),
            last_present_damage_after_moves: FrameDamage::None,
            last_patch_shift_dl: 0,
            presented_layout_seq: None,
            native_target: None,
            rendered_native: false,
            native_target_pool_order: false,
            native_target_holds_previous_frame: false,
            native_target_row_padding_px: 0,
            previous_scroll_offsets: crate::cpurender::ScrollOffsetMap::new(),
            previous_vview_dls: BTreeMap::new(),
            previous_gpu_values: crate::cpurender::WindowGpuValues::default(),
            previous_zombie_rects: Vec::new(),
            retained_frames: Arc::new(crate::cpurender::RetainedFrames::new()),
        }
    }

    /// Read the window's transparency / shape flags off the window state
    /// before a frame: what the frame is cleared to, and whether its shape
    /// is computed. Every backend's CPU present calls this first.
    pub fn sync_window_flags(&mut self, state: &FullWindowState) {
        self.transparent = !matches!(
            state.flags.background_material,
            azul_core::window::WindowBackgroundMaterial::Opaque
        );
        self.shape_from_alpha = self.transparent;
    }

    /// After a present: the frame's shape if it differs from the one the
    /// OS has (recorded as applied). `None` = nothing to do. A frame that
    /// painted nothing opaque keeps the OS shape (the window does not vanish).
    pub fn take_changed_shape(&mut self) -> Option<Vec<crate::cpurender::ShapeRect>> {
        let shape = self.last_shape.as_ref()?;
        if shape.is_empty() || self.applied_shape.as_ref() == Some(shape) {
            return None;
        }
        self.applied_shape = Some(shape.clone());
        self.applied_shape.clone()
    }

    /// Render the current display list into `last_frame`.
    ///
    /// Uses damage-rect-based incremental rendering when possible:
    /// - Compares current display list against `previous_display_list`
    /// - If items match structurally, only repaints changed regions
    /// - On resize, uses grow-only buffer reuse for window expansion
    ///
    /// Returns the damage rects that were rendered (empty = full repaint).
    pub fn render_frame(
        &mut self,
        layout_window: &LayoutWindow,
        renderer_resources: &RendererResources,
        width: f32,
        height: f32,
        dpi_factor: f32,
    ) -> Vec<azul_core::geom::LogicalRect> {
        // Engine observability: frame duration + probe spans per present
        // (drop-guard covers all return paths).
        #[cfg(feature = "telemetry")]
        let _frame_pump = crate::telemetry::FramePump::begin("present");

        use azul_core::dom::DomId;
        use crate::cpurender;

        // #27: describes THIS call only — set at target acquisition below.
        self.rendered_native = false;

        // Every shell's CPU frame comes through here: the window's presented-
        // frame generation moves (the debug server's screenshot cache keys
        // on it).
        layout_window.note_frame_presented();
        // What this frame does is counted for the window's frame report.
        let _counted = crate::cpu_counters::FrameScope::begin(layout_window);

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
                self.last_present_moves.clear();
                self.last_present_damage_after_moves = FrameDamage::None;
                return Vec::new();
            }
        };
        let display_list = &result.display_list;

        let pixel_w = (width * dpi_factor).ceil() as u32;
        let pixel_h = (height * dpi_factor).ceil() as u32;
        if pixel_w == 0 || pixel_h == 0 {
            self.last_frame_damage = FrameDamage::None;
            self.last_present_damage = FrameDamage::None;
            self.last_present_moves.clear();
            self.last_present_damage_after_moves = FrameDamage::None;
            return Vec::new();
        }

        // Allocate or resize compositor.
        //
        // The canvas colour is THE clear colour (`common::window_clear_color`),
        // the one WebRender is kept on too: transparent for a material, else
        // the window's `background_color` (the app's, or the one its MODE
        // derived), else - for a real window on a desktop - the system window
        // background of the mode the window SHOWS. The mode is asked of the
        // layout window (`window_mode_for`, the one decision) rather than
        // read raw: the desktop's palette under a dark app pin painted a light
        // sheet around dark widgets, on every backend this path serves (macOS,
        // X11, Wayland, Windows, iOS, Android and headless).
        let clear_color: [u8; 4] = {
            let ws = &layout_window.current_window_state;
            let c = crate::window_background::window_clear_color(
                ws.background_color,
                layout_window.window_mode_for(ws.mode),
                layout_window.system_style.as_deref(),
                self.follow_system_background,
                self.transparent,
            );
            [c.r, c.g, c.b, c.a]
        };
        let compositor = self
            .compositor
            .get_or_insert_with(|| cpurender::CompositorState::new(pixel_w, pixel_h));
        compositor.set_clear_color(clear_color);

        // A different canvas colour invalidates every reused pixel, so this
        // frame cannot be incremental no matter what the display-list diff
        // says (see `last_clear_color`).
        let clear_color_changed = self
            .last_clear_color
            .is_some_and(|previous| previous != clear_color);
        self.last_clear_color = Some(clear_color);
        // Nor can a frame painted at another scale factor be repainted in parts: every pixel
        // of it is another pixel now (a window moved to a display of another scale; the shells
        // also force a full frame then, which this makes redundant, not wrong). The kept group
        // surfaces are pictures at the old factor (their keys hold it, so none would be used);
        // they are dropped now instead of when their lists change.
        let dpi_changed = self
            .last_dpi_factor
            .is_some_and(|previous| previous.to_bits() != dpi_factor.to_bits());
        self.last_dpi_factor = Some(dpi_factor);
        if dpi_changed {
            self.retained_frames.clear();
        }
        let clear_color_changed = clear_color_changed || dpi_changed;

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
                    let _ = root_layer
                        .pixbuf
                        .resize_grow_only(pixel_w, pixel_h, 255, 255, 255, 255);
                    root_layer.bounds.size = azul_core::geom::LogicalSize {
                        width: pixel_w as f32,
                        height: pixel_h as f32,
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
                // A shrink, or a mixed resize (wider but shorter): the layers
                // are made again at the new size (they serve the full
                // repaint), and the frame keeps its pixels - the target is
                // cut to the new size from its top-left (`resize_reuse`, or
                // the platform's own buffer), so what is still on screen is
                // valid and only the strip a side gained is unknown
                // (`compute_resize_damage`: none for a shrink). Every step
                // of a shrinking window-edge drag repainted the whole window.
                *compositor = cpurender::CompositorState::new(pixel_w, pixel_h);
                if old_pw > 0 && old_ph > 0 {
                    resize_preserved_pixels = true;
                    resize_damage = cpurender::compute_resize_damage(
                        old_pw as f32 / dpi_factor,
                        old_ph as f32 / dpi_factor,
                        width,
                        height,
                    );
                }
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

        // The `VirtualView` child lists (the nested DOMs - e.g. the MapWidget's tile
        // grid): the GPU-value diff below reaches into them, the child-content damage
        // diffs them against last frame's, and the renderer composites them.
        let vview_dls: BTreeMap<
            DomId,
            Arc<crate::solver3::display_list::DisplayList>,
        > = layout_window
            .layout_results
            .iter()
            .filter(|(id, _)| id.inner != dom_id.inner)
            .map(|(id, r)| (*id, r.display_list.clone()))
            .collect();

        // GPU-value diff: thumb position / fade opacity / drag & CSS
        // transforms change WITHOUT any display-list item changing (items
        // only carry the keys). Diff the cache values against last frame's
        // and damage the bound items — this is what lets ScrollBarStyled
        // compare as visually-equal (skip path reachable for scrollbar'd
        // windows) without freezing the thumb. The values of EVERY DOM: a node
        // inside a view binds its key in the view's own cache, and a write of
        // the value alone (`set_css_property(transform)` on a map's picture)
        // moves it there without a new list - the view's list is diffed too.
        let gpu_values = cpurender::WindowGpuValues::of_doms(
            &layout_window.gpu_state_manager,
            layout_window.layout_results.keys().copied(),
        );
        // In VIEWPORT space: an item inside a scrolled frame (a thumb on a
        // scrolled page) is repainted where the frame paints it. A colour fade
        // frame - the bound background rects whose colour moved - is part of it.
        let gpu_damage = cpurender::window_value_damage(
            display_list,
            &vview_dls,
            &self.previous_gpu_values,
            &gpu_values,
            &scroll_offsets,
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
        // The frame before's matrices stay for the list diff below: a change
        // under an animated frame is damaged where each frame painted it.
        let old_gpu_values = core::mem::replace(&mut self.previous_gpu_values, gpu_values);

        // Can the pixels of the previous frame still be trusted? Yes when the
        // buffer did not change size at all, and yes on a GROW (the old pixels
        // were copied over verbatim). No on a shrink / first allocation.
        //
        // Whose pixels they are depends on where this frame is going: an armed
        // platform backbuffer answers for itself (the shell sets the flag at
        // every arming), the owned path answers with the retained frame -
        // `last_frame` is `None` on the very first frame and after any frame
        // that went to a native target, and a freshly allocated pixmap holds
        // nothing to stand on.
        let target_holds_previous_frame = if self.native_target.is_some() {
            self.native_target_holds_previous_frame
        } else {
            self.last_frame.is_some()
        };
        let can_reuse_previous_frame = frame_may_reuse_previous_pixels(
            needs_resize,
            resize_preserved_pixels,
            clear_color_changed,
            core::mem::take(&mut self.force_full_repaint),
            target_holds_previous_frame,
        );

        // ROUND 3: the layout patch's presentation hint. Eligible when the
        // dominant delta is INTEGRAL in physical pixels (a fractional blit
        // would change every subpixel phase — those frames re-render), the
        // previous frame's pixels are trustworthy, and this display list has
        // not already been shifted (buffers-held retry).
        let dl_arc_ptr = Arc::as_ptr(display_list) as usize;
        // Shared with the e2e harness on purpose: this logic used to live only
        // here, and layout/src/e2e/cpu_backend.rs had no notion of a translate
        // hint at all — zero mentions of TranslateHint, dominant_delta or the
        // already-shifted guard. So no e2e scenario could execute the blit path,
        // which is exactly where "typing does not repaint" lives.
        let patch_hint: Option<(cpurender::TranslateHint, Vec<azul_core::geom::LogicalRect>)> =
            cpurender::translate_hint_for_patch(
                layout_window.layout_cache.last_patch_move.as_ref().filter(|_| {
                self.presented_layout_seq == Some(layout_window.layout_cache.last_patch_move_from_seq)
            }),
                dpi_factor,
                can_reuse_previous_frame,
                self.last_patch_shift_dl == dl_arc_ptr,
                layout_window.scroll_manager.any_nonzero_offset(),
            );

        // Compute display list damage (incremental path)
        let mut patch_moved_union: Option<azul_core::geom::LogicalRect> = None;
        let dl_damage = match &self.previous_display_list {
            // SAME Arc = the DL cache served the identical list (the scroll
            // steady state): zero items changed BY DEFINITION, and the
            // shift/strip path owns every changed pixel. The O(items) walk
            // below cost ~6ms/frame on a multi-thousand-item page for a
            // provably-empty answer.
            Some(old_dl)
                if can_reuse_previous_frame
                    && !gpu_damage.needs_full
                    && Arc::ptr_eq(old_dl, display_list) =>
            {
                Some(Vec::new())
            }
            Some(old_dl) if can_reuse_previous_frame && !gpu_damage.needs_full => {
                // The per-frame OLD-vs-NEW item walk - O(items) visual
                // equality over the whole list, every frame. Spanned so the
                // "what eats the frame" question has its biggest hidden
                // candidate on the board.
                let _p = crate::probe::Probe::span("damage_dl_diff");
                cpurender::compute_display_list_damage_live(
                    old_dl,
                    display_list,
                    &self.previous_scroll_offsets,
                    &scroll_offsets,
                    patch_hint.as_ref().map(|(h, _)| h),
                    cpurender::FrameValues {
                        old: Some(&old_gpu_values.transforms),
                        new: Some(&self.previous_gpu_values.transforms),
                    },
                )
                .map(|(rects, moved)| {
                    patch_moved_union = moved;
                    rects
                })
            }
            _ => None, // first frame, shrink or ref-frame transform → full repaint
        };

        let vview_span = crate::probe::Probe::span("damage_vview_scan");
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
        // One shared, viewport-projected collector with the e2e twin - see
        // `cpurender::collect_scroll_shifts` (nested frames used to get an
        // unprojected clip).
        let scroll_shifts = cpurender::collect_scroll_shifts(
            display_list,
            &scroll_offsets,
            &self.previous_scroll_offsets,
            // The matrices this frame paints with, kept for the next one.
            &self.previous_gpu_values.transforms,
            dpi_factor,
        );
        let has_scroll = !scroll_shifts.is_empty();
        // Advance the scroll baseline ONLY for frames that actually get painted
        // at their new offset this call (shifted frames now; ALL frames on the
        // full-repaint path — finalised at the end of render_frame). Frames
        // whose sub-half-pixel delta was dropped keep their previous baseline,
        // so slow trackpad scrolling ACCUMULATES until it crosses a device
        // pixel instead of being silently swallowed frame after frame (content
        // frozen while the logical offset advances arbitrarily far).
        let shifted_ids: BTreeSet<u64> = scroll_shifts.iter().map(|(sid, ..)| *sid).collect();
        let next_scroll_baseline: cpurender::ScrollOffsetMap = scroll_offsets
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

        // The views' damage lands where this frame PAINTS them: an incremental
        // frame rasterises at `next_scroll_baseline` (see `render_offsets`
        // below), and a view inside a scrolled box is painted at its content
        // box minus that offset. At the content box a view far down a scrolled
        // page (the AzWidgets Video card) was repainted off-screen on every
        // frame and froze on its first one. The full-repaint path paints
        // everything and ignores these rects.
        // What changed inside a view is damaged where the view's pixels are painted: through
        // the item's content offset, the offsets and the live matrices of the frames inside
        // the view and around it (`compute_virtual_view_damage_live`).
        let vview_damage = cpurender::compute_virtual_view_damage_live(
            display_list,
            &vview_dls,
            &self.previous_vview_dls,
            &self.previous_scroll_offsets,
            &next_scroll_baseline,
            cpurender::FrameValues {
                old: Some(&old_gpu_values.transforms),
                new: Some(&self.previous_gpu_values.transforms),
            },
        );
        let has_vview_damage = !vview_damage.is_empty();
        self.previous_vview_dls = vview_dls.clone();

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
                "[HLDMG-HINT] move_summary={:?} hint={:?} scroll_nonzero={} moved={:?}",
                layout_window
                    .layout_cache
                    .last_patch_move
                    .as_ref()
                    .map(|m| (m.dominant_delta, m.mover_rects_old.len(), m.exceptions.len())),
                patch_hint.as_ref().map(|(h, _)| h.delta),
                layout_window.scroll_manager.any_nonzero_offset(),
                patch_moved_union,
            );
            eprintln!(
                "[HLDMG-PRE] item_diff={:?} prev_is_same_arc={} prev_items={:?} new_items={}",
                dl_damage,
                self.previous_display_list
                    .as_ref()
                    .is_some_and(|p| Arc::ptr_eq(p, display_list)),
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
        drop(vview_span);
        let pending = layout_window
            .layout_cache
            .pending_patch_damage(self.last_consumed_build_seq);
        // Patch rects are built from calculated_positions in CONTENT space
        // (no scroll projection on the producer side); with any active scroll
        // offset they would land offset-pixels away from the pixels that
        // changed — stale bands rescued by rects in the wrong place. Until
        // the producer projects, a scrolled window demotes them to
        // FullBuildSincePresent semantics (item diff decides; its bail is a
        // full repaint).
        let patch_rects_trustworthy = !layout_window.scroll_manager.any_nonzero_offset();
        let dl_damage = if diff_path_ran && layout_window.layout_cache.last_build_was_patched {
            use crate::solver3::cache::PendingPatchDamage as P;
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
                (d, P::Rects(_)) if !patch_rects_trustworthy => d,
                (Some(mut d), P::Rects(p)) => {
                    d.extend(p);
                    Some(d)
                }
                // The diff bailed: the patch rects are the damage - or, when the
                // lists pair and the build logged none (the diff was to decide), a
                // full repaint.
                (None, P::Rects(p)) if !p.is_empty() => Some(p),
                (None, P::Rects(_)) => None,
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
            // WHY a frame with an EMPTY item diff still repaints: the idle-skip
            // arm is guarded by six more conditions, and any one of them turns
            // "nothing changed" into work.
            eprintln!(
                "[HLDMG-GATE] needs_resize={} resize_damage={} has_scroll={} vview={} \
                 gpu_rects={} gpu_full={} zombie={} patch_moved={}",
                needs_resize,
                resize_damage.len(),
                has_scroll,
                has_vview_damage,
                gpu_damage.rects.len(),
                gpu_damage.needs_full,
                zombie_damage.len(),
                patch_moved_union.is_some(),
            );
        }
        // Views whose content scrolled: moved (or damaged) after the target
        // is acquired, and no frame to skip.
        let view_shifts = match &self.previous_display_list {
            Some(old_dl) if can_reuse_previous_frame => cpurender::collect_virtual_view_shifts(
                old_dl,
                display_list,
                &self.previous_scroll_offsets,
                &scroll_offsets,
            ),
            _ => Vec::new(),
        };
        // The ONE skip condition, shared with the e2e runner's twin
        // (`layout/src/e2e/cpu_backend.rs`): see `cpurender::FrameChanges`. A
        // pending patch MOVE with empty damage is NOT an idle frame - skipping
        // would swallow the translation (`layout_move`).
        let frame_is_idle = cpurender::FrameChanges {
            items: dl_damage.as_ref().is_none_or(|rects| !rects.is_empty()),
            view_shifts: !view_shifts.is_empty(),
            resized: needs_resize,
            resize_damage: !resize_damage.is_empty(),
            scrolled: has_scroll,
            view_content: has_vview_damage,
            gpu_values: has_gpu_damage,
            zombies: !zombie_damage.is_empty(),
            layout_move: patch_moved_union.is_some(),
        }
        .is_idle();
        match dl_damage {
            Some(_) if frame_is_idle => {
                // A resize is never "nothing" (`FrameChanges::resized`): skipping
                // leaves `last_frame` at the OLD dimensions while the compositor
                // is already at the new ones, so the host would publish (and
                // present) a wrongly-sized buffer. A frame whose backing store
                // changed size is never "nothing".
                //
                // Nothing changed — skip rendering entirely. (A VirtualView child
                // DOM that changed (`view_content`) keeps us out of this branch —
                // that case must still re-composite, see below.)
                self.previous_display_list = Some(display_list.clone());
                self.last_consumed_build_seq = layout_window.layout_cache.build_seq;
                self.presented_layout_seq = Some(layout_window.layout_cache.layout_seq);
                // Nothing painted: baseline keeps accumulating dropped
                // sub-pixel scroll deltas (see next_scroll_baseline above).
                self.previous_scroll_offsets = next_scroll_baseline;
                self.last_frame_damage = FrameDamage::None;
                self.last_present_damage = FrameDamage::None;
                self.last_present_moves.clear();
                self.last_present_damage_after_moves = FrameDamage::None;
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

        if std::env::var_os("AZ_PATCH_DEBUG").is_some() {
            eprintln!(
                "[HLDMG-PATH] incremental={} all_damage={}",
                is_incremental,
                all_damage.len(),
            );
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
        // A padded target (`native_target_row_padding_px`) is that many pixels
        // wider than the frame - its width is the slot's row pitch.
        let target_w = pixel_w.saturating_add(self.native_target_row_padding_px);
        let native = match self.native_target.take() {
            Some(ext) if ext.width() == target_w && ext.height() == pixel_h => Some(ext),
            Some(ext) => {
                eprintln!(
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
                    Some(mut p) => {
                        p.fill(255, 255, 255, 255);
                        p
                    }
                    None => {
                        // Same contract as the early returns at the top: a call
                        // that produced no frame must not leave stale damage.
                        self.last_frame_damage = FrameDamage::None;
                        self.last_present_damage = FrameDamage::None;
                        self.last_present_moves.clear();
                        self.last_present_damage_after_moves = FrameDamage::None;
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
        // The same as moves, for a presenter that can move its own pixels, and the boxes
        // that were moved without a move being reported (they stay present damage for it).
        let mut present_moves: Vec<cpurender::PresentMove> = Vec::new();
        let mut present_unmoved: Vec<azul_core::geom::LogicalRect> = Vec::new();
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
                let _ = moved;
                let (mover_rects, rider_rects) = layout_window
                    .layout_cache
                    .last_patch_move
                    .as_ref()
                    .map(|m| (m.mover_rects_old.clone(), m.rider_rects_new.clone()))
                    .unwrap_or_default();
                // The whole blit (per-mover memmove, LCD fringe strips, the
                // fringe-touching-runs rule) lives in cpurender so the e2e
                // twin executes the IDENTICAL path — it used to be inlined
                // here only, making every mover-blit bug untestable in the
                // harness.
                let blit = cpurender::execute_translate_blit(
                    &mut output,
                    hint,
                    exceptions,
                    &mover_rects,
                    &rider_rects,
                    display_list,
                    dpi_factor,
                );
                all_damage.extend(blit.damage);
                present_unmoved.extend(blit.present_extra.iter().copied());
                present_extra.extend(blit.present_extra);
            }
            for (scroll_id, clip, delta, offset) in &scroll_shifts {
                // One shared recipe with the e2e twin — see
                // `cpurender::execute_scroll_shift`.
                let out = cpurender::execute_scroll_shift(
                    &mut output,
                    display_list,
                    *scroll_id,
                    clip,
                    *delta,
                    *offset,
                    dpi_factor,
                    // The offsets and matrices the clip was projected with.
                    &scroll_offsets,
                    &self.previous_gpu_values.transforms,
                );
                all_damage.extend(out.damage);
                present_extra.extend(out.present_extra);
                present_moves.extend(out.moves);
            }
            // A view whose content scrolled moves the same way: its pixels, the
            // strip that came into view, what is painted over it.
            for shift in &view_shifts {
                let out = cpurender::execute_virtual_view_shift(
                    &mut output,
                    display_list,
                    &vview_dls,
                    shift,
                    dpi_factor,
                    &scroll_offsets,
                );
                all_damage.extend(out.damage);
                present_extra.extend(out.present_extra);
                present_moves.extend(out.moves);
            }

            // #32 pool-order target (a native ARGB8888 slot the commit swizzle
            // converts in place): everything MOVED above came from a committed
            // slot and is still in pool byte order, while the commit swizzle
            // will convert the whole presented area. Convert what was moved
            // back to renderer order here - ONCE over the union, because two
            // moves that overlap (nested scrollers, a layout blit crossing a
            // scroll clip) would otherwise convert their overlap twice and
            // paint it with R and B swapped. The exposed strips are repainted
            // right after this, so including them is harmless.
            if self.rendered_native && self.native_target_pool_order && !present_extra.is_empty() {
                let (bw, bh) = (output.width(), output.height());
                let moved = cpurender::logical_rects_to_buffer(&present_extra, dpi_factor, bw, bh);
                cpurender::swap_rb_in_rects(
                    output.data_mut(),
                    bw as usize * 4,
                    bh as usize,
                    &moved,
                );
            }
        }

        // Merge duplicates/overlaps accumulated from the independent damage
        // sources (DL diff, vview, strips, overlay-after-shift): the renderer
        // merges overlapping rects internally anyway, but the recorded
        // paint/present damage (and the pixel-count metric built on it) must
        // not double-count the same region.
        if is_incremental {
            if std::env::var_os("AZ_PATCH_DEBUG").is_some() {
                eprintln!("[HLDMG-FINAL] before merge {all_damage:?}");
            }
            cpurender::coalesce_damage_rects(&mut all_damage);
        }

        // Per-frame paint-damage AREA into telemetry: the one number that
        // says whether a scroll frame is thin strips or a full-window
        // repaint. dpi^2 converts logical rects to physical px.
        #[cfg(feature = "telemetry")]
        {
            let px: f64 = all_damage
                .iter()
                .map(|r| f64::from(r.size.width) * f64::from(r.size.height))
                .sum::<f64>()
                * f64::from(dpi_factor)
                * f64::from(dpi_factor);
            crate::telemetry::record_paint_damage_pixels(px);
        }

        // Build render state from the GPU value caches (opacity/transform/colour, of
        // every DOM) + scroll offsets — the SAME construction the real X11/Wayland CPU
        // paths use, so this render_frame is reusable by them. The headless harness has
        // empty GPU caches, so this is equivalent to `new(scroll_offsets)` there.
        // `vview_dls` (the nested VirtualView child DOM display lists — e.g. the
        // MapWidget's tile grid) was built earlier for the child-DOM damage diff;
        // it's handed to the renderer here so the CPU `VirtualView` arm can
        // composite them. Without this the CPU backend only drew a placeholder.
        if std::env::var("AZ_MAP_DEBUG").is_ok() {
            let summary: Vec<(usize, usize)> = vview_dls
                .iter()
                .map(|(id, dl)| (id.inner, dl.items.len()))
                .collect();
            let all_ids: Vec<usize> = layout_window
                .layout_results
                .keys()
                .map(|k| k.inner)
                .collect();
            eprintln!(
                "[cpu-vview] render_frame: layout_results ids={:?}, vview_dls (id,items)={:?}",
                all_ids, summary
            );
            // Item-kind census of the ROOT display list being rendered + whether
            // the maps header's #2b2b2b background rect made it in.
            use crate::solver3::display_list::DisplayListItem as I;
            let mut rects = 0;
            let mut texts = 0;
            let mut vviews = 0;
            let mut other = 0;
            let mut dark_rect = false;
            for it in display_list.items.iter() {
                match it {
                    I::Rect { color, .. } => {
                        rects += 1;
                        if color.r == 0x2b && color.g == 0x2b && color.b == 0x2b {
                            dark_rect = true;
                        }
                    }
                    I::Text { .. } | I::TextLayout { .. } => texts += 1,
                    I::VirtualView { .. } | I::VirtualViewPlaceholder { .. } => vviews += 1,
                    _ => other += 1,
                }
            }
            eprintln!(
                "[cpu-vview] ROOT DL census: total={} rects={} texts={} vviews={} other={} \
                 header_dark_rect={}",
                display_list.items.len(),
                rects,
                texts,
                vviews,
                other,
                dark_rect
            );
            // One-shot full item dump (first frame only): every Push/Pop with
            // bounds — the header is dropped by SOMETHING among these.
            use std::sync::atomic::{AtomicBool, Ordering as AOrd};
            static DUMPED_ITEMS: AtomicBool = AtomicBool::new(false);
            if !DUMPED_ITEMS.swap(true, AOrd::Relaxed) {
                for (i, it) in display_list.items.iter().enumerate() {
                    let desc = match it {
                        I::Rect { color, bounds, .. } => format!(
                            "Rect rgb({},{},{}) {:?}",
                            color.r,
                            color.g,
                            color.b,
                            bounds.inner()
                        ),
                        I::Text { .. } => "Text".to_string(),
                        I::TextLayout { .. } => "TextLayout".to_string(),
                        I::VirtualView { bounds, .. } => format!("VView {:?}", bounds.inner()),
                        I::VirtualViewPlaceholder { bounds, .. } => {
                            format!("VViewPh {:?}", bounds.inner())
                        }
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
        // The values of every DOM paint, a view's child's included: the values this frame's
        // damage was diffed from, which `previous_gpu_values` holds now.
        // The values are MOVED into the render state and back when the frame is painted (the
        // frame's own copy of three maps was 299 KB per frame for a map view: A9 H5). Nothing
        // between here and there reads `previous_gpu_values`.
        let render_state = cpurender::CpuRenderState::new(render_offsets.clone())
            .with_gpu_values(core::mem::take(&mut self.previous_gpu_values))
            .with_system_style(layout_window.system_style.clone())
            .with_clear_color(clear_color)
            .with_virtual_view_display_lists(vview_dls)
            .with_retained_frames(Arc::clone(&self.retained_frames))
            .with_root_display_list(Arc::clone(display_list));
        // The kept group surfaces of lists this frame does not paint any more (a view that
        // rendered again, a list that was patched) are dropped, and those lists let go of.
        let painted_lists: Vec<usize> = core::iter::once(Arc::as_ptr(display_list) as usize)
            .chain(
                render_state
                    .virtual_view_display_lists
                    .values()
                    .map(|list| Arc::as_ptr(list) as usize),
            )
            .collect();
        // A kept surface whose list was replaced by one that differs in a few items (a tile
        // that arrived during a zoom, a label that was edited) is repaired, not dropped.
        if let Err(e) = cpurender::refresh_retained_surfaces(
            &render_state,
            renderer_resources,
            &layout_window.font_manager,
            &mut self.glyph_cache,
            dpi_factor,
        ) {
            eprintln!("[CpuBackend] refresh_retained_surfaces error: {e}");
        }
        self.retained_frames.begin_frame(&painted_lists);

        // An INCREMENTAL frame paints exactly its damage - and nothing when it
        // has none: the target already holds this frame (a scroll step that
        // rounds to zero device pixels, a GPU-only value change that moved
        // nothing). Choosing the branch by "damage is non-empty" sent such a
        // frame into the FULL repaint below while it still reported empty
        // damage; on a native ARGB8888 commit-swizzle pool that rewrote the
        // whole buffer in renderer byte order and nothing converted it (the
        // UI turned orange after a scroll), and everywhere else it was a
        // wasted full repaint at the tail of every smooth scroll.
        // What the FRAME damaged, as asked for. The rasteriser paints
        // overlapping rects as their bounding box - an L-shaped diagonal pan
        // is one box, not two strips - and every pixel it wrote has to be
        // presented and, on an ARGB8888 pool, converted. That is a fact about
        // the WRITE, not about what changed: reporting it as the frame's
        // damage told everything downstream that a diagonal pan had repainted
        // the whole scrollport.
        let requested_damage = all_damage.clone();
        if is_incremental {
            if !all_damage.is_empty() {
                // Incremental: render only damaged regions
                if let Ok(painted) = cpurender::render_display_list_damaged(
                    display_list,
                    &mut output,
                    dpi_factor,
                    renderer_resources,
                    &layout_window.font_manager,
                    &mut self.glyph_cache,
                    &render_state,
                    &all_damage,
                ) {
                    // What was WRITTEN, not what was requested: overlapping
                    // damage rects are painted as their bounding box, and the
                    // frame must present (and, on an ARGB8888 pool, convert)
                    // every pixel it wrote.
                    all_damage = painted;
                }
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
            }
        } else {
            // Full render
            output.fill(
                clear_color[0],
                clear_color[1],
                clear_color[2],
                clear_color[3],
            );
            compositor.allocate_layers_from_display_list(
                display_list,
                dpi_factor,
                &render_state.transforms,
                &render_state.opacities,
            );
            if let Err(e) = compositor.render_layers(
                display_list,
                dpi_factor,
                renderer_resources,
                &layout_window.font_manager,
                &mut self.glyph_cache,
                &render_state,
            ) {
                eprintln!("[CpuBackend] render_layers error: {e}");
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

        // What this frame's passes painted of a group that may only be moved next frame is
        // kept from here on (`cpurender::retained`).
        self.retained_frames.commit();

        // AZ_DUMP_DL_DIR=/tmp/dls dumps the first display lists as JSON —
        // splits "the DL's clip is collapsed" (layout/reconcile bug) from
        // "the DL is fine, the renderer clips wrong" (cpurender bug).
        if let Ok(dir) = std::env::var("AZ_DUMP_DL_DIR") {
            use std::sync::atomic::{AtomicUsize, Ordering};
            static DL_N: AtomicUsize = AtomicUsize::new(0);
            let n = DL_N.fetch_add(1, Ordering::Relaxed);
            if n < 8 {
                let _ = std::fs::create_dir_all(&dir);
                let _ = std::fs::write(
                    format!("{dir}/dl_{n:03}.json"),
                    display_list.to_debug_json(),
                );
            }
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
                        format!(
                            "{}/frame_{:03}_{}.png",
                            dir,
                            n,
                            if is_incremental { "inc" } else { "full" }
                        ),
                        bytes,
                    );
                }
            }
        }

        // Back from the render state: the values this frame painted with, for the next
        // frame's diff.
        let cpurender::CpuRenderState {
            transforms,
            opacities,
            colors,
            ..
        } = render_state;
        self.previous_gpu_values = cpurender::WindowGpuValues {
            transforms,
            opacities,
            colors,
        };
        self.previous_zombie_rects = zombie_rects;
        self.previous_display_list = Some(display_list.clone());
        self.last_consumed_build_seq = layout_window.layout_cache.build_seq;
        self.presented_layout_seq = Some(layout_window.layout_cache.layout_seq);
        // Full render paints EVERY frame at its current offset → baseline is
        // the current offsets. Incremental: only shifted frames advanced.
        self.previous_scroll_offsets = if is_incremental {
            next_scroll_baseline
        } else {
            scroll_offsets.clone()
        };
        // The window's shape, from what this frame painted (every frame:
        // the content decides; a popover's arrow may move).
        self.last_shape = if self.shape_from_alpha {
            Some(cpurender::alpha_shape_rects(
                &output,
                cpurender::SHAPE_ALPHA_THRESHOLD,
            ))
        } else {
            None
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
            FrameDamage::Rects(requested_damage)
        } else {
            FrameDamage::Full
        };
        // Present damage = paint damage ∪ the full clips that were pixel-shifted
        // (their content moved on screen even though only a strip was repainted).
        self.last_present_damage_after_moves = if is_incremental {
            let mut present = all_damage.clone();
            present.extend(present_unmoved);
            FrameDamage::Rects(present)
        } else {
            FrameDamage::Full
        };
        self.last_present_moves = if is_incremental { present_moves } else { Vec::new() };
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


#[cfg(all(test, feature = "std"))]
mod tests {
    use std::path::{Path, PathBuf};

    /// The files under `dir` (recursively) that declare a type named `CpuBackend`.
    fn declaring_files(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                declaring_files(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let source = std::fs::read_to_string(&path).unwrap_or_default();
                let declares = source.lines().any(|line| {
                    let line = line.trim_start();
                    line.starts_with("pub struct CpuBackend")
                        || line.starts_with("pub(crate) struct CpuBackend")
                        || line.starts_with("pub(super) struct CpuBackend")
                        || line.starts_with("struct CpuBackend")
                });
                if declares {
                    out.push(path);
                }
            }
        }
    }

    /// The CPU frame is written once. It used to be written twice - the dll's and a port in the E2E
    /// runner - and every damage rule had to be put into both and tested in both; a copy of it
    /// anywhere (layout or dll) is a second renderer the shells do not run.
    #[test]
    fn there_is_one_cpu_backend_in_the_tree() {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut found = Vec::new();
        declaring_files(&manifest.join("src"), &mut found);
        // The dll sits next to this crate in the workspace; a packaged crate has no neighbour.
        declaring_files(&manifest.join("../dll/src"), &mut found);
        let shown: Vec<String> = found
            .iter()
            .map(|p| p.strip_prefix(manifest).unwrap_or(p).display().to_string())
            .collect();
        assert_eq!(shown, ["src/cpu_backend.rs"], "one declaration of `CpuBackend`: {shown:?}");
    }
}
