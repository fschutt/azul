//! Inline formatting contexts: line layout through text3, text-box-trim and initial letters.

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

// Inline Formatting Context (CSS 2.2 § 9.4.2)
// +spec:display-property:ede6f4 - inline layout: mixed stream of text and inline-level boxes

/// Lays out an Inline Formatting Context (IFC) by delegating to the `text3` engine.
///
/// This function acts as a bridge between the box-tree world of `solver3` and the
/// rich text layout world of `text3`. Its responsibilities are:
///
/// 1. **Collect Content**: Traverse the direct children of the IFC root and convert them into a
///    `Vec<InlineContent>`, the input format for `text3`. This involves:
///
///     - Recursively laying out `inline-block` children to determine their final size and baseline,
///       which are then passed to `text3` as opaque objects.
///     - Extracting raw text runs from inline text nodes.
///
/// 2. **Translate Constraints**: Convert the `LayoutConstraints` (available space, floats) from
///    `solver3` into the more detailed `UnifiedConstraints` that `text3` requires.
///
/// 3. **Invoke Text Layout**: Call the `text3` cache's `layout_flow` method to perform the complex
///    tasks of BIDI analysis, shaping, line breaking, justification, and vertical alignment.
///    +spec:display-property:e96c82 - inline formatting context: flow of elements/text wrapped into
///    lines
///
/// 4. **Integrate Results**: Process the `UnifiedLayout` returned by `text3`:
///
///     - Store the rich layout result on the IFC root `LayoutNode` for the display list generation
///       pass.
///     - Update the `positions` map for all `inline-block` children based on the positions
///       calculated by `text3`.
///     - Extract the final overflow size and baseline for the IFC root itself
// NOTE(writing-modes): The IFC currently assumes inline direction = horizontal
// and block direction = vertical. In vertical writing modes, line boxes would
// stack horizontally and inline content would flow vertically. The writing mode
// is now available via constraints.writing_mode_ctx for agents to use when
// implementing vertical text layout in the text3 engine.
// +spec:display-property:574e7b - text-box-trim for inline boxes trims block-end to content edge
// (TODO: implement trimming per text-box-edge metric) +spec:display-property:da284a - IFC: flow
// inline-level boxes into line boxes, size/position each fragment +spec:inline-formatting-context:
// 275f64 - IFC: boxes laid out horizontally into line boxes, respecting margins/borders/padding
#[allow(clippy::field_reassign_with_default)] // struct built incrementally / test setup; a struct literal is not clearer here
#[allow(clippy::too_many_lines, clippy::cognitive_complexity)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
/// CSS Inline 3 §6.2 text-box-trim for the IFC's block container, applied to
/// the finished [`LayoutOutput`]. MUST run on EVERY `layout_ifc` exit that
/// produces content bounds - the incremental cache-reuse arms included -
/// or measure passes that hit the cache size the box untrimmed.
pub(super) fn apply_text_box_trim(
    styled_dom: &StyledDom,
    ifc_root_dom_id: NodeId,
    cached_constraints: &UnifiedConstraints,
    has_items: bool,
    output: &mut LayoutOutput,
) {
    // +spec:box-model:929f42 - text-box-trim: trim half-leading from first/last formatted line
    // +spec:box-model:02e0f9 - text-box-trim: trim-end and trim-both, no effect with non-zero
    // padding/border
    //
    // CSS Inline 3 § 6.2: For block containers, trim the block-start/block-end side
    // of the first/last formatted line. If there is intervening non-zero padding or
    // borders, there is no effect. Does not apply to flex, grid, or table contexts.
    let ifc_node_state = &styled_dom.styled_nodes.as_container()[ifc_root_dom_id].styled_node_state;
    // Fast path: if no node in the DOM declared text-box-trim, the cascade
    // walk would always return None → skip it.
    let text_box_trim = {
        let skip = styled_dom
            .css_property_cache
            .ptr
            .compact_cache
            .as_ref()
            .is_some_and(|cc| {
                cc.dom_declared_flags & azul_css::compact_cache::DOM_HAS_TEXT_BOX_TRIM == 0
            });
        if skip {
            StyleTextBoxTrim::None
        } else {
            get_text_box_trim_property(styled_dom, ifc_root_dom_id, ifc_node_state)
                .unwrap_or(StyleTextBoxTrim::None)
        }
    };

    if text_box_trim != StyleTextBoxTrim::None && has_items {
        // Half-leading = (line-height - (ascent + descent)) / 2
        let half_leading = (cached_constraints.resolved_line_height()
            - (cached_constraints.strut_ascent + cached_constraints.strut_descent))
            / 2.0;
        let half_leading = half_leading.max(0.0);

        // +spec:display-property:db5125 - text-box-edge selects the metric the trim cuts to
        // +spec:font-metrics:d3b654 - cap/alphabetic edges use the cap-height and alphabetic
        // baseline The over edge trims PAST the half-leading down to the chosen
        // metric: `cap` cuts ascent - cap-height further, `ex` cuts
        // ascent - x-height; the under edge's `alphabetic` cuts the whole
        // descent (down to the baseline). `text` (and `auto`, and the
        // ideographic metrics we have no strut data for) trim the
        // half-leading only. Trimming reduces the IFC's block size; the
        // first line's glyphs keep their positions (the same model the
        // half-leading-only implementation used - a start-trim shift of
        // the line stack is still TODO).
        let edge = get_text_box_edge_property(styled_dom, ifc_root_dom_id, ifc_node_state)
            .unwrap_or(azul_css::props::style::text::StyleTextBoxEdge::AUTO);
        let over_extra = match edge.over {
            azul_css::props::style::text::TextBoxEdgeOver::Cap => {
                (cached_constraints.strut_ascent - cached_constraints.strut_cap_height).max(0.0)
            }
            azul_css::props::style::text::TextBoxEdgeOver::Ex => {
                (cached_constraints.strut_ascent - cached_constraints.strut_x_height).max(0.0)
            }
            _ => 0.0,
        };
        let under_extra = match edge.under {
            azul_css::props::style::text::TextBoxEdgeUnder::Alphabetic => {
                cached_constraints.strut_descent.max(0.0)
            }
            _ => 0.0,
        };

        // Check for intervening non-zero padding/border on block-start (top)
        let has_pad_or_border_top =
            match get_css_padding_top(styled_dom, ifc_root_dom_id, ifc_node_state) {
                MultiValue::Exact(pv) => pv.number.get() != 0.0,
                _ => false,
            } || match get_css_border_top_width(styled_dom, ifc_root_dom_id, ifc_node_state) {
                MultiValue::Exact(pv) => pv.number.get() != 0.0,
                _ => false,
            };

        // Check for intervening non-zero padding/border on block-end (bottom)
        let has_pad_or_border_bottom =
            match get_css_padding_bottom(styled_dom, ifc_root_dom_id, ifc_node_state) {
                MultiValue::Exact(pv) => pv.number.get() != 0.0,
                _ => false,
            } || match get_css_border_bottom_width(styled_dom, ifc_root_dom_id, ifc_node_state) {
                MultiValue::Exact(pv) => pv.number.get() != 0.0,
                _ => false,
            };

        let trim_start = matches!(
            text_box_trim,
            StyleTextBoxTrim::TrimStart | StyleTextBoxTrim::TrimBoth
        ) && !has_pad_or_border_top;
        let trim_end = matches!(
            text_box_trim,
            StyleTextBoxTrim::TrimEnd | StyleTextBoxTrim::TrimBoth
        ) && !has_pad_or_border_bottom;

        let mut height_reduction = 0.0;
        if trim_start {
            height_reduction += half_leading + over_extra;
        }
        if trim_end {
            height_reduction += half_leading + under_extra;
        }

        if height_reduction > 0.0 {
            output.overflow_size.height = (output.overflow_size.height - height_reduction).max(0.0);
        }
    }
}

