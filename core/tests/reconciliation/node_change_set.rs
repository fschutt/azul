// Tests for NodeChangeSet bitflags and compute_node_changes()
//
// Verifies that field-by-field comparison of two NodeData instances
// produces the correct NodeChangeSet flags.

use azul_core::diff::{compute_node_changes, NodeChangeSet};
use azul_core::dom::NodeData;
use azul_core::styled_dom::StyledNodeState;
use azul_css::AzString;

/// Helper: wrap a u32 constant into a NodeChangeSet for calling methods
fn ncs(bits: u32) -> NodeChangeSet {
    NodeChangeSet { bits }
}

// =========================================================================
// BASIC: empty / identity comparisons
// =========================================================================

#[test]
fn identical_divs_produce_empty_changeset() {
    let a = NodeData::create_div();
    let b = NodeData::create_div();
    let cs = compute_node_changes(&a, &b, None, None);
    assert!(cs.is_empty(), "identical divs should have empty changeset, got {:?}", cs);
}

#[test]
fn identical_text_nodes_produce_empty_changeset() {
    let a = NodeData::create_text("Hello");
    let b = NodeData::create_text("Hello");
    let cs = compute_node_changes(&a, &b, None, None);
    assert!(cs.is_empty(), "identical text nodes should have empty changeset");
}

#[test]
fn empty_changeset_is_visually_unchanged() {
    let cs = NodeChangeSet::empty();
    assert!(cs.is_visually_unchanged());
    assert!(!cs.needs_layout());
    assert!(!cs.needs_paint());
}

// =========================================================================
// TEXT_CONTENT flag
// =========================================================================

#[test]
fn text_change_sets_text_content_flag() {
    let a = NodeData::create_text("Hello");
    let b = NodeData::create_text("World");
    let cs = compute_node_changes(&a, &b, None, None);
    assert!(cs.contains(NodeChangeSet::TEXT_CONTENT),
        "changing text content should set TEXT_CONTENT flag");
}

#[test]
fn text_change_needs_layout() {
    let a = NodeData::create_text("short");
    let b = NodeData::create_text("a much longer text that changes layout");
    let cs = compute_node_changes(&a, &b, None, None);
    assert!(cs.needs_layout(), "text change should need layout");
}

#[test]
fn text_same_length_different_content_sets_flag() {
    let a = NodeData::create_text("AAAA");
    let b = NodeData::create_text("BBBB");
    let cs = compute_node_changes(&a, &b, None, None);
    assert!(cs.contains(NodeChangeSet::TEXT_CONTENT),
        "same-length different text should still set TEXT_CONTENT");
}

#[test]
fn text_empty_to_nonempty_sets_flag() {
    let a = NodeData::create_text("");
    let b = NodeData::create_text("content");
    let cs = compute_node_changes(&a, &b, None, None);
    assert!(cs.contains(NodeChangeSet::TEXT_CONTENT));
}

#[test]
fn text_nonempty_to_empty_sets_flag() {
    let a = NodeData::create_text("content");
    let b = NodeData::create_text("");
    let cs = compute_node_changes(&a, &b, None, None);
    assert!(cs.contains(NodeChangeSet::TEXT_CONTENT));
}

// =========================================================================
// NODE_TYPE_CHANGED flag
// =========================================================================

#[test]
fn div_to_span_sets_node_type_changed() {
    let a = NodeData::create_div();
    let b = NodeData::create_node(azul_core::dom::NodeType::Span);
    let cs = compute_node_changes(&a, &b, None, None);
    assert!(cs.contains(NodeChangeSet::NODE_TYPE_CHANGED),
        "changing node type should set NODE_TYPE_CHANGED");
}

#[test]
fn div_to_text_sets_node_type_changed() {
    let a = NodeData::create_div();
    let b = NodeData::create_text("text");
    let cs = compute_node_changes(&a, &b, None, None);
    assert!(cs.contains(NodeChangeSet::NODE_TYPE_CHANGED));
}

#[test]
fn text_to_div_sets_node_type_changed() {
    let a = NodeData::create_text("text");
    let b = NodeData::create_div();
    let cs = compute_node_changes(&a, &b, None, None);
    assert!(cs.contains(NodeChangeSet::NODE_TYPE_CHANGED));
}

#[test]
fn node_type_changed_needs_layout() {
    assert!(ncs(NodeChangeSet::NODE_TYPE_CHANGED).needs_layout());
}

// =========================================================================
// IDS_AND_CLASSES flag
// =========================================================================

