// +spec:height-calculation-p022 - non-normative: extracted paragraph is spec document stylesheet (css-display-3.html), not normative height/margin rules
//! solver3/sizing.rs
//!
//! Pass 2: Sizing calculations (intrinsic and used sizes)

use std::{
    collections::{BTreeMap, BTreeSet},
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
        geometry::{BoxProps, BoxSizing, IntrinsicSizes},
        getters::{
            get_css_box_sizing, get_css_height, get_css_width, get_display_property,
            get_flex_direction, get_float, get_style_properties, get_writing_mode, MultiValue,
        },
        layout_tree::{AnonymousBoxType, LayoutNode, LayoutTree, get_display_type},
        positioning::get_position_type,
        LayoutContext, LayoutError, Result,
    },
};

/// Resolves a percentage value against an available size, accounting for the CSS box model.
///
/// According to CSS 2.1 Section 10.2, percentages are resolved against the containing block's
/// dimensions. However, when an element has margins, borders, or padding, these must be
/// subtracted from the containing block size to get the "available" space that the percentage
/// resolves against.
///
/// This is critical for correct layout calculations, especially when elements use percentage
/// widths/heights combined with margins. Without this adjustment, elements overflow their
/// containing blocks.
///
/// # Arguments
///
/// * `containing_block_dimension` - The full dimension of the containing block (width or height)
/// * `percentage` - The percentage value to resolve (e.g., 100% = 1.0, 50% = 0.5)
/// * `margins` - The two margins in the relevant axis (left+right for width, top+bottom for height)
/// * `borders` - The two borders in the relevant axis
/// * `paddings` - The two paddings in the relevant axis
///
/// # Returns
///
/// The resolved pixel value, which is:
/// `percentage * (containing_block_dimension - margins - borders - paddings)`
///
/// The result is clamped to a minimum of 0.0 to prevent negative sizes.
///
/// # Example
///
/// ```text
/// // Body element: width: 100%, margin: 20px
/// // Containing block (html): 595px wide
/// // Expected body width: 595 - 20 - 20 = 555px
///
/// let body_width = resolve_percentage_with_box_model(
///     595.0,           // containing block width
///     1.0,             // 100%
///     (20.0, 20.0),    // left and right margins
///     (0.0, 0.0),      // no borders
///     (0.0, 0.0),      // no paddings
/// );
/// assert_eq!(body_width, 555.0);
/// ```
///
/// # CSS Specification
///
/// From CSS 2.1 Section 10.2: "If the width is set to a percentage, it is calculated
/// with respect to the width of the generated box's containing block."
///
/// The percentage is resolved against the containing block dimension directly.
/// Margins, borders, and padding are NOT subtracted from the base for percentage
/// resolution in content-box sizing. They may cause overflow if the total exceeds
/// the containing block width.
// +spec:width-calculation-p016 - §10.2: percentage width resolved against containing block dimension
pub fn resolve_percentage_with_box_model(
    containing_block_dimension: f32,
    percentage: f32,
    _margins: (f32, f32),
    _borders: (f32, f32),
    _paddings: (f32, f32),
) -> f32 {
    // CSS 2.1 Section 10.2: percentages resolve against containing block,
    // not available space after margins/borders/padding
    (containing_block_dimension * percentage).max(0.0)
}

/// Phase 2a: Calculate intrinsic sizes (bottom-up pass)
// +spec:intrinsic-sizing-p047 - css-sizing-3 index: entry point for computing intrinsic sizes (§5.1 min-content, max-content)
pub fn calculate_intrinsic_sizes<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    tree: &mut LayoutTree,
    dirty_nodes: &BTreeSet<usize>,
) -> Result<()> {
    if dirty_nodes.is_empty() {
        return Ok(());
    }

    ctx.debug_log("Starting intrinsic size calculation");
    let mut calculator = IntrinsicSizeCalculator::new(ctx);
    calculator.calculate_intrinsic_recursive(tree, tree.root)?;
    ctx.debug_log("Finished intrinsic size calculation");
    Ok(())
}

struct IntrinsicSizeCalculator<'a, 'b, T: ParsedFontTrait> {
    ctx: &'a mut LayoutContext<'b, T>,
    text_cache: LayoutCache,
}

impl<'a, 'b, T: ParsedFontTrait> IntrinsicSizeCalculator<'a, 'b, T> {
    fn new(ctx: &'a mut LayoutContext<'b, T>) -> Self {
        Self {
            ctx,
            text_cache: LayoutCache::new(),
        }
    }

    fn calculate_intrinsic_recursive(
        &mut self,
        tree: &mut LayoutTree,
        node_index: usize,
    ) -> Result<IntrinsicSizes> {
        static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let count = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if count % 50 == 0 {}

        let node = tree
            .get(node_index)
            .cloned()
            .ok_or(LayoutError::InvalidTree)?;

        // Out-of-flow elements do not contribute to their parent's intrinsic size.
        let position = get_position_type(self.ctx.styled_dom, node.dom_node_id);
        if position == LayoutPosition::Absolute || position == LayoutPosition::Fixed {
            if let Some(n) = tree.get_mut(node_index) {
                n.intrinsic_sizes = Some(IntrinsicSizes::default());
            }
            return Ok(IntrinsicSizes::default());
        }

        // First, calculate children's intrinsic sizes
        let mut child_intrinsics = BTreeMap::new();
        let children = tree.children(node_index).to_vec();
        for &child_index in &children {
            let child_intrinsic = self.calculate_intrinsic_recursive(tree, child_index)?;
            child_intrinsics.insert(child_index, child_intrinsic);
        }

        // Then calculate this node's intrinsic size based on its children
        let intrinsic = self.calculate_node_intrinsic_sizes(tree, node_index, &child_intrinsics)?;

        if let Some(n) = tree.get_mut(node_index) {
            n.intrinsic_sizes = Some(intrinsic);
        }

        Ok(intrinsic)
    }

