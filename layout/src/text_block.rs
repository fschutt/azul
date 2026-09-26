//! Which text block a node, a caret or a selection end lives in.
//!
//! A [`TextBlock`] names one inline formatting context - the box whose inline
//! layout a `TextCursor` indexes. It is resolved here, by ONE rule, from any
//! node that can carry text: the IFC root itself, a text leaf, an inline
//! element (`<b>`, `<span>`) inside it, or a node that generated no box (an
//! empty text node), which belongs to its nearest boxed ancestor's block.
//! Anonymous block boxes are named too.
//!
//! The rule itself lives on the layout tree
//! ([`LayoutTree::owning_ifc_root`], [`LayoutTree::text_block_at`]) so that the
//! display-list builder, which has the tree but no window, resolves the block
//! it paints exactly as the editing paths resolve the block they edit.
//!
//! [`TextTarget`] is the choke point on top: one block, resolved once with its
//! materialized layout, its blank-line caret and its `user-select` /
//! editability - what the click, the drag, the keyboard, Ctrl+A and the focus
//! seed all resolve through.
//!
//! Beside it, the [`EditHost`]: the contenteditable host a key or a focus
//! lands on. The two used to be the same bare `NodeId` in different places -
//! the host passed where a block was needed (every keyboard op in a text
//! widget read the host's inline layout, which it has none of), a block or a
//! leaf passed where the host was.
//!
//! [`LayoutTree::owning_ifc_root`]: crate::solver3::layout_tree::LayoutTree::owning_ifc_root
//! [`LayoutTree::text_block_at`]: crate::solver3::layout_tree::LayoutTree::text_block_at

use alloc::{collections::BTreeSet, sync::Arc};

use azul_core::{
    dom::{DomId, DomNodeId, NodeId},
    selection::{
        CursorAffinity, GraphemeClusterId, MultiCursorState, Selection, SelectionRange, TextBlock,
        TextCursor,
    },
    spaces::ScrolledContentPoint,
    styled_dom::{NodeHierarchyItem, NodeHierarchyItemId},
};

use azul_core::styled_dom::StyledDom;

use crate::{
    callbacks::CallbackChange,
    solver3::{getters, layout_tree::LayoutNodeId},
    text3::{
        cache::{ShapedItem, UnifiedLayout},
        dense::DenseText,
    },
    window::LayoutWindow,
};

/// How far [`LayoutWindow::text_block_of`] walks up the DOM from a node that
/// generated no box before giving up.
const BOXLESS_ANCESTOR_WALK_LIMIT: usize = 64;

/// The contenteditable HOST of an edit: the nearest self-or-ancestor of a node
/// that carries the `contenteditable` flag.
///
/// What focus lands on, what a keyboard edit's reach is bounded by, and what
/// the session key and a structural edit's resume point are anchored to - but
/// never what an edit of TEXT is keyed to: that is the caret's [`TextBlock`]
/// ([`LayoutWindow::edit_element`]).
///
/// Minted only by [`LayoutWindow::find_contenteditable_host`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EditHost {
    dom: DomId,
    node: NodeId,
}

impl EditHost {
    /// The DOM the host is in.
    #[must_use]
    pub const fn dom(self) -> DomId {
        self.dom
    }

    /// The host element.
    #[must_use]
    pub const fn node(self) -> NodeId {
        self.node
    }

    /// The host element as a `DomNodeId`.
    #[must_use]
    pub fn dom_node(self) -> DomNodeId {
        DomNodeId {
            dom: self.dom,
            node: NodeHierarchyItemId::from_crate_internal(Some(self.node)),
        }
    }
}

/// Which text blocks a walk in document order yields
/// ([`LayoutWindow::text_block_roots`]).
///
/// There used to be five "document orders" - `NodeId` index, layout index,
/// a DOM depth-first walk, a caret-owner-first breadth-first walk and
/// on-screen distance - and each walk filtered differently: the painter
/// skipped `user-select: none` text, the selection that it painted did not.
/// One walk, and the filters said out loud.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockFilter {
    /// Only blocks whose text `user-select` lets be selected.
    pub selectable_only: bool,
    /// Only blocks inside this node's subtree (its own block included).
    pub within: Option<DomNodeId>,
}

