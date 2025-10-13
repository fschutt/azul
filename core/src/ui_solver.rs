#[cfg(not(feature = "std"))]
use alloc::string::{String, ToString};
use alloc::{boxed::Box, collections::btree_map::BTreeMap, vec::Vec};
#[cfg(target_arch = "x86_64")]
use core::arch::x86_64::__m256;
use core::{
    fmt,
    sync::atomic::{AtomicBool, Ordering as AtomicOrdering},
};

use azul_css::{
    css::CssPropertyValue,
    props::{
        basic::{
            ColorU as StyleColorU, LayoutPoint, LayoutRect, LayoutRectVec, LayoutSize, PixelValue,
            StyleFontSize,
        },
        layout::{
            LayoutBottom, LayoutBoxSizing, LayoutDisplay, LayoutFlexDirection, LayoutFloat,
            LayoutHeight, LayoutJustifyContent, LayoutLeft, LayoutMarginBottom, LayoutMarginLeft,
            LayoutMarginRight, LayoutMarginTop, LayoutMaxHeight, LayoutMaxWidth, LayoutMinHeight,
            LayoutMinWidth, LayoutOverflow, LayoutPaddingBottom, LayoutPaddingLeft,
            LayoutPaddingRight, LayoutPaddingTop, LayoutPosition, LayoutRight, LayoutTop,
            LayoutWidth,
        },
        style::{
            LayoutBorderBottomWidth, LayoutBorderLeftWidth, LayoutBorderRightWidth,
            LayoutBorderTopWidth, OptionStyleTextAlign, StyleBoxShadow, StyleTextAlign,
            StyleTextColor, StyleTransform, StyleTransformOrigin, StyleVerticalAlign,
        },
    },
    AzString, OptionF32,
};
use rust_fontconfig::FcFontCache;

use crate::{
    callbacks::{
        DocumentId, HidpiAdjustedBounds, HitTestItem, IFrameCallbackInfo, IFrameCallbackReturn,
        PipelineId, ScrollHitTestItem,
    },
    dom::{DomNodeHash, ScrollTagId, TagId},
    gl::OptionGlContextPtr,
    gpu::GpuEventChanges,
    id::{NodeDataContainer, NodeDataContainerRef, NodeId},
    resources::{
        Epoch, FontInstanceKey, GlTextureCache, IdNamespace, ImageCache, OpacityKey,
        RenderCallbacks, RendererResources, TransformKey, UpdateImageResult,
    },
    styled_dom::{DomId, NodeHierarchyItemId, StyledDom},
    window::{
        LogicalPosition, LogicalRect, LogicalRectVec, LogicalSize, OptionChar, ScrollStates,
        WindowSize, WindowTheme,
    },
};

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
    /// Nodes that need to clip their direct children (i.e. nodes with overflow-x and overflow-y
    /// set to "Hidden")
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

/// Represents the CSS formatting context for an element
#[derive(Clone, PartialEq)]
pub enum FormattingContext {
    /// Block-level formatting context
    Block {
        /// Whether this element establishes a new block formatting context
        establishes_new_context: bool,
    },
    /// Inline-level formatting context
    Inline,
    /// Inline-block (participates in an IFC but creates a BFC)
    InlineBlock,
    /// Flex formatting context
    Flex,
    /// Float (left or right)
    Float(LayoutFloat),
    /// Absolutely positioned (out of flow)
    OutOfFlow(LayoutPosition),
    /// Table formatting context (container)
    Table,
    /// Table row group formatting context (thead, tbody, tfoot)
    TableRowGroup,
    /// Table row formatting context
    TableRow,
    /// Table cell formatting context (td, th)
    TableCell,
    /// Table column group formatting context
    TableColumnGroup,
    /// Table caption formatting context
    TableCaption,
    /// Grid formatting context
    Grid,
    /// No formatting context (display: none)
    None,
}

impl fmt::Debug for FormattingContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FormattingContext::Block {
                establishes_new_context,
            } => write!(
                f,
                "Block {{ establishes_new_context: {establishes_new_context:?} }}"
            ),
            FormattingContext::Inline => write!(f, "Inline"),
            FormattingContext::InlineBlock => write!(f, "InlineBlock"),
            FormattingContext::Flex => write!(f, "Flex"),
            FormattingContext::Float(layout_float) => write!(f, "Float({layout_float:?})"),
            FormattingContext::OutOfFlow(layout_position) => {
                write!(f, "OutOfFlow({layout_position:?})")
            }
            FormattingContext::Grid => write!(f, "Grid"),
            FormattingContext::None => write!(f, "None"),
            FormattingContext::Table => write!(f, "Table"),
            FormattingContext::TableRowGroup => write!(f, "TableRowGroup"),
            FormattingContext::TableRow => write!(f, "TableRow"),
            FormattingContext::TableCell => write!(f, "TableCell"),
            FormattingContext::TableColumnGroup => write!(f, "TableColumnGroup"),
            FormattingContext::TableCaption => write!(f, "TableCaption"),
        }
    }
}

impl Default for FormattingContext {
    fn default() -> Self {
        FormattingContext::Block {
            establishes_new_context: false,
        }
    }
}

pub type GlyphIndex = u32;

#[derive(Debug, Default, Copy, Clone, PartialEq, PartialOrd)]
pub struct GlyphInstance {
    pub index: GlyphIndex,
    pub point: LogicalPosition,
    pub size: LogicalSize,
}

impl GlyphInstance {
    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        self.point.scale_for_dpi(scale_factor);
        self.size.scale_for_dpi(scale_factor);
    }
}
pub struct QuickResizeResult {
    pub gpu_event_changes: GpuEventChanges,
    pub updated_images: Vec<UpdateImageResult>,
    pub resized_nodes: BTreeMap<DomId, Vec<NodeId>>,
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum GpuOpacityKeyEvent {
    Added(NodeId, OpacityKey, f32),
    Changed(NodeId, OpacityKey, f32, f32),
    Removed(NodeId, OpacityKey),
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

#[derive(Debug, Default, Clone, PartialEq, PartialOrd)]
pub struct OverflowInfo {
    pub overflow_x: DirectionalOverflowInfo,
    pub overflow_y: DirectionalOverflowInfo,
}

// stores how much the children overflow the parent in the given direction
// if amount is negative, the children do not overflow the parent
// if the amount is set to None, that means there are no children for this node, so no overflow can
// be calculated
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

impl PositionInfo {
    /// Shift this node vertically by `offset_amount`.
    /// i.e. add `offset_amount` to both the relative and static y-offsets.
    pub fn translate_vertical(&mut self, offset_amount: f32) {
        match self {
            PositionInfo::Static(ref mut info)
            | PositionInfo::Absolute(ref mut info)
            | PositionInfo::Fixed(ref mut info)
            | PositionInfo::Relative(ref mut info) => {
                info.y_offset += offset_amount;
                info.static_y_offset += offset_amount;
            }
        }
    }

    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        match self {
            PositionInfo::Static(p) => p.scale_for_dpi(scale_factor),
            PositionInfo::Fixed(p) => p.scale_for_dpi(scale_factor),
            PositionInfo::Absolute(p) => p.scale_for_dpi(scale_factor),
            PositionInfo::Relative(p) => p.scale_for_dpi(scale_factor),
        }
    }
}

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

    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        self.x_offset *= scale_factor;
        self.y_offset *= scale_factor;
        self.static_x_offset *= scale_factor;
        self.static_y_offset *= scale_factor;
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
