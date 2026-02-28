//! Unified Restyle Architecture Tests
//!
//! These tests verify that the unified restyle system correctly:
//! 1. Updates StyledNodeState when focus/hover/active changes
//! 2. Computes which CSS properties changed
//! 3. Determines whether layout or display list updates are needed
//! 4. Handles GPU-only property changes efficiently
//!
//! The architecture ensures that:
//! - FocusManager state → StyledNodeState.focused is synchronized
//! - HoverManager state → StyledNodeState.hover is synchronized  
//! - ActiveChange state → StyledNodeState.active is synchronized

use azul_core::{
    dom::{Dom, NodeId},
    styled_dom::{ActiveChange, FocusChange, HoverChange, RestyleResult, StyledDom},
};
use azul_css::{
    css::{Css, CssPropertyValue},
    dynamic_selector::CssPropertyWithConditions,
    props::{
        basic::ColorU,
        layout::LayoutWidth,
        property::CssProperty,
        style::{
            background::{StyleBackgroundContent, StyleBackgroundContentVec},
            effects::StyleOpacity,
        },
    },
};

// ==================== Helper Functions ====================

fn empty_css() -> Css {
    Css::empty()
}

/// Creates a simple styled DOM with 3 div children that have :focus, :hover, :active inline CSS
fn create_test_dom_with_pseudo_states() -> StyledDom {
    let mut dom = Dom::create_div();
    
    // Add three children with inline pseudo-state CSS
    for _i in 0..3 {
        let mut child = Dom::create_div();
        
        // Normal state: red background
        let bg: StyleBackgroundContentVec = vec![StyleBackgroundContent::Color(ColorU {
            r: 231, g: 76, b: 60, a: 255, // #e74c3c
        })].into();
        child.root.add_css_property(CssPropertyWithConditions::simple(CssProperty::BackgroundContent(CssPropertyValue::Exact(bg))));

        // Focus state: light red background
        let focus_bg: StyleBackgroundContentVec = vec![StyleBackgroundContent::Color(ColorU {
            r: 255, g: 107, b: 107, a: 255, // #ff6b6b
        })].into();
        child.root.add_css_property(CssPropertyWithConditions::on_focus(CssProperty::BackgroundContent(CssPropertyValue::Exact(focus_bg))));

        // Hover state: blue background
        let hover_bg: StyleBackgroundContentVec = vec![StyleBackgroundContent::Color(ColorU {
            r: 52, g: 152, b: 219, a: 255, // #3498db
        })].into();
        child.root.add_css_property(CssPropertyWithConditions::on_hover(CssProperty::BackgroundContent(CssPropertyValue::Exact(hover_bg))));

        // Active state: green background
        let active_bg: StyleBackgroundContentVec = vec![StyleBackgroundContent::Color(ColorU {
            r: 46, g: 204, b: 113, a: 255, // #2ecc71
        })].into();
        child.root.add_css_property(CssPropertyWithConditions::on_active(CssProperty::BackgroundContent(CssPropertyValue::Exact(active_bg))));
        
        dom.add_child(child);
    }
    
    StyledDom::create(&mut dom, empty_css())
}

/// Creates a DOM with layout-affecting :focus CSS (width changes on focus)
fn create_test_dom_with_layout_pseudo_states() -> StyledDom {
    let mut dom = Dom::create_div();
    
    let mut child = Dom::create_div();
    
    // Normal state: 100px width
    child.root.add_css_property(CssPropertyWithConditions::simple(CssProperty::Width(CssPropertyValue::Exact(LayoutWidth::const_px(100)))));

    // Focus state: 200px width (layout-affecting!)
    child.root.add_css_property(CssPropertyWithConditions::on_focus(CssProperty::Width(CssPropertyValue::Exact(LayoutWidth::const_px(200)))));
    
    dom.add_child(child);
    StyledDom::create(&mut dom, empty_css())
}

/// Creates a DOM with GPU-only :focus CSS (opacity changes on focus)
fn create_test_dom_with_gpu_only_pseudo_states() -> StyledDom {
    let mut dom = Dom::create_div();
    
    let mut child = Dom::create_div();
    
    // Normal state: opacity 1.0
    child.root.add_css_property(CssPropertyWithConditions::simple(CssProperty::Opacity(CssPropertyValue::Exact(StyleOpacity::const_new(100)))));

    // Focus state: opacity 0.5 (GPU-only!)
    child.root.add_css_property(CssPropertyWithConditions::on_focus(CssProperty::Opacity(CssPropertyValue::Exact(StyleOpacity::const_new(50)))));
    
    dom.add_child(child);
    StyledDom::create(&mut dom, empty_css())
}

// ==================== Focus Change Tests ====================

