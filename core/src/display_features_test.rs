//! The lanes of a band around the cutouts, the simulated devices and their names.
//!
//! Not compiled by the author (house rule).

use azul_css::props::basic::pixel::OptionPixelValue;

use super::*;

fn size(width: f32, height: f32) -> LogicalSize {
    LogicalSize { width, height }
}

fn features_with(cutouts: &[(f32, f32, f32, f32)]) -> DisplayFeatures {
    let mut f = DisplayFeatures::none();
    f.cutouts = DisplayCutoutVec::from_vec(
        cutouts
            .iter()
            .map(|&(x, y, w, h)| cutout(x, y, w, h, CutoutShape::Circle))
            .collect(),
    );
    f
}

fn spans(lanes: &LogicalRectVec) -> Vec<(f32, f32)> {
    lanes
        .as_ref()
        .iter()
        .map(|r| (r.origin.x, r.origin.x + r.size.width))
        .collect()
}

fn inset_px(v: OptionPixelValue) -> f32 {
    v.into_option()
        .and_then(|p| p.to_pixels_absolute().into_option())
        .unwrap_or(0.0)
}

#[test]
fn a_band_without_a_cutout_is_one_lane_as_wide_as_the_band() {
    let lanes = DisplayFeatures::none().free_lanes(0.0, 48.0, 0.0, 360.0, 8.0);
    assert_eq!(spans(&lanes), vec![(0.0, 360.0)]);
    let lane = lanes.as_ref()[0];
    assert_eq!((lane.origin.y, lane.size.height), (0.0, 48.0));
}

#[test]
fn one_hole_in_the_middle_makes_two_lanes_and_the_margin_widens_the_gap_on_both_sides() {
    let f = features_with(&[(168.0, 12.0, 24.0, 24.0)]);
    let lanes = f.free_lanes(0.0, 48.0, 0.0, 360.0, 8.0);
    assert_eq!(spans(&lanes), vec![(0.0, 160.0), (200.0, 360.0)]);
    let bare = f.free_lanes(0.0, 48.0, 0.0, 360.0, 0.0);
    assert_eq!(spans(&bare), vec![(0.0, 168.0), (192.0, 360.0)]);
}

#[test]
fn a_hole_below_the_band_or_above_it_does_not_cut_the_band() {
    let below = features_with(&[(168.0, 60.0, 24.0, 24.0)]);
    assert_eq!(
        spans(&below.free_lanes(0.0, 48.0, 0.0, 360.0, 8.0)),
        vec![(0.0, 360.0)]
    );
    let above = features_with(&[(168.0, -40.0, 24.0, 24.0)]);
    assert_eq!(
        spans(&above.free_lanes(0.0, 48.0, 0.0, 360.0, 8.0)),
        vec![(0.0, 360.0)]
    );
}

#[test]
fn a_hole_that_only_touches_the_band_edge_does_not_cut_it() {
    // y + h == top: the hole ends where the band begins.
    let f = features_with(&[(168.0, 0.0, 24.0, 24.0)]);
    assert_eq!(
        spans(&f.free_lanes(24.0, 48.0, 0.0, 360.0, 0.0)),
        vec![(0.0, 360.0)]
    );
}

#[test]
fn a_hole_in_the_corner_leaves_one_lane_and_a_hole_covering_everything_leaves_none() {
    let corner = features_with(&[(304.0, 12.0, 28.0, 28.0)]);
    assert_eq!(
        spans(&corner.free_lanes(0.0, 48.0, 0.0, 360.0, 8.0)),
        vec![(0.0, 296.0), (340.0, 360.0)]
    );
    let edge = features_with(&[(340.0, 12.0, 28.0, 28.0)]);
    assert_eq!(
        spans(&edge.free_lanes(0.0, 48.0, 0.0, 360.0, 8.0)),
        vec![(0.0, 332.0)],
        "the lane a gap pushes off the band's end is dropped"
    );
    let all = features_with(&[(-10.0, 0.0, 400.0, 48.0)]);
    assert!(all
        .free_lanes(0.0, 48.0, 0.0, 360.0, 8.0)
        .as_ref()
        .is_empty());
}

