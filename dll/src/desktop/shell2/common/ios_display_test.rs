use super::*;

const PRO_16: (f32, f32) = (402.0, 874.0);
const PRO_16_INSETS: (f32, f32, f32, f32) = (62.0, 0.0, 34.0, 0.0); // top, right, bottom, left
const PRO_16_ON_ITS_SIDE_INSETS: (f32, f32, f32, f32) = (0.0, 62.0, 21.0, 62.0);

fn rect(x: f32, y: f32, width: f32, height: f32) -> IosRect {
    IosRect { x, y, width, height }
}

#[test]
fn every_class_is_found_by_its_portrait_and_by_its_landscape_size() {
    for c in &IOS_DISPLAY_CLASSES {
        assert_eq!(display_class(c.short, c.long), Some(c), "{}x{} upright", c.short, c.long);
        assert_eq!(display_class(c.long, c.short), Some(c), "{}x{} on its side", c.long, c.short);
    }
}

#[test]
fn no_two_classes_share_a_size_and_every_cutout_fits_in_its_screen() {
    for (i, a) in IOS_DISPLAY_CLASSES.iter().enumerate() {
        for b in &IOS_DISPLAY_CLASSES[i + 1..] {
            assert!(a.short != b.short || a.long != b.long, "{}x{} twice", a.short, a.long);
        }
        let (w, h) = a.cutout_size;
        assert!(w <= a.short && a.cutout_top + h <= a.long, "{}x{}: the cutout is bigger than the screen", a.short, a.long);
        assert_eq!(a.cutout == IosCutout::None, w == 0.0 && h == 0.0, "{}x{}: a cutout has a size, none has not", a.short, a.long);
        assert!(!a.home_button || a.cutout == IosCutout::None, "a home button phone has no cutout");
    }
}

#[test]
fn an_unknown_size_is_in_no_class() {
    assert_eq!(display_class(400.0, 800.0), None);
    assert_eq!(display_class(0.0, 0.0), None);
}

#[test]
fn the_island_stands_centred_below_the_top_edge_of_an_upright_iphone_16_pro() {
    let d = ios_display(PRO_16.0, PRO_16.1, PRO_16_INSETS, false, false, Turn::Portrait);
    assert_eq!(d.cutout, Some((rect(138.5, 14.0, 125.0, 37.0), IosCutout::Island)));
    assert_eq!(d.corner_radius, 62.0);
    assert_eq!(d.status_bar, Some(rect(0.0, 0.0, 402.0, 54.0)));
    assert_eq!(d.home_indicator, Some(rect(0.0, 840.0, 402.0, 34.0)));
    assert_eq!(d.fingerprint, IosFingerprint::None, "Face ID has no sensor to place");
}

#[test]
fn on_its_side_the_island_is_at_the_edge_the_top_of_the_phone_is_at() {
    let left = ios_display(PRO_16.1, PRO_16.0, PRO_16_ON_ITS_SIDE_INSETS, false, false, Turn::CutoutLeft);
    assert_eq!(left.cutout, Some((rect(14.0, 138.5, 37.0, 125.0), IosCutout::Island)));
    let right = ios_display(PRO_16.1, PRO_16.0, PRO_16_ON_ITS_SIDE_INSETS, false, false, Turn::CutoutRight);
    assert_eq!(right.cutout, Some((rect(823.0, 138.5, 37.0, 125.0), IosCutout::Island)));
    // an iPhone on its side has no status bar and a thinner home indicator
    assert_eq!(left.status_bar, None);
    assert_eq!(left.home_indicator, Some(rect(0.0, 381.0, 874.0, 21.0)));
}

#[test]
fn the_notch_touches_the_top_edge() {
    let d = ios_display(390.0, 844.0, (47.0, 0.0, 34.0, 0.0), false, false, Turn::Portrait);
    assert_eq!(d.cutout, Some((rect(390.0 / 2.0 - 104.5, 0.0, 209.0, 34.0), IosCutout::Notch)));
    assert_eq!(d.corner_radius, 47.0);
    assert_eq!(d.status_bar, Some(rect(0.0, 0.0, 390.0, 47.0)));
}

