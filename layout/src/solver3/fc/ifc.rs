//! solver3/fc/ifc.rs
//!
//! Inline Formatting Context - integrates with text3 for inline layout

use std::sync::Arc;

use azul_core::{
    app_resources::RendererResources,
    dom::{NodeId, NodeType},
    styled_dom::StyledDom,
    window::{LogicalPosition, LogicalSize},
};
use azul_css::LayoutDebugMessage;
use rust_fontconfig::{FcFontCache, FcWeight};

use super::{FormattingContextManager, LayoutConstraints, LayoutResult, TextAlign, WritingMode};
use crate::{
    parsedfont::ParsedFont,
    solver3::{
        layout_tree::{LayoutNode, LayoutTree},
        LayoutError, Result,
    },
    text3::cache::{
        Color, FontManager, FontProviderTrait, FontRef, FontStyle, ImageSource, InlineContent, InlineImage, LayoutCache, LayoutFragment, ObjectFit, Size, StyleProperties, StyledRun, TextAlign as Text3TextAlign, UnifiedConstraints, UnifiedLayout, VerticalAlign, WritingMode as Text3WritingMode
    },
};

/// Inline layout manager that integrates with text3
pub struct InlineLayoutManager {
    text_cache: LayoutCache<ParsedFont>,
}

impl InlineLayoutManager {
    pub fn new() -> Self {
        Self {
            text_cache: LayoutCache::new(),
        }
    }

    /// Extended layout method for IFC that needs renderer resources
    pub fn layout(
        &mut self,
        tree: &mut LayoutTree,
        node_index: usize,
        constraints: &LayoutConstraints,
        styled_dom: &StyledDom,
        renderer_resources: &mut RendererResources,
        debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    ) -> Result<LayoutResult> {
        let node = tree.get(node_index).ok_or(LayoutError::InvalidTree)?;
        let children = node.children.clone();

        debug_log(
            debug_messages,
            &format!("IFC: Processing {} inline children", children.len()),
        );

        // Collect inline content from children
        let inline_content =
            self.collect_inline_content(tree, &children, styled_dom, debug_messages)?;

        if inline_content.is_empty() {
            return Ok(LayoutResult {
                positions: Vec::new(),
                overflow_size: None,
                baseline_offset: 0.0,
            });
        }

        // Convert solver3 constraints to text3 constraints
        let text3_constraints = self.convert_constraints(constraints);

        // Create layout fragments for text3
        let fragments = vec![LayoutFragment {
            id: "main".to_string(),
            constraints: text3_constraints,
        }];

        // Layout text with text3
        // 
        // NOTE: This will re-initialize the FcFontCache on EVERY LAYOUT CALL - 
        // MASSIVE BUG BUT OK FOR TESTING RIGHT NOW
        let fc_cache = FcFontCache::build();
        let font_provider = Arc::new(crate::text3::default::PathLoader::new());
        let font_manager = FontManager::with_loader(fc_cache, font_provider).unwrap();

        // Returns the FlowLayout
        let layout_result = self
            .text_cache
            .layout_flow(
                &inline_content,
                &[], // No style overrides for now
                &fragments,
                &font_manager,
            )
            .map_err(|_| LayoutError::PositioningFailed)?;

        // Extract and store the layout result
        let main_layout = layout_result
            .fragment_layouts
            .get("main")
            .ok_or(LayoutError::PositioningFailed)?
            .clone();

        // Store text3 result in the layout tree node
        if let Some(node_mut) = tree.get_mut(node_index) {
            node_mut.inline_layout_result = Some(main_layout.clone());
        }

        // Extract positions for each child node
        let positions = self.extract_child_positions(&children, &main_layout, debug_messages);

        let baseline_offset = calculate_baseline_offset(&main_layout);
        let overflow_size = if main_layout.overflow.overflow_items.is_empty() {
            None
        } else {
            Some(LogicalSize::new(
                main_layout.overflow.unclipped_bounds.width,
                main_layout.overflow.unclipped_bounds.height,
            ))
        };

        debug_log(
            debug_messages,
            &format!(
                "IFC: Layout complete, {} positioned items",
                main_layout.items.len()
            ),
        );

        Ok(LayoutResult {
            positions,
            overflow_size,
            baseline_offset,
        })
    }
}

impl FormattingContextManager for InlineLayoutManager {
    fn layout(
        &mut self,
        tree: &mut LayoutTree,
        node_index: usize,
        constraints: &LayoutConstraints,
        styled_dom: &StyledDom,
        debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    ) -> Result<LayoutResult> {
        // Default implementation without renderer resources
        let mut default_resources = RendererResources::default();
        self.layout(
            tree,
            node_index,
            constraints,
            styled_dom,
            &mut default_resources,
            debug_messages,
        )
    }
}

impl InlineLayoutManager {
    fn collect_inline_content(
        &self,
        tree: &LayoutTree,
        children: &[usize],
        styled_dom: &StyledDom,
        debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    ) -> Result<Vec<InlineContent>> {
        let mut content = Vec::new();

        for &child_index in children {
            let child = tree.get(child_index).ok_or(LayoutError::InvalidTree)?;

            match child.dom_node_id {
                Some(dom_id) => {
                    let inline_item =
                        self.convert_dom_node_to_inline_content(styled_dom, dom_id, child)?;
                    content.push(inline_item);
                }
                None => {
                    // Anonymous box - collect its children
                    let child_content = self.collect_inline_content(
                        tree,
                        &child.children,
                        styled_dom,
                        debug_messages,
                    )?;
                    content.extend(child_content);
                }
            }
        }

        debug_log(
            debug_messages,
            &format!("IFC: Collected {} inline content items", content.len()),
        );

        Ok(content)
    }

