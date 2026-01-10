//! solver3/layout_tree.rs
//!
//! Layout tree generation and anonymous box handling
use std::{
    collections::BTreeMap,
    hash::{Hash, Hasher},
    sync::Arc,
};

use azul_core::{
    dom::{FormattingContext, NodeId, NodeType},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    styled_dom::StyledDom,
};
use azul_css::{
    corety::LayoutDebugMessage,
    css::CssPropertyValue,
    format_rust_code::GetHash,
    props::{
        basic::{
            pixel::DEFAULT_FONT_SIZE, PhysicalSize, PixelValue, PropertyContext, ResolutionContext,
        },
        layout::{LayoutDisplay, LayoutFloat, LayoutOverflow, LayoutPosition},
        property::{CssProperty, CssPropertyType},
    },
};
use taffy::{Cache as TaffyCache, Layout, LayoutInput, LayoutOutput};

#[cfg(feature = "text_layout")]
use crate::text3;
use crate::{
    debug_log,
    font::parsed::ParsedFont,
    font_traits::{FontLoaderTrait, ParsedFontTrait, UnifiedLayout},
    solver3::{
        geometry::{BoxProps, IntrinsicSizes, PositionedRectangle},
        getters::{get_float, get_overflow_x, get_overflow_y, get_position},
        scrollbar::ScrollbarRequirements,
        LayoutContext, Result,
    },
    text3::cache::AvailableSpace,
};

/// Represents the invalidation state of a layout node.
///
/// The states are ordered by severity, allowing for easy "upgrading" of the dirty state.
/// A node marked for `Layout` does not also need to be marked for `Paint`.
///
/// Because this enum derives `PartialOrd` and `Ord`, you can directly compare variants:
///
/// - `DirtyFlag::Layout > DirtyFlag::Paint` is `true`
/// - `DirtyFlag::Paint >= DirtyFlag::None` is `true`
/// - `DirtyFlag::Paint < DirtyFlag::Layout` is `true`
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum DirtyFlag {
    /// The node's layout is valid and no repaint is needed. This is the "clean" state.
    #[default]
    None,
    /// The node's geometry is valid, but its appearance (e.g., color) has changed.
    /// Requires a display list update only.
    Paint,
    /// The node's geometry (size or position) is invalid.
    /// Requires a full layout pass and a display list update.
    Layout,
}

/// A hash that represents the content and style of a node PLUS all of its descendants.
/// If two SubtreeHashes are equal, their entire subtrees are considered identical for layout
/// purposes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct SubtreeHash(pub u64);

/// Cached inline layout result with the constraints used to compute it.
///
/// This structure solves a fundamental architectural problem: inline layouts
/// (text wrapping, inline-block positioning) depend on the available width.
/// Different layout phases may compute the layout with different widths:
///
/// 1. **Min-content measurement**: width = MinContent (effectively 0)
/// 2. **Max-content measurement**: width = MaxContent (effectively infinite)
/// 3. **Final layout**: width = Definite(actual_column_width)
///
/// Without tracking which constraints were used, a cached result from phase 1
/// would incorrectly be reused in phase 3, causing text to wrap at the wrong
/// positions (the root cause of table cell width bugs).
///
/// By storing the constraints alongside the result, we can:
/// - Invalidate the cache when constraints change
/// - Keep multiple cached results for different constraint types if needed
/// - Ensure the final render always uses a layout computed with correct widths
#[derive(Debug, Clone)]
pub struct CachedInlineLayout {
    /// The computed inline layout
    pub layout: Arc<UnifiedLayout>,
    /// The available width constraint used to compute this layout.
    /// This is the key for cache validity checking.
    pub available_width: AvailableSpace,
    /// Whether this layout was computed with float exclusions.
    /// Float-aware layouts should not be overwritten by non-float layouts.
    pub has_floats: bool,
}

impl CachedInlineLayout {
    /// Creates a new cached inline layout.
    pub fn new(
        layout: Arc<UnifiedLayout>,
        available_width: AvailableSpace,
        has_floats: bool,
    ) -> Self {
        Self {
            layout,
            available_width,
            has_floats,
        }
    }

    /// Checks if this cached layout is valid for the given constraints.
    ///
    /// A cached layout is valid if:
    /// 1. The available width matches (definite widths must be equal, or both are the same
    ///    indefinite type)
    /// 2. OR the new request doesn't have floats but the cached one does (keep float-aware layout)
    ///
    /// The second condition preserves float-aware layouts, which are more "correct" than
    /// non-float layouts and shouldn't be overwritten.
    pub fn is_valid_for(&self, new_width: AvailableSpace, new_has_floats: bool) -> bool {
        // If we have a float-aware layout and the new request doesn't have floats,
        // keep the float-aware layout (it's more accurate)
        if self.has_floats && !new_has_floats {
            // But only if the width constraint type matches
            return self.width_constraint_matches(new_width);
        }

        // Otherwise, require exact width match
        self.width_constraint_matches(new_width)
    }

    /// Checks if the width constraint matches.
    fn width_constraint_matches(&self, new_width: AvailableSpace) -> bool {
        match (self.available_width, new_width) {
            // Definite widths must match within a small epsilon
            (AvailableSpace::Definite(old), AvailableSpace::Definite(new)) => {
                (old - new).abs() < 0.1
            }
            // MinContent matches MinContent
            (AvailableSpace::MinContent, AvailableSpace::MinContent) => true,
            // MaxContent matches MaxContent
            (AvailableSpace::MaxContent, AvailableSpace::MaxContent) => true,
            // Different constraint types don't match
            _ => false,
        }
    }

    /// Determines if this cached layout should be replaced by a new layout.
    ///
    /// Returns true if the new layout should replace this one.
    pub fn should_replace_with(&self, new_width: AvailableSpace, new_has_floats: bool) -> bool {
        // Always replace if we gain float information
        if new_has_floats && !self.has_floats {
            return true;
        }

        // Replace if width constraint changed
        !self.width_constraint_matches(new_width)
    }

    /// Returns a reference to the inner UnifiedLayout.
    ///
    /// This is a convenience method for code that only needs the layout data
    /// and doesn't care about the caching metadata.
    #[inline]
    pub fn get_layout(&self) -> &Arc<UnifiedLayout> {
        &self.layout
    }

    /// Returns a clone of the inner Arc<UnifiedLayout>.
    ///
    /// This is useful for APIs that need to return an owned reference
    /// to the layout without exposing the caching metadata.
    #[inline]
    pub fn clone_layout(&self) -> Arc<UnifiedLayout> {
        self.layout.clone()
    }
}