/// The height of the strut line box an EMPTY IFC root keeps when it is an
/// editing host (or inside one): one `line-height` of its font, the same
/// rect the caret painter uses (`display_list::empty_editable_caret_rect`).
/// `None` for every other empty IFC — those render nothing.
pub(super) fn editing_host_strut_height<T: ParsedFontTrait>(
    ctx: &LayoutContext<'_, T>,
    tree: &LayoutTree,
    node_index: usize,
) -> Option<f32> {
    let dom_id = tree.get(LayoutNodeId::new(node_index))?.dom_node_id?;
    if !crate::solver3::getters::is_node_contenteditable_inherited(ctx.styled_dom, dom_id) {
        return None;
    }
    let node_state = &ctx.styled_dom.styled_nodes.as_container()[dom_id].styled_node_state;
    let font_size = get_element_font_size(ctx.styled_dom, dom_id, node_state);
    // `normal` as 1.2em. (This read `normalized()` as a factor, so an absolute
    // line-height - stored negative - gave a 1px strut.)
    let line_height = crate::solver3::getters::get_used_line_height(
        ctx.styled_dom,
        dom_id,
        node_state,
        font_size,
        PhysicalSize::new(ctx.viewport_size.width, ctx.viewport_size.height),
    )
    .resolve(font_size, 0.0, 0.0, 0.0, 0);
    Some(
        crate::solver3::display_list::empty_editable_caret_rect(line_height)
            .size
            .height,
    )
}

/// The extent of a laid-out IFC: its `overflow_size` - the ONE computation
/// every exit of [`layout_ifc`] reports, the fresh layout and the cache-reuse
/// path alike, so two layouts of unchanged content cannot drift (a textarea
/// measured 13.0 px tall once and 12.999999 px after a slider drag when the
/// reuse exit took `bounds()` alone).
///
/// The UNCLIPPED content bounds, not `bounds()` alone. `bounds()` maxes over
/// `layout.items`, which under dense-text retention is an empty sentinel (the
/// real clusters live in the dense view) - so `bounds()` collapses to 0 and a
/// horizontal scroll container's overflow was never detected
/// (`overflow_size.width == 0` → `needs_horizontal == false` → the value `<p>`
/// of a single-line field never became a scroll box the caret-reveal could
/// shift: the append-only caret bug). `unclipped_bounds` is captured during
/// line breaking to enclose every positioned item - and the line box a lone
/// `<br>` ends, which no item spans - and survives the sentinel swap. The max
/// of the two, so a path that leaves `unclipped_bounds` at its default still
/// gets the fragment's own bounds.
pub(super) fn ifc_extent(main_frag: &text3::cache::UnifiedLayout) -> LogicalSize {
    let frag_bounds = main_frag.bounds();
    let unclipped = main_frag.overflow.unclipped_bounds;
    LogicalSize::new(
        frag_bounds.width.max(unclipped.width),
        frag_bounds.height.max(unclipped.height),
    )
}

