//! The CPU frame: what every CPU-drawn window of every shell paints and presents.
//!
//! `CpuBackend` has a similar *purpose* to the `WebRender` pipeline (render-api, renderer,
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
    cpurender,
    window::{FrameDamage, LayoutWindow},
    window_state::FullWindowState,
};

// ---------------------------------------------------------------------------
// CpuBackend — replaces WebRender in headless / CPU-only windows
// ---------------------------------------------------------------------------

/// CPU-based rendering backend that replaces the `WebRender` pipeline.
///
/// In the GPU path every window holds a `WrRenderApi` (for submitting
/// display-lists, registering fonts/images), a `webrender::Renderer`
/// (for rasterising on the GPU) and an `AsyncHitTester` (for spatial
/// queries).  `CpuBackend` fills the same role with a much simpler,
/// fully CPU-based implementation:
///
/// | GPU path               | `CpuBackend` equivalent                       |
/// |------------------------|---------------------------------------------|
/// | `WrRenderApi`          | not needed – fonts/images stay in `LayoutWindow` |
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
#[must_use] 
// the flags of one frame, named at the call
#[allow(clippy::fn_params_excessive_bools)]
pub const fn frame_may_reuse_previous_pixels(
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

/// May a frame that has damage repaint that damage alone (the flat walk over its rects)?
///
/// Only while the previous frame's pixels can be reused (`can_reuse_previous_frame`), and not
/// for a list with a group only the layered compositor paints right - a blur or another
/// filter the walk does not apply, a backdrop filter (`cpurender::full_repaint_needs_layers`):
/// the walk paints such a group UNFILTERED inside every rect it repaints, so "damaged ==
/// full" does not hold for it (A1-38, A2-cpu-R08: a caret blinking in a blurred panel left a
/// sharp patch per blink). Not for the frame after such a list either: what its layers put
/// on screen is not what the walk would have. Those frames are full repaints.
#[must_use]
pub fn frame_may_repaint_its_damage_alone(
    can_reuse_previous_frame: bool,
    list: &crate::solver3::display_list::DisplayList,
    previous: Option<&crate::solver3::display_list::DisplayList>,
) -> bool {
    can_reuse_previous_frame
        && !cpurender::full_repaint_needs_layers(list)
        && !previous.is_some_and(cpurender::full_repaint_needs_layers)
}

/// What a frame reports for presenting it by moves: the moves, and the rects to copy from the
/// frame after them. The invariant: the previous frame, the moves in order, then those rects
/// copied, is the new frame.
///
/// - A frame that is not incremental: no moves, everything.
/// - A frame whose pixels were also moved by the layout-translation blit (`blit_boxes`: what
///   `execute_translate_blit` moved, not empty then): NO moves, and every rect that was
///   written or moved (`moved_or_painted`, the plain present damage). The blit runs before
///   the reported moves and is not one of them, so a reported move carried blitted pixels,
///   which the presenter's copy of the previous frame does not hold: a panel moved down 5 px
///   by the blit under a page scrolled by 8 came out with 240 px of the old panel
///   (A2-cpu-R02).
/// - Otherwise: the moves, and the damage with the boxes that were moved by something else
///   than a reported move (none, without a blit).
#[must_use]
pub fn present_by_moves(
    is_incremental: bool,
    damage: &[azul_core::geom::LogicalRect],
    blit_boxes: Vec<azul_core::geom::LogicalRect>,
    moved_or_painted: &[azul_core::geom::LogicalRect],
    moves: Vec<cpurender::PresentMove>,
) -> (Vec<cpurender::PresentMove>, FrameDamage) {
    if !is_incremental {
        return (Vec::new(), FrameDamage::Full);
    }
    let mut present = damage.to_vec();
    if blit_boxes.is_empty() {
        (moves, FrameDamage::Rects(present))
    } else {
        present.extend_from_slice(moved_or_painted);
        (Vec::new(), FrameDamage::Rects(present))
    }
}

/// How many of the debug dumps one backend has written: what `AZ_DUMP_DL_DIR` (the first display
/// lists as JSON: splits "the list's clip is collapsed" from "the list is fine, the renderer
/// clips wrong") and `AZ_DUMP_FRAME_DIR` (every rendered frame as a numbered PNG: splits
/// "rendered wrong" from "presented wrong") ask of it. WHERE they go is the window's setting
/// (`RenderSettings::dump_display_lists`, `RenderSettings::dump_frames`); the counts are this
/// backend's: a dump is limited to the first few frames of a window, not of the process.
#[derive(Debug, Clone, Copy, Default)]
pub struct DebugDumps {
    /// Display lists dumped so far.
    lists: usize,
    /// Frames dumped so far.
    frames: usize,
    /// Whether the one-shot item dump of `AZ_MAP_DEBUG` was printed.
    items_printed: bool,
}

impl DebugDumps {
    /// Most display lists a backend dumps.
    const MAX_LISTS: usize = 8;
    /// Most frames a backend dumps.
    const MAX_FRAMES: usize = 40;

    /// A backend that has dumped nothing yet.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            lists: 0,
            frames: 0,
            items_printed: false,
        }
    }
}