impl BlockFilter {
    /// Every text block - anonymous ones included.
    pub const ALL: Self = Self {
        selectable_only: false,
        within: None,
    };

    /// Every block whose text a selection may cover: `user-select` allows it.
    /// The rule the painter applies, so a selection never holds text it
    /// does not highlight.
    pub const SELECTABLE: Self = Self {
        selectable_only: true,
        within: None,
    };
}

/// Whether `user-select` lets `block`'s text be selected - read off the
/// element that holds it (an anonymous block has no style of its own).
fn block_is_selectable(styled_dom: &StyledDom, block: TextBlock) -> bool {
    let style_node = block.container();
    styled_dom
        .styled_nodes
        .as_container()
        .get(style_node)
        .is_some_and(|n| getters::is_text_selectable(styled_dom, style_node, &n.styled_node_state))
}

/// ONE text block, resolved once with everything a text operation on it
/// needs - the choke point every pointer, keyboard and selection path goes
/// through instead of re-deriving it:
///
/// - the IFC root and its inline layout, MATERIALIZED: under the default dense text path the
///   stored sparse layout is the empty retirement sentinel, and every caller that forgot to
///   expand it found no cluster and did nothing (a click placed no caret, Ctrl+A selected
///   nothing);
/// - the one caret a blank editable line owns, BUILT IN: a line with no cluster has no glyph to
///   hit or to start/end on, and each path that did not carry its own copy of that fallback could
///   not reach a blank line;
/// - whether the block's text is selectable (`user-select`) and editable, read off the element
///   whose box holds it (an anonymous block's container).
#[derive(Debug, Clone)]
pub struct TextTarget {
    /// The block.
    pub block: TextBlock,
    /// The editing host the block is edited through, if any.
    pub host: Option<EditHost>,
    /// The layout node that owns the block's inline layout.
    pub layout_index: LayoutNodeId,
    /// The block's inline layout, materialized.
    pub layout: Arc<UnifiedLayout>,
    /// The dense view, when retained.
    pub dense: Option<Arc<DenseText>>,
    /// `user-select` lets the block's text be selected.
    pub selectable: bool,
    /// The block's text is editable (an inherited `contenteditable`).
    pub editable: bool,
}

impl TextTarget {
    /// Whether the block has no cluster at all: a blank line.
    fn is_blank(&self) -> bool {
        !self
            .layout
            .items
            .iter()
            .any(|item| matches!(item.item, ShapedItem::Cluster(_)))
    }

    /// The one caret position a blank EDITABLE line owns: offset 0, leading.
    ///
    /// `layout_ifc` keeps a strut line box for an empty editable IFC so a caret
    /// can stand there, but there is no glyph to hit-test or to take a first or
    /// last cluster from. `None` for a block with text, and for a blank block
    /// that is not editable (an empty `<div>` is not something you put a caret
    /// in).
    #[must_use]
    pub fn blank_line_caret(&self) -> Option<TextCursor> {
        (self.editable && self.is_blank()).then_some(TextCursor {
            cluster_id: GraphemeClusterId {
                source_run: 0,
                start_byte_in_run: 0,
            },
            affinity: CursorAffinity::Leading,
        })
    }

    /// Leading on the block's first cluster.
    #[must_use]
    pub fn first_cluster_caret(&self) -> Option<TextCursor> {
        self.layout.get_first_cluster_cursor()
    }

    /// Trailing on the block's last cluster.
    #[must_use]
    pub fn last_cluster_caret(&self) -> Option<TextCursor> {
        self.layout.get_last_cluster_cursor()
    }

    /// The block's first caret: on its first cluster, or on a blank editable
    /// line the one position it owns.
    #[must_use]
    pub fn first_caret(&self) -> Option<TextCursor> {
        self.first_cluster_caret()
            .or_else(|| self.blank_line_caret())
    }