pub(super) fn layout_ifc<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    text_cache: &mut TextLayoutCache,
    tree: &mut LayoutTree,
    node_index: usize,
    constraints: &LayoutConstraints<'_>,
) -> Result<LayoutOutput> {
    unsafe {
        crate::az_mark(0x60704_u32, (0x20u32));
    }
    // [g147 az-web-lift DIAG] CALLER-side tree validity at layout_ifc entry, indexed by node_index
    // (0x60900+ = nodes.len, 0x60920+ = tree ptr) to dodge marker-overwrite across multiple IFCs.
    // Compare vs _impl's CALLEE-side (0x60940+/0x60960+): ptr differs ⇒ &mut tree mis-passes across
    // the call; ptr same but len differs ⇒ the tree's `nodes` Vec is emptied in place.
    #[cfg(feature = "web_lift")]
    unsafe {
        let slot = (node_index & 7) * 4;
        crate::az_mark((0x60900 + slot) as u32, (tree.nodes.len() as u32) as u32);
        crate::az_mark(
            (0x60920 + slot) as u32,
            ((&*tree as *const LayoutTree as usize) as u32) as u32,
        );
    }
    let float_count = constraints
        .bfc_state
        .as_ref()
        .map_or(0, |s| s.floats.floats.len());
    debug_info!(
        ctx,
        "[layout_ifc] ENTRY: node_index={}, has_bfc_state={}, float_count={}",
        node_index,
        constraints.bfc_state.is_some(),
        float_count
    );
    debug_ifc_layout!(ctx, "CALLED for node_index={}", node_index);

    // +spec:display-property:7f3c1d - Anonymous inline boxes: text directly in block containers
    // treated as anonymous inline elements in IFC +spec:display-property:5a795c - root inline
    // box: block container generates anonymous inline box holding all inline-level contents,
    // inheriting from parent For anonymous boxes, we need to find the DOM ID from a parent or
    // child CSS 2.2 § 9.2.1.1: Anonymous boxes inherit properties from their enclosing box
    let node = tree
        .get(LayoutNodeId::new(node_index))
        .ok_or(LayoutError::InvalidTree)?;
    // An anonymous box borrows an element's id to resolve its style, but
    // only the INHERITED properties are its own (§9.2.1.1): it has no
    // columns of the element's (see translate_to_text3_constraints).
    let ifc_root_is_anonymous = node.dom_node_id.is_none();
    let ifc_root_dom_id =
        ifc_root_style_dom_id(tree, node_index).ok_or(LayoutError::InvalidTree)?;

    debug_ifc_layout!(ctx, "ifc_root_dom_id={:?}", ifc_root_dom_id);

    // +spec:display-property:a469a6 - line boxes created as needed for inline-level content in IFC
    // +spec:display-property:f3c875 - calculate layout bounds (size contributions) of each
    // inline-level box Phase 1: Collect and measure all inline-level children.
    // Fold the IFC subtree's per-node fingerprints into one key. The
    // reconcile pass already computes and stores these (they are what marks
    // a node clean/dirty), so this is a handful of hashes over data already
    // in cache — versus re-resolving the FULL cascade
    // (`get_style_properties`) for every text run and inline span, which is
    // what collection does and what made it 2 ms per IFC / 32 ms per
    // pagination even when the line layout was going to be reused.
    let subtree_fingerprint = {
        use core::hash::{Hash, Hasher};
        let mut h = azul_core::hash::FastHasher::new();

        // vw/vh/vmin/vmax resolve against the viewport, so a resize MUST
        // invalidate collected styles — but ONLY for documents that actually
        // use a viewport unit. Folding the viewport in unconditionally
        // invalidated EVERY collection on EVERY resize (552 re-collections
        // ≈ 19.6 ms per resize on big.md) to protect a feature the document
        // did not use. `uses_viewport_units` is detected at compact-build
        // time from the same values the cache encodes; a missing compact
        // cache degrades to the old always-invalidate behaviour.
        let doc_uses_viewport_units = ctx
            .styled_dom
            .css_property_cache
            .ptr
            .compact_cache
            .as_ref()
            .is_none_or(|cc| cc.uses_viewport_units);
        if doc_uses_viewport_units {
            ctx.viewport_size.width.to_bits().hash(&mut h);
            ctx.viewport_size.height.to_bits().hash(&mut h);
        }

        let compact = ctx.styled_dom.css_property_cache.ptr.compact_cache.as_ref();
        let mut stack = alloc::vec![node_index];
        while let Some(idx) = stack.pop() {
            if let Some(cold) = tree.cold(LayoutNodeId::new(idx)) {
                cold.node_data_fingerprint.hash(&mut h);
                // NodeDataFingerprint covers the node's own data and INLINE
                // css — not what the AUTHOR STYLESHEET resolved onto it. Two
                // DOMs with identical nodes but different stylesheets would
                // otherwise share a key and serve each other's styles. The
                // compact cache holds the RESOLVED values, so folding this
                // node's entries in makes any cascade change invalidate.
                let dom_id_opt = tree.get(LayoutNodeId::new(idx)).and_then(|n| n.dom_node_id);
                if let (Some(cc), Some(dom_id)) = (compact, dom_id_opt) {
                    hash_resolved_style(cc, dom_id.index(), &mut h);
                }
            }
            idx.hash(&mut h);
            for &c in tree.children(idx) {
                stack.push(c);
            }
        }
        h.finish()
    };

    // Visit-type census (AZ_PROFILE=cpu): which constraint TYPE reaches this
    // IFC. Min/Max-content visits that fall through to layout_flow are the
    // measure-vs-final cache thrash — intrinsic WIDTHS are cached on warm,
    // so a min/max-content visit that still re-runs line breaking is either
    // a min-content-HEIGHT request or a bug.
    drop(crate::probe::Probe::span(
        match constraints.available_width_type {
            Text3AvailableSpace::Definite(_) => "ifc_visit_definite",
            Text3AvailableSpace::MinContent => "ifc_visit_min",
            Text3AvailableSpace::MaxContent => "ifc_visit_max",
        },
    ));

    let cached_collection = tree
        .warm(LayoutNodeId::new(node_index))
        .and_then(|w| w.inline_content_cache.as_ref())
        .filter(|c| c.subtree_fingerprint == subtree_fingerprint)
        .filter(|c| {
            c.atomics_measured_against
                .is_none_or(|key| key == atomic_inline_measure_key(constraints))
        })
        .map(|c| (c.content.clone(), c.child_map.clone(), c.content_hash_base));

    // Second precondition, checked HERE because this is the only place that can.
    //
    // The fingerprint proves the collection is unchanged. It says nothing about
    // the cache's other half: `collect_and_measure_inline_content` does not only
    // collect, it LAYS OUT every atomic inline-level child, and that layout is
    // written to the TREE, not to the cache. Reuse the collection in a tree
    // where those children have not been laid out and the only call that would
    // size them is skipped — they keep their 0x0 default while the cached
    // `InlineShape` still carries the size measured in some earlier tree. The
    // text then flows correctly around a box that paints and hit-tests as
    // nothing.
    let cached_collection = match cached_collection {
        Some((content, child_map, base))
            if atomic_inline_children_are_laid_out(tree, &content, &child_map) =>
        {
            Some((content, child_map, base))
        }
        Some(_) => {
            debug_info!(
                ctx,
                "[layout_ifc] node {}: cached collection REJECTED — an atomic inline child is not \
                 laid out in this tree; re-collecting",
                node_index
            );
            None
        }
        None => None,
    };

    // `content_hash_base` rides with the collection: hashed ONCE per rebuild,
    // reused by every subsequent visit (see CachedInlineContent::content_hash_base
    // for the 29 ms this replaces).
    let (collect_result, content_hash_base) =
        if let Some((content, child_map, base)) = cached_collection {
            drop(crate::probe::Probe::span("ifc_collect_cached"));
            (Ok((content, child_map)), Some(base))
        } else {
            let _p = crate::probe::Probe::span("ifc_collect_content");
            let res =
                collect_and_measure_inline_content(ctx, text_cache, tree, node_index, constraints)
                    .map(|(content, child_map)| (Arc::new(content), Arc::new(child_map)));
            let mut base = None;
            if let Ok((content, child_map)) = res.as_ref() {
                let computed_base = {
                    let _p = crate::probe::Probe::span("ifc_content_hash_base");
                    use std::hash::{Hash, Hasher};
                    let mut h = azul_core::hash::FastHasher::new();
                    content.hash(&mut h);
                    h.finish()
                };
                base = Some(computed_base);
                if let Some(w) = tree.warm_mut(LayoutNodeId::new(node_index)) {
                    w.inline_content_cache =
                        Some(Box::new(crate::solver3::layout_tree::CachedInlineContent {
                            content: Arc::clone(content),
                            child_map: Arc::clone(child_map),
                            subtree_fingerprint,
                            content_hash_base: computed_base,
                            atomics_measured_against: content
                                .iter()
                                .any(|c| matches!(c, InlineContent::Shape(_)))
                                .then(|| atomic_inline_measure_key(constraints)),
                        }));
                }
            }
            (res, base)
        };
    // [g133 az-web-lift DIAG] which early-return fires in POSITIONING's layout_ifc.
    #[cfg(feature = "web_lift")]
    unsafe {
        crate::az_mark(
            (0x60680) as u32,
            (collect_result.as_ref().map(|(c, _)| c.len()).unwrap_or(0) as u32) as u32,
        );
        crate::az_mark(
            (0x60684) as u32,
            (if collect_result.is_ok() {
                0xC0DE0680u32
            } else {
                0x000000EEu32
            }) as u32,
        );
    }
    let (inline_content, child_map) = collect_result?;

    // #11 fix: hash the inline content once. Used to (a) skip stale Phase 2d
    // fast-path reuse and (b) force a cache REPLACE when content changed even
    // though available width is unchanged — the display-list generator paints
    // text from the cached `inline_layout_result` (display_list.rs), so a
    // content change at a same-width constraint MUST overwrite it or the old
    // glyphs keep rendering (#11 stale display list).
    // Phase 2 (translate early): resolve the container-level (IFC) constraints now,
    // so the Phase 2d cache-reuse decision below can key on them too. Reuse was keyed
    // on available width + per-run content hash only; a change to a container-level
    // property (text-align, text-align-last, text-indent, direction, line-height,
    // white-space, columns) — which is NOT covered by the per-run content hash — would
    // otherwise silently reuse a stale, differently-aligned/indented cached layout.
    let mut text3_constraints = translate_to_text3_constraints(
        ctx,
        constraints,
        ctx.styled_dom,
        ifc_root_dom_id,
        ifc_root_is_anonymous,
    );
    // CSS 2.1 s16.1 / CSS Text 3 s8.1: `text-indent` indents the FIRST
    // formatted line of the block container. An anonymous block (which
    // borrows the container's style) holds that line only when it is the
    // container's first in-flow box: the text after a nested block
    // (`<div>first<div>..</div>after</div>`) is not indented (Chrome);
    // it was (TEXT7's finding). `each-line` keeps its own rule.
    if ifc_root_is_anonymous
        && !text3_constraints.text_indent_each_line
        && !anonymous_block_holds_the_first_line(tree, ctx.styled_dom, node_index)
    {
        text3_constraints.text_indent = 0.0;
    }
    // An SVG `<text>`: its sizes are its `<svg>`'s user units, on the screen
    // at the scale of the viewBox onto the `<svg>`'s current box and of its
    // transforms (`solver3::svg`). The cached collection stays in user units;
    // the scale enters the validity key, so a resized `<svg>` lays it out
    // again.
    let (inline_content, content_hash_base) =
        match super::super::svg::text_scale(ctx.styled_dom, tree, node_index) {
            Some(scale) => {
                super::super::svg::scale_constraints(&mut text3_constraints, scale);
                (
                    Arc::new(super::super::svg::svg_text_content(ctx.styled_dom, &inline_content, scale)),
                    content_hash_base.map(|base| base ^ u64::from(scale.to_bits()).rotate_left(17)),
                )
            }
            None => (inline_content, content_hash_base),
        };

    let current_content_hash = {
        let _p = crate::probe::Probe::span("ifc_content_hash");
        use std::hash::{Hash, Hasher};
        let mut h = azul_core::hash::FastHasher::new();
        // The content component comes pre-hashed from the collection cache —
        // an equal subtree_fingerprint admitted it, so its bytes are the ones
        // this hash used to re-derive per visit. `None` cannot happen when
        // `inline_content` exists (the miss arm computes a base for every Ok
        // collection), but fall back to hashing rather than unwrapping.
        match content_hash_base {
            Some(base) => base.hash(&mut h),
            None => inline_content.hash(&mut h),
        }
        // Fold the constraint-relevant container properties into the validity key.
        text3_constraints.text_align.hash(&mut h);
        text3_constraints.text_align_last.hash(&mut h);
        text3_constraints.white_space_mode.hash(&mut h);
        text3_constraints.direction.hash(&mut h);
        text3_constraints.columns.hash(&mut h);
        // A split into a multi-column flow moves the lines: a layout of the
        // other split (or of none) must not be reused.
        text3_constraints.column_flow.hash(&mut h);
        text3_constraints.text_indent.to_bits().hash(&mut h);
        match text3_constraints.line_height {
            text3::cache::LineHeight::Normal => 0u64.hash(&mut h),
            text3::cache::LineHeight::Px(v) => {
                1u64.hash(&mut h);
                v.to_bits().hash(&mut h);
            }
        }
        h.finish()
    };

    debug_info!(
        ctx,
        "[layout_ifc] Collected {} inline content items for node {}",
        inline_content.len(),
        node_index
    );
    for (i, item) in inline_content.iter().enumerate() {
        match item {
            InlineContent::Text(run) => debug_info!(ctx, "  [{}] Text: '{}'", i, run.text),
            InlineContent::Marker {
                run,
                position_outside,
            } => debug_info!(
                ctx,
                "  [{}] Marker: '{}' (outside={})",
                i,
                run.text,
                position_outside
            ),
            InlineContent::Shape(_) => debug_info!(ctx, "  [{}] Shape", i),
            InlineContent::Image(_) => debug_info!(ctx, "  [{}] Image", i),
            _ => debug_info!(ctx, "  [{}] Other", i),
        }
    }

    debug_ifc_layout!(
        ctx,
        "Collected {} inline content items",
        inline_content.len()
    );

    if inline_content.is_empty() {
        debug_warning!(ctx, "inline_content is empty, returning default output!");
        // THE EDITING-HOST STRUT. An IFC root with nothing to type into yet
        // — an empty TextInput's value `<p>`, a `<div contenteditable>`
        // before its first character — still gets ONE line box, the strut:
        // one line-height tall, no items. That is the line the caret stands
        // on (display_list `paint_cursor` paints the strut caret for an
        // inline layout with no items) and the height the block keeps, so a
        // focused empty field is neither blank nor collapsed. Browsers do
        // the same (the editing host's placeholder `<br>`). Every OTHER
        // empty IFC renders nothing, as before.
        if let Some(strut_height) = editing_host_strut_height(ctx, tree, node_index) {
            if let Some(warm_node) = tree.warm_mut(LayoutNodeId::new(node_index)) {
                // WITH the IFC's constraints, exactly like a real line box:
                // `reshape_text_node` shapes an edit into the cached layout's
                // constraints and bails when there are none. Stored bare
                // (`CachedInlineLayout::new`), the strut was a line the caret
                // could stand on but nothing could be typed into — the first
                // keystroke into an empty TextInput was silently dropped.
                warm_node.inline_layout_result =
                    Some(Box::new(CachedInlineLayout::new_with_constraints(
                        Arc::new(text3::cache::UnifiedLayout {
                            items: Vec::new(),
                            overflow: text3::cache::OverflowInfo::default(),
                        }),
                        constraints.available_width_type,
                        false,
                        text3_constraints,
                    )));
                // A strut has no glyphs: its baseline sits where a line of
                // this font would put one (ascent ~ 0.8 em within the line).
                warm_node.baseline = Some(strut_height * 0.8);
            }
            ctx.reflowed_ifcs.insert(node_index);
            return Ok(LayoutOutput {
                positions: BTreeMap::new(),
                overflow_size: LogicalSize::new(0.0, strut_height),
                baseline: Some(strut_height * 0.8),
                static_positions: BTreeMap::new(),
            });
        }
        // The node has no inline-level content this pass (e.g. its only
        // inline child — a text run or an inline image — was removed by a
        // relayout). Any `inline_layout_result` left over from a previous
        // frame is now stale: the display-list generator paints inline
        // objects (images, inline-block shapes) straight out of this cached
        // layout (see display_list.rs `paint_inline_*`), so a leftover entry
        // would re-emit the removed content AND index `styled_nodes` with a
        // `source_node_id` that no longer exists in the new DOM (OOB panic).
        // Clear it so the empty IFC renders nothing.
        if let Some(warm_node) = tree.warm_mut(LayoutNodeId::new(node_index)) {
            warm_node.inline_layout_result = None;
        }
        return Ok(LayoutOutput::default());
    }

    // === Phase 2d: IFC incremental relayout decision tree ===
    //
    // Check if a cached layout exists with matching constraints. If so,
    // try incremental relayout (GlyphSwap or LineShift) before falling
    // back to full layout_flow().
    {
        let cached_ifc = tree
            .warm(LayoutNodeId::new(node_index))
            .and_then(|n| n.inline_layout_result.as_ref());

        // Only reuse the cached inline layout when the available WIDTH is unchanged.
        // This fast path was built for text edits (content changes, width constant); on a
        // viewport/container resize the width differs and the text must RE-WRAP, so the
        // cached old-width layout must NOT be reused — fall through to full layout_flow()
        // below. Without this guard, resizing kept the stale line breaks (#45). Real
        // text-edit incremental relayout (with dirty items) lives in
        // LayoutWindow::try_incremental_text_relayout.
        let resize_has_floats = constraints
            .bfc_state
            .as_ref()
            .is_some_and(|s| !s.floats.floats.is_empty());
        // #11 fix: cache validity is keyed on WIDTH only, so a same-width
        // RefreshDom whose text CHANGED would otherwise reuse the stale shaped
        // layout. Require the inline content hash to match too.
        //
        // Re-flow triage (AZ_PROFILE=cpu): a steady-state fixed-page-width
        // resize still ran text_layout_flow 477× — these markers name which
        // gate rejected the cached layout for every one of those.
        let cached_ifc = match cached_ifc {
            None => {
                drop(crate::probe::Probe::span("ifc_reflow_cold"));
                None
            }
            Some(c) if !c.is_valid_for(constraints.available_width_type, resize_has_floats) => {
                // Finer buckets: dd = both DEFINITE (then by delta size —
                // "small" is sub-pixel jitter above the 0.1 eps, a rounding
                // provenance bug, not a real width change), type = the
                // constraint TYPE flipped (measure min/max-content vs final
                // definite), float = the float-gain rule.
                use Text3AvailableSpace as Avs;
                let reason = if resize_has_floats && !c.has_floats {
                    "ifc_reflow_width_floatgain"
                } else {
                    match (c.available_width, constraints.available_width_type) {
                        (Avs::Definite(old), Avs::Definite(new)) => {
                            if (old - new).abs() < 1.0 {
                                "ifc_reflow_width_dd_small"
                            } else {
                                "ifc_reflow_width_dd_big"
                            }
                        }
                        _ => "ifc_reflow_width_type",
                    }
                };
                drop(crate::probe::Probe::span(reason));
                None
            }
            Some(c) if c.inline_content_hash != current_content_hash => {
                drop(crate::probe::Probe::span("ifc_reflow_content"));
                None
            }
            Some(c) => Some(c),
        };

        if let Some(cached) = cached_ifc {
            if cached.line_breaks.is_none() {
                drop(crate::probe::Probe::span("ifc_reflow_no_linebreaks"));
            }
            if let Some(ref line_breaks) = cached.line_breaks {
                // Collect per-item advance widths from cached metrics
                let old_advances: Vec<f32> = cached
                    .item_metrics
                    .iter()
                    .map(|m| m.advance_width)
                    .collect();

                // Cache-reuse fast path. Real incremental relayout for text
                // edits lives in LayoutWindow::try_incremental_text_relayout
                // (window.rs) — it has the newly-shaped items and the edited
                // node id, so it can compute real dirty_item_indices and
                // take the GlyphSwap / LineShift branches. Here we only
                // know the IFC is being re-entered (e.g. viewport resize on
                // a static IFC); with nothing re-shaped yet, the best we can
                // do is "no items changed at this level" → trivial GlyphSwap
                // to return the cached layout unchanged.
                let result = text3::cache::try_incremental_relayout(
                    &[], // empty = no dirty items detected at this level
                    &old_advances,
                    &old_advances, // same advances since we haven't reshaped yet
                    line_breaks,
                );

                if matches!(result, text3::cache::IncrementalRelayoutResult::GlyphSwap) {
                    // No items changed — return cached layout directly
                    debug_info!(
                        ctx,
                        "[layout_ifc] Phase 2d: GlyphSwap — reusing cached layout"
                    );
                    // (d6h) Materialized: the stored layout may be the
                    // retirement sentinel; measuring it raw zeroed the
                    // reuse path's overflow_size (scrollbars vanished on
                    // every GlyphSwap reuse).
                    let main_frag = cached.materialized();
                    let mut output = LayoutOutput {
                        overflow_size: ifc_extent(&main_frag),
                        baseline: main_frag.last_baseline(),
                        ..Default::default()
                    };
                    // The cache-reuse exit must trim like the full path: a
                    // measure pass that lands here would otherwise size the
                    // box untrimmed while the final pass trims (see
                    // apply_text_box_trim).
                    apply_text_box_trim(
                        ctx.styled_dom,
                        ifc_root_dom_id,
                        &text3_constraints,
                        !main_frag.items.is_empty(),
                        &mut output,
                    );
                    // Re-position inline-block children from cached layout.
                    //
                    // This is the discipline a memoized call with a side effect
                    // needs: the exit REPLAYS what the full path would have
                    // written. Note it replays POSITIONS only — the children's
                    // sizes are assumed to be in the tree already. A carried
                    // `inline_layout_result` (a clone's, and a matched
                    // anonymous block's since `try_reuse_anon_wrapper` carries
                    // layout state) can reach this exit in a new tree, and the
                    // assumption still holds there: the collection above is
                    // only reused while `atomic_inline_children_are_laid_out`
                    // says the atomic children it places have sizes (carried
                    // by their own clones), and a re-collection lays them out.
                    for positioned_item in &main_frag.items {
                        if let ShapedItem::Object { source, .. } = &positioned_item.item {
                            if let Some(&child_node_index) = child_map.get(source) {
                                output.positions.insert(
                                    child_node_index,
                                    LogicalPosition {
                                        x: positioned_item.position.x,
                                        y: positioned_item.position.y,
                                    },
                                );
                            }
                        }
                    }
                    return Ok(output);
                }
                // Fall through to full layout_flow
                drop(crate::probe::Probe::span("ifc_reflow_incr_declined"));
            }
        }
    }

    // Phase 2: text3_constraints was resolved early (above) so the cache-reuse key
    // could include container-level properties.
    // Clone constraints for caching (before they're moved into fragments)
    let cached_constraints = text3_constraints.clone();

    debug_info!(
        ctx,
        "[layout_ifc] CALLING text_cache.layout_flow for node {} with {} exclusions",
        node_index,
        text3_constraints.shape_exclusions.len()
    );

    let fragments = vec![LayoutFragment {
        id: "main".to_string(),
        constraints: text3_constraints,
    }];

    // Phase 3: Invoke the text layout engine.
    // Get pre-loaded fonts from font manager (fonts should be loaded before layout)
    let loaded_fonts = ctx.font_manager.get_loaded_fonts();
    let text_layout_result = match text_cache.layout_flow(
        &inline_content,
        &[],
        &fragments,
        &ctx.font_manager.font_chain_cache,
        &ctx.font_manager.fc_cache,
        &loaded_fonts,
        ctx.debug_messages,
    ) {
        Ok(result) => {
            // [g133 az-web-lift DIAG] layout_flow returned Ok.
            #[cfg(feature = "web_lift")]
            unsafe {
                crate::az_mark((0x60688) as u32, (0xC0DE0688u32) as u32);
            }
            result
        }
        Err(e) => {
            // [g133 az-web-lift DIAG] layout_flow returned Err → zero-sized (text not positioned).
            #[cfg(feature = "web_lift")]
            unsafe {
                crate::az_mark((0x60688) as u32, (0x000000EEu32) as u32);
                // Read the error's first byte (discriminant) for the marker — a
                // `*const u8` read is always aligned + in-bounds; the old
                // `*const u32` read was UB on a 1-aligned / <4-byte enum.
                crate::az_mark((0x6068C) as u32, (*(&e as *const _ as *const u8)) as u32);
            }
            // Font errors should not stop layout of other elements.
            // Log the error and return a zero-sized layout.
            debug_warning!(ctx, "Text layout failed: {:?}", e);
            debug_warning!(
                ctx,
                "Continuing with zero-sized layout for node {}",
                node_index
            );

            return Ok(LayoutOutput {
                overflow_size: LogicalSize::new(0.0, 0.0),
                ..Default::default()
            });
        }
    };
    // Phase 4: Integrate results back into the solver3 layout tree.
    let mut output = LayoutOutput::default();

    debug_ifc_layout!(
        ctx,
        "text_layout_result has {} fragment_layouts",
        text_layout_result.fragment_layouts.len()
    );

    if let Some(main_frag) = text_layout_result.fragment_layouts.get("main") {
        let frag_bounds = main_frag.bounds();
        debug_ifc_layout!(
            ctx,
            "Found 'main' fragment with {} items, bounds={}x{}",
            main_frag.items.len(),
            frag_bounds.width,
            frag_bounds.height
        );
        debug_ifc_layout!(ctx, "Storing inline_layout_result on node {}", node_index);

        // Determine if we should store this layout result using the new
        // CachedInlineLayout system. The key insight is that inline layouts
        // depend on available width:
        //
        // - Min-content measurement uses width ≈ 0 (maximum line wrapping)
        // - Max-content measurement uses width = ∞ (no line wrapping)
        // - Final layout uses the actual column/container width
        //
        // We must track which constraint type was used, otherwise a min-content
        // measurement would incorrectly be reused for final rendering.
        let has_floats = constraints
            .bfc_state
            .as_ref()
            .is_some_and(|s| !s.floats.floats.is_empty());
        let current_width_type = constraints.available_width_type;

        // A layout that placed NOTHING for non-empty text content is a
        // font-race artifact, not a layout: the first pass can run before
        // `load_missing_for_chains` has parsed this run's font, shaping
        // yields zero items, and text3's own caches self-heal on the next
        // call — but THIS per-node store is keyed by width+content only, so
        // an empty result would be served to every later pass ("reuse")
        // and the paragraph would measure 0.0 forever. (miniword: the
        // sample document reported 1 page; WHICH node got poisoned flipped
        // on a single leading whitespace character re-ordering the first
        // font-less pass.) Skip the store; the next pass recomputes with
        // fonts present.
        let content_has_text = inline_content
            .iter()
            .any(|c| matches!(c, InlineContent::Text(r) if !r.text.trim().is_empty()));
        if content_has_text && main_frag.items.is_empty() {
            debug_info!(
                ctx,
                "[layout_ifc] NOT caching empty layout for node {} (fonts not loaded yet?)",
                node_index
            );
            output.overflow_size = LogicalSize::zero();
            return Ok(output);
        }

        let warm_node = tree
            .warm_mut(LayoutNodeId::new(node_index))
            .ok_or(LayoutError::InvalidTree)?;

        let should_store = match &warm_node.inline_layout_result {
            None => {
                // No cached result - always store
                debug_info!(
                    ctx,
                    "[layout_ifc] Storing NEW inline_layout_result for node {} (width_type={:?}, \
                     has_floats={})",
                    node_index,
                    current_width_type,
                    has_floats
                );
                true
            }
            Some(cached) => {
                // Check if the new result should replace the cached one
                if cached.should_replace_with(current_width_type, has_floats)
                    || cached.inline_content_hash != current_content_hash
                {
                    // #11 fix: the cached layout is what the display-list
                    // generator paints from; replace it when the inline content
                    // changed, even if the width constraint is unchanged.
                    debug_info!(
                        ctx,
                        "[layout_ifc] REPLACING inline_layout_result for node {} (old: \
                         width={:?}, floats={}) with (new: width={:?}, floats={})",
                        node_index,
                        cached.available_width,
                        cached.has_floats,
                        current_width_type,
                        has_floats
                    );
                    true
                } else {
                    debug_info!(
                        ctx,
                        "[layout_ifc] KEEPING cached inline_layout_result for node {} (cached: \
                         width={:?}, floats={}, new: width={:?}, floats={})",
                        node_index,
                        cached.available_width,
                        cached.has_floats,
                        current_width_type,
                        has_floats
                    );
                    false
                }
            }
        };

        if should_store {
            let mut cil = CachedInlineLayout::new_with_constraints(
                main_frag.clone(),
                current_width_type,
                has_floats,
                cached_constraints.clone(),
            );
            // #11 fix: record the content hash so Phase 2d only fast-path-reuses
            // this layout when the inline content is genuinely unchanged, and so
            // the store decision above can detect content changes.
            cil.inline_content_hash = current_content_hash;
            warm_node.inline_layout_result = Some(Box::new(cil));
            // DL-patching invalidation: this IFC's line layout was
            // recomputed — its text items must re-emit on a patched pass.
            ctx.reflowed_ifcs.insert(node_index);
        }

        // Extract the overall size and baseline for the IFC root.
        // +spec:display-property:a0d0ab - IFC height = top of topmost line box to bottom of
        // bottommost line box +spec:display-property:a63b8f - baseline-source defaults to
        // auto (last baseline for inline-block/IFC)
        //
        // The ONE measure of an IFC's extent (`ifc_extent`): the cache-reuse
        // exit above reports the same, so two layouts of unchanged content
        // agree to the bit.
        output.overflow_size = ifc_extent(main_frag);
        output.baseline = main_frag.last_baseline();
        warm_node.baseline = output.baseline;

        apply_text_box_trim(
            ctx.styled_dom,
            ifc_root_dom_id,
            &cached_constraints,
            !main_frag.items.is_empty(),
            &mut output,
        );

        // Position all the inline-block children based on text3's calculations.
        // [CoordinateSpace::Parent] - positions are relative to IFC's content-box (0,0)
        for positioned_item in &main_frag.items {
            if let ShapedItem::Object {
                source, content, ..
            } = &positioned_item.item
            {
                if let Some(&child_node_index) = child_map.get(source) {
                    // new_relative_pos is [CoordinateSpace::Parent] - relative to this IFC's
                    // content-box
                    let new_relative_pos = LogicalPosition {
                        x: positioned_item.position.x,
                        y: positioned_item.position.y,
                    };
                    output.positions.insert(child_node_index, new_relative_pos);
                }
            }
        }
    }

    // [g132 az-web-lift VERIFY] Capture the IFC content geometry (the line-box bounds from
    // main_frag.bounds(), set above as output.overflow_size). height>0 proves the text LAID OUT
    // (not just shaped). Free-band addrs, f32 bits. REVERT at cleanup.
    #[cfg(feature = "web_lift")]
    unsafe {
        crate::az_mark(
            (0x60670) as u32,
            (output.overflow_size.width.to_bits()) as u32,
        );
        crate::az_mark(
            (0x60674) as u32,
            (output.overflow_size.height.to_bits()) as u32,
        );
        crate::az_mark((0x60678) as u32, (output.positions.len() as u32) as u32);
        crate::az_mark((0x6067C) as u32, (0xC0DE0132u32) as u32);
    }

    Ok(output)
}

