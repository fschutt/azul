//! The lanes of the status bar's band and where the selected pill stands on a strip that
//! scrolls: a pill is never drawn under a cutout, and the selected one is never split.
//!
//! Not compiled by the author (house rule).

use azul_core::display_features::{DisplayFeatures, DisplayPreset};

use super::*;

fn band_of(preset: DisplayPreset, width: f32, height: f32) -> StatusBarBand {
    let size = LogicalSize { width, height };
    let features = DisplayFeatures::simulate(preset, size);
    let safe_area = preset.safe_area(size);
    StatusBarBand::from_display(&features, &safe_area, size)
}

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.01
}

fn spans_near(a: &[(f32, f32)], b: &[(f32, f32)]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(x, y)| near(x.0, y.0) && near(x.1, y.1))
}

/// Ten pills, 84 wide, 8 apart, the first at 12: the strip of a tab bar with ten tabs.
fn ten_pills() -> Vec<(f32, f32)> {
    (0..10).map(|i| (12.0 + i as f32 * 92.0, 84.0)).collect()
}

fn content_width(pills: &[(f32, f32)]) -> f32 {
    pills.last().map_or(0.0, |p| p.0 + p.1 + 12.0)
}

#[test]
fn a_window_without_a_status_bar_or_a_cutout_has_no_band_and_one_lane() {
    let band = StatusBarBand::from_display(
        &DisplayFeatures::none(),
        &SafeAreaInsets::default(),
        LogicalSize {
            width: 900.0,
            height: 700.0,
        },
    );
    assert!(!band.is_in_band());
    assert!(band.is_one_lane());
    assert_eq!(band.height, 0.0);
}

#[test]
fn the_pixel_8_band_is_two_lanes_around_the_camera() {
    let band = band_of(DisplayPreset::Pixel8, 360.0, 720.0);
    assert!(band.is_in_band());
    assert!(near(band.height, 48.0));
    assert!(!band.is_one_lane());
    // The hole is 24 wide in the middle; the margin of 8 on each side makes the gap 160..200.
    assert!(spans_near(
        &band.lane_spans(),
        &[(0.0, 160.0), (200.0, 360.0)]
    ));
    assert!(spans_near(&band.gaps(), &[(160.0, 200.0)]));
}

#[test]
fn on_its_side_the_pixel_8_has_one_lane_left_of_which_the_inset_is_a_gap() {
    let band = band_of(DisplayPreset::Pixel8, 780.0, 360.0);
    assert!(near(band.height, 24.0));
    // The camera is at the left edge half way down: not in the band; the left inset (36) is.
    assert!(band.is_one_lane());
    assert!(spans_near(&band.lane_spans(), &[(36.0, 780.0)]));
    assert!(spans_near(&band.gaps(), &[(0.0, 36.0)]));
}

#[test]
fn the_dynamic_island_band_has_two_equal_lanes() {
    let band = band_of(DisplayPreset::IPhoneDynamicIsland, 393.0, 852.0);
    assert!(near(band.height, 54.0));
    let lanes = band.lane_spans();
    assert_eq!(lanes.len(), 2);
    assert!(near(lanes[0].0, 0.0) && near(lanes[1].1, 393.0));
    // The pill is 126 wide, 8 on each side: the gap is 142 wide in the middle.
    assert!(near(lanes[0].1, 393.0 / 2.0 - 71.0));
    assert!(near(lanes[1].0, 393.0 / 2.0 + 71.0));
}

#[test]
fn a_double_hole_band_has_three_lanes_and_two_gaps() {
    let band = band_of(DisplayPreset::DoubleHole, 360.0, 720.0);
    assert_eq!(band.lane_spans().len(), 3);
    assert_eq!(band.gaps().len(), 2);
}