    /// The block's last caret: on its last cluster, or on a blank editable
    /// line the one position it owns.
    #[must_use]
    pub fn last_caret(&self) -> Option<TextCursor> {
        self.last_cluster_caret()
            .or_else(|| self.blank_line_caret())
    }

    /// The caret at `point`, in the block's own (scrolled content) space, or
    /// on a blank editable line the one position it owns.
    #[must_use]
    pub fn hittest(&self, point: ScrolledContentPoint) -> Option<TextCursor> {
        self.layout
            .hittest_point(point)
            .or_else(|| self.blank_line_caret())
    }

    /// The caret at the flat BYTE offset `offset` into the block's text - the
    /// one converter the byte-offset protocols (the IME's `selectedRange` /
    /// `firstRectForCharacterRange:`, an accessibility `SetTextSelection`)
    /// go through, so they cannot drift apart. The dense view when it is
    /// retained, else the same walk over the materialized layout's clusters
    /// (the two are pinned equal by `dense_cursor_helpers_agree_with_the_sparse_walks`);
    /// on a blank editable line, the one position it owns.
    #[must_use]
    pub fn caret_at_byte(&self, offset: usize) -> Option<TextCursor> {
        let offset = u32::try_from(offset).unwrap_or(u32::MAX);
        self.dense
            .as_ref()
            .and_then(|dense| dense.byte_offset_to_cursor(offset))
            .or_else(|| caret_at_byte_in_layout(&self.layout, offset))
            .or_else(|| self.blank_line_caret())
    }
}

/// [`TextTarget::caret_at_byte`] over the sparse layout: byte 0 is before
/// the first cluster (`Leading`); any other offset is after the first
/// cluster, in item order and counting each cluster's text bytes, whose span
/// reaches it; past the end falls to the last cluster. `None` for a layout
/// with no cluster.
fn caret_at_byte_in_layout(layout: &UnifiedLayout, offset: u32) -> Option<TextCursor> {
    let mut clusters = layout.items.iter().filter_map(|item| match &item.item {
        ShapedItem::Cluster(cluster) => Some(cluster),
        _ => None,
    });
    let trailing = |cluster: &crate::text3::cache::ShapedCluster| TextCursor {
        cluster_id: cluster.source_cluster_id,
        affinity: CursorAffinity::Trailing,
    };
    if offset == 0 {
        // BEFORE the first character. `Trailing` on it - what this returned -
        // is after it: byte 0 read back as byte 1, and the IME's caret rect
        // for "the start" stood one glyph in.
        return clusters.next().map(|cluster| TextCursor {
            cluster_id: cluster.source_cluster_id,
            affinity: CursorAffinity::Leading,
        });
    }
    let mut start = 0u32;
    let mut last = None;
    for cluster in clusters {
        let end = start + u32::try_from(cluster.text().len()).unwrap_or(u32::MAX);
        if offset >= start && offset <= end {
            return Some(trailing(cluster));
        }
        start = end;
        last = Some(cluster);
    }
    last.map(trailing)
}

impl LayoutWindow {
    /// The [`TextTarget`] of `block`, or `None` when it is not laid out.
    #[must_use]
    pub fn text_target(&self, block: TextBlock) -> Option<TextTarget> {
        let root = self
            .layout_results
            .get(&block.dom())?
            .layout_tree
            .text_block_root(block.key())?;
        self.text_target_with_root(block, root)
    }

    /// The [`TextTarget`] of the text block `node`'s text is in
    /// ([`Self::text_block_of`]).
    #[must_use]
    pub fn text_target_at_node(&self, node: DomNodeId) -> Option<TextTarget> {
        self.text_target(self.text_block_of(node)?)
    }

    /// The [`TextTarget`] whose IFC root is the layout node `ifc_root` of
    /// `dom` - the one a pointer hit or a candidate scan found.
    #[must_use]
    pub fn text_target_at_layout_index(
        &self,
        dom: DomId,
        ifc_root: LayoutNodeId,
    ) -> Option<TextTarget> {
        let block = self.text_block_at_layout_index(dom, ifc_root)?;
        self.text_target_with_root(block, ifc_root.index())
    }

