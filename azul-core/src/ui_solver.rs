use crate::{
    app_resources::{
        Epoch, FontInstanceKey, GlTextureCache, IdNamespace, ImageCache, OpacityKey,
        RendererResources, ShapedWords, TransformKey, UpdateImageResult, WordPositions, Words,
    },
    callbacks::{
        DocumentId, HidpiAdjustedBounds, HitTestItem, IFrameCallbackInfo, IFrameCallbackReturn,
        PipelineId, ScrollHitTestItem,
    },
    display_list::{CachedDisplayList, RenderCallbacks},
    dom::{DomNodeHash, ScrollTagId, TagId},
    gl::OptionGlContextPtr,
    id_tree::{NodeDataContainer, NodeDataContainerRef, NodeId},
    styled_dom::{DomId, NodeHierarchyItemId, StyledDom},
    window::{
        FullWindowState, LogicalPosition, LogicalRect, LogicalRectVec, LogicalSize, ScrollStates,
        WindowSize, WindowTheme,
    },
    window_state::RelayoutFn,
};
use alloc::boxed::Box;
use alloc::collections::btree_map::BTreeMap;
use alloc::vec::Vec;
use azul_css::{
    ColorU as StyleColorU, CssPropertyValue, LayoutBorderBottomWidth, LayoutBorderLeftWidth,
    LayoutBorderRightWidth, LayoutBorderTopWidth, LayoutBottom, LayoutBoxSizing, LayoutDisplay,
    LayoutFlexDirection, LayoutJustifyContent, LayoutLeft, LayoutMarginBottom, LayoutMarginLeft,
    LayoutMarginRight, LayoutMarginTop, LayoutOverflow, LayoutPaddingBottom, LayoutPaddingLeft,
    LayoutPaddingRight, LayoutPaddingTop, LayoutPoint, LayoutPosition, LayoutRect, LayoutRectVec,
    LayoutRight, LayoutSize, LayoutTop, OptionF32, PixelValue, StyleBoxShadow, StyleFontSize,
    StyleTextAlign, StyleTextColor, StyleTransform, StyleTransformOrigin, StyleVerticalAlign,
};
#[cfg(target_arch = "x86_64")]
use core::arch::x86_64::__m256;
use core::fmt;
use core::sync::atomic::AtomicBool;
use core::sync::atomic::Ordering as AtomicOrdering;
use rust_fontconfig::FcFontCache;

static INITIALIZED: AtomicBool = AtomicBool::new(false);
static USE_AVX: AtomicBool = AtomicBool::new(false);
static USE_SSE: AtomicBool = AtomicBool::new(false);

pub const DEFAULT_FONT_SIZE_PX: isize = 16;
pub const DEFAULT_FONT_SIZE: StyleFontSize = StyleFontSize {
    inner: PixelValue::const_px(DEFAULT_FONT_SIZE_PX),
};
pub const DEFAULT_FONT_ID: &str = "serif";
pub const DEFAULT_TEXT_COLOR: StyleTextColor = StyleTextColor {
    inner: StyleColorU {
        r: 0,
        b: 0,
        g: 0,
        a: 255,
    },
};
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

impl_vec!(
    InlineTextLayout,
    InlineTextLayoutVec,
    InlineTextLayoutVecDestructor
);
impl_vec_clone!(
    InlineTextLayout,
    InlineTextLayoutVec,
    InlineTextLayoutVecDestructor
);
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

impl_vec!(
    InlineTextLine,
    InlineTextLineVec,
    InlineTextLineVecDestructor
);
impl_vec_clone!(
    InlineTextLine,
    InlineTextLineVec,
    InlineTextLineVecDestructor
);
impl_vec_mut!(InlineTextLine, InlineTextLineVec);
impl_vec_debug!(InlineTextLine, InlineTextLineVec);
impl_vec_partialeq!(InlineTextLine, InlineTextLineVec);
impl_vec_partialord!(InlineTextLine, InlineTextLineVec);

impl InlineTextLine {
    pub const fn new(bounds: LogicalRect, word_start: usize, word_end: usize) -> Self {
        Self {
            bounds,
            word_start,
            word_end,
        }
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
        horizontal_alignment: StyleTextAlign,
    ) {
        let shift_multiplier = match calculate_horizontal_shift_multiplier(horizontal_alignment) {
            None => return,
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
        vertical_alignment: StyleVerticalAlign,
    ) {
        let shift_multiplier = match calculate_vertical_shift_multiplier(vertical_alignment) {
            None => return,
            Some(s) => s,
        };

        let glyphs_vertical_bottom = self
            .lines
            .as_ref()
            .last()
            .map(|l| l.bounds.origin.y)
            .unwrap_or(0.0);
        let vertical_shift = (parent_size.height - glyphs_vertical_bottom) * shift_multiplier;

        for line in self.lines.as_mut().iter_mut() {
            line.bounds.origin.y += vertical_shift;
        }
    }
}

#[inline]
pub fn calculate_horizontal_shift_multiplier(horizontal_alignment: StyleTextAlign) -> Option<f32> {
    use azul_css::StyleTextAlign::*;
    match horizontal_alignment {
        Left => None,
        Center => Some(0.5), // move the line by the half width
        Right => Some(1.0),  // move the line by the full width
    }
}

#[inline]
pub fn calculate_vertical_shift_multiplier(vertical_alignment: StyleVerticalAlign) -> Option<f32> {
    use azul_css::StyleVerticalAlign::*;
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
        write!(f, "ExternalScrollId({})", self.0)
    }
}

impl ::core::fmt::Debug for ExternalScrollId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self)
    }
}

#[derive(Debug, Default, Clone, PartialEq, PartialOrd)]
pub struct ScrolledNodes {
    pub overflowing_nodes: BTreeMap<NodeHierarchyItemId, OverflowingScrollNode>,
    /// Nodes that need to clip their direct children (i.e. nodes with overflow-x and overflow-y set to "Hidden")
    pub clip_nodes: BTreeMap<NodeId, LogicalSize>,
    pub tags_to_node_ids: BTreeMap<ScrollTagId, NodeHierarchyItemId>,
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub struct OverflowingScrollNode {
    pub parent_rect: LogicalRect,
    pub child_rect: LogicalRect,
    pub virtual_child_rect: LogicalRect,
    pub parent_external_scroll_id: ExternalScrollId,
    pub parent_dom_hash: DomNodeHash,
    pub scroll_tag_id: ScrollTagId,
}

impl Default for OverflowingScrollNode {
    fn default() -> Self {
        use crate::dom::TagId;
        Self {
            parent_rect: LogicalRect::zero(),
            child_rect: LogicalRect::zero(),
            virtual_child_rect: LogicalRect::zero(),
            parent_external_scroll_id: ExternalScrollId(0, PipelineId::DUMMY),
            parent_dom_hash: DomNodeHash(0),
            scroll_tag_id: ScrollTagId(TagId(0)),
        }
    }
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
    fn default() -> Self {
        WhConstraint::Unconstrained
    }
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
            Between(_, max) => Some(*max),
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
            WhConstraint::Between(min, max) => relative_parent_width.max(*min).min(*max),
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
    pub fn overflow_width(&self) -> f32 {
        if !self.flex_grow_px.is_sign_positive() {
            self.min_inner_size_px
        } else {
            self.min_inner_size_px + self.flex_grow_px
        }
    }

    pub fn get_border_left(&self, percent_resolve: f32) -> f32 {
        self.border_left
            .as_ref()
            .and_then(|p| {
                p.get_property()
                    .map(|px| px.inner.to_pixels(percent_resolve))
            })
            .unwrap_or(0.0)
    }

    pub fn get_border_right(&self, percent_resolve: f32) -> f32 {
        self.border_right
            .as_ref()
            .and_then(|p| {
                p.get_property()
                    .map(|px| px.inner.to_pixels(percent_resolve))
            })
            .unwrap_or(0.0)
    }

    pub fn get_raw_padding_left(&self, percent_resolve: f32) -> f32 {
        self.padding_left
            .as_ref()
            .and_then(|p| {
                p.get_property()
                    .map(|px| px.inner.to_pixels(percent_resolve))
            })
            .unwrap_or(0.0)
    }

    pub fn get_raw_padding_right(&self, percent_resolve: f32) -> f32 {
        self.padding_right
            .as_ref()
            .and_then(|p| {
                p.get_property()
                    .map(|px| px.inner.to_pixels(percent_resolve))
            })
            .unwrap_or(0.0)
    }

    pub fn get_padding_left(&self, percent_resolve: f32) -> f32 {
        self.get_raw_padding_left(percent_resolve) + self.get_border_left(percent_resolve)
    }

    pub fn get_padding_right(&self, percent_resolve: f32) -> f32 {
        self.get_raw_padding_right(percent_resolve) + self.get_border_right(percent_resolve)
    }

