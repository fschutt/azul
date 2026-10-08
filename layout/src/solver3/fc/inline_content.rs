//! An inline formatting context's content: text runs, inline boxes, images and atomic inlines.

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

pub(super) fn collect_and_measure_inline_content<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    text_cache: &mut TextLayoutCache,
    tree: &mut LayoutTree,
    ifc_root_index: usize,
    constraints: &LayoutConstraints<'_>,
) -> Result<(Vec<InlineContent>, HashMap<ContentIndex, usize>)> {
    use crate::{
        solver3::layout_tree::{IfcId, IfcMembership},
        text3::cache::InlineContent,
    };

    let mut content = Vec::new();
    let mut child_map = HashMap::new();
    collect_and_measure_inline_content_impl(
        ctx,
        text_cache,
        tree,
        ifc_root_index,
        constraints,
        &mut content,
        &mut child_map,
    )?;

    // CSS Text Decoration 3 §2.1: the decorations of the boxes around this
    // context reach all of its text. A run is styled by its own node, and
    // `text-decoration` is not inherited: `<p style="text-decoration:
    // underline">text</p>` drew no line.
    if let Some(root) = ifc_root_style_dom_id(tree, ifc_root_index) {
        let around = crate::solver3::getters::propagated_text_decoration(ctx.styled_dom, root);
        if around != crate::text3::cache::TextDecoration::default() {
            for item in &mut content {
                if let InlineContent::Text(run) = item {
                    let with = run.style.text_decoration.with(around);
                    if with != run.style.text_decoration {
                        Arc::make_mut(&mut run.style).text_decoration = with;
                    }
                }
            }
        }
    }

    // O3-render: a split-preview PART displays only its byte slice of the
    // node's content (both parts collect the same full content; the range
    // partitions it — part 1 `[0, at)`, part 2 `[at, ∞)`).
    if let Some((start, end)) = tree
        .cold(LayoutNodeId::new(ifc_root_index))
        .and_then(|c| c.preview_byte_range)
    {
        content = slice_inline_content_by_bytes(content, start as usize, end as usize);
    }

    Ok((content, child_map))
}

/// Byte-slice inline content (flat text bytes; Text runs cut at char
/// boundaries, non-text items kept when their position falls inside the
/// range) — the read-side twin of the structural split's partition rule.
pub(super) fn slice_inline_content_by_bytes(
    content: Vec<InlineContent>,
    start: usize,
    end: usize,
) -> Vec<InlineContent> {
    let mut out = Vec::new();
    let mut consumed = 0_usize;
    for item in content {
        match item {
            InlineContent::Text(mut run) => {
                let len = run.text.len();
                let item_start = consumed;
                let item_end = consumed + len;
                consumed = item_end;
                if item_end <= start || item_start >= end {
                    continue;
                }
                let cut_from = start.saturating_sub(item_start).min(len);
                let cut_to = (end - item_start).min(len);
                let cut_from = (0..=cut_from)
                    .rev()
                    .find(|&c| run.text.is_char_boundary(c))
                    .unwrap_or(0);
                let cut_to = (cut_to..=len)
                    .find(|&c| run.text.is_char_boundary(c))
                    .unwrap_or(len);
                if cut_from != 0 || cut_to != len {
                    run.text = Arc::from(&run.text[cut_from..cut_to]);
                    run.logical_start_byte = 0;
                }
                out.push(InlineContent::Text(run));
            }
            other => {
                if consumed >= start && consumed < end {
                    out.push(other);
                }
            }
        }
    }
    out
}

#[allow(clippy::cast_possible_truncation)] // bounded graphics/coord/font/fixed-point/debug-marker cast
#[allow(clippy::too_many_lines, clippy::cognitive_complexity)] // large but cohesive: single-purpose
                                                               // layout/render/parse routine (one
                                                               // branch per case)
