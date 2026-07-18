//! Intrinsic and used size calculations for layout nodes

use crate::debug_log;
use std::{
    collections::BTreeSet,
    sync::Arc,
};

use azul_core::{
    dom::{FormattingContext, NodeId, NodeType},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::{StyledDom, StyledNodeState},
};
use azul_css::{
    css::CssPropertyValue,
    props::{
        basic::PixelValue,
        layout::{LayoutDisplay, LayoutFlexDirection, LayoutFlexWrap, LayoutFloat, LayoutHeight, LayoutPosition, LayoutWidth, LayoutWritingMode},
        property::{CssProperty, CssPropertyType},
    },
    LayoutDebugMessage,
};
use rust_fontconfig::FcFontCache;

#[cfg(feature = "text_layout")]
use crate::text3;
use crate::{
    font::parsed::ParsedFont,
    font_traits::{
        AvailableSpace, FontLoaderTrait, FontManager, ImageSource, InlineContent, InlineImage,
        InlineShape, LayoutCache, LayoutFragment, ObjectFit, ParsedFontTrait, ShapeDefinition,
        StyleProperties, UnifiedConstraints,
    },
    solver3::{
        fc::split_text_for_whitespace,
        geometry::{BoxProps, IntrinsicSizes, WritingModeContext},
        getters::{
            get_css_box_sizing, get_css_height, get_css_width, get_display_property,
            get_direction_property, get_element_font_size, get_flex_direction, get_float,
            get_style_properties, get_text_orientation_property, get_writing_mode, MultiValue,
        },
        layout_tree::{LayoutNodeHot, LayoutTree, get_display_type},
        positioning::get_position_type,
        LayoutContext, LayoutError, Result,
    },
};

const FALLBACK_MIN_CONTENT_WIDTH: f32 = 100.0;
const FALLBACK_MAX_CONTENT_WIDTH: f32 = 300.0;
const FALLBACK_MIN_CONTENT_HEIGHT: f32 = 20.0;
const FALLBACK_MAX_CONTENT_HEIGHT: f32 = 20.0;

/// Resolves a min/max sizing `PixelValue`, falling back to percentage-against-
/// containing-block resolution (with box-model adjustment) when the value is a
/// percentage rather than an absolute length.
///
/// `is_horizontal` selects which axis of `box_props` (left/right vs top/bottom)
/// is subtracted during percentage resolution.
fn resolve_px_with_box_model(
    px: &PixelValue,
    containing: f32,
    box_props: &BoxProps,
    is_horizontal: bool,
    em: f32,
    rem: f32,
) -> Option<f32> {
    if let Some(v) = super::calc::resolve_pixel_value_no_percent(px, em, rem) {
        return Some(v);
    }

    let percent = px.to_percent()?;
    let (margin, border, padding) = if is_horizontal {
        (
            (box_props.margin.left, box_props.margin.right),
            (box_props.border.left, box_props.border.right),
            (box_props.padding.left, box_props.padding.right),
        )
    } else {
        (
            (box_props.margin.top, box_props.margin.bottom),
            (box_props.border.top, box_props.border.bottom),
            (box_props.padding.top, box_props.padding.bottom),
        )
    };
    Some(resolve_percentage_with_box_model(
        containing,
        percent.get(),
        margin,
        border,
        padding,
    ))
}

/// Resolves a percentage value against the containing block dimension.
///
/// Per CSS 2.1 Section 10.2, percentages resolve directly against the containing
/// block's width or height. The margin/border/padding parameters are accepted for
/// call-site convenience but are intentionally unused — percentage resolution does
/// not subtract box-model extras in content-box sizing.
///
/// Returns `(containing_block_dimension * percentage).max(0.0)`.
// +spec:containing-block:43c719 - percentages resolved against containing block width/height
// +spec:containing-block:723eee - Percentages specify sizing with respect to the containing block
// +spec:containing-block:8ad6f4 - Percentage resolution against containing block (editorial note: transferred percentages)
// +spec:containing-block:257f3b - Block-axis percentages resolve against containing block size
// +spec:containing-block:f1344e - percentage min/max-width resolved against containing block width; negative CB width yields zero
#[must_use] pub fn resolve_percentage_with_box_model(
    containing_block_dimension: f32,
    percentage: f32,
    _margins: (f32, f32),
    _borders: (f32, f32),
    _paddings: (f32, f32),
) -> f32 {
    // +spec:containing-block:b3388b - percentage resolved against containing block size without re-resolution (css-sizing-3 §5.2.1)
    // CSS 2.1 Section 10.2: percentages resolve against containing block,
    // not available space after margins/borders/padding
    (containing_block_dimension * percentage).max(0.0)
}

/// Returns true if the DOM subtree rooted at `dom_id` contains any `NodeType::Text`.
///
/// Used when deciding whether a `FormattingContext::Inline` node should measure
/// its inline content (it acts as an IFC root when nested inlines eventually
/// hold text) versus returning zero (pure inline wrapper with no text reaches).
fn subtree_contains_text(styled_dom: &StyledDom, dom_id: NodeId) -> bool {
    let node_hierarchy = styled_dom.node_hierarchy.as_container();
    let node_data = styled_dom.node_data.as_container();
    if matches!(node_data[dom_id].get_node_type(), NodeType::Text(_)) {
        return true;
    }
    dom_id
        .az_children(&node_hierarchy)
        .any(|child| subtree_contains_text(styled_dom, child))
}

/// Phase 2a: Calculate intrinsic sizes (bottom-up pass)
/// // +spec:display-contents:f12d4e - intrinsic sizing: size determined by contents, not context
// [g71 TEST] #[inline(never)] — RELIABLE bisection (g70, markers in free band) showed new_tree drops
// 2→0 RIGHT BEFORE this call. It's currently INLINED into layout_document (absent from the lift log),
// so its frame/entry isn't a separate SP-wrapped @sub_ call. Forcing it OUT makes the call a wrapped
// @sub_ → enforce_sp_preservation save/restores SP around it. If new_tree survives (sizingEntry=2),
// the inlined entry/frame-setup was mis-lifting SP. (g60's inline(always) was a no-op — already inlined.)
#[inline(never)]
#[allow(clippy::cast_possible_truncation)] // bounded graphics/coord/font/fixed-point/debug-marker cast
/// # Errors
///
/// Returns a `LayoutError` if intrinsic sizing fails.
pub fn calculate_intrinsic_sizes<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    tree: &mut LayoutTree,
    text_cache: &mut LayoutCache,
    dirty_nodes: &BTreeSet<usize>,
) -> Result<()> {
    // [az-diag g59 REVERT] RELIABLE field-access bracket (pointer CASTS mis-lift to 0 — g58 proved
    // it; use tree.nodes.len() which is reliable). 0x407B0 = nodes.len at ENTRY. If 2 here but
    // 0x40734 (line ~142, after compute_dirty_ancestor_closure + calculator) reads 0, the corruption
    // is in 121-142. compute_dirty_ancestor_closure RETURNS a HashSet by sret — prime suspect:
    // sret-slot overlapping new_tree, or the hashbrown empty-map bug. 0x407B4 (post-compute_dirty)
    // isolates compute_dirty vs calculator-creation.
    unsafe { crate::az_mark(0x607B0_u32, (tree.nodes.len() as u32)); }
    if dirty_nodes.is_empty() {
        return Ok(());
    }

    debug_log!(ctx, "Starting intrinsic size calculation");
    // Pre-compute the "ancestor closure" of dirty_nodes: every dirty
    // node AND each of its ancestors up to root. A node not in this
    // set (and whose `intrinsic_sizes` is already populated) can
    // reuse its cached intrinsic — we skip its entire subtree walk.
    // Before this, `calculate_intrinsic_recursive` walked the full
    // tree from root regardless, costing ~2 ms per warm render on
    // excel.html even when only 3 nodes were actually dirty.
    let dirty_closure = compute_dirty_ancestor_closure(tree, dirty_nodes);
    // [az-diag g59 REVERT] 0x407B4 = nodes.len AFTER compute_dirty_ancestor_closure (its HashSet sret).
    unsafe { crate::az_mark(0x607B4_u32, (tree.nodes.len() as u32)); }

    let mut calculator = IntrinsicSizeCalculator::new(ctx, text_cache);
    calculator.dirty_closure = Some(dirty_closure);
    // Fix C (re-enabled §58 Win #3): skip intrinsic computation for subtrees
    // whose values will never be consumed. `tree.subtree_needs_intrinsic` is a
    // static-DOM bitmap precomputed at tree-build time — true if this node or
    // any descendant establishes a shrink-to-fit context. When both the
    // caller and the subtree are non-STF, no one reads the intrinsic, so the
    // whole descent is pure waste.
    //
    // The previous attempt (7667d13e, reverted in bd9ad36d) wrote default
    // (zero) intrinsics and broke auto-height rendering because
    // calculate_used_size_for_node read intrinsic.max_content_height as the
    // height:auto fallback. 97c3d3db refactored that dependency away: for
    // block-level auto-height, used_size.height is 0 pre-layout and
    // apply_content_based_height fills it from the laid-out content size.
    // With that gone, skipping intrinsic is safe.
    // [az-diag g53 REVERT] DECISIVE: is the lifted LayoutTree itself empty/broken? If
    // tree.get(root)=None (0x40738=0) or nodes.len()=0 (0x40734), the InvalidTree@229 is
    // because RECONCILE produced a broken tree — root cause is reconcile, not sizing.
    unsafe {
        crate::az_mark(0x60730_u32, (tree.root as u32));
        crate::az_mark(0x60734_u32, (tree.nodes.len() as u32));
        crate::az_mark(0x60738_u32, u32::from(tree.get(tree.root).is_some()));
        // [az-diag g55] 0x4075C = the `tree` ptr the CALLEE sees. Compare with 0x40748
        // (caller's &new_tree). Same → nodes-field-offset mis-lift; differ → &mut arg mis-passed.
        crate::az_mark(0x6075C_u32, ((std::ptr::from_ref::<LayoutTree>(tree) as usize) as u32));
    }
    calculator.calculate_intrinsic_recursive(tree, tree.root, false)?;
    debug_log!(ctx, "Finished intrinsic size calculation");
    Ok(())
}

fn compute_dirty_ancestor_closure(
    tree: &LayoutTree,
    dirty_nodes: &BTreeSet<usize>,
) -> std::collections::HashSet<usize> {
    let mut closure: std::collections::HashSet<usize> = std::collections::HashSet::new();
    for &dirty in dirty_nodes {
        let mut cur = Some(dirty);
        while let Some(idx) = cur {
            if !closure.insert(idx) {
                break;
            }
            cur = tree.get(idx).and_then(|n| n.parent);
        }
    }
    closure
}

struct IntrinsicSizeCalculator<'a, 'b, 'c, T: ParsedFontTrait> {
    ctx: &'a mut LayoutContext<'b, T>,
    /// Shared text shaping cache, threaded through from the caller so
    /// stages 1–3 of the inline layout pipeline (logical / `BiDi` / shaping)
    /// are cache-hits across the sizing pass's min/max-content measurements
    /// AND the subsequent real layout pass. Previously each pass held its
    /// own `LayoutCache`, so identical text was shaped three times per
    /// `root_layout_pass` — once per min-content measurement, once per
    /// max-content measurement, once at final layout.
    text_cache: &'c mut LayoutCache,
    /// If `Some`, only nodes in this set (the ancestor-closure of
    /// dirty nodes) need recomputation. A clean node whose
    /// `warm.intrinsic_sizes` is already populated reuses the
    /// cached value and skips its entire subtree descent.
    dirty_closure: Option<std::collections::HashSet<usize>>,
}

impl<'a, 'b, 'c, T: ParsedFontTrait> IntrinsicSizeCalculator<'a, 'b, 'c, T> {
    const fn new(ctx: &'a mut LayoutContext<'b, T>, text_cache: &'c mut LayoutCache) -> Self {
        Self {
            ctx,
            text_cache,
            dirty_closure: None,
        }
    }

    #[allow(clippy::cast_possible_truncation)] // bounded graphics/coord/font/fixed-point/debug-marker cast
    fn calculate_intrinsic_recursive(
        &mut self,
        tree: &mut LayoutTree,
        node_index: usize,
        ancestor_is_stf: bool,
    ) -> Result<IntrinsicSizes> {
        // [az-diag g52 REVERT] 0x40720 = node_index entering calculate_intrinsic_recursive
        // (last value after the run = the node that InvalidTree'd or the stray child).
        unsafe { crate::az_mark(0x60720_u32, (node_index as u32)); }
        // Fast path: if this subtree has no dirty nodes AND we
        // already have a cached intrinsic, return the cached value
        // and skip the whole descent. Caller is the ancestor-closure
        // computation in `calculate_intrinsic_sizes` — anything not
        // in that set is guaranteed clean through every descendant.
        if let Some(closure) = self.dirty_closure.as_ref() {
            if !closure.contains(&node_index) {
                if let Some(cached) = tree
                    .warm(node_index)
                    .and_then(|w| w.intrinsic_sizes)
                {
                    return Ok(cached);
                }
            }
        }

        // Fix C static-DOM short-circuit: if no ancestor needs this intrinsic
        // (none are STF) AND no descendant in this subtree is STF, nobody
        // will ever read the value. Write a default and skip the recursion.
        // `subtree_needs_intrinsic` is precomputed at tree-build time from
        // the DOM's display/position/float properties, so this is a constant
        // lookup with no per-pass work.
        if !ancestor_is_stf
            && tree
                .subtree_needs_intrinsic
                .get(node_index)
                .copied()
                .is_some_and(|v| !v)
        {
            let default = IntrinsicSizes::default();
            if let Some(n) = tree.warm_mut(node_index) {
                n.intrinsic_sizes = Some(default);
            }
            return Ok(default);
        }

        // Previously cloned the full LayoutNode to sidestep borrow conflicts
        // with the `&mut tree` recursive calls below, but we only need the
        // DOM id here — a `Copy` scalar. The clone was allocating a
        // Vec<usize> for children and a TaffyCache on every recursion
        // (~300x on excel.html).
        let dom_node_id = tree
            .get(node_index)
            .ok_or(LayoutError::InvalidTree)?
            .dom_node_id;

        // Out-of-flow elements do not contribute to their parent's intrinsic size.
        let position = get_position_type(self.ctx.styled_dom, dom_node_id);
        if position == LayoutPosition::Absolute || position == LayoutPosition::Fixed {
            if let Some(n) = tree.warm_mut(node_index) {
                n.intrinsic_sizes = Some(IntrinsicSizes::default());
            }
            return Ok(IntrinsicSizes::default());
        }

        // Copy child indices before recursive calls (which need &mut tree).
        // Stack buffer for the common case (≤32 children); heap only for huge nodes.
        let children_slice = tree.children(node_index);
        let n = children_slice.len();
        let mut stack_buf = [0usize; 32];
        let heap_buf: Vec<usize>;
        let children: &[usize] = if n <= 32 {
            stack_buf[..n].copy_from_slice(children_slice);
            &stack_buf[..n]
        } else {
            heap_buf = children_slice.to_vec();
            &heap_buf
        };
        // Propagate STF flag: children inherit `ancestor_is_stf=true` if any
        // ancestor up to and including self is STF.
        let self_is_stf = tree
            .get(node_index)
            .is_some_and(|n| {
                crate::solver3::layout_tree::is_shrink_to_fit_context(
                    self.ctx.styled_dom,
                    n.dom_node_id,
                    n.formatting_context,
                )
            });
        let child_ancestor_is_stf = ancestor_is_stf || self_is_stf;

        let mut child_intrinsics = Vec::with_capacity(n);
        for &child_index in children {
            // [az-diag g52 REVERT] 0x40728 = child_index about to recurse (last = the stray).
            unsafe { crate::az_mark(0x60728_u32, (child_index as u32)); }
            // [g52 FIX] Defensive: reconcile can mis-list a stray/out-of-range child_index
            // (a Text node mis-listed as a layout child, or a lift artifact in the children
            // array). The unguarded recursion would hit `tree.get(child_index).ok_or(InvalidTree)`
            // at line ~226 and abort the WHOLE intrinsic-sizing pass. Skip gracefully so
            // measurement continues — mirrors process_layout_children's guard (line ~1079).
            // REAL fix = reconcile not listing the stray child.
            if tree.get(child_index).is_none() {
                continue;
            }
            let child_intrinsic =
                self.calculate_intrinsic_recursive(tree, child_index, child_ancestor_is_stf)?;
            child_intrinsics.push((child_index, child_intrinsic));
        }

        // Then calculate this node's intrinsic size based on its children
        let mut intrinsic = self.calculate_node_intrinsic_sizes(tree, node_index, &child_intrinsics)?;

        // +spec:min-max-sizing:970fef - if min-width/min-height is a <length>, use as floor for intrinsic sizes
        if let Some(dom_id) = tree.get(node_index).and_then(|n| n.dom_node_id) {
            use crate::solver3::getters::{get_css_min_width, get_css_min_height, MultiValue};

            let node_state = &self.ctx.styled_dom.styled_nodes.as_container()[dom_id].styled_node_state;

            // Resolve em against the element's OWN font-size and rem against the root.
            let em = get_element_font_size(self.ctx.styled_dom, dom_id, node_state);
            let rem = super::getters::get_root_font_size(self.ctx.styled_dom, node_state);

            if let MultiValue::Exact(mw) = get_css_min_width(self.ctx.styled_dom, dom_id, node_state) {
                if let Some(min_w) = super::calc::resolve_pixel_value_no_percent(&mw.inner, em, rem) {
                    intrinsic.min_content_width = intrinsic.min_content_width.max(min_w);
                    intrinsic.max_content_width = intrinsic.max_content_width.max(min_w);
                }
            }

            if let MultiValue::Exact(mh) = get_css_min_height(self.ctx.styled_dom, dom_id, node_state) {
                if let Some(min_h) = super::calc::resolve_pixel_value_no_percent(&mh.inner, em, rem) {
                    intrinsic.min_content_height = intrinsic.min_content_height.max(min_h);
                    intrinsic.max_content_height = intrinsic.max_content_height.max(min_h);
                }
            }
        }

        if let Some(n) = tree.warm_mut(node_index) {
            n.intrinsic_sizes = Some(intrinsic);
        }

        Ok(intrinsic)
    }

