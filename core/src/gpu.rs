//! GPU value caching for CSS transforms and opacity.
//!
//! This module manages the synchronization between DOM CSS properties (transforms and opacity)
//! and GPU-side keys used by WebRender. It tracks changes to transform and opacity values
//! and generates events when values are added, changed, or removed.
//!
//! # Performance
//!
//! The cache uses CPU feature detection (SSE/AVX on x86_64) to optimize transform calculations.
//! Values are only recalculated when CSS properties change, minimizing GPU updates.
//!
//! # Architecture
//!
//! - `GpuValueCache`: Stores current transform/opacity keys and values for all nodes
//! - `GpuEventChanges`: Contains delta events for transform/opacity changes
//! - `GpuTransformKeyEvent`: Events for transform additions, changes, and removals
//!
//! The cache is synchronized with the `StyledDom` on each frame, generating minimal
//! update events to send to the GPU.

use alloc::vec::Vec;
#[cfg(feature = "std")]
use std::collections::HashMap;
#[cfg(not(feature = "std"))]
use alloc::collections::BTreeMap as HashMap;
use core::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};

use azul_css::props::style::StyleTransformOrigin;

use crate::{
    dom::{DomId, NodeId},
    resources::{OpacityKey, TransformKey},
    styled_dom::StyledDom,
    transform::{ComputedTransform3D, RotationMode, INITIALIZED, USE_AVX, USE_SSE},
};

/// Caches GPU transform and opacity keys and their current values for all nodes.
///
/// This cache stores the WebRender keys and computed values for nodes with
/// CSS transforms or opacity. It's synchronized with the `StyledDom` to detect
/// changes and generate minimal update events.
#[derive(Default, Debug, Clone)]
pub struct GpuValueCache {
    /// Vertical scrollbar thumb transform keys (keyed by scrollable node ID)
    pub transform_keys: HashMap<NodeId, TransformKey>,
    /// Current vertical scrollbar thumb transform values
    pub current_transform_values: HashMap<NodeId, ComputedTransform3D>,
    /// Horizontal scrollbar thumb transform keys (keyed by scrollable node ID)
    pub h_transform_keys: HashMap<NodeId, TransformKey>,
    /// Current horizontal scrollbar thumb transform values
    pub h_current_transform_values: HashMap<NodeId, ComputedTransform3D>,
    /// CSS transform keys (keyed by node ID) — for CSS `transform` property animation.
    /// Separate from scrollbar transform keys to avoid SpatialTreeItemKey collisions.
    pub css_transform_keys: HashMap<NodeId, TransformKey>,
    /// Current CSS transform values (keyed by node ID)
    pub css_current_transform_values: HashMap<NodeId, ComputedTransform3D>,
    /// CSS opacity keys (keyed by node ID)
    pub opacity_keys: HashMap<NodeId, OpacityKey>,
    /// Current CSS opacity values (keyed by node ID)
    pub current_opacity_values: HashMap<NodeId, f32>,
    /// Vertical scrollbar opacity keys (keyed by DOM ID and scrollable node ID)
    pub scrollbar_v_opacity_keys: HashMap<(DomId, NodeId), OpacityKey>,
    /// Horizontal scrollbar opacity keys (keyed by DOM ID and scrollable node ID)
    pub scrollbar_h_opacity_keys: HashMap<(DomId, NodeId), OpacityKey>,
    /// Current vertical scrollbar opacity values
    pub scrollbar_v_opacity_values: HashMap<(DomId, NodeId), f32>,
    /// Current horizontal scrollbar opacity values
    pub scrollbar_h_opacity_values: HashMap<(DomId, NodeId), f32>,
}

/// Represents a change to a GPU transform key.
///
/// These events are generated when synchronizing the cache with the `StyledDom`
/// and are used to update WebRender's transform state efficiently.
#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum GpuTransformKeyEvent {
    /// A new transform was added to a node
    Added(NodeId, TransformKey, ComputedTransform3D),
    /// An existing transform was modified (includes old and new values)
    Changed(
        NodeId,
        TransformKey,
        ComputedTransform3D,
        ComputedTransform3D,
    ),
    /// A transform was removed from a node
    Removed(NodeId, TransformKey),
}

