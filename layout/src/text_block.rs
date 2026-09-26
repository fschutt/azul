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
//! [`LayoutTree::owning_ifc_root`]: crate::solver3::layout_tree::LayoutTree::owning_ifc_root
//! [`LayoutTree::text_block_at`]: crate::solver3::layout_tree::LayoutTree::text_block_at

use azul_core::{
    dom::{DomId, DomNodeId},
    selection::{MultiCursorState, SelectionRange, TextBlock, TextCursor},
    styled_dom::NodeHierarchyItem,
};

use crate::{solver3::layout_tree::LayoutNodeId, window::LayoutWindow};

/// How far [`LayoutWindow::text_block_of`] walks up the DOM from a node that
/// generated no box before giving up.
const BOXLESS_ANCESTOR_WALK_LIMIT: usize = 64;

impl LayoutWindow {
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

    /// Every text block of `dom_id`, in DOCUMENT order.
    ///
    /// The layout tree is built in pre-order, so its own order IS document
    /// order. Anonymous blocks are left out (nothing selects in them yet).
    #[must_use]
    pub fn text_blocks_in_document_order(&self, dom_id: DomId) -> Vec<TextBlock> {
        let Some(layout_result) = self.layout_results.get(&dom_id) else {
            return Vec::new();
        };
        let tree = &layout_result.layout_tree;
        (0..tree.nodes.len())
            .filter_map(|idx| tree.text_block_at(dom_id, idx))
            .filter(|block| !block.is_anonymous())
            .collect()
    }

    /// The text blocks inside `node`'s subtree - its own block included, when
    /// it is one - in document order.
    #[must_use]
    pub fn text_blocks_within(&self, node: DomNodeId) -> Vec<TextBlock> {
        let Some(node_id) = node.node.into_crate_internal() else {
            return Vec::new();
        };
        self.text_blocks_in_document_order(node.dom)
            .into_iter()
            .filter(|block| self.node_is_self_or_descendant(node.dom, block.first_node(), node_id))
            .collect()
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
        node: azul_core::dom::NodeId,
        contenteditable_key: u64,
    ) -> bool {
        let Some(block) = self.text_block_named_by(DomNodeId {
            dom,
            node: azul_core::styled_dom::NodeHierarchyItemId::from_crate_internal(Some(node)),
        }) else {
            return false;
        };
        self.text_edit_manager
            .initialize_editing(cursor, block, contenteditable_key);
        true
    }

    /// The app's `AddCursor` (`CallbackChange::AddCursor`): a caret at `cursor`.
    /// Added to the editing session when there is one; otherwise a session
    /// opens in the block `node` names ([`Self::text_block_named_by`]).
    /// Returns whether anything changed.
    pub fn add_app_cursor(&mut self, node: DomNodeId, cursor: TextCursor) -> bool {
        if let Some(mc) = self.text_edit_manager.multi_cursor.as_mut() {
            let _ = mc.add_cursor(cursor);
        } else {
            let Some(block) = self.text_block_named_by(node) else {
                return false;
            };
            self.text_edit_manager.multi_cursor =
                Some(MultiCursorState::new_with_cursor(cursor, block, 0));
        }
        self.text_edit_manager.mark_dirty();
        true
    }

    /// The app's `AddSelectionRange` (`CallbackChange::AddSelectionRange`):
    /// [`Self::add_app_cursor`] for a range.
    pub fn add_app_selection_range(&mut self, node: DomNodeId, range: SelectionRange) -> bool {
        if let Some(mc) = self.text_edit_manager.multi_cursor.as_mut() {
            let _ = mc.add_selection(range);
        } else {
            let Some(block) = self.text_block_named_by(node) else {
                return false;
            };
            let mut mc = MultiCursorState::new_with_cursor(range.start, block, 0);
            mc.set_single_range(range);
            self.text_edit_manager.multi_cursor = Some(mc);
        }
        self.text_edit_manager.mark_dirty();
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