#[test]
fn a_band_made_for_a_window_that_is_not_a_number_is_empty_not_a_crash() {
    let band = StatusBarBand::from_display(
        &DisplayFeatures::simulate(
            DisplayPreset::Pixel8,
            LogicalSize {
                width: 360.0,
                height: 720.0,
            },
        ),
        &SafeAreaInsets::default(),
        LogicalSize {
            width: f32::NAN,
            height: 720.0,
        },
    );
    assert_eq!(band.width, 0.0);
    assert!(band.lanes.as_ref().len() <= 1);
}

#[test]
fn the_gaps_of_the_lanes_are_what_the_lanes_leave() {
    assert_eq!(
        gaps_between(&[(0.0, 100.0), (140.0, 300.0)], 300.0),
        vec![(100.0, 140.0)]
    );
    assert_eq!(
        gaps_between(&[(10.0, 100.0)], 120.0),
        vec![(0.0, 10.0), (100.0, 120.0)]
    );
    assert_eq!(gaps_between(&[], 50.0), vec![(0.0, 50.0)]);
    assert!(gaps_between(&[(0.0, 50.0)], 50.0).is_empty());
}

#[test]
fn the_covers_are_the_gaps_as_tall_as_the_band() {
    let band = band_of(DisplayPreset::Pixel8, 360.0, 720.0);
    let covers = cover_rects(&band);
    assert_eq!(covers.len(), 1);
    assert!(near(covers[0].origin.x, 160.0) && near(covers[0].size.width, 40.0));
    assert!(near(covers[0].size.height, 48.0));
}

#[test]
fn what_a_pill_shows_of_itself_is_its_pieces_in_the_lanes() {
    let lanes = [(0.0, 160.0), (200.0, 360.0)];
    // Whole in the left lane.
    assert_eq!(visible_pieces((20.0, 100.0), &lanes), vec![(20.0, 100.0)]);
    // Straddling the camera: two pieces, nothing between 160 and 200.
    assert_eq!(
        visible_pieces((120.0, 240.0), &lanes),
        vec![(120.0, 160.0), (200.0, 240.0)]
    );
    // Entirely under the camera: nothing.
    assert!(visible_pieces((165.0, 195.0), &lanes).is_empty());
    assert!(is_whole_in_a_lane((20.0, 100.0), &lanes));
    assert!(!is_whole_in_a_lane((120.0, 240.0), &lanes));
}

