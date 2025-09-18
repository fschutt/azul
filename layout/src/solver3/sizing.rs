//! solver3/sizing.rs
//!
//! Pass 2: Sizing calculations (intrinsic and used sizes)

use std::{collections::BTreeMap, sync::Arc};

use azul_core::{
    app_resources::RendererResources,
    dom::{NodeId, NodeType},
    styled_dom::StyledDom,
    ui_solver::{FormattingContext, IntrinsicSizes},
    window::{LogicalSize, WritingMode},
};
use azul_css::LayoutDebugMessage;
use rust_fontconfig::FcFontCache;

use crate::{
    parsedfont::ParsedFont,
    solver3::{
        geometry::{BoxProps, BoxSizing, CssSize, DisplayType},
        layout_tree::{AnonymousBoxType, LayoutTree},
        positioning::{get_position_type, PositionType},
        LayoutContext, LayoutError, Result,
    },
    text3::cache::{
        FontLoaderTrait, FontManager, FontProviderTrait, InlineContent, LayoutCache,
        LayoutFragment, ParsedFontTrait, StyleProperties, StyledRun, UnifiedConstraints,
    },
};

/// Phase 2a: Calculate intrinsic sizes (bottom-up pass)
pub fn calculate_intrinsic_sizes<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    ctx: &LayoutContext<T, Q>,
    tree: &LayoutTree,
) -> Result<BTreeMap<usize, IntrinsicSizes>> {
    ctx.debug_log("Starting intrinsic size calculation");

    let mut intrinsic_sizes = BTreeMap::new();
    let mut calculator = IntrinsicSizeCalculator::new(ctx);

    // Post-order traversal (children first, then parent)
    calculate_intrinsic_recursive(tree, tree.root, &mut calculator, &mut intrinsic_sizes)?;

    ctx.debug_log(&format!(
        "Calculated intrinsic sizes for {} nodes",
        intrinsic_sizes.len()
    ));

    Ok(intrinsic_sizes)
}

/// Phase 2b: Calculate used sizes (top-down pass)
pub fn calculate_used_sizes<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    ctx: &LayoutContext<T, Q>,
    tree: &LayoutTree,
    intrinsic_sizes: &BTreeMap<usize, IntrinsicSizes>,
    viewport_size: LogicalSize,
) -> Result<BTreeMap<usize, LogicalSize>> {
    ctx.debug_log("Starting used size calculation");

    let mut used_sizes = BTreeMap::new();
    let root_size = LogicalSize::new(viewport_size.width, viewport_size.height);
    used_sizes.insert(tree.root, root_size);

    calculate_used_recursive(
        tree,
        tree.root,
        root_size,
        intrinsic_sizes,
        &mut used_sizes,
        ctx.debug_messages,
    )?;

    ctx.debug_log(&format!(
        "Calculated used sizes for {} nodes",
        used_sizes.len()
    ));

    Ok(used_sizes)
}

struct IntrinsicSizeCalculator<'a, 'b, T: ParsedFontTrait, Q: FontLoaderTrait<T>> {
    ctx: &'a LayoutContext<'b, T, Q>,
    text_cache: LayoutCache<ParsedFont>,
}

impl<'a, 'b, T: ParsedFontTrait, Q: FontLoaderTrait<T>> IntrinsicSizeCalculator<'a, 'b, T, Q> {
    fn new(ctx: &'a LayoutContext<'b, T, Q>) -> Self {
        Self {
            ctx,
            text_cache: LayoutCache::new(),
        }
    }

    fn calculate_node_intrinsic_sizes(
        &mut self,
        tree: &LayoutTree,
        node_index: usize,
        child_intrinsics: &BTreeMap<usize, IntrinsicSizes>,
    ) -> Result<IntrinsicSizes> {
        let node = tree.get(node_index).ok_or(LayoutError::InvalidTree)?;

        match node.formatting_context {
            FormattingContext::Block { .. } => {
                self.calculate_block_intrinsic_sizes(tree, node_index, child_intrinsics)
            }
            FormattingContext::Inline => self.calculate_inline_intrinsic_sizes(tree, node_index),
            FormattingContext::Table => {
                self.calculate_table_intrinsic_sizes(tree, node_index, child_intrinsics)
            }
            // Add cases for MultiColumn, Flex, Grid...
            _ => self.calculate_block_intrinsic_sizes(tree, node_index, child_intrinsics),
        }
    }

    /// **FIX**: Restored full implementation for BFC intrinsic sizing.
    fn calculate_block_intrinsic_sizes(
        &self,
        tree: &LayoutTree,
        node_index: usize,
        child_intrinsics: &BTreeMap<usize, IntrinsicSizes>,
    ) -> Result<IntrinsicSizes> {
        let node = tree.get(node_index).ok_or(LayoutError::InvalidTree)?;
        let writing_mode = get_writing_mode(node.dom_node_id); // Assuming this helper exists

        let mut max_child_min_cross = 0.0;
        let mut max_child_pref_cross = 0.0;
        let mut total_main_size = 0.0;

        // Block stacks children along the main axis, so:
        // - cross-size = max of children's cross-sizes
        // - main-size = sum of children's main-sizes
        for &child_index in &node.children {
            if let Some(child_intrinsic) = child_intrinsics.get(&child_index) {
                let (child_min_cross, child_pref_cross, child_main_size) = match writing_mode {
                    WritingMode::HorizontalTb => (
                        child_intrinsic.min_width,
                        child_intrinsic.pref_width,
                        child_intrinsic.pref_height,
                    ),
                    _ => (
                        child_intrinsic.min_height,
                        child_intrinsic.pref_height,
                        child_intrinsic.pref_width,
                    ),
                };

                max_child_min_cross = max_child_min_cross.max(child_min_cross);
                max_child_pref_cross = max_child_pref_cross.max(child_pref_cross);
                total_main_size += child_main_size;
            }
        }

        let (min_width, pref_width, min_height, pref_height) = match writing_mode {
            WritingMode::HorizontalTb => (
                max_child_min_cross,
                max_child_pref_cross,
                total_main_size,
                total_main_size,
            ),
            _ => (
                total_main_size,
                total_main_size,
                max_child_min_cross,
                max_child_pref_cross,
            ),
        };

        Ok(IntrinsicSizes {
            min_width,
            pref_width,
            min_height,
            pref_height,
        })
    }