    #[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
    fn calculate_node_intrinsic_sizes(
        &mut self,
        tree: &LayoutTree,
        node_index: usize,
        child_intrinsics: &[(usize, IntrinsicSizes)],
    ) -> Result<IntrinsicSizes> {
        let node = tree.get(node_index).ok_or(LayoutError::InvalidTree)?;

        // +spec:block-formatting-context:30def2 - replaced elements use physical 300x150 default, not re-oriented by writing-mode
        // +spec:display-property:015c41 - replaced elements default to 300x150 intrinsic size per css-sizing-3 §5.1
        // +spec:display-property:2c6af3 - replaced elements with auto width/height use max-content size
        // +spec:replaced-elements:6d6030 - Intrinsic sizes for replaced elements (images, virtual views)
        // VirtualViews are replaced elements with a default intrinsic size of 300x150px
        // (same as virtualized view elements)
        if let Some(dom_id) = node.dom_node_id {
            let node_data = &self.ctx.styled_dom.node_data.as_container()[dom_id];
            if node_data.is_virtual_view_node() {
                return Ok(IntrinsicSizes {
                    min_content_width: 300.0,
                    max_content_width: 300.0,
                    preferred_width: None, // Will be determined by CSS or flex-grow
                    min_content_height: 150.0,
                    max_content_height: 150.0,
                    preferred_height: None, // Will be determined by CSS or flex-grow
                });
            }
            
            // +spec:containing-block:bb5a12 - replaced element intrinsic sizes using initial containing block
            // +spec:display-property:7127f9 - intrinsic sizes of replaced elements without natural sizes (300x150 fallback, aspect ratio)
            // +spec:display-property:f9cede - replaced elements derive intrinsic size from natural dimensions
            // +spec:writing-modes:b18121 - stretch fit inline size from available space, calculate block size via aspect ratio
            if let NodeType::Image(image_ref) = node_data.get_node_type() {
                let size = image_ref.get_size();
                // +spec:containing-block:1da6dc - use initial CB inline size for replaced elements with aspect ratio but no intrinsic size
                // Per css-sizing-3 §5.1: "use an inline size matching the corresponding dimension
                // of the initial containing block and calculate the other dimension using the aspect ratio"
                let has_intrinsic = size.width > 0.0 || size.height > 0.0;
                let (width, height) = if size.width > 0.0 && size.height > 0.0 {
                    (size.width, size.height)
                } else if size.width > 0.0 {
                    (size.width, size.width / 2.0)
                } else if size.height > 0.0 {
                    // Has intrinsic height but no width — use initial CB inline dimension
                    (self.ctx.viewport_size.width, size.height)
                } else {
                    // +spec:replaced-elements:43376b - 300px fallback with 2:1 ratio for replaced elements
                    // No intrinsic dimensions — cap at 300x150 per CSS 2.2 §10.3.2
                    // +spec:width-calculation:3b0efe - auto width fallback: 300px capped to device width
                    // +spec:width-calculation:16c305 - auto height fallback: 2:1 ratio, max 150px
                    let w = self.ctx.viewport_size.width.min(300.0);
                    (w, w / 2.0)
                };
                // A replaced element with NO intrinsic size (e.g. a RenderImageCallback
                // <img> like the AzulPaint canvas) must behave like a VirtualView: keep
                // the 300×150 fallback as the min/max-content (so it has a sensible
                // default) but leave `preferred` as None so `flex-grow` / explicit CSS
                // can size it. A `Some(preferred)` here pins the box and defeats
                // flex-grow (the canvas was laid out 300×0 — see the VirtualView arm
                // above, which already uses None for exactly this reason). Images WITH
                // a real intrinsic size keep `preferred = Some` so they display at their
                // natural size when unconstrained.
                let (pref_w, pref_h) = if has_intrinsic {
                    (Some(width), Some(height))
                } else {
                    (None, None)
                };
                return Ok(IntrinsicSizes {
                    min_content_width: width,
                    max_content_width: width,
                    preferred_width: pref_w,
                    min_content_height: height,
                    max_content_height: height,
                    preferred_height: pref_h,
                });
            }
        }

        match node.formatting_context {
            FormattingContext::Block { .. } => {
                // Check if this block establishes an Inline Formatting Context (IFC).
                // Per CSS 2.2 §9.2.1.1: A block container with mixed block-level and
                // inline-level children creates anonymous block boxes to wrap the inline
                // content. So we only treat as IFC root if there are NO block-level children.
                //
                // We check the actual CSS display property, NOT formatting_context,
                // because a display:block element with only inline children gets
                // FormattingContext::Inline (meaning "establishes IFC for its children"),
                // which is different from being an inline element itself.
                let has_block_child = tree.children(node_index).iter().any(|&child_idx| {
                    tree.get(child_idx)
                        .and_then(|c| c.dom_node_id)
                        .is_some_and(|dom_id| {
                            let node_data = &self.ctx.styled_dom.node_data.as_container()[dom_id];
                            // Text nodes are inline-level
                            if matches!(node_data.get_node_type(), NodeType::Text(_)) {
                                return false;
                            }
                            let display = get_display_type(self.ctx.styled_dom, dom_id);
                            display.creates_block_context()
                        })
                });

                let has_inline_child = tree.children(node_index).iter().any(|&child_idx| {
                    tree.get(child_idx)
                        .and_then(|c| c.dom_node_id)
                        .is_some_and(|dom_id| {
                            let node_data = &self.ctx.styled_dom.node_data.as_container()[dom_id];
                            if matches!(node_data.get_node_type(), NodeType::Text(_)) {
                                return true;
                            }
                            let display = get_display_type(self.ctx.styled_dom, dom_id);
                            matches!(display,
                                LayoutDisplay::Inline
                                | LayoutDisplay::InlineBlock
                                | LayoutDisplay::InlineFlex
                                | LayoutDisplay::InlineGrid
                                | LayoutDisplay::InlineTable
                            )
                        })
                });

                // IFC root only if there are inline children and NO block children.
                // If there are block children, text nodes get anonymous block wrappers.
                let is_ifc_root = has_inline_child && !has_block_child;
                
                // Also check if this block has direct text content (text nodes in DOM)
                // but ONLY if there are no block-level layout children
                let has_direct_text = if has_block_child {
                    false
                } else if let Some(dom_id) = node.dom_node_id {
                    let node_hierarchy = &self.ctx.styled_dom.node_hierarchy.as_container();
                    dom_id.az_children(node_hierarchy).any(|child_id| {
                        let child_node_data = &self.ctx.styled_dom.node_data.as_container()[child_id];
                        matches!(child_node_data.get_node_type(), NodeType::Text(_))
                    })
                } else {
                    false
                };
                
                if is_ifc_root || has_direct_text {
                    // This block is an IFC root - measure all inline content ONCE
                    self.calculate_ifc_root_intrinsic_sizes(tree, node_index)
                } else {
                    // This is a BFC root (only block children) - aggregate child sizes
                    self.calculate_block_intrinsic_sizes(tree, node_index, child_intrinsics)
                }
            }
            FormattingContext::Inline => {
                // There are THREE cases for FormattingContext::Inline:
                // 1. A Text node (NodeType::Text) - this IS the text content itself
                //    -> Needs to measure itself as an atomic inline unit
                // 2. An IFC root - a block with only inline children (has text child nodes)
                //    -> Should measure its inline content
                // 3. A true inline element (display: inline, e.g., <span>) with no text
                //    -> Returns default(0,0), measured by parent IFC root
                //
                // We distinguish by:
                // - Checking if THIS node is a Text node (case 1)
                // - Checking if this subtree contains any text (case 2)
                //
                // Why descendants, not just direct children: for `<span><a>text</a></span>`,
                // the `<span>` is a layout-tree IFC root (layout_ifc is called on it), but
                // its direct DOM children are inline elements, not text. Restricting the
                // check to direct text children would zero out the span's intrinsic width
                // even though the cell content width depends on it.
                let is_text_node = if let Some(dom_id) = node.dom_node_id {
                    let node_data = &self.ctx.styled_dom.node_data.as_container()[dom_id];
                    matches!(node_data.get_node_type(), NodeType::Text(_))
                } else {
                    false
                };

                let has_text_in_subtree = if let Some(dom_id) = node.dom_node_id {
                    subtree_contains_text(self.ctx.styled_dom, dom_id)
                } else {
                    false
                };

                if is_text_node || has_text_in_subtree {
                    // Case 1 or 2: Text node or IFC root - measure inline content
                    self.calculate_ifc_root_intrinsic_sizes(tree, node_index)
                } else {
                    // Case 3: True inline element - measured by parent IFC root
                    Ok(IntrinsicSizes::default())
                }
            }
            FormattingContext::InlineBlock => {
                // Inline-block IS an atomic inline - it needs its own intrinsic size.
                // Check layout tree children AND direct DOM text children (text nodes
                // are not in the layout tree, only in the DOM).
                let has_inline_children = tree.children(node_index).iter().any(|&child_idx| {
                    tree.get(child_idx)
                        .is_some_and(|c| matches!(c.formatting_context, FormattingContext::Inline))
                });

                let has_direct_text = if let Some(dom_id) = node.dom_node_id {
                    let node_hierarchy = &self.ctx.styled_dom.node_hierarchy.as_container();
                    dom_id.az_children(node_hierarchy).any(|child_id| {
                        let child_node_data = &self.ctx.styled_dom.node_data.as_container()[child_id];
                        matches!(child_node_data.get_node_type(), NodeType::Text(_))
                    })
                } else {
                    false
                };

                if has_inline_children || has_direct_text {
                    // InlineBlock with inline children - measure as IFC root.
                    // Returns content-level intrinsic sizes (no margin/padding/border).
                    // The parent adds box-model extras via calculate_block_intrinsic_sizes,
                    // and calculate_used_size_for_node adds padding+border for border-box.
                    let intrinsic = self.calculate_ifc_root_intrinsic_sizes(tree, node_index)?;

                    Ok(intrinsic)
                } else {
                    // InlineBlock with block children - aggregate like block
                    self.calculate_block_intrinsic_sizes(tree, node_index, child_intrinsics)
                }
            }
            FormattingContext::Table => {
                Ok(self.calculate_table_intrinsic_sizes(tree, node_index, child_intrinsics))
            }
            FormattingContext::Flex => {
                self.calculate_flex_intrinsic_sizes(tree, node_index, child_intrinsics)
            }
            _ => self.calculate_block_intrinsic_sizes(tree, node_index, child_intrinsics),
        }
    }
    
    // +spec:intrinsic-sizing:ea2c2c - §5.1 min-content size = size as float with auto; max-content = no wrapping
    /// Calculate intrinsic sizes for an IFC root (a block containing inline content).
    /// This collects ALL inline descendants' text and measures it ONCE.
    // +spec:intrinsic-sizing:8f3c0c - hanging glyphs must be excluded from intrinsic size measurement
    #[allow(clippy::cast_possible_truncation)] // bounded graphics/coord/font/fixed-point/debug-marker cast
    fn calculate_ifc_root_intrinsic_sizes(
        &mut self,
        tree: &LayoutTree,
        node_index: usize,
    ) -> Result<IntrinsicSizes> {
        // [g75] 0x60758 = how many times this IFC sizer is entered; 0x6075C = node_index of THIS call.
        unsafe {
            let c = crate::az_mark_read(0x60758).wrapping_add(1);
            crate::az_mark(0x60758_u32, (c));
            crate::az_mark(0x6075C_u32, (node_index as u32));
        }
        // Collect all inline content from this IFC root and its inline descendants
        // [g76] EXPLICIT match (was `?`): the g75 markers showed collect_inline_content reaching its
        // completion marker B8 (Ok at the source level) yet the IFC sizer never advancing to 0xA1 —
        // i.e. the lifted `Result<Vec<InlineContent>, LayoutError>` return arrives as Err at this call
        // site (a complex by-value Result-return mis-lift). 0x60760 = 1 (Ok) / 0xEE (Err-but-B8-ran).
        // RESILIENCE: on Err, degrade to empty content (→ default intrinsic) instead of aborting the
        // WHOLE layout with InvalidTree, so the page renders and the next real blocker surfaces.
        // g76 PROVED: degrading to Vec::new() here (resilience) lets layout proceed past this
        // InvalidTree but then HANGS in the downstream actual-layout shaping (the documented g47
        // hashbrown empty-map infinite loop). So for a CLEAN (non-hanging) state we PROPAGATE the Err
        // (same as the original `?`), keeping the 0x60760 diagnostic. To chase the g47 hang, flip the
        // Err arm back to `Vec::new()`. 0x60760 = 1 (Ok) / 0xEE (Err-despite-B8 = the Result mis-lift).
        // [g78] OUT-PARAM refactor: the by-value `Result<Vec<InlineContent>, LayoutError>` return
        // mis-lifted Ok→Err (g76/g77 PROVED it: 0x60760=0xEE despite the source reaching B8). Filling
        // a `&mut Vec` out-param and returning `Result<()>` (register-returned, NO sret-of-Vec) lifts
        // cleanly — the established M12.7 "a pointer arg lifts cleanly" pattern. 0x60760 should now =1.
        let collect_result = collect_inline_content(self.ctx, tree, node_index);
        #[cfg(feature = "web_lift")]
        unsafe { crate::az_mark((0x60760) as u32, (if collect_result.is_ok() { 0x00000001u32 } else { 0x000000EEu32 }) as u32); }
        let inline_content: Vec<InlineContent> = collect_result?;

        if inline_content.is_empty() {
            return Ok(IntrinsicSizes::default());
        }

        // Get pre-loaded fonts from font manager
        let loaded_fonts = self.ctx.font_manager.get_loaded_fonts();

        // +spec:intrinsic-sizing:ae8beb - min-content = zero-width CB, max-content = infinite-width CB
        // +spec:intrinsic-sizing:8c94e2 - min-content/max-content intrinsic size determination via constrained layout
        // Use `measure_intrinsic_widths` instead of two `layout_flow` passes (fix B):
        // it runs stages 1–4 of the pipeline once (logical → BiDi → shape → orient)
        // and derives min/max-content by scanning the shaped items directly. This
        // avoids the BreakCursor line-breaking loop entirely — that loop clones
        // every ShapedCluster it inspects via `peek_next_unit` and accounted for
        // 24% of total CPU on the text_2000 stress fixture. Shaping is cached
        // at the per-item level (keyed on text+style), so the subsequent real
        // layout_flow call for this content gets pure cache hits for stages 1–3.
        // Populate the measurement constraints from the IFC root's real white-space
        // mode instead of always using defaults. With the default (Normal) the scan
        // treats every space as a break opportunity, so a white-space:nowrap / pre
        // element reports a min-content SMALLER than its true unbreakable width and
        // the flex/shrink-to-fit algorithm clips it.
        let mut constraints = UnifiedConstraints::default();
        if let Some(dom_id) = tree.get(node_index).and_then(|n| n.dom_node_id) {
            use crate::solver3::getters::{get_white_space_property, MultiValue};
            use azul_css::props::style::text::StyleWhiteSpace;
            let node_state =
                &self.ctx.styled_dom.styled_nodes.as_container()[dom_id].styled_node_state;
            let ws = match get_white_space_property(self.ctx.styled_dom, dom_id, node_state) {
                MultiValue::Exact(v) => v,
                _ => StyleWhiteSpace::Normal,
            };
            constraints.white_space_mode = match ws {
                StyleWhiteSpace::Normal => crate::text3::cache::WhiteSpaceMode::Normal,
                StyleWhiteSpace::Nowrap => crate::text3::cache::WhiteSpaceMode::Nowrap,
                StyleWhiteSpace::Pre => crate::text3::cache::WhiteSpaceMode::Pre,
                StyleWhiteSpace::PreWrap => crate::text3::cache::WhiteSpaceMode::PreWrap,
                StyleWhiteSpace::PreLine => crate::text3::cache::WhiteSpaceMode::PreLine,
                StyleWhiteSpace::BreakSpaces => crate::text3::cache::WhiteSpaceMode::BreakSpaces,
            };
        }
        // [g79 DIAG] Probe the font state at shaping time, then convert the downstream shape_text
        // HANG (g47 hashbrown empty-map loop) → trap so the harness RETURNS and these markers are
        // readable (can't read markers from a hung wasm call). Tests the #4→#3 coupling: if
        // 0x60768(font_chain_cache.len) / 0x6076C(loaded_fonts.len) are 0, the EMPTY FONT CHAIN is
        // the ROOT of the hang (no font → allsorts builds empty hashbrown maps → RawIter loops).
        // [g82] CONDITIONAL trap: if the font_chain_cache is still EMPTY, shaping would HANG (allsorts
        // builds empty hashbrown maps → g47 RawIter loop) → trap instead so the markers are readable
        // (non-hang). If the chain is NON-empty (unique_font_keys BTreeMap fix worked + populated it),
        // PROCEED into measure → shape → text should MEASURE. g81 hung (no conditional) → need to know
        // whether the chain populated. 0x60768=chain.len, 0x6076C=loaded_fonts.len, 0x60704=0xA15.
        #[cfg(feature = "web_lift")]
        {
            let cl = self.ctx.font_manager.font_chain_cache.len();
            unsafe {
                crate::az_mark((0x60768) as u32, (cl as u32) as u32);
                crate::az_mark((0x6076C) as u32, (loaded_fonts.len() as u32) as u32);
                crate::az_mark((0x60704) as u32, (0xA15u32) as u32);
            }
            // [g88] g85+g87 PROVED whack-a-mole does NOT converge: BTreeMap'd unique_font_keys (chain
            // ✓), supported_features+lookups_index (g85), ReadCache (g87) — STILL HANGS. Too many
            // hashbrown empty-map sites across allsorts/std/rust-fontconfig. The ONLY convergent fix is
            // the SYSTEMIC transpiler empty-static mirror (force the lifted hashbrown ctrl-scan to read
            // 0xFF not 0x00). TEMP non-hang trap until that lands. ★ REMOVE to test the systemic fix.
            // [g93] PROCEED into shaping to test the AZ_FORCE_MIRROR_VMADDRS hashbrown-EMPTY_GROUP fix.
            // If text MEASURES → the forced const pages contained EMPTY_GROUP → systemic fix found.
            let _ = (cl, loaded_fonts.len());
        }
        let Ok(intrinsic_text) = self.text_cache.measure_intrinsic_widths(
            &inline_content,
            &[],
            &constraints,
            &self.ctx.font_manager.font_chain_cache,
            &self.ctx.font_manager.fc_cache,
            &loaded_fonts,
            self.ctx.debug_messages,
        ) else {
            return Ok(IntrinsicSizes {
                min_content_width: FALLBACK_MIN_CONTENT_WIDTH,
                max_content_width: FALLBACK_MAX_CONTENT_WIDTH,
                preferred_width: None,
                min_content_height: FALLBACK_MIN_CONTENT_HEIGHT,
                max_content_height: FALLBACK_MAX_CONTENT_HEIGHT,
                preferred_height: None,
            });
        };

        let min_width = intrinsic_text.min_content_width;
        let max_width = intrinsic_text.max_content_width;

        // +spec:display-property:c587fd - min-content block size equals max-content block size for block containers, tables, inline boxes
        // +spec:intrinsic-sizing:02eedc - min-content block size equals max-content block size for block containers
        // For a single-line max-content layout the height is one line box;
        // `measure_intrinsic_widths` returns exactly that.
        let max_content_height = intrinsic_text.max_content_height;

        // NOTE(writing-modes): min_content_width / max_content_width are named for
        // the physical axis. In vertical writing modes the "inline" axis is vertical,
        // so these are swapped by calculate_block_intrinsic_sizes when computing
        // the parent's intrinsic sizes. The physical naming is intentional here.
        Ok(IntrinsicSizes {
            min_content_width: min_width,
            max_content_width: max_width,
            preferred_width: None,
            min_content_height: max_content_height,
            max_content_height,
            preferred_height: None,
        })
    }

    // +spec:containing-block:bb0658 - percentage block-sizes behave as auto during intrinsic computation (no CSS height resolution here)
    // +spec:display-contents:84fe7f - cyclic percentage contributions: percentage-sized children use auto during intrinsic sizing
    // +spec:min-max-sizing:411904 - percentage block-sizes treated as auto during intrinsic sizing (content-sized CB)
    // +spec:min-max-sizing:737e62 - percentage heights don't resolve inside content-sized containing blocks
    fn calculate_block_intrinsic_sizes(
        &self,
        tree: &LayoutTree,
        node_index: usize,
        child_intrinsics: &[(usize, IntrinsicSizes)],
    ) -> Result<IntrinsicSizes> {
        let node = tree.get(node_index).ok_or(LayoutError::InvalidTree)?;
        let writing_mode = node.dom_node_id.map_or_else(LayoutWritingMode::default, |dom_id| {
            let node_state =
                &self.ctx.styled_dom.styled_nodes.as_container()[dom_id].styled_node_state;
            get_writing_mode(self.ctx.styled_dom, dom_id, node_state).unwrap_or_default()
        });

        // NOTE: Text content detection is now handled in calculate_node_intrinsic_sizes
        // which calls calculate_ifc_root_intrinsic_sizes for blocks with inline content.
        // This function now only handles pure block containers (BFC roots).
        // +spec:height-calculation:d9ca8d - cyclic percentage contributions: percentage min-height/max-height on children should behave as auto when computing intrinsic contributions (not yet implemented)

        let mut max_child_min_cross = 0.0f32;
        let mut max_child_max_cross = 0.0f32;
        let mut total_main_size = 0.0;
        // Track margins for CSS 2.2 §8.3.1 collapsing in the block direction.
        // Block margins collapse between siblings (max instead of sum) and
        // parent-child margins can escape (first/last child).
        let mut last_margin_main_end = 0.0f32;
        let mut is_first_child = true;

        for &child_index in tree.children(node_index) {
            if let Some(child_intrinsic) = child_intrinsics.iter().find(|(k, _)| k == &child_index).map(|(_, v)| v) {
                // +spec:intrinsic-sizing:ed72bb - intrinsic contributions based on outer size, auto margins as zero
                let child_node = tree.get(child_index);
                let (cross_extras, main_border_padding, main_margin_start, main_margin_end) =
                    child_node.map_or((0.0, 0.0, 0.0, 0.0), |cn| {
                        let bp = cn.box_props.unpack();
                        let h = bp.margin.left + bp.margin.right
                              + bp.border.left + bp.border.right
                              + bp.padding.left + bp.padding.right;
                        let v_bp = bp.border.top + bp.border.bottom
                              + bp.padding.top + bp.padding.bottom;
                        match writing_mode {
                            LayoutWritingMode::HorizontalTb => (h, v_bp, bp.margin.top, bp.margin.bottom),
                            _ => (v_bp, h, bp.margin.left, bp.margin.right),
                        }
                    });

                let (child_min_cross, child_max_cross, child_border_box_main) = match writing_mode {
                    LayoutWritingMode::HorizontalTb => (
                        child_intrinsic.min_content_width + cross_extras,
                        child_intrinsic.max_content_width + cross_extras,
                        child_intrinsic.max_content_height + main_border_padding,
                    ),
                    _ => (
                        child_intrinsic.min_content_height + cross_extras,
                        child_intrinsic.max_content_height + cross_extras,
                        child_intrinsic.max_content_width + main_border_padding,
                    ),
                };

                max_child_min_cross = max_child_min_cross.max(child_min_cross);
                max_child_max_cross = max_child_max_cross.max(child_max_cross);

                // CSS 2.2 §8.3.1 margin collapsing for intrinsic sizing:
                // - First child's margin-start can escape (don't add to total)
                // - Between siblings: collapsed gap = max(prev_end, curr_start)
                // - Last child's margin-end can escape (don't add to total)
                if is_first_child {
                    is_first_child = false;
                    // First child: top margin may escape, don't add it
                } else {
                    // Sibling gap: collapsed margin between prev bottom and current top
                    let collapsed_gap = crate::solver3::fc::collapse_margins(
                        last_margin_main_end, main_margin_start
                    );
                    total_main_size += collapsed_gap;
                }

                total_main_size += child_border_box_main;
                last_margin_main_end = main_margin_end;
            }
        }
        // Last child's margin-end may escape — don't add it to total_main_size

        let (min_width, max_width, min_height, max_height) = match writing_mode {
            LayoutWritingMode::HorizontalTb => (
                max_child_min_cross,
                max_child_max_cross,
                total_main_size,
                total_main_size,
            ),
            _ => (
                total_main_size,
                total_main_size,
                max_child_min_cross,
                max_child_max_cross,
            ),
        };

        Ok(IntrinsicSizes {
            min_content_width: min_width,
            max_content_width: max_width,
            preferred_width: None,
            min_content_height: min_height,
            max_content_height: max_height,
            preferred_height: None,
        })
    }

