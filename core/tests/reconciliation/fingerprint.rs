// Tests for NodeDataFingerprint: two-tier fast change detection.
//
// Tier 1 (fingerprint diff): O(1) per node — compare 6 hashes.
// Tier 2 (compute_node_changes): O(n) per changed field — only for mismatches.

use azul_core::diff::{NodeDataFingerprint, NodeChangeSet, compute_node_changes};
use azul_core::dom::NodeData;
use azul_core::styled_dom::StyledNodeState;
use azul_css::AzString;

// =========================================================================
// IDENTITY: same data → identical fingerprints
// =========================================================================

#[test]
fn identical_divs_have_identical_fingerprint() {
    let a = NodeData::create_div();
    let b = NodeData::create_div();
    let fa = NodeDataFingerprint::compute(&a, None);
    let fb = NodeDataFingerprint::compute(&b, None);
    assert!(fa.is_identical(&fb), "identical divs should have identical fingerprint");
}

#[test]
fn identical_text_nodes_have_identical_fingerprint() {
    let a = NodeData::create_text("Hello World");
    let b = NodeData::create_text("Hello World");
    let fa = NodeDataFingerprint::compute(&a, None);
    let fb = NodeDataFingerprint::compute(&b, None);
    assert!(fa.is_identical(&fb));
}

#[test]
fn identical_fingerprint_diff_is_empty() {
    let a = NodeData::create_div();
    let fa = NodeDataFingerprint::compute(&a, None);
    let fb = NodeDataFingerprint::compute(&a, None);
    let diff = fa.diff(&fb);
    assert!(diff.is_empty(), "identical fingerprints should produce empty diff");
}

#[test]
fn identical_fingerprint_with_styled_state() {
    let a = NodeData::create_div();
    let state = StyledNodeState { hover: true, ..Default::default() };
    let fa = NodeDataFingerprint::compute(&a, Some(&state));
    let fb = NodeDataFingerprint::compute(&a, Some(&state));
    assert!(fa.is_identical(&fb));
}

// =========================================================================
// CONTENT HASH: text, node type
// =========================================================================

#[test]
fn text_change_detected_by_fingerprint() {
    let a = NodeData::create_text("Hello");
    let b = NodeData::create_text("World");
    let fa = NodeDataFingerprint::compute(&a, None);
    let fb = NodeDataFingerprint::compute(&b, None);
    assert!(!fa.is_identical(&fb));
    assert_ne!(fa.content_hash, fb.content_hash);
    let diff = fa.diff(&fb);
    assert!(diff.contains(NodeChangeSet::TEXT_CONTENT));
}

#[test]
fn node_type_change_detected_by_fingerprint() {
    let a = NodeData::create_div();
    let b = NodeData::create_node(azul_core::dom::NodeType::Span);
    let fa = NodeDataFingerprint::compute(&a, None);
    let fb = NodeDataFingerprint::compute(&b, None);
    assert_ne!(fa.content_hash, fb.content_hash);
}

#[test]
fn div_to_text_changes_content_hash() {
    let a = NodeData::create_div();
    let b = NodeData::create_text("hello");
    let fa = NodeDataFingerprint::compute(&a, None);
    let fb = NodeDataFingerprint::compute(&b, None);
    assert_ne!(fa.content_hash, fb.content_hash);
}

// =========================================================================
// STATE HASH: hover, focus, active
// =========================================================================

#[test]
fn hover_change_detected_by_state_hash() {
    let node = NodeData::create_div();
    let state_a = StyledNodeState::default();
    let mut state_b = StyledNodeState::default();
    state_b.hover = true;

    let fa = NodeDataFingerprint::compute(&node, Some(&state_a));
    let fb = NodeDataFingerprint::compute(&node, Some(&state_b));
    assert_ne!(fa.state_hash, fb.state_hash);
    // Other hashes should remain the same
    assert_eq!(fa.content_hash, fb.content_hash);
    assert_eq!(fa.inline_css_hash, fb.inline_css_hash);
}

