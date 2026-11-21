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
use core::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
#[cfg(feature = "std")]
use std::collections::HashMap;

use azul_css::props::style::StyleTransformOrigin;

use crate::{
    dom::{DomId, NodeId},
    resources::{ColorKey, OpacityKey, TransformKey},
    styled_dom::StyledDom,
    transform::{ComputedTransform3D, RotationMode, INITIALIZED, USE_AVX, USE_SSE},
};

#[cfg(all(test, feature = "std"))]
std::thread_local! {
    /// Tests: how many CSS matrices [`GpuValueCache`] has built on this thread
    /// (`css_transform_of`): the cascade lookups and the matrix products of
    /// one node's `transform`.
    static CSS_MATRICES_BUILT: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
}

/// Caches GPU transform and opacity keys and their current values for all nodes.
///
/// This cache stores the `WebRender` keys and computed values for nodes with
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
    /// Separate from scrollbar transform keys to avoid `SpatialTreeItemKey` collisions.
    pub css_transform_keys: HashMap<NodeId, TransformKey>,
    /// Current CSS transform values (keyed by node ID)
    pub css_current_transform_values: HashMap<NodeId, ComputedTransform3D>,
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
    pub anim_transform_keys: HashMap<NodeId, TransformKey>,
    /// Current animation transform values (keyed by node ID).
    pub anim_current_transform_values: HashMap<NodeId, ComputedTransform3D>,
    /// STICKY transform keys (keyed by node ID): one per `position: sticky`
    /// box, minted by `synchronize`, so its display list wraps the box in a
    /// reference frame. A channel of its own for the reason the animation
    /// channel has one: `synchronize` owns the CSS map.
    pub sticky_transform_keys: HashMap<NodeId, TransformKey>,
    /// Current sticky translations (keyed by node ID): how far a scroll
    /// since the layout pass moved the box from where that pass placed it.
    /// The identity right after layout; written per frame by the layout
    /// crate's `GpuStateManager::update_scrollbar_transforms`.
    pub sticky_current_transform_values: HashMap<NodeId, ComputedTransform3D>,
    /// The cursor's SHEEN over a material (keyed by the material's node): the
    /// transform that moves the sheen's highlight to the cursor. A channel of
    /// its own, minted by the layout window for a window `WebRender` draws; the
    /// display list opens a sheen layer over a material exactly where its node
    /// has both sheen keys.
    pub sheen_transform_keys: HashMap<NodeId, TransformKey>,
    /// Where each sheen's highlight sits now: a translation to the cursor,
    /// logical px from the material's top-left.
    pub sheen_current_transform_values: HashMap<NodeId, ComputedTransform3D>,
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
    /// WRITE GENERATION of the bound values: a counter that moves whenever
    /// a value a renderer reads from this cache, or the set of keys that
    /// bind one, changes ([`Self::note_value_write`]). A frame that finds
    /// the generation it last read has nothing to read again - no copy of
    /// the values, no comparison with the previous ones, no synchronization
    /// with the compositor.
    ///
    /// The methods of this type that write values move it, and only when a
    /// value or a key really changed (a `synchronize` that finds every value
    /// as it was leaves it alone, however many events it reports). CODE THAT
    /// WRITES ONE OF THE MAPS ABOVE DIRECTLY MUST CALL
    /// [`Self::note_value_write`]: until every such writer does, a reader
    /// must not skip work by it. Only equality is compared; it wraps.
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

    /// The write generation of the bound values ([`Self::value_generation`]).
    #[must_use]
    pub const fn value_generation(&self) -> u64 {
        self.value_generation
    }

    /// Notes that a bound value or the set of bound keys changed: moves
    /// [`Self::value_generation`]. For every writer of this cache's maps
    /// that is not one of its own methods.
    pub const fn note_value_write(&mut self) {
        self.value_generation = self.value_generation.wrapping_add(1);
    }

    /// The reference frame the display list opens for `node`, as its key
    /// and the matrix it is painted with now - or `None` when it opens none.
    ///
    /// THE rule, shared by the display-list builder (which emits a
    /// `PushReferenceFrame` exactly when this is `Some`), the hit tester and
    /// every "where is it on screen" question: the CSS `transform` channel
    /// first, then the ANIMATION channel - an engine-driven transition (a
    /// FLIP slide, a keyframe transform) moves a node that has no CSS
    /// `transform` of its own - then the STICKY channel, a `position:
    /// sticky` box following its scroller. A key without a value opens
    /// nothing. A sticky box with a transform of its own keeps the
    /// transform and is placed where its layout pass stuck it.
    #[must_use]
    pub fn reference_frame_of(&self, node: NodeId) -> Option<(TransformKey, ComputedTransform3D)> {
        self.css_transform_keys
            .get(&node)
            .zip(self.css_current_transform_values.get(&node))
            .or_else(|| {
                self.anim_transform_keys
                    .get(&node)
                    .zip(self.anim_current_transform_values.get(&node))
            })
            .or_else(|| {
                self.sticky_transform_keys
                    .get(&node)
                    .zip(self.sticky_current_transform_values.get(&node))
            })
            .map(|(key, transform)| (*key, *transform))
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
        css.or_else(|| self.reference_frame_of(node))
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
        // The ANIMATION channel is separate from the CSS one because
        // `synchronize` owns `css_transform_keys` and evicts anything not
        // backed by a CSS `transform`; by key id the two are
        // indistinguishable, which is the intent.
        for (node_id, key) in &self.anim_transform_keys {
            if let Some(value) = self.anim_current_transform_values.get(node_id) {
                on_transform(key.id, value);
            }
        }
        for (node_id, key) in &self.css_transform_keys {
            if let Some(value) = self.css_current_transform_values.get(node_id) {
                on_transform(key.id, value);
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
        for (node_id, key) in &self.anim_opacity_keys {
            if let Some(&value) = self.anim_current_opacity_values.get(node_id) {
                on_opacity(key.id, value);
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
        self.sticky_transform_keys.retain(|node, _| sticky.contains(node));
        self.sticky_current_transform_values
            .retain(|node, _| sticky.contains(node));
        for node in sticky {
            let mut fresh = false;
            self.sticky_transform_keys.entry(node).or_insert_with(|| {
                fresh = true;
                TransformKey::unique()
            });
            if fresh {
                self.sticky_current_transform_values
                    .insert(node, ComputedTransform3D::IDENTITY);
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
}