    /// The [`TextTarget`] of the editing session.
    #[must_use]
    pub fn session_text_target(&self) -> Option<TextTarget> {
        self.text_target(self.text_edit_manager.get_editing_block()?)
    }

    /// The block a KEY that went to `scope` (the focused node of seat
    /// `seat_id`: its editing host, or a node inside one) acts in: the seat's
    /// caret block when it lies inside `scope` (the primary's editing session
    /// for seat 0); else the block `scope`'s own text is in; else the first
    /// block below `scope` - a `TextInput`'s value paragraph under its host -
    /// found by the candidate scan the edit paths use.
    #[must_use]
    pub fn keyboard_text_target(&self, seat_id: u64, scope: DomNodeId) -> Option<TextTarget> {
        let scope_node = scope.node.into_crate_internal()?;
        let caret_block = if seat_id == azul_core::window::PRIMARY_POINTER_SEAT {
            self.text_edit_manager.get_editing_block()
        } else {
            self.text_edit_manager.seat_caret(seat_id).map(|c| c.block)
        };
        let in_scope = caret_block.filter(|block| {
            block.dom() == scope.dom
                && self.node_is_self_or_descendant(scope.dom, block.first_node(), scope_node)
        });
        if let Some(target) = in_scope.and_then(|block| self.text_target(block)) {
            return Some(target);
        }
        let ifc_node = self
            .resolve_ifc_layout_node(scope.dom, scope_node)
            .unwrap_or(scope_node);
        self.text_target_at_node(DomNodeId {
            dom: scope.dom,
            node: NodeHierarchyItemId::from_crate_internal(Some(ifc_node)),
        })
    }

    fn text_target_with_root(&self, block: TextBlock, root: usize) -> Option<TextTarget> {
        let layout_result = self.layout_results.get(&block.dom())?;
        let tree = &layout_result.layout_tree;
        let layout = tree.materialized_inline_layout_for_node(root)?;
        let dense = tree.get_dense_for_node(root).cloned();
        let styled_dom = &layout_result.styled_dom;
        // An anonymous block has no style of its own: its container's.
        let style_node = block.container();
        let selectable = block_is_selectable(styled_dom, block);
        let editable = style_node.index() < styled_dom.node_data.as_container().len()
            && getters::is_node_contenteditable_inherited(styled_dom, style_node);
        Some(TextTarget {
            block,
            host: self.find_contenteditable_host(block.container_dom_node()),
            layout_index: LayoutNodeId::new(root),
            layout,
            dense,
            selectable,
            editable,
        })
    }

    /// The editing host of `node`: the nearest self-or-ancestor with the
    /// `contenteditable` flag. `None` outside any.
    #[must_use]
    pub fn find_contenteditable_host(&self, node: DomNodeId) -> Option<EditHost> {
        let node_id = node.node.into_crate_internal()?;
        let layout_result = self.layout_results.get(&node.dom)?;
        let node_data = layout_result.styled_dom.node_data.as_container();
        let hierarchy = layout_result.styled_dom.node_hierarchy.as_container();
        let mut current = Some(node_id);
        while let Some(nid) = current {
            if node_data
                .get(nid)
                .is_some_and(azul_core::dom::NodeData::is_contenteditable)
            {
                return Some(EditHost {
                    dom: node.dom,
                    node: nid,
                });
            }
            current = hierarchy.get(nid).and_then(NodeHierarchyItem::parent_id);
        }
        None
    }