    pub fn get_margin_left(&self, percent_resolve: f32) -> f32 {
        self.margin_left
            .as_ref()
            .and_then(|p| {
                p.get_property()
                    .map(|px| px.inner.to_pixels(percent_resolve))
            })
            .unwrap_or(0.0)
    }

    pub fn get_margin_right(&self, percent_resolve: f32) -> f32 {
        self.margin_right
            .as_ref()
            .and_then(|p| {
                p.get_property()
                    .map(|px| px.inner.to_pixels(percent_resolve))
            })
            .unwrap_or(0.0)
    }

    /// Get the flex basis in the horizontal direction - vertical axis has to be calculated differently
    pub fn get_flex_basis_horizontal(&self, parent_width: f32) -> f32 {
        self.min_inner_size_px
            + self.get_margin_left(parent_width)
            + self.get_margin_right(parent_width)
            + self.get_raw_padding_left(parent_width)
            + self.get_raw_padding_right(parent_width)
            + self.get_border_left(parent_width)
            + self.get_border_right(parent_width)
    }

    pub fn get_horizontal_border(&self, parent_width: f32) -> f32 {
        self.get_border_left(parent_width) + self.get_border_right(parent_width)
    }

    /// Get the sum of the horizontal padding amount (`padding.left + padding.right`)
    pub fn get_horizontal_padding(&self, parent_width: f32) -> f32 {
        self.get_padding_left(parent_width) + self.get_padding_right(parent_width)
    }