    // The max-content main size is the sum of items' max-content contributions.
    // The min-content main size of a single-line flex container is the sum of items'
    // min-content contributions. For multi-line, it is the largest min-content contribution.
    // Auto margins on flex items are treated as 0 for this computation.
    fn calculate_flex_intrinsic_sizes(
        &self,
        tree: &LayoutTree,
        node_index: usize,
        child_intrinsics: &[(usize, IntrinsicSizes)],
    ) -> Result<IntrinsicSizes> {
        let node = tree.get(node_index).ok_or(LayoutError::InvalidTree)?;

        // Determine flex-direction to know if main axis is horizontal or vertical
        let is_row = node.dom_node_id.is_none_or(|dom_id| {
            let node_state =
                &self.ctx.styled_dom.styled_nodes.as_container()[dom_id].styled_node_state;
            match get_flex_direction(self.ctx.styled_dom, dom_id, node_state) {
                MultiValue::Exact(dir) => matches!(dir, LayoutFlexDirection::Row | LayoutFlexDirection::RowReverse),
                _ => true, // default is row
            }
        });

        let mut sum_main_min: f32 = 0.0;
        let mut sum_main_max: f32 = 0.0;
        let mut max_main_min: f32 = 0.0;
        let mut max_cross_min: f32 = 0.0;
        let mut max_cross_max: f32 = 0.0;

        for &child_index in tree.children(node_index) {
            if let Some(child_intrinsic) = child_intrinsics.iter().find(|(k, _)| k == &child_index).map(|(_, v)| v) {
                let (child_main_min, child_main_max, child_cross_min, child_cross_max) = if is_row {
                    (
                        child_intrinsic.min_content_width,
                        child_intrinsic.max_content_width,
                        child_intrinsic.min_content_height,
                        child_intrinsic.max_content_height,
                    )
                } else {
                    (
                        child_intrinsic.min_content_height,
                        child_intrinsic.max_content_height,
                        child_intrinsic.min_content_width,
                        child_intrinsic.max_content_width,
                    )
                };

                sum_main_max += child_main_max;
                sum_main_min += child_main_min;
                // For multi-line min-content, track the largest single item
                max_main_min = max_main_min.max(child_main_min);

                // Cross axis: largest child determines the container's cross size
                max_cross_min = max_cross_min.max(child_cross_min);
                max_cross_max = max_cross_max.max(child_cross_max);
            }
        }

        // For single-line (nowrap), min-content = sum; for multi-line (wrap), min-content = max
        // Default flex-wrap is nowrap (single-line)
        let is_single_line = node.dom_node_id.is_none_or(|dom_id| {
            let node_state =
                &self.ctx.styled_dom.styled_nodes.as_container()[dom_id].styled_node_state;
            let wrap_prop = crate::solver3::getters::get_flex_wrap_prop(
                self.ctx.styled_dom, dom_id, node_state,
            );
            wrap_prop.is_none_or(|val| matches!(
                    val.get_property_or_default().unwrap_or_default(),
                    LayoutFlexWrap::NoWrap
                ))
        });

        let min_main = if is_single_line { sum_main_min } else { max_main_min };
        let max_main = sum_main_max;

        if is_row {
            Ok(IntrinsicSizes {
                min_content_width: min_main,
                max_content_width: max_main,
                preferred_width: None,
                min_content_height: max_cross_min,
                max_content_height: max_cross_max,
                preferred_height: None,
            })
        } else {
            Ok(IntrinsicSizes {
                min_content_width: max_cross_min,
                max_content_width: max_cross_max,
                preferred_width: None,
                min_content_height: min_main,
                max_content_height: max_main,
                preferred_height: None,
            })
        }
    }