pub(super) fn collect_and_measure_inline_content_impl<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    text_cache: &mut TextLayoutCache,
    tree: &mut LayoutTree,
    ifc_root_index: usize,
    constraints: &LayoutConstraints<'_>,
    content: &mut Vec<InlineContent>,
    child_map: &mut HashMap<ContentIndex, usize>,
) -> Result<()> {
    use crate::solver3::layout_tree::{IfcId, IfcMembership};

    debug_ifc_layout!(
        ctx,
        "collect_and_measure_inline_content: node_index={}",
        ifc_root_index
    );

    // Generate a unique IFC ID for this inline formatting context
    let ifc_id = IfcId::unique();

    // Store IFC ID on the IFC root node
    if let Some(cold_node) = tree.cold_mut(LayoutNodeId::new(ifc_root_index)) {
        cold_node.ifc_id = Some(ifc_id);
    }

    // [g129/g130 az-web-lift] `content` and `child_map` are now caller-provided out-params
    // (the by-value `(Vec, HashMap)` return mis-lifted its len on the web backend). The caller
    // passes them in EMPTY; this body fills them exactly as before. `child_map` maps the
    // `ContentIndex` used by text3 back to the `LayoutNode` index.
    // Track the current run index for IFC membership assignment
    let mut current_run_index: u32 = 0;

    // [g134/g135 az-web-lift DIAG] out-param pointer + tree validity at _impl entry. The early Err
    // is the `tree.get(ifc_root_index).ok_or(InvalidTree)?` at 6449/6706 (no other `?` before
    // the first content push) — capture whether `tree` is valid (nodes.len) and tree.get(idx)
    // actually works.
    #[cfg(feature = "web_lift")]
    unsafe {
        crate::az_mark(
            (0x60690) as u32,
            (content as *const _ as usize as u32) as u32,
        );
        crate::az_mark(
            (0x60694) as u32,
            (ifc_root_index as u32 | 0xC0DE0000u32) as u32,
        );
        crate::az_mark((0x606A8) as u32, (tree.nodes.len() as u32) as u32);
        crate::az_mark(
            (0x606AC) as u32,
            (tree.get(ifc_root_index).is_some() as u32 | 0xC0DE0000u32) as u32,
        );
        crate::az_mark((0x606B0) as u32, (tree.root as u32) as u32);
        // [g147 az-web-lift DIAG] CALLEE-side tree ptr + nodes.len indexed by ifc_root_index
        // (0x60940+ = nodes.len, 0x60960+ = tree ptr). Pair with layout_ifc's 0x60900+/0x60920+.
        let slot = (ifc_root_index & 7) * 4;
        crate::az_mark((0x60940 + slot) as u32, (tree.nodes.len() as u32) as u32);
        crate::az_mark(
            (0x60960 + slot) as u32,
            ((&*tree as *const LayoutTree as usize) as u32) as u32,
        );
    }

    let ifc_root_node = tree
        .get(LayoutNodeId::new(ifc_root_index))
        .ok_or(LayoutError::InvalidTree)?;
    // [g135] reached past the 6449 tree.get.
    #[cfg(feature = "web_lift")]
    unsafe {
        crate::az_mark((0x606A4) as u32, (0x0000_6449u32) as u32);
    }

    // Check if this is an anonymous IFC wrapper (has no DOM ID)
    let is_anonymous = ifc_root_node.dom_node_id.is_none();

    // Get the DOM node ID of the IFC root, or find it from parent/children for anonymous boxes
    // CSS 2.2 § 9.2.1.1: Anonymous boxes inherit properties from their enclosing box
    let ifc_root_dom_id = if let Some(id) = ifc_root_node.dom_node_id {
        id
    } else {
        // Anonymous box - get DOM ID from parent or first child with DOM ID
        let parent_dom_id = ifc_root_node
            .parent
            .and_then(|p| tree.get(LayoutNodeId::new(p)))
            .and_then(|n| n.dom_node_id);

        if let Some(id) = parent_dom_id {
            id
        } else {
            // Try to find DOM ID from first child
            if let Some(id) = tree
                .children(ifc_root_index)
                .iter()
                .filter_map(|&child_idx| tree.get(LayoutNodeId::new(child_idx)))
                .find_map(|n| n.dom_node_id)
            {
                id
            } else {
                debug_warning!(ctx, "IFC root and all ancestors/children have no DOM ID");
                return Ok(());
            }
        }
    };

    // Collect children to avoid holding an immutable borrow during iteration
    let children: Vec<_> = tree.children(ifc_root_index).to_vec();
    drop(ifc_root_node);

    debug_ifc_layout!(
        ctx,
        "Node {} has {} layout children, is_anonymous={}",
        ifc_root_index,
        children.len(),
        is_anonymous
    );

    // CSS Lists 3 s3.1: the `::marker` of every list item whose FIRST LINE
    // BOX this IFC holds opens its content - the item's own IFC, the
    // anonymous block of `<li>Item<ul>..</ul></li>`, the first `<p>` of
    // `<li><p>a</p><p>b</p></li>` (and only the first: every IFC whose parent
    // was a list item got a marker before, the anonymous block none).
    for marker_idx in markers_on_first_line(tree, ctx.styled_dom, ifc_root_index) {
        push_marker_content(ctx, tree, marker_idx, content);
    }
    // A marker laid out as an IFC of its own (its item has no line box to
    // ride) holds its marker and nothing else: its DOM node is the LIST
    // ITEM, whose children are the item's content, not the marker's - read
    // as its own, they were laid out (and painted) a second time.
    if is_marker_box(tree, ifc_root_index) {
        return Ok(());
    }

    // For anonymous IFC wrappers, we collect content from layout tree children
    // For regular IFC roots, we also check DOM children for text nodes
    if is_anonymous {
        // Anonymous IFC wrapper - iterate over layout tree children and collect their content
        for (item_idx, &child_index) in children.iter().enumerate() {
            let content_index = ContentIndex {
                run_index: ifc_root_index as u32,
                item_index: item_idx as u32,
            };

            let child_node = tree
                .get(LayoutNodeId::new(child_index))
                .ok_or(LayoutError::InvalidTree)?;
            let Some(dom_id) = child_node.dom_node_id else {
                debug_warning!(
                    ctx,
                    "Anonymous IFC child at index {} has no DOM ID",
                    child_index
                );
                continue;
            };

            let node_data = &ctx.styled_dom.node_data.as_container()[dom_id];

            // Check if this is a text node
            if let NodeType::Text(ref text_content) = node_data.get_node_type() {
                debug_info!(
                    ctx,
                    "[collect_and_measure_inline_content] OK: Found text node (DOM {:?}) in \
                     anonymous wrapper: '{}'",
                    dom_id,
                    text_content.as_str()
                );
                // Get style from the TEXT NODE itself (dom_id), not the IFC root
                // This ensures inline styles like color: #666666 are applied to the text
                let style = crate::solver3::getters::get_style_properties_cached(
                    &mut ctx.style_cache,
                    ctx.styled_dom,
                    dom_id,
                    ctx.system_style.as_ref(),
                    PhysicalSize::new(ctx.viewport_size.width, ctx.viewport_size.height),
                );
                let text_items = split_text_for_whitespace(
                    ctx.styled_dom,
                    dom_id,
                    text_content.as_str(),
                    &style,
                );
                content.extend(text_items);
                child_map.insert(content_index, child_index);

                // Set IFC membership on the text node - drop child_node borrow first
                drop(child_node);
                if let Some(warm_mut) = tree.warm_mut(LayoutNodeId::new(child_index)) {
                    warm_mut.ifc_membership = Some(IfcMembership {
                        ifc_id,
                        ifc_root_layout_index: ifc_root_index,
                        run_index: current_run_index,
                    });
                }
                current_run_index += 1;

                continue;
            }

            // A <br> forces a hard line break inside this IFC (see UA css: <br>
            // is inline). Without this it would collect as an empty inline span
            // and never break the line.
            if matches!(node_data.get_node_type(), NodeType::Br) {
                content.push(InlineContent::LineBreak(InlineBreak {
                    break_type: BreakType::Hard,
                    clear: ClearType::None,
                    content_index: content.len(),
                }));
                continue;
            }

            // +spec:positioning:17239f - abspos elements are taken out of flow and must
            // not contribute their content to this IFC (laid out independently).
            if matches!(
                get_position_type(ctx.styled_dom, Some(dom_id)),
                LayoutPosition::Absolute | LayoutPosition::Fixed
            ) {
                continue;
            }

            // Non-text inline child - add as shape for inline-block
            let display = get_display_property(ctx.styled_dom, Some(dom_id)).unwrap_or_default();

            if display == LayoutDisplay::Inline {
                // Regular inline element - collect its text children
                let span_style = get_style_properties(
                    ctx.styled_dom,
                    dom_id,
                    ctx.system_style.as_ref(),
                    PhysicalSize::new(ctx.viewport_size.width, ctx.viewport_size.height),
                );
                collect_inline_span_recursive(
                    ctx,
                    text_cache,
                    tree,
                    dom_id,
                    &span_style,
                    content,
                    child_map,
                    &children,
                    constraints,
                )?;
            } else {
                // +spec:display-property:a37a9a - atomic inline-level boxes treated as neutral
                // characters in bidi reordering This is an atomic inline-level box
                // (e.g., inline-block, image): its size and baseline go to text3 with it.
                let shape = measure_atomic_inline(
                    ctx,
                    tree,
                    text_cache,
                    child_index,
                    dom_id,
                    constraints,
                )?;
                // For inline-block shapes, text3 uses the content array index as run_index
                // and always item_index=0 for objects. We must match this when inserting into
                // child_map.
                let shape_content_index = ContentIndex {
                    run_index: content.len() as u32,
                    item_index: 0,
                };
                content.push(InlineContent::Shape(shape));
                child_map.insert(shape_content_index, child_index);
            }
        }

        return Ok(());
    }

    // Regular (non-anonymous) IFC root: DOM traversal (its list marker, if
    // any, is already in `content` - `markers_on_first_line` above)

    // IMPORTANT: We need to traverse the DOM, not just the layout tree!
    //
    // According to CSS spec, a block container with inline-level children establishes
    // an IFC and should collect ALL inline content, including text nodes.
    // Text nodes exist in the DOM but might not have their own layout tree nodes.

    // Debug: Check what the node_hierarchy says about this node
    let node_hier_item = &ctx.styled_dom.node_hierarchy.as_container()[ifc_root_dom_id];
    debug_info!(
        ctx,
        "[collect_and_measure_inline_content] DEBUG: node_hier_item.first_child={:?}, \
         last_child={:?}",
        node_hier_item.first_child_id(ifc_root_dom_id),
        node_hier_item.last_child_id()
    );

    let ifc_root_node_data = &ctx.styled_dom.node_data.as_container()[ifc_root_dom_id];

    // SPECIAL CASE: If the IFC root itself is a text node (leaf node),
    // add its text content directly instead of iterating over children
    if let NodeType::Text(ref text_content) = ifc_root_node_data.get_node_type() {
        let style = crate::solver3::getters::get_style_properties_cached(
            &mut ctx.style_cache,
            ctx.styled_dom,
            ifc_root_dom_id,
            ctx.system_style.as_ref(),
            PhysicalSize::new(ctx.viewport_size.width, ctx.viewport_size.height),
        );
        let text_items = split_text_for_whitespace(
            ctx.styled_dom,
            ifc_root_dom_id,
            text_content.as_str(),
            &style,
        );
        content.extend(text_items);
        return Ok(());
    }

    let _ifc_root_node_type = match ifc_root_node_data.get_node_type() {
        NodeType::Div => "Div",
        NodeType::Text(_) => "Text",
        NodeType::Body => "Body",
        _ => "Other",
    };

    // [g138 az-web-lift] Collect `dom_children` HERE — immediately before the loop, AFTER the
    // get_node_type() calls above. Those calls were corrupting the `dom_children` Vec's stack-slot
    // header (g137 PROVED: `dom_children.len()` reads 1 right after `.collect()` but 0 in the
    // loop's `0..len` range a few calls later — the recurring SP-leak / stack-address
    // mis-lift). With NO call between this `.collect()` and the loop, the header survives the
    // range evaluation + first index.
    let dom_children: Vec<NodeId> = ifc_root_dom_id
        .az_children(&ctx.styled_dom.node_hierarchy.as_container())
        .collect();
    // [g139 az-web-lift] The loop's `dom_children.len()` read MIS-LIFTS to 0 even though the
    // in-memory value is 1 (g138: the volatile marker reads 1 but the loop's `0..len` range
    // reads 0 with NOTHING between — the optimizer's SROA'd len read is mis-tracked by the
    // lift; only a FORCED/volatile read is correct; same Vec-len mis-lift class as the original
    // sret bug, here on std `collect()` which can't be out-param'd). Read len via a volatile
    // round-trip (guaranteed-correct, like the marker) and index via get_unchecked (the index's
    // bounds-check len read mis-lifts the same way; len is valid → sound).
    // [g195 — collect_and_measure_inline_content_impl is DEAD on the web lift (NOT lifted for
    // hello-world OR web-nested-text; both lay out via measure_intrinsic_widths + layout_flow
    // instead). So this g139 Vec-len workaround never executes on the web lift → it's
    // irrelevant/deletable (kept: harmless, and unverified-dead for other layouts). The cron's
    // "collect_and_measure Vec-len" target is a DEAD PATH.]
    #[cfg(feature = "web_lift")]
    let dom_children_len = unsafe {
        crate::az_mark((0x606B4) as u32, (dom_children.len() as u32) as u32);
        crate::az_mark((0x606A4) as u32, (0x0000_6863u32) as u32);
        crate::az_mark_read(0x606B4) as usize
    };
    #[cfg(not(feature = "web_lift"))]
    let dom_children_len = dom_children.len();

    for item_idx in 0..dom_children_len {
        let dom_child_id = unsafe { *dom_children.get_unchecked(item_idx) };
        let content_index = ContentIndex {
            run_index: ifc_root_index as u32,
            item_index: item_idx as u32,
        };

        let node_data = &ctx.styled_dom.node_data.as_container()[dom_child_id];
        // [g136] loop body entered; capture the FIRST child's node_type (does it read as Text?).
        #[cfg(feature = "web_lift")]
        unsafe {
            if item_idx == 0 {
                crate::az_mark(
                    (0x606B8) as u32,
                    (match node_data.get_node_type() {
                        NodeType::Text(_) => 0xC0DE_7E70u32,
                        NodeType::Div => 0xC0DE_D11Fu32,
                        NodeType::Body => 0xC0DE_B0D1u32,
                        _ => 0xC0DE_0000u32,
                    }) as u32,
                );
            }
            crate::az_mark((0x606A4) as u32, (0x0000_6896u32) as u32);
        }

        // Check if this is a text node
        if let NodeType::Text(ref text_content) = node_data.get_node_type() {
            debug_info!(
                ctx,
                "[collect_and_measure_inline_content] OK: Found text node (DOM child {:?}): '{}'",
                dom_child_id,
                text_content.as_str()
            );

            // Get style from the TEXT NODE itself (dom_child_id), not the IFC root
            // This ensures inline styles like color: #666666 are applied to the text
            // Uses split_text_for_whitespace to correctly handle white-space: pre with \n
            let style = crate::solver3::getters::get_style_properties_cached(
                &mut ctx.style_cache,
                ctx.styled_dom,
                dom_child_id,
                ctx.system_style.as_ref(),
                PhysicalSize::new(ctx.viewport_size.width, ctx.viewport_size.height),
            );
            let text_items = split_text_for_whitespace(
                ctx.styled_dom,
                dom_child_id,
                text_content.as_str(),
                &style,
            );
            content.extend(text_items);
            // [g136] TEXT branch taken + pushed; content.len now.
            #[cfg(feature = "web_lift")]
            unsafe {
                crate::az_mark((0x606A4) as u32, (0x0000_6905u32) as u32);
                crate::az_mark((0x606BC) as u32, (content.len() as u32) as u32);
            }

            // Set IFC membership on the text node's layout node (if it exists)
            // Text nodes may or may not have their own layout tree entry depending on
            // whether they're wrapped in an anonymous IFC wrapper
            if let Some(&layout_idx) = tree
                .dom_to_layout
                .get(&dom_child_id)
                .and_then(|v| v.first())
            {
                if let Some(warm_mut) = tree.warm_mut(layout_idx) {
                    warm_mut.ifc_membership = Some(IfcMembership {
                        ifc_id,
                        ifc_root_layout_index: ifc_root_index,
                        run_index: current_run_index,
                    });
                }
            }
            current_run_index += 1;

            continue;
        }

        // A <br> forces a hard line break inside this IFC (see UA css: <br> is
        // inline). It needs no layout node of its own — just emit the break.
        if matches!(node_data.get_node_type(), NodeType::Br) {
            content.push(InlineContent::LineBreak(InlineBreak {
                break_type: BreakType::Hard,
                clear: ClearType::None,
                content_index: content.len(),
            }));
            continue;
        }

        // For non-text nodes, find their corresponding layout tree node
        let child_index = children
            .iter()
            .find(|&&idx| {
                tree.get(LayoutNodeId::new(idx))
                    .and_then(|n| n.dom_node_id)
                    .is_some_and(|id| id == dom_child_id)
            })
            .copied();

        let Some(child_index) = child_index else {
            debug_info!(
                ctx,
                "[collect_and_measure_inline_content] WARN: DOM child {:?} has no layout node",
                dom_child_id
            );
            continue;
        };

        // [g136] NON-TEXT branch taken (text child mis-classified?) — reached
        // tree.get(child_index).
        #[cfg(feature = "web_lift")]
        unsafe {
            crate::az_mark((0x606A4) as u32, (0x0000_6942u32) as u32);
        }
        let child_node = tree
            .get(LayoutNodeId::new(child_index))
            .ok_or(LayoutError::InvalidTree)?;
        // At this point we have a non-text DOM child with a layout node
        let dom_id = child_node.dom_node_id.unwrap();

        // +spec:positioning:17239f - abspos elements are taken out of flow
        // An out-of-flow child (position:absolute/fixed) is removed from normal flow
        // entirely: neither its box nor its (recursively) flattened text may participate
        // in this containing block's inline formatting context. It is laid out
        // independently by `process_out_of_flow_children`; contributing its content here
        // would double-render it at the static position. (Same predicate as
        // process_out_of_flow_children.)
        if matches!(
            get_position_type(ctx.styled_dom, Some(dom_id)),
            LayoutPosition::Absolute | LayoutPosition::Fixed
        ) {
            continue;
        }

        let display = get_display_property(ctx.styled_dom, Some(dom_id)).unwrap_or_default();
        if display != LayoutDisplay::Inline {
            // This is an atomic inline-level box (e.g., inline-block, image):
            // its size and baseline go to text3 with it.
            let shape = measure_atomic_inline(
                ctx,
                tree,
                text_cache,
                child_index,
                dom_id,
                constraints,
            )?;
            // For inline-block shapes, text3 uses the content array index as run_index
            // and always item_index=0 for objects. We must match this when inserting into
            // child_map.
            let shape_content_index = ContentIndex {
                run_index: content.len() as u32,
                item_index: 0,
            };
            content.push(InlineContent::Shape(shape));
            child_map.insert(shape_content_index, child_index);
        } else if matches!(
            ctx.styled_dom.node_data.as_container()[dom_id].get_node_type(),
            NodeType::Image(_)
        ) {
            push_inline_image(ctx, tree, child_index, dom_id, constraints, content, child_map)?;
        } else {
            // This is a regular inline box (display: inline) - e.g., <span>, <em>, <strong>
            //
            // According to CSS Inline-3 spec §2, inline boxes are "transparent" wrappers
            // We must recursively collect their text children with inherited style
            debug_info!(
                ctx,
                "[collect_and_measure_inline_content] Found inline span (DOM {:?}), recursing",
                dom_id
            );

            let span_style = get_style_properties(
                ctx.styled_dom,
                dom_id,
                ctx.system_style.as_ref(),
                PhysicalSize::new(ctx.viewport_size.width, ctx.viewport_size.height),
            );
            collect_inline_span_recursive(
                ctx,
                text_cache,
                tree,
                dom_id,
                &span_style,
                content,
                child_map,
                &children,
                constraints,
            )?;
        }
    }
    // [g134 az-web-lift DIAG] _impl reached its FINAL return; content.len as _impl sees it.
    #[cfg(feature = "web_lift")]
    unsafe {
        crate::az_mark((0x60698) as u32, (content.len() as u32) as u32);
        crate::az_mark((0x6069C) as u32, (0xC0DE069Cu32) as u32);
    }
    Ok(())
}