#[test]
fn test_restyle_focus_change_updates_styled_node_state() {
    let mut styled_dom = create_test_dom_with_pseudo_states();
    
    // Node 1 is the first child div (index 0 is root)
    let node_id = NodeId::new(1);
    
    // Initially, node should not be focused
    let initial_state = styled_dom.styled_nodes.as_container()[node_id].styled_node_state;
    assert!(!initial_state.focused, "Node should not be focused initially");
    
    // Apply focus change
    let result = styled_dom.restyle_on_state_change(
        Some(FocusChange {
            lost_focus: None,
            gained_focus: Some(node_id),
        }),
        None,
        None,
    );
    
    // Node should now be focused
    let new_state = styled_dom.styled_nodes.as_container()[node_id].styled_node_state;
    assert!(new_state.focused, "Node should be focused after restyle");
    
    // Result should indicate changes occurred
    assert!(result.has_changes(), "Restyle should report changes");
    assert!(result.changed_nodes.contains_key(&node_id), "Changed nodes should include the focused node");
}

#[test]
fn test_restyle_focus_lost_updates_styled_node_state() {
    let mut styled_dom = create_test_dom_with_pseudo_states();
    let node_id = NodeId::new(1);
    
    // First, focus the node
    styled_dom.restyle_on_state_change(
        Some(FocusChange {
            lost_focus: None,
            gained_focus: Some(node_id),
        }),
        None,
        None,
    );
    
    // Verify focused
    assert!(styled_dom.styled_nodes.as_container()[node_id].styled_node_state.focused);
    
    // Now remove focus
    let result = styled_dom.restyle_on_state_change(
        Some(FocusChange {
            lost_focus: Some(node_id),
            gained_focus: None,
        }),
        None,
        None,
    );
    
    // Node should no longer be focused
    let final_state = styled_dom.styled_nodes.as_container()[node_id].styled_node_state;
    assert!(!final_state.focused, "Node should not be focused after losing focus");
    assert!(result.has_changes(), "Restyle should report changes when focus lost");
}

#[test]
fn test_restyle_focus_transfer_between_nodes() {
    let mut styled_dom = create_test_dom_with_pseudo_states();
    let node1 = NodeId::new(1);
    let node2 = NodeId::new(2);
    
    // Focus node1
    styled_dom.restyle_on_state_change(
        Some(FocusChange {
            lost_focus: None,
            gained_focus: Some(node1),
        }),
        None,
        None,
    );
    
    assert!(styled_dom.styled_nodes.as_container()[node1].styled_node_state.focused);
    assert!(!styled_dom.styled_nodes.as_container()[node2].styled_node_state.focused);
    
    // Transfer focus from node1 to node2
    let result = styled_dom.restyle_on_state_change(
        Some(FocusChange {
            lost_focus: Some(node1),
            gained_focus: Some(node2),
        }),
        None,
        None,
    );
    
    // node1 should lose focus, node2 should gain focus
    assert!(!styled_dom.styled_nodes.as_container()[node1].styled_node_state.focused,
            "node1 should lose focus");
    assert!(styled_dom.styled_nodes.as_container()[node2].styled_node_state.focused,
            "node2 should gain focus");
    
    // Both nodes should have changes
    assert!(result.changed_nodes.contains_key(&node1), "node1 should have CSS changes");
    assert!(result.changed_nodes.contains_key(&node2), "node2 should have CSS changes");
}

// ==================== Hover Change Tests ====================

#[test]
fn test_restyle_hover_enter_updates_styled_node_state() {
    let mut styled_dom = create_test_dom_with_pseudo_states();
    let node_id = NodeId::new(1);
    
    // Initially not hovered
    assert!(!styled_dom.styled_nodes.as_container()[node_id].styled_node_state.hover);
    
    // Mouse enters node
    let result = styled_dom.restyle_on_state_change(
        None,
        Some(HoverChange {
            left_nodes: vec![],
            entered_nodes: vec![node_id],
        }),
        None,
    );
    
    // Node should now be hovered
    assert!(styled_dom.styled_nodes.as_container()[node_id].styled_node_state.hover,
            "Node should be hovered after mouse enter");
    assert!(result.has_changes(), "Restyle should report changes");
}

#[test]
fn test_restyle_hover_leave_updates_styled_node_state() {
    let mut styled_dom = create_test_dom_with_pseudo_states();
    let node_id = NodeId::new(1);
    
    // First hover the node
    styled_dom.restyle_on_state_change(
        None,
        Some(HoverChange {
            left_nodes: vec![],
            entered_nodes: vec![node_id],
        }),
        None,
    );
    
    assert!(styled_dom.styled_nodes.as_container()[node_id].styled_node_state.hover);
    
    // Mouse leaves node
    let result = styled_dom.restyle_on_state_change(
        None,
        Some(HoverChange {
            left_nodes: vec![node_id],
            entered_nodes: vec![],
        }),
        None,
    );
    
    // Node should no longer be hovered
    assert!(!styled_dom.styled_nodes.as_container()[node_id].styled_node_state.hover,
            "Node should not be hovered after mouse leave");
    assert!(result.has_changes(), "Restyle should report changes");
}