    /// The element an edit made through `scope` - the node a key went to:
    /// its editing host, or a node inside one - is keyed to.
    ///
    /// The caret's text block, when it lies inside `scope`: its ELEMENT owns
    /// the runs the caret's cluster ids index, so typing, Backspace/Delete,
    /// paste and the undo snapshots all splice that element's content.
    /// Otherwise `scope` itself (a flat editable is its own block).
    ///
    /// `None` - no edit - for a caret in an ANONYMOUS block inside `scope`:
    /// its text is its container's inline run, not any node's content, and
    /// falling back to `scope` spliced the scope's whole flattened text at
    /// the caret and shaped it into the first block below (the paragraph
    /// beside the loose text).
    ///
    /// Keying an edit to the focused host stored one flattened blob of every
    /// paragraph, spliced at per-block cursor indices; keying it to the
    /// nearest boxed element above a text LEAF picked a `<b>` whose one run
    /// the caret's run 1 missed. The block is the only key the caret agrees
    /// with.
    #[must_use]
    pub fn edit_element(&self, scope: DomNodeId, caret: Option<TextBlock>) -> Option<NodeId> {
        let scope_node = scope.node.into_crate_internal()?;
        let in_scope = caret.filter(|block| {
            block.dom() == scope.dom
                && self.node_is_self_or_descendant(scope.dom, block.first_node(), scope_node)
        });
        match in_scope {
            Some(block) => block.element(),
            None => Some(scope_node),
        }
    }

    /// THE resolver: the text block that holds `node`'s text.
    ///
    /// `node` may be the block's own element, a text leaf in it, an inline
    /// element in it, or a node that generated no box of its own (an empty
    /// text node is filtered out of the layout tree; it belongs to the block
    /// of its nearest boxed ancestor). `None` for a node whose box is not part
    /// of any inline formatting context - a block container such as an
    /// editing host whose text sits in paragraphs (see
    /// `LayoutTree::owning_ifc_root`) - and for a node this window has not laid
    /// out.
    #[must_use]
    pub fn text_block_of(&self, node: DomNodeId) -> Option<TextBlock> {
        let node_id = node.node.into_crate_internal()?;
        let layout_result = self.layout_results.get(&node.dom)?;
        let tree = &layout_result.layout_tree;
        let hierarchy = layout_result.styled_dom.node_hierarchy.as_container();
        let mut current = Some(node_id);
        for _ in 0..BOXLESS_ANCESTOR_WALK_LIMIT {
            let n = current?;
            if let Some(boxes) = tree.dom_to_layout.get(&n) {
                // A list item's `::marker` box names the item too; the item's
                // text is in its PRINCIPAL box's block.
                return boxes.iter().find_map(|&idx| {
                    let is_pseudo = tree
                        .warm(idx)
                        .is_some_and(|w| w.pseudo_element.is_some());
                    if is_pseudo {
                        return None;
                    }
                    let root = tree.owning_ifc_root(idx.index())?;
                    tree.text_block_at(node.dom, root)
                });
            }
            current = hierarchy.get(n).and_then(NodeHierarchyItem::parent_id);
        }
        None
    }

    /// The layout node that owns `block`'s inline layout, in its own DOM's
    /// tree. `None` when the block is not (or no longer) laid out.
    #[must_use]
    pub fn text_block_layout_index(&self, block: TextBlock) -> Option<LayoutNodeId> {
        self.layout_results
            .get(&block.dom())?
            .layout_tree
            .text_block_root(block.key())
            .map(LayoutNodeId::new)
    }

    /// THE document-order walk: every text block of `dom_id` that `filter`
    /// lets through, with the layout node that owns its inline layout.
    ///
    /// The layout tree is built in pre-order, so its own order IS document
    /// order - the same order `TextBlock`'s `Ord` gives.
    #[must_use]
    pub fn text_block_roots(
        &self,
        dom_id: DomId,
        filter: BlockFilter,
    ) -> Vec<(TextBlock, LayoutNodeId)> {
        let Some(layout_result) = self.layout_results.get(&dom_id) else {
            return Vec::new();
        };
        let scope = match filter.within {
            None => None,
            Some(within) if within.dom == dom_id => match within.node.into_crate_internal() {
                Some(node) => Some(node),
                None => return Vec::new(),
            },
            // A scope in another DOM holds nothing of this one.
            Some(_) => return Vec::new(),
        };
        let tree = &layout_result.layout_tree;
        (0..tree.nodes.len())
            .filter_map(|idx| Some((tree.text_block_at(dom_id, idx)?, LayoutNodeId::new(idx))))
            .filter(|(block, _)| {
                scope.is_none_or(|scope| {
                    self.node_is_self_or_descendant(dom_id, block.first_node(), scope)
                })
            })
            .filter(|(block, _)| {
                !filter.selectable_only || block_is_selectable(&layout_result.styled_dom, *block)
            })
            .collect()
    }

