use core::fmt;
use alloc::collections::btree_map::BTreeMap;
use alloc::vec::Vec;
use azul_css::{
    LayoutRect, LayoutRectVec, LayoutPoint, LayoutSize, PixelValue, StyleFontSize,
    StyleTextColor, ColorU as StyleColorU, OptionF32,
    StyleTextAlignmentHorz, StyleTextAlignmentVert, LayoutPosition,
    CssPropertyValue, LayoutMarginTop, LayoutMarginRight, LayoutMarginLeft, LayoutMarginBottom,
    LayoutPaddingTop, LayoutPaddingLeft, LayoutPaddingRight, LayoutPaddingBottom,
    LayoutLeft, LayoutRight, LayoutTop, LayoutBottom, LayoutFlexDirection, LayoutJustifyContent,
};
use crate::{
    styled_dom::{StyledDom, AzNodeId, DomId},
    app_resources::{Words, ShapedWords, FontInstanceKey, WordPositions},
    id_tree::{NodeId, NodeDataContainer},
    dom::{DomNodeHash, ScrollTagId},
    callbacks::{PipelineId, HitTestItem, ScrollHitTestItem},
    window::{ScrollStates, LogicalRect, LogicalSize},
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
}

impl_vec!(InlineTextLayout, InlineTextLayoutVec, InlineTextLayoutVecDestructor);
impl_vec_clone!(InlineTextLayout, InlineTextLayoutVec, InlineTextLayoutVecDestructor);
impl_vec_debug!(InlineTextLayout, InlineTextLayoutVec);
impl_vec_partialeq!(InlineTextLayout, InlineTextLayoutVec);
impl_vec_partialord!(InlineTextLayout, InlineTextLayoutVec);

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

    #[inline]
    pub fn new(lines: Vec<InlineTextLine>) -> Self {
        Self { lines: lines.into() }
    }

    #[inline]
    #[must_use = "get_bounds calls union(self.lines) and is expensive to call"]
    pub fn get_bounds(&self) -> Option<LayoutRect> {
        // because of sub-pixel text positioning, calculating the bound has to be done using floating point
        LogicalRect::union(self.lines.as_ref().iter().map(|c| c.bounds)).map(|s| {
            LayoutRect {
                origin: LayoutPoint::new(libm::floorf(s.origin.x) as isize, libm::floorf(s.origin.y) as isize),
                size: LayoutSize::new(libm::ceilf(s.size.width) as isize, libm::ceilf(s.size.height) as isize),
            }
        })
    }

    #[must_use = "function is expensive to call since it iterates + collects over self.lines"]
    pub fn get_children_horizontal_diff_to_right_edge(&self, parent: &LayoutRect) -> Vec<f32> {
        let parent_right_edge = (parent.origin.x + parent.size.width) as f32;
        let parent_left_edge = parent.origin.x as f32;
        self.lines.as_ref().iter().map(|line| {
            let child_right_edge = line.bounds.origin.x + line.bounds.size.width;
            let child_left_edge = line.bounds.origin.x;
            ((child_left_edge - parent_left_edge) + (parent_right_edge - child_right_edge)) as f32
        }).collect()
    }

    /// Align the lines horizontal to *their bounding box*
    pub fn align_children_horizontal(&mut self, horizontal_alignment: StyleTextAlignmentHorz) {
        let shift_multiplier = match calculate_horizontal_shift_multiplier(horizontal_alignment) {
            None =>  return,
            Some(s) => s,
        };
        let self_bounds = match self.get_bounds() { Some(s) => s, None => { return; }, };
        let horz_diff = self.get_children_horizontal_diff_to_right_edge(&self_bounds);

        for (line, shift) in self.lines.as_mut().iter_mut().zip(horz_diff.into_iter()) {
            line.bounds.origin.x += shift * shift_multiplier;
        }
    }

    /// Align the lines vertical to *their parents container*
    pub fn align_children_vertical_in_parent_bounds(&mut self, parent_size: &LogicalSize, vertical_alignment: StyleTextAlignmentVert) {

        let shift_multiplier = match calculate_vertical_shift_multiplier(vertical_alignment) {
            None =>  return,
            Some(s) => s,
        };

        let self_bounds = match self.get_bounds() { Some(s) => s, None => { return; }, };
        let child_bottom_edge = (self_bounds.origin.y + self_bounds.size.height) as f32;
        let child_top_edge = self_bounds.origin.y as f32;
        let shift = child_top_edge + (parent_size.height - child_bottom_edge);

        for line in self.lines.as_mut().iter_mut() {
            line.bounds.origin.y += shift * shift_multiplier;
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
}

#[derive(Debug, Default, Copy, Clone, PartialEq)]
pub struct WidthCalculatedRect {
    pub preferred_width: WhConstraint,
    pub margin_right: Option<CssPropertyValue<LayoutMarginRight>>,
    pub margin_left: Option<CssPropertyValue<LayoutMarginLeft>>,
    pub padding_right: Option<CssPropertyValue<LayoutPaddingRight>>,
    pub padding_left: Option<CssPropertyValue<LayoutPaddingLeft>>,
    pub left: Option<CssPropertyValue<LayoutLeft>>,
    pub right: Option<CssPropertyValue<LayoutRight>>,
    pub flex_grow_px: f32,
    pub min_inner_size_px: f32,
}

impl WidthCalculatedRect {
    /// Get the flex basis in the horizontal direction - vertical axis has to be calculated differently
    pub fn get_flex_basis_horizontal(&self, parent_width: f32) -> f32 {
        self.preferred_width.min_needed_space().unwrap_or(0.0) +
        self.margin_left.as_ref().and_then(|p| p.get_property().map(|px| px.inner.to_pixels(parent_width))).unwrap_or(0.0) +
        self.margin_right.as_ref().and_then(|p| p.get_property().map(|px| px.inner.to_pixels(parent_width))).unwrap_or(0.0) +
        self.padding_left.as_ref().and_then(|p| p.get_property().map(|px| px.inner.to_pixels(parent_width))).unwrap_or(0.0) +
        self.padding_right.as_ref().and_then(|p| p.get_property().map(|px| px.inner.to_pixels(parent_width))).unwrap_or(0.0)
    }

    /// Get the sum of the horizontal padding amount (`padding.left + padding.right`)
    pub fn get_horizontal_padding(&self, parent_width: f32) -> f32 {
        self.padding_left.as_ref().and_then(|p| p.get_property().map(|px| px.inner.to_pixels(parent_width))).unwrap_or(0.0) +
        self.padding_right.as_ref().and_then(|p| p.get_property().map(|px| px.inner.to_pixels(parent_width))).unwrap_or(0.0)
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
    pub top: Option<CssPropertyValue<LayoutTop>>,
    pub bottom: Option<CssPropertyValue<LayoutBottom>>,
    pub flex_grow_px: f32,
    pub min_inner_size_px: f32,
}

impl HeightCalculatedRect {
    /// Get the flex basis in the horizontal direction - vertical axis has to be calculated differently
    pub fn get_flex_basis_vertical(&self, parent_height: f32) -> f32 {
        let parent_height = parent_height as f32;
        self.preferred_height.min_needed_space().unwrap_or(0.0) +
        self.margin_top.as_ref().and_then(|p| p.get_property().map(|px| px.inner.to_pixels(parent_height))).unwrap_or(0.0) +
        self.margin_bottom.as_ref().and_then(|p| p.get_property().map(|px| px.inner.to_pixels(parent_height))).unwrap_or(0.0) +
        self.padding_top.as_ref().and_then(|p| p.get_property().map(|px| px.inner.to_pixels(parent_height))).unwrap_or(0.0) +
        self.padding_bottom.as_ref().and_then(|p| p.get_property().map(|px| px.inner.to_pixels(parent_height))).unwrap_or(0.0)
    }

    /// Get the sum of the horizontal padding amount (`padding_top + padding_bottom`)
    pub fn get_vertical_padding(&self, parent_height: f32) -> f32 {
        self.padding_top.as_ref().and_then(|p| p.get_property().map(|px| px.inner.to_pixels(parent_height))).unwrap_or(0.0) +
        self.padding_bottom.as_ref().and_then(|p| p.get_property().map(|px| px.inner.to_pixels(parent_height))).unwrap_or(0.0)
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
    pub width_calculated_rects: NodeDataContainer<WidthCalculatedRect>,
    pub height_calculated_rects: NodeDataContainer<HeightCalculatedRect>,
    pub solved_pos_x: NodeDataContainer<HorizontalSolvedPosition>,
    pub solved_pos_y: NodeDataContainer<VerticalSolvedPosition>,
    pub layout_flex_grows: NodeDataContainer<f32>,
    pub layout_positions: NodeDataContainer<LayoutPosition>,
    pub layout_flex_directions: NodeDataContainer<LayoutFlexDirection>,
    pub layout_justify_contents: NodeDataContainer<LayoutJustifyContent>,
    pub rects: NodeDataContainer<PositionedRectangle>,
    pub words_cache: BTreeMap<NodeId, Words>,
    pub shaped_words_cache: BTreeMap<NodeId, ShapedWords>,
    pub positioned_words_cache: BTreeMap<NodeId, (WordPositions, FontInstanceKey)>,
    pub scrollable_nodes: ScrolledNodes,
    pub iframe_mapping: BTreeMap<NodeId, DomId>,
}

impl LayoutResult {
    pub fn get_bounds(&self) -> LayoutRect { LayoutRect::new(self.root_position, self.root_size) }
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
    pub fn get_hits(&self, cursor: &LayoutPoint, scroll_states: &ScrollStates) -> HitTest {

        // insert the regular hit items
        let regular_hit_test_nodes =
        self.styled_dom.tag_ids_to_node_ids
        .as_ref().iter().filter_map(|t| {

            let node_id = t.node_id.into_crate_internal()?;
            let layout_offset = self.rects.as_ref()[node_id].get_static_offset();
            let layout_size = LayoutSize::new(self.width_calculated_rects.as_ref()[node_id].total() as isize, self.height_calculated_rects.as_ref()[node_id].total() as isize);
            let layout_rect = LayoutRect::new(layout_offset, layout_size);

            layout_rect
            .hit_test(cursor)
            .map(|relative_to_item| {
                (node_id, HitTestItem {
                    point_in_viewport: *cursor,
                    point_relative_to_item: relative_to_item,
                    is_iframe_hit: self.iframe_mapping.get(&node_id).map(|iframe_dom_id| {
                        (*iframe_dom_id, layout_offset)
                    }),
                    is_focusable: self.styled_dom.node_data.as_container()[node_id].get_tab_index().into_option().is_some(),
                })
            })
        }).collect();

        // insert the scroll node hit items
        let scroll_hit_test_nodes = self.scrollable_nodes.tags_to_node_ids.iter().filter_map(|(_scroll_tag_id, node_id)| {

            let overflowing_scroll_node = self.scrollable_nodes.overflowing_nodes.get(node_id)?;
            let node_id = node_id.into_crate_internal()?;
            let scroll_state = scroll_states.get_scroll_position(&overflowing_scroll_node.parent_external_scroll_id)?;

            let mut scrolled_cursor = *cursor;
            scrolled_cursor.x += libm::roundf(scroll_state.x) as isize;
            scrolled_cursor.y += libm::roundf(scroll_state.y) as isize;

            let rect = overflowing_scroll_node.child_rect.clone();

            rect.hit_test(&scrolled_cursor).map(|relative_to_scroll| {
                (node_id, ScrollHitTestItem {
                    point_in_viewport: *cursor,
                    point_relative_to_item: relative_to_scroll,
                    scroll_node: overflowing_scroll_node.clone(),
                })
            })
        }).collect();

        HitTest {
            regular_hit_test_nodes,
            scroll_hit_test_nodes,
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
    /// If this is an inline rectangle, resolve the %-based font sizes
    /// and store them here.
    pub resolved_text_layout_options: Option<(ResolvedTextLayoutOptions, InlineTextLayout)>,
    /// Determines if the rect should be clipped or not (TODO: x / y as separate fields!)
    pub overflow: OverflowInfo,
}

impl Default for PositionedRectangle {
    fn default() -> Self {
        PositionedRectangle {
            size: LogicalSize::zero(),
            position: PositionInfo::Static { x_offset: 0.0, y_offset: 0.0, static_x_offset: 0.0, static_y_offset: 0.0 },
            padding: ResolvedOffsets::zero(),
            margin: ResolvedOffsets::zero(),
            border_widths: ResolvedOffsets::zero(),
            resolved_text_layout_options: None,
            overflow: OverflowInfo::default(),
        }
    }
}

#[derive(Debug, Default, Copy, Clone, PartialEq, PartialOrd)]
pub struct OverflowInfo {
    pub overflow_x: DirectionalOverflowInfo,
    pub overflow_y: DirectionalOverflowInfo,
}

// stores how much the children overflow the parent in the given direction
// if amount is negative, the children do not overflow the parent
// if the amount is set to None, that means there are no children for this node, so no overflow can be calculated
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
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

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub enum PositionInfo {
    Static { x_offset: f32, y_offset: f32, static_x_offset: f32, static_y_offset: f32 },
    Fixed { x_offset: f32, y_offset: f32, static_x_offset: f32, static_y_offset: f32 },
    Absolute { x_offset: f32, y_offset: f32, static_x_offset: f32, static_y_offset: f32 },
    Relative { x_offset: f32, y_offset: f32, static_x_offset: f32, static_y_offset: f32 },
}

impl PositionInfo {
    pub fn is_positioned(&self) -> bool {
        match self {
            PositionInfo::Static { .. } => false,
            PositionInfo::Fixed { .. } => true,
            PositionInfo::Absolute { .. } => true,
            PositionInfo::Relative { .. } => true,
        }
    }
}
impl PositionedRectangle {

    pub(crate) fn get_approximate_static_bounds(&self) -> LayoutRect {
        LayoutRect::new(self.get_static_offset(), self.get_content_size())
    }

    // Returns the rect where the content should be placed (for example the text itself)
    fn get_content_size(&self) -> LayoutSize {
        LayoutSize::new(libm::roundf(self.size.width) as isize, libm::roundf(self.size.height) as isize)
    }

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

    pub const fn to_layouted_rectangle(&self) -> LayoutedRectangle {
        LayoutedRectangle {
            size: self.size,
            position: self.position,
            padding: self.padding,
            margin: self.margin,
            border_widths: self.border_widths,
            overflow: self.overflow,
        }
    }

    // Returns the rect that includes bounds, expanded by the padding + the border widths
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

    pub fn get_margin_box_width(&self) -> f32 {
        self.size.width +
        self.padding.total_horizontal() +
        self.border_widths.total_horizontal() +
        self.margin.total_horizontal()
    }

    pub fn get_margin_box_height(&self) -> f32 {
        self.size.height +
        self.padding.total_vertical() +
        self.border_widths.total_vertical() +
        self.margin.total_vertical()
    }

    pub fn get_left_leading(&self) -> f32 {
        self.margin.left +
        self.padding.left +
        self.border_widths.left
    }

    pub fn get_top_leading(&self) -> f32 {
        self.margin.top +
        self.padding.top +
        self.border_widths.top
    }
}

/// Same as `PositionedRectangle`, but without the `text_layout_options`,
/// so that the struct implements `Copy`.
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub struct LayoutedRectangle {
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
    /// Determines if the rect should be clipped or not (TODO: x / y as separate fields!)
    pub overflow: OverflowInfo,
}