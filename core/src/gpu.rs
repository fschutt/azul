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

#[cfg(not(feature = "std"))]
use alloc::collections::BTreeMap as HashMap;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering as AtomicOrdering};
#[cfg(feature = "std")]
use std::collections::HashMap;

use azul_css::props::style::StyleTransformOrigin;

use crate::{
    dom::{DomId, NodeId, ScrollbarOrientation},
    resources::{ColorKey, OpacityKey, TransformKey},
    styled_dom::StyledDom,
    transform::{ComputedTransform3D, RotationMode, INITIALIZED, USE_AVX, USE_SSE},
};

/// A map of this module keyed by a node (a `HashMap`, or a `BTreeMap` in a
/// build without `std`).
pub type NodeMap<V> = HashMap<NodeId, V>;
/// A map of this module keyed by a DOM and one of its nodes.
pub type DomNodeMap<V> = HashMap<(DomId, NodeId), V>;

/// The transform channels of a [`GpuValueCache`]: each is a map of keys and
/// a map of values by node.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum TransformChannel {
    /// The thumb of a scroll container's vertical scrollbar.
    ScrollbarVertical,
    /// The thumb of a scroll container's horizontal scrollbar.
    ScrollbarHorizontal,
    /// The node's CSS `transform` (owned by [`GpuValueCache::synchronize`]).
    Css,
    /// An engine-driven movement of the painted node: a FLIP slide, an enter
    /// track, a drag.
    Animation,
    /// How far a scroll moved a `position: sticky` box since its layout.
    Sticky,
    /// The highlight of a material's sheen layer, moved to the cursor.
    Sheen,
}

/// The opacity channels of a [`GpuValueCache`] that are keyed by a node (a
/// scrollbar's fade is keyed by its DOM and node: `set_scrollbar_opacity`).
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum OpacityChannel {
    /// The node's CSS `opacity` (owned by [`GpuValueCache::synchronize`]).
    Css,
    /// An engine-driven fade: an enter or exit, a FLIP's opacity.
    Animation,
    /// The eased strength of a material's sheen layer.
    Sheen,
}

/// What rewrites the node ids of a [`GpuValueCache`]'s maps after a DOM
/// rebuild ([`GpuValueCache::rekey_nodes`]): called once per map.
pub trait RekeyNodes {
    /// Rewrites the keys of a map keyed by a node.
    fn node_map<V>(&mut self, map: &mut NodeMap<V>);
    /// Rewrites the keys of a map keyed by a DOM and a node.
    fn dom_node_map<V>(&mut self, map: &mut DomNodeMap<V>);
}

#[cfg(all(test, feature = "std"))]
std::thread_local! {
    /// Tests: how many CSS matrices [`GpuValueCache`] has built on this thread
    /// (`css_transform_of`): the cascade lookups and the matrix products of
    /// one node's `transform`.
    static CSS_MATRICES_BUILT: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
}

/// One of the twelve transform maps of a [`GpuValueCache`]: the keys or the
/// values of one [`TransformChannel`], by node. A `HashMap` that carries a
/// STAMP of its content - its own write generation. Two readers use the
/// stamps: [`GpuValueCache::value_generation`] is the newest of them (a
/// frame that finds it unchanged reads no value again), and
/// [`GpuValueCache::transform_stamps`] are those of the six maps a node's
/// reference frame is read from (the hit tester keeps where every frame's
/// boxes are painted while they are what they were, instead of reading
/// every frame's matrix again).
///
/// The rule the stamp keeps: two maps with the same stamp have the same
/// content. 0 is the empty map nobody wrote; every change takes a number
/// never handed out before (one counter for the process), and a clone keeps
/// the stamp of what it copies. The stamp cannot be forgotten by a writer:
/// reading goes through `Deref`, and every way to change the map goes
/// through [`Self::insert`], [`Self::remove`], [`Self::retain`] (which
/// restamp only when the content did change) or through `DerefMut` (which
/// restamps always: `entry`, `clear`, a `&mut HashMap` handed to a function).
#[derive(Clone)]
pub struct StampedMap<V> {
    map: HashMap<NodeId, V>,
    stamp: u64,
}

/// A stamp no [`StampedMap`] has had before.
fn next_map_stamp() -> u64 {
    static NEXT: AtomicUsize = AtomicUsize::new(1);
    NEXT.fetch_add(1, AtomicOrdering::Relaxed) as u64
}

impl<V> Default for StampedMap<V> {
    fn default() -> Self {
        Self {
            map: HashMap::new(),
            stamp: 0,
        }
    }
}

impl<V: core::fmt::Debug> core::fmt::Debug for StampedMap<V> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Debug::fmt(&self.map, f)
    }
}

impl<V> core::ops::Deref for StampedMap<V> {
    type Target = HashMap<NodeId, V>;

    fn deref(&self) -> &Self::Target {
        &self.map
    }
}

impl<V> core::ops::DerefMut for StampedMap<V> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.stamp = next_map_stamp();
        &mut self.map
    }
}

impl<'a, V> IntoIterator for &'a StampedMap<V> {
    type Item = (&'a NodeId, &'a V);
    type IntoIter = <&'a HashMap<NodeId, V> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        (&self.map).into_iter()
    }
}

impl<V> StampedMap<V> {
    /// The stamp of the content: equal stamps, equal content.
    #[must_use]
    pub const fn stamp(&self) -> u64 {
        self.stamp
    }

    /// `HashMap::remove`; a new stamp when the node had an entry.
    pub fn remove(&mut self, node: &NodeId) -> Option<V> {
        let removed = self.map.remove(node);
        if removed.is_some() {
            self.stamp = next_map_stamp();
        }
        removed
    }

    /// `HashMap::retain` for a test that only reads the value; a new stamp
    /// when an entry was dropped.
    pub fn retain<F: FnMut(&NodeId, &V) -> bool>(&mut self, mut keep: F) {
        let before = self.map.len();
        self.map.retain(|node, value| keep(node, value));
        if self.map.len() != before {
            self.stamp = next_map_stamp();
        }
    }
}

impl<V: PartialEq> StampedMap<V> {
    /// `HashMap::insert`; a new stamp unless the node already had this
    /// value.
    pub fn insert(&mut self, node: NodeId, value: V) -> Option<V> {
        if self.map.get(&node) != Some(&value) {
            self.stamp = next_map_stamp();
        }
        self.map.insert(node, value)
    }
}

/// Caches GPU transform and opacity keys and their current values for all nodes.
///
/// This cache stores the `WebRender` keys and computed values for nodes with
/// CSS transforms or opacity. It's synchronized with the `StyledDom` to detect
/// changes and generate minimal update events.
#[derive(Default, Debug, Clone)]
pub struct GpuValueCache {
    /// Vertical scrollbar thumb transform keys (keyed by scrollable node ID)
    pub transform_keys: StampedMap<TransformKey>,
    /// Current vertical scrollbar thumb transform values
    pub current_transform_values: StampedMap<ComputedTransform3D>,
    /// Horizontal scrollbar thumb transform keys (keyed by scrollable node ID)
    pub h_transform_keys: StampedMap<TransformKey>,
    /// Current horizontal scrollbar thumb transform values
    pub h_current_transform_values: StampedMap<ComputedTransform3D>,
    /// CSS transform keys (keyed by node ID) — for CSS `transform` property animation.
    /// Separate from scrollbar transform keys to avoid `SpatialTreeItemKey` collisions.
    pub css_transform_keys: StampedMap<TransformKey>,
    /// Current CSS transform values (keyed by node ID)
    pub css_current_transform_values: StampedMap<ComputedTransform3D>,
    /// The border-box size (logical px) each value of
    /// [`Self::css_current_transform_values`] was resolved against: what its
    /// `transform-origin` and `translate()` percentages were taken of.
    ///
    /// A value is a function of the cascade and of this size, and one layout
    /// pass asks for it three times with one cascade (the sync before layout,
    /// the display-list builder, the refresh after layout). The sync builds
    /// the matrix and compares it; the other two find the size they ask for
    /// here and take the value as it is ([`Self::reference_frame_at`],
    /// [`Self::refresh_transform_values`]). An entry exists only for a value
    /// THIS type's methods resolved from the cascade; an event applied from
    /// outside drops it, and a value without an entry is built again.
    css_transform_sizes: HashMap<NodeId, (f32, f32)>,
    /// ANIMATION transform keys (keyed by node ID).
    ///
    /// A separate channel from `css_transform_keys` on purpose. That map is
    /// OWNED by `synchronize`, which adds and removes entries to match the
    /// DOM's CSS `transform` property — so an animation writing into it has its
    /// keys evicted on the very next frame, and the element snaps instead of
    /// moving. Scrollbar thumbs already have their own channel for the same
    /// reason; this follows that precedent rather than fighting the cascade for
    /// one map.
    pub anim_transform_keys: StampedMap<TransformKey>,
    /// Current animation transform values (keyed by node ID).
    pub anim_current_transform_values: StampedMap<ComputedTransform3D>,
    /// STICKY transform keys (keyed by node ID): one per `position: sticky`
    /// box, minted by `synchronize`, so its display list wraps the box in a
    /// reference frame. A channel of its own for the reason the animation
    /// channel has one: `synchronize` owns the CSS map.
    pub sticky_transform_keys: StampedMap<TransformKey>,
    /// Current sticky translations (keyed by node ID): how far a scroll
    /// since the layout pass moved the box from where that pass placed it.
    /// The identity right after layout; written per frame by the layout
    /// crate's `GpuStateManager::update_scrollbar_transforms`.
    pub sticky_current_transform_values: StampedMap<ComputedTransform3D>,
    /// The cursor's SHEEN over a material (keyed by the material's node): the
    /// transform that moves the sheen's highlight to the cursor. A channel of
    /// its own, minted by the layout window for a window `WebRender` draws; the
    /// display list opens a sheen layer over a material exactly where its node
    /// has both sheen keys.
    pub sheen_transform_keys: StampedMap<TransformKey>,
    /// Where each sheen's highlight sits now: a translation to the cursor,
    /// logical px from the material's top-left.
    pub sheen_current_transform_values: StampedMap<ComputedTransform3D>,
    /// The sheen's eased strength, bound as the opacity of its layer.
    pub sheen_opacity_keys: HashMap<NodeId, OpacityKey>,
    /// Each sheen's eased strength now, 0 (none) to 1.
    pub sheen_current_opacity_values: HashMap<NodeId, f32>,
    /// The light the sheens shine in: the window's material light's colour
    /// and intensity, which the layer bakes into its highlight's stops (so it
    /// is in the emission fingerprint: a new colour is a new list). `None`
    /// until the layout window sets it: the default light's.
    pub sheen_light: Option<(azul_css::props::basic::ColorU, f32)>,
    /// BACKGROUND COLOUR keys (keyed by node ID): one per node whose solid
    /// `background` colour fades. The display list binds the node's
    /// background rect to it (`DisplayList::color_bindings`), so each frame
    /// of the fade is a property value - `WebRender`'s colour binding of a
    /// rect, the CPU renderer's bound colour - instead of a new list. Kept
    /// by the layout window: minted when a fade starts, released by the
    /// display-list build after it settled.
    pub color_keys: HashMap<NodeId, ColorKey>,
    /// The colour each bound background shows now.
    pub current_color_values: HashMap<NodeId, azul_css::props::basic::ColorU>,
    /// Animation opacity keys (keyed by node ID).
    pub anim_opacity_keys: HashMap<NodeId, OpacityKey>,
    /// Current animation opacity values (keyed by node ID).
    pub anim_current_opacity_values: HashMap<NodeId, f32>,
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
    /// The stamp of the last NOTED write ([`Self::note_value_write`]): one
    /// part of the WRITE GENERATION of the bound values, which is read with
    /// [`Self::value_generation`] and is the newest of this and of the
    /// transform maps' own stamps. A frame that finds the generation it last
    /// read has nothing to read again - no copy of the values, no comparison
    /// with the previous ones, no synchronization with the compositor.
    ///
    /// The methods of this type that write values move the generation, and
    /// only when a value or a key really changed (a `synchronize` that finds
    /// every value as it was leaves it alone, however many events it
    /// reports). The twelve TRANSFORM maps move it by themselves, whoever
    /// writes them ([`StampedMap`]). CODE THAT WRITES ONE OF THE OTHER MAPS
    /// (opacities, colours, scrollbar opacities) DIRECTLY MUST CALL
    /// [`Self::note_value_write`]: until every such writer does, a reader
    /// must not skip work by it. Only equality is compared.
    pub value_generation: u64,
}