/// A layout tree node representing the CSS box model
///
/// Note: An absolute position is a final paint-time value and shouldn't be
/// cached on the node itself, as it can change even if the node's
/// layout is clean (e.g., if a sibling changes size). We will calculate
/// it in a separate map.
#[derive(Debug, Clone)]
pub struct LayoutNode {
    /// Reference back to the original DOM node (None for anonymous boxes)
    pub dom_node_id: Option<NodeId>,
    /// Pseudo-element type (::marker, ::before, ::after) if this node is a pseudo-element
    pub pseudo_element: Option<PseudoElement>,
    /// Whether this is an anonymous box generated by the layout engine
    pub is_anonymous: bool,
    /// Type of anonymous box (if applicable)
    pub anonymous_type: Option<AnonymousBoxType>,
    /// Children indices in the layout tree
    pub children: Vec<usize>,
    /// Parent index (None for root)
    pub parent: Option<usize>,
    /// Dirty flags to track what needs recalculation.
    pub dirty_flag: DirtyFlag,
    /// The resolved box model properties (margin, border, padding)
    /// in logical pixels.
    pub box_props: BoxProps,
    /// Cache for Taffy layout computations for this node.
    pub taffy_cache: TaffyCache, // NEW FIELD
    /// A hash of this node's data (style, text content, etc.) used for
    /// fast reconciliation.
    pub node_data_hash: u64,
    /// A hash of this node's data and all of its descendants. Used for
    /// fast reconciliation.
    pub subtree_hash: SubtreeHash,
    /// The formatting context this node establishes or participates in.
    pub formatting_context: FormattingContext,
    /// Parent's formatting context (needed to determine if stretch applies)
    pub parent_formatting_context: Option<FormattingContext>,
    /// Cached intrinsic sizes (min-content, max-content, etc.)
    pub intrinsic_sizes: Option<IntrinsicSizes>,
    /// The size used during the last layout pass.
    pub used_size: Option<LogicalSize>,
    /// The position of this node *relative to its parent's content box*.
    pub relative_position: Option<LogicalPosition>,
    /// The baseline of this box, if applicable, measured from its content-box top edge.
    pub baseline: Option<f32>,
    /// Cached inline layout result with the constraints used to compute it.
    ///
    /// This field stores both the computed layout AND the constraints (available width,
    /// float state) under which it was computed. This is essential for correctness:
    /// - Table cells are measured multiple times with different widths
    /// - Min-content/max-content intrinsic sizing uses special constraint values
    /// - The final layout must use the actual available width, not a measurement width
    ///
    /// By tracking the constraints, we avoid the bug where a min-content measurement
    /// (with width=0) would be incorrectly reused for final rendering.
    pub inline_layout_result: Option<CachedInlineLayout>,
    /// Escaped top margin (CSS 2.1 margin collapsing)
    /// If this BFC's first child's top margin "escaped" the BFC, this contains
    /// the collapsed margin that should be applied by the parent.
    pub escaped_top_margin: Option<f32>,
    /// Escaped bottom margin (CSS 2.1 margin collapsing)  
    /// If this BFC's last child's bottom margin "escaped" the BFC, this contains
    /// the collapsed margin that should be applied by the parent.
    pub escaped_bottom_margin: Option<f32>,
    /// Cached scrollbar information (calculated during layout)
    /// Used to determine if scrollbars appeared/disappeared requiring reflow
    pub scrollbar_info: Option<ScrollbarRequirements>,
    /// The actual content size (children overflow size) for scrollable containers.
    /// This is the size of all content that might need to be scrolled, which can
    /// be larger than `used_size` when content overflows the container.
    pub overflow_content_size: Option<LogicalSize>,
}

impl LayoutNode {
    /// Calculates the actual content size of this node, including all children and text.
    /// This is used to determine if scrollbars should appear for overflow: auto.
    pub fn get_content_size(&self) -> LogicalSize {
        // First, check if we have overflow_content_size from layout computation
        if let Some(content_size) = self.overflow_content_size {
            return content_size;
        }

        // Fall back to computing from used_size and text layout
        let mut content_size = self.used_size.unwrap_or_default();

        // If this node has text layout, calculate the bounds of all text items
        if let Some(ref cached_layout) = self.inline_layout_result {
            let text_layout = &cached_layout.layout;
            // Find the maximum extent of all positioned items
            let mut max_x: f32 = 0.0;
            let mut max_y: f32 = 0.0;

            for positioned_item in &text_layout.items {
                let item_bounds = positioned_item.item.bounds();
                let item_right = positioned_item.position.x + item_bounds.width;
                let item_bottom = positioned_item.position.y + item_bounds.height;

                max_x = max_x.max(item_right);
                max_y = max_y.max(item_bottom);
            }

            // Use the maximum extent as content size if it's larger
            content_size.width = content_size.width.max(max_x);
            content_size.height = content_size.height.max(max_y);
        }

        // TODO: Also check children positions to get max content bounds
        // For now, this handles the most common case (text overflowing)

        content_size
    }
}

/// CSS pseudo-elements that can be generated
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PseudoElement {
    /// ::marker pseudo-element for list items
    Marker,
    /// ::before pseudo-element
    Before,
    /// ::after pseudo-element
    After,
}

/// Types of anonymous boxes that can be generated
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AnonymousBoxType {
    /// Anonymous block box wrapping inline content
    InlineWrapper,
    /// Anonymous box for a list item marker (bullet or number)
    /// DEPRECATED: Use PseudoElement::Marker instead
    ListItemMarker,
    /// Anonymous table wrapper
    TableWrapper,
    /// Anonymous table row group (tbody)
    TableRowGroup,
    /// Anonymous table row
    TableRow,
    /// Anonymous table cell
    TableCell,
}

/// The complete layout tree structure
#[derive(Debug, Clone)]
pub struct LayoutTree {
    /// Arena-style storage for layout nodes
    pub nodes: Vec<LayoutNode>,
    /// Root node index
    pub root: usize,
    /// Mapping from DOM node IDs to layout node indices
    pub dom_to_layout: BTreeMap<NodeId, Vec<usize>>,
}

impl LayoutTree {
    pub fn get(&self, index: usize) -> Option<&LayoutNode> {
        self.nodes.get(index)
    }

    pub fn get_mut(&mut self, index: usize) -> Option<&mut LayoutNode> {
        self.nodes.get_mut(index)
    }

    pub fn root_node(&self) -> &LayoutNode {
        &self.nodes[self.root]
    }

