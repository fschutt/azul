// Tests for ChangeAccumulator: merging changes from multiple paths,
// classification of layout/paint/no-visual changes, mount/unmount tracking.

use azul_core::diff::{
    ChangeAccumulator, NodeChangeSet, NodeChangeReport, TextChange,
};
use azul_core::id::NodeId;
use azul_css::props::property::{RelayoutScope, CssPropertyType};

// =========================================================================
// BASIC CONSTRUCTION
// =========================================================================

#[test]
fn new_accumulator_is_empty() {
    let acc = ChangeAccumulator::new();
    assert!(acc.is_empty());
    assert!(!acc.needs_layout());
    assert!(!acc.needs_paint_only());
    assert!(acc.is_visually_unchanged());
}

#[test]
fn default_accumulator_is_empty() {
    let acc = ChangeAccumulator::default();
    assert!(acc.is_empty());
}

// =========================================================================
// ADD DOM CHANGE — single node
// =========================================================================

#[test]
fn add_text_change_needs_layout() {
    let mut acc = ChangeAccumulator::new();
    acc.add_text_change(
        NodeId::new(0),
        "old".into(),
        "new".into(),
    );
    assert!(!acc.is_empty());
    assert!(acc.needs_layout(), "text change should need layout");
    assert!(!acc.is_visually_unchanged());
}

#[test]
fn add_text_change_records_old_and_new() {
    let mut acc = ChangeAccumulator::new();
    acc.add_text_change(NodeId::new(5), "Hello".into(), "World".into());

    let report = acc.per_node.get(&NodeId::new(5)).unwrap();
    assert!(report.change_set.contains(NodeChangeSet::TEXT_CONTENT));
    assert_eq!(
        report.text_change,
        Some(TextChange {
            old_text: "Hello".into(),
            new_text: "World".into(),
        })
    );
}

#[test]
fn add_text_change_sets_ifc_only_scope() {
    let mut acc = ChangeAccumulator::new();
    acc.add_text_change(NodeId::new(0), "a".into(), "b".into());
    assert_eq!(acc.max_scope, RelayoutScope::IfcOnly);
}

// =========================================================================
// ADD CSS CHANGE — layout vs paint
// =========================================================================

#[test]
fn add_css_layout_change_needs_layout() {
    let mut acc = ChangeAccumulator::new();
    acc.add_css_change(
        NodeId::new(0),
        CssPropertyType::Width,
        RelayoutScope::SizingOnly,
    );
    assert!(acc.needs_layout());
    assert!(!acc.needs_paint_only());
}

#[test]
fn add_css_paint_change_needs_paint_only() {
    let mut acc = ChangeAccumulator::new();
    acc.add_css_change(
        NodeId::new(0),
        CssPropertyType::TextColor,
        RelayoutScope::None,
    );
    assert!(!acc.needs_layout(), "paint-only change should not need layout");
    assert!(acc.needs_paint_only(), "paint-only change should need paint");
}

#[test]
fn add_css_change_records_property_type() {
    let mut acc = ChangeAccumulator::new();
    acc.add_css_change(
        NodeId::new(3),
        CssPropertyType::Height,
        RelayoutScope::SizingOnly,
    );
    let report = acc.per_node.get(&NodeId::new(3)).unwrap();
    assert!(report.changed_css_properties.contains(&CssPropertyType::Height));
}

#[test]
fn add_layout_css_sets_inline_style_layout_flag() {
    let mut acc = ChangeAccumulator::new();
    acc.add_css_change(
        NodeId::new(0),
        CssPropertyType::Display,
        RelayoutScope::Full,
    );
    let report = acc.per_node.get(&NodeId::new(0)).unwrap();
    assert!(report.change_set.contains(NodeChangeSet::INLINE_STYLE_LAYOUT));
}

#[test]
fn add_paint_css_sets_inline_style_paint_flag() {
    let mut acc = ChangeAccumulator::new();
    acc.add_css_change(
        NodeId::new(0),
        CssPropertyType::Opacity,
        RelayoutScope::None,
    );
    let report = acc.per_node.get(&NodeId::new(0)).unwrap();
    assert!(report.change_set.contains(NodeChangeSet::INLINE_STYLE_PAINT));
}

// =========================================================================
// ADD IMAGE CHANGE
// =========================================================================

#[test]
fn add_image_change_records_flag() {
    let mut acc = ChangeAccumulator::new();
    acc.add_image_change(NodeId::new(2), RelayoutScope::SizingOnly);
    let report = acc.per_node.get(&NodeId::new(2)).unwrap();
    assert!(report.change_set.contains(NodeChangeSet::IMAGE_CHANGED));
}

