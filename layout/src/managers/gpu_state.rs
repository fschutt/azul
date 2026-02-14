//! Centralized GPU state management.
//!
//! This module provides management of GPU property keys
//! (opacity, transforms, etc.), fade-in/fade-out animations
//! for scrollbar opacity - as a single source of truth for
//! the GPU cache.

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
    managers::scroll_state::ScrollManager,
    solver3::{layout_tree::LayoutTree, scrollbar::ScrollbarRequirements},
    text3::cache::ParsedFontTrait,
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
}

impl Default for GpuStateManager {
    fn default() -> Self {
        Self::new(
            Duration::System(SystemTimeDiff::from_millis(DEFAULT_FADE_DELAY_MS)),
            Duration::System(SystemTimeDiff::from_millis(DEFAULT_FADE_DURATION_MS)),
        )
    }
}

/// Internal state for tracking opacity transitions.
///
/// Used to implement smooth fade-in/fade-out animations for scrollbar opacity.
/// The opacity interpolates from `current_value` to `target_value` over time.
#[derive(Debug, Clone)]
struct OpacityState {
    /// Current opacity value (0.0 = transparent, 1.0 = opaque)
    current_value: f32,
    /// Target opacity value to transition towards
    target_value: f32,
    /// Timestamp of last scroll or hover activity
    last_activity_time: Instant,
    /// When the transition animation started (None if not transitioning)
    transition_start_time: Option<Instant>,
}

/// Result of a GPU state tick operation.
///
/// Contains information about whether the GPU state changed and
/// what specific changes occurred for the renderer to process.
#[derive(Debug, Default)]
pub struct GpuTickResult {
    /// Whether any GPU state changed requiring a repaint
    pub needs_repaint: bool,
    /// Detailed changes to transform and opacity keys
    pub changes: GpuEventChanges,
}

impl GpuStateManager {
    /// Creates a new GPU state manager with specified fade timing.
    pub fn new(fade_delay: Duration, fade_duration: Duration) -> Self {
        Self {
            caches: BTreeMap::new(),
            fade_delay,
            fade_duration,
        }
    }

    /// Advances GPU state by one tick, interpolating animated values.
    ///
    /// This should be called each frame to update opacity transitions
    /// for smooth scrollbar fading.
    pub fn tick(&mut self, now: Instant) -> GpuTickResult {
        // For now, this is a placeholder. A full implementation would
        // interpolate opacity values for smooth fading.
        GpuTickResult::default()
    }

    /// Gets or creates the GPU cache for a specific DOM.
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
                .map(|l| {
                    let bounds = l.layout.bounds();
                    LogicalSize {
                        width: bounds.width,
                        height: bounds.height,
                    }
                })
                .unwrap_or(container_size);

            if scrollbar_info.needs_vertical {
                let transform = compute_vertical_thumb_transform(
                    scrollbar_info,
                    &container_size,
                    &content_size,
                    scroll_offset.y,
                );

                update_transform_key(gpu_cache, &mut changes, dom_id, node_id, transform);
            }
        }

        changes
    }

    /// Returns a clone of all GPU value caches.
    pub fn get_gpu_value_cache(&self) -> BTreeMap<DomId, GpuValueCache> {
        self.caches.clone()
    }
}

/// Computes the transform for a vertical scrollbar thumb.
fn compute_vertical_thumb_transform(
    scrollbar_info: &ScrollbarRequirements,
    container_size: &LogicalSize,
    content_size: &LogicalSize,
    scroll_y: f32,
) -> ComputedTransform3D {
    let track_height = container_size.height - scrollbar_info.scrollbar_height;
    let thumb_height = (container_size.height / content_size.height) * track_height;
    let scrollable_dist = content_size.height - container_size.height;
    let thumb_dist = track_height - thumb_height;

    let scroll_ratio = if scrollable_dist > 0.0 {
        scroll_y / scrollable_dist
    } else {
        0.0
    };
    let thumb_offset_y = scroll_ratio * thumb_dist;

    ComputedTransform3D::new_translation(0.0, thumb_offset_y, 0.0)
}

/// Updates or creates a transform key in the GPU cache.
fn update_transform_key(
    gpu_cache: &mut GpuValueCache,
    changes: &mut GpuEventChanges,
    dom_id: DomId,
    node_id: NodeId,
    transform: ComputedTransform3D,
) {
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
