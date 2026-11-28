use super::*;
use crate::desktop::shell2::common::ios_display::{ios_display, Turn};

fn rect(x: f32, y: f32, width: f32, height: f32) -> LogicalRect {
    LogicalRect::new(LogicalPosition { x, y }, LogicalSize { width, height })
}

#[test]
fn an_iphone_16_pro_upright_reports_one_approximate_pill_below_the_top_edge_and_two_bars() {
    let display = ios_display(402.0, 874.0, (62.0, 0.0, 34.0, 0.0), false, false, Turn::Portrait);
    let features = features_of(&display, true, true);
    let cutouts = features.cutouts.as_ref();
    assert_eq!(cutouts.len(), 1);
    assert_eq!(cutouts[0].rect, rect(138.5, 14.0, 125.0, 37.0));
    assert_eq!(cutouts[0].shape, CutoutShape::Pill);
    assert!(cutouts[0].approximate, "a cutout from a table is never reported as measured");
    assert_eq!(features.corner_radii, CornerRadii { top_left: 62.0, top_right: 62.0, bottom_right: 62.0, bottom_left: 62.0 });
    assert_eq!(features.bars.status, OptionSystemBar::Some(SystemBar { rect: rect(0.0, 0.0, 402.0, 54.0), visible: true }));
    assert_eq!(features.bars.navigation, OptionSystemBar::Some(SystemBar { rect: rect(0.0, 840.0, 402.0, 34.0), visible: true }));
    assert_eq!(features.fingerprint, OptionFingerprintSensor::None, "Face ID has no sensor to put a symbol on");
}

#[test]
fn a_hidden_bar_keeps_its_rect_and_says_it_is_not_visible() {
    let display = ios_display(402.0, 874.0, (62.0, 0.0, 34.0, 0.0), false, false, Turn::Portrait);
    let features = features_of(&display, false, false);
    assert_eq!(features.bars.status, OptionSystemBar::Some(SystemBar { rect: rect(0.0, 0.0, 402.0, 54.0), visible: false }));
    assert_eq!(features.bars.navigation, OptionSystemBar::Some(SystemBar { rect: rect(0.0, 840.0, 402.0, 34.0), visible: false }));
}

#[test]
fn the_notch_is_a_notch_and_a_home_button_phone_has_no_cutout_and_a_front_sensor() {
    let notch = features_of(&ios_display(390.0, 844.0, (47.0, 0.0, 34.0, 0.0), false, false, Turn::Portrait), true, true);
    assert_eq!(notch.cutouts.as_ref()[0].shape, CutoutShape::Notch);
    let se = features_of(&ios_display(375.0, 667.0, (20.0, 0.0, 0.0, 0.0), false, true, Turn::Portrait), true, true);
    assert!(se.cutouts.as_ref().is_empty());
    assert_eq!(se.bars.navigation, OptionSystemBar::None, "no home indicator without a bottom inset");
    assert_eq!(
        se.fingerprint,
        OptionFingerprintSensor::Some(FingerprintSensor { kind: FingerprintKind::Front, spot: OptionFingerprintSpot::None })
    );
}

#[test]
fn on_its_side_the_pill_is_at_the_left_edge_and_an_iphone_has_no_status_bar_there() {
    let display = ios_display(874.0, 402.0, (0.0, 62.0, 21.0, 62.0), false, false, Turn::CutoutLeft);
    let features = features_of(&display, true, true);
    assert_eq!(features.cutouts.as_ref()[0].rect, rect(14.0, 138.5, 37.0, 125.0));
    assert_eq!(features.bars.status, OptionSystemBar::None);
}

#[test]
fn a_display_with_nothing_reports_nothing() {
    let features = features_of(&ios_display(500.0, 700.0, (0.0, 0.0, 0.0, 0.0), false, false, Turn::Portrait), true, true);
    assert_eq!(features, DisplayFeatures::none());
}

#[test]
fn an_unknown_touch_id_device_has_a_sensor_of_an_unknown_kind() {
    let features = features_of(&ios_display(820.0, 1180.0, (24.0, 0.0, 20.0, 0.0), true, true, Turn::Portrait), true, true);
    assert_eq!(
        features.fingerprint,
        OptionFingerprintSensor::Some(FingerprintSensor { kind: FingerprintKind::Unknown, spot: OptionFingerprintSpot::None })
    );
}
