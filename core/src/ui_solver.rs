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
    callbacks::{HidpiAdjustedBounds, IFrameCallbackInfo, IFrameCallbackReturn},
    dom::{DomId, DomNodeHash, ScrollTagId, TagId},
    geom::{LogicalPosition, LogicalRect, LogicalRectVec, LogicalSize},
    gl::OptionGlContextPtr,
    gpu::GpuEventChanges,
    hit_test::{ExternalScrollId, HitTestItem, ScrollHitTestItem, ScrollStates, ScrolledNodes},
    id::{NodeDataContainer, NodeDataContainerRef, NodeId},
    resources::{
        Epoch, FontInstanceKey, GlTextureCache, IdNamespace, ImageCache, OpacityKey,
        RendererResources, TransformKey, UpdateImageResult,
    },
    styled_dom::{NodeHierarchyItemId, StyledDom},
    window::{OptionChar, WindowSize, WindowTheme},
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