    fn calculate_node_intrinsic_sizes(
        &mut self,
        tree: &LayoutTree,
        node_index: usize,
        child_intrinsics: &BTreeMap<usize, IntrinsicSizes>,
    ) -> Result<IntrinsicSizes> {
        let node = tree.get(node_index).ok_or(LayoutError::InvalidTree)?;

        // +spec:width-calculation-p010 - §10.3.8: abs-pos replaced elements use inline replaced width rules (§10.3.2); intrinsic width used here
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
            
            // +spec:containing-block-p031 - §5.1 css-sizing-3: replaced element with aspect ratio
            // but no intrinsic size uses initial CB inline size
            // +spec:width-calculation-p010 - §10.3.8/§10.3.2: abs-pos replaced image width from intrinsic size
            // +spec:width-calculation-p006 - §10.3.2: inline replaced elements — intrinsic width/height/ratio cascading rules and 300x150 fallback
            // +spec:width-calculation-p007 - §10.3.2: inline replaced element width from intrinsic dimensions
            if let NodeType::Image(image_ref) = node_data.get_node_type() {
                let size = image_ref.get_size();
                // Per css-sizing-3 §5.1: "use an inline size matching the corresponding dimension
                // of the initial containing block and calculate the other dimension using the aspect ratio"
                // +spec:width-calculation-p006 - §10.3.2: if width+height both auto and intrinsic width exists, use intrinsic width
                // +spec:width-calculation-p006 - §10.3.2: if width auto, height auto, no intrinsic width but has intrinsic height+ratio → width = height * ratio
                // +spec:width-calculation-p006 - §10.3.2: fallback 300px (or 2:1 device-fit rectangle)
                let (width, height) = if size.width > 0.0 && size.height > 0.0 {
                    // +spec:width-calculation-p007 - §10.3.2: both intrinsic width and height available
                    (size.width, size.height)
                } else if size.width > 0.0 {
                    // +spec:width-calculation-p007 - §10.3.2: has intrinsic width, use it directly
                    (size.width, size.width / 2.0)
                } else if size.height > 0.0 {
                    // Has intrinsic height but no width — use initial CB inline dimension
                    (self.ctx.viewport_size.width, size.height)
                } else {
                    // No intrinsic dimensions — cap at 300x150 per CSS 2.2 §10.3.2
                    // +spec:width-calculation-p005 - §10.3.2: auto width with no intrinsic width fallback to 300px (2:1 ratio)
                    let w = self.ctx.viewport_size.width.min(300.0);
                    (w, w / 2.0)
                };
                return Ok(IntrinsicSizes {
                    min_content_width: width,
                    max_content_width: width,
                    preferred_width: Some(width),
                    min_content_height: height,
                    max_content_height: height,
                    preferred_height: Some(height),
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
                            // Block-level display values
                            matches!(display,
                                LayoutDisplay::Block
                                | LayoutDisplay::Flex
                                | LayoutDisplay::Grid
                                | LayoutDisplay::Table
                                | LayoutDisplay::ListItem
                                | LayoutDisplay::FlowRoot
                            )
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
                // - Checking if this node has direct text children (case 2)
                let is_text_node = if let Some(dom_id) = node.dom_node_id {
                    let node_data = &self.ctx.styled_dom.node_data.as_container()[dom_id];
                    matches!(node_data.get_node_type(), NodeType::Text(_))
                } else {
                    false
                };

                let has_direct_text_children = if let Some(dom_id) = node.dom_node_id {
                    let node_hierarchy = &self.ctx.styled_dom.node_hierarchy.as_container();
                    dom_id.az_children(node_hierarchy).any(|child_id| {
                        let child_node_data = &self.ctx.styled_dom.node_data.as_container()[child_id];
                        matches!(child_node_data.get_node_type(), NodeType::Text(_))
                    })
                } else {
                    false
                };
                
                if is_text_node || has_direct_text_children {
                    // Case 1 or 2: Text node or IFC root - measure inline content
                    self.calculate_ifc_root_intrinsic_sizes(tree, node_index)
                } else {
                    // Case 3: True inline element - measured by parent IFC root
                    Ok(IntrinsicSizes::default())
                }
            }
            FormattingContext::InlineBlock => {
                // Inline-block IS an atomic inline - it needs its own intrinsic size.
                // BUT, if the inline-block contains inline/text children, it's an IFC root
                // and we need to measure its inline content, not just aggregate child intrinsics.
                let has_inline_children = tree.children(node_index).iter().any(|&child_idx| {
                    tree.get(child_idx)
                        .map(|c| matches!(c.formatting_context, FormattingContext::Inline))
                        .unwrap_or(false)
                });
                
                if has_inline_children {
                    // InlineBlock with inline children - measure as IFC root
                    let mut intrinsic = self.calculate_ifc_root_intrinsic_sizes(tree, node_index)?;

                    // +spec:intrinsic-sizing-p024 - css-sizing-3 §2.2: intrinsic size contributions
                    // are based on the outer size of the box. Add margin, padding, and border
                    // to the content intrinsic size. Auto margins are treated as zero.
                    // +spec:intrinsic-sizing-p047 - css-sizing-3 §5.2.1: padding/border added to intrinsic contributions
                    let h_extras = node.box_props.margin.left + node.box_props.margin.right
                                 + node.box_props.padding.left + node.box_props.padding.right
                                 + node.box_props.border.left + node.box_props.border.right;
                    // +spec:height-calculation-p034 - §8.1: padding and border areas surround content area vertically
                    let v_extras = node.box_props.margin.top + node.box_props.margin.bottom
                                 + node.box_props.padding.top + node.box_props.padding.bottom
                                 + node.box_props.border.top + node.box_props.border.bottom;

                    intrinsic.min_content_width += h_extras;
                    intrinsic.max_content_width += h_extras;
                    intrinsic.min_content_height += v_extras;
                    intrinsic.max_content_height += v_extras;

                    Ok(intrinsic)
                } else {
                    // InlineBlock with block children - aggregate like block
                    self.calculate_block_intrinsic_sizes(tree, node_index, child_intrinsics)
                }
            }
            FormattingContext::Table => {
                self.calculate_table_intrinsic_sizes(tree, node_index, child_intrinsics)
            }
            // +spec:intrinsic-sizing-p027 - flex container intrinsic sizes per css-flexbox-1 §9.9.1
            FormattingContext::Flex => {
                self.calculate_flex_intrinsic_sizes(tree, node_index, child_intrinsics)
            }
            _ => self.calculate_block_intrinsic_sizes(tree, node_index, child_intrinsics),
        }
    }
    
    /// Calculate intrinsic sizes for an IFC root (a block containing inline content).
    /// This collects ALL inline descendants' text and measures it ONCE.
    fn calculate_ifc_root_intrinsic_sizes(
        &mut self,
        tree: &LayoutTree,
        node_index: usize,
    ) -> Result<IntrinsicSizes> {
        // Collect all inline content from this IFC root and its inline descendants
        let inline_content = collect_inline_content(&mut self.ctx, tree, node_index)?;



        if inline_content.is_empty() {
            return Ok(IntrinsicSizes::default());
        }

        // Get pre-loaded fonts from font manager
        let loaded_fonts = self.ctx.font_manager.get_loaded_fonts();

        // Layout with "min-content" constraints (wrap at every opportunity)
        let min_fragments = vec![LayoutFragment {
            id: "min".to_string(),
            constraints: UnifiedConstraints {
                available_width: AvailableSpace::MinContent,
                ..Default::default()
            },
        }];

        let min_layout = match self.text_cache.layout_flow(
            &inline_content,
            &[],
            &min_fragments,
            &self.ctx.font_manager.font_chain_cache,
            &self.ctx.font_manager.fc_cache,
            &loaded_fonts,
            self.ctx.debug_messages,
        ) {
            Ok(layout) => layout,
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

        // Layout with "max-content" constraints (infinite width, no wrapping)
        let max_fragments = vec![LayoutFragment {
            id: "max".to_string(),
            constraints: UnifiedConstraints {
                available_width: AvailableSpace::MaxContent,
                ..Default::default()
            },
        }];

        let max_layout = match self.text_cache.layout_flow(
            &inline_content,
            &[],
            &max_fragments,
            &self.ctx.font_manager.font_chain_cache,
            &self.ctx.font_manager.fc_cache,
            &loaded_fonts,
            self.ctx.debug_messages,
        ) {
            Ok(layout) => layout,
            Err(_) => min_layout.clone(),
        };

        let min_width = min_layout
            .fragment_layouts
            .get("min")
            .map(|l| l.bounds().width)
            .unwrap_or(0.0);

        let max_width = max_layout
            .fragment_layouts
            .get("max")
            .map(|l| l.bounds().width)
            .unwrap_or(0.0);

        // +spec:inline-block-p049 - css-sizing-3 §2.1: "The min-content size in the
        // block axis. For block containers, tables, and inline boxes, this is equivalent
        // to the max-content block size." So min_content_height = max_content_height.
        let max_content_height = max_layout
            .fragment_layouts
            .get("max")
            .map(|l| l.bounds().height)
            .unwrap_or(0.0);

        Ok(IntrinsicSizes {
            min_content_width: min_width,
            max_content_width: max_width,
            preferred_width: None,
            min_content_height: max_content_height,
            max_content_height,
            preferred_height: None,
        })
    }

    fn calculate_block_intrinsic_sizes(
        &mut self,
        tree: &LayoutTree,
        node_index: usize,
        child_intrinsics: &BTreeMap<usize, IntrinsicSizes>,
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

        let mut max_child_min_cross = 0.0f32;
        let mut max_child_max_cross = 0.0f32;
        let mut total_main_size = 0.0;

        for &child_index in tree.children(node_index) {
            if let Some(child_intrinsic) = child_intrinsics.get(&child_index) {
                // +spec:intrinsic-sizing-p024 - css-sizing-3 §2.2: intrinsic size contributions
                // are based on the outer size of the box (margin-box). Add the child's margin,
                // border, and padding to its intrinsic content size. Auto margins are treated
                // as zero for this purpose.
                let child_node = tree.get(child_index);
                let (cross_extras, main_extras) = if let Some(cn) = child_node {
                    let bp = &cn.box_props;
                    let h = bp.margin.left + bp.margin.right
                          + bp.border.left + bp.border.right
                          + bp.padding.left + bp.padding.right;
                    let v = bp.margin.top + bp.margin.bottom
                          + bp.border.top + bp.border.bottom
                          + bp.padding.top + bp.padding.bottom;
                    match writing_mode {
                        LayoutWritingMode::HorizontalTb => (h, v),
                        _ => (v, h),
                    }
                } else {
                    (0.0, 0.0)
                };

                let (child_min_cross, child_max_cross, child_main_size) = match writing_mode {
                    LayoutWritingMode::HorizontalTb => (
                        child_intrinsic.min_content_width + cross_extras,
                        child_intrinsic.max_content_width + cross_extras,
                        child_intrinsic.max_content_height + main_extras,
                    ),
                    _ => (
                        child_intrinsic.min_content_height + cross_extras,
                        child_intrinsic.max_content_height + cross_extras,
                        child_intrinsic.max_content_width + main_extras,
                    ),
                };

                max_child_min_cross = max_child_min_cross.max(child_min_cross);
                max_child_max_cross = max_child_max_cross.max(child_max_cross);
                total_main_size += child_main_size;
            }
        }

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

    // +spec:intrinsic-sizing-p027 - css-flexbox-1 §9.9.1: intrinsic sizes of flex containers
    // The max-content main size is the sum of items' max-content contributions.
    // The min-content main size of a single-line flex container is the sum of items'
    // min-content contributions. For multi-line, it is the largest min-content contribution.
    // Auto margins on flex items are treated as 0 for this computation.
    fn calculate_flex_intrinsic_sizes(
        &mut self,
        tree: &LayoutTree,
        node_index: usize,
        child_intrinsics: &BTreeMap<usize, IntrinsicSizes>,
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
            if let Some(child_intrinsic) = child_intrinsics.get(&child_index) {
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

                // +spec:intrinsic-sizing-p027 - max-content main size = sum of items' max-content contributions
                sum_main_max += child_main_max;
                // +spec:intrinsic-sizing-p027 - min-content main size (single-line) = sum of items' min-content contributions
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

        // +spec:intrinsic-sizing-p027 - single-line: min-content = sum; multi-line: min-content = largest item
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

    fn calculate_inline_intrinsic_sizes(
        &mut self,
        tree: &LayoutTree,
        node_index: usize,
    ) -> Result<IntrinsicSizes> {

        
        self.ctx.debug_log(&format!(
            "Calculating inline intrinsic sizes for node {}",
            node_index
        ));

        // This call is now valid because we added the function to fc.rs
        let inline_content = collect_inline_content(&mut self.ctx, tree, node_index)?;

        if inline_content.is_empty() {
            self.ctx
                .debug_log("No inline content found, returning default sizes");
            return Ok(IntrinsicSizes::default());
        }

        self.ctx.debug_log(&format!(
            "Found {} inline content items",
            inline_content.len()
        ));

        // Layout with "min-content" constraints (effectively zero width).
        // This forces all possible line breaks, giving the width of the longest unbreakable unit.
        let min_fragments = vec![LayoutFragment {
            id: "min".to_string(),
            constraints: UnifiedConstraints {
                available_width: AvailableSpace::MinContent,
                ..Default::default()
            },
        }];

        // Get pre-loaded fonts from font manager
        let loaded_fonts = self.ctx.font_manager.get_loaded_fonts();

        let min_layout = match self.text_cache.layout_flow(
            &inline_content,
            &[],
            &min_fragments,
            &self.ctx.font_manager.font_chain_cache,
            &self.ctx.font_manager.fc_cache,
            &loaded_fonts,
            self.ctx.debug_messages,
        ) {
            Ok(layout) => layout,
            Err(e) => {
                self.ctx.debug_log(&format!(
                    "Warning: Sizing failed during min-content layout: {:?}",
                    e
                ));
                self.ctx
                    .debug_log("Using fallback: returning default intrinsic sizes");
                // Return reasonable defaults instead of crashing
                return Ok(IntrinsicSizes {
                    min_content_width: 100.0, // Arbitrary fallback width
                    max_content_width: 300.0,
                    preferred_width: None,
                    min_content_height: 20.0, // Arbitrary fallback height
                    max_content_height: 20.0,
                    preferred_height: None,
                });
            }
        };

        // Layout with "max-content" constraints (infinite width).
        // This produces a single, long line, giving the natural width of the content.
        let max_fragments = vec![LayoutFragment {
            id: "max".to_string(),
            constraints: UnifiedConstraints {
                available_width: AvailableSpace::MaxContent,
                ..Default::default()
            },
        }];

        let max_layout = match self.text_cache.layout_flow(
            &inline_content,
            &[],
            &max_fragments,
            &self.ctx.font_manager.font_chain_cache,
            &self.ctx.font_manager.fc_cache,
            &loaded_fonts,
            self.ctx.debug_messages,
        ) {
            Ok(layout) => layout,
            Err(e) => {
                self.ctx.debug_log(&format!(
                    "Warning: Sizing failed during max-content layout: {:?}",
                    e
                ));
                self.ctx.debug_log("Using fallback from min-content layout");
                // If max-content fails but min-content succeeded, use min as fallback
                min_layout.clone()
            }
        };

        let min_width = min_layout
            .fragment_layouts
            .get("min")
            .map(|l| l.bounds().width)
            .unwrap_or(0.0);

        let max_width = max_layout
            .fragment_layouts
            .get("max")
            .map(|l| l.bounds().width)
            .unwrap_or(0.0);

        // The height is typically calculated at the max_content_width.
        let height = max_layout
            .fragment_layouts
            .get("max")
            .map(|l| l.bounds().height)
            .unwrap_or(0.0);

        Ok(IntrinsicSizes {
            min_content_width: min_width,
            max_content_width: max_width,
            preferred_width: None, // preferred_width comes from CSS, not content.
            min_content_height: height, // Height can change with width, but this is a common model.
            max_content_height: height,
            preferred_height: None,
        })
    }

    fn calculate_table_intrinsic_sizes(
        &self,
        _tree: &LayoutTree,
        _node_index: usize,
        _child_intrinsics: &BTreeMap<usize, IntrinsicSizes>,
    ) -> Result<IntrinsicSizes> {
        Ok(IntrinsicSizes::default())
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
) -> Result<Vec<InlineContent>> {
    ctx.debug_log(&format!(
        "Collecting inline content from node {} for intrinsic sizing",
        ifc_root_index
    ));

    let mut content = Vec::new();

    // Recursively collect inline content from this node and its inline descendants
    collect_inline_content_recursive(ctx, tree, ifc_root_index, &mut content)?;

    ctx.debug_log(&format!(
        "Collected {} inline content items from node {}",
        content.len(),
        ifc_root_index
    ));

    Ok(content)
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
    let node = tree.get(node_index).ok_or(LayoutError::InvalidTree)?;

    // CRITICAL FIX: Text nodes may exist in the DOM but not as separate layout nodes!
    // We need to check the DOM children for text content.
    let Some(dom_id) = node.dom_node_id else {
        // No DOM ID means this is a synthetic node, skip text extraction
        return process_layout_children(ctx, tree, node_index, content);
    };

    // First check if THIS node is a text node
    if let Some(text) = extract_text_from_node(ctx.styled_dom, dom_id) {
        let style_props = Arc::new(get_style_properties(ctx.styled_dom, dom_id, ctx.system_style.as_ref()));
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
            let style_props = Arc::new(get_style_properties(ctx.styled_dom, child_id, ctx.system_style.as_ref()));
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

    // Process layout tree children (these are elements with layout properties)
    for &child_index in tree.children(node_index) {
        let child_node = tree.get(child_index).ok_or(LayoutError::InvalidTree)?;
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
            let intrinsic_sizes = child_node.intrinsic_sizes.unwrap_or_default();

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
                        // For percentages and viewport units, fall back to intrinsic
                        _ => intrinsic_sizes.max_content_width,
                    }
                }
                // +spec:intrinsic-sizing-p014 - css-sizing-3 §3.2: min-content uses min-content size
                // +spec:intrinsic-sizing-p047 - css-sizing-3 §3.2: min-content/max-content sizing values for width
                MultiValue::Exact(LayoutWidth::MinContent) => intrinsic_sizes.min_content_width,
                // +spec:intrinsic-sizing-p014 - css-sizing-3 §3.2: max-content uses max-content size
                MultiValue::Exact(LayoutWidth::MaxContent) => intrinsic_sizes.max_content_width,
                MultiValue::Exact(LayoutWidth::FitContent(_)) => {
                    // During intrinsic sizing, fit-content resolves to max-content
                    intrinsic_sizes.max_content_width
                }
                // For Auto or other values, use intrinsic size
                _ => intrinsic_sizes.max_content_width,
            };

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
                        _ => intrinsic_sizes.max_content_height,
                    }
                }
                // +spec:intrinsic-sizing-p014 - css-sizing-3 §3.2: min-content for block size
                // is equivalent to automatic size
                // +spec:intrinsic-sizing-p047 - css-sizing-3 §3.2: min-content/max-content sizing values for height
                MultiValue::Exact(LayoutHeight::MinContent) => intrinsic_sizes.max_content_height,
                // +spec:intrinsic-sizing-p014 - css-sizing-3 §3.2: max-content for block size
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
    collect_inline_content_for_sizing(ctx, tree, ifc_root_index)
}

fn calculate_intrinsic_recursive<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    tree: &mut LayoutTree,
    node_index: usize,
) -> Result<IntrinsicSizes> {
    let node = tree
        .get(node_index)
        .cloned()
        .ok_or(LayoutError::InvalidTree)?;

    // Out-of-flow elements do not contribute to their parent's intrinsic size.
    let position = get_position_type(ctx.styled_dom, node.dom_node_id);
    if position == LayoutPosition::Absolute || position == LayoutPosition::Fixed {
        if let Some(n) = tree.get_mut(node_index) {
            n.intrinsic_sizes = Some(IntrinsicSizes::default());
        }
        return Ok(IntrinsicSizes::default());
    }

    // First, calculate children's intrinsic sizes
    let mut child_intrinsics = BTreeMap::new();
    let children = tree.children(node_index).to_vec();
    for &child_index in &children {
        let child_intrinsic = calculate_intrinsic_recursive(ctx, tree, child_index)?;
        child_intrinsics.insert(child_index, child_intrinsic);
    }

    // Then calculate this node's intrinsic size based on its children
    let intrinsic = calculate_node_intrinsic_sizes_stub(ctx, &node, &child_intrinsics);

    if let Some(n) = tree.get_mut(node_index) {
        n.intrinsic_sizes = Some(intrinsic.clone());
    }

    Ok(intrinsic)
}

/// Calculates intrinsic sizes for a node based on its children and formatting context.
///
/// CSS Intrinsic & Extrinsic Sizing (§ 4/5):
/// - **Block (vertical stacking):** The intrinsic width is the *maximum* child width
///   (children stack vertically, so the widest child determines the parent's width).
///   The intrinsic height is the *sum* of child heights.
/// - **Inline / Flex-row (horizontal stacking):** The intrinsic width is the *sum*
///   of child widths. The intrinsic height is the *maximum* child height.
/// - **Flex-column:** Same as block for width (max), height is sum.
fn calculate_node_intrinsic_sizes_stub<T: ParsedFontTrait>(
    _ctx: &LayoutContext<'_, T>,
    _node: &LayoutNode,
    child_intrinsics: &BTreeMap<usize, IntrinsicSizes>,
) -> IntrinsicSizes {
    use azul_core::dom::FormattingContext;

    // Determine stacking direction from the node's formatting context.
    // "horizontal" means children are laid out side-by-side (inline, flex-row).
    // "vertical" means children stack top-to-bottom (block, flex-column).
    // +spec:intrinsic-sizing-p027 - flex container intrinsic sizes (stub path, assumes row)
    // +spec:intrinsic-sizing-p044 - flex container intrinsic main size:
    // max-content = sum of items' max-content contributions;
    // min-content (single-line) = sum of items' min-content contributions;
    // min-content (multi-line) = largest of items' min-content contributions.
    // Auto margins on flex items treated as 0 for this computation.
    let is_horizontal = match &_node.formatting_context {
        FormattingContext::Flex => {
            // Flex containers: check if the main axis is horizontal.
            // NOTE: flex-direction not yet queried; assumes row (horizontal main axis).
            true
        }
        FormattingContext::Inline | FormattingContext::InlineBlock | FormattingContext::Grid => true,
        // Block, Table, and everything else stacks vertically
        _ => false,
    };

    let mut max_width: f32 = 0.0;
    let mut max_height: f32 = 0.0;
    let mut total_width: f32 = 0.0;
    let mut total_height: f32 = 0.0;
    // +spec:intrinsic-sizing-p044 - track min-content contributions separately
    let mut max_min_width: f32 = 0.0;
    let mut max_min_height: f32 = 0.0;
    let mut total_min_width: f32 = 0.0;
    let mut total_min_height: f32 = 0.0;

    for intrinsic in child_intrinsics.values() {
        max_width = max_width.max(intrinsic.max_content_width);
        max_height = max_height.max(intrinsic.max_content_height);
        total_width += intrinsic.max_content_width;
        total_height += intrinsic.max_content_height;
        max_min_width = max_min_width.max(intrinsic.min_content_width);
        max_min_height = max_min_height.max(intrinsic.min_content_height);
        total_min_width += intrinsic.min_content_width;
        total_min_height += intrinsic.min_content_height;
    }

    let is_flex = matches!(&_node.formatting_context, FormattingContext::Flex);

    if is_horizontal {
        // +spec:intrinsic-sizing-p044 - flex-row intrinsic main size (width):
        // max-content width = sum of items' max-content width contributions
        // min-content width: single-line flex = sum, multi-line = largest, inline = largest
        // NOTE: flex-wrap not yet queried; defaults to nowrap (single-line), so use sum for flex.
        let min_w = if is_flex {
            total_min_width  // single-line flex: sum of min-content contributions
        } else {
            max_min_width    // inline: largest min-content item
        };
        IntrinsicSizes {
            min_content_width: min_w,
            min_content_height: max_min_height,
            max_content_width: total_width,  // all children side-by-side
            max_content_height: max_height,
            preferred_width: None,
            preferred_height: None,
        }
    } else {
        // Vertical stacking (Block / flex-column)
        // +spec:intrinsic-sizing-p044 - flex-column intrinsic main size (height):
        // max-content height = sum of items' max-content height contributions
        // min-content height: single-line flex = sum, block = largest
        let min_h = if is_flex {
            total_min_height  // single-line flex-column: sum of min-content contributions
        } else {
            max_min_height    // block: tallest single child as min
        };
        IntrinsicSizes {
            min_content_width: max_min_width,
            min_content_height: min_h,
            max_content_width: max_width,    // widest child determines width
            max_content_height: total_height, // all children stacked
            preferred_width: None,
            preferred_height: None,
        }
    }
}

/// Calculates the used size of a single node based on its CSS properties and
/// the available space provided by its containing block.
///
/// This implementation correctly handles writing modes and percentage-based sizes
/// according to the CSS specification:
/// 1. `width` and `height` CSS properties are resolved to pixel values. Percentages are calculated
///    based on the containing block's PHYSICAL dimensions (`width` for `width`, `height` for
///    `height`), regardless of writing mode.
/// 2. The resolved physical `width` is then mapped to the node's logical CROSS size.
/// 3. The resolved physical `height` is then mapped to the node's logical MAIN size.
/// 4. A final `LogicalSize` is constructed from these logical dimensions.
pub fn calculate_used_size_for_node(
    styled_dom: &StyledDom,
    dom_id: Option<NodeId>,
    containing_block_size: LogicalSize,
    intrinsic: IntrinsicSizes,
    _box_props: &BoxProps,
    viewport_size: LogicalSize,
) -> Result<LogicalSize> {
    let Some(id) = dom_id else {
        // Anonymous boxes:
        // - Width fills the containing block (like block-level elements)
        // - Height is auto (content-based)
        // CSS 2.2 § 9.2.1.1: Anonymous boxes inherit from their enclosing box
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

    // +spec:width-calculation-p007 - §10.3: width/margin calculation depends on box type
    // Determine if this element is a replaced element (images, virtual views)
    let node_data = &styled_dom.node_data.as_container()[id];
    let is_replaced = matches!(node_data.get_node_type(), NodeType::Image(_))
        || node_data.is_virtual_view_node();

    // +spec:width-calculation-p008 - §10.3.1: the 'width' property does not apply to inline, non-replaced elements
    // For inline non-replaced elements, override any explicit width to Auto.
    let css_width = if display.unwrap_or_default() == LayoutDisplay::Inline
        && !is_replaced
    {
        MultiValue::Exact(LayoutWidth::Auto)
    } else {
        css_width
    };

    // Step 1: Resolve the CSS `width` property into a concrete pixel value.
    // +spec:width-calculation-p005 - §10.3.2/§10.3.3/§10.3.4/§10.3.5: width calculation dispatch by box type
    // Percentage values for `width` are resolved against the containing block's width.
    let resolved_width = match css_width.unwrap_or_default() {
        LayoutWidth::Auto => {
            // +spec:width-calculation-p005 - §10.3.2: inline replaced elements with auto width use intrinsic width
            // +spec:width-calculation-p005 - §10.3.4: block-level replaced elements use inline replaced width rules
            // CSS 2.2 §10.3.2: If 'width' has a computed value of 'auto', and the element
            // has an intrinsic width, then that intrinsic width is the used value of 'width'.
            // §10.3.4: "The used value of 'width' is determined as for inline replaced elements."
            if is_replaced {
                // For replaced elements (inline or block-level), auto width = intrinsic width.
                // The intrinsic sizes were already computed with the 300px fallback per §10.3.2.
                intrinsic.max_content_width
            }
            // +spec:width-calculation-p005 - §10.3.5: floating non-replaced elements with auto width use shrink-to-fit
            else if get_float(styled_dom, id, node_state).unwrap_or(LayoutFloat::None) != LayoutFloat::None {
                // CSS 2.2 §10.3.5: For floats, auto width = shrink-to-fit
                // shrink-to-fit = min(max(preferred minimum width, available width), preferred width)
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
            // +spec:width-calculation-p002 - §10.3.7: absolutely positioned non-replaced elements with auto width use shrink-to-fit
            else if matches!(position, LayoutPosition::Absolute | LayoutPosition::Fixed) {
                // §10.3.7: abs-pos elements with auto width use shrink-to-fit
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
            // 'auto' width resolution depends on the display type.
            match display.unwrap_or_default() {
                // +spec:width-calculation-p005 - §10.3.3: block-level non-replaced auto width fills containing block
                // +spec:width-calculation-p006 - §10.3.3: block-level non-replaced width:auto → solve constraint equation for width
                // +spec:width-calculation-p007 - §10.3.3: block-level non-replaced auto width = CB width - margins - borders - padding
                LayoutDisplay::Block
                | LayoutDisplay::FlowRoot
                | LayoutDisplay::ListItem
                | LayoutDisplay::Flex
                | LayoutDisplay::Grid => {
                    // For block-level non-replaced elements,
                    // 'auto' width fills the containing block (minus margins, borders, padding).
                    // CSS 2.2 §10.3.3: width = containing_block_width - margin_left -
                    // margin_right - border_left - border_right - padding_left - padding_right
                    // +spec:width-calculation-p005 - §10.3.3: constraint equation solved for width when width is auto, other auto values become 0
                    let available_width = containing_block_size.width
                        - _box_props.margin.left
                        - _box_props.margin.right
                        - _box_props.border.left
                        - _box_props.border.right
                        - _box_props.padding.left
                        - _box_props.padding.right;

                    available_width.max(0.0)
                }
                // +spec:width-calculation-p024 - §10.3.9: inline-block non-replaced auto width uses shrink-to-fit width as for floats
                // +spec:width-calculation-p001 - §10.3.9: inline-block non-replaced with auto width uses shrink-to-fit width
                // +spec:display-property-p047 - inline-grid in IFC is sized as atomic inline-level box (CSS Grid 5.2)
                // +spec:inline-block-p018 - inline-grid in IFC sized as atomic inline-level box (like inline-block) per CSS Grid 5.2
                // +spec:inline-block-p020 - inline-level grid/flex containers sized as atomic inline-level box per CSS Grid §5.2
                // +spec:inline-block-p043 - §10.3.9: inline-block non-replaced auto width uses shrink-to-fit
                LayoutDisplay::InlineBlock | LayoutDisplay::InlineGrid | LayoutDisplay::InlineFlex => {
                    // CSS 2.2 §10.3.9: If 'width' is 'auto', the used value is the
                    // shrink-to-fit width as for floating elements.
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
                // +spec:width-calculation-p001 - §10.3.10: inline-block replaced uses intrinsic width (same as inline replaced)
                // +spec:width-calculation-p006 - §10.3.2: inline replaced elements with auto width use intrinsic width (via max_content_width from intrinsic size pass)
                LayoutDisplay::Inline => {
                    // For inline elements, 'auto' width is the intrinsic/max-content width
                    intrinsic.max_content_width
                }
                // +spec:width-calculation-p017 - §17.5.2: table with width:auto does NOT fill containing block; uses intrinsic (max-content) width
                // +spec:width-calculation-p018 - §17.5.2: table with width:auto does NOT auto-fill containing block; uses intrinsic sizing instead
                LayoutDisplay::Table | LayoutDisplay::InlineTable => intrinsic.max_content_width,
                // Other display types use intrinsic sizing
                _ => intrinsic.max_content_width,
            }
            }
        }
        // +spec:width-calculation-p016 - §10.2: <length> specifies content area width; <percentage> w.r.t. containing block width
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
        // +spec:intrinsic-sizing-p014 - css-sizing-3 §3.2: min-content uses min-content size in relevant axis
        // +spec:intrinsic-sizing-p047 - css-sizing-3 §3.2: min-content/max-content keyword values for width property
        LayoutWidth::MinContent => intrinsic.min_content_width,
        // +spec:intrinsic-sizing-p014 - css-sizing-3 §3.2: max-content uses max-content size in relevant axis
        LayoutWidth::MaxContent => intrinsic.max_content_width,
        // css-sizing-3 §3.2: fit-content(<length-percentage>) = min(max-content, max(min-content, <length-percentage>))
        LayoutWidth::FitContent(px) => {
            use azul_css::props::basic::pixel::DEFAULT_FONT_SIZE;
            let arg = super::calc::resolve_pixel_value_with_viewport(
                &px, containing_block_size.width, DEFAULT_FONT_SIZE, DEFAULT_FONT_SIZE,
                viewport_size.width, viewport_size.height,
            );
            intrinsic.max_content_width.min(intrinsic.min_content_width.max(arg))
        }
        LayoutWidth::Calc(_) => intrinsic.max_content_width, // TODO: resolve calc
    };
    // css-sizing-3: "the used value is floored to preserve a non-negative inner size"
    let resolved_width = resolved_width.max(0.0);

    // Step 2: Resolve the CSS `height` property into a concrete pixel value.
    // Percentage values for `height` are resolved against the containing block's height.
    let resolved_height = match css_height.unwrap_or_default() {
        LayoutHeight::Auto => {
            // For 'auto' height, we initially use the intrinsic content height.
            // For block containers, this will be updated later in the layout process
            // after the children's heights are known.
            // +spec:display-property-p047 - grid container's auto block size is its max-content size (CSS Grid 5.2)
            // +spec:inline-block-p018 - grid container auto block size is its max-content size (CSS Grid 5.2)
            // +spec:inline-block-p020 - grid container auto block size is max-content size in both inline and block contexts (CSS Grid §5.2)
            intrinsic.max_content_height
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
        // +spec:intrinsic-sizing-p014 - css-sizing-3 §3.2: min-content for block size is
        // equivalent to automatic size (not min_content_height which is height at min-content width)
        // +spec:intrinsic-sizing-p047 - css-sizing-3 §3.2: min-content/max-content keyword values for height property
        LayoutHeight::MinContent => intrinsic.max_content_height,
        // +spec:intrinsic-sizing-p014 - css-sizing-3 §3.2: max-content for block size is
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
        LayoutHeight::Calc(_) => intrinsic.max_content_height, // TODO: resolve calc
    };
    // css-sizing-3: "the used value is floored to preserve a non-negative inner size"
    let resolved_height = resolved_height.max(0.0);

    // Step 3: Apply min/max constraints (CSS 2.2 § 10.4 and § 10.7)
    // "The tentative used width is calculated (without 'min-width' and 'max-width')
    // ...If the tentative used width is greater than 'max-width', the rules above are
    // applied again using the computed value of 'max-width' as the computed value for 'width'.
    // If the resulting width is smaller than 'min-width', the rules above are applied again
    // using the value of 'min-width' as the computed value for 'width'."

    // +spec:height-calculation-p043 - §10.4: for replaced elements with intrinsic ratios,
    // use the constraint violation table to coordinate width+height together;
    // for non-replaced elements, apply width and height constraints independently
    let has_intrinsic_ratio = intrinsic.preferred_width.is_some()
        && intrinsic.preferred_height.is_some()
        && intrinsic.preferred_width.unwrap_or(0.0) > 0.0
        && intrinsic.preferred_height.unwrap_or(0.0) > 0.0;

    let (constrained_width, constrained_height) = if has_intrinsic_ratio {
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

    // +spec:width-calculation-p042 - §8.1: width defines content edge; content-box adds padding+border outside, border-box includes them
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
            // border-box: The width/height values already include border and padding
            // CSS Box Sizing Level 3: "the specified width and height (and respective min/max
            // properties) on this element determine the border box of the element"
            (constrained_width, constrained_height)
        }
        azul_css::props::layout::LayoutBoxSizing::ContentBox => {
            // content-box: The width/height values set the content size,
            // border and padding are added outside
            // CSS 2.2 § 8.4: "The properties that apply to and affect box dimensions are:
            // margin, border, padding, width, and height."
            // +spec:width-calculation-p039 - §8.1: content-box width adds padding and border areas outside content area
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

    // Step 5: Map the resolved physical dimensions to logical dimensions.
    // The `width` property always corresponds to the cross (inline) axis size.
    // The `height` property always corresponds to the main (block) axis size.
    let cross_size = border_box_width;
    let main_size = border_box_height;

    // Step 6: Construct the final LogicalSize from the logical dimensions.
    let result =
        LogicalSize::from_main_cross(main_size, cross_size, writing_mode.unwrap_or_default());

    Ok(result)
}

// +spec:height-calculation-p047 - §10.4: constraint violation table for replaced elements
// with intrinsic ratios. Implements all 10 cases from the spec table, coordinating
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

    // +spec:height-calculation-p047 - §10.4: "Take the max-width and max-height as
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

    // +spec:intrinsic-sizing-p014 - css-sizing-3 §3.2: auto for min-width resolves to 0
    // for backwards-compat with CSS2 display types (block, inline, inline-block, table)
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

    // +spec:intrinsic-sizing-p014 - css-sizing-3 §3.2: none means no limit on box size
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

    // +spec:intrinsic-sizing-p014 - css-sizing-3 §3.2: auto for min-height resolves to 0
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
        NodeType::Text(text_data) => Some(text_data.as_str().to_string()),
        _ => None,
    }
}

fn debug_log(debug_messages: &mut Option<Vec<LayoutDebugMessage>>, message: &str) {
    if let Some(messages) = debug_messages {
        messages.push(LayoutDebugMessage::info(message));
    }
}
