use std::collections::{BTreeMap, BTreeSet};

use azul_core::{
    app_resources::{ImageCache, RendererResources},
    callbacks::DocumentId,
    dom::NodeId,
    styled_dom::{ChangedCssProperty, DomId, StyledDom},
    ui_solver::{
        FormattingContext, GpuEventChanges, IntrinsicSizes, LayoutResult, PositionedRectangle,
        RelayoutChanges,
    },
    window::{LogicalPosition, LogicalRect, LogicalSize},
};
use azul_css::{
    props::{
        basic::LayoutRect,
        property::{CssProperty, CssPropertyType},
    },
    AzString, LayoutDebugMessage,
};

use super::{
    context::determine_formatting_contexts, intrinsic::calculate_intrinsic_sizes,
    layout::calculate_layout,
};

/// Determines which nodes need to be re-layouted based on property changes
pub fn determine_affected_nodes(
    styled_dom: &StyledDom,
    nodes_to_relayout: &BTreeMap<NodeId, Vec<ChangedCssProperty>>,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> BTreeSet<NodeId> {
    let mut affected_nodes = BTreeSet::new();

    for (node_id, properties) in nodes_to_relayout {
        // Check if any property can trigger relayout
        for prop in properties {
            let prop_type = prop.previous_prop.get_type();
            if prop_type.can_trigger_relayout() {
                affected_nodes.insert(*node_id);

                // Also add parent to affected nodes
                if let Some(parent_id) =
                    styled_dom.node_hierarchy.as_container()[*node_id].parent_id()
                {
                    affected_nodes.insert(parent_id);
                }

                // No need to check other properties for this node
                break;
            }
        }
    }

    if let Some(messages) = debug_messages {
        messages.push(LayoutDebugMessage {
            message: format!(
                "Determined {} nodes affected by property changes",
                affected_nodes.len()
            )
            .into(),
            location: "determine_affected_nodes".to_string().into(),
        });
    }

    affected_nodes
}

/// Propagates layout changes up to ancestors and down to descendants
pub fn propagate_layout_changes(
    styled_dom: &StyledDom,
    affected_nodes: &BTreeSet<NodeId>,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> BTreeSet<NodeId> {
    let mut all_affected_nodes = affected_nodes.clone();

    // Propagate to descendants
    let mut descendants = BTreeSet::new();
    for node_id in &all_affected_nodes {
        // Get all children recursively
        styled_dom
            .get_subtree(*node_id)
            .into_iter()
            .for_each(|child_id| {
                if let Some(messages) = debug_messages {
                    messages.push(LayoutDebugMessage {
                        message: format!(
                            "Layout changes: inserting node {child_id:?} to be affected as \
                             descendant of node {child_id:?}",
                        )
                        .into(),
                        location: "propagate_layout_changes".to_string().into(),
                    });
                }

                descendants.insert(child_id);
            });
    }
    all_affected_nodes.extend(descendants);

    // Propagate to ancestors
    for node_id in affected_nodes {
        let mut current_id = Some(*node_id);

        if let Some(messages) = debug_messages {
            messages.push(LayoutDebugMessage {
                message: format!("Propagating layout changes to all parents of {current_id:?}",)
                    .into(),
                location: "propagate_layout_changes".to_string().into(),
            });
        }

        while let Some(id) = current_id {
            if let Some(parent_id) = styled_dom.node_hierarchy.as_container()[id].parent_id() {
                all_affected_nodes.insert(parent_id);
                current_id = Some(parent_id);
            } else {
                current_id = None;
            }
        }
    }

    if let Some(messages) = debug_messages {
        messages.push(LayoutDebugMessage {
            message: format!(
                "Propagated layout changes to {} total nodes",
                all_affected_nodes.len()
            )
            .into(),
            location: "propagate_layout_changes".to_string().into(),
        });
    }

    all_affected_nodes
}

/// Checks if a property change can affect formatting context
pub fn affects_formatting_context(property_type: CssPropertyType) -> bool {
    matches!(
        property_type,
        CssPropertyType::Display | CssPropertyType::Position | CssPropertyType::Float
    )
}

/// Checks if a property change can affect intrinsic sizes
pub fn affects_intrinsic_size(property_type: CssPropertyType) -> bool {
    matches!(
        property_type,
        CssPropertyType::Width
            | CssPropertyType::MinWidth
            | CssPropertyType::MaxWidth
            | CssPropertyType::Height
            | CssPropertyType::MinHeight
            | CssPropertyType::MaxHeight
            | CssPropertyType::PaddingLeft
            | CssPropertyType::PaddingRight
            | CssPropertyType::PaddingTop
            | CssPropertyType::PaddingBottom
            | CssPropertyType::FontSize
            | CssPropertyType::FontFamily
            | CssPropertyType::LineHeight
            | CssPropertyType::LetterSpacing
            | CssPropertyType::WordSpacing
    )
}

/// Optimized relayout function that only updates what's necessary
pub fn do_the_incremental_relayout(
    dom_id: DomId,
    root_bounds: LayoutRect,
    layout_result: &mut LayoutResult,
    image_cache: &ImageCache,
    renderer_resources: &mut RendererResources,
    document_id: &DocumentId,
    nodes_to_relayout: Option<&BTreeMap<NodeId, Vec<ChangedCssProperty>>>,
    words_to_relayout: Option<&BTreeMap<NodeId, AzString>>,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> RelayoutChanges {
    // Early return if nothing needs to be recomputed
    let root_size = root_bounds.size;
    let root_size_changed = root_bounds != layout_result.get_bounds();

    if !root_size_changed && nodes_to_relayout.is_none() && words_to_relayout.is_none() {
        return RelayoutChanges::empty();
    }

    if let Some(messages) = debug_messages {
        messages.push(LayoutDebugMessage {
            message: format!(
                "Starting incremental relayout: root_size_changed={}, nodes_changed={}, \
                 words_changed={}",
                root_size_changed,
                nodes_to_relayout.as_ref().map_or(0, |m| m.len()),
                words_to_relayout.as_ref().map_or(0, |m| m.len())
            )
            .into(),
            location: "do_the_incremental_relayout".to_string().into(),
        });
    }

    // 1. Determine which nodes need format context updates
    let mut need_format_context_update = false;
    if let Some(nodes) = nodes_to_relayout {
        for (_, properties) in nodes {
            for prop in properties {
                if affects_formatting_context(prop.previous_prop.get_type()) {
                    need_format_context_update = true;
                    break;
                }
            }
            if need_format_context_update {
                break;
            }
        }
    }

    // 2. Determine which nodes need intrinsic size updates
    let mut need_intrinsic_size_update = false;
    if let Some(nodes) = nodes_to_relayout {
        for (_, properties) in nodes {
            for prop in properties {
                if affects_intrinsic_size(prop.previous_prop.get_type()) {
                    need_intrinsic_size_update = true;
                    break;
                }
            }
            if need_intrinsic_size_update {
                break;
            }
        }
    }

    // Words changes always need intrinsic size update
    if words_to_relayout.is_some() {
        need_intrinsic_size_update = true;
    }

    // Root size change always requires full update
    if root_size_changed {
        need_format_context_update = true;
        need_intrinsic_size_update = true;
    }

    // 3. Update format contexts if needed
    if need_format_context_update {
        layout_result.formatting_contexts =
            determine_formatting_contexts(&layout_result.styled_dom);

        if let Some(messages) = debug_messages {
            messages.push(LayoutDebugMessage {
                message: "Updated formatting contexts".into(),
                location: "do_the_incremental_relayout".to_string().into(),
            });
        }
    }

    // 4. Update intrinsic sizes if needed
    if need_intrinsic_size_update {
        layout_result.intrinsic_sizes = calculate_intrinsic_sizes(
            &layout_result.styled_dom,
            &layout_result.formatting_contexts,
            renderer_resources,
        );

        if let Some(messages) = debug_messages {
            messages.push(LayoutDebugMessage {
                message: "Updated intrinsic sizes".into(),
                location: "do_the_incremental_relayout".to_string().into(),
            });
        }
    }

    // 5. Determine affected nodes
    let mut affected_nodes = BTreeSet::new();

    // Add nodes with changed properties
    if let Some(nodes) = nodes_to_relayout {
        let nodes_with_layout_changes =
            determine_affected_nodes(&layout_result.styled_dom, nodes, debug_messages);
        affected_nodes.extend(nodes_with_layout_changes);
    }

    // Add nodes with text changes
    if let Some(words) = words_to_relayout {
        for node_id in words.keys() {
            affected_nodes.insert(*node_id);

            // Add parent to affected nodes
            if let Some(parent_id) =
                layout_result.styled_dom.node_hierarchy.as_container()[*node_id].parent_id()
            {
                affected_nodes.insert(parent_id);
            }
        }
    }

    // Add root node if root size changed
    if root_size_changed {
        let root_id = layout_result
            .styled_dom
            .root
            .into_crate_internal()
            .unwrap_or(NodeId::ZERO);
        affected_nodes.insert(root_id);
    }

    // 6. Propagate changes to related nodes
    let nodes_to_update =
        propagate_layout_changes(&layout_result.styled_dom, &affected_nodes, debug_messages);

    // 7. Convert LayoutRect to LogicalRect for layout calculation
    let logical_bounds = LogicalRect::new(
        LogicalPosition::new(root_bounds.origin.x as f32, root_bounds.origin.y as f32),
        LogicalSize::new(
            root_bounds.size.width as f32,
            root_bounds.size.height as f32,
        ),
    );

    // 8. Store original rects for comparison
    let original_rects: BTreeMap<NodeId, PositionedRectangle> = nodes_to_update
        .iter()
        .filter_map(|node_id| {
            layout_result
                .rects
                .as_ref()
                .get(*node_id)
                .map(|rect| (*node_id, rect.clone()))
        })
        .collect();

    // 9. Perform layout update
    // For incremental optimization, this could be improved to only layout affected subtrees
    // But for now, we'll use the existing calculate_layout function
    let updated_layout_result = calculate_layout(
        dom_id,
        &layout_result.styled_dom,
        layout_result.formatting_contexts.clone(),
        layout_result.intrinsic_sizes.clone(),
        logical_bounds,
        renderer_resources,
        debug_messages,
    );

    // 10. Track which nodes changed size
    let mut resized_nodes = Vec::new();
    for node_id in &nodes_to_update {
        if let (Some(original), Some(updated)) = (
            original_rects.get(node_id),
            updated_layout_result.rects.as_ref().get(*node_id),
        ) {
            if original.size != updated.size || original.position != updated.position {
                resized_nodes.push(*node_id);
            }
        }
    }

    // 11. Update layout result fields
    layout_result.rects = updated_layout_result.rects;
    layout_result.scrollable_nodes = updated_layout_result.scrollable_nodes;
    layout_result.words_cache = updated_layout_result.words_cache;
    layout_result.shaped_words_cache = updated_layout_result.shaped_words_cache;
    layout_result.positioned_words_cache = updated_layout_result.positioned_words_cache;
    layout_result.root_size = root_size;
    layout_result.root_position = root_bounds.origin;

    // 12. Update GPU cache
    let gpu_key_changes = layout_result
        .gpu_value_cache
        .synchronize(&layout_result.rects.as_ref(), &layout_result.styled_dom);

    if let Some(messages) = debug_messages {
        messages.push(LayoutDebugMessage {
            message: format!(
                "Incremental relayout completed, {} nodes resized",
                resized_nodes.len()
            )
            .into(),
            location: "do_the_incremental_relayout".to_string().into(),
        });
    }

    RelayoutChanges {
        resized_nodes,
        gpu_key_changes,
    }
}