/// Folds the RESOLVED style of DOM node `i` - every compact tier the inline
/// collection reads - into the inline-collection key of `layout_ifc`: the
/// tier-1 enums, the tier-2 dimensions (font size, and the box sizes and
/// edges of inline boxes and atomic inlines, whose measured sizes the
/// collection caches) and the whole tier-2b text block (font family,
/// colour, line height, letter / word spacing, indent).
///
/// The node fingerprints cover a node's own data and inline CSS, not what an
/// author STYLESHEET resolved onto it: the same DOM under another stylesheet
/// (or an inherited value moved by an ancestor's restyle) compared equal on
/// the tier-1 enums and the font family alone, and reused runs of the old
/// font size and atomic inlines of the old size (FIX9 1.2, TEXT7 found (c)).
/// The compact cache stores computed values (inherited ones included), so a
/// change anywhere above reaches every node of the subtree here.
pub(super) fn hash_resolved_style<H: core::hash::Hasher>(
    cc: &azul_css::compact_cache::CompactLayoutCache,
    i: usize,
    h: &mut H,
) {
    use core::hash::Hash;
    if let Some(t1) = cc.tier1_enums.get(i) {
        t1.hash(h);
    }
    if let Some(d) = cc.tier2_dims.get(i) {
        (
            d.width,
            d.height,
            d.min_width,
            d.max_width,
            d.min_height,
            d.max_height,
            d.flex_basis,
            d.font_size,
        )
            .hash(h);
        (
            d.padding_top,
            d.padding_right,
            d.padding_bottom,
            d.padding_left,
            d.margin_top,
            d.margin_right,
            d.margin_bottom,
            d.margin_left,
        )
            .hash(h);
        (
            d.border_top_width,
            d.border_right_width,
            d.border_bottom_width,
            d.border_left_width,
            d.top,
            d.right,
            d.bottom,
            d.left,
        )
            .hash(h);
        (d.flex_grow, d.flex_shrink, d.row_gap, d.column_gap).hash(h);
    }
    if let Some(t) = cc.tier2b_text.get(i) {
        (
            t.text_color,
            t.font_family_hash,
            t.line_height,
            t.letter_spacing,
            t.word_spacing,
            t.text_indent,
        )
            .hash(h);
    }
}