    /// Calculate intrinsic sizes for a table element by aggregating cell content
    /// widths per column and row heights.
    /// +spec:table-layout:93b13c - shrink-to-fit for tables uses intrinsic sizing
    fn calculate_table_intrinsic_sizes(
        &mut self,
        tree: &LayoutTree,
        node_index: usize,
        child_intrinsics: &[(usize, IntrinsicSizes)],
    ) -> IntrinsicSizes {
        // Collect per-column min/max widths and total row heights.
        // Table structure: table > row-group? > row > cell
        let mut col_min: Vec<f32> = Vec::new();
        let mut col_max: Vec<f32> = Vec::new();
        let mut total_height = 0.0f32;

        // Iterate rows — children may be row groups (thead/tbody/tfoot) or direct rows
        let mut rows: Vec<usize> = Vec::new();
        for &child_idx in tree.children(node_index) {
            let Some(child) = tree.get(child_idx) else { continue };
            match child.formatting_context {
                FormattingContext::TableRow => rows.push(child_idx),
                FormattingContext::TableRowGroup => {
                    // Row group contains rows
                    for &row_idx in tree.children(child_idx) {
                        if let Some(row) = tree.get(row_idx) {
                            if matches!(row.formatting_context, FormattingContext::TableRow) {
                                rows.push(row_idx);
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        for &row_idx in &rows {
            let mut row_height = 0.0f32;
            for (col, &cell_idx) in tree.children(row_idx).iter().enumerate() {
                let cell_intrinsic = child_intrinsics.iter().find(|(k, _)| k == &cell_idx).map(|(_, v)| *v)
                    .unwrap_or_default();
                // Also check if cell has IFC content we can measure
                let cell_is = if cell_intrinsic.max_content_width > 0.0 {
                    cell_intrinsic
                } else {
                    // Try to measure cell content via IFC
                    self.calculate_ifc_root_intrinsic_sizes(tree, cell_idx)
                        .unwrap_or_default()
                };

                // Add cell box-model extras
                let cell_node = tree.get(cell_idx);
                let (h_extras, v_extras) = cell_node.map_or((0.0, 0.0), |cn| {
                    let bp = cn.box_props.unpack();
                    (bp.padding.left + bp.padding.right + bp.border.left + bp.border.right,
                     bp.padding.top + bp.padding.bottom + bp.border.top + bp.border.bottom)
                });

                let cell_min = cell_is.min_content_width + h_extras;
                let cell_max = cell_is.max_content_width + h_extras;
                let cell_h = cell_is.max_content_height + v_extras;

                if col >= col_min.len() {
                    col_min.push(cell_min);
                    col_max.push(cell_max);
                } else {
                    col_min[col] = col_min[col].max(cell_min);
                    col_max[col] = col_max[col].max(cell_max);
                }
                row_height = row_height.max(cell_h);
            }
            total_height += row_height;
        }

        let min_width: f32 = col_min.iter().sum();
        let max_width: f32 = col_max.iter().sum();

        IntrinsicSizes {
            min_content_width: min_width,
            max_content_width: max_width,
            min_content_height: total_height,
            max_content_height: total_height,
            preferred_width: None,
            preferred_height: None,
        }
    }
}

/// Gathers all inline content for the intrinsic sizing pass.
///
/// This function recursively collects text and inline-level content according to
/// CSS Sizing Level 3, Section 4.1: "Intrinsic Sizes"
/// <https://www.w3.org/TR/css-sizing-3/#intrinsic-sizes>
///
/// For inline formatting contexts, we need to gather:
/// 1. Text nodes (inline content)
/// 2. Inline-level boxes (display: inline, inline-block, etc.)
/// 3. Atomic inline-level elements (replaced elements like images)
///
/// The key difference from `collect_and_measure_inline_content` in fc.rs is that
/// this version is used for intrinsic sizing (calculating min/max-content widths)
/// before the actual layout pass, so it must recursively gather content from
/// inline descendants without laying them out first.
fn collect_inline_content_for_sizing<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    tree: &LayoutTree,
    ifc_root_index: usize,
    out: &mut Vec<InlineContent>,
) -> Result<()> {
    debug_log!(ctx, "Collecting inline content from node {} for intrinsic sizing", ifc_root_index);

    // [g78] fill the caller's out-param (was a local Vec returned by value → Ok→Err mis-lift).
    // Recursively collect inline content from this node and its inline descendants
    collect_inline_content_recursive(ctx, tree, ifc_root_index, out)?;
    // [g73] B8 = top-level recursion returned Ok (collect_inline_content complete).
    unsafe { crate::az_mark(0x6071C_u32, (0xB8u32)); }
    debug_log!(ctx, "Collected {} inline content items from node {}", out.len(), ifc_root_index);

    Ok(())
}

/// Recursive helper for collecting inline content.
///
/// According to CSS Sizing Level 3, the intrinsic size of an inline formatting context
/// is based on all inline-level content, including text in nested inline elements.
///
/// This function:
/// - Collects text from the current node if it's a text node
/// - Collects text from DOM children (text nodes may not be in layout tree)
/// - Recursively collects from inline children (display: inline)
/// - Treats non-inline children as atomic inline-level boxes
#[allow(clippy::cast_possible_truncation)] // bounded graphics/coord/font/fixed-point/debug-marker cast
fn collect_inline_content_recursive<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    tree: &LayoutTree,
    node_index: usize,
    content: &mut Vec<InlineContent>,
) -> Result<()> {
    // [g75] capture node_index of EVERY recursion entry (0x60754) and mark the entry-tree.get
    // FAILURE distinctly (inline-phase=0xBAD) so a node_index that fails HERE (before B1) is
    // visible even though a PRIOR successful call already wrote B8. This is the suspected
    // InvalidTree site (phase stuck at 0xA0 + B8 reached ⇒ a 2nd IFC call fails at this get).
    unsafe { crate::az_mark(0x60754_u32, (node_index as u32)); }
    let Some(node) = tree.get(node_index) else {
        unsafe { crate::az_mark(0x6071C_u32, (0xBADu32)); }
        return Err(LayoutError::InvalidTree);
    };

    // CRITICAL FIX: Text nodes may exist in the DOM but not as separate layout nodes!
    // We need to check the DOM children for text content.
    let Some(dom_id) = node.dom_node_id else {
        // No DOM ID means this is a synthetic node, skip text extraction
        return process_layout_children(ctx, tree, node_index, content);
    };

    // First check if THIS node is a text node
    if let Some(text) = extract_text_from_node(ctx.styled_dom, dom_id) {
        let style_props = Arc::new(get_style_properties(ctx.styled_dom, dom_id, ctx.system_style.as_ref(), azul_css::props::basic::PhysicalSize::new(ctx.viewport_size.width, ctx.viewport_size.height)));
        debug_log!(ctx, "Found text in node {}: '{}'", node_index, text);
        // Use split_text_for_whitespace to correctly handle white-space: pre with \n
        let text_items = split_text_for_whitespace(
            ctx.styled_dom,
            dom_id,
            &text,
            &style_props,
        );
        content.extend(text_items);
    }

    // CRITICAL: Also check DOM children for text nodes!
    // Text nodes are often not represented as separate layout nodes.
    // However, we must SKIP children that already have a layout tree entry,
    // because those will be handled by process_layout_children() below.
    // Without this guard, text nodes present in both DOM and layout tree
    // get collected twice, causing inline-block containers to be ~2x too wide.
    let node_hierarchy = &ctx.styled_dom.node_hierarchy.as_container();
    for child_id in dom_id.az_children(node_hierarchy) {
        // Skip DOM children that have layout tree nodes - they will be
        // processed via process_layout_children -> collect_inline_content_recursive
        if tree.dom_to_layout.contains_key(&child_id) {
            continue;
        }
        // Check if this DOM child is a text node
        let child_dom_node = &ctx.styled_dom.node_data.as_container()[child_id];
        if let NodeType::Text(text_data) = child_dom_node.get_node_type() {
            let text = text_data.as_str().to_string();
            let style_props = Arc::new(get_style_properties(ctx.styled_dom, child_id, ctx.system_style.as_ref(), azul_css::props::basic::PhysicalSize::new(ctx.viewport_size.width, ctx.viewport_size.height)));
            debug_log!(ctx, "Found text in DOM child of node {}: '{}'", node_index, text);
            // Use split_text_for_whitespace to correctly handle white-space: pre with \n
            let text_items = split_text_for_whitespace(
                ctx.styled_dom,
                child_id,
                &text,
                &style_props,
            );
            content.extend(text_items);
        }
    }
    // [g73] B6 = DOM-children loop done (about to process_layout_children).
    unsafe { crate::az_mark(0x6071C_u32, (0xB6u32)); }

    process_layout_children(ctx, tree, node_index, content)
}

/// Helper to process layout tree children for inline content collection
#[allow(clippy::cast_possible_truncation)] // bounded graphics/coord/font/fixed-point/debug-marker cast
#[allow(clippy::match_same_arms)] // enum/value mapping/dispatch table: one arm per input variant (or cross-type bindings that can't merge)
fn process_layout_children<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    tree: &LayoutTree,
    node_index: usize,
    content: &mut Vec<InlineContent>,
) -> Result<()> {
    use azul_css::props::layout::{LayoutHeight, LayoutWidth};

    // [g73] PLC entry: 0x60708 = 0xC0<<24 | node_index (which node's children we process).
    unsafe { crate::az_mark(0x60708_u32, (0xC000_0000_u32 | (node_index as u32 & 0x00FF_FFFF))); }
    // Process layout tree children (these are elements with layout properties)
    for &child_index in tree.children(node_index) {
        // [g73] PLC loop: 0x6070C = current child_index being processed.
        unsafe { crate::az_mark(0x6070C_u32, (child_index as u32)); }
        // 2026-06-02: was `.ok_or(LayoutError::InvalidTree)?` — a stray/invalid child_index in
        // tree.children (likely a Text node mis-listed during reconcile, since Text is INLINE
        // content not a layout-tree node) aborted the WHOLE intrinsic-sizing pass with
        // InvalidTree BEFORE the inline text got measured → label height 0. Skip gracefully so
        // measurement continues (the inline text is collected separately above, at the
        // collect_inline_content_recursive DOM-children loop). REAL fix = reconcile not listing it.
        let Some(child_node) = tree.get(child_index) else { continue; };
        let Some(child_dom_id) = child_node.dom_node_id else {
            continue;
        };

        let display = get_display_property(ctx.styled_dom, Some(child_dom_id));

        // CSS Sizing Level 3: Inline-level boxes participate in the IFC
        if display.unwrap_or_default() == LayoutDisplay::Inline {
            // Recursively collect content from inline children
            // This is CRITICAL for proper intrinsic width calculation!
            debug_log!(ctx, "Recursing into inline child at node {}", child_index);
            collect_inline_content_recursive(ctx, tree, child_index, content)?;
        } else {
            // Non-inline children are treated as atomic inline-level boxes
            // (e.g., inline-block, images, floats)
            // Their intrinsic size must have been calculated in the bottom-up pass
            let intrinsic_sizes = tree.warm(child_index).and_then(|w| w.intrinsic_sizes).unwrap_or_default();

            // CSS 2.2 § 10.3.9: For inline-block elements with explicit CSS width/height,
            // use the CSS-defined values instead of intrinsic sizes.
            let node_state =
                &ctx.styled_dom.styled_nodes.as_container()[child_dom_id].styled_node_state;
            let css_width = get_css_width(ctx.styled_dom, child_dom_id, node_state);
            let css_height = get_css_height(ctx.styled_dom, child_dom_id, node_state);

            // Resolve CSS width - use explicit value if set, otherwise fall back to intrinsic
            let used_width = match css_width {
                MultiValue::Exact(LayoutWidth::Px(px)) => {
                    // +spec:containing-block:495930 - percentages in intrinsic sizing fall back to intrinsic contribution (css-sizing-3 §5.2.1)
                    // +spec:containing-block:5246c0 - cyclic percentage: when containing block size depends on this box's intrinsic contribution, percentages fall back to intrinsic size
                    // +spec:containing-block:598124 - cyclic percentage contributions use intrinsic size
                    // +spec:height-calculation:ca9f19 - percentage-sized boxes use intrinsic size as contribution during intrinsic sizing
                    // +spec:width-calculation:7a384a - percentage-sized boxes behave as width:auto for intrinsic contributions (cyclic percentage)
                    // Resolve em/rem against the element's OWN font-size and the root
                    // font-size, NOT a hard-coded 16px — otherwise `width: 5em` on a
                    // font-size:24px inline-block sizes to 80px instead of 120px.
                    let em = get_element_font_size(ctx.styled_dom, child_dom_id, node_state);
                    let rem = super::getters::get_root_font_size(ctx.styled_dom, node_state);
                    super::calc::resolve_pixel_value_no_percent(&px, em, rem)
                        .unwrap_or(intrinsic_sizes.max_content_width)
                }
                MultiValue::Exact(LayoutWidth::MinContent) => intrinsic_sizes.min_content_width,
                MultiValue::Exact(LayoutWidth::MaxContent) => intrinsic_sizes.max_content_width,
                MultiValue::Exact(LayoutWidth::FitContent(_)) => {
                    // During intrinsic sizing, fit-content resolves to max-content
                    intrinsic_sizes.max_content_width
                }
                // For Auto or other values, use intrinsic size
                _ => intrinsic_sizes.max_content_width,
            };

            // +spec:containing-block:5145c5 - percentage block-size ignored in content-sized containing blocks during intrinsic sizing
            // Resolve CSS height - use explicit value if set, otherwise fall back to intrinsic
            let used_height = match css_height {
                MultiValue::Exact(LayoutHeight::Px(px)) => {
                    // +spec:containing-block:7d5e79 - percentages behave as auto when containing block height is auto (cyclic percentage contribution)
                    // +spec:height-calculation:7d807b - css-sizing-3 §5.2.1: percentage heights behave as auto during intrinsic sizing (cyclic percentage contribution)
                    // Resolve em/rem against the element's own + root font-size (see width above).
                    let em = get_element_font_size(ctx.styled_dom, child_dom_id, node_state);
                    let rem = super::getters::get_root_font_size(ctx.styled_dom, node_state);
                    super::calc::resolve_pixel_value_no_percent(&px, em, rem)
                        .unwrap_or(intrinsic_sizes.max_content_height)
                }
                // is equivalent to automatic size
                MultiValue::Exact(LayoutHeight::MinContent) => intrinsic_sizes.max_content_height,
                // is equivalent to automatic size
                MultiValue::Exact(LayoutHeight::MaxContent) => intrinsic_sizes.max_content_height,
                MultiValue::Exact(LayoutHeight::FitContent(_)) => intrinsic_sizes.max_content_height,
                _ => intrinsic_sizes.max_content_height,
            };

            debug_log!(ctx, "Found atomic inline child at node {}: display={:?}, intrinsic_width={}, used_width={}, css_width={:?}",
                child_index, display, intrinsic_sizes.max_content_width, used_width, css_width);

            // Represent as a rectangular shape with the resolved dimensions
            content.push(InlineContent::Shape(InlineShape {
                shape_def: ShapeDefinition::Rectangle {
                    size: crate::text3::cache::Size {
                        width: used_width,
                        height: used_height,
                    },
                    corner_radius: None,
                },
                fill: None,
                stroke: None,
                baseline_offset: used_height,
                alignment: crate::solver3::getters::get_vertical_align_for_node(ctx.styled_dom, child_dom_id),
                source_node_id: Some(child_dom_id),
            }));
        }
    }

    Ok(())
}

// Keep old name as an alias for backward compatibility
/// # Errors
///
/// Returns a `LayoutError` if collecting inline content fails.
pub fn collect_inline_content<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    tree: &LayoutTree,
    ifc_root_index: usize,
) -> Result<Vec<InlineContent>> {
    let mut out = Vec::new();
    collect_inline_content_for_sizing(ctx, tree, ifc_root_index, &mut out)?;
    Ok(out)
}

// +spec:height-calculation:1c899b - width and height properties specify the preferred size of the box
/// Calculates the used size of a single node based on its CSS properties and
/// the available space provided by its containing block.
///
/// // +spec:display-contents:71ccde - extrinsic sizing: size determined by context (containing block), not contents
///
/// This implementation correctly handles writing modes and percentage-based sizes
/// according to the CSS specification:
/// 1. `width` and `height` CSS properties are resolved to pixel values. Percentages are calculated
///    based on the containing block's PHYSICAL dimensions (`width` for `width`, `height` for
///    `height`), regardless of writing mode.
/// 2. The resolved physical `width` is then mapped to the node's logical CROSS size.
/// 3. The resolved physical `height` is then mapped to the node's logical MAIN size.
/// 4. A final `LogicalSize` is constructed from these logical dimensions.
// +spec:overflow:3c4f25 - auto box sizes: four auto-determined size types resolved here
// +spec:width-calculation:fb0629 - width/margin used values depend on box type, auto replaced by suitable value
///    M12.7: out-of-line auto-width-block inline size — `(cb.width - margins - borders -
/// padding).max(0.0)`. Extracted from `calc_used_size`'s auto-width Block arm so the
///    `.max(0.0)` runs in a small fn (proven to lift correctly), with a FRESH pointer
///    deref (the huge `calc_used_size` body hoists/spills cb.width and the remill lift then
///    reads it back 0). Returns by f32 (D0/V0 — the standard scalar return), NOT an out-ptr:
///    the out-ptr version computed 800 correctly but the caller's reload was opt-forwarded
///    to the init 0.0 across the opaque call (the helper's `*out` lowers to a direct
///    linear-mem store not modeled as aliasing the caller's slot). The f32 return is the
///    call's SSA result, which opt cannot replace. (The earlier "f32-return mis-lift" worry
///    was the 2×f32 *struct* HFA — a single scalar f32 return is fine.)
#[inline(never)]
#[allow(clippy::trivially_copy_pass_by_ref)] // <=8B Copy param kept by-ref intentionally (hot pixel/coord path or to avoid churning call sites for a perf-neutral change)
fn auto_block_inline_size(cb: &LogicalSize, bp: &BoxProps) -> f32 {
    let aw = cb.width
        - bp.margin.left
        - bp.margin.right
        - bp.border.left
        - bp.border.right
        - bp.padding.left
        - bp.padding.right;
    aw.max(0.0)
}

#[allow(clippy::match_same_arms)] // enum/value mapping/dispatch table: one arm per input variant (or cross-type bindings that can't merge)
#[allow(clippy::too_many_lines, clippy::cognitive_complexity)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
/// # Errors
///
/// Returns a `LayoutError` if computing the used size fails.
pub fn calculate_used_size_for_node(
    styled_dom: &StyledDom,
    dom_id: Option<NodeId>,
    // M12.7: by-reference (GP-register pointer). A by-value LogicalSize is an HFA
    // (2×f32) the remill lift stages as an 8-byte double into a V register, and that
    // f64/d-register copyload mis-tracks to 0 in the wasm lift (single-f32 reads work,
    // the 64-bit one doesn't) — so cb + viewport arrived 0 and every width came out 0.
    // A pointer arg lifts cleanly; the body reads only .width/.height (auto-deref).
    containing_block_size: &LogicalSize,
    intrinsic: IntrinsicSizes,
    box_props: &BoxProps,
    viewport_size: &LogicalSize,
) -> Result<LogicalSize> {
    let Some(id) = dom_id else {
        // Anonymous boxes:
        // CSS 2.2 § 9.2.1.1: Anonymous boxes inherit from their enclosing box.
        // The inline dimension fills the containing block's inline size,
        // and the block dimension is auto (content-based).
        // In horizontal-tb: inline=width, block=height.
        // In vertical modes: inline=height, block=width.
        //
        // Since anonymous boxes don't have a DOM node, we default to horizontal-tb.
        // The parent's writing mode is already reflected in containing_block_size.
        return Ok(LogicalSize::new(
            containing_block_size.width,
            if intrinsic.max_content_height > 0.0 {
                intrinsic.max_content_height
            } else {
                // Auto height - will be resolved from content
                0.0
            },
        ));
    };

    let node_state = &styled_dom.styled_nodes.as_container()[id].styled_node_state;
    let css_width = get_css_width(styled_dom, id, node_state);
    let css_height = get_css_height(styled_dom, id, node_state);
    let writing_mode = get_writing_mode(styled_dom, id, node_state);
    let display = get_display_property(styled_dom, Some(id));
    let position = get_position_type(styled_dom, dom_id);

    // Construct the full WritingModeContext from resolved styles.
    // This determines how logical dimensions (inline/block) map to physical (width/height).
    let wm_ctx = WritingModeContext::new(
        writing_mode.unwrap_or_default(),
        get_direction_property(styled_dom, id, node_state).unwrap_or_default(),
        get_text_orientation_property(styled_dom, id, node_state).unwrap_or_default(),
    );
    let is_vertical = !wm_ctx.is_horizontal();

    // +spec:display-property:06e0b1 - form controls (non-image) treated as non-replaced
    // Determine if this element is a replaced element (images, virtual views)
    let node_data = &styled_dom.node_data.as_container()[id];
    let is_replaced = matches!(node_data.get_node_type(), NodeType::Image(_))
        || node_data.is_virtual_view_node();

    // +spec:width-calculation:79cdf8 - inline non-replaced: width property does not apply
    // +spec:width-calculation:972e86 - §10.3.1: width property does not apply to inline non-replaced elements
    // For inline non-replaced elements, override any explicit width to Auto.
    let css_width = if display.unwrap_or_default() == LayoutDisplay::Inline
        && !is_replaced
    {
        MultiValue::Exact(LayoutWidth::Auto)
    } else {
        css_width
    };

    // +spec:box-model:1197a5 - height does not apply to non-replaced inline elements
    // +spec:display-property:9cb33d - height does not apply to inline boxes
    // +spec:height-calculation:c03717 - height does not apply to inline non-replaced elements
    // CSS 2.2 §10.6.1 / CSS Inline 3 §6.4: height property does not apply to
    // inline, non-replaced elements. Override any explicit height to Auto.
    let css_height = if display.unwrap_or_default() == LayoutDisplay::Inline
        && !is_replaced
    {
        MultiValue::Exact(LayoutHeight::Auto)
    } else {
        css_height
    };

    // Remember if width/height were auto before consuming them
    let width_is_auto = css_width.is_auto() || matches!(&css_width, MultiValue::Exact(LayoutWidth::Auto));
    let height_is_auto = css_height.is_auto() || matches!(&css_height, MultiValue::Exact(LayoutHeight::Auto));

    // +spec:intrinsic-sizing:9e1c9d - non-quantitative values (auto, min-content, max-content) are not influenced by box-sizing
    let width_is_quantitative = matches!(
        &css_width,
        MultiValue::Exact(LayoutWidth::Px(_) | LayoutWidth::FitContent(_) | LayoutWidth::Calc(_))
    );
    let height_is_quantitative = matches!(
        &css_height,
        MultiValue::Exact(LayoutHeight::Px(_) | LayoutHeight::FitContent(_) | LayoutHeight::Calc(_))
    );

    // +spec:width-calculation:50d67a - automatic sizing concepts (width/height auto resolution)
    // +spec:width-calculation:564315 - §10.3 width calculation dispatch for all box types
    // Step 1: Resolve the CSS `width` property into a concrete pixel value.
    // CSS `width` always refers to the physical horizontal dimension, regardless of writing mode.
    // Percentage values resolve against the containing block's physical width.
    // In horizontal-tb: width = inline size. In vertical modes: width = block size.
    // The physical-to-logical mapping happens in Step 5 below.
    // Percentage values for `width` are resolved against the containing block's width.
    // +spec:width-calculation:febf0c - width/height "behaves as auto" when computed auto or percentage resolves against indefinite
    let resolved_width = match css_width.unwrap_or_default() {
        LayoutWidth::Auto => {
            // +spec:width-calculation:ed6a34 - auto width on replaced element uses intrinsic width
            // CSS 2.2 §10.3.2: If 'width' has a computed value of 'auto', and the element
            // has an intrinsic width, then that intrinsic width is the used value of 'width'.
            // +spec:replaced-elements:992ea5 - block-level replaced elements use inline replaced width rules
            // §10.3.4: "The used value of 'width' is determined as for inline replaced elements."
            // +spec:replaced-elements:36de3e - §10.3.2/§10.3.4: auto width for inline/block replaced elements uses intrinsic width
            // +spec:replaced-elements:b9a780 - §10.3.2: inline replaced auto width = intrinsic width (conditions resolved during intrinsic size calc)
            if is_replaced {
                // +spec:width-calculation:b41dbe - floating/inline replaced: auto width = intrinsic width
                // +spec:width-calculation:c62d35 - §10.3.2: auto width for replaced elements uses intrinsic width
                // +spec:width-calculation:d87ca4 - abs-replaced: auto width+height uses intrinsic width
                // For replaced elements (inline or block-level), auto width = intrinsic width.
                // The intrinsic sizes were already computed with the 300px fallback per §10.3.2.
                intrinsic.max_content_width
            }
            // +spec:intrinsic-sizing:560697 - shrink-to-fit = clamp(min-content, stretch-fit, max-content)
            else if get_float(styled_dom, id, node_state).unwrap_or(LayoutFloat::None) != LayoutFloat::None {
                // +spec:width-calculation:8d7047 - shrink-to-fit width per CSS2.1§10.3.5
                // +spec:width-calculation:0bb038 - shrink-to-fit for floating non-replaced elements (§10.3.5)
                // shrink-to-fit = min(max(preferred minimum width, available width), preferred width)
                // +spec:table-layout:93b13c - shrink-to-fit for floats, inline-blocks, table-cells;
                // orthogonal flows would require child block size as input (not yet implemented)
                // +spec:width-calculation:a6fd29 - shrink-to-fit width for floats: min(max(preferred minimum, available), preferred)
                // CSS 2.2 §10.3.5: For floats, auto width = shrink-to-fit
                let available_width = (containing_block_size.width
                    - box_props.margin.left
                    - box_props.margin.right
                    - box_props.border.left
                    - box_props.border.right
                    - box_props.padding.left
                    - box_props.padding.right)
                    .max(0.0);
                let preferred_minimum = intrinsic.min_content_width;
                let preferred = intrinsic.max_content_width;
                preferred_minimum.max(available_width).min(preferred).max(0.0)
            }
            else if matches!(position, LayoutPosition::Absolute | LayoutPosition::Fixed) {
                // +spec:intrinsic-sizing:12a531 - abspos auto size = fit-content (shrink-to-fit)
                // +spec:width-calculation:0bb038 - shrink-to-fit width for abs-pos non-replaced elements
                // §10.3.7: abs-pos elements with auto width use shrink-to-fit
                // +spec:intrinsic-sizing:087b57 - abspos automatic size is fit-content (shrink-to-fit)
                // +spec:width-calculation:1661b4 - abs-pos non-replaced auto width uses shrink-to-fit (§10.3.7)
                // shrink-to-fit = min(max(preferred_minimum, available), preferred)
                let available_width = (containing_block_size.width
                    - box_props.margin.left
                    - box_props.margin.right
                    - box_props.border.left
                    - box_props.border.right
                    - box_props.padding.left
                    - box_props.padding.right)
                    .max(0.0);
                let preferred_minimum = intrinsic.min_content_width;
                let preferred = intrinsic.max_content_width;
                preferred_minimum.max(available_width).min(preferred).max(0.0)
            } else {
            // +spec:width-calculation:472065 - orthogonal flow auto inline size: if this block
            // container establishes an orthogonal flow (child writing mode axis differs from
            // parent), its auto inline size should use the parent's block-axis size as available
            // space, falling back to the initial containing block size. Currently not implemented;
            // auto width always resolves against the containing block's width.
            // 'auto' width resolution depends on the display type.
            match display.unwrap_or_default() {
                LayoutDisplay::Block
                | LayoutDisplay::FlowRoot
                | LayoutDisplay::ListItem
                | LayoutDisplay::Flex
                | LayoutDisplay::Grid => {
                    // +spec:box-model:503ea3 - margin + border + padding + width = containing block width
                    // +spec:box-model:5ed651 - stretch fit: size minus margins (auto=0), border, padding, floored at 0
                    // +spec:box-model:33b951 - stretch-fit inline size: available space minus margins/border/padding, floored at zero
                    // +spec:box-model:30b4d0 - stretch fit: available size minus margins (auto as zero), border, padding, floored at zero
                    // +spec:width-calculation:e2c8f6 - auto width for non-replaced blocks in normal flow per CSS2.1§10.3.3
                    // For block-level non-replaced elements,
                    // 'auto' width fills the containing block (minus margins, borders, padding).
                    // CSS 2.2 §10.3.3: width = containing_block_width - margin_left -
                    // margin_right - border_left - border_right - padding_left - padding_right
                    // +spec:width-calculation:aef2da - auto width: other auto values become 0, width follows from constraint equality
                    // M12.7: compute in a small #[inline(never)] helper with by-ref/out-ptr
                    // args. calc_used_size is a ~6KB fn (38 maxnum, heavy SROA); the remill
                    // lift spills + diverges the available_width copyload feeding `.max`
                    // (a marker read sees 800, the maxnum's copyload reads 0 → width 0). A
                    // small fn has clean register allocation; out-ptr avoids the f32-return
                    // mis-lift. cb/bp are already &-refs (GP-pointer args lift cleanly).
                    // M12.7: compute the auto-width in a small f32-RETURNING helper.
                    // Inline-in-calc reads cb.width back 0 (huge-fn lift divergence); the
                    // out-ptr helper's readback was opt-forwarded to init 0. The f32
                    // return comes back in D0 as the call's SSA result (opt can't forward
                    // the init over it), and with D8-D15 preserved across calc's later
                    // calls the value survives to the return.
                    auto_block_inline_size(containing_block_size, box_props)
                }
                LayoutDisplay::InlineBlock | LayoutDisplay::InlineGrid | LayoutDisplay::InlineFlex => {
                    // +spec:width-calculation:c01de8 - inline-block auto width uses shrink-to-fit (§10.3.9)
                    // shrink-to-fit = min(max(preferred_minimum, available), preferred)
                    let available_width = (containing_block_size.width
                        - box_props.margin.left
                        - box_props.margin.right
                        - box_props.border.left
                        - box_props.border.right
                        - box_props.padding.left
                        - box_props.padding.right)
                        .max(0.0);
                    let preferred_minimum = intrinsic.min_content_width;
                    let preferred = intrinsic.max_content_width;
                    preferred_minimum.max(available_width).min(preferred).max(0.0)
                }
                LayoutDisplay::Inline => {
                    // For inline elements, 'auto' width is the intrinsic/max-content width
                    intrinsic.max_content_width
                }
                LayoutDisplay::Table | LayoutDisplay::InlineTable => intrinsic.max_content_width,
                // Table cells: during intrinsic measurement, intrinsic sizes
                // aren't known yet (0). Use containing block width so content
                // can expand and be measured. The table layout algorithm sets
                // the final cell width from computed column widths.
                LayoutDisplay::TableCell => {
                    if intrinsic.max_content_width > 0.0 {
                        intrinsic.max_content_width
                    } else {
                        (containing_block_size.width
                            - box_props.margin.left
                            - box_props.margin.right
                            - box_props.border.left
                            - box_props.border.right
                            - box_props.padding.left
                            - box_props.padding.right)
                            .max(0.0)
                    }
                }
                // Other display types use intrinsic sizing
                _ => intrinsic.max_content_width,
            }
            }
        }
        LayoutWidth::Px(px) => {
            let em = get_element_font_size(styled_dom, id, node_state);
            let rem = super::getters::get_root_font_size(styled_dom, node_state);
            let pixels_opt = super::calc::resolve_pixel_value_no_percent_with_viewport(
                &px, em, rem,
                viewport_size.width, viewport_size.height,
            );

            pixels_opt.unwrap_or_else(|| {
                px.to_percent().map_or(intrinsic.max_content_width, |p| {
                    resolve_percentage_with_box_model(
                        containing_block_size.width,
                        p.get(),
                        (box_props.margin.left, box_props.margin.right),
                        (box_props.border.left, box_props.border.right),
                        (box_props.padding.left, box_props.padding.right),
                    )
                })
            })
        }
        // +spec:intrinsic-sizing:069c75 - min-content, max-content, fit-content() sizing value keywords
        // +spec:intrinsic-sizing:1ce4fa - §3.2 min-content/max-content/fit-content() sizing values
        LayoutWidth::MinContent => intrinsic.min_content_width,
        LayoutWidth::MaxContent => intrinsic.max_content_width,
        // +spec:width-calculation:7b2128 - fit-content formula and non-negative inner size flooring (css-sizing-3 §3.2)
        // +spec:width-calculation:bf694a - min-content, max-content, fit-content() sizing values
        // css-sizing-3 §3.2: fit-content(<length-percentage>) = min(max-content, max(min-content, <length-percentage>))
        LayoutWidth::FitContent(px) => {
            let em = get_element_font_size(styled_dom, id, node_state);
            let rem = super::getters::get_root_font_size(styled_dom, node_state);
            let arg = super::calc::resolve_pixel_value_with_viewport(
                &px, containing_block_size.width, em, rem,
                viewport_size.width, viewport_size.height,
            );
            intrinsic.max_content_width.min(intrinsic.min_content_width.max(arg))
        }
        LayoutWidth::Calc(items) => {
            use azul_css::props::basic::pixel::DEFAULT_FONT_SIZE;
            let em = get_element_font_size(styled_dom, id, node_state);
            let calc_ctx = super::calc::CalcResolveContext {
                items, em_size: em, rem_size: DEFAULT_FONT_SIZE,
            };
            super::calc::evaluate_calc(&calc_ctx, containing_block_size.width)
        }
    };
    // css-sizing-3: "the used value is floored to preserve a non-negative inner size"
    let resolved_width = resolved_width.max(0.0);

    // +spec:height-calculation:7880e3 - Distinction between box types for height/margin calculation
    // +spec:height-calculation:753d8d - Height calculation for various box types (§10.6)
    // +spec:positioning:d5184e - percentage height resolved against containing block height
    // +spec:height-calculation:6a6cac - §10.5 content height resolution (auto, length, percentage)
    // +spec:height-calculation:d398e4 - §10.5/10.6 height property resolution for different box types
    // Step 2: Resolve the CSS `height` property into a concrete pixel value.
    // CSS `height` always refers to the physical vertical dimension, regardless of writing mode.
    // Percentage values resolve against the containing block's physical height.
    // In horizontal-tb: height = block size. In vertical modes: height = inline size.
    // The physical-to-logical mapping happens in Step 5 below.
    // Percentage values for `height` are resolved against the containing block's height.
    // +spec:height-calculation:0b5b0a - abs-pos replaced elements use intrinsic height for auto
    let resolved_height = match css_height.unwrap_or_default() {
        LayoutHeight::Auto => {
            // +spec:width-calculation:be5eb1 - auto height means available block space is infinite (unconstrained)
            // +spec:replaced-elements:994ac6 - §10.6.2: auto height for replaced elements uses intrinsic height or (used width)/ratio
            //
            // For block-level non-replaced containers in normal flow, CSS 2.2 §10.6.3
            // says auto height is resolved from children after layout. We return 0.0
            // as a placeholder; `apply_content_based_height` (cache.rs) overwrites it
            // with the laid-out content size. Reading `intrinsic.max_content_height`
            // here is unsafe: when the intrinsic pass short-circuits (e.g. a non-STF
            // subtree whose intrinsics are never consumed), that field is zero anyway
            // — so any caller that "trusts" the pre-layout value is depending on an
            // estimate that isn't guaranteed to exist.
            //
            // Shrink-to-fit contexts (inline-block, float, abspos, table/table-cell)
            // genuinely need intrinsic for width sizing; auto-height for those is
            // still driven by content, but we keep the intrinsic fallback for
            // backwards compatibility with the existing paths.
            // CSS 2.2 §10.6.4: an absolutely/fixed-positioned non-replaced box with
            // `height:auto` and BOTH `top` and `bottom` specified has a STRETCH-FIT
            // height = cb_height − top − bottom − margins. `position_out_of_flow_
            // elements` also derives this, but it runs AFTER the subtree is laid out —
            // so resolving it HERE (a definite, computed height) lets percentage-height
            // CHILDREN resolve against the real box during their own layout instead of
            // collapsing against a 0 placeholder. (Root cause of the slippy-map
            // VirtualView blank-bounds bug: its container fills via abs inset:0.)
            let abs_stretch_fit = if matches!(
                position,
                LayoutPosition::Absolute | LayoutPosition::Fixed
            ) && !is_replaced
            {
                let off = crate::solver3::positioning::resolve_position_offsets(
                    styled_dom, dom_id, *containing_block_size, *viewport_size,
                );
                match (off.top, off.bottom) {
                    (Some(t), Some(b)) => Some(
                        (containing_block_size.height
                            - t
                            - b
                            - box_props.margin.top
                            - box_props.margin.bottom)
                            .max(0.0),
                    ),
                    _ => None,
                }
            } else {
                None
            };
            match abs_stretch_fit {
                Some(h) => h,
                // §10.6.2: auto height for a replaced element (image / VirtualView)
                // uses its intrinsic height — mirrors the auto-WIDTH replaced branch
                // above. Without this, replaced nodes (no flow content) get 0 height
                // (the blank-image / "300x0" bug).
                None if is_replaced => intrinsic.max_content_height,
                None => match display.unwrap_or_default() {
                    LayoutDisplay::Block
                    | LayoutDisplay::FlowRoot
                    | LayoutDisplay::ListItem
                    | LayoutDisplay::Flex
                    | LayoutDisplay::Grid => 0.0,
                    // Inline: height property does not apply (§10.6.1), handled earlier
                    // via css_height override, but be explicit anyway.
                    LayoutDisplay::Inline => 0.0,
                    // Shrink-to-fit and intrinsically-sized: keep using intrinsic pre-layout.
                    _ => intrinsic.max_content_height,
                },
            }
        }
        LayoutHeight::Px(px) => {
            let em = get_element_font_size(styled_dom, id, node_state);
            let rem = super::getters::get_root_font_size(styled_dom, node_state);
            let pixels_opt = super::calc::resolve_pixel_value_no_percent_with_viewport(
                &px, em, rem,
                viewport_size.width, viewport_size.height,
            );

            // +spec:height-calculation:37bc8c - percentage heights resolve against definite containing block height
            pixels_opt.unwrap_or_else(|| {
                px.to_percent().map_or(intrinsic.max_content_height, |p| {
                    resolve_percentage_with_box_model(
                        containing_block_size.height,
                        p.get(),
                        (box_props.margin.top, box_props.margin.bottom),
                        (box_props.border.top, box_props.border.bottom),
                        (box_props.padding.top, box_props.padding.bottom),
                    )
                })
            })
        }
        // equivalent to automatic size (not min_content_height which is height at min-content width)
        LayoutHeight::MinContent => intrinsic.max_content_height,
        // equivalent to automatic size
        LayoutHeight::MaxContent => intrinsic.max_content_height,
        // css-sizing-3 §3.2: fit-content(<length-percentage>) = min(max-content, max(min-content, <length-percentage>))
        // For block axis, both min-content and max-content equal auto height
        LayoutHeight::FitContent(px) => {
            let em = get_element_font_size(styled_dom, id, node_state);
            let rem = super::getters::get_root_font_size(styled_dom, node_state);
            let arg = super::calc::resolve_pixel_value_with_viewport(
                &px, containing_block_size.height, em, rem,
                viewport_size.width, viewport_size.height,
            );
            let auto_height = intrinsic.max_content_height;
            auto_height.min(auto_height.max(arg))
        }
        LayoutHeight::Calc(items) => {
            use azul_css::props::basic::pixel::DEFAULT_FONT_SIZE;
            let em = get_element_font_size(styled_dom, id, node_state);
            let calc_ctx = super::calc::CalcResolveContext {
                items, em_size: em, rem_size: DEFAULT_FONT_SIZE,
            };
            super::calc::evaluate_calc(&calc_ctx, containing_block_size.height)
        }
    };
    // css-sizing-3: "the used value is floored to preserve a non-negative inner size"
    let resolved_height = resolved_height.max(0.0);

    // +spec:replaced-elements:5a85ce - abs-pos replaced: derive auto width from height × intrinsic ratio
    // +spec:replaced-elements:aedb26 - abs-pos replaced: both auto, ratio but no intrinsic w/h → block constraint
    // CSS Position 3 §6.2 (abs-replaced-width): For absolutely positioned replaced elements,
    // if width is auto and the element has an intrinsic ratio, width may be derived from height.
    let (resolved_width, resolved_height) = if is_replaced
        && width_is_auto
        && matches!(position, LayoutPosition::Absolute | LayoutPosition::Fixed)
    {
        let has_intrinsic_width = intrinsic.preferred_width.is_some_and(|w| w > 0.0);
        let has_intrinsic_height = intrinsic.preferred_height.is_some_and(|h| h > 0.0);
        let intrinsic_ratio = match (intrinsic.preferred_width, intrinsic.preferred_height) {
            (Some(iw), Some(ih)) if ih > 0.0 => Some(iw / ih),
            _ => None,
        };

        intrinsic_ratio.map_or((resolved_width, resolved_height), |ratio| if height_is_auto && !has_intrinsic_width && has_intrinsic_height {
                // §6.2 case: both auto, no intrinsic width, has intrinsic height + ratio
                // → width = used height × ratio
                (resolved_height * ratio, resolved_height)
            } else if !height_is_auto {
                // §6.2 case: width auto, height not auto, has intrinsic ratio
                // → width = used height × ratio
                (resolved_height * ratio, resolved_height)
            } else if height_is_auto && !has_intrinsic_width && !has_intrinsic_height {
                // §6.2 case: both auto, has ratio but no intrinsic width or height
                // → use block-level non-replaced constraint equation for width
                let block_width = (containing_block_size.width
                    - box_props.margin.left
                    - box_props.margin.right
                    - box_props.border.left
                    - box_props.border.right
                    - box_props.padding.left
                    - box_props.padding.right)
                    .max(0.0);
                (block_width, block_width / ratio)
            } else {
                (resolved_width, resolved_height)
            })
    } else {
        (resolved_width, resolved_height)
    };

    // +spec:aspect-ratio:0 - CSS Sizing 4: a non-replaced box with `aspect-ratio` and
    // exactly one auto axis derives the auto axis from the definite one via the ratio.
    // The ratio is applied to the content box here (box-sizing:border-box, which would
    // fold in padding+border, is not yet handled). Replaced elements use their intrinsic
    // ratio in the block above.
    #[allow(clippy::cast_precision_loss)] // small integer aspect-ratio components (e.g. 2000/1000)
    let (resolved_width, resolved_height) = if is_replaced {
        (resolved_width, resolved_height)
    } else if let MultiValue::Exact(azul_css::props::style::effects::StyleAspectRatio::Ratio(ar)) =
        crate::solver3::getters::get_aspect_ratio_property(styled_dom, id, node_state)
    {
        let ratio = if ar.height == 0 { 0.0 } else { ar.width as f32 / ar.height as f32 };
        if ratio > 0.0 && height_is_auto && !width_is_auto {
            (resolved_width, resolved_width / ratio)
        } else if ratio > 0.0 && width_is_auto && !height_is_auto {
            (resolved_height * ratio, resolved_height)
        } else {
            (resolved_width, resolved_height)
        }
    } else {
        (resolved_width, resolved_height)
    };

    // +spec:min-max-sizing:58869e - sizing properties width/height/min-width/min-height/max-width/max-height applied here
    // +spec:min-max-sizing:2e2414 - max-width/max-height specify maximum box dimensions, applied here
    // +spec:min-max-sizing:73f51a - tentative width clamped by max-width then min-width per §10.4
    // +spec:min-max-sizing:e98c4e - preferred size clamped by min/max, box-sizing handled
    // Step 3: Apply min/max constraints (CSS 2.2 § 10.4 and § 10.7)
    // "The tentative used width is calculated (without 'min-width' and 'max-width')
    // ...If the tentative used width is greater than 'max-width', the rules above are
    // applied again using the computed value of 'max-width' as the computed value for 'width'.
    // If the resulting width is smaller than 'min-width', the rules above are applied again
    // using the value of 'min-width' as the computed value for 'width'."

    // use the constraint violation table to coordinate width+height together;
    // for non-replaced elements, apply width and height constraints independently
    let has_intrinsic_ratio = intrinsic.preferred_width.is_some()
        && intrinsic.preferred_height.is_some()
        && intrinsic.preferred_width.unwrap_or(0.0) > 0.0
        && intrinsic.preferred_height.unwrap_or(0.0) > 0.0;

    // +spec:margin-collapsing:840eb6 - aspect ratio transfers size constraints across dimensions
    let (constrained_width, constrained_height) = if has_intrinsic_ratio {
        // +spec:width-calculation:ef71c4 - replaced elements with both width/height auto use constraint violation table
        // Replaced element with intrinsic ratio: use §10.4 constraint violation table
        apply_constraint_violation_table(
            styled_dom,
            id,
            node_state,
            resolved_width,
            resolved_height,
            containing_block_size.width,
            containing_block_size.height,
            box_props,
        )
    } else {
        // Non-replaced element: apply width and height constraints independently
        let cw = apply_width_constraints(
            styled_dom,
            id,
            node_state,
            resolved_width,
            containing_block_size.width,
            box_props,
        );

        let ch = apply_height_constraints(
            styled_dom,
            id,
            node_state,
            resolved_height,
            containing_block_size.height,
            box_props,
        );
        (cw, ch)
    };

    // +spec:box-model:cc170b - box-sizing: border-box includes padding+border in specified size; content-box adds them outside; content size floored at zero
    // +spec:box-model:d9d797 - box-sizing: content-box vs border-box dimension interpretation
    // +spec:box-model:e2a773 - box-sizing: border-box includes padding+border in width/height; content-box adds them outside
    // +spec:box-sizing:8159a8 - box-sizing property indicates whether content-box or border-box is measured
    // +spec:box-sizing:b0ff05 - border-box sets border-box to specified size, content-box calculated from it
    // +spec:box-sizing:aefeb2 - box-sizing: content-box vs border-box width/height interpretation
    // +spec:box-sizing:e2e28c - width/height refer to content-box size by default (content-box); box-sizing: border-box makes them refer to border-box size
    // Step 4: Convert to border-box dimensions, respecting box-sizing property
    // CSS box-sizing:
    // - content-box (default): width/height set content size, border+padding are added
    // - border-box: width/height set border-box size, border+padding are included
    let box_sizing = match get_css_box_sizing(styled_dom, id, node_state) {
        MultiValue::Exact(bs) => bs,
        MultiValue::Auto | MultiValue::Initial | MultiValue::Inherit => {
            azul_css::props::layout::LayoutBoxSizing::ContentBox
        }
    };

    let (border_box_width, border_box_height) = match box_sizing {
        azul_css::props::layout::LayoutBoxSizing::BorderBox => {
            // +spec:box-sizing:cdfe09 - box-sizing: border-box makes width/height set the border box
            // +spec:box-sizing:3ba6d3 - content-box floors at 0px, so border-box can't be less than padding+border
            let min_border_box_w = box_props.padding.left
                + box_props.padding.right
                + box_props.border.left
                + box_props.border.right;
            let min_border_box_h = box_props.padding.top
                + box_props.padding.bottom
                + box_props.border.top
                + box_props.border.bottom;
            // +spec:box-model:4f423b - used values refer to the border box when box-sizing: border-box
            // border-box: The width/height values already include border and padding
            // CSS Box Sizing Level 3: "the specified width and height (and respective min/max
            // properties) on this element determine the border box of the element"
            // However, non-quantitative values (auto, min-content, max-content) are not
            // influenced by box-sizing, so they still need border+padding added.
            // Floor: content-box cannot go negative, so border-box >= padding+border
            let bw = if width_is_quantitative {
                constrained_width.max(min_border_box_w)
            } else {
                constrained_width
                    + box_props.padding.left
                    + box_props.padding.right
                    + box_props.border.left
                    + box_props.border.right
            };
            let bh = if height_is_quantitative {
                constrained_height.max(min_border_box_h)
            } else {
                constrained_height
                    + box_props.padding.top
                    + box_props.padding.bottom
                    + box_props.border.top
                    + box_props.border.bottom
            };
            (bw, bh)
        }
        azul_css::props::layout::LayoutBoxSizing::ContentBox => {
            // +spec:box-sizing:fead70 - content-box: width/height set content size, border+padding added outside
            let border_box_width = constrained_width
                + box_props.padding.left
                + box_props.padding.right
                + box_props.border.left
                + box_props.border.right;
            let border_box_height = constrained_height
                + box_props.padding.top
                + box_props.padding.bottom
                + box_props.border.top
                + box_props.border.bottom;
            (border_box_width, border_box_height)
        }
    };

    // +spec:block-formatting-context:c6fb58 - vertical writing modes swap layout dimensions
    // +spec:min-max-sizing:d97870 - width/height/min/max refer to physical dimensions; layout rules are logical
    // Step 5: Map the resolved physical dimensions to logical dimensions.
    //
    // CSS Writing Modes Level 4:
    // - In horizontal-tb: width = inline (cross) size, height = block (main) size.
    // - In vertical-rl/lr: width = block (main) size, height = inline (cross) size.
    //
    // `from_main_cross` handles this mapping: given (main, cross) and writing mode,
    // it produces the correct LogicalSize with physical (width, height).
    let (main_size, cross_size) = if is_vertical {
        // Vertical writing mode: width is the block (main) dimension,
        // height is the inline (cross) dimension.
        (border_box_width, border_box_height)
    } else {
        // Horizontal writing mode (default): width is cross, height is main.
        (border_box_height, border_box_width)
    };

    // Step 6: Construct the final LogicalSize from the logical dimensions.
    // +spec:min-max-sizing:2f66a6 - direction-dependent layout rules abstracted to logical start/end via writing mode
    let result =
        LogicalSize::from_main_cross(main_size, cross_size, writing_mode.unwrap_or_default());

    Ok(result)
}

// +spec:min-max-sizing:b02ebc - sizing properties min-width/max-width/min-height/max-height and preferred aspect ratio
// +spec:replaced-elements:740f3e - constraint violation table for replaced elements with intrinsic ratio and both width/height auto
// +spec:min-max-sizing:939f2c - use min-width/min-height <length> with aspect ratio for replaced elements
// with intrinsic ratios. Implements all 10 cases from the spec table, coordinating
// +spec:min-max-sizing:07620d - CSS 2.2 §10.4 constraint violation table for replaced elements with intrinsic ratios
// Implements all 11 cases from the spec table, coordinating
// width and height together to preserve the aspect ratio while respecting min/max constraints.
fn apply_constraint_violation_table(
    styled_dom: &StyledDom,
    id: NodeId,
    node_state: &StyledNodeState,
    w: f32,  // tentative width (ignoring min/max)
    h: f32,  // tentative height (ignoring min/max)
    containing_block_width: f32,
    containing_block_height: f32,
    box_props: &BoxProps,
) -> (f32, f32) {
    use crate::solver3::getters::{
        get_css_min_width, get_css_max_width, get_css_min_height, get_css_max_height, MultiValue,
    };

    // Resolve em against the element's OWN font-size and rem against the root
    // font-size, NOT a hard-coded 16px.
    let em = get_element_font_size(styled_dom, id, node_state);
    let rem = super::getters::get_root_font_size(styled_dom, node_state);

    // +spec:min-max-sizing:92ab8d - constraint violation table for replaced elements with intrinsic ratio (cyclic percentage contributions use auto fallback)
    // +spec:min-max-sizing:ad8605 - min-height/max-height interact with percentage heights; percentages behave as auto in intrinsic contribution calc

    // +spec:positioning:c0af55 - automatic minimum size of abspos box is always zero (default 0.0)
    // Resolve min-width (default 0)
    let min_w = match get_css_min_width(styled_dom, id, node_state) {
        MultiValue::Exact(mw) => resolve_px_with_box_model(&mw.inner, containing_block_width, box_props, true, em, rem).unwrap_or(0.0),
        _ => 0.0,
    };

    // Resolve max-width (default infinity)
    let max_w = match get_css_max_width(styled_dom, id, node_state) {
        MultiValue::Exact(mw) => {
            if mw.inner.number.get() >= core::f32::MAX - 1.0 {
                f32::MAX
            } else {
                resolve_px_with_box_model(&mw.inner, containing_block_width, box_props, true, em, rem).unwrap_or(f32::MAX)
            }
        }
        _ => f32::MAX,
    };

    // Resolve min-height (default 0)
    let min_h = match get_css_min_height(styled_dom, id, node_state) {
        MultiValue::Exact(mh) => resolve_px_with_box_model(&mh.inner, containing_block_height, box_props, false, em, rem).unwrap_or(0.0),
        _ => 0.0,
    };

    // Resolve max-height (default infinity)
    let max_h = match get_css_max_height(styled_dom, id, node_state) {
        MultiValue::Exact(mh) => {
            if mh.inner.number.get() >= core::f32::MAX - 1.0 {
                f32::MAX
            } else {
                resolve_px_with_box_model(&mh.inner, containing_block_height, box_props, false, em, rem).unwrap_or(f32::MAX)
            }
        }
        _ => f32::MAX,
    };

    // max(min, max) so that min ≤ max holds true."
    let max_w = max_w.max(min_w);
    let max_h = max_h.max(min_h);

    // Guard against zero dimensions (avoid division by zero)
    if w <= 0.0 || h <= 0.0 {
        return (w.max(min_w).min(max_w), h.max(min_h).min(max_h));
    }

    let w_over = w > max_w;
    let w_under = w < min_w;
    let h_over = h > max_h;
    let h_under = h < min_h;

    // +spec:min-max-sizing:713560 - constraint violation table for replaced elements with intrinsic ratio
    match (w_over, w_under, h_over, h_under) {
        // Row 1: no constraint violation
        (false, false, false, false) => (w, h),

        // Row 2: w > max-width only
        (true, false, false, false) => {
            (max_w, (max_w * h / w).max(min_h))
        }

        // Row 3: w < min-width only
        (false, true, false, false) => {
            (min_w, (min_w * h / w).min(max_h))
        }

        // Row 4: h > max-height only
        (false, false, true, false) => {
            ((max_h * w / h).max(min_w), max_h)
        }

        // Row 5: h < min-height only
        (false, false, false, true) => {
            ((min_h * w / h).min(max_w), min_h)
        }

        // Row 6+7: (w > max-width) and (h > max-height)
        (true, false, true, false) => {
            if max_w / w <= max_h / h {
                (max_w, (max_w * h / w).max(min_h))
            } else {
                ((max_h * w / h).max(min_w), max_h)
            }
        }

        // Row 8+9: (w < min-width) and (h < min-height)
        (false, true, false, true) => {
            if min_w / w <= min_h / h {
                ((min_h * w / h).min(max_w), min_h)
            } else {
                (min_w, (min_w * h / w).min(max_h))
            }
        }

        // Row 10: (w < min-width) and (h > max-height)
        (false, true, true, false) => (min_w, max_h),

        // Row 11: (w > max-width) and (h < min-height)
        (true, false, false, true) => (max_w, min_h),

        // Fallback (impossible combinations like w_over && w_under)
        _ => (w.max(min_w).min(max_w), h.max(min_h).min(max_h)),
    }
}

// +spec:min-max-sizing:114b53 - min-width/max-width/min-height/max-height property definitions: initial values, percentage resolution against containing block, applies to elements accepting width/height
// +spec:min-max-sizing:12667d - width/height/min-width/min-height/max-width/max-height properties from CSS Sizing 3
/// +spec:min-max-sizing:205e9e - intrinsic size constraints (min/max-content contributions, min/max sizing properties)
// +spec:min-max-sizing:cac146 - min-width/min-height specify minimum box dimensions; max overridden by min
// +spec:width-calculation:e77d58 - min/max-width clamping algorithm per CSS 2.2 § 10.4
// +spec:width-calculation:1d63f0 - min-width/max-width property resolution and value meanings
/// Apply min-width and max-width constraints to tentative width
/// Per CSS 2.2 § 10.4: min-width overrides max-width if min > max
fn apply_width_constraints(
    styled_dom: &StyledDom,
    id: NodeId,
    node_state: &StyledNodeState,
    tentative_width: f32,
    containing_block_width: f32,
    box_props: &BoxProps,
) -> f32 {
    use crate::solver3::getters::{get_css_max_width, get_css_min_width, MultiValue};

    // Resolve em against the element's OWN font-size and rem against the root.
    let em = get_element_font_size(styled_dom, id, node_state);
    let rem = super::getters::get_root_font_size(styled_dom, node_state);

    // +spec:display-property:0c55e5 - auto min-width resolves to 0 for CSS2 display types
    // Resolve min-width (default is 0)
    let min_width = match get_css_min_width(styled_dom, id, node_state) {
        MultiValue::Exact(mw) => resolve_px_with_box_model(&mw.inner, containing_block_width, box_props, true, em, rem).unwrap_or(0.0),
        _ => 0.0,
    };

    // Resolve max-width (default is infinity/none)
    let max_width = match get_css_max_width(styled_dom, id, node_state) {
        MultiValue::Exact(mw) => {
            if mw.inner.number.get() >= core::f32::MAX - 1.0 {
                None
            } else {
                resolve_px_with_box_model(&mw.inner, containing_block_width, box_props, true, em, rem)
            }
        }
        _ => None,
    };

    // Apply constraints: max(min_width, min(tentative, max_width))
    // If min > max, min wins per CSS spec
    let mut result = tentative_width;
    if let Some(max) = max_width {
        result = result.min(max);
    }
    result.max(min_width)
}

/// Apply min-height and max-height constraints to tentative height
/// Per CSS 2.2 § 10.7: min-height overrides max-height if min > max
// +spec:height-calculation:22a77a - percentage min/max-height resolved against containing block; if CB height depends on content and element is not absolutely positioned, percentage treated as 0 (min-height) or none (max-height)
// +spec:height-calculation:982aaf - min-height/max-height constrain box heights to a range
// +spec:height-calculation:c6c33a - min-height and max-height property resolution and application
fn apply_height_constraints(
    styled_dom: &StyledDom,
    id: NodeId,
    node_state: &StyledNodeState,
    tentative_height: f32,
    containing_block_height: f32,
    box_props: &BoxProps,
) -> f32 {
    use crate::solver3::getters::{get_css_max_height, get_css_min_height, MultiValue};

    // Resolve em against the element's OWN font-size and rem against the root.
    let em = get_element_font_size(styled_dom, id, node_state);
    let rem = super::getters::get_root_font_size(styled_dom, node_state);

    // for backwards-compat with CSS2 display types (block, inline, inline-block, table)
    // Resolve min-height (default is 0)
    let min_height = match get_css_min_height(styled_dom, id, node_state) {
        MultiValue::Exact(mh) => resolve_px_with_box_model(&mh.inner, containing_block_height, box_props, false, em, rem).unwrap_or(0.0),
        _ => 0.0,
    };

    // Resolve max-height (default is infinity/none)
    let max_height = match get_css_max_height(styled_dom, id, node_state) {
        MultiValue::Exact(mh) => {
            if mh.inner.number.get() >= core::f32::MAX - 1.0 {
                None
            } else {
                resolve_px_with_box_model(&mh.inner, containing_block_height, box_props, false, em, rem)
            }
        }
        _ => None,
    };

    // +spec:height-calculation:297001 - min/max height constraint algorithm per CSS 2.2 §10.7
    // Apply constraints: max(min_height, min(tentative, max_height))
    // If min > max, min wins per CSS spec
    let mut result = tentative_height;
    if let Some(max) = max_height {
        result = result.min(max);
    }
    result.max(min_height)
}

#[must_use] pub fn extract_text_from_node(styled_dom: &StyledDom, node_id: NodeId) -> Option<String> {
    match &styled_dom.node_data.as_container()[node_id].get_node_type() {
        NodeType::Text(text_data) => {
            Some(text_data.as_str().to_string())
        }
        _ => None,
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp, clippy::too_many_lines)]
mod autotest_generated {
    use std::collections::{BTreeMap, HashMap, HashSet};

