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
    selection::TextCursor,
    styled_dom::StyledDom,
};
use azul_css::AzString;

use crate::managers::{NodeIdMap, NodeIdRemap};
use crate::text3::cache::InlineContent;

/// The text overlay entry: an IFC root's edited inline content, layered over
/// the immutable DOM's (now stale) text until the app's next generation
/// catches up ("optimistic state").
#[derive(Debug, Clone)]
pub struct DirtyTextNode {
    /// The new inline content (text + images) after editing
    pub content: Vec<InlineContent>,
    /// The new cursor position after editing
    pub cursor: Option<TextCursor>,
    /// Whether this edit requires ancestor relayout (e.g., text grew taller)
    pub needs_ancestor_relayout: bool,
    /// The document text revision that wrote this entry
    /// (`LayoutWindow::document_text_revision` after its bump). `0` = written
    /// by a path that predates revisions (tests, direct construction) — such
    /// entries are exempt from the acked-revision GC and fall back to the
    /// text-equality rule.
    pub revision: u64,
}

/// Flatten inline content to the plain string it displays.
///
/// This is the SAME flattening every consumer (a11y, convergence GC,
/// exports) must share, or two of them will disagree about whether an edit
/// "is" committed.
#[must_use]
pub fn flatten_inline_content(content: &[InlineContent]) -> String {
    let mut result = String::new();
    for item in content {
        match item {
            InlineContent::Text(text_run) => result.push_str(&text_run.text),
            InlineContent::Space(_) => result.push(' '),
            InlineContent::LineBreak(_) => result.push('\n'),
            InlineContent::Tab { .. } => result.push('\t'),
            InlineContent::Ruby { base, .. } => {
                result.push_str(&flatten_inline_content(base));
            }
            InlineContent::Marker { run, .. } => result.push_str(&run.text),
            InlineContent::Image(_) | InlineContent::Shape(_) => {}
        }
    }
    result
}

/// How many PRESENTED frames of history the journal keeps.
///
/// A backend re-presenting a not-fully-redrawn buffer composed `k` frames
/// ago may still sample the previous image of a node via
/// [`ContentJournal::image_as_of`]; `3` covers the deepest swapchain in the
/// tree (triple buffering — `wl_shm` double-buffer needs 2).
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
    /// Restyle a node at runtime (animation frames, `:hover`-driven writes,
    /// the css-override e2e op). Writes go through the retained cascade
    /// (`restyle_user_property` — the property cache's single write site);
    /// with `override_only` the node's inline vec is left alone (the
    /// fast animation channel). Tier: paint-only properties rebuild the DL,
    /// layout-affecting ones relayout — decided by
    /// `callbacks::css_properties_need_relayout`, so hosts cannot drift.
    NodeCss {
        dom_id: DomId,
        node_id: NodeId,
        props: Vec<azul_css::props::property::CssProperty>,
        override_only: bool,
    },
    /// Change a node's image mask (an attribute-slot write like css props —
    /// fingerprinted by reconcile, not a content-identity mutation).
    ImageMask {
        dom_id: DomId,
        node_id: NodeId,
        mask: azul_core::resources::ImageMask,
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
    ///   (CPU: the DL diff sees the `ImageRef` identity change and damages those
    ///   bounds; GPU: the translator re-reads the patched DL).
    /// - `RebuildDisplayList`: DL regeneration + re-render.
    /// - `Relayout`: incremental relayout (which rebuilds the DL).
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

/// Identity of one overlay part — "NodeIdGen2": minted from a monotonic
/// per-process counter, NEVER a real `NodeId` (a previewed part has no DOM
/// node until the app's re-render creates one).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OverlayPartId(pub u64);

impl OverlayPartId {
    fn mint() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self(NEXT.fetch_add(1, Ordering::Relaxed))
    }
}