/// Represents a change to a GPU transform key.
///
/// These events are generated when synchronizing the cache with the `StyledDom`
/// and are used to update `WebRender`'s transform state efficiently.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
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
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// The write generation of the bound values: the NEWEST stamp of this
    /// cache - of its twelve transform maps, which stamp themselves
    /// ([`StampedMap`]), and of the last write that was noted
    /// ([`Self::note_value_write`]: the opacities, the colours, the sheen
    /// light, a rekeying). One clock for both ([`next_map_stamp`]), so the
    /// newest stamp is another number after any of them changed. Only
    /// equality is meaningful.
    #[must_use]
    pub const fn value_generation(&self) -> u64 {
        let stamps = self.transform_map_stamps();
        let mut newest = self.value_generation;
        let mut i = 0;
        while i < stamps.len() {
            if stamps[i] > newest {
                newest = stamps[i];
            }
            i += 1;
        }
        newest
    }

    /// The stamps of the twelve transform maps (keys and values of the six
    /// channels).
    const fn transform_map_stamps(&self) -> [u64; 12] {
        [
            self.transform_keys.stamp(),
            self.current_transform_values.stamp(),
            self.h_transform_keys.stamp(),
            self.h_current_transform_values.stamp(),
            self.css_transform_keys.stamp(),
            self.css_current_transform_values.stamp(),
            self.anim_transform_keys.stamp(),
            self.anim_current_transform_values.stamp(),
            self.sticky_transform_keys.stamp(),
            self.sticky_current_transform_values.stamp(),
            self.sheen_transform_keys.stamp(),
            self.sheen_current_transform_values.stamp(),
        ]
    }

    /// Notes that a bound value or the set of bound keys changed, for what
    /// does not stamp itself: takes a stamp from the maps' clock, so
    /// [`Self::value_generation`] is another number. For the write methods of
    /// the opacity, colour and scrollbar maps, and for every writer of THOSE
    /// maps that is not one of this type's methods. A transform map needs no
    /// note: it cannot be written without restamping.
    pub fn note_value_write(&mut self) {
        self.value_generation = next_map_stamp();
    }

    // ------------------------------------------------------------------
    // The write methods: every write of a bound key or value goes through
    // one of these, and each moves the write generation exactly when it
    // changed something. (The maps are still public fields; a writer that
    // uses them directly has to call `note_value_write` itself, see
    // `audit/A2-gpu.md`, "Integration".)
    // ------------------------------------------------------------------

    fn transform_maps(
        &self,
        channel: TransformChannel,
    ) -> (&StampedMap<TransformKey>, &StampedMap<ComputedTransform3D>) {
        match channel {
            TransformChannel::ScrollbarVertical => {
                (&self.transform_keys, &self.current_transform_values)
            }
            TransformChannel::ScrollbarHorizontal => {
                (&self.h_transform_keys, &self.h_current_transform_values)
            }
            TransformChannel::Css => (&self.css_transform_keys, &self.css_current_transform_values),
            TransformChannel::Animation => {
                (&self.anim_transform_keys, &self.anim_current_transform_values)
            }
            TransformChannel::Sticky => {
                (&self.sticky_transform_keys, &self.sticky_current_transform_values)
            }
            TransformChannel::Sheen => {
                (&self.sheen_transform_keys, &self.sheen_current_transform_values)
            }
        }
    }

    fn transform_maps_mut(
        &mut self,
        channel: TransformChannel,
    ) -> (&mut StampedMap<TransformKey>, &mut StampedMap<ComputedTransform3D>) {
        match channel {
            TransformChannel::ScrollbarVertical => {
                (&mut self.transform_keys, &mut self.current_transform_values)
            }
            TransformChannel::ScrollbarHorizontal => {
                (&mut self.h_transform_keys, &mut self.h_current_transform_values)
            }
            TransformChannel::Css => {
                (&mut self.css_transform_keys, &mut self.css_current_transform_values)
            }
            TransformChannel::Animation => {
                (&mut self.anim_transform_keys, &mut self.anim_current_transform_values)
            }
            TransformChannel::Sticky => {
                (&mut self.sticky_transform_keys, &mut self.sticky_current_transform_values)
            }
            TransformChannel::Sheen => {
                (&mut self.sheen_transform_keys, &mut self.sheen_current_transform_values)
            }
        }
    }

    /// What every transform write ends with: a CSS value written this way was
    /// not resolved from the cascade at a known size (`css_transform_sizes`),
    /// and a change moves the generation. Returns `changed`.
    fn wrote_transform(&mut self, channel: TransformChannel, node: NodeId, changed: bool) -> bool {
        if matches!(channel, TransformChannel::Css) {
            self.css_transform_sizes.remove(&node);
        }
        if changed {
            self.note_value_write();
        }
        changed
    }

    /// The key `node` has in `channel`.
    #[must_use]
    pub fn transform_key(&self, channel: TransformChannel, node: NodeId) -> Option<TransformKey> {
        self.transform_maps(channel).0.get(&node).copied()
    }

    /// The value `node` has in `channel` (its own, not the product of its
    /// channels: that is [`Self::reference_frame_of`]).
    #[must_use]
    pub fn transform_value(
        &self,
        channel: TransformChannel,
        node: NodeId,
    ) -> Option<ComputedTransform3D> {
        self.transform_maps(channel).1.get(&node).copied()
    }

    /// Every node that has a key in `channel`, with the key.
    pub fn transform_keys_of(
        &self,
        channel: TransformChannel,
    ) -> impl Iterator<Item = (NodeId, TransformKey)> + '_ {
        self.transform_maps(channel)
            .0
            .iter()
            .map(|(node, key)| (*node, *key))
    }

    /// Binds `node` to `key` in `channel` with `value`, replacing what it
    /// had. Returns whether the key or the value is another one now.
    pub fn set_transform(
        &mut self,
        channel: TransformChannel,
        node: NodeId,
        key: TransformKey,
        value: ComputedTransform3D,
    ) -> bool {
        let (keys, values) = self.transform_maps_mut(channel);
        let key_changed = keys.insert(node, key) != Some(key);
        let value_changed = values.insert(node, value) != Some(value);
        self.wrote_transform(channel, node, key_changed || value_changed)
    }

    /// [`Self::set_transform`] for a node that has NO key in `channel` yet;
    /// a node that has one keeps its key and its value. Returns whether the
    /// node was bound (a display list's key registered for the first time).
    pub fn register_transform(
        &mut self,
        channel: TransformChannel,
        node: NodeId,
        key: TransformKey,
        value: ComputedTransform3D,
    ) -> bool {
        let (keys, values) = self.transform_maps_mut(channel);
        if keys.contains_key(&node) {
            return false;
        }
        keys.insert(node, key);
        values.insert(node, value);
        self.wrote_transform(channel, node, true)
    }

    /// Gives `node` the value `value` in `channel` under the key it has, or
    /// under a new one: the write of a channel that keys its nodes itself
    /// (an animation tick, a drag). Returns the key.
    pub fn key_transform(
        &mut self,
        channel: TransformChannel,
        node: NodeId,
        value: ComputedTransform3D,
    ) -> TransformKey {
        let (keys, values) = self.transform_maps_mut(channel);
        // Not `entry()`: that is a `&mut` to the map inside, which restamps
        // the keys whether or not one is minted (`StampedMap`).
        let (key, minted) = match keys.get(&node).copied() {
            Some(key) => (key, false),
            None => {
                let key = TransformKey::unique();
                keys.insert(node, key);
                (key, true)
            }
        };
        let value_changed = values.insert(node, value) != Some(value);
        self.wrote_transform(channel, node, minted || value_changed);
        key
    }

    /// Gives `node` a key in `channel` when it has none, and leaves its
    /// value alone: a frame that is keyed before its value is known (a
    /// sheen layer, keyed with its material and moved when the cursor
    /// comes). Returns the key.
    pub fn ensure_transform_key(&mut self, channel: TransformChannel, node: NodeId) -> TransformKey {
        let (keys, _) = self.transform_maps_mut(channel);
        // Looked up first, not `entry()`: a map that stamps itself when it
        // is written (the `StampedMap` the transform maps are on the
        // integration branch) takes every `entry()` for a write, minted or
        // not, and the generation then moved on every call for a node that
        // had its key.
        if let Some(key) = keys.get(&node).copied() {
            return key;
        }
        let key = TransformKey::unique();
        keys.insert(node, key);
        self.wrote(true);
        key
    }

    /// Writes `value` for `node` in `channel` and leaves its key alone (a
    /// value without a key binds nothing). Returns the value it had.
    pub fn set_transform_value(
        &mut self,
        channel: TransformChannel,
        node: NodeId,
        value: ComputedTransform3D,
    ) -> Option<ComputedTransform3D> {
        let (_, values) = self.transform_maps_mut(channel);
        let old = values.insert(node, value);
        self.wrote_transform(channel, node, old != Some(value));
        old
    }

    /// Takes `node` out of `channel`, key and value. Returns the key it had.
    pub fn remove_transform(
        &mut self,
        channel: TransformChannel,
        node: NodeId,
    ) -> Option<TransformKey> {
        let (keys, values) = self.transform_maps_mut(channel);
        let key = keys.remove(&node);
        let had_value = values.remove(&node).is_some();
        self.wrote_transform(channel, node, key.is_some() || had_value);
        key
    }

    /// Keeps in `channel` the nodes `keep` holds for, keys and values.
    pub fn retain_transforms(
        &mut self,
        channel: TransformChannel,
        mut keep: impl FnMut(NodeId) -> bool,
    ) {
        let (keys, values) = self.transform_maps_mut(channel);
        let before = keys.len() + values.len();
        keys.retain(|node, _| keep(*node));
        values.retain(|node, _| keep(*node));
        let changed = keys.len() + values.len() != before;
        if matches!(channel, TransformChannel::Css) {
            let Self {
                ref css_current_transform_values,
                ref mut css_transform_sizes,
                ..
            } = *self;
            css_transform_sizes.retain(|node, _| css_current_transform_values.contains_key(node));
        }
        if changed {
            self.note_value_write();
        }
    }

    /// Empties `channel`.
    pub fn clear_transforms(&mut self, channel: TransformChannel) {
        self.retain_transforms(channel, |_| false);
    }

    fn opacity_maps(
        &self,
        channel: OpacityChannel,
    ) -> (&NodeMap<OpacityKey>, &NodeMap<f32>) {
        match channel {
            OpacityChannel::Css => (&self.opacity_keys, &self.current_opacity_values),
            OpacityChannel::Animation => {
                (&self.anim_opacity_keys, &self.anim_current_opacity_values)
            }
            OpacityChannel::Sheen => (&self.sheen_opacity_keys, &self.sheen_current_opacity_values),
        }
    }

    fn opacity_maps_mut(
        &mut self,
        channel: OpacityChannel,
    ) -> (&mut NodeMap<OpacityKey>, &mut NodeMap<f32>) {
        match channel {
            OpacityChannel::Css => (&mut self.opacity_keys, &mut self.current_opacity_values),
            OpacityChannel::Animation => {
                (&mut self.anim_opacity_keys, &mut self.anim_current_opacity_values)
            }
            OpacityChannel::Sheen => {
                (&mut self.sheen_opacity_keys, &mut self.sheen_current_opacity_values)
            }
        }
    }

    /// Moves the generation when `changed`; returns `changed`.
    fn wrote(&mut self, changed: bool) -> bool {
        if changed {
            self.note_value_write();
        }
        changed
    }

    /// The opacity key `node` has in `channel`.
    #[must_use]
    pub fn opacity_key(&self, channel: OpacityChannel, node: NodeId) -> Option<OpacityKey> {
        self.opacity_maps(channel).0.get(&node).copied()
    }

    /// The opacity `node` has in `channel` (its own, not the product a layer
    /// is painted at).
    #[must_use]
    pub fn opacity_value(&self, channel: OpacityChannel, node: NodeId) -> Option<f32> {
        self.opacity_maps(channel).1.get(&node).copied()
    }

    /// Binds `node` to `key` in `channel` with `value`, replacing what it
    /// had. Returns whether the key or the value is another one now.
    pub fn set_opacity(
        &mut self,
        channel: OpacityChannel,
        node: NodeId,
        key: OpacityKey,
        value: f32,
    ) -> bool {
        let (keys, values) = self.opacity_maps_mut(channel);
        let key_changed = keys.insert(node, key) != Some(key);
        let value_changed = values.insert(node, value) != Some(value);
        self.wrote(key_changed || value_changed)
    }

    /// Gives `node` the opacity `value` in `channel` under the key it has, or
    /// under a new one. Returns the key.
    pub fn key_opacity(&mut self, channel: OpacityChannel, node: NodeId, value: f32) -> OpacityKey {
        let (keys, values) = self.opacity_maps_mut(channel);
        let mut minted = false;
        let key = *keys.entry(node).or_insert_with(|| {
            minted = true;
            OpacityKey::unique()
        });
        let value_changed = values.insert(node, value) != Some(value);
        self.wrote(minted || value_changed);
        key
    }

    /// Gives `node` an opacity key in `channel` when it has none, and leaves
    /// its value alone. Returns the key.
    pub fn ensure_opacity_key(&mut self, channel: OpacityChannel, node: NodeId) -> OpacityKey {
        let (keys, _) = self.opacity_maps_mut(channel);
        let mut minted = false;
        let key = *keys.entry(node).or_insert_with(|| {
            minted = true;
            OpacityKey::unique()
        });
        self.wrote(minted);
        key
    }

    /// Every node that has an opacity key in `channel`, with the key.
    pub fn opacity_keys_of(
        &self,
        channel: OpacityChannel,
    ) -> impl Iterator<Item = (NodeId, OpacityKey)> + '_ {
        self.opacity_maps(channel)
            .0
            .iter()
            .map(|(node, key)| (*node, *key))
    }

    /// Writes the opacity `value` for `node` in `channel` and leaves its key
    /// alone. Returns the value it had.
    pub fn set_opacity_value(
        &mut self,
        channel: OpacityChannel,
        node: NodeId,
        value: f32,
    ) -> Option<f32> {
        let (_, values) = self.opacity_maps_mut(channel);
        let old = values.insert(node, value);
        self.wrote(old != Some(value));
        old
    }

    /// Takes `node` out of `channel`, key and value. Returns the key it had.
    pub fn remove_opacity(&mut self, channel: OpacityChannel, node: NodeId) -> Option<OpacityKey> {
        let (keys, values) = self.opacity_maps_mut(channel);
        let key = keys.remove(&node);
        let had_value = values.remove(&node).is_some();
        self.wrote(key.is_some() || had_value);
        key
    }

    /// Keeps in `channel` the nodes `keep` holds for, keys and values.
    pub fn retain_opacities(&mut self, channel: OpacityChannel, mut keep: impl FnMut(NodeId) -> bool) {
        let (keys, values) = self.opacity_maps_mut(channel);
        let before = keys.len() + values.len();
        keys.retain(|node, _| keep(*node));
        values.retain(|node, _| keep(*node));
        let changed = keys.len() + values.len() != before;
        self.wrote(changed);
    }

    /// Empties `channel`.
    pub fn clear_opacities(&mut self, channel: OpacityChannel) {
        self.retain_opacities(channel, |_| false);
    }

    fn scrollbar_opacity_maps_mut(
        &mut self,
        orientation: ScrollbarOrientation,
    ) -> (&mut DomNodeMap<OpacityKey>, &mut DomNodeMap<f32>) {
        match orientation {
            ScrollbarOrientation::Vertical => (
                &mut self.scrollbar_v_opacity_keys,
                &mut self.scrollbar_v_opacity_values,
            ),
            ScrollbarOrientation::Horizontal => (
                &mut self.scrollbar_h_opacity_keys,
                &mut self.scrollbar_h_opacity_values,
            ),
        }
    }

    /// The opacity key of the `orientation` scrollbar of `node` in `dom`.
    #[must_use]
    pub fn scrollbar_opacity_key(
        &self,
        orientation: ScrollbarOrientation,
        dom: DomId,
        node: NodeId,
    ) -> Option<OpacityKey> {
        let keys = match orientation {
            ScrollbarOrientation::Vertical => &self.scrollbar_v_opacity_keys,
            ScrollbarOrientation::Horizontal => &self.scrollbar_h_opacity_keys,
        };
        keys.get(&(dom, node)).copied()
    }

    /// The opacity of the `orientation` scrollbar of `node` in `dom`.
    #[must_use]
    pub fn scrollbar_opacity(
        &self,
        orientation: ScrollbarOrientation,
        dom: DomId,
        node: NodeId,
    ) -> Option<f32> {
        let values = match orientation {
            ScrollbarOrientation::Vertical => &self.scrollbar_v_opacity_values,
            ScrollbarOrientation::Horizontal => &self.scrollbar_h_opacity_values,
        };
        values.get(&(dom, node)).copied()
    }

    /// Binds the `orientation` scrollbar of `node` in `dom` to `key` at
    /// `value`, replacing what it had. Returns whether anything changed.
    pub fn set_scrollbar_opacity(
        &mut self,
        orientation: ScrollbarOrientation,
        dom: DomId,
        node: NodeId,
        key: OpacityKey,
        value: f32,
    ) -> bool {
        let (keys, values) = self.scrollbar_opacity_maps_mut(orientation);
        let key_changed = keys.insert((dom, node), key) != Some(key);
        let value_changed = values.insert((dom, node), value) != Some(value);
        self.wrote(key_changed || value_changed)
    }

    /// [`Self::set_scrollbar_opacity`] for a bar that has NO key yet; a bar
    /// that has one keeps its key and its value. Returns whether it was bound.
    pub fn register_scrollbar_opacity(
        &mut self,
        orientation: ScrollbarOrientation,
        dom: DomId,
        node: NodeId,
        key: OpacityKey,
        value: f32,
    ) -> bool {
        let (keys, values) = self.scrollbar_opacity_maps_mut(orientation);
        if keys.contains_key(&(dom, node)) {
            return false;
        }
        keys.insert((dom, node), key);
        values.insert((dom, node), value);
        self.wrote(true)
    }

    /// Writes the opacity of the `orientation` scrollbar of `node` in `dom`
    /// and leaves its key alone. Returns the value it had.
    pub fn set_scrollbar_opacity_value(
        &mut self,
        orientation: ScrollbarOrientation,
        dom: DomId,
        node: NodeId,
        value: f32,
    ) -> Option<f32> {
        let (_, values) = self.scrollbar_opacity_maps_mut(orientation);
        let old = values.insert((dom, node), value);
        self.wrote(old != Some(value));
        old
    }

    /// Takes the `orientation` scrollbar of `node` in `dom` out, key and
    /// value. Returns the key it had.
    pub fn remove_scrollbar_opacity(
        &mut self,
        orientation: ScrollbarOrientation,
        dom: DomId,
        node: NodeId,
    ) -> Option<OpacityKey> {
        let (keys, values) = self.scrollbar_opacity_maps_mut(orientation);
        let key = keys.remove(&(dom, node));
        let had_value = values.remove(&(dom, node)).is_some();
        self.wrote(key.is_some() || had_value);
        key
    }

    /// The colour key of `node`'s background.
    #[must_use]
    pub fn color_key(&self, node: NodeId) -> Option<ColorKey> {
        self.color_keys.get(&node).copied()
    }

    /// The colour `node`'s bound background shows now.
    #[must_use]
    pub fn color_value(&self, node: NodeId) -> Option<azul_css::props::basic::ColorU> {
        self.current_color_values.get(&node).copied()
    }

    /// Every node whose background is bound to a colour key, with the key.
    pub fn color_keys_of(&self) -> impl Iterator<Item = (NodeId, ColorKey)> + '_ {
        self.color_keys.iter().map(|(node, key)| (*node, *key))
    }

    /// Gives `node`'s background the colour `value` under the key it has, or
    /// under a new one. Returns the key.
    pub fn key_color(&mut self, node: NodeId, value: azul_css::props::basic::ColorU) -> ColorKey {
        let mut minted = false;
        let key = *self.color_keys.entry(node).or_insert_with(|| {
            minted = true;
            ColorKey::unique()
        });
        let value_changed = self.current_color_values.insert(node, value) != Some(value);
        self.wrote(minted || value_changed);
        key
    }

    /// Writes the colour of `node`'s background and leaves its key alone.
    /// Returns the colour it had.
    pub fn set_color_value(
        &mut self,
        node: NodeId,
        value: azul_css::props::basic::ColorU,
    ) -> Option<azul_css::props::basic::ColorU> {
        let old = self.current_color_values.insert(node, value);
        self.wrote(old != Some(value));
        old
    }

    /// Takes `node`'s background out, key and colour. Returns the key it had.
    pub fn remove_color(&mut self, node: NodeId) -> Option<ColorKey> {
        let key = self.color_keys.remove(&node);
        let had_value = self.current_color_values.remove(&node).is_some();
        self.wrote(key.is_some() || had_value);
        key
    }

    /// Keeps the bound backgrounds of the nodes `keep` holds for.
    pub fn retain_colors(&mut self, mut keep: impl FnMut(NodeId) -> bool) {
        let before = self.color_keys.len() + self.current_color_values.len();
        self.color_keys.retain(|node, _| keep(*node));
        self.current_color_values.retain(|node, _| keep(*node));
        let changed = self.color_keys.len() + self.current_color_values.len() != before;
        self.wrote(changed);
    }

    /// Sets the light the sheens shine in ([`Self::sheen_light`]). Returns
    /// whether it is another one.
    pub fn set_sheen_light(&mut self, light: Option<(azul_css::props::basic::ColorU, f32)>) -> bool {
        let changed = self.sheen_light != light;
        self.sheen_light = light;
        self.wrote(changed)
    }

    /// Drops every key and every value.
    pub fn clear(&mut self) {
        *self = Self::default();
        // Empty maps carry stamp 0: the note makes the emptied cache another
        // generation than the one it had (and than any it had before).
        self.note_value_write();
    }

    /// Hands every map of this cache that is keyed by a node to `rekey`, to
    /// have its node ids rewritten after a DOM rebuild (keys AND values AND
    /// what belongs to them: a map left out keeps a stranger's value under a
    /// reused id). The one place that lists the maps for that.
    pub fn rekey_nodes(&mut self, rekey: &mut impl RekeyNodes) {
        // `&mut *map` of a transform map: the map inside a `StampedMap`,
        // which is restamped by handing it out.
        rekey.node_map(&mut *self.transform_keys);
        rekey.node_map(&mut *self.current_transform_values);
        rekey.node_map(&mut *self.h_transform_keys);
        rekey.node_map(&mut *self.h_current_transform_values);
        rekey.node_map(&mut *self.css_transform_keys);
        rekey.node_map(&mut *self.css_current_transform_values);
        rekey.node_map(&mut self.css_transform_sizes);
        rekey.node_map(&mut *self.anim_transform_keys);
        rekey.node_map(&mut *self.anim_current_transform_values);
        rekey.node_map(&mut *self.sticky_transform_keys);
        rekey.node_map(&mut *self.sticky_current_transform_values);
        rekey.node_map(&mut *self.sheen_transform_keys);
        rekey.node_map(&mut *self.sheen_current_transform_values);
        rekey.node_map(&mut self.sheen_opacity_keys);
        rekey.node_map(&mut self.sheen_current_opacity_values);
        rekey.node_map(&mut self.color_keys);
        rekey.node_map(&mut self.current_color_values);
        rekey.node_map(&mut self.anim_opacity_keys);
        rekey.node_map(&mut self.anim_current_opacity_values);
        rekey.node_map(&mut self.opacity_keys);
        rekey.node_map(&mut self.current_opacity_values);
        rekey.dom_node_map(&mut self.scrollbar_v_opacity_keys);
        rekey.dom_node_map(&mut self.scrollbar_h_opacity_keys);
        rekey.dom_node_map(&mut self.scrollbar_v_opacity_values);
        rekey.dom_node_map(&mut self.scrollbar_h_opacity_values);
        self.note_value_write();
    }

    /// The reference frame the display list opens for `node`, as its key
    /// and the matrix it is painted with now - or `None` when it opens none.
    ///
    /// THE rule, shared by the display-list builder (which emits a
    /// `PushReferenceFrame` exactly when this is `Some`), the hit tester and
    /// every "where is it on screen" question. A node has up to three
    /// transform channels: its CSS `transform`, the ANIMATION channel - an
    /// engine-driven movement of the painted node (a FLIP slide, an enter
    /// track, a drag) - and the STICKY channel, a `position: sticky` box
    /// following its scroller. A key without a value is no channel.
    ///
    /// The node gets ONE frame. Its key is the key of the first channel it
    /// has, in that order (so the key a list binds does not change when a
    /// second channel starts or ends), and its matrix is the PRODUCT of all
    /// the channels it has, in that order: the CSS transform first, then the
    /// animation, then the sticky translation (`css.then(anim).then(sticky)`).
    /// Every channel's matrix acts in the node's own box, about its top-left
    /// corner, so the product does too. A dragged or sliding node that has a
    /// CSS transform of its own used to show the CSS matrix alone: the drag
    /// and the slide were not drawn, and a transformed sticky box did not
    /// follow its scroller between layouts.
    ///
    /// The value under the frame's key in [`Self::for_each_bound_value`] is
    /// this product: the display list, both renderers and the hit tester
    /// read the same matrix.
    #[must_use]
    pub fn reference_frame_of(&self, node: NodeId) -> Option<(TransformKey, ComputedTransform3D)> {
        let css = self
            .css_transform_keys
            .get(&node)
            .zip(self.css_current_transform_values.get(&node))
            .map(|(key, transform)| (*key, *transform));
        self.frame_of_channels(node, css)
    }

    /// The one frame of `node` ([`Self::reference_frame_of`]) with `css` as
    /// its CSS channel: the first channel's key and the product of the
    /// channels in the order CSS, animation, sticky.
    fn frame_of_channels(
        &self,
        node: NodeId,
        css: Option<(TransformKey, ComputedTransform3D)>,
    ) -> Option<(TransformKey, ComputedTransform3D)> {
        // The two other channels are empty in almost every DOM: no lookup then.
        let animation = if self.anim_current_transform_values.is_empty() {
            None
        } else {
            self.anim_transform_keys
                .get(&node)
                .zip(self.anim_current_transform_values.get(&node))
                .map(|(key, transform)| (*key, *transform))
        };
        let sticky = if self.sticky_current_transform_values.is_empty() {
            None
        } else {
            self.sticky_transform_keys
                .get(&node)
                .zip(self.sticky_current_transform_values.get(&node))
                .map(|(key, transform)| (*key, *transform))
        };
        let mut frame: Option<(TransformKey, ComputedTransform3D)> = None;
        for (key, transform) in [css, animation, sticky].into_iter().flatten() {
            frame = Some(match frame {
                None => (key, transform),
                Some((first_key, so_far)) => (first_key, so_far.then(&transform)),
            });
        }
        frame
    }

    /// The stamps of the six maps [`Self::reference_frame_of`] reads (keys and
    /// values of the CSS, animation and sticky channel). While they are what
    /// they were, every node of this cache has the reference frame and the
    /// matrix it had ([`StampedMap`]).
    #[must_use]
    pub const fn transform_stamps(&self) -> [u64; 6] {
        [
            self.css_transform_keys.stamp(),
            self.css_current_transform_values.stamp(),
            self.anim_transform_keys.stamp(),
            self.anim_current_transform_values.stamp(),
            self.sticky_transform_keys.stamp(),
            self.sticky_current_transform_values.stamp(),
        ]
    }

    /// [`Self::reference_frame_of`] for a node laid out at `size` (its border
    /// box, logical px): the CSS channel's matrix is resolved against that
    /// box. The cache holds what the PREVIOUS pass's sizes gave, and nothing
    /// is known before a node's first layout, so a percentage origin (the
    /// default `50% 50%`) would pivot at the corner. The display list is
    /// built after layout, so it bakes this pass's matrix: a renderer that
    /// reads the list's own value (a `VirtualView`'s list on the CPU, a
    /// screenshot) draws the same as one that reads the live value.
    ///
    /// Where the cache's value was resolved against this very `size` (the
    /// steady state: the layout kept the node's box) it IS that matrix and
    /// nothing is built. `styled_dom` must be the DOM the cache was last
    /// synchronized with ([`Self::synchronize_with_sizes`], which every
    /// display-list build runs first).
    #[must_use]
    pub fn reference_frame_at(
        &self,
        styled_dom: &StyledDom,
        node: NodeId,
        size: (f32, f32),
    ) -> Option<(TransformKey, ComputedTransform3D)> {
        let css = self
            .css_transform_keys
            .get(&node)
            .zip(self.css_current_transform_values.get(&node))
            .and_then(|(key, live)| {
                // The value the cache holds was resolved against this very
                // size from this cascade (`css_transform_sizes`): it is the
                // matrix a build would give, without the build.
                let at_size = if self.css_transform_sizes.get(&node) == Some(&size) {
                    Some(*live)
                } else {
                    Self::css_transform_of(styled_dom, node, size)
                };
                at_size.map(|transform| (*key, transform))
            });
        // The CSS channel at this pass's size, with the node's other channels
        // on top of it; a node whose cascade holds no transform any more
        // keeps what the cache has for it until the next sync removes it.
        match css {
            Some(css) => self.frame_of_channels(node, Some(css)),
            None => self.reference_frame_of(node),
        }
    }

    /// The matrix `node` is painted with now ([`Self::reference_frame_of`]),
    /// `None` when it is painted untransformed.
    #[must_use]
    pub fn painted_transform_of(&self, node: NodeId) -> Option<ComputedTransform3D> {
        self.reference_frame_of(node).map(|(_, transform)| transform)
    }

    /// Fingerprint of the KEY POPULATION the display-list builder consumes —
    /// which nodes carry which transform/opacity keys, and (for the channels
    /// the builder `zip`s with their value map) whether a value exists.
    ///
    /// This exists because the solver's structural-identity display-list cache
    /// keyed on (root subtree hash, viewport) alone, and the emitted list is
    /// ALSO a function of this population: `PushReferenceFrame` is emitted for
    /// a node exactly when it has a key+value pair. Diff-driven animation
    /// mints its keys AFTER the first layout (First/Last need solved rects),
    /// so the very next relayout of the unchanged DOM hit the cache and served
    /// the PRE-KEY display list back — no reference frames, so no GPU damage,
    /// so the animation was invisible and every subsequent screenshot froze.
    ///
    /// Deliberately a population fingerprint, not a value fingerprint: values
    /// change every animation tick, and serving the cached list across ticks
    /// is the entire point of routing animation through GPU keys. The hash
    /// covers exactly the maps the builder reads: css/anim transform keys
    /// (plus the keysets of their value maps — a key without a value emits
    /// nothing), scrollbar v/h thumb transform keys, and scrollbar v/h
    /// opacity keys. In-process comparison only, so hasher stability across
    /// runs is not required; iteration order is normalised by sorting.
    #[must_use]
    pub fn dl_emission_fingerprint(&self) -> u64 {
        const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
        const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
        let mut entries: Vec<(u8, u64, u64)> = Vec::with_capacity(
            self.css_transform_keys.len()
                + self.anim_transform_keys.len()
                + self.css_current_transform_values.len()
                + self.anim_current_transform_values.len()
                + self.transform_keys.len()
                + self.h_transform_keys.len()
                + self.scrollbar_v_opacity_keys.len()
                + self.scrollbar_h_opacity_keys.len()
                + self.opacity_keys.len()
                + self.current_opacity_values.len()
                + self.sticky_transform_keys.len()
                + self.sheen_transform_keys.len()
                + self.sheen_opacity_keys.len(),
        );
        for (n, k) in &self.css_transform_keys {
            entries.push((0, n.index() as u64, k.id as u64));
        }
        for n in self.css_current_transform_values.keys() {
            entries.push((1, n.index() as u64, 0));
        }
        for (n, k) in &self.anim_transform_keys {
            entries.push((2, n.index() as u64, k.id as u64));
        }
        for n in self.anim_current_transform_values.keys() {
            entries.push((3, n.index() as u64, 0));
        }
        for (n, k) in &self.transform_keys {
            entries.push((4, n.index() as u64, k.id as u64));
        }
        for (n, k) in &self.h_transform_keys {
            entries.push((5, n.index() as u64, k.id as u64));
        }
        for ((d, n), k) in &self.scrollbar_v_opacity_keys {
            entries.push((6, (d.inner as u64) << 32 | n.index() as u64, k.id as u64));
        }
        for ((d, n), k) in &self.scrollbar_h_opacity_keys {
            entries.push((7, (d.inner as u64) << 32 | n.index() as u64, k.id as u64));
        }
        // Animated opacity binds `PushOpacity.opacity_key`, so its population
        // shapes the emitted list the same way animated transforms do.
        for (n, k) in &self.anim_opacity_keys {
            entries.push((8, n.index() as u64, k.id as u64));
        }
        for n in self.anim_current_opacity_values.keys() {
            entries.push((9, n.index() as u64, 0));
        }
        // So does the CSS `opacity` channel: `PushOpacity` binds a node's CSS
        // key when it has no animation key (an `opacity` tween then steps by
        // value alone, `refresh_opacity_value_of`).
        for (n, k) in &self.opacity_keys {
            entries.push((10, n.index() as u64, k.id as u64));
        }
        for n in self.current_opacity_values.keys() {
            entries.push((11, n.index() as u64, 0));
        }
        // A sticky box opens a reference frame exactly when it has a key
        // (its value always exists with it).
        for (n, k) in &self.sticky_transform_keys {
            entries.push((12, n.index() as u64, k.id as u64));
        }
        // A material opens its sheen layer exactly when it has both keys.
        for (n, k) in &self.sheen_transform_keys {
            entries.push((13, n.index() as u64, k.id as u64));
        }
        for (n, k) in &self.sheen_opacity_keys {
            entries.push((14, n.index() as u64, k.id as u64));
        }
        // A background rect is bound to a colour key exactly when its node
        // has one (`color_keys`).
        for (n, k) in &self.color_keys {
            entries.push((16, n.index() as u64, k.id as u64));
        }
        // ...in the colour of the window's light, baked into its stops.
        if let Some((c, intensity)) = self.sheen_light {
            let rgba = u64::from(u32::from_be_bytes([c.r, c.g, c.b, c.a]));
            entries.push((15, rgba, u64::from(intensity.to_bits())));
        }
        entries.sort_unstable();
        // FNV-1a over the sorted entry words. Hand-rolled because this file
        // builds under no_std (where `HashMap` above is really `BTreeMap` and
        // `DefaultHasher` does not exist) — and in-process comparison needs
        // no cryptographic strength, only sensitivity to every entry.
        let mut h: u64 = FNV_OFFSET;
        for (tag, a, b) in entries {
            for word in [u64::from(tag), a, b] {
                h ^= word;
                h = h.wrapping_mul(FNV_PRIME);
            }
        }
        // An empty population must not collide with "no cache entry" sentinels
        // downstream; FNV_OFFSET is a fine non-zero value for it.
        h
    }

    /// Every value a display list can BIND, by key id: `on_transform` for
    /// each transform key with a value (scrollbar thumbs, CSS `transform`,
    /// the animation channel), `on_opacity` for each opacity key with a value
    /// (scrollbar fades of `dom_id`, CSS `opacity`, the animation channel).
    ///
    /// THE one "current animated values by key" source. The CPU renderer
    /// (`cpurender::extract_gpu_values`) and `WebRender`'s dynamic properties
    /// (`wr_translate2::synchronize_gpu_values`) both read it, so a value that
    /// changes without a display-list rebuild - an animation tick - reaches
    /// both backends the same way.
    pub fn for_each_bound_value(
        &self,
        dom_id: DomId,
        mut on_transform: impl FnMut(usize, &ComputedTransform3D),
        mut on_opacity: impl FnMut(usize, f32),
    ) {
        for (node_id, key) in &self.transform_keys {
            if let Some(value) = self.current_transform_values.get(node_id) {
                on_transform(key.id, value);
            }
        }
        for (node_id, key) in &self.h_transform_keys {
            if let Some(value) = self.h_current_transform_values.get(node_id) {
                on_transform(key.id, value);
            }
        }
        // A node's frame is bound to the key of its FIRST channel (CSS,
        // animation, sticky) and painted with the product of all its
        // channels (`reference_frame_of`): that product is the value under
        // the frame's key. A key that is not its node's frame key binds
        // nothing in a list and carries its own channel's value. A DOM in
        // which no node has a second channel takes the plain values without a
        // lookup more than before.
        let animated = !self.anim_current_transform_values.is_empty();
        let stuck = !self.sticky_current_transform_values.is_empty();
        // The ANIMATION channel is separate from the CSS one because
        // `synchronize` owns `css_transform_keys` and evicts anything not
        // backed by a CSS `transform`.
        for (node_id, key) in &self.anim_transform_keys {
            if let Some(value) = self.anim_current_transform_values.get(node_id) {
                // The animation key is the frame key of a node without a CSS
                // transform; its product differs from its own value only when
                // the node is sticky as well.
                let product = if stuck {
                    self.reference_frame_of(*node_id)
                        .filter(|(frame_key, _)| frame_key == key)
                        .map(|(_, product)| product)
                } else {
                    None
                };
                match product {
                    Some(product) => on_transform(key.id, &product),
                    None => on_transform(key.id, value),
                }
            }
        }
        for (node_id, key) in &self.css_transform_keys {
            if let Some(value) = self.css_current_transform_values.get(node_id) {
                if animated || stuck {
                    // The CSS key is its node's frame key whenever it has a value.
                    let product = self
                        .reference_frame_of(*node_id)
                        .map_or(*value, |(_, product)| product);
                    on_transform(key.id, &product);
                } else {
                    on_transform(key.id, value);
                }
            }
        }
        for (node_id, key) in &self.sticky_transform_keys {
            if let Some(value) = self.sticky_current_transform_values.get(node_id) {
                on_transform(key.id, value);
            }
        }
        for (node_id, key) in &self.sheen_transform_keys {
            if let Some(value) = self.sheen_current_transform_values.get(node_id) {
                on_transform(key.id, value);
            }
        }
        for (node_id, key) in &self.sheen_opacity_keys {
            if let Some(&value) = self.sheen_current_opacity_values.get(node_id) {
                on_opacity(key.id, value);
            }
        }
        // The opacity twin of the frame rule: a node's layer binds its
        // ANIMATION opacity key when it has one, else its CSS key (the
        // display-list builder's `PushOpacity`), and is painted at the
        // product of the two - a half-transparent node that slides or fades
        // in stays half transparent instead of showing the animation's
        // opacity alone.
        for (node_id, key) in &self.anim_opacity_keys {
            if let Some(&value) = self.anim_current_opacity_values.get(node_id) {
                let css = if self.current_opacity_values.is_empty() {
                    None
                } else {
                    self.current_opacity_values.get(node_id)
                };
                on_opacity(key.id, css.map_or(value, |css| value * css));
            }
        }
        for (node_id, key) in &self.opacity_keys {
            if let Some(&value) = self.current_opacity_values.get(node_id) {
                on_opacity(key.id, value);
            }
        }
        for ((d, node_id), key) in &self.scrollbar_v_opacity_keys {
            if *d == dom_id {
                if let Some(&value) = self.scrollbar_v_opacity_values.get(&(*d, *node_id)) {
                    on_opacity(key.id, value);
                }
            }
        }
        for ((d, node_id), key) in &self.scrollbar_h_opacity_keys {
            if *d == dom_id {
                if let Some(&value) = self.scrollbar_h_opacity_values.get(&(*d, *node_id)) {
                    on_opacity(key.id, value);
                }
            }
        }
    }

    /// Every bound background colour, by key id: `on_color` for each colour
    /// key with a value ([`Self::color_keys`]) - the colour twin of
    /// [`Self::for_each_bound_value`], read by both renderers.
    pub fn for_each_bound_color(&self, mut on_color: impl FnMut(usize, azul_css::props::basic::ColorU)) {
        for (node_id, key) in &self.color_keys {
            if let Some(&value) = self.current_color_values.get(node_id) {
                on_color(key.id, value);
            }
        }
    }

    /// Synchronizes the cache with the current `StyledDom`, generating change events
    /// for CSS transform and opacity additions, modifications, and removals.
    ///
    /// Split into read-only `compute_*_events` passes (which diff against the cache)
    /// and `apply_*_events` passes (which mutate it).
    #[must_use]
    pub fn synchronize(&mut self, styled_dom: &StyledDom) -> GpuEventChanges {
        self.synchronize_with_sizes(styled_dom, &|_| None)
    }

    /// [`Self::synchronize`] with the nodes' border-box sizes (logical px),
    /// which is what `transform-origin` and `translate()` percentages
    /// resolve against (CSS Transforms 1: the element's own box). Layout
    /// runs AFTER this sync, so callers pass the PREVIOUS pass's sizes —
    /// exact in steady state — and correct the values once the new sizes
    /// exist with [`Self::refresh_transform_values`]. With no size source
    /// a percentage origin resolves to 0, i.e. the top-left corner: that
    /// was the only behaviour before, and it pivoted every `rotate()` /
    /// `scale()` under the default `transform-origin: 50% 50%` at the
    /// corner instead of the centre.
    #[must_use]
    pub fn synchronize_with_sizes(
        &mut self,
        styled_dom: &StyledDom,
        node_size: &dyn Fn(NodeId) -> Option<(f32, f32)>,
    ) -> GpuEventChanges {
        Self::init_simd_features();

        // Every node with a CSS transform and the size its matrix was just
        // resolved against (one build per node: the only one of this pass
        // when the layout that follows keeps the node's size).
        let mut resolved_at: Vec<(NodeId, (f32, f32))> =
            Vec::with_capacity(self.css_transform_keys.len());
        let transform_key_changes =
            self.compute_transform_events(styled_dom, node_size, &mut resolved_at);
        self.apply_transform_events(&transform_key_changes);
        for (node_id, size) in resolved_at {
            if self.css_transform_sizes.get(&node_id) != Some(&size) {
                self.css_transform_sizes.insert(node_id, size);
            }
        }

        let opacity_key_changes = self.compute_opacity_events(styled_dom);
        self.apply_opacity_events(&opacity_key_changes);

        self.sync_sticky_keys(styled_dom);

        GpuEventChanges {
            transform_key_changes,
            opacity_key_changes,
            scrollbar_opacity_changes: Vec::new(), // Filled by separate synchronization
        }
    }

    /// Gives every `position: sticky` node of `styled_dom` a key in the
    /// sticky channel (its value the identity until a scroll moves the box)
    /// and drops the keys of nodes that are no longer sticky. Read from the
    /// compact cache where it exists (one byte per node); a node in an
    /// interaction state, or a DOM without the cache, walks the cascade.
    fn sync_sticky_keys(&mut self, styled_dom: &StyledDom) {
        use azul_css::props::layout::LayoutPosition;

        let cache = styled_dom.get_css_property_cache();
        let node_data = styled_dom.node_data.as_container();
        let states = styled_dom.styled_nodes.as_container();
        let mut sticky: Vec<NodeId> = Vec::new();
        for index in 0..node_data.len() {
            let node = NodeId::new(index);
            let state = &states[node].styled_node_state;
            let position = match cache.compact_cache.as_ref() {
                Some(cc) if state.is_normal() => cc.get_position(index),
                _ => cache
                    .get_position(&node_data[node], &node, state)
                    .and_then(|p| p.get_property().copied())
                    .unwrap_or_default(),
            };
            if position == LayoutPosition::Sticky {
                sticky.push(node);
            }
        }
        if sticky.is_empty() && self.sticky_transform_keys.is_empty() {
            return;
        }
        // `sticky` is in node order (the loop above): a binary search, where
        // `Vec::contains` made this quadratic in the sticky boxes. Through
        // the write methods, so a box that became or stopped being sticky
        // moves the write generation.
        self.retain_transforms(TransformChannel::Sticky, |node| {
            sticky.binary_search(&node).is_ok()
        });
        for node in sticky {
            if !self.sticky_transform_keys.contains_key(&node) {
                self.set_transform(
                    TransformChannel::Sticky,
                    node,
                    TransformKey::unique(),
                    ComputedTransform3D::IDENTITY,
                );
            }
        }
    }

    /// One-time CPU feature detection (SSE/AVX) for the transform math fast paths.
    #[allow(clippy::missing_const_for_fn)] // non-x86_64 body is empty; x86_64 uses atomics
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
    /// The node's CSS `transform` as a matrix, with percentages resolved
    /// against `size` (its border box, logical px); `None` when the node
    /// has no transform.
    fn css_transform_of(
        styled_dom: &StyledDom,
        node_id: NodeId,
        size: (f32, f32),
    ) -> Option<ComputedTransform3D> {
        let css_property_cache = styled_dom.get_css_property_cache();
        let node_data = &styled_dom.node_data.as_container()[node_id];
        let styled_node_state = &styled_dom.styled_nodes.as_container()[node_id].styled_node_state;
        let transform_prop =
            css_property_cache.get_transform(node_data, &node_id, styled_node_state);
        let t = transform_prop.as_ref().and_then(|v| v.get_property())?;
        #[cfg(all(test, feature = "std"))]
        CSS_MATRICES_BUILT.with(|built| built.set(built.get() + 1));
        let default_transform_origin = StyleTransformOrigin::default();
        let transform_origin =
            css_property_cache.get_transform_origin(node_data, &node_id, styled_node_state);
        let transform_origin = transform_origin
            .as_ref()
            .and_then(|o| o.get_property())
            .unwrap_or(&default_transform_origin);
        Some(ComputedTransform3D::from_style_transform_vec(
            t.as_ref(),
            transform_origin,
            size.0,
            size.1,
            RotationMode::ForWebRender,
        ))
    }

    /// AFTER layout: recompute every cached CSS transform with the nodes'
    /// real sizes (see [`Self::synchronize_with_sizes`]). Returns how many
    /// values changed. Both compositors read the LIVE values from this cache
    /// (the display list's baked matrix is only the fallback for a key
    /// nothing has published), so a corrected value reaches the screen in
    /// the same frame.
    ///
    /// Only a value whose node has another size than the one it was
    /// resolved against is built again: call this after
    /// [`Self::synchronize_with_sizes`] of the SAME `styled_dom` (the layout
    /// pass does), which has just read every value from the cascade.
    pub fn refresh_transform_values(
        &mut self,
        styled_dom: &StyledDom,
        node_size: &dyn Fn(NodeId) -> Option<(f32, f32)>,
    ) -> usize {
        let node_count = styled_dom.node_data.len();
        // The values whose size is another one than they were resolved
        // against (a node the layout resized, a node laid out for the first
        // time): usually none, and then nothing is built and nothing
        // allocated. The cascade is the one the sync of this pass read, so
        // the same size is the same matrix.
        let mut stale: Vec<(NodeId, (f32, f32))> = Vec::new();
        for node_id in self.css_transform_keys.keys() {
            if node_id.index() >= node_count {
                continue;
            }
            let Some(size) = node_size(*node_id) else {
                continue;
            };
            if self.css_transform_sizes.get(node_id) == Some(&size)
                && self.css_current_transform_values.contains_key(node_id)
            {
                continue;
            }
            stale.push((*node_id, size));
        }
        let mut changed = 0;
        for (node_id, size) in stale {
            let Some(fresh) = Self::css_transform_of(styled_dom, node_id, size) else {
                continue;
            };
            self.css_transform_sizes.insert(node_id, size);
            if self.css_current_transform_values.get(&node_id) != Some(&fresh) {
                self.css_current_transform_values.insert(node_id, fresh);
                changed += 1;
            }
        }
        if changed > 0 {
            self.note_value_write();
        }
        changed
    }

    /// One node's [`Self::refresh_transform_values`]: re-read `node`'s CSS
    /// `transform` from the cascade (its user override first) and publish it
    /// under the key it ALREADY has, with percentages resolved against
    /// `size` (its border box, logical px).
    ///
    /// The per-frame channel of a `transform` tween: the reference frame
    /// exists (the key was minted by `synchronize`), only its matrix moves,
    /// and both compositors read the matrix live from here - no layout, no
    /// display-list rebuild. Returns `false` and changes nothing when the
    /// node has no key, or no longer resolves to a transform: that is a
    /// change of the key POPULATION, which only a display-list build can
    /// show (`synchronize` then adds or removes the key).
    pub fn refresh_transform_value_of(
        &mut self,
        styled_dom: &StyledDom,
        node_id: NodeId,
        size: (f32, f32),
    ) -> bool {
        if node_id.index() >= styled_dom.node_data.len()
            || !self.css_transform_keys.contains_key(&node_id)
        {
            return false;
        }
        let Some(fresh) = Self::css_transform_of(styled_dom, node_id, size) else {
            return false;
        };
        self.css_transform_sizes.insert(node_id, size);
        if self.css_current_transform_values.insert(node_id, fresh) != Some(fresh) {
            self.note_value_write();
        }
        true
    }

    /// The `opacity` twin of [`Self::refresh_transform_value_of`]: re-read
    /// `node`'s CSS `opacity` from the cascade (its user override first) and
    /// publish it under the key it ALREADY has - the per-frame channel of an
    /// `opacity` tween, read live by both compositors through the key the
    /// display list's `PushOpacity` binds. Returns that key, or `None` and
    /// changes nothing when the node has no key or no longer resolves to an
    /// opacity (a change of the key population: only a display-list build
    /// can show it).
    pub fn refresh_opacity_value_of(
        &mut self,
        styled_dom: &StyledDom,
        node_id: NodeId,
    ) -> Option<OpacityKey> {
        if node_id.index() >= styled_dom.node_data.len() {
            return None;
        }
        let key = *self.opacity_keys.get(&node_id)?;
        let node_data = &styled_dom.node_data.as_container()[node_id];
        let state = &styled_dom.styled_nodes.as_container()[node_id].styled_node_state;
        let value = styled_dom
            .get_css_property_cache()
            .get_opacity(node_data, &node_id, state)?
            .get_property()?
            .inner
            .normalized();
        if self.current_opacity_values.insert(node_id, value) != Some(value) {
            self.note_value_write();
        }
        Some(key)
    }

    /// Computes CSS-transform change events against the cached values
    /// (read-only). `resolved_at` receives every node that has a transform
    /// now, with the size its matrix was resolved against.
    fn compute_transform_events(
        &self,
        styled_dom: &StyledDom,
        node_size: &dyn Fn(NodeId) -> Option<(f32, f32)>,
        resolved_at: &mut Vec<(NodeId, (f32, f32))>,
    ) -> Vec<GpuTransformKeyEvent> {
        let css_property_cache = styled_dom.get_css_property_cache();
        let node_states = styled_dom.styled_nodes.as_container();

        // calculate the transform values of every single node that has a non-default transform.
        //
        // GPU fast path: `has_transform` is a single bit in the compact cache.
        // The overwhelmingly common case is "no transform set", which now reads one
        // byte and bails — no cascade walk. Only nodes that actually have a
        // transform pay the slow-walk cost (required to retrieve the parsed value).
        let mut events = (0..styled_dom.node_data.len())
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
                                || !self.css_current_transform_values.contains_key(&node_id))
                        {
                            return None;
                        }
                    }
                }
                // `css_transform_of` turns "no transform cascade entry" (the
                // ordinary case) into `None` rather than skipping the node, so
                // a node that just LOST its transform still reaches the
                // `(Some(old), None) => Removed` arm and its cached
                // TransformKey is evicted. Percentages resolve against the
                // node's own box — the previous pass's size before layout, 0
                // (the corner) when no size is known yet.
                let size = node_size(node_id).unwrap_or((0.0, 0.0));
                let current_transform = Self::css_transform_of(styled_dom, node_id, size);

                let existing_transform = if self.css_current_transform_values.is_empty() {
                    None
                } else {
                    self.css_current_transform_values.get(&node_id)
                };

                // `resolved_at` takes the node in every arm that leaves the
                // cache holding `new`, the matrix resolved against `size`.
                match (existing_transform, current_transform) {
                    (None, None) => None, // no new transform, no old transform
                    (None, Some(new)) => {
                        resolved_at.push((node_id, size));
                        Some(GpuTransformKeyEvent::Added(
                            node_id,
                            TransformKey::unique(),
                            new,
                        ))
                    }
                    // A value that is what it was is no event: a map's 2 000
                    // labels each reported `Changed(old, old)` on every sync.
                    (Some(old), Some(new)) if *old == new => {
                        resolved_at.push((node_id, size));
                        None
                    }
                    (Some(old), Some(new)) => {
                        let key = self.css_transform_keys.get(&node_id).copied()?;
                        resolved_at.push((node_id, size));
                        Some(GpuTransformKeyEvent::Changed(node_id, key, *old, new))
                    }
                    (Some(_old), None) => Some(GpuTransformKeyEvent::Removed(
                        node_id,
                        self.css_transform_keys.get(&node_id).copied()?,
                    )),
                }
            })
            .collect::<Vec<GpuTransformKeyEvent>>();

        // Structural shrink: any cached transform key whose node no longer
        // exists in the (smaller) DOM is never visited by the loop above, so it
        // would leak on the GPU. Emit an explicit Removed for those.
        let node_count = styled_dom.node_data.len();
        for (node_id, key) in &self.css_transform_keys {
            if node_id.index() >= node_count {
                events.push(GpuTransformKeyEvent::Removed(*node_id, *key));
            }
        }

        events
    }

    /// Applies transform key changes (additions/removals) to the cache.
    fn apply_transform_events(&mut self, events: &[GpuTransformKeyEvent]) {
        // remove / add the CSS transform keys accordingly
        // Whether a key or a value is another one after an event (an event
        // that writes the value the cache holds changes nothing).
        let mut wrote = false;
        for event in events {
            match &event {
                // An event says nothing of the size its value was resolved
                // against: the entry goes, and `synchronize_with_sizes` (which
                // knows the size) writes it again after this.
                GpuTransformKeyEvent::Added(node_id, key, matrix) => {
                    wrote |= self.css_transform_keys.insert(*node_id, *key) != Some(*key);
                    wrote |= self.css_current_transform_values.insert(*node_id, *matrix)
                        != Some(*matrix);
                    self.css_transform_sizes.remove(node_id);
                }
                GpuTransformKeyEvent::Changed(node_id, _key, _old_state, new_state) => {
                    wrote |= self
                        .css_current_transform_values
                        .insert(*node_id, *new_state)
                        != Some(*new_state);
                    self.css_transform_sizes.remove(node_id);
                }
                GpuTransformKeyEvent::Removed(node_id, _key) => {
                    wrote |= self.css_transform_keys.remove(node_id).is_some();
                    wrote |= self.css_current_transform_values.remove(node_id).is_some();
                    self.css_transform_sizes.remove(node_id);
                }
            }
        }
        if wrote {
            self.note_value_write();
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
        let mut events = (0..styled_dom.node_data.len())
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
                            self.current_opacity_values.get(&node_id)?;
                            None
                        } else {
                            Some(f32::from(raw) / 254.0)
                        };
                    }
                }

                let node_data = &node_data[node_id];
                let current_opacity: Option<f32> = if let Some(v) = compact_opacity {
                    // Fast path: value already read from compact cache.
                    Some(v)
                } else if styled_node_state.is_normal()
                    && css_property_cache.compact_cache.is_some()
                {
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
                    // As for a transform: the same opacity is no event.
                    (Some(old), Some(new)) if *old == new => None,
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
            .collect::<Vec<GpuOpacityKeyEvent>>();

        // Structural shrink: emit Removed for cached opacity keys whose node no
        // longer exists in the (smaller) DOM (never visited by the loop above).
        let node_count = styled_dom.node_data.len();
        for (node_id, key) in &self.opacity_keys {
            if node_id.index() >= node_count {
                events.push(GpuOpacityKeyEvent::Removed(*node_id, *key));
            }
        }

        events
    }

    /// Applies opacity key changes (additions/removals) to the cache.
    fn apply_opacity_events(&mut self, events: &[GpuOpacityKeyEvent]) {
        // remove / add the opacity keys accordingly
        let mut wrote = false;
        for event in events {
            match &event {
                GpuOpacityKeyEvent::Added(node_id, key, opacity) => {
                    wrote |= self.opacity_keys.insert(*node_id, *key) != Some(*key);
                    wrote |=
                        self.current_opacity_values.insert(*node_id, *opacity) != Some(*opacity);
                }
                GpuOpacityKeyEvent::Changed(node_id, _key, _old_state, new_state) => {
                    wrote |= self.current_opacity_values.insert(*node_id, *new_state)
                        != Some(*new_state);
                }
                GpuOpacityKeyEvent::Removed(node_id, _key) => {
                    wrote |= self.opacity_keys.remove(node_id).is_some();
                    wrote |= self.current_opacity_values.remove(node_id).is_some();
                }
            }
        }
        if wrote {
            self.note_value_write();
        }
    }
}

