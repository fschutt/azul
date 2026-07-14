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

#[cfg(test)]
mod autotest_generated {
    use azul_core::{
        dom::FormattingContext,
        resources::OpacityKey,
        task::{Instant, SystemTick, SystemTickDiff},
    };

    use super::*;
    use crate::{
        managers::{NodeIdMap, NodeIdRemap},
        solver3::{
            geometry::PackedBoxProps,
            layout_tree::{LayoutNodeCold, LayoutNodeHot, LayoutNodeWarm},
            scrollbar::ScrollbarRequirements,
        },
    };

    // ------------------------------------------------------------------
    // Fixtures
    // ------------------------------------------------------------------

    fn dom(inner: usize) -> DomId {
        DomId { inner }
    }

    fn t0() -> Instant {
        Instant::Tick(SystemTick::new(0))
    }

    fn millis(ms: u64) -> Duration {
        Duration::System(SystemTimeDiff::from_millis(ms))
    }

    /// Translation transform — the only shape `update_scrollbar_transforms`
    /// ever produces.
    fn tx(x: f32, y: f32) -> ComputedTransform3D {
        ComputedTransform3D::new_translation(x, y, 0.0)
    }

    fn hot(dom_node_id: Option<NodeId>, used: Option<LogicalSize>) -> LayoutNodeHot {
        LayoutNodeHot {
            box_props: PackedBoxProps::default(),
            dom_node_id,
            used_size: used,
            formatting_context: FormattingContext::Block {
                establishes_new_context: true,
            },
            parent: None,
        }
    }

    fn warm_node(
        scrollbar_info: Option<ScrollbarRequirements>,
        content: Option<LogicalSize>,
    ) -> LayoutNodeWarm {
        LayoutNodeWarm {
            scrollbar_info,
            overflow_content_size: content,
            ..Default::default()
        }
    }

    /// A classic (space-reserving) vertical scrollbar with an explicit visual
    /// width — the un-ambiguous case where no field-fallback logic kicks in.
    fn v_scrollbar() -> ScrollbarRequirements {
        ScrollbarRequirements {
            needs_horizontal: false,
            needs_vertical: true,
            scrollbar_width: 16.0,
            scrollbar_height: 16.0,
            visual_width_px: 16.0,
        }
    }

    fn h_scrollbar() -> ScrollbarRequirements {
        ScrollbarRequirements {
            needs_horizontal: true,
            needs_vertical: false,
            scrollbar_width: 16.0,
            scrollbar_height: 16.0,
            visual_width_px: 16.0,
        }
    }

    fn tree(nodes: Vec<LayoutNodeHot>, warm: Vec<LayoutNodeWarm>) -> LayoutTree {
        let n = nodes.len();
        LayoutTree {
            nodes,
            warm,
            cold: vec![LayoutNodeCold::default(); n],
            root: 0,
            dom_to_layout: BTreeMap::new(),
            children_arena: Vec::new(),
            children_offsets: vec![(0, 0); n],
            subtree_needs_intrinsic: Vec::new(),
        }
    }

    /// Single scrollable node: 100×100 border-box, `content` content-box.
    fn one_node_tree(sb: ScrollbarRequirements, content: LogicalSize) -> LayoutTree {
        tree(
            vec![hot(
                Some(NodeId::new(1)),
                Some(LogicalSize::new(100.0, 100.0)),
            )],
            vec![warm_node(Some(sb), Some(content))],
        )
    }

    /// The y-translation of the single transform event emitted, if any.
    fn sole_added_y(changes: &GpuEventChanges) -> f32 {
        assert_eq!(changes.transform_key_changes.len(), 1);
        match changes.transform_key_changes[0] {
            GpuTransformKeyEvent::Added(_, _, t) => t.m[3][1],
            ref other => panic!("expected Added, got {other:?}"),
        }
    }

    fn sole_added_x(changes: &GpuEventChanges) -> f32 {
        assert_eq!(changes.transform_key_changes.len(), 1);
        match changes.transform_key_changes[0] {
            GpuTransformKeyEvent::Added(_, _, t) => t.m[3][0],
            ref other => panic!("expected Added, got {other:?}"),
        }
    }

    // ------------------------------------------------------------------
    // GpuStateManager::new / Default — constructor invariants
    // ------------------------------------------------------------------

    #[test]
    fn new_stores_both_durations_and_starts_empty_and_idle() {
        let m = GpuStateManager::new(millis(1), millis(2));
        assert_eq!(m.fade_delay, millis(1));
        assert_eq!(m.fade_duration, millis(2));
        assert!(m.caches.is_empty());
        assert!(!m.scrollbar_fade_active);
        assert!(m.pending_changes.is_empty());
    }

    #[test]
    fn new_survives_zero_and_u64_max_durations_without_panicking() {
        // Zero fade window: the fade math elsewhere divides by fade_duration,
        // so a 0 duration must at least be constructible without panicking here.
        let zero = GpuStateManager::new(millis(0), millis(0));
        assert_eq!(zero.fade_delay, millis(0));
        assert_eq!(zero.fade_duration, millis(0));

        // from_millis(u64::MAX) is ~584 million years — no overflow, no panic.
        let huge = GpuStateManager::new(millis(u64::MAX), millis(u64::MAX));
        assert_eq!(huge.fade_delay, millis(u64::MAX));
        assert!(huge.caches.is_empty());
    }

    #[test]
    fn new_accepts_tick_durations_not_just_system_durations() {
        // Duration is an enum; the constructor must not assume the System variant.
        let tick = Duration::Tick(SystemTickDiff { tick_diff: u64::MAX });
        let m = GpuStateManager::new(tick, tick);
        assert_eq!(m.fade_delay, tick);
        assert_eq!(m.fade_duration, tick);
    }

    #[test]
    fn default_matches_the_documented_fade_constants() {
        let m = GpuStateManager::default();
        assert_eq!(m.fade_delay, millis(DEFAULT_FADE_DELAY_MS));
        assert_eq!(m.fade_duration, millis(DEFAULT_FADE_DURATION_MS));
        assert_eq!(DEFAULT_FADE_DELAY_MS, 500);
        assert_eq!(DEFAULT_FADE_DURATION_MS, 200);
    }

    // ------------------------------------------------------------------
    // take_pending_changes
    // ------------------------------------------------------------------