    /// Marks a node and its ancestors as dirty with the given flag.
    ///
    /// The dirty state is "upgraded" if the new flag is more severe than the
    /// existing one (e.g., upgrading from `Paint` to `Layout`). Propagation stops
    /// if an ancestor is already marked with an equal or more severe flag.
    pub fn mark_dirty(&mut self, start_index: usize, flag: DirtyFlag) {
        // A "None" flag is a no-op for marking dirty.
        if flag == DirtyFlag::None {
            return;
        }

        let mut current_index = Some(start_index);
        while let Some(index) = current_index {
            if let Some(node) = self.get_mut(index) {
                // If the node's current flag is already as dirty or dirtier,
                // then all ancestors are also sufficiently marked, so we can stop.
                if node.dirty_flag >= flag {
                    break;
                }

                // Upgrade the flag to the new, more severe state.
                node.dirty_flag = flag;
                current_index = node.parent;
            } else {
                break;
            }
        }
    }

    /// Marks a node and its entire subtree of descendants with the given dirty flag.
    ///
    /// This is used for inherited CSS properties. Each node in the subtree
    /// will be upgraded to at least the new flag's severity.
    pub fn mark_subtree_dirty(&mut self, start_index: usize, flag: DirtyFlag) {
        // A "None" flag is a no-op.
        if flag == DirtyFlag::None {
            return;
        }

        // Using a stack for an iterative traversal to avoid deep recursion
        // on large subtrees.
        let mut stack = vec![start_index];
        while let Some(index) = stack.pop() {
            if let Some(node) = self.get_mut(index) {
                // Only update if the new flag is an upgrade.
                if node.dirty_flag < flag {
                    node.dirty_flag = flag;
                }
                // Add all children to be processed.
                stack.extend_from_slice(&node.children);
            }
        }
    }

    /// Resets the dirty flags of all nodes in the tree to `None` after layout is complete.
    pub fn clear_all_dirty_flags(&mut self) {
        for node in &mut self.nodes {
            node.dirty_flag = DirtyFlag::None;
        }
    }
}

/// Generate layout tree from styled DOM with proper anonymous box generation
pub fn generate_layout_tree<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
) -> Result<LayoutTree> {
    let mut builder = LayoutTreeBuilder::new();
    let root_id = ctx
        .styled_dom
        .root
        .into_crate_internal()
        .unwrap_or(NodeId::ZERO);
    let root_index =
        builder.process_node(ctx.styled_dom, root_id, None, &mut ctx.debug_messages)?;
    let layout_tree = builder.build(root_index);

    debug_log!(
        ctx,
        "Generated layout tree with {} nodes (incl. anonymous)",
        layout_tree.nodes.len()
    );

    Ok(layout_tree)
}

pub struct LayoutTreeBuilder {
    nodes: Vec<LayoutNode>,
    dom_to_layout: BTreeMap<NodeId, Vec<usize>>,
}

