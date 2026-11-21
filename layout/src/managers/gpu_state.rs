//! Centralized GPU state management.
//!
//! This module provides management of GPU property keys
//! (opacity, transforms, etc.), fade-in/fade-out animations
//! for scrollbar opacity - as a single source of truth for
//! the GPU cache.

use alloc::collections::BTreeMap;
#[cfg(not(feature = "std"))]
use alloc::collections::BTreeMap as HashMap;
#[cfg(feature = "std")]
use std::collections::HashMap;

use azul_core::{
    dom::{DomId, NodeId, ScrollbarOrientation},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    gpu::{GpuEventChanges, GpuTransformKeyEvent, GpuValueCache, TransformChannel},
    resources::TransformKey,
    task::{Duration, SystemTimeDiff},
    transform::ComputedTransform3D,
};

use crate::{
    managers::scroll_state::ScrollManager,
    solver3::{
        layout_tree::{LayoutNodeId, LayoutTree},
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
/// A tiny FNV-1a accumulator for "is this the same payload I sent last frame?".
///
/// Floats go in BITWISE (`to_bits`): a NaN opacity compares unequal to itself
/// and would reinstate the very redraw loop this exists to break.
#[derive(Debug, Copy, Clone)]
pub struct FrameDigest(u64);

impl Default for FrameDigest {
    fn default() -> Self {
        Self::new()
    }
}

impl FrameDigest {
    #[must_use]
    pub const fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }

    pub fn push_u64(&mut self, v: u64) {
        for b in v.to_le_bytes() {
            self.0 ^= u64::from(b);
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01B3);
        }
    }

    pub fn push_f32(&mut self, v: f32) {
        self.push_u64(u64::from(v.to_bits()));
    }

    #[must_use]
    pub const fn finish(&self) -> u64 {
        self.0
    }
}

/// Which of the renderer submissions [`GpuStateManager::submitted_digest_changed`]
/// is being asked about.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum SubmittedDigest {
    /// Image keys registered / updated on the transaction.
    Images,
    /// Scroll offsets pushed to `WebRender`'s scroll frames.
    ScrollOffsets,
}

#[derive(Debug, Clone)]
pub struct GpuStateManager {
    /// GPU value caches indexed by DOM ID
    pub caches: BTreeMap<DomId, GpuValueCache>,
    /// Delay before scrollbars start fading out after last activity
    pub fade_delay: Duration,
    /// Duration of the fade-out animation
    pub fade_duration: Duration,
    /// Whether any scrollbar is MID-FADE (0 < opacity < 1) and the platform loop
    /// has to keep generating frames for it. NOT set in the hold before the
    /// fade (opacity == 1.0): that phase is driven by the scroll timer, and
    /// counting it repainted forever. [`Self::scrollbar_fade_left_ms`] is the
    /// one that sees the hold. Set by `LayoutWindow::synchronize_scrollbar_opacity`,
    /// read by the platform render loop.
    pub scrollbar_fade_active: bool,
    /// Whether the last scan of the display lists for text halos left some of
    /// them undrawn (a frame draws a bounded number of new ones,
    /// `cpurender::within_halo_budget`): the lists were translated with a
    /// blurred shadow where a halo is still to come, so the platform loop has
    /// to keep generating frames - each of which scans again and draws the next
    /// ones - until none is left ([`Self::frames_owed`]). Set and cleared by
    /// the renderer host's scan, once per transaction.
    pub halos_owed: bool,
    /// How many scans in a row were made only because the last one left halos
    /// undrawn ([`Self::halos_owed`]); the chain ends at a limit the host sets, for
    /// the lists that name more halos than can be kept.
    pub halo_follow_ups: u32,
    /// How much longer each DOM's overlay scrollbars are still on screen, in
    /// whole milliseconds rounded up: the longest of its painted, unheld bars
    /// has `hold + fade - time since its box last scrolled` left. A DOM with no
    /// such bar is absent. It is what makes a window that has just been
    /// scrolled NOT settled while its bar waits to fade
    /// ([`Self::scrollbar_fade_left`]): `scrollbar_fade_active` is false for the
    /// whole hold, and `wait_settled` answered "settled" 50 ms after a wheel
    /// with the track still to fall some 750 ms later (E2E-M3). Set by
    /// `LayoutWindow::synchronize_scrollbar_opacity`; a bar with a delay of 0
    /// never fades and counts nothing.
    pub scrollbar_fade_left_ms: BTreeMap<DomId, u64>,
    /// The instant each DOM's overlay scrollbars run out
    /// ([`Self::scrollbar_fade_left_ms`] as of the last synchronization, as a
    /// deadline): what [`Self::scrollbar_fade_left`] counts down to by the
    /// clock. The milliseconds alone are a snapshot, and only a frame
    /// refreshes it: a window that has stopped drawing - an idle one, which
    /// is what a settle wait waits for - held its last answer for ever, so
    /// `wait_settled` never saw a scrolled list's bar fade.
    pub scrollbar_fade_ends: BTreeMap<DomId, azul_core::task::Instant>,
    /// Digest of the dynamic properties (scrollbar opacities + transforms)
    /// last handed to the renderer, so an IDENTICAL set can be recognised and
    /// skipped. See [`GpuStateManager::gpu_values_changed`].
    pub last_gpu_value_digest: Option<u64>,
    /// Digest of the image registrations last handed to the renderer.
    /// See [`GpuStateManager::submitted_digest_changed`].
    pub last_image_digest: Option<u64>,
    /// Digest of the scroll offsets last handed to the renderer.
    /// See [`GpuStateManager::submitted_digest_changed`].
    pub last_scroll_digest: Option<u64>,
    /// The display list of each DOM as `WebRender` last received it
    /// ([`Self::note_display_lists_submitted`]). A list changed since - an
    /// in-place patch of a colour or gradient fade, a content patch - is
    /// owed to the renderer even on a frame that rebuilt nothing
    /// ([`Self::display_lists_unsubmitted`]). Weak: the renderer's copy is
    /// its own, and a held `Arc` would make every in-place patch clone the
    /// list. A weak reference keeps the allocation, so a new list never
    /// reuses the address of the one it replaces.
    pub submitted_display_lists:
        BTreeMap<DomId, alloc::sync::Weak<crate::solver3::display_list::DisplayList>>,
    /// What the lists in [`Self::submitted_display_lists`] were translated
    /// against besides the lists themselves (the renderer's scale, the kept
    /// text-halo pictures): an opaque number the renderer's host computes
    /// ([`Self::note_display_lists_submitted`]). A list is HELD by the renderer
    /// only while this still equals what a translation would be made against
    /// ([`Self::display_list_is_held`]).
    pub submitted_translation_env: u64,
    /// What each DOM's display list paints of text halos, as the renderer
    /// host's last scan found it: per DOM a digest over every shadow painted as
    /// a kept picture (its image key, rect and colour); a DOM without one is
    /// absent. Written by the host's scan only.
    pub text_halo_digests: BTreeMap<DomId, u64>,
    /// The DOMs whose list the renderer holds translated against other
    /// pictures than the host's last scan found - the halos of its shadows
    /// ([`Self::text_halo_digests`] changed since the list was sent), a place
    /// kept for a node's picture to come: owed to the renderer like a list
    /// that was built again. A scan that only replaced pictures under the keys
    /// the lists name leaves it empty. Set by the host's scan, cleared by the
    /// host for each DOM whose list it sends.
    pub stale_display_lists: alloc::collections::BTreeSet<DomId>,
    /// GPU events produced during layout (CSS transform / opacity synchronization,
    /// scrollbar transform / opacity updates) that have not yet been pushed to
    /// the renderer. Drained by the platform render path when a transaction is
    /// built.
    pub pending_changes: GpuEventChanges,
    /// Per DOM: the boxes of its layout tree whose bound values follow a
    /// scroll offset, as [`Self::update_scrollbar_transforms`] last found
    /// them (`ScrollDrivenBoxes`). What a frame reads instead of every
    /// box ([`Self::refresh_scroll_driven_transforms`]).
    scroll_driven: BTreeMap<DomId, ScrollDrivenBoxes>,
    /// How many layout boxes the two functions above have looked at since
    /// this manager was made: every box of the tree per walk, the listed
    /// ones per frame. A counter for the guards (a frame of a window with
    /// two scroll boxes among a thousand adds 2).
    pub scroll_boxes_looked_at: u64,
}

/// The boxes of one DOM's layout tree whose bound values follow a scroll
/// offset: the ones with a scrollbar on an axis (their thumbs) and the
/// `position: sticky` ones. A layout tree stores a `ScrollbarRequirements`
/// on nearly every box, a bar on a handful, so finding them is a walk over
/// all boxes; the walk is made once per tree
/// ([`GpuStateManager::update_scrollbar_transforms`]) and its answer kept
/// here for the frames in between.
///
/// Only WHICH boxes is kept. Every value (sizes, extents, offsets, the
/// bars' kind) is read from the tree and the scroll manager again by each
/// frame, so a box that is listed and no longer scroll-driven costs a look
/// and nothing else.
#[derive(Debug, Clone, Default)]
struct ScrollDrivenBoxes {
    /// Layout index and DOM node of every box of every listed node (a node
    /// may have several: a list item and its marker), ascending by index -
    /// the order the walk visits them in.
    boxes: Vec<(usize, NodeId)>,
    /// The DOM nodes of `boxes`, ascending, each once.
    nodes: Vec<NodeId>,
    /// `LayoutTree::nodes.len()` of the tree the walk was made over.
    tree_len: usize,
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
    /// Have the dynamic GPU properties changed since they were last submitted?
    ///
    /// THE IDLE REDRAW LOOP THIS EXISTS TO BREAK. The lightweight transaction
    /// path — taken precisely because nothing needs rebuilding — pushed these
    /// values and then asked `WebRender` for a frame unconditionally.
    /// `WebRender`
    /// built one, signalled `new_frame_ready`, the backend woke and presented,
    /// and the present ran the lightweight path again. Measured idle on
    /// `X11`/GPU: 1087 submissions, 1087 frame-ready signals, 1086 presents, all
    /// carrying the same two floats and two transforms — ~59 fps of full
    /// render+present for a window nobody was touching.
    ///
    /// Floats are compared BITWISE (`to_bits`), not by value: a NaN opacity
    /// compares unequal to itself and would reinstate the loop it is meant to
    /// prevent.
    ///
    /// Returns `true` the first time (nothing to compare against) and whenever
    /// the set genuinely differs — a changed value, a new key, or a lost one.
    pub fn gpu_values_changed(
        &mut self,
        floats: &[(u64, f32)],
        transforms: &[(u64, [f32; 16])],
    ) -> bool {
        self.gpu_values_changed_with_colors(floats, transforms, &[])
    }