    #[test]
    fn take_pending_changes_drains_the_buffer_and_the_second_take_is_empty() {
        let mut m = GpuStateManager::default();
        m.pending_changes
            .transform_key_changes
            .push(GpuTransformKeyEvent::Added(
                NodeId::new(1),
                TransformKey::unique(),
                tx(0.0, 4.0),
            ));

        let taken = m.take_pending_changes();
        assert_eq!(taken.transform_key_changes.len(), 1);
        // The whole point of `take`: the buffer must not replay next frame.
        assert!(m.pending_changes.is_empty());
        assert!(m.take_pending_changes().is_empty());
    }

    #[test]
    fn take_pending_changes_on_a_fresh_manager_is_an_empty_no_panic() {
        let mut m = GpuStateManager::default();
        for _ in 0..3 {
            assert_eq!(m.take_pending_changes(), GpuEventChanges::empty());
        }
    }

    // ------------------------------------------------------------------
    // get_cache / get_or_create_cache
    // ------------------------------------------------------------------

    #[test]
    fn get_cache_on_an_unknown_dom_returns_none_and_does_not_create_it() {
        let m = GpuStateManager::default();
        assert!(m.get_cache(dom(0)).is_none());
        assert!(m.get_cache(dom(usize::MAX)).is_none());
        assert!(m.caches.is_empty(), "get_cache must not mutate");
    }

    #[test]
    fn get_or_create_cache_is_idempotent_and_never_clobbers_existing_state() {
        let mut m = GpuStateManager::default();
        let node = NodeId::new(7);
        let key = TransformKey::unique();

        m.get_or_create_cache(dom(0)).transform_keys.insert(node, key);
        assert_eq!(m.caches.len(), 1);

        // Second call must hand back the *same* cache, not a fresh default one —
        // otherwise every frame would mint new GPU keys and leak them.
        let again = m.get_or_create_cache(dom(0));
        assert_eq!(again.transform_keys.get(&node), Some(&key));
        assert_eq!(m.caches.len(), 1);
        assert!(m.get_cache(dom(0)).is_some());
    }

    #[test]
    fn caches_for_distinct_dom_ids_including_usize_max_do_not_alias() {
        let mut m = GpuStateManager::default();
        let node = NodeId::new(0);

        m.get_or_create_cache(dom(0))
            .current_opacity_values
            .insert(node, 0.25);
        m.get_or_create_cache(dom(usize::MAX))
            .current_opacity_values
            .insert(node, 0.75);

        assert_eq!(m.caches.len(), 2);
        assert_eq!(
            m.get_cache(dom(0)).unwrap().current_opacity_values.get(&node),
            Some(&0.25)
        );
        assert_eq!(
            m.get_cache(dom(usize::MAX))
                .unwrap()
                .current_opacity_values
                .get(&node),
            Some(&0.75)
        );
    }

    // ------------------------------------------------------------------
    // update_scrollbar_transform_key (private)
    // ------------------------------------------------------------------

    #[test]
    fn first_update_emits_added_and_populates_both_the_key_and_value_map() {
        let mut cache = GpuValueCache::default();
        let mut changes = GpuEventChanges::empty();
        let node = NodeId::new(3);
        let t = tx(0.0, 12.0);

        update_scrollbar_transform_key(
            &mut cache,
            &mut changes,
            node,
            t,
            ScrollbarOrientation::Vertical,
        );

        let key = cache.transform_keys.get(&node).copied().expect("key stored");
        assert_eq!(cache.current_transform_values.get(&node), Some(&t));
        assert_eq!(
            changes.transform_key_changes,
            vec![GpuTransformKeyEvent::Added(node, key, t)]
        );
    }

    #[test]
    fn re_updating_with_an_identical_transform_emits_nothing() {
        // The cache exists to suppress redundant GPU traffic: a node that did not
        // move must produce zero events, forever.
        let mut cache = GpuValueCache::default();
        let node = NodeId::new(3);
        let t = tx(0.0, 12.0);

        let mut first = GpuEventChanges::empty();
        update_scrollbar_transform_key(
            &mut cache,
            &mut first,
            node,
            t,
            ScrollbarOrientation::Vertical,
        );
        let key = cache.transform_keys.get(&node).copied().unwrap();

        for _ in 0..10 {
            let mut again = GpuEventChanges::empty();
            update_scrollbar_transform_key(
                &mut cache,
                &mut again,
                node,
                t,
                ScrollbarOrientation::Vertical,
            );
            assert!(again.is_empty(), "unchanged transform must not re-emit");
        }
        // ...and the key must be stable across those no-op frames.
        assert_eq!(cache.transform_keys.get(&node).copied(), Some(key));
    }

    #[test]
    fn changing_the_transform_emits_changed_with_old_and_new_and_reuses_the_key() {
        let mut cache = GpuValueCache::default();
        let node = NodeId::new(3);
        let old = tx(0.0, 12.0);
        let new = tx(0.0, 40.0);

        let mut changes = GpuEventChanges::empty();
        update_scrollbar_transform_key(
            &mut cache,
            &mut changes,
            node,
            old,
            ScrollbarOrientation::Vertical,
        );
        let key = cache.transform_keys.get(&node).copied().unwrap();

        let mut changes = GpuEventChanges::empty();
        update_scrollbar_transform_key(
            &mut cache,
            &mut changes,
            node,
            new,
            ScrollbarOrientation::Vertical,
        );

        assert_eq!(
            changes.transform_key_changes,
            vec![GpuTransformKeyEvent::Changed(node, key, old, new)]
        );
        // Key is reused (not re-minted) and the stored value advances.
        assert_eq!(cache.transform_keys.get(&node).copied(), Some(key));
        assert_eq!(cache.current_transform_values.get(&node), Some(&new));
    }

    #[test]
    fn vertical_and_horizontal_keys_for_the_same_node_are_fully_independent() {
        // Both orientations key off the same NodeId but must land in disjoint
        // maps — otherwise a node with both scrollbars would have its vertical
        // thumb overwritten by its horizontal one (SpatialTreeItemKey collision).
        let mut cache = GpuValueCache::default();
        let mut changes = GpuEventChanges::empty();
        let node = NodeId::new(5);
        let v = tx(0.0, 10.0);
        let h = tx(20.0, 0.0);

        update_scrollbar_transform_key(
            &mut cache,
            &mut changes,
            node,
            v,
            ScrollbarOrientation::Vertical,
        );
        update_scrollbar_transform_key(
            &mut cache,
            &mut changes,
            node,
            h,
            ScrollbarOrientation::Horizontal,
        );

        assert_eq!(changes.transform_key_changes.len(), 2);
        assert_eq!(cache.current_transform_values.get(&node), Some(&v));
        assert_eq!(cache.h_current_transform_values.get(&node), Some(&h));
        assert_ne!(
            cache.transform_keys.get(&node),
            cache.h_transform_keys.get(&node),
            "the two orientations must not share a TransformKey"
        );
    }