impl LayoutTreeBuilder {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            dom_to_layout: BTreeMap::new(),
        }
    }

    pub fn get(&self, index: usize) -> Option<&LayoutNode> {
        self.nodes.get(index)
    }

    pub fn get_mut(&mut self, index: usize) -> Option<&mut LayoutNode> {
        self.nodes.get_mut(index)
    }

    /// Main entry point for recursively building the layout tree.
    /// This function dispatches to specialized handlers based on the node's
    /// `display` property to correctly generate anonymous boxes.
    pub fn process_node(
        &mut self,
        styled_dom: &StyledDom,
        dom_id: NodeId,
        parent_idx: Option<usize>,
        debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    ) -> Result<usize> {
        let node_data = &styled_dom.node_data.as_container()[dom_id];
        let node_idx = self.create_node_from_dom(styled_dom, dom_id, parent_idx, debug_messages)?;
        let display_type = get_display_type(styled_dom, dom_id);

        // If this is a list-item, inject a ::marker pseudo-element as its first child
        // Per CSS spec, the ::marker is generated as the first child of the list-item
        if display_type == LayoutDisplay::ListItem {
            self.create_marker_pseudo_element(styled_dom, dom_id, node_idx);
        }

        match display_type {
            LayoutDisplay::Block
            | LayoutDisplay::InlineBlock
            | LayoutDisplay::FlowRoot
            | LayoutDisplay::ListItem => {
                self.process_block_children(styled_dom, dom_id, node_idx, debug_messages)?
            }
            LayoutDisplay::Table => {
                self.process_table_children(styled_dom, dom_id, node_idx, debug_messages)?
            }
            LayoutDisplay::TableRowGroup => {
                self.process_table_row_group_children(styled_dom, dom_id, node_idx, debug_messages)?
            }
            LayoutDisplay::TableRow => {
                self.process_table_row_children(styled_dom, dom_id, node_idx, debug_messages)?
            }
            // Inline, TableCell, etc., have their children processed as part of their
            // formatting context layout and don't require anonymous box generation at this stage.
            _ => {
                let children: Vec<NodeId> = dom_id
                    .az_children(&styled_dom.node_hierarchy.as_container())
                    .collect();

                for child_dom_id in children {
                    self.process_node(styled_dom, child_dom_id, Some(node_idx), debug_messages)?;
                }
            }
        }
        Ok(node_idx)
    }

    /// Handles children of a block-level element, creating anonymous block
    /// wrappers for consecutive runs of inline-level children if necessary.
    fn process_block_children(
        &mut self,
        styled_dom: &StyledDom,
        parent_dom_id: NodeId,
        parent_idx: usize,
        debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    ) -> Result<()> {
        let children: Vec<NodeId> = parent_dom_id
            .az_children(&styled_dom.node_hierarchy.as_container())
            .collect();

        // Debug: log which children we found
        if let Some(msgs) = debug_messages.as_mut() {
            msgs.push(LayoutDebugMessage::info(format!(
                "[process_block_children] DOM node {} has {} children: {:?}",
                parent_dom_id.index(),
                children.len(),
                children.iter().map(|c| c.index()).collect::<Vec<_>>()
            )));
        }

        let has_block_child = children.iter().any(|&id| is_block_level(styled_dom, id));

        if let Some(msgs) = debug_messages.as_mut() {
            msgs.push(LayoutDebugMessage::info(format!(
                "[process_block_children] has_block_child={}, children display types: {:?}",
                has_block_child,
                children
                    .iter()
                    .map(|c| {
                        let dt = get_display_type(styled_dom, *c);
                        let is_block = is_block_level(styled_dom, *c);
                        format!("{}:{:?}(block={})", c.index(), dt, is_block)
                    })
                    .collect::<Vec<_>>()
            )));
        }

        if !has_block_child {
            // All children are inline, no anonymous boxes needed.
            if let Some(msgs) = debug_messages.as_mut() {
                msgs.push(LayoutDebugMessage::info(format!(
                    "[process_block_children] All inline, processing {} children directly",
                    children.len()
                )));
            }
            for child_id in children {
                self.process_node(styled_dom, child_id, Some(parent_idx), debug_messages)?;
            }
            return Ok(());
        }

        // Mixed block and inline content requires anonymous wrappers.
        let mut inline_run = Vec::new();

        for child_id in children {
            if is_block_level(styled_dom, child_id) {
                // End the current inline run
                if !inline_run.is_empty() {
                    if let Some(msgs) = debug_messages.as_mut() {
                        msgs.push(LayoutDebugMessage::info(format!(
                            "[process_block_children] Creating anon wrapper for inline run: {:?}",
                            inline_run
                                .iter()
                                .map(|c: &NodeId| c.index())
                                .collect::<Vec<_>>()
                        )));
                    }
                    let anon_idx = self.create_anonymous_node(
                        parent_idx,
                        AnonymousBoxType::InlineWrapper,
                        FormattingContext::Block {
                            // Anonymous wrappers are BFC roots
                            establishes_new_context: true,
                        },
                    );
                    for inline_child_id in inline_run.drain(..) {
                        self.process_node(
                            styled_dom,
                            inline_child_id,
                            Some(anon_idx),
                            debug_messages,
                        )?;
                    }
                }
                // Process the block-level child directly
                if let Some(msgs) = debug_messages.as_mut() {
                    msgs.push(LayoutDebugMessage::info(format!(
                        "[process_block_children] Processing block child DOM {}",
                        child_id.index()
                    )));
                }
                self.process_node(styled_dom, child_id, Some(parent_idx), debug_messages)?;
            } else {
                inline_run.push(child_id);
            }
        }
        // Process any remaining inline children at the end
        if !inline_run.is_empty() {
            if let Some(msgs) = debug_messages.as_mut() {
                msgs.push(LayoutDebugMessage::info(format!(
                    "[process_block_children] Creating anon wrapper for remaining inline run: {:?}",
                    inline_run.iter().map(|c| c.index()).collect::<Vec<_>>()
                )));
            }
            let anon_idx = self.create_anonymous_node(
                parent_idx,
                AnonymousBoxType::InlineWrapper,
                FormattingContext::Block {
                    establishes_new_context: true, // Anonymous wrappers are BFC roots
                },
            );
            for inline_child_id in inline_run {
                self.process_node(styled_dom, inline_child_id, Some(anon_idx), debug_messages)?;
            }
        }

        Ok(())
    }

    /// CSS 2.2 Section 17.2.1 - Anonymous box generation for tables:
    /// "Generate missing child wrappers. If a child C of a table-row parent P is not a
    /// table-cell, then generate an anonymous table-cell box around C and all consecutive
    /// siblings of C that are not table-cells."
    ///
    /// Handles children of a `display: table`, inserting anonymous `table-row`
    /// wrappers for any direct `table-cell` children.
    ///
    /// Per CSS 2.2 Section 17.2.1, Stage 2 & 3:
    /// - Stage 2: Wrap consecutive table-cell children in anonymous table-rows
    /// - Stage 1 (implemented here): Skip whitespace-only text nodes
    fn process_table_children(
        &mut self,
        styled_dom: &StyledDom,
        parent_dom_id: NodeId,
        parent_idx: usize,
        debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    ) -> Result<()> {
        let parent_display = get_display_type(styled_dom, parent_dom_id);
        let mut row_children = Vec::new();

        for child_id in parent_dom_id.az_children(&styled_dom.node_hierarchy.as_container()) {
            // CSS 2.2 Section 17.2.1, Stage 1: Skip whitespace-only text nodes
            // "Remove all irrelevant boxes. These are boxes that do not contain table-related
            // boxes and do not themselves have 'display' set to a table-related value."
            if should_skip_for_table_structure(styled_dom, child_id, parent_display) {
                continue;
            }

            let child_display = get_display_type(styled_dom, child_id);

            // CSS 2.2 Section 17.2.1, Stage 2:
            // "Generate missing child wrappers"
            if child_display == LayoutDisplay::TableCell {
                // Accumulate consecutive table-cell children
                row_children.push(child_id);
            } else {
                // CSS 2.2 Section 17.2.1, Stage 2:
                // If we have accumulated cells, wrap them in an anonymous table-row
                if !row_children.is_empty() {
                    let anon_row_idx = self.create_anonymous_node(
                        parent_idx,
                        AnonymousBoxType::TableRow,
                        FormattingContext::TableRow,
                    );

                    for cell_id in row_children.drain(..) {
                        self.process_node(styled_dom, cell_id, Some(anon_row_idx), debug_messages)?;
                    }
                }

                // Process non-cell child (could be row, row-group, caption, etc.)
                self.process_node(styled_dom, child_id, Some(parent_idx), debug_messages)?;
            }
        }

        // CSS 2.2 Section 17.2.1, Stage 2:
        // Flush any remaining accumulated cells
        if !row_children.is_empty() {
            let anon_row_idx = self.create_anonymous_node(
                parent_idx,
                AnonymousBoxType::TableRow,
                FormattingContext::TableRow,
            );

            for cell_id in row_children {
                self.process_node(styled_dom, cell_id, Some(anon_row_idx), debug_messages)?;
            }
        }

        Ok(())
    }

    /// CSS 2.2 Section 17.2.1 - Anonymous box generation:
    /// Handles children of a `display: table-row-group`, `table-header-group`,
    /// or `table-footer-group`, inserting anonymous `table-row` wrappers as needed.
    ///
    /// The logic is identical to process_table_children per CSS 2.2 Section 17.2.1:
    /// "If a child C of a table-row-group parent P is not a table-row, then generate
    /// an anonymous table-row box around C and all consecutive siblings of C that are
    /// not table-rows."
    fn process_table_row_group_children(
        &mut self,
        styled_dom: &StyledDom,
        parent_dom_id: NodeId,
        parent_idx: usize,
        debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    ) -> Result<()> {
        // CSS 2.2 Section 17.2.1: Row groups need the same anonymous box generation
        // as tables (wrapping consecutive non-row children in anonymous rows)
        self.process_table_children(styled_dom, parent_dom_id, parent_idx, debug_messages)
    }

    /// CSS 2.2 Section 17.2.1 - Anonymous box generation, Stage 2:
    /// "Generate missing child wrappers. If a child C of a table-row parent P is not a
    /// table-cell, then generate an anonymous table-cell box around C and all consecutive
    /// siblings of C that are not table-cells."
    ///
    /// Handles children of a `display: table-row`, inserting anonymous `table-cell` wrappers
    /// for any non-cell children.
    fn process_table_row_children(
        &mut self,
        styled_dom: &StyledDom,
        parent_dom_id: NodeId,
        parent_idx: usize,
        debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    ) -> Result<()> {
        let parent_display = get_display_type(styled_dom, parent_dom_id);

        for child_id in parent_dom_id.az_children(&styled_dom.node_hierarchy.as_container()) {
            // CSS 2.2 Section 17.2.1, Stage 1: Skip whitespace-only text nodes
            if should_skip_for_table_structure(styled_dom, child_id, parent_display) {
                continue;
            }

            let child_display = get_display_type(styled_dom, child_id);

            // CSS 2.2 Section 17.2.1, Stage 2:
            // "If a child C of a table-row parent P is not a table-cell, then generate
            // an anonymous table-cell box around C"
            if child_display == LayoutDisplay::TableCell {
                // Normal table cell - process directly
                self.process_node(styled_dom, child_id, Some(parent_idx), debug_messages)?;
            } else {
                // CSS 2.2 Section 17.2.1, Stage 2:
                // Non-cell child must be wrapped in an anonymous table-cell
                let anon_cell_idx = self.create_anonymous_node(
                    parent_idx,
                    AnonymousBoxType::TableCell,
                    FormattingContext::Block {
                        establishes_new_context: true,
                    },
                );

                self.process_node(styled_dom, child_id, Some(anon_cell_idx), debug_messages)?;
            }
        }

        Ok(())
    }
    /// CSS 2.2 Section 17.2.1 - Anonymous box generation:
    /// "In this process, inline-level boxes are wrapped in anonymous boxes as needed
    /// to satisfy the constraints of the table model."
    ///
    /// Helper to create an anonymous node in the tree.
    /// Anonymous boxes don't have a corresponding DOM node and are used to enforce
    /// the CSS box model structure (e.g., wrapping inline content in blocks,
    /// or creating missing table structural elements).
    pub fn create_anonymous_node(
        &mut self,
        parent: usize,
        anon_type: AnonymousBoxType,
        fc: FormattingContext,
    ) -> usize {
        let index = self.nodes.len();

        // CSS 2.2 Section 17.2.1: Anonymous boxes inherit properties from their
        // enclosing non-anonymous box
        let parent_fc = self.nodes.get(parent).map(|n| n.formatting_context.clone());

        self.nodes.push(LayoutNode {
            // Anonymous boxes have no DOM correspondence
            dom_node_id: None,
            pseudo_element: None,
            parent: Some(parent),
            formatting_context: fc,
            parent_formatting_context: parent_fc,
            // Anonymous boxes inherit from parent
            box_props: BoxProps::default(),
            taffy_cache: TaffyCache::new(),
            is_anonymous: true,
            anonymous_type: Some(anon_type),
            children: Vec::new(),
            dirty_flag: DirtyFlag::Layout,
            // Anonymous boxes don't have style/data
            node_data_hash: 0,
            subtree_hash: SubtreeHash(0),
            intrinsic_sizes: None,
            used_size: None,
            relative_position: None,
            baseline: None,
            inline_layout_result: None,
            escaped_top_margin: None,
            escaped_bottom_margin: None,
            scrollbar_info: None,
            overflow_content_size: None,
        });

        self.nodes[parent].children.push(index);
        index
    }

    /// Creates a ::marker pseudo-element as the first child of a list-item.
    ///
    /// Per CSS Lists Module Level 3, Section 3.1:
    /// "For elements with display: list-item, user agents must generate a
    /// ::marker pseudo-element as the first child of the principal box."
    ///
    /// The ::marker references the same DOM node as its parent list-item,
    /// but is marked as a pseudo-element for proper counter resolution and styling.
    pub fn create_marker_pseudo_element(
        &mut self,
        styled_dom: &StyledDom,
        list_item_dom_id: NodeId,
        list_item_idx: usize,
    ) -> usize {
        let index = self.nodes.len();

        // The marker references the same DOM node as the list-item
        // This is important for style resolution (the marker inherits from the list-item)
        let parent_fc = self
            .nodes
            .get(list_item_idx)
            .map(|n| n.formatting_context.clone());
        self.nodes.push(LayoutNode {
            dom_node_id: Some(list_item_dom_id),
            pseudo_element: Some(PseudoElement::Marker),
            parent: Some(list_item_idx),
            // Markers contain inline text
            formatting_context: FormattingContext::Inline,
            parent_formatting_context: parent_fc,
            // Will be resolved from ::marker styles
            box_props: BoxProps::default(),
            taffy_cache: TaffyCache::new(),
            // Pseudo-elements are not anonymous boxes
            is_anonymous: false,
            anonymous_type: None,
            children: Vec::new(),
            dirty_flag: DirtyFlag::Layout,
            // Pseudo-elements don't have separate style in current impl
            node_data_hash: 0,
            subtree_hash: SubtreeHash(0),
            intrinsic_sizes: None,
            used_size: None,
            relative_position: None,
            baseline: None,
            inline_layout_result: None,
            escaped_top_margin: None,
            escaped_bottom_margin: None,
            scrollbar_info: None,
            overflow_content_size: None,
        });

        // Insert as FIRST child (per spec)
        self.nodes[list_item_idx].children.insert(0, index);

        // Register with DOM mapping for counter resolution
        self.dom_to_layout
            .entry(list_item_dom_id)
            .or_default()
            .push(index);

        index
    }

    pub fn create_node_from_dom(
        &mut self,
        styled_dom: &StyledDom,
        dom_id: NodeId,
        parent: Option<usize>,
        debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    ) -> Result<usize> {
        let index = self.nodes.len();
        let parent_fc =
            parent.and_then(|p| self.nodes.get(p).map(|n| n.formatting_context.clone()));
        self.nodes.push(LayoutNode {
            dom_node_id: Some(dom_id),
            pseudo_element: None,
            parent,
            formatting_context: determine_formatting_context(styled_dom, dom_id),
            parent_formatting_context: parent_fc,
            box_props: resolve_box_props(styled_dom, dom_id, debug_messages),
            taffy_cache: TaffyCache::new(),
            is_anonymous: false,
            anonymous_type: None,
            children: Vec::new(),
            dirty_flag: DirtyFlag::Layout,
            node_data_hash: hash_node_data(styled_dom, dom_id),
            subtree_hash: SubtreeHash(0),
            intrinsic_sizes: None,
            used_size: None,
            relative_position: None,
            baseline: None,
            inline_layout_result: None,
            escaped_top_margin: None,
            escaped_bottom_margin: None,
            scrollbar_info: None,
            overflow_content_size: None,
        });
        if let Some(p) = parent {
            self.nodes[p].children.push(index);
        }
        self.dom_to_layout.entry(dom_id).or_default().push(index);
        Ok(index)
    }

    pub fn clone_node_from_old(&mut self, old_node: &LayoutNode, parent: Option<usize>) -> usize {
        let index = self.nodes.len();
        let mut new_node = old_node.clone();
        new_node.parent = parent;
        new_node.parent_formatting_context =
            parent.and_then(|p| self.nodes.get(p).map(|n| n.formatting_context.clone()));
        new_node.children = Vec::new();
        new_node.dirty_flag = DirtyFlag::None;
        self.nodes.push(new_node);
        if let Some(p) = parent {
            self.nodes[p].children.push(index);
        }
        if let Some(dom_id) = old_node.dom_node_id {
            self.dom_to_layout.entry(dom_id).or_default().push(index);
        }
        index
    }

    pub fn build(self, root_idx: usize) -> LayoutTree {
        LayoutTree {
            nodes: self.nodes,
            root: root_idx,
            dom_to_layout: self.dom_to_layout,
        }
    }
}