/// A pending STRUCTURAL delta over the immutable DOM.
///
/// What a mutable DOM would have done with `insertChild` / `removeChild` /
/// `replaceChild` / split / merge, recorded here as a PREVIEW until the
/// app's re-render lands. The preview never copies content: it references
/// DOM nodes (by id) and pending subtrees (the changeset's own `Dom`
/// payloads).
#[derive(Debug, Clone)]
pub enum StructuralPreview {
    /// `node` renders as TWO parts split at the structural position.
    Split {
        node: NodeId,
        at: crate::managers::changeset::NodePosition,
        part_ids: [OverlayPartId; 2],
    },
    /// `second` renders MERGED into `first` (its children appended; `second`
    /// itself hidden).
    Merge { first: NodeId, second: NodeId },
    /// A pending subtree renders under `parent` at child `index`.
    Insert {
        parent: NodeId,
        index: u32,
        content: azul_core::dom::Dom,
    },
    /// `parent`'s children `[start, end)` render REMOVED.
    Remove {
        parent: NodeId,
        start: u32,
        end: u32,
    },
    /// `parent`'s children `[start, end)` render REPLACED by a pending
    /// subtree.
    Replace {
        parent: NodeId,
        start: u32,
        end: u32,
        content: azul_core::dom::Dom,
    },
}

/// One entry in the pending-structure list: the changeset it anticipates
/// (the commit handshake ties the two lifecycles) + the delta.
#[derive(Debug, Clone)]
pub struct PendingStructure {
    pub changeset_id: u64,
    pub preview: StructuralPreview,
}

/// One RESOLVED child in a node's adjusted child list — what a consumer
/// iterating "the children of X as the user should see them" receives.
#[derive(Debug, Clone)]
pub enum ResolvedChild<'a> {
    /// An existing DOM child, unchanged.
    Existing(NodeId),
    /// An existing DOM child that is part of a SPLIT: render only the given
    /// byte range of its text (`None` end = to the end).
    ExistingTextSlice {
        node: NodeId,
        start_byte: u32,
        end_byte: Option<u32>,
    },
    /// A pending (not-yet-real) subtree from a recorded insert/replace.
    Pending(&'a azul_core::dom::Dom),
}

/// The overlay proper. Fields are private on purpose: reads go through
/// [`ResolvedContent`] / the accessors below, writes only through the
/// chokepoint (`pub(crate)` mutators).
#[derive(Debug, Default)]
pub struct ContentOverlay {
    /// Node-image arm: the currently-displayed image for a node, overriding
    /// the immutable DOM's `NodeType::Image` content.
    images: BTreeMap<(DomId, NodeId), ImageRef>,
    /// Text arm: edited inline content per IFC root ("optimistic state"),
    /// overriding the immutable DOM's text. Written only through
    /// `LayoutWindow::update_text_cache_after_edit` (the documented single
    /// text mutation point, itself fed by `apply_text_changeset`); retired by
    /// [`Self::gc_converged_text`] when the app's regenerated DOM catches up.
    text: BTreeMap<(DomId, NodeId), DirtyTextNode>,
    /// Structural arm: pending tree deltas (split/merge/insert/remove/
    /// replace) previewed ahead of the app's re-render. Consumers resolve
    /// via [`ResolvedContent::children_for_node`] /
    /// [`Self::pending_structure`]; empty = the DOM is the whole truth.
    pending_structure: BTreeMap<DomId, Vec<PendingStructure>>,
}

impl ContentOverlay {
    /// The overlay's image for a node, if any. Callers wanting the full
    /// overlay→DOM read order use [`ResolvedContent`] instead.
    #[must_use]
    pub fn image_for_node(&self, dom_id: DomId, node_id: NodeId) -> Option<&ImageRef> {
        self.images.get(&(dom_id, node_id))
    }

    /// Iterate all image-overlay entries (renderer registration: the WR/GL
    /// backend walks produced callback frames to register external textures).
    pub fn iter_images(&self) -> impl Iterator<Item = (&(DomId, NodeId), &ImageRef)> {
        self.images.iter()
    }