    #[test]
    fn a_value_without_its_key_silently_drops_the_update_and_stays_stale() {
        // Desync #1: current_transform_values has an entry but transform_keys does
        // not. The `let Some(&transform_key) = keys.get(..) else { return }` bails
        // out *before* writing the new value, so the node is stuck at the stale
        // transform and never self-heals — no event, no repair, forever.
        let mut cache = GpuValueCache::default();
        let node = NodeId::new(3);
        let stale = tx(0.0, 5.0);
        cache.current_transform_values.insert(node, stale);

        let mut changes = GpuEventChanges::empty();
        update_scrollbar_transform_key(
            &mut cache,
            &mut changes,
            node,
            tx(0.0, 99.0),
            ScrollbarOrientation::Vertical,
        );

        assert!(changes.is_empty(), "no event is emitted for a keyless value");
        assert_eq!(
            cache.current_transform_values.get(&node),
            Some(&stale),
            "the value is left stale rather than repaired"
        );
        assert!(
            !cache.transform_keys.contains_key(&node),
            "and no key is minted to recover"
        );
    }

    #[test]
    fn a_key_without_its_value_mints_a_fresh_key_and_orphans_the_old_one() {
        // Desync #2 (the mirror image): transform_keys has an entry but
        // current_transform_values does not. The else-branch unconditionally
        // overwrites the key, so the previously-published TransformKey is
        // orphaned — the renderer still holds it, nothing ever removes it.
        let mut cache = GpuValueCache::default();
        let node = NodeId::new(3);
        let orphan = TransformKey::unique();
        cache.transform_keys.insert(node, orphan);

        let mut changes = GpuEventChanges::empty();
        let t = tx(0.0, 7.0);
        update_scrollbar_transform_key(
            &mut cache,
            &mut changes,
            node,
            t,
            ScrollbarOrientation::Vertical,
        );

        let fresh = cache.transform_keys.get(&node).copied().unwrap();
        assert_ne!(fresh, orphan, "a brand-new key replaces the orphan");
        assert_eq!(
            changes.transform_key_changes,
            vec![GpuTransformKeyEvent::Added(node, fresh, t)],
            "and it is announced as Added, never as Removed(orphan)"
        );
    }

    #[test]
    fn a_nan_transform_never_converges_and_re_emits_changed_every_single_call() {
        // ComputedTransform3D derives PartialEq over f32s, so NaN != NaN. Once a
        // NaN thumb offset lands in the cache, `*existing != transform` is true on
        // every subsequent call *even for the bit-identical transform* — the cache
        // can never converge and the renderer gets an unbounded stream of Changed
        // events, one per frame, for a node that is not moving.
        let mut cache = GpuValueCache::default();
        let node = NodeId::new(3);
        let nan = tx(0.0, f32::NAN);

        let mut changes = GpuEventChanges::empty();
        update_scrollbar_transform_key(
            &mut cache,
            &mut changes,
            node,
            nan,
            ScrollbarOrientation::Vertical,
        );
        assert_eq!(changes.transform_key_changes.len(), 1, "Added");

        // Feed the exact same NaN transform back in 5 more times.
        for _ in 0..5 {
            update_scrollbar_transform_key(
                &mut cache,
                &mut changes,
                node,
                nan,
                ScrollbarOrientation::Vertical,
            );
        }
        assert_eq!(
            changes.transform_key_changes.len(),
            6,
            "NaN re-emits Changed on every call instead of settling"
        );
        assert!(cache.current_transform_values[&node].m[3][1].is_nan());
    }

    // ------------------------------------------------------------------
    // remap_hashmap
    // ------------------------------------------------------------------

    /// `NodeIdMap` in which nothing survived the rebuild.
    fn no_survivors() -> NodeIdMap {
        NodeIdMap::from_pairs(Vec::<(NodeId, NodeId)>::new())
    }

    #[test]
    fn remap_hashmap_on_empty_inputs_is_a_no_op() {
        let mut map: HashMap<NodeId, u32> = HashMap::new();
        remap_hashmap(&mut map, &no_survivors());
        assert!(map.is_empty());
    }

    #[test]
    fn remap_hashmap_drops_entries_for_unmounted_nodes() {
        let mut map: HashMap<NodeId, u32> = HashMap::new();
        map.insert(NodeId::new(1), 10);
        map.insert(NodeId::new(2), 20);
        map.insert(NodeId::new(3), 30);

        // Only node 2 survives the rebuild (as node 9).
        remap_hashmap(
            &mut map,
            &NodeIdMap::from_pairs([(NodeId::new(2), NodeId::new(9))]),
        );

        assert_eq!(map.len(), 1);
        assert_eq!(map.get(&NodeId::new(9)), Some(&20));
        assert!(!map.contains_key(&NodeId::new(1)));
        assert!(!map.contains_key(&NodeId::new(2)), "old id must not linger");
    }

    #[test]
    fn remap_hashmap_survives_a_full_id_swap_without_losing_entries() {
        // 1 -> 2 and 2 -> 1 simultaneously. An in-place rewrite would clobber one
        // of them depending on iteration order; the take-then-reinsert must not.
        let mut map: HashMap<NodeId, u32> = HashMap::new();
        map.insert(NodeId::new(1), 111);
        map.insert(NodeId::new(2), 222);

        remap_hashmap(
            &mut map,
            &NodeIdMap::from_pairs([
                (NodeId::new(1), NodeId::new(2)),
                (NodeId::new(2), NodeId::new(1)),
            ]),
        );

        assert_eq!(map.len(), 2, "no entry may be lost to the swap");
        assert_eq!(map.get(&NodeId::new(2)), Some(&111));
        assert_eq!(map.get(&NodeId::new(1)), Some(&222));
    }

