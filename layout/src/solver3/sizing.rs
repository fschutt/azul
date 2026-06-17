//! Intrinsic and used size calculations for layout nodes

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
        geometry::{BoxProps, BoxSizing, IntrinsicSizes, WritingModeContext},
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
pub fn resolve_percentage_with_box_model(
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
    unsafe { crate::az_mark((0x607B0) as u32, (tree.nodes.len() as u32) as u32); }
    if dirty_nodes.is_empty() {
        return Ok(());
    }

    ctx.debug_log("Starting intrinsic size calculation");
    // Pre-compute the "ancestor closure" of dirty_nodes: every dirty
    // node AND each of its ancestors up to root. A node not in this
    // set (and whose `intrinsic_sizes` is already populated) can
    // reuse its cached intrinsic — we skip its entire subtree walk.
    // Before this, `calculate_intrinsic_recursive` walked the full
    // tree from root regardless, costing ~2 ms per warm render on
    // excel.html even when only 3 nodes were actually dirty.
    let dirty_closure = compute_dirty_ancestor_closure(tree, dirty_nodes);
    // [az-diag g59 REVERT] 0x407B4 = nodes.len AFTER compute_dirty_ancestor_closure (its HashSet sret).
    unsafe { crate::az_mark((0x607B4) as u32, (tree.nodes.len() as u32) as u32); }

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
        crate::az_mark((0x60730) as u32, (tree.root as u32) as u32);
        crate::az_mark((0x60734) as u32, (tree.nodes.len() as u32) as u32);
        crate::az_mark((0x60738) as u32, (tree.get(tree.root).is_some() as u32) as u32);
        // [az-diag g55] 0x4075C = the `tree` ptr the CALLEE sees. Compare with 0x40748
        // (caller's &new_tree). Same → nodes-field-offset mis-lift; differ → &mut arg mis-passed.
        crate::az_mark((0x6075C) as u32, ((tree as *const LayoutTree as usize) as u32) as u32);
    }
    calculator.calculate_intrinsic_recursive(tree, tree.root, false)?;
    ctx.debug_log("Finished intrinsic size calculation");
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
    /// stages 1–3 of the inline layout pipeline (logical / BiDi / shaping)
    /// are cache-hits across the sizing pass's min/max-content measurements
    /// AND the subsequent real layout pass. Previously each pass held its
    /// own `LayoutCache`, so identical text was shaped three times per
    /// root_layout_pass — once per min-content measurement, once per
    /// max-content measurement, once at final layout.
    text_cache: &'c mut LayoutCache,
    /// If `Some`, only nodes in this set (the ancestor-closure of
    /// dirty nodes) need recomputation. A clean node whose
    /// `warm.intrinsic_sizes` is already populated reuses the
    /// cached value and skips its entire subtree descent.
    dirty_closure: Option<std::collections::HashSet<usize>>,
}

impl<'a, 'b, 'c, T: ParsedFontTrait> IntrinsicSizeCalculator<'a, 'b, 'c, T> {
    fn new(ctx: &'a mut LayoutContext<'b, T>, text_cache: &'c mut LayoutCache) -> Self {
        Self {
            ctx,
            text_cache,
            dirty_closure: None,
        }
    }

