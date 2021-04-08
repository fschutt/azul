use core::fmt;
use alloc::collections::btree_map::BTreeMap;
use alloc::vec::Vec;
use azul_css::{
    LayoutRect, LayoutRectVec, LayoutPoint, LayoutSize, PixelValue, StyleFontSize,
    StyleTextColor, ColorU as StyleColorU, OptionF32, LayoutOverflow,
    StyleTextAlignmentHorz, StyleTextAlignmentVert, LayoutPosition,
    CssPropertyValue, LayoutMarginTop, LayoutMarginRight, LayoutMarginLeft, LayoutMarginBottom,
    LayoutPaddingTop, LayoutPaddingLeft, LayoutPaddingRight, LayoutPaddingBottom,
    LayoutLeft, LayoutRight, LayoutTop, LayoutBottom, LayoutFlexDirection, LayoutJustifyContent,
    LayoutBoxSizing, LayoutBorderRightWidth, LayoutBorderLeftWidth, LayoutBorderTopWidth,
    LayoutBorderBottomWidth, StyleTransform, StyleTransformOrigin, StyleBoxShadow,
};
use crate::{
    styled_dom::{StyledDom, AzNodeId, DomId},
    app_resources::{Words, ShapedWords, TransformKey, OpacityKey, FontInstanceKey, WordPositions},
    id_tree::{NodeId, NodeDataContainer, NodeDataContainerRef},
    dom::{DomNodeHash, ScrollTagId},
    callbacks::{PipelineId, HitTestItem, ScrollHitTestItem},
    window::{ScrollStates, LogicalPosition, LogicalRect, LogicalSize},
};

pub const DEFAULT_FONT_SIZE_PX: isize = 16;
pub const DEFAULT_FONT_SIZE: StyleFontSize = StyleFontSize { inner: PixelValue::const_px(DEFAULT_FONT_SIZE_PX) };
pub const DEFAULT_FONT_ID: &str = "serif";
pub const DEFAULT_TEXT_COLOR: StyleTextColor = StyleTextColor { inner: StyleColorU { r: 0, b: 0, g: 0, a: 255 } };
pub const DEFAULT_LINE_HEIGHT: f32 = 1.0;
pub const DEFAULT_WORD_SPACING: f32 = 1.0;
pub const DEFAULT_LETTER_SPACING: f32 = 0.0;
pub const DEFAULT_TAB_WIDTH: f32 = 4.0;

#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct InlineTextLayout {
    pub lines: InlineTextLineVec,
    pub content_size: LogicalSize,
}

impl_vec!(InlineTextLayout, InlineTextLayoutVec, InlineTextLayoutVecDestructor);
impl_vec_clone!(InlineTextLayout, InlineTextLayoutVec, InlineTextLayoutVecDestructor);
impl_vec_debug!(InlineTextLayout, InlineTextLayoutVec);
impl_vec_partialeq!(InlineTextLayout, InlineTextLayoutVec);
impl_vec_partialord!(InlineTextLayout, InlineTextLayoutVec);

/// NOTE: The bounds of the text line is the TOP left corner (relative to the text origin),
/// but the word_position is the BOTTOM left corner (relative to the text line)
#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct InlineTextLine {
    pub bounds: LogicalRect,
    /// At which word does this line start?
    pub word_start: usize,
    /// At which word does this line end
    pub word_end: usize,
}

impl_vec!(InlineTextLine, InlineTextLineVec, InlineTextLineVecDestructor);
impl_vec_clone!(InlineTextLine, InlineTextLineVec, InlineTextLineVecDestructor);
impl_vec_mut!(InlineTextLine, InlineTextLineVec);
impl_vec_debug!(InlineTextLine, InlineTextLineVec);
impl_vec_partialeq!(InlineTextLine, InlineTextLineVec);
impl_vec_partialord!(InlineTextLine, InlineTextLineVec);

impl InlineTextLine {
    pub const fn new(bounds: LogicalRect, word_start: usize, word_end: usize) -> Self {
        Self { bounds, word_start, word_end }
    }
}

impl InlineTextLayout {

    #[inline]
    pub fn get_leading(&self) -> f32 {
        match self.lines.as_ref().first() {
            None => 0.0,
            Some(s) => s.bounds.origin.x as f32,
        }
    }

    #[inline]
    pub fn get_trailing(&self) -> f32 {
        match self.lines.as_ref().first() {
            None => 0.0,
            Some(s) => (s.bounds.origin.x + s.bounds.size.width) as f32,
        }
    }

    /// Align the lines horizontal to *their bounding box*
    pub fn align_children_horizontal(
        &mut self,
        parent_size: &LogicalSize,
        horizontal_alignment: StyleTextAlignmentHorz
    ) {
        let shift_multiplier = match calculate_horizontal_shift_multiplier(horizontal_alignment) {
            None =>  return,
            Some(s) => s,
        };

        for line in self.lines.as_mut().iter_mut() {
            line.bounds.origin.x += shift_multiplier * (parent_size.width - line.bounds.size.width);
        }
    }

    /// Align the lines vertical to *their parents container*
    pub fn align_children_vertical_in_parent_bounds(
        &mut self,
        parent_size: &LogicalSize,
        vertical_alignment: StyleTextAlignmentVert
    ) {

        let shift_multiplier = match calculate_vertical_shift_multiplier(vertical_alignment) {
            None =>  return,
            Some(s) => s,
        };

        let glyphs_vertical_bottom = self.lines.as_ref().last().map(|l| l.bounds.origin.y).unwrap_or(0.0);
        let vertical_shift = (parent_size.height - glyphs_vertical_bottom) * shift_multiplier;

        for line in self.lines.as_mut().iter_mut() {
            line.bounds.origin.y += vertical_shift;
        }
    }
}

#[inline]
pub fn calculate_horizontal_shift_multiplier(horizontal_alignment: StyleTextAlignmentHorz) -> Option<f32> {
    use azul_css::StyleTextAlignmentHorz::*;
    match horizontal_alignment {
        Left => None,
        Center => Some(0.5), // move the line by the half width
        Right => Some(1.0), // move the line by the full width
    }
}

#[inline]
pub fn calculate_vertical_shift_multiplier(vertical_alignment: StyleTextAlignmentVert) -> Option<f32> {
    use azul_css::StyleTextAlignmentVert::*;
    match vertical_alignment {
        Top => None,
        Center => Some(0.5), // move the line by the half width
        Bottom => Some(1.0), // move the line by the full width
    }
}

#[derive(Clone, Copy, Eq, Hash, PartialEq, Ord, PartialOrd)]
#[repr(C)]
pub struct ExternalScrollId(pub u64, pub PipelineId);

impl ::core::fmt::Display for ExternalScrollId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "ExternalScrollId({:0x}, {})", self.0, self.1)
    }
}

impl ::core::fmt::Debug for ExternalScrollId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self)
    }
}