    #[test]
    fn remap_hashmap_collapses_two_old_ids_that_alias_onto_one_new_id() {
        // A malformed NodeIdMap (two survivors claiming the same new slot) must
        // not panic — one entry silently wins. Pinning the *shape* of that loss:
        // the map shrinks rather than corrupting.
        let mut map: HashMap<NodeId, u32> = HashMap::new();
        map.insert(NodeId::new(1), 111);
        map.insert(NodeId::new(2), 222);

        remap_hashmap(
            &mut map,
            &NodeIdMap::from_pairs([
                (NodeId::new(1), NodeId::new(5)),
                (NodeId::new(2), NodeId::new(5)),
            ]),
        );

        assert_eq!(map.len(), 1, "the alias collapses both entries into one");
        let survivor = map.get(&NodeId::new(5)).copied().unwrap();
        assert!(survivor == 111 || survivor == 222);
    }

    #[test]
    fn remap_hashmap_handles_node_id_max_without_overflowing() {
        let mut map: HashMap<NodeId, u32> = HashMap::new();
        map.insert(NodeId::new(usize::MAX), 1);
        map.insert(NodeId::ZERO, 2);

        remap_hashmap(
            &mut map,
            &NodeIdMap::from_pairs([
                (NodeId::new(usize::MAX), NodeId::ZERO),
                (NodeId::ZERO, NodeId::new(usize::MAX)),
            ]),
        );

        assert_eq!(map.get(&NodeId::ZERO), Some(&1));
        assert_eq!(map.get(&NodeId::new(usize::MAX)), Some(&2));
    }

    // ------------------------------------------------------------------
    // remap_dom_hashmap
    // ------------------------------------------------------------------

    #[test]
    fn remap_dom_hashmap_leaves_other_doms_completely_untouched() {
        let mut map: HashMap<(DomId, NodeId), u32> = HashMap::new();
        map.insert((dom(0), NodeId::new(1)), 1);
        map.insert((dom(1), NodeId::new(1)), 2);

        // Reconcile DOM 0 only: 1 -> 4. DOM 1's node 1 is *not* mentioned in the
        // map, but it must survive anyway — this reconciliation says nothing
        // about a different DOM.
        remap_dom_hashmap(
            &mut map,
            dom(0),
            &NodeIdMap::from_pairs([(NodeId::new(1), NodeId::new(4))]),
        );

        assert_eq!(map.len(), 2);
        assert_eq!(map.get(&(dom(0), NodeId::new(4))), Some(&1));
        assert_eq!(
            map.get(&(dom(1), NodeId::new(1))),
            Some(&2),
            "a foreign DOM's entry must not be dropped as 'unmounted'"
        );
    }

    #[test]
    fn remap_dom_hashmap_drops_only_the_target_doms_unmounted_nodes() {
        let mut map: HashMap<(DomId, NodeId), u32> = HashMap::new();
        map.insert((dom(0), NodeId::new(1)), 1); // survives -> 4
        map.insert((dom(0), NodeId::new(2)), 2); // unmounted -> dropped
        map.insert((dom(1), NodeId::new(2)), 3); // other DOM -> kept as-is

        remap_dom_hashmap(
            &mut map,
            dom(0),
            &NodeIdMap::from_pairs([(NodeId::new(1), NodeId::new(4))]),
        );

        assert_eq!(map.len(), 2);
        assert_eq!(map.get(&(dom(0), NodeId::new(4))), Some(&1));
        assert!(!map.contains_key(&(dom(0), NodeId::new(2))));
        assert_eq!(map.get(&(dom(1), NodeId::new(2))), Some(&3));
    }

    #[test]
    fn remap_dom_hashmap_does_not_let_a_remap_collide_across_doms() {
        // (dom0, 1) -> (dom0, 2), while (dom1, 2) already exists. Different DOM,
        // so the tuple keys stay distinct and neither entry is lost.
        let mut map: HashMap<(DomId, NodeId), u32> = HashMap::new();
        map.insert((dom(0), NodeId::new(1)), 11);
        map.insert((dom(1), NodeId::new(2)), 22);

        remap_dom_hashmap(
            &mut map,
            dom(0),
            &NodeIdMap::from_pairs([(NodeId::new(1), NodeId::new(2))]),
        );

        assert_eq!(map.len(), 2);
        assert_eq!(map.get(&(dom(0), NodeId::new(2))), Some(&11));
        assert_eq!(map.get(&(dom(1), NodeId::new(2))), Some(&22));
    }

    #[test]
    fn remap_dom_hashmap_survives_a_shift_chain_within_one_dom() {
        // 1->2 and 2->3 at once. Resolving from the *old* snapshot means the
        // 1->2 insert cannot clobber the entry that used to live at 2.
        let mut map: HashMap<(DomId, NodeId), u32> = HashMap::new();
        map.insert((dom(0), NodeId::new(1)), 11);
        map.insert((dom(0), NodeId::new(2)), 22);

        remap_dom_hashmap(
            &mut map,
            dom(0),
            &NodeIdMap::from_pairs([
                (NodeId::new(1), NodeId::new(2)),
                (NodeId::new(2), NodeId::new(3)),
            ]),
        );

        assert_eq!(map.len(), 2);
        assert_eq!(map.get(&(dom(0), NodeId::new(2))), Some(&11));
        assert_eq!(map.get(&(dom(0), NodeId::new(3))), Some(&22));
    }

    // ------------------------------------------------------------------
    // NodeIdRemap for GpuStateManager
    // ------------------------------------------------------------------

    #[test]
    fn remap_node_ids_for_a_dom_with_no_cache_is_a_no_op() {
        let mut m = GpuStateManager::default();
        m.remap_node_ids(
            dom(3),
            &NodeIdMap::from_pairs([(NodeId::new(1), NodeId::new(2))]),
        );
        assert!(m.caches.is_empty());
    }