#[test]
fn a_pill_is_never_drawn_under_the_camera_at_any_scroll() {
    let band = band_of(DisplayPreset::Pixel8, 360.0, 720.0);
    let lanes = band.lane_spans();
    let gaps = band.gaps();
    for pill in ten_pills() {
        for step in 0..=60 {
            let scroll = step as f32 * 10.0;
            for piece in visible_pieces(pill_span(pill, scroll), &lanes) {
                for gap in &gaps {
                    assert!(
                        piece.1 <= gap.0 + 0.001 || piece.0 >= gap.1 - 0.001,
                        "pill {pill:?} at scroll {scroll}: the piece {piece:?} is under the gap {gap:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn the_selected_pill_stands_whole_in_a_lane_whatever_the_scroll_it_starts_from() {
    let band = band_of(DisplayPreset::Pixel8, 360.0, 720.0);
    let lanes = band.lane_spans();
    let pills = ten_pills();
    let max = max_scroll(content_width(&pills), band.width);
    for selected in 0..pills.len() {
        for step in 0..=((max / 10.0) as i32 + 1) {
            let now = (step as f32 * 10.0).min(max);
            let target = scroll_for_pill(pills[selected], now, max, &lanes);
            assert!(
                target >= 0.0 && target <= max + 0.001,
                "{target} outside 0..={max}"
            );
            assert!(
                is_whole_in_a_lane(pill_span(pills[selected], target), &lanes),
                "pill {selected} from scroll {now}: at {target} it is {:?} in {lanes:?}",
                pill_span(pills[selected], target)
            );
        }
    }
}

#[test]
fn a_pill_that_is_in_place_stays_where_it_is() {
    let lanes = [(0.0, 160.0), (200.0, 360.0)];
    // The first pill at scroll 0 is 12 from the lane's edge: in place.
    assert_eq!(scroll_for_pill((12.0, 84.0), 0.0, 600.0, &lanes), 0.0);
    // A pill in the right lane, 20 in: in place.
    assert_eq!(scroll_for_pill((520.0, 84.0), 300.0, 600.0, &lanes), 300.0);
}

#[test]
fn a_pill_that_is_not_in_place_is_centred_in_the_nearest_lane_as_far_as_the_range_allows() {
    // One lane the width of the view: the common strip.
    let lanes = [(0.0, 360.0)];
    // A pill far to the right is centred...
    let target = scroll_for_pill((700.0, 80.0), 0.0, 900.0, &lanes);
    assert!(near(target, 700.0 + 40.0 - 180.0), "centred, got {target}");
    // ... except where the scroll range ends: the last pill cannot be pulled to the middle.
    let last = scroll_for_pill((700.0, 80.0), 0.0, 480.0, &lanes);
    assert!(near(last, 480.0), "clamped to the range, got {last}");
    // The first pill, scrolled away from, comes back to 0 and no further.
    let first = scroll_for_pill((12.0, 80.0), 400.0, 900.0, &lanes);
    assert!(near(first, 0.0), "the first pill at the start, got {first}");
}

#[test]
fn a_pill_takes_the_lane_that_costs_the_shorter_scroll() {
    let lanes = [(0.0, 160.0), (200.0, 360.0)];
    // Scrolled to 300 and asked for the pill at 220..304: in the right lane it needs no scroll
    // beyond a few px, in the left one 300 px: the right lane.
    let target = scroll_for_pill((420.0, 84.0), 300.0, 800.0, &lanes);
    assert!(is_whole_in_a_lane(pill_span((420.0, 84.0), target), &lanes));
    assert!(
        (target - 300.0).abs() < 100.0,
        "a short scroll, got {target}"
    );
}

#[test]
fn a_pill_wider_than_every_lane_is_brought_to_the_start_of_the_widest_one() {
    let lanes = [(0.0, 60.0), (100.0, 190.0)];
    let target = scroll_for_pill((400.0, 120.0), 0.0, 500.0, &lanes);
    // The widest lane starts at 100: the pill at 100 means scroll 300.
    assert!(near(target, 300.0), "got {target}");
}

#[test]
fn numbers_that_are_not_numbers_do_not_move_the_strip_or_crash() {
    let lanes = [(0.0, 360.0)];
    assert_eq!(scroll_for_pill((f32::NAN, 80.0), 50.0, 400.0, &lanes), 50.0);
    assert_eq!(
        scroll_for_pill((100.0, f32::INFINITY), 50.0, 400.0, &lanes),
        50.0
    );
    assert_eq!(scroll_for_pill((100.0, 80.0), f32::NAN, 400.0, &lanes), 0.0);
    assert_eq!(scroll_for_pill((100.0, 80.0), 50.0, f32::NAN, &lanes), 0.0);
    assert_eq!(scroll_for_pill((100.0, 80.0), 50.0, 400.0, &[]), 50.0);
    assert_eq!(scroll_for_pill((100.0, -5.0), 50.0, 400.0, &lanes), 50.0);
}

#[test]
fn the_scroll_range_is_the_overflow_and_never_negative() {
    assert_eq!(max_scroll(1000.0, 360.0), 640.0);
    assert_eq!(max_scroll(300.0, 360.0), 0.0);
    assert_eq!(max_scroll(f32::NAN, 360.0), 0.0);
}

#[test]
fn a_strip_that_fits_does_not_scroll_and_every_pill_is_in_place() {
    let lanes = [(0.0, 360.0)];
    let pills: Vec<(f32, f32)> = (0..3).map(|i| (12.0 + i as f32 * 92.0, 84.0)).collect();
    let max = max_scroll(content_width(&pills), 360.0);
    assert_eq!(max, 0.0);
    for pill in pills {
        assert_eq!(scroll_for_pill(pill, 0.0, max, &lanes), 0.0);
    }
}