#[derive(Debug, Default, Clone, PartialEq, PartialOrd)]
pub struct ScrolledNodes {
    pub overflowing_nodes: BTreeMap<AzNodeId, OverflowingScrollNode>,
    /// Nodes that need to clip their direct children (i.e. nodes with overflow-x and overflow-y set to "Hidden")
    pub clip_nodes: BTreeMap<NodeId, LogicalSize>,
    pub tags_to_node_ids: BTreeMap<ScrollTagId, AzNodeId>,
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub struct OverflowingScrollNode {
    pub child_rect: LayoutRect,
    pub parent_external_scroll_id: ExternalScrollId,
    pub parent_dom_hash: DomNodeHash,
    pub scroll_tag_id: ScrollTagId,
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum WhConstraint {
    /// between min, max
    Between(f32, f32),
    /// Value needs to be exactly X
    EqualTo(f32),
    /// Value can be anything
    Unconstrained,
}

impl Default for WhConstraint {
    fn default() -> Self { WhConstraint::Unconstrained }
}

impl WhConstraint {

    /// Returns the minimum value or 0 on `Unconstrained`
    /// (warning: this might not be what you want)
    pub fn min_needed_space(&self) -> Option<f32> {
        use self::WhConstraint::*;
        match self {
            Between(min, _) => Some(*min),
            EqualTo(exact) => Some(*exact),
            Unconstrained => None,
        }
    }

    /// Returns the maximum space until the constraint is violated - returns
    /// `None` if the constraint is unbounded
    pub fn max_available_space(&self) -> Option<f32> {
        use self::WhConstraint::*;
        match self {
            Between(_, max) => { Some(*max) },
            EqualTo(exact) => Some(*exact),
            Unconstrained => None,
        }
    }

    /// Returns if this `WhConstraint` is an `EqualTo` constraint
    pub fn is_fixed_constraint(&self) -> bool {
        use self::WhConstraint::*;
        match self {
            EqualTo(_) => true,
            _ => false,
        }
    }

    // The absolute positioned node might have a max-width constraint, which has a
    // higher precedence than `top, bottom, left, right`.
    pub fn calculate_from_relative_parent(&self, relative_parent_width: f32) -> f32 {
        match self {
            WhConstraint::EqualTo(e) => *e,
            WhConstraint::Between(min, max) => {
                relative_parent_width.max(*min).min(*max)
            },
            WhConstraint::Unconstrained => relative_parent_width,
        }
    }
}

#[derive(Debug, Default, Copy, Clone, PartialEq)]
pub struct WidthCalculatedRect {
    pub preferred_width: WhConstraint,

    pub margin_right: Option<CssPropertyValue<LayoutMarginRight>>,
    pub margin_left: Option<CssPropertyValue<LayoutMarginLeft>>,

    pub padding_right: Option<CssPropertyValue<LayoutPaddingRight>>,
    pub padding_left: Option<CssPropertyValue<LayoutPaddingLeft>>,

    pub border_right: Option<CssPropertyValue<LayoutBorderRightWidth>>,
    pub border_left: Option<CssPropertyValue<LayoutBorderLeftWidth>>,

    pub box_sizing: LayoutBoxSizing,

    pub left: Option<CssPropertyValue<LayoutLeft>>,
    pub right: Option<CssPropertyValue<LayoutRight>>,

    pub flex_grow_px: f32,
    pub min_inner_size_px: f32,
}

impl WidthCalculatedRect {

    pub fn get_border_left(&self, percent_resolve: f32) -> f32 {
        self.border_left.as_ref()
        .and_then(|p| p.get_property().map(|px| px.inner.to_pixels(percent_resolve)))
        .unwrap_or(0.0)
    }

    pub fn get_border_right(&self, percent_resolve: f32) -> f32 {
        self.border_right.as_ref()
        .and_then(|p| p.get_property().map(|px| px.inner.to_pixels(percent_resolve)))
        .unwrap_or(0.0)
    }

    pub fn get_padding_left(&self, percent_resolve: f32) -> f32 {
        let mut self_padding_left = self.padding_left.as_ref()
            .and_then(|p| p.get_property().map(|px| px.inner.to_pixels(percent_resolve)))
            .unwrap_or(0.0);

        if self.box_sizing == LayoutBoxSizing::ContentBox {
            self_padding_left += self.get_border_left(percent_resolve);
        }

        self_padding_left
    }

    pub fn get_padding_right(&self, percent_resolve: f32) -> f32 {
        let mut self_padding_right = self.padding_right.as_ref()
            .and_then(|p| p.get_property().map(|px| px.inner.to_pixels(percent_resolve)))
            .unwrap_or(0.0);

        if self.box_sizing == LayoutBoxSizing::ContentBox {
            self_padding_right += self.get_border_right(percent_resolve);
        }

        self_padding_right
    }

    /// Get the flex basis in the horizontal direction - vertical axis has to be calculated differently
    pub fn get_flex_basis_horizontal(&self, parent_width: f32) -> f32 {
        self.min_inner_size_px +
        self.margin_left.as_ref().and_then(|p| p.get_property().map(|px| px.inner.to_pixels(parent_width))).unwrap_or(0.0) +
        self.margin_right.as_ref().and_then(|p| p.get_property().map(|px| px.inner.to_pixels(parent_width))).unwrap_or(0.0) +
        self.get_padding_left(parent_width) +
        self.get_padding_right(parent_width)
    }

    /// Get the sum of the horizontal padding amount (`padding.left + padding.right`)
    pub fn get_horizontal_padding(&self, parent_width: f32) -> f32 {
        self.get_padding_left(parent_width) +
        self.get_padding_right(parent_width)
    }

    /// Called after solver has run: Solved width of rectangle
    pub fn total(&self) -> f32 {
        self.min_inner_size_px + self.flex_grow_px
    }

    pub fn solved_result(&self) -> WidthSolvedResult {
        WidthSolvedResult {
            min_width: self.min_inner_size_px,
            space_added: self.flex_grow_px,
        }
    }
}

#[derive(Debug, Default, Copy, Clone, PartialEq)]
pub struct HeightCalculatedRect {
    pub preferred_height: WhConstraint,

    pub margin_top: Option<CssPropertyValue<LayoutMarginTop>>,
    pub margin_bottom: Option<CssPropertyValue<LayoutMarginBottom>>,

    pub padding_top: Option<CssPropertyValue<LayoutPaddingTop>>,
    pub padding_bottom: Option<CssPropertyValue<LayoutPaddingBottom>>,

    pub border_top: Option<CssPropertyValue<LayoutBorderTopWidth>>,
    pub border_bottom: Option<CssPropertyValue<LayoutBorderBottomWidth>>,

    pub top: Option<CssPropertyValue<LayoutTop>>,
    pub bottom: Option<CssPropertyValue<LayoutBottom>>,

    pub box_sizing: LayoutBoxSizing,

    pub flex_grow_px: f32,
    pub min_inner_size_px: f32,
}

impl HeightCalculatedRect {

    pub fn get_border_top(&self, percent_resolve: f32) -> f32 {
        self.border_top.as_ref()
        .and_then(|p| p.get_property().map(|px| px.inner.to_pixels(percent_resolve)))
        .unwrap_or(0.0)
    }

    pub fn get_border_bottom(&self, percent_resolve: f32) -> f32 {
        self.border_bottom.as_ref()
        .and_then(|p| p.get_property().map(|px| px.inner.to_pixels(percent_resolve)))
        .unwrap_or(0.0)
    }

    pub fn get_padding_bottom(&self, percent_resolve: f32) -> f32 {
        let mut self_padding_bottom = self.padding_bottom.as_ref()
            .and_then(|p| p.get_property().map(|px| px.inner.to_pixels(percent_resolve)))
            .unwrap_or(0.0);

        if self.box_sizing == LayoutBoxSizing::ContentBox {
            self_padding_bottom += self.get_border_bottom(percent_resolve);
        }

        self_padding_bottom
    }

    pub fn get_padding_top(&self, percent_resolve: f32) -> f32 {
        let mut self_padding_top = self.padding_top.as_ref()
            .and_then(|p| p.get_property().map(|px| px.inner.to_pixels(percent_resolve)))
            .unwrap_or(0.0);

        if self.box_sizing == LayoutBoxSizing::ContentBox {
            self_padding_top += self.get_border_top(percent_resolve);
        }

        self_padding_top
    }


    /// Get the flex basis in the horizontal direction - vertical axis has to be calculated differently
    pub fn get_flex_basis_vertical(&self, parent_height: f32) -> f32 {
        let parent_height = parent_height as f32;
        self.min_inner_size_px +
        self.margin_top.as_ref().and_then(|p| p.get_property().map(|px| px.inner.to_pixels(parent_height))).unwrap_or(0.0) +
        self.margin_bottom.as_ref().and_then(|p| p.get_property().map(|px| px.inner.to_pixels(parent_height))).unwrap_or(0.0) +
        self.get_padding_top(parent_height) +
        self.get_padding_bottom(parent_height)
    }

    /// Get the sum of the horizontal padding amount (`padding_top + padding_bottom`)
    pub fn get_vertical_padding(&self, parent_height: f32) -> f32 {
        self.get_padding_top(parent_height) +
        self.get_padding_bottom(parent_height)
    }

    /// Called after solver has run: Solved height of rectangle
    pub fn total(&self) -> f32 {
        self.min_inner_size_px + self.flex_grow_px
    }

    /// Called after solver has run: Solved width of rectangle
    pub fn solved_result(&self) -> HeightSolvedResult {
        HeightSolvedResult {
            min_height: self.min_inner_size_px,
            space_added: self.flex_grow_px,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct WidthSolvedResult {
    pub min_width: f32,
    pub space_added: f32,
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct HeightSolvedResult {
    pub min_height: f32,
    pub space_added: f32,
}

#[repr(transparent)]
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub struct HorizontalSolvedPosition(pub f32);

#[repr(transparent)]
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub struct VerticalSolvedPosition(pub f32);

#[derive(Debug)]
pub struct LayoutResult {
    pub dom_id: DomId,
    pub parent_dom_id: Option<DomId>,
    pub styled_dom: StyledDom,
    pub root_size: LayoutSize,
    pub root_position: LayoutPoint,
    pub preferred_widths: NodeDataContainer<Option<f32>>,
    pub preferred_heights: NodeDataContainer<Option<f32>>,
    pub width_calculated_rects: NodeDataContainer<WidthCalculatedRect>, // TODO: warning: large struct
    pub height_calculated_rects: NodeDataContainer<HeightCalculatedRect>, // TODO: warning: large struct
    pub solved_pos_x: NodeDataContainer<HorizontalSolvedPosition>,
    pub solved_pos_y: NodeDataContainer<VerticalSolvedPosition>,
    pub layout_flex_grows: NodeDataContainer<f32>,
    pub layout_positions: NodeDataContainer<LayoutPosition>,
    pub layout_flex_directions: NodeDataContainer<LayoutFlexDirection>,
    pub layout_justify_contents: NodeDataContainer<LayoutJustifyContent>,
    pub rects: NodeDataContainer<PositionedRectangle>,  // TODO: warning: large struct
    pub words_cache: BTreeMap<NodeId, Words>,
    pub shaped_words_cache: BTreeMap<NodeId, ShapedWords>,
    pub positioned_words_cache: BTreeMap<NodeId, (WordPositions, FontInstanceKey)>,
    pub scrollable_nodes: ScrolledNodes,
    pub iframe_mapping: BTreeMap<NodeId, DomId>,
    pub gpu_value_cache: GpuValueCache,
}

impl LayoutResult {
    pub fn get_bounds(&self) -> LayoutRect { LayoutRect::new(self.root_position, self.root_size) }
}

#[derive(Default, Debug, Clone, PartialEq, PartialOrd)]
pub struct GpuValueCache {
    pub transform_keys: BTreeMap<NodeId, TransformKey>,
    pub current_transform_values: BTreeMap<NodeId, ComputedTransform3D>,
    pub opacity_keys: BTreeMap<NodeId, OpacityKey>,
    pub current_opacity_values: BTreeMap<NodeId, f32>,
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum GpuTransformKeyEvent {
    Added(NodeId, TransformKey, ComputedTransform3D),
    Changed(NodeId, TransformKey, ComputedTransform3D, ComputedTransform3D),
    Removed(NodeId, TransformKey),
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum GpuOpacityKeyEvent {
    Added(NodeId, OpacityKey, f32),
    Changed(NodeId, OpacityKey, f32, f32),
    Removed(NodeId, OpacityKey),
}

#[derive(Default, Debug, Clone, PartialEq, PartialOrd)]
pub struct GpuEventChanges {
    pub transform_key_changes: Vec<GpuTransformKeyEvent>,
    pub opacity_key_changes: Vec<GpuOpacityKeyEvent>,
}

impl GpuEventChanges {
    pub fn empty() -> Self {
        Self::default()
    }
    pub fn is_empty(&self) -> bool {
        self.transform_key_changes.is_empty() &&
        self.opacity_key_changes.is_empty()
    }
}

#[derive(Default, Debug, Clone, PartialEq, PartialOrd)]
pub struct RelayoutChanges {
    pub resized_nodes: Vec<NodeId>,
    pub gpu_key_changes: GpuEventChanges,
}

impl RelayoutChanges {
    pub fn empty() -> Self {
        Self::default()
    }
}

impl GpuValueCache {

    pub fn empty() -> Self {
        Self::default()
    }

    #[cfg(feature = "multithreading")]
    #[must_use]
    pub fn synchronize<'a>(
        &mut self,
        positioned_rects: &NodeDataContainerRef<'a, PositionedRectangle>,
        styled_dom: &StyledDom,
    ) -> GpuEventChanges {

        use rayon::prelude::*;

        let css_property_cache = styled_dom.get_css_property_cache();
        let node_data = styled_dom.node_data.as_container();
        let node_states = styled_dom.styled_nodes.as_container();

        let default_transform_origin = StyleTransformOrigin::default();

        // calculate the transform values of every single node that has a non-default transform
        let all_current_transform_events = (0..styled_dom.node_data.len())
        .into_par_iter()
        .filter_map(|node_id| {
            let node_id = NodeId::new(node_id);
            let styled_node_state = &node_states[node_id].state;
            let node_data = &node_data[node_id];
            let current_transform = css_property_cache.get_transform(node_data, &node_id, styled_node_state)?.get_property().map(|t| {
                let parent_size = positioned_rects[node_id].size;
                let transform_origin = css_property_cache.get_transform_origin(node_data, &node_id, styled_node_state);
                let transform_origin = transform_origin
                    .as_ref()
                    .and_then(|o| o.get_property())
                    .unwrap_or(&default_transform_origin);
                ComputedTransform3D::from_style_transform_vec(t.as_ref(), transform_origin, parent_size.width, parent_size.height)
            });
            let existing_transform = self.current_transform_values.get(&node_id);

            match (existing_transform, current_transform) {
                (None, None) => None, // no new transform, no old transform
                (None, Some(new)) => Some(GpuTransformKeyEvent::Added(node_id, TransformKey::unique(), new)),
                (Some(old), Some(new)) => Some(GpuTransformKeyEvent::Changed(node_id, self.transform_keys.get(&node_id).copied()?, *old, new)),
                (Some(_old), None) => Some(GpuTransformKeyEvent::Removed(node_id, self.transform_keys.get(&node_id).copied()?)),
            }
        }).collect::<Vec<GpuTransformKeyEvent>>();

        // remove / add the transform keys accordingly
        for event in all_current_transform_events.iter() {
            match &event {
                GpuTransformKeyEvent::Added(node_id, key, matrix) => {
                    self.transform_keys.insert(*node_id, *key);
                    self.current_transform_values.insert(*node_id, *matrix);
                },
                GpuTransformKeyEvent::Changed(node_id, _key, _old_state, new_state) => {
                    self.current_transform_values.insert(*node_id, *new_state);
                },
                GpuTransformKeyEvent::Removed(node_id, _key) => {
                    self.transform_keys.remove(node_id);
                    self.current_transform_values.remove(node_id);
                },
            }
        }

        // calculate the opacity of every single node that has a non-default opacity
        let all_current_opacity_events = (0..styled_dom.node_data.len())
        .into_par_iter()
        .filter_map(|node_id| {
            let node_id = NodeId::new(node_id);
            let styled_node_state = &node_states[node_id].state;
            let node_data = &node_data[node_id];
            let current_opacity = css_property_cache.get_opacity(node_data, &node_id, styled_node_state)?;
            let current_opacity = current_opacity.get_property();
            let existing_opacity = self.current_opacity_values.get(&node_id);

            match (existing_opacity, current_opacity) {
                (None, None) => None, // no new opacity, no old transform
                (None, Some(new)) => Some(GpuOpacityKeyEvent::Added(node_id, OpacityKey::unique(), new.get())),
                (Some(old), Some(new)) => Some(GpuOpacityKeyEvent::Changed(node_id, self.opacity_keys.get(&node_id).copied()?, *old, new.get())),
                (Some(_old), None) => Some(GpuOpacityKeyEvent::Removed(node_id, self.opacity_keys.get(&node_id).copied()?)),
            }
        }).collect::<Vec<GpuOpacityKeyEvent>>();

        // remove / add the opacity keys accordingly
        for event in all_current_opacity_events.iter() {
            match &event {
                GpuOpacityKeyEvent::Added(node_id, key, opacity) => {
                    self.opacity_keys.insert(*node_id, *key);
                    self.current_opacity_values.insert(*node_id, *opacity);
                },
                GpuOpacityKeyEvent::Changed(node_id, _key, _old_state, new_state) => {
                    self.current_opacity_values.insert(*node_id, *new_state);
                },
                GpuOpacityKeyEvent::Removed(node_id, _key) => {
                    self.opacity_keys.remove(node_id);
                    self.current_opacity_values.remove(node_id);
                },
            }
        }

        GpuEventChanges {
            transform_key_changes: all_current_transform_events,
            opacity_key_changes: all_current_opacity_events,
        }
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct HitTest {
    pub regular_hit_test_nodes: BTreeMap<NodeId, HitTestItem>,
    pub scroll_hit_test_nodes: BTreeMap<NodeId, ScrollHitTestItem>,
}

impl HitTest {
    pub fn is_empty(&self) -> bool {
        self.regular_hit_test_nodes.is_empty() && self.scroll_hit_test_nodes.is_empty()
    }
}

impl LayoutResult {

    pub fn get_hits(&self, cursor: &LogicalPosition, scroll_states: &ScrollStates) -> HitTest {

        // TODO: SIMD-optimize!
        let css_property_cache = self.styled_dom.get_css_property_cache();
        let node_data = &self.styled_dom.node_data.as_container();
        let styled_nodes = &self.styled_dom.styled_nodes.as_container();

        // insert the regular hit items
        let regular_hit_test_nodes =
        self.styled_dom.tag_ids_to_node_ids
        .as_ref()
        .iter() // par_iter?
        .filter_map(|t| {

            let node_id = t.node_id.into_crate_internal()?;
            let logical_offset = self.rects.as_ref()[node_id].get_logical_static_offset();
            let logical_size = self.rects.as_ref()[node_id].size;
            let logical_rect = LogicalRect::new(logical_offset, logical_size);

            // Traverse up the tree and  calculate the transformation matrix of
            // this rect, then transform the point into the current coordinate space
            let mut transform = ComputedTransform3D::IDENTITY;
            let mut node = Some(node_id);

            while let Some(p) = node {

                // Go from the root and calculate the transformation matrix of this rect
                // Then transform the point into the current coordinate space
                let parent_transform = css_property_cache
                .get_transform(&node_data[p], &p, &styled_nodes[p].state)
                .and_then(|t| t.get_property())
                .map(|transform_vec| {

                    let parent_size = self.styled_dom.node_hierarchy.as_container()[p]
                    .parent_id().map(|p| self.rects.as_ref()[p].size)
                    .unwrap_or(LogicalSize::new(
                        self.root_size.width as f32,
                        self.root_size.height as f32,
                    ));

                    let transform_origin = css_property_cache
                    .get_transform_origin(&node_data[p], &p, &styled_nodes[p].state)
                    .and_then(|t| t.get_property())
                    .cloned()
                    .unwrap_or_default();

                    ComputedTransform3D::from_style_transform_vec(
                        transform_vec.as_ref(), &transform_origin,
                        parent_size.width, parent_size.height
                    )
                });

                if let Some(parent) = parent_transform {
                    transform = parent.then(&transform);
                }

                if let Some(parent) = self.styled_dom.node_hierarchy.as_container()[p].parent_id() {
                    node = Some(parent);
                } else {
                    break;
                }
            }

            // transform is computed now, un-project the rect into screen
            // space by using the inverse transform
            let inverse_transform = transform.inverse().unwrap_or(ComputedTransform3D::IDENTITY);
            let cursor_projected = inverse_transform.transform_point2d(*cursor).unwrap_or(*cursor);

            // TODO: If the item is a scroll rect, then also unproject
            // the scroll transform!

            logical_rect
            .hit_test(&cursor_projected)
            .map(|relative_to_item| {
                (node_id, HitTestItem {
                    point_in_viewport: *cursor,
                    point_relative_to_item: relative_to_item,
                    is_iframe_hit: self.iframe_mapping.get(&node_id).map(|iframe_dom_id| {
                        (*iframe_dom_id, logical_offset)
                    }),
                    is_focusable: self.styled_dom.node_data.as_container()[node_id].get_tab_index().into_option().is_some(),
                })
            })
        }).collect();

        /*
        // insert the scroll node hit items
        let scroll_hit_test_nodes = self.scrollable_nodes.tags_to_node_ids.iter().filter_map(|(_scroll_tag_id, node_id)| {

            let overflowing_scroll_node = self.scrollable_nodes.overflowing_nodes.get(node_id)?;
            let node_id = node_id.into_crate_internal()?;
            let scroll_state = scroll_states.get_scroll_position(&overflowing_scroll_node.parent_external_scroll_id)?;

            let mut scrolled_cursor = *cursor;
            scrolled_cursor.x += scroll_state.x;
            scrolled_cursor.y += scroll_state.y;

            let rect = overflowing_scroll_node.child_rect.clone();

            rect
            .hit_test(&scrolled_cursor)
            .map(|relative_to_scroll| {
                (node_id, ScrollHitTestItem {
                    point_in_viewport: *cursor,
                    point_relative_to_item: relative_to_scroll,
                    scroll_node: overflowing_scroll_node.clone(),
                })
            })
        }).collect();
        */

        HitTest {
            regular_hit_test_nodes,
            scroll_hit_test_nodes: BTreeMap::default(),
        }
    }
}

/// Layout options that can impact the flow of word positions
#[derive(Debug, Clone, PartialEq, PartialOrd, Default)]
pub struct TextLayoutOptions {
    /// Font size (in pixels) that this text has been laid out with
    pub font_size_px: PixelValue,
    /// Multiplier for the line height, default to 1.0
    pub line_height: Option<f32>,
    /// Additional spacing between glyphs (in pixels)
    pub letter_spacing: Option<PixelValue>,
    /// Additional spacing between words (in pixels)
    pub word_spacing: Option<PixelValue>,
    /// How many spaces should a tab character emulate
    /// (multiplying value, i.e. `4.0` = one tab = 4 spaces)?
    pub tab_width: Option<f32>,
    /// Maximum width of the text (in pixels) - if the text is set to `overflow:visible`, set this to None.
    pub max_horizontal_width: Option<f32>,
    /// How many pixels of leading does the first line have? Note that this added onto to the holes,
    /// so for effects like `:first-letter`, use a hole instead of a leading.
    pub leading: Option<f32>,
    /// This is more important for inline text layout where items can punch "holes"
    /// into the text flow, for example an image that floats to the right.
    ///
    /// TODO: Currently unused!
    pub holes: Vec<LayoutRect>,
}

/// Same as `TextLayoutOptions`, but with the widths / heights of the `PixelValue`s
/// resolved to regular f32s (because `letter_spacing`, `word_spacing`, etc. may be %-based value)
#[derive(Debug, Clone, PartialEq, PartialOrd, Default)]
pub struct ResolvedTextLayoutOptions {
    /// Font size (in pixels) that this text has been laid out with
    pub font_size_px: f32,
    /// Multiplier for the line height, default to 1.0
    pub line_height: OptionF32,
    /// Additional spacing between glyphs (in pixels)
    pub letter_spacing: OptionF32,
    /// Additional spacing between words (in pixels)
    pub word_spacing: OptionF32,
    /// How many spaces should a tab character emulate
    /// (multiplying value, i.e. `4.0` = one tab = 4 spaces)?
    pub tab_width: OptionF32,
    /// Maximum width of the text (in pixels) - if the text is set to `overflow:visible`, set this to None.
    pub max_horizontal_width: OptionF32,
    /// How many pixels of leading does the first line have? Note that this added onto to the holes,
    /// so for effects like `:first-letter`, use a hole instead of a leading.
    pub leading: OptionF32,
    /// This is more important for inline text layout where items can punch "holes"
    /// into the text flow, for example an image that floats to the right.
    ///
    /// TODO: Currently unused!
    pub holes: LayoutRectVec,
}

#[derive(Debug, Default, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct ResolvedOffsets {
    pub top: f32,
    pub left: f32,
    pub right: f32,
    pub bottom: f32,
}

impl ResolvedOffsets {
    pub const fn zero() -> Self { Self { top: 0.0, left: 0.0, right: 0.0, bottom: 0.0 } }
    pub fn total_vertical(&self) -> f32 { self.top + self.bottom }
    pub fn total_horizontal(&self) -> f32 { self.left + self.right }
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct PositionedRectangle {
    /// Outer bounds of the rectangle
    pub size: LogicalSize,
    /// How the rectangle should be positioned
    pub position: PositionInfo,
    /// Padding of the rectangle
    pub padding: ResolvedOffsets,
    /// Margin of the rectangle
    pub margin: ResolvedOffsets,
    /// Border widths of the rectangle
    pub border_widths: ResolvedOffsets,
    /// Widths of the box shadow(s), necessary to calculate clip rect
    pub box_shadow: StyleBoxShadowOffsets,
    /// Whether the borders are included in the size or not
    pub box_sizing: LayoutBoxSizing,
    /// Evaluated result of the overflow-x property
    pub overflow_x: LayoutOverflow,
    /// Evaluated result of the overflow-y property
    pub overflow_y: LayoutOverflow,
    // TODO: box_shadow_widths
    /// If this is an inline rectangle, resolve the %-based font sizes
    /// and store them here.
    pub resolved_text_layout_options: Option<(ResolvedTextLayoutOptions, InlineTextLayout)>,
}

impl Default for PositionedRectangle {
    fn default() -> Self {
        PositionedRectangle {
            size: LogicalSize::zero(),
            overflow_x: LayoutOverflow::default(),
            overflow_y: LayoutOverflow::default(),
            position: PositionInfo::Static {
                x_offset: 0.0,
                y_offset: 0.0,
                static_x_offset: 0.0,
                static_y_offset: 0.0
            },
            padding: ResolvedOffsets::zero(),
            margin: ResolvedOffsets::zero(),
            border_widths: ResolvedOffsets::zero(),
            box_shadow: StyleBoxShadowOffsets::default(),
            box_sizing: LayoutBoxSizing::default(),
            resolved_text_layout_options: None,
        }
    }
}

impl PositionedRectangle {

    #[inline]
    pub fn size_including_borders(&self) -> LogicalSize {
        let size_x = self.size.width + self.border_widths.total_horizontal();
        let size_y = self.size.height + self.border_widths.total_vertical();
        LogicalSize::new(size_x, size_y)
    }

    #[inline]
    pub fn get_approximate_static_bounds(&self) -> LayoutRect {
        LayoutRect::new(self.get_static_offset(), self.get_content_size())
    }

    // Returns the rect where the content should be placed (for example the text itself)
    #[inline]
    fn get_content_size(&self) -> LayoutSize {
        LayoutSize::new(libm::roundf(self.size.width) as isize, libm::roundf(self.size.height) as isize)
    }

    #[inline]
    fn get_logical_static_offset(&self) -> LogicalPosition {
        match self.position {
            PositionInfo::Static { static_x_offset, static_y_offset, .. } |
            PositionInfo::Fixed { static_x_offset, static_y_offset, .. } |
            PositionInfo::Absolute { static_x_offset, static_y_offset, .. } |
            PositionInfo::Relative { static_x_offset, static_y_offset, .. } => {
                LogicalPosition::new(static_x_offset, static_y_offset)
            },
        }
    }

    #[inline]
    fn get_static_offset(&self) -> LayoutPoint {
        match self.position {
            PositionInfo::Static { static_x_offset, static_y_offset, .. } |
            PositionInfo::Fixed { static_x_offset, static_y_offset, .. } |
            PositionInfo::Absolute { static_x_offset, static_y_offset, .. } |
            PositionInfo::Relative { static_x_offset, static_y_offset, .. } => {
                LayoutPoint::new(libm::roundf(static_x_offset) as isize, libm::roundf(static_y_offset) as isize)
            },
        }
    }

    // Returns the rect that includes bounds, expanded by the padding + the border widths
    #[inline]
    pub fn get_background_bounds(&self) -> (LogicalSize, PositionInfo) {

        use crate::ui_solver::PositionInfo::*;

        let b_size = LogicalSize {
            width: self.size.width + self.padding.total_horizontal() + self.border_widths.total_horizontal(),
            height: self.size.height + self.padding.total_vertical() + self.border_widths.total_vertical(),
        };

        let x_offset_add = 0.0 - self.padding.left - self.border_widths.left;
        let y_offset_add = 0.0 - self.padding.top - self.border_widths.top;

        let b_position = match self.position {
            Static { x_offset, y_offset, static_x_offset, static_y_offset } => Static { x_offset: x_offset + x_offset_add, y_offset: y_offset + y_offset_add, static_x_offset, static_y_offset },
            Fixed { x_offset, y_offset, static_x_offset, static_y_offset } => Fixed { x_offset: x_offset + x_offset_add, y_offset: y_offset + y_offset_add, static_x_offset, static_y_offset },
            Relative { x_offset, y_offset, static_x_offset, static_y_offset } => Relative { x_offset: x_offset + x_offset_add, y_offset: y_offset + y_offset_add, static_x_offset, static_y_offset },
            Absolute { x_offset, y_offset, static_x_offset, static_y_offset } => Absolute { x_offset: x_offset + x_offset_add, y_offset: y_offset + y_offset_add, static_x_offset, static_y_offset },
        };

        (b_size, b_position)
    }

    #[inline]
    pub fn get_margin_box_width(&self) -> f32 {
        self.size.width +
        self.padding.total_horizontal() +
        self.border_widths.total_horizontal() +
        self.margin.total_horizontal()
    }

    #[inline]
    pub fn get_margin_box_height(&self) -> f32 {
        self.size.height +
        self.padding.total_vertical() +
        self.border_widths.total_vertical() +
        self.margin.total_vertical()
    }

    #[inline]
    pub fn get_left_leading(&self) -> f32 {
        self.margin.left +
        self.padding.left +
        self.border_widths.left
    }

    #[inline]
    pub fn get_top_leading(&self) -> f32 {
        self.margin.top +
        self.padding.top +
        self.border_widths.top
    }
}

#[derive(Debug, Default, Clone, PartialEq, PartialOrd)]
pub struct OverflowInfo {
    pub overflow_x: DirectionalOverflowInfo,
    pub overflow_y: DirectionalOverflowInfo,
}

// stores how much the children overflow the parent in the given direction
// if amount is negative, the children do not overflow the parent
// if the amount is set to None, that means there are no children for this node, so no overflow can be calculated
#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum DirectionalOverflowInfo {
    Scroll { amount: Option<isize> },
    Auto { amount: Option<isize> },
    Hidden { amount: Option<isize> },
    Visible { amount: Option<isize> },
}

impl Default for DirectionalOverflowInfo {
    fn default() -> DirectionalOverflowInfo {
        DirectionalOverflowInfo::Auto { amount: None }
    }
}

impl DirectionalOverflowInfo {

    #[inline]
    pub fn get_amount(&self) -> Option<isize> {
        match self {
            DirectionalOverflowInfo::Scroll { amount: Some(s) } |
            DirectionalOverflowInfo::Auto { amount: Some(s) } |
            DirectionalOverflowInfo::Hidden { amount: Some(s) } |
            DirectionalOverflowInfo::Visible { amount: Some(s) } => Some(*s),
            _ => None
        }
    }

    #[inline]
    pub fn is_negative(&self) -> bool {
        match self {
            DirectionalOverflowInfo::Scroll { amount: Some(s) } |
            DirectionalOverflowInfo::Auto { amount: Some(s) } |
            DirectionalOverflowInfo::Hidden { amount: Some(s) } |
            DirectionalOverflowInfo::Visible { amount: Some(s) } => { *s < 0_isize },
            _ => true // no overflow = no scrollbar
        }
    }

    #[inline]
    pub fn is_none(&self) -> bool {
        match self {
            DirectionalOverflowInfo::Scroll { amount: None } |
            DirectionalOverflowInfo::Auto { amount: None } |
            DirectionalOverflowInfo::Hidden { amount: None } |
            DirectionalOverflowInfo::Visible { amount: None } => true,
            _ => false
        }
    }
}

#[derive(Clone, PartialEq, PartialOrd)]
pub enum PositionInfo {
    Static { x_offset: f32, y_offset: f32, static_x_offset: f32, static_y_offset: f32 },
    Fixed { x_offset: f32, y_offset: f32, static_x_offset: f32, static_y_offset: f32 },
    Absolute { x_offset: f32, y_offset: f32, static_x_offset: f32, static_y_offset: f32 },
    Relative { x_offset: f32, y_offset: f32, static_x_offset: f32, static_y_offset: f32 },
}

impl ::core::fmt::Debug for PositionInfo {
    fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
        match self {
            PositionInfo::Static { x_offset, y_offset, .. } => write!(f, "static({}, {})", x_offset, y_offset),
            PositionInfo::Fixed { x_offset, y_offset, .. } => write!(f, "fixed({}, {})", x_offset, y_offset),
            PositionInfo::Absolute { x_offset, y_offset, .. } => write!(f, "absolute({}, {})", x_offset, y_offset),
            PositionInfo::Relative { x_offset, y_offset, .. } => write!(f, "relative({}, {})", x_offset, y_offset),
        }
    }
}

impl PositionInfo {
    #[inline]
    pub fn is_positioned(&self) -> bool {
        match self {
            PositionInfo::Static { .. } => false,
            PositionInfo::Fixed { .. } => true,
            PositionInfo::Absolute { .. } => true,
            PositionInfo::Relative { .. } => true,
        }
    }
    #[inline]
    pub fn get_relative_offset(&self) -> (f32, f32) {
        match self {
            PositionInfo::Static { x_offset, y_offset, .. } |
            PositionInfo::Fixed { x_offset, y_offset, .. } |
            PositionInfo::Absolute { x_offset, y_offset, .. } |
            PositionInfo::Relative { x_offset, y_offset, .. } => (*x_offset, *y_offset)
        }
    }
}

#[derive(Default, Debug, Copy, Clone, PartialEq, PartialOrd)]
pub struct StyleBoxShadowOffsets {
    pub left: Option<CssPropertyValue<StyleBoxShadow>>,
    pub right: Option<CssPropertyValue<StyleBoxShadow>>,
    pub top: Option<CssPropertyValue<StyleBoxShadow>>,
    pub bottom: Option<CssPropertyValue<StyleBoxShadow>>,
}

/// Computed transform of pixels in pixel space, optimized
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(packed)]
pub struct ComputedTransform3D {
    pub m:[[f32;4];4]
}

impl ComputedTransform3D {

    pub const IDENTITY: Self = Self {
        m: [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ]
    };

    pub const fn new(
        m11: f32, m12: f32, m13: f32, m14: f32,
        m21: f32, m22: f32, m23: f32, m24: f32,
        m31: f32, m32: f32, m33: f32, m34: f32,
        m41: f32, m42: f32, m43: f32, m44: f32
    ) -> Self {
        Self {
            m: [
                [m11, m12, m13, m14],
                [m21, m22, m23, m24],
                [m31, m32, m33, m34],
                [m41, m42, m43, m44],
            ]
        }
    }

    pub const fn new_2d(
        m11: f32, m12: f32,
        m21: f32, m22: f32,
        m41: f32, m42: f32
    ) -> Self {
         Self::new(
             m11,  m12, 0.0, 0.0,
             m21,  m22, 0.0, 0.0,
             0.0,  0.0, 1.0, 0.0,
             m41,  m42, 0.0, 1.0
        )
    }

    // Computes the matrix of a rect from a Vec<StyleTransform>
    pub fn from_style_transform_vec(
        t_vec: &[StyleTransform],
        transform_origin: &StyleTransformOrigin,
        percent_resolve_x: f32,
        percent_resolve_y: f32,
    ) -> Self {

        // TODO: use correct SIMD optimization!
        let mut matrix = Self::IDENTITY;

        for t in t_vec.iter() {
            matrix = matrix.then(&Self::from_style_transform(
                t, transform_origin,
                percent_resolve_x, percent_resolve_y
            ));
        }

        matrix
    }

    /// Creates a new transform from a style transform using the
    /// parent width as a way to resolve for percentages
    pub fn from_style_transform(
        t: &StyleTransform,
        transform_origin: &StyleTransformOrigin,
        percent_resolve_x: f32,
        percent_resolve_y: f32,
    ) -> Self {
        use azul_css::StyleTransform::*;
        match t {
            Matrix(mat2d) => {
                let a = mat2d.a.to_pixels(percent_resolve_x);
                let b = mat2d.b.to_pixels(percent_resolve_x);
                let c = mat2d.c.to_pixels(percent_resolve_x);
                let d = mat2d.d.to_pixels(percent_resolve_x);
                let tx = mat2d.tx.to_pixels(percent_resolve_x);
                let ty = mat2d.ty.to_pixels(percent_resolve_x);

                Self::new_2d(a, b, c, d, tx, ty)
            },
            Matrix3D(mat3d) => {
                let m11 = mat3d.m11.to_pixels(percent_resolve_x);
                let m12 = mat3d.m12.to_pixels(percent_resolve_x);
                let m13 = mat3d.m13.to_pixels(percent_resolve_x);
                let m14 = mat3d.m14.to_pixels(percent_resolve_x);
                let m21 = mat3d.m21.to_pixels(percent_resolve_x);
                let m22 = mat3d.m22.to_pixels(percent_resolve_x);
                let m23 = mat3d.m23.to_pixels(percent_resolve_x);
                let m24 = mat3d.m24.to_pixels(percent_resolve_x);
                let m31 = mat3d.m31.to_pixels(percent_resolve_x);
                let m32 = mat3d.m32.to_pixels(percent_resolve_x);
                let m33 = mat3d.m33.to_pixels(percent_resolve_x);
                let m34 = mat3d.m34.to_pixels(percent_resolve_x);
                let m41 = mat3d.m41.to_pixels(percent_resolve_x);
                let m42 = mat3d.m42.to_pixels(percent_resolve_x);
                let m43 = mat3d.m43.to_pixels(percent_resolve_x);
                let m44 = mat3d.m44.to_pixels(percent_resolve_x);

                Self::new(
                    m11,
                    m12,
                    m13,
                    m14,
                    m21,
                    m22,
                    m23,
                    m24,
                    m31,
                    m32,
                    m33,
                    m34,
                    m41,
                    m42,
                    m43,
                    m44,
                )
            },
            Translate(trans2d) => Self::new_translation(
                trans2d.x.to_pixels(percent_resolve_x),
                trans2d.y.to_pixels(percent_resolve_y),
                0.0
            ),
            Translate3D(trans3d) => Self::new_translation(
                trans3d.x.to_pixels(percent_resolve_x),
                trans3d.y.to_pixels(percent_resolve_y),
                trans3d.z.to_pixels(percent_resolve_x) // ???
            ),
            TranslateX(trans_x) => Self::new_translation(trans_x.to_pixels(percent_resolve_x), 0.0, 0.0),
            TranslateY(trans_y) => Self::new_translation(0.0, trans_y.to_pixels(percent_resolve_y), 0.0),
            TranslateZ(trans_z) => Self::new_translation(0.0, 0.0, trans_z.to_pixels(percent_resolve_x)), // ???
            Rotate3D(rot3d) => {
                let rotation_origin = (
                    transform_origin.x.to_pixels(percent_resolve_x),
                    transform_origin.y.to_pixels(percent_resolve_y)
                );
                Self::make_rotation(
                    rotation_origin,
                    rot3d.angle.to_degrees(),
                    rot3d.x.normalized(),
                    rot3d.y.normalized(),
                    rot3d.z.normalized(),
                )
            },
            RotateX(angle_x) => {
                let rotation_origin = (
                    transform_origin.x.to_pixels(percent_resolve_x),
                    transform_origin.y.to_pixels(percent_resolve_y)
                );
                Self::make_rotation(
                    rotation_origin,
                    angle_x.to_degrees(),
                    1.0,
                    0.0,
                    0.0,
                )
            },
            RotateY(angle_y) => {
                let rotation_origin = (
                    transform_origin.x.to_pixels(percent_resolve_x),
                    transform_origin.y.to_pixels(percent_resolve_y)
                );
                Self::make_rotation(
                    rotation_origin,
                    angle_y.to_degrees(),
                    0.0,
                    1.0,
                    0.0,
                )
            },
            Rotate(angle_z) | RotateZ(angle_z) => {
                let rotation_origin = (
                    transform_origin.x.to_pixels(percent_resolve_x),
                    transform_origin.y.to_pixels(percent_resolve_y)
                );
                Self::make_rotation(
                    rotation_origin,
                    angle_z.to_degrees(),
                    0.0,
                    0.0,
                    1.0,
                )
            },
            Scale(scale2d) => Self::new_scale(
                scale2d.x.normalized(),
                scale2d.y.normalized(),
                0.0,
            ),
            Scale3D(scale3d) => Self::new_scale(
                scale3d.x.normalized(),
                scale3d.y.normalized(),
                scale3d.z.normalized(),
            ),
            ScaleX(scale_x) => Self::new_scale(scale_x.normalized(), 0.0, 0.0),
            ScaleY(scale_y) => Self::new_scale(0.0, scale_y.normalized(), 0.0),
            ScaleZ(scale_z) => Self::new_scale(0.0, 0.0, scale_z.normalized()),
            Skew(skew2d) => Self::new_skew(skew2d.x.normalized(), skew2d.y.normalized()),
            SkewX(skew_x) => Self::new_skew(skew_x.normalized(), 0.0),
            SkewY(skew_y) => Self::new_skew(0.0, skew_y.normalized()),
            Perspective(px) => Self::new_perspective(px.to_pixels(percent_resolve_x)),
        }
    }

    #[inline]
    pub const fn new_scale(x: f32, y: f32, z: f32) -> Self {
        Self::new(
            x,   0.0, 0.0, 0.0,
            0.0, y,   0.0, 0.0,
            0.0, 0.0, z,   0.0,
            0.0, 0.0, 0.0, 1.0,
        )
    }

    #[inline]
    pub const fn new_translation(x: f32, y: f32, z: f32) -> Self {
        Self::new(
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
             x,  y,   z,   1.0,
        )
    }

    #[inline]
    pub fn new_perspective(d: f32) -> Self {
        Self::new(
            1.0, 0.0, 0.0,  0.0,
            0.0, 1.0, 0.0,  0.0,
            0.0, 0.0, 1.0, -1.0 / d,
            0.0, 0.0, 0.0,  1.0,
        )
    }

    /// Create a 3d rotation transform from an angle / axis.
    /// The supplied axis must be normalized.
    #[inline]
    pub fn new_rotation(x: f32, y: f32, z: f32, theta_radians: f32) -> Self {

        let xx = x * x;
        let yy = y * y;
        let zz = z * z;

        let half_theta = theta_radians / 2.0;
        let sc = half_theta.sin() * half_theta.cos();
        let sq = half_theta.sin() * half_theta.sin();

        Self::new(
            1.0 - 2.0 * (yy + zz) * sq,
            2.0 * (x * y * sq + z * sc),
            2.0 * (x * z * sq - y * sc),
            0.0,


            2.0 * (x * y * sq - z * sc),
            1.0 - 2.0 * (xx + zz) * sq,
            2.0 * (y * z * sq + x * sc),
            0.0,

            2.0 * (x * z * sq + y * sc),
            2.0 * (y * z * sq - x * sc),
            1.0 - 2.0 * (xx + yy) * sq,
            0.0,

            0.0,
            0.0,
            0.0,
            1.0
        )
    }

    #[inline]
    pub fn new_skew(alpha: f32, beta: f32) -> Self {
        let (sx, sy) = (beta.to_radians().tan(), alpha.to_radians().tan());
        Self::new(
            1.0, sx,  0.0, 0.0,
            sy,  1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        )
    }

    // Transforms a 2D point into the target coordinate space
    #[must_use]
    pub fn transform_point2d(&self, p: LogicalPosition) -> Option<LogicalPosition> {
        let w = p.x.mul_add(self.m[0][3], p.y.mul_add(self.m[1][3], self.m[3][3]));

        if !w.is_sign_positive() { return None; }

        let x = p.x.mul_add(self.m[0][0], p.y.mul_add(self.m[1][0], self.m[3][0]));
        let y = p.x.mul_add(self.m[0][1], p.y.mul_add(self.m[1][1], self.m[3][1]));

        Some(LogicalPosition { x: x / w, y: y / w })
    }

    /// Computes the sum of two matrices while applying `other` AFTER the current matrix.
    #[must_use]
    pub fn then(&self, other: &Self) -> Self {
        Self::new(
            self.m[0][0].mul_add(other.m[0][0], self.m[0][1].mul_add(other.m[1][0], self.m[0][2].mul_add(other.m[2][0], self.m[0][3] * other.m[3][0]))),
            self.m[0][0].mul_add(other.m[0][1], self.m[0][1].mul_add(other.m[1][1], self.m[0][2].mul_add(other.m[2][1], self.m[0][3] * other.m[3][1]))),
            self.m[0][0].mul_add(other.m[0][2], self.m[0][1].mul_add(other.m[1][2], self.m[0][2].mul_add(other.m[2][2], self.m[0][3] * other.m[3][2]))),
            self.m[0][0].mul_add(other.m[0][3], self.m[0][1].mul_add(other.m[1][3], self.m[0][2].mul_add(other.m[2][3], self.m[0][3] * other.m[3][3]))),

            self.m[1][0].mul_add(other.m[0][0], self.m[1][1].mul_add(other.m[1][0], self.m[1][2].mul_add(other.m[2][0], self.m[1][3] * other.m[3][0]))),
            self.m[1][0].mul_add(other.m[0][1], self.m[1][1].mul_add(other.m[1][1], self.m[1][2].mul_add(other.m[2][1], self.m[1][3] * other.m[3][1]))),
            self.m[1][0].mul_add(other.m[0][2], self.m[1][1].mul_add(other.m[1][2], self.m[1][2].mul_add(other.m[2][2], self.m[1][3] * other.m[3][2]))),
            self.m[1][0].mul_add(other.m[0][3], self.m[1][1].mul_add(other.m[1][3], self.m[1][2].mul_add(other.m[2][3], self.m[1][3] * other.m[3][3]))),

            self.m[2][0].mul_add(other.m[0][0], self.m[2][1].mul_add(other.m[1][0], self.m[2][2].mul_add(other.m[2][0], self.m[2][3] * other.m[3][0]))),
            self.m[2][0].mul_add(other.m[0][1], self.m[2][1].mul_add(other.m[1][1], self.m[2][2].mul_add(other.m[2][1], self.m[2][3] * other.m[3][1]))),
            self.m[2][0].mul_add(other.m[0][2], self.m[2][1].mul_add(other.m[1][2], self.m[2][2].mul_add(other.m[2][2], self.m[2][3] * other.m[3][2]))),
            self.m[2][0].mul_add(other.m[0][3], self.m[2][1].mul_add(other.m[1][3], self.m[2][2].mul_add(other.m[2][3], self.m[2][3] * other.m[3][3]))),

            self.m[3][0].mul_add(other.m[0][0], self.m[3][1].mul_add(other.m[1][0], self.m[3][2].mul_add(other.m[2][0], self.m[3][3] * other.m[3][0]))),
            self.m[3][0].mul_add(other.m[0][1], self.m[3][1].mul_add(other.m[1][1], self.m[3][2].mul_add(other.m[2][1], self.m[3][3] * other.m[3][1]))),
            self.m[3][0].mul_add(other.m[0][2], self.m[3][1].mul_add(other.m[1][2], self.m[3][2].mul_add(other.m[2][2], self.m[3][3] * other.m[3][2]))),
            self.m[3][0].mul_add(other.m[0][3], self.m[3][1].mul_add(other.m[1][3], self.m[3][2].mul_add(other.m[2][3], self.m[3][3] * other.m[3][3]))),
        )
    }

    /// Computes the inverse of the matrix, returns None if the determinant is zero.
    #[must_use]
    pub fn inverse(&self) -> Option<Self> {
        let det = self.determinant();

        if det == 0.0 {
            return None;
        }

        // todo(gw): this could be made faster by special casing
        // for simpler transform types.
        let m = Self::new(
            self.m[1][2]*self.m[2][3]*self.m[3][1] - self.m[1][3]*self.m[3][2]*self.m[3][1] +
            self.m[1][3]*self.m[2][1]*self.m[3][2] - self.m[1][1]*self.m[2][3]*self.m[3][2] -
            self.m[1][2]*self.m[2][1]*self.m[3][3] + self.m[1][1]*self.m[3][2]*self.m[3][3],

            self.m[0][3]*self.m[3][2]*self.m[3][1] - self.m[0][2]*self.m[2][3]*self.m[3][1] -
            self.m[0][3]*self.m[2][1]*self.m[3][2] + self.m[0][1]*self.m[2][3]*self.m[3][2] +
            self.m[0][2]*self.m[2][1]*self.m[3][3] - self.m[0][1]*self.m[3][2]*self.m[3][3],

            self.m[0][2]*self.m[1][3]*self.m[3][1] - self.m[0][3]*self.m[1][2]*self.m[3][1] +
            self.m[0][3]*self.m[1][1]*self.m[3][2] - self.m[0][1]*self.m[1][3]*self.m[3][2] -
            self.m[0][2]*self.m[1][1]*self.m[3][3] + self.m[0][1]*self.m[1][2]*self.m[3][3],

            self.m[0][3]*self.m[1][2]*self.m[2][1] - self.m[0][2]*self.m[1][3]*self.m[2][1] -
            self.m[0][3]*self.m[1][1]*self.m[3][2] + self.m[0][1]*self.m[1][3]*self.m[3][2] +
            self.m[0][2]*self.m[1][1]*self.m[2][3] - self.m[0][1]*self.m[1][2]*self.m[2][3],

            self.m[1][3]*self.m[3][2]*self.m[3][0] - self.m[1][2]*self.m[2][3]*self.m[3][0] -
            self.m[1][3]*self.m[2][0]*self.m[3][2] + self.m[1][0]*self.m[2][3]*self.m[3][2] +
            self.m[1][2]*self.m[2][0]*self.m[3][3] - self.m[1][0]*self.m[3][2]*self.m[3][3],

            self.m[0][2]*self.m[2][3]*self.m[3][0] - self.m[0][3]*self.m[3][2]*self.m[3][0] +
            self.m[0][3]*self.m[2][0]*self.m[3][2] - self.m[0][0]*self.m[2][3]*self.m[3][2] -
            self.m[0][2]*self.m[2][0]*self.m[3][3] + self.m[0][0]*self.m[3][2]*self.m[3][3],

            self.m[0][3]*self.m[1][2]*self.m[3][0] - self.m[0][2]*self.m[1][3]*self.m[3][0] -
            self.m[0][3]*self.m[1][0]*self.m[3][2] + self.m[0][0]*self.m[1][3]*self.m[3][2] +
            self.m[0][2]*self.m[1][0]*self.m[3][3] - self.m[0][0]*self.m[1][2]*self.m[3][3],

            self.m[0][2]*self.m[1][3]*self.m[2][0] - self.m[0][3]*self.m[1][2]*self.m[2][0] +
            self.m[0][3]*self.m[1][0]*self.m[3][2] - self.m[0][0]*self.m[1][3]*self.m[3][2] -
            self.m[0][2]*self.m[1][0]*self.m[2][3] + self.m[0][0]*self.m[1][2]*self.m[2][3],

            self.m[1][1]*self.m[2][3]*self.m[3][0] - self.m[1][3]*self.m[2][1]*self.m[3][0] +
            self.m[1][3]*self.m[2][0]*self.m[3][1] - self.m[1][0]*self.m[2][3]*self.m[3][1] -
            self.m[1][1]*self.m[2][0]*self.m[3][3] + self.m[1][0]*self.m[2][1]*self.m[3][3],

            self.m[0][3]*self.m[2][1]*self.m[3][0] - self.m[0][1]*self.m[2][3]*self.m[3][0] -
            self.m[0][3]*self.m[2][0]*self.m[3][1] + self.m[0][0]*self.m[2][3]*self.m[3][1] +
            self.m[0][1]*self.m[2][0]*self.m[3][3] - self.m[0][0]*self.m[2][1]*self.m[3][3],

            self.m[0][1]*self.m[1][3]*self.m[3][0] - self.m[0][3]*self.m[1][1]*self.m[3][0] +
            self.m[0][3]*self.m[1][0]*self.m[3][1] - self.m[0][0]*self.m[1][3]*self.m[3][1] -
            self.m[0][1]*self.m[1][0]*self.m[3][3] + self.m[0][0]*self.m[1][1]*self.m[3][3],

            self.m[0][3]*self.m[1][1]*self.m[2][0] - self.m[0][1]*self.m[1][3]*self.m[2][0] -
            self.m[0][3]*self.m[1][0]*self.m[2][1] + self.m[0][0]*self.m[1][3]*self.m[2][1] +
            self.m[0][1]*self.m[1][0]*self.m[2][3] - self.m[0][0]*self.m[1][1]*self.m[2][3],

            self.m[1][2]*self.m[2][1]*self.m[3][0] - self.m[1][1]*self.m[3][2]*self.m[3][0] -
            self.m[1][2]*self.m[2][0]*self.m[3][1] + self.m[1][0]*self.m[3][2]*self.m[3][1] +
            self.m[1][1]*self.m[2][0]*self.m[3][2] - self.m[1][0]*self.m[2][1]*self.m[3][2],

            self.m[0][1]*self.m[3][2]*self.m[3][0] - self.m[0][2]*self.m[2][1]*self.m[3][0] +
            self.m[0][2]*self.m[2][0]*self.m[3][1] - self.m[0][0]*self.m[3][2]*self.m[3][1] -
            self.m[0][1]*self.m[2][0]*self.m[3][2] + self.m[0][0]*self.m[2][1]*self.m[3][2],

            self.m[0][2]*self.m[1][1]*self.m[3][0] - self.m[0][1]*self.m[1][2]*self.m[3][0] -
            self.m[0][2]*self.m[1][0]*self.m[3][1] + self.m[0][0]*self.m[1][2]*self.m[3][1] +
            self.m[0][1]*self.m[1][0]*self.m[3][2] - self.m[0][0]*self.m[1][1]*self.m[3][2],

            self.m[0][1]*self.m[1][2]*self.m[2][0] - self.m[0][2]*self.m[1][1]*self.m[2][0] +
            self.m[0][2]*self.m[1][0]*self.m[2][1] - self.m[0][0]*self.m[1][2]*self.m[2][1] -
            self.m[0][1]*self.m[1][0]*self.m[3][2] + self.m[0][0]*self.m[1][1]*self.m[3][2]
        );

        Some(m.multiply_scalar(1.0 / det))
    }

    /// Compute the determinant of the transform.
    #[inline]
    pub fn determinant(&self) -> f32 {
        // TODO: SIMD
        self.m[0][3] * self.m[1][2] * self.m[2][1] * self.m[3][0] -
        self.m[0][2] * self.m[1][3] * self.m[2][1] * self.m[3][0] -
        self.m[0][3] * self.m[1][1] * self.m[2][2] * self.m[3][0] +
        self.m[0][1] * self.m[1][3] * self.m[2][2] * self.m[3][0] +
        self.m[0][2] * self.m[1][1] * self.m[2][3] * self.m[3][0] -
        self.m[0][1] * self.m[1][2] * self.m[2][3] * self.m[3][0] -
        self.m[0][3] * self.m[1][2] * self.m[2][0] * self.m[3][1] +
        self.m[0][2] * self.m[1][3] * self.m[2][0] * self.m[3][1] +
        self.m[0][3] * self.m[1][0] * self.m[2][2] * self.m[3][1] -
        self.m[0][0] * self.m[1][3] * self.m[2][2] * self.m[3][1] -
        self.m[0][2] * self.m[1][0] * self.m[2][3] * self.m[3][1] +
        self.m[0][0] * self.m[1][2] * self.m[2][3] * self.m[3][1] +
        self.m[0][3] * self.m[1][1] * self.m[2][0] * self.m[3][2] -
        self.m[0][1] * self.m[1][3] * self.m[2][0] * self.m[3][2] -
        self.m[0][3] * self.m[1][0] * self.m[2][1] * self.m[3][2] +
        self.m[0][0] * self.m[1][3] * self.m[2][1] * self.m[3][2] +
        self.m[0][1] * self.m[1][0] * self.m[2][3] * self.m[3][2] -
        self.m[0][0] * self.m[1][1] * self.m[2][3] * self.m[3][2] -
        self.m[0][2] * self.m[1][1] * self.m[2][0] * self.m[3][3] +
        self.m[0][1] * self.m[1][2] * self.m[2][0] * self.m[3][3] +
        self.m[0][2] * self.m[1][0] * self.m[2][1] * self.m[3][3] -
        self.m[0][0] * self.m[1][2] * self.m[2][1] * self.m[3][3] -
        self.m[0][1] * self.m[1][0] * self.m[2][2] * self.m[3][3] +
        self.m[0][0] * self.m[1][1] * self.m[2][2] * self.m[3][3]
    }

    /// Multiplies all of the transform's component by a scalar and returns the result.
    #[must_use]
    #[inline]
    pub fn multiply_scalar(&self, x: f32) -> Self {
        Self::new(
            self.m[0][0] * x, self.m[0][1] * x, self.m[0][2] * x, self.m[0][3] * x,
            self.m[1][0] * x, self.m[1][1] * x, self.m[1][2] * x, self.m[1][3] * x,
            self.m[2][0] * x, self.m[2][1] * x, self.m[2][2] * x, self.m[2][3] * x,
            self.m[3][0] * x, self.m[3][1] * x, self.m[3][2] * x, self.m[3][3] * x
        )
    }

    /*

    #[inline]
    #[must_use]
    pub unsafe fn then_sse(&self, x: f32) -> Self { }
    #[inline]
    #[must_use]
    pub unsafe fn then_avx4(&self, x: f32) -> Self { }
    #[inline]
    #[must_use]
    pub unsafe fn then_avx8(&self, x: f32) -> Self { }

    #[inline]
    #[must_use]
    pub unsafe fn inverse_sse(&self, x: f32) -> Self { }
    #[inline]
    #[must_use]
    pub unsafe fn inverse_avx4(&self, x: f32) -> Self { }
    #[inline]
    #[must_use]
    pub unsafe fn inverse_avx8(&self, x: f32) -> Self { }

    #[inline]
    #[must_use]
    pub unsafe fn determinant_sse(&self) -> f32 { }
    #[inline]
    #[must_use]
    pub unsafe fn determinant_avx4(&self) -> f32 { }
    #[inline]
    #[must_use]
    pub unsafe fn determinant_avx8(&self) -> f32 { }

    #[inline]
    #[must_use]
    pub unsafe fn multiply_scalar_sse(&self, x: f32) -> Self { }
    #[inline]
    #[must_use]
    pub unsafe fn multiply_scalar_avx4(&self, x: f32) -> Self { }
    #[inline]
    #[must_use]
    pub unsafe fn multiply_scalar_avx8(&self, x: f32) -> Self { }

    */

    #[inline]
    pub fn make_rotation(
        rotation_origin: (f32, f32),
        degrees: f32,
        axis_x: f32,
        axis_y: f32,
        axis_z: f32,
    ) -> Self {

        let degrees = -degrees; // CSS rotations are clockwise

        let (origin_x, origin_y) = rotation_origin;
        let pre_transform = Self::new_translation(-origin_x, -origin_y, -0.0);
        let post_transform = Self::new_translation(origin_x, origin_y, 0.0);
        let theta = 2.0_f32 * core::f32::consts::PI - degrees.to_radians();
        let rotate_transform = Self::new_rotation(axis_x, axis_y, axis_z, theta).then(&Self::IDENTITY);

        pre_transform
        .then(&rotate_transform)
        .then(&post_transform)
    }
}