    #[test]
    fn remap_node_ids_rewrites_every_one_of_the_twelve_cache_maps() {
        // A single map left un-remapped is a stale scrollbar thumb / stuck
        // animation, so assert all twelve move together.
        let mut m = GpuStateManager::default();
        let old = NodeId::new(1);
        let new = NodeId::new(8);
        let d = dom(0);
        {
            let c = m.get_or_create_cache(d);
            c.transform_keys.insert(old, TransformKey::unique());
            c.current_transform_values.insert(old, tx(0.0, 1.0));
            c.h_transform_keys.insert(old, TransformKey::unique());
            c.h_current_transform_values.insert(old, tx(2.0, 0.0));
            c.css_transform_keys.insert(old, TransformKey::unique());
            c.css_current_transform_values.insert(old, tx(3.0, 3.0));
            c.opacity_keys.insert(old, OpacityKey::unique());
            c.current_opacity_values.insert(old, 0.5);
            c.scrollbar_v_opacity_keys.insert((d, old), OpacityKey::unique());
            c.scrollbar_h_opacity_keys.insert((d, old), OpacityKey::unique());
            c.scrollbar_v_opacity_values.insert((d, old), 0.25);
            c.scrollbar_h_opacity_values.insert((d, old), 0.75);
        }

        m.remap_node_ids(d, &NodeIdMap::from_pairs([(old, new)]));

        let c = m.get_cache(d).unwrap();
        assert!(c.transform_keys.contains_key(&new));
        assert_eq!(c.current_transform_values.get(&new), Some(&tx(0.0, 1.0)));
        assert!(c.h_transform_keys.contains_key(&new));
        assert_eq!(c.h_current_transform_values.get(&new), Some(&tx(2.0, 0.0)));
        assert!(c.css_transform_keys.contains_key(&new));
        assert_eq!(c.css_current_transform_values.get(&new), Some(&tx(3.0, 3.0)));
        assert!(c.opacity_keys.contains_key(&new));
        assert_eq!(c.current_opacity_values.get(&new), Some(&0.5));
        assert!(c.scrollbar_v_opacity_keys.contains_key(&(d, new)));
        assert!(c.scrollbar_h_opacity_keys.contains_key(&(d, new)));
        assert_eq!(c.scrollbar_v_opacity_values.get(&(d, new)), Some(&0.25));
        assert_eq!(c.scrollbar_h_opacity_values.get(&(d, new)), Some(&0.75));

        // ...and nothing is left behind under the old id.
        assert!(!c.transform_keys.contains_key(&old));
        assert!(!c.current_opacity_values.contains_key(&old));
        assert!(!c.scrollbar_v_opacity_values.contains_key(&(d, old)));
    }

    #[test]
    fn remap_node_ids_drops_the_gpu_keys_of_an_unmounted_node() {
        let mut m = GpuStateManager::default();
        let gone = NodeId::new(1);
        let d = dom(0);
        {
            let c = m.get_or_create_cache(d);
            c.transform_keys.insert(gone, TransformKey::unique());
            c.current_transform_values.insert(gone, tx(0.0, 1.0));
            c.scrollbar_v_opacity_values.insert((d, gone), 1.0);
        }

        // Empty map == every node unmounted.
        m.remap_node_ids(d, &no_survivors());

        let c = m.get_cache(d).unwrap();
        assert!(c.transform_keys.is_empty());
        assert!(c.current_transform_values.is_empty());
        assert!(c.scrollbar_v_opacity_values.is_empty());
    }

    // ------------------------------------------------------------------
    // update_scrollbar_transforms
    // ------------------------------------------------------------------

    #[test]
    fn update_scrollbar_transforms_on_an_empty_tree_yields_no_events() {
        let mut m = GpuStateManager::default();
        let sm = ScrollManager::new();
        let t = tree(Vec::new(), Vec::new());

        let changes = m.update_scrollbar_transforms(dom(0), &sm, &t);
        assert!(changes.is_empty());
        // ...but the cache is created (get_or_create_cache runs unconditionally).
        assert!(m.get_cache(dom(0)).is_some());
    }

    #[test]
    fn nodes_without_scrollbar_info_or_without_a_dom_node_id_are_skipped() {
        let mut m = GpuStateManager::default();
        let sm = ScrollManager::new();
        let t = tree(
            vec![
                // has scrollbar info but is an anonymous box (no dom_node_id)
                hot(None, Some(LogicalSize::new(100.0, 100.0))),
                // has a dom_node_id but no scrollbar info
                hot(Some(NodeId::new(2)), Some(LogicalSize::new(100.0, 100.0))),
            ],
            vec![
                warm_node(Some(v_scrollbar()), Some(LogicalSize::new(100.0, 1000.0))),
                warm_node(None, None),
            ],
        );

        let changes = m.update_scrollbar_transforms(dom(0), &sm, &t);
        assert!(changes.is_empty());
        assert!(m.get_cache(dom(0)).unwrap().transform_keys.is_empty());
    }

    #[test]
    fn a_warm_array_shorter_than_the_node_array_does_not_index_out_of_bounds() {
        // Mismatched SoA lengths: `warm(idx)` returns None for the tail nodes and
        // they must be skipped, not panic.
        let mut m = GpuStateManager::default();
        let sm = ScrollManager::new();
        let t = tree(
            vec![
                hot(Some(NodeId::new(1)), Some(LogicalSize::new(100.0, 100.0))),
                hot(Some(NodeId::new(2)), Some(LogicalSize::new(100.0, 100.0))),
            ],
            // only one warm entry for two hot nodes
            vec![warm_node(Some(v_scrollbar()), Some(LogicalSize::new(100.0, 1000.0)))],
        );

        let changes = m.update_scrollbar_transforms(dom(0), &sm, &t);
        assert_eq!(changes.transform_key_changes.len(), 1, "only node 0 is seen");
    }

    #[test]
    fn a_vertical_scrollbar_at_scroll_zero_parks_the_thumb_at_offset_zero() {
        let mut m = GpuStateManager::default();
        let sm = ScrollManager::new();
        let t = one_node_tree(v_scrollbar(), LogicalSize::new(100.0, 1000.0));

        let changes = m.update_scrollbar_transforms(dom(0), &sm, &t);
        // No scroll state registered at all -> get_current_offset() is None ->
        // unwrap_or_default() -> (0, 0). Thumb sits at the top.
        assert_eq!(sole_added_y(&changes), 0.0);
    }

