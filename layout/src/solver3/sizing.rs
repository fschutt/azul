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
        layout::{LayoutDisplay, LayoutHeight, LayoutPosition, LayoutWidth, LayoutWritingMode},
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
            get_style_properties, get_writing_mode, MultiValue,
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
        tree: &LayoutTree,
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
            
            // Images are replaced elements - get intrinsic size from the ImageRef
            if let NodeType::Image(image_ref) = node_data.get_node_type() {
                let size = image_ref.get_size();
                let width = if size.width > 0.0 { size.width } else { 100.0 };
                let height = if size.height > 0.0 { size.height } else { 100.0 };
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
                let has_block_child = node.children.iter().any(|&child_idx| {
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

                let has_inline_child = node.children.iter().any(|&child_idx| {
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
                let has_inline_children = node.children.iter().any(|&child_idx| {
                    tree.get(child_idx)
                        .map(|c| matches!(c.formatting_context, FormattingContext::Inline))
                        .unwrap_or(false)
                });
                
                if has_inline_children {
                    // InlineBlock with inline children - measure as IFC root
                    let mut intrinsic = self.calculate_ifc_root_intrinsic_sizes(tree, node_index)?;
                    
                    // FIX: Add padding and border to the intrinsic size.
                    // The measurement above only accounts for the text content.
                    // Since this node is an InlineBlock, it is a box that includes its own chrome.
                    // We use the resolved box_props (resolved during tree generation).
                    let h_extras = node.box_props.padding.left + node.box_props.padding.right 
                                 + node.box_props.border.left + node.box_props.border.right;
                    let v_extras = node.box_props.padding.top + node.box_props.padding.bottom 
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

        // CSS Intrinsic & Extrinsic Sizing Module Level 3:
        // min-content height is the height when content is laid out at min-content width
        // max-content height is the height when content is laid out at max-content width
        // These can differ when text wraps differently at different widths.
        let min_content_height = min_layout
            .fragment_layouts
            .get("min")
            .map(|l| l.bounds().height)
            .unwrap_or(0.0);

        let max_content_height = max_layout
            .fragment_layouts
            .get("max")
            .map(|l| l.bounds().height)
            .unwrap_or(0.0);

        Ok(IntrinsicSizes {
            min_content_width: min_width,
            max_content_width: max_width,
            preferred_width: None,
            min_content_height,
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
        return process_layout_children(ctx, tree, node, content);
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

    process_layout_children(ctx, tree, node, content)
}

/// Helper to process layout tree children for inline content collection
fn process_layout_children<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    tree: &LayoutTree,
    node: &LayoutNode,
    content: &mut Vec<InlineContent>,
) -> Result<()> {
    use azul_css::props::basic::SizeMetric;
    use azul_css::props::layout::{LayoutHeight, LayoutWidth};

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
                MultiValue::Exact(LayoutWidth::MinContent) => intrinsic_sizes.min_content_width,
                MultiValue::Exact(LayoutWidth::MaxContent) => intrinsic_sizes.max_content_width,
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
                MultiValue::Exact(LayoutHeight::MinContent) => intrinsic_sizes.min_content_height,
                MultiValue::Exact(LayoutHeight::MaxContent) => intrinsic_sizes.max_content_height,
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
    let is_horizontal = match &_node.formatting_context {
        FormattingContext::Flex => {
            // Flex containers: check if the main axis is horizontal.
            // We don't have direct access to flex-direction here, but flex
            // defaults to row (horizontal). For a proper implementation we'd
            // query the style; for now, treat Flex as horizontal.
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

    for intrinsic in child_intrinsics.values() {
        max_width = max_width.max(intrinsic.max_content_width);
        max_height = max_height.max(intrinsic.max_content_height);
        total_width += intrinsic.max_content_width;
        total_height += intrinsic.max_content_height;
    }

    if is_horizontal {
        // Horizontal stacking: width is sum of children, height is tallest child
        IntrinsicSizes {
            min_content_width: max_width,   // narrowest word across all children
            min_content_height: max_height,
            max_content_width: total_width,  // all children side-by-side
            max_content_height: max_height,
            preferred_width: None,
            preferred_height: None,
        }
    } else {
        // Vertical stacking (Block): width is widest child, height is sum of children
        IntrinsicSizes {
            min_content_width: max_width,
            min_content_height: max_height,  // tallest single child as min
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

    // Step 1: Resolve the CSS `width` property into a concrete pixel value.
    // Percentage values for `width` are resolved against the containing block's width.
    let resolved_width = match css_width.unwrap_or_default() {
        LayoutWidth::Auto => {
            // 'auto' width resolution depends on the display type.
            match display.unwrap_or_default() {
                LayoutDisplay::Block
                | LayoutDisplay::FlowRoot
                | LayoutDisplay::ListItem
                | LayoutDisplay::Flex
                | LayoutDisplay::Grid => {
                    // For block-level elements (including flex and grid containers),
                    // 'auto' width fills the containing block (minus margins, borders, padding).
                    // CSS 2.1 Section 10.3.3: width = containing_block_width - margin_left -
                    // margin_right - border_left - border_right - padding_left - padding_right
                    //
                    // Note: Flex/Grid CONTAINERS behave like blocks for sizing purposes.
                    // Flex/Grid ITEMS have different sizing, but that's handled by Taffy
                    // during the formatting context layout, not here.
                    let available_width = containing_block_size.width
                        - _box_props.margin.left
                        - _box_props.margin.right
                        - _box_props.border.left
                        - _box_props.border.right
                        - _box_props.padding.left
                        - _box_props.padding.right;

                    available_width.max(0.0)
                }
                LayoutDisplay::Inline | LayoutDisplay::InlineBlock => {
                    // For inline-level elements, 'auto' width is the shrink-to-fit width,
                    // which is the max-content width
                    intrinsic.max_content_width
                }
                // Table and other display types use intrinsic sizing
                _ => intrinsic.max_content_width,
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
        LayoutWidth::MinContent => intrinsic.min_content_width,
        LayoutWidth::MaxContent => intrinsic.max_content_width,
        LayoutWidth::Calc(_) => intrinsic.max_content_width, // TODO: resolve calc
    };

    // Step 2: Resolve the CSS `height` property into a concrete pixel value.
    // Percentage values for `height` are resolved against the containing block's height.
    let resolved_height = match css_height.unwrap_or_default() {
        LayoutHeight::Auto => {
            // For 'auto' height, we initially use the intrinsic content height.
            // For block containers, this will be updated later in the layout process
            // after the children's heights are known.
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
        LayoutHeight::MinContent => intrinsic.min_content_height,
        LayoutHeight::MaxContent => intrinsic.max_content_height,
        LayoutHeight::Calc(_) => intrinsic.max_content_height, // TODO: resolve calc
    };

    // Step 3: Apply min/max constraints (CSS 2.2 § 10.4 and § 10.7)
    // "The tentative used width is calculated (without 'min-width' and 'max-width')
    // ...If the tentative used width is greater than 'max-width', the rules above are
    // applied again using the computed value of 'max-width' as the computed value for 'width'.
    // If the resulting width is smaller than 'min-width', the rules above are applied again
    // using the value of 'min-width' as the computed value for 'width'."

    let constrained_width = apply_width_constraints(
        styled_dom,
        id,
        node_state,
        resolved_width,
        containing_block_size.width,
        _box_props,
    );

    let constrained_height = apply_height_constraints(
        styled_dom,
        id,
        node_state,
        resolved_height,
        containing_block_size.height,
        _box_props,
    );

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
        // CSS spec: percentage resolves against containing block, NOT available space
        // Expected: 595px (margins are ignored for percentage resolution)
        let result = resolve_percentage_with_box_model(
            595.0,
            1.0, // 100%
            (20.0, 20.0),
            (0.0, 0.0),
            (0.0, 0.0),
        );
        assert_eq!(result, 595.0);
    }

    #[test]
    fn test_resolve_percentage_with_box_model_with_all_box_properties() {
        // Element with margin: 10px, border: 5px, padding: 8px
        // width: 100% of 500px container
        // CSS spec: percentage resolves against containing block
        // Expected: 500px (margins/borders/padding are ignored)
        let result = resolve_percentage_with_box_model(
            500.0,
            1.0, // 100%
            (10.0, 10.0),
            (5.0, 5.0),
            (8.0, 8.0),
        );
        assert_eq!(result, 500.0);
    }

    #[test]
    fn test_resolve_percentage_with_box_model_50_percent() {
        // 50% of 600px containing block
        // CSS spec: 50% of containing block = 300px
        // (margins don't affect percentage resolution)
        let result = resolve_percentage_with_box_model(
            600.0,
            0.5, // 50%
            (20.0, 20.0),
            (0.0, 0.0),
            (0.0, 0.0),
        );
        assert_eq!(result, 300.0);
    }

    #[test]
    fn test_resolve_percentage_with_box_model_asymmetric() {
        // Asymmetric margins/borders/paddings
        // Container: 1000px
        // CSS spec: percentage resolves against containing block
        // 100% of 1000px = 1000px (margins/borders/padding ignored)
        let result = resolve_percentage_with_box_model(
            1000.0,
            1.0,
            (100.0, 50.0),
            (10.0, 20.0),
            (5.0, 15.0),
        );
        assert_eq!(result, 1000.0);
    }

    #[test]
    fn test_resolve_percentage_with_box_model_negative_clamping() {
        // Edge case: margins larger than container
        // CSS spec: percentage still resolves against containing block
        // Result should still be 100px (100% of 100px)
        let result = resolve_percentage_with_box_model(
            100.0,
            1.0,
            (60.0, 60.0), // margins ignored for percentage resolution
            (0.0, 0.0),
            (0.0, 0.0),
        );
        assert_eq!(result, 100.0);
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