    fn calculate_intrinsic_recursive(
        &mut self,
        tree: &mut LayoutTree,
        node_index: usize,
        ancestor_is_stf: bool,
    ) -> Result<IntrinsicSizes> {
        // [az-diag g52 REVERT] 0x40720 = node_index entering calculate_intrinsic_recursive
        // (last value after the run = the node that InvalidTree'd or the stray child).
        unsafe { crate::az_mark((0x60720) as u32, (node_index as u32) as u32); }
        // Fast path: if this subtree has no dirty nodes AND we
        // already have a cached intrinsic, return the cached value
        // and skip the whole descent. Caller is the ancestor-closure
        // computation in `calculate_intrinsic_sizes` — anything not
        // in that set is guaranteed clean through every descendant.
        if let Some(closure) = self.dirty_closure.as_ref() {
            if !closure.contains(&node_index) {
                if let Some(cached) = tree
                    .warm(node_index)
                    .and_then(|w| w.intrinsic_sizes.clone())
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
                .map(|v| !v)
                .unwrap_or(false)
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
            .map(|n| {
                crate::solver3::layout_tree::is_shrink_to_fit_context(
                    self.ctx.styled_dom,
                    n.dom_node_id,
                    &n.formatting_context,
                )
            })
            .unwrap_or(false);
        let child_ancestor_is_stf = ancestor_is_stf || self_is_stf;

        let mut child_intrinsics = Vec::with_capacity(n);
        for &child_index in children {
            // [az-diag g52 REVERT] 0x40728 = child_index about to recurse (last = the stray).
            unsafe { crate::az_mark((0x60728) as u32, (child_index as u32) as u32); }
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
            use azul_css::props::basic::{pixel::{DEFAULT_FONT_SIZE, PT_TO_PX}, SizeMetric};
            use crate::solver3::getters::{get_css_min_width, get_css_min_height, MultiValue};

            let node_state = &self.ctx.styled_dom.styled_nodes.as_container()[dom_id].styled_node_state;

            if let MultiValue::Exact(mw) = get_css_min_width(self.ctx.styled_dom, dom_id, node_state) {
                let px = &mw.inner;
                let resolved = match px.metric {
                    SizeMetric::Px => Some(px.number.get()),
                    SizeMetric::Pt => Some(px.number.get() * PT_TO_PX),
                    SizeMetric::In => Some(px.number.get() * 96.0),
                    SizeMetric::Cm => Some(px.number.get() * 96.0 / 2.54),
                    SizeMetric::Mm => Some(px.number.get() * 96.0 / 25.4),
                    SizeMetric::Em | SizeMetric::Rem => Some(px.number.get() * DEFAULT_FONT_SIZE),
                    _ => None, // percentages are not <length>
                };
                if let Some(min_w) = resolved {
                    intrinsic.min_content_width = intrinsic.min_content_width.max(min_w);
                    intrinsic.max_content_width = intrinsic.max_content_width.max(min_w);
                }
            }

            if let MultiValue::Exact(mh) = get_css_min_height(self.ctx.styled_dom, dom_id, node_state) {
                let px = &mh.inner;
                let resolved = match px.metric {
                    SizeMetric::Px => Some(px.number.get()),
                    SizeMetric::Pt => Some(px.number.get() * PT_TO_PX),
                    SizeMetric::In => Some(px.number.get() * 96.0),
                    SizeMetric::Cm => Some(px.number.get() * 96.0 / 2.54),
                    SizeMetric::Mm => Some(px.number.get() * 96.0 / 25.4),
                    SizeMetric::Em | SizeMetric::Rem => Some(px.number.get() * DEFAULT_FONT_SIZE),
                    _ => None,
                };
                if let Some(min_h) = resolved {
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
                        .map(|dom_id| {
                            let node_data = &self.ctx.styled_dom.node_data.as_container()[dom_id];
                            // Text nodes are inline-level
                            if matches!(node_data.get_node_type(), NodeType::Text(_)) {
                                return false;
                            }
                            let display = get_display_type(self.ctx.styled_dom, dom_id);
                            display.creates_block_context()
                        })
                        .unwrap_or(false)
                });

                let has_inline_child = tree.children(node_index).iter().any(|&child_idx| {
                    tree.get(child_idx)
                        .and_then(|c| c.dom_node_id)
                        .map(|dom_id| {
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
                        .unwrap_or(false)
                });

                // IFC root only if there are inline children and NO block children.
                // If there are block children, text nodes get anonymous block wrappers.
                let is_ifc_root = has_inline_child && !has_block_child;
                
                // Also check if this block has direct text content (text nodes in DOM)
                // but ONLY if there are no block-level layout children
                let has_direct_text = if !has_block_child {
                    if let Some(dom_id) = node.dom_node_id {
                        let node_hierarchy = &self.ctx.styled_dom.node_hierarchy.as_container();
                        dom_id.az_children(node_hierarchy).any(|child_id| {
                            let child_node_data = &self.ctx.styled_dom.node_data.as_container()[child_id];
                            matches!(child_node_data.get_node_type(), NodeType::Text(_))
                        })
                    } else {
                        false
                    }
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
                        .map(|c| matches!(c.formatting_context, FormattingContext::Inline))
                        .unwrap_or(false)
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
                self.calculate_table_intrinsic_sizes(tree, node_index, child_intrinsics)
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
    fn calculate_ifc_root_intrinsic_sizes(
        &mut self,
        tree: &LayoutTree,
        node_index: usize,
    ) -> Result<IntrinsicSizes> {
        // [g75] 0x60758 = how many times this IFC sizer is entered; 0x6075C = node_index of THIS call.
        unsafe {
            let c = crate::az_mark_read(0x60758).wrapping_add(1);
            crate::az_mark((0x60758) as u32, (c) as u32);
            crate::az_mark((0x6075C) as u32, (node_index as u32) as u32);
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
        let collect_result = collect_inline_content(&mut self.ctx, tree, node_index);
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
        let constraints = UnifiedConstraints::default();
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
        let intrinsic_text = match self.text_cache.measure_intrinsic_widths(
            &inline_content,
            &[],
            &constraints,
            &self.ctx.font_manager.font_chain_cache,
            &self.ctx.font_manager.fc_cache,
            &loaded_fonts,
            self.ctx.debug_messages,
        ) {
            Ok(r) => r,
            Err(_) => {
                return Ok(IntrinsicSizes {
                    min_content_width: 100.0,
                    max_content_width: 300.0,
                    preferred_width: None,
                    min_content_height: 20.0,
                    max_content_height: 20.0,
                    preferred_height: None,
                });
            }
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
        &mut self,
        tree: &LayoutTree,
        node_index: usize,
        child_intrinsics: &[(usize, IntrinsicSizes)],
    ) -> Result<IntrinsicSizes> {
        let node = tree.get(node_index).ok_or(LayoutError::InvalidTree)?;
        let writing_mode = if let Some(dom_id) = node.dom_node_id {
            let node_state =
                &self.ctx.styled_dom.styled_nodes.as_container()[dom_id].styled_node_state;
            get_writing_mode(self.ctx.styled_dom, dom_id, node_state).unwrap_or_default()
        } else {
            LayoutWritingMode::default()
        };

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
                    if let Some(cn) = child_node {
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
                    } else {
                        (0.0, 0.0, 0.0, 0.0)
                    };

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
        &mut self,
        tree: &LayoutTree,
        node_index: usize,
        child_intrinsics: &[(usize, IntrinsicSizes)],
    ) -> Result<IntrinsicSizes> {
        let node = tree.get(node_index).ok_or(LayoutError::InvalidTree)?;

        // Determine flex-direction to know if main axis is horizontal or vertical
        let is_row = if let Some(dom_id) = node.dom_node_id {
            let node_state =
                &self.ctx.styled_dom.styled_nodes.as_container()[dom_id].styled_node_state;
            match get_flex_direction(self.ctx.styled_dom, dom_id, &node_state) {
                MultiValue::Exact(dir) => matches!(dir, LayoutFlexDirection::Row | LayoutFlexDirection::RowReverse),
                _ => true, // default is row
            }
        } else {
            true // default flex-direction is row
        };

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
        let is_single_line = if let Some(dom_id) = node.dom_node_id {
            let node_state =
                &self.ctx.styled_dom.styled_nodes.as_container()[dom_id].styled_node_state;
            let wrap_prop = crate::solver3::getters::get_flex_wrap_prop(
                self.ctx.styled_dom, dom_id, &node_state,
            );
            match wrap_prop {
                Some(val) => matches!(
                    val.get_property_or_default().unwrap_or_default(),
                    LayoutFlexWrap::NoWrap
                ),
                None => true, // default is nowrap
            }
        } else {
            true
        };

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
    ) -> Result<IntrinsicSizes> {
        // Collect per-column min/max widths and total row heights.
        // Table structure: table > row-group? > row > cell
        let mut col_min: Vec<f32> = Vec::new();
        let mut col_max: Vec<f32> = Vec::new();
        let mut total_height = 0.0f32;

        // Iterate rows — children may be row groups (thead/tbody/tfoot) or direct rows
        let mut rows: Vec<usize> = Vec::new();
        for &child_idx in tree.children(node_index) {
            let child = match tree.get(child_idx) { Some(c) => c, None => continue };
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
            let mut col = 0usize;
            for &cell_idx in tree.children(row_idx) {
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
                let (h_extras, v_extras) = if let Some(cn) = cell_node {
                    let bp = cn.box_props.unpack();
                    (bp.padding.left + bp.padding.right + bp.border.left + bp.border.right,
                     bp.padding.top + bp.padding.bottom + bp.border.top + bp.border.bottom)
                } else { (0.0, 0.0) };

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
                col += 1;
            }
            total_height += row_height;
        }

        let min_width: f32 = col_min.iter().sum();
        let max_width: f32 = col_max.iter().sum();

        Ok(IntrinsicSizes {
            min_content_width: min_width,
            max_content_width: max_width,
            min_content_height: total_height,
            max_content_height: total_height,
            preferred_width: None,
            preferred_height: None,
        })
    }
}

/// Gathers all inline content for the intrinsic sizing pass.
///
/// This function recursively collects text and inline-level content according to
/// CSS Sizing Level 3, Section 4.1: "Intrinsic Sizes"
/// https://www.w3.org/TR/css-sizing-3/#intrinsic-sizes
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
    ctx.debug_log(&format!(
        "Collecting inline content from node {} for intrinsic sizing",
        ifc_root_index
    ));

    // [g78] fill the caller's out-param (was a local Vec returned by value → Ok→Err mis-lift).
    // Recursively collect inline content from this node and its inline descendants
    collect_inline_content_recursive(ctx, tree, ifc_root_index, out)?;
    // [g73] B8 = top-level recursion returned Ok (collect_inline_content complete).
    unsafe { crate::az_mark((0x6071C) as u32, (0xB8u32) as u32); }
    ctx.debug_log(&format!(
        "Collected {} inline content items from node {}",
        out.len(),
        ifc_root_index
    ));

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
    unsafe { crate::az_mark((0x60754) as u32, (node_index as u32) as u32); }
    let node = match tree.get(node_index) {
        Some(n) => n,
        None => {
            unsafe { crate::az_mark((0x6071C) as u32, (0xBADu32) as u32); }
            return Err(LayoutError::InvalidTree);
        }
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
        ctx.debug_log(&format!("Found text in node {}: '{}'", node_index, text));
        // Use split_text_for_whitespace to correctly handle white-space: pre with \n
        let text_items = split_text_for_whitespace(
            ctx.styled_dom,
            dom_id,
            &text,
            style_props,
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
            ctx.debug_log(&format!(
                "Found text in DOM child of node {}: '{}'",
                node_index, text
            ));
            // Use split_text_for_whitespace to correctly handle white-space: pre with \n
            let text_items = split_text_for_whitespace(
                ctx.styled_dom,
                child_id,
                &text,
                style_props,
            );
            content.extend(text_items);
        }
    }
    // [g73] B6 = DOM-children loop done (about to process_layout_children).
    unsafe { crate::az_mark((0x6071C) as u32, (0xB6u32) as u32); }

    process_layout_children(ctx, tree, node_index, content)
}

/// Helper to process layout tree children for inline content collection
fn process_layout_children<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    tree: &LayoutTree,
    node_index: usize,
    content: &mut Vec<InlineContent>,
) -> Result<()> {
    use azul_css::props::basic::SizeMetric;
    use azul_css::props::layout::{LayoutHeight, LayoutWidth};

    // [g73] PLC entry: 0x60708 = 0xC0<<24 | node_index (which node's children we process).
    unsafe { crate::az_mark((0x60708) as u32, (0xC0000000u32 | (node_index as u32 & 0xFFFFFF)) as u32); }
    // Process layout tree children (these are elements with layout properties)
    for &child_index in tree.children(node_index) {
        // [g73] PLC loop: 0x6070C = current child_index being processed.
        unsafe { crate::az_mark((0x6070C) as u32, (child_index as u32) as u32); }
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
            ctx.debug_log(&format!(
                "Recursing into inline child at node {}",
                child_index
            ));
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
                    // Convert PixelValue to f32
                    use azul_css::props::basic::pixel::{DEFAULT_FONT_SIZE, PT_TO_PX};
                    match px.metric {
                        SizeMetric::Px => px.number.get(),
                        SizeMetric::Pt => px.number.get() * PT_TO_PX,
                        SizeMetric::In => px.number.get() * 96.0,
                        SizeMetric::Cm => px.number.get() * 96.0 / 2.54,
                        SizeMetric::Mm => px.number.get() * 96.0 / 25.4,
                        SizeMetric::Em | SizeMetric::Rem => px.number.get() * DEFAULT_FONT_SIZE,
                        // +spec:containing-block:495930 - percentages in intrinsic sizing fall back to intrinsic contribution (css-sizing-3 §5.2.1)
                        // For percentages and viewport units, fall back to intrinsic
                        // +spec:containing-block:5246c0 - cyclic percentage: when containing block size depends on this box's intrinsic contribution, percentages fall back to intrinsic size
                        // +spec:containing-block:598124 - cyclic percentage contributions use intrinsic size
                        // +spec:height-calculation:ca9f19 - percentage-sized boxes use intrinsic size as contribution during intrinsic sizing
                        // +spec:width-calculation:7a384a - percentage-sized boxes behave as width:auto for intrinsic contributions (cyclic percentage)
                        _ => intrinsic_sizes.max_content_width,
                    }
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
                    use azul_css::props::basic::pixel::{DEFAULT_FONT_SIZE, PT_TO_PX};
                    match px.metric {
                        SizeMetric::Px => px.number.get(),
                        SizeMetric::Pt => px.number.get() * PT_TO_PX,
                        SizeMetric::In => px.number.get() * 96.0,
                        SizeMetric::Cm => px.number.get() * 96.0 / 2.54,
                        SizeMetric::Mm => px.number.get() * 96.0 / 25.4,
                        SizeMetric::Em | SizeMetric::Rem => px.number.get() * DEFAULT_FONT_SIZE,
                        // +spec:containing-block:7d5e79 - percentages behave as auto when containing block height is auto (cyclic percentage contribution)
                        // +spec:height-calculation:7d807b - css-sizing-3 §5.2.1: percentage heights behave as auto during intrinsic sizing (cyclic percentage contribution)
                        // Percentages and viewport units fall back to intrinsic (treated as auto)
                        _ => intrinsic_sizes.max_content_height,
                    }
                }
                // is equivalent to automatic size
                MultiValue::Exact(LayoutHeight::MinContent) => intrinsic_sizes.max_content_height,
                // is equivalent to automatic size
                MultiValue::Exact(LayoutHeight::MaxContent) => intrinsic_sizes.max_content_height,
                MultiValue::Exact(LayoutHeight::FitContent(_)) => intrinsic_sizes.max_content_height,
                _ => intrinsic_sizes.max_content_height,
            };

            ctx.debug_log(&format!(
                "Found atomic inline child at node {}: display={:?}, intrinsic_width={}, used_width={}, css_width={:?}",
                child_index, display, intrinsic_sizes.max_content_width, used_width, css_width
            ));

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
/// M12.7: out-of-line auto-width-block inline size — `(cb.width - margins - borders -
/// padding).max(0.0)`. Extracted from calc_used_size's auto-width Block arm so the
/// `.max(0.0)` runs in a small fn (proven to lift correctly), with a FRESH pointer
/// deref (the huge calc_used_size body hoists/spills cb.width and the remill lift then
/// reads it back 0). Returns by f32 (D0/V0 — the standard scalar return), NOT an out-ptr:
/// the out-ptr version computed 800 correctly but the caller's reload was opt-forwarded
/// to the init 0.0 across the opaque call (the helper's `*out` lowers to a direct
/// linear-mem store not modeled as aliasing the caller's slot). The f32 return is the
/// call's SSA result, which opt cannot replace. (The earlier "f32-return mis-lift" worry
/// was the 2×f32 *struct* HFA — a single scalar f32 return is fine.)
#[inline(never)]
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
    _box_props: &BoxProps,
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
                    - _box_props.margin.left
                    - _box_props.margin.right
                    - _box_props.border.left
                    - _box_props.border.right
                    - _box_props.padding.left
                    - _box_props.padding.right)
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
                    - _box_props.margin.left
                    - _box_props.margin.right
                    - _box_props.border.left
                    - _box_props.border.right
                    - _box_props.padding.left
                    - _box_props.padding.right)
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
                    auto_block_inline_size(containing_block_size, _box_props)
                }
                LayoutDisplay::InlineBlock | LayoutDisplay::InlineGrid | LayoutDisplay::InlineFlex => {
                    // +spec:width-calculation:c01de8 - inline-block auto width uses shrink-to-fit (§10.3.9)
                    // shrink-to-fit = min(max(preferred_minimum, available), preferred)
                    let available_width = (containing_block_size.width
                        - _box_props.margin.left
                        - _box_props.margin.right
                        - _box_props.border.left
                        - _box_props.border.right
                        - _box_props.padding.left
                        - _box_props.padding.right)
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
                            - _box_props.margin.left
                            - _box_props.margin.right
                            - _box_props.border.left
                            - _box_props.border.right
                            - _box_props.padding.left
                            - _box_props.padding.right)
                            .max(0.0)
                    }
                }
                // Other display types use intrinsic sizing
                _ => intrinsic.max_content_width,
            }
            }
        }
        LayoutWidth::Px(px) => {
            // Resolve percentage or absolute pixel value
            use azul_css::props::basic::{
                pixel::{DEFAULT_FONT_SIZE, PT_TO_PX},
                SizeMetric,
            };
            let pixels_opt = match px.metric {
                SizeMetric::Px => Some(px.number.get()),
                SizeMetric::Pt => Some(px.number.get() * PT_TO_PX),
                SizeMetric::In => Some(px.number.get() * 96.0),
                SizeMetric::Cm => Some(px.number.get() * 96.0 / 2.54),
                SizeMetric::Mm => Some(px.number.get() * 96.0 / 25.4),
                SizeMetric::Em | SizeMetric::Rem => Some(px.number.get() * DEFAULT_FONT_SIZE),
                SizeMetric::Vw => Some(px.number.get() / 100.0 * viewport_size.width),
                SizeMetric::Vh => Some(px.number.get() / 100.0 * viewport_size.height),
                SizeMetric::Vmin => Some(px.number.get() / 100.0 * viewport_size.width.min(viewport_size.height)),
                SizeMetric::Vmax => Some(px.number.get() / 100.0 * viewport_size.width.max(viewport_size.height)),
                SizeMetric::Percent => None,
            };

            match pixels_opt {
                Some(pixels) => pixels,
                None => match px.to_percent() {
                    Some(p) => {
                        let result = resolve_percentage_with_box_model(
                            containing_block_size.width,
                            p.get(),
                            (_box_props.margin.left, _box_props.margin.right),
                            (_box_props.border.left, _box_props.border.right),
                            (_box_props.padding.left, _box_props.padding.right),
                        );

                        result
                    }
                    None => intrinsic.max_content_width,
                },
            }
        }
        // +spec:intrinsic-sizing:069c75 - min-content, max-content, fit-content() sizing value keywords
        // +spec:intrinsic-sizing:1ce4fa - §3.2 min-content/max-content/fit-content() sizing values
        LayoutWidth::MinContent => intrinsic.min_content_width,
        LayoutWidth::MaxContent => intrinsic.max_content_width,
        // +spec:width-calculation:7b2128 - fit-content formula and non-negative inner size flooring (css-sizing-3 §3.2)
        // +spec:width-calculation:bf694a - min-content, max-content, fit-content() sizing values
        // css-sizing-3 §3.2: fit-content(<length-percentage>) = min(max-content, max(min-content, <length-percentage>))
        LayoutWidth::FitContent(px) => {
            use azul_css::props::basic::pixel::DEFAULT_FONT_SIZE;
            let arg = super::calc::resolve_pixel_value_with_viewport(
                &px, containing_block_size.width, DEFAULT_FONT_SIZE, DEFAULT_FONT_SIZE,
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
                    styled_dom, dom_id, *containing_block_size,
                );
                match (off.top, off.bottom) {
                    (Some(t), Some(b)) => Some(
                        (containing_block_size.height
                            - t
                            - b
                            - _box_props.margin.top
                            - _box_props.margin.bottom)
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
            // Resolve percentage or absolute pixel value
            use azul_css::props::basic::{
                pixel::{DEFAULT_FONT_SIZE, PT_TO_PX},
                SizeMetric,
            };
            let pixels_opt = match px.metric {
                SizeMetric::Px => Some(px.number.get()),
                SizeMetric::Pt => Some(px.number.get() * PT_TO_PX),
                SizeMetric::In => Some(px.number.get() * 96.0),
                SizeMetric::Cm => Some(px.number.get() * 96.0 / 2.54),
                SizeMetric::Mm => Some(px.number.get() * 96.0 / 25.4),
                SizeMetric::Em | SizeMetric::Rem => Some(px.number.get() * DEFAULT_FONT_SIZE),
                SizeMetric::Vw => Some(px.number.get() / 100.0 * viewport_size.width),
                SizeMetric::Vh => Some(px.number.get() / 100.0 * viewport_size.height),
                SizeMetric::Vmin => Some(px.number.get() / 100.0 * viewport_size.width.min(viewport_size.height)),
                SizeMetric::Vmax => Some(px.number.get() / 100.0 * viewport_size.width.max(viewport_size.height)),
                SizeMetric::Percent => None,
            };

            match pixels_opt {
                Some(pixels) => pixels,
                // +spec:height-calculation:37bc8c - percentage heights resolve against definite containing block height
                None => match px.to_percent() {
                    Some(p) => resolve_percentage_with_box_model(
                        containing_block_size.height,
                        p.get(),
                        (_box_props.margin.top, _box_props.margin.bottom),
                        (_box_props.border.top, _box_props.border.bottom),
                        (_box_props.padding.top, _box_props.padding.bottom),
                    ),
                    None => intrinsic.max_content_height,
                },
            }
        }
        // equivalent to automatic size (not min_content_height which is height at min-content width)
        LayoutHeight::MinContent => intrinsic.max_content_height,
        // equivalent to automatic size
        LayoutHeight::MaxContent => intrinsic.max_content_height,
        // css-sizing-3 §3.2: fit-content(<length-percentage>) = min(max-content, max(min-content, <length-percentage>))
        // For block axis, both min-content and max-content equal auto height
        LayoutHeight::FitContent(px) => {
            use azul_css::props::basic::pixel::DEFAULT_FONT_SIZE;
            let arg = super::calc::resolve_pixel_value_with_viewport(
                &px, containing_block_size.height, DEFAULT_FONT_SIZE, DEFAULT_FONT_SIZE,
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
        let has_intrinsic_width = intrinsic.preferred_width.map_or(false, |w| w > 0.0);
        let has_intrinsic_height = intrinsic.preferred_height.map_or(false, |h| h > 0.0);
        let intrinsic_ratio = match (intrinsic.preferred_width, intrinsic.preferred_height) {
            (Some(iw), Some(ih)) if ih > 0.0 => Some(iw / ih),
            _ => None,
        };

        if let Some(ratio) = intrinsic_ratio {
            if height_is_auto && !has_intrinsic_width && has_intrinsic_height {
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
                    - _box_props.margin.left
                    - _box_props.margin.right
                    - _box_props.border.left
                    - _box_props.border.right
                    - _box_props.padding.left
                    - _box_props.padding.right)
                    .max(0.0);
                (block_width, block_width / ratio)
            } else {
                (resolved_width, resolved_height)
            }
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
            _box_props,
        )
    } else {
        // Non-replaced element: apply width and height constraints independently
        let cw = apply_width_constraints(
            styled_dom,
            id,
            node_state,
            resolved_width,
            containing_block_size.width,
            _box_props,
        );

        let ch = apply_height_constraints(
            styled_dom,
            id,
            node_state,
            resolved_height,
            containing_block_size.height,
            _box_props,
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
            let min_border_box_w = _box_props.padding.left
                + _box_props.padding.right
                + _box_props.border.left
                + _box_props.border.right;
            let min_border_box_h = _box_props.padding.top
                + _box_props.padding.bottom
                + _box_props.border.top
                + _box_props.border.bottom;
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
                    + _box_props.padding.left
                    + _box_props.padding.right
                    + _box_props.border.left
                    + _box_props.border.right
            };
            let bh = if height_is_quantitative {
                constrained_height.max(min_border_box_h)
            } else {
                constrained_height
                    + _box_props.padding.top
                    + _box_props.padding.bottom
                    + _box_props.border.top
                    + _box_props.border.bottom
            };
            (bw, bh)
        }
        azul_css::props::layout::LayoutBoxSizing::ContentBox => {
            // +spec:box-sizing:fead70 - content-box: width/height set content size, border+padding added outside
            let border_box_width = constrained_width
                + _box_props.padding.left
                + _box_props.padding.right
                + _box_props.border.left
                + _box_props.border.right;
            let border_box_height = constrained_height
                + _box_props.padding.top
                + _box_props.padding.bottom
                + _box_props.border.top
                + _box_props.border.bottom;
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
    use azul_css::props::basic::{
        pixel::{DEFAULT_FONT_SIZE, PT_TO_PX},
        SizeMetric,
    };
    use crate::solver3::getters::{
        get_css_min_width, get_css_max_width, get_css_min_height, get_css_max_height, MultiValue,
    };

    // Helper to resolve a pixel value to f32
    fn resolve_px(px: &azul_css::props::basic::pixel::PixelValue, containing: f32, box_props: &BoxProps, is_horizontal: bool) -> Option<f32> {
        let pixels_opt = match px.metric {
            SizeMetric::Px => Some(px.number.get()),
            SizeMetric::Pt => Some(px.number.get() * PT_TO_PX),
            SizeMetric::In => Some(px.number.get() * 96.0),
            SizeMetric::Cm => Some(px.number.get() * 96.0 / 2.54),
            SizeMetric::Mm => Some(px.number.get() * 96.0 / 25.4),
            SizeMetric::Em | SizeMetric::Rem => Some(px.number.get() * DEFAULT_FONT_SIZE),
            SizeMetric::Percent => None,
            _ => None,
        };
        match pixels_opt {
            Some(v) => Some(v),
            None => {
                px.to_percent().map(|p| {
                    let (m1, m2, b1, b2, p1, p2) = if is_horizontal {
                        (box_props.margin.left, box_props.margin.right,
                         box_props.border.left, box_props.border.right,
                         box_props.padding.left, box_props.padding.right)
                    } else {
                        (box_props.margin.top, box_props.margin.bottom,
                         box_props.border.top, box_props.border.bottom,
                         box_props.padding.top, box_props.padding.bottom)
                    };
                    resolve_percentage_with_box_model(containing, p.get(), (m1, m2), (b1, b2), (p1, p2))
                })
            }
        }
    }

    // +spec:min-max-sizing:92ab8d - constraint violation table for replaced elements with intrinsic ratio (cyclic percentage contributions use auto fallback)
    // +spec:min-max-sizing:ad8605 - min-height/max-height interact with percentage heights; percentages behave as auto in intrinsic contribution calc

    // +spec:positioning:c0af55 - automatic minimum size of abspos box is always zero (default 0.0)
    // Resolve min-width (default 0)
    let min_w = match get_css_min_width(styled_dom, id, node_state) {
        MultiValue::Exact(mw) => resolve_px(&mw.inner, containing_block_width, box_props, true).unwrap_or(0.0),
        _ => 0.0,
    };

    // Resolve max-width (default infinity)
    let max_w = match get_css_max_width(styled_dom, id, node_state) {
        MultiValue::Exact(mw) => {
            if mw.inner.number.get() >= core::f32::MAX - 1.0 {
                f32::MAX
            } else {
                resolve_px(&mw.inner, containing_block_width, box_props, true).unwrap_or(f32::MAX)
            }
        }
        _ => f32::MAX,
    };

    // Resolve min-height (default 0)
    let min_h = match get_css_min_height(styled_dom, id, node_state) {
        MultiValue::Exact(mh) => resolve_px(&mh.inner, containing_block_height, box_props, false).unwrap_or(0.0),
        _ => 0.0,
    };

    // Resolve max-height (default infinity)
    let max_h = match get_css_max_height(styled_dom, id, node_state) {
        MultiValue::Exact(mh) => {
            if mh.inner.number.get() >= core::f32::MAX - 1.0 {
                f32::MAX
            } else {
                resolve_px(&mh.inner, containing_block_height, box_props, false).unwrap_or(f32::MAX)
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
    use azul_css::props::basic::{
        pixel::{DEFAULT_FONT_SIZE, PT_TO_PX},
        SizeMetric,
    };

    use crate::solver3::getters::{get_css_max_width, get_css_min_width, MultiValue};

    // +spec:display-property:0c55e5 - auto min-width resolves to 0 for CSS2 display types
    // Resolve min-width (default is 0)
    let min_width = match get_css_min_width(styled_dom, id, node_state) {
        MultiValue::Exact(mw) => {
            let px = &mw.inner;
            let pixels_opt = match px.metric {
                SizeMetric::Px => Some(px.number.get()),
                SizeMetric::Pt => Some(px.number.get() * PT_TO_PX),
                SizeMetric::In => Some(px.number.get() * 96.0),
                SizeMetric::Cm => Some(px.number.get() * 96.0 / 2.54),
                SizeMetric::Mm => Some(px.number.get() * 96.0 / 25.4),
                SizeMetric::Em | SizeMetric::Rem => Some(px.number.get() * DEFAULT_FONT_SIZE),
                SizeMetric::Percent => None,
                _ => None,
            };

            match pixels_opt {
                Some(pixels) => pixels,
                None => px
                    .to_percent()
                    .map(|p| {
                        resolve_percentage_with_box_model(
                            containing_block_width,
                            p.get(),
                            (box_props.margin.left, box_props.margin.right),
                            (box_props.border.left, box_props.border.right),
                            (box_props.padding.left, box_props.padding.right),
                        )
                    })
                    .unwrap_or(0.0),
            }
        }
        _ => 0.0,
    };

    // Resolve max-width (default is infinity/none)
    let max_width = match get_css_max_width(styled_dom, id, node_state) {
        MultiValue::Exact(mw) => {
            let px = &mw.inner;
            // Check if it's the default "max" value (f32::MAX)
            if px.number.get() >= core::f32::MAX - 1.0 {
                None
            } else {
                let pixels_opt = match px.metric {
                    SizeMetric::Px => Some(px.number.get()),
                    SizeMetric::Pt => Some(px.number.get() * PT_TO_PX),
                    SizeMetric::In => Some(px.number.get() * 96.0),
                    SizeMetric::Cm => Some(px.number.get() * 96.0 / 2.54),
                    SizeMetric::Mm => Some(px.number.get() * 96.0 / 25.4),
                    SizeMetric::Em | SizeMetric::Rem => Some(px.number.get() * DEFAULT_FONT_SIZE),
                    SizeMetric::Percent => None,
                    _ => None,
                };

                match pixels_opt {
                    Some(pixels) => Some(pixels),
                    None => px.to_percent().map(|p| {
                        resolve_percentage_with_box_model(
                            containing_block_width,
                            p.get(),
                            (box_props.margin.left, box_props.margin.right),
                            (box_props.border.left, box_props.border.right),
                            (box_props.padding.left, box_props.padding.right),
                        )
                    }),
                }
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

    result = result.max(min_width);

    result
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
    use azul_css::props::basic::{
        pixel::{DEFAULT_FONT_SIZE, PT_TO_PX},
        SizeMetric,
    };

    use crate::solver3::getters::{get_css_max_height, get_css_min_height, MultiValue};

    // for backwards-compat with CSS2 display types (block, inline, inline-block, table)
    // Resolve min-height (default is 0)
    let min_height = match get_css_min_height(styled_dom, id, node_state) {
        MultiValue::Exact(mh) => {
            let px = &mh.inner;
            let pixels_opt = match px.metric {
                SizeMetric::Px => Some(px.number.get()),
                SizeMetric::Pt => Some(px.number.get() * PT_TO_PX),
                SizeMetric::In => Some(px.number.get() * 96.0),
                SizeMetric::Cm => Some(px.number.get() * 96.0 / 2.54),
                SizeMetric::Mm => Some(px.number.get() * 96.0 / 25.4),
                SizeMetric::Em | SizeMetric::Rem => Some(px.number.get() * DEFAULT_FONT_SIZE),
                SizeMetric::Percent => None,
                _ => None,
            };

            match pixels_opt {
                Some(pixels) => pixels,
                None => px
                    .to_percent()
                    .map(|p| {
                        resolve_percentage_with_box_model(
                            containing_block_height,
                            p.get(),
                            (box_props.margin.top, box_props.margin.bottom),
                            (box_props.border.top, box_props.border.bottom),
                            (box_props.padding.top, box_props.padding.bottom),
                        )
                    })
                    .unwrap_or(0.0),
            }
        }
        _ => 0.0,
    };

    // Resolve max-height (default is infinity/none)
    let max_height = match get_css_max_height(styled_dom, id, node_state) {
        MultiValue::Exact(mh) => {
            let px = &mh.inner;
            // Check if it's the default "max" value (f32::MAX)
            if px.number.get() >= core::f32::MAX - 1.0 {
                None
            } else {
                let pixels_opt = match px.metric {
                    SizeMetric::Px => Some(px.number.get()),
                    SizeMetric::Pt => Some(px.number.get() * PT_TO_PX),
                    SizeMetric::In => Some(px.number.get() * 96.0),
                    SizeMetric::Cm => Some(px.number.get() * 96.0 / 2.54),
                    SizeMetric::Mm => Some(px.number.get() * 96.0 / 25.4),
                    SizeMetric::Em | SizeMetric::Rem => Some(px.number.get() * DEFAULT_FONT_SIZE),
                    SizeMetric::Percent => None,
                    _ => None,
                };

                match pixels_opt {
                    Some(pixels) => Some(pixels),
                    None => px.to_percent().map(|p| {
                        resolve_percentage_with_box_model(
                            containing_block_height,
                            p.get(),
                            (box_props.margin.top, box_props.margin.bottom),
                            (box_props.border.top, box_props.border.bottom),
                            (box_props.padding.top, box_props.padding.bottom),
                        )
                    }),
                }
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

    result = result.max(min_height);

    result
}

pub fn extract_text_from_node(styled_dom: &StyledDom, node_id: NodeId) -> Option<String> {
    match &styled_dom.node_data.as_container()[node_id].get_node_type() {
        NodeType::Text(text_data) => {
            Some(text_data.as_str().to_string())
        }
        _ => None,
    }
}