#[test]
fn two_holes_make_three_lanes_and_overlapping_gaps_merge() {
    let two = features_with(&[(100.0, 12.0, 24.0, 24.0), (236.0, 12.0, 24.0, 24.0)]);
    assert_eq!(
        spans(&two.free_lanes(0.0, 48.0, 0.0, 360.0, 8.0)),
        vec![(0.0, 92.0), (132.0, 228.0), (268.0, 360.0)]
    );
    let near = features_with(&[(150.0, 12.0, 24.0, 24.0), (180.0, 12.0, 24.0, 24.0)]);
    assert_eq!(
        spans(&near.free_lanes(0.0, 48.0, 0.0, 360.0, 8.0)),
        vec![(0.0, 142.0), (212.0, 360.0)],
        "holes whose margins overlap are one gap"
    );
    // The order the shell lists them in does not matter.
    let reversed = features_with(&[(236.0, 12.0, 24.0, 24.0), (100.0, 12.0, 24.0, 24.0)]);
    assert_eq!(
        spans(&reversed.free_lanes(0.0, 48.0, 0.0, 360.0, 8.0)),
        spans(&two.free_lanes(0.0, 48.0, 0.0, 360.0, 8.0))
    );
}

#[test]
fn a_report_that_is_not_a_number_and_a_flood_of_reports_cannot_break_the_lanes() {
    let mut f = features_with(&[
        (f32::NAN, 12.0, 24.0, 24.0),
        (168.0, 12.0, f32::INFINITY, 24.0),
    ]);
    assert_eq!(
        spans(&f.free_lanes(0.0, 48.0, 0.0, 360.0, 8.0)),
        vec![(0.0, 360.0)]
    );
    let flood: Vec<(f32, f32, f32, f32)> = (0..500).map(|i| (i as f32, 12.0, 0.5, 24.0)).collect();
    f = features_with(&flood);
    assert!(f.cutouts_in_band(0.0, 48.0).len() <= MAX_CUTOUTS);
    // A margin that is not a number is no margin.
    let one = features_with(&[(168.0, 12.0, 24.0, 24.0)]);
    assert_eq!(
        spans(&one.free_lanes(0.0, 48.0, 0.0, 360.0, f32::NAN)),
        vec![(0.0, 168.0), (192.0, 360.0)]
    );
}

#[test]
fn a_lane_is_never_narrower_than_the_minimum() {
    let f = features_with(&[(0.5, 12.0, 24.0, 24.0)]);
    let lanes = f.free_lanes(0.0, 48.0, 0.0, 360.0, 0.0);
    assert_eq!(spans(&lanes), vec![(24.5, 360.0)]);
}

#[test]
fn off_is_nothing_and_every_other_preset_has_a_status_bar_in_portrait() {
    assert!(DisplayFeatures::simulate(DisplayPreset::Off, size(360.0, 720.0)).is_none());
    assert_eq!(
        DisplayPreset::Off.safe_area(size(360.0, 720.0)),
        SafeAreaInsets::default()
    );
    assert!(DisplayFeatures::none().is_none());
    for preset in [
        DisplayPreset::Pixel8,
        DisplayPreset::IPhoneDynamicIsland,
        DisplayPreset::IPhoneNotch,
        DisplayPreset::IPhoneHomeButton,
        DisplayPreset::CornerHole,
        DisplayPreset::DoubleHole,
    ] {
        let f = DisplayFeatures::simulate(preset, size(390.0, 844.0));
        assert!(!f.is_none(), "{preset:?}");
        let status = f
            .bars
            .status
            .into_option()
            .expect("a status bar in portrait");
        assert!(status.visible);
        assert_eq!((status.rect.origin.x, status.rect.origin.y), (0.0, 0.0));
        assert_eq!(
            status.rect.size.width, 390.0,
            "{preset:?}: as wide as the window"
        );
        // The band the insets keep clear holds every cutout's top.
        let top = inset_px(preset.safe_area(size(390.0, 844.0)).top);
        assert!(
            top >= status.rect.size.height - 0.01,
            "{preset:?}: inset {top}"
        );
    }
}

#[test]
fn the_pixel_8_has_one_hole_in_the_middle_of_the_top_edge_that_follows_the_window() {
    for width in [360.0_f32, 411.0, 900.0] {
        let f = DisplayFeatures::simulate(DisplayPreset::Pixel8, size(width, 914.0));
        let holes = f.cutouts.as_ref();
        assert_eq!(holes.len(), 1);
        let hole = holes[0];
        assert_eq!(hole.shape, CutoutShape::Circle);
        assert!(hole.approximate, "a simulation is never a measurement");
        let centre = hole.rect.origin.x + hole.rect.size.width / 2.0;
        assert!(
            (centre - width / 2.0).abs() < 0.01,
            "centred at {centre} in {width}"
        );
        assert!(hole.rect.origin.y >= 0.0 && hole.rect.origin.y + hole.rect.size.height <= 48.0);
    }
}