#[test]
fn test_restyle_hover_multiple_nodes() {
    let mut styled_dom = create_test_dom_with_pseudo_states();
    let node1 = NodeId::new(1);
    let node2 = NodeId::new(2);
    
    // Hover both nodes (can happen in nested elements)
    let result = styled_dom.restyle_on_state_change(
        None,
        Some(HoverChange {
            left_nodes: vec![],
            entered_nodes: vec![node1, node2],
        }),
        None,
    );
    
    assert!(styled_dom.styled_nodes.as_container()[node1].styled_node_state.hover);
    assert!(styled_dom.styled_nodes.as_container()[node2].styled_node_state.hover);
    assert!(result.changed_nodes.len() >= 2, "Multiple nodes should have changes");
}

// ==================== Active Change Tests ====================

#[test]
fn test_restyle_active_mouse_down_updates_styled_node_state() {
    let mut styled_dom = create_test_dom_with_pseudo_states();
    let node_id = NodeId::new(1);
    
    // Initially not active
    assert!(!styled_dom.styled_nodes.as_container()[node_id].styled_node_state.active);
    
    // Mouse down on node
    let result = styled_dom.restyle_on_state_change(
        None,
        None,
        Some(ActiveChange {
            deactivated: vec![],
            activated: vec![node_id],
        }),
    );
    
    // Node should now be active
    assert!(styled_dom.styled_nodes.as_container()[node_id].styled_node_state.active,
            "Node should be active after mouse down");
    assert!(result.has_changes(), "Restyle should report changes");
}

#[test]
fn test_restyle_active_mouse_up_updates_styled_node_state() {
    let mut styled_dom = create_test_dom_with_pseudo_states();
    let node_id = NodeId::new(1);
    
    // First activate the node
    styled_dom.restyle_on_state_change(
        None,
        None,
        Some(ActiveChange {
            deactivated: vec![],
            activated: vec![node_id],
        }),
    );
    
    assert!(styled_dom.styled_nodes.as_container()[node_id].styled_node_state.active);
    
    // Mouse up
    let result = styled_dom.restyle_on_state_change(
        None,
        None,
        Some(ActiveChange {
            deactivated: vec![node_id],
            activated: vec![],
        }),
    );
    
    // Node should no longer be active
    assert!(!styled_dom.styled_nodes.as_container()[node_id].styled_node_state.active,
            "Node should not be active after mouse up");
    assert!(result.has_changes(), "Restyle should report changes");
}

// ==================== Combined State Tests ====================

#[test]
fn test_restyle_combined_focus_and_hover() {
    let mut styled_dom = create_test_dom_with_pseudo_states();
    let node_id = NodeId::new(1);
    
    // Apply both focus and hover at once
    let result = styled_dom.restyle_on_state_change(
        Some(FocusChange {
            lost_focus: None,
            gained_focus: Some(node_id),
        }),
        Some(HoverChange {
            left_nodes: vec![],
            entered_nodes: vec![node_id],
        }),
        None,
    );
    
    let state = styled_dom.styled_nodes.as_container()[node_id].styled_node_state;
    assert!(state.focused, "Node should be focused");
    assert!(state.hover, "Node should be hovered");
    assert!(result.has_changes(), "Restyle should report changes");
}

#[test]
fn test_restyle_all_states_combined() {
    let mut styled_dom = create_test_dom_with_pseudo_states();
    let node_id = NodeId::new(1);
    
    // Apply focus, hover, and active at once
    let result = styled_dom.restyle_on_state_change(
        Some(FocusChange {
            lost_focus: None,
            gained_focus: Some(node_id),
        }),
        Some(HoverChange {
            left_nodes: vec![],
            entered_nodes: vec![node_id],
        }),
        Some(ActiveChange {
            deactivated: vec![],
            activated: vec![node_id],
        }),
    );
    
    let state = styled_dom.styled_nodes.as_container()[node_id].styled_node_state;
    assert!(state.focused, "Node should be focused");
    assert!(state.hover, "Node should be hovered");
    assert!(state.active, "Node should be active");
    assert!(result.has_changes(), "Restyle should report changes");
}

// ==================== Layout vs Display List Tests ====================