/// Represents a change to a scrollbar opacity key.
///
/// Scrollbar opacity is managed separately from CSS opacity to enable
/// independent fading animations without affecting element opacity.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
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
/// for efficient batch processing when updating `WebRender`.
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
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// Returns `true` if there are no transform, opacity, or scrollbar opacity changes.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.transform_key_changes.is_empty()
            && self.opacity_key_changes.is_empty()
            && self.scrollbar_opacity_changes.is_empty()
    }

    /// Merges another `GpuEventChanges` into this one, consuming the other.
    ///
    /// This is useful for combining changes from multiple sources.
    pub fn merge(&mut self, other: &mut Self) {
        self.transform_key_changes
            .append(&mut other.transform_key_changes);
        self.opacity_key_changes
            .append(&mut other.opacity_key_changes);
        self.scrollbar_opacity_changes
            .append(&mut other.scrollbar_opacity_changes);
    }
}

/// Represents a change to a GPU opacity key.
///
/// These events are generated when synchronizing the cache with the `StyledDom`
/// and are used to update `WebRender`'s opacity state efficiently.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub enum GpuOpacityKeyEvent {
    /// A new opacity was added to a node
    Added(NodeId, OpacityKey, f32),
    /// An existing opacity was modified (includes old and new values)
    Changed(NodeId, OpacityKey, f32, f32),
    /// An opacity was removed from a node
    Removed(NodeId, OpacityKey),
}