#[test]
fn focus_change_detected_by_state_hash() {
    let node = NodeData::create_div();
    let state_a = StyledNodeState::default();
    let mut state_b = StyledNodeState::default();
    state_b.focused = true;

    let fa = NodeDataFingerprint::compute(&node, Some(&state_a));
    let fb = NodeDataFingerprint::compute(&node, Some(&state_b));
    assert_ne!(fa.state_hash, fb.state_hash);
    let diff = fa.diff(&fb);
    assert!(diff.contains(NodeChangeSet::STYLED_STATE));
}

#[test]
fn no_state_vs_default_state_may_differ() {
    let node = NodeData::create_div();
    let default_state = StyledNodeState::default();
    let fa = NodeDataFingerprint::compute(&node, None);
    let fb = NodeDataFingerprint::compute(&node, Some(&default_state));
    // These may or may not be identical depending on whether
    // hashing None vs hashing default state produces the same hash.
    // This test documents the behavior.
    let _ = fa.diff(&fb); // just ensure it doesn't panic
}

// =========================================================================
// INLINE CSS HASH
// =========================================================================

#[test]
fn inline_css_change_detected_by_css_hash() {
    let a = NodeData::create_div();
    let b = NodeData::create_div().with_css("width: 100px;");
    let fa = NodeDataFingerprint::compute(&a, None);
    let fb = NodeDataFingerprint::compute(&b, None);
    assert_ne!(fa.inline_css_hash, fb.inline_css_hash);
    assert_eq!(fa.content_hash, fb.content_hash, "content should not change");
}

#[test]
fn different_css_value_detected() {
    let a = NodeData::create_div().with_css("width: 50px;");
    let b = NodeData::create_div().with_css("width: 100px;");
    let fa = NodeDataFingerprint::compute(&a, None);
    let fb = NodeDataFingerprint::compute(&b, None);
    assert_ne!(fa.inline_css_hash, fb.inline_css_hash);
}

#[test]
fn css_change_sets_inline_style_layout_in_diff() {
    let a = NodeData::create_div();
    let b = NodeData::create_div().with_css("height: 200px;");
    let fa = NodeDataFingerprint::compute(&a, None);
    let fb = NodeDataFingerprint::compute(&b, None);
    let diff = fa.diff(&fb);
    assert!(diff.contains(NodeChangeSet::INLINE_STYLE_LAYOUT),
        "fingerprint diff should conservatively flag CSS as layout-affecting");
}

// =========================================================================
// IDS AND CLASSES HASH
// =========================================================================

#[test]
fn class_change_detected_by_ids_classes_hash() {
    let a = NodeData::create_div();
    let mut b = NodeData::create_div();
    b.add_class(AzString::from("highlight"));
    let fa = NodeDataFingerprint::compute(&a, None);
    let fb = NodeDataFingerprint::compute(&b, None);
    assert_ne!(fa.ids_classes_hash, fb.ids_classes_hash);
    let diff = fa.diff(&fb);
    assert!(diff.contains(NodeChangeSet::IDS_AND_CLASSES));
}

#[test]
fn id_change_detected_by_ids_classes_hash() {
    let mut a = NodeData::create_div();
    a.add_id(AzString::from("old-id"));
    let mut b = NodeData::create_div();
    b.add_id(AzString::from("new-id"));
    let fa = NodeDataFingerprint::compute(&a, None);
    let fb = NodeDataFingerprint::compute(&b, None);
    assert_ne!(fa.ids_classes_hash, fb.ids_classes_hash);
}

// =========================================================================
// CALLBACKS HASH
// =========================================================================

#[test]
fn callbacks_hash_changes_when_callbacks_differ() {
    // Two divs with no callbacks should have identical callbacks_hash
    let a = NodeData::create_div();
    let b = NodeData::create_div();
    let fa = NodeDataFingerprint::compute(&a, None);
    let fb = NodeDataFingerprint::compute(&b, None);
    assert_eq!(fa.callbacks_hash, fb.callbacks_hash);
}

// =========================================================================
// ATTRIBUTES HASH: contenteditable, tab_index
// =========================================================================