    #[test]
    fn scrolling_to_the_bottom_drives_the_thumb_to_the_end_of_the_usable_track() {
        // inner 100x100, content 100x1000, 16px bar with 16px buttons:
        //   usable_track = 100 - 2*16 = 68
        //   thumb        = max(68 * (100/1000), 16*2) = 32
        //   max_scroll   = 1000 - 100 = 900
        // At full scroll the thumb must land exactly at 68 - 32 = 36, never past it.
        let mut m = GpuStateManager::default();
        let mut sm = ScrollManager::new();
        sm.set_scroll_position_unclamped(
            dom(0),
            NodeId::new(1),
            LogicalPosition::new(0.0, 900.0),
            t0(),
        );
        let t = one_node_tree(v_scrollbar(), LogicalSize::new(100.0, 1000.0));

        let y = sole_added_y(&m.update_scrollbar_transforms(dom(0), &sm, &t));
        assert!((y - 36.0).abs() < 0.01, "expected thumb at 36.0, got {y}");
    }

    #[test]
    fn an_overscrolled_offset_clamps_the_thumb_instead_of_running_off_the_track() {
        // Rubber-banding pushes the offset far past max_scroll; scroll_ratio is
        // clamped to [0, 1] so the thumb must stop at the same 36.0.
        let mut m = GpuStateManager::default();
        let mut sm = ScrollManager::new();
        sm.set_scroll_position_unclamped(
            dom(0),
            NodeId::new(1),
            LogicalPosition::new(0.0, 1.0e9),
            t0(),
        );
        let t = one_node_tree(v_scrollbar(), LogicalSize::new(100.0, 1000.0));

        let y = sole_added_y(&m.update_scrollbar_transforms(dom(0), &sm, &t));
        assert!((y - 36.0).abs() < 0.01, "overscroll must clamp, got {y}");
    }

    #[test]
    fn a_negative_scroll_offset_is_treated_as_its_absolute_value() {
        // compute_thumb_geometry uses scroll_offset.abs(), so an overscroll *above*
        // the top does not produce a negative thumb offset.
        let mut m = GpuStateManager::default();
        let mut sm = ScrollManager::new();
        sm.set_scroll_position_unclamped(
            dom(0),
            NodeId::new(1),
            LogicalPosition::new(0.0, -900.0),
            t0(),
        );
        let t = one_node_tree(v_scrollbar(), LogicalSize::new(100.0, 1000.0));

        let y = sole_added_y(&m.update_scrollbar_transforms(dom(0), &sm, &t));
        assert!(y >= 0.0, "thumb offset must never go negative, got {y}");
        assert!((y - 36.0).abs() < 0.01);
    }

    #[test]
    fn running_the_same_layout_twice_emits_no_second_event() {
        // The convergence invariant: a static scroll position must not generate
        // GPU traffic every frame.
        let mut m = GpuStateManager::default();
        let sm = ScrollManager::new();
        let t = one_node_tree(v_scrollbar(), LogicalSize::new(100.0, 1000.0));

        let first = m.update_scrollbar_transforms(dom(0), &sm, &t);
        assert_eq!(first.transform_key_changes.len(), 1);

        for _ in 0..5 {
            let again = m.update_scrollbar_transforms(dom(0), &sm, &t);
            assert!(again.is_empty(), "an idle scrollbar must stay silent");
        }
    }

    #[test]
    fn scrolling_after_a_first_pass_emits_changed_and_reuses_the_key() {
        let mut m = GpuStateManager::default();
        let mut sm = ScrollManager::new();
        let t = one_node_tree(v_scrollbar(), LogicalSize::new(100.0, 1000.0));

        m.update_scrollbar_transforms(dom(0), &sm, &t);
        let key = m
            .get_cache(dom(0))
            .unwrap()
            .transform_keys
            .get(&NodeId::new(1))
            .copied()
            .unwrap();

        sm.set_scroll_position_unclamped(
            dom(0),
            NodeId::new(1),
            LogicalPosition::new(0.0, 900.0),
            t0(),
        );
        let changes = m.update_scrollbar_transforms(dom(0), &sm, &t);

        assert_eq!(changes.transform_key_changes.len(), 1);
        match changes.transform_key_changes[0] {
            GpuTransformKeyEvent::Changed(node, k, old, new) => {
                assert_eq!(node, NodeId::new(1));
                assert_eq!(k, key, "the key must be reused across the scroll");
                assert_eq!(old.m[3][1], 0.0);
                assert!((new.m[3][1] - 36.0).abs() < 0.01);
            }
            ref other => panic!("expected Changed, got {other:?}"),
        }
    }

    #[test]
    fn a_horizontal_scrollbar_translates_on_x_and_leaves_y_at_zero() {
        let mut m = GpuStateManager::default();
        let mut sm = ScrollManager::new();
        sm.set_scroll_position_unclamped(
            dom(0),
            NodeId::new(1),
            LogicalPosition::new(900.0, 0.0),
            t0(),
        );
        let t = one_node_tree(h_scrollbar(), LogicalSize::new(1000.0, 100.0));

        let changes = m.update_scrollbar_transforms(dom(0), &sm, &t);
        let x = sole_added_x(&changes);
        assert!((x - 36.0).abs() < 0.01, "expected thumb at x=36.0, got {x}");

        // The horizontal thumb must be filed under the h_* maps, not the v_* ones.
        let c = m.get_cache(dom(0)).unwrap();
        assert!(c.h_transform_keys.contains_key(&NodeId::new(1)));
        assert!(c.transform_keys.is_empty());
    }

    #[test]
    fn a_node_needing_both_scrollbars_gets_two_independent_keys_and_two_events() {
        let mut m = GpuStateManager::default();
        let sm = ScrollManager::new();
        let both = ScrollbarRequirements {
            needs_horizontal: true,
            needs_vertical: true,
            scrollbar_width: 16.0,
            scrollbar_height: 16.0,
            visual_width_px: 16.0,
        };
        let t = one_node_tree(both, LogicalSize::new(1000.0, 1000.0));

        let changes = m.update_scrollbar_transforms(dom(0), &sm, &t);
        assert_eq!(changes.transform_key_changes.len(), 2);

        let c = m.get_cache(dom(0)).unwrap();
        let node = NodeId::new(1);
        let v = c.transform_keys.get(&node).copied().unwrap();
        let h = c.h_transform_keys.get(&node).copied().unwrap();
        assert_ne!(v, h);
    }

