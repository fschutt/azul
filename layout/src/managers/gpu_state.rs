//! Centralized GPU state management.
//!
//! This module provides management of GPU property keys
//! (opacity, transforms, etc.), fade-in/fade-out animations
//! for scrollbar opacity - as a single source of truth for
//! the GPU cache.

use alloc::collections::BTreeMap;

#[cfg(feature = "std")]
use std::collections::HashMap;
#[cfg(not(feature = "std"))]
use alloc::collections::BTreeMap as HashMap;

use azul_core::{
    dom::{DomId, NodeId},
    dom::ScrollbarOrientation,
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    gpu::{GpuEventChanges, GpuTransformKeyEvent, GpuValueCache},
    resources::TransformKey,
    task::{Duration, SystemTimeDiff},
    transform::ComputedTransform3D,
};

use crate::{
    managers::scroll_state::ScrollManager,
    solver3::{
        fc::DEFAULT_SCROLLBAR_WIDTH_PX,
        layout_tree::LayoutTree,
        scrollbar::compute_scrollbar_geometry_with_button_size,
    },
};

/// Default delay before scrollbars start fading out (500ms)
pub const DEFAULT_FADE_DELAY_MS: u64 = 500;
/// Default duration of scrollbar fade-out animation (200ms)
pub const DEFAULT_FADE_DURATION_MS: u64 = 200;

/// Manages GPU-accelerated properties across all DOMs.
///
/// The `GpuStateManager` maintains caches for transform and opacity keys
/// that are used by the GPU renderer. It handles:
///
/// - Scrollbar thumb position transforms (updated on scroll)
/// - Opacity fading for scrollbars (fade in on activity, fade out after delay)
/// - Per-DOM GPU value caches for efficient rendering
#[derive(Debug, Clone)]
pub struct GpuStateManager {
    /// GPU value caches indexed by DOM ID
    pub caches: BTreeMap<DomId, GpuValueCache>,
    /// Delay before scrollbars start fading out after last activity
    pub fade_delay: Duration,
    /// Duration of the fade-out animation
    pub fade_duration: Duration,
    /// Whether any scrollbar has non-zero opacity and needs continued frame
    /// generation. Set during both the `fade_delay` period (opacity == 1.0)
    /// and the active fade-out phase (0 < opacity < 1).
    /// Set by `LayoutWindow::synchronize_scrollbar_opacity`, read by the platform render loop.
    pub scrollbar_fade_active: bool,
    /// GPU events produced during layout (CSS transform / opacity synchronization,
    /// scrollbar transform / opacity updates) that have not yet been pushed to
    /// the renderer. Drained by the platform render path when a transaction is
    /// built.
    pub pending_changes: GpuEventChanges,
}

impl Default for GpuStateManager {
    fn default() -> Self {
        Self::new(
            Duration::System(SystemTimeDiff::from_millis(DEFAULT_FADE_DELAY_MS)),
            Duration::System(SystemTimeDiff::from_millis(DEFAULT_FADE_DURATION_MS)),
        )
    }
}

