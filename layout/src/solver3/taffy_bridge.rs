// file: layout/src/solver3/taffy_bridge.rs

//! A bridge module to integrate the Taffy Flexbox and Grid layout library
//! into solver3's layout engine using Taffy's low-level API.

use std::{collections::BTreeMap, sync::Arc};

use azul_core::{dom::NodeId, styled_dom::StyledDom};
use azul_css::{
    AlignItems as AzulAlignItems, AlignSelf as AzulAlignSelf, CssProperty, CssPropertyType,
    CssPropertyValue, FlexDirection as AzulFlexDirection, FlexWrap as AzulFlexWrap,
    JustifyContent as AzulJustifyContent, LayoutDisplay, LayoutPosition, PixelValue,
};
use taffy::{
    compute_cached_layout, compute_flexbox_layout, compute_grid_layout, compute_leaf_layout,
    prelude::*, CacheTree, LayoutFlexboxContainer, LayoutGridContainer, LayoutInput, LayoutOutput,
    RunMode,
};

use crate::{
    solver3::{
        geometry::CssSize,
        layout_tree::{LayoutNode, LayoutTree},
        sizing, LayoutContext,
    },
    text3::cache::{FontLoaderTrait, ParsedFontTrait},
};

/// The bridge struct that implements Taffy's traits.
/// It holds mutable references to the solver's data structures, allowing Taffy
/// to read styles and write layout results back into our `LayoutTree`.
struct TaffyBridge<'a, 'b, T: ParsedFontTrait, Q: FontLoaderTrait<T>> {
    ctx: &'a mut LayoutContext<'b, T, Q>,
    tree: &'a mut LayoutTree<T>,
}

impl<'a, 'b, T: ParsedFontTrait, Q: FontLoaderTrait<T>> TaffyBridge<'a, 'b, T, Q> {
    fn new(ctx: &'a mut LayoutContext<'b, T, Q>, tree: &'a mut LayoutTree<T>) -> Self {
        Self { ctx, tree }
    }

