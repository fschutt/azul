//! solver3/sizing.rs
//!
//! Pass 2: Sizing calculations (intrinsic and used sizes)

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use azul_core::{
    dom::{NodeId, NodeType},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::StyledDom,
    ui_solver::FormattingContext,
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
        &self,
        tree: &LayoutTree<T>,
        node_index: usize,
        child_intrinsics: &BTreeMap<usize, IntrinsicSizes>,
    ) -> Result<IntrinsicSizes> {
        let node = tree.get(node_index).ok_or(LayoutError::InvalidTree)?;
        let writing_mode = if let Some(dom_id) = node.dom_node_id {
            let node_state = &self.ctx.styled_dom.styled_nodes.as_container()[dom_id].state;
            get_writing_mode(self.ctx.styled_dom, dom_id, node_state)
        } else {
            LayoutWritingMode::default()
        };

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
            .map(|l| l.bounds.width)
            .unwrap_or(0.0);

        let max_width = max_layout
            .fragment_layouts
            .get("max")
            .map(|l| l.bounds.width)
            .unwrap_or(0.0);

        // The height is typically calculated at the max_content_width.
        let height = max_layout
            .fragment_layouts
            .get("max")
            .map(|l| l.bounds.height)
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
fn collect_inline_content_for_sizing<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    ctx: &mut LayoutContext<T, Q>,
    tree: &LayoutTree<T>,
    ifc_root_index: usize,
) -> Result<Vec<InlineContent>> {
    ctx.debug_log(&format!(
        "Collecting inline content from node {}",
        ifc_root_index
    ));

    let mut content = Vec::new();
    let ifc_root_node = tree.get(ifc_root_index).ok_or(LayoutError::InvalidTree)?;

    // Check if the root itself is a text node
    if let Some(dom_id) = ifc_root_node.dom_node_id {
        if let Some(text) = extract_text_from_node(ctx.styled_dom, dom_id) {
            let style_props = get_style_properties(ctx.styled_dom, dom_id);
            ctx.debug_log(&format!(
                "Root node has text: '{}', font: {}",
                text, style_props.font_selector.family
            ));
            content.push(InlineContent::Text(StyledRun {
                text,
                style: Arc::new(style_props),
                logical_start_byte: 0,
            }));
        }
    }

    // Also collect from children, which is necessary for mixed inline content
    // like `<div>Text <span>more text</span></div>`
    for &child_index in &ifc_root_node.children {
        let child_node = tree.get(child_index).ok_or(LayoutError::InvalidTree)?;
        let Some(dom_id) = child_node.dom_node_id else {
            continue;
        };

        if get_display_property(ctx.styled_dom, Some(dom_id)) != LayoutDisplay::Inline {
            let intrinsic_sizes = child_node.intrinsic_sizes.unwrap_or_default();
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
        } else if let Some(text) = extract_text_from_node(ctx.styled_dom, dom_id) {
            content.push(InlineContent::Text(StyledRun {
                text,
                style: Arc::new(get_style_properties(ctx.styled_dom, dom_id)),
                logical_start_byte: 0,
            }));
        }
    }
    Ok(content)
}

// Keep old name as an alias for backward compatibility
pub(crate) fn collect_inline_content<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
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

    // Step 1: Resolve the CSS `width` property into a concrete pixel value.
    // Percentage values for `width` are resolved against the containing block's width.
    let resolved_width = match css_width {
        LayoutWidth::Px(px) => {
            // Resolve percentage or absolute pixel value
            match px.to_pixels_no_percent() {
                Some(pixels) => pixels,
                None => match px.to_percent() {
                    Some(p) => (p / 100.0) * containing_block_size.width,
                    None => intrinsic.max_content_width,
                },
            }
        }
        LayoutWidth::MinContent => intrinsic.min_content_width,
        LayoutWidth::MaxContent => intrinsic.max_content_width,
    };

    // Step 2: Resolve the CSS `height` property into a concrete pixel value.
    // Percentage values for `height` are resolved against the containing block's height.
    let resolved_height = match css_height {
        LayoutHeight::Px(px) => {
            // Resolve percentage or absolute pixel value
            match px.to_pixels_no_percent() {
                Some(pixels) => pixels,
                None => match px.to_percent() {
                    Some(p) => (p / 100.0) * containing_block_size.height,
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
    Ok(LogicalSize::from_main_cross(
        main_size,
        cross_size,
        writing_mode,
    ))
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
        messages.push(LayoutDebugMessage {
            message: message.into(),
            location: "sizing".into(),
        });
    }
}
