//! List markers: which box carries a marker, which line it rides, and its content.

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

// ==== list markers on the first line box (CSS Lists 3 s3.1, CSS 2.2 s12.5.1) ====

/// Deepest chain of first children the marker helpers follow.
pub(super) const MARKER_WALK_LIMIT: usize = 128;

/// Whether the box at `index` is a `::marker` pseudo-element.
pub(crate) fn is_marker_box(tree: &LayoutTree, index: usize) -> bool {
    tree.warm(LayoutNodeId::new(index))
        .is_some_and(|w| w.pseudo_element == Some(PseudoElement::Marker))
}

/// Whether the list item whose box is `list_item` puts its marker INSIDE
/// (`list-style-position: inside`; the initial value is `outside`). A
/// `::marker` box carries its item's DOM node, so this answers for a
/// marker box too.
pub(super) fn has_inside_marker(tree: &LayoutTree, styled_dom: &StyledDom, list_item: usize) -> bool {
    tree.get(LayoutNodeId::new(list_item))
        .and_then(|n| n.dom_node_id)
        .is_some_and(|dom| {
            matches!(
                get_list_style_position(styled_dom, Some(dom)),
                StyleListStylePosition::Inside
            )
        })
}

/// Whether the box at `index` is a `::marker` hanging OUTSIDE its item.
pub(crate) fn is_outside_marker(tree: &LayoutTree, styled_dom: &StyledDom, index: usize) -> bool {
    is_marker_box(tree, index) && !has_inside_marker(tree, styled_dom, index)
}

/// The DOM node whose style an inline formatting context rooted at
/// `ifc_root` resolves: its own, or - for an anonymous block, which has
/// none and inherits from its enclosing box (CSS 2.2 s9.2.1.1) - its
/// parent's, else its first child's that has one. The one rule for the
/// IFC layout (`layout_ifc`) and its intrinsic sizes
/// (`sizing::calculate_ifc_root_intrinsic_sizes`).
pub(crate) fn ifc_root_style_dom_id(tree: &LayoutTree, ifc_root: usize) -> Option<NodeId> {
    let node = tree.get(LayoutNodeId::new(ifc_root))?;
    node.dom_node_id
        .or_else(|| {
            node.parent
                .and_then(|p| tree.get(LayoutNodeId::new(p)))
                .and_then(|n| n.dom_node_id)
        })
        .or_else(|| {
            tree.children(ifc_root)
                .iter()
                .filter_map(|&child| tree.get(LayoutNodeId::new(child)))
                .find_map(|n| n.dom_node_id)
        })
}

/// Whether the anonymous block `index` holds its container's FIRST
/// formatted line - the line `text-indent` indents (CSS 2.1 s16.1, CSS Text 3
/// s8.1): only when it is the container's first in-flow box. The text after
/// a nested block (`<div>first<div>..</div>after</div>`) is no first line
/// (Chrome). The one gate for the IFC layout and its intrinsic sizes.
pub(crate) fn anonymous_block_holds_the_first_line(
    tree: &LayoutTree,
    styled_dom: &StyledDom,
    index: usize,
) -> bool {
    tree.get(LayoutNodeId::new(index))
        .and_then(|n| n.parent)
        .is_none_or(|parent| first_in_flow_child(tree, styled_dom, parent) == Some(index))
}

/// The inline formatting context holding a list item's FIRST LINE BOX, the
/// line its `::marker` sits on (CSS Lists 3 s3.1, CSS 2.2 s12.5.1): the item
/// itself when it is one, else the first one down the item's first in-flow
/// block children - the anonymous block of `<li>Item<div>..</div></li>`, the
/// `<p>` of `<li><div><p>..</p></div></li>`.
///
/// `None` when there is no line box there (an empty item, or a first child
/// that is a table, a flex box, a replaced element): the marker box is then
/// laid out by the item's block flow (`layout_bfc`).
///
/// An INSIDE marker is an inline box at the very start of its item (CSS 2.2
/// 12.5.1): it shares the item's OWN first line - the item's IFC, or the
/// anonymous block wrapping the item's leading inline content - and nothing
/// deeper. Before a first block child it is a line of its own (an anonymous
/// block): it never rides a nested block's line (WPT
/// list-style-position-023: three nested inside markers piled onto the
/// innermost item's line).
pub(crate) fn marker_line_host(
    tree: &LayoutTree,
    styled_dom: &StyledDom,
    list_item: usize,
) -> Option<usize> {
    if has_inside_marker(tree, styled_dom, list_item) {
        return match tree.get(LayoutNodeId::new(list_item))?.formatting_context {
            FormattingContext::Inline => Some(list_item),
            FormattingContext::Block { .. } => {
                let first = first_in_flow_child(tree, styled_dom, list_item)?;
                let first_node = tree.get(LayoutNodeId::new(first))?;
                let leading_inline_content = first_node.dom_node_id.is_none()
                    && matches!(first_node.formatting_context, FormattingContext::Inline);
                leading_inline_content.then_some(first)
            }
            _ => None,
        };
    }
    let mut node = list_item;
    for _ in 0..MARKER_WALK_LIMIT {
        match tree.get(LayoutNodeId::new(node))?.formatting_context {
            FormattingContext::Inline => return Some(node),
            FormattingContext::Block { .. } => {}
            _ => return None,
        }
        node = first_in_flow_child(tree, styled_dom, node)?;
    }
    None
}