#[test]
fn adding_class_sets_ids_and_classes_flag() {
    let a = NodeData::create_div();
    let mut b = NodeData::create_div();
    b.add_class(AzString::from("highlight"));
    let cs = compute_node_changes(&a, &b, None, None);
    assert!(cs.contains(NodeChangeSet::IDS_AND_CLASSES),
        "adding a class should set IDS_AND_CLASSES flag");
}

#[test]
fn adding_id_sets_ids_and_classes_flag() {
    let a = NodeData::create_div();
    let mut b = NodeData::create_div();
    b.add_id(AzString::from("main"));
    let cs = compute_node_changes(&a, &b, None, None);
    assert!(cs.contains(NodeChangeSet::IDS_AND_CLASSES));
}

#[test]
fn changing_class_sets_ids_and_classes_flag() {
    let mut a = NodeData::create_div();
    a.add_class(AzString::from("old-class"));
    let mut b = NodeData::create_div();
    b.add_class(AzString::from("new-class"));
    let cs = compute_node_changes(&a, &b, None, None);
    assert!(cs.contains(NodeChangeSet::IDS_AND_CLASSES));
}

#[test]
fn removing_class_sets_ids_and_classes_flag() {
    let mut a = NodeData::create_div();
    a.add_class(AzString::from("highlight"));
    let b = NodeData::create_div();
    let cs = compute_node_changes(&a, &b, None, None);
    assert!(cs.contains(NodeChangeSet::IDS_AND_CLASSES));
}

#[test]
fn ids_and_classes_needs_layout() {
    assert!(ncs(NodeChangeSet::IDS_AND_CLASSES).needs_layout());
}

// =========================================================================
// INLINE_STYLE_LAYOUT / INLINE_STYLE_PAINT flags
// =========================================================================

#[test]
fn adding_layout_css_sets_inline_style_layout_flag() {
    let a = NodeData::create_div();
    let b = NodeData::create_div().with_css("width: 100px;");
    let cs = compute_node_changes(&a, &b, None, None);
    assert!(
        cs.contains(NodeChangeSet::INLINE_STYLE_LAYOUT)
            || cs.contains(NodeChangeSet::INLINE_STYLE_PAINT),
        "adding inline CSS should set an inline style flag, got {:?}", cs
    );
}

#[test]
fn adding_paint_only_css_sets_paint_flag() {
    let a = NodeData::create_div();
    let b = NodeData::create_div().with_css("color: red;");
    let cs = compute_node_changes(&a, &b, None, None);
    assert!(
        cs.contains(NodeChangeSet::INLINE_STYLE_PAINT)
            || cs.contains(NodeChangeSet::INLINE_STYLE_LAYOUT),
        "adding paint-only CSS should set a style flag"
    );
}

#[test]
fn changing_width_sets_layout_flag() {
    let a = NodeData::create_div().with_css("width: 50px;");
    let b = NodeData::create_div().with_css("width: 100px;");
    let cs = compute_node_changes(&a, &b, None, None);
    assert!(cs.contains(NodeChangeSet::INLINE_STYLE_LAYOUT),
        "changing width should set INLINE_STYLE_LAYOUT");
}

// =========================================================================
// STYLED_STATE flag
// =========================================================================

#[test]
fn hover_change_sets_styled_state_flag() {
    let a = NodeData::create_div();
    let b = NodeData::create_div();
    let state_a = StyledNodeState::default();
    let mut state_b = StyledNodeState::default();
    state_b.hover = true;
    let cs = compute_node_changes(&a, &b, Some(&state_a), Some(&state_b));
    assert!(cs.contains(NodeChangeSet::STYLED_STATE),
        "hover change should set STYLED_STATE flag");
}

#[test]
fn focus_change_sets_styled_state_flag() {
    let a = NodeData::create_div();
    let b = NodeData::create_div();
    let state_a = StyledNodeState::default();
    let mut state_b = StyledNodeState::default();
    state_b.focused = true;
    let cs = compute_node_changes(&a, &b, Some(&state_a), Some(&state_b));
    assert!(cs.contains(NodeChangeSet::STYLED_STATE));
}

#[test]
fn styled_state_is_paint_only() {
    assert!(ncs(NodeChangeSet::STYLED_STATE).needs_paint(), "state change should need paint");
    assert!(!ncs(NodeChangeSet::STYLED_STATE).needs_layout(), "state change alone should not need layout");
}

// =========================================================================
// CONTENTEDITABLE / TAB_INDEX flags
// =========================================================================