impl GpuStateManager {
    /// Creates a new GPU state manager with specified fade timing.
    #[must_use] pub fn new(fade_delay: Duration, fade_duration: Duration) -> Self {
        Self {
            caches: BTreeMap::new(),
            fade_delay,
            fade_duration,
            scrollbar_fade_active: false,
            pending_changes: GpuEventChanges::empty(),
        }
    }

    /// Take any queued transform / opacity events that have been accumulated
    /// during layout. Clears the internal buffer.
    pub fn take_pending_changes(&mut self) -> GpuEventChanges {
        core::mem::take(&mut self.pending_changes)
    }

    // NOTE: the per-frame scrollbar-fade interpolation lives in
    // `LayoutWindow::synchronize_scrollbar_opacity` (layout/src/window.rs),
    // which reads the last-activity time straight from the `ScrollManager` and
    // is the single source of truth for fade opacity. An earlier duplicate
    // tick-based subsystem here (`tick` / `record_scroll_activity` /
    // `calculate_fade_opacity` + a `fade_states` map) was never wired into any
    // render loop and has been removed.

    /// Gets or creates the GPU cache for a specific DOM.
    #[must_use] pub fn get_cache(&self, dom_id: DomId) -> Option<&GpuValueCache> {
        self.caches.get(&dom_id)
    }

    pub fn get_or_create_cache(&mut self, dom_id: DomId) -> &mut GpuValueCache {
        self.caches.entry(dom_id).or_default()
    }

    /// Updates scrollbar thumb transforms based on current scroll positions.
    ///
    /// Calculates the transform needed to position scrollbar thumbs correctly
    /// based on the scroll offset and content/container sizes. Returns the
    /// GPU event changes that need to be applied by the renderer.
    pub fn update_scrollbar_transforms(
        &mut self,
        dom_id: DomId,
        scroll_manager: &ScrollManager,
        layout_tree: &LayoutTree,
    ) -> GpuEventChanges {
        let mut changes = GpuEventChanges::empty();
        let gpu_cache = self.get_or_create_cache(dom_id);

        for (node_idx, node) in layout_tree.nodes.iter().enumerate() {
            let warm = layout_tree.warm(node_idx);
            let Some(scrollbar_info) = warm.and_then(|w| w.scrollbar_info.as_ref()) else {
                continue;
            };
            let Some(node_id) = node.dom_node_id else {
                continue;
            };

            let scroll_offset = scroll_manager
                .get_current_offset(dom_id, node_id)
                .unwrap_or_default();

            // Compute inner_rect (padding-box) by subtracting borders from used_size
            let border_box_size = node.used_size.unwrap_or_default();
            let nbp = node.box_props.unpack();
            let border = &nbp.border;
            let inner_size = LogicalSize {
                width: (border_box_size.width - border.left - border.right).max(0.0),
                height: (border_box_size.height - border.top - border.bottom).max(0.0),
            };
            // Use zero origin since we only need the geometry ratios, not absolute position
            let inner_rect = LogicalRect {
                origin: LogicalPosition::new(0.0, 0.0),
                size: inner_size,
            };

            // Use get_content_size() as the single source of truth for content dimensions
            let content_size = layout_tree.get_content_size(node_idx);

            if scrollbar_info.needs_vertical {
                // Use the visual width from the scrollbar style — same value used
                // by display_list.rs to paint the scrollbar. For overlay scrollbars,
                // visual_width_px is non-zero (e.g. 8.0) even though the layout-
                // reserved width (scrollbar_height) is 0.0.
                let is_overlay = scrollbar_info.scrollbar_height == 0.0;
                let scrollbar_width_px = if scrollbar_info.visual_width_px > 0.0 {
                    scrollbar_info.visual_width_px
                } else if !is_overlay {
                    scrollbar_info.scrollbar_height
                } else {
                    DEFAULT_SCROLLBAR_WIDTH_PX
                };
                // Overlay scrollbars (macOS-style) have no arrow buttons
                let button_size = if is_overlay { 0.0 } else { scrollbar_width_px };

                let v_geom = compute_scrollbar_geometry_with_button_size(
                    ScrollbarOrientation::Vertical,
                    inner_rect,
                    content_size,
                    scroll_offset.y,
                    scrollbar_width_px,
                    scrollbar_info.needs_horizontal,
                    button_size,
                );

                let transform =
                    ComputedTransform3D::new_translation(0.0, v_geom.thumb_offset, 0.0);
                update_scrollbar_transform_key(gpu_cache, &mut changes, node_id, transform, ScrollbarOrientation::Vertical);
            }

            if scrollbar_info.needs_horizontal {
                let is_overlay = scrollbar_info.scrollbar_width == 0.0;
                let scrollbar_width_px = if scrollbar_info.visual_width_px > 0.0 {
                    scrollbar_info.visual_width_px
                } else if !is_overlay {
                    scrollbar_info.scrollbar_width
                } else {
                    DEFAULT_SCROLLBAR_WIDTH_PX
                };
                let button_size = if is_overlay { 0.0 } else { scrollbar_width_px };

                let h_geom = compute_scrollbar_geometry_with_button_size(
                    ScrollbarOrientation::Horizontal,
                    inner_rect,
                    content_size,
                    scroll_offset.x,
                    scrollbar_width_px,
                    scrollbar_info.needs_vertical,
                    button_size,
                );

                let transform =
                    ComputedTransform3D::new_translation(h_geom.thumb_offset, 0.0, 0.0);
                update_scrollbar_transform_key(gpu_cache, &mut changes, node_id, transform, ScrollbarOrientation::Horizontal);
            }
        }

        changes
    }
}