// plain flags, set and read one by one: a state enum would not be simpler
#[allow(clippy::struct_excessive_bools)]
// holds renderer state without a `Debug` of its own
#[allow(missing_debug_implementations)]
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
    pub last_shape: Option<Vec<cpurender::ShapeRect>>,
    /// The shape the backend last handed to the OS, so an unchanged frame
    /// shape does not re-issue the (not free) OS call.
    pub applied_shape: Option<Vec<cpurender::ShapeRect>>,
    /// Last rendered pixmap (if CPU rendering is enabled).
    /// `None` when rendering is disabled (layout-only mode).
    pub last_frame: Option<cpurender::AzulPixmap>,
    /// Retained compositor state with per-layer pixbufs. Made by the first full repaint that
    /// needs layers (`cpurender::full_repaint_needs_layers`), `None` until then and after a
    /// resize.
    pub compositor: Option<cpurender::CompositorState>,
    /// The frame's size in device pixels at the last `render_frame`, to tell a resize.
    pub last_frame_px: Option<(u32, u32)>,
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
    /// `REGION_PLAN)`: small paint region, larger present region. Equals the paint
    /// damage when nothing was pixel-shifted.
    pub last_present_damage: FrameDamage,
    /// The pixels the most recent `render_frame` MOVED inside the frame instead of painting
    /// them (a scroll, a pan of a view), in the order it moved them
    /// (`cpurender::PresentMove`: `new[p] = old[p - (dx, dy)]` for every device pixel `p` of
    /// `dst`). A presenter that can move the pixels it already shows applies these and THEN
    /// presents `last_present_damage_after_moves`, which wins where it overlaps a `dst`; one
    /// that cannot ignores both and presents `last_present_damage`, which covers the moved
    /// clips whole. Empty for a full repaint and for a frame that moved nothing.
    pub last_present_moves: Vec<cpurender::PresentMove>,
    /// What a presenter that applied `last_present_moves` still has to present: the paint
    /// damage (the strips that came into view, what is painted over the moved content, every
    /// other change of the frame) and the boxes of moves that are not reported as moves (the
    /// layout translation blit). A pan of a 2510 x 1402 view by 12 px is 18,226 px here and
    /// 3.5 M px in `last_present_damage`.
    pub last_present_damage_after_moves: FrameDamage,
    /// Arc ptr of the display list the round-3 patch SHIFT was last applied
    /// for. A buffers-held retry re-runs `render_frame` with the SAME list —
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
    pub native_target: Option<cpurender::AzulPixmap>,
    /// True when the LAST `render_frame` drew into a `native_target` — the
    /// shell branches its present on this (the pixels are already in the
    /// platform buffer; there is nothing to copy).
    pub rendered_native: bool,
    /// #32: the armed native target is a POOL-ORDER (B,G,R,A) buffer — an
    /// ARGB8888 `wl_shm` slot whose commit swizzles the damage rects. In-target
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
    /// Scroll offsets from the previous frame (`scroll_id` → (x,y)). Used to detect
    /// scroll-offset changes and damage the affected frame's viewport so its
    /// content re-renders at the new offset (#13 — the display list is unchanged
    /// on scroll, so the diff alone only catches the scrollbar).
    pub previous_scroll_offsets: cpurender::ScrollOffsetMap,
    /// Previous frame's `VirtualView` child-DOM display lists (keyed by child
    /// `DomId`). The parent display list's `VirtualView` item is unchanged when
    /// only the child re-renders (async tile writeback, etc.), so the parent-DL
    /// diff can't see it. Comparing child DLs frame-to-frame lets `render_frame`
    /// damage the `VirtualView` region when its content changed — otherwise the
    /// "nothing changed → skip" path freezes async `VirtualView` content. Without
    /// this, the `MapWidget` showed only the placeholder grid on backends (Wayland)
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
    pub previous_gpu_values: cpurender::WindowGpuValues,
    /// The write generation of the window's GPU values (`GpuStateManager::value_generation`)
    /// that `previous_gpu_values` was collected at; `None` while the backend holds no values
    /// it could use again (before its first frame, and while a frame's render state has
    /// them). A frame that finds this generation again, with the display lists of the frame
    /// before, reads no value: `previous_gpu_values` are its values (A6-25).
    pub previous_gpu_values_generation: Option<u64>,
    /// Where zombie exits painted LAST frame (logical px) — see the
    /// zombie-damage computation in `render_frame`.
    pub previous_zombie_rects: Vec<azul_core::geom::LogicalRect>,
    /// The group surfaces kept between frames (`cpurender::retained`): the picture of a turned
    /// or scaled reference frame whose content did not change is composited again instead of
    /// painted again. Handed to every incremental frame's render state.
    pub retained_frames: Arc<cpurender::RetainedFrames>,
    /// (A10-06) The shell composites layers and asks for the layer present
    /// (`cpurender::present_layer`): the picture of one reference frame of the window (the
    /// largest whose picture can be kept, a map view's) is not painted into the window's
    /// frame but reported as `last_present_layer`, for the shell to show over the frame.
    /// `false` (the default, and every shell but one that was asked to): nothing of it
    /// runs, and every frame is painted and reported as before.
    pub promote_for_present: bool,
    /// (A10-06) The layer the frame on screen is shown with, `None` when no frame is
    /// promoted: the window's frame alone is the picture then. A call that paints nothing
    /// because nothing changed leaves it as it is.
    pub last_present_layer: Option<cpurender::present_layer::PresentLayer>,
    /// (A10-06) The plan `last_present_layer` was painted with: what the next frame is
    /// compared with (does the promotion start, stop, or go on), and the key of the surface.
    pub promoted: Option<cpurender::present_layer::LayerPlan>,
    /// (A10-06) Frames to pass before a frame is promoted again, after a promoted frame's
    /// surface was not complete when its frame was painted.
    pub present_layer_backoff: u32,
    /// How many of the debug dumps this backend has written (`AZ_DUMP_DL_DIR`,
    /// `AZ_DUMP_FRAME_DIR`; where they go is the window's `RenderSettings`).
    pub debug_dumps: DebugDumps,
    /// How many bound GPU values (transforms, opacities and colours) this backend's frames have
    /// collected from the window's caches for their own use: a frame whose values and lists are
    /// the frame before's collects none (the collection a debug build makes to check that is not
    /// counted). A number the backend keeps, so that a test pins the work of its own frames.
    pub gpu_values_collected: usize,
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
    #[must_use] 
    pub fn for_config(config: &azul_core::resources::AppConfig) -> Self {
        #[allow(unused_mut)]
        let mut backend = Self::new();
        backend
            .glyph_cache
            .set_text_aa(config.text_antialiasing.into());
        backend
    }

    #[must_use] 
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
            last_frame_px: None,
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
            previous_scroll_offsets: cpurender::ScrollOffsetMap::new(),
            previous_vview_dls: BTreeMap::new(),
            previous_gpu_values: cpurender::WindowGpuValues::default(),
            previous_gpu_values_generation: None,
            previous_zombie_rects: Vec::new(),
            retained_frames: Arc::new(cpurender::RetainedFrames::new()),
            promote_for_present: false,
            last_present_layer: None,
            promoted: None,
            present_layer_backoff: 0,
            debug_dumps: DebugDumps::new(),
            gpu_values_collected: 0,
        }
    }

    /// Let go of everything this backend keeps only to make frames cheaper, on the thread
    /// that paints its frames: the kept group surfaces, the kept halos and turned text
    /// pictures, the layered compositor. For memory pressure and for a window that is not
    /// shown. The frame on screen (`last_frame`) and the window's glyph cache stay (the band
    /// threads of a repaint keep no cache of their own between frames). The next frame is
    /// correct and costs what a first frame costs for what was dropped: a zoom step paints
    /// its picture once, a repaint draws the halos and turned labels it shows.
    pub fn release_caches(&mut self) {
        self.retained_frames.clear();
        cpurender::release_cpu_halos(&mut self.glyph_cache);
        self.compositor = None;
        // (A10-06) A promoted frame's picture was in the kept surface alone: not in the
        // window's frame. The next frame is painted in full (and is never skipped as idle),
        // and promotes again; until then there is no layer.
        if self.promoted.take().is_some() {
            self.last_present_layer = None;
            self.force_full_repaint = true;
        }
    }

    /// (A10-06) The surface `last_present_layer` shows (RGBA8 premultiplied, top row first,
    /// tight rows, `surface_size` px), handed to `read`. `None` without a layer. The surface
    /// is the renderer's: `read` copies what it keeps, and nothing of it is held after the
    /// call (a surface held elsewhere is replaced, not completed, by the next part painted).
    pub fn with_present_layer_surface<R>(
        &self,
        read: impl FnOnce(&cpurender::AzulPixmap) -> R,
    ) -> Option<R> {
        self.last_present_layer.as_ref()?;
        let plan = self.promoted.as_ref()?;
        let (surface, _) = self.retained_frames.kept(&plan.key)?;
        Some(read(&surface))
    }

    /// Read the window's transparency / shape flags off the window state
    /// before a frame: what the frame is cleared to, and whether its shape
    /// is computed. Every backend's CPU present calls this first.
    pub const fn sync_window_flags(&mut self, state: &FullWindowState) {
        self.transparent = !matches!(
            state.flags.background_material,
            azul_core::window::WindowBackgroundMaterial::Opaque
        );
        self.shape_from_alpha = self.transparent;
    }

    /// After a present: the frame's shape if it differs from the one the
    /// OS has (recorded as applied). `None` = nothing to do. A frame that
    /// painted nothing opaque keeps the OS shape (the window does not vanish).
    pub fn take_changed_shape(&mut self) -> Option<Vec<cpurender::ShapeRect>> {
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
    ///
    /// What the frame does is counted for the window's frame report
    /// ([`crate::cpu_counters`]): the counters of this backend's glyph cache, which paints the
    /// frame, start at zero, and what they hold when it is done - with the scans of the lists'
    /// indexes and the kept surfaces dropped, which the painting does not count - is the
    /// frame's, kept by `layout_window`.
    pub fn render_frame(
        &mut self,
        layout_window: &LayoutWindow,
        renderer_resources: &RendererResources,
        width: f32,
        height: f32,
        dpi_factor: f32,
    ) -> Vec<azul_core::geom::LogicalRect> {
        use crate::cpu_counters::Counter;
        self.glyph_cache.counts().reset();
        // (Drops before this frame are not this frame's.)
        let _ = self.retained_frames.take_invalidated();
        let lists = self.lists_scanned(layout_window);
        let damage =
            self.render_frame_counted(layout_window, renderer_resources, width, height, dpi_factor);
        let scanned: u64 = lists
            .iter()
            .map(|(list, before)| list.index_items_scanned().saturating_sub(*before))
            .sum();
        let counts = self.glyph_cache.counts();
        counts.add(Counter::ScanItems, scanned);
        counts.add(Counter::RetainedInvalidations, self.retained_frames.take_invalidated());
        layout_window.record_cpu_counters(counts.take());
        damage
    }

    /// The lists a frame is charged the scans of their frame indexes for
    /// ([`Counter::ScanItems`](crate::cpu_counters::Counter::ScanItems)): the window's own and its
    /// views' and the ones the frame before painted (its diff reads their indexes too), each once,
    /// with how many items the scans that made its indexes had read when the frame began. An
    /// index is made once for a list's items (`DisplayList::frame_index`), so what the frame owes
    /// is what the lists' counts rose by while it ran.
    fn lists_scanned(
        &self,
        layout_window: &LayoutWindow,
    ) -> Vec<(Arc<crate::solver3::display_list::DisplayList>, u64)> {
        let mut lists: Vec<(Arc<crate::solver3::display_list::DisplayList>, u64)> = Vec::new();
        let candidates = layout_window
            .layout_results
            .values()
            .map(|result| &result.display_list)
            .chain(self.previous_display_list.iter())
            .chain(self.previous_vview_dls.values());
        for list in candidates {
            if !lists.iter().any(|(held, _)| Arc::ptr_eq(held, list)) {
                lists.push((Arc::clone(list), list.index_items_scanned()));
            }
        }
        lists
    }

    /// [`Self::render_frame`] without the counting around it.
    fn render_frame_counted(
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

        // Every early return below must leave `last_frame_damage` /
        // `last_present_damage` describing THIS call ("nothing changed"), not
        // whatever the previous call recorded. The platform blit paths read
        // `last_present_damage` after every render_frame — with the fields
        // left stale, a call that rendered nothing (window minimised to 0×0,
        // no layout result yet, allocation failure) made the caller re-blit
        // the PREVIOUS frame's damage rects out of the retained pixmap.

        // Get the layout result from layout results
        let dom_id = DomId { inner: 0 };
        let Some(result) = layout_window.layout_results.get(&dom_id) else {
            self.last_frame_damage = FrameDamage::None;
            self.last_present_damage = FrameDamage::None;
            self.last_present_moves.clear();
            self.last_present_damage_after_moves = FrameDamage::None;
            return Vec::new();
        };
        let display_list = &result.display_list;
        // What the window's settings ask of this frame: the paint switches (which travel with
        // the render state and the damage passes), the traces of the display-list patching, the
        // thread budget, the kept surfaces' bound, the layered full repaint and the dumps.
        let render = &layout_window.settings.render;
        let paint = render.paint;
        self.hit_tester.set_trace(layout_window.settings.window.trace_hit);
        let trace_patch = layout_window.display_list_options.trace_patch;
        self.retained_frames
            .set_max_bytes(render.retained_bytes.unwrap_or(cpurender::retained::DEFAULT_MAX_BYTES));
        let paint_threads = cpurender::bands::max_threads_for(render.threads);
        self.glyph_cache.scratch.threads = paint_threads;

        // The frame's size is the shells' formula (`present_geometry::frame_size_px`, A10):
        // a shell makes its buffer with the same function from the same three numbers, so the
        // frame handed over and the buffer it lands in cannot differ by a rounding. The
        // factor is the one the shell passes: `LayoutWindow::frame_scale()`.
        let (pixel_w, pixel_h) =
            crate::window::present_geometry::frame_size_px(width, height, dpi_factor);
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
        // The layered compositor is made when a frame needs it (a full repaint of a list with
        // a filter only layers apply): its root layer is a second buffer of the window's size,
        // and a window that never needs it does not hold one.

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

        // Did the frame change its size since the last call?
        let (old_pw, old_ph) = self.last_frame_px.unwrap_or((0, 0));
        self.last_frame_px = Some((pixel_w, pixel_h));
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
                // The layers (if there are any) are made again at the new size by the frame
                // that needs them; the frame itself keeps its pixels (`resize_reuse` below).
                self.compositor = None;
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
                self.compositor = None;
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
        // (A6-25) Is anything this frame's values are made of another thing than in the frame
        // before? They are a function of the value caches of the window's DOMs - whose write
        // generation moves with every write that changes a key or a value, whoever makes it
        // (`GpuStateManager::value_generation`) - and of which DOMs there are. The lists are
        // compared by allocation: the backend holds the frame before's, so an address cannot
        // be another list's, and a list built again (also with the same values) is another.
        let value_generation = layout_window.gpu_state_manager.value_generation();
        let same_lists = self
            .previous_display_list
            .as_ref()
            .is_some_and(|old| Arc::ptr_eq(old, display_list))
            && self.previous_vview_dls.len() == vview_dls.len()
            && self
                .previous_vview_dls
                .iter()
                .zip(&vview_dls)
                .all(|((old_dom, old), (dom, list))| old_dom == dom && Arc::ptr_eq(old, list));
        let values_unchanged =
            same_lists && self.previous_gpu_values_generation == Some(value_generation);
        // `None`: nothing was written. The values of the frame before (`previous_gpu_values`)
        // are this frame's: none is read, copied or compared.
        let fresh_gpu_values = if values_unchanged {
            // A wrong skip paints stale values: a debug build collects them anyway and holds
            // the skip against them.
            debug_assert!(
                cpurender::WindowGpuValues::of_doms(
                    &layout_window.gpu_state_manager,
                    layout_window.layout_results.keys().copied(),
                ) == self.previous_gpu_values,
                "the GPU values changed and their write generation did not"
            );
            None
        } else {
            let values = cpurender::WindowGpuValues::of_doms(
                &layout_window.gpu_state_manager,
                layout_window.layout_results.keys().copied(),
            );
            self.gpu_values_collected +=
                values.transforms.len() + values.opacities.len() + values.colors.len();
            Some(values)
        };
        // (A10-06) The promoted frame's own matrix is no value of the window's frame: its
        // picture is not in the frame's pixels, and a change of the matrix moves the shell's
        // layer (`present_layer::plan` reads the new value below). The value of the frame
        // before is made this frame's before the two are compared, so that the change
        // damages nothing (`window_value_damage`) and places nothing differently in the
        // list diffs. A frame that stops promoting repaints the layer's clip whole.
        // Such a frame is not idle, though it may paint no pixel: the layer it reports is
        // another one (`promoted_frame_moved`, read where the frame is asked whether
        // anything changed).
        let mut promoted_frame_moved = false;
        if let (Some(key), Some(fresh)) = (
            self.promoted
                .as_ref()
                .filter(|_| self.promote_for_present)
                .map(|plan| plan.transform_key),
            fresh_gpu_values.as_ref(),
        ) {
            let now = fresh.transforms.get(&key).copied();
            promoted_frame_moved = now != self.previous_gpu_values.transforms.get(&key).copied();
            match now {
                Some(value) => {
                    self.previous_gpu_values.transforms.insert(key, value);
                }
                None => {
                    self.previous_gpu_values.transforms.remove(&key);
                }
            }
        }
        let gpu_values: &cpurender::WindowGpuValues =
            fresh_gpu_values.as_ref().unwrap_or(&self.previous_gpu_values);
        // Which frames scroll by whole device pixels this frame, and the offsets an incremental
        // frame PAINTS at (`next_scroll_baseline`: a frame whose delta is under half a device
        // pixel keeps the offset it was last painted at). Found before the damage, because the
        // damage has to land where the frame paints (A2-cpu-20): placed with the real offsets
        // it was up to half a device pixel off, and a column of a changed item inside such a
        // frame could stay stale.
        let scroll_shifts = cpurender::collect_scroll_shifts(
            display_list,
            &scroll_offsets,
            &self.previous_scroll_offsets,
            // The matrices this frame paints with (they become `previous_gpu_values` below).
            &gpu_values.transforms,
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

        // In VIEWPORT space: an item inside a scrolled frame (a thumb on a
        // scrolled page) is repainted where the frame paints it. A colour fade
        // frame - the bound background rects whose colour moved - is part of it.
        let gpu_damage = if values_unchanged {
            // The same values: no key changed, nothing is owed (what `window_value_damage`
            // answers after comparing every value).
            cpurender::GpuValueDamage::default()
        } else {
            cpurender::window_value_damage(
                display_list,
                &vview_dls,
                &self.previous_gpu_values,
                gpu_values,
                &next_scroll_baseline,
            )
        };
        self.glyph_cache
            .counts()
            .add(crate::cpu_counters::Counter::ScanItems, gpu_damage.scanned_items);
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
        // `None`: nothing was written; the values stay where they are and are the frame
        // before's as well.
        let old_gpu_values: Option<cpurender::WindowGpuValues> = fresh_gpu_values
            .map(|values| core::mem::replace(&mut self.previous_gpu_values, values));
        self.previous_gpu_values_generation = Some(value_generation);

        // A frame tap that has not heard a frame of this size has no picture to lay rectangles over: this
        // frame is made, and made whole, even if the window changed nothing (a recording that starts on
        // a window that is at rest).
        if layout_window
            .frame_tap
            .as_ref()
            .is_some_and(|tap| tap.needs_a_whole_frame(pixel_w, pixel_h))
        {
            self.force_full_repaint = true;
        }
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
                    &next_scroll_baseline,
                    patch_hint.as_ref().map(|(h, _)| h),
                    cpurender::FrameValues {
                        old: Some(
                            &old_gpu_values
                                .as_ref()
                                .unwrap_or(&self.previous_gpu_values)
                                .transforms,
                        ),
                        new: Some(&self.previous_gpu_values.transforms),
                        switches: paint,
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
                old: Some(
                    &old_gpu_values
                        .as_ref()
                        .unwrap_or(&self.previous_gpu_values)
                        .transforms,
                ),
                new: Some(&self.previous_gpu_values.transforms),
                switches: paint,
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
        if trace_patch {
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
        if trace_patch {
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
                &next_scroll_baseline,
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
            // (A10-06) A move of the promoted frame alone is a frame: the layer moves.
            gpu_values: has_gpu_damage || promoted_frame_moved,
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
            // Not for a list with a group only the layered compositor paints right (a blur or
            // another filter, a backdrop filter): the walk that repaints damage paints such a
            // group UNFILTERED inside every rect it repaints - a caret blinking in a blurred
            // panel left a sharp patch per blink. Those frames are full repaints through the
            // layers (A1-38); an idle frame of such a window is still skipped above.
            Some(mut rects)
                if frame_may_repaint_its_damage_alone(
                    can_reuse_previous_frame,
                    display_list,
                    self.previous_display_list.as_deref(),
                ) =>
            {
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

        if trace_patch {
            eprintln!(
                "[HLDMG-PATH] incremental={} all_damage={}",
                is_incremental,
                all_damage.len(),
            );
        }

        // (A10-06) The layer present: which frame of the window, if any, is shown by the
        // shell as a layer this frame (`cpurender::present_layer`), and what that changes of
        // the damage. Nothing of this runs unless the shell asked (`promote_for_present`).
        let old_plan = self.promoted.take();
        let mut layer_plan: Option<cpurender::present_layer::LayerPlan> = None;
        if self.promote_for_present {
            if self.present_layer_backoff > 0 {
                self.present_layer_backoff -= 1;
            } else if !(self.transparent
                // A resize and a change of the scale factor go through the window's frame
                // alone (the first frame of a window is no resize).
                || (needs_resize && old_pw > 0 && old_ph > 0)
                || dpi_changed
                // The layered compositor paints its own way.
                || cpurender::full_repaint_needs_layers(display_list)
                || self
                    .previous_display_list
                    .as_deref()
                    .is_some_and(cpurender::full_repaint_needs_layers)
                || render.full_in_layers)
            {
                // The state the frame is painted with (the offsets, the values, the lists),
                // for the plan and for the repair of kept surfaces whose list was replaced:
                // the plan asks what is kept for the lists of THIS frame.
                let offsets = if is_incremental {
                    &next_scroll_baseline
                } else {
                    &scroll_offsets
                };
                let probe = cpurender::CpuRenderState::new(offsets.clone())
                    .with_switches(paint)
                    .with_paint_threads(paint_threads)
                    .with_gpu_values(core::mem::take(&mut self.previous_gpu_values))
                    .with_system_style(layout_window.system_style.clone())
                    .with_virtual_view_display_lists(vview_dls.clone())
                    .with_retained_frames(Arc::clone(&self.retained_frames))
                    .with_root_display_list(Arc::clone(display_list));
                if let Err(e) = cpurender::refresh_retained_surfaces(
                    &probe,
                    renderer_resources,
                    &layout_window.font_manager,
                    &mut self.glyph_cache,
                    dpi_factor,
                ) {
                    eprintln!("[CpuBackend] refresh_retained_surfaces error: {e}");
                }
                layer_plan = cpurender::present_layer::plan(
                    &probe,
                    (pixel_w, pixel_h),
                    dpi_factor,
                    &self.glyph_cache,
                    &layout_window.font_manager,
                );
                let cpurender::CpuRenderState {
                    transforms,
                    opacities,
                    colors,
                    ..
                } = probe;
                self.previous_gpu_values = cpurender::WindowGpuValues {
                    transforms,
                    opacities,
                    colors,
                };
            }
        }
        // What the layer needs of its surface and is not painted (`None`: no surface is kept
        // for it), and how far ahead this frame's passes paint the surface.
        let layer_missing = layer_plan
            .as_ref()
            .and_then(|plan| self.retained_frames.missing(&plan.key, plan.needed));
        let layer_paint_margin = layer_plan.as_ref().and_then(|plan| {
            cpurender::present_layer::paint_margin_for(layer_missing.as_deref(), plan.needed)
        });
        if is_incremental {
            let same_layer = |a: &cpurender::present_layer::LayerPlan,
                              b: &cpurender::present_layer::LayerPlan| {
                a.transform_key == b.transform_key && a.clip == b.clip
            };
            if let Some(plan) = layer_plan.as_ref() {
                // What shows of the layer's surface and is not painted: its passes paint it.
                all_damage.extend(cpurender::present_layer::damage_of_plan(
                    plan,
                    layer_missing,
                    dpi_factor,
                ));
                // The promotion starts: the window's frame holds the picture under the clip.
                if !old_plan.as_ref().is_some_and(|old| same_layer(old, plan)) {
                    all_damage.push(plan.clip_rect(dpi_factor));
                }
            }
            if let Some(old) = old_plan.as_ref() {
                // The promotion stops: the window's frame holds nothing of the picture there.
                if !layer_plan.as_ref().is_some_and(|plan| same_layer(old, plan)) {
                    all_damage.push(old.clip_rect(dpi_factor));
                }
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
                None => if let Some(mut p) = cpurender::AzulPixmap::new(pixel_w, pixel_h) {
                    p.fill(255, 255, 255, 255);
                    p
                } else {
                    // Same contract as the early returns at the top: a call
                    // that produced no frame must not leave stale damage.
                    self.last_frame_damage = FrameDamage::None;
                    self.last_present_damage = FrameDamage::None;
                    self.last_present_moves.clear();
                    self.last_present_damage_after_moves = FrameDamage::None;
                    return Vec::new();
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
                let blit = cpurender::execute_translate_blit_switched(
                    &mut output,
                    hint,
                    exceptions,
                    &mover_rects,
                    &rider_rects,
                    display_list,
                    dpi_factor,
                    paint,
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
                    // The offsets and matrices the clip was projected with: the ones this
                    // frame paints at (A1-45).
                    &next_scroll_baseline,
                    &self.previous_gpu_values.transforms,
                );
                all_damage.extend(out.damage);
                present_extra.extend(out.present_extra);
                present_moves.extend(out.moves);
            }
            // A view whose content scrolled moves the same way: its pixels, the
            // strip that came into view, what is painted over it.
            for shift in &view_shifts {
                // (A10-06) The view that shows the promoted frame: its picture is not in
                // `output`, there is nothing of it to move. A view that shows the frame
                // alone moves no pixel (the pan is the layer's matrix); one that shows
                // more is repainted in its box.
                if let Some(plan) = layer_plan.as_ref().filter(|plan| plan.view == Some(shift.index)) {
                    if !plan.alone {
                        all_damage.push(shift.clip);
                    }
                    continue;
                }
                let out = cpurender::execute_virtual_view_shift_switched(
                    &mut output,
                    display_list,
                    &vview_dls,
                    shift,
                    dpi_factor,
                    // The offsets the shift's clip was collected with and the frame paints
                    // at (A1-45).
                    &next_scroll_baseline,
                    // The matrices this frame paints with.
                    &self.previous_gpu_values.transforms,
                    paint,
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
            // The damage names pixels of the frame, and only those: a shrink damages the old
            // bounds of the boxes it narrowed, which lie past the new edge. The rasteriser
            // clamps such a rect to its buffer, so the PRESENT damage below (what it wrote)
            // never held it; the PAINT damage (what was asked for) must not either, or the
            // frame reports pixels painted that are no pixels of it (`present ⊇ paint`).
            cpurender::clip_damage_to_frame(
                &mut all_damage,
                pixel_w as f32 / dpi_factor,
                pixel_h as f32 / dpi_factor,
            );
            if trace_patch {
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
        if paint.map_debug {
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
                "[cpu-vview] render_frame: layout_results ids={all_ids:?}, vview_dls (id,items)={summary:?}"
            );
            // Item-kind census of the ROOT display list being rendered + whether
            // the maps header's #2b2b2b background rect made it in.
            use crate::solver3::display_list::DisplayListItem as I;
            let mut rects = 0;
            let mut texts = 0;
            let mut vviews = 0;
            let mut other = 0;
            let mut dark_rect = false;
            for it in &display_list.items {
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
            // One-shot full item dump (first frame of this backend only): every Push/Pop with
            // bounds — the header is dropped by SOMETHING among these.
            if !core::mem::replace(&mut self.debug_dumps.items_printed, true) {
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
                            let s = format!("{other:?}");
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
        // Their write generation goes with them: until they are back the backend holds no
        // values it could use again.
        let painted_generation = self.previous_gpu_values_generation.take();
        let mut render_state = cpurender::CpuRenderState::new(render_offsets.clone())
            .with_switches(paint)
            .with_paint_threads(paint_threads)
            .with_gpu_values(core::mem::take(&mut self.previous_gpu_values))
            .with_system_style(layout_window.system_style.clone())
            .with_clear_color(clear_color)
            .with_virtual_view_display_lists(vview_dls)
            .with_retained_frames(Arc::clone(&self.retained_frames))
            .with_root_display_list(Arc::clone(display_list));
        // (A10-06) The promoted frame is painted into its surface and not composited.
        render_state.promote = layer_plan.as_ref().map(|plan| cpurender::PromoteFrame {
            transform_key: plan.transform_key,
            composite: false,
            paint_margin: layer_paint_margin,
        });
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
        // The halos this frame and the frame before use are not evicted while it paints.
        cpurender::begin_cpu_halo_frame(&mut self.glyph_cache);
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
            // The walk that repaints damage paints the full frame, too, in bands: a first
            // frame and every frame of a live resize are full repaints. The layered
            // compositor is for a list whose filters only it applies (a blur), and for
            // `AZ_CPU_FULL=layers` (the path before, for comparing the two).
            let layered = cpurender::full_repaint_needs_layers(display_list)
                || render.full_in_layers;
            if layered {
                let compositor = self
                    .compositor
                    .get_or_insert_with(|| cpurender::CompositorState::new(pixel_w, pixel_h));
                compositor.set_clear_color(clear_color);
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
                compositor.composite_frame_counted(&mut output, dpi_factor, self.glyph_cache.counts());
            } else if let Err(e) = cpurender::render_display_list_whole(
                display_list,
                &mut output,
                dpi_factor,
                renderer_resources,
                &layout_window.font_manager,
                &mut self.glyph_cache,
                &render_state,
            ) {
                eprintln!("[CpuBackend] render_display_list_whole error: {e}");
            }
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

        // (A10-06) The layer of this frame: the promoted frame's surface, if what shows of
        // it is painted now. If it is not (a part no pass could paint: not expected), the
        // frame is completed the old way - the layer's clip is painted again with the
        // surface composited - nothing is promoted for a while, and the next frame is
        // painted in full, in place.
        self.last_present_layer = None;
        if let Some(plan) = layer_plan {
            let complete = self
                .retained_frames
                .missing(&plan.key, plan.shown)
                .is_some_and(|missing| missing.is_empty());
            let stamp = if complete {
                self.retained_frames.kept(&plan.key).map(|(_, stamp)| stamp)
            } else {
                None
            };
            if let Some(stamp) = stamp {
                self.last_present_layer = Some(plan.layer(stamp));
                self.promoted = Some(plan);
            } else {
                eprintln!(
                    "[CpuBackend] layer present: the surface of the promoted frame is not complete \
                     after its frame; this frame is presented without a layer"
                );
                render_state.promote = Some(cpurender::PromoteFrame {
                    transform_key: plan.transform_key,
                    composite: true,
                    paint_margin: None,
                });
                let clip = plan.clip_rect(dpi_factor);
                if cpurender::render_display_list_damaged(
                    display_list,
                    &mut output,
                    dpi_factor,
                    renderer_resources,
                    &layout_window.font_manager,
                    &mut self.glyph_cache,
                    &render_state,
                    &[clip],
                )
                .is_ok()
                {
                    all_damage.push(clip);
                }
                self.retained_frames.commit();
                self.present_layer_backoff = 60;
                self.force_full_repaint = true;
            }
        }

        // AZ_DUMP_DL_DIR=/tmp/dls dumps the first display lists as JSON —
        // splits "the DL's clip is collapsed" (layout/reconcile bug) from
        // "the DL is fine, the renderer clips wrong" (cpurender bug).
        if let Some(dir) = render.dump_display_lists.as_deref() {
            let n = self.debug_dumps.lists;
            if n < DebugDumps::MAX_LISTS {
                self.debug_dumps.lists += 1;
                let _unused = std::fs::create_dir_all(dir);
                let _unused = std::fs::write(
                    dir.join(format!("dl_{n:03}.json")),
                    display_list.to_debug_json(),
                );
            }
        }
        // AZ_DUMP_FRAME_DIR=/tmp/frames dumps every rendered CPU frame as a
        // numbered PNG — splits "rendered wrong" from "presented wrong" when a
        // backend shows pixels that contradict the display list.
        if let Some(dir) = render.dump_frames.as_deref() {
            let n = self.debug_dumps.frames;
            if n < DebugDumps::MAX_FRAMES {
                self.debug_dumps.frames += 1;
                if let Ok(bytes) = output.encode_png() {
                    let _unused = std::fs::create_dir_all(dir);
                    let _unused = std::fs::write(
                        dir.join(format!(
                            "frame_{:03}_{}.png",
                            n,
                            if is_incremental { "inc" } else { "full" }
                        )),
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
        self.previous_gpu_values_generation = painted_generation;
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
        self.last_frame_damage = if is_incremental {
            FrameDamage::Rects(requested_damage)
        } else {
            FrameDamage::Full
        };
        // Present damage = paint damage ∪ the full clips that were pixel-shifted
        // (their content moved on screen even though only a strip was repainted).
        let (moves, after_moves) = present_by_moves(
            is_incremental,
            &all_damage,
            present_unmoved,
            &present_extra,
            present_moves,
        );
        self.last_present_damage_after_moves = after_moves;
        self.last_present_moves = moves;
        self.last_present_damage = if is_incremental {
            let mut present = all_damage.clone();
            present.extend(present_extra);
            FrameDamage::Rects(present)
        } else {
            FrameDamage::Full
        };
        // The app's frame tap hears the finished frame, and what of it changed on screen (the
        // present damage: the moved clips too). It is heard HERE, with `output` in hand: a frame
        // painted into the platform's own buffer is kept nowhere else (below).
        if let Some(tap) = &layout_window.frame_tap {
            // (A target of the platform's can be wider than the frame: its rows are padded.)
            crate::frame_tap::deliver(
                tap,
                output.data(),
                pixel_w,
                pixel_h,
                output.width() as usize * 4,
                dpi_factor,
                &self.last_present_damage,
            );
        }
        if output.is_external() {
            // #27: the pixels live in the platform's backbuffer and the shell
            // presents them from there. Retaining the borrowed wrapper would
            // dangle once the shm pool is recreated — keep nothing.
            self.last_frame = None;
        } else {
            self.last_frame = Some(output);
        }
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

#[cfg(test)]
mod audit_a1_tests {
    use azul_core::geom::{LogicalPosition, LogicalRect, LogicalSize};

    use super::*;

    fn lr(x: f32, y: f32, w: f32, h: f32) -> LogicalRect {
        LogicalRect {
            origin: LogicalPosition { x, y },
            size: LogicalSize { width: w, height: h },
        }
    }

    /// A2-cpu-R02: a frame whose pixels the layout-translation blit moved reports no moves.
    /// A 100 x 80 frame: a panel (10, 10) 30 x 20 moved down 5 by the blit (its box, rows
    /// 10 .. 35) and the page scrolled from 8 to 0 in the same frame (a move of rows 0 .. 72
    /// down by 8). Reported as a move, the scroll carried the presenter's OLD panel (rows
    /// 10 .. 30 to 18 .. 38) where the frame holds the blitted one. Reported as rects, the
    /// presenter copies the scrolled clip and the blit's box from the frame.
    /// Mutation: the moves reported whenever the frame is incremental shows 240 px of the
    /// old panel.
    #[test]
    fn a_frame_that_blits_a_layout_move_reports_no_moves() {
        let strip = lr(0.0, 0.0, 100.0, 8.0);
        let blit_box = lr(10.0, 10.0, 30.0, 25.0);
        let scrolled_clip = lr(0.0, 0.0, 100.0, 80.0);
        let moves = vec![cpurender::PresentMove { dst: (0, 8, 100, 72), dx: 0, dy: 8 }];
        let moved_or_painted = [blit_box, scrolled_clip];
        // The blit ran: no moves, and everything that was written or moved is copied.
        let (reported, after) = present_by_moves(true, &[strip], vec![blit_box], &moved_or_painted, moves.clone());
        assert!(reported.is_empty());
        assert_eq!(after, FrameDamage::Rects(vec![strip, blit_box, scrolled_clip]));
        // No blit: the moves, and the damage alone after them.
        let (reported, after) = present_by_moves(true, &[strip], Vec::new(), &[scrolled_clip], moves.clone());
        assert_eq!(reported, moves);
        assert_eq!(after, FrameDamage::Rects(vec![strip]));
        // A full repaint: no moves, everything.
        assert_eq!(present_by_moves(false, &[], Vec::new(), &[], moves), (Vec::new(), FrameDamage::Full));
    }

    /// A2-cpu-R08 (A1-38): a frame of a list with `filter: blur()` never repaints its damage
    /// alone. A 200 x 100 box with `filter: blur(4px)` and a caret over it: the full repaint
    /// blurs the box through its layer, the walk over the caret's damage rect (2 x 18) would
    /// paint that rect of the box unblurred - so the frame is a full repaint. The frame after
    /// such a list is one too. A list without such a group repaints its damage.
    /// Mutation: the gate as it was (`can_reuse_previous_frame` alone) lets the blurred list
    /// through.
    #[test]
    fn a_list_with_a_blur_never_repaints_its_damage_alone() {
        use azul_css::props::{
            basic::{pixel::PixelValue, ColorU},
            style::filter::{StyleBlur, StyleFilter},
        };

        use crate::solver3::display_list::{BorderRadius, DisplayList, DisplayListItem};
        let boxed = |blur: bool| {
            let fill = DisplayListItem::Rect {
                bounds: lr(10.0, 10.0, 200.0, 100.0).into(),
                color: ColorU { r: 200, g: 0, b: 0, a: 255 },
                border_radius: BorderRadius::default(),
            };
            let items = if blur {
                vec![
                    DisplayListItem::PushFilter {
                        bounds: lr(10.0, 10.0, 200.0, 100.0).into(),
                        filters: vec![StyleFilter::Blur(StyleBlur { width: PixelValue::px(4.0), height: PixelValue::px(4.0) })],
                    },
                    fill,
                    DisplayListItem::PopFilter,
                ]
            } else {
                vec![fill]
            };
            DisplayList { items, ..Default::default() }
        };
        let (blurred, plain) = (boxed(true), boxed(false));
        assert!(frame_may_repaint_its_damage_alone(true, &plain, Some(&plain)));
        assert!(frame_may_repaint_its_damage_alone(true, &plain, None));
        assert!(!frame_may_repaint_its_damage_alone(true, &blurred, Some(&blurred)), "the walk does not blur");
        assert!(!frame_may_repaint_its_damage_alone(true, &plain, Some(&blurred)), "the frame after it");
        assert!(!frame_may_repaint_its_damage_alone(false, &plain, Some(&plain)));
    }
}

#[cfg(test)]
#[path = "cpu_backend_test.rs"]
mod cpu_backend_test;