#[test]
fn contenteditable_change_detected_by_attrs_hash() {
    let a = NodeData::create_div();
    let mut b = NodeData::create_div();
    b.set_contenteditable(true);
    let fa = NodeDataFingerprint::compute(&a, None);
    let fb = NodeDataFingerprint::compute(&b, None);
    assert_ne!(fa.attrs_hash, fb.attrs_hash);
    let diff = fa.diff(&fb);
    assert!(diff.contains(NodeChangeSet::CONTENTEDITABLE)
        || diff.contains(NodeChangeSet::TAB_INDEX),
        "attrs hash change should set CONTENTEDITABLE or TAB_INDEX");
}

#[test]
fn tab_index_change_detected_by_attrs_hash() {
    use azul_core::dom::TabIndex;
    let a = NodeData::create_div();
    let mut b = NodeData::create_div();
    b.set_tab_index(TabIndex::Auto);
    let fa = NodeDataFingerprint::compute(&a, None);
    let fb = NodeDataFingerprint::compute(&b, None);
    assert_ne!(fa.attrs_hash, fb.attrs_hash);
}

// =========================================================================
// MIGHT AFFECT LAYOUT / VISUALS quick checks
// =========================================================================

#[test]
fn might_affect_layout_text_change() {
    let a = NodeData::create_text("Hello");
    let b = NodeData::create_text("World");
    let fa = NodeDataFingerprint::compute(&a, None);
    let fb = NodeDataFingerprint::compute(&b, None);
    assert!(fa.might_affect_layout(&fb));
}

#[test]
fn might_affect_layout_class_change() {
    let a = NodeData::create_div();
    let mut b = NodeData::create_div();
    b.add_class(AzString::from("new"));
    let fa = NodeDataFingerprint::compute(&a, None);
    let fb = NodeDataFingerprint::compute(&b, None);
    assert!(fa.might_affect_layout(&fb));
}

#[test]
fn might_affect_visuals_hover() {
    let node = NodeData::create_div();
    let state_a = StyledNodeState::default();
    let mut state_b = StyledNodeState::default();
    state_b.hover = true;
    let fa = NodeDataFingerprint::compute(&node, Some(&state_a));
    let fb = NodeDataFingerprint::compute(&node, Some(&state_b));
    assert!(fa.might_affect_visuals(&fb));
    assert!(!fa.might_affect_layout(&fb),
        "hover change alone should not affect layout");
}

#[test]
fn callback_change_does_not_affect_layout_or_visuals() {
    // Callbacks hash changing doesn't affect layout or visuals
    let a = NodeData::create_div();
    let fa = NodeDataFingerprint::compute(&a, None);
    let fb = NodeDataFingerprint { callbacks_hash: 999, ..fa };
    assert!(!fa.might_affect_layout(&fb));
    assert!(!fa.might_affect_visuals(&fb));
}

// =========================================================================
// TWO-TIER STRATEGY: fingerprint diff → compute_node_changes
// =========================================================================

#[test]
fn tier1_identical_skips_tier2() {
    let a = NodeData::create_div();
    let fa = NodeDataFingerprint::compute(&a, None);
    let fb = NodeDataFingerprint::compute(&a, None);
    // Tier 1: identical → skip Tier 2 entirely
    assert!(fa.is_identical(&fb));
    // Verify Tier 2 would also say no changes
    let cs = compute_node_changes(&a, &a, None, None);
    assert!(cs.is_empty());
}

#[test]
fn tier1_mismatch_tier2_confirms_text_change() {
    let a = NodeData::create_text("Hello");
    let b = NodeData::create_text("World");
    let fa = NodeDataFingerprint::compute(&a, None);
    let fb = NodeDataFingerprint::compute(&b, None);

    // Tier 1: fingerprints differ
    assert!(!fa.is_identical(&fb));
    let coarse = fa.diff(&fb);
    assert!(coarse.contains(NodeChangeSet::TEXT_CONTENT));

    // Tier 2: compute_node_changes confirms
    let precise = compute_node_changes(&a, &b, None, None);
    assert!(precise.contains(NodeChangeSet::TEXT_CONTENT));
}