    use azul_core::{
        dom::{Dom, DomId, IdOrClass},
        selection::TextSelection,
    };
    use azul_css::props::basic::{FontRef, SizeMetric};

    use super::*;
    use crate::solver3::{
        geometry::{EdgeSizes, MarginAuto, PackedBoxProps},
        layout_tree::{generate_layout_tree, LayoutNodeCold, LayoutNodeWarm},
    };

    // ==================================================================
    // Fixtures
    // ==================================================================

    const VIEWPORT: LogicalSize = LogicalSize {
        width: 800.0,
        height: 600.0,
    };

    const BLOCK: FormattingContext = FormattingContext::Block {
        establishes_new_context: false,
    };

    fn size(w: f32, h: f32) -> LogicalSize {
        LogicalSize::new(w, h)
    }

    fn all_edges(v: f32) -> EdgeSizes {
        EdgeSizes {
            top: v,
            right: v,
            bottom: v,
            left: v,
        }
    }

    /// `BoxProps` with the same value on every edge of each ring.
    fn props(margin: f32, border: f32, padding: f32) -> BoxProps {
        BoxProps {
            margin: all_edges(margin),
            border: all_edges(border),
            padding: all_edges(padding),
            margin_auto: MarginAuto::default(),
        }
    }

    fn zero_props() -> BoxProps {
        props(0.0, 0.0, 0.0)
    }