pub fn is_block_level(styled_dom: &StyledDom, node_id: NodeId) -> bool {
    matches!(
        get_display_type(styled_dom, node_id),
        LayoutDisplay::Block
            | LayoutDisplay::FlowRoot
            | LayoutDisplay::Table
            | LayoutDisplay::TableRow
            | LayoutDisplay::TableRowGroup
            | LayoutDisplay::ListItem
    )
}

/// Checks if a node is inline-level (including text nodes).
/// According to CSS spec, inline-level content includes:
///
/// - Elements with display: inline, inline-block, inline-table, inline-flex, inline-grid
/// - Text nodes
/// - Generated content
fn is_inline_level(styled_dom: &StyledDom, node_id: NodeId) -> bool {
    // Text nodes are always inline-level
    let node_data = &styled_dom.node_data.as_container()[node_id];
    if matches!(node_data.get_node_type(), NodeType::Text(_)) {
        return true;
    }

    // Check the display property
    matches!(
        get_display_type(styled_dom, node_id),
        LayoutDisplay::Inline
            | LayoutDisplay::InlineBlock
            | LayoutDisplay::InlineTable
            | LayoutDisplay::InlineFlex
            | LayoutDisplay::InlineGrid
    )
}

/// Checks if a block container has only inline-level children.
/// According to CSS 2.2 Section 9.4.2: "An inline formatting context is established
/// by a block container box that contains no block-level boxes."
fn has_only_inline_children(styled_dom: &StyledDom, node_id: NodeId) -> bool {
    let hierarchy = styled_dom.node_hierarchy.as_container();
    let node_hier = match hierarchy.get(node_id) {
        Some(n) => n,
        None => {
            return false;
        }
    };

    // Get the first child
    let mut current_child = node_hier.first_child_id(node_id);

    // If there are no children, it's not an IFC (it's empty)
    if current_child.is_none() {
        return false;
    }

    // Check all children
    while let Some(child_id) = current_child {
        let is_inline = is_inline_level(styled_dom, child_id);

        if !is_inline {
            // Found a block-level child
            return false;
        }

        // Move to next sibling
        if let Some(child_hier) = hierarchy.get(child_id) {
            current_child = child_hier.next_sibling_id();
        } else {
            break;
        }
    }

    // All children are inline-level
    true
}

