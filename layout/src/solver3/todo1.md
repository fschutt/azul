```rust
// file: layout/src/solver3/taffy_bridge.rs

//! A bridge module to integrate the Taffy Flexbox and Grid layout library
//! into solver3's layout engine using Taffy's low-level API.

use std::collections::BTreeMap;
use taffy::{
    compute_cached_layout, compute_flexbox_layout, compute_grid_layout, compute_leaf_layout,
    prelude::*, CacheTree,
};

use crate::{
    solver3::{
        geometry::CssSize,
        layout_tree::{LayoutNode, LayoutTree},
        sizing, LayoutContext, LayoutError,
    },
    text3::cache::{FontLoaderTrait, ParsedFontTrait},
};
use azul_core::{dom::NodeId, styled_dom::StyledDom};
use azul_css::{CssProperty, CssPropertyType, CssPropertyValue, LayoutDisplay, PixelValue};

/// The bridge struct that implements Taffy's traits.
/// It holds mutable references to the solver's data structures, allowing Taffy
/// to read styles and write layout results back into our `LayoutTree`.
struct TaffyBridge<'a, 'b, T: ParsedFontTrait, Q: FontLoaderTrait<T>> {
    ctx: &'a mut LayoutContext<'b, T, Q>,
    tree: &'a mut LayoutTree<T>,
    // A map to store translated styles, avoiding re-computation for each trait method.
    style_cache: BTreeMap<usize, Style>,
}

impl<'a, 'b, T: ParsedFontTrait, Q: FontLoaderTrait<T>> TaffyBridge<'a, 'b, T, Q> {
    fn new(ctx: &'a mut LayoutContext<'b, T, Q>, tree: &'a mut LayoutTree<T>) -> Self {
        Self { ctx, tree, style_cache: BTreeMap::new() }
    }

    /// Translates CSS properties from the `StyledDom` into a `taffy::Style` struct.
    /// This is the core of the integration, mapping one style system to another.
    fn translate_style_to_taffy(&self, dom_id: Option<NodeId>) -> Style {
        let Some(id) = dom_id else { return Style::default() };
        let Some(styled_node) = self.ctx.styled_dom.styled_nodes.as_container().get(id) else {
            return Style::default();
        };

        let style = styled_node.state.get_style();
        let mut taffy_style = Style::default();

        // Display
        if let Some(CssProperty::Display(CssPropertyValue::Exact(d))) =
            style.get(&CssPropertyType::Display)
        {
            taffy_style.display = match d {
                LayoutDisplay::Flex => Display::Flex,
                LayoutDisplay::Grid => Display::Grid,
                _ => Display::Block,
            };
        }

        // Flex Direction
        if let Some(val) =
            style.get(&CssPropertyType::FlexDirection).and_then(|p| p.get_exact())
        {
            taffy_style.flex_direction = (*val).into();
        }

        // Align Items
        if let Some(val) = style.get(&CssPropertyType::AlignItems).and_then(|p| p.get_exact()) {
            taffy_style.align_items = Some((*val).into());
        }

        // Justify Content
        if let Some(val) =
            style.get(&CssPropertyType::JustifyContent).and_then(|p| p.get_exact())
        {
            taffy_style.justify_content = Some((*val).into());
        }

        // Gap
        let row_gap = style.get(&CssPropertyType::RowGap).and_then(|p| p.get_exact());
        let col_gap = style.get(&CssPropertyType::ColumnGap).and_then(|p| p.get_exact());
        taffy_style.gap = Size {
            width: row_gap.map_or(Length::ZERO, |v| (*v).into()),
            height: col_gap.map_or(Length::ZERO, |v| (*v).into()),
        };

        // Size (Width / Height)
        let width = sizing::get_css_width(self.ctx.styled_dom, dom_id);
        let height = sizing::get_css_height(self.ctx.styled_dom, dom_id);
        taffy_style.size = Size {
            width: width.into(),
            height: height.into(),
        };

        // TODO: Implement translation for other properties:
        // - flex_wrap, align_self, flex_grow, flex_shrink, flex_basis
        // - grid_template_columns, grid_template_rows, grid_auto_flow, etc.
        // - padding, border, margin

        taffy_style
    }

    /// Gets or computes the Taffy style for a given node index.
    fn get_taffy_style(&mut self, node_idx: usize) -> Style {
        if let Some(cached) = self.style_cache.get(&node_idx) {
            return cached.clone();
        }

        let dom_id = self.tree.get(node_idx).and_then(|n| n.dom_node_id);
        let style = self.translate_style_to_taffy(dom_id);
        self.style_cache.insert(node_idx, style.clone());
        style
    }
}

/// Main entry point for laying out a Flexbox or Grid container using Taffy.
pub fn layout_taffy_subtree<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    ctx: &mut LayoutContext<T, Q>,
    tree: &mut LayoutTree<T>,
    node_idx: usize,
    inputs: LayoutInput,
) -> LayoutOutput {
    let mut bridge = TaffyBridge::new(ctx, tree);

    // Taffy's low-level API is node-at-a-time. We call the correct layout
    // function for the root of the subtree we want to lay out. Taffy will
    // then recursively call back into our `compute_child_layout` implementation.
    let node = bridge.tree.get(node_idx).unwrap();
    let output = match node.formatting_context {
        azul_core::ui_solver::FormattingContext::Flex => {
            compute_flexbox_layout(&mut bridge, node_idx.into(), inputs)
        }
        azul_core::ui_solver::FormattingContext::Grid => {
            compute_grid_layout(&mut bridge, node_idx.into(), inputs)
        }
        _ => {
            // This function should only be called for Taffy-handled contexts.
            // We return a zero-sized output as a safe fallback.
            LayoutOutput::new(Size::ZERO, Point::ZERO)
        }
    };

    // The results have already been written back into `bridge.tree` via the
    // `set_unrounded_layout` trait method. We just need to return the final size.
    output
}

// Taffy's NodeId is just a wrapper around usize, so we can convert directly.
impl From<usize> for NodeId {
    fn from(val: usize) -> Self {
        NodeId::new(val as u64)
    }
}
impl From<NodeId> for usize {
    fn from(val: NodeId) -> Self {
        val.into_u64() as usize
    }
}

// --- Trait Implementations for the Bridge ---

impl<'a, 'b, T: ParsedFontTrait, Q: FontLoaderTrait<T>> TraversePartialTree
    for TaffyBridge<'a, 'b, T, Q>
{
    type ChildIter<'c> = std::vec::IntoIter<NodeId> where Self: 'c;

    fn child_ids(&self, node_id: NodeId) -> Self::ChildIter<'_> {
        let node_idx: usize = node_id.into();
        self.tree
            .get(node_idx)
            .map_or(Vec::new(), |n| n.children.iter().map(|&c| c.into()).collect())
            .into_iter()
    }

    fn child_count(&self, node_id: NodeId) -> usize {
        let node_idx: usize = node_id.into();
        self.tree.get(node_idx).map_or(0, |n| n.children.len())
    }

    fn get_child_id(&self, node_id: NodeId, index: usize) -> NodeId {
        let node_idx: usize = node_id.into();
        self.tree.get(node_idx).unwrap().children[index].into()
    }
}

impl<'a, 'b, T: ParsedFontTrait, Q: FontLoaderTrait<T>> LayoutPartialTree
    for TaffyBridge<'a, 'b, T, Q>
{
    type CoreContainerStyle<'c> = Style where Self: 'c;
    type CustomIdent = (); // Not using custom idents for now

    fn get_core_container_style(&self, node_id: NodeId) -> Self::CoreContainerStyle<'_> {
        // Taffy clones the style, so we can't easily use the cache here without
        // more complex lifetime management. Direct computation is simpler.
        let node_idx: usize = node_id.into();
        let dom_id = self.tree.get(node_idx).and_then(|n| n.dom_node_id);
        self.translate_style_to_taffy(dom_id)
    }

    fn set_unrounded_layout(&mut self, node_id: NodeId, layout: &Layout) {
        let node_idx: usize = node_id.into();
        if let Some(node) = self.tree.get_mut(node_idx) {
            node.used_size = Some(layout.size.into());
            node.relative_position = Some(layout.location.into());
        }
    }

    fn compute_child_layout(
        &mut self,
        node_id: NodeId,
        inputs: LayoutInput,
    ) -> LayoutOutput {
        // This is the recursive dispatcher. Taffy calls this when it needs a child laid out.
        compute_cached_layout(self, node_id, inputs, |tree, node_id, inputs| {
            let node_idx: usize = node_id.into();
            let node = tree.tree.get(node_idx).unwrap();

            match node.formatting_context {
                azul_core::ui_solver::FormattingContext::Flex => {
                    compute_flexbox_layout(tree, node_id, inputs)
                }
                azul_core::ui_solver::FormattingContext::Grid => {
                    compute_grid_layout(tree, node_id, inputs)
                }
                // For non-Taffy nodes, we must treat them as "leaf" nodes from Taffy's perspective.
                // We calculate their intrinsic size using solver3's own logic and report it back.
                _ => {
                    let style = tree.get_taffy_style(node_idx);
                    compute_leaf_layout(
                        inputs,
                        &style,
                        |_, _| 0.0, // Not using calc()
                        |known_dimensions, _available_space| {
                            // This is the "measure function".
                            // It calls back into solver3's sizing logic.
                            let intrinsic =
                                node.intrinsic_sizes.unwrap_or_default();
                            Size {
                                width: known_dimensions.width.unwrap_or(intrinsic.max_content_width),
                                height: known_dimensions.height.unwrap_or(intrinsic.max_content_height),
                            }
                        },
                    )
                }
            }
        })
    }
}

impl<'a, 'b, T: ParsedFontTrait, Q: FontLoaderTrait<T>> CacheTree for TaffyBridge<'a, 'b, T, Q> {
    fn cache_get(
        &self,
        node_id: NodeId,
        known_dimensions: Size<Option<f32>>,
        available_space: Size<AvailableSpace>,
        run_mode: RunMode,
    ) -> Option<LayoutOutput> {
        let node_idx: usize = node_id.into();
        self.tree.get(node_idx)?.taffy_cache.get(known_dimensions, available_space, run_mode)
    }

    fn cache_store(
        &mut self,
        node_id: NodeId,
        known_dimensions: Size<Option<f32>>,
        available_space: Size<AvailableSpace>,
        run_mode: RunMode,
        layout_output: LayoutOutput,
    ) {
        let node_idx: usize = node_id.into();
        if let Some(node) = self.tree.get_mut(node_idx) {
            node.taffy_cache.store(known_dimensions, available_space, run_mode, layout_output);
        }
    }

    fn cache_clear(&mut self, node_id: NodeId) {
        let node_idx: usize = node_id.into();
        if let Some(node) = self.tree.get_mut(node_idx) {
            node.taffy_cache.clear();
        }
    }
}

// The Flexbox and Grid traits simply require providing the style for the container and children.
impl<'a, 'b, T: ParsedFontTrait, Q: FontLoaderTrait<T>> LayoutFlexboxContainer
    for TaffyBridge<'a, 'b, T, Q>
{
    type FlexboxContainerStyle<'c> = Style where Self: 'c;
    type FlexboxItemStyle<'c> = Style where Self: 'c;

    fn get_flexbox_container_style(&self, node_id: NodeId) -> Self::FlexboxContainerStyle<'_> {
        self.get_core_container_style(node_id)
    }

    fn get_flexbox_child_style(&self, child_node_id: NodeId) -> Self::FlexboxItemStyle<'_> {
        self.get_core_container_style(child_node_id)
    }
}

impl<'a, 'b, T: ParsedFontTrait, Q: FontLoaderTrait<T>> LayoutGridContainer
    for TaffyBridge<'a, 'b, T, Q>
{
    type GridContainerStyle<'c> = Style where Self: 'c;
    type GridItemStyle<'c> = Style where Self: 'c;

    fn get_grid_container_style(&self, node_id: NodeId) -> Self::GridContainerStyle<'_> {
        self.get_core_container_style(node_id)
    }

    fn get_grid_child_style(&self, child_node_id: NodeId) -> Self::GridItemStyle<'_> {
        self.get_core_container_style(child_node_id)
    }
}

// --- Trait impls to convert between azul_css types and taffy types ---

impl From<CssSize> for Dimension {
    fn from(val: CssSize) -> Self {
        match val {
            CssSize::Auto => Dimension::Auto,
            CssSize::Px(px) => Dimension::Length(px),
            CssSize::Percent(p) => Dimension::Percent(p / 100.0),
            // Taffy doesn't have direct equivalents for min/max-content as input sizes,
            // but they are handled via the measure function for leaf nodes.
            // For container sizes, Auto is the closest semantic equivalent.
            CssSize::MinContent => Dimension::Auto,
            CssSize::MaxContent => Dimension::Auto,
        }
    }
}

impl From<PixelValue> for Length {
    fn from(val: PixelValue) -> Self {
        match val {
            PixelValue::Px(px) => Length::from_points(px),
            PixelValue::Percent(p) => Length::from_percent(p / 100.0),
        }
    }
}

impl From<Size<f32>> for azul_core::window::LogicalSize {
    fn from(val: Size<f32>) -> Self {
        Self { width: val.width, height: val.height }
    }
}

impl From<Point<f32>> for azul_core::window::LogicalPosition {
    fn from(val: Point<f32>) -> Self {
        Self { x: val.x, y: val.y }
    }
}

// NOTE: These From impls require adding `#[derive(Copy)]` to the azul_css enums
// or cloning them (`*val`). For this example, we assume cloning is acceptable.
impl From<azul_css::LayoutAlignItems> for AlignItems {
    fn from(val: azul_css::LayoutAlignItems) -> Self {
        use azul_css::LayoutAlignItems::*;
        match val {
            FlexStart => AlignItems::FlexStart,
            FlexEnd => AlignItems::FlexEnd,
            Center => AlignItems::Center,
            Baseline => AlignItems::Baseline,
            Stretch => AlignItems::Stretch,
        }
    }
}

