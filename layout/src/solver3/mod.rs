//! solver3/mod.rs
//!
//! Next-generation CSS layout engine with proper formatting context separation

pub mod display_list;
pub mod fc;
pub mod layout_tree;
pub mod positioning;
pub mod sizing;

use std::sync::Arc;

use azul_core::{
    app_resources::RendererResources,
    callbacks::DocumentId,
    styled_dom::{DomId, StyledDom},
    window::{LogicalRect, LogicalSize},
};
use azul_css::LayoutDebugMessage;

use self::{
    display_list::generate_display_list,
    layout_tree::{generate_layout_tree, LayoutTree},
    positioning::calculate_positions,
    sizing::{calculate_intrinsic_sizes, calculate_used_sizes},
};
use crate::solver3::fc::LayoutResult;

/// Main entry point for solver3 layout engine
pub fn layout_document(
    dom_id: DomId,
    parent_dom_id: Option<DomId>,
    styled_dom: StyledDom,
    renderer_resources: &mut RendererResources,
    document_id: &DocumentId,
    viewport: LogicalRect,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> LayoutResult {
    debug_log(debug_messages, "Starting solver3 layout pipeline");

    // Pass 1: Generate layout tree with anonymous boxes
    let layout_tree = generate_layout_tree(&styled_dom, debug_messages)?;
    debug_log(debug_messages, "Pass 1: Generated layout tree");

    // Pass 2: Calculate sizes (bottom-up then top-down)
    let intrinsic_sizes = calculate_intrinsic_sizes(
        &layout_tree,
        &styled_dom,
        renderer_resources,
        debug_messages,
    ).unwrap_or_default(); // <- TODO: bug? ok for now.
    debug_log(debug_messages, "Pass 2a: Calculated intrinsic sizes");

    let used_sizes = calculate_used_sizes(
        &layout_tree,
        &intrinsic_sizes,
        viewport.size,
        debug_messages,
    )?;
    debug_log(debug_messages, "Pass 2b: Calculated used sizes");

    // Pass 3: Calculate final positions
    let positioned_tree = calculate_positions(
        &layout_tree,
        &used_sizes,
        &styled_dom,
        viewport,
        renderer_resources,
        debug_messages,
    )?;
    debug_log(debug_messages, "Pass 3: Calculated positions");

    // Pass 4: Generate display list
    let display_list = generate_display_list(&positioned_tree, &styled_dom, debug_messages)?;
    debug_log(debug_messages, "Pass 4: Generated display list");

    LayoutResult {
        dom_id,
        parent_dom_id,
        styled_dom,
        rects: positioned_tree.get_rectangles(),
        scrollable_nodes: Default::default(),
        iframe_mapping: Default::default(),
        word_positions: positioned_tree.get_word_positions(),
        display_list: Some(display_list),
        // positions:
        // overflow_size:
        // baseline_offset:
    }
}

fn debug_log(debug_messages: &mut Option<Vec<LayoutDebugMessage>>, message: &str) {
    if let Some(messages) = debug_messages {
        messages.push(LayoutDebugMessage {
            message: message.into(),
            location: "solver3".into(),
        });
    }
}

#[derive(Debug)]
pub enum LayoutError {
    InvalidTree,
    SizingFailed,
    PositioningFailed,
    DisplayListFailed,
}

impl std::fmt::Display for LayoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LayoutError::InvalidTree => write!(f, "Invalid layout tree"),
            LayoutError::SizingFailed => write!(f, "Sizing calculation failed"),
            LayoutError::PositioningFailed => write!(f, "Position calculation failed"),
            LayoutError::DisplayListFailed => write!(f, "Display list generation failed"),
        }
    }
}

impl std::error::Error for LayoutError {}

type Result<T> = std::result::Result<T, LayoutError>;