    /// The overlay's edited text entry for an IFC root, if any.
    #[must_use]
    pub fn text_for_node(&self, dom_id: DomId, node_id: NodeId) -> Option<&DirtyTextNode> {
        self.text.get(&(dom_id, node_id))
    }

    /// Iterate all text-overlay entries (a11y snapshot, diagnostics).
    pub fn iter_text(&self) -> impl Iterator<Item = (&(DomId, NodeId), &DirtyTextNode)> {
        self.text.iter()
    }

    /// Number of text-overlay entries (manager fingerprints, tests).
    #[must_use]
    pub fn text_len(&self) -> usize {
        self.text.len()
    }

    /// Whether ANY text entry needs an ancestor relayout.
    #[must_use]
    pub fn any_text_needs_ancestor_relayout(&self) -> bool {
        self.text.values().any(|d| d.needs_ancestor_relayout)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.images.is_empty() && self.text.is_empty() && self.pending_structure.is_empty()
    }

    pub(crate) fn set_image(
        &mut self,
        dom_id: DomId,
        node_id: NodeId,
        image: ImageRef,
    ) -> Option<ImageRef> {
        self.images.insert((dom_id, node_id), image)
    }

    pub(crate) fn set_text(
        &mut self,
        dom_id: DomId,
        node_id: NodeId,
        entry: DirtyTextNode,
    ) -> Option<DirtyTextNode> {
        self.text.insert((dom_id, node_id), entry)
    }

    pub(crate) fn text_for_node_mut(
        &mut self,
        dom_id: DomId,
        node_id: NodeId,
    ) -> Option<&mut DirtyTextNode> {
        self.text.get_mut(&(dom_id, node_id))
    }

    /// The pending structural deltas of `dom` (empty slice = none).
    #[must_use]
    pub fn pending_structure(&self, dom_id: DomId) -> &[PendingStructure] {
        self.pending_structure
            .get(&dom_id)
            .map_or(&[], |v| v.as_slice())
    }

    /// Number of pending structural deltas across all DOMs (tests).
    #[must_use]
    pub fn pending_structure_len(&self) -> usize {
        self.pending_structure.values().map(Vec::len).sum()
    }

    /// Materialize a recorded structural changeset as a PREVIEW — the
    /// generic entry for EVERY operation kind (the recorded delta IS the
    /// preview; nothing is copied except the changeset's own subtree
    /// payloads, which are refcounted).
    pub(crate) fn preview_structural_change(
        &mut self,
        dom_id: DomId,
        changeset: &crate::managers::changeset::DocumentChangeset,
    ) {
        use crate::managers::changeset::DocumentOperation as Op;
        let preview = match &changeset.operation {
            Op::SplitNode(sp) => {
                sp.node
                    .node
                    .into_crate_internal()
                    .map(|node| StructuralPreview::Split {
                        node,
                        at: sp.at,
                        part_ids: [OverlayPartId::mint(), OverlayPartId::mint()],
                    })
            }
            Op::MergeNodes(m) => match (
                m.first.node.into_crate_internal(),
                m.second.node.into_crate_internal(),
            ) {
                (Some(first), Some(second)) => Some(StructuralPreview::Merge { first, second }),
                _ => None,
            },
            Op::InsertChildren(i) => {
                i.parent
                    .node
                    .into_crate_internal()
                    .map(|parent| StructuralPreview::Insert {
                        parent,
                        index: i.index,
                        content: i.content.clone(),
                    })
            }
            Op::RemoveChildren(r) => {
                r.parent
                    .node
                    .into_crate_internal()
                    .map(|parent| StructuralPreview::Remove {
                        parent,
                        start: r.start,
                        end: r.end,
                    })
            }
            Op::ReplaceChildren(r) => {
                r.parent
                    .node
                    .into_crate_internal()
                    .map(|parent| StructuralPreview::Replace {
                        parent,
                        start: r.start,
                        end: r.end,
                        content: r.content.clone(),
                    })
            }
            // Wrap/unwrap previews are staged with the render consumption
            // (they restructure WITHIN a node — the child-list adjustment
            // needs the part-aware renderer to matter visually).
            Op::WrapRange(_) | Op::UnwrapRange(_) => None,
        };
        if let Some(preview) = preview {
            self.pending_structure
                .entry(dom_id)
                .or_default()
                .push(PendingStructure {
                    changeset_id: changeset.id,
                    preview,
                });
        }
    }