    fn isz(min_w: f32, max_w: f32, min_h: f32, max_h: f32) -> IntrinsicSizes {
        IntrinsicSizes {
            min_content_width: min_w,
            max_content_width: max_w,
            preferred_width: None,
            min_content_height: min_h,
            max_content_height: max_h,
            preferred_height: None,
        }
    }

    fn styled(dom: Dom, css_str: &str) -> StyledDom {
        let mut dom = dom;
        let (css, _warnings) = azul_css::parser2::new_from_str(css_str);
        StyledDom::create(&mut dom, css)
    }

    fn div_class(class: &str) -> Dom {
        Dom::create_div().with_ids_and_classes(vec![IdOrClass::Class(class.into())].into())
    }

    /// Owns everything a `LayoutContext` borrows. A font-less `FontManager`
    /// (empty `FcFontCache`) is enough for every function exercised here:
    /// text *shaping* is never reached — the DOM fixtures that go through the
    /// intrinsic-sizing pass carry no text nodes, and the text fixtures only
    /// go through `collect_inline_content`, which gathers but never measures.
    struct Env {
        styled_dom: StyledDom,
        font_manager: FontManager<FontRef>,
        text_selections: BTreeMap<DomId, TextSelection>,
        counters: HashMap<(usize, String), i32>,
        image_cache: azul_core::resources::ImageCache,
        debug_messages: Option<Vec<LayoutDebugMessage>>,
    }

    impl Env {
        fn new(styled_dom: StyledDom) -> Self {
            Self {
                styled_dom,
                font_manager: FontManager::new(FcFontCache::default())
                    .expect("FontManager over an empty font cache"),
                text_selections: BTreeMap::new(),
                counters: HashMap::new(),
                image_cache: azul_core::resources::ImageCache::default(),
                debug_messages: None,
            }
        }

        fn ctx(&mut self) -> LayoutContext<'_, FontRef> {
            LayoutContext {
                scrollbar_style_cache: core::cell::RefCell::new(HashMap::new()),
                styled_dom: &self.styled_dom,
                font_manager: &self.font_manager,
                text_selections: &self.text_selections,
                debug_messages: &mut self.debug_messages,
                counters: &mut self.counters,
                viewport_size: VIEWPORT,
                fragmentation_context: None,
                cursor_is_visible: true,
                cursor_locations: Vec::new(),
                preedit_text: None,
                dirty_text_overrides: BTreeMap::new(),
                cache_map: crate::solver3::cache::LayoutCacheMap::default(),
                image_cache: &self.image_cache,
                system_style: None,
                get_system_time_fn: azul_core::task::GetSystemTimeCallback {
                    cb: azul_core::task::get_system_time_libstd,
                },
            }
        }
    }

    fn hot(parent: Option<usize>, fc: FormattingContext, bp: &BoxProps) -> LayoutNodeHot {
        LayoutNodeHot {
            box_props: PackedBoxProps::pack(bp),
            dom_node_id: None,
            used_size: None,
            formatting_context: fc,
            parent,
        }
    }

    /// Hand-builds a `LayoutTree` (SoA invariants kept consistent) from hot
    /// nodes + per-node child lists. `dom_node_id` is `None` throughout, which
    /// is exactly the "anonymous box" path through the sizing code.
    fn tree_of(nodes: Vec<LayoutNodeHot>, child_lists: &[Vec<usize>]) -> LayoutTree {
        let n = nodes.len();
        let mut children_arena: Vec<usize> = Vec::new();
        let mut children_offsets: Vec<(u32, u32)> = Vec::with_capacity(n);
        for cl in child_lists {
            let start = u32::try_from(children_arena.len()).expect("arena fits in u32");
            children_arena.extend_from_slice(cl);
            children_offsets.push((start, u32::try_from(cl.len()).expect("len fits in u32")));
        }
        while children_offsets.len() < n {
            children_offsets.push((0, 0));
        }
        LayoutTree {
            nodes,
            warm: vec![LayoutNodeWarm::default(); n],
            cold: vec![LayoutNodeCold::default(); n],
            root: 0,
            dom_to_layout: BTreeMap::new(),
            children_arena,
            children_offsets,
            subtree_needs_intrinsic: Vec::new(),
        }
    }

    /// The single layout index of a DOM node (fixtures never produce splits).
    fn layout_index(tree: &LayoutTree, dom_id: NodeId) -> usize {
        *tree
            .dom_to_layout
            .get(&dom_id)
            .and_then(|v| v.first())
            .expect("DOM node has a layout node")
    }

    // ==================================================================
    // resolve_percentage_with_box_model  (numeric)
    // ==================================================================

    #[test]
    fn resolve_percentage_at_zero_is_zero_on_both_operands() {
        assert_eq!(
            resolve_percentage_with_box_model(0.0, 0.5, (0.0, 0.0), (0.0, 0.0), (0.0, 0.0)),
            0.0
        );
        assert_eq!(
            resolve_percentage_with_box_model(800.0, 0.0, (0.0, 0.0), (0.0, 0.0), (0.0, 0.0)),
            0.0
        );
    }

    #[test]
    fn resolve_percentage_ignores_the_box_model_arguments_entirely() {
        // Documented contract: margins/borders/paddings are accepted for
        // call-site convenience and MUST NOT influence the result (CSS 2.1
        // §10.2 — percentages resolve against the containing block itself).
        let plain =
            resolve_percentage_with_box_model(800.0, 0.5, (0.0, 0.0), (0.0, 0.0), (0.0, 0.0));
        let poisoned = resolve_percentage_with_box_model(
            800.0,
            0.5,
            (f32::NAN, f32::INFINITY),
            (f32::MAX, f32::MIN),
            (-1e30, 1e30),
        );
        assert_eq!(plain, 400.0);
        assert_eq!(poisoned, 400.0, "box-model args must not leak into the result");
    }

    #[test]
    fn resolve_percentage_floors_negative_products_at_zero() {
        // +spec:containing-block:f1344e — negative CB width yields zero.
        assert_eq!(
            resolve_percentage_with_box_model(-800.0, 0.5, (0.0, 0.0), (0.0, 0.0), (0.0, 0.0)),
            0.0
        );
        assert_eq!(
            resolve_percentage_with_box_model(800.0, -0.5, (0.0, 0.0), (0.0, 0.0), (0.0, 0.0)),
            0.0
        );
        // Two negatives multiply back to a positive — still deterministic.
        assert_eq!(
            resolve_percentage_with_box_model(-800.0, -0.5, (0.0, 0.0), (0.0, 0.0), (0.0, 0.0)),
            400.0
        );
    }

    #[test]
    fn resolve_percentage_never_returns_nan() {
        // f32::max(NaN, 0.0) == 0.0, so every NaN-producing combination
        // (NaN operand, or inf * 0 == NaN) collapses to a defined 0.0.
        for (cb, pct) in [
            (f32::NAN, 0.5),
            (800.0, f32::NAN),
            (f32::NAN, f32::NAN),
            (f32::INFINITY, 0.0),
            (f32::NEG_INFINITY, 0.0),
            (f32::NEG_INFINITY, 0.5),
        ] {
            let r = resolve_percentage_with_box_model(cb, pct, (0.0, 0.0), (0.0, 0.0), (0.0, 0.0));
            assert!(!r.is_nan(), "NaN escaped for cb={cb}, pct={pct}");
            assert_eq!(r, 0.0, "cb={cb}, pct={pct}");
        }
    }

    #[test]
    fn resolve_percentage_saturates_to_infinity_on_overflow() {
        // f32::MAX * 100 overflows to +inf — saturation, not a panic.
        let r =
            resolve_percentage_with_box_model(f32::MAX, 100.0, (0.0, 0.0), (0.0, 0.0), (0.0, 0.0));
        assert!(r.is_infinite() && r.is_sign_positive());
        // An infinite CB with a finite non-zero percentage stays infinite.
        let r = resolve_percentage_with_box_model(
            f32::INFINITY,
            0.5,
            (0.0, 0.0),
            (0.0, 0.0),
            (0.0, 0.0),
        );
        assert!(r.is_infinite() && r.is_sign_positive());
    }

    #[test]
    fn resolve_percentage_is_monotone_in_the_percentage() {
        let at = |p: f32| {
            resolve_percentage_with_box_model(800.0, p, (0.0, 0.0), (0.0, 0.0), (0.0, 0.0))
        };
        assert!(at(0.0) <= at(0.25) && at(0.25) <= at(0.5) && at(0.5) <= at(1.0));
        assert_eq!(at(1.0), 800.0);
    }

    // ==================================================================
    // resolve_px_with_box_model  (numeric)
    // ==================================================================

    #[test]
    fn resolve_px_absolute_length_ignores_the_containing_block() {
        let bp = props(7.0, 3.0, 11.0);
        let px = PixelValue::const_px(50);
        assert_eq!(
            resolve_px_with_box_model(&px, 800.0, &bp, true, 16.0, 16.0),
            Some(50.0)
        );
        // A wildly different containing block cannot move an absolute length.
        assert_eq!(
            resolve_px_with_box_model(&px, -1.0e30, &bp, false, 16.0, 16.0),
            Some(50.0)
        );
    }

    #[test]
    fn resolve_px_percentage_resolves_against_the_containing_block_on_either_axis() {
        let bp = props(10.0, 2.0, 5.0);
        let px = PixelValue::const_percent(50);
        let horizontal = resolve_px_with_box_model(&px, 800.0, &bp, true, 16.0, 16.0);
        let vertical = resolve_px_with_box_model(&px, 800.0, &bp, false, 16.0, 16.0);
        assert_eq!(horizontal, Some(400.0));
        // `is_horizontal` picks which box-model edges are passed down, but the
        // resolver discards them — so both axes agree for the same CB extent.
        assert_eq!(vertical, horizontal);
    }

    #[test]
    fn resolve_px_percentage_against_a_degenerate_containing_block_is_zero() {
        let bp = zero_props();
        let px = PixelValue::const_percent(50);
        for cb in [-800.0, f32::NAN, f32::NEG_INFINITY] {
            let r = resolve_px_with_box_model(&px, cb, &bp, true, 16.0, 16.0)
                .expect("a percentage always resolves to Some");
            assert!(!r.is_nan(), "NaN escaped for cb={cb}");
            assert_eq!(r, 0.0, "cb={cb}");
        }
    }

    #[test]
    fn resolve_px_em_and_rem_resolve_against_the_supplied_font_sizes() {
        let bp = zero_props();
        assert_eq!(
            resolve_px_with_box_model(&PixelValue::const_em(3), 800.0, &bp, true, 20.0, 16.0),
            Some(60.0)
        );
        assert_eq!(
            resolve_px_with_box_model(
                &PixelValue::from_metric(SizeMetric::Rem, 2.0),
                800.0,
                &bp,
                true,
                20.0,
                16.0
            ),
            Some(32.0)
        );
        // A zero font-size collapses em to 0 rather than producing NaN.
        assert_eq!(
            resolve_px_with_box_model(&PixelValue::const_em(3), 800.0, &bp, true, 0.0, 0.0),
            Some(0.0)
        );
    }

    #[test]
    fn resolve_px_returns_none_for_viewport_units() {
        // Viewport units are neither absolute (no viewport is threaded in here)
        // nor percentages, so this resolver reports "cannot resolve". Every
        // caller (`apply_width_constraints`, `apply_height_constraints`,
        // `apply_constraint_violation_table`) turns that into the *default*
        // (0 / none), i.e. a `min-width: 10vw` constraint is silently dropped.
        let bp = zero_props();
        for metric in [
            SizeMetric::Vw,
            SizeMetric::Vh,
            SizeMetric::Vmin,
            SizeMetric::Vmax,
        ] {
            let px = PixelValue::from_metric(metric, 10.0);
            assert_eq!(
                resolve_px_with_box_model(&px, 800.0, &bp, true, 16.0, 16.0),
                None,
                "{metric:?} unexpectedly resolved"
            );
        }
    }

    #[test]
    fn resolve_px_extreme_lengths_stay_finite() {
        // `PixelValue` stores its number as a fixed-point isize, so f32
        // extremes are clamped at construction — nothing infinite or NaN can
        // reach the sizing math through a `PixelValue`.
        let bp = zero_props();
        for raw in [f32::MAX, f32::MIN, f32::INFINITY, f32::NEG_INFINITY, f32::NAN] {
            let px = PixelValue::px(raw);
            let r = resolve_px_with_box_model(&px, 800.0, &bp, true, 16.0, 16.0)
                .expect("px metric always resolves to Some");
            assert!(r.is_finite(), "non-finite length from PixelValue::px({raw})");
        }
        assert_eq!(
            resolve_px_with_box_model(&PixelValue::px(f32::NAN), 800.0, &bp, true, 16.0, 16.0),
            Some(0.0),
            "NaN saturates to 0 in the fixed-point encoding"
        );
    }

    // ==================================================================
    // auto_block_inline_size  (numeric)
    // ==================================================================

    #[test]
    fn auto_block_inline_size_subtracts_the_full_horizontal_box_model() {
        let cb = size(800.0, 600.0);
        // margin 10 + border 2 + padding 5, on both sides = 34 total.
        assert_eq!(auto_block_inline_size(&cb, &props(10.0, 2.0, 5.0)), 766.0);
        assert_eq!(auto_block_inline_size(&cb, &zero_props()), 800.0);
    }

    #[test]
    fn auto_block_inline_size_floors_at_zero_when_the_box_model_exceeds_the_cb() {
        let cb = size(10.0, 600.0);
        assert_eq!(auto_block_inline_size(&cb, &props(100.0, 50.0, 25.0)), 0.0);
        // A zero-width containing block is exactly the boundary case.
        assert_eq!(auto_block_inline_size(&size(0.0, 0.0), &zero_props()), 0.0);
        // A negative containing block never yields a negative inline size.
        assert_eq!(auto_block_inline_size(&size(-800.0, 0.0), &zero_props()), 0.0);
    }

    #[test]
    fn auto_block_inline_size_never_returns_nan() {
        let cases = [
            (size(f32::NAN, 0.0), zero_props()),
            (size(f32::INFINITY, 0.0), props(f32::INFINITY, 0.0, 0.0)),
            (size(f32::NEG_INFINITY, 0.0), zero_props()),
            (size(0.0, 0.0), props(f32::NAN, 0.0, 0.0)),
        ];
        for (cb, bp) in cases {
            let r = auto_block_inline_size(&cb, &bp);
            assert!(!r.is_nan(), "NaN escaped for cb.width={}", cb.width);
            assert_eq!(r, 0.0);
        }
    }

    #[test]
    fn auto_block_inline_size_saturates_rather_than_overflowing() {
        // f32::MAX minus finite edges stays finite; +inf CB stays +inf.
        let r = auto_block_inline_size(&size(f32::MAX, 0.0), &props(1.0, 1.0, 1.0));
        assert!(r.is_finite() && r > 0.0);
        let r = auto_block_inline_size(&size(f32::INFINITY, 0.0), &props(1.0, 1.0, 1.0));
        assert!(r.is_infinite() && r.is_sign_positive());
    }

    // ==================================================================
    // compute_dirty_ancestor_closure  (other)
    // ==================================================================

    /// 0 → 1 → 2 (2's parent is 1, 1's parent is 0).
    fn chain_tree() -> LayoutTree {
        let bp = zero_props();
        tree_of(
            vec![
                hot(None, BLOCK, &bp),
                hot(Some(0), BLOCK, &bp),
                hot(Some(1), BLOCK, &bp),
            ],
            &[vec![1], vec![2], vec![]],
        )
    }

    #[test]
    fn dirty_closure_of_an_empty_set_is_empty() {
        let tree = chain_tree();
        let closure = compute_dirty_ancestor_closure(&tree, &BTreeSet::new());
        assert!(closure.is_empty());
    }

    #[test]
    fn dirty_closure_of_a_leaf_contains_every_ancestor_up_to_the_root() {
        let tree = chain_tree();
        let dirty: BTreeSet<usize> = [2].into_iter().collect();
        let closure = compute_dirty_ancestor_closure(&tree, &dirty);
        assert_eq!(closure, [0, 1, 2].into_iter().collect::<HashSet<usize>>());
    }

    #[test]
    fn dirty_closure_tolerates_out_of_range_dirty_indices() {
        let tree = chain_tree();
        let dirty: BTreeSet<usize> = [usize::MAX, 999, 2].into_iter().collect();
        let closure = compute_dirty_ancestor_closure(&tree, &dirty);
        // The bogus ids are inserted (they have no parent to walk), the real
        // one still drags in its ancestors. No panic, no index arithmetic.
        assert!(closure.contains(&usize::MAX) && closure.contains(&999));
        assert!(closure.contains(&0) && closure.contains(&1) && closure.contains(&2));
    }

    #[test]
    fn dirty_closure_terminates_on_a_cyclic_parent_chain() {
        // A malformed tree (0's parent is 1, 1's parent is 0) must not spin
        // forever: the `insert` returning false breaks the walk.
        let bp = zero_props();
        let tree = tree_of(
            vec![hot(Some(1), BLOCK, &bp), hot(Some(0), BLOCK, &bp)],
            &[vec![], vec![]],
        );
        let dirty: BTreeSet<usize> = [0].into_iter().collect();
        let closure = compute_dirty_ancestor_closure(&tree, &dirty);
        assert_eq!(closure, [0, 1].into_iter().collect::<HashSet<usize>>());
    }

    #[test]
    fn dirty_closure_terminates_on_a_self_parenting_node() {
        let bp = zero_props();
        let tree = tree_of(vec![hot(Some(0), BLOCK, &bp)], &[vec![]]);
        let dirty: BTreeSet<usize> = [0].into_iter().collect();
        let closure = compute_dirty_ancestor_closure(&tree, &dirty);
        assert_eq!(closure, [0].into_iter().collect::<HashSet<usize>>());
    }

    // ==================================================================
    // IntrinsicSizeCalculator::new  (constructor)
    // ==================================================================

    #[test]
    fn intrinsic_size_calculator_new_starts_without_a_dirty_closure() {
        let mut env = Env::new(styled(Dom::create_body(), ""));
        let mut ctx = env.ctx();
        let mut text_cache = LayoutCache::new();
        let calc = IntrinsicSizeCalculator::new(&mut ctx, &mut text_cache);
        assert!(
            calc.dirty_closure.is_none(),
            "a fresh calculator must not skip any node"
        );
        assert_eq!(calc.ctx.viewport_size, VIEWPORT, "ctx is threaded through");
    }

    // ==================================================================
    // calculate_intrinsic_recursive / calculate_node_intrinsic_sizes
    // ==================================================================

    #[test]
    fn calculate_intrinsic_recursive_rejects_an_out_of_range_node_index() {
        let mut env = Env::new(styled(Dom::create_body(), ""));
        let mut ctx = env.ctx();
        let mut text_cache = LayoutCache::new();
        let mut calc = IntrinsicSizeCalculator::new(&mut ctx, &mut text_cache);
        let mut tree = chain_tree();

        for bogus in [3, 999, usize::MAX] {
            let r = calc.calculate_intrinsic_recursive(&mut tree, bogus, false);
            assert!(
                matches!(r, Err(LayoutError::InvalidTree)),
                "index {bogus} must be rejected, not panic"
            );
        }
    }

    #[test]
    fn calculate_node_intrinsic_sizes_rejects_an_out_of_range_node_index() {
        let mut env = Env::new(styled(Dom::create_body(), ""));
        let mut ctx = env.ctx();
        let mut text_cache = LayoutCache::new();
        let mut calc = IntrinsicSizeCalculator::new(&mut ctx, &mut text_cache);
        let tree = chain_tree();
        let r = calc.calculate_node_intrinsic_sizes(&tree, usize::MAX, &[]);
        assert!(matches!(r, Err(LayoutError::InvalidTree)));
    }

    #[test]
    fn calculate_intrinsic_recursive_skips_stray_child_indices_instead_of_aborting() {
        // Reconcile can mis-list a child index that has no node (the g52 case).
        // The whole pass must survive it, not abort with InvalidTree.
        let bp = zero_props();
        let mut tree = tree_of(
            vec![hot(None, BLOCK, &bp), hot(Some(0), BLOCK, &bp)],
            &[vec![1, 4242], vec![]],
        );
        let mut env = Env::new(styled(Dom::create_body(), ""));
        let mut ctx = env.ctx();
        let mut text_cache = LayoutCache::new();
        let mut calc = IntrinsicSizeCalculator::new(&mut ctx, &mut text_cache);

        let r = calc.calculate_intrinsic_recursive(&mut tree, 0, false);
        let sizes = r.expect("a stray child index must be skipped, not fatal");
        assert!(sizes.min_content_width.is_finite());
        assert!(tree.warm(0).and_then(|w| w.intrinsic_sizes).is_some());
    }