    /// Translates CSS properties from the `StyledDom` into a `taffy::Style` struct.
    /// This is the core of the integration, mapping one style system to another.
    fn translate_style_to_taffy(&self, dom_id: Option<NodeId>) -> Style {
        let Some(id) = dom_id else {
            return Style::default();
        };
        let Some(styled_node) = self.ctx.styled_dom.styled_nodes.as_container().get(id) else {
            return Style::default();
        };

        let style = styled_node.state.get_style();
        let mut taffy_style = Style::default();

        // Display Mode
        if let Some(CssProperty::Display(CssPropertyValue::Exact(d))) =
            style.get(&CssPropertyType::Display)
        {
            taffy_style.display = match d {
                LayoutDisplay::Flex => Display::Flex,
                LayoutDisplay::Grid => Display::Grid,
                LayoutDisplay::None => Display::None,
                _ => Display::Block,
            };
        }

        // Position
        if let Some(val) = style
            .get(&CssPropertyType::Position)
            .and_then(|p| p.get_exact())
        {
            taffy_style.position = (*val).into();
        }

        // Inset (top, left, bottom, right)
        taffy_style.inset = Rect {
            left: style
                .get(&CssPropertyType::Left)
                .and_then(|p| p.get_exact())
                .map_or(LengthPercentageAuto::auto(), |v| (*v).into()),
            right: style
                .get(&CssPropertyType::Right)
                .and_then(|p| p.get_exact())
                .map_or(LengthPercentageAuto::auto(), |v| (*v).into()),
            top: style
                .get(&CssPropertyType::Top)
                .and_then(|p| p.get_exact())
                .map_or(LengthPercentageAuto::auto(), |v| (*v).into()),
            bottom: style
                .get(&CssPropertyType::Bottom)
                .and_then(|p| p.get_exact())
                .map_or(LengthPercentageAuto::auto(), |v| (*v).into()),
        };

        // Size
        let width = sizing::get_css_width(self.ctx.styled_dom, dom_id);
        let height = sizing::get_css_height(self.ctx.styled_dom, dom_id);
        taffy_style.size = Size {
            width: width.into(),
            height: height.into(),
        };

        // Min/Max Size
        taffy_style.min_size = todo!(); // Read min-width / min-height
        taffy_style.max_size = todo!(); // Read max-width / max-height

        // Box Model
        taffy_style.padding = todo!(); // Read padding-* properties
        taffy_style.border = todo!(); // Read border-width-* properties
        taffy_style.margin = todo!(); // Read margin-* properties

        // Flexbox
        taffy_style.flex_direction = style
            .get(&CssPropertyType::FlexDirection)
            .and_then(|p| p.get_exact())
            .map_or(FlexDirection::Row, |v| (*v).into());
        taffy_style.flex_wrap = style
            .get(&CssPropertyType::FlexWrap)
            .and_then(|p| p.get_exact())
            .map_or(FlexWrap::NoWrap, |v| (*v).into());
        taffy_style.align_items = style
            .get(&CssPropertyType::AlignItems)
            .and_then(|p| p.get_exact())
            .map(|v| (*v).into());
        taffy_style.align_self = style
            .get(&CssPropertyType::AlignSelf)
            .and_then(|p| p.get_exact())
            .map(|v| (*v).into());
        taffy_style.justify_content = style
            .get(&CssPropertyType::JustifyContent)
            .and_then(|p| p.get_exact())
            .map(|v| (*v).into());
        taffy_style.flex_grow = style
            .get(&CssPropertyType::FlexGrow)
            .and_then(|p| p.get_exact())
            .map_or(0.0, |v| *v);
        taffy_style.flex_shrink = style
            .get(&CssPropertyType::FlexShrink)
            .and_then(|p| p.get_exact())
            .map_or(1.0, |v| *v);
        taffy_style.flex_basis = todo!(); // Read flex-basis

        // Gap
        let row_gap = style
            .get(&CssPropertyType::RowGap)
            .and_then(|p| p.get_exact());
        let col_gap = style
            .get(&CssPropertyType::ColumnGap)
            .and_then(|p| p.get_exact());
        taffy_style.gap = Size {
            width: row_gap.map_or(Length::ZERO, |v| (*v).into()),
            height: col_gap.map_or(Length::ZERO, |v| (*v).into()),
        };

        // Grid
        taffy_style.grid_template_columns = todo!(); // Read grid-template-columns
        taffy_style.grid_template_rows = todo!(); // Read grid-template-rows
        taffy_style.grid_auto_columns = todo!(); // Read grid-auto-columns
        taffy_style.grid_auto_rows = todo!(); // Read grid-auto-rows
        taffy_style.grid_row = todo!(); // Read grid-row-start/end
        taffy_style.grid_column = todo!(); // Read grid-column-start/end

        taffy_style
    }

    /// Gets or computes the Taffy style for a given node index.
    fn get_taffy_style(&self, node_idx: usize) -> Style {
        let dom_id = self.tree.get(node_idx).and_then(|n| n.dom_node_id);
        let style = self.translate_style_to_taffy(dom_id);
        style
    }

    /// Helper to get children that participate in layout (i.e., not `display: none`).
    fn get_layout_children(&self, node_idx: usize) -> Vec<usize> {
        let Some(node) = self.tree.get(node_idx) else {
            return Vec::new();
        };
        node.children
            .iter()
            .filter(|&&child_idx| {
                let Some(child_node) = self.tree.get(child_idx) else {
                    return false;
                };
                let Some(child_dom_id) = child_node.dom_node_id else {
                    return true;
                };

                if let Some(styled) = self
                    .ctx
                    .styled_dom
                    .styled_nodes
                    .as_container()
                    .get(child_dom_id)
                {
                    if let Some(CssProperty::Display(CssPropertyValue::Exact(
                        LayoutDisplay::None,
                    ))) = styled.state.get_style().get(&CssPropertyType::Display)
                    {
                        return false;
                    }
                }
                true
            })
            .copied()
            .collect()
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
    let node = bridge.tree.get(node_idx).unwrap();
    let output = match node.formatting_context {
        azul_core::ui_solver::FormattingContext::Flex => {
            compute_flexbox_layout(&mut bridge, node_idx.into(), inputs)
        }
        azul_core::ui_solver::FormattingContext::Grid => {
            compute_grid_layout(&mut bridge, node_idx.into(), inputs)
        }
        _ => LayoutOutput::HIDDEN,
    };
    output
}

// --- Trait Implementations for the Bridge ---

impl<'a, 'b, T: ParsedFontTrait, Q: FontLoaderTrait<T>> TraversePartialTree
    for TaffyBridge<'a, 'b, T, Q>
{
    type ChildIter<'c>
        = std::vec::IntoIter<taffy::NodeId>
    where
        Self: 'c;

    fn child_ids(&self, node_id: taffy::NodeId) -> Self::ChildIter<'_> {
        self.get_layout_children(node_id.into())
            .into_iter()
            .map(|id| id.into())
            .collect::<Vec<taffy::NodeId>>()
            .into_iter()
    }