fn hash_node_data(dom: &StyledDom, node_id: NodeId) -> u64 {
    let mut hasher = std::hash::DefaultHasher::new();
    // Use node_state flags and node_type as a reasonable surrogate for now.
    if let Some(styled_node) = dom.node_data.as_container().get(node_id) {
        styled_node.get_hash().hash(&mut hasher);
    }
    hasher.finish()
}

/// Helper function to get element's computed font-size
fn get_element_font_size(styled_dom: &StyledDom, dom_id: NodeId) -> f32 {
    use crate::solver3::getters::*;

    let node_data = &styled_dom.node_data.as_container()[dom_id];
    let node_state = styled_dom
        .styled_nodes
        .as_container()
        .get(dom_id)
        .map(|n| &n.styled_node_state)
        .cloned()
        .unwrap_or_default();

    let cache = &styled_dom.css_property_cache.ptr;

    // Try to get from dependency chain first (proper resolution)
    if let Some(node_chains) = cache.dependency_chains.get(&dom_id) {
        if let Some(chain) = node_chains.get(&CssPropertyType::FontSize) {
            if let Some(cached) = chain.cached_pixels {
                return cached;
            }
        }
    }

    // Fallback: get from property cache
    cache
        .get_font_size(node_data, &dom_id, &node_state)
        .and_then(|v| v.get_property().cloned())
        .map(|v| {
            // Fallback using hardcoded 16px base
            v.inner.to_pixels_internal(0.0, DEFAULT_FONT_SIZE)
        })
        .unwrap_or(DEFAULT_FONT_SIZE)
}

/// Helper function to get parent's computed font-size
fn get_parent_font_size(styled_dom: &StyledDom, dom_id: NodeId) -> f32 {
    styled_dom
        .node_hierarchy
        .as_container()
        .get(dom_id)
        .and_then(|node| node.parent_id())
        .map(|parent_id| get_element_font_size(styled_dom, parent_id))
        .unwrap_or(azul_css::props::basic::pixel::DEFAULT_FONT_SIZE)
}

/// Helper function to get root element's font-size
fn get_root_font_size(styled_dom: &StyledDom) -> f32 {
    // Root is always NodeId(0) in Azul
    get_element_font_size(styled_dom, NodeId::new(0))
}

/// Create a ResolutionContext for a given node
fn create_resolution_context(
    styled_dom: &StyledDom,
    dom_id: NodeId,
    containing_block_size: Option<azul_css::props::basic::PhysicalSize>,
) -> azul_css::props::basic::ResolutionContext {
    let element_font_size = get_element_font_size(styled_dom, dom_id);
    let parent_font_size = get_parent_font_size(styled_dom, dom_id);
    let root_font_size = get_root_font_size(styled_dom);

    ResolutionContext {
        element_font_size,
        parent_font_size,
        root_font_size,
        containing_block_size: containing_block_size.unwrap_or(PhysicalSize::new(0.0, 0.0)),
        element_size: None, // Not yet laid out
        viewport_size: PhysicalSize::new(0.0, 0.0),
    }
}