#[test]
fn on_its_side_the_hole_moves_to_the_left_edge_and_the_insets_follow() {
    let size = size(914.0, 411.0);
    let f = DisplayFeatures::simulate(DisplayPreset::Pixel8, size);
    let hole = f.cutouts.as_ref()[0];
    assert!(
        hole.rect.origin.x < 40.0,
        "at the left edge: {:?}",
        hole.rect
    );
    assert!((hole.rect.origin.y + hole.rect.size.height / 2.0 - 205.5).abs() < 0.01);
    let insets = DisplayPreset::Pixel8.safe_area(size);
    assert!(inset_px(insets.left) > inset_px(insets.right));
    // No hole in the status bar's band: a strip there has one lane.
    let status = f.bars.status.into_option().expect("a status bar");
    let lanes = f.free_lanes(
        status.rect.origin.y,
        status.rect.origin.y + status.rect.size.height,
        0.0,
        size.width,
        8.0,
    );
    assert_eq!(lanes.as_ref().len(), 1);
}

#[test]
fn the_dynamic_island_is_a_pill_and_the_notch_a_notch_and_the_home_button_phone_has_neither() {
    let island = DisplayFeatures::simulate(DisplayPreset::IPhoneDynamicIsland, size(393.0, 852.0));
    let c = island.cutouts.as_ref()[0];
    assert_eq!(c.shape, CutoutShape::Pill);
    assert_eq!((c.rect.size.width, c.rect.size.height), (126.0, 37.0));
    let notch = DisplayFeatures::simulate(DisplayPreset::IPhoneNotch, size(390.0, 844.0));
    assert_eq!(notch.cutouts.as_ref()[0].shape, CutoutShape::Notch);
    let se = DisplayFeatures::simulate(DisplayPreset::IPhoneHomeButton, size(375.0, 667.0));
    assert!(se.cutouts.as_ref().is_empty());
    let sensor = se.fingerprint.into_option().expect("Touch ID");
    assert_eq!(sensor.kind, FingerprintKind::Front);
    assert!(sensor.spot.is_none(), "a button has no spot on the display");
}

#[test]
fn the_double_hole_phone_makes_three_lanes_of_the_status_bar() {
    let f = DisplayFeatures::simulate(DisplayPreset::DoubleHole, size(360.0, 720.0));
    assert_eq!(f.cutouts.as_ref().len(), 2);
    let lanes = f.free_lanes(0.0, 40.0, 0.0, 360.0, 8.0);
    assert_eq!(lanes.as_ref().len(), 3);
}

#[test]
fn a_simulation_never_carries_a_keyboard_inset() {
    for preset in [DisplayPreset::Pixel8, DisplayPreset::IPhoneDynamicIsland] {
        for s in [size(360.0, 720.0), size(720.0, 360.0)] {
            assert!(preset.safe_area(s).keyboard.is_none());
        }
    }
}

#[test]
fn a_preset_is_found_by_its_name_whatever_the_case_and_the_spaces() {
    for preset in [
        DisplayPreset::Off,
        DisplayPreset::Pixel8,
        DisplayPreset::IPhoneDynamicIsland,
        DisplayPreset::IPhoneNotch,
        DisplayPreset::IPhoneHomeButton,
        DisplayPreset::CornerHole,
        DisplayPreset::DoubleHole,
    ] {
        assert_eq!(DisplayPreset::from_name(preset.name()), Some(preset));
        assert_eq!(
            DisplayPreset::from_name(&format!("  {}  ", preset.name().to_uppercase())),
            Some(preset)
        );
    }
    assert_eq!(DisplayPreset::from_name("pixel9"), None);
    assert_eq!(DisplayPreset::from_name(""), None);
}

#[test]
fn the_defaults_ask_for_nothing_and_change_nothing() {
    assert_eq!(DisplayPreset::default(), DisplayPreset::Off);
    let options = SystemBarsOptions::default();
    assert_eq!(options.status, SystemBarMode::Visible);
    assert_eq!(options.navigation, SystemBarMode::Visible);
    assert_eq!(options.icons, SystemBarIcons::Auto);
    assert!(!options.draw_into_cutout);
    assert!(!options.status_hidden());
    assert!(SystemBarsOptions {
        status: SystemBarMode::HiddenTransient,
        ..options
    }
    .status_hidden());
    assert!(SystemBarsOptions {
        status: SystemBarMode::HiddenSticky,
        ..options
    }
    .status_hidden());
    assert!(DisplayFeatures::default().is_none());
    let flags = crate::window::WindowFlags::default();
    assert_eq!(flags.system_bars, options);
    assert_eq!(flags.display_preset, DisplayPreset::Off);
}