impl From<azul_css::LayoutJustifyContent> for JustifyContent {
    fn from(val: azul_css::LayoutJustifyContent) -> Self {
        use azul_css::LayoutJustifyContent::*;
        match val {
            FlexStart => JustifyContent::FlexStart,
            FlexEnd => JustifyContent::FlexEnd,
            Center => JustifyContent::Center,
            SpaceBetween => JustifyContent::SpaceBetween,
            SpaceAround => JustifyContent::SpaceAround,
            SpaceEvenly => JustifyContent::SpaceEvenly,
            _ => JustifyContent::FlexStart, // Fallback for unsupported values
        }
    }
}

impl From<azul_css::LayoutDirection> for FlexDirection {
    fn from(val: azul_css::LayoutDirection) -> Self {
        use azul_css::LayoutDirection::*;
        match val {
            Row => FlexDirection::Row,
            RowReverse => FlexDirection::RowReverse,
            Column => FlexDirection::Column,
            ColumnReverse => FlexDirection::ColumnReverse,
        }
    }
}
```

```rust
// file: layout/src/solver3/layout_tree.rs

// Add the taffy dependency and the new cache field to LayoutNode.

use taffy::Cache as TaffyCache;

// ... other imports

#[derive(Debug, Clone)]
pub struct LayoutNode<T: ParsedFontTrait> {
    /// Reference back to the original DOM node (None for anonymous boxes)
    pub dom_node_id: Option<NodeId>,
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
    pub node_data_hash: u64,
    /// A hash of this node's data and all of its descendants. Used for
    /// fast reconciliation.
    pub subtree_hash: SubtreeHash,
    pub formatting_context: FormattingContext,
    pub intrinsic_sizes: Option<IntrinsicSizes>,
    pub used_size: Option<LogicalSize>,
    /// The position of this node *relative to its parent's content box*.
    pub relative_position: Option<LogicalPosition>,
    /// The baseline of this box, if applicable, measured from its content-box top edge.
    pub baseline: Option<f32>,
    /// Optional layouted text that this layout node carries
    pub inline_layout_result: Option<Arc<UnifiedLayout<T>>>,
}

