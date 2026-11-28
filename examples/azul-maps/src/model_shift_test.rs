//! Where the map is centred so that a place is seen beside the panel and above the sheet.

use super::*;

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

#[test]
fn a_point_of_the_world_comes_back_from_its_pixels() {
    for (lat, lon, zoom) in [(48.5216, 9.0576, 12.0), (-33.87, 151.21, 5.0), (0.0, 0.0, 2.0), (60.0, -170.0, 8.0)] {
        let (x, y) = mercator_px(lat, lon, zoom);
        let (la, lo) = from_mercator_px(x, y, zoom);
        assert!(near(la, lat) && near(lo, lon), "{lat} {lon} {zoom}: {la} {lo}");
    }
}

#[test]
fn nothing_covered_leaves_the_centre_on_the_place() {
    let at = (48.5216, 9.0576);
    let c = centre_in_free(at, 12.0, 0.0, 0.0);
    assert!(near(c.0, at.0) && near(c.1, at.1));
    // a side that is no number covers nothing
    let n = centre_in_free(at, 12.0, f32::NAN, -5.0);
    assert!(near(n.0, at.0) && near(n.1, at.1));
}

#[test]
fn a_panel_at_the_left_moves_the_centre_west_by_half_its_width_in_pixels() {
    let at = (48.5216, 9.0576);
    let c = centre_in_free(at, 12.0, 340.0, 0.0);
    assert!(c.1 < at.1, "west of the place: {c:?}");
    assert!(near(c.0, at.0), "the same latitude");
    let (px, _) = mercator_px(at.0, at.1, 12.0);
    let (cx, _) = mercator_px(c.0, c.1, 12.0);
    assert!((px - cx - 170.0).abs() < 1e-3, "{} px", px - cx);
}

#[test]
fn a_sheet_at_the_bottom_moves_the_centre_south_by_half_its_height_in_pixels() {
    let at = (48.5216, 9.0576);
    let c = centre_in_free(at, 12.0, 0.0, 300.0);
    assert!(c.0 < at.0, "south of the place: {c:?}");
    assert!(near(c.1, at.1));
    let (_, py) = mercator_px(at.0, at.1, 12.0);
    let (_, cy) = mercator_px(c.0, c.1, 12.0);
    assert!((cy - py - 150.0).abs() < 1e-3, "{} px", cy - py);
}

#[test]
fn the_searched_places_marker_is_no_pin_and_the_pins_are_far_below_it() {
    assert_eq!(pin_of_marker(PLACE_MARKER), None);
    assert_eq!(pin_of_marker(FROM_MARKER), None);
    assert_eq!(pin_of_marker(0), Some(0));
    assert_eq!(pin_of_marker(41), Some(41));
    const _: () = assert!(PLACE_MARKER < FROM_MARKER && FROM_MARKER < TO_MARKER && TO_MARKER < HERE_MARKER);
}