#[cfg(test)]
#[path = "gpu_test.rs"]
mod gpu_test;

/// A2-gpu (audit 2026-11-21): what a synchronization reports and builds, how the
/// channels of one node compose, the write methods and the write generation
/// (`audit/A2-gpu.md`, findings A2-gpu-08, -10, -13).
#[cfg(test)]
mod audit_a2_gpu_tests {
    use super::*;
    use crate::dom::Dom;

    /// `body` with `n` children: the even ones `.t` (`rotate(45deg)` about the
    /// default origin, 50% 50%), the odd ones `.o` (`opacity: 0.25`).
    fn labels(n: usize) -> StyledDom {
        let css = azul_css::parser2::new_from_str(
            ".t { transform: rotate(45deg); } .o { opacity: 0.25; }",
        )
        .0;
        let children = (0..n)
            .map(|i| {
                let class = if i % 2 == 0 { "t" } else { "o" };
                Dom::create_div().with_class(class.to_string().into())
            })
            .collect::<Vec<Dom>>();
        let mut dom = Dom::create_body().with_children(children.into());
        StyledDom::create(&mut dom, css)
    }

    /// A2-gpu-08. A map view's labels each carry a CSS transform; a pass that
    /// changed none of them reported `Changed(old, old)` for every one (2 118
    /// events, about 300 KB, per layout pass of the Tuebingen view).
    ///
    /// 40 children: 20 transforms and 20 opacities. First sync: 20 + 20
    /// `Added`. Second sync of the same DOM at the same sizes: 0 + 0 events
    /// (was 20 + 20 `Changed`) and the write generation where it was. A sync at
    /// another box size (the origin is 50% 50%: (100, 20) instead of
    /// (50, 20)) moves every transform: 20 `Changed`, 0 opacity events.
    ///
    /// Mutation: the arm `(Some(old), Some(new)) if *old == new => None`
    /// removed from `compute_transform_events` fails "no event for a value
    /// that did not change".
    #[test]
    fn a_sync_of_unchanged_values_reports_no_event() {
        let styled = labels(40);
        let small = |_: NodeId| Some((100.0_f32, 40.0_f32));
        let wide = |_: NodeId| Some((200.0_f32, 40.0_f32));
        let mut cache = GpuValueCache::empty();

        let first = cache.synchronize_with_sizes(&styled, &small);
        assert_eq!(first.transform_key_changes.len(), 20);
        assert_eq!(first.opacity_key_changes.len(), 20);
        assert!(first
            .transform_key_changes
            .iter()
            .all(|e| matches!(e, GpuTransformKeyEvent::Added(..))));
        let generation = cache.value_generation();

        let second = cache.synchronize_with_sizes(&styled, &small);
        assert!(
            second.transform_key_changes.is_empty(),
            "no event for a value that did not change: {:?}",
            second.transform_key_changes
        );
        assert!(
            second.opacity_key_changes.is_empty(),
            "no event for an opacity that did not change: {:?}",
            second.opacity_key_changes
        );
        assert_eq!(cache.value_generation(), generation, "nothing was written");

        let third = cache.synchronize_with_sizes(&styled, &wide);
        assert_eq!(third.transform_key_changes.len(), 20, "another origin, another matrix");
        assert!(third
            .transform_key_changes
            .iter()
            .all(|e| matches!(e, GpuTransformKeyEvent::Changed(..))));
        assert!(third.opacity_key_changes.is_empty());
        assert_ne!(cache.value_generation(), generation);
    }