// ... in LayoutTreeBuilder ...

// Update create_anonymous_node to initialize the Taffy cache
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
        taffy_cache: TaffyCache::new(), // Initialize here
        is_anonymous: true,
        anonymous_type: Some(anon_type),
        children: Vec::new(),
        dirty_flag: DirtyFlag::Layout,
        node_data_hash: 0,
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


// Update create_node_from_dom to initialize the Taffy cache
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
        taffy_cache: TaffyCache::new(), // Initialize here
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
```

```rust
// file: layout/src/solver3/fc.rs

// Add the new module and update layout_formatting_context
use crate::solver3::taffy_bridge;
use taffy::prelude::{AvailableSpace, LayoutInput, Size};

// ... other imports

pub fn layout_formatting_context<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    ctx: &mut LayoutContext<T, Q>,
    tree: &mut LayoutTree<T>,
    text_cache: &mut text3::cache::LayoutCache<T>,
    node_index: usize,
    constraints: &LayoutConstraints,
) -> Result<LayoutOutput> {
    let node = tree.get(node_index).ok_or(LayoutError::InvalidTree)?;

    match node.formatting_context {
        FormattingContext::Block { .. } => layout_bfc(ctx, tree, node_index, constraints),
        FormattingContext::Inline => layout_ifc(ctx, text_cache, tree, node_index, constraints),
        FormattingContext::Table => layout_table_fc(ctx, tree, node_index, constraints),

        // NEW: Handle Flex and Grid by calling the Taffy bridge
        FormattingContext::Flex | FormattingContext::Grid => {
            let available_space = Size {
                width: AvailableSpace::Definite(constraints.available_size.width),
                height: AvailableSpace::Definite(constraints.available_size.height),
            };

            let taffy_inputs = LayoutInput {
                known_dimensions: Size::NONE,
                parent_size: constraints.available_size.into(),
                available_space,
                run_mode: taffy::RunMode::PerformLayout,
            };

            let taffy_output =
                taffy_bridge::layout_taffy_subtree(ctx, tree, node_index, taffy_inputs);

            // The bridge has already updated the positions and sizes of the children in the tree.
            // We just need to construct the LayoutOutput for the parent.
            let mut output = LayoutOutput::default();
            output.overflow_size = taffy_output.size.into();

            // Taffy's results are stored directly on the nodes, so we read them back here.
            for &child_idx in &tree.get(node_index).unwrap().children {
                if let Some(child_node) = tree.get(child_idx) {
                    if let Some(pos) = child_node.relative_position {
                        output.positions.insert(child_idx, pos);
                    }
                }
            }

            Ok(output)
        }
        _ => layout_bfc(ctx, tree, node_index, constraints),
    }
}
```
