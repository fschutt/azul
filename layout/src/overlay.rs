//! The content overlay: the ONE home for quickly-mutable content state.
//!
//! The DOM (`StyledDom`) is immutable by design — `NodeId`s stay stable, and
//! every fast-changing piece of content (camera frames, canvas repaints,
//! swapped images, in-progress text edits) lives in an overlay that readers
//! consult FIRST, falling back to the DOM. Before this module, that overlay
//! was scattered: a side map only the CPU rasterizer knew about
//! (`cpu_image_callback_results`), an in-place `set_node_type` DOM mutation
//! only some paths performed, a css-id image cache mirrored between shell and
//! layout, and text in `dirty_text_nodes`. Every combination that missed one
//! of them was a shipped bug — per BACKEND, because each of the 8 event loops
//! assembled the pipeline by hand.
//!
//! The rules this module enforces:
//!
//! 1. **One write chokepoint**: [`crate::window::LayoutWindow::apply_content_change`]
//!    is the only way content state changes. It validates, writes the overlay
//!    arm, journals the change, and returns the dirty tier the frame loop must
//!    honor. Backends never see content — they receive a tier.
//! 2. **One read order**: overlay first, immutable DOM second, via
//!    [`ResolvedContent`]. Every consumer (display-list build, IFC build,
//!    raster, hit-test, a11y, exports) resolves through it.
//! 3. **One retention clock**: [`ContentJournal`] entries are retired by frame
//!    age (swapchain depth), never by document size or session length.
//!    Journal = what the RENDERER may still need; the `UndoRedoManager`
//!    (user intent) is fed separately by the same chokepoint.

use std::collections::{BTreeMap, VecDeque};

use azul_core::{
    dom::{DomId, NodeId, NodeType},
    resources::{ImageRef, ImageRefHash},
    styled_dom::StyledDom,
};
use azul_css::AzString;

use crate::managers::{NodeIdMap, NodeIdRemap};

/// How many PRESENTED frames of history the journal keeps. A backend
/// re-presenting a not-fully-redrawn buffer composed `k` frames ago may still
/// sample the previous image of a node via [`ContentJournal::image_as_of`];
/// `3` covers the deepest swapchain in the tree (triple buffering — wl_shm
/// double-buffer needs 2).
pub const JOURNAL_RETENTION_FRAMES: u64 = 3;

/// A content mutation, as accepted by the chokepoint.
///
/// Constructors on `LayoutWindow` (e.g. `apply_content_change`) decide
/// per-variant whether the change is user-undoable; per-frame producer writes
/// (camera/callback frames) never are.
#[derive(Debug, Clone)]
pub enum ContentChange {
    /// Swap the displayed image of a node (camera / video / screenshare /
    /// explicit `ChangeNodeImage`). Participates in intrinsic-size tier
    /// detection: a different-sized image relayouts, a same-sized one repaints.
    Image {
        dom_id: DomId,
        node_id: NodeId,
        image: ImageRef,
    },
    /// A `RenderImageCallback` produced a frame for a callback-image node.
    /// Always paint-tier: callback frames are PAINT content — the box is
    /// CSS-determined, and the callback's declared image stays the layout
    /// authority (otherwise a producer could resize the document per frame).
    ImageCallbackResult {
        dom_id: DomId,
        node_id: NodeId,
        image: ImageRef,
    },
    /// Register (`Some`) or remove (`None`) an image under a css id
    /// (`background-image: url("id")`). Takes effect on the NEXT display-list
    /// build — the chokepoint returns the rebuild tier instead of the old
    /// `DoNothing`.
    ImageById {
        id: AzString,
        image: Option<ImageRef>,
    },
}

/// What the frame loop must do after a content change — the ONLY thing
/// backends learn about content. Ordered weakest → strongest so results merge
/// with `max`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ContentDirtyTier {
    /// The change was a no-op (same image re-set, unknown node).
    Unchanged,
    /// Display-list items were patched in place; repaint. Damage discovery is
    /// the backend diff's job — `ImageRef` identity makes patched items
    /// unequal to the previous frame's.
    Paint,
    /// The display list must be rebuilt (css-id images resolve at build time).
    RebuildDisplayList,
    /// Intrinsic content size changed: relayout (which also rebuilds the DL).
    Relayout,
}