    /// A2-gpu-08. One layout pass asked for every CSS matrix three times with
    /// one cascade: the sync before layout, the display-list builder
    /// (`reference_frame_at`), the refresh after layout. 2 118 labels: 6 354
    /// cascade walks and matrix products per pass.
    ///
    /// 20 transformed nodes. First pass, sizes unknown before layout: 20
    /// (sync at (0, 0)) + 20 (list at 100 x 40) + 20 (refresh at 100 x 40) =
    /// 60 builds, as before. Second pass, nothing changed: 20 + 0 + 0 = 20
    /// builds (was 60), no event, and the list bakes the matrix a build
    /// gives. One node resized by the layout: 1 build by the list and 1 by
    /// the refresh, none for the 19 others.
    ///
    /// Mutations: the size comparison dropped from `reference_frame_at`
    /// (always build) fails "one build per transformed node"; the size
    /// comparison dropped from `refresh_transform_values` fails it too; a
    /// `refresh_transform_values` that skips every node fails "the refresh
    /// publishes the resized node's matrix".
    #[cfg(feature = "std")]
    #[test]
    fn a_pass_that_changes_nothing_builds_each_matrix_once() {
        let styled = labels(40);
        let small = |_: NodeId| Some((100.0_f32, 40.0_f32));
        let built = || CSS_MATRICES_BUILT.with(|count| count.get());
        let mut cache = GpuValueCache::empty();

        // The first pass: nothing is laid out yet when the sync runs.
        let before = built();
        let _ = cache.synchronize_with_sizes(&styled, &|_| None);
        let transformed: Vec<NodeId> = cache.css_transform_keys.keys().copied().collect();
        assert_eq!(transformed.len(), 20);
        assert_eq!(built() - before, 20, "the sync builds each matrix once");
        for node in &transformed {
            let _ = cache.reference_frame_at(&styled, *node, (100.0, 40.0));
        }
        assert_eq!(built() - before, 40, "the first list is built at sizes the sync did not know");
        assert_eq!(cache.refresh_transform_values(&styled, &small), 20);
        assert_eq!(built() - before, 60);

        // The second pass changes nothing.
        let before = built();
        let events = cache.synchronize_with_sizes(&styled, &small);
        assert!(events.transform_key_changes.is_empty());
        let frames: Vec<(TransformKey, ComputedTransform3D)> = transformed
            .iter()
            .map(|node| {
                cache
                    .reference_frame_at(&styled, *node, (100.0, 40.0))
                    .expect("a transformed node opens a frame")
            })
            .collect();
        assert_eq!(cache.refresh_transform_values(&styled, &small), 0);
        assert_eq!(
            built() - before,
            20,
            "one build per transformed node in a pass that changes nothing"
        );
        for (node, (key, baked)) in transformed.iter().zip(&frames) {
            assert_eq!(cache.css_transform_keys.get(node), Some(key));
            assert_eq!(
                GpuValueCache::css_transform_of(&styled, *node, (100.0, 40.0)),
                Some(*baked),
                "the list bakes what a build gives"
            );
        }

        // The layout resizes one node: the list and the refresh build its
        // matrix at the new size, nobody else's.
        let resized = transformed[0];
        let sizes = move |node: NodeId| {
            Some(if node == resized {
                (300.0_f32, 40.0_f32)
            } else {
                (100.0, 40.0)
            })
        };
        let before = built();
        let at_new_size = cache
            .reference_frame_at(&styled, resized, (300.0, 40.0))
            .expect("a frame")
            .1;
        assert_eq!(built() - before, 1);
        assert_ne!(
            cache.css_current_transform_values.get(&resized),
            Some(&at_new_size),
            "harness: another size is another matrix (the origin is 50% 50%)"
        );
        assert_eq!(cache.refresh_transform_values(&styled, &sizes), 1);
        assert_eq!(
            cache.css_current_transform_values.get(&resized),
            Some(&at_new_size),
            "the refresh publishes the resized node's matrix"
        );
        assert_eq!(built() - before, 2);
    }