    /// The blocks a selection anchored in `anchor` may extend over: selectable
    /// text, and - when the anchor is inside an editing host - only that
    /// host's (a drag that starts in a text field stays in it).
    #[must_use]
    pub fn selection_extent(&self, anchor: TextBlock) -> BlockFilter {
        BlockFilter {
            within: self
                .find_contenteditable_host(anchor.container_dom_node())
                .map(EditHost::dom_node),
            ..BlockFilter::SELECTABLE
        }
    }

    /// The first and the last text block Ctrl+A covers for a focus on `node`:
    /// the selectable blocks of its editing host (a focus outside any host is
    /// its own root), from the document walk - anonymous blocks included.
    ///
    /// The last is the last OUTERMOST block: one nested in another (an
    /// inline-block's, inside its paragraph) ends inside that one, so it
    /// never ends the host. A host that holds no block of its own - an inline
    /// editable inside a paragraph - selects in the block its text is in.
    #[must_use]
    pub fn select_all_extent(&self, node: DomNodeId) -> Option<(TextBlock, TextBlock)> {
        let root = self
            .find_contenteditable_host(node)
            .map_or(node, EditHost::dom_node);
        let roots = self.text_block_roots(
            node.dom,
            BlockFilter {
                within: Some(root),
                ..BlockFilter::SELECTABLE
            },
        );
        let Some(&(first, _)) = roots.first() else {
            let block = self
                .text_block_of(root)
                .filter(|block| self.text_target(*block).is_some_and(|t| t.selectable))?;
            return Some((block, block));
        };
        let tree = &self.layout_results.get(&node.dom)?.layout_tree;
        let indices: BTreeSet<usize> = roots.iter().map(|(_, idx)| idx.index()).collect();
        let nested = |index: usize| {
            let mut current = tree.nodes.get(index).and_then(|n| n.parent);
            while let Some(parent) = current {
                if indices.contains(&parent) {
                    return true;
                }
                current = tree.nodes.get(parent).and_then(|n| n.parent);
            }
            false
        };
        let (last, _) = *roots.iter().rev().find(|(_, idx)| !nested(idx.index()))?;
        Some((first, last))
    }

    /// [`Self::text_block_roots`] without the layout nodes.
    #[must_use]
    pub fn text_blocks(&self, dom_id: DomId, filter: BlockFilter) -> Vec<TextBlock> {
        self.text_block_roots(dom_id, filter)
            .into_iter()
            .map(|(block, _)| block)
            .collect()
    }

    /// The text blocks inside `node`'s subtree - its own block included, when
    /// it is one - in document order.
    #[must_use]
    pub fn text_blocks_within(&self, node: DomNodeId) -> Vec<TextBlock> {
        self.text_blocks(
            node.dom,
            BlockFilter {
                within: Some(node),
                ..BlockFilter::ALL
            },
        )
    }

    /// The text block an app-facing call naming `node` means: the block
    /// `node`'s text is in, or - for a container of blocks - the first block
    /// inside it.
    #[must_use]
    pub fn text_block_named_by(&self, node: DomNodeId) -> Option<TextBlock> {
        self.text_block_of(node)
            .or_else(|| self.text_blocks_within(node).first().copied())
    }

    /// Open an editing session with the caret at `cursor`, in the text block
    /// `node` names ([`Self::text_block_named_by`]) - the node-level entry to
    /// `TextEditManager::initialize_editing`, which takes the block itself.
    /// `false` (and nothing changes) when `node` names no laid-out block.
    pub fn start_editing_at(
        &mut self,
        cursor: TextCursor,
        dom: DomId,
        node: NodeId,
        contenteditable_key: u64,
    ) -> bool {
        let Some(block) = self.text_block_named_by(DomNodeId {
            dom,
            node: NodeHierarchyItemId::from_crate_internal(Some(node)),
        }) else {
            return false;
        };
        self.text_edit_manager
            .initialize_editing(cursor, block, contenteditable_key);
        true
    }