/// An `<img>` (a replaced element, `display: inline`) as a line's
/// [`InlineImage`]: its used size (intrinsic, constrained by CSS width /
/// height and the `width` / `height` attributes) set on its layout node and
/// handed to text3, mapped for positioning. The ONE image path of the IFC
/// collection: the IFC root's children and an inline span's children
/// ([`collect_inline_span_recursive`]) both come here - an `<img>` in `<a>`
/// was an empty inline span, its picture taking no room in the line
/// (MAILENG6 item 6).
pub(super) fn push_inline_image<T: ParsedFontTrait>(
    ctx: &LayoutContext<'_, T>,
    tree: &mut LayoutTree,
    child_index: usize,
    dom_id: NodeId,
    constraints: &LayoutConstraints<'_>,
    content: &mut Vec<InlineContent>,
    child_map: &mut HashMap<ContentIndex, usize>,
) -> Result<()> {
    // +spec:replaced-elements:31a782 - replaced elements (img) not rendered purely by CSS
    // box concepts Images are replaced elements - they have intrinsic
    // dimensions and CSS width/height can constrain them

    // Re-get child_node since we dropped it earlier for the inline-block case
    let child_node = tree
        .get(LayoutNodeId::new(child_index))
        .ok_or(LayoutError::InvalidTree)?;
    let box_props = child_node.box_props.unpack();

    // Get intrinsic size from the image data or fall back to layout node
    let intrinsic_size = tree
        .warm(LayoutNodeId::new(child_index))
        .and_then(|w| w.intrinsic_sizes)
        .unwrap_or_else(|| IntrinsicSizes {
            max_content_width: 50.0,
            max_content_height: 50.0,
            ..Default::default()
        });

    // Get styled node state for CSS property lookup
    let styled_node_state = ctx
        .styled_dom
        .styled_nodes
        .as_container()
        .get(dom_id)
        .map(|n| n.styled_node_state)
        .unwrap_or_default();

    // Calculate the used size respecting CSS width/height constraints
    let tentative_size = crate::solver3::sizing::calculate_used_size_for_node(
        ctx.styled_dom,
        Some(dom_id),
        &CBTY::from_flattened_with_width_type(
            atomic_inline_containing_block(constraints),
            constraints.available_width_type,
        ),
        intrinsic_size,
        &box_props,
        &ctx.viewport_size,
    )?;

    // Drop immutable borrow before mutable access
    drop(child_node);

    // Set the used_size on the layout node so paint_rect works correctly
    let final_size = LogicalSize::new(tentative_size.width, tentative_size.height);
    tree.get_mut(LayoutNodeId::new(child_index))
        .unwrap()
        .used_size = Some(final_size);

    // Calculate display size for text3 (this is what text3 uses for positioning)
    let display_width = if final_size.width > 0.0 {
        Some(final_size.width)
    } else {
        None
    };
    let display_height = if final_size.height > 0.0 {
        Some(final_size.height)
    } else {
        None
    };

    content.push(InlineContent::Image(InlineImage {
        // Snapshot the NODE, not the ImageRef: paint resolves the live
        // content (overlay→DOM) at display-list build, so a runtime
        // image swap repaints without rebuilding this IFC. (The old
        // `Ref` snapshot froze the ImageRef here — inline `<img>`
        // swaps stayed invisible until an unrelated full relayout.)
        source: ImageSource::Node(dom_id),
        intrinsic_size: crate::text3::cache::Size {
            width: intrinsic_size.max_content_width,
            height: intrinsic_size.max_content_height,
        },
        display_size: if display_width.is_some() || display_height.is_some() {
            Some(crate::text3::cache::Size {
                width: display_width.unwrap_or(intrinsic_size.max_content_width),
                height: display_height.unwrap_or(intrinsic_size.max_content_height),
            })
        } else {
            None
        },
        // Images are bottom-aligned with the baseline by default
        baseline_offset: 0.0,
        alignment: text3::cache::VerticalAlign::Baseline,
        object_fit: ObjectFit::Fill,
    }));
    // For images, text3 uses the content array index as run_index
    // and always item_index=0 for objects. We must match this.
    let image_content_index = ContentIndex {
        run_index: (content.len() - 1) as u32, // -1 because we just pushed
        item_index: 0,
    };
    child_map.insert(image_content_index, child_index);
    Ok(())
}

