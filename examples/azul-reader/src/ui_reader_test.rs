//! The reader's screen parts that need no window: the three tap zones over a phone's page.

use super::*;

/// The ids of a node.
fn ids_of(dom: &Dom) -> Vec<String> {
    dom.root
        .get_ids_and_classes()
        .iter()
        .filter_map(|c| c.as_id().map(|s| s.as_str().to_string()))
        .collect()
}

#[test]
fn a_phones_page_has_a_left_a_middle_and_a_right_zone_each_named_for_a_screen_reader() {
    let zones = tap_zones(&RefAny::new(0_u8));
    let children = zones.children.as_ref();
    assert_eq!(children.len(), 3);
    let wanted = [ids::ZONE_LEFT, ids::ZONE_MIDDLE, ids::ZONE_RIGHT];
    for (zone, id) in children.iter().zip(wanted) {
        assert_eq!(ids_of(zone), vec![id.as_str().to_string()]);
        let info = zone.root.get_accessibility_info().expect("a zone is named");
        assert!(info.accessibility_name.as_ref().is_some_and(|n| !n.as_str().is_empty()));
    }
}

#[test]
fn the_zones_share_the_width_so_that_the_sides_turn_and_the_middle_shows_the_controls() {
    // 30 % either side, 40 % in the middle.
    let side = (chrome::SIDE_SHARE * 100.0).round();
    assert!((side - 30.0).abs() < f32::EPSILON);
    assert!((100.0 - 2.0 * side - 40.0).abs() < f32::EPSILON);
}