    /// Open the editing session on `block` with `range` selected - its
    /// `start` the anchor, its `end` the focus; a collapsed range is a caret.
    ///
    /// The one way a session opens on a block the caller has resolved: keyed
    /// on the block's editing host, so the same editable carries the same
    /// identity however it was opened, and in the focusable the caret now sits
    /// in, so a caret crossing into another field jumps rather than glides
    /// ([`TextEditManager::enter_focus_scope`]). It replaces any previous
    /// session and ends a document selection.
    ///
    /// [`TextEditManager::enter_focus_scope`]: crate::managers::text_edit::TextEditManager::enter_focus_scope
    pub fn open_session(&mut self, block: TextBlock, range: SelectionRange) {
        let key = self.contenteditable_session_key(block.dom(), block.container());
        let scope = self.find_focusable_ancestor(block.container_dom_node());
        self.text_edit_manager.enter_focus_scope(scope);
        self.text_edit_manager
            .initialize_editing(range.start, block, key);
        if range.start != range.end {
            if let Some(mc) = self.text_edit_manager.multi_cursor.as_mut() {
                mc.set_single_range(range);
            }
        }
    }

    /// Whether an app call naming `node` means the editing session's block:
    /// `node`'s text is in it, or the block lies inside `node` (its host, a
    /// container of blocks).
    fn names_session_block(&self, node: DomNodeId) -> bool {
        let Some(block) = self.text_edit_manager.get_editing_block() else {
            return false;
        };
        block.dom() == node.dom
            && (self.text_block_of(node) == Some(block)
                || self.text_blocks_within(node).contains(&block))
    }

    /// The app's `AddCursor` (`CallbackChange::AddCursor`): a caret at `cursor`
    /// in the block `node` names.
    ///
    /// Added to the editing session when `node` means the session's block
    /// ([`Self::names_session_block`]). Otherwise the session opens in the
    /// block `node` names ([`Self::text_block_named_by`]), as a click opens
    /// it: `cursor` indexes THAT block, and adding it to another block's
    /// session put a caret at an unrelated place there. Returns whether
    /// anything changed.
    pub fn add_app_cursor(&mut self, node: DomNodeId, cursor: TextCursor) -> bool {
        self.add_app_selection(
            node,
            SelectionRange {
                start: cursor,
                end: cursor,
            },
            |mc| {
                let _ = mc.add_cursor(cursor);
            },
        )
    }

    /// The app's `AddSelectionRange` (`CallbackChange::AddSelectionRange`):
    /// [`Self::add_app_cursor`] for a range.
    pub fn add_app_selection_range(&mut self, node: DomNodeId, range: SelectionRange) -> bool {
        self.add_app_selection(node, range, |mc| {
            let _ = mc.add_selection(range);
        })
    }

    /// [`Self::add_app_cursor`] / [`Self::add_app_selection_range`]: `add`
    /// into the session `node` means, else a session opened on `node`'s block
    /// with `range`.
    fn add_app_selection(
        &mut self,
        node: DomNodeId,
        range: SelectionRange,
        add: impl FnOnce(&mut MultiCursorState),
    ) -> bool {
        if self.names_session_block(node) {
            if let Some(mc) = self.text_edit_manager.multi_cursor.as_mut() {
                add(mc);
            }
            self.text_edit_manager.mark_dirty();
            return true;
        }
        let Some(block) = self.text_block_named_by(node) else {
            return false;
        };
        self.open_session(block, range);
        true
    }

    /// The app's `SetSelection` and `SetSelectAllRange`
    /// (`CallbackChange::SetSelection`, `CallbackChange::SetSelectAllRange`):
    /// `selection` as the editing session's one selection, for both hosts.
    /// Returns whether there was a session to set it in.
    pub fn set_app_selection(&mut self, node: DomNodeId, selection: Selection) -> bool {
        let _ = node;
        let Some(mc) = self.text_edit_manager.multi_cursor.as_mut() else {
            return false;
        };
        match selection {
            Selection::Cursor(cursor) => mc.set_single_cursor(cursor),
            Selection::Range(range) => mc.set_single_range(range),
        }
        true
    }