impl GpuValueCache {
    /// Creates an empty GPU value cache.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Synchronizes the cache with the current `StyledDom`, generating change events
    /// for CSS transform and opacity additions, modifications, and removals.
    ///
    /// Split into read-only `compute_*_events` passes (which diff against the cache)
    /// and `apply_*_events` passes (which mutate it).
    #[must_use]
    pub fn synchronize(&mut self, styled_dom: &StyledDom) -> GpuEventChanges {
        Self::init_simd_features();

        let transform_key_changes = self.compute_transform_events(styled_dom);
        self.apply_transform_events(&transform_key_changes);

        let opacity_key_changes = self.compute_opacity_events(styled_dom);
        self.apply_opacity_events(&opacity_key_changes);

        GpuEventChanges {
            transform_key_changes,
            opacity_key_changes,
            scrollbar_opacity_changes: Vec::new(), // Filled by separate synchronization
        }
    }

    /// One-time CPU feature detection (SSE/AVX) for the transform math fast paths.
    fn init_simd_features() {
        #[cfg(target_arch = "x86_64")]
        unsafe {
            if !INITIALIZED.load(AtomicOrdering::SeqCst) {
                use core::arch::x86_64::__cpuid;

                let mut cpuid = __cpuid(0);
                let n_ids = cpuid.eax;

                if n_ids > 0 {
                    // cpuid instruction is present
                    cpuid = __cpuid(1);
                    USE_SSE.store((cpuid.edx & (1_u32 << 25)) != 0, AtomicOrdering::SeqCst);
                    USE_AVX.store((cpuid.ecx & (1_u32 << 28)) != 0, AtomicOrdering::SeqCst);
                }
                INITIALIZED.store(true, AtomicOrdering::SeqCst);
            }
        }
    }

    /// Computes CSS-transform change events against the cached values (read-only).
    fn compute_transform_events(&self, styled_dom: &StyledDom) -> Vec<GpuTransformKeyEvent> {
        let css_property_cache = styled_dom.get_css_property_cache();
        let node_data = styled_dom.node_data.as_container();
        let node_states = styled_dom.styled_nodes.as_container();

        let default_transform_origin = StyleTransformOrigin::default();

        // calculate the transform values of every single node that has a non-default transform.
        //
        // GPU fast path: `has_transform` is a single bit in the compact cache.
        // The overwhelmingly common case is "no transform set", which now reads one
        // byte and bails — no cascade walk. Only nodes that actually have a
        // transform pay the slow-walk cost (required to retrieve the parsed value).
        (0..styled_dom.node_data.len())
            .filter_map(|node_id| {
                let node_id = NodeId::new(node_id);
                let styled_node_state = &node_states[node_id].styled_node_state;
                // Bit-check short-circuit: only proceed if the node might have a transform.
                if styled_node_state.is_normal() {
                    if let Some(ref cc) = css_property_cache.compact_cache {
                        // M12.7: short-circuit the empty-map get. hashbrown's
                        // empty-map probe touches the static empty control-group,
                        // which mis-lifts to wasm (out-of-bounds access); the web
                        // headless layout uses a fresh (empty) GpuValueCache. An
                        // empty map has no entry anyway, and is_empty() is len-based
                        // (no probe), so the result is identical on desktop.
                        if !cc.has_transform(node_id.index())
                            && (self.css_current_transform_values.is_empty()
                                || self.css_current_transform_values.get(&node_id).is_none())
                        {
                            return None;
                        }
                    }
                }
                let node_data = &node_data[node_id];
                let current_transform = css_property_cache
                    .get_transform(node_data, &node_id, styled_node_state)?
                    .get_property()
                    .map(|t| {
                        // TODO: look up the parent nodes size properly to resolve animation of
                        // transforms with %
                        let parent_size_width = 0.0;
                        let parent_size_height = 0.0;
                        let transform_origin = css_property_cache.get_transform_origin(
                            node_data,
                            &node_id,
                            styled_node_state,
                        );
                        let transform_origin = transform_origin
                            .as_ref()
                            .and_then(|o| o.get_property())
                            .unwrap_or(&default_transform_origin);

                        ComputedTransform3D::from_style_transform_vec(
                            t.as_ref(),
                            transform_origin,
                            parent_size_width,
                            parent_size_height,
                            RotationMode::ForWebRender,
                        )
                    });

                let existing_transform = if self.css_current_transform_values.is_empty() {
                    None
                } else {
                    self.css_current_transform_values.get(&node_id)
                };

                match (existing_transform, current_transform) {
                    (None, None) => None, // no new transform, no old transform
                    (None, Some(new)) => Some(GpuTransformKeyEvent::Added(
                        node_id,
                        TransformKey::unique(),
                        new,
                    )),
                    (Some(old), Some(new)) => Some(GpuTransformKeyEvent::Changed(
                        node_id,
                        self.css_transform_keys.get(&node_id).copied()?,
                        *old,
                        new,
                    )),
                    (Some(_old), None) => Some(GpuTransformKeyEvent::Removed(
                        node_id,
                        self.css_transform_keys.get(&node_id).copied()?,
                    )),
                }
            })
            .collect::<Vec<GpuTransformKeyEvent>>()
    }