/// The `vertical-align` the content of an inline box NESTED in another one is
/// laid out with, relative to the IFC root's baseline - what text3 aligns
/// every run and atomic inline against. CSS 2.2 s10.8.1 aligns a box against
/// its PARENT inline box: `parent` is the enclosing box's alignment (already
/// relative to the root), `parent_shift_font` the font size its own `sub` /
/// `super` are measured against (its parent's), `own` the nested box's
/// declared alignment and `own_shift_font` the enclosing box's font size
/// (Chrome: the parent's font size / 5 + 1px down, / 3 + 1px up).
///
/// A baseline-aligned box sits on its parent's shifted baseline: it takes
/// `parent` as it is (`<sup><i>1</i></sup>`, the footnote mark, sat on the
/// line's baseline). The values measured from the parent's baseline (`sub`,
/// `super`, a length) add up into one raise (a superscript of a superscript
/// rises by both). The line- and content-area-relative values (`top`,
/// `bottom`, `middle`, `text-top`, `text-bottom`) keep their own reading.
pub(super) fn nested_vertical_align(
    parent: text3::cache::VerticalAlign,
    parent_shift_font: f32,
    own: text3::cache::VerticalAlign,
    own_shift_font: f32,
) -> text3::cache::VerticalAlign {
    use text3::cache::VerticalAlign as V;
    // How far `align` raises a box's baseline above its parent's (text3's
    // `Offset` is such a raise), for the values measured from it.
    let raise = |align: V, font: f32| match align {
        V::Baseline => Some(0.0),
        V::Sub => Some(-(font / 5.0 + 1.0)),
        V::Super => Some(font / 3.0 + 1.0),
        V::Offset(up) => Some(up),
        V::Top | V::Bottom | V::Middle | V::TextTop | V::TextBottom => None,
    };
    if matches!(own, V::Baseline) {
        return parent;
    }
    match (raise(parent, parent_shift_font), raise(own, own_shift_font)) {
        (Some(p), Some(o)) => V::Offset(p + o),
        _ => own,
    }
}