    /// Every bound transform and opacity of `cache`, by key id.
    fn bound_values(cache: &GpuValueCache) -> (HashMap<usize, ComputedTransform3D>, HashMap<usize, f32>) {
        let mut transforms = HashMap::new();
        let mut opacities = HashMap::new();
        cache.for_each_bound_value(
            DomId::ROOT_ID,
            |key, value| {
                transforms.insert(key, *value);
            },
            |key, value| {
                opacities.insert(key, value);
            },
        );
        (transforms, opacities)
    }

    fn moved(m: &ComputedTransform3D, x: f32, y: f32) -> (f32, f32) {
        let p = m
            .transform_point2d(crate::geom::LogicalPosition::new(x, y))
            .expect("an affine matrix maps every point");
        (p.x, p.y)
    }

    /// A2-gpu-10. A node has one frame, and it showed one channel: a node
    /// with a CSS `transform` that was dragged, slid (FLIP) or stuck was
    /// painted with its CSS matrix alone, on both renderers and in the hit
    /// tester.
    ///
    /// By hand, in the node's own box (origin = its top-left corner): CSS
    /// `scale(2)` about the corner, a drag by (30, 10), a sticky shift of
    /// (0, -5). The point (10, 10): scaled (20, 20), dragged (50, 30), stuck
    /// (50, 25). Old: (20, 20). The other order - the drag and the shift
    /// scaled with the node - would give ((10 + 30) x 2, (10 - 5 + 10) x 2) =
    /// (80, 30).
    ///
    /// Mutations: `so_far.then(&transform)` -> `transform.then(&so_far)` in
    /// `frame_of_channels` gives (80, 30) and fails "CSS, then the drag, then
    /// the sticky shift"; `for_each_bound_value` publishing the CSS value
    /// under the CSS key fails "the frame's key carries the product".
    #[test]
    fn a_node_is_painted_with_the_product_of_its_channels() {
        let scale = ComputedTransform3D::new_scale(2.0, 2.0, 1.0);
        let drag = ComputedTransform3D::new_translation(30.0, 10.0, 0.0);
        let stick = ComputedTransform3D::new_translation(0.0, -5.0, 0.0);
        let (all, sliding, plain) = (NodeId::new(1), NodeId::new(2), NodeId::new(3));
        let (css_key, anim_key, sticky_key) =
            (TransformKey::unique(), TransformKey::unique(), TransformKey::unique());
        let (sliding_anim_key, sliding_sticky_key, plain_key) =
            (TransformKey::unique(), TransformKey::unique(), TransformKey::unique());

        let mut cache = GpuValueCache::empty();
        // `all`: every channel.
        cache.css_transform_keys.insert(all, css_key);
        cache.css_current_transform_values.insert(all, scale);
        cache.anim_transform_keys.insert(all, anim_key);
        cache.anim_current_transform_values.insert(all, drag);
        cache.sticky_transform_keys.insert(all, sticky_key);
        cache.sticky_current_transform_values.insert(all, stick);
        // `sliding`: no CSS transform, an animation and a sticky shift.
        cache.anim_transform_keys.insert(sliding, sliding_anim_key);
        cache.anim_current_transform_values.insert(sliding, drag);
        cache.sticky_transform_keys.insert(sliding, sliding_sticky_key);
        cache.sticky_current_transform_values.insert(sliding, stick);
        // `plain`: a CSS transform alone.
        cache.css_transform_keys.insert(plain, plain_key);
        cache.css_current_transform_values.insert(plain, scale);

        let (frame_key, product) = cache.reference_frame_of(all).expect("a frame");
        assert_eq!(frame_key, css_key, "the frame keeps the key of its first channel");
        assert_eq!(
            moved(&product, 10.0, 10.0),
            (50.0, 25.0),
            "CSS, then the drag, then the sticky shift"
        );
        assert_eq!(cache.painted_transform_of(all), Some(product));

        let (frame_key, product_sliding) = cache.reference_frame_of(sliding).expect("a frame");
        assert_eq!(frame_key, sliding_anim_key);
        assert_eq!(moved(&product_sliding, 10.0, 10.0), (40.0, 15.0));

        assert_eq!(cache.reference_frame_of(plain), Some((plain_key, scale)));
        assert_eq!(cache.reference_frame_of(NodeId::new(9)), None);

        // What both renderers read by key.
        let (transforms, _) = bound_values(&cache);
        assert_eq!(
            transforms.get(&css_key.id),
            Some(&product),
            "the frame's key carries the product"
        );
        assert_eq!(transforms.get(&anim_key.id), Some(&drag), "a key no list binds: its own value");
        assert_eq!(transforms.get(&sticky_key.id), Some(&stick));
        assert_eq!(transforms.get(&sliding_anim_key.id), Some(&product_sliding));
        assert_eq!(transforms.get(&sliding_sticky_key.id), Some(&stick));
        assert_eq!(transforms.get(&plain_key.id), Some(&scale));

        // The second channel ends: the frame is the CSS matrix again, under
        // the same key (no list is rebuilt for a key that did not change).
        cache.anim_transform_keys.remove(&all);
        cache.anim_current_transform_values.remove(&all);
        cache.sticky_transform_keys.remove(&all);
        cache.sticky_current_transform_values.remove(&all);
        assert_eq!(cache.reference_frame_of(all), Some((css_key, scale)));
    }