    /// Applies transform key changes (additions/removals) to the cache.
    fn apply_transform_events(&mut self, events: &[GpuTransformKeyEvent]) {
        // remove / add the CSS transform keys accordingly
        for event in events.iter() {
            match &event {
                GpuTransformKeyEvent::Added(node_id, key, matrix) => {
                    self.css_transform_keys.insert(*node_id, *key);
                    self.css_current_transform_values.insert(*node_id, *matrix);
                }
                GpuTransformKeyEvent::Changed(node_id, _key, _old_state, new_state) => {
                    self.css_current_transform_values.insert(*node_id, *new_state);
                }
                GpuTransformKeyEvent::Removed(node_id, _key) => {
                    self.css_transform_keys.remove(node_id);
                    self.css_current_transform_values.remove(node_id);
                }
            }
        }
    }

    /// Computes opacity change events against the cached values (read-only).
    fn compute_opacity_events(&self, styled_dom: &StyledDom) -> Vec<GpuOpacityKeyEvent> {
        let css_property_cache = styled_dom.get_css_property_cache();
        let node_data = styled_dom.node_data.as_container();
        let node_states = styled_dom.styled_nodes.as_container();

        // calculate the opacity of every single node that has a non-default opacity
        //
        // GPU fast path: compact cache encodes opacity as a single u8. Nodes with
        // no author-set opacity (the common case) have `OPACITY_SENTINEL` and
        // return immediately — no cascade walk. Only non-default opacities
        // generate key events.
        (0..styled_dom.node_data.len())
            .filter_map(|node_id| {
                let node_id = NodeId::new(node_id);
                let styled_node_state = &node_states[node_id].styled_node_state;

                // Fast-path opacity read via compact cache.
                let mut compact_opacity: Option<f32> = None;
                if styled_node_state.is_normal() {
                    if let Some(ref cc) = css_property_cache.compact_cache {
                        let raw = cc.get_opacity_raw(node_id.index());
                        compact_opacity = if raw == azul_css::compact_cache::OPACITY_SENTINEL {
                            // unset → default (1.0) — bail out unless we had a prior opacity key
                            if self.current_opacity_values.get(&node_id).is_none() {
                                return None;
                            }
                            None
                        } else {
                            Some((raw as f32) / 254.0)
                        };
                    }
                }

                let node_data = &node_data[node_id];
                let current_opacity: Option<f32> = if let Some(v) = compact_opacity {
                    // Fast path: value already read from compact cache.
                    Some(v)
                } else if styled_node_state.is_normal() && css_property_cache.compact_cache.is_some() {
                    // Fast path: sentinel — unset → default (1.0, treated as None here).
                    None
                } else {
                    css_property_cache
                        .get_opacity(node_data, &node_id, styled_node_state)?
                        .get_property()
                        .map(|p| p.inner.normalized())
                };
                let existing_opacity = self.current_opacity_values.get(&node_id);

                match (existing_opacity, current_opacity) {
                    (None, None) => None, // no new opacity, no old opacity
                    (None, Some(new)) => Some(GpuOpacityKeyEvent::Added(
                        node_id,
                        OpacityKey::unique(),
                        new,
                    )),
                    (Some(old), Some(new)) => Some(GpuOpacityKeyEvent::Changed(
                        node_id,
                        self.opacity_keys.get(&node_id).copied()?,
                        *old,
                        new,
                    )),
                    (Some(_old), None) => Some(GpuOpacityKeyEvent::Removed(
                        node_id,
                        self.opacity_keys.get(&node_id).copied()?,
                    )),
                }
            })
            .collect::<Vec<GpuOpacityKeyEvent>>()
    }

