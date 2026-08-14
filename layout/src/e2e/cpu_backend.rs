//! CPU render backend for the headless E2E runner.
//!
//! Verbatim port of the DLL's `desktop::shell2::headless::CpuBackend`
//! (`dll/src/desktop/shell2/headless/mod.rs`) minus the pieces that need a
//! `PlatformWindow` (the hit tester, the `AZ_MAP_DEBUG` / `AZ_DUMP_FRAME_DIR`
//! dumps). Everything it calls lives in `azul_layout::cpurender`, so the port is
//! mechanical.
//!
//! WHY THIS EXISTS: the damage assertions (`assert_changed`,
//! `assert_damage_covers_changes`, `assert_damage_incremental`,
//! `assert_idle_stable`) read `LayoutWindow::frame_report`, which is written by
//! `FrameReport::record_frame` — and the ONLY producer of the paint/present
//! damage it records is this render pass. A runner that never renders a frame
//! reports `FrameDamage::None` forever, so every damage assertion fails with
//! "nothing was repainted (stale screen)" no matter what the engine did.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use azul_core::dom::DomId;
use azul_core::geom::{LogicalRect, LogicalSize};
use azul_core::resources::RendererResources;

use azul_layout::cpurender;
use azul_layout::solver3::display_list::DisplayList;
use azul_layout::window::{FrameDamage, LayoutWindow};

/// CPU rendering backend (the headless replacement for WebRender).
///
/// Holds the retained compositor state, the previous frame's display list /
/// scroll offsets / GPU values — everything the frame-to-frame damage diff
/// needs — and the damage of the most recent `render_frame`.
pub(super) struct CpuBackend {
    /// Last rendered pixmap.
    pub(super) last_frame: Option<cpurender::AzulPixmap>,
    /// Retained compositor state with per-layer pixbufs.
    pub(super) compositor: Option<cpurender::CompositorState>,
    /// Glyph cache — persists across frames for text rendering.
    pub(super) glyph_cache: azul_layout::glyph_cache::GlyphCache,
    /// Previous display list for damage-rect computation.
    pub(super) previous_display_list: Option<Arc<DisplayList>>,
    /// PAINT damage of the most recent `render_frame` — the region actually
    /// re-rasterised.
    pub(super) last_frame_damage: FrameDamage,
    /// PRESENT damage of the most recent `render_frame` — the region that
    /// visually CHANGED on screen (⊇ paint damage; a scroll memmoves a large
    /// region but paints a strip).
    pub(super) last_present_damage: FrameDamage,
    /// Scroll offsets of the previous frame (`scroll_id` → (x,y)).
    pub(super) previous_scroll_offsets: cpurender::ScrollOffsetMap,
    /// Previous frame's `VirtualView` child-DOM display lists.
    pub(super) previous_vview_dls: BTreeMap<DomId, Arc<DisplayList>>,
    /// GPU-animated values of the previous frame, for the frame-to-frame diff.
    pub(super) previous_gpu_transforms:
        std::collections::HashMap<usize, azul_core::transform::ComputedTransform3D>,
    pub(super) previous_gpu_opacities: std::collections::HashMap<usize, f32>,
}

impl Default for CpuBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl CpuBackend {
    #[must_use]
    pub(super) fn new() -> Self {
        Self {
            last_frame: None,
            compositor: None,
            glyph_cache: azul_layout::glyph_cache::GlyphCache::new(),
            previous_display_list: None,
            last_frame_damage: FrameDamage::None,
            last_present_damage: FrameDamage::None,
            previous_scroll_offsets: cpurender::ScrollOffsetMap::new(),
            previous_vview_dls: BTreeMap::new(),
            previous_gpu_transforms: std::collections::HashMap::new(),
            previous_gpu_opacities: std::collections::HashMap::new(),
        }
    }