#[test]
fn a_home_button_phone_has_no_cutout_a_20_point_status_bar_and_a_touch_id_button() {
    let d = ios_display(375.0, 667.0, (20.0, 0.0, 0.0, 0.0), false, true, Turn::Portrait);
    assert_eq!(d.cutout, None);
    assert_eq!(d.status_bar, Some(rect(0.0, 0.0, 375.0, 20.0)));
    assert_eq!(d.home_indicator, None);
    assert_eq!(d.corner_radius, 0.0);
    assert_eq!(d.fingerprint, IosFingerprint::Front);
}

#[test]
fn a_phone_of_an_unknown_size_whose_safe_area_says_cutout_gets_the_island_not_nothing() {
    let d = ios_display(420.0, 900.0, (66.0, 0.0, 34.0, 0.0), false, false, Turn::Portrait);
    let (r, kind) = d.cutout.expect("a cutout");
    assert_eq!(kind, IosCutout::Island);
    assert_eq!(r.x, 210.0 - 63.0);
    // and without such an inset (a plain top of 20 points) there is none
    let plain = ios_display(420.0, 900.0, (20.0, 0.0, 0.0, 0.0), false, false, Turn::Portrait);
    assert_eq!(plain.cutout, None);
    assert_eq!(plain.status_bar, Some(rect(0.0, 0.0, 420.0, 20.0)), "the measured inset is the bar");
}

#[test]
fn an_ipad_has_no_cutout_no_public_corner_radius_and_its_status_bar_is_the_top_inset() {
    let d = ios_display(820.0, 1180.0, (24.0, 0.0, 20.0, 0.0), true, false, Turn::Portrait);
    assert_eq!(d.cutout, None);
    assert_eq!(d.corner_radius, 0.0);
    assert_eq!(d.status_bar, Some(rect(0.0, 0.0, 820.0, 24.0)));
    assert_eq!(d.home_indicator, Some(rect(0.0, 1160.0, 820.0, 20.0)));
    // even at the size of a phone with a cutout
    assert_eq!(ios_display(402.0, 874.0, (62.0, 0.0, 34.0, 0.0), true, false, Turn::Portrait).cutout, None);
}

#[test]
fn a_window_without_insets_has_neither_bars_nor_a_cutout_it_cannot_know_of() {
    let d = ios_display(500.0, 700.0, (0.0, 0.0, 0.0, 0.0), false, false, Turn::Portrait);
    assert_eq!(d, IosDisplay { cutout: None, corner_radius: 0.0, status_bar: None, home_indicator: None, fingerprint: IosFingerprint::None });
}

#[test]
fn the_turn_comes_from_the_size_and_the_orientation_uikit_names() {
    assert_eq!(turn_of(402.0, 874.0, 1), Turn::Portrait);
    assert_eq!(turn_of(402.0, 874.0, 4), Turn::Portrait, "upright whatever a stale orientation says");
    assert_eq!(turn_of(874.0, 402.0, 3), Turn::CutoutLeft, "landscape right: the home button is on the right, the top of the phone on the left");
    assert_eq!(turn_of(874.0, 402.0, 4), Turn::CutoutRight);
    assert_eq!(turn_of(874.0, 402.0, 0), Turn::CutoutLeft, "unknown: the left");
}

#[test]
fn the_default_window_options_ask_nothing_of_the_system_bars() {
    assert_eq!(system_ui_of(false, false, 0), IosSystemUi::default());
    assert_eq!(IosSystemUi::default().status_bar_style, STATUS_BAR_STYLE_DEFAULT);
    assert_eq!(IosSystemUi::default().deferred_edges, 0);
}

#[test]
fn a_hidden_bar_defers_the_system_gesture_of_its_edge_so_that_a_swipe_shows_it_first() {
    let status = system_ui_of(true, false, 0);
    assert!(status.status_bar_hidden && !status.home_indicator_hidden);
    assert_eq!(status.deferred_edges, EDGE_TOP);
    let navigation = system_ui_of(false, true, 0);
    assert!(navigation.home_indicator_hidden && !navigation.status_bar_hidden);
    assert_eq!(navigation.deferred_edges, EDGE_BOTTOM);
    assert_eq!(system_ui_of(true, true, 0).deferred_edges, EDGE_TOP | EDGE_BOTTOM);
}

