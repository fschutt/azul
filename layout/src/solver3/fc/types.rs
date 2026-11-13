//! What flows through the formatting contexts: constraints, layout outputs and block formatting state.

use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};
use azul_core::{
    dom::{FormattingContext, NodeId, NodeType},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    resources::RendererResources,
    styled_dom::{StyledDom, StyledNodeState},
};
use azul_css::{
    css::CssPropertyValue,
    props::{
        basic::{
            font::{StyleFontStyle, StyleFontWeight},
            pixel::{DEFAULT_FONT_SIZE, PT_TO_PX},
            ColorU, PhysicalSize, PropertyContext, ResolutionContext, SizeMetric,
        },
        layout::{
            LayoutBorderSpacing, LayoutClear, LayoutDisplay, LayoutFloat, LayoutHeight,
            LayoutJustifyContent, LayoutOverflow, LayoutPosition, LayoutTableLayout,
            LayoutTextJustify, LayoutWidth, LayoutWritingMode, ShapeInside, ShapeOutside,
            StyleBorderCollapse, StyleCaptionSide, StyleEmptyCells,
        },
        property::CssProperty,
        style::{
            BorderStyle, StyleDirection, StyleHyphens, StyleLineBreak, StyleListStylePosition,
            StyleListStyleType, StyleOverflowWrap, StyleTextAlign, StyleTextAlignLast,
            StyleTextBoxTrim, StyleTextCombineUpright, StyleTextOrientation, StyleUnicodeBidi,
            StyleVerticalAlign, StyleVisibility, StyleWhiteSpace, StyleWordBreak,
        },
    },
};
use rust_fontconfig::FcWeight;
use taffy::{AvailableSpace, LayoutInput, Line, Size as TaffySize};
#[cfg(feature = "text_layout")]
use crate::text3;
use crate::{
    debug_ifc_layout, debug_info, debug_log, debug_table_layout, debug_warning,
    font_traits::{
        ContentIndex, FontLoaderTrait, ImageSource, InlineContent, InlineImage, InlineShape,
        LayoutFragment, ObjectFit, ParsedFontTrait, SegmentAlignment, ShapeBoundary,
        ShapeDefinition, ShapedItem, Size, StyleProperties, StyledRun, TextLayoutCache,
        UnifiedConstraints,
    },
    solver3::{
        geometry::{BoxProps, ContainingBlock as CBTY, EdgeSizes, IntrinsicSizes},
        getters::{
            get_clear, get_css_border_bottom_width, get_css_border_top_width, get_css_box_sizing,
            get_css_height, get_css_padding_bottom, get_css_padding_top, get_css_width,
            get_direction_property, get_display_property, get_element_font_size, get_float,
            get_list_style_position, get_list_style_type, get_overflow_x, get_overflow_y,
            get_parent_font_size, get_root_font_size, get_style_properties, get_text_align,
            get_text_box_edge_property, get_text_box_trim_property, get_text_orientation_property,
            get_unicode_bidi_property, get_vertical_align_property, get_visibility,
            get_white_space_property, get_writing_mode, MultiValue,
        },
        layout_tree::{
            AnonymousBoxType, CachedInlineLayout, LayoutNode, LayoutNodeCold, LayoutNodeHot,
            LayoutNodeId, LayoutNodeWarm, LayoutTree, PseudoElement,
        },
        positioning::get_position_type,
        scrollbar::{ScrollbarKind, ScrollbarRequirements},
        sizing::extract_text_from_node,
        taffy_bridge, LayoutContext, LayoutDebugMessage, LayoutError, Result,
    },
    text3::cache::{
        AvailableSpace as Text3AvailableSpace, BreakType, ClearType, InlineBreak,
        TextAlign as Text3TextAlign,
    },
};
#[allow(clippy::wildcard_imports)]
// the formatting contexts' items, re-exported from the sibling modules by mod.rs
use super::*;