    #[test]
    fn borders_wider_than_the_border_box_clamp_the_inner_size_to_zero() {
        // 50x50 border-box with a 100px border on every side would give a -150px
        // inner size; the `.max(0.0)` must clamp it, and the geometry must stay
        // finite (no NaN thumb offset from a negative track).
        let mut m = GpuStateManager::default();
        let sm = ScrollManager::new();
        let mut node = hot(Some(NodeId::new(1)), Some(LogicalSize::new(50.0, 50.0)));
        node.box_props = PackedBoxProps {
            border: [1000, 1000, 1000, 1000], // 100.0 px each, i16 x10 encoding
            ..Default::default()
        };
        let t = tree(
            vec![node],
            vec![warm_node(
                Some(v_scrollbar()),
                Some(LogicalSize::new(100.0, 1000.0)),
            )],
        );

        let y = sole_added_y(&m.update_scrollbar_transforms(dom(0), &sm, &t));
        assert!(y.is_finite(), "a degenerate inner box must not yield {y}");
        assert_eq!(y, 0.0);
    }

    #[test]
    fn a_zero_sized_node_with_zero_content_does_not_divide_by_zero() {
        let mut m = GpuStateManager::default();
        let sm = ScrollManager::new();
        let t = tree(
            vec![hot(Some(NodeId::new(1)), Some(LogicalSize::zero()))],
            vec![warm_node(Some(v_scrollbar()), Some(LogicalSize::zero()))],
        );

        let y = sole_added_y(&m.update_scrollbar_transforms(dom(0), &sm, &t));
        assert!(y.is_finite());
        assert_eq!(y, 0.0);
    }

    #[test]
    fn a_missing_used_size_defaults_to_zero_rather_than_panicking() {
        let mut m = GpuStateManager::default();
        let sm = ScrollManager::new();
        let t = tree(
            vec![hot(Some(NodeId::new(1)), None)],
            vec![warm_node(Some(v_scrollbar()), None)],
        );

        let y = sole_added_y(&m.update_scrollbar_transforms(dom(0), &sm, &t));
        assert!(y.is_finite());
        assert_eq!(y, 0.0);
    }

    #[test]
    fn an_infinite_used_size_produces_a_nan_thumb_offset() {
        // BUG (characterisation): an infinite border-box makes both the viewport
        // and the content length +inf, so compute_thumb_geometry ends at
        //   thumb_offset = (inf - inf) * 0.0 = NaN
        // and gpu_state feeds that NaN straight into a translation matrix with no
        // finite-check. Combined with `a_nan_transform_never_converges_...` above,
        // one infinite used_size means the scrollbar re-emits a Changed event on
        // *every* frame, forever, and WebRender is handed a NaN transform.
        // Asserting the current behaviour so a future finite-guard trips this test.
        let mut m = GpuStateManager::default();
        let sm = ScrollManager::new();
        let t = tree(
            vec![hot(
                Some(NodeId::new(1)),
                Some(LogicalSize::new(f32::INFINITY, f32::INFINITY)),
            )],
            vec![warm_node(Some(v_scrollbar()), None)],
        );

        let y = sole_added_y(&m.update_scrollbar_transforms(dom(0), &sm, &t));
        assert!(y.is_nan(), "expected the documented NaN, got {y}");

        // And it never settles: a second identical pass re-emits Changed.
        let again = m.update_scrollbar_transforms(dom(0), &sm, &t);
        assert_eq!(
            again.transform_key_changes.len(),
            1,
            "NaN keeps the cache from ever converging"
        );
    }

    #[test]
    fn a_nan_used_size_is_sanitised_to_zero_by_the_max_clamp() {
        // Unlike infinity, NaN *is* neutralised: f32::max(NaN, 0.0) == 0.0, so the
        // `.max(0.0)` on the inner size scrubs it before it reaches the geometry.
        let mut m = GpuStateManager::default();
        let sm = ScrollManager::new();
        let t = tree(
            vec![hot(
                Some(NodeId::new(1)),
                Some(LogicalSize::new(f32::NAN, f32::NAN)),
            )],
            vec![warm_node(Some(v_scrollbar()), None)],
        );

        let y = sole_added_y(&m.update_scrollbar_transforms(dom(0), &sm, &t));
        assert!(y.is_finite(), "NaN width/height must be clamped, got {y}");
        assert_eq!(y, 0.0);
    }

    #[test]
    fn overlay_vertical_scrollbars_fall_back_to_the_default_width() {
        // visual_width_px == 0 and no reserved space at all -> the DEFAULT_SCROLLBAR
        // _WIDTH_PX (16.0) fallback with button_size 0:
        //   usable = 100, thumb = max(100*0.1, 32) = 32, offset@full = 100 - 32 = 68
        let mut m = GpuStateManager::default();
        let mut sm = ScrollManager::new();
        sm.set_scroll_position_unclamped(
            dom(0),
            NodeId::new(1),
            LogicalPosition::new(0.0, 900.0),
            t0(),
        );
        let overlay = ScrollbarRequirements {
            needs_horizontal: false,
            needs_vertical: true,
            scrollbar_width: 0.0,
            scrollbar_height: 0.0,
            visual_width_px: 0.0,
        };
        let t = one_node_tree(overlay, LogicalSize::new(100.0, 1000.0));

        let y = sole_added_y(&m.update_scrollbar_transforms(dom(0), &sm, &t));
        assert!((y - 68.0).abs() < 0.01, "expected the 16px default, got {y}");
        assert_eq!(DEFAULT_SCROLLBAR_WIDTH_PX, 16.0);
    }

    #[test]
    fn classic_vertical_scrollbar_overlay_check_reads_the_wrong_reserved_field() {
        // BUG (characterisation). ScrollbarRequirements documents:
        //   scrollbar_width  = layout-reserved width  for a *vertical*   scrollbar
        //   scrollbar_height = layout-reserved height for a *horizontal* scrollbar
        // but the needs_vertical branch tests `scrollbar_height == 0.0` to decide
        // whether the *vertical* bar is an overlay, and falls back to
        // `scrollbar_height` for its width. The two fields are swapped.
        //
        // Repro: a classic, space-reserving vertical-only scrollbar --
        //   scrollbar_width  = 16.0  (16px reserved for the vertical bar)
        //   scrollbar_height =  0.0  (no horizontal bar -> nothing reserved)
        //   visual_width_px  =  0.0  (unset, so the fallback actually runs)
        // is misread as an overlay: button_size collapses to 0, so the usable
        // track is 100 instead of 68 and the thumb travels to 68.0 rather than the
        // correct 36.0 -- the thumb overshoots its own track by ~32px.
        let mut m = GpuStateManager::default();
        let mut sm = ScrollManager::new();
        sm.set_scroll_position_unclamped(
            dom(0),
            NodeId::new(1),
            LogicalPosition::new(0.0, 900.0),
            t0(),
        );
        let classic_vertical_only = ScrollbarRequirements {
            needs_horizontal: false,
            needs_vertical: true,
            scrollbar_width: 16.0,
            scrollbar_height: 0.0,
            visual_width_px: 0.0,
        };
        let t = one_node_tree(classic_vertical_only, LogicalSize::new(100.0, 1000.0));

        let y = sole_added_y(&m.update_scrollbar_transforms(dom(0), &sm, &t));
        assert!(
            (y - 68.0).abs() < 0.01,
            "pinning the buggy value; 36.0 once the field swap is fixed, got {y}"
        );
    }