    /// Get the sum of the horizontal padding amount (`margin.left + margin.right`)
    pub fn get_horizontal_margin(&self, parent_width: f32) -> f32 {
        self.get_margin_left(parent_width) + self.get_margin_right(parent_width)
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
    pub fn overflow_height(&self) -> f32 {
        if !self.flex_grow_px.is_sign_positive() {
            self.min_inner_size_px
        } else {
            self.min_inner_size_px + self.flex_grow_px
        }
    }

    pub fn get_border_top(&self, percent_resolve: f32) -> f32 {
        self.border_top
            .as_ref()
            .and_then(|p| {
                p.get_property()
                    .map(|px| px.inner.to_pixels(percent_resolve))
            })
            .unwrap_or(0.0)
    }

    pub fn get_border_bottom(&self, percent_resolve: f32) -> f32 {
        self.border_bottom
            .as_ref()
            .and_then(|p| {
                p.get_property()
                    .map(|px| px.inner.to_pixels(percent_resolve))
            })
            .unwrap_or(0.0)
    }

    pub fn get_raw_padding_top(&self, percent_resolve: f32) -> f32 {
        self.padding_top
            .as_ref()
            .and_then(|p| {
                p.get_property()
                    .map(|px| px.inner.to_pixels(percent_resolve))
            })
            .unwrap_or(0.0)
    }

    pub fn get_raw_padding_bottom(&self, percent_resolve: f32) -> f32 {
        self.padding_bottom
            .as_ref()
            .and_then(|p| {
                p.get_property()
                    .map(|px| px.inner.to_pixels(percent_resolve))
            })
            .unwrap_or(0.0)
    }

    pub fn get_padding_bottom(&self, percent_resolve: f32) -> f32 {
        self.get_raw_padding_bottom(percent_resolve) + self.get_border_bottom(percent_resolve)
    }

    pub fn get_padding_top(&self, percent_resolve: f32) -> f32 {
        self.get_raw_padding_top(percent_resolve) + self.get_border_top(percent_resolve)
    }

    pub fn get_margin_top(&self, percent_resolve: f32) -> f32 {
        self.margin_top
            .as_ref()
            .and_then(|p| {
                p.get_property()
                    .map(|px| px.inner.to_pixels(percent_resolve))
            })
            .unwrap_or(0.0)
    }

    pub fn get_margin_bottom(&self, percent_resolve: f32) -> f32 {
        self.margin_bottom
            .as_ref()
            .and_then(|p| {
                p.get_property()
                    .map(|px| px.inner.to_pixels(percent_resolve))
            })
            .unwrap_or(0.0)
    }

    /// Get the flex basis in the horizontal direction - vertical axis has to be calculated differently
    pub fn get_flex_basis_vertical(&self, parent_height: f32) -> f32 {
        self.min_inner_size_px
            + self.get_margin_top(parent_height)
            + self.get_margin_bottom(parent_height)
            + self.get_raw_padding_top(parent_height)
            + self.get_raw_padding_bottom(parent_height)
            + self.get_border_top(parent_height)
            + self.get_border_bottom(parent_height)
    }

    /// Get the sum of the horizontal padding amount (`padding_top + padding_bottom`)
    pub fn get_vertical_padding(&self, parent_height: f32) -> f32 {
        self.get_padding_top(parent_height) + self.get_padding_bottom(parent_height)
    }

    /// Get the sum of the horizontal padding amount (`padding_top + padding_bottom`)
    pub fn get_vertical_border(&self, parent_height: f32) -> f32 {
        self.get_border_top(parent_height) + self.get_border_bottom(parent_height)
    }

    /// Get the sum of the horizontal margin amount (`margin_top + margin_bottom`)
    pub fn get_vertical_margin(&self, parent_height: f32) -> f32 {
        self.get_margin_top(parent_height) + self.get_margin_bottom(parent_height)
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
    pub layout_displays: NodeDataContainer<CssPropertyValue<LayoutDisplay>>,
    pub layout_positions: NodeDataContainer<LayoutPosition>,
    pub layout_flex_directions: NodeDataContainer<LayoutFlexDirection>,
    pub layout_justify_contents: NodeDataContainer<LayoutJustifyContent>,
    pub rects: NodeDataContainer<PositionedRectangle>, // TODO: warning: large struct
    pub words_cache: BTreeMap<NodeId, Words>,
    pub shaped_words_cache: BTreeMap<NodeId, ShapedWords>,
    pub positioned_words_cache: BTreeMap<NodeId, (WordPositions, FontInstanceKey)>,
    pub scrollable_nodes: ScrolledNodes,
    pub iframe_mapping: BTreeMap<NodeId, DomId>,
    pub gpu_value_cache: GpuValueCache,
}

impl fmt::Debug for LayoutResult {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "LayoutResult {{
            dom_id: {},
            bounds: {:?} @ {:?},
            styled_dom (len = {}): {:#?},
            preferred_widths(len = {}),
            preferred_heights(len = {}),
            width_calculated_rects(len = {}),
            height_calculated_rects(len = {}),
            solved_pos_x(len = {}),
            solved_pos_y(len = {}),
            layout_flex_grows(len = {}),
            layout_displays(len = {}),
            layout_positions(len = {}),
            layout_flex_directions(len = {}),
            layout_justify_contents(len = {}),
            rects(len = {}),
            words_cache(len = {}),
            shaped_words_cache(len = {}),
            positioned_words_cache(len = {}),
            scrollable_nodes: {:#?},
            iframe_mapping(len = {}): {:#?},
            gpu_value_cache: {:#?},
        }}",
            self.dom_id.inner,
            self.root_size,
            self.root_position,
            self.styled_dom.node_hierarchy.len(),
            self.styled_dom,
            self.preferred_widths.len(),
            self.preferred_heights.len(),
            self.width_calculated_rects.len(),
            self.height_calculated_rects.len(),
            self.solved_pos_x.len(),
            self.solved_pos_y.len(),
            self.layout_flex_grows.len(),
            self.layout_displays.len(),
            self.layout_positions.len(),
            self.layout_flex_directions.len(),
            self.layout_justify_contents.len(),
            self.rects.len(),
            self.words_cache.len(),
            self.shaped_words_cache.len(),
            self.positioned_words_cache.len(),
            self.scrollable_nodes,
            self.iframe_mapping.len(),
            self.iframe_mapping,
            self.gpu_value_cache,
        )
    }
}

pub struct QuickResizeResult {
    pub gpu_event_changes: GpuEventChanges,
    pub updated_images: Vec<UpdateImageResult>,
    pub resized_nodes: BTreeMap<DomId, Vec<NodeId>>,
}

impl LayoutResult {
    pub fn get_bounds(&self) -> LayoutRect {
        LayoutRect::new(self.root_position, self.root_size)
    }

    #[cfg(feature = "multithreading")]
    pub fn get_cached_display_list(
        document_id: &DocumentId,
        dom_id: DomId,
        epoch: Epoch,
        layout_results: &[LayoutResult],
        full_window_state: &FullWindowState,
        gl_texture_cache: &GlTextureCache,
        renderer_resources: &RendererResources,
        image_cache: &ImageCache,
    ) -> CachedDisplayList {
        use crate::display_list::{
            displaylist_handle_rect, push_rectangles_into_displaylist, DisplayListFrame,
            DisplayListMsg, DisplayListParametersRef, LayoutRectContent, RectBackground,
        };
        use rayon::prelude::*;

        let layout_result = match layout_results.get(dom_id.inner) {
            Some(s) => s,
            None => return CachedDisplayList::empty(),
        };

        let rects_in_rendering_order = layout_result.styled_dom.get_rects_in_rendering_order();
        let referenced_content = DisplayListParametersRef {
            dom_id,
            document_id,
            epoch,
            full_window_state,
            layout_results,
            gl_texture_cache,
            renderer_resources,
            image_cache,
        };

        let root_width =
            layout_result.width_calculated_rects.as_ref()[NodeId::ZERO].overflow_width();
        let root_height =
            layout_result.height_calculated_rects.as_ref()[NodeId::ZERO].overflow_height();
        let root_size = LogicalSize::new(root_width, root_height);

        let mut root_content = displaylist_handle_rect(
            rects_in_rendering_order.root.into_crate_internal().unwrap(),
            &referenced_content,
        )
        .unwrap_or(DisplayListMsg::Frame(DisplayListFrame::root(
            LayoutSize::zero(),
            LayoutPoint::zero(),
        )));

        let children = rects_in_rendering_order
            .children
            .as_ref()
            .par_iter()
            .filter_map(|child_content_group| {
                push_rectangles_into_displaylist(child_content_group, &referenced_content)
            })
            .collect();

        root_content.append_children(children);

        let mut dl = CachedDisplayList {
            root: root_content,
            root_size,
        };

        // push the window background color, if the
        // root node doesn't have any content
        if dl.root.is_content_empty() {
            dl.root.push_content(LayoutRectContent::Background {
                content: RectBackground::Color(full_window_state.background_color),
                size: None,
                offset: None,
                repeat: None,
            });
        }

        dl
    }

    // Does a "quick" re-layout of the given DomId, calls iframe callbacks that have been resized
    // and updates their DOM IDs.
    //
    // Assumes that an OpenGL context is active
    #[must_use]
    pub fn do_quick_resize(
        id_namespace: IdNamespace,
        document_id: DocumentId,
        epoch: Epoch,
        dom_id: DomId,
        image_cache: &ImageCache,
        gl_context: &OptionGlContextPtr,
        layout_results: &mut [LayoutResult],
        gl_texture_cache: &mut GlTextureCache,
        renderer_resources: &mut RendererResources,
        callbacks: &RenderCallbacks,
        relayout_fn: RelayoutFn,
        fc_cache: &FcFontCache,
        window_size: &WindowSize,
        window_theme: WindowTheme,
    ) -> QuickResizeResult {
        let dom_bounds = LogicalRect::new(LogicalPosition::zero(), window_size.dimensions);
        let mut dom_ids_to_resize = vec![(dom_id, dom_bounds)];
        let mut gpu_event_changes = GpuEventChanges::default();
        let mut rsn = BTreeMap::new(); // resized nodes [DomID => Vec<NodeId>]

        loop {
            let mut new_dom_ids_to_resize = Vec::new();

            for (dom_id, new_size) in dom_ids_to_resize.iter() {
                let layout_size = new_size.to_layout_rect();

                // Call the relayout function on the DOM to get the resized DOM
                let mut resized_nodes = (relayout_fn)(
                    *dom_id,
                    layout_size,
                    &mut layout_results[dom_id.inner],
                    image_cache,
                    renderer_resources,
                    &document_id,
                    None, // no new nodes to relayout
                    None, // no text changes
                );

                rsn.insert(*dom_id, resized_nodes.resized_nodes.clone());

                gpu_event_changes.merge(&mut resized_nodes.gpu_key_changes);

                for node_id in resized_nodes.resized_nodes.into_iter() {
                    let iframe_dom_id =
                        match layout_results[dom_id.inner].iframe_mapping.get(&node_id) {
                            Some(dom_id) => *dom_id,
                            None => continue,
                        };

                    let iframe_rect_relative_to_parent = LayoutRect {
                        origin: layout_results[iframe_dom_id.inner].root_position,
                        size: layout_results[iframe_dom_id.inner].root_size,
                    };

                    let iframe_needs_to_be_invoked =
                        !layout_size.contains_rect(&iframe_rect_relative_to_parent);

                    if !iframe_needs_to_be_invoked {
                        continue; // old iframe size still covers the new extent
                    }

                    let iframe_return: IFrameCallbackReturn = {
                        let layout_result = &mut layout_results[dom_id.inner];
                        let mut node_data_mut =
                            layout_result.styled_dom.node_data.as_container_mut();
                        let mut node = &mut node_data_mut[node_id];
                        let iframe_node = match node.get_iframe_node() {
                            Some(iframe_node) => iframe_node,
                            None => continue, // not an iframe
                        };

                        // invoke the iframe with the new size and replace the dom with the DOM ID
                        let hidpi_bounds = HidpiAdjustedBounds::from_bounds(
                            layout_size.size,
                            window_size.hidpi_factor,
                        );
                        let scroll_node = layout_result
                            .scrollable_nodes
                            .overflowing_nodes
                            .get(&NodeHierarchyItemId::from_crate_internal(Some(node_id)))
                            .cloned()
                            .unwrap_or_default();

                        let mut iframe_callback_info = IFrameCallbackInfo::new(
                            fc_cache,
                            image_cache,
                            window_theme,
                            hidpi_bounds,
                            // see /examples/assets/images/scrollbounds.png for documentation!
                            /* scroll_size  */
                            scroll_node.child_rect.size,
                            /* scroll_offset */
                            scroll_node.child_rect.origin - scroll_node.parent_rect.origin,
                            /* virtual_scroll_size  */ scroll_node.virtual_child_rect.size,
                            /* virtual_scroll_offset */
                            scroll_node.virtual_child_rect.origin - scroll_node.parent_rect.origin,
                        );
                        (iframe_node.callback.cb)(&mut iframe_node.data, &mut iframe_callback_info)
                    };

                    // TODO: what to do if the new iframe has less or more sub-iframes
                    // than the current one? edge-case, solve later.

                    layout_results[iframe_dom_id.inner].styled_dom = iframe_return.dom;

                    let new_iframe_rect = LogicalRect {
                        // TODO: correct? or layout_results[dom_id.0].positioned_rects[node_id]?
                        origin: LogicalPosition::zero(),
                        size: layout_results[dom_id.inner].rects.as_ref()[node_id].size,
                    };

                    // Store the new scroll position
                    // (trust the iframe to return these values correctly)
                    let osn = layout_results[dom_id.inner]
                        .scrollable_nodes
                        .overflowing_nodes
                        .entry(NodeHierarchyItemId::from_crate_internal(Some(node_id)))
                        .or_insert_with(|| OverflowingScrollNode::default());

                    osn.child_rect = LogicalRect {
                        origin: iframe_return.scroll_offset,
                        size: iframe_return.scroll_size,
                    };
                    osn.virtual_child_rect = LogicalRect {
                        origin: iframe_return.virtual_scroll_offset,
                        size: iframe_return.virtual_scroll_size,
                    };

                    new_dom_ids_to_resize.push((iframe_dom_id, new_iframe_rect));
                }
            }

            if new_dom_ids_to_resize.is_empty() {
                break;
            } else {
                dom_ids_to_resize = new_dom_ids_to_resize; // recurse
            }
        }

        let updated_images = Self::resize_images(
            id_namespace,
            document_id,
            epoch,
            dom_id,
            image_cache,
            gl_context,
            layout_results,
            gl_texture_cache,
            renderer_resources,
            callbacks,
            relayout_fn,
            fc_cache,
            window_size,
            window_theme,
            &rsn,
        );

        QuickResizeResult {
            gpu_event_changes,
            updated_images,
            resized_nodes: rsn,
        }
    }

    pub fn resize_images(
        id_namespace: IdNamespace,
        document_id: DocumentId,
        epoch: Epoch,
        dom_id: DomId,
        image_cache: &ImageCache,
        gl_context: &OptionGlContextPtr,
        layout_results: &mut [LayoutResult],
        gl_texture_cache: &mut GlTextureCache,
        renderer_resources: &mut RendererResources,
        callbacks: &RenderCallbacks,
        relayout_fn: RelayoutFn,
        fc_cache: &FcFontCache,
        window_size: &WindowSize,
        window_theme: WindowTheme,
        rsn: &BTreeMap<DomId, Vec<NodeId>>,
    ) -> Vec<UpdateImageResult> {
        let mut updated_images = Vec::new();

        for (dom_id, node_ids) in rsn.iter() {
            for node_id in node_ids.iter() {
                if let Some(update) = renderer_resources.rerender_image_callback(
                    *dom_id,
                    *node_id,
                    document_id,
                    epoch,
                    id_namespace,
                    gl_context,
                    image_cache,
                    fc_cache,
                    window_size.hidpi_factor,
                    callbacks,
                    layout_results,
                    gl_texture_cache,
                ) {
                    updated_images.push(update);
                }
            }
        }

        updated_images
    }

    // Calls the IFrame callbacks again if they are currently
    // scrolled out of bounds
    pub fn scroll_iframes(
        document_id: &DocumentId,
        dom_id: DomId,
        epoch: Epoch,
        layout_results: &[LayoutResult],
        full_window_state: &FullWindowState,
        gl_texture_cache: &GlTextureCache,
        renderer_resources: &RendererResources,
        image_cache: &ImageCache,
    ) {
        // TODO
    }
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
    Changed(
        NodeId,
        TransformKey,
        ComputedTransform3D,
        ComputedTransform3D,
    ),
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
        self.transform_key_changes.is_empty() && self.opacity_key_changes.is_empty()
    }
    pub fn merge(&mut self, other: &mut Self) {
        self.transform_key_changes
            .extend(other.transform_key_changes.drain(..));
        self.opacity_key_changes
            .extend(other.opacity_key_changes.drain(..));
    }
}

#[derive(Default, Debug, Clone, PartialEq, PartialOrd)]
pub struct RelayoutChanges {
    pub resized_nodes: Vec<NodeId>,
    pub gpu_key_changes: GpuEventChanges,
}

impl RelayoutChanges {
    pub const EMPTY: RelayoutChanges = RelayoutChanges {
        resized_nodes: Vec::new(),
        gpu_key_changes: GpuEventChanges {
            transform_key_changes: Vec::new(),
            opacity_key_changes: Vec::new(),
        },
    };