#[test]
fn add_image_change_upgrades_max_scope() {
    let mut acc = ChangeAccumulator::new();
    acc.add_css_change(NodeId::new(0), CssPropertyType::TextColor, RelayoutScope::None);
    assert_eq!(acc.max_scope, RelayoutScope::None);

    acc.add_image_change(NodeId::new(1), RelayoutScope::SizingOnly);
    assert_eq!(acc.max_scope, RelayoutScope::SizingOnly);
}

// =========================================================================
// MOUNT / UNMOUNT tracking
// =========================================================================

#[test]
fn mounted_node_makes_accumulator_non_empty() {
    let mut acc = ChangeAccumulator::new();
    acc.add_mount(NodeId::new(5));
    assert!(!acc.is_empty());
    assert!(acc.needs_layout(), "mounted nodes always need layout");
}

#[test]
fn unmounted_node_makes_accumulator_non_empty() {
    let mut acc = ChangeAccumulator::new();
    acc.add_unmount(NodeId::new(3));
    assert!(!acc.is_empty());
}

#[test]
fn unmounted_node_is_not_visually_unchanged() {
    let mut acc = ChangeAccumulator::new();
    acc.add_unmount(NodeId::new(0));
    assert!(!acc.is_visually_unchanged());
}

#[test]
fn multiple_mounts_tracked() {
    let mut acc = ChangeAccumulator::new();
    acc.add_mount(NodeId::new(0));
    acc.add_mount(NodeId::new(1));
    acc.add_mount(NodeId::new(2));
    assert_eq!(acc.mounted_nodes.len(), 3);
}

// =========================================================================
// MAX SCOPE TRACKING
// =========================================================================

#[test]
fn max_scope_upgrades_from_none_to_ifc() {
    let mut acc = ChangeAccumulator::new();
    acc.add_text_change(NodeId::new(0), "a".into(), "b".into());
    assert_eq!(acc.max_scope, RelayoutScope::IfcOnly);
}

#[test]
fn max_scope_upgrades_from_ifc_to_sizing() {
    let mut acc = ChangeAccumulator::new();
    acc.add_text_change(NodeId::new(0), "a".into(), "b".into());
    acc.add_css_change(NodeId::new(1), CssPropertyType::Width, RelayoutScope::SizingOnly);
    assert_eq!(acc.max_scope, RelayoutScope::SizingOnly);
}

#[test]
fn max_scope_upgrades_from_sizing_to_full() {
    let mut acc = ChangeAccumulator::new();
    acc.add_css_change(NodeId::new(0), CssPropertyType::Width, RelayoutScope::SizingOnly);
    acc.add_css_change(NodeId::new(1), CssPropertyType::Display, RelayoutScope::Full);
    assert_eq!(acc.max_scope, RelayoutScope::Full);
}

#[test]
fn max_scope_does_not_downgrade() {
    let mut acc = ChangeAccumulator::new();
    acc.add_css_change(NodeId::new(0), CssPropertyType::Display, RelayoutScope::Full);
    acc.add_css_change(NodeId::new(1), CssPropertyType::TextColor, RelayoutScope::None);
    assert_eq!(acc.max_scope, RelayoutScope::Full, "max scope should not downgrade");
}

// =========================================================================
// MULTIPLE CHANGES TO SAME NODE
// =========================================================================

#[test]
fn multiple_changes_same_node_merge() {
    let mut acc = ChangeAccumulator::new();
    // First change: text
    acc.add_text_change(NodeId::new(0), "old".into(), "new".into());
    // Second change: CSS
    acc.add_css_change(NodeId::new(0), CssPropertyType::Width, RelayoutScope::SizingOnly);

    let report = acc.per_node.get(&NodeId::new(0)).unwrap();
    assert!(report.change_set.contains(NodeChangeSet::TEXT_CONTENT));
    assert!(report.change_set.contains(NodeChangeSet::INLINE_STYLE_LAYOUT));
    assert_eq!(report.relayout_scope, RelayoutScope::SizingOnly,
        "node scope should be upgraded to SizingOnly");
}

#[test]
fn multiple_css_changes_same_node_accumulate_properties() {
    let mut acc = ChangeAccumulator::new();
    acc.add_css_change(NodeId::new(0), CssPropertyType::Width, RelayoutScope::SizingOnly);
    acc.add_css_change(NodeId::new(0), CssPropertyType::Height, RelayoutScope::SizingOnly);

    let report = acc.per_node.get(&NodeId::new(0)).unwrap();
    assert_eq!(report.changed_css_properties.len(), 2);
    assert!(report.changed_css_properties.contains(&CssPropertyType::Width));
    assert!(report.changed_css_properties.contains(&CssPropertyType::Height));
}