/// Default scrollbar width in pixels (CSS `scrollbar-width: auto`).
///
/// This is only used as a fallback when per-node CSS cannot be queried.
/// Prefer `getters::get_layout_scrollbar_width_px()` for per-node resolution.
pub const DEFAULT_SCROLLBAR_WIDTH_PX: f32 = 16.0;

// Note: DEFAULT_FONT_SIZE and PT_TO_PX are imported from pixel

/// Result of BFC layout with margin escape information
#[derive(Debug, Clone)]
pub(crate) struct BfcLayoutResult {
    /// Standard layout output (positions, overflow size, baseline)
    pub output: LayoutOutput,
    /// Top margin that escaped the BFC (for parent-child collapse)
    /// If Some, this margin should be used by parent instead of positioning this BFC
    pub escaped_top_margin: Option<f32>,
    /// Bottom margin that escaped the BFC (for parent-child collapse)
    /// If Some, this margin should collapse with next sibling
    pub escaped_bottom_margin: Option<f32>,
    /// K30b: `Some` = this BFC ran out of fragmentainer space; the token
    /// resumes it in the next fragmentainer. Always `None` on the
    /// continuous path (`constraints.fragmentainer == None`).
    pub outgoing_token: Option<crate::solver3::break_token::BreakToken>,
    /// A descendant laid out by this formatting context discovered that it
    /// needs a scrollbar that RESERVES space. Its children were already sized
    /// against the unreserved width, so the document-level layout loop has to
    /// run another pass; this is how that need leaves the subtree.
    pub scrollbar_reflow_needed: bool,
    /// The scrollbar gutter this formatting context took out of its own
    /// children's containing block BEFORE laying them out. A node that then
    /// turns out to need exactly this much has nothing to lay out again.
    pub reserved_scrollbar_width: f32,
}

impl BfcLayoutResult {
    pub(crate) const fn from_output(output: LayoutOutput) -> Self {
        Self {
            output,
            escaped_top_margin: None,
            escaped_bottom_margin: None,
            outgoing_token: None,
            scrollbar_reflow_needed: false,
            reserved_scrollbar_width: 0.0,
        }
    }
}

/// The CSS `overflow` property behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverflowBehavior {
    Visible,
    Hidden,
    Clip,
    Scroll,
    Auto,
}

impl OverflowBehavior {
    #[must_use]
    pub const fn is_clipped(&self) -> bool {
        matches!(self, Self::Hidden | Self::Clip | Self::Scroll | Self::Auto)
    }

    #[must_use]
    pub const fn is_scroll(&self) -> bool {
        matches!(self, Self::Scroll | Self::Auto)
    }
}

/// The fragmentainer the current layout call is filling.
/// `None` = continuous media — every
/// existing path passes `None` and behaves bit-for-bit as before.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FragmentainerSpace<'a> {
    /// Block-extent remaining in the CURRENT fragmentainer, measured from
    /// this box's block-start (the pen compares against it directly).
    pub remaining_block_extent: f32,
    /// Extent of a FRESH next fragmentainer ("would it fit on the next
    /// page at all?" — monolith classification).
    pub next_fragmentainer_extent: f32,
    /// True while filling the very first fragmentainer of the flow.
    pub is_first: bool,
    /// Incoming resume state for THIS box (the page loop threads page
    /// N−1's outgoing token back in). The token rides the fragmentainer
    /// input instead of a separate parameter so the continuous path stays
    /// signature-identical.
    pub resume: Option<&'a crate::solver3::break_token::BlockBreakToken>,
}