    /// A2-gpu-10, the list's side: `reference_frame_at` bakes the CSS matrix
    /// of this pass's size WITH the node's other channels.
    ///
    /// A `.t` node (`rotate(45deg)` about the centre of its 100 x 40 box,
    /// (50, 20)) dragged by (30, 10): its centre stays the centre under the
    /// rotation and is then moved by the drag: (80, 30). Old: (50, 20).
    #[test]
    fn the_list_bakes_the_product_of_the_channels() {
        let styled = labels(4);
        let size = (100.0_f32, 40.0_f32);
        let mut cache = GpuValueCache::empty();
        let _ = cache.synchronize_with_sizes(&styled, &|_| Some(size));
        let node = *cache.css_transform_keys.keys().next().expect("a transformed node");
        let css_key = cache.css_transform_keys[&node];
        let drag = ComputedTransform3D::new_translation(30.0, 10.0, 0.0);
        cache.anim_transform_keys.insert(node, TransformKey::unique());
        cache.anim_current_transform_values.insert(node, drag);

        for at in [size, (60.0_f32, 40.0_f32)] {
            let (key, baked) = cache.reference_frame_at(&styled, node, at).expect("a frame");
            assert_eq!(key, css_key);
            let (cx, cy) = (at.0 / 2.0, at.1 / 2.0);
            let (x, y) = moved(&baked, cx, cy);
            assert!(
                (x - (cx + 30.0)).abs() < 1e-3 && (y - (cy + 10.0)).abs() < 1e-3,
                "box {at:?}: the centre ({cx}, {cy}) is painted at ({x}, {y})"
            );
        }
    }

    /// A2-gpu-10, opacity: a node's layer binds its animation opacity key
    /// when it has one, and showed the animation's opacity alone. A node at
    /// CSS `opacity: 0.5` half way through a fade-in (0.4): 0.5 x 0.4 = 0.2
    /// under the key its layer binds. Old: 0.4.
    ///
    /// Mutation: `css.map_or(value, |css| value * css)` -> `value` fails it.
    #[test]
    fn a_fading_node_keeps_its_css_opacity() {
        let (both, fading) = (NodeId::new(1), NodeId::new(2));
        let (css_key, anim_key, fading_key) =
            (OpacityKey::unique(), OpacityKey::unique(), OpacityKey::unique());
        let mut cache = GpuValueCache::empty();
        cache.opacity_keys.insert(both, css_key);
        cache.current_opacity_values.insert(both, 0.5);
        cache.anim_opacity_keys.insert(both, anim_key);
        cache.anim_current_opacity_values.insert(both, 0.4);
        cache.anim_opacity_keys.insert(fading, fading_key);
        cache.anim_current_opacity_values.insert(fading, 0.4);

        let (_, opacities) = bound_values(&cache);
        let layer = opacities[&anim_key.id];
        assert!((layer - 0.2).abs() < 1e-6, "the layer's key carries 0.5 x 0.4: {layer}");
        assert_eq!(opacities.get(&css_key.id), Some(&0.5), "the CSS key: its own value");
        assert_eq!(opacities.get(&fading_key.id), Some(&0.4), "no CSS opacity: the animation's");
    }

    const TRANSFORM_CHANNELS: [TransformChannel; 6] = [
        TransformChannel::ScrollbarVertical,
        TransformChannel::ScrollbarHorizontal,
        TransformChannel::Css,
        TransformChannel::Animation,
        TransformChannel::Sticky,
        TransformChannel::Sheen,
    ];
    const OPACITY_CHANNELS: [OpacityChannel; 3] =
        [OpacityChannel::Css, OpacityChannel::Animation, OpacityChannel::Sheen];

    /// A2-gpu-13. The write generation is what lets a pan or an idle frame
    /// skip the copy of the bound values, their comparison and the walk for
    /// the compositor: it has to move with every write that changes a key or
    /// a value, and with no other (a thumb written where it is, a `retain`
    /// that drops nothing).
    ///
    /// Mutations: `self.wrote_transform(channel, node, key_changed ||
    /// value_changed)` -> `..., key_changed)` in `set_transform` fails "a new
    /// value"; `note_value_write` called unconditionally in `wrote` fails
    /// "the same value"; two channels sharing a map in `transform_maps_mut`
    /// fail "each channel is its own pair of maps".
    #[test]
    fn a_write_method_moves_the_generation_exactly_when_it_changes_something() {
        let node = NodeId::new(4);
        let a = ComputedTransform3D::new_translation(1.0, 2.0, 0.0);
        let b = ComputedTransform3D::new_translation(3.0, 4.0, 0.0);
        let mut cache = GpuValueCache::empty();
        // `moved(&mut generation)`: did the generation move since the last ask?
        fn moved_since(cache: &GpuValueCache, last: &mut u64) -> bool {
            let now = cache.value_generation();
            let moved = now != *last;
            *last = now;
            moved
        }
        let mut last = cache.value_generation();

        for channel in TRANSFORM_CHANNELS {
            let key = TransformKey::unique();
            assert!(cache.set_transform(channel, node, key, a));
            assert!(moved_since(&cache, &mut last), "{channel:?}: a new key and value");
            assert!(!cache.set_transform(channel, node, key, a));
            assert!(!moved_since(&cache, &mut last), "{channel:?}: the same key and value");
            assert_eq!(cache.set_transform_value(channel, node, a), Some(a));
            assert!(!moved_since(&cache, &mut last), "{channel:?}: the same value");
            assert_eq!(cache.set_transform_value(channel, node, b), Some(a));
            assert!(moved_since(&cache, &mut last), "{channel:?}: a new value");
            assert_eq!(cache.transform_key(channel, node), Some(key));
            assert_eq!(cache.transform_value(channel, node), Some(b));
            assert!(!cache.register_transform(channel, node, TransformKey::unique(), a));
            assert_eq!(cache.transform_key(channel, node), Some(key), "a bound node keeps its key");
            assert_eq!(cache.key_transform(channel, node, b), key);
            cache.retain_transforms(channel, |_| true);
            assert!(!moved_since(&cache, &mut last), "{channel:?}: nothing registered, keyed or dropped");
            assert_eq!(cache.remove_transform(channel, node), Some(key));
            assert!(moved_since(&cache, &mut last), "{channel:?}: the node is gone");
            assert_eq!(cache.remove_transform(channel, node), None);
            assert!(!moved_since(&cache, &mut last), "{channel:?}: nothing was left to remove");
            let minted = cache.key_transform(channel, node, a);
            assert!(moved_since(&cache, &mut last), "{channel:?}: a key was minted");
            assert_eq!(cache.transform_keys_of(channel).collect::<Vec<_>>(), vec![(node, minted)]);
            assert!(cache.register_transform(channel, NodeId::new(5), key, b));
            assert!(moved_since(&cache, &mut last));
            cache.clear_transforms(channel);
            assert!(moved_since(&cache, &mut last), "{channel:?}: emptied");
            cache.clear_transforms(channel);
            assert!(!moved_since(&cache, &mut last), "{channel:?}: it was empty");
            let only_a_key = cache.ensure_transform_key(channel, node);
            assert!(moved_since(&cache, &mut last), "{channel:?}: a key without a value yet");
            assert_eq!(cache.ensure_transform_key(channel, node), only_a_key);
            assert!(!moved_since(&cache, &mut last), "{channel:?}: the node had its key");
            assert_eq!(cache.transform_value(channel, node), None);
            cache.clear_transforms(channel);
            assert!(moved_since(&cache, &mut last));
        }

        for channel in OPACITY_CHANNELS {
            let key = OpacityKey::unique();
            assert!(cache.set_opacity(channel, node, key, 0.5));
            assert!(moved_since(&cache, &mut last), "{channel:?}");
            assert!(!cache.set_opacity(channel, node, key, 0.5));
            assert_eq!(cache.set_opacity_value(channel, node, 0.5), Some(0.5));
            assert_eq!(cache.key_opacity(channel, node, 0.5), key);
            cache.retain_opacities(channel, |_| true);
            assert!(!moved_since(&cache, &mut last), "{channel:?}: the same opacity");
            assert_eq!(cache.set_opacity_value(channel, node, 0.25), Some(0.5));
            assert!(moved_since(&cache, &mut last), "{channel:?}: another opacity");
            assert_eq!(cache.opacity_key(channel, node), Some(key));
            assert_eq!(cache.opacity_value(channel, node), Some(0.25));
            assert_eq!(cache.remove_opacity(channel, node), Some(key));
            assert!(moved_since(&cache, &mut last));
            assert_eq!(cache.remove_opacity(channel, node), None);
            assert!(!moved_since(&cache, &mut last));
            let _ = cache.key_opacity(channel, node, 0.75);
            assert!(moved_since(&cache, &mut last));
            cache.clear_opacities(channel);
            assert!(moved_since(&cache, &mut last));
            let only_a_key = cache.ensure_opacity_key(channel, node);
            assert!(moved_since(&cache, &mut last), "{channel:?}: a key without a value yet");
            assert_eq!(cache.ensure_opacity_key(channel, node), only_a_key);
            assert!(!moved_since(&cache, &mut last));
            assert_eq!(cache.opacity_keys_of(channel).collect::<Vec<_>>(), vec![(node, only_a_key)]);
            assert_eq!(cache.opacity_value(channel, node), None);
            cache.clear_opacities(channel);
            assert!(moved_since(&cache, &mut last));
        }

        let dom = DomId::ROOT_ID;
        for orientation in [ScrollbarOrientation::Vertical, ScrollbarOrientation::Horizontal] {
            let key = OpacityKey::unique();
            assert!(cache.register_scrollbar_opacity(orientation, dom, node, key, 0.0));
            assert!(moved_since(&cache, &mut last), "{orientation:?}");
            assert!(!cache.register_scrollbar_opacity(orientation, dom, node, OpacityKey::unique(), 1.0));
            assert_eq!(cache.set_scrollbar_opacity_value(orientation, dom, node, 0.0), Some(0.0));
            assert!(!cache.set_scrollbar_opacity(orientation, dom, node, key, 0.0));
            assert!(!moved_since(&cache, &mut last), "{orientation:?}: a bar that did not fade");
            assert_eq!(cache.set_scrollbar_opacity_value(orientation, dom, node, 0.6), Some(0.0));
            assert!(moved_since(&cache, &mut last), "{orientation:?}: a step of the fade");
            assert_eq!(cache.scrollbar_opacity_key(orientation, dom, node), Some(key));
            assert_eq!(cache.scrollbar_opacity(orientation, dom, node), Some(0.6));
            assert_eq!(cache.remove_scrollbar_opacity(orientation, dom, node), Some(key));
            assert!(moved_since(&cache, &mut last));
            assert_eq!(cache.remove_scrollbar_opacity(orientation, dom, node), None);
            assert!(!moved_since(&cache, &mut last));
        }

        let red = azul_css::props::basic::ColorU::new(255, 0, 0, 255);
        let blue = azul_css::props::basic::ColorU::new(0, 0, 255, 255);
        let color_key = cache.key_color(node, red);
        assert!(moved_since(&cache, &mut last), "a colour key was minted");
        assert_eq!(cache.key_color(node, red), color_key);
        assert_eq!(cache.set_color_value(node, red), Some(red));
        cache.retain_colors(|_| true);
        assert!(!moved_since(&cache, &mut last), "the same colour");
        assert_eq!(cache.set_color_value(node, blue), Some(red));
        assert!(moved_since(&cache, &mut last), "a step of the fade");
        assert_eq!((cache.color_key(node), cache.color_value(node)), (Some(color_key), Some(blue)));
        assert_eq!(cache.color_keys_of().collect::<Vec<_>>(), vec![(node, color_key)]);
        assert_eq!(cache.remove_color(node), Some(color_key));
        assert!(moved_since(&cache, &mut last));
        cache.retain_colors(|_| false);
        assert!(!moved_since(&cache, &mut last));
        assert!(cache.set_sheen_light(Some((red, 0.5))));
        assert!(moved_since(&cache, &mut last));
        assert!(!cache.set_sheen_light(Some((red, 0.5))));
        assert!(!moved_since(&cache, &mut last));

        cache.clear();
        assert!(moved_since(&cache, &mut last), "a cleared cache is another set");
        assert_eq!(cache.sheen_light, None);
    }