#[test]
fn per_node_scope_upgrades_independently() {
    let mut acc = ChangeAccumulator::new();
    acc.add_css_change(NodeId::new(0), CssPropertyType::TextColor, RelayoutScope::None);
    acc.add_css_change(NodeId::new(0), CssPropertyType::Display, RelayoutScope::Full);

    let report = acc.per_node.get(&NodeId::new(0)).unwrap();
    assert_eq!(report.relayout_scope, RelayoutScope::Full);
}

// =========================================================================
// MIXED PATHS — combining DOM, CSS, and image changes
// =========================================================================

#[test]
fn mixed_dom_css_image_changes() {
    let mut acc = ChangeAccumulator::new();
    // DOM path: text change on node 0
    acc.add_text_change(NodeId::new(0), "a".into(), "b".into());
    // CSS path: width on node 1
    acc.add_css_change(NodeId::new(1), CssPropertyType::Width, RelayoutScope::SizingOnly);
    // Image path: image on node 2
    acc.add_image_change(NodeId::new(2), RelayoutScope::SizingOnly);
    // Paint path: color on node 3
    acc.add_css_change(NodeId::new(3), CssPropertyType::TextColor, RelayoutScope::None);

    assert_eq!(acc.per_node.len(), 4);
    assert_eq!(acc.max_scope, RelayoutScope::SizingOnly);
    assert!(acc.needs_layout());
}

#[test]
fn paint_only_changes_detected_correctly() {
    let mut acc = ChangeAccumulator::new();
    acc.add_css_change(NodeId::new(0), CssPropertyType::TextColor, RelayoutScope::None);
    acc.add_css_change(NodeId::new(1), CssPropertyType::Opacity, RelayoutScope::None);

    assert!(!acc.needs_layout());
    assert!(acc.needs_paint_only());
    assert!(!acc.is_visually_unchanged());
}

// =========================================================================
// NODE CHANGE REPORT
// =========================================================================

#[test]
fn empty_report_is_visually_unchanged() {
    let report = NodeChangeReport::default();
    assert!(report.is_visually_unchanged());
    assert!(!report.needs_layout());
    assert!(!report.needs_paint());
}

#[test]
fn report_with_text_change_needs_layout() {
    let mut report = NodeChangeReport::default();
    report.change_set.insert(NodeChangeSet::TEXT_CONTENT);
    report.relayout_scope = RelayoutScope::IfcOnly;
    assert!(report.needs_layout());
}

#[test]
fn report_with_paint_flag_needs_paint() {
    let mut report = NodeChangeReport::default();
    report.change_set.insert(NodeChangeSet::INLINE_STYLE_PAINT);
    assert!(report.needs_paint());
    assert!(!report.needs_layout());
}

// =========================================================================
// ADD DOM CHANGE (raw API)
// =========================================================================

#[test]
fn add_dom_change_with_all_params() {
    let mut acc = ChangeAccumulator::new();
    acc.add_dom_change(
        NodeId::new(7),
        NodeChangeSet { bits: NodeChangeSet::TEXT_CONTENT | NodeChangeSet::IDS_AND_CLASSES },
        RelayoutScope::Full,
        Some(TextChange {
            old_text: "old".into(),
            new_text: "new".into(),
        }),
        vec![CssPropertyType::Width, CssPropertyType::Display],
    );

    let report = acc.per_node.get(&NodeId::new(7)).unwrap();
    assert!(report.change_set.contains(NodeChangeSet::TEXT_CONTENT));
    assert!(report.change_set.contains(NodeChangeSet::IDS_AND_CLASSES));
    assert_eq!(report.relayout_scope, RelayoutScope::Full);
    assert!(report.text_change.is_some());
    assert_eq!(report.changed_css_properties.len(), 2);
    assert_eq!(acc.max_scope, RelayoutScope::Full);
}

#[test]
fn add_dom_change_with_empty_changeset_still_tracked() {
    let mut acc = ChangeAccumulator::new();
    acc.add_dom_change(
        NodeId::new(0),
        NodeChangeSet::empty(),
        RelayoutScope::None,
        None,
        Vec::new(),
    );
    // Empty changes are still recorded (the entry exists)
    assert!(acc.per_node.contains_key(&NodeId::new(0)));
    // But overall the accumulator is not visually changed
    assert!(acc.is_visually_unchanged());
}