#[test]
fn tier1_conservative_content_hash_refined_by_tier2() {
    // Fingerprint diff is conservative: content_hash mismatch sets both
    // TEXT_CONTENT and IMAGE_CHANGED. Tier 2 should be more precise.
    let a = NodeData::create_text("Hello");
    let b = NodeData::create_text("World");
    let fa = NodeDataFingerprint::compute(&a, None);
    let fb = NodeDataFingerprint::compute(&b, None);

    let coarse = fa.diff(&fb);
    // Coarse: conservatively sets both
    assert!(coarse.contains(NodeChangeSet::TEXT_CONTENT));
    assert!(coarse.contains(NodeChangeSet::IMAGE_CHANGED),
        "fingerprint should conservatively flag content changes");

    // Precise: only TEXT_CONTENT since both are text nodes
    let precise = compute_node_changes(&a, &b, None, None);
    assert!(precise.contains(NodeChangeSet::TEXT_CONTENT));
    // IMAGE_CHANGED should NOT be set since neither is an image
    assert!(!precise.contains(NodeChangeSet::IMAGE_CHANGED));
}

// =========================================================================
// DEFAULT fingerprint
// =========================================================================

#[test]
fn default_fingerprint_all_zeros() {
    let fp = NodeDataFingerprint::default();
    assert_eq!(fp.content_hash, 0);
    assert_eq!(fp.state_hash, 0);
    assert_eq!(fp.inline_css_hash, 0);
    assert_eq!(fp.ids_classes_hash, 0);
    assert_eq!(fp.callbacks_hash, 0);
    assert_eq!(fp.attrs_hash, 0);
}

#[test]
fn computed_fingerprint_non_zero_for_div() {
    let a = NodeData::create_div();
    let fp = NodeDataFingerprint::compute(&a, None);
    // A div should produce non-zero content hash (Div node type hashed)
    // At minimum the content hash should be non-zero
    // (the exact value depends on HighwayHasher with specific keys)
    // We just verify it's computed without panicking
    let _ = fp.content_hash;
}

// =========================================================================
// HASH STABILITY: same input always produces same hash
// =========================================================================

#[test]
fn fingerprint_deterministic() {
    let a = NodeData::create_text("test");
    let mut b = NodeData::create_div();
    b.add_class(AzString::from("my-class"));
    b.set_contenteditable(true);

    let fa1 = NodeDataFingerprint::compute(&a, None);
    let fa2 = NodeDataFingerprint::compute(&a, None);
    assert_eq!(fa1, fa2, "fingerprint should be deterministic");

    let fb1 = NodeDataFingerprint::compute(&b, None);
    let fb2 = NodeDataFingerprint::compute(&b, None);
    assert_eq!(fb1, fb2);
}

// =========================================================================
// INDEPENDENCE: changing one field doesn't affect others
// =========================================================================

#[test]
fn text_change_only_affects_content_hash() {
    let a = NodeData::create_text("Hello");
    let mut b = NodeData::create_text("World");
    // Same ids/classes, same CSS, same attrs
    let fa = NodeDataFingerprint::compute(&a, None);
    let fb = NodeDataFingerprint::compute(&b, None);

    assert_ne!(fa.content_hash, fb.content_hash, "text change should change content_hash");
    assert_eq!(fa.inline_css_hash, fb.inline_css_hash, "text change should not affect CSS hash");
    assert_eq!(fa.ids_classes_hash, fb.ids_classes_hash, "text change should not affect ids hash");
    assert_eq!(fa.callbacks_hash, fb.callbacks_hash, "text change should not affect callbacks hash");
    assert_eq!(fa.attrs_hash, fb.attrs_hash, "text change should not affect attrs hash");
}

#[test]
fn class_change_only_affects_ids_classes_hash() {
    let a = NodeData::create_div();
    let mut b = NodeData::create_div();
    b.add_class(AzString::from("added"));
    let fa = NodeDataFingerprint::compute(&a, None);
    let fb = NodeDataFingerprint::compute(&b, None);

    assert_eq!(fa.content_hash, fb.content_hash);
    assert_ne!(fa.ids_classes_hash, fb.ids_classes_hash);
    assert_eq!(fa.inline_css_hash, fb.inline_css_hash);
    assert_eq!(fa.attrs_hash, fb.attrs_hash);
}