/// Updates or creates a scrollbar transform key in the GPU cache for the given orientation.
fn update_scrollbar_transform_key(
    gpu_cache: &mut GpuValueCache,
    changes: &mut GpuEventChanges,
    node_id: NodeId,
    transform: ComputedTransform3D,
    orientation: ScrollbarOrientation,
) {
    let (keys, values) = match orientation {
        ScrollbarOrientation::Vertical => (
            &mut gpu_cache.transform_keys,
            &mut gpu_cache.current_transform_values,
        ),
        ScrollbarOrientation::Horizontal => (
            &mut gpu_cache.h_transform_keys,
            &mut gpu_cache.h_current_transform_values,
        ),
    };

    if let Some(existing_transform) = values.get(&node_id) {
        if *existing_transform != transform {
            let Some(&transform_key) = keys.get(&node_id) else {
                return;
            };
            changes
                .transform_key_changes
                .push(GpuTransformKeyEvent::Changed(
                    node_id,
                    transform_key,
                    *existing_transform,
                    transform,
                ));
            values.insert(node_id, transform);
        }
    } else {
        let transform_key = TransformKey::unique();
        keys.insert(node_id, transform_key);
        values.insert(node_id, transform);
        changes
            .transform_key_changes
            .push(GpuTransformKeyEvent::Added(
                node_id,
                transform_key,
                transform,
            ));
    }
}

impl crate::managers::NodeIdRemap for GpuStateManager {
    /// Remap the per-node GPU caches (transform / opacity keys and values).
    ///
    /// Without this, a rebuild that shifts `NodeIds` left the scrollbar/CSS
    /// transform + opacity keys attached to the wrong node — the visible symptom
    /// being a scrollbar thumb (or an animated element) that keeps painting at a
    /// stale offset, plus `scrollbar_fade_active` never settling because the
    /// platform loop keeps generating frames for a node that is not the one
    /// actually scrolling.
    fn remap_node_ids(&mut self, dom: DomId, map: &crate::managers::NodeIdMap) {
        let Some(cache) = self.caches.get_mut(&dom) else {
            return;
        };

        remap_hashmap(&mut cache.transform_keys, map);
        remap_hashmap(&mut cache.current_transform_values, map);
        remap_hashmap(&mut cache.h_transform_keys, map);
        remap_hashmap(&mut cache.h_current_transform_values, map);
        remap_hashmap(&mut cache.css_transform_keys, map);
        remap_hashmap(&mut cache.css_current_transform_values, map);
        remap_hashmap(&mut cache.opacity_keys, map);
        remap_hashmap(&mut cache.current_opacity_values, map);
        remap_dom_hashmap(&mut cache.scrollbar_v_opacity_keys, dom, map);
        remap_dom_hashmap(&mut cache.scrollbar_h_opacity_keys, dom, map);
        remap_dom_hashmap(&mut cache.scrollbar_v_opacity_values, dom, map);
        remap_dom_hashmap(&mut cache.scrollbar_h_opacity_values, dom, map);
    }
}

/// Rewrite `NodeId` keys, dropping entries for unmounted nodes.
fn remap_hashmap<V>(map: &mut HashMap<NodeId, V>, node_map: &crate::managers::NodeIdMap) {
    let old = core::mem::take(map);
    for (old_id, v) in old {
        if let Some(new_id) = node_map.resolve(old_id) {
            map.insert(new_id, v);
        }
    }
}

/// Rewrite `(DomId, NodeId)` keys for `dom` only, dropping unmounted nodes.
fn remap_dom_hashmap<V>(
    map: &mut HashMap<(DomId, NodeId), V>,
    dom: DomId,
    node_map: &crate::managers::NodeIdMap,
) {
    let old = core::mem::take(map);
    for ((d, old_id), v) in old {
        if d != dom {
            map.insert((d, old_id), v);
        } else if let Some(new_id) = node_map.resolve(old_id) {
            map.insert((d, new_id), v);
        }
    }
}