    #[test]
    fn classic_horizontal_scrollbar_overlay_check_reads_the_wrong_reserved_field() {
        // The mirror image of the above: the needs_horizontal branch tests
        // `scrollbar_width == 0.0` (the *vertical* bar's reserved width) to decide
        // whether the *horizontal* bar is an overlay.
        let mut m = GpuStateManager::default();
        let mut sm = ScrollManager::new();
        sm.set_scroll_position_unclamped(
            dom(0),
            NodeId::new(1),
            LogicalPosition::new(900.0, 0.0),
            t0(),
        );
        let classic_horizontal_only = ScrollbarRequirements {
            needs_horizontal: true,
            needs_vertical: false,
            scrollbar_width: 0.0,
            scrollbar_height: 16.0,
            visual_width_px: 0.0,
        };
        let t = one_node_tree(classic_horizontal_only, LogicalSize::new(1000.0, 100.0));

        let x = sole_added_x(&m.update_scrollbar_transforms(dom(0), &sm, &t));
        assert!(
            (x - 68.0).abs() < 0.01,
            "pinning the buggy value; 36.0 once the field swap is fixed, got {x}"
        );
    }

    #[test]
    fn transforms_are_recorded_per_dom_and_do_not_leak_across_caches() {
        let mut m = GpuStateManager::default();
        let sm = ScrollManager::new();
        let t = one_node_tree(v_scrollbar(), LogicalSize::new(100.0, 1000.0));

        let a = m.update_scrollbar_transforms(dom(0), &sm, &t);
        let b = m.update_scrollbar_transforms(dom(1), &sm, &t);

        // The same layout tree under a different DomId is a *different* cache, so
        // it must Add (not stay silent) and mint its own key.
        assert_eq!(a.transform_key_changes.len(), 1);
        assert_eq!(b.transform_key_changes.len(), 1);
        assert_eq!(m.caches.len(), 2);

        let node = NodeId::new(1);
        let ka = m.get_cache(dom(0)).unwrap().transform_keys[&node];
        let kb = m.get_cache(dom(1)).unwrap().transform_keys[&node];
        assert_ne!(ka, kb, "each DOM must get its own TransformKey");
    }

    #[test]
    fn update_scrollbar_transforms_does_not_touch_pending_changes() {
        // The function *returns* its changes; it must not also stash them, or the
        // renderer would apply every event twice.
        let mut m = GpuStateManager::default();
        let sm = ScrollManager::new();
        let t = one_node_tree(v_scrollbar(), LogicalSize::new(100.0, 1000.0));

        let changes = m.update_scrollbar_transforms(dom(0), &sm, &t);
        assert!(!changes.is_empty());
        assert!(
            m.pending_changes.is_empty(),
            "returned events must not be double-queued"
        );
    }

    #[test]
    fn a_thousand_scrollable_nodes_each_get_exactly_one_distinct_key() {
        let mut m = GpuStateManager::default();
        let sm = ScrollManager::new();
        let n = 1000;
        let t = tree(
            (0..n)
                .map(|i| hot(Some(NodeId::new(i)), Some(LogicalSize::new(100.0, 100.0))))
                .collect(),
            (0..n)
                .map(|_| warm_node(Some(v_scrollbar()), Some(LogicalSize::new(100.0, 1000.0))))
                .collect(),
        );

        let changes = m.update_scrollbar_transforms(dom(0), &sm, &t);
        assert_eq!(changes.transform_key_changes.len(), n);

        let cache = m.get_cache(dom(0)).unwrap();
        assert_eq!(cache.transform_keys.len(), n);
        let mut keys: Vec<_> = cache.transform_keys.values().map(|k| k.id).collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), n, "every node must get a unique TransformKey");
    }

    #[test]
    fn two_layout_nodes_sharing_one_dom_node_id_fight_over_a_single_key() {
        // Anonymous-box splitting can produce two layout nodes pointing at the same
        // DOM node. The cache keys off dom_node_id, not the layout index, so the
        // second node is misread as a *change* to the first: one key, two events
        // in a single pass, and the last node in tree order silently wins.
        //
        //   node 0: inner 100x100, content 100x1000 -> thumb 36.0
        //   node 1: inner 200x200, content 200x2000 -> thumb 68.0
        let mut m = GpuStateManager::default();
        let mut sm = ScrollManager::new();
        sm.set_scroll_position_unclamped(
            dom(0),
            NodeId::new(1),
            LogicalPosition::new(0.0, 900.0),
            t0(),
        );
        let t = tree(
            vec![
                hot(Some(NodeId::new(1)), Some(LogicalSize::new(100.0, 100.0))),
                hot(Some(NodeId::new(1)), Some(LogicalSize::new(200.0, 200.0))),
            ],
            vec![
                warm_node(Some(v_scrollbar()), Some(LogicalSize::new(100.0, 1000.0))),
                warm_node(Some(v_scrollbar()), Some(LogicalSize::new(200.0, 2000.0))),
            ],
        );

        let changes = m.update_scrollbar_transforms(dom(0), &sm, &t);
        assert_eq!(
            m.get_cache(dom(0)).unwrap().transform_keys.len(),
            1,
            "both layout nodes collapse onto one TransformKey"
        );
        assert!(matches!(
            changes.transform_key_changes.as_slice(),
            [
                GpuTransformKeyEvent::Added(..),
                GpuTransformKeyEvent::Changed(..)
            ]
        ));
        // The second node overwrote the first within the same pass.
        let stored = m.get_cache(dom(0)).unwrap().current_transform_values[&NodeId::new(1)];
        assert!((stored.m[3][1] - 68.0).abs() < 0.01);
    }
}