/// A list item's `::marker` box that rides the item's first line box
/// ([`marker_line_host`]): it is no block of the item's flow - its content
/// is laid out with that line - so the block layout and the intrinsic
/// sizes pass it by (it took a line of its own: every `<li>` with a block
/// child was one line too tall).
pub(crate) fn is_marker_on_a_line(tree: &LayoutTree, styled_dom: &StyledDom, index: usize) -> bool {
    is_marker_box(tree, index)
        && tree
            .get(LayoutNodeId::new(index))
            .and_then(|n| n.parent)
            .is_some_and(|item| marker_line_host(tree, styled_dom, item).is_some())
}

/// The `::marker` boxes whose content opens the first line of the inline
/// formatting context rooted at `ifc_root`, outermost list item first: one
/// for each list item (the root itself, or an ancestor it starts) whose
/// [`marker_line_host`] it is. A marker box laid out as an IFC of its own
/// holds just itself.
pub(super) fn markers_on_first_line(tree: &LayoutTree, styled_dom: &StyledDom, ifc_root: usize) -> Vec<usize> {
    if is_marker_box(tree, ifc_root) {
        return vec![ifc_root];
    }
    let mut markers = Vec::new();
    let mut node = ifc_root;
    for _ in 0..MARKER_WALK_LIMIT {
        if let Some(marker) = tree
            .children(node)
            .iter()
            .copied()
            .find(|&child| is_marker_box(tree, child))
        {
            if marker_line_host(tree, styled_dom, node) == Some(ifc_root) {
                markers.push(marker);
            }
        }
        let Some(parent) = tree.get(LayoutNodeId::new(node)).and_then(|n| n.parent) else {
            break;
        };
        // The first line of `parent` is here only if `node` starts it.
        if first_in_flow_child(tree, styled_dom, parent) != Some(node) {
            break;
        }
        node = parent;
    }
    markers.reverse();
    markers
}

/// The content of the `::marker` box `marker_index` (its list item's
/// counter in its `list-style-type`, styled as the item) appended to an
/// IFC's `content`, inside or outside by the item's `list-style-position`.
pub(super) fn push_marker_content<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    tree: &LayoutTree,
    marker_index: usize,
    content: &mut Vec<InlineContent>,
) {
    // The marker box references its list item's DOM node.
    let Some(list_dom_id) = tree
        .get(LayoutNodeId::new(marker_index))
        .and_then(|n| n.dom_node_id)
    else {
        return;
    };
    // Default is 'outside' per CSS Lists Module Level 3
    let position_outside = matches!(
        get_list_style_position(ctx.styled_dom, Some(list_dom_id)),
        StyleListStylePosition::Outside
    );
    // Font fallback happens during shaping
    let base_style = crate::solver3::getters::get_style_properties_cached(
        &mut ctx.style_cache,
        ctx.styled_dom,
        list_dom_id,
        ctx.system_style.as_ref(),
        PhysicalSize::new(ctx.viewport_size.width, ctx.viewport_size.height),
    );
    let segments = generate_list_marker_segments(
        tree,
        ctx.styled_dom,
        marker_index,
        ctx.counters,
        base_style,
        ctx.debug_messages,
    );
    // Outside markers are positioned in the padding gutter by text3
    for segment in segments {
        content.push(InlineContent::Marker {
            run: segment,
            position_outside,
        });
    }
}

