//! Formatting context layout (block, inline, table, and flex/grid via Taffy)

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

// The modules of what was one file. Every item keeps the visibility it had there: a
// `pub(crate)` one in a private module is what clippy would rather call `pub`, which a
// `pub use` would publish.
#[allow(clippy::redundant_pub_crate)]
mod types;
pub use types::*;
mod floats;
pub use floats::*;
mod dispatch;
pub use dispatch::*;
mod flex_grid;
pub use flex_grid::*;
mod bfc;
pub(crate) use bfc::*;
mod block_rules;
pub use block_rules::*;
mod columns;
pub(crate) use columns::*;
mod ifc;
pub use ifc::*;
mod text_constraints;
pub use text_constraints::*;
mod inline_content;
pub(crate) use inline_content::*;
mod atomic_inline;
pub(crate) use atomic_inline::*;
#[allow(clippy::redundant_pub_crate)]
mod list_markers;
pub(crate) use list_markers::*;
mod white_space;
pub use white_space::*;
#[allow(clippy::redundant_pub_crate)]
mod table;
pub use table::*;
#[allow(clippy::redundant_pub_crate)]
mod table_borders;
pub use table_borders::*;
#[allow(clippy::redundant_pub_crate)]
mod table_columns;
pub(crate) use table_columns::*;
mod table_rows;
pub(crate) use table_rows::*;

#[cfg(test)]
#[allow(clippy::float_cmp, clippy::too_many_lines)]
mod autotest_generated;

/// The formatting contexts of this file checked through a whole window
/// layout (`LayoutWindow`, the product path).
///
/// The inline-collection cache of `layout_ifc` (`CachedInlineContent`, keyed
/// by the IFC subtree's fingerprint) across a STYLESHEET-ONLY rebuild: the
/// same DOM, the same classes, another stylesheet. The node fingerprints are
/// all unchanged, so only the resolved values the key folds in can tell the
/// collection is stale. The rebuild goes through the product path:
/// `begin_reconciliation` (the CSS diff) then the layout pass.
#[cfg(test)]
mod window_layout_tests;

/// `<sup>`, `<sub>` and `vertical-align: super` move their text off the line's
/// baseline - and a box nested in a shifted one rides its parent's shift
/// (CSS 2.2 s10.8.1: a box aligns against its PARENT inline box; Chrome:
/// the parent's font size / 3 + 1px up, / 5 + 1px down, each nested box
/// adding its own). pdfocr's engine-issue report, issue 1 (the repro shape:
/// `<p>Cock<sup>a</sup> and Job<span style="vertical-align: super;
/// font-size: 0.6em">b</span> and x<sub>2</sub></p>`), plus the footnote
/// mark of every book (`<sup><i>1</i></sup>`) and a superscript of a
/// superscript. Font-independent: the shifts are measured between baselines.
#[cfg(test)]
mod vertical_align_of_nested_inline_boxes_tests;

/// A `flex-grow: 1` item of a flex row lays its content out at the width the row gave it, on
/// the FIRST layout. AzContacts (E2E-A, 2026-10-06): the list's search field - a
/// `TextInput::create_search()` in a `flex-grow: 1` block next to a fixed-width segmented
/// control - painted a 6 px input (its border and padding) inside a 148 px field box, so a
/// click on the search row reached no input; the card's notes value wrapped one word per line.
/// A window resize relaid both out correctly; the first layout and every restyle relayout (a
/// CSS override on any node) did not. Seen on the libazul of 2026-10-06 03:14 (889dccf30,
/// e1688c746 in it).
#[cfg(test)]
mod a_flex_items_content_takes_its_final_width_tests;

/// A block-level flex item measured with an unknown width (a column's item at
/// `align-items: flex-start`: fit-content) lays its content out in the space
/// LEFT FOR ITS CONTENT - the available width minus its margins, border and
/// padding - not the available width itself. Its text was broken at the full
/// width, so a padded paragraph came out wider than the column by its own
/// padding (and the same paragraph was laid out at two widths per pass,
/// thrashing its line-layout cache: AzContacts, 2026-10-06).
#[cfg(test)]
mod a_fit_content_block_item_fits_its_column_tests;

#[cfg(test)]
mod an_absolute_box_keeps_its_static_position_tests;
