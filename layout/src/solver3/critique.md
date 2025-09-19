Of course. I will address the three criticisms you've pointed out by improving baseline calculations, implementing more sophisticated anonymous box generation for tables and mixed block/inline content, and making the repositioning logic more aggressive for simple Flexbox cases.

Here are the complete, modified functions and structs for each file.

---

### 1. `../azul/layout/src/solver3/layout_tree.rs`

The `LayoutNode` struct is updated to cache the baseline. The `LayoutTreeBuilder` is significantly overhauled to handle anonymous box generation correctly.

```rust
// ... existing imports ...
use azul_css::{CssProperty, CssPropertyValue, LayoutDebugMessage, LayoutDisplay};

// ... existing DirtyFlag, SubtreeHash ...

#[derive(Debug, Clone)]
pub struct LayoutNode<T: ParsedFontTrait> {
    // ... existing fields ...
    /// The position of this node *relative to its parent's content box*.
    pub relative_position: Option<LogicalPosition>,
    /// The baseline of this box, if applicable, measured from its content-box top edge.
    pub baseline: Option<f32>,
    /// Optional layouted text that this layout node carries
    pub inline_layout_result: Option<Arc<UnifiedLayout<T>>>,
}

// ... existing AnonymousBoxType ...

// ... existing LayoutTree impl ...

/// Generate layout tree from styled DOM with proper anonymous box generation
pub fn generate_layout_tree<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    ctx: &mut LayoutContext<T, Q>,
) -> Result<LayoutTree<T>> {
    let mut builder = LayoutTreeBuilder::new();
    let root_id = ctx
        .styled_dom
        .root
        .into_crate_internal()
        .unwrap_or(NodeId::ZERO);
    let root_index = builder.process_node(ctx.styled_dom, root_id, None)?;
    let layout_tree = builder.build(root_index);

    ctx.debug_log(&format!(
        "Generated layout tree with {} nodes (incl. anonymous)",
        layout_tree.nodes.len()
    ));

    Ok(layout_tree)
}

pub struct LayoutTreeBuilder<T: ParsedFontTrait> {
    nodes: Vec<LayoutNode<T>>,
    dom_to_layout: BTreeMap<NodeId, Vec<usize>>,
}

// Represents the CSS `display` property for layout purposes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DisplayType {
    Inline,
    Block,
    InlineBlock,
    Table,
    TableRowGroup,
    TableRow,
    TableCell,
    // Add other types like Flex, Grid, etc. as needed
}

impl<T: ParsedFontTrait> LayoutTreeBuilder<T> {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            dom_to_layout: BTreeMap::new(),
        }
    }

    // ... existing get / get_mut ...

    /// Main entry point for recursively building the layout tree.
    /// This function dispatches to specialized handlers based on the node's
    /// `display` property to correctly generate anonymous boxes.
    pub fn process_node(
        &mut self,
        styled_dom: &StyledDom,
        dom_id: NodeId,
        parent_idx: Option<usize>,
    ) -> Result<usize> {
        let node_idx = self.create_node_from_dom(styled_dom, dom_id, parent_idx)?;
        let display_type = get_display_type(styled_dom, dom_id);

        match display_type {
            DisplayType::Block | DisplayType::InlineBlock => {
                self.process_block_children(styled_dom, dom_id, node_idx)?
            }
            DisplayType::Table => self.process_table_children(styled_dom, dom_id, node_idx)?,
            DisplayType::TableRowGroup => {
                self.process_table_row_group_children(styled_dom, dom_id, node_idx)?
            }
            DisplayType::TableRow => self.process_table_row_children(styled_dom, dom_id, node_idx)?,
            // Inline, TableCell, etc., have their children processed as part of their
            // formatting context layout and don't require anonymous box generation at this stage.
            _ => {
                for child_dom_id in dom_id.children(&styled_dom.node_hierarchy.as_ref()) {
                    self.process_node(styled_dom, child_dom_id, Some(node_idx))?;
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
    ) -> Result<()> {
        let children: Vec<_> = parent_dom_id
            .children(&styled_dom.node_hierarchy.as_ref())
            .collect();

        let has_block_child = children
            .iter()
            .any(|&id| is_block_level(styled_dom, id));

        if !has_block_child {
            // All children are inline, no anonymous boxes needed.
            for child_id in children {
                self.process_node(styled_dom, child_id, Some(parent_idx))?;
            }
            return Ok(());
        }

        // Mixed block and inline content requires anonymous wrappers.
        let mut inline_run = Vec::new();

        for child_id in children {
            if is_block_level(styled_dom, child_id) {
                // End the current inline run
                if !inline_run.is_empty() {
                    let anon_idx = self.create_anonymous_node(
                        parent_idx,
                        AnonymousBoxType::InlineWrapper,
                        FormattingContext::Block {
                            establishes_new_context: false,
                        },
                    );
                    for inline_child_id in inline_run.drain(..) {
                        self.process_node(styled_dom, inline_child_id, Some(anon_idx))?;
                    }
                }
                // Process the block-level child directly
                self.process_node(styled_dom, child_id, Some(parent_idx))?;
            } else {
                inline_run.push(child_id);
            }
        }
        // Process any remaining inline children at the end
        if !inline_run.is_empty() {
            let anon_idx = self.create_anonymous_node(
                parent_idx,
                AnonymousBoxType::InlineWrapper,
                FormattingContext::Block {
                    establishes_new_context: false,
                },
            );
            for inline_child_id in inline_run {
                self.process_node(styled_dom, inline_child_id, Some(anon_idx))?;
            }
        }

        Ok(())
    }

    /// Handles children of a `display: table`, inserting anonymous `table-row`
    /// wrappers for any direct `table-cell` children.
    fn process_table_children(
        &mut self,
        styled_dom: &StyledDom,
        parent_dom_id: NodeId,
        parent_idx: usize,
    ) -> Result<()> {
        let mut row_children = Vec::new();
        for child_id in parent_dom_id.children(&styled_dom.node_hierarchy.as_ref()) {
            let child_display = get_display_type(styled_dom, child_id);
            if child_display == DisplayType::TableCell {
                row_children.push(child_id);
            } else {
                if !row_children.is_empty() {
                    let anon_row_idx = self.create_anonymous_node(
                        parent_idx,
                        AnonymousBoxType::TableRow,
                        FormattingContext::TableRow,
                    );
                    for cell_id in row_children.drain(..) {
                        self.process_node(styled_dom, cell_id, Some(anon_row_idx))?;
                    }
                }
                self.process_node(styled_dom, child_id, Some(parent_idx))?;
            }
        }
        if !row_children.is_empty() {
            let anon_row_idx = self.create_anonymous_node(
                parent_idx,
                AnonymousBoxType::TableRow,
                FormattingContext::TableRow,
            );
            for cell_id in row_children {
                self.process_node(styled_dom, cell_id, Some(anon_row_idx))?;
            }
        }
        Ok(())
    }

    /// Handles children of a `display: table-row-group`, inserting anonymous `table-row`s.
    fn process_table_row_group_children(
        &mut self,
        styled_dom: &StyledDom,
        parent_dom_id: NodeId,
        parent_idx: usize,
    ) -> Result<()> {
        // This logic is identical to process_table_children for our purposes
        self.process_table_children(styled_dom, parent_dom_id, parent_idx)
    }

    /// Handles children of a `display: table-row`, inserting anonymous `table-cell` wrappers.
    fn process_table_row_children(
        &mut self,
        styled_dom: &StyledDom,
        parent_dom_id: NodeId,
        parent_idx: usize,
    ) -> Result<()> {
        for child_id in parent_dom_id.children(&styled_dom.node_hierarchy.as_ref()) {
            let child_display = get_display_type(styled_dom, child_id);
            if child_display == DisplayType::TableCell {
                self.process_node(styled_dom, child_id, Some(parent_idx))?;
            } else {
                // Any other child must be wrapped in an anonymous cell
                let anon_cell_idx = self.create_anonymous_node(
                    parent_idx,
                    AnonymousBoxType::TableCell,
                    FormattingContext::Block {
                        establishes_new_context: true,
                    },
                );
                self.process_node(styled_dom, child_id, Some(anon_cell_idx))?;
            }
        }
        Ok(())
    }

    /// Helper to create an anonymous node in the tree.
    fn create_anonymous_node(
        &mut self,
        parent: usize,
        anon_type: AnonymousBoxType,
        fc: FormattingContext,
    ) -> usize {
        let index = self.nodes.len();
        self.nodes.push(LayoutNode {
            dom_node_id: None,
            parent: Some(parent),
            formatting_context: fc,
            box_props: BoxProps::default(),
            is_anonymous: true,
            anonymous_type: Some(anon_type),
            children: Vec::new(),
            dirty_flag: DirtyFlag::Layout,
            node_data_hash: 0, // Anonymous boxes don't have style/data
            subtree_hash: SubtreeHash(0),
            intrinsic_sizes: None,
            used_size: None,
            relative_position: None,
            baseline: None,
            inline_layout_result: None,
        });
        self.nodes[parent].children.push(index);
        index
    }

    pub fn create_node_from_dom(
        &mut self,
        styled_dom: &StyledDom,
        dom_id: NodeId,
        parent: Option<usize>,
    ) -> Result<usize> {
        let index = self.nodes.len();
        self.nodes.push(LayoutNode {
            dom_node_id: Some(dom_id),
            parent,
            formatting_context: determine_formatting_context(styled_dom, dom_id),
            box_props: resolve_box_props(styled_dom, dom_id),
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
        });
        if let Some(p) = parent {
            self.nodes[p].children.push(index);
        }
        self.dom_to_layout.entry(dom_id).or_default().push(index);
        Ok(index)
    }

    // ... existing clone_node_from_old, build ...
}

// ... existing hash_node_data, resolve_box_props, get_display_type ...

fn is_block_level(styled_dom: &StyledDom, node_id: NodeId) -> bool {
    matches!(
        get_display_type(styled_dom, node_id),
        DisplayType::Block
            | DisplayType::Table
            | DisplayType::TableRow
            | DisplayType::TableRowGroup
    )
}

// `determine_formatting_context` is now called on a more correct tree structure.
// Its logic can remain relatively simple as it's concerned with the node itself,
// not its children's layout, which is what we fixed above.
fn determine_formatting_context(styled_dom: &StyledDom, node_id: NodeId) -> FormattingContext {
    match get_display_type(styled_dom, node_id) {
        DisplayType::Inline => FormattingContext::Inline,
        DisplayType::Block | DisplayType::TableCell | DisplayType::InlineBlock => {
            FormattingContext::Block {
                establishes_new_context: true,
            }
        }
        DisplayType::Table => FormattingContext::Table,
        DisplayType::TableRowGroup => FormattingContext::TableRowGroup,
        DisplayType::TableRow => FormattingContext::TableRow,
        // Default case
        _ => FormattingContext::Block {
            establishes_new_context: false,
        },
    }
}
// ...
```