impl ContentDirtyTier {
    /// The ONE mapping from content dirty tier to the event-loop result every
    /// host consumes. Defined here — next to the tier — so a backend cannot
    /// invent its own interpretation:
    /// - `Paint`: the DL was already patched in place; a re-render picks it up
    ///   (CPU: the DL diff sees the ImageRef identity change and damages those
    ///   bounds; GPU: the translator re-reads the patched DL).
    /// - `RebuildDisplayList`: DL regeneration + re-render.
    /// - `Relayout`: incremental relayout (which rebuilds the DL).
    #[must_use]
    pub const fn to_process_event_result(self) -> azul_core::events::ProcessEventResult {
        use azul_core::events::ProcessEventResult;
        match self {
            Self::Unchanged => ProcessEventResult::DoNothing,
            Self::Paint => ProcessEventResult::ShouldReRenderCurrentWindow,
            Self::RebuildDisplayList => ProcessEventResult::ShouldUpdateDisplayListCurrentWindow,
            Self::Relayout => ProcessEventResult::ShouldIncrementalRelayout,
        }
    }
}

/// Result of one `apply_content_change`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContentChangeResult {
    pub tier: ContentDirtyTier,
}

/// The overlay proper. Fields are private on purpose: reads go through
/// [`ResolvedContent`] / the accessors below, writes only through the
/// chokepoint (`pub(crate)` mutators).
#[derive(Debug, Default)]
pub struct ContentOverlay {
    /// Node-image arm: the currently-displayed image for a node, overriding
    /// the immutable DOM's `NodeType::Image` content.
    images: BTreeMap<(DomId, NodeId), ImageRef>,
}

impl ContentOverlay {
    /// The overlay's image for a node, if any. Callers wanting the full
    /// overlay→DOM read order use [`ResolvedContent`] instead.
    #[must_use]
    pub fn image_for_node(&self, dom_id: DomId, node_id: NodeId) -> Option<&ImageRef> {
        self.images.get(&(dom_id, node_id))
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.images.is_empty()
    }

    pub(crate) fn set_image(
        &mut self,
        dom_id: DomId,
        node_id: NodeId,
        image: ImageRef,
    ) -> Option<ImageRef> {
        self.images.insert((dom_id, node_id), image)
    }

    /// Drop every overlay entry of `dom` (full DOM regeneration without a
    /// remap — the new generation's DOM is the authority again).
    pub(crate) fn clear_dom(&mut self, dom_id: DomId) {
        self.images.retain(|(d, _), _| *d != dom_id);
    }
}

impl NodeIdRemap for ContentOverlay {
    fn remap_node_ids(&mut self, dom: DomId, map: &NodeIdMap) {
        crate::managers::remap_dom_keys(&mut self.images, dom, map);
    }
}

/// The one overlay→DOM read order, borrowed by every consumer.
///
/// Constructed at the few pipeline entries that own both halves (display-list
/// build / IFC build via `LayoutContext`, exports); everything downstream
/// takes this instead of reaching into `StyledDom` for content.
#[derive(Clone, Copy)]
pub struct ResolvedContent<'a> {
    pub overlay: Option<&'a ContentOverlay>,
    pub styled_dom: &'a StyledDom,
    pub dom_id: DomId,
}

impl ResolvedContent<'_> {
    /// The image to PAINT for `node_id`: overlay first (produced callback
    /// frames, swapped images), then the DOM's `NodeType::Image`.
    #[must_use]
    pub fn image_for_paint(&self, node_id: NodeId) -> Option<ImageRef> {
        if let Some(overlay) = self.overlay {
            if let Some(img) = overlay.image_for_node(self.dom_id, node_id) {
                return Some(img.clone());
            }
        }
        self.dom_image(node_id)
    }

    /// The image whose intrinsic size LAYOUT uses for `node_id`. Overlay
    /// first — EXCEPT when the DOM declares a callback image: produced frames
    /// are paint content and must not resize the box per frame.
    #[must_use]
    pub fn image_for_layout(&self, node_id: NodeId) -> Option<ImageRef> {
        let dom_image = self.dom_image(node_id);
        if let Some(dom_ref) = &dom_image {
            if dom_ref.is_callback() {
                return dom_image;
            }
        }
        if let Some(overlay) = self.overlay {
            if let Some(img) = overlay.image_for_node(self.dom_id, node_id) {
                return Some(img.clone());
            }
        }
        dom_image
    }

    fn dom_image(&self, node_id: NodeId) -> Option<ImageRef> {
        let node_data = self.styled_dom.node_data.as_container();
        match node_data.get(node_id)?.get_node_type() {
            NodeType::Image(image_ref) => Some(image_ref.as_ref().clone()),
            _ => None,
        }
    }
}