    fn child_count(&self, node_id: taffy::NodeId) -> usize {
        self.get_layout_children(node_id.into()).len()
    }

    fn get_child_id(&self, node_id: taffy::NodeId, index: usize) -> taffy::NodeId {
        self.get_layout_children(node_id.into())[index].into()
    }
}

impl<'a, 'b, T: ParsedFontTrait, Q: FontLoaderTrait<T>> LayoutPartialTree
    for TaffyBridge<'a, 'b, T, Q>
{
    type CoreContainerStyle<'c>
        = Style
    where
        Self: 'c;
    type CustomIdent = String;

    fn get_core_container_style(&self, node_id: taffy::NodeId) -> Self::CoreContainerStyle<'_> {
        let node_idx: usize = node_id.into();
        let dom_id = self.tree.get(node_idx).and_then(|n| n.dom_node_id);
        self.translate_style_to_taffy(dom_id)
    }

    fn set_unrounded_layout(&mut self, node_id: taffy::NodeId, layout: &Layout) {
        let node_idx: usize = node_id.into();
        if let Some(node) = self.tree.get_mut(node_idx) {
            node.used_size = Some(layout.size.into());
            node.relative_position = Some(layout.location.into());
        }
    }

    fn compute_child_layout(
        &mut self,
        node_id: taffy::NodeId,
        inputs: LayoutInput,
    ) -> LayoutOutput {
        compute_cached_layout(self, node_id, inputs, |tree, node_id, inputs| {
            let node_idx: usize = node_id.into();
            let fc = tree
                .tree
                .get(node_idx)
                .map(|s| s.formatting_context.clone())
                .unwrap_or_default();

            match fc {
                azul_core::ui_solver::FormattingContext::Flex => {
                    compute_flexbox_layout(tree, node_id, inputs)
                }
                azul_core::ui_solver::FormattingContext::Grid => {
                    compute_grid_layout(tree, node_id, inputs)
                }
                _ => {
                    let node = tree.tree.get(node_idx).unwrap();
                    let style = tree.get_taffy_style(node_idx);
                    compute_leaf_layout(
                        inputs,
                        &style,
                        |_, _| 0.0,
                        |known_dimensions, _available_space| {
                            let intrinsic = node.intrinsic_sizes.unwrap_or_default();
                            Size {
                                width: known_dimensions
                                    .width
                                    .unwrap_or(intrinsic.max_content_width),
                                height: known_dimensions
                                    .height
                                    .unwrap_or(intrinsic.max_content_height),
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
        node_id: taffy::NodeId,
        known_dimensions: Size<Option<f32>>,
        available_space: Size<AvailableSpace>,
        run_mode: RunMode,
    ) -> Option<LayoutOutput> {
        let node_idx: usize = node_id.into();
        self.tree
            .get(node_idx)?
            .taffy_cache
            .get(known_dimensions, available_space, run_mode)
    }

    fn cache_store(
        &mut self,
        node_id: taffy::NodeId,
        known_dimensions: Size<Option<f32>>,
        available_space: Size<AvailableSpace>,
        run_mode: RunMode,
        layout_output: LayoutOutput,
    ) {
        let node_idx: usize = node_id.into();
        if let Some(node) = self.tree.get_mut(node_idx) {
            node.taffy_cache
                .store(known_dimensions, available_space, run_mode, layout_output);
        }
    }

    fn cache_clear(&mut self, node_id: taffy::NodeId) {
        let node_idx: usize = node_id.into();
        if let Some(node) = self.tree.get_mut(node_idx) {
            node.taffy_cache.clear();
        }
    }
}

/*

    /// A typed representation of the CSS style information for a single node.
    ///
    /// The most important idea in flexbox is the notion of a "main" and "cross" axis, which are always perpendicular to each other.
    /// The orientation of these axes are controlled via the [`FlexDirection`] field of this struct.
    ///
    /// This struct follows the [CSS equivalent](https://developer.mozilla.org/en-US/docs/Web/CSS/CSS_Flexible_Box_Layout/Basic_Concepts_of_Flexbox) directly;
    /// information about the behavior on the web should transfer directly.
    ///
    /// Detailed information about the exact behavior of each of these fields
    /// can be found on [MDN](https://developer.mozilla.org/en-US/docs/Web/CSS) by searching for the field name.
    /// The distinction between margin, padding and border is explained well in
    /// this [introduction to the box model](https://developer.mozilla.org/en-US/docs/Web/CSS/CSS_Box_Model/Introduction_to_the_CSS_box_model).
    ///
    /// If the behavior does not match the flexbox layout algorithm on the web, please file a bug!
    #[derive(Clone, PartialEq, Debug)]
    #[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
    #[cfg_attr(feature = "serde", serde(default))]
    pub struct Style<S: CheapCloneStr = DefaultCheapStr> {
        /// This is a dummy field which is necessary to make Taffy compile with the `grid` feature disabled
        /// It should always be set to `core::marker::PhantomData`.
        pub dummy: core::marker::PhantomData<S>,
        /// What layout strategy should be used?
        pub display: Display,
        /// Whether a child is display:table or not. This affects children of block layouts.
        /// This should really be part of `Display`, but it is currently seperate because table layout isn't implemented
        pub item_is_table: bool,
        /// Is it a replaced element like an image or form field?
        /// <https://drafts.csswg.org/css-sizing-3/#min-content-zero>
        pub item_is_replaced: bool,
        /// Should size styles apply to the content box or the border box of the node
        pub box_sizing: BoxSizing,

        // Overflow properties
        /// How children overflowing their container should affect layout
        pub overflow: Point<Overflow>,
        /// How much space (in points) should be reserved for the scrollbars of `Overflow::Scroll` and `Overflow::Auto` nodes.
        pub scrollbar_width: f32,

        // Position properties
        /// What should the `position` value of this struct use as a base offset?
        pub position: Position,
        /// How should the position of this element be tweaked relative to the layout defined?
        #[cfg_attr(feature = "serde", serde(default = "style_helpers::auto"))]
        pub inset: Rect<LengthPercentageAuto>,

        // Size properties
        /// Sets the initial size of the item
        #[cfg_attr(feature = "serde", serde(default = "style_helpers::auto"))]
        pub size: Size<Dimension>,
        /// Controls the minimum size of the item
        #[cfg_attr(feature = "serde", serde(default = "style_helpers::auto"))]
        pub min_size: Size<Dimension>,
        /// Controls the maximum size of the item
        #[cfg_attr(feature = "serde", serde(default = "style_helpers::auto"))]
        pub max_size: Size<Dimension>,
        /// Sets the preferred aspect ratio for the item
        ///
        /// The ratio is calculated as width divided by height.
        pub aspect_ratio: Option<f32>,

        // Spacing Properties
        /// How large should the margin be on each side?
        #[cfg_attr(feature = "serde", serde(default = "style_helpers::zero"))]
        pub margin: Rect<LengthPercentageAuto>,
        /// How large should the padding be on each side?
        #[cfg_attr(feature = "serde", serde(default = "style_helpers::zero"))]
        pub padding: Rect<LengthPercentage>,
        /// How large should the border be on each side?
        #[cfg_attr(feature = "serde", serde(default = "style_helpers::zero"))]
        pub border: Rect<LengthPercentage>,

        // Alignment properties
        /// How this node's children aligned in the cross/block axis?
        #[cfg(any(feature = "flexbox", feature = "grid"))]
        pub align_items: Option<AlignItems>,
        /// How this node should be aligned in the cross/block axis
        /// Falls back to the parents [`AlignItems`] if not set
        #[cfg(any(feature = "flexbox", feature = "grid"))]
        pub align_self: Option<AlignSelf>,
        /// How this node's children should be aligned in the inline axis
        #[cfg(feature = "grid")]
        pub justify_items: Option<AlignItems>,
        /// How this node should be aligned in the inline axis
        /// Falls back to the parents [`JustifyItems`] if not set
        #[cfg(feature = "grid")]
        pub justify_self: Option<AlignSelf>,
        /// How should content contained within this item be aligned in the cross/block axis
        #[cfg(any(feature = "flexbox", feature = "grid"))]
        pub align_content: Option<AlignContent>,
        /// How should content contained within this item be aligned in the main/inline axis
        #[cfg(any(feature = "flexbox", feature = "grid"))]
        pub justify_content: Option<JustifyContent>,
        /// How large should the gaps between items in a grid or flex container be?
        #[cfg(any(feature = "flexbox", feature = "grid"))]
        #[cfg_attr(feature = "serde", serde(default = "style_helpers::zero"))]
        pub gap: Size<LengthPercentage>,

        // Block container properties
        /// How items elements should aligned in the inline axis
        #[cfg(feature = "block_layout")]
        pub text_align: TextAlign,

        // Flexbox container properties
        /// Which direction does the main axis flow in?
        #[cfg(feature = "flexbox")]
        pub flex_direction: FlexDirection,
        /// Should elements wrap, or stay in a single line?
        #[cfg(feature = "flexbox")]
        pub flex_wrap: FlexWrap,

        // Flexbox item properties
        /// Sets the initial main axis size of the item
        #[cfg(feature = "flexbox")]
        pub flex_basis: Dimension,
        /// The relative rate at which this item grows when it is expanding to fill space
        ///
        /// 0.0 is the default value, and this value must be positive.
        #[cfg(feature = "flexbox")]
        pub flex_grow: f32,
        /// The relative rate at which this item shrinks when it is contracting to fit into space
        ///
        /// 1.0 is the default value, and this value must be positive.
        #[cfg(feature = "flexbox")]
        pub flex_shrink: f32,

        // Grid container properies
        /// Defines the track sizing functions (heights) of the grid rows
        #[cfg(feature = "grid")]
        pub grid_template_rows: GridTrackVec<GridTemplateComponent<S>>,
        /// Defines the track sizing functions (widths) of the grid columns
        #[cfg(feature = "grid")]
        pub grid_template_columns: GridTrackVec<GridTemplateComponent<S>>,
        /// Defines the size of implicitly created rows
        #[cfg(feature = "grid")]
        pub grid_auto_rows: GridTrackVec<TrackSizingFunction>,
        /// Defined the size of implicitly created columns
        #[cfg(feature = "grid")]
        pub grid_auto_columns: GridTrackVec<TrackSizingFunction>,
        /// Controls how items get placed into the grid for auto-placed items
        #[cfg(feature = "grid")]
        pub grid_auto_flow: GridAutoFlow,

        // Grid container named properties
        /// Defines the rectangular grid areas
        #[cfg(feature = "grid")]
        pub grid_template_areas: GridTrackVec<GridTemplateArea<S>>,
        /// The named lines between the columns
        #[cfg(feature = "grid")]
        pub grid_template_column_names: GridTrackVec<GridTrackVec<S>>,
        /// The named lines between the rows
        #[cfg(feature = "grid")]
        pub grid_template_row_names: GridTrackVec<GridTrackVec<S>>,

        // Grid child properties
        /// Defines which row in the grid the item should start and end at
        #[cfg(feature = "grid")]
        pub grid_row: Line<GridPlacement<S>>,
        /// Defines which column in the grid the item should start and end at
        #[cfg(feature = "grid")]
        pub grid_column: Line<GridPlacement<S>>,
    }

    impl<S: CheapCloneStr> Style<S> {
        /// The [`Default`] layout, in a form that can be used in const functions
        pub const DEFAULT: Style<S> = Style {
            dummy: core::marker::PhantomData,
            display: Display::DEFAULT,
            item_is_table: false,
            item_is_replaced: false,
            box_sizing: BoxSizing::BorderBox,
            overflow: Point { x: Overflow::Visible, y: Overflow::Visible },
            scrollbar_width: 0.0,
            position: Position::Relative,
            inset: Rect::auto(),
            margin: Rect::zero(),
            padding: Rect::zero(),
            border: Rect::zero(),
            size: Size::auto(),
            min_size: Size::auto(),
            max_size: Size::auto(),
            aspect_ratio: None,
            #[cfg(any(feature = "flexbox", feature = "grid"))]
            gap: Size::zero(),
            // Alignment
            #[cfg(any(feature = "flexbox", feature = "grid"))]
            align_items: None,
            #[cfg(any(feature = "flexbox", feature = "grid"))]
            align_self: None,
            #[cfg(feature = "grid")]
            justify_items: None,
            #[cfg(feature = "grid")]
            justify_self: None,
            #[cfg(any(feature = "flexbox", feature = "grid"))]
            align_content: None,
            #[cfg(any(feature = "flexbox", feature = "grid"))]
            justify_content: None,
            // Block
            #[cfg(feature = "block_layout")]
            text_align: TextAlign::Auto,
            // Flexbox
            #[cfg(feature = "flexbox")]
            flex_direction: FlexDirection::Row,
            #[cfg(feature = "flexbox")]
            flex_wrap: FlexWrap::NoWrap,
            #[cfg(feature = "flexbox")]
            flex_grow: 0.0,
            #[cfg(feature = "flexbox")]
            flex_shrink: 1.0,
            #[cfg(feature = "flexbox")]
            flex_basis: Dimension::AUTO,
            // Grid
            #[cfg(feature = "grid")]
            grid_template_rows: GridTrackVec::new(),
            #[cfg(feature = "grid")]
            grid_template_columns: GridTrackVec::new(),
            #[cfg(feature = "grid")]
            grid_template_areas: GridTrackVec::new(),
            #[cfg(feature = "grid")]
            grid_template_column_names: GridTrackVec::new(),
            #[cfg(feature = "grid")]
            grid_template_row_names: GridTrackVec::new(),
            #[cfg(feature = "grid")]
            grid_auto_rows: GridTrackVec::new(),
            #[cfg(feature = "grid")]
            grid_auto_columns: GridTrackVec::new(),
            #[cfg(feature = "grid")]
            grid_auto_flow: GridAutoFlow::Row,
            #[cfg(feature = "grid")]
            grid_row: Line { start: GridPlacement::<S>::Auto, end: GridPlacement::<S>::Auto },
            #[cfg(feature = "grid")]
            grid_column: Line { start: GridPlacement::<S>::Auto, end: GridPlacement::<S>::Auto },
        };
    }

    impl<S: CheapCloneStr> Default for Style<S> {
        fn default() -> Self {
            Style::DEFAULT
        }
    }

*/

impl<'a, 'b, T: ParsedFontTrait, Q: FontLoaderTrait<T>> LayoutFlexboxContainer
    for TaffyBridge<'a, 'b, T, Q>
{
    type FlexboxContainerStyle<'c>
        = Style
    where
        Self: 'c;
    type FlexboxItemStyle<'c>
        = Style
    where
        Self: 'c;

    fn get_flexbox_container_style(
        &self,
        node_id: taffy::NodeId,
    ) -> Self::FlexboxContainerStyle<'_> {
        self.get_core_container_style(node_id)
    }

    fn get_flexbox_child_style(&self, child_node_id: taffy::NodeId) -> Self::FlexboxItemStyle<'_> {
        self.get_core_container_style(child_node_id)
    }
}

impl<'a, 'b, T: ParsedFontTrait, Q: FontLoaderTrait<T>> LayoutGridContainer
    for TaffyBridge<'a, 'b, T, Q>
{
    type GridContainerStyle<'c>
        = Style
    where
        Self: 'c;
    type GridItemStyle<'c>
        = Style
    where
        Self: 'c;

    fn get_grid_container_style(&self, node_id: taffy::NodeId) -> Self::GridContainerStyle<'_> {
        self.get_core_container_style(node_id)
    }

    fn get_grid_child_style(&self, child_node_id: taffy::NodeId) -> Self::GridItemStyle<'_> {
        self.get_core_container_style(child_node_id)
    }
}

// --- Conversion Functions ---

fn from_css_size(val: CssSize) -> Dimension {
    match val {
        CssSize::Px(px) => Dimension::length(px),
        CssSize::Percent(p) => Dimension::percent(p / 100.0),
        CssSize::MinContent | CssSize::MaxContent | CssSize::Auto => Dimension::auto(),
    }
}

fn from_pixel_value_lp(val: PixelValue) -> LengthPercentage {
    match val.to_pixels_no_percent() {
        Some(px) => LengthPercentage::length(px),
        None => match val.to_percent() {
            Some(p) => LengthPercentage::percent(p),
            None => LengthPercentage::length(0.0), // Fallback to 0 if neither px nor percent
        },
    }
}

fn from_pixel_value_lpa(val: PixelValue) -> LengthPercentageAuto {
    match val.to_pixels_no_percent() {
        Some(px) => LengthPercentageAuto::length(px),
        None => match val.to_percent() {
            Some(p) => LengthPercentageAuto::percent(p),
            None => LengthPercentageAuto::auto(),
        },
    }
}

fn from_taffy_size(val: Size<f32>) -> azul_core::window::LogicalSize {
    azul_core::window::LogicalSize {
        width: val.width,
        height: val.height,
    }
}

#[allow(dead_code)]
fn from_logical_size(val: azul_core::window::LogicalSize) -> Size<AvailableSpace> {
    Size {
        width: AvailableSpace::Definite(val.width),
        height: AvailableSpace::Definite(val.height),
    }
}

fn from_layout_position(val: LayoutPosition) -> Position {
    match val {
        LayoutPosition::Static => Position::Relative, // Taffy treats Static as Relative
        LayoutPosition::Relative => Position::Relative,
        LayoutPosition::Absolute => Position::Absolute,
        LayoutPosition::Fixed => Position::Absolute, // Taffy doesn't distinguish Fixed
    }
}

fn from_taffy_point(val: taffy::Point<f32>) -> azul_core::window::LogicalPosition {
    azul_core::window::LogicalPosition { x: val.x, y: val.y }
}

fn from_flex_wrap(val: AzulFlexWrap) -> FlexWrap {
    match val {
        AzulFlexWrap::NoWrap => FlexWrap::NoWrap,
        AzulFlexWrap::Wrap => FlexWrap::Wrap,
        AzulFlexWrap::WrapReverse => FlexWrap::WrapReverse,
    }
}

fn from_flex_direction(val: AzulFlexDirection) -> FlexDirection {
    match val {
        AzulFlexDirection::Row => FlexDirection::Row,
        AzulFlexDirection::RowReverse => FlexDirection::RowReverse,
        AzulFlexDirection::Column => FlexDirection::Column,
        AzulFlexDirection::ColumnReverse => FlexDirection::ColumnReverse,
    }
}

fn from_align_items(val: AzulAlignItems) -> AlignItems {
    match val {
        AzulAlignItems::FlexStart => AlignItems::FlexStart,
        AzulAlignItems::FlexEnd => AlignItems::FlexEnd,
        AzulAlignItems::Center => AlignItems::Center,
        AzulAlignItems::Baseline => AlignItems::Baseline,
        AzulAlignItems::Stretch => AlignItems::Stretch,
    }
}

fn from_align_self(val: AzulAlignSelf) -> AlignSelf {
    match val {
        AzulAlignSelf::Auto => AlignSelf::FlexStart, // Taffy doesn't have Auto for AlignSelf
        AzulAlignSelf::FlexStart => AlignSelf::FlexStart,
        AzulAlignSelf::FlexEnd => AlignSelf::FlexEnd,
        AzulAlignSelf::Center => AlignSelf::Center,
        AzulAlignSelf::Baseline => AlignSelf::Baseline,
        AzulAlignSelf::Stretch => AlignSelf::Stretch,
    }
}

fn from_justify_content(val: AzulJustifyContent) -> JustifyContent {
    match val {
        AzulJustifyContent::FlexStart => JustifyContent::FlexStart,
        AzulJustifyContent::FlexEnd => JustifyContent::FlexEnd,
        AzulJustifyContent::Center => JustifyContent::Center,
        AzulJustifyContent::SpaceBetween => JustifyContent::SpaceBetween,
        AzulJustifyContent::SpaceAround => JustifyContent::SpaceAround,
        AzulJustifyContent::SpaceEvenly => JustifyContent::SpaceEvenly,
        _ => JustifyContent::FlexStart,
    }
}