    /// The app's caret moves - `CallbackChange::MoveCursorLeft`, `Right`,
    /// `Up`, `Down`, `ToLineStart`, `ToLineEnd`, `ToDocumentStart`,
    /// `ToDocumentEnd` - for both hosts (the desktop event loop and the E2E
    /// runner). `false` when `change` is none of them.
    ///
    /// Each is the key it is named after (arrow, Home/End, Ctrl+Home/End,
    /// Shift for `extend_selection`), so it runs through the keyboard's own
    /// path, [`Self::apply_selection_op`], as if the key went to the node:
    /// the block it acts in is resolved there ([`Self::keyboard_text_target`]
    /// - the session's block inside the node), with its materialized layout.
    /// Reading the node's STORED layout instead made every one a no-op under
    /// dense text (the empty retirement sentinel), and one naming a field's
    /// host found no layout at all.
    pub fn apply_app_cursor_move(&mut self, change: &CallbackChange) -> bool {
        use azul_core::events::{SelectionDirection, SelectionMode, SelectionOp, SelectionStep};

        let (dom_id, node_id, extend_selection, direction, step) = match change {
            CallbackChange::MoveCursorLeft {
                dom_id,
                node_id,
                extend_selection,
            } => (
                dom_id,
                node_id,
                extend_selection,
                SelectionDirection::Backward,
                SelectionStep::Character,
            ),
            CallbackChange::MoveCursorRight {
                dom_id,
                node_id,
                extend_selection,
            } => (
                dom_id,
                node_id,
                extend_selection,
                SelectionDirection::Forward,
                SelectionStep::Character,
            ),
            CallbackChange::MoveCursorUp {
                dom_id,
                node_id,
                extend_selection,
            } => (
                dom_id,
                node_id,
                extend_selection,
                SelectionDirection::Backward,
                SelectionStep::VisualLine,
            ),
            CallbackChange::MoveCursorDown {
                dom_id,
                node_id,
                extend_selection,
            } => (
                dom_id,
                node_id,
                extend_selection,
                SelectionDirection::Forward,
                SelectionStep::VisualLine,
            ),
            CallbackChange::MoveCursorToLineStart {
                dom_id,
                node_id,
                extend_selection,
            } => (
                dom_id,
                node_id,
                extend_selection,
                SelectionDirection::Backward,
                SelectionStep::Line,
            ),
            CallbackChange::MoveCursorToLineEnd {
                dom_id,
                node_id,
                extend_selection,
            } => (
                dom_id,
                node_id,
                extend_selection,
                SelectionDirection::Forward,
                SelectionStep::Line,
            ),
            CallbackChange::MoveCursorToDocumentStart {
                dom_id,
                node_id,
                extend_selection,
            } => (
                dom_id,
                node_id,
                extend_selection,
                SelectionDirection::Backward,
                SelectionStep::Document,
            ),
            CallbackChange::MoveCursorToDocumentEnd {
                dom_id,
                node_id,
                extend_selection,
            } => (
                dom_id,
                node_id,
                extend_selection,
                SelectionDirection::Forward,
                SelectionStep::Document,
            ),
            _ => return false,
        };
        let mode = if *extend_selection {
            SelectionMode::Extend
        } else {
            SelectionMode::Move
        };
        let node = DomNodeId {
            dom: *dom_id,
            node: NodeHierarchyItemId::from_crate_internal(Some(*node_id)),
        };
        let _ = self.apply_selection_op(node, &SelectionOp::new(direction, step, mode));
        true
    }

    /// The text block whose IFC root is the layout node `ifc_root` of `dom`.
    #[must_use]
    pub fn text_block_at_layout_index(
        &self,
        dom: DomId,
        ifc_root: LayoutNodeId,
    ) -> Option<TextBlock> {
        self.layout_results
            .get(&dom)?
            .layout_tree
            .text_block_at(dom, ifc_root.index())
    }
}