// ============================================================================
// INITIAL LETTER / DROP CAPS STUB
// ============================================================================

/// Computes the geometric exclusion area for an initial letter (drop cap).
///
/// CSS Inline Layout Module Level 3, section 3:
/// The `initial-letter` property specifies styling for dropped, raised, and sunken
/// initial letters. When set, the first glyph(s) of the first line are enlarged to
/// span multiple lines, with the remaining text wrapping around them.
// +spec:box-model:c93797 - initial-letter alignment points determined from contents (not
// border-box)
///
/// # Algorithm
///
/// 1. The letter box height spans `size` lines: `height = size * line_height`.
/// 2. The letter box width is estimated using a typical capital letter aspect ratio
///    (cap-height-to-advance-width ~0.7 for Latin text). A proper implementation would measure the
///    actual glyph, but this gives a reasonable default.
/// 3. The letter is positioned at the inline-start of the first line.
/// 4. The `sink` value determines how many lines the letter drops below the first baseline. When
///    `sink == size`, this is a classic drop cap. When `sink < size`, the letter rises above the
///    first line (raised cap).
/// 5. A small gap (4px default) is added between the letter box and adjacent text.
///
/// # Parameters
/// - `initial_letter_size`: The number of lines the initial letter should span (e.g., 3.0)
/// - `initial_letter_sink`: How many lines the letter sinks below the first line
/// - `content_box_width`: Available width in the content box (for clamping)
/// - `line_height`: The computed line height for the containing block
///
/// # Returns
/// A tuple of `(letter_width, letter_height)` representing the space reserved for
/// the initial letter exclusion, or `(0.0, 0.0)` if the parameters are invalid.
///
/// The caller should use these dimensions to create a float-like exclusion at the
/// start of the block container, causing subsequent lines to wrap around the letter.
// +spec:width-calculation:7f4f68 - initial-letter-wrap exclusion area (none behavior; first/grid
// require glyph outlines)
#[allow(clippy::cast_precision_loss)] // bounded graphics/coord/font/fixed-point/debug-marker cast
#[must_use]
pub fn layout_initial_letter(
    initial_letter_size: f32,
    initial_letter_sink: u32,
    content_box_width: f32,
    line_height: f32,
) -> (f32, f32) {
    // Estimate the letter width using a typical Latin capital letter aspect ratio.
    // The advance width of a capital letter is approximately 0.7x the cap height.
    // This is a heuristic; a full implementation would measure the actual glyph(s).
    const CAP_WIDTH_RATIO: f32 = 0.7;

    // Add a small gap between the letter box and the adjacent inline content.
    // CSS Inline Level 3 section 3.5: browsers typically add ~4px padding.
    const LETTER_GAP: f32 = 4.0;

    // Guard against degenerate values
    if initial_letter_size <= 0.0 || line_height <= 0.0 || content_box_width <= 0.0 {
        return (0.0, 0.0);
    }

    // +spec:overflow:dd0679 - auto-sized initial letter content box fits exactly to content;
    // alignment props do not apply +spec:width-calculation:170742 - atomic initial letters with
    // auto block size use inline initial letter sizing CSS Inline Level 3 section 3.3: The
    // initial letter box height spans `size` lines.
    let letter_height = initial_letter_size * line_height;

    let letter_width_raw = letter_height * CAP_WIDTH_RATIO;

    let letter_width = (letter_width_raw + LETTER_GAP).min(content_box_width);

    // +spec:containing-block:67fd99 - block-axis positioning: size >= sink shifts by
    // (sink-1)*line_height toward block-end The actual exclusion height accounts for the sink
    // value. sink == size means the letter is fully dropped (classic drop cap).
    // sink < size means part of the letter rises above the first line (raised cap).
    // The exclusion area height is always `sink * line_height` since that's how
    // many lines of subsequent text need to wrap around the letter.
    let exclusion_height = (initial_letter_sink as f32) * line_height;

    // Use the larger of exclusion_height and letter_height as the actual
    // vertical space consumed. For raised caps (sink < size), the letter
    // extends above the first line but the exclusion only covers sink lines.
    // For sunken caps (sink >= size), the exclusion covers the full letter height.
    let effective_height = exclusion_height.max(letter_height);

    (letter_width, effective_height)
}
