//! Centralized GPU state management.
//!
//! This module provides management of GPU property keys
//! (opacity, transforms, etc.), fade-in/fade-out animations
//! for scrollbar opacity - as a single source of truth for
//! the GPU cache.

use alloc::collections::BTreeMap;

use azul_core::{
    dom::{DomId, NodeId},
    dom::ScrollbarOrientation,
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    gpu::{GpuEventChanges, GpuScrollbarOpacityEvent, GpuTransformKeyEvent, GpuValueCache},
    resources::{OpacityKey, TransformKey},
    task::{Duration, Instant, SystemTimeDiff},
    transform::{ComputedTransform3D, RotationMode},
};

use crate::{
    managers::scroll_state::ScrollManager,
    solver3::{
        layout_tree::LayoutTree,
        scrollbar::{ScrollbarRequirements, compute_scrollbar_geometry},
    },
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
    /// Per-scrollbar fade state: (DomId, NodeId) → last activity time
    pub fade_states: BTreeMap<(DomId, NodeId), ScrollbarFadeState>,
}

impl Default for GpuStateManager {
    fn default() -> Self {
        Self::new(
            Duration::System(SystemTimeDiff::from_millis(DEFAULT_FADE_DELAY_MS)),
            Duration::System(SystemTimeDiff::from_millis(DEFAULT_FADE_DURATION_MS)),
        )
    }
}