    /// [`Self::gpu_values_changed`] for a set that also carries the bound
    /// background colours (`key -> RGBA`): a colour fade frame is a change,
    /// a colour that stood still is none.
    pub fn gpu_values_changed_with_colors(
        &mut self,
        floats: &[(u64, f32)],
        transforms: &[(u64, [f32; 16])],
        colors: &[(u64, [u8; 4])],
    ) -> bool {
        // Sorted by key before digesting. The caller collects these out of
        // `HashMap`s, whose iteration order is not a promise; an order that
        // flapped while the values stayed the same would report a change on a
        // frame that changed nothing, and the loop would be back.
        let mut sorted_floats = floats.iter().collect::<Vec<_>>();
        sorted_floats.sort_unstable_by_key(|(k, _)| *k);
        let mut sorted_transforms = transforms.iter().collect::<Vec<_>>();
        sorted_transforms.sort_unstable_by_key(|(k, _)| *k);

        let mut h = FrameDigest::new();
        h.push_u64(floats.len() as u64);
        for (k, v) in sorted_floats {
            h.push_u64(*k);
            h.push_f32(*v);
        }
        h.push_u64(transforms.len() as u64);
        for (k, m) in sorted_transforms {
            h.push_u64(*k);
            for f in m {
                h.push_f32(*f);
            }
        }
        let mut sorted_colors = colors.iter().collect::<Vec<_>>();
        sorted_colors.sort_unstable_by_key(|(k, _)| *k);
        h.push_u64(colors.len() as u64);
        for (k, rgba) in sorted_colors {
            h.push_u64(*k);
            h.push_u64(u64::from(u32::from_be_bytes(*rgba)));
        }
        let digest = h.finish();

        if self.last_gpu_value_digest == Some(digest) {
            return false;
        }
        self.last_gpu_value_digest = Some(digest);
        true
    }

    /// Same idea as [`Self::gpu_values_changed`] for the other two things a
    /// "nothing was rebuilt" transaction carries: image registrations and
    /// scroll offsets. The caller digests its own payload (the types live in
    /// the renderer crate); this only owns the remembering.
    ///
    /// `slot` picks which of the two is meant.
    pub fn submitted_digest_changed(&mut self, slot: SubmittedDigest, digest: u64) -> bool {
        let stored = match slot {
            SubmittedDigest::Images => &mut self.last_image_digest,
            SubmittedDigest::ScrollOffsets => &mut self.last_scroll_digest,
        };
        if *stored == Some(digest) {
            return false;
        }
        *stored = Some(digest);
        true
    }

    /// Forget what was last submitted, so the next submission pushes
    /// everything again.
    ///
    /// The full-frame path MUST call this before it builds: a scene rebuild
    /// gives `WebRender` a new display list whose property bindings start from
    /// the values baked into the list, and "identical to what I sent last
    /// time" is then the wrong question — the values have to be re-pushed even
    /// though they did not change.
    pub const fn invalidate_submitted_digests(&mut self) {
        self.last_gpu_value_digest = None;
        self.last_image_digest = None;
        self.last_scroll_digest = None;
    }

    /// The longest any DOM's overlay scrollbar still has to be on screen, in
    /// milliseconds ([`Self::scrollbar_fade_left_ms`]); 0 when none is. Counted
    /// down by the clock to the deadline the last synchronization set
    /// ([`Self::scrollbar_fade_ends`]), so it reaches 0 while no frame is drawn.
    #[must_use]
    pub fn scrollbar_fade_left(&self) -> u64 {
        #[cfg(feature = "std")]
        let now = azul_core::task::Instant::now();
        self.scrollbar_fade_left_ms
            .iter()
            .map(|(dom, snapshot)| {
                // Only a deadline on the system clock can be counted down by it: a
                // window that runs on ticks (a driven clock, a test's) has no
                // other clock than its frames, and its snapshot is the answer.
                #[cfg(feature = "std")]
                if let Some(end) = self
                    .scrollbar_fade_ends
                    .get(dom)
                    .filter(|end| matches!(end, azul_core::task::Instant::System(_)))
                {
                    let left = end.duration_since(&now).as_nanos().div_ceil(1_000_000);
                    return u64::try_from(left).unwrap_or(u64::MAX).min(*snapshot);
                }
                let _ = dom;
                *snapshot
            })
            .max()
            .unwrap_or(0)
    }

    /// Whether the engine itself still owes frames, whatever the app does: a
    /// scrollbar that is fading (or waiting to) and text halos left for a
    /// later frame. The platform loop requests another frame while this is
    /// true.
    #[must_use]
    pub const fn frames_owed(&self) -> bool {
        self.scrollbar_fade_active || self.halos_owed
    }

    /// Creates a new GPU state manager with specified fade timing.
    #[must_use]
    pub fn new(fade_delay: Duration, fade_duration: Duration) -> Self {
        Self {
            caches: BTreeMap::new(),
            fade_delay,
            fade_duration,
            scrollbar_fade_active: false,
            halos_owed: false,
            halo_follow_ups: 0,
            scrollbar_fade_left_ms: BTreeMap::new(),
            scrollbar_fade_ends: BTreeMap::new(),
            last_gpu_value_digest: None,
            last_image_digest: None,
            last_scroll_digest: None,
            submitted_display_lists: BTreeMap::new(),
            submitted_translation_env: 0,
            text_halo_digests: BTreeMap::new(),
            stale_display_lists: alloc::collections::BTreeSet::new(),
            pending_changes: GpuEventChanges::empty(),
            scroll_driven: BTreeMap::new(),
            scroll_boxes_looked_at: 0,
        }
    }

    /// `WebRender` received these lists, one per DOM, translated against
    /// `env` (see [`Self::submitted_translation_env`]).
    pub fn note_display_lists_submitted(
        &mut self,
        layout_results: &BTreeMap<DomId, crate::window::DomLayoutResult>,
        env: u64,
    ) {
        self.submitted_translation_env = env;
        self.submitted_display_lists = layout_results
            .iter()
            .map(|(dom, r)| (*dom, alloc::sync::Arc::downgrade(&r.display_list)))
            .collect();
    }

    /// `WebRender` received this one list of `dom` (a `VirtualView` child sent
    /// alone), translated against the environment of the last full submission.
    pub fn note_one_display_list_submitted(
        &mut self,
        dom: DomId,
        list: &alloc::sync::Arc<crate::solver3::display_list::DisplayList>,
    ) {
        self.submitted_display_lists
            .insert(dom, alloc::sync::Arc::downgrade(list));
    }

    /// The list of `dom` moved from the allocation `before` to `after` by a
    /// patch that changes nothing of what the renderer's copy of it says: an
    /// image the renderer shows through a key of the node's own was replaced
    /// in place (`RendererResources::node_image_is_replaced_in_place`), so the
    /// copy names the same key in the same rect, and the new pixels reach the
    /// renderer as an update of the key. If the renderer held the list before
    /// the patch it holds it after; a list it did not hold stays owed.
    ///
    /// `before`: the list's address taken before the patch
    /// (`Arc::as_ptr`). Without this a tile that arrives in a map sent the
    /// map's whole list again and had the scene built again - for new pixels
    /// under a key the list already named (audit A4-09).
    pub fn display_list_moved(
        &mut self,
        dom: DomId,
        before: *const crate::solver3::display_list::DisplayList,
        after: &alloc::sync::Arc<crate::solver3::display_list::DisplayList>,
    ) {
        if let Some(held) = self.submitted_display_lists.get_mut(&dom) {
            if core::ptr::eq(held.as_ptr(), before) {
                *held = alloc::sync::Arc::downgrade(after);
            }
        }
    }

    /// Does `WebRender` already hold exactly this list of `dom`, translated
    /// against `env`? Then a transaction need not translate it again or send
    /// it: `WebRender` keeps every pipeline's list until a new one replaces it
    /// (`Scene::set_display_list`), which is what lets a `VirtualView`'s child
    /// be sent alone (`process_virtual_view_updates`).
    ///
    /// The identity is the list's allocation: a list rebuilt or patched in
    /// place is another one (the held `Weak` keeps the old allocation alive, so
    /// a new list never takes its address, and `Arc::make_mut` moves a patched
    /// list to a new one). A hover that rebuilt the host's list therefore sends
    /// the host's list and leaves the 5 000-item child of a map where it is.
    #[must_use]
    pub fn display_list_is_held(
        &self,
        dom: DomId,
        list: &alloc::sync::Arc<crate::solver3::display_list::DisplayList>,
        env: u64,
    ) -> bool {
        self.submitted_translation_env == env
            && self
                .submitted_display_lists
                .get(&dom)
                .is_some_and(|weak| core::ptr::eq(weak.as_ptr(), alloc::sync::Arc::as_ptr(list)))
    }