    /// Acked-revision GC — the EXPLICIT half of convergence: the app declared
    /// (via `LayoutWindow::mark_text_revision_synced`) that its model has
    /// incorporated every text edit up to and including `acked`, so entries
    /// stamped at or below it are committed and dropped — even when the app
    /// NORMALIZED the text on the way in (trimming, canonicalization), which
    /// the equality rule in [`Self::gc_converged_text`] can never see past.
    /// Un-stamped entries (`revision == 0`) are left for the equality rule.
    pub(crate) fn gc_acked_text(&mut self, dom_id: DomId, acked: u64) {
        if acked == 0 {
            return;
        }
        self.text.retain(|&(d, _), dirty| {
            d != dom_id || dirty.revision == 0 || dirty.revision > acked
        });
    }

    /// Drop every pending structural delta of `dom` — called when a new
    /// generation lands (the app either applied the changeset, so REAL nodes
    /// exist, or rejected it, so the content reverts; both end the preview).
    pub(crate) fn gc_splits(&mut self, dom_id: DomId) {
        self.pending_structure.remove(&dom_id);
    }

    /// Convergence GC — the rule that closes the edit commit loop: after a
    /// generation swap (keys already remapped), an overlay text entry whose
    /// flattened text EQUALS the new DOM's text at that node has been
    /// committed by the app → drop it. Not equal → the app hasn't caught up;
    /// the overlay stays authoritative. Before this rule, entries were
    /// remapped forward FOREVER and DOM-reading exports silently saw pre-edit
    /// text.
    pub(crate) fn gc_converged_text(&mut self, dom_id: DomId, styled_dom: &StyledDom) {
        let node_data = styled_dom.node_data.as_container();
        self.text.retain(|&(d, node_id), dirty| {
            if d != dom_id {
                return true;
            }
            let Some(node) = node_data.get(node_id) else {
                // Node gone in the new generation: nothing to converge to.
                return false;
            };
            let dom_text = if let NodeType::Text(s) = node.get_node_type() {
                s.as_str().to_string()
            } else {
                // Non-text IFC roots (contenteditable hosts): compare against
                // the concatenated text of DIRECT text children.
                let hierarchy = styled_dom.node_hierarchy.as_container();
                let mut s = String::new();
                if let Some(n) = hierarchy.get(node_id) {
                    let mut child = n.first_child_id(node_id);
                    while let Some(c) = child {
                        if let Some(cd) = node_data.get(c) {
                            if let NodeType::Text(t) = cd.get_node_type() {
                                s.push_str(t.as_str());
                            }
                        }
                        child = hierarchy
                            .get(c)
                            .and_then(azul_core::styled_dom::NodeHierarchyItem::next_sibling_id);
                    }
                }
                s
            };
            flatten_inline_content(&dirty.content) != dom_text
        });
    }

    /// Drop every overlay entry of `dom` (full DOM regeneration without a
    /// remap — the new generation's DOM is the authority again).
    pub(crate) fn clear_dom(&mut self, dom_id: DomId) {
        self.images.retain(|(d, _), _| *d != dom_id);
        self.text.retain(|(d, _), _| *d != dom_id);
        self.pending_structure.remove(&dom_id);
    }
}

