//! Centralized GPU state management
//!
//! This module provides:
//! - Management of all GPU property keys (opacity, transforms, etc.)
//! - Fade-in/fade-out animations for scrollbar opacity
//! - A single source of truth for the GPU cache

use alloc::collections::BTreeMap;

use azul_core::{
    dom::{DomId, NodeId},
    geom::LogicalSize,
    gpu::{GpuEventChanges, GpuScrollbarOpacityEvent, GpuTransformKeyEvent, GpuValueCache},
    resources::{OpacityKey, TransformKey},
    task::{Duration, Instant, SystemTimeDiff},
    transform::{ComputedTransform3D, RotationMode},
};

use crate::{
    scroll::{FrameScrollInfo, ScrollManager},
    solver3::{layout_tree::LayoutTree, scrollbar::ScrollbarInfo},
    text3::cache::ParsedFontTrait,
};

#[derive(Debug, Clone)]
pub struct GpuStateManager {
    pub caches: BTreeMap<DomId, GpuValueCache>,
    pub fade_delay: Duration,
    pub fade_duration: Duration,
}

impl Default for GpuStateManager {
    fn default() -> Self {
        Self::new(
            Duration::System(SystemTimeDiff::from_millis(500)),
            Duration::System(SystemTimeDiff::from_millis(200)),
        )
    }
}

#[derive(Debug, Clone)]
struct OpacityState {
    current_value: f32,
    target_value: f32,
    last_activity_time: Instant,
    transition_start_time: Option<Instant>,
}

#[derive(Debug, Default)]
pub struct GpuTickResult {
    pub needs_repaint: bool,
    pub changes: GpuEventChanges,
}

impl GpuStateManager {
    pub fn new(fade_delay: Duration, fade_duration: Duration) -> Self {
        Self {
            caches: BTreeMap::new(),
            fade_delay,
            fade_duration,
        }
    }

    pub fn tick(&mut self, now: Instant) -> GpuTickResult {
        // For now, this is a placeholder. A full implementation would
        // interpolate opacity values for smooth fading.
        GpuTickResult::default()
    }

    pub fn get_or_create_cache(&mut self, dom_id: DomId) -> &mut GpuValueCache {
        self.caches.entry(dom_id).or_default()
    }

    /// Updates scrollbar transforms based on current scroll positions.
    pub fn update_scrollbar_transforms(
        &mut self,
        dom_id: DomId,
        scroll_manager: &ScrollManager,
        layout_tree: &LayoutTree<impl ParsedFontTrait>,
    ) -> GpuEventChanges {
        let mut changes = GpuEventChanges::empty();
        let gpu_cache = self.get_or_create_cache(dom_id);

        for (node_idx, node) in layout_tree.nodes.iter().enumerate() {
            let Some(scrollbar_info) = &node.scrollbar_info else {
                continue;
            };
            let Some(node_id) = node.dom_node_id else {
                continue;
            };

            let scroll_offset = scroll_manager
                .get_current_offset(dom_id, node_id)
                .unwrap_or_default();
            let container_size = node.used_size.unwrap_or_default();
            let content_size = node
                .inline_layout_result
                .as_ref()
                .map(|l| LogicalSize {
                    width: l.bounds.width,
                    height: l.bounds.height,
                })
                .unwrap_or(container_size);

            if scrollbar_info.needs_vertical {
                let track_height = container_size.height - scrollbar_info.scrollbar_height;
                let thumb_height = (container_size.height / content_size.height) * track_height;
                let scrollable_dist = content_size.height - container_size.height;
                let thumb_dist = track_height - thumb_height;

                let scroll_ratio = if scrollable_dist > 0.0 {
                    scroll_offset.y / scrollable_dist
                } else {
                    0.0
                };
                let thumb_offset_y = scroll_ratio * thumb_dist;

                let transform = ComputedTransform3D::new_translation(0.0, thumb_offset_y, 0.0);
                let key = (dom_id, node_id);

                if let Some(existing_transform) = gpu_cache.current_transform_values.get(&node_id) {
                    if *existing_transform != transform {
                        let transform_key = gpu_cache.transform_keys[&node_id];
                        changes
                            .transform_key_changes
                            .push(GpuTransformKeyEvent::Changed(
                                node_id,
                                transform_key,
                                *existing_transform,
                                transform,
                            ));
                        gpu_cache
                            .current_transform_values
                            .insert(node_id, transform);
                    }
                } else {
                    let transform_key = TransformKey::unique();
                    gpu_cache.transform_keys.insert(node_id, transform_key);
                    gpu_cache
                        .current_transform_values
                        .insert(node_id, transform);
                    changes
                        .transform_key_changes
                        .push(GpuTransformKeyEvent::Added(
                            node_id,
                            transform_key,
                            transform,
                        ));
                }
            }
        }

        changes
    }

    pub fn get_gpu_value_cache(&self) -> BTreeMap<DomId, GpuValueCache> {
        self.caches.clone()
    }
}