    /// Applies opacity key changes (additions/removals) to the cache.
    fn apply_opacity_events(&mut self, events: &[GpuOpacityKeyEvent]) {
        // remove / add the opacity keys accordingly
        for event in events.iter() {
            match &event {
                GpuOpacityKeyEvent::Added(node_id, key, opacity) => {
                    self.opacity_keys.insert(*node_id, *key);
                    self.current_opacity_values.insert(*node_id, *opacity);
                }
                GpuOpacityKeyEvent::Changed(node_id, _key, _old_state, new_state) => {
                    self.current_opacity_values.insert(*node_id, *new_state);
                }
                GpuOpacityKeyEvent::Removed(node_id, _key) => {
                    self.opacity_keys.remove(node_id);
                    self.current_opacity_values.remove(node_id);
                }
            }
        }
    }
}

/// Represents a change to a scrollbar opacity key.
///
/// Scrollbar opacity is managed separately from CSS opacity to enable
/// independent fading animations without affecting element opacity.
#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum GpuScrollbarOpacityEvent {
    /// A vertical scrollbar was added to a node
    VerticalAdded(DomId, NodeId, OpacityKey, f32),
    /// A vertical scrollbar opacity was changed
    VerticalChanged(DomId, NodeId, OpacityKey, f32, f32),
    /// A vertical scrollbar was removed from a node
    VerticalRemoved(DomId, NodeId, OpacityKey),
    /// A horizontal scrollbar was added to a node
    HorizontalAdded(DomId, NodeId, OpacityKey, f32),
    /// A horizontal scrollbar opacity was changed
    HorizontalChanged(DomId, NodeId, OpacityKey, f32, f32),
    /// A horizontal scrollbar was removed from a node
    HorizontalRemoved(DomId, NodeId, OpacityKey),
}

/// Contains all GPU-related change events from a cache synchronization.
///
/// This structure groups transform, opacity, and scrollbar opacity changes together
/// for efficient batch processing when updating WebRender.
#[derive(Default, Debug, Clone, PartialEq, PartialOrd)]
pub struct GpuEventChanges {
    /// All transform key changes (additions, modifications, removals)
    pub transform_key_changes: Vec<GpuTransformKeyEvent>,
    /// All opacity key changes (additions, modifications, removals)
    pub opacity_key_changes: Vec<GpuOpacityKeyEvent>,
    /// All scrollbar opacity key changes (additions, modifications, removals)
    pub scrollbar_opacity_changes: Vec<GpuScrollbarOpacityEvent>,
}

impl GpuEventChanges {
    /// Creates an empty set of GPU event changes.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Returns `true` if there are no transform, opacity, or scrollbar opacity changes.
    pub fn is_empty(&self) -> bool {
        self.transform_key_changes.is_empty()
            && self.opacity_key_changes.is_empty()
            && self.scrollbar_opacity_changes.is_empty()
    }

    /// Merges another `GpuEventChanges` into this one, consuming the other.
    ///
    /// This is useful for combining changes from multiple sources.
    pub fn merge(&mut self, other: &mut Self) {
        self.transform_key_changes
            .extend(other.transform_key_changes.drain(..));
        self.opacity_key_changes
            .extend(other.opacity_key_changes.drain(..));
        self.scrollbar_opacity_changes
            .extend(other.scrollbar_opacity_changes.drain(..));
    }
}

/// Represents a change to a GPU opacity key.
///
/// These events are generated when synchronizing the cache with the `StyledDom`
/// and are used to update WebRender's opacity state efficiently.
#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum GpuOpacityKeyEvent {
    /// A new opacity was added to a node
    Added(NodeId, OpacityKey, f32),
    /// An existing opacity was modified (includes old and new values)
    Changed(NodeId, OpacityKey, f32, f32),
    /// An opacity was removed from a node
    Removed(NodeId, OpacityKey),
}