    /// **FIX**: Restored full implementation for IFC intrinsic sizing using text3.
    fn calculate_inline_intrinsic_sizes(
        &mut self,
        tree: &LayoutTree,
        node_index: usize,
    ) -> Result<IntrinsicSizes> {
        // For IFCs, we need text3 to determine intrinsic sizes.
        // This helper function would need to be implemented to collect all text
        // and inline-block children into a format text3 understands.
        let inline_content = collect_inline_content(self.ctx, tree, node_index)?;

        if inline_content.is_empty() {
            return Ok(IntrinsicSizes::default());
        }

        // Calculate min-content width (longest unbreakable word)
        let min_fragments = vec![LayoutFragment {
            id: "min".to_string(),
            constraints: UnifiedConstraints {
                available_width: 0.0,
                ..Default::default()
            },
        }];

        // **FIX**: Use the font_manager from the context, preventing re-initialization.
        let min_layout = self
            .text_cache
            .layout_flow(&inline_content, &[], &min_fragments, self.ctx.font_manager)
            .map_err(|_| LayoutError::SizingFailed)?;

        // Calculate max-content width (width on a single line)
        let max_fragments = vec![LayoutFragment {
            id: "max".to_string(),
            constraints: UnifiedConstraints {
                available_width: f32::INFINITY,
                ..Default::default()
            },
        }];

        let max_layout = self
            .text_cache
            .layout_flow(&inline_content, &[], &max_fragments, self.ctx.font_manager)
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
            .unwrap_or(0.0);

        Ok(IntrinsicSizes {
            min_width,
            pref_width,
            min_height: height,
            pref_height: height,
        })
    }

    fn calculate_table_intrinsic_sizes(
        &self,
        tree: &LayoutTree,
        node_index: usize,
        child_intrinsics: &BTreeMap<usize, IntrinsicSizes>,
    ) -> Result<IntrinsicSizes> {
        // STUB: This would implement the first passes of the table-grid algorithm
        // to determine the table's intrinsic min/max content size based on its columns.
        Ok(IntrinsicSizes::default())
    }
}

fn calculate_intrinsic_recursive<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    tree: &LayoutTree,
    node_index: usize,
    calculator: &mut IntrinsicSizeCalculator<T, Q>,
    intrinsic_sizes: &mut BTreeMap<usize, IntrinsicSizes>,
) -> Result<()> {
    let node = tree.get(node_index).ok_or(LayoutError::InvalidTree)?;

    // **FIX**: Out-of-flow elements do not contribute to their parent's intrinsic size.
    // We can skip them and their descendants in this pass.
    let position = get_position_type(calculator.ctx.styled_dom, node.dom_node_id);
    if position == PositionType::Absolute || position == PositionType::Fixed {
        intrinsic_sizes.insert(node_index, IntrinsicSizes::default());
        return Ok(());
    }

    // First, calculate children's intrinsic sizes
    for &child_index in &node.children {
        calculate_intrinsic_recursive(tree, child_index, calculator, intrinsic_sizes)?;
    }

    // Then calculate this node's intrinsic size based on its children
    let intrinsic = calculator.calculate_node_intrinsic_sizes(tree, node_index, intrinsic_sizes)?;
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
    let dom_id = node.dom_node_id;

    // These helpers would now read from the StyledDom correctly.
    let css_width = get_css_width(dom_id);
    let css_height = get_css_height(dom_id);
    let box_props = get_box_props(dom_id);
    let box_sizing = get_box_sizing_property(dom_id);
    let writing_mode = get_writing_mode(dom_id);

    let available_cross_size = containing_block_size.cross(writing_mode);

    // 1. Resolve cross size (was width).
    let cross_size = match css_width {
        // Assuming css_width maps to cross-axis size
        CssSize::Px(px) => {
            /* logic with box_sizing */
            px
        }
        CssSize::Percent(p) => {
            /* logic with box_sizing */
            (p / 100.0) * available_cross_size
        }
        // ... other cases
        _ => intrinsic.pref_width, // Placeholder
    };

    // 2. Resolve main size (was height).
    let main_size = match css_height {
        // Assuming css_height maps to main-axis size
        // ... similar logic as cross_size, but using main axis properties
        _ => intrinsic.pref_height, // Placeholder
    };

    // 3. Construct final LogicalSize from logical dimensions.
    Ok(LogicalSize::new(0.0, 0.0)
        .with_cross(writing_mode, cross_size)
        .with_main(writing_mode, main_size))
}

// TODO: STUB: Functions to simulate reading computed CSS values.
fn get_css_width(dom_id: Option<NodeId>) -> CssSize {
    CssSize::Auto
}
fn get_css_height(dom_id: Option<NodeId>) -> CssSize {
    CssSize::Auto
}
fn get_box_props(dom_id: Option<NodeId>) -> BoxProps {
    BoxProps::default()
}
fn get_writing_mode(dom_id: Option<NodeId>) -> WritingMode {
    WritingMode::default()
}
fn get_box_sizing_property(dom_id: Option<NodeId>) -> BoxSizing {
    BoxSizing::default()
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
