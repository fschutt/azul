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
    styled_dom::StyledDom,
};
use azul_css::{
    css::CssPropertyValue,
    props::{
        basic::PixelValue,
        layout::{LayoutDisplay, LayoutHeight, LayoutPosition, LayoutWidth, LayoutWritingMode},
        property::{CssProperty, CssPropertyType},
    },
    LayoutDebugMessage,
};
use rust_fontconfig::FcFontCache;

use crate::{
    font::parsed::ParsedFont,
    solver3::{
        geometry::{BoxProps, BoxSizing, IntrinsicSizes},
        getters::{
            get_css_height, get_css_width, get_display_property, get_style_properties,
            get_writing_mode,
        },
        layout_tree::{AnonymousBoxType, LayoutNode, LayoutTree},
        positioning::get_position_type,
        LayoutContext, LayoutError, Result,
    },
    text3::cache::{
        FontLoaderTrait, FontManager, FontProviderTrait, ImageSource, InlineContent, InlineImage,
        InlineShape, LayoutCache, LayoutFragment, ObjectFit, ParsedFontTrait, ShapeDefinition,
        StyleProperties, StyledRun, UnifiedConstraints,
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
/// From CSS 2.1 Section 10.3.3 (Block-level, non-replaced elements in normal flow):
/// > 'margin-left' + 'border-left-width' + 'padding-left' + 'width' + 
/// > 'padding-right' + 'border-right-width' + 'margin-right' = width of containing block
///
/// This function ensures that when `width` is a percentage, it resolves to a value that
/// satisfies this constraint.
pub fn resolve_percentage_with_box_model(
    containing_block_dimension: f32,
    percentage: f32,
    margins: (f32, f32),
    borders: (f32, f32),
    paddings: (f32, f32),
) -> f32 {
    let available = containing_block_dimension 
        - margins.0 
        - margins.1
        - borders.0
        - borders.1
        - paddings.0
        - paddings.1;
    
    (percentage * available).max(0.0)
}

/// Phase 2a: Calculate intrinsic sizes (bottom-up pass)
pub fn calculate_intrinsic_sizes<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    ctx: &mut LayoutContext<T, Q>,
    tree: &mut LayoutTree<T>,
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

struct IntrinsicSizeCalculator<'a, 'b, T: ParsedFontTrait, Q: FontLoaderTrait<T>> {
    ctx: &'a mut LayoutContext<'b, T, Q>,
    text_cache: LayoutCache<T>,
}

impl<'a, 'b, T: ParsedFontTrait, Q: FontLoaderTrait<T>> IntrinsicSizeCalculator<'a, 'b, T, Q> {
    fn new(ctx: &'a mut LayoutContext<'b, T, Q>) -> Self {
        Self {
            ctx,
            text_cache: LayoutCache::new(),
        }
    }

    fn calculate_intrinsic_recursive(
        &mut self,
        tree: &mut LayoutTree<T>,
        node_index: usize,
    ) -> Result<IntrinsicSizes> {
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
        for &child_index in &node.children {
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
        tree: &LayoutTree<T>,
        node_index: usize,
        child_intrinsics: &BTreeMap<usize, IntrinsicSizes>,
    ) -> Result<IntrinsicSizes> {
        let node = tree.get(node_index).ok_or(LayoutError::InvalidTree)?;

        // IFrames are replaced elements with a default intrinsic size of 300x150px
        // (same as HTML <iframe> elements)
        if let Some(dom_id) = node.dom_node_id {
            let node_data = &self.ctx.styled_dom.node_data.as_container()[dom_id];
            if node_data.is_iframe_node() {
                return Ok(IntrinsicSizes {
                    min_content_width: 300.0,
                    max_content_width: 300.0,
                    preferred_width: None, // Will be determined by CSS or flex-grow
                    min_content_height: 150.0,
                    max_content_height: 150.0,
                    preferred_height: None, // Will be determined by CSS or flex-grow
                });
            }
        }

        match node.formatting_context {
            FormattingContext::Block { .. } => {
                self.calculate_block_intrinsic_sizes(tree, node_index, child_intrinsics)
            }
            FormattingContext::Inline => self.calculate_inline_intrinsic_sizes(tree, node_index),
            FormattingContext::Table => {
                self.calculate_table_intrinsic_sizes(tree, node_index, child_intrinsics)
            }
            _ => self.calculate_block_intrinsic_sizes(tree, node_index, child_intrinsics),
        }
    }

    fn calculate_block_intrinsic_sizes(
        &mut self,
        tree: &LayoutTree<T>,
        node_index: usize,
        child_intrinsics: &BTreeMap<usize, IntrinsicSizes>,
    ) -> Result<IntrinsicSizes> {
        let node = tree.get(node_index).ok_or(LayoutError::InvalidTree)?;
        let writing_mode = if let Some(dom_id) = node.dom_node_id {
            let node_state = &self.ctx.styled_dom.styled_nodes.as_container()[dom_id].state;
            get_writing_mode(self.ctx.styled_dom, dom_id, node_state).unwrap_or_default()
        } else {
            LayoutWritingMode::default()
        };

        // If there are no layout children but this block contains text content directly,
        // we need to calculate intrinsic sizes based on the text
        if child_intrinsics.is_empty() && node.dom_node_id.is_some() {
            let dom_id = node.dom_node_id.unwrap();
            let node_hierarchy = &self.ctx.styled_dom.node_hierarchy.as_container();
            
            // Check if this node has DOM children with text
            let has_text = dom_id
                .az_children(node_hierarchy)
                .any(|child_id| {
                    let child_node_data = &self.ctx.styled_dom.node_data.as_container()[child_id];
                    matches!(child_node_data.get_node_type(), NodeType::Text(_))
                });
            
            if has_text {
                self.ctx.debug_log(&format!(
                    "Block node {} has no layout children but has text DOM children - calculating as inline content",
                    node_index
                ));
                // This block contains inline content (text), so calculate its intrinsic size
                // using inline content measurement
                return self.calculate_inline_intrinsic_sizes(tree, node_index);
            }
        }

        let mut max_child_min_cross = 0.0f32;
        let mut max_child_max_cross = 0.0f32;
        let mut total_main_size = 0.0;

        for &child_index in &node.children {
            if let Some(child_intrinsic) = child_intrinsics.get(&child_index) {
                let (child_min_cross, child_max_cross, child_main_size) = match writing_mode {
                    LayoutWritingMode::HorizontalTb => (
                        child_intrinsic.min_content_width,
                        child_intrinsic.max_content_width,
                        child_intrinsic.max_content_height,
                    ),
                    _ => (
                        child_intrinsic.min_content_height,
                        child_intrinsic.max_content_height,
                        child_intrinsic.max_content_width,
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

    fn calculate_inline_intrinsic_sizes(
        &mut self,
        tree: &LayoutTree<T>,
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
                available_width: 0.0,
                ..Default::default()
            },
        }];

        let min_layout = match self.text_cache.layout_flow(
            &inline_content,
            &[],
            &min_fragments,
            self.ctx.font_manager,
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
                available_width: f32::INFINITY,
                ..Default::default()
            },
        }];

        let max_layout = match self.text_cache.layout_flow(
            &inline_content,
            &[],
            &max_fragments,
            self.ctx.font_manager,
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
        _tree: &LayoutTree<T>,
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
fn collect_inline_content_for_sizing<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    ctx: &mut LayoutContext<T, Q>,
    tree: &LayoutTree<T>,
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
fn collect_inline_content_recursive<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    ctx: &mut LayoutContext<T, Q>,
    tree: &LayoutTree<T>,
    node_index: usize,
    content: &mut Vec<InlineContent>,
) -> Result<()> {
    let node = tree.get(node_index).ok_or(LayoutError::InvalidTree)?;
    
    // CRITICAL FIX: Text nodes may exist in the DOM but not as separate layout nodes!
    // We need to check the DOM children for text content.
    if let Some(dom_id) = node.dom_node_id {
        // First check if THIS node is a text node
        if let Some(text) = extract_text_from_node(ctx.styled_dom, dom_id) {
            let style_props = get_style_properties(ctx.styled_dom, dom_id);
            ctx.debug_log(&format!(
                "Found text in node {}: '{}'",
                node_index, text
            ));
            content.push(InlineContent::Text(StyledRun {
                text,
                style: Arc::new(style_props),
                logical_start_byte: 0,
            }));
        }
        
        // CRITICAL: Also check DOM children for text nodes!
        // Text nodes are often not represented as separate layout nodes.
        let node_hierarchy = &ctx.styled_dom.node_hierarchy.as_container();
        for child_id in dom_id.az_children(node_hierarchy) {
            // Check if this DOM child is a text node
            let child_dom_node = &ctx.styled_dom.node_data.as_container()[child_id];
            if let NodeType::Text(text_data) = child_dom_node.get_node_type() {
                let text = text_data.as_str().to_string();
                let style_props = get_style_properties(ctx.styled_dom, child_id);
                ctx.debug_log(&format!(
                    "Found text in DOM child of node {}: '{}'",
                    node_index, text
                ));
                content.push(InlineContent::Text(StyledRun {
                    text,
                    style: Arc::new(style_props),
                    logical_start_byte: 0,
                }));
            }
        }
    }

    // Process layout tree children (these are elements with layout properties)
    for &child_index in &node.children {
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
            
            ctx.debug_log(&format!(
                "Found atomic inline child at node {}: display={:?}, intrinsic_width={}",
                child_index, display, intrinsic_sizes.max_content_width
            ));
            
            // Represent as a rectangular shape with the child's intrinsic dimensions
            content.push(InlineContent::Shape(InlineShape {
                shape_def: ShapeDefinition::Rectangle {
                    size: crate::text3::cache::Size {
                        width: intrinsic_sizes.max_content_width,
                        height: intrinsic_sizes.max_content_height,
                    },
                    corner_radius: None,
                },
                fill: None,
                stroke: None,
                baseline_offset: intrinsic_sizes.max_content_height,
            }));
        }
    }
    
    Ok(())
}

// Keep old name as an alias for backward compatibility
pub fn collect_inline_content<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    ctx: &mut LayoutContext<T, Q>,
    tree: &LayoutTree<T>,
    ifc_root_index: usize,
) -> Result<Vec<InlineContent>> {
    collect_inline_content_for_sizing(ctx, tree, ifc_root_index)
}

fn calculate_intrinsic_recursive<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    ctx: &mut LayoutContext<T, Q>,
    tree: &mut LayoutTree<T>,
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
    for &child_index in &node.children {
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

/// STUB: Calculates intrinsic sizes for a node based on its children
/// TODO: Implement proper intrinsic size calculation logic
fn calculate_node_intrinsic_sizes_stub<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    _ctx: &LayoutContext<T, Q>,
    _node: &LayoutNode<T>,
    child_intrinsics: &BTreeMap<usize, IntrinsicSizes>,
) -> IntrinsicSizes {
    // Simple stub: aggregate children's sizes
    let mut max_width: f32 = 0.0;
    let mut max_height: f32 = 0.0;
    let mut total_width: f32 = 0.0;
    let mut total_height: f32 = 0.0;

    for intrinsic in child_intrinsics.values() {
        max_width = max_width.max(intrinsic.max_content_width);
        max_height = max_height.max(intrinsic.max_content_height);
        total_width += intrinsic.max_content_width;
        total_height += intrinsic.max_content_height;
    }

    IntrinsicSizes {
        min_content_width: total_width.min(max_width),
        min_content_height: total_height.min(max_height),
        max_content_width: max_width.max(total_width),
        max_content_height: max_height.max(total_height),
        preferred_width: None,
        preferred_height: None,
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
) -> Result<LogicalSize> {
    eprintln!(
        "[calculate_used_size_for_node] dom_id={:?}, containing_block_size={:?}",
        dom_id, containing_block_size
    );

    let Some(id) = dom_id else {
        return Ok(LogicalSize::new(
            intrinsic.max_content_width,
            intrinsic.max_content_height,
        ));
    };

    let node_state = &styled_dom.styled_nodes.as_container()[id].state;
    let css_width = get_css_width(styled_dom, id, node_state);
    let css_height = get_css_height(styled_dom, id, node_state);
    let writing_mode = get_writing_mode(styled_dom, id, node_state);
    let display = get_display_property(styled_dom, Some(id));

    eprintln!(
        "[calculate_used_size_for_node] css_width={:?}, css_height={:?}, display={:?}",
        css_width, css_height, display
    );

    // Step 1: Resolve the CSS `width` property into a concrete pixel value.
    // Percentage values for `width` are resolved against the containing block's width.
    let resolved_width = match css_width.unwrap_or_default() {
        LayoutWidth::Auto => {
            // 'auto' width resolution depends on the display type.
            match display.unwrap_or_default() {
                LayoutDisplay::Block | LayoutDisplay::FlowRoot => {
                    // For block-level, non-replaced elements, 'auto' width fills the
                    // containing block (minus margins, borders, padding)
                    // CSS 2.1 Section 10.3.3: width = containing_block_width - margin_left - margin_right - border_left - border_right - padding_left - padding_right
                    let available_width = containing_block_size.width 
                        - _box_props.margin.left 
                        - _box_props.margin.right
                        - _box_props.border.left
                        - _box_props.border.right
                        - _box_props.padding.left
                        - _box_props.padding.right;
                    
                    eprintln!("[calculate_used_size_for_node] Auto width for block: containing_block={}, margins=({},{}), border=({},{}), padding=({},{}), available_width={}", 
                        containing_block_size.width, 
                        _box_props.margin.left, _box_props.margin.right,
                        _box_props.border.left, _box_props.border.right,
                        _box_props.padding.left, _box_props.padding.right,
                        available_width);
                    
                    available_width.max(0.0)
                },
                LayoutDisplay::Inline | LayoutDisplay::InlineBlock => {
                    // For inline-level elements, 'auto' width is the shrink-to-fit width,
                    // which is the max-content width
                    intrinsic.max_content_width
                },
                // Flex and Grid item sizing is handled by Taffy, not this function.
                _ => intrinsic.max_content_width,
            }
        },
        LayoutWidth::Px(px) => {
            // Resolve percentage or absolute pixel value
            use azul_css::props::basic::{SizeMetric, pixel::{PT_TO_PX, DEFAULT_FONT_SIZE}};
            let pixels_opt = match px.metric {
                SizeMetric::Px => Some(px.number.get()),
                SizeMetric::Pt => Some(px.number.get() * PT_TO_PX),
                SizeMetric::In => Some(px.number.get() * 96.0),
                SizeMetric::Cm => Some(px.number.get() * 96.0 / 2.54),
                SizeMetric::Mm => Some(px.number.get() * 96.0 / 25.4),
                SizeMetric::Em | SizeMetric::Rem => Some(px.number.get() * DEFAULT_FONT_SIZE),
                SizeMetric::Percent => None,
                SizeMetric::Vw | SizeMetric::Vh | SizeMetric::Vmin | SizeMetric::Vmax => None,
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
                        
                        eprintln!("[calculate_used_size_for_node] Percentage width: {}%, containing_block={}, margins=({},{}), border=({},{}), padding=({},{}), result={}", 
                            p.get(), 
                            containing_block_size.width, 
                            _box_props.margin.left, _box_props.margin.right,
                            _box_props.border.left, _box_props.border.right,
                            _box_props.padding.left, _box_props.padding.right,
                            result);
                        
                        result
                    },
                    None => intrinsic.max_content_width,
                },
            }
        }
        LayoutWidth::MinContent => intrinsic.min_content_width,
        LayoutWidth::MaxContent => intrinsic.max_content_width,
    };

    // Step 2: Resolve the CSS `height` property into a concrete pixel value.
    // Percentage values for `height` are resolved against the containing block's height.
    let resolved_height = match css_height.unwrap_or_default() {
        LayoutHeight::Auto => {
            // For 'auto' height, we initially use the intrinsic content height.
            // For block containers, this will be updated later in the layout process
            // after the children's heights are known.
            intrinsic.max_content_height
        },
        LayoutHeight::Px(px) => {
            // Resolve percentage or absolute pixel value
            use azul_css::props::basic::{SizeMetric, pixel::{PT_TO_PX, DEFAULT_FONT_SIZE}};
            let pixels_opt = match px.metric {
                SizeMetric::Px => Some(px.number.get()),
                SizeMetric::Pt => Some(px.number.get() * PT_TO_PX),
                SizeMetric::In => Some(px.number.get() * 96.0),
                SizeMetric::Cm => Some(px.number.get() * 96.0 / 2.54),
                SizeMetric::Mm => Some(px.number.get() * 96.0 / 25.4),
                SizeMetric::Em | SizeMetric::Rem => Some(px.number.get() * DEFAULT_FONT_SIZE),
                SizeMetric::Percent => None,
                SizeMetric::Vw | SizeMetric::Vh | SizeMetric::Vmin | SizeMetric::Vmax => None,
            };
            
            match pixels_opt {
                Some(pixels) => pixels,
                None => match px.to_percent() {
                    Some(p) => {
                        let result = resolve_percentage_with_box_model(
                            containing_block_size.height,
                            p.get(),
                            (_box_props.margin.top, _box_props.margin.bottom),
                            (_box_props.border.top, _box_props.border.bottom),
                            (_box_props.padding.top, _box_props.padding.bottom),
                        );
                        
                        eprintln!("[calculate_used_size_for_node] Percentage height: {}%, containing_block={}, margins=({},{}), border=({},{}), padding=({},{}), result={}", 
                            p.get(), 
                            containing_block_size.height, 
                            _box_props.margin.top, _box_props.margin.bottom,
                            _box_props.border.top, _box_props.border.bottom,
                            _box_props.padding.top, _box_props.padding.bottom,
                            result);
                        
                        result
                    },
                    None => intrinsic.max_content_height,
                },
            }
        }
        LayoutHeight::MinContent => intrinsic.min_content_height,
        LayoutHeight::MaxContent => intrinsic.max_content_height,
    };

    // Step 3: Map the resolved physical dimensions to logical dimensions.
    // The `width` property always corresponds to the cross (inline) axis size.
    // The `height` property always corresponds to the main (block) axis size.
    let cross_size = resolved_width;
    let main_size = resolved_height;

    // Step 4: Construct the final LogicalSize from the logical dimensions.
    let result = LogicalSize::from_main_cross(main_size, cross_size, writing_mode.unwrap_or_default());

    eprintln!(
        "[calculate_used_size_for_node] RESULT: {:?} (resolved_width={}, resolved_height={})",
        result, resolved_width, resolved_height
    );

    Ok(result)
}

fn collect_text_recursive<T: ParsedFontTrait>(
    tree: &LayoutTree<T>,
    node_index: usize,
    styled_dom: &StyledDom,
    content: &mut Vec<InlineContent>,
) {
    let node = match tree.get(node_index) {
        Some(n) => n,
        None => return,
    };

    // If this node has text content, add it
    if let Some(dom_id) = node.dom_node_id {
        if let Some(text) = extract_text_from_node(styled_dom, dom_id) {
            content.push(InlineContent::Text(StyledRun {
                text,
                style: std::sync::Arc::new(StyleProperties::default()),
                logical_start_byte: 0,
            }));
        }
    }

    // Recurse into children
    for &child_index in &node.children {
        collect_text_recursive(tree, child_index, styled_dom, content);
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_percentage_with_box_model_basic() {
        // 100% of 595px with no margins/borders/paddings should be 595px
        let result = resolve_percentage_with_box_model(
            595.0,
            1.0, // 100%
            (0.0, 0.0),
            (0.0, 0.0),
            (0.0, 0.0),
        );
        assert_eq!(result, 595.0);
    }

    #[test]
    fn test_resolve_percentage_with_box_model_with_margins() {
        // Body element: width: 100%, margin: 20px
        // Containing block (html): 595px wide
        // Expected: 595 - 20 - 20 = 555px
        let result = resolve_percentage_with_box_model(
            595.0,
            1.0, // 100%
            (20.0, 20.0),
            (0.0, 0.0),
            (0.0, 0.0),
        );
        assert_eq!(result, 555.0);
    }

    #[test]
    fn test_resolve_percentage_with_box_model_with_all_box_properties() {
        // Element with margin: 10px, border: 5px, padding: 8px
        // width: 100% of 500px container
        // Expected: 500 - 10 - 10 - 5 - 5 - 8 - 8 = 454px
        let result = resolve_percentage_with_box_model(
            500.0,
            1.0, // 100%
            (10.0, 10.0),
            (5.0, 5.0),
            (8.0, 8.0),
        );
        assert_eq!(result, 454.0);
    }

    #[test]
    fn test_resolve_percentage_with_box_model_50_percent() {
        // 50% of 600px with 20px margins on each side
        // Available: 600 - 20 - 20 = 560px
        // 50% of 560 = 280px
        let result = resolve_percentage_with_box_model(
            600.0,
            0.5, // 50%
            (20.0, 20.0),
            (0.0, 0.0),
            (0.0, 0.0),
        );
        assert_eq!(result, 280.0);
    }

    #[test]
    fn test_resolve_percentage_with_box_model_asymmetric() {
        // Asymmetric margins/borders/paddings
        // Container: 1000px
        // Left margin: 100px, Right margin: 50px
        // Left border: 10px, Right border: 20px
        // Left padding: 5px, Right padding: 15px
        // Available: 1000 - 100 - 50 - 10 - 20 - 5 - 15 = 800px
        // 100% = 800px
        let result = resolve_percentage_with_box_model(
            1000.0,
            1.0,
            (100.0, 50.0),
            (10.0, 20.0),
            (5.0, 15.0),
        );
        assert_eq!(result, 800.0);
    }

    #[test]
    fn test_resolve_percentage_with_box_model_negative_clamping() {
        // Edge case: margins larger than container
        // Should clamp to 0, not return negative
        let result = resolve_percentage_with_box_model(
            100.0,
            1.0,
            (60.0, 60.0), // Total margins = 120px > 100px container
            (0.0, 0.0),
            (0.0, 0.0),
        );
        assert_eq!(result, 0.0);
    }

    #[test]
    fn test_resolve_percentage_with_box_model_zero_percent() {
        // 0% should always give 0, regardless of margins
        let result = resolve_percentage_with_box_model(
            1000.0,
            0.0, // 0%
            (100.0, 100.0),
            (10.0, 10.0),
            (5.0, 5.0),
        );
        assert_eq!(result, 0.0);
    }
}