    fn convert_dom_node_to_inline_content(
        &self,
        styled_dom: &StyledDom,
        node_id: NodeId,
        layout_node: &LayoutNode,
    ) -> Result<InlineContent> {
        let node_data = &styled_dom.node_data.as_container()[node_id];

        match node_data.get_node_type() {
            NodeType::Text(text_data) => {
                let style = extract_text_style(styled_dom, node_id)?;
                Ok(InlineContent::Text(StyledRun {
                    text: text_data.as_str().to_string(),
                    style: Arc::new(style),
                    logical_start_byte: 0,
                }))
            }
            NodeType::Image(img_data) => {
                let size = img_data.get_size();
                let image = InlineImage {
                    source: ImageSource::Url(img_data.get_hash().0.to_string()),
                    intrinsic_size: Size {
                        width: size.width,
                        height: size.height,
                    },
                    display_size: layout_node.used_size.map(|s| Size {
                        width: s.width,
                        height: s.height,
                    }),
                    baseline_offset: 0.0,
                    alignment: VerticalAlign::Baseline,
                    object_fit: ObjectFit::Fill,
                };
                Ok(InlineContent::Image(image))
            }
            NodeType::Div | NodeType::Label => {
                // Inline-block element - treat as object
                let size = layout_node.used_size.unwrap_or(LogicalSize::zero());
                let image = InlineImage {
                    source: ImageSource::Placeholder(Size {
                        width: size.width,
                        height: size.height,
                    }),
                    intrinsic_size: Size {
                        width: size.width,
                        height: size.height,
                    },
                    display_size: Some(Size {
                        width: size.width,
                        height: size.height,
                    }),
                    baseline_offset: 0.0,
                    alignment: VerticalAlign::Baseline,
                    object_fit: ObjectFit::None,
                };
                Ok(InlineContent::Image(image))
            }
            _ => {
                // Fallback for unsupported inline content
                Ok(InlineContent::Text(StyledRun {
                    text: " ".to_string(), // Placeholder
                    style: Arc::new(StyleProperties::default()),
                    logical_start_byte: 0,
                }))
            }
        }
    }

    fn convert_constraints(&self, constraints: &LayoutConstraints) -> UnifiedConstraints {
        UnifiedConstraints {
            available_width: constraints.available_size.width,
            available_height: Some(constraints.available_size.height),
            writing_mode: Some(match constraints.writing_mode {
                WritingMode::HorizontalTb => Text3WritingMode::HorizontalTb,
                WritingMode::VerticalRl => Text3WritingMode::VerticalRl,
                WritingMode::VerticalLr => Text3WritingMode::VerticalLr,
            }),
            text_align: match constraints.text_align {
                TextAlign::Start => Text3TextAlign::Start,
                TextAlign::End => Text3TextAlign::End,
                TextAlign::Center => Text3TextAlign::Center,
                TextAlign::Justify => Text3TextAlign::Justify,
            },
            line_height: 1.2, // Default line height
            ..Default::default()
        }
    }

    fn extract_child_positions(
        &self,
        children: &[usize],
        layout: &UnifiedLayout<ParsedFont>,
        debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    ) -> Vec<(usize, LogicalPosition)> {
        // This is simplified - a full implementation would map text3's
        // PositionedItem results back to the original child layout nodes
        let mut positions = Vec::new();

        for (i, &child_index) in children.iter().enumerate() {
            // For now, just stack them horizontally as a placeholder
            let x = i as f32 * 50.0;
            positions.push((child_index, LogicalPosition::new(x, 0.0)));
        }

        debug_log(
            debug_messages,
            &format!("IFC: Extracted {} child positions", positions.len()),
        );

        positions
    }
}

fn extract_text_style(styled_dom: &StyledDom, node_id: NodeId) -> Result<StyleProperties> {
    // Extract CSS properties and convert to text3 StyleProperties
    // This is simplified - real implementation would parse all relevant CSS properties
    Ok(StyleProperties {
        font_ref: FontRef {
            family: "serif".to_string(), // Default
            weight: FcWeight::Normal,
            style: FontStyle::Normal,
            unicode_ranges: Vec::new(),
        },
        font_size_px: 16.0,
        color: Color {
            r: 0,
            g: 0,
            b: 0,
            a: 255,
        },
        line_height: 1.2,
        ..Default::default()
    })
}

fn calculate_baseline_offset(layout: &UnifiedLayout<ParsedFont>) -> f32 {
    // Calculate the baseline offset from the text3 layout result
    // This would examine the positioned items to find the baseline
    0.0 // Placeholder
}

fn debug_log(debug_messages: &mut Option<Vec<LayoutDebugMessage>>, message: &str) {
    if let Some(messages) = debug_messages {
        messages.push(LayoutDebugMessage {
            message: message.into(),
            location: "ifc".into(),
        });
    }
}