#[test]
fn test_restyle_layout_property_sets_needs_layout() {
    let mut styled_dom = create_test_dom_with_layout_pseudo_states();
    let node_id = NodeId::new(1);
    
    // Focus should change width (layout property)
    let result = styled_dom.restyle_on_state_change(
        Some(FocusChange {
            lost_focus: None,
            gained_focus: Some(node_id),
        }),
        None,
        None,
    );
    
    // Width changes require layout
    assert!(result.needs_layout, 
            "Width change should require layout recalculation");
    assert!(result.needs_display_list,
            "Layout change should also require display list update");
    assert!(!result.gpu_only_changes,
            "Layout change is not GPU-only");
}

#[test]
fn test_restyle_visual_property_does_not_need_layout() {
    let mut styled_dom = create_test_dom_with_pseudo_states();
    let node_id = NodeId::new(1);
    
    // Focus should change background-color (visual property, not layout)
    let result = styled_dom.restyle_on_state_change(
        Some(FocusChange {
            lost_focus: None,
            gained_focus: Some(node_id),
        }),
        None,
        None,
    );
    
    // Background color doesn't affect layout
    assert!(!result.needs_layout, 
            "Background color change should NOT require layout");
    assert!(result.needs_display_list,
            "Background color change should require display list update");
}

#[test]
fn test_restyle_gpu_only_property() {
    let mut styled_dom = create_test_dom_with_gpu_only_pseudo_states();
    let node_id = NodeId::new(1);
    
    // Focus should change opacity (GPU-only property)
    let result = styled_dom.restyle_on_state_change(
        Some(FocusChange {
            lost_focus: None,
            gained_focus: Some(node_id),
        }),
        None,
        None,
    );
    
    // Opacity is GPU-only
    assert!(!result.needs_layout, 
            "Opacity change should NOT require layout");
    // Note: Display list still needs update even for GPU-only, 
    // but GPU-only flag indicates optimization opportunity
    if result.has_changes() {
        assert!(result.gpu_only_changes,
                "Opacity-only change should be GPU-only");
    }
}

// ==================== No-Change Tests ====================

#[test]
fn test_restyle_empty_changes_returns_no_changes() {
    let mut styled_dom = create_test_dom_with_pseudo_states();
    
    // Call restyle with no changes
    let result = styled_dom.restyle_on_state_change(None, None, None);
    
    assert!(!result.has_changes(), "Empty restyle should have no changes");
    assert!(!result.needs_layout, "Empty restyle should not need layout");
    assert!(!result.needs_display_list, "Empty restyle should not need display list");
}

// ==================== RestyleResult Tests ====================

#[test]
fn test_restyle_result_merge() {
    let mut result1 = RestyleResult::default();
    result1.changed_nodes.insert(NodeId::new(1), vec![]);
    result1.needs_layout = false;
    result1.needs_display_list = true;
    result1.gpu_only_changes = true;
    
    let mut result2 = RestyleResult::default();
    result2.changed_nodes.insert(NodeId::new(2), vec![]);
    result2.needs_layout = true;
    result2.needs_display_list = true;
    result2.gpu_only_changes = false;
    
    result1.merge(result2);
    
    assert!(result1.changed_nodes.contains_key(&NodeId::new(1)));
    assert!(result1.changed_nodes.contains_key(&NodeId::new(2)));
    assert!(result1.needs_layout, "Merged result should need layout if any did");
    assert!(result1.needs_display_list, "Merged result should need display list");
    assert!(!result1.gpu_only_changes, "GPU-only should be false if any was false");
}

#[test]
fn test_restyle_result_has_changes() {
    let mut result = RestyleResult::default();
    assert!(!result.has_changes(), "Empty result should not have changes");
    
    result.changed_nodes.insert(NodeId::new(1), vec![]);
    assert!(result.has_changes(), "Result with nodes should have changes");
}

// ==================== Edge Case Tests ====================

#[test]
fn test_restyle_preserves_other_state_flags() {
    let mut styled_dom = create_test_dom_with_pseudo_states();
    let node_id = NodeId::new(1);
    
    // First set hover and active
    styled_dom.restyle_on_state_change(
        None,
        Some(HoverChange {
            left_nodes: vec![],
            entered_nodes: vec![node_id],
        }),
        Some(ActiveChange {
            deactivated: vec![],
            activated: vec![node_id],
        }),
    );
    
    // Now add focus - should preserve hover and active
    styled_dom.restyle_on_state_change(
        Some(FocusChange {
            lost_focus: None,
            gained_focus: Some(node_id),
        }),
        None,
        None,
    );
    
    let state = styled_dom.styled_nodes.as_container()[node_id].styled_node_state;
    assert!(state.focused, "Should be focused");
    assert!(state.hover, "Should still be hovered");
    assert!(state.active, "Should still be active");
}