#[test]
fn the_icons_of_the_status_bar_follow_the_option() {
    assert_eq!(system_ui_of(false, false, 1).status_bar_style, STATUS_BAR_STYLE_LIGHT);
    assert_eq!(system_ui_of(false, false, 2).status_bar_style, STATUS_BAR_STYLE_DARK);
    assert_eq!(system_ui_of(false, false, 0).status_bar_style, STATUS_BAR_STYLE_DEFAULT);
    assert_eq!(system_ui_of(false, false, 9).status_bar_style, STATUS_BAR_STYLE_DEFAULT, "an unknown choice is the system's");
    assert_eq!((EDGE_TOP, EDGE_BOTTOM, STATUS_BAR_STYLE_LIGHT, STATUS_BAR_STYLE_DARK), (1, 4, 1, 3), "the UIKit values");
}

#[test]
fn an_ipad_with_touch_id_has_a_sensor_of_an_unknown_kind_and_place() {
    let d = ios_display(820.0, 1180.0, (24.0, 0.0, 20.0, 0.0), true, true, Turn::Portrait);
    assert_eq!(d.fingerprint, IosFingerprint::Unknown);
    // an iPhone that has Touch ID and is in no home button class is as unknown as the iPad
    assert_eq!(ios_display(400.0, 800.0, (20.0, 0.0, 0.0, 0.0), false, true, Turn::Portrait).fingerprint, IosFingerprint::Unknown);
}

#[test]
fn a_face_id_phone_upright_has_the_status_band_on_top_and_the_home_indicator_as_the_mandatory_bottom() {
    let display = ios_display(402.0, 874.0, PRO_16_INSETS, false, false, Turn::Portrait);
    let zones = gesture_zones(&display);
    assert_eq!(zones.top_normal, display.status_bar.expect("an upright iPhone has a status bar").height);
    assert_eq!(zones.bottom_mandatory, display.home_indicator.expect("a Face ID phone has a home indicator").height);
    assert_eq!(zones.bottom_normal, 0.0, "the indicator's band is the bottom zone: no second strip");
}

#[test]
fn a_phone_on_its_side_has_no_status_bar_and_the_pull_down_strip_is_the_edge_strip() {
    let display = ios_display(874.0, 402.0, PRO_16_ON_ITS_SIDE_INSETS, false, false, Turn::CutoutLeft);
    assert!(display.status_bar.is_none());
    let zones = gesture_zones(&display);
    assert_eq!(zones.top_normal, SYSTEM_EDGE_STRIP_POINTS);
    assert!(zones.bottom_mandatory > 0.0, "the indicator is still there on its side (21 points)");
}

#[test]
fn a_home_button_phone_has_the_bottom_strip_and_no_mandatory_band() {
    // an iPhone SE (3rd generation): 375 x 667, no home indicator, Touch ID
    let display = ios_display(375.0, 667.0, (20.0, 0.0, 0.0, 0.0), false, true, Turn::Portrait);
    assert!(display.home_indicator.is_none());
    let zones = gesture_zones(&display);
    assert_eq!(zones.bottom_mandatory, 0.0);
    assert_eq!(zones.bottom_normal, SYSTEM_EDGE_STRIP_POINTS);
}

#[test]
fn no_zone_is_negative_or_wider_than_a_quarter_of_the_screen_and_the_sides_are_free() {
    for class in &IOS_DISPLAY_CLASSES {
        for turn in [Turn::Portrait, Turn::CutoutLeft, Turn::CutoutRight] {
            let (w, h) = if turn == Turn::Portrait { (class.short, class.long) } else { (class.long, class.short) };
            let insets = if turn == Turn::Portrait { (class.cutout_top.max(47.0), 0.0, 34.0, 0.0) } else { (0.0, 47.0, 21.0, 47.0) };
            let zones = gesture_zones(&ios_display(w, h, insets, false, class.home_button, turn));
            for z in [zones.top_normal, zones.bottom_normal, zones.bottom_mandatory] {
                assert!(z >= 0.0 && z <= h / 4.0, "{}x{} {:?}: {z}", class.short, class.long, turn);
            }
        }
    }
}