/// Input constraints for a layout function.
#[derive(Debug)]
pub struct LayoutConstraints<'a> {
    /// The available space for the content, excluding padding and borders.
    pub available_size: LogicalSize,
    /// The CSS writing-mode of the context.
    pub writing_mode: LayoutWritingMode,
    /// Full writing mode context (writing-mode + direction + text-orientation).
    /// Used by writing-mode-aware layout code to correctly map inline/block
    /// dimensions to physical x/y coordinates.
    pub writing_mode_ctx: super::super::geometry::WritingModeContext,
    /// The state of the parent Block Formatting Context, if applicable.
    /// This is how state (like floats) is passed down.
    pub bfc_state: Option<&'a mut BfcState>,
    // Other properties like text-align would go here.
    pub text_align: TextAlign,
    /// The size of the containing block (parent's content box).
    /// This is used for resolving percentage-based sizes and as `parent_size` for Taffy.
    pub containing_block_size: LogicalSize,
    /// The semantic type of the available width constraint.
    ///
    /// This field is crucial for correct inline layout caching:
    /// - `Definite(w)`: Normal layout with a specific available width
    /// - `MinContent`: Intrinsic minimum width measurement (maximum wrapping)
    /// - `MaxContent`: Intrinsic maximum width measurement (no wrapping)
    ///
    /// When caching inline layouts, we must track which constraint type was used
    /// to compute the cached result. A layout computed with `MinContent` (width=0)
    /// must not be reused when the actual available width is known.
    pub available_width_type: Text3AvailableSpace,
    /// K30b fragmentation: `None` = continuous (screen) layout, identical
    /// to pre-token behavior. `Some` arms the fit checks in `layout_bfc`.
    pub fragmentainer: Option<FragmentainerSpace<'a>>,
    /// The inline formatting context laid out is one piece of a
    /// multi-column block container's flow, its lines continuing in the
    /// next columns at these line indices (`solver3::multicol`). `None` =
    /// not split, for every caller but the multi-column block layout.
    pub column_flow: Option<crate::text3::cache::ColumnFlow>,
}

/// Manages all layout state for a single Block Formatting Context.
/// This struct is created by the BFC root and lives for the duration of its layout.
#[derive(Debug, Clone)]
pub struct BfcState {
    /// The current position for the next in-flow block element.
    pub pen: LogicalPosition,
    /// The state of all floated elements within this BFC.
    pub floats: FloatingContext,
    /// The state of margin collapsing within this BFC.
    pub margins: MarginCollapseContext,
}

impl Default for BfcState {
    fn default() -> Self {
        Self::new()
    }
}

impl BfcState {
    #[must_use]
    pub fn new() -> Self {
        Self {
            pen: LogicalPosition::zero(),
            floats: FloatingContext::default(),
            margins: MarginCollapseContext::default(),
        }
    }
}

/// Manages vertical margin collapsing within a BFC.
#[derive(Copy, Debug, Default, Clone)]
pub struct MarginCollapseContext {
    /// The bottom margin of the last in-flow, block-level element.
    /// Can be positive or negative.
    pub last_in_flow_margin_bottom: f32,
}

/// The result of laying out a formatting context.
#[derive(Debug, Default, Clone)]
pub struct LayoutOutput {
    /// The final positions of child nodes, relative to the container's content-box origin.
    pub positions: BTreeMap<usize, LogicalPosition>,
    /// The total size occupied by the content, which may exceed `available_size`.
    pub overflow_size: LogicalSize,
    // +spec:inline-formatting-context:f7eebb - baseline along inline axis for glyph alignment
    /// The baseline of the context, if applicable, measured from the top of its content box.
    pub baseline: Option<f32>,
    /// The STATIC positions of the out-of-flow (absolute / fixed) children
    /// this context placed no box for: the border-box origin each would
    /// have had in the flow (CSS 2.2 10.3.7 / 10.6.4), relative to the
    /// container's content-box origin like `positions`. A child missing
    /// here takes the content-box origin.
    pub static_positions: BTreeMap<usize, LogicalPosition>,
}

/// Text alignment options
#[derive(Debug, Clone, Copy, Default)]
pub enum TextAlign {
    #[default]
    Start,
    End,
    Center,
    Justify,
}

/// Encapsulates all state needed to lay out a single Block Formatting Context.
pub(super) struct BfcLayoutState {
    /// The current position for the next in-flow block element.
    pen: LogicalPosition,
    floats: FloatingContext,
    margins: MarginCollapseContext,
    /// The writing mode of the BFC root.
    writing_mode: LayoutWritingMode,
}