impl NodeIdRemap for ContentOverlay {
    fn remap_node_ids(&mut self, dom: DomId, map: &NodeIdMap) {
        crate::managers::remap_dom_keys(&mut self.images, dom, map);
        crate::managers::remap_dom_keys(&mut self.text, dom, map);
        // Previews do NOT remap: a remap means a new generation landed,
        // which ends every preview's life (gc at the layout tail); remapping
        // would keep a preview alive over content that superseded it.
        self.pending_structure.remove(&dom);
    }
}

/// The one overlay→DOM read order, borrowed by every consumer.
///
/// Constructed at the few pipeline entries that own both halves (display-list
/// build / IFC build via `LayoutContext`, exports); everything downstream
/// takes this instead of reaching into `StyledDom` for content.
#[derive(Debug, Clone, Copy)]
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

    /// The children of `node_id` AS THE USER SHOULD SEE THEM: the immutable
    /// DOM's child list with every pending structural delta applied on top —
    /// the read side of the `.insertChild`-through-the-overlay design.
    /// With no pending structure this is exactly the DOM's children.
    ///
    /// Split previews surface on the SPLIT NODE itself via
    /// [`Self::split_positions_for_node`]; here a split node still occupies
    /// one slot (its parts are an internal regrouping).
    #[must_use]
    pub fn children_for_node(&self, node_id: NodeId) -> Vec<ResolvedChild<'_>> {
        let hierarchy = self.styled_dom.node_hierarchy.as_container();
        let mut out: Vec<ResolvedChild<'_>> = Vec::new();
        let mut child = hierarchy
            .get(node_id)
            .and_then(|n| n.first_child_id(node_id));
        while let Some(c) = child {
            out.push(ResolvedChild::Existing(c));
            child = hierarchy
                .get(c)
                .and_then(azul_core::styled_dom::NodeHierarchyItem::next_sibling_id);
        }

        let Some(overlay) = self.overlay else {
            return out;
        };
        for pending in overlay.pending_structure(self.dom_id) {
            match &pending.preview {
                StructuralPreview::Insert {
                    parent,
                    index,
                    content,
                } if *parent == node_id => {
                    let at = (*index as usize).min(out.len());
                    for (offset, pending_child) in content.children.as_ref().iter().enumerate() {
                        out.insert(at + offset, ResolvedChild::Pending(pending_child));
                    }
                }
                StructuralPreview::Remove { parent, start, end } if *parent == node_id => {
                    let s = (*start as usize).min(out.len());
                    let e = (*end as usize).min(out.len());
                    out.drain(s..e);
                }
                StructuralPreview::Replace {
                    parent,
                    start,
                    end,
                    content,
                } if *parent == node_id => {
                    let s = (*start as usize).min(out.len());
                    let e = (*end as usize).min(out.len());
                    let replacement: Vec<ResolvedChild<'_>> = content
                        .children
                        .as_ref()
                        .iter()
                        .map(ResolvedChild::Pending)
                        .collect();
                    out.splice(s..e, replacement);
                }
                StructuralPreview::Merge { first, second } => {
                    // The merged-away node disappears from ITS parent's list;
                    // its children surface under `first` (when iterating
                    // first's children).
                    if out
                        .iter()
                        .any(|c| matches!(c, ResolvedChild::Existing(n) if n == second))
                    {
                        out.retain(|c| !matches!(c, ResolvedChild::Existing(n) if n == second));
                    }
                    if *first == node_id {
                        let mut sc = hierarchy
                            .get(*second)
                            .and_then(|n| n.first_child_id(*second));
                        while let Some(c) = sc {
                            out.push(ResolvedChild::Existing(c));
                            sc = hierarchy.get(c).and_then(
                                azul_core::styled_dom::NodeHierarchyItem::next_sibling_id,
                            );
                        }
                    }
                }
                // A split regroups the node's OWN content; its parent's child
                // list is unchanged (the second part becomes real only when
                // the app applies).
                _ => {}
            }
        }
        out
    }

    /// The pending SPLIT positions of `node_id` (usually 0 or 1) — a
    /// part-aware consumer renders the node's children regrouped at these
    /// structural positions.
    #[must_use]
    pub fn split_positions_for_node(
        &self,
        node_id: NodeId,
    ) -> Vec<crate::managers::changeset::NodePosition> {
        let Some(overlay) = self.overlay else {
            return Vec::new();
        };
        overlay
            .pending_structure(self.dom_id)
            .iter()
            .filter_map(|p| match &p.preview {
                StructuralPreview::Split { node, at, .. } if *node == node_id => Some(*at),
                _ => None,
            })
            .collect()
    }

    /// The text to READ for `node_id` (a11y, exports): the overlay's edited
    /// content flattened, falling back to the DOM's `NodeType::Text`.
    /// `None` when the node has neither.
    #[must_use]
    pub fn text_for_node(&self, node_id: NodeId) -> Option<String> {
        if let Some(overlay) = self.overlay {
            if let Some(dirty) = overlay.text_for_node(self.dom_id, node_id) {
                return Some(flatten_inline_content(&dirty.content));
            }
        }
        let node_data = self.styled_dom.node_data.as_container();
        match node_data.get(node_id)?.get_node_type() {
            NodeType::Text(s) => Some(s.as_str().to_string()),
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
    /// Manager-mutated window state moved this frame (focus / text selection
    /// / scroll positions). These live OUTSIDE the DOM by design — the
    /// journal records THAT they changed (fingerprint transitions), so every
    /// non-DOM mutation is at least auditable per frame, same clock as
    /// content.
    ManagerState {
        focus_changed: bool,
        selection_changed: bool,
        scroll_changed: bool,
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
    /// Fingerprints of (focus, selection, scroll) as of the last frame —
    /// the diff basis for [`Self::record_manager_state`].
    last_manager_fingerprint: Option<[u64; 3]>,
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
        while self.entries.front().is_some_and(|e| e.frame_seq < cutoff) {
            self.entries.pop_front();
        }
    }

    /// Diff the manager-state fingerprints against last frame's and record
    /// a [`AppliedChange::ManagerState`] entry when anything moved. Called
    /// from the shared frame preparation (the same clock content uses).
    pub(crate) fn record_manager_state(&mut self, fingerprint: [u64; 3]) {
        if let Some(last) = self.last_manager_fingerprint {
            let focus_changed = last[0] != fingerprint[0];
            let selection_changed = last[1] != fingerprint[1];
            let scroll_changed = last[2] != fingerprint[2];
            if focus_changed || selection_changed || scroll_changed {
                self.record(AppliedChange::ManagerState {
                    focus_changed,
                    selection_changed,
                    scroll_changed,
                });
            }
        }
        self.last_manager_fingerprint = Some(fingerprint);
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
    pub fn image_as_of(&self, dom_id: DomId, node_id: NodeId, frame_seq: u64) -> Option<&ImageRef> {
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
            AppliedChange::ImageById { .. } | AppliedChange::ManagerState { .. } => true,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn img(w: usize, h: usize) -> ImageRef {
        ImageRef::null_image(
            w,
            h,
            azul_core::resources::RawImageFormat::BGRA8,
            Vec::new(),
        )
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
        assert!(
            journal.is_empty(),
            "entries older than the swapchain depth must retire"
        );
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
        assert!(journal
            .image_as_of(dom0(), node, journal.frame_seq())
            .is_none());
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
    fn structural_previews_adjust_the_resolved_child_list() {
        use crate::managers::changeset::{
            DocOpInsertChildren, DocOpRemoveChildren, DocumentChangeset, DocumentOperation,
            EditResumePoint, NodePosition,
        };
        use azul_core::dom::{Dom, DomNodeId};
        use azul_core::styled_dom::NodeHierarchyItemId;
        use azul_core::task::{Instant, SystemTick};

        // DOM: div > [p, p] (nodes 1, 2 with their text children 3, 4… the
        // exact ids come from creation order; resolve them dynamically).
        let mut dom = Dom::create_div();
        let mut p1 = Dom::create_p();
        p1.add_child(Dom::create_text_do_not_use_without_block_level_wrapper(
            "one",
        ));
        let mut p2 = Dom::create_p();
        p2.add_child(Dom::create_text_do_not_use_without_block_level_wrapper(
            "two",
        ));
        dom.add_child(p1);
        dom.add_child(p2);
        let styled = StyledDom::create_from_dom(dom);
        let root = NodeId::new(0);

        let dom_node = |n: NodeId| DomNodeId {
            dom: dom0(),
            node: NodeHierarchyItemId::from_crate_internal(Some(n)),
        };
        let resume = EditResumePoint {
            anchor_key: 1,
            node_path: vec![0].into(),
            position: NodePosition::before_child(0),
        };

        let mut overlay = ContentOverlay::default();

        // Baseline: with no pending structure the resolved children ARE the
        // DOM children (two <p> elements).
        let resolved = ResolvedContent {
            overlay: Some(&overlay),
            styled_dom: &styled,
            dom_id: dom0(),
        };
        let base = resolved.children_for_node(root);
        assert_eq!(base.len(), 2);
        assert!(base.iter().all(|c| matches!(c, ResolvedChild::Existing(_))));

        // A recorded INSERT previews a PENDING subtree between them — the
        // .insertChild made visible without any DOM mutation.
        let mut ul = Dom::create_node(azul_core::xml::tag_to_node_type("ul"));
        ul.add_child(Dom::create_p());
        let insert_cs = DocumentChangeset::new(
            dom_node(root),
            DocumentOperation::InsertChildren(DocOpInsertChildren {
                parent: dom_node(root),
                index: 1,
                content: {
                    let mut frag = Dom::create_div();
                    frag.add_child(ul);
                    frag
                },
            }),
            resume.clone(),
            Instant::Tick(SystemTick::new(0)),
        );
        overlay.preview_structural_change(dom0(), &insert_cs);
        let resolved = ResolvedContent {
            overlay: Some(&overlay),
            styled_dom: &styled,
            dom_id: dom0(),
        };
        let with_insert = resolved.children_for_node(root);
        assert_eq!(with_insert.len(), 3);
        assert!(matches!(with_insert[1], ResolvedChild::Pending(_)));

        // A recorded REMOVE previews children [0..1) gone.
        let remove_cs = DocumentChangeset::new(
            dom_node(root),
            DocumentOperation::RemoveChildren(DocOpRemoveChildren {
                parent: dom_node(root),
                start: 0,
                end: 1,
            }),
            resume,
            Instant::Tick(SystemTick::new(0)),
        );
        overlay.preview_structural_change(dom0(), &remove_cs);
        let resolved = ResolvedContent {
            overlay: Some(&overlay),
            styled_dom: &styled,
            dom_id: dom0(),
        };
        let with_both = resolved.children_for_node(root);
        assert_eq!(
            with_both.len(),
            2,
            "insert(+1) then remove(-1): {with_both:?}"
        );

        // A new generation ends every preview.
        overlay.gc_splits(dom0());
        assert_eq!(overlay.pending_structure_len(), 0);
    }

    #[test]
    fn acked_revision_gc_converges_where_text_equality_cannot() {
        use crate::text3::cache::{InlineContent, StyledRun};
        use std::sync::Arc;

        fn dirty_rev(text: &str, revision: u64) -> DirtyTextNode {
            DirtyTextNode {
                content: vec![InlineContent::Text(StyledRun {
                    text: Arc::from(text),
                    style: Arc::new(Default::default()),
                    logical_start_byte: 0,
                    source_node_id: None,
                })],
                cursor: None,
                needs_ancestor_relayout: false,
                revision,
            }
        }

        let mut overlay = ContentOverlay::default();
        let committed = NodeId::new(1); // acked (rev 3 ≤ 3): drops WITHOUT text comparison
        let unacked = NodeId::new(2); // typed after the ack (rev 5 > 3): stays
        let unstamped = NodeId::new(3); // rev 0: exempt, left for the equality rule
        overlay.set_text(dom0(), committed, dirty_rev("  hello  ", 3));
        overlay.set_text(dom0(), unacked, dirty_rev("newer typing", 5));
        overlay.set_text(dom0(), unstamped, dirty_rev("legacy path", 0));

        // acked == 0 (nothing ever acked) must be a no-op.
        overlay.gc_acked_text(dom0(), 0);
        assert!(overlay.text_for_node(dom0(), committed).is_some());

        overlay.gc_acked_text(dom0(), 3);
        assert!(
            overlay.text_for_node(dom0(), committed).is_none(),
            "an acked entry retires even though the app NORMALIZED the text \
             (\"  hello  \" vs whatever the DOM now carries) — the case the \
             equality rule keeps authoritative forever"
        );
        assert!(
            overlay.text_for_node(dom0(), unacked).is_some(),
            "an edit typed AFTER the acked revision must stay authoritative"
        );
        assert!(
            overlay.text_for_node(dom0(), unstamped).is_some(),
            "an un-stamped (revision 0) entry is exempt from the acked GC"
        );
    }

    #[test]
    fn text_gc_drops_converged_entries_and_keeps_diverged_ones() {
        use crate::text3::cache::{InlineContent, StyledRun};
        use std::sync::Arc;

        fn dirty(text: &str) -> DirtyTextNode {
            DirtyTextNode {
                content: vec![InlineContent::Text(StyledRun {
                    text: Arc::from(text),
                    style: Arc::new(Default::default()),
                    logical_start_byte: 0,
                    source_node_id: None,
                })],
                cursor: None,
                needs_ancestor_relayout: false,
                revision: 0,
            }
        }

        // DOM: div > [text "committed"]
        let mut dom = azul_core::dom::Dom::create_div();
        dom.add_child(
            azul_core::dom::Dom::create_text_do_not_use_without_block_level_wrapper("committed"),
        );
        let styled = StyledDom::create_from_dom(dom);
        let text_node = NodeId::new(1);

        let mut overlay = ContentOverlay::default();
        // Entry keyed on the TEXT node whose edit the app has committed:
        overlay.set_text(dom0(), text_node, dirty("committed"));
        overlay.gc_converged_text(dom0(), &styled);
        assert!(
            overlay.text_for_node(dom0(), text_node).is_none(),
            "app committed the edit → overlay entry retires (the commit loop closes)"
        );

        // Entry the app has NOT committed stays authoritative:
        overlay.set_text(dom0(), text_node, dirty("still-editing"));
        overlay.gc_converged_text(dom0(), &styled);
        assert!(
            overlay.text_for_node(dom0(), text_node).is_some(),
            "un-committed edit must survive the generation swap"
        );

        // Entry keyed on the HOST (div): compares against concatenated direct
        // text children.
        overlay.clear_dom(dom0());
        overlay.set_text(dom0(), NodeId::new(0), dirty("committed"));
        overlay.gc_converged_text(dom0(), &styled);
        assert!(
            overlay.text_for_node(dom0(), NodeId::new(0)).is_none(),
            "host-keyed entry converges against its direct text children"
        );
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
        assert!(
            overlay.image_for_node(dom0(), NodeId::new(3)).is_none(),
            "unmounted dropped"
        );
        assert_eq!(
            overlay
                .image_for_node(other_dom, NodeId::new(2))
                .map(ImageRef::get_hash),
            Some(other_hash),
            "other DOMs untouched"
        );
    }
}