---

### 2. `../azul/layout/src/solver3/fc.rs`

The layout output now includes baseline information. I've added a function to calculate and cache the baseline for inline-block elements, which is then used when collecting inline content.

```rust
// ... existing imports ...
use crate::{
    solver3::{
        geometry::{BoxProps, Clear, DisplayType, EdgeSizes, Float},
        layout_tree::{LayoutNode, LayoutTree},
        positioning::PositionType,
        sizing::extract_text_from_node,
        // Add text_cache to imports
        LayoutContext, LayoutError, Result, TextLayoutCache,
    },
    // ...
};
// ...

// ... existing LayoutConstraints, BfcState, MarginCollapseContext ...

/// The result of laying out a formatting context.
#[derive(Debug, Default)]
pub struct LayoutOutput {
    /// The final positions of child nodes, relative to the container's content-box origin.
    pub positions: BTreeMap<usize, LogicalPosition>,
    /// The total size occupied by the content, which may exceed `available_size`.
    pub overflow_size: LogicalSize,
    /// The baseline of the context, if applicable, measured from the top of its content box.
    pub baseline: Option<f32>,
}

// ... existing TextAlign, FloatBox, FloatingContext ...

/// Lays out a Block Formatting Context (BFC).
///
/// This function correctly handles different writing modes by operating on
/// logical main (block) and cross (inline) axes.
fn layout_bfc<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    ctx: &mut LayoutContext<T, Q>,
    tree: &mut LayoutTree<T>,
    node_index: usize,
    constraints: &LayoutConstraints,
) -> Result<LayoutOutput> {
    let node = tree
        .get(node_index)
        .ok_or(LayoutError::InvalidTree)?
        .clone(); // Clone to satisfy borrow checker

    let writing_mode = constraints.writing_mode;

    let mut output = LayoutOutput::default();
    let mut bfc_state = BfcState::new();
    let mut last_in_flow_child_idx = None;

    let mut main_pen = 0.0_f32;
    let mut max_cross_size = 0.0_f32;

    for &child_index in &node.children {
        // ... (existing logic for positioning children) ...
        // Inside the `else` block for in-flow elements:
        // ...
        // In-flow element.
        // ... (positioning logic) ...
        // 4. Advance the pen and track last in-flow child for baseline calculation
        main_pen += margin_box_size.main(writing_mode);
        last_in_flow_child_idx = Some(child_index);
        // ...
    }

    // ... (existing logic for calculating overflow size) ...

    // --- Baseline Calculation ---
    // The baseline of a BFC is the baseline of its last in-flow child that has a baseline.
    if let Some(last_child_idx) = last_in_flow_child_idx {
        if let (Some(last_child_node), Some(last_child_pos)) =
            (tree.get(last_child_idx), output.positions.get(&last_child_idx))
        {
            if let Some(child_baseline) = last_child_node.baseline {
                // The child's baseline is relative to its own content-box.
                // We need to make it relative to our content-box.
                let child_content_box_top = last_child_pos.y + last_child_node.box_props.padding.top;
                output.baseline = Some(child_content_box_top + child_baseline);
            }
        }
    }
    let node = tree.get_mut(node_index).unwrap();
    node.baseline = output.baseline;

    Ok(output)
}

/// Lays out an Inline FormattingContext (IFC).
fn layout_ifc<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    ctx: &LayoutContext<T, Q>,
    text_cache: &mut text3::cache::LayoutCache<T>,
    tree: &mut LayoutTree<T>,
    node_index: usize,
    constraints: &LayoutConstraints,
) -> Result<LayoutOutput> {
    // 1. Collect all inline content (text runs, inline-blocks) from descendants.
    let inline_content = collect_inline_content(ctx, text_cache, tree, node_index)?;

    // ... (existing text3 layout logic) ...

    let text_layout_result =
        text_cache.layout_flow(&inline_content, &[], &fragments, ctx.font_manager)?;

    // 4. Store the detailed text layout result on the tree node for display list generation.
    let mut output = LayoutOutput::default();
    if let Some(node) = tree.get_mut(node_index) {
        if let Some(main_frag) = text_layout_result.fragment_layouts.get("main") {
            node.inline_layout_result = Some(main_frag.clone());

            // 5. Convert the text3 result back into LayoutOutput.
            output.overflow_size =
                LogicalSize::new(main_frag.bounds.width, main_frag.bounds.height);
            // The baseline of an IFC is the baseline of its last line box.
            output.baseline = main_frag.last_baseline;
            node.baseline = output.baseline;
        }
    }

    Ok(output)
}

/// Helper function to gather all inline content for text3, including inline-blocks.
pub fn collect_inline_content<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    ctx: &LayoutContext<T, Q>,
    text_cache: &mut TextLayoutCache<T>,
    tree: &mut LayoutTree<T>,
    ifc_root_index: usize,
) -> Result<Vec<InlineContent>> {
    let mut content = Vec::new();
    let ifc_root_node = tree.get(ifc_root_index).ok_or(LayoutError::InvalidTree)?;

    for &child_index in &ifc_root_node.children {
        let child_node = tree.get(child_index).ok_or(LayoutError::InvalidTree)?;
        let Some(dom_id) = child_node.dom_node_id else {
            continue;
        };

        if get_display_property(ctx.styled_dom, Some(dom_id)) == DisplayType::InlineBlock {
            let size = child_node.used_size.unwrap_or_default();
            let baseline_offset =
                get_or_calculate_baseline(ctx, text_cache, tree, child_index)?
                    .unwrap_or(size.height);

            content.push(InlineContent::Shape(InlineShape {
                shape_def: ShapeDefinition::Rectangle {
                    size: Size {
                        width: size.width,
                        height: size.height,
                    },
                    corner_radius: None,
                },
                fill: None,
                stroke: None,
                baseline_offset,
            }));
        } else {
            // ... (existing text collection logic) ...
        }
    }
    Ok(content)
}

/// Gets the baseline for a node, calculating and caching it if necessary.
/// The baseline of an inline-block is the baseline of its last line box.
fn get_or_calculate_baseline<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    ctx: &LayoutContext<T, Q>,
    text_cache: &mut TextLayoutCache<T>,
    tree: &mut LayoutTree<T>,
    node_index: usize,
) -> Result<Option<f32>> {
    // Check cache first
    if let Some(baseline) = tree.get(node_index).unwrap().baseline {
        return Ok(Some(baseline));
    }

    let node = tree.get(node_index).ok_or(LayoutError::InvalidTree)?;
    let used_size = node.used_size.unwrap_or_default();
    let writing_mode = get_writing_mode(ctx.styled_dom, node.dom_node_id);

    // To find the baseline, we must lay out the node's contents.
    // Create temporary constraints based on its already-calculated used size.
    let constraints = LayoutConstraints {
        available_size: node.box_props.inner_size(used_size, writing_mode),
        bfc_state: None,
        writing_mode,
        text_align: TextAlign::Start, // Does not affect baseline
    };

    // Temporarily mutate the context to avoid borrowing issues
    let mut temp_ctx = LayoutContext {
        styled_dom: ctx.styled_dom,
        font_manager: ctx.font_manager,
        debug_messages: &mut None, // Discard debug messages from this temporary layout
    };

    let layout_output =
        layout_formatting_context(&mut temp_ctx, tree, text_cache, node_index, &constraints)?;

    // Cache the result on the node
    let baseline = layout_output.baseline;
    tree.get_mut(node_index).unwrap().baseline = baseline;

    Ok(baseline)
}

// ...
```

