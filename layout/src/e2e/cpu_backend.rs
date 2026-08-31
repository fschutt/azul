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
    /// `LayoutCache::build_seq` at the last present — drains the patch log
    /// from there, so two patched builds between presents both repaint.
    pub(super) last_consumed_build_seq: u64,
    /// Arc pointer of the display list the translate blit last shifted —
    /// the already-shifted guard (a re-present of the same list must not
    /// blit twice). Mirrors the dll headless backend.
    pub(super) last_patch_shift_dl: usize,
    /// PAINT damage of the most recent `render_frame` — the region actually
    /// re-rasterised.
    pub(super) last_frame_damage: FrameDamage,
    /// PRESENT damage of the most recent `render_frame` — the region that
    /// visually CHANGED on screen (⊇ paint damage; a scroll memmoves a large
    /// region but paints a strip).
    pub(super) last_present_damage: FrameDamage,
    /// Scroll offsets of the previous frame (`scroll_id` → (x,y)).
    pub(super) previous_scroll_offsets: cpurender::ScrollOffsetMap,
    /// Where zombie exits painted LAST frame (logical px). Per frame the
    /// zombie contribution to damage is `previous ∪ current`: restore the
    /// live pixels where an exit was, paint it where it is — the reap frame
    /// (zombies gone, previous non-empty) erases the leftovers the same way.
    pub(super) previous_zombie_rects: Vec<azul_core::geom::LogicalRect>,
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
            last_consumed_build_seq: 0,
            last_patch_shift_dl: 0,
            last_frame_damage: FrameDamage::None,
            last_present_damage: FrameDamage::None,
            previous_scroll_offsets: cpurender::ScrollOffsetMap::new(),
            previous_zombie_rects: Vec::new(),
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
        // Engine observability: every e2e/headless frame reports its
        // duration + probe spans (drop-guard covers all return paths).
        #[cfg(feature = "telemetry")]
        let _frame_pump = crate::telemetry::FramePump::begin("present");

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
                    let _ = root_layer
                        .pixbuf
                        .resize_grow_only(pixel_w, pixel_h, 255, 255, 255, 255);
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
            .build_scroll_offset_map(dom_id, &result.scroll_id_to_node_id);

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
        if has_gpu_damage && std::env::var_os("AZ_PATCH_DEBUG").is_some() {
            let td: Vec<_> = gpu_transforms
                .iter()
                .filter(|(k, v)| self.previous_gpu_transforms.get(k) != Some(v))
                .map(|(k, v)| (*k, v.m[3][0], v.m[3][1]))
                .collect();
            let od: Vec<_> = gpu_opacities
                .iter()
                .filter(|(k, v)| self.previous_gpu_opacities.get(k) != Some(v))
                .map(|(k, v)| (*k, *v))
                .collect();
            eprintln!(
                "[GPUDMG] prev_t={} cur_t={} prev_o={} cur_o={} changed_t={td:?} changed_o={od:?} rects={:?}",
                self.previous_gpu_transforms.len(),
                gpu_transforms.len(),
                self.previous_gpu_opacities.len(),
                gpu_opacities.len(),
                gpu_damage.rects,
            );
        }
        // Retained exits repaint every tick without any display-list item
        // changing — their per-frame truth is `previous ∪ current` painted
        // rects: restore the live frame where the exit WAS, paint it where
        // it IS. That keeps the incremental path (and even the reap frame's
        // cleanup) on bounded damage instead of forcing full composites for
        // the whole exit duration.
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
        self.previous_gpu_transforms = gpu_transforms;
        self.previous_gpu_opacities = gpu_opacities;

        // Can the pixels of the previous frame still be trusted? Yes when the
        // buffer did not change size at all, and yes on a GROW (the old pixels
        // were copied over verbatim). No on a shrink / first allocation.
        let can_reuse_previous_frame = !needs_resize || resize_preserved_pixels;

        // Translate hint: identical decision to the dll headless backend, so
        // the harness executes the SAME blit path a device does (it used to
        // have no notion of a TranslateHint at all — every mover-blit bug was
        // structurally untestable here).
        let dl_arc_ptr = std::sync::Arc::as_ptr(display_list) as usize;
        let patch_hint = cpurender::translate_hint_for_patch(
            layout_window.layout_cache.last_patch_move.as_ref(),
            dpi_factor,
            can_reuse_previous_frame,
            self.last_patch_shift_dl == dl_arc_ptr,
            layout_window.scroll_manager.any_nonzero_offset(),
        );

        // Display-list damage (incremental path)
        let mut patch_moved_union: Option<LogicalRect> = None;
        let dl_damage = match &self.previous_display_list {
            // SAME Arc = the DL cache served the identical list (scroll
            // steady state): zero item changes by definition - mirror of the
            // dll present path's shortcut, so the twin measures what the
            // device does.
            Some(old_dl)
                if can_reuse_previous_frame
                    && !gpu_damage.needs_full
                    && std::sync::Arc::ptr_eq(old_dl, display_list) =>
            {
                Some(Vec::new())
            }
            Some(old_dl) if can_reuse_previous_frame && !gpu_damage.needs_full => {
                match cpurender::compute_display_list_damage_translated(
                    old_dl,
                    display_list,
                    &self.previous_scroll_offsets,
                    &scroll_offsets,
                    patch_hint.as_ref().map(|(h, _)| h),
                ) {
                    Some((damage, moved)) => {
                        patch_moved_union = moved;
                        Some(damage)
                    }
                    None => None,
                }
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
        // One shared, viewport-projected collector with the shell - see
        // `cpurender::collect_scroll_shifts`.
        let scroll_shifts = cpurender::collect_scroll_shifts(
            display_list,
            &scroll_offsets,
            &self.previous_scroll_offsets,
            dpi_factor,
        );
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
                        self.previous_scroll_offsets
                            .get(id)
                            .copied()
                            .unwrap_or(*off),
                    )
                }
            })
            .collect();

        // Determine render path.
        let mut all_damage: Vec<LogicalRect>;
        let is_incremental;

        // A PATCHED build may change the item count, which the old-vs-new
        // item diff reads as structural (None -> full). The patch recorded
        // its own precise damage at build time — and on a patched build it
        // is AUTHORITATIVE, not a fallback: the index-pairing diff
        // under-damages a same-count splice (re-emitted node + translated
        // neighbours mis-pair). Guarded to the same conditions the diff ran
        // under, so gpu needs_full / shrink / first frame stay full repaints.
        let diff_path_ran = self.previous_display_list.is_some()
            && can_reuse_previous_frame
            && !gpu_damage.needs_full;
        let dl_damage = if diff_path_ran && layout_window.layout_cache.last_build_was_patched {
            // On a PATCHED build the patch's own damage AUGMENTS the item
            // diff: the index-pairing diff under-damages a same-count splice
            // (one stale rect where a reflow moved three nodes), so union
            // the two when the diff produced rects, and use the patch's
            // damage alone when the diff gave up (count change -> None).
            // Never REPLACE a Some(diff) wholesale: unpatched-equal frames
            // must keep their baseline damage exactly (an empty diff on a
            // quiet frame stays the idle skip).
            // EVERY patched build since this backend last presented (see
            // the headless twin): two patched builds in one pass each know
            // only their own vacated rects.
            use azul_layout::solver3::cache::PendingPatchDamage as P;
            let pending = layout_window
                .layout_cache
                .pending_patch_damage(self.last_consumed_build_seq);
            // Patch rects are CONTENT-space (no scroll projection on the
            // producer); with any active offset they land offset-pixels away
            // from the changed pixels. Demote to full-build semantics then —
            // same gate as the dll present path.
            let patch_rects_trustworthy = !layout_window.scroll_manager.any_nonzero_offset();
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
                (None, P::Rects(p)) => Some(p),
                (d, P::FullBuildSincePresent) => d,
                (d, P::None) => d,
                (_, P::Unknown) => None,
            }
        } else {
            dl_damage
        };
        if std::env::var_os("AZ_PATCH_DEBUG").is_some() {
            eprintln!(
                "[E2EDMG] dl_damage={:?} diff_ran={} patched={} resize={:?} gpu_full={} gpu_rects={} zombie={}",
                dl_damage.as_ref().map(|r| r.len()),
                diff_path_ran,
                layout_window.layout_cache.last_build_was_patched,
                resize_damage.len(),
                gpu_damage.needs_full,
                gpu_damage.rects.len(),
                zombie_damage.len(),
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
                    && zombie_damage.is_empty() =>
            {
                // Nothing changed — skip rendering entirely.
                //
                // `!needs_resize` is load-bearing now that a resize can reach
                // this match at all: skipping leaves `last_frame` at the OLD
                // dimensions while the compositor is already at the new ones, so
                // the host would publish (and present) a wrongly-sized buffer.
                // A frame whose backing store changed size is never "nothing".
                self.previous_display_list = Some(display_list.clone());
                self.last_consumed_build_seq = layout_window.layout_cache.build_seq;
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
        if is_incremental && !zombie_damage.is_empty() {
            all_damage.extend(zombie_damage.iter().copied());
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
            if let (Some((hint, exceptions)), Some(moved)) =
                (patch_hint.as_ref(), patch_moved_union)
            {
                self.last_patch_shift_dl = dl_arc_ptr;
                let _ = moved;
                let mover_rects = layout_window
                    .layout_cache
                    .last_patch_move
                    .as_ref()
                    .map(|m| m.mover_rects_old.clone())
                    .unwrap_or_default();
                let blit = cpurender::execute_translate_blit(
                    &mut output,
                    hint,
                    exceptions,
                    &mover_rects,
                    display_list,
                    dpi_factor,
                    false, // e2e twin renders owned pixmaps, never pool-order
                );
                all_damage.extend(blit.damage);
                present_extra.extend(blit.present_extra);
            }
            for (scroll_id, clip, delta, offset) in &scroll_shifts {
                // One shared recipe with the shell — see
                // `cpurender::execute_scroll_shift`.
                let out = cpurender::execute_scroll_shift(
                    &mut output,
                    display_list,
                    *scroll_id,
                    clip,
                    *delta,
                    *offset,
                    dpi_factor,
                    false, // e2e twin renders owned pixmaps, never pool-order
                );
                all_damage.extend(out.damage);
                present_extra.extend(out.present_extra);
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
            output.fill(255, 255, 255, 255);
            compositor.allocate_layers_from_display_list(
                display_list,
                dpi_factor,
                &render_state.transforms,
                &render_state.opacities,
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
            // The design doc's invariant: the rendered frame is B ∪ zombies.
            layout_window.composite_zombies_cpu(
                &mut output,
                dpi_factor,
                renderer_resources,
                &mut self.glyph_cache,
            );
        }

        self.previous_zombie_rects = zombie_rects;
        self.previous_display_list = Some(display_list.clone());
        self.last_consumed_build_seq = layout_window.layout_cache.build_seq;
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

#[cfg(test)]
mod tests {
    use super::*;
    use azul_core::{
        dom::{Dom, DomId, NodeId},
        geom::LogicalSize,
        resources::RendererResources,
        styled_dom::StyledDom,
    };
    use azul_css::AzString;
    use rust_fontconfig::FcFontCache;

    /// `::placeholder` is REAL CSS: an author rule restyles the prompt the
    /// engine paints, through the same cascade every other property uses.
    ///
    /// Pinned by COLOUR, because that is what a stylesheet actually controls
    /// here: with no rule the prompt is the host's colour at half alpha;
    /// with `::placeholder { color: red }` it is exactly red.
    #[test]
    fn a_placeholder_rule_restyles_the_prompt_through_the_cascade() {
        use azul_core::dom::AttributeType;

        let prompt_pixels = |css_src: &str| -> Vec<(u8, u8, u8)> {
            let mut dom = Dom::create_body().with_child(
                Dom::create_div()
                    .with_ids_and_classes(
                        vec![azul_core::dom::IdOrClass::Class("ed".into())].into(),
                    )
                    .with_attribute(AttributeType::ContentEditable(true))
                    .with_attribute(AttributeType::Placeholder("Hint me".into())),
            );
            let (css, _) = azul_css::parser2::new_from_str(css_src);
            let styled_dom = StyledDom::create(&mut dom, css);
            let mut lw = crate::window::LayoutWindow::new(FcFontCache::build()).unwrap();
            lw.system_animations_override =
                Some(azul_core::resources::SystemAnimations::disabled());
            let mut ws = crate::window_state::FullWindowState::default();
            ws.size.dimensions = LogicalSize::new(400.0, 100.0);
            lw.current_window_state = ws.clone();
            let resources = RendererResources::default();
            let cbs = crate::callbacks::ExternalSystemCallbacks::rust_internal();
            let mut dbg = Some(Vec::new());
            lw.layout_and_generate_display_list(styled_dom, &ws, &resources, &cbs, &mut dbg)
                .unwrap();
            let lr = lw.get_layout_result(&DomId::ROOT_ID).unwrap();
            lr.display_list
                .items
                .iter()
                .filter_map(|it| match it {
                    crate::solver3::display_list::DisplayListItem::Text {
                        color, glyphs, ..
                    } if !glyphs.is_empty() => Some((color.r, color.g, color.b)),
                    _ => None,
                })
                .collect()
        };

        const BASE: &str = "body { width: 400px; height: 100px; } \
                            .ed { width: 300px; height: 40px; font-size: 16px; color: black; }";

        let unstyled = prompt_pixels(BASE);
        assert_eq!(unstyled.len(), 1, "exactly one prompt run: {unstyled:?}");

        let styled = prompt_pixels(&format!(
            "{BASE} .ed::placeholder {{ color: rgb(255, 0, 0); }}"
        ));
        assert_eq!(
            styled,
            vec![(255, 0, 0)],
            "a ::placeholder rule must restyle the prompt (unstyled was {unstyled:?})"
        );
        assert_ne!(
            styled, unstyled,
            "the rule must actually change something"
        );
    }

    /// ENGINE-LEVEL PLACEHOLDER: the `placeholder` ATTRIBUTE on a
    /// contenteditable host paints a prompt while the host is EMPTY and
    /// UNFOCUSED - no overlay <p>, no per-widget toggle code. The control
    /// (no attribute) must paint nothing, pinning that the ink really is
    /// the attribute's doing.
    #[test]
    fn the_placeholder_attribute_paints_for_an_empty_unfocused_editable() {
        use azul_core::dom::AttributeType;

        let ink_rows = |with_attr: bool, focused: bool, value: &str| -> usize {
            let mut ed = Dom::create_div()
                .with_ids_and_classes(
                    vec![azul_core::dom::IdOrClass::Class("ed".into())].into(),
                )
                .with_attribute(AttributeType::ContentEditable(true));
            if with_attr {
                ed = ed.with_attribute(AttributeType::Placeholder("Hint me".into()));
            }
            if !value.is_empty() {
                ed = ed.with_children(
                    vec![crate::widgets::widget_p().with_children(
                        vec![Dom::create_text_do_not_use_without_block_level_wrapper(
                            value.to_string(),
                        )]
                        .into(),
                    )]
                    .into(),
                );
            }
            let mut dom = Dom::create_body().with_child(ed);
            let (css, _) = azul_css::parser2::new_from_str(
                "body { width: 400px; height: 100px; } \
                 .ed { width: 300px; height: 40px; font-size: 16px; }",
            );
            let styled_dom = StyledDom::create(&mut dom, css);
            let mut lw = crate::window::LayoutWindow::new(FcFontCache::build()).unwrap();
            lw.system_animations_override =
                Some(azul_core::resources::SystemAnimations::disabled());
            let styled_dom = if focused {
                // Stamp :focus exactly the way the SHELL does before layout
                // (dll `apply_runtime_states_before_layout`) - that stamping
                // lives in the shell, so an engine-only harness must mirror
                // it or the painter's focus check can never see focus.
                let mut sd = styled_dom;
                {
                    let mut styled_nodes = sd.styled_nodes.as_container_mut();
                    if let Some(n) = styled_nodes.get_mut(NodeId::new(1)) {
                        n.styled_node_state.focused = true;
                    }
                }
                sd
            } else {
                styled_dom
            };
            let mut ws = crate::window_state::FullWindowState::default();
            ws.size.dimensions = LogicalSize::new(400.0, 100.0);
            lw.current_window_state = ws.clone();
            let resources = RendererResources::default();
            let cbs = crate::callbacks::ExternalSystemCallbacks::rust_internal();
            let mut dbg = Some(Vec::new());
            lw.layout_and_generate_display_list(styled_dom, &ws, &resources, &cbs, &mut dbg)
                .unwrap();
            let lr = lw.get_layout_result(&DomId::ROOT_ID).unwrap();
            let mut gc = crate::glyph_cache::GlyphCache::new();
            let frame = crate::cpurender::render_with_font_manager(
            &lr.display_list,
            &resources,
            &lw.font_manager,
            crate::cpurender::RenderOptions {
                width: 400.0,
                height: 100.0,
                dpi_factor: 1.0,
            },
            &mut gc,
        )
        .unwrap();
            let (w, data) = (frame.width() as usize, frame.data());
            (0..40usize)
                .filter(|y| {
                    (0..300usize).any(|x| {
                        let i = ((y * w + x) * 4).min(data.len().saturating_sub(4));
                        (u16::from(data[i]) + u16::from(data[i + 1]) + u16::from(data[i + 2]))
                            < 690
                    })
                })
                .count()
        };

        assert_eq!(
            ink_rows(false, false, ""),
            0,
            "the control (no placeholder attribute) must paint nothing"
        );
        let with_attr = ink_rows(true, false, "");
        assert!(
            with_attr >= 6,
            "the placeholder attribute must paint a text line (got {with_attr} ink rows)"
        );
        assert_eq!(
            ink_rows(true, true, ""),
            0,
            "a FOCUSED host hides its prompt (2026-08-31 ruling) - recomputed \
             per build, no latch to stick"
        );

        // THE DEVICE REGRESSION (2026-08-31): a DEFOCUSED host that HAS text
        // painted its prompt under that text, because emptiness was read off
        // `inline_layout_result` and its absence counted as empty. Emptiness
        // is a question about CONTENT: the filled host must paint exactly
        // what a host with no placeholder attribute paints.
        let filled_with_prompt = ink_rows(true, false, "typed");
        let filled_control = ink_rows(false, false, "typed");
        assert_eq!(
            filled_with_prompt, filled_control,
            "a defocused FILLED host must paint no prompt: {filled_with_prompt} \
             ink rows with the attribute vs {filled_control} without it"
        );
    }

    /// Control: the same field WITHOUT the scroll column (no layer). If this
    /// is green while the column variant is red, the truncation lives in the
    /// LAYERED render path, not in text rendering itself.
    #[test]
    fn first_flat_frame_paints_the_placeholder_fully() {
        use crate::widgets::text_input::TextInput;
        let mut dom = Dom::create_body().with_child(
            TextInput::create()
                .with_placeholder("Type something".into())
                .dom(),
        );
        let (css, _) =
            azul_css::parser2::new_from_str("body { width: 640px; height: 480px; }");
        let styled_dom = StyledDom::create(&mut dom, css);
        let mut lw = crate::window::LayoutWindow::new(FcFontCache::build()).unwrap();
        lw.system_animations_override =
            Some(azul_core::resources::SystemAnimations::disabled());
        let mut ws = crate::window_state::FullWindowState::default();
        ws.size.dimensions = LogicalSize::new(640.0, 480.0);
        lw.current_window_state = ws.clone();
        let resources = RendererResources::default();
        let cbs = crate::callbacks::ExternalSystemCallbacks::rust_internal();
        let mut dbg = Some(Vec::new());
        lw.layout_and_generate_display_list(styled_dom, &ws, &resources, &cbs, &mut dbg)
            .unwrap();
        let lr = lw.get_layout_result(&DomId::ROOT_ID).unwrap();
        let idx = lr
            .layout_tree
            .dom_to_layout
            .get(&NodeId::new(1))
            .and_then(|v| v.first())
            .expect("container laid out")
            .index();
        let pos = lr.calculated_positions[idx];
        let size = lr
            .layout_tree
            .get(crate::solver3::LayoutNodeId::new(idx))
            .and_then(|n| n.used_size)
            .expect("container size");
        let dpi: f32 = std::env::var("AZ_REPRO_DPI")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(2.0);
        let ink = |frame: &crate::cpurender::AzulPixmap| -> (usize, Vec<usize>) {
            let (w, data) = (frame.width() as usize, frame.data());
            let x0 = ((pos.x + 4.0) * dpi) as usize;
            let x1 = ((pos.x + size.width - 4.0) * dpi) as usize;
            let rows: Vec<usize> = (0..frame.height() as usize)
                .filter(|y| {
                    (x0..x1).any(|x| {
                        let i = ((y * w + x) * 4).min(data.len().saturating_sub(4)); // clamp: field can overflow the window row
                        (u16::from(data[i]) + u16::from(data[i + 1]) + u16::from(data[i + 2]))
                            < 690 // engine prompt = host colour at half alpha (~#A6A6A6)
                    })
                })
                .collect();
            let y0 = ((pos.y + 2.0) * dpi) as usize;
            let y1 = ((pos.y + size.height - 2.0) * dpi) as usize;
            let inside = rows.iter().filter(|y| (y0..y1).contains(y)).count();
            (inside, rows)
        };
        let mut backend = CpuBackend::new();
        backend.render_frame(&lw, &resources, 640.0, 480.0, dpi);
        let frame = backend.last_frame.as_ref().unwrap().clone_pixmap();
        let (inside_b, rows_b) = ink(&frame);
        let mut gc = crate::glyph_cache::GlyphCache::new();
        let flat = crate::cpurender::render_with_font_manager(
            &lr.display_list,
            &resources,
            &lw.font_manager,
            crate::cpurender::RenderOptions {
                width: 640.0,
                height: 480.0,
                dpi_factor: dpi,
            },
            &mut gc,
        )
        .unwrap();
        let (inside_f, rows_f) = ink(&flat);
        eprintln!(
            "[repro] dpi={dpi} field_y_dev={:?} backend: inside={inside_b} all_ink_rows={:?} | flat: inside={inside_f} all_ink_rows={:?}",
            (((pos.y + 2.0) * dpi) as usize, ((pos.y + size.height - 2.0) * dpi) as usize),
            &rows_b[..rows_b.len().min(30)],
            &rows_f[..rows_f.len().min(30)]
        );
        assert!(inside_b >= 8, "flat control: got {inside_b} ink rows (backend)");
    }

    /// DEVICE 2026-08-31: on the first frames, every 11px placeholder inside
    /// the page's scroll column rendered as a ~4px strip of glyph TOPS -
    /// while the display list carried a healthy 13px clip (dl dumps) and any
    /// damaged repaint painted the identical list correctly. The layered
    /// full render is the only path that differs. This pins it headlessly:
    /// the field's inner text region must contain a full line of ink.
    #[test]
    fn first_layered_frame_paints_the_placeholder_fully() {
        use crate::widgets::text_input::TextInput;
        let mut dom = Dom::create_body().with_child(
            Dom::create_div().with_class("col".into()).with_children(
                vec![
                    TextInput::create()
                        .with_placeholder("Type something".into())
                        .dom(),
                    Dom::create_div().with_class("filler".into()),
                ]
                .into(),
            ),
        );
        let (css, _) = azul_css::parser2::new_from_str(
            "body { width: 640px; height: 480px; }              .col { overflow-y: auto; width: 100%; height: 100%; }              .filler { height: 2000px; }",
        );
        let styled_dom = StyledDom::create(&mut dom, css);
        let mut lw = crate::window::LayoutWindow::new(FcFontCache::build()).unwrap();
        lw.system_animations_override =
            Some(azul_core::resources::SystemAnimations::disabled());
        let mut ws = crate::window_state::FullWindowState::default();
        ws.size.dimensions = LogicalSize::new(640.0, 480.0);
        lw.current_window_state = ws.clone();
        let resources = RendererResources::default();
        let cbs = crate::callbacks::ExternalSystemCallbacks::rust_internal();
        let mut dbg = Some(Vec::new());
        lw.layout_and_generate_display_list(styled_dom, &ws, &resources, &cbs, &mut dbg)
            .unwrap();

        // The TextInput container: body(0) > col(1) > container(2).
        let lr = lw.get_layout_result(&DomId::ROOT_ID).unwrap();
        let idx = lr
            .layout_tree
            .dom_to_layout
            .get(&NodeId::new(2))
            .and_then(|v| v.first())
            .expect("container laid out")
            .index();
        let pos = lr.calculated_positions[idx];
        let size = lr
            .layout_tree
            .get(crate::solver3::LayoutNodeId::new(idx))
            .and_then(|n| n.used_size)
            .expect("container size");

        let dpi = 2.0;
        let mut backend = CpuBackend::new();
        backend.render_frame(&lw, &resources, 640.0, 480.0, dpi);
        let frame = backend
            .last_frame
            .as_ref()
            .expect("first layered frame")
            .clone_pixmap();

        // Count device rows with ink inside the field's inner text region.
        let (w, data) = (frame.width() as usize, frame.data());
        let x0 = ((pos.x + 4.0) * dpi) as usize;
        let x1 = ((pos.x + size.width - 4.0) * dpi) as usize;
        let y0 = ((pos.y + 2.0) * dpi) as usize;
        let y1 = ((pos.y + size.height - 2.0) * dpi) as usize;
        // Diagnostic: keep the frame on disk for eyeballing.
        if let Ok(dir) = std::env::var("AZ_REPRO_DUMP") {
            if let Ok(bytes) = frame.encode_png() {
                let _ = std::fs::write(format!("{dir}/repro_layered.png"), bytes);
            }
        }
        let ink_rows = (y0..y1)
            .filter(|y| {
                (x0..x1).any(|x| {
                    let i = ((y * w + x) * 4).min(data.len().saturating_sub(4)); // clamp: field can overflow the window row
                    let (r, g, b) = (data[i], data[i + 1], data[i + 2]);
                    // darker than the border grey - real glyph ink
                    // engine prompt = host colour at half alpha (~#A6A6A6)
                    (u16::from(r) + u16::from(g) + u16::from(b)) < 690
                })
            })
            .count();
        assert!(
            ink_rows >= 8,
            "the placeholder must paint a full text line in the first layered              frame; got {ink_rows} ink rows in y {y0}..{y1} (the device strip              was <= 5 rows). Field at {pos:?} {size:?}"
        );
    }
}

