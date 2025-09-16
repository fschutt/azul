//! solver3/fc/mod.rs
//!
//! Formatting context managers for different CSS display types

pub mod bfc;
pub mod flex;
pub mod grid;
pub mod ifc;
pub mod table;
// pub mod table2;

use azul_core::{
    app_resources::RendererResources,
    styled_dom::StyledDom,
    ui_solver::FormattingContext,
    window::{LogicalPosition, LogicalSize},
};
use azul_css::LayoutDebugMessage;

use self::{
    bfc::BlockLayoutManager, flex::FlexLayoutManager, grid::GridLayoutManager,
    ifc::InlineLayoutManager, table::TableLayoutManager,
};
use crate::solver3::{
    layout_tree::{LayoutNode, LayoutTree},
    LayoutError, Result,
};

/// Available layout constraints for a formatting context
#[derive(Debug, Clone)]
pub struct LayoutConstraints {
    pub available_size: LogicalSize,
    pub writing_mode: WritingMode,
    pub text_align: TextAlign,
    pub definite_size: Option<LogicalSize>,
}

/// Writing mode support
#[derive(Debug, Clone, Copy, Default)]
pub enum WritingMode {
    #[default]
    HorizontalTb,
    VerticalRl,
    VerticalLr,
}

/// Text alignment options
#[derive(Debug, Clone, Copy, Default)]
pub enum TextAlign {
    #[default]
    Start,
    End,
    Center,
    Justify,
}

/// Result of a formatting context layout operation
#[derive(Debug)]
pub struct LayoutResult {
    pub positions: Vec<(usize, LogicalPosition)>,
    pub overflow_size: Option<LogicalSize>,
    pub baseline_offset: f32,
}

/// Main dispatcher for formatting context layout
pub fn layout_formatting_context(
    tree: &mut LayoutTree,
    node_index: usize,
    constraints: &LayoutConstraints,
    styled_dom: &StyledDom,
    renderer_resources: &mut RendererResources,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> Result<LayoutResult> {
    let node = tree.get(node_index).ok_or(LayoutError::InvalidTree)?;

    match &node.formatting_context {
        FormattingContext::Block {
            establishes_new_context,
        } => BlockLayoutManager::new().layout(
            tree,
            node_index,
            constraints,
            styled_dom,
            debug_messages,
        ),
        FormattingContext::Inline => InlineLayoutManager::new().layout(
            tree,
            node_index,
            constraints,
            styled_dom,
            renderer_resources,
            debug_messages,
        ),
        FormattingContext::Flex => FlexLayoutManager::new().layout(
            tree,
            node_index,
            constraints,
            styled_dom,
            debug_messages,
        ),
        FormattingContext::Grid => GridLayoutManager::new().layout(
            tree,
            node_index,
            constraints,
            styled_dom,
            debug_messages,
        ),
        FormattingContext::Table => TableLayoutManager::new().layout(
            tree,
            node_index,
            constraints,
            styled_dom,
            debug_messages,
        ),
    }
}

/// Trait for formatting context managers
pub trait FormattingContextManager {
    fn layout(
        &mut self,
        tree: &mut LayoutTree,
        node_index: usize,
        constraints: &LayoutConstraints,
        styled_dom: &StyledDom,
        debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    ) -> Result<LayoutResult>;
}

/// Helper to determine if scrollbars are needed
pub fn check_scrollbar_necessity(
    content_size: LogicalSize,
    container_size: LogicalSize,
    overflow_x: OverflowBehavior,
    overflow_y: OverflowBehavior,
) -> ScrollbarInfo {
    let needs_horizontal = match overflow_x {
        OverflowBehavior::Visible => false,
        OverflowBehavior::Hidden => false,
        OverflowBehavior::Scroll => true,
        OverflowBehavior::Auto => content_size.width > container_size.width,
    };

    let needs_vertical = match overflow_y {
        OverflowBehavior::Visible => false,
        OverflowBehavior::Hidden => false,
        OverflowBehavior::Scroll => true,
        OverflowBehavior::Auto => content_size.height > container_size.height,
    };

    ScrollbarInfo {
        needs_horizontal,
        needs_vertical,
        scrollbar_width: if needs_vertical { 16.0 } else { 0.0 },
        scrollbar_height: if needs_horizontal { 16.0 } else { 0.0 },
    }
}

#[derive(Debug, Clone, Copy)]
pub enum OverflowBehavior {
    Visible,
    Hidden,
    Scroll,
    Auto,
}

#[derive(Debug, Clone)]
pub struct ScrollbarInfo {
    pub needs_horizontal: bool,
    pub needs_vertical: bool,
    pub scrollbar_width: f32,
    pub scrollbar_height: f32,
}

/// Margin collapsing calculation for block layout
pub fn calculate_collapsed_margins(top_margin: f32, bottom_margin: f32, is_adjacent: bool) -> f32 {
    if !is_adjacent {
        return 0.0;
    }

    // Simplified margin collapsing - real implementation would be more complex
    if top_margin.signum() == bottom_margin.signum() {
        top_margin.abs().max(bottom_margin.abs()) * top_margin.signum()
    } else {
        top_margin + bottom_margin
    }
}

fn debug_log(debug_messages: &mut Option<Vec<LayoutDebugMessage>>, message: &str) {
    if let Some(messages) = debug_messages {
        messages.push(LayoutDebugMessage {
            message: message.into(),
            location: "formatting_contexts".into(),
        });
    }
}
