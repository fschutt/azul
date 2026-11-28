//! The reader's screen parts that need no window: the three tap zones over a phone's page.
//!
//! That the zones are named for a screen reader is not read back here (the node's accessibility
//! info has no getter in the C API): the scenario `reader_phone_taps.json` taps each zone by its
//! id, and the phones' accessibility audit lists their names.

use azul::dom::IdOrClass;

use super::*;

/// The ids of a node.
fn ids_of(dom: &Dom) -> Vec<String> {
    dom.root
        .get_ids_and_classes()
        .iter()
        .filter_map(|c| match c {
            IdOrClass::Id(id) => Some(id.as_str().to_string()),
            IdOrClass::Class(_) => None,
        })
        .collect()
}

#[test]
fn a_phones_page_has_a_left_a_middle_and_a_right_zone() {
    let zones = tap_zones(&RefAny::new(0_u8));
    let children = zones.children.as_ref();
    assert_eq!(children.len(), 3);
    let wanted = [ids::ZONE_LEFT, ids::ZONE_MIDDLE, ids::ZONE_RIGHT];
    for (zone, id) in children.iter().zip(wanted) {
        assert_eq!(ids_of(zone), vec![id.as_str().to_string()]);
    }
}

#[test]
fn the_zones_share_the_width_so_that_the_sides_turn_and_the_middle_shows_the_controls() {
    // 30 % either side, 40 % in the middle.
    let side = (chrome::SIDE_SHARE * 100.0).round();
    assert!((side - 30.0).abs() < f32::EPSILON);
    assert!((100.0 - 2.0 * side - 40.0).abs() < f32::EPSILON);
}