    pub fn empty() -> Self {
        Self::EMPTY.clone()
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

        #[cfg(target_arch = "x86_64")]
        unsafe {
            if !INITIALIZED.load(AtomicOrdering::SeqCst) {
                use core::arch::x86_64::__cpuid;

                let mut cpuid = __cpuid(0);
                let n_ids = cpuid.eax;

                if n_ids > 0 {
                    // cpuid instruction is present
                    cpuid = __cpuid(1);
                    USE_SSE.store((cpuid.edx & (1_u32 << 25)) != 0, AtomicOrdering::SeqCst);
                    USE_AVX.store((cpuid.ecx & (1_u32 << 28)) != 0, AtomicOrdering::SeqCst);
                }
                INITIALIZED.store(true, AtomicOrdering::SeqCst);
            }
        }

        // calculate the transform values of every single node that has a non-default transform
        let all_current_transform_events = (0..styled_dom.node_data.len())
            .into_par_iter()
            .filter_map(|node_id| {
                let node_id = NodeId::new(node_id);
                let styled_node_state = &node_states[node_id].state;
                let node_data = &node_data[node_id];
                let current_transform = css_property_cache
                    .get_transform(node_data, &node_id, styled_node_state)?
                    .get_property()
                    .map(|t| {
                        let parent_size = positioned_rects[node_id].size;
                        let transform_origin = css_property_cache.get_transform_origin(
                            node_data,
                            &node_id,
                            styled_node_state,
                        );
                        let transform_origin = transform_origin
                            .as_ref()
                            .and_then(|o| o.get_property())
                            .unwrap_or(&default_transform_origin);

                        ComputedTransform3D::from_style_transform_vec(
                            t.as_ref(),
                            transform_origin,
                            parent_size.width,
                            parent_size.height,
                            RotationMode::ForWebRender,
                        )
                    });

                let existing_transform = self.current_transform_values.get(&node_id);

                match (existing_transform, current_transform) {
                    (None, None) => None, // no new transform, no old transform
                    (None, Some(new)) => Some(GpuTransformKeyEvent::Added(
                        node_id,
                        TransformKey::unique(),
                        new,
                    )),
                    (Some(old), Some(new)) => Some(GpuTransformKeyEvent::Changed(
                        node_id,
                        self.transform_keys.get(&node_id).copied()?,
                        *old,
                        new,
                    )),
                    (Some(_old), None) => Some(GpuTransformKeyEvent::Removed(
                        node_id,
                        self.transform_keys.get(&node_id).copied()?,
                    )),
                }
            })
            .collect::<Vec<GpuTransformKeyEvent>>();

        // remove / add the transform keys accordingly
        for event in all_current_transform_events.iter() {
            match &event {
                GpuTransformKeyEvent::Added(node_id, key, matrix) => {
                    self.transform_keys.insert(*node_id, *key);
                    self.current_transform_values.insert(*node_id, *matrix);
                }
                GpuTransformKeyEvent::Changed(node_id, _key, _old_state, new_state) => {
                    self.current_transform_values.insert(*node_id, *new_state);
                }
                GpuTransformKeyEvent::Removed(node_id, _key) => {
                    self.transform_keys.remove(node_id);
                    self.current_transform_values.remove(node_id);
                }
            }
        }

        // calculate the opacity of every single node that has a non-default opacity
        let all_current_opacity_events = (0..styled_dom.node_data.len())
            .into_par_iter()
            .filter_map(|node_id| {
                let node_id = NodeId::new(node_id);
                let styled_node_state = &node_states[node_id].state;
                let node_data = &node_data[node_id];
                let current_opacity =
                    css_property_cache.get_opacity(node_data, &node_id, styled_node_state)?;
                let current_opacity = current_opacity.get_property();
                let existing_opacity = self.current_opacity_values.get(&node_id);

                match (existing_opacity, current_opacity) {
                    (None, None) => None, // no new opacity, no old transform
                    (None, Some(new)) => Some(GpuOpacityKeyEvent::Added(
                        node_id,
                        OpacityKey::unique(),
                        new.inner.normalized(),
                    )),
                    (Some(old), Some(new)) => Some(GpuOpacityKeyEvent::Changed(
                        node_id,
                        self.opacity_keys.get(&node_id).copied()?,
                        *old,
                        new.inner.normalized(),
                    )),
                    (Some(_old), None) => Some(GpuOpacityKeyEvent::Removed(
                        node_id,
                        self.opacity_keys.get(&node_id).copied()?,
                    )),
                }
            })
            .collect::<Vec<GpuOpacityKeyEvent>>();

        // remove / add the opacity keys accordingly
        for event in all_current_opacity_events.iter() {
            match &event {
                GpuOpacityKeyEvent::Added(node_id, key, opacity) => {
                    self.opacity_keys.insert(*node_id, *key);
                    self.current_opacity_values.insert(*node_id, *opacity);
                }
                GpuOpacityKeyEvent::Changed(node_id, _key, _old_state, new_state) => {
                    self.current_opacity_values.insert(*node_id, *new_state);
                }
                GpuOpacityKeyEvent::Removed(node_id, _key) => {
                    self.opacity_keys.remove(node_id);
                    self.current_opacity_values.remove(node_id);
                }
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
    pub fn empty() -> Self {
        Self {
            regular_hit_test_nodes: BTreeMap::new(),
            scroll_hit_test_nodes: BTreeMap::new(),
        }
    }
    pub fn is_empty(&self) -> bool {
        self.regular_hit_test_nodes.is_empty() && self.scroll_hit_test_nodes.is_empty()
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
#[repr(C)]
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
    pub holes: LogicalRectVec,
}

impl_option!(
    ResolvedTextLayoutOptions,
    OptionResolvedTextLayoutOptions,
    copy = false,
    [Debug, Clone, PartialEq, PartialOrd]
);

#[derive(Debug, Default, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct ResolvedOffsets {
    pub top: f32,
    pub left: f32,
    pub right: f32,
    pub bottom: f32,
}

impl ResolvedOffsets {
    pub const fn zero() -> Self {
        Self {
            top: 0.0,
            left: 0.0,
            right: 0.0,
            bottom: 0.0,
        }
    }
    pub fn total_vertical(&self) -> f32 {
        self.top + self.bottom
    }
    pub fn total_horizontal(&self) -> f32 {
        self.left + self.right
    }
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
            position: PositionInfo::Static(PositionInfoInner {
                x_offset: 0.0,
                y_offset: 0.0,
                static_x_offset: 0.0,
                static_y_offset: 0.0,
            }),
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
    pub fn get_approximate_static_bounds(&self) -> LayoutRect {
        LayoutRect::new(self.get_static_offset(), self.get_content_size())
    }

    // Returns the rect where the content should be placed (for example the text itself)
    #[inline]
    fn get_content_size(&self) -> LayoutSize {
        LayoutSize::new(
            libm::roundf(self.size.width) as isize,
            libm::roundf(self.size.height) as isize,
        )
    }

    #[inline]
    fn get_logical_static_offset(&self) -> LogicalPosition {
        match self.position {
            PositionInfo::Static(p)
            | PositionInfo::Fixed(p)
            | PositionInfo::Absolute(p)
            | PositionInfo::Relative(p) => {
                LogicalPosition::new(p.static_x_offset, p.static_y_offset)
            }
        }
    }

    #[inline]
    fn get_logical_relative_offset(&self) -> LogicalPosition {
        match self.position {
            PositionInfo::Static(p)
            | PositionInfo::Fixed(p)
            | PositionInfo::Absolute(p)
            | PositionInfo::Relative(p) => LogicalPosition::new(p.x_offset, p.y_offset),
        }
    }

    #[inline]
    fn get_static_offset(&self) -> LayoutPoint {
        match self.position {
            PositionInfo::Static(p)
            | PositionInfo::Fixed(p)
            | PositionInfo::Absolute(p)
            | PositionInfo::Relative(p) => LayoutPoint::new(
                libm::roundf(p.static_x_offset) as isize,
                libm::roundf(p.static_y_offset) as isize,
            ),
        }
    }

    // Returns the rect that includes bounds, expanded by the padding + the border widths
    #[inline]
    pub fn get_background_bounds(&self) -> (LogicalSize, PositionInfo) {
        use crate::ui_solver::PositionInfo::*;

        let b_size = LogicalSize {
            width: self.size.width
                + self.padding.total_horizontal()
                + self.border_widths.total_horizontal(),
            height: self.size.height
                + self.padding.total_vertical()
                + self.border_widths.total_vertical(),
        };

        let x_offset_add = 0.0 - self.padding.left - self.border_widths.left;
        let y_offset_add = 0.0 - self.padding.top - self.border_widths.top;

        let b_position = match self.position {
            Static(PositionInfoInner {
                x_offset,
                y_offset,
                static_x_offset,
                static_y_offset,
            }) => Static(PositionInfoInner {
                x_offset: x_offset + x_offset_add,
                y_offset: y_offset + y_offset_add,
                static_x_offset,
                static_y_offset,
            }),
            Fixed(PositionInfoInner {
                x_offset,
                y_offset,
                static_x_offset,
                static_y_offset,
            }) => Fixed(PositionInfoInner {
                x_offset: x_offset + x_offset_add,
                y_offset: y_offset + y_offset_add,
                static_x_offset,
                static_y_offset,
            }),
            Relative(PositionInfoInner {
                x_offset,
                y_offset,
                static_x_offset,
                static_y_offset,
            }) => Relative(PositionInfoInner {
                x_offset: x_offset + x_offset_add,
                y_offset: y_offset + y_offset_add,
                static_x_offset,
                static_y_offset,
            }),
            Absolute(PositionInfoInner {
                x_offset,
                y_offset,
                static_x_offset,
                static_y_offset,
            }) => Absolute(PositionInfoInner {
                x_offset: x_offset + x_offset_add,
                y_offset: y_offset + y_offset_add,
                static_x_offset,
                static_y_offset,
            }),
        };

        (b_size, b_position)
    }

    #[inline]
    pub fn get_margin_box_width(&self) -> f32 {
        self.size.width
            + self.padding.total_horizontal()
            + self.border_widths.total_horizontal()
            + self.margin.total_horizontal()
    }

    #[inline]
    pub fn get_margin_box_height(&self) -> f32 {
        self.size.height
            + self.padding.total_vertical()
            + self.border_widths.total_vertical()
            + self.margin.total_vertical()
    }

    #[inline]
    pub fn get_left_leading(&self) -> f32 {
        self.margin.left + self.padding.left + self.border_widths.left
    }

    #[inline]
    pub fn get_top_leading(&self) -> f32 {
        self.margin.top + self.padding.top + self.border_widths.top
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
            DirectionalOverflowInfo::Scroll { amount: Some(s) }
            | DirectionalOverflowInfo::Auto { amount: Some(s) }
            | DirectionalOverflowInfo::Hidden { amount: Some(s) }
            | DirectionalOverflowInfo::Visible { amount: Some(s) } => Some(*s),
            _ => None,
        }
    }

    #[inline]
    pub fn is_negative(&self) -> bool {
        match self {
            DirectionalOverflowInfo::Scroll { amount: Some(s) }
            | DirectionalOverflowInfo::Auto { amount: Some(s) }
            | DirectionalOverflowInfo::Hidden { amount: Some(s) }
            | DirectionalOverflowInfo::Visible { amount: Some(s) } => *s < 0_isize,
            _ => true, // no overflow = no scrollbar
        }
    }

    #[inline]
    pub fn is_none(&self) -> bool {
        match self {
            DirectionalOverflowInfo::Scroll { amount: None }
            | DirectionalOverflowInfo::Auto { amount: None }
            | DirectionalOverflowInfo::Hidden { amount: None }
            | DirectionalOverflowInfo::Visible { amount: None } => true,
            _ => false,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C, u8)]
pub enum PositionInfo {
    Static(PositionInfoInner),
    Fixed(PositionInfoInner),
    Absolute(PositionInfoInner),
    Relative(PositionInfoInner),
}

/*
impl ::core::fmt::Debug for PositionInfo {
    fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
        match self {
            PositionInfo::Static(p) => write!(f, "static({}, {})", p.x_offset, p.y_offset),
            PositionInfo::Fixed(p) => write!(f, "fixed({}, {})", p.x_offset, p.y_offset),
            PositionInfo::Absolute(p) => write!(f, "absolute({}, {})", p.x_offset, p.y_offset),
            PositionInfo::Relative(p) => write!(f, "relative({}, {})", p.x_offset, p.y_offset),
        }
    }
}
*/
impl_option!(
    PositionInfo,
    OptionPositionInfo,
    [Debug, Copy, Clone, PartialEq, PartialOrd]
);

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct PositionInfoInner {
    pub x_offset: f32,
    pub y_offset: f32,
    pub static_x_offset: f32,
    pub static_y_offset: f32,
}

impl PositionInfoInner {
    #[inline]
    pub const fn zero() -> Self {
        Self {
            x_offset: 0.0,
            y_offset: 0.0,
            static_x_offset: 0.0,
            static_y_offset: 0.0,
        }
    }
}

impl PositionInfo {
    #[inline]
    pub fn is_positioned(&self) -> bool {
        match self {
            PositionInfo::Static(_) => false,
            PositionInfo::Fixed(_) => true,
            PositionInfo::Absolute(_) => true,
            PositionInfo::Relative(_) => true,
        }
    }

    #[inline]
    pub fn get_relative_offset(&self) -> LogicalPosition {
        match self {
            PositionInfo::Static(p)
            | PositionInfo::Fixed(p)
            | PositionInfo::Absolute(p)
            | PositionInfo::Relative(p) => LogicalPosition {
                x: p.x_offset,
                y: p.y_offset,
            },
        }
    }

    #[inline]
    pub fn get_static_offset(&self) -> LogicalPosition {
        match self {
            PositionInfo::Static(p)
            | PositionInfo::Fixed(p)
            | PositionInfo::Absolute(p)
            | PositionInfo::Relative(p) => LogicalPosition {
                x: p.static_x_offset,
                y: p.static_y_offset,
            },
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

/// For some reason the rotation matrix for webrender is inverted:
/// When rendering, the matrix turns the rectangle counter-clockwise
/// direction instead of clockwise.
///
/// This is technically a workaround, but it's necessary so that
/// rotation works properly
#[derive(Debug, Copy, Clone)]
pub enum RotationMode {
    ForWebRender,
    ForHitTesting,
}

/// Computed transform of pixels in pixel space
///
/// NOTE: Matrix is row-major, not column-major
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct ComputedTransform3D {
    pub m: [[f32; 4]; 4],
}

impl ComputedTransform3D {
    pub const IDENTITY: Self = Self {
        m: [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ],
    };

    pub const fn new(
        m11: f32,
        m12: f32,
        m13: f32,
        m14: f32,
        m21: f32,
        m22: f32,
        m23: f32,
        m24: f32,
        m31: f32,
        m32: f32,
        m33: f32,
        m34: f32,
        m41: f32,
        m42: f32,
        m43: f32,
        m44: f32,
    ) -> Self {
        Self {
            m: [
                [m11, m12, m13, m14],
                [m21, m22, m23, m24],
                [m31, m32, m33, m34],
                [m41, m42, m43, m44],
            ],
        }
    }

    pub const fn new_2d(m11: f32, m12: f32, m21: f32, m22: f32, m41: f32, m42: f32) -> Self {
        Self::new(
            m11, m12, 0.0, 0.0, m21, m22, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, m41, m42, 0.0, 1.0,
        )
    }

    // very slow inverse function
    pub fn inverse(&self) -> Self {
        let det = self.determinant();

        // if det == 0.0 { return None; }

        let m = ComputedTransform3D::new(
            self.m[1][2] * self.m[2][3] * self.m[3][1] - self.m[1][3] * self.m[2][2] * self.m[3][1]
                + self.m[1][3] * self.m[2][1] * self.m[3][2]
                - self.m[1][1] * self.m[2][3] * self.m[3][2]
                - self.m[1][2] * self.m[2][1] * self.m[3][3]
                + self.m[1][1] * self.m[2][2] * self.m[3][3],
            self.m[0][3] * self.m[2][2] * self.m[3][1]
                - self.m[0][2] * self.m[2][3] * self.m[3][1]
                - self.m[0][3] * self.m[2][1] * self.m[3][2]
                + self.m[0][1] * self.m[2][3] * self.m[3][2]
                + self.m[0][2] * self.m[2][1] * self.m[3][3]
                - self.m[0][1] * self.m[2][2] * self.m[3][3],
            self.m[0][2] * self.m[1][3] * self.m[3][1] - self.m[0][3] * self.m[1][2] * self.m[3][1]
                + self.m[0][3] * self.m[1][1] * self.m[3][2]
                - self.m[0][1] * self.m[1][3] * self.m[3][2]
                - self.m[0][2] * self.m[1][1] * self.m[3][3]
                + self.m[0][1] * self.m[1][2] * self.m[3][3],
            self.m[0][3] * self.m[1][2] * self.m[2][1]
                - self.m[0][2] * self.m[1][3] * self.m[2][1]
                - self.m[0][3] * self.m[1][1] * self.m[2][2]
                + self.m[0][1] * self.m[1][3] * self.m[2][2]
                + self.m[0][2] * self.m[1][1] * self.m[2][3]
                - self.m[0][1] * self.m[1][2] * self.m[2][3],
            self.m[1][3] * self.m[2][2] * self.m[3][0]
                - self.m[1][2] * self.m[2][3] * self.m[3][0]
                - self.m[1][3] * self.m[2][0] * self.m[3][2]
                + self.m[1][0] * self.m[2][3] * self.m[3][2]
                + self.m[1][2] * self.m[2][0] * self.m[3][3]
                - self.m[1][0] * self.m[2][2] * self.m[3][3],
            self.m[0][2] * self.m[2][3] * self.m[3][0] - self.m[0][3] * self.m[2][2] * self.m[3][0]
                + self.m[0][3] * self.m[2][0] * self.m[3][2]
                - self.m[0][0] * self.m[2][3] * self.m[3][2]
                - self.m[0][2] * self.m[2][0] * self.m[3][3]
                + self.m[0][0] * self.m[2][2] * self.m[3][3],
            self.m[0][3] * self.m[1][2] * self.m[3][0]
                - self.m[0][2] * self.m[1][3] * self.m[3][0]
                - self.m[0][3] * self.m[1][0] * self.m[3][2]
                + self.m[0][0] * self.m[1][3] * self.m[3][2]
                + self.m[0][2] * self.m[1][0] * self.m[3][3]
                - self.m[0][0] * self.m[1][2] * self.m[3][3],
            self.m[0][2] * self.m[1][3] * self.m[2][0] - self.m[0][3] * self.m[1][2] * self.m[2][0]
                + self.m[0][3] * self.m[1][0] * self.m[2][2]
                - self.m[0][0] * self.m[1][3] * self.m[2][2]
                - self.m[0][2] * self.m[1][0] * self.m[2][3]
                + self.m[0][0] * self.m[1][2] * self.m[2][3],
            self.m[1][1] * self.m[2][3] * self.m[3][0] - self.m[1][3] * self.m[2][1] * self.m[3][0]
                + self.m[1][3] * self.m[2][0] * self.m[3][1]
                - self.m[1][0] * self.m[2][3] * self.m[3][1]
                - self.m[1][1] * self.m[2][0] * self.m[3][3]
                + self.m[1][0] * self.m[2][1] * self.m[3][3],
            self.m[0][3] * self.m[2][1] * self.m[3][0]
                - self.m[0][1] * self.m[2][3] * self.m[3][0]
                - self.m[0][3] * self.m[2][0] * self.m[3][1]
                + self.m[0][0] * self.m[2][3] * self.m[3][1]
                + self.m[0][1] * self.m[2][0] * self.m[3][3]
                - self.m[0][0] * self.m[2][1] * self.m[3][3],
            self.m[0][1] * self.m[1][3] * self.m[3][0] - self.m[0][3] * self.m[1][1] * self.m[3][0]
                + self.m[0][3] * self.m[1][0] * self.m[3][1]
                - self.m[0][0] * self.m[1][3] * self.m[3][1]
                - self.m[0][1] * self.m[1][0] * self.m[3][3]
                + self.m[0][0] * self.m[1][1] * self.m[3][3],
            self.m[0][3] * self.m[1][1] * self.m[2][0]
                - self.m[0][1] * self.m[1][3] * self.m[2][0]
                - self.m[0][3] * self.m[1][0] * self.m[2][1]
                + self.m[0][0] * self.m[1][3] * self.m[2][1]
                + self.m[0][1] * self.m[1][0] * self.m[2][3]
                - self.m[0][0] * self.m[1][1] * self.m[2][3],
            self.m[1][2] * self.m[2][1] * self.m[3][0]
                - self.m[1][1] * self.m[2][2] * self.m[3][0]
                - self.m[1][2] * self.m[2][0] * self.m[3][1]
                + self.m[1][0] * self.m[2][2] * self.m[3][1]
                + self.m[1][1] * self.m[2][0] * self.m[3][2]
                - self.m[1][0] * self.m[2][1] * self.m[3][2],
            self.m[0][1] * self.m[2][2] * self.m[3][0] - self.m[0][2] * self.m[2][1] * self.m[3][0]
                + self.m[0][2] * self.m[2][0] * self.m[3][1]
                - self.m[0][0] * self.m[2][2] * self.m[3][1]
                - self.m[0][1] * self.m[2][0] * self.m[3][2]
                + self.m[0][0] * self.m[2][1] * self.m[3][2],
            self.m[0][2] * self.m[1][1] * self.m[3][0]
                - self.m[0][1] * self.m[1][2] * self.m[3][0]
                - self.m[0][2] * self.m[1][0] * self.m[3][1]
                + self.m[0][0] * self.m[1][2] * self.m[3][1]
                + self.m[0][1] * self.m[1][0] * self.m[3][2]
                - self.m[0][0] * self.m[1][1] * self.m[3][2],
            self.m[0][1] * self.m[1][2] * self.m[2][0] - self.m[0][2] * self.m[1][1] * self.m[2][0]
                + self.m[0][2] * self.m[1][0] * self.m[2][1]
                - self.m[0][0] * self.m[1][2] * self.m[2][1]
                - self.m[0][1] * self.m[1][0] * self.m[2][2]
                + self.m[0][0] * self.m[1][1] * self.m[2][2],
        );

        m.multiply_scalar(1.0 / det)
    }

    fn determinant(&self) -> f32 {
        self.m[0][3] * self.m[1][2] * self.m[2][1] * self.m[3][0]
            - self.m[0][2] * self.m[1][3] * self.m[2][1] * self.m[3][0]
            - self.m[0][3] * self.m[1][1] * self.m[2][2] * self.m[3][0]
            + self.m[0][1] * self.m[1][3] * self.m[2][2] * self.m[3][0]
            + self.m[0][2] * self.m[1][1] * self.m[2][3] * self.m[3][0]
            - self.m[0][1] * self.m[1][2] * self.m[2][3] * self.m[3][0]
            - self.m[0][3] * self.m[1][2] * self.m[2][0] * self.m[3][1]
            + self.m[0][2] * self.m[1][3] * self.m[2][0] * self.m[3][1]
            + self.m[0][3] * self.m[1][0] * self.m[2][2] * self.m[3][1]
            - self.m[0][0] * self.m[1][3] * self.m[2][2] * self.m[3][1]
            - self.m[0][2] * self.m[1][0] * self.m[2][3] * self.m[3][1]
            + self.m[0][0] * self.m[1][2] * self.m[2][3] * self.m[3][1]
            + self.m[0][3] * self.m[1][1] * self.m[2][0] * self.m[3][2]
            - self.m[0][1] * self.m[1][3] * self.m[2][0] * self.m[3][2]
            - self.m[0][3] * self.m[1][0] * self.m[2][1] * self.m[3][2]
            + self.m[0][0] * self.m[1][3] * self.m[2][1] * self.m[3][2]
            + self.m[0][1] * self.m[1][0] * self.m[2][3] * self.m[3][2]
            - self.m[0][0] * self.m[1][1] * self.m[2][3] * self.m[3][2]
            - self.m[0][2] * self.m[1][1] * self.m[2][0] * self.m[3][3]
            + self.m[0][1] * self.m[1][2] * self.m[2][0] * self.m[3][3]
            + self.m[0][2] * self.m[1][0] * self.m[2][1] * self.m[3][3]
            - self.m[0][0] * self.m[1][2] * self.m[2][1] * self.m[3][3]
            - self.m[0][1] * self.m[1][0] * self.m[2][2] * self.m[3][3]
            + self.m[0][0] * self.m[1][1] * self.m[2][2] * self.m[3][3]
    }

    fn multiply_scalar(&self, x: f32) -> Self {
        ComputedTransform3D::new(
            self.m[0][0] * x,
            self.m[0][1] * x,
            self.m[0][2] * x,
            self.m[0][3] * x,
            self.m[1][0] * x,
            self.m[1][1] * x,
            self.m[1][2] * x,
            self.m[1][3] * x,
            self.m[2][0] * x,
            self.m[2][1] * x,
            self.m[2][2] * x,
            self.m[2][3] * x,
            self.m[3][0] * x,
            self.m[3][1] * x,
            self.m[3][2] * x,
            self.m[3][3] * x,
        )
    }

    // Computes the matrix of a rect from a Vec<StyleTransform>
    pub fn from_style_transform_vec(
        t_vec: &[StyleTransform],
        transform_origin: &StyleTransformOrigin,
        percent_resolve_x: f32,
        percent_resolve_y: f32,
        rotation_mode: RotationMode,
    ) -> Self {
        // TODO: use correct SIMD optimization!
        let mut matrix = Self::IDENTITY;
        let use_avx =
            INITIALIZED.load(AtomicOrdering::SeqCst) && USE_AVX.load(AtomicOrdering::SeqCst);
        let use_sse = !use_avx
            && INITIALIZED.load(AtomicOrdering::SeqCst)
            && USE_SSE.load(AtomicOrdering::SeqCst);

        if use_avx {
            for t in t_vec.iter() {
                #[cfg(target_arch = "x86_64")]
                unsafe {
                    matrix = matrix.then_avx8(&Self::from_style_transform(
                        t,
                        transform_origin,
                        percent_resolve_x,
                        percent_resolve_y,
                        rotation_mode,
                    ));
                }
            }
        } else if use_sse {
            for t in t_vec.iter() {
                #[cfg(target_arch = "x86_64")]
                unsafe {
                    matrix = matrix.then_sse(&Self::from_style_transform(
                        t,
                        transform_origin,
                        percent_resolve_x,
                        percent_resolve_y,
                        rotation_mode,
                    ));
                }
            }
        } else {
            // fallback for everything else
            for t in t_vec.iter() {
                matrix = matrix.then(&Self::from_style_transform(
                    t,
                    transform_origin,
                    percent_resolve_x,
                    percent_resolve_y,
                    rotation_mode,
                ));
            }
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
        rotation_mode: RotationMode,
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
            }
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
                    m11, m12, m13, m14, m21, m22, m23, m24, m31, m32, m33, m34, m41, m42, m43, m44,
                )
            }
            Translate(trans2d) => Self::new_translation(
                trans2d.x.to_pixels(percent_resolve_x),
                trans2d.y.to_pixels(percent_resolve_y),
                0.0,
            ),
            Translate3D(trans3d) => Self::new_translation(
                trans3d.x.to_pixels(percent_resolve_x),
                trans3d.y.to_pixels(percent_resolve_y),
                trans3d.z.to_pixels(percent_resolve_x), // ???
            ),
            TranslateX(trans_x) => {
                Self::new_translation(trans_x.to_pixels(percent_resolve_x), 0.0, 0.0)
            }
            TranslateY(trans_y) => {
                Self::new_translation(0.0, trans_y.to_pixels(percent_resolve_y), 0.0)
            }
            TranslateZ(trans_z) => {
                Self::new_translation(0.0, 0.0, trans_z.to_pixels(percent_resolve_x))
            } // ???
            Rotate3D(rot3d) => {
                let rotation_origin = (
                    transform_origin.x.to_pixels(percent_resolve_x),
                    transform_origin.y.to_pixels(percent_resolve_y),
                );
                Self::make_rotation(
                    rotation_origin,
                    rot3d.angle.to_degrees(),
                    rot3d.x.normalized(),
                    rot3d.y.normalized(),
                    rot3d.z.normalized(),
                    rotation_mode,
                )
            }
            RotateX(angle_x) => {
                let rotation_origin = (
                    transform_origin.x.to_pixels(percent_resolve_x),
                    transform_origin.y.to_pixels(percent_resolve_y),
                );
                Self::make_rotation(
                    rotation_origin,
                    angle_x.to_degrees(),
                    1.0,
                    0.0,
                    0.0,
                    rotation_mode,
                )
            }
            RotateY(angle_y) => {
                let rotation_origin = (
                    transform_origin.x.to_pixels(percent_resolve_x),
                    transform_origin.y.to_pixels(percent_resolve_y),
                );
                Self::make_rotation(
                    rotation_origin,
                    angle_y.to_degrees(),
                    0.0,
                    1.0,
                    0.0,
                    rotation_mode,
                )
            }
            Rotate(angle_z) | RotateZ(angle_z) => {
                let rotation_origin = (
                    transform_origin.x.to_pixels(percent_resolve_x),
                    transform_origin.y.to_pixels(percent_resolve_y),
                );
                Self::make_rotation(
                    rotation_origin,
                    angle_z.to_degrees(),
                    0.0,
                    0.0,
                    1.0,
                    rotation_mode,
                )
            }
            Scale(scale2d) => Self::new_scale(scale2d.x.normalized(), scale2d.y.normalized(), 1.0),
            Scale3D(scale3d) => Self::new_scale(
                scale3d.x.normalized(),
                scale3d.y.normalized(),
                scale3d.z.normalized(),
            ),
            ScaleX(scale_x) => Self::new_scale(scale_x.normalized(), 1.0, 1.0),
            ScaleY(scale_y) => Self::new_scale(1.0, scale_y.normalized(), 1.0),
            ScaleZ(scale_z) => Self::new_scale(1.0, 1.0, scale_z.normalized()),
            Skew(skew2d) => Self::new_skew(skew2d.x.normalized(), skew2d.y.normalized()),
            SkewX(skew_x) => Self::new_skew(skew_x.normalized(), 0.0),
            SkewY(skew_y) => Self::new_skew(0.0, skew_y.normalized()),
            Perspective(px) => Self::new_perspective(px.to_pixels(percent_resolve_x)),
        }
    }

    #[inline]
    pub const fn new_scale(x: f32, y: f32, z: f32) -> Self {
        Self::new(
            x, 0.0, 0.0, 0.0, 0.0, y, 0.0, 0.0, 0.0, 0.0, z, 0.0, 0.0, 0.0, 0.0, 1.0,
        )
    }

    #[inline]
    pub const fn new_translation(x: f32, y: f32, z: f32) -> Self {
        Self::new(
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, x, y, z, 1.0,
        )
    }

    #[inline]
    pub fn new_perspective(d: f32) -> Self {
        Self::new(
            1.0,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
            -1.0 / d,
            0.0,
            0.0,
            0.0,
            1.0,
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
            1.0,
        )
    }

    #[inline]
    pub fn new_skew(alpha: f32, beta: f32) -> Self {
        let (sx, sy) = (beta.to_radians().tan(), alpha.to_radians().tan());
        Self::new(
            1.0, sx, 0.0, 0.0, sy, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        )
    }

    pub fn get_column_major(&self) -> Self {
        ComputedTransform3D::new(
            self.m[0][0],
            self.m[1][0],
            self.m[2][0],
            self.m[3][0],
            self.m[0][1],
            self.m[1][1],
            self.m[2][1],
            self.m[3][1],
            self.m[0][2],
            self.m[1][2],
            self.m[2][2],
            self.m[3][2],
            self.m[0][3],
            self.m[1][3],
            self.m[2][3],
            self.m[3][3],
        )
    }

    // Transforms a 2D point into the target coordinate space
    #[must_use]
    pub fn transform_point2d(&self, p: LogicalPosition) -> Option<LogicalPosition> {
        let w =
            p.x.mul_add(self.m[0][3], p.y.mul_add(self.m[1][3], self.m[3][3]));

        if !w.is_sign_positive() {
            return None;
        }

        let x =
            p.x.mul_add(self.m[0][0], p.y.mul_add(self.m[1][0], self.m[3][0]));
        let y =
            p.x.mul_add(self.m[0][1], p.y.mul_add(self.m[1][1], self.m[3][1]));

        Some(LogicalPosition { x: x / w, y: y / w })
    }

    /// Computes the sum of two matrices while applying `other` AFTER the current matrix.
    #[must_use]
    #[inline]
    pub fn then(&self, other: &Self) -> Self {
        Self::new(
            self.m[0][0].mul_add(
                other.m[0][0],
                self.m[0][1].mul_add(
                    other.m[1][0],
                    self.m[0][2].mul_add(other.m[2][0], self.m[0][3] * other.m[3][0]),
                ),
            ),
            self.m[0][0].mul_add(
                other.m[0][1],
                self.m[0][1].mul_add(
                    other.m[1][1],
                    self.m[0][2].mul_add(other.m[2][1], self.m[0][3] * other.m[3][1]),
                ),
            ),
            self.m[0][0].mul_add(
                other.m[0][2],
                self.m[0][1].mul_add(
                    other.m[1][2],
                    self.m[0][2].mul_add(other.m[2][2], self.m[0][3] * other.m[3][2]),
                ),
            ),
            self.m[0][0].mul_add(
                other.m[0][3],
                self.m[0][1].mul_add(
                    other.m[1][3],
                    self.m[0][2].mul_add(other.m[2][3], self.m[0][3] * other.m[3][3]),
                ),
            ),
            self.m[1][0].mul_add(
                other.m[0][0],
                self.m[1][1].mul_add(
                    other.m[1][0],
                    self.m[1][2].mul_add(other.m[2][0], self.m[1][3] * other.m[3][0]),
                ),
            ),
            self.m[1][0].mul_add(
                other.m[0][1],
                self.m[1][1].mul_add(
                    other.m[1][1],
                    self.m[1][2].mul_add(other.m[2][1], self.m[1][3] * other.m[3][1]),
                ),
            ),
            self.m[1][0].mul_add(
                other.m[0][2],
                self.m[1][1].mul_add(
                    other.m[1][2],
                    self.m[1][2].mul_add(other.m[2][2], self.m[1][3] * other.m[3][2]),
                ),
            ),
            self.m[1][0].mul_add(
                other.m[0][3],
                self.m[1][1].mul_add(
                    other.m[1][3],
                    self.m[1][2].mul_add(other.m[2][3], self.m[1][3] * other.m[3][3]),
                ),
            ),
            self.m[2][0].mul_add(
                other.m[0][0],
                self.m[2][1].mul_add(
                    other.m[1][0],
                    self.m[2][2].mul_add(other.m[2][0], self.m[2][3] * other.m[3][0]),
                ),
            ),
            self.m[2][0].mul_add(
                other.m[0][1],
                self.m[2][1].mul_add(
                    other.m[1][1],
                    self.m[2][2].mul_add(other.m[2][1], self.m[2][3] * other.m[3][1]),
                ),
            ),
            self.m[2][0].mul_add(
                other.m[0][2],
                self.m[2][1].mul_add(
                    other.m[1][2],
                    self.m[2][2].mul_add(other.m[2][2], self.m[2][3] * other.m[3][2]),
                ),
            ),
            self.m[2][0].mul_add(
                other.m[0][3],
                self.m[2][1].mul_add(
                    other.m[1][3],
                    self.m[2][2].mul_add(other.m[2][3], self.m[2][3] * other.m[3][3]),
                ),
            ),
            self.m[3][0].mul_add(
                other.m[0][0],
                self.m[3][1].mul_add(
                    other.m[1][0],
                    self.m[3][2].mul_add(other.m[2][0], self.m[3][3] * other.m[3][0]),
                ),
            ),
            self.m[3][0].mul_add(
                other.m[0][1],
                self.m[3][1].mul_add(
                    other.m[1][1],
                    self.m[3][2].mul_add(other.m[2][1], self.m[3][3] * other.m[3][1]),
                ),
            ),
            self.m[3][0].mul_add(
                other.m[0][2],
                self.m[3][1].mul_add(
                    other.m[1][2],
                    self.m[3][2].mul_add(other.m[2][2], self.m[3][3] * other.m[3][2]),
                ),
            ),
            self.m[3][0].mul_add(
                other.m[0][3],
                self.m[3][1].mul_add(
                    other.m[1][3],
                    self.m[3][2].mul_add(other.m[2][3], self.m[3][3] * other.m[3][3]),
                ),
            ),
        )
    }

    // credit: https://gist.github.com/rygorous/4172889

    // linear combination:
    // a[0] * B.row[0] + a[1] * B.row[1] + a[2] * B.row[2] + a[3] * B.row[3]
    #[cfg(target_arch = "x86_64")]
    #[inline]
    unsafe fn linear_combine_sse(a: [f32; 4], b: &ComputedTransform3D) -> [f32; 4] {
        use core::arch::x86_64::__m128;
        use core::arch::x86_64::{_mm_add_ps, _mm_mul_ps, _mm_shuffle_ps};
        use core::mem;

        let a: __m128 = mem::transmute(a);
        let mut result = _mm_mul_ps(_mm_shuffle_ps(a, a, 0x00), mem::transmute(b.m[0]));
        result = _mm_add_ps(
            result,
            _mm_mul_ps(_mm_shuffle_ps(a, a, 0x55), mem::transmute(b.m[1])),
        );
        result = _mm_add_ps(
            result,
            _mm_mul_ps(_mm_shuffle_ps(a, a, 0xaa), mem::transmute(b.m[2])),
        );
        result = _mm_add_ps(
            result,
            _mm_mul_ps(_mm_shuffle_ps(a, a, 0xff), mem::transmute(b.m[3])),
        );

        mem::transmute(result)
    }

    #[cfg(target_arch = "x86_64")]
    #[inline]
    pub unsafe fn then_sse(&self, other: &Self) -> Self {
        Self {
            m: [
                Self::linear_combine_sse(self.m[0], other),
                Self::linear_combine_sse(self.m[1], other),
                Self::linear_combine_sse(self.m[2], other),
                Self::linear_combine_sse(self.m[3], other),
            ],
        }
    }

    // dual linear combination using AVX instructions on YMM regs
    #[cfg(target_arch = "x86_64")]
    pub unsafe fn linear_combine_avx8(a01: __m256, b: &ComputedTransform3D) -> __m256 {
        use core::arch::x86_64::{
            _mm256_add_ps, _mm256_broadcast_ps, _mm256_mul_ps, _mm256_shuffle_ps,
        };
        use core::mem;

        let mut result = _mm256_mul_ps(
            _mm256_shuffle_ps(a01, a01, 0x00),
            _mm256_broadcast_ps(mem::transmute(&b.m[0])),
        );
        result = _mm256_add_ps(
            result,
            _mm256_mul_ps(
                _mm256_shuffle_ps(a01, a01, 0x55),
                _mm256_broadcast_ps(mem::transmute(&b.m[1])),
            ),
        );
        result = _mm256_add_ps(
            result,
            _mm256_mul_ps(
                _mm256_shuffle_ps(a01, a01, 0xaa),
                _mm256_broadcast_ps(mem::transmute(&b.m[2])),
            ),
        );
        result = _mm256_add_ps(
            result,
            _mm256_mul_ps(
                _mm256_shuffle_ps(a01, a01, 0xff),
                _mm256_broadcast_ps(mem::transmute(&b.m[3])),
            ),
        );
        result
    }

    #[cfg(target_arch = "x86_64")]
    #[inline]
    pub unsafe fn then_avx8(&self, other: &Self) -> Self {
        use core::arch::x86_64::{_mm256_loadu_ps, _mm256_storeu_ps, _mm256_zeroupper};
        use core::mem;

        _mm256_zeroupper();

        let a01: __m256 = _mm256_loadu_ps(mem::transmute(&self.m[0][0]));
        let a23: __m256 = _mm256_loadu_ps(mem::transmute(&self.m[2][0]));

        let out01x = Self::linear_combine_avx8(a01, other);
        let out23x = Self::linear_combine_avx8(a23, other);

        let mut out = Self {
            m: [self.m[0], self.m[1], self.m[2], self.m[3]],
        };

        _mm256_storeu_ps(mem::transmute(&mut out.m[0][0]), out01x);
        _mm256_storeu_ps(mem::transmute(&mut out.m[2][0]), out23x);

        out
    }

    // NOTE: webrenders RENDERING has a different
    // rotation mode (positive / negative angle)
    #[inline]
    pub fn make_rotation(
        rotation_origin: (f32, f32),
        mut degrees: f32,
        axis_x: f32,
        axis_y: f32,
        axis_z: f32,
        // see documentation for RotationMode
        rotation_mode: RotationMode,
    ) -> Self {
        degrees = match rotation_mode {
            RotationMode::ForWebRender => -degrees, // CSS rotations are clockwise
            RotationMode::ForHitTesting => degrees, // hit-testing turns counter-clockwise
        };

        let (origin_x, origin_y) = rotation_origin;
        let pre_transform = Self::new_translation(-origin_x, -origin_y, -0.0);
        let post_transform = Self::new_translation(origin_x, origin_y, 0.0);
        let theta = 2.0_f32 * core::f32::consts::PI - degrees.to_radians();
        let rotate_transform =
            Self::new_rotation(axis_x, axis_y, axis_z, theta).then(&Self::IDENTITY);

        pre_transform.then(&rotate_transform).then(&post_transform)
    }
}