/// Internal state for tracking per-scrollbar fade activity.
///
/// Stores the last scroll activity time so that `tick()` can
/// independently recalculate opacity values each frame without
/// needing access to the `ScrollManager`.
#[derive(Debug, Clone)]
pub struct ScrollbarFadeState {
    /// Timestamp of last scroll activity for this scrollbar
    pub last_activity_time: Option<Instant>,
    /// Whether this scrollbar needs vertical fading
    pub needs_vertical: bool,
    /// Whether this scrollbar needs horizontal fading
    pub needs_horizontal: bool,
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
            fade_states: BTreeMap::new(),
        }
    }

    /// Advances GPU state by one tick, interpolating animated opacity values.
    ///
    /// This should be called each frame to update opacity transitions
    /// for smooth scrollbar fading. Returns whether a repaint is needed
    /// (i.e., any opacity value changed).
    pub fn tick(&mut self, now: Instant) -> GpuTickResult {
        let mut needs_repaint = false;
        let fade_delay = self.fade_delay;
        let fade_duration = self.fade_duration;

        // Iterate over all tracked fade states and recalculate opacity
        for (&(dom_id, node_id), fade_state) in &self.fade_states {
            let cache = match self.caches.get_mut(&dom_id) {
                Some(c) => c,
                None => continue,
            };

            let opacity = Self::calculate_fade_opacity(
                fade_state.last_activity_time.as_ref(),
                &now,
                fade_delay,
                fade_duration,
            );

            // Update vertical opacity
            if fade_state.needs_vertical {
                let key = (dom_id, node_id);
                if let Some(old_val) = cache.scrollbar_v_opacity_values.get(&key) {
                    if (old_val - opacity).abs() > 0.001 {
                        cache.scrollbar_v_opacity_values.insert(key, opacity);
                        needs_repaint = true;
                    }
                }
            }

            // Update horizontal opacity
            if fade_state.needs_horizontal {
                let key = (dom_id, node_id);
                if let Some(old_val) = cache.scrollbar_h_opacity_values.get(&key) {
                    if (old_val - opacity).abs() > 0.001 {
                        cache.scrollbar_h_opacity_values.insert(key, opacity);
                        needs_repaint = true;
                    }
                }
            }
        }

        GpuTickResult {
            needs_repaint,
            changes: GpuEventChanges::empty(),
        }
    }

    /// Calculate scrollbar opacity based on elapsed time since last activity.
    ///
    /// Three-phase model:
    /// 1. During `fade_delay`: fully visible (1.0)
    /// 2. During `fade_duration` after delay: linear fade from 1.0 to 0.0
    /// 3. After delay + duration: fully hidden (0.0)
    fn calculate_fade_opacity(
        last_activity: Option<&Instant>,
        now: &Instant,
        fade_delay: Duration,
        fade_duration: Duration,
    ) -> f32 {
        let Some(last_activity) = last_activity else {
            return 0.0;
        };

        let time_since_activity = now.duration_since(last_activity);

        // Phase 1: Scrollbar stays fully visible during fade_delay
        if time_since_activity.div(&fade_delay) < 1.0 {
            return 1.0;
        }

        // Phase 2: Fade out over fade_duration
        let time_into_fade = time_since_activity.div(&fade_delay) - 1.0;
        let fade_progress = (time_into_fade * fade_duration.div(&fade_duration)).min(1.0);

        // Phase 3: Fully faded
        (1.0 - fade_progress).max(0.0)
    }

    /// Record scroll activity for a scrollbar node, resetting the fade timer.
    ///
    /// This should be called whenever scroll activity occurs to keep the
    /// scrollbar visible and reset the fade-out timer.
    pub fn record_scroll_activity(
        &mut self,
        dom_id: DomId,
        node_id: NodeId,
        now: Instant,
        needs_vertical: bool,
        needs_horizontal: bool,
    ) {
        let state = self.fade_states
            .entry((dom_id, node_id))
            .or_insert(ScrollbarFadeState {
                last_activity_time: None,
                needs_vertical: false,
                needs_horizontal: false,
            });
        state.last_activity_time = Some(now);
        state.needs_vertical = needs_vertical;
        state.needs_horizontal = needs_horizontal;
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

            // Compute inner_rect (padding-box) by subtracting borders from used_size
            let border_box_size = node.used_size.unwrap_or_default();
            let border = &node.box_props.border;
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
            let content_size = node.get_content_size();

            if scrollbar_info.needs_vertical {
                // Scrollbar thickness: use the layout-reserved width if available,
                // otherwise fall back to a default. scrollbar_info.scrollbar_width
                // represents the horizontal scrollbar's reserved height, and
                // scrollbar_info.scrollbar_height represents the vertical scrollbar's
                // reserved width — BUT for overlay scrollbars these are 0.
                // Use a sensible rendering default when 0.
                let scrollbar_width_px = if scrollbar_info.scrollbar_height > 0.0 {
                    scrollbar_info.scrollbar_height
                } else {
                    16.0 // default rendering width for overlay scrollbars
                };

                let v_geom = compute_scrollbar_geometry(
                    ScrollbarOrientation::Vertical,
                    inner_rect,
                    content_size,
                    scroll_offset.y,
                    scrollbar_width_px,
                    scrollbar_info.needs_horizontal,
                );

                let transform =
                    ComputedTransform3D::new_translation(0.0, v_geom.thumb_offset, 0.0);
                update_transform_key(gpu_cache, &mut changes, dom_id, node_id, transform);
            }

            if scrollbar_info.needs_horizontal {
                let scrollbar_width_px = if scrollbar_info.scrollbar_width > 0.0 {
                    scrollbar_info.scrollbar_width
                } else {
                    16.0 // default rendering width for overlay scrollbars
                };

                let h_geom = compute_scrollbar_geometry(
                    ScrollbarOrientation::Horizontal,
                    inner_rect,
                    content_size,
                    scroll_offset.x,
                    scrollbar_width_px,
                    scrollbar_info.needs_vertical,
                );

                let transform =
                    ComputedTransform3D::new_translation(h_geom.thumb_offset, 0.0, 0.0);
                update_h_transform_key(gpu_cache, &mut changes, dom_id, node_id, transform);
            }
        }

        changes
    }

    /// Returns a clone of all GPU value caches.
    pub fn get_gpu_value_cache(&self) -> BTreeMap<DomId, GpuValueCache> {
        self.caches.clone()
    }
}

/// Updates or creates a vertical scrollbar transform key in the GPU cache.
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

/// Updates or creates a horizontal scrollbar transform key in the GPU cache.
fn update_h_transform_key(
    gpu_cache: &mut GpuValueCache,
    changes: &mut GpuEventChanges,
    dom_id: DomId,
    node_id: NodeId,
    transform: ComputedTransform3D,
) {
    if let Some(existing_transform) = gpu_cache.h_current_transform_values.get(&node_id) {
        if *existing_transform != transform {
            let transform_key = gpu_cache.h_transform_keys[&node_id];
            changes
                .transform_key_changes
                .push(GpuTransformKeyEvent::Changed(
                    node_id,
                    transform_key,
                    *existing_transform,
                    transform,
                ));
            gpu_cache
                .h_current_transform_values
                .insert(node_id, transform);
        }
    } else {
        let transform_key = TransformKey::unique();
        gpu_cache.h_transform_keys.insert(node_id, transform_key);
        gpu_cache
            .h_current_transform_values
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