// +spec:display-property:c05c53 - inlinifying boxes can't contain block-level boxes; children are
// recursively inlinified it recursively inlinifies all of its in-flow children, so that no
// block-level descendants break up the inline formatting context in which it participates.
// +spec:display-property:aee879 - recursively inlinifies in-flow children of inline boxes
/// Recursively collects inline content from an inline span (display: inline) element.
///
/// According to CSS Inline Layout Module Level 3 §2:
///
/// "Inline boxes are transparent wrappers that wrap their content."
///
/// They don't create a new formatting context - their children participate in the
/// same IFC as the parent. This function processes:
///
/// - Text nodes: collected with the span's inherited style
/// - Nested inline spans: recursively descended
/// - Inline-blocks, images: measured and added as shapes
#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine
                                 // (one branch per case)
#[allow(clippy::too_many_arguments)] // the IFC collection's whole state, threaded down
pub(super) fn collect_inline_span_recursive<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    text_cache: &mut TextLayoutCache,
    tree: &mut LayoutTree,
    span_dom_id: NodeId,
    span_style: &StyleProperties,
    content: &mut Vec<InlineContent>,
    child_map: &mut HashMap<ContentIndex, usize>,
    // The layout children of the box this span sits in (the IFC root's, or
    // the enclosing span's); the span's own box is one of them.
    parent_children: &[usize],
    constraints: &LayoutConstraints<'_>,
) -> Result<()> {
    debug_info!(
        ctx,
        "[collect_inline_span_recursive] Processing inline span {:?}",
        span_dom_id
    );

    // Get DOM children of this span
    let span_dom_children: Vec<NodeId> = span_dom_id
        .az_children(&ctx.styled_dom.node_hierarchy.as_container())
        .collect();

    debug_info!(
        ctx,
        "[collect_inline_span_recursive] Span has {} DOM children",
        span_dom_children.len()
    );

    // +spec:box-model:b7428d - empty inline boxes still have margins, padding, borders, line-height
    // +spec:box-model:cc79a4 - empty inline elements still have margins, padding, borders and line
    // height
    if span_dom_children.is_empty() {
        let node_state = &ctx.styled_dom.styled_nodes.as_container()[span_dom_id].styled_node_state;
        let font_size = get_element_font_size(ctx.styled_dom, span_dom_id, node_state);

        let line_height = crate::solver3::getters::get_used_line_height(
            ctx.styled_dom,
            span_dom_id,
            node_state,
            font_size,
            PhysicalSize::new(ctx.viewport_size.width, ctx.viewport_size.height),
        );

        let cb_width = constraints
            .containing_block_size
            .main(constraints.writing_mode);
        let padding_top = get_css_padding_top(ctx.styled_dom, span_dom_id, node_state)
            .exact()
            .map_or(0.0, |pv| {
                pv.to_pixels_internal(cb_width, font_size, DEFAULT_FONT_SIZE)
            });
        let padding_bottom = get_css_padding_bottom(ctx.styled_dom, span_dom_id, node_state)
            .exact()
            .map_or(0.0, |pv| {
                pv.to_pixels_internal(cb_width, font_size, DEFAULT_FONT_SIZE)
            });
        let padding_left =
            crate::solver3::getters::get_css_padding_left(ctx.styled_dom, span_dom_id, node_state)
                .exact()
                .map_or(0.0, |pv| {
                    pv.to_pixels_internal(cb_width, font_size, DEFAULT_FONT_SIZE)
                });
        let padding_right =
            crate::solver3::getters::get_css_padding_right(ctx.styled_dom, span_dom_id, node_state)
                .exact()
                .map_or(0.0, |pv| {
                    pv.to_pixels_internal(cb_width, font_size, DEFAULT_FONT_SIZE)
                });
        let border_top = get_css_border_top_width(ctx.styled_dom, span_dom_id, node_state)
            .exact()
            .map_or(0.0, |pv| {
                pv.to_pixels_internal(cb_width, font_size, DEFAULT_FONT_SIZE)
            });
        let border_bottom = get_css_border_bottom_width(ctx.styled_dom, span_dom_id, node_state)
            .exact()
            .map_or(0.0, |pv| {
                pv.to_pixels_internal(cb_width, font_size, DEFAULT_FONT_SIZE)
            });
        let border_left = crate::solver3::getters::get_css_border_left_width(
            ctx.styled_dom,
            span_dom_id,
            node_state,
        )
        .exact()
        .map_or(0.0, |pv| {
            pv.to_pixels_internal(cb_width, font_size, DEFAULT_FONT_SIZE)
        });
        let border_right = crate::solver3::getters::get_css_border_right_width(
            ctx.styled_dom,
            span_dom_id,
            node_state,
        )
        .exact()
        .map_or(0.0, |pv| {
            pv.to_pixels_internal(cb_width, font_size, DEFAULT_FONT_SIZE)
        });
        let margin_left =
            crate::solver3::getters::get_css_margin_left(ctx.styled_dom, span_dom_id, node_state)
                .exact()
                .map_or(0.0, |pv| {
                    pv.to_pixels_internal(cb_width, font_size, DEFAULT_FONT_SIZE)
                });
        let margin_right =
            crate::solver3::getters::get_css_margin_right(ctx.styled_dom, span_dom_id, node_state)
                .exact()
                .map_or(0.0, |pv| {
                    pv.to_pixels_internal(cb_width, font_size, DEFAULT_FONT_SIZE)
                });

        let resolved_line_height = line_height.resolve(font_size, 0.0, 0.0, 0.0, 0);
        // CSS 2.2 10.8.1: an inline box with no glyphs holds a strut of its
        // first available font - line-height tall, straddling the baseline
        // like the strut of the line itself: that face's rounded A and D with
        // the leading shared out (`LayoutFontMetrics::inline_box_px`, a
        // glyph's own box). Its vertical padding and borders are no part of
        // the line box (10.6.1 / 10.8.1). It was line-height + padding +
        // borders tall and sat ON the baseline: `<span style="padding: 4px">`
        // made its line 27.2px (Chrome 18).
        let (strut_above, strut_below) = ctx
            .font_manager
            .first_available_font_metrics(&span_style.font_stack)
            .filter(|m| m.units_per_em > 0)
            .and_then(|m| m.inline_box_px(span_style.font_size_px, &span_style.line_height))
            .unwrap_or_else(|| {
                crate::text3::cache::split_leading(
                    resolved_line_height,
                    font_size * 0.8,
                    font_size * 0.2,
                )
            });
        let total_height = strut_above + strut_below;
        let total_width =
            margin_left + padding_left + border_left + border_right + padding_right + margin_right;

        // CSS 2.1 s9.4.2: an inline element with no content and no non-zero
        // margins, padding or borders makes nothing on its line - a line
        // holding only such elements is a phantom line box, zero tall
        // (`<div><a name="top"></a></div>`, a mail's anchor: Chrome 0). The
        // box below sits on the baseline at full line-height, so it made
        // that line 19.2px - and, with the strut in every line of boxes
        // (text3 `perform_fragment_layout`), 23.2.
        let is_phantom = [
            total_width,
            padding_top,
            padding_bottom,
            border_top,
            border_bottom,
        ]
        .iter()
        .all(|v| v.abs() < f32::EPSILON);
        if is_phantom {
            return Ok(());
        }

        content.push(InlineContent::Shape(InlineShape {
            shape_def: ShapeDefinition::Rectangle {
                size: crate::text3::cache::Size {
                    width: total_width,
                    height: total_height,
                },
                corner_radius: None,
            },
            fill: None,
            stroke: None,
            // From the bottom edge: the strut's share below the baseline.
            baseline_offset: strut_below,
            // The span's own, or - nested in a shifted box - the one its
            // caller folded in (`nested_vertical_align`).
            alignment: span_style.vertical_align,
            source_node_id: Some(span_dom_id),
        }));

        return Ok(());
    }

    // The layout children of THIS span's box. The tree builder processes an
    // inline box's children under the inline box's own node, so an element
    // nested in the span is a layout child of the span, not of the IFC root:
    // looking it up among the root's children (`parent_children`) never found
    // it, and an inline-block in a span was dropped from the line. A span the
    // tree has no node for keeps looking where its parent looked.
    let span_layout_children: Vec<usize> = parent_children
        .iter()
        .find(|&&idx| {
            tree.get(LayoutNodeId::new(idx))
                .and_then(|n| n.dom_node_id)
                .is_some_and(|id| id == span_dom_id)
        })
        .map_or_else(
            || parent_children.to_vec(),
            |&span_index| tree.children(span_index).to_vec(),
        );

    // What a box nested in this span aligns by (`nested_vertical_align`):
    // the span's alignment folded with its own. The font size this span's
    // own `sub` / `super` was measured against is its parent's.
    let span_shift_font = if matches!(
        span_style.vertical_align,
        text3::cache::VerticalAlign::Sub | text3::cache::VerticalAlign::Super
    ) {
        let span_state = ctx.styled_dom.styled_nodes.as_container()[span_dom_id].styled_node_state;
        get_parent_font_size(ctx.styled_dom, span_dom_id, &span_state)
    } else {
        // Unread: only `sub` / `super` are measured against a font size.
        0.0
    };
    let nested_align = |own: text3::cache::VerticalAlign| {
        nested_vertical_align(
            span_style.vertical_align,
            span_shift_font,
            own,
            span_style.font_size_px,
        )
    };

    for &child_dom_id in &span_dom_children {
        let node_data = &ctx.styled_dom.node_data.as_container()[child_dom_id];

        // CASE 1: Text node - collect with span's style
        if let NodeType::Text(ref text_content) = node_data.get_node_type() {
            debug_info!(
                ctx,
                "[collect_inline_span_recursive] ✓ Found text in span: '{}'",
                text_content.as_str()
            );
            let text_items = split_text_for_whitespace(
                ctx.styled_dom,
                child_dom_id,
                text_content.as_str(),
                &Arc::new(span_style.clone()),
            );
            content.extend(text_items);
            continue;
        }

        // CASE 1b: <br> inside an inline span forces a hard line break.
        if matches!(node_data.get_node_type(), NodeType::Br) {
            content.push(InlineContent::LineBreak(InlineBreak {
                break_type: BreakType::Hard,
                clear: ClearType::None,
                content_index: content.len(),
            }));
            continue;
        }

        // +spec:positioning:17239f - abspos elements are taken out of flow: an
        // out-of-flow descendant of an in-flow inline span must not contribute its
        // content to the enclosing IFC (it is laid out independently).
        if matches!(
            get_position_type(ctx.styled_dom, Some(child_dom_id)),
            LayoutPosition::Absolute | LayoutPosition::Fixed
        ) {
            continue;
        }

        // CASE 2: Element node - check its display type
        let child_display =
            get_display_property(ctx.styled_dom, Some(child_dom_id)).unwrap_or_default();

        // Find the corresponding layout tree node: a layout child of the span.
        let child_index = span_layout_children
            .iter()
            .find(|&&idx| {
                tree.get(LayoutNodeId::new(idx))
                    .and_then(|n| n.dom_node_id)
                    .is_some_and(|id| id == child_dom_id)
            })
            .copied();

        match child_display {
            // An `<img>` is a replaced box, not an inline span: the IFC
            // root's image path (`push_inline_image`), whatever wraps it.
            LayoutDisplay::Inline if matches!(node_data.get_node_type(), NodeType::Image(_)) => {
                let Some(child_index) = child_index else {
                    debug_info!(
                        ctx,
                        "[collect_inline_span_recursive] WARNING: img {:?} has no layout node",
                        child_dom_id
                    );
                    continue;
                };
                push_inline_image(
                    ctx,
                    tree,
                    child_index,
                    child_dom_id,
                    constraints,
                    content,
                    child_map,
                )?;
            }
            LayoutDisplay::Inline => {
                // Nested inline span - recurse with child's style
                debug_info!(
                    ctx,
                    "[collect_inline_span_recursive] Found nested inline span {:?}",
                    child_dom_id
                );
                let mut child_style = get_style_properties(
                    ctx.styled_dom,
                    child_dom_id,
                    ctx.system_style.as_ref(),
                    PhysicalSize::new(ctx.viewport_size.width, ctx.viewport_size.height),
                );
                // It aligns against THIS span, not the line (CSS 2.2 s10.8.1).
                child_style.vertical_align = nested_align(child_style.vertical_align);
                // And its text carries this span's lines (CSS Text Decoration
                // 3 §2.1: an inline box decorates everything in it).
                child_style.text_decoration =
                    child_style.text_decoration.with(span_style.text_decoration);
                collect_inline_span_recursive(
                    ctx,
                    text_cache,
                    tree,
                    child_dom_id,
                    &child_style,
                    content,
                    child_map,
                    &span_layout_children,
                    constraints,
                )?;
            }
            LayoutDisplay::InlineBlock
            | LayoutDisplay::InlineFlex
            | LayoutDisplay::InlineGrid
            | LayoutDisplay::InlineTable => {
                // An atomic inline inside the span (an inline-block, and the
                // inline-level flex / grid / table boxes, CSS Display 3 §2.4)
                // is the same atomic inline as a direct child of the IFC root
                // (an inline box is a transparent wrapper): measured and
                // mapped for positioning exactly like one. The inline-flex /
                // grid / table boxes fell to the "inlinify" arm below and
                // poured their children into the line (MAILENG6 item 6).
                let Some(child_index) = child_index else {
                    debug_info!(
                        ctx,
                        "[collect_inline_span_recursive] WARNING: atomic inline {:?} has no \
                         layout node",
                        child_dom_id
                    );
                    continue;
                };
                let mut shape = measure_atomic_inline(
                    ctx,
                    tree,
                    text_cache,
                    child_index,
                    child_dom_id,
                    constraints,
                )?;
                // It aligns against THIS span, not the line (CSS 2.2 s10.8.1).
                shape.alignment = nested_align(shape.alignment);
                // For inline-block shapes, text3 uses the content array index as run_index
                // and always item_index=0 for objects. We must match this when inserting into
                // child_map.
                let shape_content_index = ContentIndex {
                    run_index: content.len() as u32,
                    item_index: 0,
                };
                content.push(InlineContent::Shape(shape));
                child_map.insert(shape_content_index, child_index);
            }
            _ => {
                // +spec:display-property:0684c4 - block box inlinified: inner display becomes
                // flow-root (treated as atomic inline) in-flow children of an
                // inline box are recursively inlinified so they don't break the
                // IFC. Treat them as inline spans and recurse into their
                // children to collect text and inline content.
                debug_info!(
                    ctx,
                    "[collect_inline_span_recursive] Inlinifying block-level child {:?} (display: \
                     {:?}) inside inline span per css-display-3 §2.7",
                    child_dom_id,
                    child_display
                );
                let mut child_style = get_style_properties(
                    ctx.styled_dom,
                    child_dom_id,
                    ctx.system_style.as_ref(),
                    PhysicalSize::new(ctx.viewport_size.width, ctx.viewport_size.height),
                );
                child_style.vertical_align = nested_align(child_style.vertical_align);
                child_style.text_decoration =
                    child_style.text_decoration.with(span_style.text_decoration);
                collect_inline_span_recursive(
                    ctx,
                    text_cache,
                    tree,
                    child_dom_id,
                    &child_style,
                    content,
                    child_map,
                    &span_layout_children,
                    constraints,
                )?;
            }
        }
    }

    Ok(())
}