/// One journaled content mutation.
#[derive(Debug, Clone)]
pub struct JournalEntry {
    /// The frame sequence number the change was applied in.
    pub frame_seq: u64,
    pub change: AppliedChange,
}

/// The mechanical record of an applied change — enough for a compositor to
/// reach content as of frame `N − k` and for damage to know old vs new.
#[derive(Debug, Clone)]
pub enum AppliedChange {
    Image {
        dom_id: DomId,
        node_id: NodeId,
        /// The image displayed BEFORE this change (holds the pixels alive for
        /// backends still compositing an old buffer). `None`: node had none.
        old: Option<ImageRef>,
        new_hash: ImageRefHash,
    },
    ImageById {
        id: AzString,
        old: Option<ImageRef>,
        removed: bool,
    },
}

/// Frame-scoped record of applied content changes.
///
/// Retention is bounded by the PRESENT loop: `begin_frame` (called once per
/// frame from shared frame code — never from a backend) retires entries older
/// than [`JOURNAL_RETENTION_FRAMES`]. The journal never grows with document
/// size or session length.
#[derive(Debug, Default)]
pub struct ContentJournal {
    frame_seq: u64,
    entries: VecDeque<JournalEntry>,
}

impl ContentJournal {
    /// The current frame sequence number. Bumped only by [`Self::begin_frame`].
    #[must_use]
    pub const fn frame_seq(&self) -> u64 {
        self.frame_seq
    }

    /// Advance the frame clock and retire entries older than the swapchain
    /// depth. Called from shared per-frame code (`LayoutWindow::prepare_frame_cpu`
    /// / the GPU frame orchestration) — backends never call this directly.
    pub fn begin_frame(&mut self) {
        self.frame_seq = self.frame_seq.wrapping_add(1);
        let cutoff = self.frame_seq.saturating_sub(JOURNAL_RETENTION_FRAMES);
        while self
            .entries
            .front()
            .is_some_and(|e| e.frame_seq < cutoff)
        {
            self.entries.pop_front();
        }
    }

    pub(crate) fn record(&mut self, change: AppliedChange) {
        self.entries.push_back(JournalEntry {
            frame_seq: self.frame_seq,
            change,
        });
    }

    /// The image `node` displayed as of `frame_seq` (≤ [`JOURNAL_RETENTION_FRAMES`]
    /// frames back): the `old` of the first change recorded AFTER that frame,
    /// or `None` if the node's image hasn't changed since (current is valid).
    #[must_use]
    pub fn image_as_of(
        &self,
        dom_id: DomId,
        node_id: NodeId,
        frame_seq: u64,
    ) -> Option<&ImageRef> {
        self.entries.iter().find_map(|e| match &e.change {
            AppliedChange::Image {
                dom_id: d,
                node_id: n,
                old,
                ..
            } if *d == dom_id && *n == node_id && e.frame_seq > frame_seq => old.as_ref(),
            _ => None,
        })
    }