/// Generates marker text for a list item marker.
///
/// This function looks up the counter value from the cache and formats it
/// according to the list-style-type property.
///
/// Per CSS Lists Module Level 3, the `::marker` pseudo-element is the first child
/// of the list-item, and references the same DOM node. Counter resolution happens
/// on the list-item (parent) node.
pub(super) fn generate_list_marker_text(
    tree: &LayoutTree,
    styled_dom: &StyledDom,
    marker_index: usize,
    counters: &HashMap<(usize, String), i32>,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> String {
    use crate::solver3::counters::format_counter;

    // Get the marker node
    let Some(marker_node) = tree.get(LayoutNodeId::new(marker_index)) else {
        return String::new();
    };

    // Verify this is actually a ::marker pseudo-element
    // Per spec, markers must be pseudo-elements, not anonymous boxes
    let marker_pseudo = tree
        .warm(LayoutNodeId::new(marker_index))
        .and_then(|w| w.pseudo_element);
    let marker_anonymous_type = tree
        .cold(LayoutNodeId::new(marker_index))
        .and_then(|c| c.anonymous_type);
    if marker_pseudo != Some(PseudoElement::Marker) {
        if let Some(msgs) = debug_messages {
            msgs.push(LayoutDebugMessage::warning(format!(
                "[generate_list_marker_text] WARNING: Node {marker_index} is not a ::marker \
                 pseudo-element (pseudo={marker_pseudo:?}, \
                 anonymous_type={marker_anonymous_type:?})"
            )));
        }
        // Fallback for old-style anonymous markers during transition
        if marker_anonymous_type != Some(AnonymousBoxType::ListItemMarker) {
            return String::new();
        }
    }

    // Get the parent list-item node (::marker is first child of list-item)
    let Some(list_item_index) = marker_node.parent else {
        if let Some(msgs) = debug_messages {
            msgs.push(LayoutDebugMessage::error(
                "[generate_list_marker_text] ERROR: Marker has no parent".to_string(),
            ));
        }
        return String::new();
    };

    let Some(list_item_node) = tree.get(LayoutNodeId::new(list_item_index)) else {
        return String::new();
    };

    let Some(list_item_dom_id) = list_item_node.dom_node_id else {
        if let Some(msgs) = debug_messages {
            msgs.push(LayoutDebugMessage::error(
                "[generate_list_marker_text] ERROR: List-item has no DOM ID".to_string(),
            ));
        }
        return String::new();
    };

    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "[generate_list_marker_text] marker_index={marker_index}, \
             list_item_index={list_item_index}, list_item_dom_id={list_item_dom_id:?}"
        )));
    }

    // The list item's own list-style-type: it inherits, so `<ol>`'s decimal
    // reaches its items, and an item's own value wins over its list's (the
    // container's type was read first, so `<li style="list-style-type:
    // none">` in a styled list kept the list's marker). The same value
    // decides whether the marker has a box at all
    // (`LayoutTreeBuilder::create_marker_pseudo_element`).
    let list_style_type = get_list_style_type(styled_dom, Some(list_item_dom_id));

    // Get the counter value for "list-item" counter from the LIST-ITEM node
    // Per CSS spec, counters are scoped to elements, and the list-item counter
    // is incremented at the list-item element, not the marker pseudo-element
    let counter_value = counters
        .get(&(list_item_index, "list-item".to_string()))
        .copied()
        .unwrap_or_else(|| {
            if let Some(msgs) = debug_messages {
                msgs.push(LayoutDebugMessage::warning(format!(
                    "[generate_list_marker_text] WARNING: No counter found for list-item at index \
                     {list_item_index}, defaulting to 1"
                )));
            }
            1
        });

    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "[generate_list_marker_text] counter_value={counter_value} for \
             list_item_index={list_item_index}"
        )));
    }

    // Format the counter according to the list-style-type
    let marker_text = format_counter(counter_value, list_style_type);
    // No marker string (`none`): no marker - not a lone space.
    if marker_text.is_empty() {
        return String::new();
    }

    // For ordered lists (non-symbolic markers), add a period and space
    // For unordered lists (symbolic markers like •, ◦, ▪), just add a space
    if matches!(
        list_style_type,
        StyleListStyleType::Decimal
            | StyleListStyleType::DecimalLeadingZero
            | StyleListStyleType::LowerAlpha
            | StyleListStyleType::UpperAlpha
            | StyleListStyleType::LowerRoman
            | StyleListStyleType::UpperRoman
            | StyleListStyleType::LowerGreek
            | StyleListStyleType::UpperGreek
    ) {
        format!("{marker_text}. ")
    } else {
        format!("{marker_text} ")
    }
}

/// Generates marker text segments for a list item marker.
///
/// Simply returns a single `StyledRun` with the marker text using the `base_style`.
/// The font stack in `base_style` already includes fallbacks with 100% Unicode coverage,
/// so font resolution happens during text shaping, not here.
pub(super) fn generate_list_marker_segments(
    tree: &LayoutTree,
    styled_dom: &StyledDom,
    marker_index: usize,
    counters: &HashMap<(usize, String), i32>,
    base_style: Arc<StyleProperties>,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> Vec<StyledRun> {
    // Generate the marker text
    let marker_text =
        generate_list_marker_text(tree, styled_dom, marker_index, counters, debug_messages);
    if marker_text.is_empty() {
        return Vec::new();
    }

    if let Some(msgs) = debug_messages {
        let font_families: Vec<&str> = match &base_style.font_stack {
            text3::cache::FontStack::Stack(selectors) => {
                selectors.iter().map(|f| f.family.as_str()).collect()
            }
            text3::cache::FontStack::Ref(_) => vec!["<embedded-font>"],
        };
        msgs.push(LayoutDebugMessage::info(format!(
            "[generate_list_marker_segments] Marker text: '{marker_text}' with font stack: \
             {font_families:?}"
        )));
    }

    // Return single segment - font fallback happens during shaping
    // List markers are generated content, not from DOM nodes
    vec![StyledRun {
        text: Arc::from(marker_text.as_str()),
        style: base_style,
        logical_start_byte: 0,
        source_node_id: None,
    }]
}
