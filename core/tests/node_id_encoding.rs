//! NodeId Encoding Tests
//!
//! Tests for the FFI-safe NodeId encoding where 0 = None.
//! These tests verify the invariant that:
//! - `from_usize(0)` returns `None`
//! - `from_usize(n)` for n > 0 returns `Some(NodeId(n-1))`
//! - `into_raw(None)` returns `0`
//! - `into_raw(Some(NodeId(n)))` returns `n + 1`
//!
//! This encoding prevents underflow issues when manipulating node hierarchies.

use azul_core::id::NodeId;

// ==================== from_usize Tests ====================

#[test]
fn test_from_usize_zero_is_none() {
    let result = NodeId::from_usize(0);
    assert!(result.is_none(), "from_usize(0) should return None");
}

#[test]
fn test_from_usize_one_is_node_zero() {
    let result = NodeId::from_usize(1);
    assert!(result.is_some(), "from_usize(1) should return Some");
    assert_eq!(
        result.unwrap().index(),
        0,
        "from_usize(1) should return NodeId(0)"
    );
}

#[test]
fn test_from_usize_two_is_node_one() {
    let result = NodeId::from_usize(2);
    assert!(result.is_some(), "from_usize(2) should return Some");
    assert_eq!(
        result.unwrap().index(),
        1,
        "from_usize(2) should return NodeId(1)"
    );
}

#[test]
fn test_from_usize_large_value() {
    let result = NodeId::from_usize(1000);
    assert!(result.is_some());
    assert_eq!(result.unwrap().index(), 999);
}

#[test]
fn test_from_usize_max_minus_one() {
    // usize::MAX - 1 should map to NodeId(usize::MAX - 2)
    let result = NodeId::from_usize(usize::MAX - 1);
    assert!(result.is_some());
    assert_eq!(result.unwrap().index(), usize::MAX - 2);
}

// ==================== into_raw Tests ====================

#[test]
fn test_into_raw_none_is_zero() {
    let result = NodeId::into_raw(&None);
    assert_eq!(result, 0, "into_raw(None) should return 0");
}

#[test]
fn test_into_raw_node_zero_is_one() {
    let node = Some(NodeId::new(0));
    let result = NodeId::into_raw(&node);
    assert_eq!(result, 1, "into_raw(Some(NodeId(0))) should return 1");
}

#[test]
fn test_into_raw_node_one_is_two() {
    let node = Some(NodeId::new(1));
    let result = NodeId::into_raw(&node);
    assert_eq!(result, 2, "into_raw(Some(NodeId(1))) should return 2");
}

#[test]
fn test_into_raw_large_value() {
    let node = Some(NodeId::new(999));
    let result = NodeId::into_raw(&node);
    assert_eq!(result, 1000);
}

// ==================== Round-trip Tests ====================

#[test]
fn test_roundtrip_none() {
    let original: Option<NodeId> = None;
    let encoded = NodeId::into_raw(&original);
    let decoded = NodeId::from_usize(encoded);
    assert_eq!(decoded, original, "None should roundtrip correctly");
}

#[test]
fn test_roundtrip_zero() {
    let original = Some(NodeId::new(0));
    let encoded = NodeId::into_raw(&original);
    let decoded = NodeId::from_usize(encoded);
    assert_eq!(decoded, original, "NodeId(0) should roundtrip correctly");
}

#[test]
fn test_roundtrip_various_values() {
    for i in 0..100 {
        let original = Some(NodeId::new(i));
        let encoded = NodeId::into_raw(&original);
        let decoded = NodeId::from_usize(encoded);
        assert_eq!(
            decoded, original,
            "NodeId({}) should roundtrip correctly",
            i
        );
    }
}

// ==================== Invariant Tests ====================

#[test]
fn test_zero_encoding_invariant() {
    // The critical invariant: 0 in the encoded form ALWAYS means None
    // This prevents the underflow bug where we used usize::MAX as the sentinel

    // Encoding 0 should produce 1 (not 0)
    let node = Some(NodeId::new(0));
    let encoded = NodeId::into_raw(&node);
    assert_ne!(encoded, 0, "NodeId(0) should NOT encode to 0");
    assert_eq!(encoded, 1, "NodeId(0) should encode to 1");
}

#[test]
fn test_no_underflow_on_none_check() {
    // This test verifies that checking for "no node" uses 0, not usize::MAX
    // The old buggy code would check `value != usize::MAX` which was wrong

    let none_encoded = NodeId::into_raw(&None);
    assert_eq!(none_encoded, 0, "None should encode to 0, not usize::MAX");

    // Verify that usize::MAX is a valid encoded value (represents NodeId(usize::MAX - 1))
    // and NOT a sentinel for None
    let max_node = NodeId::from_usize(usize::MAX);
    assert!(
        max_node.is_some(),
        "usize::MAX should decode to Some, not None"
    );
}

#[test]
fn test_encoded_zero_is_always_none() {
    // Decoding 0 should always give None
    assert!(NodeId::from_usize(0).is_none());

    // There's no valid NodeId that encodes to 0
    for i in 0..1000 {
        let encoded = NodeId::into_raw(&Some(NodeId::new(i)));
        assert_ne!(encoded, 0, "No valid NodeId should encode to 0");
    }
}