    /// A2-gpu-13. Each channel is its own pair of maps, and they are the maps
    /// the renderers read (`for_each_bound_value`): six transforms and three
    /// opacities written through the methods, one node per channel, come out
    /// under their keys with their values.
    #[test]
    fn each_channel_is_its_own_pair_of_maps_and_the_renderers_read_them() {
        let mut cache = GpuValueCache::empty();
        let mut want_transforms = HashMap::new();
        for (i, channel) in TRANSFORM_CHANNELS.into_iter().enumerate() {
            let key = TransformKey::unique();
            let value = ComputedTransform3D::new_translation(i as f32 + 1.0, 0.0, 0.0);
            cache.set_transform(channel, NodeId::new(10 + i), key, value);
            want_transforms.insert(key.id, value);
        }
        let mut want_opacities = HashMap::new();
        for (i, channel) in OPACITY_CHANNELS.into_iter().enumerate() {
            let key = OpacityKey::unique();
            let value = 0.1 * (i as f32 + 1.0);
            cache.set_opacity(channel, NodeId::new(20 + i), key, value);
            want_opacities.insert(key.id, value);
        }
        for (i, orientation) in [ScrollbarOrientation::Vertical, ScrollbarOrientation::Horizontal]
            .into_iter()
            .enumerate()
        {
            let key = OpacityKey::unique();
            let value = 0.7 + 0.1 * i as f32;
            cache.set_scrollbar_opacity(orientation, DomId::ROOT_ID, NodeId::new(30), key, value);
            want_opacities.insert(key.id, value);
        }
        let (transforms, opacities) = bound_values(&cache);
        assert_eq!(transforms, want_transforms);
        assert_eq!(opacities, want_opacities);
    }

    /// A2-gpu-13 and A2-gpu-08. A CSS value written through a write method
    /// was not resolved from the cascade at a known size: the list builds its
    /// matrix from the cascade again instead of baking what was written.
    ///
    /// Mutation: `self.css_transform_sizes.remove(&node)` removed from
    /// `wrote_transform` makes `reference_frame_at` return the written value
    /// (a translation by (7, 7)) for the rotation the cascade holds.
    #[test]
    fn a_css_value_written_by_hand_is_not_taken_for_the_cascades() {
        let styled = labels(4);
        let size = (100.0_f32, 40.0_f32);
        let mut cache = GpuValueCache::empty();
        let _ = cache.synchronize_with_sizes(&styled, &|_| Some(size));
        let node = *cache.css_transform_keys.keys().next().expect("a transformed node");
        let from_cascade = cache.reference_frame_at(&styled, node, size).expect("a frame").1;
        let by_hand = ComputedTransform3D::new_translation(7.0, 7.0, 0.0);
        assert_ne!(from_cascade, by_hand);

        cache.set_transform_value(TransformChannel::Css, node, by_hand);
        assert_eq!(cache.reference_frame_of(node).map(|f| f.1), Some(by_hand), "the live value");
        assert_eq!(
            cache.reference_frame_at(&styled, node, size).map(|f| f.1),
            Some(from_cascade),
            "the list's value is the cascade's"
        );
    }

    /// Counts the maps `rekey_nodes` hands over.
    #[derive(Default)]
    struct CountMaps {
        node_maps: usize,
        dom_node_maps: usize,
    }

    impl RekeyNodes for CountMaps {
        fn node_map<V>(&mut self, _map: &mut NodeMap<V>) {
            self.node_maps += 1;
        }
        fn dom_node_map<V>(&mut self, _map: &mut DomNodeMap<V>) {
            self.dom_node_maps += 1;
        }
    }

    /// Moves node 1 to node 8 in every map.
    struct OneToEight;

    impl RekeyNodes for OneToEight {
        fn node_map<V>(&mut self, map: &mut NodeMap<V>) {
            if let Some(value) = map.remove(&NodeId::new(1)) {
                map.insert(NodeId::new(8), value);
            }
        }
        fn dom_node_map<V>(&mut self, map: &mut DomNodeMap<V>) {
            if let Some(value) = map.remove(&(DomId::ROOT_ID, NodeId::new(1))) {
                map.insert((DomId::ROOT_ID, NodeId::new(8)), value);
            }
        }
    }

    /// A2-gpu-13. A DOM rebuild renumbers the nodes, and every map keyed by
    /// a node has to follow: 6 transform channels x 2, the sizes of the CSS
    /// values, 3 opacity channels x 2, the colour keys and values = 21 maps
    /// by node, and the 4 scrollbar fade maps by (DOM, node). The layout
    /// crate's remap named 24 of the 25 one by one and left the colours out;
    /// the cache lists them now.
    ///
    /// Mutation: a `rekey.node_map(..)` line removed from `rekey_nodes` fails
    /// the count and "<map> follows its node".
    #[test]
    fn a_rebuild_rekeys_every_map_of_the_cache() {
        let mut cache = GpuValueCache::empty();
        let mut count = CountMaps::default();
        let before = cache.value_generation();
        cache.rekey_nodes(&mut count);
        assert_eq!((count.node_maps, count.dom_node_maps), (21, 4));
        assert_ne!(cache.value_generation(), before, "renumbered nodes are another set");

        let (old, new) = (NodeId::new(1), NodeId::new(8));
        let value = ComputedTransform3D::new_translation(5.0, 6.0, 0.0);
        let red = azul_css::props::basic::ColorU::new(255, 0, 0, 255);
        let dom = DomId::ROOT_ID;
        for channel in TRANSFORM_CHANNELS {
            let _ = cache.key_transform(channel, old, value);
        }
        cache.css_transform_sizes.insert(old, (100.0, 40.0));
        for channel in OPACITY_CHANNELS {
            let _ = cache.key_opacity(channel, old, 0.5);
        }
        let _ = cache.key_color(old, red);
        for orientation in [ScrollbarOrientation::Vertical, ScrollbarOrientation::Horizontal] {
            cache.set_scrollbar_opacity(orientation, dom, old, OpacityKey::unique(), 0.5);
        }

        cache.rekey_nodes(&mut OneToEight);

        for channel in TRANSFORM_CHANNELS {
            assert!(cache.transform_key(channel, new).is_some(), "{channel:?} keys follow their node");
            assert_eq!(cache.transform_value(channel, new), Some(value), "{channel:?} values follow");
            assert_eq!(cache.transform_key(channel, old), None);
            assert_eq!(cache.transform_value(channel, old), None);
        }
        assert_eq!(cache.css_transform_sizes.get(&new), Some(&(100.0, 40.0)), "the size follows");
        assert!(!cache.css_transform_sizes.contains_key(&old));
        for channel in OPACITY_CHANNELS {
            assert!(cache.opacity_key(channel, new).is_some(), "{channel:?} keys follow their node");
            assert_eq!(cache.opacity_value(channel, new), Some(0.5));
            assert_eq!(cache.opacity_key(channel, old), None);
        }
        assert!(cache.color_key(new).is_some(), "the colour key follows its node");
        assert_eq!(cache.color_value(new), Some(red));
        assert_eq!(cache.color_key(old), None);
        for orientation in [ScrollbarOrientation::Vertical, ScrollbarOrientation::Horizontal] {
            assert!(cache.scrollbar_opacity_key(orientation, dom, new).is_some());
            assert_eq!(cache.scrollbar_opacity(orientation, dom, new), Some(0.5));
            assert_eq!(cache.scrollbar_opacity_key(orientation, dom, old), None);
        }
    }
}

/// Audit A6-05: the stamps of the transform maps.
#[cfg(test)]
mod audit_a6_tests {
    use super::*;

    fn tx(x: f32) -> ComputedTransform3D {
        let mut t = ComputedTransform3D::IDENTITY;
        t.m[3][0] = x;
        t
    }

    /// Equal stamps, equal content: a write that changes nothing keeps the stamp, every write
    /// that changes the map takes a new one, and so does every `&mut` to the map inside.
    /// Mutation: `insert` never restamping fails the second comparison; `deref_mut` not
    /// restamping fails the `entry` step (a frame's matrix would change under the hit
    /// tester's kept bounds).
    #[test]
    fn a_transform_map_is_restamped_when_its_content_changes() {
        let (a, b) = (NodeId::new(1), NodeId::new(2));
        let mut map: StampedMap<ComputedTransform3D> = StampedMap::default();
        assert_eq!(map.stamp(), 0, "the empty map nobody wrote");

        assert_eq!(map.insert(a, tx(1.0)), None);
        let first = map.stamp();
        assert_ne!(first, 0);
        assert_eq!(map.insert(a, tx(1.0)), Some(tx(1.0)));
        assert_eq!(map.stamp(), first, "the same value again: the same content");
        assert_eq!(map.insert(a, tx(2.0)), Some(tx(1.0)));
        let second = map.stamp();
        assert_ne!(second, first, "another value");

        assert_eq!(map.remove(&b), None);
        assert_eq!(map.stamp(), second, "nothing was removed");
        map.retain(|node, _| *node == a);
        assert_eq!(map.stamp(), second, "nothing was dropped");

        // Reading does not restamp.
        assert_eq!(map.get(&a), Some(&tx(2.0)));
        assert_eq!(map.len(), 1);
        assert_eq!((&map).into_iter().count(), 1);
        assert_eq!(map.stamp(), second);

        // A clone has the content, and the stamp; they part at the first change.
        let mut copy = map.clone();
        assert_eq!(copy.stamp(), second);
        copy.entry(b).or_insert(tx(3.0));
        assert_ne!(copy.stamp(), second, "a `&mut` to the map inside: restamped");
        assert_eq!(map.stamp(), second, "the original was not written");

        assert_eq!(map.remove(&a), Some(tx(2.0)));
        let third = map.stamp();
        assert!(third != second && third != first && third != 0);
        assert!(map.is_empty());
    }

    /// The cache's six stamps change with the frame of a node, and with nothing else: an
    /// opacity or a colour written leaves them alone.
    /// Mutation: a map left out of `transform_stamps` (the animation keys, say) fails the
    /// step that removes only that map's entry.
    #[test]
    fn the_caches_stamps_follow_the_reference_frames() {
        let node = NodeId::new(4);
        let mut cache = GpuValueCache::default();
        assert_eq!(cache.transform_stamps(), [0; 6]);

        cache.opacity_keys.insert(node, OpacityKey::unique());
        cache.current_opacity_values.insert(node, 0.5);
        assert_eq!(cache.transform_stamps(), [0; 6], "an opacity is no frame");

        cache.anim_transform_keys.insert(node, TransformKey::unique());
        cache.anim_current_transform_values.insert(node, tx(5.0));
        let keyed = cache.transform_stamps();
        assert_eq!(cache.painted_transform_of(node), Some(tx(5.0)));
        assert_ne!(keyed, [0; 6]);

        cache.anim_current_transform_values.insert(node, tx(5.0));
        assert_eq!(cache.transform_stamps(), keyed, "the same matrix: the same frame");
        cache.anim_current_transform_values.insert(node, tx(6.0));
        let moved = cache.transform_stamps();
        assert_ne!(moved, keyed);

        // The key alone goes: the node has no frame any more, with its value still there.
        cache.anim_transform_keys.remove(&node);
        assert_eq!(cache.painted_transform_of(node), None);
        assert_ne!(cache.transform_stamps(), moved);
    }

    /// ONE clock (the merge of A3-core's write generation, A2-gpu's write methods and the
    /// stamps): the write generation is the newest stamp of the cache. A transform map
    /// written directly - no write method, no `note_value_write` - moves it; a write that
    /// changes nothing moves neither it nor the stamps; a noted write (an opacity) moves the
    /// generation and leaves the frames' stamps alone; a scrollbar thumb moves the generation
    /// and not the frames' stamps either (its maps are stamped, and are no frame's).
    /// Mutations: `value_generation()` answering the noted stamp only (the field) fails the
    /// direct write; `key_transform` through `entry()` again fails "the same value" (the key
    /// map restamps); a thumb map among `transform_stamps` fails the last step.
    #[test]
    fn the_write_generation_is_the_newest_stamp_of_the_cache() {
        let node = NodeId::new(9);
        let mut cache = GpuValueCache::default();
        assert_eq!(cache.value_generation(), 0);

        // A direct write, as the animation code of the layout crate does.
        cache.anim_transform_keys.insert(node, TransformKey::unique());
        cache.anim_current_transform_values.insert(node, tx(1.0));
        let direct = cache.value_generation();
        assert_ne!(direct, 0, "a transform map cannot be written without the generation moving");

        // The write methods, with what is there already: nothing moves.
        let frames = cache.transform_stamps();
        let key = cache.key_transform(TransformChannel::Animation, node, tx(1.0));
        assert_eq!(cache.transform_key(TransformChannel::Animation, node), Some(key));
        assert_eq!(cache.set_transform_value(TransformChannel::Animation, node, tx(1.0)), Some(tx(1.0)));
        cache.retain_transforms(TransformChannel::Animation, |_| true);
        assert_eq!(cache.value_generation(), direct, "the same key, the same value");
        assert_eq!(cache.transform_stamps(), frames);

        // Another value through a write method: both move.
        let _ = cache.set_transform_value(TransformChannel::Animation, node, tx(2.0));
        let written = cache.value_generation();
        assert_ne!(written, direct);
        assert_ne!(cache.transform_stamps(), frames);

        // A noted write and a scrollbar thumb: the generation, not the frames.
        let frames = cache.transform_stamps();
        cache.note_value_write();
        let noted = cache.value_generation();
        assert_ne!(noted, written);
        let _ = cache.key_transform(TransformChannel::ScrollbarVertical, node, tx(3.0));
        assert_ne!(cache.value_generation(), noted);
        assert_eq!(cache.transform_stamps(), frames, "a thumb is no reference frame of the node");

        // Emptied: another generation than any it had.
        let before = cache.value_generation();
        cache.clear();
        assert!(cache.value_generation() != before && cache.value_generation() != 0);
    }
}