    #[test]
    fn calculate_intrinsic_recursive_reuses_the_cache_for_nodes_outside_the_dirty_closure() {
        let mut tree = chain_tree();
        let cached = isz(11.0, 22.0, 33.0, 44.0);
        tree.warm_mut(0)
            .expect("root warm slot")
            .intrinsic_sizes = Some(cached);

        let mut env = Env::new(styled(Dom::create_body(), ""));
        let mut ctx = env.ctx();
        let mut text_cache = LayoutCache::new();
        let mut calc = IntrinsicSizeCalculator::new(&mut ctx, &mut text_cache);
        // An empty closure means "nothing is dirty" — the cached value wins
        // and the descent is skipped entirely.
        calc.dirty_closure = Some(HashSet::new());

        let sizes = calc
            .calculate_intrinsic_recursive(&mut tree, 0, false)
            .expect("cached path");
        assert_eq!(sizes.min_content_width, 11.0);
        assert_eq!(sizes.max_content_width, 22.0);
        assert_eq!(sizes.min_content_height, 33.0);
        assert_eq!(sizes.max_content_height, 44.0);
        // Children were never visited, so their warm slots stay empty.
        assert!(tree.warm(1).and_then(|w| w.intrinsic_sizes).is_none());
    }

    // ==================================================================
    // calculate_block_intrinsic_sizes  (numeric)
    // ==================================================================

    /// root(0) with `n` block children, no box-model extras.
    fn block_parent_with_children(n: usize) -> LayoutTree {
        parent_with_children(BLOCK, n)
    }

    /// root(0) establishing `fc`, with `n` childless block children.
    fn parent_with_children(fc: FormattingContext, n: usize) -> LayoutTree {
        let bp = zero_props();
        let mut nodes = vec![hot(None, fc, &bp)];
        let mut kids = Vec::new();
        for i in 0..n {
            nodes.push(hot(Some(0), BLOCK, &bp));
            kids.push(i + 1);
        }
        let mut child_lists = vec![kids];
        child_lists.resize(n + 1, Vec::new());
        tree_of(nodes, &child_lists)
    }

    #[test]
    fn block_intrinsic_sizes_take_the_max_width_and_the_sum_of_heights() {
        let tree = block_parent_with_children(2);
        let mut env = Env::new(styled(Dom::create_body(), ""));
        let mut ctx = env.ctx();
        let mut text_cache = LayoutCache::new();
        let calc = IntrinsicSizeCalculator::new(&mut ctx, &mut text_cache);

        let children = [(1usize, isz(10.0, 20.0, 5.0, 6.0)), (2usize, isz(30.0, 40.0, 7.0, 8.0))];
        let r = calc
            .calculate_block_intrinsic_sizes(&tree, 0, &children)
            .expect("valid tree");
        assert_eq!(r.min_content_width, 30.0, "cross axis = widest child");
        assert_eq!(r.max_content_width, 40.0);
        assert_eq!(r.min_content_height, 14.0, "main axis = stacked heights");
        assert_eq!(r.max_content_height, 14.0);
    }

    #[test]
    fn block_intrinsic_sizes_ignore_children_missing_from_the_intrinsics_slice() {
        let tree = block_parent_with_children(2);
        let mut env = Env::new(styled(Dom::create_body(), ""));
        let mut ctx = env.ctx();
        let mut text_cache = LayoutCache::new();
        let calc = IntrinsicSizeCalculator::new(&mut ctx, &mut text_cache);

        let r = calc
            .calculate_block_intrinsic_sizes(&tree, 0, &[])
            .expect("valid tree");
        assert_eq!(r.min_content_width, 0.0);
        assert_eq!(r.max_content_width, 0.0);
        assert_eq!(r.min_content_height, 0.0);
        assert_eq!(r.max_content_height, 0.0);
    }

    #[test]
    fn block_intrinsic_sizes_saturate_to_infinity_instead_of_overflowing() {
        let tree = block_parent_with_children(2);
        let mut env = Env::new(styled(Dom::create_body(), ""));
        let mut ctx = env.ctx();
        let mut text_cache = LayoutCache::new();
        let calc = IntrinsicSizeCalculator::new(&mut ctx, &mut text_cache);

        let huge = isz(f32::MAX, f32::MAX, f32::MAX, f32::MAX);
        let children = [(1usize, huge), (2usize, huge)];
        let r = calc
            .calculate_block_intrinsic_sizes(&tree, 0, &children)
            .expect("valid tree");
        // MAX + MAX overflows the f32 range: +inf, not a wrap, not a panic.
        assert!(r.min_content_height.is_infinite() && r.min_content_height.is_sign_positive());
        assert_eq!(r.max_content_width, f32::MAX, "cross axis only takes a max");
    }

    #[test]
    fn block_intrinsic_sizes_sanitize_nan_on_the_cross_axis() {
        let tree = block_parent_with_children(1);
        let mut env = Env::new(styled(Dom::create_body(), ""));
        let mut ctx = env.ctx();
        let mut text_cache = LayoutCache::new();
        let calc = IntrinsicSizeCalculator::new(&mut ctx, &mut text_cache);

        let nan = isz(f32::NAN, f32::NAN, f32::NAN, f32::NAN);
        let r = calc
            .calculate_block_intrinsic_sizes(&tree, 0, &[(1usize, nan)])
            .expect("valid tree");
        // The cross axis goes through `f32::max`, which drops NaN — so a NaN
        // child cannot poison the parent's width. The main axis is a plain
        // sum, so it does carry the NaN through (unreachable in practice:
        // every measured/fallback intrinsic is finite).
        assert!(!r.min_content_width.is_nan() && r.min_content_width == 0.0);
        assert!(!r.max_content_width.is_nan() && r.max_content_width == 0.0);
        assert!(r.min_content_height.is_nan());
    }

    #[test]
    fn block_intrinsic_sizes_reject_an_out_of_range_node_index() {
        let tree = block_parent_with_children(1);
        let mut env = Env::new(styled(Dom::create_body(), ""));
        let mut ctx = env.ctx();
        let mut text_cache = LayoutCache::new();
        let calc = IntrinsicSizeCalculator::new(&mut ctx, &mut text_cache);
        let r = calc.calculate_block_intrinsic_sizes(&tree, usize::MAX, &[]);
        assert!(matches!(r, Err(LayoutError::InvalidTree)));
    }

    // ==================================================================
    // calculate_flex_intrinsic_sizes  (numeric)
    // ==================================================================

    fn flex_parent_with_children(n: usize) -> LayoutTree {
        parent_with_children(FormattingContext::Flex, n)
    }

    #[test]
    fn flex_row_intrinsic_sizes_sum_the_main_axis_and_max_the_cross_axis() {
        let tree = flex_parent_with_children(2);
        let mut env = Env::new(styled(Dom::create_body(), ""));
        let mut ctx = env.ctx();
        let mut text_cache = LayoutCache::new();
        let calc = IntrinsicSizeCalculator::new(&mut ctx, &mut text_cache);

        let children = [(1usize, isz(10.0, 20.0, 5.0, 6.0)), (2usize, isz(30.0, 40.0, 7.0, 8.0))];
        let r = calc
            .calculate_flex_intrinsic_sizes(&tree, 0, &children)
            .expect("valid tree");
        // No DOM node → default flex-direction: row, default flex-wrap: nowrap
        // → single line → min-content main = SUM of item min-contents.
        assert_eq!(r.min_content_width, 40.0);
        assert_eq!(r.max_content_width, 60.0);
        assert_eq!(r.min_content_height, 7.0);
        assert_eq!(r.max_content_height, 8.0);
    }

    #[test]
    fn flex_intrinsic_sizes_are_zero_when_no_child_intrinsics_are_supplied() {
        let tree = flex_parent_with_children(3);
        let mut env = Env::new(styled(Dom::create_body(), ""));
        let mut ctx = env.ctx();
        let mut text_cache = LayoutCache::new();
        let calc = IntrinsicSizeCalculator::new(&mut ctx, &mut text_cache);

        let r = calc
            .calculate_flex_intrinsic_sizes(&tree, 0, &[])
            .expect("valid tree");
        assert_eq!(r.min_content_width, 0.0);
        assert_eq!(r.max_content_width, 0.0);
        assert_eq!(r.min_content_height, 0.0);
        assert_eq!(r.max_content_height, 0.0);
    }

    #[test]
    fn flex_intrinsic_sizes_saturate_on_a_summing_overflow() {
        let tree = flex_parent_with_children(2);
        let mut env = Env::new(styled(Dom::create_body(), ""));
        let mut ctx = env.ctx();
        let mut text_cache = LayoutCache::new();
        let calc = IntrinsicSizeCalculator::new(&mut ctx, &mut text_cache);

        let huge = isz(f32::MAX, f32::MAX, 1.0, 2.0);
        let children = [(1usize, huge), (2usize, huge)];
        let r = calc
            .calculate_flex_intrinsic_sizes(&tree, 0, &children)
            .expect("valid tree");
        assert!(r.min_content_width.is_infinite() && r.min_content_width.is_sign_positive());
        assert!(r.max_content_width.is_infinite() && r.max_content_width.is_sign_positive());
        // The cross axis only takes maxima, so it stays finite.
        assert_eq!(r.max_content_height, 2.0);
    }

    #[test]
    fn flex_intrinsic_sizes_reject_an_out_of_range_node_index() {
        let tree = flex_parent_with_children(1);
        let mut env = Env::new(styled(Dom::create_body(), ""));
        let mut ctx = env.ctx();
        let mut text_cache = LayoutCache::new();
        let calc = IntrinsicSizeCalculator::new(&mut ctx, &mut text_cache);
        let r = calc.calculate_flex_intrinsic_sizes(&tree, usize::MAX, &[]);
        assert!(matches!(r, Err(LayoutError::InvalidTree)));
    }

    // ==================================================================
    // calculate_table_intrinsic_sizes  (numeric)
    // ==================================================================

    /// table(0) > row(1) > [cell(2), cell(3)]
    fn table_tree() -> LayoutTree {
        let bp = zero_props();
        tree_of(
            vec![
                hot(None, FormattingContext::Table, &bp),
                hot(Some(0), FormattingContext::TableRow, &bp),
                hot(Some(1), FormattingContext::TableCell, &bp),
                hot(Some(1), FormattingContext::TableCell, &bp),
            ],
            &[vec![1], vec![2, 3], vec![], vec![]],
        )
    }

    #[test]
    fn table_intrinsic_sizes_sum_columns_and_stack_row_heights() {
        let tree = table_tree();
        let mut env = Env::new(styled(Dom::create_body(), ""));
        let mut ctx = env.ctx();
        let mut text_cache = LayoutCache::new();
        let mut calc = IntrinsicSizeCalculator::new(&mut ctx, &mut text_cache);

        // Cell intrinsics keyed by the *cell* indices (the aggregation path).
        let cells = [(2usize, isz(30.0, 50.0, 10.0, 20.0)), (3usize, isz(40.0, 60.0, 10.0, 15.0))];
        let r = calc.calculate_table_intrinsic_sizes(&tree, 0, &cells);
        assert_eq!(r.min_content_width, 70.0, "sum of per-column minima");
        assert_eq!(r.max_content_width, 110.0, "sum of per-column maxima");
        assert_eq!(r.min_content_height, 20.0, "row height = tallest cell");
        assert_eq!(r.max_content_height, 20.0);
    }

    #[test]
    fn table_intrinsic_sizes_are_zero_when_cells_carry_no_measurable_content() {
        // The real caller passes the table's *direct* children (rows), so cell
        // lookups miss and each cell is re-measured through the IFC path. With
        // anonymous (DOM-less) cells there is nothing to measure → all zeros.
        let tree = table_tree();
        let mut env = Env::new(styled(Dom::create_body(), ""));
        let mut ctx = env.ctx();
        let mut text_cache = LayoutCache::new();
        let mut calc = IntrinsicSizeCalculator::new(&mut ctx, &mut text_cache);

        let rows = [(1usize, isz(1.0, 2.0, 3.0, 4.0))];
        let r = calc.calculate_table_intrinsic_sizes(&tree, 0, &rows);
        assert_eq!(r.min_content_width, 0.0);
        assert_eq!(r.max_content_width, 0.0);
        assert_eq!(r.max_content_height, 0.0);
    }

    #[test]
    fn table_intrinsic_sizes_of_a_table_without_rows_are_zero() {
        // A `FormattingContext::Table` whose children are neither rows nor row
        // groups must not panic — it simply aggregates nothing.
        let bp = zero_props();
        let tree = tree_of(
            vec![
                hot(None, FormattingContext::Table, &bp),
                hot(Some(0), BLOCK, &bp),
            ],
            &[vec![1], vec![]],
        );
        let mut env = Env::new(styled(Dom::create_body(), ""));
        let mut ctx = env.ctx();
        let mut text_cache = LayoutCache::new();
        let mut calc = IntrinsicSizeCalculator::new(&mut ctx, &mut text_cache);

        let r = calc.calculate_table_intrinsic_sizes(&tree, 0, &[(1usize, isz(9.0, 9.0, 9.0, 9.0))]);
        assert_eq!(r.min_content_width, 0.0);
        assert_eq!(r.max_content_width, 0.0);
        assert_eq!(r.min_content_height, 0.0);
    }

    #[test]
    fn table_intrinsic_sizes_saturate_on_extreme_cell_widths() {
        let tree = table_tree();
        let mut env = Env::new(styled(Dom::create_body(), ""));
        let mut ctx = env.ctx();
        let mut text_cache = LayoutCache::new();
        let mut calc = IntrinsicSizeCalculator::new(&mut ctx, &mut text_cache);

        let huge = isz(f32::MAX, f32::MAX, 1.0, 1.0);
        let cells = [(2usize, huge), (3usize, huge)];
        let r = calc.calculate_table_intrinsic_sizes(&tree, 0, &cells);
        assert!(r.min_content_width.is_infinite() && r.min_content_width.is_sign_positive());
        assert!(r.max_content_width.is_infinite() && r.max_content_width.is_sign_positive());
        assert_eq!(r.max_content_height, 1.0);
    }

    #[test]
    fn table_intrinsic_sizes_with_an_out_of_range_index_are_zero() {
        let tree = table_tree();
        let mut env = Env::new(styled(Dom::create_body(), ""));
        let mut ctx = env.ctx();
        let mut text_cache = LayoutCache::new();
        let mut calc = IntrinsicSizeCalculator::new(&mut ctx, &mut text_cache);

        // `tree.children(usize::MAX)` must yield an empty slice, not panic.
        let r = calc.calculate_table_intrinsic_sizes(&tree, usize::MAX, &[]);
        assert_eq!(r.min_content_width, 0.0);
        assert_eq!(r.max_content_height, 0.0);
    }

    // ==================================================================
    // calculate_intrinsic_sizes (phase 2a entry point)
    // ==================================================================

    /// `body(0) > .flex(1) > .a(2)` — deliberately text-free, so the whole
    /// intrinsic pass runs without ever entering text shaping. `.flex` is a
    /// shrink-to-fit context, so Fix C does not short-circuit the subtree.
    fn flex_dom() -> StyledDom {
        styled(
            Dom::create_body().with_child(div_class("flex").with_child(div_class("a"))),
            ".flex { display: flex; } .a { display: block; min-width: 120px; min-height: 30px; }",
        )
    }

    #[test]
    fn calculate_intrinsic_sizes_is_a_no_op_when_nothing_is_dirty() {
        let mut env = Env::new(flex_dom());
        let mut ctx = env.ctx();
        let mut tree = generate_layout_tree(&mut ctx).expect("layout tree");
        let mut text_cache = LayoutCache::new();

        calculate_intrinsic_sizes(&mut ctx, &mut tree, &mut text_cache, &BTreeSet::new())
            .expect("empty dirty set returns early");
        assert!(
            tree.warm.iter().all(|w| w.intrinsic_sizes.is_none()),
            "an empty dirty set must not compute anything"
        );
    }

    #[test]
    fn calculate_intrinsic_sizes_applies_the_min_width_floor_bottom_up() {
        let mut env = Env::new(flex_dom());
        let mut ctx = env.ctx();
        let mut tree = generate_layout_tree(&mut ctx).expect("layout tree");
        let mut text_cache = LayoutCache::new();
        let dirty: BTreeSet<usize> = (0..tree.nodes.len()).collect();

        calculate_intrinsic_sizes(&mut ctx, &mut tree, &mut text_cache, &dirty).expect("sizing");

        // `.a` is an empty block: its content intrinsic is 0, but `min-width`
        // / `min-height` are <length>s, so they floor both min- and
        // max-content (+spec:min-max-sizing:970fef).
        let a = layout_index(&tree, NodeId::new(2));
        let a_sizes = tree
            .warm(a)
            .and_then(|w| w.intrinsic_sizes)
            .expect("`.a` was measured");
        assert_eq!(a_sizes.min_content_width, 120.0);
        assert_eq!(a_sizes.max_content_width, 120.0);
        assert_eq!(a_sizes.min_content_height, 30.0);
        assert_eq!(a_sizes.max_content_height, 30.0);

        // The flex container aggregates its single item on both axes.
        let f = layout_index(&tree, NodeId::new(1));
        let f_sizes = tree
            .warm(f)
            .and_then(|w| w.intrinsic_sizes)
            .expect("`.flex` was measured");
        assert_eq!(f_sizes.min_content_width, 120.0);
        assert_eq!(f_sizes.max_content_width, 120.0);
        assert_eq!(f_sizes.max_content_height, 30.0);
    }

    #[test]
    fn calculate_intrinsic_sizes_tolerates_bogus_dirty_node_indices() {
        let mut env = Env::new(flex_dom());
        let mut ctx = env.ctx();
        let mut tree = generate_layout_tree(&mut ctx).expect("layout tree");
        let mut text_cache = LayoutCache::new();
        // Dirty ids that no longer exist (a stale dirty set after a DOM shrink)
        // must not index out of bounds nor abort the pass.
        let dirty: BTreeSet<usize> = [0, 999, usize::MAX].into_iter().collect();

        calculate_intrinsic_sizes(&mut ctx, &mut tree, &mut text_cache, &dirty)
            .expect("stale dirty ids must be ignored, not fatal");
        let root = tree.warm(tree.root).and_then(|w| w.intrinsic_sizes);
        assert!(root.is_some(), "the root is still measured");
    }

    #[test]
    fn calculate_intrinsic_sizes_is_idempotent_across_repeated_passes() {
        let mut env = Env::new(flex_dom());
        let mut ctx = env.ctx();
        let mut tree = generate_layout_tree(&mut ctx).expect("layout tree");
        let mut text_cache = LayoutCache::new();
        let dirty: BTreeSet<usize> = (0..tree.nodes.len()).collect();

        calculate_intrinsic_sizes(&mut ctx, &mut tree, &mut text_cache, &dirty).expect("pass 1");
        let a = layout_index(&tree, NodeId::new(2));
        let first = tree.warm(a).and_then(|w| w.intrinsic_sizes).expect("measured");

        calculate_intrinsic_sizes(&mut ctx, &mut tree, &mut text_cache, &dirty).expect("pass 2");
        let second = tree.warm(a).and_then(|w| w.intrinsic_sizes).expect("measured");

        assert_eq!(first.min_content_width, second.min_content_width);
        assert_eq!(first.max_content_width, second.max_content_width);
        assert_eq!(first.min_content_height, second.min_content_height);
        assert_eq!(first.max_content_height, second.max_content_height);
    }

    // ==================================================================
    // collect_inline_content / collect_inline_content_recursive
    // ==================================================================

    fn text_dom(text: &str) -> StyledDom {
        styled(
            Dom::create_body().with_child(div_class("p").with_child(Dom::create_text(text))),
            ".p { display: block; }",
        )
    }

    fn collected_text(items: &[InlineContent]) -> String {
        items
            .iter()
            .filter_map(|item| match item {
                InlineContent::Text(run) => Some(run.text.as_str().to_string()),
                _ => None,
            })
            .collect()
    }

    /// The layout node the IFC sizer measures the text through: the text node's
    /// own layout node when reconcile produced one, otherwise the enclosing
    /// block (whose DOM-children scan then picks the text up). Both routes must
    /// surface the same characters — which is exactly the invariant under test.
    fn text_ifc_index(tree: &LayoutTree, text_dom: NodeId, block_dom: NodeId) -> usize {
        tree.dom_to_layout
            .get(&text_dom)
            .and_then(|v| v.first())
            .copied()
            .unwrap_or_else(|| layout_index(tree, block_dom))
    }

    #[test]
    fn collect_inline_content_gathers_the_text_of_an_ifc_root() {
        let mut env = Env::new(text_dom("hello world"));
        let mut ctx = env.ctx();
        let tree = generate_layout_tree(&mut ctx).expect("layout tree");
        let idx = text_ifc_index(&tree, NodeId::new(2), NodeId::new(1));

        let items = collect_inline_content(&mut ctx, &tree, idx).expect("collect");
        assert!(!items.is_empty(), "the IFC root must see its text");
        assert!(collected_text(&items).contains("hello"));
    }

    #[test]
    fn collect_inline_content_preserves_unicode_verbatim() {
        // Combining marks, an RTL run, an emoji ZWJ sequence — none of this may
        // be truncated, re-encoded, or split mid-scalar.
        let needle = "e\u{301}llo مرحبا 👨\u{200d}👩\u{200d}👧";
        let mut env = Env::new(text_dom(needle));
        let mut ctx = env.ctx();
        let tree = generate_layout_tree(&mut ctx).expect("layout tree");
        let idx = text_ifc_index(&tree, NodeId::new(2), NodeId::new(1));

        let items = collect_inline_content(&mut ctx, &tree, idx).expect("collect");
        let text = collected_text(&items);
        assert!(text.contains('\u{301}'), "combining acute survived");
        assert!(text.contains("مرحبا"), "RTL run survived");
        assert!(text.contains("👨\u{200d}👩\u{200d}👧"), "ZWJ sequence survived");
    }