    /// Render the current display list into `last_frame`, recording the paint /
    /// present damage of the frame.
    ///
    /// Uses damage-rect-based incremental rendering when possible: the current
    /// display list is diffed against `previous_display_list`, and only the
    /// changed regions are repainted. Returns the damage rects that were
    /// rendered (empty = nothing changed, or a full repaint).
    #[allow(clippy::too_many_lines)]
    pub(super) fn render_frame(
        &mut self,
        layout_window: &LayoutWindow,
        renderer_resources: &RendererResources,
        width: f32,
        height: f32,
        dpi_factor: f32,
    ) -> Vec<LogicalRect> {
        let dom_id = DomId { inner: 0 };
        let Some(result) = layout_window.layout_results.get(&dom_id) else {
            return Vec::new();
        };
        let display_list = &result.display_list;

        let pixel_w = (width * dpi_factor).ceil() as u32;
        let pixel_h = (height * dpi_factor).ceil() as u32;
        if pixel_w == 0 || pixel_h == 0 {
            return Vec::new();
        }

        // Allocate or resize compositor
        let compositor = self
            .compositor
            .get_or_insert_with(|| cpurender::CompositorState::new(pixel_w, pixel_h));

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
                if let Some(root_layer) = compositor.layers.get_mut(&compositor.root_layer) {
                    let _ = root_layer.pixbuf.resize_grow_only(pixel_w, pixel_h, 255, 255, 255, 255);
                    root_layer.bounds.size = LogicalSize {
                        width: pixel_w as f32,
                        height: pixel_h as f32,
                    };
                }
                // Damage rects are LOGICAL everywhere downstream.
                resize_damage = cpurender::compute_resize_damage(
                    old_pw as f32 / dpi_factor,
                    old_ph as f32 / dpi_factor,
                    width,
                    height,
                );
            } else {
                // Shrink (or a MIXED resize — wider but shorter lands here too,
                // `is_grow` demands both axes). This branch stays a FULL
                // repaint, and that is a measured decision, not an oversight: it
                // recreates the compositor AND never calls
                // `compute_resize_damage`, so letting it reuse the previous
                // frame under-paints. Measured with the resize probe at
                // 500x600 -> 700x400: 53200 changed pixels uncovered by any
                // damage rect, the first at (500, 134) — i.e. the whole
                // newly-exposed right strip, stale on a real screen. A shrink
                // also exposes nothing new, so a full repaint here costs at most
                // the NEW (smaller) buffer.
                *compositor = cpurender::CompositorState::new(pixel_w, pixel_h);
            }
        }

        // Real scroll offsets for this frame — needed by the damage diff (items
        // inside scroll frames are stored at CONTENT coords) and by the
        // scroll-shift machinery further down.
        let scroll_offsets = layout_window
            .scroll_manager
            .build_scroll_offset_map(dom_id, &result.scroll_ids);

        // GPU-value diff: thumb position / fade opacity / transforms change
        // WITHOUT any display-list item changing (items only carry the keys).
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
        self.previous_gpu_transforms = gpu_transforms;
        self.previous_gpu_opacities = gpu_opacities;

        // Can the pixels of the previous frame still be trusted? Yes when the
        // buffer did not change size at all, and yes on a GROW (the old pixels
        // were copied over verbatim). No on a shrink / first allocation.
        let can_reuse_previous_frame = !needs_resize || resize_preserved_pixels;

        // Display-list damage (incremental path)
        let dl_damage = match &self.previous_display_list {
            Some(old_dl) if can_reuse_previous_frame && !gpu_damage.needs_full => {
                cpurender::compute_display_list_damage(
                    old_dl,
                    display_list,
                    &self.previous_scroll_offsets,
                    &scroll_offsets,
                )
            }
            _ => None, // first frame, shrink or ref-frame transform → full repaint
        };

        // VirtualView child-DOM damage.
        let vview_dls: BTreeMap<DomId, Arc<DisplayList>> = layout_window
            .layout_results
            .iter()
            .filter(|(id, _)| id.inner != dom_id.inner)
            .map(|(id, r)| (*id, r.display_list.clone()))
            .collect();
        let vview_damage = cpurender::compute_virtual_view_damage(
            display_list,
            &vview_dls,
            &self.previous_vview_dls,
        );
        let has_vview_damage = !vview_damage.is_empty();
        self.previous_vview_dls = vview_dls.clone();

        // Scroll: the display list is UNCHANGED on scroll, so the diff above
        // only ever catches the scrollbar. Collect (clip, delta) per frame whose
        // offset changed so the still-visible pixels can be MOVED and only the
        // exposed strip repainted.
        let mut scroll_shifts: Vec<(u64, LogicalRect, (f32, f32), (f32, f32))> = Vec::new();
        for (scroll_id, offset) in &scroll_offsets {
            let prev = self
                .previous_scroll_offsets
                .get(scroll_id)
                .copied()
                .unwrap_or((0.0, 0.0));
            let delta = (offset.0 - prev.0, offset.1 - prev.1);
            // Threshold in PHYSICAL pixels.
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
        // Advance the scroll baseline ONLY for frames actually painted at their
        // new offset this call, so sub-device-pixel deltas ACCUMULATE instead of
        // being swallowed frame after frame.
        let shifted_ids: BTreeSet<u64> = scroll_shifts.iter().map(|(sid, ..)| *sid).collect();
        let next_scroll_baseline: cpurender::ScrollOffsetMap = scroll_offsets
            .iter()
            .map(|(id, off)| {
                if shifted_ids.contains(id) {
                    (*id, *off)
                } else {
                    (
                        *id,
                        self.previous_scroll_offsets.get(id).copied().unwrap_or(*off),
                    )
                }
            })
            .collect();

        // Determine render path.
        let mut all_damage: Vec<LogicalRect>;
        let is_incremental;

        match dl_damage {
            Some(rects)
                if rects.is_empty()
                    && !needs_resize
                    && resize_damage.is_empty()
                    && !has_scroll
                    && !has_vview_damage
                    && !has_gpu_damage =>
            {
                // Nothing changed — skip rendering entirely.
                //
                // `!needs_resize` is load-bearing now that a resize can reach
                // this match at all: skipping leaves `last_frame` at the OLD
                // dimensions while the compositor is already at the new ones, so
                // the host would publish (and present) a wrongly-sized buffer.
                // A frame whose backing store changed size is never "nothing".
                self.previous_display_list = Some(display_list.clone());
                self.previous_scroll_offsets = next_scroll_baseline;
                self.last_frame_damage = FrameDamage::None;
                self.last_present_damage = FrameDamage::None;
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
                rects.extend(resize_damage);
                all_damage = rects;
                is_incremental = true;
            }
            _ => {
                all_damage = resize_damage;
                is_incremental = false;
            }
        }

        if is_incremental && has_vview_damage {
            all_damage.extend(vview_damage);
        }
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
                Some(mut p) => {
                    p.fill(255, 255, 255, 255);
                    p
                }
                None => return Vec::new(),
            },
        };

        // Thin-strip scroll: MOVE the still-visible pixels and repaint only the
        // strip that scrolled into view. Regions that were pixel-SHIFTED belong
        // to PRESENT damage (the whole clip changed on screen) but not to paint
        // damage (only a strip was rasterised).
        let mut present_extra: Vec<LogicalRect> = Vec::new();
        if is_incremental {
            for (scroll_id, clip, delta, offset) in &scroll_shifts {
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
                    all_damage.extend(cpurender::overlay_rects_after_frame(
                        display_list,
                        *scroll_id,
                        clip,
                    ));
                    present_extra.push(*clip);
                } else {
                    all_damage.push(*clip);
                }
            }
        }

        // The recorded paint/present damage must not double-count a region.
        if is_incremental {
            cpurender::coalesce_damage_rects(&mut all_damage);
        }

        let gpu_cache = layout_window.gpu_state_manager.get_cache(dom_id);
        // Incremental repaints must raster at the offsets the surrounding
        // (un-repainted) pixels are ALREADY at — the baseline.
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
            drop(cpurender::render_display_list_damaged(
                display_list,
                &mut output,
                dpi_factor,
                renderer_resources,
                &layout_window.font_manager,
                &mut self.glyph_cache,
                &render_state,
                &all_damage,
            ));
        } else {
            output.fill(255, 255, 255, 255);
            compositor.allocate_layers_from_display_list(
                display_list,
                dpi_factor,
                &render_state.transforms,
            );
            drop(compositor.render_layers(
                display_list,
                dpi_factor,
                renderer_resources,
                &layout_window.font_manager,
                &mut self.glyph_cache,
                &render_state,
            ));
            compositor.composite_frame(&mut output, dpi_factor);
        }

        self.previous_display_list = Some(display_list.clone());
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