    /// Does any DOM have a display list `WebRender` has not received - one
    /// built or patched since the last submission, or a DOM that is new or
    /// gone? A frame that rebuilt nothing still owes the renderer such a
    /// list: a fade patched into the list in place reached the CPU renderer
    /// and never the GPU one, which showed the colour the fade started from.
    #[must_use]
    pub fn display_lists_unsubmitted(
        &self,
        layout_results: &BTreeMap<DomId, crate::window::DomLayoutResult>,
    ) -> bool {
        let mut count = 0usize;
        for (dom, result) in layout_results {
            count += 1;
            let list = &result.display_list;
            let submitted = self
                .submitted_display_lists
                .get(dom)
                .is_some_and(|weak| core::ptr::eq(weak.as_ptr(), alloc::sync::Arc::as_ptr(list)));
            if !submitted {
                return true;
            }
        }
        count != self.submitted_display_lists.len()
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
    #[must_use]
    pub fn get_cache(&self, dom_id: DomId) -> Option<&GpuValueCache> {
        self.caches.get(&dom_id)
    }

    /// The write generation of the bound values of EVERY DOM of the window:
    /// one number that is another one after any cache gained or lost a key,
    /// had a value written that differs from what it held
    /// (`GpuValueCache::value_generation`), or after a DOM's cache appeared
    /// or went. A frame that finds the number it last read has no bound
    /// value to read again: no copy of the values (`WindowGpuValues::of_doms`),
    /// no comparison with the previous ones, no walk for the compositor
    /// (`synchronize_gpu_values`).
    ///
    /// NOT yet safe to skip work by: until the cache's maps are private,
    /// every writer outside `GpuValueCache`'s own methods has to say so
    /// (`audit/A2-gpu.md`, "Integration", lists them). Only equality is
    /// meaningful.
    #[must_use]
    pub fn value_generation(&self) -> u64 {
        // The DOM ids go in with the generations: a cache that went and
        // another that came with the same count is another set.
        const STEP: u64 = 0x9E37_79B9_7F4A_7C15;
        self.caches.iter().fold(self.caches.len() as u64, |acc, (dom, cache)| {
            (acc ^ dom.inner as u64)
                .wrapping_mul(STEP)
                .wrapping_add(cache.value_generation())
                .rotate_left(17)
        })
    }

    pub fn get_or_create_cache(&mut self, dom_id: DomId) -> &mut GpuValueCache {
        self.caches.entry(dom_id).or_default()
    }

    /// The matrix `node` of `dom_id` is painted with now - the transform of
    /// the reference frame the display list opens for it, from the CSS or
    /// the animation channel (`GpuValueCache::reference_frame_of`). THE
    /// `resolve_transform` every hit test and "where is it on screen"
    /// question passes, so pointer targets follow the pixels.
    #[must_use]
    pub fn painted_transform_of(
        &self,
        dom_id: DomId,
        node: NodeId,
    ) -> Option<ComputedTransform3D> {
        self.caches
            .get(&dom_id)
            .and_then(|cache| cache.painted_transform_of(node))
    }

    /// Updates the scroll-driven transforms based on current scroll positions:
    /// the scrollbar thumbs, and the `position: sticky` boxes.
    ///
    /// Calculates the transform needed to position scrollbar thumbs correctly
    /// based on the scroll offset and content/container sizes, and how far a
    /// scroll since the layout pass moves each sticky box from where that
    /// pass placed it (`StickyConstraint::shift_at` now minus what the pass
    /// baked in), so a header holds while its list scrolls without a
    /// relayout. Returns the GPU event changes that need to be applied by
    /// the renderer.
    ///
    /// THE FORM FOR A TREE NOBODY HAS WALKED: it looks at every box of
    /// `layout_tree`, whatever tree that is, and leaves the list of the
    /// scroll-driven ones behind (`ScrollDrivenBoxes`). The layout pass
    /// calls it for the tree it made. A frame calls
    /// [`Self::refresh_scroll_driven_transforms`], which reads the list.
    pub fn update_scrollbar_transforms(
        &mut self,
        dom_id: DomId,
        scroll_manager: &ScrollManager,
        layout_tree: &LayoutTree,
    ) -> GpuEventChanges {
        let mut changes = GpuEventChanges::empty();
        // The list this walk leaves; the storage of the one it replaces is
        // used again, so a walk allocates only while a list grows.
        let mut listed = self.scroll_driven.remove(&dom_id).unwrap_or_default();
        listed.boxes.clear();
        listed.nodes.clear();
        listed.tree_len = layout_tree.nodes.len();

        let gpu_cache = self.caches.entry(dom_id).or_default();
        for node_idx in 0..layout_tree.nodes.len() {
            if let Some(node_id) = update_scroll_driven_box(
                gpu_cache,
                &mut changes,
                scroll_manager,
                dom_id,
                layout_tree,
                node_idx,
            ) {
                listed.boxes.push((node_idx, node_id));
                listed.nodes.push(node_id);
            }
        }
        self.scroll_boxes_looked_at = self
            .scroll_boxes_looked_at
            .wrapping_add(layout_tree.nodes.len() as u64);

        // Every box of a listed node, not only the one that is scroll-driven
        // now: a bar raised later on another box of the same node (its
        // marker, the second part of a split) is then a box a frame looks
        // at. A node the scroll manager holds a bar for and this list does
        // not name sends the frame back here (`scroll_driven_boxes_hold`).
        listed.nodes.sort_unstable();
        listed.nodes.dedup();
        for node_id in &listed.nodes {
            let Some(boxes) = layout_tree.dom_to_layout.get(node_id) else {
                continue;
            };
            for layout_id in boxes {
                let of_this_node = layout_tree
                    .nodes
                    .get(layout_id.index())
                    .is_some_and(|node| node.dom_node_id == Some(*node_id));
                if of_this_node {
                    listed.boxes.push((layout_id.index(), *node_id));
                }
            }
        }
        listed.boxes.sort_unstable();
        listed.boxes.dedup();

        self.scroll_driven.insert(dom_id, listed);
        changes
    }

    /// [`Self::update_scrollbar_transforms`] for a frame: the same values
    /// and the same events, from the boxes that function listed instead of
    /// every box of the tree. A window with a thousand boxes and two scroll
    /// boxes looks at 2 per frame, one without a scroll box or a sticky box
    /// at none ([`Self::scroll_boxes_looked_at`]).
    ///
    /// FOR THE TREE THE WALK WAS MADE OVER, written to since or not: the
    /// one in `LayoutWindow::layout_results`, between two layout passes.
    /// Whoever installs ANOTHER tree calls `update_scrollbar_transforms`
    /// for it (`LayoutWindow::layout_dom_recursive_impl` does).
    ///
    /// What is checked here, per call, and sends the call to the walk when
    /// it fails (`scroll_driven_boxes_hold`): the tree has the
    /// number of boxes the walk saw, each listed box still belongs to its
    /// node, and every node the scroll manager holds a bar for is listed.
    /// The last one is what sees a bar raised in place after the pass
    /// (`register_scroll_nodes` for a `VirtualView` or a text edit that
    /// began to overflow): whoever raises it registers the node with that
    /// bar. A sticky box is made by a layout pass only.
    ///
    /// `AZ_WALK_SCROLL_BOXES` in the environment makes every call the walk
    /// (`walk_every_frame`).
    pub fn refresh_scroll_driven_transforms(
        &mut self,
        dom_id: DomId,
        scroll_manager: &ScrollManager,
        layout_tree: &LayoutTree,
    ) -> GpuEventChanges {
        if walk_every_frame() || !self.scroll_driven_boxes_hold(dom_id, scroll_manager, layout_tree)
        {
            return self.update_scrollbar_transforms(dom_id, scroll_manager, layout_tree);
        }
        let mut changes = GpuEventChanges::empty();
        let Self {
            ref scroll_driven,
            ref mut caches,
            ref mut scroll_boxes_looked_at,
            ..
        } = *self;
        let Some(listed) = scroll_driven.get(&dom_id) else {
            return changes;
        };
        let gpu_cache = caches.entry(dom_id).or_default();
        for &(node_idx, _) in &listed.boxes {
            let _ = update_scroll_driven_box(
                gpu_cache,
                &mut changes,
                scroll_manager,
                dom_id,
                layout_tree,
                node_idx,
            );
        }
        *scroll_boxes_looked_at = scroll_boxes_looked_at.wrapping_add(listed.boxes.len() as u64);
        changes
    }

    /// Is the list of `dom_id`'s scroll-driven boxes still the list of
    /// `layout_tree`'s, as far as a frame can tell without the walk? See
    /// [`Self::refresh_scroll_driven_transforms`] for what that covers.
    fn scroll_driven_boxes_hold(
        &self,
        dom_id: DomId,
        scroll_manager: &ScrollManager,
        layout_tree: &LayoutTree,
    ) -> bool {
        let Some(listed) = self.scroll_driven.get(&dom_id) else {
            return false;
        };
        listed.tree_len == layout_tree.nodes.len()
            && listed.boxes.iter().all(|&(node_idx, node_id)| {
                layout_tree
                    .nodes
                    .get(node_idx)
                    .is_some_and(|node| node.dom_node_id == Some(node_id))
            })
            && scroll_manager
                .nodes_with_a_bar(dom_id)
                .all(|node_id| listed.nodes.binary_search(&node_id).is_ok())
    }
}

/// `AZ_WALK_SCROLL_BOXES` set (to anything): every frame walks every box of
/// every tree, as before the list of scroll-driven boxes existed. The switch
/// that tells a thumb or a sticky box that stays behind BECAUSE of the list
/// from one that stays behind for another reason. Read once per process.
#[cfg(feature = "std")]
fn walk_every_frame() -> bool {
    static SET: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *SET.get_or_init(|| std::env::var_os("AZ_WALK_SCROLL_BOXES").is_some())
}

#[cfg(not(feature = "std"))]
const fn walk_every_frame() -> bool {
    false
}

/// Refreshes what box `node_idx` of `layout_tree` binds to a scroll offset -
/// the transform of its sticky frame, the transforms of its scrollbar
/// thumbs - and returns its DOM node; `None`, and nothing done, for a box
/// that is neither sticky nor has a bar on an axis (nearly every box: the
/// test comes before anything is looked up or measured for it).
fn update_scroll_driven_box(
    gpu_cache: &mut GpuValueCache,
    changes: &mut GpuEventChanges,
    scroll_manager: &ScrollManager,
    dom_id: DomId,
    layout_tree: &LayoutTree,
    node_idx: usize,
) -> Option<NodeId> {
    let node = layout_tree.nodes.get(node_idx)?;
    let node_id = node.dom_node_id?;
    let warm = layout_tree.warm(LayoutNodeId::new(node_idx))?;
    let sticky = warm.sticky.as_deref();
    // The bars `paint_scrollbars` drew, and only those: the one
    // per-axis answer layout resolved from the axis's overflow and the
    // node's style (`ScrollbarRequirements::presence`) - thickness
    // and arrow buttons included. Deciding "overlay" here from the
    // RESERVED width, with a 16px fallback for a zero one, moved a
    // thumb for a `scrollbar-width: none` box that paints no bar, and
    // measured a bar whose style shows no buttons with them.
    //
    // The necessity flags are the layout pass's, amended after layout by
    // `cache::apply_virtual_scroll_necessity` (via `register_scroll_nodes`)
    // for a `VirtualView`, whose scrollable extent layout cannot see. This
    // path deliberately re-derives nothing: it reads the one stored answer,
    // so a bar that `paint_scrollbars` drew always gets its thumb moved.
    let bars = warm
        .scrollbar_info
        .as_ref()
        .map(|info| {
            (
                info.presence(ScrollbarOrientation::Vertical),
                info.presence(ScrollbarOrientation::Horizontal),
            )
        })
        .filter(|(v_bar, h_bar)| v_bar.is_present() || h_bar.is_present());
    if sticky.is_none() && bars.is_none() {
        return None;
    }
    if let Some(sticky) = sticky {
        update_sticky_transform(gpu_cache, changes, scroll_manager, dom_id, node_id, sticky);
    }
    let Some((v_bar, h_bar)) = bars else {
        return Some(node_id);
    };

    let scroll_offset = scroll_manager
        .get_current_offset(dom_id, node_id)
        .unwrap_or_default();

    // THE VIEWPORT'S BAR runs along the window and scrolls the root's
    // margin box (CSS Overflow 3 §3.3): the scrollport
    // `register_scroll_nodes` published and the extent it measured,
    // which is the bar `paint_scrollbars` paints and the pointer drags.
    // Measured against the root's own box - which IS its content -
    // there was nothing to scroll, so the thumb stayed parked at the
    // top while the page scrolled under it.
    let is_viewport = crate::solver3::scrollbar::is_viewport_scroller(dom_id, node_id);

    // Compute inner_rect (padding-box) by subtracting borders from used_size
    let border_box_size = node.used_size.unwrap_or_default();
    let nbp = node.box_props.unpack();
    let border = &nbp.border;
    let padding_box_size = LogicalSize {
        width: (border_box_size.width - border.left - border.right).max(0.0),
        height: (border_box_size.height - border.top - border.bottom).max(0.0),
    };
    let inner_size = if is_viewport {
        scroll_manager
            .get_scroll_state(dom_id, node_id)
            .map_or(padding_box_size, |s| s.container_rect.size)
    } else {
        padding_box_size
    };
    // Use zero origin since we only need the geometry ratios, not absolute position
    let inner_rect = LogicalRect {
        origin: LogicalPosition::new(0.0, 0.0),
        size: inner_size,
    };

    // The extent the scroll manager holds - what registration
    // published for this layout (`LayoutTree::scroll_extent` plus the
    // caret gutter, `caret_scroll_extent`), or a `VirtualView`'s
    // virtual size (`effective_content_size`): the number
    // `display_list::paint_scrollbars` sizes the thumb from and the
    // press router measures it with, so the thumb moves where it is
    // drawn and pressed. Re-derived from the bare layout extent, the
    // box hosting the caret had its thumb moved along a track 6px
    // shorter than the one it was pressed on. A box registration has
    // not described (no state, or one a bare `set_scroll_position`
    // made: no bar, no extent) falls back to this layout's extent.
    let content_size = scroll_manager
        .get_scroll_state(dom_id, node_id)
        .filter(|s| {
            s.virtual_scroll_size.is_some()
                || s.bar(ScrollbarOrientation::Vertical).is_present()
                || s.bar(ScrollbarOrientation::Horizontal).is_present()
        })
        .map_or_else(
            || layout_tree.scroll_extent(LayoutNodeId::new(node_idx), is_viewport),
            super::scroll_state::AnimatedScrollState::effective_content_size,
        );

    if v_bar.is_present() {
        let v_geom = compute_scrollbar_geometry_with_button_size(
            ScrollbarOrientation::Vertical,
            inner_rect,
            content_size,
            scroll_offset.y,
            v_bar.thickness(),
            h_bar.is_present(),
            v_bar.button_size(),
        );

        // Quantised to whole logical pixels: this value is the only
        // channel that can damage the bar, and a sub-pixel thumb move
        // repaints a gutter that did not visibly change. See
        // `quantize_thumb_offset` — `paint_scrollbars` bakes the
        // display list's initial value through the same rounding.
        let transform = ComputedTransform3D::new_translation(
            0.0,
            crate::solver3::scrollbar::quantize_thumb_offset(v_geom.thumb_offset),
            0.0,
        );
        update_scrollbar_transform_key(
            gpu_cache,
            changes,
            node_id,
            transform,
            ScrollbarOrientation::Vertical,
        );
    }

    if h_bar.is_present() {
        let h_geom = compute_scrollbar_geometry_with_button_size(
            ScrollbarOrientation::Horizontal,
            inner_rect,
            content_size,
            scroll_offset.x,
            h_bar.thickness(),
            v_bar.is_present(),
            h_bar.button_size(),
        );

        let transform = ComputedTransform3D::new_translation(
            crate::solver3::scrollbar::quantize_thumb_offset(h_geom.thumb_offset),
            0.0,
            0.0,
        );
        update_scrollbar_transform_key(
            gpu_cache,
            changes,
            node_id,
            transform,
            ScrollbarOrientation::Horizontal,
        );
    }

    Some(node_id)
}

/// Moves the sticky box `node_id` by what a scroll since its layout pass
/// changed: the translation from where the pass placed it to where it
/// sticks at its scroller's offset now. Only a box `synchronize` gave a key
/// is moved - the display list opened its reference frame with that key.
fn update_sticky_transform(
    gpu_cache: &mut GpuValueCache,
    changes: &mut GpuEventChanges,
    scroll_manager: &ScrollManager,
    dom_id: DomId,
    node_id: NodeId,
    sticky: &crate::solver3::positioning::StickyConstraint,
) {
    let Some(key) = gpu_cache.transform_key(TransformChannel::Sticky, node_id) else {
        return;
    };
    let offset = sticky
        .scroller
        .and_then(|scroller| scroll_manager.get_current_offset(dom_id, scroller))
        .unwrap_or_default();
    let now = sticky.shift_at(offset);
    let transform =
        ComputedTransform3D::new_translation(now.x - sticky.baked.x, now.y - sticky.baked.y, 0.0);
    match gpu_cache.set_transform_value(TransformChannel::Sticky, node_id, transform) {
        Some(old) if old == transform => {}
        Some(old) => changes
            .transform_key_changes
            .push(GpuTransformKeyEvent::Changed(node_id, key, old, transform)),
        None => changes
            .transform_key_changes
            .push(GpuTransformKeyEvent::Added(node_id, key, transform)),
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
    // Through the cache's write methods: a thumb that moved moves the write
    // generation, one that stayed where it was does not.
    let channel = match orientation {
        ScrollbarOrientation::Vertical => TransformChannel::ScrollbarVertical,
        ScrollbarOrientation::Horizontal => TransformChannel::ScrollbarHorizontal,
    };

    if let Some(existing_transform) = gpu_cache.transform_value(channel, node_id) {
        if existing_transform != transform {
            let Some(transform_key) = gpu_cache.transform_key(channel, node_id) else {
                return;
            };
            changes
                .transform_key_changes
                .push(GpuTransformKeyEvent::Changed(
                    node_id,
                    transform_key,
                    existing_transform,
                    transform,
                ));
            gpu_cache.set_transform_value(channel, node_id, transform);
        }
    } else {
        let transform_key = TransformKey::unique();
        gpu_cache.set_transform(channel, node_id, transform_key, transform);
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
        // The list of scroll-driven boxes names the nodes by the ids the
        // rebuild replaces: the layout pass that follows a rebuild walks
        // the new tree, and a frame before it walks rather than trust it.
        self.scroll_driven.remove(&dom);
        let Some(cache) = self.caches.get_mut(&dom) else {
            return;
        };

        // Every node-keyed map of the cache, as the cache lists them
        // (`GpuValueCache::rekey_nodes`): the scrollbar thumbs, the CSS
        // channel with the size each value was resolved against, the
        // ANIMATION channel (`LayoutWindow::tick_animations` writes a FLIP's
        // transform and opacity under the node's id every tick: left behind,
        // a value stayed under the OLD id and the unrelated node that
        // inherited that slot was painted through a stranger's transform),
        // the STICKY and SHEEN channels, the bound background colours and
        // the scrollbar fades. The maps used to be named here one by one,
        // and a map added to the cache was a map this forgot.
        cache.rekey_nodes(&mut NodeRemap { dom, map });
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

/// The node ids of one DOM's GPU value maps, rewritten after a rebuild:
/// what `GpuStateManager::remap_node_ids` hands `GpuValueCache::rekey_nodes`.
struct NodeRemap<'a> {
    dom: DomId,
    map: &'a crate::managers::NodeIdMap,
}

impl azul_core::gpu::RekeyNodes for NodeRemap<'_> {
    fn node_map<V>(&mut self, map: &mut azul_core::gpu::NodeMap<V>) {
        remap_hashmap(map, self.map);
    }

    fn dom_node_map<V>(&mut self, map: &mut azul_core::gpu::DomNodeMap<V>) {
        remap_dom_hashmap(map, self.dom, self.map);
    }
}

#[cfg(test)]
mod redundant_frame_tests {
    use super::GpuStateManager;

    /// The idle redraw loop, in one invariant.
    ///
    /// `build_image_only_transaction` — the LIGHTWEIGHT path a backend takes
    /// precisely because nothing needs rebuilding — pushed the scrollbar
    /// opacities and transforms and then called `txn.generate_frame()`
    /// unconditionally. WebRender built a frame, signalled `new_frame_ready`,
    /// the backend woke and presented, and presenting ran the lightweight path
    /// again. Measured on X11/GPU, 2026-09-05, with the window completely idle:
    ///
    ///     1087 synchronize_gpu_values  ->  1087 new_frame_ready  ->  1086 presents
    ///     every one reporting the SAME payload: floats=2 transforms=2 colors=0
    ///
    /// ~59 fps of full render+present for a static window. All four backends
    /// (X11, Wayland, macOS, Windows) call that path, so this is shared, not
    /// X11-specific; X11 merely spins fastest because its present is driven by
    /// the frame-ready wake rather than a frame callback or CVDisplayLink.
    ///
    /// The rule: pushing the SAME values again is not a reason to draw.
    #[test]
    fn resubmitting_identical_gpu_values_is_not_a_reason_to_draw() {
        let mut m = GpuStateManager::default();

        // First push of a value set: genuinely new, so it must be sent.
        assert!(
            m.gpu_values_changed(&[(1, 1.0), (2, 0.0)], &[]),
            "the first submission has nothing to compare against and must go"
        );

        // The idle case: byte-identical values, over and over. None of these
        // may request a frame.
        assert!(!m.gpu_values_changed(&[(1, 1.0), (2, 0.0)], &[]));
        assert!(!m.gpu_values_changed(&[(1, 1.0), (2, 0.0)], &[]));

        // A real change (a scrollbar fading) must still get through.
        assert!(m.gpu_values_changed(&[(1, 0.5), (2, 0.0)], &[]));
        assert!(!m.gpu_values_changed(&[(1, 0.5), (2, 0.0)], &[]));

        // So must a transform change on its own.
        assert!(m.gpu_values_changed(&[(1, 0.5), (2, 0.0)], &[(7, [1.0; 16])]));
        assert!(!m.gpu_values_changed(&[(1, 0.5), (2, 0.0)], &[(7, [1.0; 16])]));

        // And losing a property is a change too - the set shrank.
        assert!(m.gpu_values_changed(&[(1, 0.5)], &[(7, [1.0; 16])]));
    }

    /// NaN must not make every frame look different from itself. Opacity is an
    /// f32 and a NaN compares unequal to itself, so a bitwise digest is what
    /// keeps a degenerate value from reinstating the very loop this prevents.
    #[test]
    fn a_nan_value_still_compares_equal_to_itself() {
        let mut m = GpuStateManager::default();
        assert!(m.gpu_values_changed(&[(1, f32::NAN)], &[]));
        assert!(
            !m.gpu_values_changed(&[(1, f32::NAN)], &[]),
            "NaN == NaN bitwise, or a NaN opacity spins the redraw loop forever"
        );
    }

    /// The other two payloads a lightweight transaction carries - image
    /// registrations and scroll offsets - answer the same question, through the
    /// same remembering.
    #[test]
    fn image_and_scroll_digests_are_independent() {
        use super::SubmittedDigest;

        let mut m = GpuStateManager::default();

        assert!(m.submitted_digest_changed(SubmittedDigest::Images, 42));
        assert!(!m.submitted_digest_changed(SubmittedDigest::Images, 42));

        // A scroll offset that happens to digest to the same number is still a
        // first submission for ITS slot: the two must not share a memory.
        assert!(m.submitted_digest_changed(SubmittedDigest::ScrollOffsets, 42));
        assert!(!m.submitted_digest_changed(SubmittedDigest::ScrollOffsets, 42));

        assert!(m.submitted_digest_changed(SubmittedDigest::Images, 43));
        assert!(!m.submitted_digest_changed(SubmittedDigest::ScrollOffsets, 42));
    }

    /// A full frame rebuilds the scene. The values then have to be pushed again
    /// even though they did not change, so the full path invalidates first.
    #[test]
    fn a_scene_rebuild_forgets_what_was_submitted() {
        use super::SubmittedDigest;

        let mut m = GpuStateManager::default();
        assert!(m.gpu_values_changed(&[(1, 1.0)], &[]));
        assert!(m.submitted_digest_changed(SubmittedDigest::Images, 7));
        assert!(m.submitted_digest_changed(SubmittedDigest::ScrollOffsets, 9));

        assert!(!m.gpu_values_changed(&[(1, 1.0)], &[]));

        m.invalidate_submitted_digests();

        assert!(
            m.gpu_values_changed(&[(1, 1.0)], &[]),
            "after a scene rebuild the property bindings start over - unchanged values still have \
             to reach the renderer"
        );
        assert!(m.submitted_digest_changed(SubmittedDigest::Images, 7));
        assert!(m.submitted_digest_changed(SubmittedDigest::ScrollOffsets, 9));
    }

    /// The digest must be sensitive to WHICH key holds a value, not just to the
    /// multiset of values: swapping two scrollbars' opacities is a change.
    #[test]
    fn swapping_two_keys_values_is_a_change() {
        let mut m = GpuStateManager::default();
        assert!(m.gpu_values_changed(&[(1, 1.0), (2, 0.0)], &[]));
        assert!(m.gpu_values_changed(&[(1, 0.0), (2, 1.0)], &[]));
    }

    /// ...but the ORDER the caller happens to collect them in is not a change.
    /// The values come out of `HashMap`s (`GpuValueCache` is keyed by
    /// `(DomId, NodeId)`), whose iteration order is not a promise; if a
    /// reordering read as a change, an idle window would draw whenever the map
    /// felt like handing them over differently.
    #[test]
    fn the_order_the_values_arrive_in_is_not_a_change() {
        let mut m = GpuStateManager::default();
        assert!(m.gpu_values_changed(&[(1, 1.0), (2, 0.25)], &[(7, [1.0; 16]), (8, [2.0; 16])]));
        assert!(
            !m.gpu_values_changed(&[(2, 0.25), (1, 1.0)], &[(8, [2.0; 16]), (7, [1.0; 16])]),
            "the same set in a different order is the same set"
        );
    }
}

#[cfg(test)]
mod autotest_generated {
    use azul_core::{
        dom::{Dom, FormattingContext, IdOrClass, NodeType},
        resources::OpacityKey,
        styled_dom::StyledDom,
        task::{Instant, SystemTick, SystemTickDiff},
    };

    use super::*;
    use crate::{
        managers::{NodeIdMap, NodeIdRemap},
        solver3::{
            cache::apply_virtual_scroll_necessity,
            geometry::PackedBoxProps,
            layout_tree::{LayoutNodeCold, LayoutNodeHot, LayoutNodeWarm},
            scrollbar::{ScrollbarKind, ScrollbarPresence, ScrollbarRequirements},
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

    /// What layout computes for an `overflow-y: auto` VirtualView: nothing.
    /// The node is a replaced element with no flow content, so its laid-out
    /// content never exceeds its box and `check_scrollbar_necessity` finds no
    /// reason for a bar — only the style's bar (kind and `visual_width_px`)
    /// survives, waiting for an axis that needs it.
    fn auto_no_scrollbar() -> ScrollbarRequirements {
        ScrollbarRequirements {
            needs_horizontal: false,
            needs_vertical: false,
            bar_kind: ScrollbarKind::Classic,
            scrollbar_width: 0.0,
            scrollbar_height: 0.0,
            visual_width_px: 16.0,
        }
    }

    /// `body(0) > VirtualView(1)` with `overflow-x: hidden; overflow-y: auto`.
    /// NodeId 1 is the node every fixture in this module registers.
    fn auto_virtual_view_dom() -> StyledDom {
        let mut dom = Dom::create_body().with_child(
            Dom::create_node(NodeType::VirtualView)
                .with_ids_and_classes(vec![IdOrClass::Class("vv".into())].into()),
        );
        let (css, _warnings) =
            azul_css::parser2::new_from_str(".vv { overflow-x: hidden; overflow-y: auto; }");
        StyledDom::create(&mut dom, css)
    }

    /// A classic (space-reserving) vertical scrollbar with an explicit visual
    /// width — the un-ambiguous case where no field-fallback logic kicks in.
    fn v_scrollbar() -> ScrollbarRequirements {
        ScrollbarRequirements {
            needs_horizontal: false,
            needs_vertical: true,
            bar_kind: ScrollbarKind::Classic,
            scrollbar_width: 16.0,
            scrollbar_height: 16.0,
            visual_width_px: 16.0,
        }
    }

    fn h_scrollbar() -> ScrollbarRequirements {
        ScrollbarRequirements {
            needs_horizontal: true,
            needs_vertical: false,
            bar_kind: ScrollbarKind::Classic,
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
        let tick = Duration::Tick(SystemTickDiff {
            tick_diff: u64::MAX,
        });
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

        m.get_or_create_cache(dom(0))
            .transform_keys
            .insert(node, key);
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
            m.get_cache(dom(0))
                .unwrap()
                .current_opacity_values
                .get(&node),
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

        let key = cache
            .transform_keys
            .get(&node)
            .copied()
            .expect("key stored");
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

        assert!(
            changes.is_empty(),
            "no event is emitted for a keyless value"
        );
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
            c.scrollbar_v_opacity_keys
                .insert((d, old), OpacityKey::unique());
            c.scrollbar_h_opacity_keys
                .insert((d, old), OpacityKey::unique());
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
        assert_eq!(
            c.css_current_transform_values.get(&new),
            Some(&tx(3.0, 3.0))
        );
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

    /// The ANIMATION channel is a per-node cache like the twelve maps above:
    /// a FLIP in flight writes its node's transform and opacity under the
    /// node's id on every tick (`LayoutWindow::tick_animations`), and a
    /// rebuild renumbers the arena. Left where it was, the value stayed
    /// under the OLD id, and the unrelated node that inherited that slot
    /// was painted through a reference frame with a stranger's transform:
    /// a document mounted over AzWidgets at 760x400 had its first `<p>`
    /// (node 4) painted off-screen - "the first text line is missing".
    #[test]
    fn remap_node_ids_moves_the_animation_channel_with_its_node_and_drops_the_unmounted() {
        let mut m = GpuStateManager::default();
        let (moved, gone, new) = (NodeId::new(1), NodeId::new(4), NodeId::new(8));
        let d = dom(0);
        {
            let c = m.get_or_create_cache(d);
            c.anim_transform_keys.insert(moved, TransformKey::unique());
            c.anim_current_transform_values.insert(moved, tx(0.0, -50.0));
            c.anim_opacity_keys.insert(moved, OpacityKey::unique());
            c.anim_current_opacity_values.insert(moved, 0.5);
            c.anim_transform_keys.insert(gone, TransformKey::unique());
            c.anim_current_transform_values.insert(gone, tx(0.0, -300.0));
            c.anim_opacity_keys.insert(gone, OpacityKey::unique());
            c.anim_current_opacity_values.insert(gone, 1.0);
        }

        m.remap_node_ids(d, &NodeIdMap::from_pairs([(moved, new)]));

        let c = m.get_cache(d).unwrap();
        assert!(
            c.anim_transform_keys.contains_key(&new),
            "the animation's transform key follows its node"
        );
        assert_eq!(
            c.anim_current_transform_values.get(&new),
            Some(&tx(0.0, -50.0))
        );
        assert!(c.anim_opacity_keys.contains_key(&new));
        assert_eq!(c.anim_current_opacity_values.get(&new), Some(&0.5));
        for old in [moved, gone] {
            assert!(
                !c.anim_transform_keys.contains_key(&old),
                "no animation transform stays under {old:?}"
            );
            assert!(!c.anim_current_transform_values.contains_key(&old));
            assert!(!c.anim_opacity_keys.contains_key(&old));
            assert!(!c.anim_current_opacity_values.contains_key(&old));
        }
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
            vec![warm_node(
                Some(v_scrollbar()),
                Some(LogicalSize::new(100.0, 1000.0)),
            )],
        );

        let changes = m.update_scrollbar_transforms(dom(0), &sm, &t);
        assert_eq!(
            changes.transform_key_changes.len(),
            1,
            "only node 0 is seen"
        );
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
    fn a_negative_overscroll_offset_parks_the_thumb_at_the_top_of_the_track() {
        // Rubber-banding *above* the top is the only source of a negative offset.
        // The ratio clamp pins the thumb at 0 — it must not mirror the pull into
        // the 36.0 that the equivalent positive offset produces.
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
        assert!(
            y.abs() < 0.01,
            "an overscroll above the top must leave the thumb at the start, got {y}"
        );
    }

    #[test]
    fn a_virtual_view_thumb_is_sized_from_the_virtual_scroll_size_like_paint_scrollbars() {
        // A VirtualView is a replaced element with no flow content, so its laid-out
        // content size IS the viewport and `get_content_size` alone reports "nothing
        // to scroll" (full-length thumb parked at 0). `paint_scrollbars` seeds the
        // transform key from `ScrollPosition::children_rect.size`, which carries the
        // callback's virtual size; this path overwrites that key on every live
        // scroll, so both must land on the same thumb offset.
        const NODE: NodeId = NodeId::new(1);
        let mut m = GpuStateManager::default();
        let mut sm = ScrollManager::new();
        sm.register_or_update_scroll_node(
            dom(0),
            NODE,
            LogicalRect::new(
                LogicalPosition::new(250.0, 120.0),
                LogicalSize::new(100.0, 100.0),
            ),
            LogicalSize::new(100.0, 100.0),
            t0(),
            ScrollbarPresence::None,
            ScrollbarPresence::Classic { thickness: 16.0 },
        );
        sm.update_virtual_scroll_bounds(dom(0), NODE, LogicalSize::new(100.0, 1000.0), None);
        sm.set_scroll_position(dom(0), NODE, LogicalPosition::new(0.0, 450.0), t0());

        // The layout tree only ever saw the viewport-sized content box.
        let t = one_node_tree(v_scrollbar(), LogicalSize::new(100.0, 100.0));
        let y = sole_added_y(&m.update_scrollbar_transforms(dom(0), &sm, &t));

        // ...what display_list::paint_scrollbars computes for the same node.
        let states = sm.get_scroll_states_for_dom(dom(0));
        let pos = states
            .get(&NODE)
            .expect("the node must report a scroll state");
        assert_eq!(pos.children_rect.size, LogicalSize::new(100.0, 1000.0));
        let painted = compute_scrollbar_geometry_with_button_size(
            ScrollbarOrientation::Vertical,
            LogicalRect::new(LogicalPosition::zero(), LogicalSize::new(100.0, 100.0)),
            pos.children_rect.size,
            pos.children_rect.origin.y,
            16.0,
            false,
            16.0,
        );
        // Both producers of the thumb TRANSFORM quantise (see
        // `quantize_thumb_offset`), so the agreement is asserted against the
        // painter's quantised value — comparing against the raw geometry would
        // make this pass only for fixtures whose offset happens to be integral.
        let painted_transform =
            crate::solver3::scrollbar::quantize_thumb_offset(painted.thumb_offset);
        assert!(
            (y - painted_transform).abs() < 0.01,
            "GPU path put the thumb at {y}, the painter at {painted_transform}"
        );
        // usable = 100 - 2*16 = 68, thumb = max(68 * 100/1000, 32) = 32,
        // max_scroll = 1000 - 100 = 900 -> half scroll = (68 - 32) * 0.5 = 18.
        // Sizing from the laid-out 100x100 instead gives max_scroll 0 and a
        // full-track thumb parked at 0.
        assert!(
            (y - 18.0).abs() < 0.01,
            "expected the half-track thumb at 18.0, got {y}"
        );
    }

    #[test]
    fn an_auto_virtual_view_agrees_on_the_flag_the_painted_thumb_and_the_gpu_thumb() {
        // The half-landed state this pins: `paint_scrollbars` raised its own
        // booleans locally and drew a bar, while this path kept gating on the
        // layout-computed `warm.scrollbar_info.needs_*` — all-false for an
        // `auto` VirtualView — so the thumb froze at the display list's seed
        // value on every live scroll. One function now decides, and
        // `register_scroll_nodes` stores the decision where this path reads it.
        const NODE: NodeId = NodeId::new(1);
        let styled_dom = auto_virtual_view_dom();

        // 1. Layout ran: the VirtualView's box is 100x100 and its laid-out content is the same
        //    100x100, so no bar is warranted yet.
        let mut t = one_node_tree(auto_no_scrollbar(), LogicalSize::new(100.0, 100.0));

        // 2. The VirtualView callback published a 100x1000 document.
        let mut sm = ScrollManager::new();
        sm.update_virtual_scroll_bounds(dom(0), NODE, LogicalSize::new(100.0, 1000.0), None);

        // 3. What `register_scroll_nodes` then does: amend the flags from the virtual size and
        //    store the answer back on the node, which is what makes the node registrable at all
        //    (the old gate skipped it, so its container_rect stayed zero and its clamp bound was
        //    the whole virtual size).
        let states = sm.get_scroll_states_for_dom(dom(0));
        let pos = states
            .get(&NODE)
            .expect("the callback created a scroll state");
        let mut info = t
            .warm(LayoutNodeId::new(0))
            .and_then(|w| w.scrollbar_info)
            .expect("layout stored one");
        assert!(
            apply_virtual_scroll_necessity(
                &styled_dom,
                NODE,
                pos.children_rect.size,
                LogicalSize::new(100.0, 100.0),
                &mut info,
            ),
            "a 1000px document in a 100px auto viewport needs a bar"
        );
        assert!(
            info.needs_vertical,
            "the amended flag is the one the GPU path gates on"
        );
        assert!(
            !info.needs_horizontal,
            "overflow-x: hidden must not gain a bar"
        );
        assert_eq!(
            (info.scrollbar_width, info.scrollbar_height),
            (0.0, 0.0),
            "no layout gutter is reserved after the fact — the bar is drawn over the content"
        );
        assert_eq!(
            info.presence(ScrollbarOrientation::Vertical),
            ScrollbarPresence::Classic { thickness: 16.0 },
            "the amended axis has the bar the node's style draws"
        );
        t.warm_mut(LayoutNodeId::new(0))
            .expect("the fixture has a warm node")
            .scrollbar_info = Some(info);
        sm.register_or_update_scroll_node(
            dom(0),
            NODE,
            LogicalRect::new(
                LogicalPosition::new(250.0, 120.0),
                LogicalSize::new(100.0, 100.0),
            ),
            LogicalSize::new(100.0, 100.0),
            t0(),
            info.presence(ScrollbarOrientation::Horizontal),
            info.presence(ScrollbarOrientation::Vertical),
        );

        // 4. Half a document down.
        sm.set_scroll_position(dom(0), NODE, LogicalPosition::new(0.0, 450.0), t0());

        let mut m = GpuStateManager::default();
        let y = sole_added_y(&m.update_scrollbar_transforms(dom(0), &sm, &t));

        // ...and what `paint_scrollbars` computes for the same node from the
        // same ScrollPosition and the same presence. The style's bar is a
        // CLASSIC one, and the painter always drew it with its arrow buttons:
        // this fixture used to measure it as a button-less overlay because no
        // gutter had been reserved, pinning the GPU thumb (and the hit test)
        // one button's worth of travel away from the painted one.
        let bar = info.presence(ScrollbarOrientation::Vertical);
        let states = sm.get_scroll_states_for_dom(dom(0));
        let pos = states.get(&NODE).expect("the node is registered now");
        let painted = compute_scrollbar_geometry_with_button_size(
            ScrollbarOrientation::Vertical,
            LogicalRect::new(LogicalPosition::zero(), LogicalSize::new(100.0, 100.0)),
            pos.children_rect.size,
            pos.children_rect.origin.y,
            bar.thickness(),
            info.presence(ScrollbarOrientation::Horizontal).is_present(),
            bar.button_size(),
        );
        // Both producers of the thumb TRANSFORM quantise (see
        // `quantize_thumb_offset`), so the agreement is asserted against the
        // painter's quantised value — comparing against the raw geometry would
        // make this pass only for fixtures whose offset happens to be integral.
        let painted_transform =
            crate::solver3::scrollbar::quantize_thumb_offset(painted.thumb_offset);
        assert!(
            (y - painted_transform).abs() < 0.01,
            "GPU path put the thumb at {y}, the painter at {painted_transform}"
        );
        // usable track = 100 - 2*16 = 68, thumb = max(68 * 100/1000, 2*16) = 32,
        // max_scroll = 900 -> half travel = (68 - 32) * 0.5 = 18.
        assert!(
            (y - 18.0).abs() < 0.01,
            "expected the half-track thumb at 18.0, got {y}"
        );

        // The pre-fix tree — layout's flags, unamended — emits nothing at all,
        // which is exactly the frozen thumb under a painted bar.
        let stale = one_node_tree(auto_no_scrollbar(), LogicalSize::new(100.0, 100.0));
        let mut fresh = GpuStateManager::default();
        assert!(
            fresh
                .update_scrollbar_transforms(dom(0), &sm, &stale)
                .is_empty(),
            "without the amendment the GPU path never sees the bar"
        );
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
            bar_kind: ScrollbarKind::Classic,
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
            // `PackedBoxProps` grew margin/padding/margin_auto after this test
            // was written, which broke the whole azul-layout TEST build while
            // the lib kept compiling — so no layout test could run at all.
            // Zeroed: this test is about a border wider than the box.
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
    fn an_infinite_used_size_is_sanitised_instead_of_emitting_a_nan_thumb() {
        // An infinite border-box makes both the viewport and the content length
        // +inf, so compute_thumb_geometry ends at
        //   thumb_offset = (inf - inf) * 0.0 = NaN
        // This used to be fed straight into a translation matrix: combined with
        // `a_nan_transform_never_converges_...` above, one infinite used_size
        // meant the scrollbar re-emitted a Changed event on *every* frame,
        // forever, and WebRender was handed a NaN transform. The finite-guard in
        // `quantize_thumb_offset` (which the paint path applies identically) is
        // that future guard the old characterisation test was waiting for.
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
        assert!(
            y.is_finite(),
            "a NaN must never reach the transform, got {y}"
        );
        assert_eq!(y, 0.0);

        // And it SETTLES: a second identical pass emits nothing.
        let again = m.update_scrollbar_transforms(dom(0), &sm, &t);
        assert!(
            again.transform_key_changes.is_empty(),
            "the sanitised value must converge, not re-emit Changed forever"
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
    fn an_overlay_vertical_bar_runs_its_whole_track_without_buttons() {
        // An overlay bar, 16px thick, reserving nothing:
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
            bar_kind: ScrollbarKind::Overlay,
            scrollbar_width: 0.0,
            scrollbar_height: 0.0,
            visual_width_px: 16.0,
        };
        let t = one_node_tree(overlay, LogicalSize::new(100.0, 1000.0));

        let y = sole_added_y(&m.update_scrollbar_transforms(dom(0), &sm, &t));
        assert!(
            (y - 68.0).abs() < 0.01,
            "an overlay thumb travels the whole 100px track, got {y}"
        );
    }

    #[test]
    fn a_style_that_draws_no_bar_gets_no_thumb_to_move() {
        // `scrollbar-width: none`: the axis scrolls (`needs_vertical`) but no
        // bar is painted, so there is no thumb for a GPU value to move. This
        // used to fall back to a 16px overlay bar and emit a transform for it.
        let mut m = GpuStateManager::default();
        let mut sm = ScrollManager::new();
        sm.set_scroll_position_unclamped(
            dom(0),
            NodeId::new(1),
            LogicalPosition::new(0.0, 900.0),
            t0(),
        );
        let none = ScrollbarRequirements {
            needs_horizontal: true,
            needs_vertical: true,
            bar_kind: ScrollbarKind::None,
            scrollbar_width: 0.0,
            scrollbar_height: 0.0,
            visual_width_px: 0.0,
        };
        let t = one_node_tree(none, LogicalSize::new(1000.0, 1000.0));

        assert!(
            m.update_scrollbar_transforms(dom(0), &sm, &t).is_empty(),
            "no painted bar, no thumb transform"
        );
    }

    #[test]
    fn classic_vertical_scrollbar_is_measured_with_its_arrow_buttons() {
        // A classic vertical-only scrollbar -- 16px thick, reserving its
        // WIDTH and no height (`scrollbar_height` is the horizontal bar's
        // reservation) -- has 16px arrow buttons: the usable track is
        // 100 - 2 x 16 = 68, the thumb max(68 x 0.1, 32) = 32, and at the
        // bottom it travels to 68 - 32 = 36.0. Read as an overlay (which the
        // old reservation-based guess did for the wrong axis) it travelled to
        // 68.0 and overshot its own track by 32px. The buttons come from the
        // bar's kind now, not from any reservation.
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
            bar_kind: ScrollbarKind::Classic,
            scrollbar_width: 16.0,
            scrollbar_height: 0.0,
            visual_width_px: 16.0,
        };
        let t = one_node_tree(classic_vertical_only, LogicalSize::new(100.0, 1000.0));

        let y = sole_added_y(&m.update_scrollbar_transforms(dom(0), &sm, &t));
        assert!(
            (y - 36.0).abs() < 0.01,
            "a vertical-only classic bar must travel its usable track between the buttons \
             (36.0), got {y}"
        );
    }

    #[test]
    fn classic_horizontal_scrollbar_is_measured_with_its_arrow_buttons() {
        // The mirror image of the above: a horizontal-only classic bar reserves
        // its HEIGHT (`scrollbar_height`) and has arrow buttons, whatever the
        // vertical bar's `scrollbar_width` says.
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
            bar_kind: ScrollbarKind::Classic,
            scrollbar_width: 0.0,
            scrollbar_height: 16.0,
            visual_width_px: 16.0,
        };
        let t = one_node_tree(classic_horizontal_only, LogicalSize::new(1000.0, 100.0));

        let x = sole_added_x(&m.update_scrollbar_transforms(dom(0), &sm, &t));
        assert!(
            (x - 36.0).abs() < 0.01,
            "a horizontal-only classic bar must travel its usable track between the buttons \
             (36.0), got {x}"
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

/// A2-gpu (audit 2026-11-21): the writes of this file go through the cache's
/// write methods and move the write generation (`audit/A2-gpu.md`, A2-gpu-13).
#[cfg(test)]
mod audit_a2_gpu_tests {
    use azul_core::{
        dom::FormattingContext,
        gpu::OpacityChannel,
        resources::OpacityKey,
        task::{Instant, SystemTick},
    };
    use azul_css::props::basic::ColorU;

    use super::*;
    use crate::{
        managers::{NodeIdMap, NodeIdRemap},
        solver3::{
            geometry::PackedBoxProps,
            layout_tree::{LayoutNodeCold, LayoutNodeHot, LayoutNodeWarm},
            scrollbar::{ScrollbarKind, ScrollbarPresence, ScrollbarRequirements},
        },
    };

    fn tx(x: f32, y: f32) -> ComputedTransform3D {
        ComputedTransform3D::new_translation(x, y, 0.0)
    }

    /// A scroll moves a thumb by whole pixels, and most frames of a glide
    /// leave it where it is (`quantize_thumb_offset`): the write generation
    /// moves with the thumb, not with the frame.
    ///
    /// First write (0, 10): an `Added` event, the generation moves. The same
    /// offset again: no event, the generation stays. (0, 11): `Changed`, it
    /// moves. The horizontal thumb of the same node is a channel of its own.
    ///
    /// Mutation: `gpu_cache.set_transform_value(channel, node_id, transform)`
    /// replaced by a direct `current_transform_values.insert` fails "a thumb
    /// that moved".
    #[test]
    fn a_thumb_that_moves_moves_the_write_generation_and_one_that_stays_does_not() {
        let node = NodeId::new(3);
        let mut cache = GpuValueCache::default();
        let mut changes = GpuEventChanges::empty();
        let start = cache.value_generation();

        update_scrollbar_transform_key(
            &mut cache,
            &mut changes,
            node,
            tx(0.0, 10.0),
            ScrollbarOrientation::Vertical,
        );
        let added = cache.value_generation();
        assert_ne!(added, start, "a thumb was keyed");
        assert_eq!(changes.transform_key_changes.len(), 1);

        update_scrollbar_transform_key(
            &mut cache,
            &mut changes,
            node,
            tx(0.0, 10.0),
            ScrollbarOrientation::Vertical,
        );
        assert_eq!(cache.value_generation(), added, "a thumb that stayed");
        assert_eq!(changes.transform_key_changes.len(), 1);

        update_scrollbar_transform_key(
            &mut cache,
            &mut changes,
            node,
            tx(0.0, 11.0),
            ScrollbarOrientation::Vertical,
        );
        let moved = cache.value_generation();
        assert_ne!(moved, added, "a thumb that moved");
        assert_eq!(
            cache.transform_value(TransformChannel::ScrollbarVertical, node),
            Some(tx(0.0, 11.0))
        );

        update_scrollbar_transform_key(
            &mut cache,
            &mut changes,
            node,
            tx(4.0, 0.0),
            ScrollbarOrientation::Horizontal,
        );
        assert_ne!(cache.value_generation(), moved);
        assert_eq!(
            cache.transform_value(TransformChannel::ScrollbarHorizontal, node),
            Some(tx(4.0, 0.0))
        );
        assert_eq!(
            cache.transform_value(TransformChannel::ScrollbarVertical, node),
            Some(tx(0.0, 11.0)),
            "the vertical thumb is another channel"
        );
        assert_ne!(
            cache.transform_key(TransformChannel::ScrollbarHorizontal, node),
            cache.transform_key(TransformChannel::ScrollbarVertical, node)
        );
    }

    /// A rebuild that renumbers a DOM's nodes moves every value of its cache
    /// with its node: also the bound background colour and the animation
    /// channel, and the window's write generation moves. Node 1 becomes node
    /// 8; node 2 is unmounted.
    ///
    /// Mutation: `cache.rekey_nodes(&mut NodeRemap { dom, map })` removed
    /// from `remap_node_ids` fails every "follows its node".
    #[test]
    fn a_rebuild_moves_every_value_with_its_node_and_the_window_generation() {
        let (old, new, gone) = (NodeId::new(1), NodeId::new(8), NodeId::new(2));
        let dom = DomId { inner: 0 };
        let mut manager = GpuStateManager::default();
        let empty = manager.value_generation();
        let red = ColorU::new(255, 0, 0, 255);
        let (css_key, anim_key, color_key, fade_key);
        {
            let cache = manager.get_or_create_cache(dom);
            css_key = cache.key_transform(TransformChannel::Css, old, tx(1.0, 1.0));
            anim_key = cache.key_transform(TransformChannel::Animation, old, tx(2.0, 2.0));
            let _ = cache.key_transform(TransformChannel::Animation, gone, tx(9.0, 9.0));
            let _ = cache.key_opacity(OpacityChannel::Animation, old, 0.5);
            color_key = cache.key_color(old, red);
            fade_key = OpacityKey::unique();
            cache.set_scrollbar_opacity(ScrollbarOrientation::Vertical, dom, old, fade_key, 0.25);
        }
        let written = manager.value_generation();
        assert_ne!(written, empty, "a cache with values is another set than none");

        manager.remap_node_ids(dom, &NodeIdMap::from_pairs([(old, new)]));

        assert_ne!(manager.value_generation(), written, "renumbered nodes are another set");
        let cache = manager.get_cache(dom).expect("the DOM's cache");
        assert_eq!(cache.transform_key(TransformChannel::Css, new), Some(css_key));
        assert_eq!(cache.transform_value(TransformChannel::Css, new), Some(tx(1.0, 1.0)));
        assert_eq!(
            cache.transform_key(TransformChannel::Animation, new),
            Some(anim_key),
            "the animation channel follows its node"
        );
        assert_eq!(cache.opacity_value(OpacityChannel::Animation, new), Some(0.5));
        assert_eq!(cache.color_key(new), Some(color_key), "the colour key follows its node");
        assert_eq!(cache.color_value(new), Some(red));
        assert_eq!(
            cache.scrollbar_opacity_key(ScrollbarOrientation::Vertical, dom, new),
            Some(fade_key)
        );
        assert_eq!(cache.transform_key(TransformChannel::Css, old), None);
        assert_eq!(cache.color_key(old), None);
        assert_eq!(
            cache.transform_key(TransformChannel::Animation, gone),
            None,
            "an unmounted node's value is dropped"
        );
        // The frame of the renumbered node: its CSS matrix, then its slide.
        let (frame_key, product) = cache.reference_frame_of(new).expect("a frame");
        assert_eq!(frame_key, css_key);
        assert_eq!(product, tx(3.0, 3.0));

        // A second DOM's cache is part of the window's generation.
        let before = manager.value_generation();
        let _ = manager.get_or_create_cache(DomId { inner: 1 });
        assert_ne!(manager.value_generation(), before);
    }

    const DOM: DomId = DomId { inner: 0 };

    fn now() -> Instant {
        Instant::Tick(SystemTick::new(0))
    }

    /// What layout stores on a box: the requirements of its style, with a
    /// classic vertical bar 16 px thick when `bar`, with none otherwise
    /// (the case of nearly every box).
    fn requirements(bar: bool) -> ScrollbarRequirements {
        ScrollbarRequirements {
            needs_horizontal: false,
            needs_vertical: bar,
            bar_kind: ScrollbarKind::Classic,
            scrollbar_width: if bar { 16.0 } else { 0.0 },
            scrollbar_height: 0.0,
            visual_width_px: 16.0,
        }
    }

    /// A tree of `count` boxes of 100 x 100 without a border, box `i` the
    /// one box of DOM node `i`; the boxes named in `bars` scroll vertically.
    fn boxes(count: usize, bars: &[usize]) -> LayoutTree {
        let nodes = (0..count)
            .map(|i| LayoutNodeHot {
                box_props: PackedBoxProps::default(),
                dom_node_id: Some(NodeId::new(i)),
                used_size: Some(LogicalSize::new(100.0, 100.0)),
                formatting_context: FormattingContext::Block {
                    establishes_new_context: true,
                },
                parent: None,
            })
            .collect();
        let warm = (0..count)
            .map(|i| LayoutNodeWarm {
                scrollbar_info: Some(requirements(bars.contains(&i))),
                ..Default::default()
            })
            .collect();
        LayoutTree {
            nodes,
            warm,
            cold: vec![LayoutNodeCold::default(); count],
            root: 0,
            dom_to_layout: (0..count)
                .map(|i| (NodeId::new(i), vec![LayoutNodeId::new(i)]))
                .collect(),
            children_arena: Vec::new(),
            children_offsets: vec![(0, 0); count],
            subtree_needs_intrinsic: Vec::new(),
        }
    }

    /// Registers `node` as registration does for a box of 100 x 100 over
    /// 1000 px of content with a classic vertical bar: 900 px to scroll.
    fn register(scroll_manager: &mut ScrollManager, node: usize) {
        scroll_manager.register_or_update_scroll_node(
            DOM,
            NodeId::new(node),
            LogicalRect::new(LogicalPosition::new(0.0, 0.0), LogicalSize::new(100.0, 100.0)),
            LogicalSize::new(100.0, 1000.0),
            now(),
            ScrollbarPresence::None,
            ScrollbarPresence::Classic { thickness: 16.0 },
        );
    }

    fn thumb_y(manager: &GpuStateManager, node: usize) -> Option<f32> {
        manager
            .get_cache(DOM)
            .and_then(|cache| {
                cache.transform_value(TransformChannel::ScrollbarVertical, NodeId::new(node))
            })
            .map(|transform| transform.m[3][1])
    }

    /// A4-10(a) / A2-gpu-14, the count: a window of 1000 boxes, two of which
    /// scroll. The walk the layout pass makes looks at all 1000; a frame
    /// looks at the 2, whether something scrolled or not; a DOM without a
    /// scroll box or a sticky box costs a frame no look at all.
    ///
    /// The thumb, by hand: track 100 px, two arrow buttons of 16 px, so
    /// 100 - 2 * 16 = 68 px for the thumb to run in. Thumb length
    /// 68 * (100 / 1000) = 6.8, not shorter than twice the bar's thickness:
    /// 32. Scrolled by 450 of 900: (68 - 32) * 0.5 = 18.
    ///
    /// Mutations: `update_scrollbar_transforms` not keeping its list
    /// (`self.scroll_driven.insert` removed) makes every frame a walk
    /// (2000 instead of 1002); the early `return None` of
    /// `update_scroll_driven_box` removed changes no count and no value
    /// (it is the cost per box, not the number of boxes).
    #[test]
    fn a_frame_looks_at_the_scroll_boxes_and_the_walk_at_every_box() {
        let tree = boxes(1000, &[10, 500]);
        let mut scroll_manager = ScrollManager::new();
        register(&mut scroll_manager, 10);
        register(&mut scroll_manager, 500);
        let mut manager = GpuStateManager::default();

        let walked = manager.update_scrollbar_transforms(DOM, &scroll_manager, &tree);
        assert_eq!(manager.scroll_boxes_looked_at, 1000, "the walk: every box");
        assert_eq!(walked.transform_key_changes.len(), 2, "two thumbs were keyed");
        assert_eq!(thumb_y(&manager, 10), Some(0.0));
        assert_eq!(thumb_y(&manager, 500), Some(0.0));

        // A frame in which nothing scrolled.
        let idle = manager.refresh_scroll_driven_transforms(DOM, &scroll_manager, &tree);
        assert_eq!(manager.scroll_boxes_looked_at, 1002, "a frame: the two scroll boxes");
        assert!(idle.is_empty(), "nothing scrolled, nothing moved");

        // A frame after box 500 scrolled half its way.
        scroll_manager.set_scroll_position(
            DOM,
            NodeId::new(500),
            LogicalPosition::new(0.0, 450.0),
            now(),
        );
        let scrolled = manager.refresh_scroll_driven_transforms(DOM, &scroll_manager, &tree);
        assert_eq!(manager.scroll_boxes_looked_at, 1004);
        assert_eq!(scrolled.transform_key_changes.len(), 1, "one thumb moved");
        match scrolled.transform_key_changes[0] {
            GpuTransformKeyEvent::Changed(node, _, old, new) => {
                assert_eq!(node, NodeId::new(500));
                assert_eq!(old.m[3][1], 0.0);
                assert_eq!(new.m[3][1], 18.0, "(68 - 32) * 450 / 900");
            }
            ref other => panic!("expected Changed, got {other:?}"),
        }
        assert_eq!(thumb_y(&manager, 500), Some(18.0));
        assert_eq!(thumb_y(&manager, 10), Some(0.0), "the other box did not scroll");

        // The walk finds the same values for the same state: no event.
        let again = manager.update_scrollbar_transforms(DOM, &scroll_manager, &tree);
        assert!(again.is_empty(), "the frame left what the walk computes");
        assert_eq!(manager.scroll_boxes_looked_at, 2004);

        // A DOM none of whose boxes follows a scroll offset.
        let plain = boxes(1000, &[]);
        let other = DomId { inner: 1 };
        let mut manager = GpuStateManager::default();
        let _ = manager.update_scrollbar_transforms(other, &scroll_manager, &plain);
        assert_eq!(manager.scroll_boxes_looked_at, 1000);
        for _ in 0..3 {
            let frame = manager.refresh_scroll_driven_transforms(other, &scroll_manager, &plain);
            assert!(frame.is_empty());
        }
        assert_eq!(manager.scroll_boxes_looked_at, 1000, "three frames, no box looked at");
    }

    /// A bar that appears between two layout passes is seen by the next
    /// frame. `register_scroll_nodes` raises the flag of a box whose
    /// content began to overflow (a `VirtualView`'s extent, a text edit)
    /// in the tree, in place, and registers the node with that bar; the
    /// frame finds a node with a bar that its list does not name and walks.
    ///
    /// Box 700, scrolled to its end: (68 - 32) * 900 / 900 = 36.
    ///
    /// Mutation: the `nodes_with_a_bar` clause removed from
    /// `scroll_driven_boxes_hold` leaves the frame at 1 box looked at and
    /// the new thumb without a key (`thumb_y` is `None`).
    #[test]
    fn a_bar_raised_after_the_pass_sends_the_frame_back_to_the_walk() {
        let mut tree = boxes(1000, &[10]);
        let mut scroll_manager = ScrollManager::new();
        register(&mut scroll_manager, 10);
        let mut manager = GpuStateManager::default();
        let _ = manager.update_scrollbar_transforms(DOM, &scroll_manager, &tree);
        let _ = manager.refresh_scroll_driven_transforms(DOM, &scroll_manager, &tree);
        assert_eq!(manager.scroll_boxes_looked_at, 1001);
        assert_eq!(thumb_y(&manager, 700), None);

        tree.warm[700].scrollbar_info = Some(requirements(true));
        register(&mut scroll_manager, 700);
        scroll_manager.set_scroll_position(
            DOM,
            NodeId::new(700),
            LogicalPosition::new(0.0, 900.0),
            now(),
        );

        let raised = manager.refresh_scroll_driven_transforms(DOM, &scroll_manager, &tree);
        assert_eq!(manager.scroll_boxes_looked_at, 2001, "the frame walked the tree");
        assert_eq!(raised.transform_key_changes.len(), 1);
        assert_eq!(thumb_y(&manager, 700), Some(36.0), "(68 - 32) * 900 / 900");

        let _ = manager.refresh_scroll_driven_transforms(DOM, &scroll_manager, &tree);
        assert_eq!(manager.scroll_boxes_looked_at, 2003, "then the two scroll boxes again");
    }

    /// A frame that is handed a tree the walk has not seen walks it: a tree
    /// with another number of boxes, a tree whose listed box belongs to
    /// another node, the tree after a rebuild renamed the nodes, and a DOM
    /// nothing was ever walked for.
    ///
    /// Mutation: `listed.tree_len == layout_tree.nodes.len()` removed from
    /// `scroll_driven_boxes_hold` fails "another number of boxes" (1 box
    /// looked at instead of 101).
    #[test]
    fn a_frame_handed_another_tree_walks_it() {
        let mut scroll_manager = ScrollManager::new();
        register(&mut scroll_manager, 10);
        let mut manager = GpuStateManager::default();

        // Nothing was walked for this DOM yet.
        let tree = boxes(100, &[10]);
        let first = manager.refresh_scroll_driven_transforms(DOM, &scroll_manager, &tree);
        assert_eq!(manager.scroll_boxes_looked_at, 100, "no list: the walk");
        assert_eq!(first.transform_key_changes.len(), 1);
        let _ = manager.refresh_scroll_driven_transforms(DOM, &scroll_manager, &tree);
        assert_eq!(manager.scroll_boxes_looked_at, 101);

        // Another number of boxes.
        let longer = boxes(101, &[10]);
        let _ = manager.refresh_scroll_driven_transforms(DOM, &scroll_manager, &longer);
        assert_eq!(manager.scroll_boxes_looked_at, 202, "another number of boxes: the walk");
        let _ = manager.refresh_scroll_driven_transforms(DOM, &scroll_manager, &longer);
        assert_eq!(manager.scroll_boxes_looked_at, 203);

        // As many boxes, but box 10 is the box of another node now.
        let mut shifted = boxes(101, &[10]);
        shifted.nodes[10].dom_node_id = Some(NodeId::new(4000));
        let _ = manager.refresh_scroll_driven_transforms(DOM, &scroll_manager, &shifted);
        assert_eq!(manager.scroll_boxes_looked_at, 304, "a listed box of another node: the walk");

        // A rebuild renamed the nodes: the list is dropped with the old names.
        let _ = manager.update_scrollbar_transforms(DOM, &scroll_manager, &longer);
        let _ = manager.refresh_scroll_driven_transforms(DOM, &scroll_manager, &longer);
        assert_eq!(manager.scroll_boxes_looked_at, 406);
        manager.remap_node_ids(DOM, &NodeIdMap::from_pairs([(NodeId::new(10), NodeId::new(10))]));
        let _ = manager.refresh_scroll_driven_transforms(DOM, &scroll_manager, &longer);
        assert_eq!(manager.scroll_boxes_looked_at, 507, "after a rebuild: the walk");
    }
}
