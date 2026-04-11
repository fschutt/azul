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
use std::collections::HashMap;
use crate::{
    dom::{DomId, NodeId},
    resources::{OpacityKey, TransformKey},
    transform::ComputedTransform3D,
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
    /// All scrollbar opacity key changes (additions, modifications, removals)
    pub scrollbar_opacity_changes: Vec<GpuScrollbarOpacityEvent>,
}

impl GpuEventChanges {
    /// Creates an empty set of GPU event changes.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Returns `true` if there are no transform or scrollbar opacity changes.
    pub fn is_empty(&self) -> bool {
        self.transform_key_changes.is_empty()
            && self.scrollbar_opacity_changes.is_empty()
    }
}

