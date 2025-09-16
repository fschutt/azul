//! solver3/sizing.rs
//! Pass 2: Sizing calculations (intrinsic and used sizes)

use std::{collections::BTreeMap, sync::Arc};

use azul_core::{
    app_resources::RendererResources,
    dom::{NodeId, NodeType},
    styled_dom::StyledDom,
    ui_solver::{FormattingContext, IntrinsicSizes},
    window::LogicalSize,
};
use azul_css::LayoutDebugMessage;
use rust_fontconfig::FcFontCache;

use crate::{
    parsedfont::ParsedFont,
    solver3::{
        layout_tree::{AnonymousBoxType, LayoutTree},
        LayoutError, Result,
    },
    text3::cache::{
        FontManager, FontProviderTrait, InlineContent, LayoutCache, LayoutFragment, StyleProperties, StyledRun, UnifiedConstraints
    },
};

/// Phase 2a: Calculate intrinsic sizes (bottom-up pass)
pub fn calculate_intrinsic_sizes(
    tree: &LayoutTree,
    styled_dom: &StyledDom,
    renderer_resources: &mut RendererResources,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> Result<BTreeMap<usize, IntrinsicSizes>> {
    debug_log(debug_messages, "Starting intrinsic size calculation");

    let mut intrinsic_sizes = BTreeMap::new();
    let mut calculator = IntrinsicSizeCalculator::new(styled_dom, renderer_resources);

    // Post-order traversal (children first, then parent)
    calculate_intrinsic_recursive(
        tree,
        tree.root,
        &mut calculator,
        &mut intrinsic_sizes,
        debug_messages,
    )?;

    debug_log(
        debug_messages,
        &format!(
            "Calculated intrinsic sizes for {} nodes",
            intrinsic_sizes.len()
        ),
    );

    Ok(intrinsic_sizes)
}

/// Phase 2b: Calculate used sizes (top-down pass)  
pub fn calculate_used_sizes(
    tree: &LayoutTree,
    intrinsic_sizes: &BTreeMap<usize, IntrinsicSizes>,
    viewport_size: LogicalSize,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> Result<BTreeMap<usize, LogicalSize>> {
    debug_log(debug_messages, "Starting used size calculation");

    let mut used_sizes = BTreeMap::new();

    // Start with root having viewport size as containing block
    let root_size = LogicalSize::new(viewport_size.width, viewport_size.height);
    used_sizes.insert(tree.root, root_size);

    // Pre-order traversal (parent first, then children)
    calculate_used_recursive(
        tree,
        tree.root,
        root_size,
        intrinsic_sizes,
        &mut used_sizes,
        debug_messages,
    )?;

    debug_log(
        debug_messages,
        &format!("Calculated used sizes for {} nodes", used_sizes.len()),
    );

    Ok(used_sizes)
}

struct IntrinsicSizeCalculator<'a> {
    styled_dom: &'a StyledDom,
    renderer_resources: &'a mut RendererResources,
    text_cache: LayoutCache<ParsedFont>,
}

impl<'a> IntrinsicSizeCalculator<'a> {
    fn new(styled_dom: &'a StyledDom, renderer_resources: &'a mut RendererResources) -> Self {
        Self {
            styled_dom,
            renderer_resources,
            text_cache: LayoutCache::new(),
        }
    }

    fn calculate_node_intrinsic_sizes(
        &mut self,
        tree: &LayoutTree,
        node_index: usize,
        child_intrinsics: &BTreeMap<usize, IntrinsicSizes>,
        debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    ) -> Result<IntrinsicSizes> {
        let node = tree.get(node_index).ok_or(LayoutError::InvalidTree)?;

        match node.formatting_context {
            FormattingContext::Block {
                establishes_new_context,
            } => self.calculate_block_intrinsic_sizes(tree, node_index, child_intrinsics),
            FormattingContext::Inline => {
                self.calculate_inline_intrinsic_sizes(tree, node_index, debug_messages)
            }
            FormattingContext::Flex => {
                self.calculate_flex_intrinsic_sizes(tree, node_index, child_intrinsics)
            }
            FormattingContext::Grid => {
                self.calculate_grid_intrinsic_sizes(tree, node_index, child_intrinsics)
            }
            FormattingContext::Table => {
                self.calculate_table_intrinsic_sizes(tree, node_index, child_intrinsics)
            }
        }
    }

    fn calculate_block_intrinsic_sizes(
        &self,
        tree: &LayoutTree,
        node_index: usize,
        child_intrinsics: &BTreeMap<usize, IntrinsicSizes>,
    ) -> Result<IntrinsicSizes> {
        let node = tree.get(node_index).ok_or(LayoutError::InvalidTree)?;

        if node.children.is_empty() {
            // Empty block has minimal size
            return Ok(IntrinsicSizes {
                min_width: 0.0,
                pref_width: 0.0,
                min_height: 0.0,
                pref_height: 0.0,
            });
        }

        let mut max_child_min_width = 0.0;
        let mut max_child_pref_width = 0.0;
        let mut total_height = 0.0;

        // Block stacks children vertically, so:
        // - width = max of children's widths
        // - height = sum of children's heights
        for &child_index in &node.children {
            if let Some(child_intrinsic) = child_intrinsics.get(&child_index) {
                max_child_min_width = max_child_min_width.max(child_intrinsic.min_width);
                max_child_pref_width = max_child_pref_width.max(child_intrinsic.pref_width);
                total_height += child_intrinsic.pref_height;
            }
        }

        Ok(IntrinsicSizes {
            min_width: max_child_min_width,
            pref_width: max_child_pref_width,
            min_height: total_height,
            pref_height: total_height,
        })
    }

    fn calculate_inline_intrinsic_sizes(
        &mut self,
        tree: &LayoutTree,
        node_index: usize,
        debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    ) -> Result<IntrinsicSizes> {
        let node = tree.get(node_index).ok_or(LayoutError::InvalidTree)?;

        // For inline formatting contexts, we need text3 to determine intrinsic sizes
        let inline_content =
            collect_inline_content_for_intrinsics(tree, node_index, self.styled_dom)?;

        if inline_content.is_empty() {
            return Ok(IntrinsicSizes::default());
        }

        debug_log(
            debug_messages,
            &format!(
                "IFC intrinsics: Processing {} inline items",
                inline_content.len()
            ),
        );

        // Calculate min-content width (width of longest unbreakable word)
        let min_width_constraint = UnifiedConstraints {
            available_width: 0.0, // Force minimum width
            available_height: None,
            ..Default::default()
        };

        let min_fragments = vec![LayoutFragment {
            id: "min".to_string(),
            constraints: min_width_constraint,
        }];

        // NOTE: Same code as in fc/ifc.rs - Stub font provider for intrinsic size calculation
        // 
        // Layout text with text3
        // 
        // NOTE: This will re-initialize the FcFontCache on EVERY LAYOUT CALL - 
        // MASSIVE BUG BUT OK FOR TESTING RIGHT NOW
        let fc_cache = FcFontCache::build();
        let font_provider = Arc::new(crate::text3::default::PathLoader::new());
        let font_manager = FontManager::with_loader(fc_cache, font_provider).unwrap();

        let min_layout = self
            .text_cache
            .layout_flow(&inline_content, &[], &min_fragments, &font_manager)
            .map_err(|_| LayoutError::SizingFailed)?;

        // Calculate max-content width (width on single line)
        let max_width_constraint = UnifiedConstraints {
            available_width: f32::INFINITY,
            available_height: None,
            ..Default::default()
        };

        let max_fragments = vec![LayoutFragment {
            id: "max".to_string(),
            constraints: max_width_constraint,
        }];

        let max_layout = self
            .text_cache
            .layout_flow(&inline_content, &[], &max_fragments, &font_provider)
            .map_err(|_| LayoutError::SizingFailed)?;

        let min_width = min_layout
            .fragment_layouts
            .get("min")
            .map(|l| l.bounds.width)
            .unwrap_or(0.0);

        let pref_width = max_layout
            .fragment_layouts
            .get("max")
            .map(|l| l.bounds.width)
            .unwrap_or(0.0);

        let height = max_layout
            .fragment_layouts
            .get("max")
            .map(|l| l.bounds.height)
            .unwrap_or(20.0); // Default line height

        Ok(IntrinsicSizes {
            min_width,
            pref_width,
            min_height: height,
            pref_height: height,
        })
    }

    fn calculate_flex_intrinsic_sizes(
        &self,
        tree: &LayoutTree,
        node_index: usize,
        child_intrinsics: &BTreeMap<usize, IntrinsicSizes>,
    ) -> Result<IntrinsicSizes> {
        // Stub: Sum children widths, max height
        let node = tree.get(node_index).ok_or(LayoutError::InvalidTree)?;

        let mut total_width = 0.0;
        let mut max_height = 0.0;

        for &child_index in &node.children {
            if let Some(child_intrinsic) = child_intrinsics.get(&child_index) {
                total_width += child_intrinsic.pref_width;
                max_height = max_height.max(child_intrinsic.pref_height);
            }
        }

        Ok(IntrinsicSizes {
            min_width: total_width,
            pref_width: total_width,
            min_height: max_height,
            pref_height: max_height,
        })
    }

    fn calculate_grid_intrinsic_sizes(
        &self,
        tree: &LayoutTree,
        node_index: usize,
        child_intrinsics: &BTreeMap<usize, IntrinsicSizes>,
    ) -> Result<IntrinsicSizes> {
        // Stub: 2-column grid layout
        let node = tree.get(node_index).ok_or(LayoutError::InvalidTree)?;

        let mut max_width = 0.0;
        let mut total_height = 0.0;

        for (i, &child_index) in node.children.iter().enumerate() {
            if let Some(child_intrinsic) = child_intrinsics.get(&child_index) {
                if i % 2 == 0 {
                    // Start of new row
                    total_height += child_intrinsic.pref_height;
                }
                max_width = (max_width as f32).max(child_intrinsic.pref_width * 2.0);
            }
        }

        Ok(IntrinsicSizes {
            min_width: max_width,
            pref_width: max_width,
            min_height: total_height,
            pref_height: total_height,
        })
    }

    fn calculate_table_intrinsic_sizes(
        &self,
        tree: &LayoutTree,
        node_index: usize,
        child_intrinsics: &BTreeMap<usize, IntrinsicSizes>,
    ) -> Result<IntrinsicSizes> {
        // Stub: Stack rows vertically
        let node = tree.get(node_index).ok_or(LayoutError::InvalidTree)?;

        let mut max_width = 0.0;
        let mut total_height = 0.0;

        for &child_index in &node.children {
            if let Some(child_intrinsic) = child_intrinsics.get(&child_index) {
                max_width = (max_width as f32).max(child_intrinsic.pref_width);
                total_height += child_intrinsic.pref_height;
            }
        }

        Ok(IntrinsicSizes {
            min_width: max_width,
            pref_width: max_width,
            min_height: total_height,
            pref_height: total_height,
        })
    }
}

fn calculate_intrinsic_recursive(
    tree: &LayoutTree,
    node_index: usize,
    calculator: &mut IntrinsicSizeCalculator,
    intrinsic_sizes: &mut BTreeMap<usize, IntrinsicSizes>,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> Result<()> {
    let node = tree.get(node_index).ok_or(LayoutError::InvalidTree)?;

    // First, calculate children's intrinsic sizes
    for &child_index in &node.children {
        calculate_intrinsic_recursive(
            tree,
            child_index,
            calculator,
            intrinsic_sizes,
            debug_messages,
        )?;
    }

    // Then calculate this node's intrinsic size based on children
    let intrinsic = calculator.calculate_node_intrinsic_sizes(
        tree,
        node_index,
        intrinsic_sizes,
        debug_messages,
    )?;

    intrinsic_sizes.insert(node_index, intrinsic);
    Ok(())
}

fn calculate_used_recursive(
    tree: &LayoutTree,
    node_index: usize,
    containing_block_size: LogicalSize,
    intrinsic_sizes: &BTreeMap<usize, IntrinsicSizes>,
    used_sizes: &mut BTreeMap<usize, LogicalSize>,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> Result<()> {
    let node = tree.get(node_index).ok_or(LayoutError::InvalidTree)?;

    // Calculate this node's used size
    let used_size =
        calculate_used_size_for_node(tree, node_index, containing_block_size, intrinsic_sizes)?;

    used_sizes.insert(node_index, used_size);

    // Recursively calculate children with this node as containing block
    for &child_index in &node.children {
        calculate_used_recursive(
            tree,
            child_index,
            used_size, // This node becomes containing block for children
            intrinsic_sizes,
            used_sizes,
            debug_messages,
        )?;
    }

    Ok(())
}

fn calculate_used_size_for_node(
    tree: &LayoutTree,
    node_id: usize,
    containing_block_size: LogicalSize,
    intrinsic_sizes: &BTreeMap<usize, IntrinsicSizes>,
) -> Result<LogicalSize> {
    let node = tree.get(node_id).ok_or(LayoutError::InvalidTree)?;
    let intrinsic = intrinsic_sizes
        .get(&node_id)
        .unwrap_or(&IntrinsicSizes::default());

    // In a real engine, we'd get computed values for width, height, min/max-width/height here.
    // For now, we simulate this with a simplified logic.
    let dom_id = node.dom_node_id;
    let is_block = matches!(node.formatting_context, FormattingContext::Block { .. });
    let is_inline_block = dom_id.is_some() && !is_block; // Simplified assumption for inline-block

    // 1. Resolve width
    // TODO: Parse CSS width property. For now, assume "auto".
    let width = if is_block {
        // Block-level elements in normal flow take up the full width of their containing block.
        containing_block_size.width
    } else {
        // Inline-level elements (inline-block, etc.) are "shrink-to-fit".
        // Their width is the preferred intrinsic width, clamped by the containing block.
        intrinsic.pref_width.min(containing_block_size.width)
    };

    // 2. Resolve height
    // TODO: Parse CSS height property. For now, assume "auto".
    let height = intrinsic.pref_height;

    // TODO: Apply min/max width/height constraints.

    Ok(LogicalSize::new(width, height))
}

fn collect_inline_content_for_intrinsics(
    tree: &LayoutTree,
    node_index: usize,
    styled_dom: &StyledDom,
) -> Result<Vec<InlineContent>> {
    let mut content = Vec::new();
    let node = tree.get(node_index).ok_or(LayoutError::InvalidTree)?;

    // Recursively collect text content
    collect_text_recursive(tree, node_index, styled_dom, &mut content);

    Ok(content)
}

fn collect_text_recursive(
    tree: &LayoutTree,
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

fn extract_text_from_node(styled_dom: &StyledDom, node_id: NodeId) -> Option<String> {
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