    /// Number of retained entries (test/diagnostic use).
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Drop journal history for a DOM whose generation was swapped — the old
    /// generation's node ids no longer mean anything, and the swap itself
    /// repaints everything.
    pub(crate) fn clear_dom(&mut self, dom_id: DomId) {
        self.entries.retain(|e| match &e.change {
            AppliedChange::Image { dom_id: d, .. } => *d != dom_id,
            AppliedChange::ImageById { .. } => true,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn img(w: usize, h: usize) -> ImageRef {
        ImageRef::null_image(w, h, azul_core::resources::RawImageFormat::BGRA8, Vec::new())
    }

    fn dom0() -> DomId {
        DomId { inner: 0 }
    }

    #[test]
    fn journal_retires_by_frame_age_never_by_count() {
        let mut journal = ContentJournal::default();
        // 100 changes in ONE frame: all retained (retention is frames, not entries).
        for i in 0..100_usize {
            journal.record(AppliedChange::Image {
                dom_id: dom0(),
                node_id: NodeId::new(i),
                old: Some(img(1, 1)),
                new_hash: img(1, 1).get_hash(),
            });
        }
        assert_eq!(journal.len(), 100);

        // After JOURNAL_RETENTION_FRAMES + 1 empty frames, everything is retired.
        for _ in 0..=JOURNAL_RETENTION_FRAMES {
            journal.begin_frame();
        }
        assert!(journal.is_empty(), "entries older than the swapchain depth must retire");
    }

    #[test]
    fn image_as_of_returns_the_pre_change_image_within_retention() {
        let mut journal = ContentJournal::default();
        let node = NodeId::new(7);
        let old = img(10, 10);
        let old_hash = old.get_hash();

        journal.begin_frame(); // frame 1
        let composed_at = journal.frame_seq(); // a backend composited frame 1
        journal.begin_frame(); // frame 2
        journal.record(AppliedChange::Image {
            dom_id: dom0(),
            node_id: node,
            old: Some(old),
            new_hash: img(10, 10).get_hash(),
        });

        // The buffer composed at frame 1 may still sample the old image.
        let as_of = journal.image_as_of(dom0(), node, composed_at);
        assert_eq!(as_of.map(ImageRef::get_hash), Some(old_hash));

        // As of frame 2 (change applied in it), the current image is valid.
        assert!(journal.image_as_of(dom0(), node, journal.frame_seq()).is_none());
    }

    #[test]
    fn resolved_content_prefers_overlay_for_paint() {
        let styled_dom = StyledDom::default();
        let mut overlay = ContentOverlay::default();
        let node = NodeId::new(0);
        let overlay_img = img(4, 4);
        let overlay_hash = overlay_img.get_hash();
        overlay.set_image(dom0(), node, overlay_img);

        let resolved = ResolvedContent {
            overlay: Some(&overlay),
            styled_dom: &styled_dom,
            dom_id: dom0(),
        };
        assert_eq!(
            resolved.image_for_paint(node).map(|i| i.get_hash()),
            Some(overlay_hash),
            "overlay wins over the (empty) DOM"
        );

        // Without the overlay: falls back to the DOM (which has no image node).
        let resolved = ResolvedContent {
            overlay: None,
            styled_dom: &styled_dom,
            dom_id: dom0(),
        };
        assert!(resolved.image_for_paint(node).is_none());
    }

    #[test]
    fn overlay_remap_moves_entries_and_drops_unmounted() {
        use std::collections::BTreeMap;
        let mut overlay = ContentOverlay::default();
        overlay.set_image(dom0(), NodeId::new(2), img(1, 1));
        overlay.set_image(dom0(), NodeId::new(3), img(2, 2));
        let other_dom = DomId { inner: 9 };
        let other_hash = {
            let i = img(5, 5);
            let h = i.get_hash();
            overlay.set_image(other_dom, NodeId::new(2), i);
            h
        };

        // Node 2 moved to 1; node 3 unmounted.
        let mut moves = BTreeMap::new();
        moves.insert(NodeId::new(2), NodeId::new(1));
        let map = NodeIdMap::from_pairs(moves);
        overlay.remap_node_ids(dom0(), &map);

        assert!(overlay.image_for_node(dom0(), NodeId::new(1)).is_some());
        assert!(overlay.image_for_node(dom0(), NodeId::new(2)).is_none());
        assert!(overlay.image_for_node(dom0(), NodeId::new(3)).is_none(), "unmounted dropped");
        assert_eq!(
            overlay.image_for_node(other_dom, NodeId::new(2)).map(ImageRef::get_hash),
            Some(other_hash),
            "other DOMs untouched"
        );
    }
}