fn resolve_box_props(
    styled_dom: &StyledDom,
    dom_id: NodeId,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> BoxProps {
    use crate::solver3::getters::*;

    let node_data = &styled_dom.node_data.as_container()[dom_id];

    // Get styled node state
    let node_state = styled_dom
        .styled_nodes
        .as_container()
        .get(dom_id)
        .map(|n| &n.styled_node_state)
        .cloned()
        .unwrap_or_default();

    // Create resolution context for this element
    // Note: containing_block_size is None here because we don't have it yet
    // This is fine - margins/padding use containing block width, but we'll handle that later
    let context = create_resolution_context(styled_dom, dom_id, None);

    // Helper to extract and resolve pixel value from MultiValue<PixelValue>
    let resolve_value = |mv: MultiValue<PixelValue>, prop_context: PropertyContext| -> f32 {
        match mv {
            MultiValue::Exact(pv) => pv.resolve_with_context(&context, prop_context),
            _ => 0.0,
        }
    };

    // Read margin, padding, border from styled_dom
    let margin_top_mv = get_css_margin_top(styled_dom, dom_id, &node_state);
    let margin_right_mv = get_css_margin_right(styled_dom, dom_id, &node_state);
    let margin_bottom_mv = get_css_margin_bottom(styled_dom, dom_id, &node_state);
    let margin_left_mv = get_css_margin_left(styled_dom, dom_id, &node_state);

    let margin = crate::solver3::geometry::EdgeSizes {
        top: resolve_value(margin_top_mv, PropertyContext::Margin),
        right: resolve_value(margin_right_mv, PropertyContext::Margin),
        bottom: resolve_value(margin_bottom_mv, PropertyContext::Margin),
        left: resolve_value(margin_left_mv, PropertyContext::Margin),
    };

    // Debug for Body nodes
    if matches!(node_data.node_type, azul_core::dom::NodeType::Body) {
        if let Some(msgs) = debug_messages.as_mut() {
            msgs.push(LayoutDebugMessage::box_props(format!(
                "Body margin resolved: top={:.2}, right={:.2}, bottom={:.2}, left={:.2}",
                margin.top, margin.right, margin.bottom, margin.left
            )));
        }
    }

    let padding = crate::solver3::geometry::EdgeSizes {
        top: resolve_value(
            get_css_padding_top(styled_dom, dom_id, &node_state),
            PropertyContext::Padding,
        ),
        right: resolve_value(
            get_css_padding_right(styled_dom, dom_id, &node_state),
            PropertyContext::Padding,
        ),
        bottom: resolve_value(
            get_css_padding_bottom(styled_dom, dom_id, &node_state),
            PropertyContext::Padding,
        ),
        left: resolve_value(
            get_css_padding_left(styled_dom, dom_id, &node_state),
            PropertyContext::Padding,
        ),
    };

    let border = crate::solver3::geometry::EdgeSizes {
        top: resolve_value(
            get_css_border_top_width(styled_dom, dom_id, &node_state),
            PropertyContext::Other,
        ),
        right: resolve_value(
            get_css_border_right_width(styled_dom, dom_id, &node_state),
            PropertyContext::Other,
        ),
        bottom: resolve_value(
            get_css_border_bottom_width(styled_dom, dom_id, &node_state),
            PropertyContext::Other,
        ),
        left: resolve_value(
            get_css_border_left_width(styled_dom, dom_id, &node_state),
            PropertyContext::Other,
        ),
    };

    BoxProps {
        margin,
        padding,
        border,
    }
}

/// CSS 2.2 Section 17.2.1 - Anonymous box generation, Stage 1:
/// "Remove all irrelevant boxes. These are boxes that do not contain table-related boxes
/// and do not themselves have 'display' set to a table-related value. In this context,
/// 'irrelevant boxes' means anonymous inline boxes that contain only white space."
///
/// Checks if a DOM node is whitespace-only text (for table anonymous box generation).
/// Returns true if the node is a text node containing only whitespace characters.
fn is_whitespace_only_text(styled_dom: &StyledDom, node_id: NodeId) -> bool {
    let binding = styled_dom.node_data.as_container();
    let node_data = binding.get(node_id);
    if let Some(data) = node_data {
        if let NodeType::Text(text) = data.get_node_type() {
            // Check if the text contains only whitespace characters
            // Per CSS 2.2 Section 17.2.1: whitespace-only anonymous boxes are irrelevant
            return text.chars().all(|c| c.is_whitespace());
        }
    }

    false
}

/// CSS 2.2 Section 17.2.1 - Anonymous box generation, Stage 1:
/// Determines if a node should be skipped in table structure generation.
/// Whitespace-only text nodes are "irrelevant" and should not generate boxes
/// when they appear between table-related elements.
///
/// Returns true if the node should be skipped (i.e., it's whitespace-only text
/// and the parent is a table structural element).
fn should_skip_for_table_structure(
    styled_dom: &StyledDom,
    node_id: NodeId,
    parent_display: LayoutDisplay,
) -> bool {
    // CSS 2.2 Section 17.2.1: Only skip whitespace text nodes when parent is
    // a table structural element (table, row group, row)
    matches!(
        parent_display,
        LayoutDisplay::Table
            | LayoutDisplay::TableRowGroup
            | LayoutDisplay::TableHeaderGroup
            | LayoutDisplay::TableFooterGroup
            | LayoutDisplay::TableRow
    ) && is_whitespace_only_text(styled_dom, node_id)
}

/// CSS 2.2 Section 17.2.1 - Anonymous box generation, Stage 3:
/// "Generate missing parents. For each table-cell box C in a sequence of consecutive
/// table-cell boxes (that are not part of a table-row), an anonymous table-row box
/// is generated around C and its consecutive table-cell siblings.
///
/// For each proper table child C in a sequence of consecutive proper table children
/// that are misparented (i.e., their parent is not a table element), an anonymous
/// table box is generated around C and its consecutive siblings."
///
/// This function checks if a node needs a parent wrapper and returns the appropriate
/// anonymous box type, or None if no wrapper is needed.
fn needs_table_parent_wrapper(
    styled_dom: &StyledDom,
    node_id: NodeId,
    parent_display: LayoutDisplay,
) -> Option<AnonymousBoxType> {
    let child_display = get_display_type(styled_dom, node_id);

    // CSS 2.2 Section 17.2.1, Stage 3:
    // If we have a table-cell but parent is not a table-row, need anonymous row
    if child_display == LayoutDisplay::TableCell {
        match parent_display {
            LayoutDisplay::TableRow
            | LayoutDisplay::TableRowGroup
            | LayoutDisplay::TableHeaderGroup
            | LayoutDisplay::TableFooterGroup => {
                // Parent can contain cells directly or via rows - no wrapper needed
                None
            }
            _ => Some(AnonymousBoxType::TableRow),
        }
    }
    // If we have a table-row but parent is not a table/row-group, need anonymous table
    else if matches!(child_display, LayoutDisplay::TableRow) {
        match parent_display {
            LayoutDisplay::Table
            | LayoutDisplay::TableRowGroup
            | LayoutDisplay::TableHeaderGroup
            | LayoutDisplay::TableFooterGroup => {
                None // Parent is correct
            }
            _ => Some(AnonymousBoxType::TableWrapper),
        }
    }
    // If we have a row-group but parent is not a table, need anonymous table
    else if matches!(
        child_display,
        LayoutDisplay::TableRowGroup
            | LayoutDisplay::TableHeaderGroup
            | LayoutDisplay::TableFooterGroup
    ) {
        match parent_display {
            LayoutDisplay::Table => None,
            _ => Some(AnonymousBoxType::TableWrapper),
        }
    } else {
        None
    }
}

// Determines the display type of a node based on its tag and CSS properties.
pub fn get_display_type(styled_dom: &StyledDom, node_id: NodeId) -> LayoutDisplay {
    if let Some(_styled_node) = styled_dom.styled_nodes.as_container().get(node_id) {
        let node_data = &styled_dom.node_data.as_container()[node_id];
        let node_state = &styled_dom.styled_nodes.as_container()[node_id].styled_node_state;

        // 1. Check author CSS first
        if let Some(d) = styled_dom
            .css_property_cache
            .ptr
            .get_display(node_data, &node_id, node_state)
            .and_then(|v| v.get_property().copied())
        {
            return d;
        }

        // 2. Check User Agent CSS (always returns a value for display)
        let node_type = &styled_dom.node_data.as_container()[node_id].node_type;
        if let Some(ua_prop) =
            azul_core::ua_css::get_ua_property(node_type, CssPropertyType::Display)
        {
            if let CssProperty::Display(azul_css::css::CssPropertyValue::Exact(d)) = ua_prop {
                return *d;
            }
        }
    }

    // 3. Final fallback (should never be reached since UA CSS always provides display)
    // Inline is the safest default per CSS spec
    LayoutDisplay::Inline
}

/// **Corrected:** Checks for all conditions that create a new Block Formatting Context.
/// A BFC contains floats and prevents margin collapse.
fn establishes_new_block_formatting_context(styled_dom: &StyledDom, node_id: NodeId) -> bool {
    let display = get_display_type(styled_dom, node_id);
    if matches!(
        display,
        LayoutDisplay::InlineBlock | LayoutDisplay::TableCell | LayoutDisplay::FlowRoot
    ) {
        return true;
    }

    if let Some(styled_node) = styled_dom.styled_nodes.as_container().get(node_id) {
        // `overflow` other than `visible`

        let overflow_x = get_overflow_x(styled_dom, node_id, &styled_node.styled_node_state);
        if !overflow_x.is_visible_or_clip() {
            return true;
        }

        let overflow_y = get_overflow_y(styled_dom, node_id, &styled_node.styled_node_state);
        if !overflow_y.is_visible_or_clip() {
            return true;
        }

        // `position: absolute` or `position: fixed`
        let position = get_position(styled_dom, node_id, &styled_node.styled_node_state);

        if position.is_absolute_or_fixed() {
            return true;
        }

        // `float` is not `none`
        let float = get_float(styled_dom, node_id, &styled_node.styled_node_state);
        if !float.is_none() {
            return true;
        }
    }

    // The root element (<html>) also establishes a BFC.
    if styled_dom.root.into_crate_internal() == Some(node_id) {
        return true;
    }

    false
}

/// The logic now correctly identifies all BFC roots.
fn determine_formatting_context(styled_dom: &StyledDom, node_id: NodeId) -> FormattingContext {
    // Special case: Text nodes should be treated as inline content.
    // They participate in their parent's inline formatting context.
    let node_data = &styled_dom.node_data.as_container()[node_id];

    if matches!(node_data.get_node_type(), NodeType::Text(_)) {
        // Text nodes are inline-level content within their parent's IFC
        return FormattingContext::Inline;
    }

    let display_type = get_display_type(styled_dom, node_id);

    match display_type {
        LayoutDisplay::Inline => FormattingContext::Inline,

        // CSS 2.2 Section 9.4.2: "An inline formatting context is established by a
        // block container box that contains no block-level boxes."
        // Check if this block container has only inline-level children.
        LayoutDisplay::Block | LayoutDisplay::FlowRoot | LayoutDisplay::ListItem => {
            if has_only_inline_children(styled_dom, node_id) {
                // This block container should establish an IFC for its inline children
                FormattingContext::Inline
            } else {
                // Normal BFC
                FormattingContext::Block {
                    establishes_new_context: establishes_new_block_formatting_context(
                        styled_dom, node_id,
                    ),
                }
            }
        }
        LayoutDisplay::InlineBlock => FormattingContext::InlineBlock,
        LayoutDisplay::Table | LayoutDisplay::InlineTable => FormattingContext::Table,
        LayoutDisplay::TableRowGroup
        | LayoutDisplay::TableHeaderGroup
        | LayoutDisplay::TableFooterGroup => FormattingContext::TableRowGroup,
        LayoutDisplay::TableRow => FormattingContext::TableRow,
        LayoutDisplay::TableCell => FormattingContext::TableCell,
        LayoutDisplay::None => FormattingContext::None,
        LayoutDisplay::Flex | LayoutDisplay::InlineFlex => FormattingContext::Flex,
        LayoutDisplay::TableColumnGroup => FormattingContext::TableColumnGroup,
        LayoutDisplay::TableCaption => FormattingContext::TableCaption,
        LayoutDisplay::Grid | LayoutDisplay::InlineGrid => FormattingContext::Grid,

        // These less common display types default to block behavior
        LayoutDisplay::TableColumn | LayoutDisplay::RunIn | LayoutDisplay::Marker => {
            FormattingContext::Block {
                establishes_new_context: true,
            }
        }
    }
}
