use std::collections::{BTreeMap, BTreeSet};

use azul_core::{
    app_resources::{ImageCache, RendererResources},
    callbacks::DocumentId,
    dom::NodeId,
    styled_dom::{ChangedCssProperty, DomId, StyledDom},
    ui_solver::{
        FormattingContext, GpuEventChanges, IntrinsicSizes, LayoutDebugMessage, LayoutResult,
        PositionedRectangle, RelayoutChanges,
    },
    window::{LogicalPosition, LogicalRect, LogicalSize},
};
use azul_css::{AzString, CssProperty, CssPropertyType, LayoutRect};

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

    // Propagate to ancestors
    for node_id in affected_nodes {
        let mut current_id = Some(*node_id);
        while let Some(id) = current_id {
            if let Some(parent_id) = styled_dom.node_hierarchy.as_container()[id].parent_id() {
                all_affected_nodes.insert(parent_id);
                current_id = Some(parent_id);
            } else {
                current_id = None;
            }
        }
    }

    // Propagate to descendants
    let mut descendants = BTreeSet::new();
    for node_id in &all_affected_nodes {
        // Get all children recursively
        styled_dom
            .get_subtree(*node_id)
            .into_iter()
            .for_each(|child_id| {
                descendants.insert(child_id);
            });
    }
    all_affected_nodes.extend(descendants);

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

// Add unit tests for the incremental layout system
#[cfg(test)]
mod tests {
    use azul_core::{
        app_resources::{FontInstanceKey, RendererResourcesTrait},
        dom::{Dom, NodeData, NodeType},
        id_tree::NodeId,
        styled_dom::{CssPropertyCache, StyledDom, StyledNodeState},
    };
    use azul_css::{
        parser::CssApiWrapper, CssProperty, CssPropertyType, CssPropertyValue, LayoutDisplay,
        LayoutHeight, LayoutWidth, PixelValue, StyleOpacity,
    };

    use super::*;

    #[test]
    fn test_property_affects_formatting_context() {
        assert!(affects_formatting_context(CssPropertyType::Display));
        assert!(affects_formatting_context(CssPropertyType::Position));
        assert!(affects_formatting_context(CssPropertyType::Float));

        assert!(!affects_formatting_context(CssPropertyType::Width));
        assert!(!affects_formatting_context(CssPropertyType::Height));
        assert!(!affects_formatting_context(CssPropertyType::Opacity));
    }

    #[test]
    fn test_property_affects_intrinsic_size() {
        assert!(affects_intrinsic_size(CssPropertyType::Width));
        assert!(affects_intrinsic_size(CssPropertyType::Height));
        assert!(affects_intrinsic_size(CssPropertyType::MinWidth));
        assert!(affects_intrinsic_size(CssPropertyType::FontSize));

        assert!(!affects_intrinsic_size(CssPropertyType::Display));
        assert!(!affects_intrinsic_size(CssPropertyType::Opacity));
        assert!(!affects_intrinsic_size(CssPropertyType::TextColor));
    }

    // Helper function to create test DOM
    fn create_test_dom() -> StyledDom {
        Dom::new(NodeType::Body)
            .with_children(
                vec![
                    Dom::from_data(NodeData::text("Hello")),
                    Dom::from_data(NodeData::div())
                        .with_children(vec![Dom::from_data(NodeData::text("World"))].into()),
                ]
                .into(),
            )
            .style(CssApiWrapper::empty())
    }

    #[test]
    fn test_determine_affected_nodes() {
        let styled_dom = create_test_dom();

        // Create a map of property changes
        let mut nodes_to_relayout = BTreeMap::new();

        // Node 1: Width change (affects layout)
        let width_change = ChangedCssProperty {
            previous_state: StyledNodeState::new(),
            previous_prop: CssProperty::Width(CssPropertyValue::Exact(LayoutWidth::const_px(100))),
            current_state: StyledNodeState::new(),
            current_prop: CssProperty::Width(CssPropertyValue::Exact(LayoutWidth::const_px(200))),
        };
        nodes_to_relayout.insert(NodeId::new(1), vec![width_change]);

        // Node 2: Opacity change (doesn't affect layout)
        let opacity_change = ChangedCssProperty {
            previous_state: StyledNodeState::new(),
            previous_prop: CssProperty::Opacity(CssPropertyValue::Exact(StyleOpacity::const_new(
                50,
            ))),
            current_state: StyledNodeState::new(),
            current_prop: CssProperty::Opacity(CssPropertyValue::Exact(StyleOpacity::const_new(
                80,
            ))),
        };
        nodes_to_relayout.insert(NodeId::new(2), vec![opacity_change]);

        let mut debug_messages = None;
        let affected_nodes =
            determine_affected_nodes(&styled_dom, &nodes_to_relayout, &mut debug_messages);

        // Node 1 should be affected, Node 2 should not be
        assert!(affected_nodes.contains(&NodeId::new(1)));
        assert!(!affected_nodes.contains(&NodeId::new(2)));

        // Node 0 (parent of Node 1) should be affected
        assert!(affected_nodes.contains(&NodeId::ZERO));
    }

    #[test]
    fn test_propagate_layout_changes() {
        let styled_dom = create_test_dom();

        // Start with Node 1 affected
        let mut affected_nodes = BTreeSet::new();
        affected_nodes.insert(NodeId::new(1));

        let mut debug_messages = Some(Vec::new());
        let all_affected =
            propagate_layout_changes(&styled_dom, &affected_nodes, &mut debug_messages);

        println!("all_affected: {all_affected:?}");
        println!("debug_messages: {:?}", debug_messages.unwrap_or_default());

        // Node 0 (parent) should be affected
        assert!(all_affected.contains(&NodeId::ZERO));

        // Node 1 should still be affected
        assert!(all_affected.contains(&NodeId::new(1)));

        // Node 2 and 3 should not be affected (not in the subtree of Node 1)
        assert!(!all_affected.contains(&NodeId::new(2)));
        assert!(!all_affected.contains(&NodeId::new(3)));

        // Now start with Node 2 affected
        let mut affected_nodes = BTreeSet::new();
        affected_nodes.insert(NodeId::new(2));

        let all_affected =
            propagate_layout_changes(&styled_dom, &affected_nodes, &mut debug_messages);

        // Node 0 (parent) should be affected
        assert!(all_affected.contains(&NodeId::ZERO));

        // Node 2 should still be affected
        assert!(all_affected.contains(&NodeId::new(2)));

        // Node 3 should be affected (child of Node 2)
        assert!(all_affected.contains(&NodeId::new(3)));

        // Node 1 should not be affected (not in the subtree of Node 2)
        assert!(!all_affected.contains(&NodeId::new(1)));
    }
}
