//! solver3/sizing.rs
//!
//! Pass 2: Sizing calculations (intrinsic and used sizes)

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use azul_core::{
    app_resources::RendererResources,
    dom::{NodeId, NodeType},
    styled_dom::StyledDom,
    ui_solver::FormattingContext,
    window::{LogicalSize, WritingMode},
};
use azul_css::{
    css::CssPropertyValue,
    props::{
        basic::PixelValue,
        layout::LayoutWritingMode,
        property::{CssProperty, CssPropertyType},
    },
    LayoutDebugMessage,
};
use rust_fontconfig::FcFontCache;

use crate::{
    parsedfont::ParsedFont,
    solver3::{
        geometry::{BoxProps, BoxSizing, CssSize, DisplayType, IntrinsicSizes},
        layout_tree::{AnonymousBoxType, LayoutTree},
        positioning::{get_position_type, PositionType},
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
        if position == PositionType::Absolute || position == PositionType::Fixed {
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
        let writing_mode = get_writing_mode(node.dom_node_id);

        let mut max_child_min_cross = 0.0f32;
        let mut max_child_max_cross = 0.0f32;
        let mut total_main_size = 0.0;

        for &child_index in &node.children {
            if let Some(child_intrinsic) = child_intrinsics.get(&child_index) {
                let (child_min_cross, child_max_cross, child_main_size) = match writing_mode {
                    WritingMode::HorizontalTb => (
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
            WritingMode::HorizontalTb => (
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
        // This call is now valid because we added the function to fc.rs
        let inline_content = collect_inline_content(&mut self.ctx, tree, node_index)?;

        if inline_content.is_empty() {
            return Ok(IntrinsicSizes::default());
        }

        // Layout with "min-content" constraints (effectively zero width).
        // This forces all possible line breaks, giving the width of the longest unbreakable unit.
        let min_fragments = vec![LayoutFragment {
            id: "min".to_string(),
            constraints: UnifiedConstraints {
                available_width: 0.0,
                ..Default::default()
            },
        }];

        let min_layout = self
            .text_cache
            .layout_flow(&inline_content, &[], &min_fragments, self.ctx.font_manager)
            .map_err(|_| LayoutError::SizingFailed)?;

        // Layout with "max-content" constraints (infinite width).
        // This produces a single, long line, giving the natural width of the content.
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

/// Gathers inline content for the intrinsic sizing pass.
///
/// This is a simplified version of `collect_and_measure_inline_content`. Instead of
/// performing a full recursive layout on atomic inlines (like inline-block), it uses
/// their already-calculated intrinsic sizes. This is necessary because during the
/// bottom-up intrinsic sizing pass, the available width for children is not yet known.
pub(crate) fn collect_inline_content<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    ctx: &mut LayoutContext<T, Q>,
    tree: &LayoutTree<T>,
    ifc_root_index: usize,
) -> Result<Vec<InlineContent>> {
    let mut content = Vec::new();
    let ifc_root_node = tree.get(ifc_root_index).ok_or(LayoutError::InvalidTree)?;

    for &child_index in &ifc_root_node.children {
        let child_node = tree.get(child_index).ok_or(LayoutError::InvalidTree)?;
        let Some(dom_id) = child_node.dom_node_id else {
            continue;
        };

        if get_display_property(ctx.styled_dom, Some(dom_id)) != DisplayType::Inline {
            // This is an atomic inline-level box (e.g., inline-block, image).
            // Use its pre-calculated intrinsic sizes.
            let intrinsic_sizes = child_node.intrinsic_sizes.unwrap_or_default();

            // For the purpose of calculating the parent's intrinsic size, we treat the
            // child as an object with its max-content dimensions.
            let width = intrinsic_sizes.max_content_width;
            let height = intrinsic_sizes.max_content_height;

            content.push(InlineContent::Shape(InlineShape {
                shape_def: ShapeDefinition::Rectangle {
                    size: crate::text3::cache::Size { width, height },
                    corner_radius: None,
                },
                fill: None,
                stroke: None,
                // The baseline is approximated as the bottom of the box for sizing.
                baseline_offset: height,
            }));
        } else if let Some(text) = extract_text_from_node(ctx.styled_dom, dom_id) {
            content.push(InlineContent::Text(StyledRun {
                text,
                style: Arc::new(get_style_properties(ctx.styled_dom, dom_id)),
                logical_start_byte: 0,
            }));
        } else if let NodeType::Image(image_data) =
            ctx.styled_dom.node_data.as_container()[dom_id].get_node_type()
        {
            let intrinsic_size = child_node.intrinsic_sizes.unwrap_or(IntrinsicSizes {
                max_content_width: 50.0,
                max_content_height: 50.0,
                ..Default::default()
            });
            content.push(InlineContent::Image(InlineImage {
                source: ImageSource::Url(String::new()), // Placeholder
                intrinsic_size: crate::text3::cache::Size {
                    width: intrinsic_size.max_content_width,
                    height: intrinsic_size.max_content_height,
                },
                display_size: None,
                baseline_offset: 0.0,
                alignment: crate::text3::cache::VerticalAlign::Baseline,
                object_fit: ObjectFit::Fill,
            }));
        }
    }
    Ok(content)
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
    if position == PositionType::Absolute || position == PositionType::Fixed {
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
    let intrinsic = calculate_node_intrinsic_sizes(ctx, &node, &child_intrinsics)?;

    if let Some(n) = tree.get_mut(node_index) {
        n.intrinsic_sizes = Some(intrinsic.clone());
    }

    Ok(intrinsic)
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
    let css_width = get_css_width(styled_dom, dom_id);
    let css_height = get_css_height(styled_dom, dom_id);
    let writing_mode = get_writing_mode(styled_dom, dom_id);

    // Step 1: Resolve the CSS `width` property into a concrete pixel value.
    // Percentage values for `width` are resolved against the containing block's width.
    let resolved_width = match css_width {
        CssSize::Px(px) => px,
        CssSize::Percent(p) => (p / 100.0) * containing_block_size.width,
        CssSize::Auto => intrinsic.max_content_width,
        CssSize::MinContent => intrinsic.min_content_width,
        CssSize::MaxContent => intrinsic.max_content_width,
    };

    // Step 2: Resolve the CSS `height` property into a concrete pixel value.
    // Percentage values for `height` are resolved against the containing block's height.
    let resolved_height = match css_height {
        CssSize::Px(px) => px,
        CssSize::Percent(p) => (p / 100.0) * containing_block_size.height,
        CssSize::Auto => intrinsic.max_content_height,
        // NOTE: min/max-content are not valid values for the height property,
        // but we handle them gracefully by falling back to max-content.
        CssSize::MinContent => intrinsic.min_content_height,
        CssSize::MaxContent => intrinsic.max_content_height,
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

fn get_css_property_value<T: Clone>(
    styled_dom: &StyledDom,
    dom_id: Option<NodeId>,
    property: CssProperty,
    extractor: fn(&CssPropertyValue<T>) -> Option<T>,
) -> Option<T> {
    let Some(id) = dom_id else {
        return None;
    };
    let Some(styled_node) = styled_dom.styled_nodes.as_container().get(id) else {
        return None;
    };
    styled_node
        .state
        .get_style()
        .get(&property)
        .and_then(extractor)
}

// TODO: STUB: Functions to simulate reading computed CSS values.
pub fn get_css_width(styled_dom: &StyledDom, dom_id: Option<NodeId>) -> CssSize {
    let Some(id) = dom_id else {
        return CssSize::Auto;
    };
    if let Some(styled_node) = styled_dom.styled_nodes.as_container().get(id) {
        if let Some(prop) = styled_node.state.get_style().get(&CssPropertyType::Width) {
            if let Some(val) = prop.get_exact() {
                return match val {
                    PixelValue::Px(px) => CssSize::Px(*px),
                    PixelValue::Percent(p) => CssSize::Percent(*p),
                };
            }
        }
    }
    CssSize::Auto
}

pub fn get_css_height(styled_dom: &StyledDom, dom_id: Option<NodeId>) -> CssSize {
    let Some(id) = dom_id else {
        return CssSize::Auto;
    };
    if let Some(styled_node) = styled_dom.styled_nodes.as_container().get(id) {
        if let Some(prop) = styled_node.state.get_style().get(&CssPropertyType::Height) {
            if let Some(val) = prop.get_exact() {
                return match val {
                    PixelValue::Px(px) => CssSize::Px(*px),
                    PixelValue::Percent(p) => CssSize::Percent(*p),
                };
            }
        }
    }
    CssSize::Auto
}

fn get_box_props(dom_id: Option<NodeId>) -> BoxProps {
    BoxProps::default()
}

fn get_writing_mode(styled_dom: &StyledDom, dom_id: Option<NodeId>) -> WritingMode {
    let Some(id) = dom_id else {
        return WritingMode::HorizontalTb;
    };
    if let Some(styled_node) = styled_dom.styled_nodes.as_container().get(id) {
        if let Some(prop) = styled_node
            .state
            .get_style()
            .get(&CssPropertyType::WritingMode)
        {
            if let Some(val) = prop.get_exact() {
                return match val {
                    LayoutWritingMode::HorizontalTb => WritingMode::HorizontalTb,
                    LayoutWritingMode::VerticalRl => WritingMode::VerticalRl,
                    LayoutWritingMode::VerticalLr => WritingMode::VerticalLr,
                };
            }
        }
    }
    WritingMode::HorizontalTb
}