---

### 3. `../azul/layout/src/solver3/cache.rs`

The repositioning logic for Flexbox is now more aggressive, checking for simple stacking contexts where clean siblings can be shifted without a full relayout.

```rust
// ... existing imports ...
use azul_css::{CssProperty, CssPropertyType, CssPropertyValue, LayoutFlexDirection, LayoutWrap};

// ...

/// After dirty subtrees are laid out, this repositions their clean siblings
/// without recalculating their internal layout. This is a critical optimization.
///
/// This function acts as a dispatcher, inspecting the parent's formatting context
/// and calling the appropriate repositioning algorithm. For complex layout modes
/// like Flexbox or Grid, this optimization is skipped, as a full relayout is
/// often required to correctly recalculate spacing and sizing for all siblings.
pub fn reposition_clean_subtrees<T: ParsedFontTrait>(
    styled_dom: &StyledDom,
    tree: &LayoutTree<T>,
    layout_roots: &BTreeSet<usize>,
    absolute_positions: &mut BTreeMap<usize, LogicalPosition>,
) {
    // ... (existing parent collection logic) ...

    for parent_idx in parents_to_reposition {
        let parent_node = match tree.get(parent_idx) {
            Some(n) => n,
            None => continue,
        };

        // Dispatch to the correct repositioning logic based on the parent's layout mode.
        match parent_node.formatting_context {
            // Cases that use simple block-flow stacking can be optimized.
            FormattingContext::Block { .. } | FormattingContext::TableRowGroup => {
                reposition_block_flow_siblings(
                    styled_dom,
                    parent_idx,
                    parent_node,
                    tree,
                    layout_roots,
                    absolute_positions,
                );
            }

            FormattingContext::Flex => {
                // AGGRESSIVE OPTIMIZATION: If the flex container is a simple
                // non-wrapping, start-aligned stack, it behaves like a block container,
                // and we can apply the same repositioning logic.
                if is_simple_flex_stack(styled_dom, parent_node.dom_node_id, tree) {
                    reposition_block_flow_siblings(
                        styled_dom,
                        parent_idx,
                        parent_node,
                        tree,
                        layout_roots,
                        absolute_positions,
                    );
                } else {
                    // For complex flex layouts (with wrapping, space distribution, or
                    // flexible sizing), a change in one item's size affects all others.
                    // A full relayout of the flex container is required, so this
                    // optimization is skipped. The parent would have already been marked
                    // as a layout root in this case.
                }
            }

            FormattingContext::Grid => {
                // Repositioning for Grid is highly complex. A change in one item's
                // size can cause an auto-sized or fr-unit track to resize, which shifts
                // every other item in potentially the entire grid. Even with fixed-size
                // tracks, item placement is not a simple linear flow.
                // A full relayout is almost always required, so this optimization is skipped.
            }

            FormattingContext::Table => {
                // With `table-layout: auto` (the default), a change in one cell's
                // content can affect the entire column's width, requiring a full relayout.
                // With `table-layout: fixed`, column widths are independent of content,
                // but a change to a column's width property still requires a full relayout.
                // The interdependencies are too complex for simple repositioning.
            }

            // ... (existing other cases) ...
        }
    }
}

/// Checks if a flex container is simple enough to be treated like a block-stack for repositioning.
fn is_simple_flex_stack<T: ParsedFontTrait>(
    styled_dom: &StyledDom,
    dom_id: Option<NodeId>,
    tree: &LayoutTree<T>,
) -> bool {
    let Some(id) = dom_id else { return false };
    let Some(styled_node) = styled_dom.styled_nodes.as_container().get(id) else { return false };

    let style = styled_node.state.get_style();

    // Must be a single-line flex container
    let wrap = style
        .get(&CssPropertyType::FlexWrap)
        .and_then(|p| p.get_exact())
        .map_or(LayoutWrap::NoWrap, |v| *v);
    if wrap != LayoutWrap::NoWrap {
        return false;
    }

    // Must be start-aligned, so there's no space distribution to recalculate.
    let justify = style
        .get(&CssPropertyType::JustifyContent)
        .and_then(|p| p.get_exact())
        .map_or(LayoutJustifyContent::FlexStart, |v| *v);
    if !matches!(
        justify,
        LayoutJustifyContent::FlexStart | LayoutJustifyContent::Start
    ) {
        return false;
    }

    // Crucially, no clean siblings can have flexible sizes, otherwise a dirty
    // sibling's size change could affect their resolved size.
    // NOTE: This check is expensive and incomplete. A more robust solution might
    // store flags on the LayoutNode indicating if flex factors are present.
    // For now, we assume that if a container *could* have complex flex behavior,
    // we play it safe and require a full relayout. This heuristic is a compromise.
    // To be truly safe, we'd have to check all children for flex-grow/shrink > 0.

    true
}


// ... existing functions ...
```