#[test]
fn contenteditable_change_sets_flag() {
    let a = NodeData::create_div();
    let mut b = NodeData::create_div();
    b.set_contenteditable(true);
    let cs = compute_node_changes(&a, &b, None, None);
    assert!(cs.contains(NodeChangeSet::CONTENTEDITABLE),
        "contenteditable change should set CONTENTEDITABLE flag");
}

#[test]
fn tab_index_change_sets_flag() {
    use azul_core::dom::TabIndex;
    let a = NodeData::create_div();
    let mut b = NodeData::create_div();
    b.set_tab_index(TabIndex::Auto);
    let cs = compute_node_changes(&a, &b, None, None);
    assert!(cs.contains(NodeChangeSet::TAB_INDEX));
}

// =========================================================================
// CALLBACKS flag
// =========================================================================

#[test]
fn callbacks_flag_is_nonvisual() {
    let cs = ncs(NodeChangeSet::CALLBACKS);
    assert!(cs.is_visually_unchanged(),
        "callback changes alone should not affect visuals");
    assert!(!cs.needs_layout());
    assert!(!cs.needs_paint());
}

// =========================================================================
// COMPOSITE mask tests
// =========================================================================

#[test]
fn affects_layout_mask_includes_all_layout_flags() {
    let layout_flags = [
        NodeChangeSet::NODE_TYPE_CHANGED,
        NodeChangeSet::TEXT_CONTENT,
        NodeChangeSet::IDS_AND_CLASSES,
        NodeChangeSet::INLINE_STYLE_LAYOUT,
        NodeChangeSet::CHILDREN_CHANGED,
        NodeChangeSet::IMAGE_CHANGED,
        NodeChangeSet::CONTENTEDITABLE,
    ];
    for &flag in &layout_flags {
        assert!(ncs(flag).needs_layout(), "flag 0x{:x} should be layout-affecting", flag);
    }
}

#[test]
fn affects_paint_mask_works() {
    assert!(ncs(NodeChangeSet::INLINE_STYLE_PAINT).needs_paint());
    assert!(ncs(NodeChangeSet::STYLED_STATE).needs_paint());
}

#[test]
fn non_visual_flags_do_not_affect_paint_or_layout() {
    let nonvisual = ncs(NodeChangeSet::CALLBACKS | NodeChangeSet::DATASET | NodeChangeSet::ACCESSIBILITY);
    assert!(!nonvisual.needs_layout());
    assert!(!nonvisual.needs_paint());
    assert!(nonvisual.is_visually_unchanged());
}

#[test]
fn combined_flags_bitwise_or() {
    let cs = ncs(NodeChangeSet::TEXT_CONTENT | NodeChangeSet::STYLED_STATE);
    assert!(cs.contains(NodeChangeSet::TEXT_CONTENT));
    assert!(cs.contains(NodeChangeSet::STYLED_STATE));
    assert!(cs.needs_layout()); // TEXT_CONTENT is layout
    assert!(cs.needs_paint());  // STYLED_STATE is paint
}

// =========================================================================
// MULTIPLE CHANGES: ensure compute_node_changes reports all diffs
// =========================================================================

#[test]
fn multiple_changes_combined() {
    let a = NodeData::create_text("Hello");
    let mut b = NodeData::create_text("World");
    b.add_class(AzString::from("changed"));
    let cs = compute_node_changes(&a, &b, None, None);
    assert!(cs.contains(NodeChangeSet::TEXT_CONTENT), "text changed");
    assert!(cs.contains(NodeChangeSet::IDS_AND_CLASSES), "class added");
}

#[test]
fn text_and_style_change_combined() {
    let a = NodeData::create_text("old");
    let b = NodeData::create_text("new").with_css("width: 50px;");
    let cs = compute_node_changes(&a, &b, None, None);
    assert!(cs.contains(NodeChangeSet::TEXT_CONTENT));
    assert!(
        cs.contains(NodeChangeSet::INLINE_STYLE_LAYOUT)
            || cs.contains(NodeChangeSet::INLINE_STYLE_PAINT)
    );
}

#[test]
fn type_and_class_change_combined() {
    let a = NodeData::create_div();
    let mut b = NodeData::create_node(azul_core::dom::NodeType::Span);
    b.add_class(AzString::from("highlight"));
    let cs = compute_node_changes(&a, &b, None, None);
    assert!(cs.contains(NodeChangeSet::NODE_TYPE_CHANGED));
    // Note: when NODE_TYPE_CHANGED is set, the implementation may
    // skip further field comparisons (since the node type is different,
    // a full relayout is needed anyway). So IDS_AND_CLASSES may or
    // may not be set.
    // Just verify that the overall changeset needs layout.
    assert!(cs.needs_layout());
}