    #[test]
    fn collect_inline_content_handles_whitespace_only_and_very_long_text() {
        for text in [" \n\t".to_string(), "x".repeat(20_000)] {
            let mut env = Env::new(text_dom(&text));
            let mut ctx = env.ctx();
            let tree = generate_layout_tree(&mut ctx).expect("layout tree");
            let idx = text_ifc_index(&tree, NodeId::new(2), NodeId::new(1));
            let items = collect_inline_content(&mut ctx, &tree, idx)
                .expect("degenerate text must still collect");
            // The DOM-children scan and the layout-children walk must not BOTH
            // pick the run up (that double-count made inline-blocks 2× too wide).
            assert!(
                collected_text(&items).len() <= text.len(),
                "the same text run was collected more than once"
            );
        }
    }

    #[test]
    fn collect_inline_content_rejects_an_out_of_range_root_index() {
        let mut env = Env::new(text_dom("hello"));
        let mut ctx = env.ctx();
        let tree = generate_layout_tree(&mut ctx).expect("layout tree");

        for bogus in [tree.nodes.len(), 999, usize::MAX] {
            let r = collect_inline_content(&mut ctx, &tree, bogus);
            assert!(
                matches!(r, Err(LayoutError::InvalidTree)),
                "index {bogus} must be rejected, not panic"
            );
        }
    }

    #[test]
    fn collect_inline_content_of_a_text_free_subtree_is_empty() {
        let mut env = Env::new(flex_dom());
        let mut ctx = env.ctx();
        let tree = generate_layout_tree(&mut ctx).expect("layout tree");
        let a = layout_index(&tree, NodeId::new(2));

        let items = collect_inline_content(&mut ctx, &tree, a).expect("collect");
        assert!(
            collected_text(&items).is_empty(),
            "a childless block has no inline text"
        );
    }

    // ==================================================================
    // subtree_contains_text  (other)
    // ==================================================================

    #[test]
    fn subtree_contains_text_sees_the_node_itself_and_its_descendants() {
        let dom = text_dom("hi");
        // body(0) > .p(1) > text(2)
        assert!(subtree_contains_text(&dom, NodeId::new(2)), "the text node itself");
        assert!(subtree_contains_text(&dom, NodeId::new(1)), "its parent");
        assert!(subtree_contains_text(&dom, NodeId::new(0)), "the root");
    }

    #[test]
    fn subtree_contains_text_is_false_for_a_text_free_subtree() {
        let dom = styled(
            Dom::create_body().with_child(div_class("a").with_child(div_class("b"))),
            "",
        );
        assert!(!subtree_contains_text(&dom, NodeId::new(0)));
        assert!(!subtree_contains_text(&dom, NodeId::new(1)));
        assert!(!subtree_contains_text(&dom, NodeId::new(2)));
    }

    #[test]
    fn subtree_contains_text_walks_a_deeply_nested_subtree() {
        // The recursion is unbounded in depth — 200 levels must not blow up.
        const DEPTH: usize = 200;
        let mut inner = Dom::create_div().with_child(Dom::create_text("deep"));
        for _ in 0..DEPTH {
            inner = Dom::create_div().with_child(inner);
        }
        let dom = styled(Dom::create_body().with_child(inner), "");
        assert!(subtree_contains_text(&dom, NodeId::new(0)));

        let mut empty = Dom::create_div();
        for _ in 0..DEPTH {
            empty = Dom::create_div().with_child(empty);
        }
        let dom = styled(Dom::create_body().with_child(empty), "");
        assert!(!subtree_contains_text(&dom, NodeId::new(0)));
    }

    // ==================================================================
    // extract_text_from_node  (other)
    // ==================================================================

    #[test]
    fn extract_text_from_node_round_trips_the_exact_string() {
        for needle in [
            "hello world",
            " \n\t",
            "e\u{301}llo مرحبا 👨\u{200d}👩\u{200d}👧",
            "line1\nline2\r\n\u{0}nul",
        ] {
            let dom = text_dom(needle);
            assert_eq!(
                extract_text_from_node(&dom, NodeId::new(2)).as_deref(),
                Some(needle),
                "text must survive the DOM round-trip byte for byte"
            );
        }
    }

    #[test]
    fn extract_text_from_node_is_none_for_non_text_nodes() {
        let dom = text_dom("hello");
        assert_eq!(extract_text_from_node(&dom, NodeId::new(0)), None, "body");
        assert_eq!(extract_text_from_node(&dom, NodeId::new(1)), None, "div");
    }

    #[test]
    fn extract_text_from_node_handles_a_very_long_string() {
        let long = "ü".repeat(50_000);
        let dom = text_dom(&long);
        let got = extract_text_from_node(&dom, NodeId::new(2)).expect("text node");
        assert_eq!(got.chars().count(), 50_000);
        assert_eq!(got, long);
    }

    // ==================================================================
    // calculate_used_size_for_node  (numeric)
    // ==================================================================

    /// One classed div per constraint case; DOM ids follow pre-order.
    ///  body(0), .plain(1), .pct(2), .clamped(3), .maxed(4), .pctmin(5),
    ///  .bbox(6), .autoblock(7), .vwmin(8), .em(9), .hclamped(10), .row10(11)
    fn constraints_dom() -> StyledDom {
        styled(
            Dom::create_body()
                .with_child(div_class("plain"))
                .with_child(div_class("pct"))
                .with_child(div_class("clamped"))
                .with_child(div_class("maxed"))
                .with_child(div_class("pctmin"))
                .with_child(div_class("bbox"))
                .with_child(div_class("autoblock"))
                .with_child(div_class("vwmin"))
                .with_child(div_class("em"))
                .with_child(div_class("hclamped"))
                .with_child(div_class("row10")),
            "
            .plain     { display: block; width: 50px; height: 20px; }
            .pct       { display: block; width: 50%; height: 25%; }
            .clamped   { display: block; width: 300px; min-width: 200px; max-width: 100px; }
            .maxed     { display: block; width: 300px; max-width: 100px; }
            .pctmin    { display: block; min-width: 50%; }
            .bbox      { display: block; width: 5px; height: 5px; box-sizing: border-box; }
            .autoblock { display: block; }
            .vwmin     { display: block; width: 300px; min-width: 10vw; }
            .em        { display: block; font-size: 20px; min-width: 3em; }
            .hclamped  { display: block; height: 300px; min-height: 200px; max-height: 100px; }
            .row10     { display: block; min-width: 200px; max-height: 50px; }
            ",
        )
    }

    const PLAIN: NodeId = NodeId::new(1);
    const PCT: NodeId = NodeId::new(2);
    const CLAMPED: NodeId = NodeId::new(3);
    const MAXED: NodeId = NodeId::new(4);
    const PCTMIN: NodeId = NodeId::new(5);
    const BBOX: NodeId = NodeId::new(6);
    const AUTOBLOCK: NodeId = NodeId::new(7);
    const VWMIN: NodeId = NodeId::new(8);
    const EM: NodeId = NodeId::new(9);
    const HCLAMPED: NodeId = NodeId::new(10);
    const ROW10: NodeId = NodeId::new(11);

    fn node_state(dom: &StyledDom, id: NodeId) -> StyledNodeState {
        dom.styled_nodes.as_container()[id]
            .styled_node_state
    }

    fn used_size(
        dom: &StyledDom,
        id: NodeId,
        cb: LogicalSize,
        bp: &BoxProps,
    ) -> LogicalSize {
        calculate_used_size_for_node(
            dom,
            Some(id),
            &cb,
            IntrinsicSizes::default(),
            bp,
            &VIEWPORT,
        )
        .expect("used size")
    }

    #[test]
    fn used_size_of_an_anonymous_box_fills_the_cb_inline_and_uses_content_height() {
        let dom = constraints_dom();
        let cb = size(800.0, 600.0);
        let bp = zero_props();

        let r = calculate_used_size_for_node(&dom, None, &cb, isz(0.0, 0.0, 0.0, 42.0), &bp, &VIEWPORT)
            .expect("anonymous box");
        assert_eq!(r.width, 800.0);
        assert_eq!(r.height, 42.0);

        // A non-positive content height means "auto" — resolved later from the
        // laid-out children, so 0.0 (not the negative value) is stored now.
        let r = calculate_used_size_for_node(&dom, None, &cb, isz(0.0, 0.0, 0.0, -5.0), &bp, &VIEWPORT)
            .expect("anonymous box");
        assert_eq!(r.height, 0.0);
    }

    #[test]
    fn used_size_resolves_absolute_lengths_and_adds_the_content_box_extras() {
        let dom = constraints_dom();
        let cb = size(800.0, 600.0);

        let r = used_size(&dom, PLAIN, cb, &zero_props());
        assert_eq!(r.width, 50.0);
        assert_eq!(r.height, 20.0);

        // content-box (default): padding + border grow the border box.
        let r = used_size(&dom, PLAIN, cb, &props(0.0, 2.0, 10.0));
        assert_eq!(r.width, 50.0 + 2.0 * (2.0 + 10.0));
        assert_eq!(r.height, 20.0 + 2.0 * (2.0 + 10.0));
    }

    #[test]
    fn used_size_resolves_percentages_against_the_physical_containing_block() {
        let dom = constraints_dom();
        let r = used_size(&dom, PCT, size(800.0, 600.0), &zero_props());
        assert_eq!(r.width, 400.0, "50% of the CB width");
        assert_eq!(r.height, 150.0, "25% of the CB height");
    }

    #[test]
    fn used_size_percentages_against_degenerate_containing_blocks_never_produce_nan() {
        let dom = constraints_dom();
        let bp = zero_props();
        for cb in [
            size(f32::NAN, f32::NAN),
            size(-800.0, -600.0),
            size(0.0, 0.0),
            size(f32::NEG_INFINITY, f32::NEG_INFINITY),
        ] {
            let r = used_size(&dom, PCT, cb, &bp);
            assert!(!r.width.is_nan() && !r.height.is_nan(), "NaN for cb={cb:?}");
            assert!(r.width >= 0.0 && r.height >= 0.0, "negative size for cb={cb:?}");
        }
        // An infinite CB stays infinite (saturation), never NaN.
        let r = used_size(&dom, PCT, size(f32::INFINITY, f32::INFINITY), &bp);
        assert!(r.width.is_infinite() && r.width.is_sign_positive());
    }

    #[test]
    fn used_size_min_width_overrides_max_width_when_they_conflict() {
        // CSS 2.2 §10.4: if min-width > max-width, min-width wins.
        let dom = constraints_dom();
        let cb = size(800.0, 600.0);
        assert_eq!(used_size(&dom, CLAMPED, cb, &zero_props()).width, 200.0);
        // Without the conflicting min, max-width clamps normally.
        assert_eq!(used_size(&dom, MAXED, cb, &zero_props()).width, 100.0);
    }

    #[test]
    fn used_size_min_height_overrides_max_height_when_they_conflict() {
        let dom = constraints_dom();
        let cb = size(800.0, 600.0);
        assert_eq!(used_size(&dom, HCLAMPED, cb, &zero_props()).height, 200.0);
    }

    #[test]
    fn used_size_border_box_floors_at_the_padding_plus_border_sum() {
        // box-sizing: border-box with width:5px and 10px padding per side:
        // the content box cannot go negative, so the border box floors at 20.
        let dom = constraints_dom();
        let r = used_size(&dom, BBOX, size(800.0, 600.0), &props(0.0, 0.0, 10.0));
        assert_eq!(r.width, 20.0);
        assert_eq!(r.height, 20.0);

        // With no padding/border the specified size IS the border box.
        let r = used_size(&dom, BBOX, size(800.0, 600.0), &zero_props());
        assert_eq!(r.width, 5.0);
        assert_eq!(r.height, 5.0);
    }

    #[test]
    fn used_size_auto_width_block_fills_the_cb_minus_its_box_model() {
        let dom = constraints_dom();
        let cb = size(800.0, 600.0);

        let r = used_size(&dom, AUTOBLOCK, cb, &props(100.0, 0.0, 0.0));
        assert_eq!(r.width, 600.0, "800 - 2*100 margin");
        assert_eq!(r.height, 0.0, "auto block height is filled in after layout");

        // Box model wider than the CB → floored at 0, never negative.
        let r = used_size(&dom, AUTOBLOCK, size(10.0, 600.0), &props(100.0, 0.0, 0.0));
        assert_eq!(r.width, 0.0);
    }

    // ==================================================================
    // apply_width_constraints / apply_height_constraints  (numeric)
    // ==================================================================

    fn width_constrained(dom: &StyledDom, id: NodeId, tentative: f32, cb_width: f32) -> f32 {
        let state = node_state(dom, id);
        apply_width_constraints(dom, id, &state, tentative, cb_width, &zero_props())
    }

    fn height_constrained(dom: &StyledDom, id: NodeId, tentative: f32, cb_height: f32) -> f32 {
        let state = node_state(dom, id);
        apply_height_constraints(dom, id, &state, tentative, cb_height, &zero_props())
    }

    #[test]
    fn width_constraints_are_the_identity_without_min_or_max() {
        let dom = constraints_dom();
        for tentative in [0.0, 42.0, f32::MAX] {
            assert_eq!(width_constrained(&dom, PLAIN, tentative, 800.0), tentative);
        }
    }

    #[test]
    fn width_constraints_clamp_then_let_min_win_over_max() {
        let dom = constraints_dom();
        assert_eq!(width_constrained(&dom, MAXED, 300.0, 800.0), 100.0, "max clamps");
        assert_eq!(width_constrained(&dom, MAXED, 50.0, 800.0), 50.0, "below max: untouched");
        assert_eq!(
            width_constrained(&dom, CLAMPED, 300.0, 800.0),
            200.0,
            "min-width overrides max-width per §10.4"
        );
    }

    #[test]
    fn width_constraints_resolve_percentage_minimums_against_the_containing_block() {
        let dom = constraints_dom();
        assert_eq!(width_constrained(&dom, PCTMIN, 10.0, 800.0), 400.0);
        // A negative CB floors the percentage at 0, so the min is inert.
        assert_eq!(width_constrained(&dom, PCTMIN, 10.0, -800.0), 10.0);
        // A NaN CB must not poison the result.
        let r = width_constrained(&dom, PCTMIN, 10.0, f32::NAN);
        assert!(!r.is_nan() && r == 10.0);
    }

    #[test]
    fn width_constraints_resolve_em_minimums_against_the_elements_own_font_size() {
        // .em has font-size: 20px and min-width: 3em → 60px, NOT 3 × 16px.
        let dom = constraints_dom();
        assert_eq!(width_constrained(&dom, EM, 10.0, 800.0), 60.0);
    }

    #[test]
    fn width_constraints_never_return_nan() {
        // A NaN tentative width is sanitized by the final `.max(min_width)`
        // (f32::max drops NaN), so nothing downstream can see a NaN size.
        let dom = constraints_dom();
        assert_eq!(width_constrained(&dom, PLAIN, f32::NAN, 800.0), 0.0);
        // MAXED has a max-width, so the FIRST `.min(max_width)` absorbs the NaN (IEEE
        // 754: f32::min drops NaN) and lands on the max, before `.max(min_width)` ever
        // sees it. NaN doesn't always collapse to min_width -- it collapses to
        // whichever clamp touches it first.
        assert_eq!(width_constrained(&dom, MAXED, f32::NAN, 800.0), 100.0);
        assert_eq!(width_constrained(&dom, CLAMPED, f32::NAN, 800.0), 200.0);
    }

    #[test]
    fn width_constraints_handle_infinite_tentative_widths() {
        let dom = constraints_dom();
        assert_eq!(width_constrained(&dom, MAXED, f32::INFINITY, 800.0), 100.0);
        // No max-width → +inf survives (a definite size is never produced from
        // an infinite one, but the function must not panic or wrap).
        assert!(width_constrained(&dom, PLAIN, f32::INFINITY, 800.0).is_infinite());
        assert_eq!(width_constrained(&dom, CLAMPED, f32::NEG_INFINITY, 800.0), 200.0);
    }

    #[test]
    fn width_constraints_ignore_viewport_unit_minimums() {
        // KNOWN GAP: `resolve_px_with_box_model` cannot resolve vw/vh/vmin/vmax
        // (no viewport is threaded into it), so `min-width: 10vw` silently
        // defaults to 0 and never floors the width — even though `width: 10vw`
        // on the very same element DOES resolve (via
        // `resolve_pixel_value_no_percent_with_viewport`).
        let dom = constraints_dom();
        assert_eq!(width_constrained(&dom, VWMIN, 300.0, 800.0), 300.0);
        assert_eq!(
            width_constrained(&dom, VWMIN, 10.0, 800.0),
            10.0,
            "10vw (= 80px on an 800px viewport) does not floor the width"
        );
    }

    #[test]
    fn height_constraints_clamp_then_let_min_win_over_max() {
        let dom = constraints_dom();
        assert_eq!(height_constrained(&dom, HCLAMPED, 300.0, 600.0), 200.0);
        assert_eq!(height_constrained(&dom, PLAIN, 42.0, 600.0), 42.0);
    }

    #[test]
    fn height_constraints_never_return_nan() {
        let dom = constraints_dom();
        assert_eq!(height_constrained(&dom, PLAIN, f32::NAN, 600.0), 0.0);
        assert_eq!(height_constrained(&dom, HCLAMPED, f32::NAN, 600.0), 200.0);
    }

    #[test]
    fn height_constraints_are_stable_at_the_f32_boundaries() {
        let dom = constraints_dom();
        assert_eq!(height_constrained(&dom, HCLAMPED, f32::MAX, 600.0), 200.0);
        assert_eq!(height_constrained(&dom, HCLAMPED, f32::MIN, 600.0), 200.0);
        // The implicit min-height of 0 floors any negative tentative height,
        // so a negative size can never escape into layout.
        assert_eq!(height_constrained(&dom, PLAIN, f32::MIN, 600.0), 0.0);
        assert_eq!(height_constrained(&dom, PLAIN, -1.0, 600.0), 0.0);
        assert!(height_constrained(&dom, PLAIN, f32::MAX, 600.0).is_finite());
    }

    // ==================================================================
    // apply_constraint_violation_table  (numeric)
    // ==================================================================

    fn cvt(dom: &StyledDom, id: NodeId, w: f32, h: f32) -> (f32, f32) {
        let state = node_state(dom, id);
        apply_constraint_violation_table(dom, id, &state, w, h, 800.0, 600.0, &zero_props())
    }

    #[test]
    fn constraint_violation_table_row1_leaves_an_unviolated_box_alone() {
        let dom = constraints_dom();
        assert_eq!(cvt(&dom, PLAIN, 200.0, 100.0), (200.0, 100.0));
    }

    #[test]
    fn constraint_violation_table_row2_preserves_the_aspect_ratio_under_max_width() {
        // w=200 > max-width=100 → w := 100, h scaled by the same factor.
        let dom = constraints_dom();
        assert_eq!(cvt(&dom, MAXED, 200.0, 100.0), (100.0, 50.0));
    }

    #[test]
    fn constraint_violation_table_row10_pins_min_width_and_max_height_together() {
        // .row10: min-width 200, max-height 50. w=100 (< min) and h=100 (> max)
        // → the ratio cannot be preserved; the spec pins both constraints.
        let dom = constraints_dom();
        assert_eq!(cvt(&dom, ROW10, 100.0, 100.0), (200.0, 50.0));
    }

    #[test]
    fn constraint_violation_table_guards_against_division_by_zero() {
        // The w<=0 / h<=0 guard must fire BEFORE any `w / h` division.
        let dom = constraints_dom();
        assert_eq!(cvt(&dom, MAXED, 0.0, 100.0), (0.0, 100.0));
        assert_eq!(cvt(&dom, MAXED, 200.0, 0.0), (100.0, 0.0));
        assert_eq!(cvt(&dom, MAXED, 0.0, 0.0), (0.0, 0.0));
        assert_eq!(cvt(&dom, MAXED, -50.0, -50.0), (0.0, 0.0), "negatives clamp up to 0");
    }

    #[test]
    fn constraint_violation_table_survives_extreme_ratios() {
        // A near-degenerate ratio (MAX / MIN_POSITIVE) must not produce NaN or
        // panic — the scaled dimension underflows to 0 and is then floored.
        let dom = constraints_dom();
        let (w, h) = cvt(&dom, MAXED, f32::MAX, f32::MIN_POSITIVE);
        assert_eq!(w, 100.0, "max-width still clamps");
        assert!(h.is_finite() && h >= 0.0, "scaled height stays finite: {h}");

        let (w, h) = cvt(&dom, MAXED, f32::MIN_POSITIVE, f32::MAX);
        assert!(w.is_finite() && w >= 0.0, "w={w}");
        assert!(h.is_finite() && h >= 0.0, "h={h}");
    }

    #[test]
    fn constraint_violation_table_is_idempotent() {
        // Re-applying the table to its own output must be a fixed point —
        // otherwise a re-layout would keep shrinking the box.
        let dom = constraints_dom();
        for (id, w, h) in [
            (MAXED, 200.0_f32, 100.0_f32),
            (ROW10, 100.0, 100.0),
            (PLAIN, 200.0, 100.0),
        ] {
            let first = cvt(&dom, id, w, h);
            let second = cvt(&dom, id, first.0, first.1);
            assert_eq!(first, second, "not a fixed point for {id:?}");
        }
    }
}
