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
    selection::TextBlock,
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
