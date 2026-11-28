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
    // The status bar is 24 dp on its side; a strip in it is a touch row at least.
    assert!(near(band.height, TOUCH_ROW));
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
    for (selected, pill) in pills.iter().copied().enumerate() {
        for step in 0..=((max / 10.0) as i32 + 1) {
            let now = (step as f32 * 10.0).min(max);
            let target = scroll_for_pill(pill, now, max, &lanes);
            assert!(
                target >= 0.0 && target <= max + 0.001,
                "{target} outside 0..={max}"
            );
            assert!(
                is_whole_in_a_lane(pill_span(pill, target), &lanes),
                "pill {selected} from scroll {now}: at {target} it is {:?} in {lanes:?}",
                pill_span(pill, target)
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

// ---- icon-only pills ----

const GAP: f32 = 8.0;

/// The pills of a layout as `(left, right)` spans.
fn pill_spans(layout: &IconPillLayout) -> Vec<(f32, f32)> {
    layout
        .positions
        .iter()
        .map(|&x| (x, x + layout.pill_width))
        .collect()
}

/// The lane each pill stands in (`None`: in no lane).
fn lane_index_of(span: (f32, f32), lanes: &[(f32, f32)]) -> Option<usize> {
    lanes
        .iter()
        .position(|&(left, right)| span.0 >= left && span.1 <= right)
}

#[test]
fn six_icon_pills_on_a_pixel_8_stand_three_and_three_around_the_camera() {
    let band = band_of(DisplayPreset::Pixel8, 411.0, 914.0);
    let lanes = band.lane_spans();
    assert_eq!(lanes.len(), 2);
    let layout = icon_pill_layout(&lanes, 6, GAP);
    assert!(layout.fits, "{layout:?}");
    assert!(!layout.capped);
    assert_eq!(layout.positions.len(), 6);
    // Every pill the same width, as wide as a touch target lets them be.
    assert!(near(layout.pill_width, ICON_PILL_MAX), "{}", layout.pill_width);
    let spans = pill_spans(&layout);
    let where_: Vec<Option<usize>> = spans.iter().map(|&s| lane_index_of(s, &lanes)).collect();
    assert_eq!(
        where_,
        vec![Some(0), Some(0), Some(0), Some(1), Some(1), Some(1)],
        "the first three in the left lane, the rest in the right one: {spans:?} in {lanes:?}"
    );
    // The same room between two pills of a lane.
    for pair in spans.windows(2).filter(|p| lane_index_of(p[0], &lanes) == lane_index_of(p[1], &lanes)) {
        assert!(near(pair[1].0 - pair[0].1, GAP), "{pair:?}");
    }
}

#[test]
fn the_two_groups_around_the_camera_mirror_each_other() {
    let band = band_of(DisplayPreset::Pixel8, 411.0, 914.0);
    let lanes = band.lane_spans();
    let layout = icon_pill_layout(&lanes, 6, GAP);
    let spans = pill_spans(&layout);
    // Lane 0 holds 3 pills, lane 1 the other 3; each group is centred in its lane, so the room
    // before the first pill of a lane equals the room after its last.
    for (lane, group) in [(lanes[0], &spans[0..3]), (lanes[1], &spans[3..6])] {
        let before = group[0].0 - lane.0;
        let after = lane.1 - group[2].1;
        assert!(near(before, after), "{before} before, {after} after, in {lane:?}");
        assert!(before >= LANE_INSET - 0.01, "at least the lane inset: {before}");
    }
}

#[test]
fn at_360_dp_six_icon_pills_are_40_wide_and_fill_the_lanes() {
    let band = band_of(DisplayPreset::Pixel8, 360.0, 720.0);
    let layout = icon_pill_layout(&band.lane_spans(), 6, GAP);
    assert!(layout.fits);
    // Lanes of 160 less 12 at both ends: 136 px; three 40 px pills and two gaps are 136.
    assert!(near(layout.pill_width, 40.0), "{}", layout.pill_width);
    let spans = pill_spans(&layout);
    assert!(near(spans[0].0, 12.0) && near(spans[2].1, 148.0), "{spans:?}");
    assert!(near(spans[3].0, 212.0) && near(spans[5].1, 348.0), "{spans:?}");
}

#[test]
fn the_dynamic_island_has_room_for_two_icon_pills_either_side() {
    let band = band_of(DisplayPreset::IPhoneDynamicIsland, 393.0, 852.0);
    let lanes = band.lane_spans();
    let four = icon_pill_layout(&lanes, 4, GAP);
    assert!(four.fits, "{four:?}");
    assert!(near(four.pill_width, ICON_PILL_MAX));
    let spans = pill_spans(&four);
    assert_eq!(
        spans.iter().map(|&s| lane_index_of(s, &lanes)).collect::<Vec<_>>(),
        vec![Some(0), Some(0), Some(1), Some(1)]
    );
    // Three: the odd one is in the first lane.
    let three = icon_pill_layout(&lanes, 3, GAP);
    assert!(three.fits);
    assert_eq!(
        pill_spans(&three).iter().map(|&s| lane_index_of(s, &lanes)).collect::<Vec<_>>(),
        vec![Some(0), Some(0), Some(1)]
    );
    // Five do not fit two lanes of 125 px at 32 px: the row scrolls, as a strip of text does.
    let five = icon_pill_layout(&lanes, 5, GAP);
    assert!(!five.fits);
    assert!(near(five.pill_width, ICON_PILL_MIN));
}

#[test]
fn a_strip_with_one_lane_is_one_group_centred_in_it() {
    let layout = icon_pill_layout(&[(0.0, 360.0)], 6, GAP);
    assert!(layout.fits);
    assert!(near(layout.pill_width, ICON_PILL_MAX));
    let spans = pill_spans(&layout);
    // 6 * 40 + 5 * 8 = 280 in 360: 40 each side.
    assert!(near(spans[0].0, 40.0) && near(spans[5].1, 320.0), "{spans:?}");
}

#[test]
fn one_pill_and_two_pills() {
    let band = band_of(DisplayPreset::Pixel8, 411.0, 914.0);
    let lanes = band.lane_spans();
    let one = icon_pill_layout(&lanes, 1, GAP);
    assert!(one.fits && one.positions.len() == 1);
    assert_eq!(lane_index_of(pill_spans(&one)[0], &lanes), Some(0));
    // Two: one in each lane, each centred in its own.
    let two = icon_pill_layout(&lanes, 2, GAP);
    assert!(two.fits);
    let spans = pill_spans(&two);
    assert_eq!(lane_index_of(spans[0], &lanes), Some(0));
    assert_eq!(lane_index_of(spans[1], &lanes), Some(1));
    let centre = |s: (f32, f32)| (s.0 + s.1) / 2.0;
    assert!(near(centre(spans[0]), (lanes[0].0 + lanes[0].1) / 2.0));
    assert!(near(centre(spans[1]), (lanes[1].0 + lanes[1].1) / 2.0));
}

#[test]
fn twelve_icon_pills_do_not_fit_and_scroll_as_a_row_of_the_smallest_pills() {
    let band = band_of(DisplayPreset::Pixel8, 360.0, 720.0);
    let layout = icon_pill_layout(&band.lane_spans(), 12, GAP);
    assert!(!layout.fits);
    assert!(near(layout.pill_width, ICON_PILL_MIN));
    assert_eq!(layout.positions.len(), 12);
    // The plain row of a strip: the first at the first lane's lead, the others 40 apart.
    assert!(near(layout.positions[0], LANE_INSET));
    for pair in layout.positions.windows(2) {
        assert!(near(pair[1] - pair[0], ICON_PILL_MIN + GAP), "{pair:?}");
    }
}

#[test]
fn whatever_the_count_no_icon_pill_is_under_a_cutout_when_they_fit() {
    for (preset, width, height) in [
        (DisplayPreset::Pixel8, 360.0, 720.0),
        (DisplayPreset::Pixel8, 411.0, 914.0),
        (DisplayPreset::IPhoneDynamicIsland, 393.0, 852.0),
        (DisplayPreset::IPhoneNotch, 390.0, 844.0),
        (DisplayPreset::CornerHole, 360.0, 760.0),
        (DisplayPreset::DoubleHole, 360.0, 720.0),
        (DisplayPreset::IPhoneHomeButton, 375.0, 667.0),
    ] {
        let band = band_of(preset, width, height);
        let lanes = band.lane_spans();
        for count in 0..=14 {
            let layout = icon_pill_layout(&lanes, count, GAP);
            assert_eq!(layout.positions.len(), count, "{preset:?} {count}");
            if !layout.fits {
                continue;
            }
            for span in pill_spans(&layout) {
                assert!(
                    lane_index_of(span, &lanes).is_some(),
                    "{preset:?} with {count} pills: {span:?} is not whole in a lane of {lanes:?}"
                );
            }
            // The pills of a lane keep the lane's inset at both ends.
            for span in pill_spans(&layout) {
                let lane = lanes[lane_index_of(span, &lanes).unwrap()];
                assert!(span.0 >= lane.0 + LANE_INSET - 0.01 && span.1 <= lane.1 - LANE_INSET + 0.01);
            }
        }
    }
}

#[test]
fn the_pills_of_a_fitting_layout_are_dealt_evenly_to_the_lanes() {
    let band = band_of(DisplayPreset::Pixel8, 411.0, 914.0);
    let lanes = band.lane_spans();
    for count in 1..=7 {
        let layout = icon_pill_layout(&lanes, count, GAP);
        if !layout.fits {
            continue;
        }
        let in_first = pill_spans(&layout)
            .iter()
            .filter(|&&s| lane_index_of(s, &lanes) == Some(0))
            .count();
        assert_eq!(in_first, count.div_ceil(2), "{count} pills");
    }
}

#[test]
fn lanes_that_cannot_take_an_even_share_pass_the_rest_to_the_lane_that_can() {
    // A narrow lane (one pill) and a wide one: five pills are 1 + 4, not 3 + 2.
    let lanes = [(0.0, 70.0), (100.0, 360.0)];
    let layout = icon_pill_layout(&lanes, 5, GAP);
    assert!(layout.fits, "{layout:?}");
    let spans = pill_spans(&layout);
    let first = spans.iter().filter(|&&s| lane_index_of(s, &lanes) == Some(0)).count();
    assert_eq!(first, 1, "{spans:?}");
}

#[test]
fn no_pills_is_a_layout_that_fits_and_a_crowd_is_capped() {
    let lanes = [(0.0, 360.0)];
    let none = icon_pill_layout(&lanes, 0, GAP);
    assert!(none.fits && none.positions.is_empty());
    let crowd = icon_pill_layout(&lanes, MAX_STRIP_PILLS + 40, GAP);
    assert!(crowd.capped, "the cap is said");
    assert_eq!(crowd.positions.len(), MAX_STRIP_PILLS);
    assert!(!crowd.fits);
    assert!(!icon_pill_layout(&lanes, MAX_STRIP_PILLS, GAP).capped);
}

#[test]
fn lanes_that_are_not_numbers_make_a_row_not_a_crash() {
    let nan = icon_pill_layout(&[(f32::NAN, 100.0), (50.0, f32::INFINITY)], 4, GAP);
    assert!(!nan.fits);
    assert_eq!(nan.positions.len(), 4);
    assert!(nan.positions.iter().all(|x| x.is_finite()));
    let none = icon_pill_layout(&[], 3, GAP);
    assert!(!none.fits);
    assert_eq!(none.positions.len(), 3);
    let backwards = icon_pill_layout(&[(300.0, 100.0)], 2, f32::NAN);
    assert_eq!(backwards.positions.len(), 2);
    assert!(backwards.gap.is_finite());
}

#[test]
fn a_strip_with_no_band_takes_the_widest_pill_the_window_lets_it() {
    // A 360 dp window: 6 pills at 40 with 12 at both ends and 8 between: 304.
    let fits = |total: f32| total <= 360.0;
    assert_eq!(icon_pill_width_where(6, 12.0, 8.0, fits), (40.0, true));
    // 8 pills: only at 32 (24 + 256 + 56 = 336; at 36 it is 368).
    assert_eq!(icon_pill_width_where(8, 12.0, 8.0, fits), (32.0, true));
    // 10 pills: not even at 32 (416): the smallest and a scrolling row.
    assert_eq!(icon_pill_width_where(10, 12.0, 8.0, fits), (32.0, false));
    // Nothing to fit always fits.
    assert_eq!(icon_pill_width_where(0, 12.0, 8.0, |_| false), (40.0, true));
}

#[test]
fn the_window_is_asked_widest_row_first_and_only_as_far_as_it_has_to() {
    let mut asked = Vec::new();
    let result = icon_pill_width_where(8, 12.0, 8.0, |total| {
        asked.push(total);
        total <= 360.0
    });
    assert_eq!(result, (32.0, true));
    // 40, 36, 32: three questions, from the widest row.
    assert_eq!(asked.len(), 3, "{asked:?}");
    assert!(asked.windows(2).all(|w| w[0] > w[1]), "{asked:?}");
    let mut once = 0;
    let _ = icon_pill_width_where(2, 12.0, 8.0, |_| {
        once += 1;
        true
    });
    assert_eq!(once, 1, "a row that fits at once is asked once");
}

// ---- at rest, no pill stands under a camera ----

/// AzWriter on a Pixel 8 overlay (QA, 2026-11-28): four tabs, `FILE HOME INSERT VIEW`, the camera's gap
/// between 175 and 237 dp. INSERT stood under it and the strip read `FILE HOME (blank) VIEW`.
#[test]
fn a_pill_that_would_stand_under_the_camera_starts_the_next_lane() {
    let lanes = [(0.0, 175.0), (237.0, 412.0)];
    let widths = [61.0, 72.0, 78.0, 67.0];
    let room = room_around_cutouts(&widths, 16.0, 8.0, &lanes, 16.0);
    // FILE 16..77 and HOME 85..157 stand in the left lane (to 175 less the fade's 16: 159); INSERT would
    // be 165..243, under the gap: it starts the right lane, 16 in from its edge (253), VIEW follows.
    assert_eq!(room.len(), 4);
    assert!(near(room[0], 0.0) && near(room[1], 0.0), "{room:?}");
    assert!(near(room[2], 253.0 - 165.0), "INSERT is moved to the right lane: {room:?}");
    assert!(near(room[3], 0.0), "VIEW follows INSERT: {room:?}");
    // and where each one stands then: none of them in the gap
    let mut x = 16.0;
    for (width, extra) in widths.iter().zip(&room) {
        x += extra;
        let span = (x, x + width);
        assert!(
            visible_pieces(span, &[(175.0, 237.0)]).is_empty(),
            "a pill at {span:?} stands in the camera's gap"
        );
        x += width + 8.0;
    }
}

#[test]
fn a_strip_that_fits_its_first_lane_has_no_room_added() {
    let lanes = [(0.0, 175.0), (237.0, 412.0)];
    assert_eq!(room_around_cutouts(&[40.0, 40.0], 16.0, 8.0, &lanes, 16.0), vec![0.0, 0.0]);
    assert_eq!(room_around_cutouts(&[], 16.0, 8.0, &lanes, 16.0), Vec::<f32>::new());
}

#[test]
fn a_strip_without_a_cutout_in_it_has_no_room_added() {
    // one lane (a window without a camera in its band, an iPhone on its side), and none at all
    for lanes in [&[(0.0, 360.0)][..], &[][..]] {
        assert_eq!(
            room_around_cutouts(&[100.0, 100.0, 100.0, 100.0], 16.0, 8.0, lanes, 16.0),
            vec![0.0; 4],
            "{lanes:?}"
        );
    }
}

#[test]
fn the_chevron_takes_room_from_the_first_lane_and_the_pills_move_over_when_it_is_full() {
    let lanes = [(0.0, 175.0), (237.0, 412.0)];
    // 32 more before the first pill: FILE stands, HOME (117..189) no longer fits the first lane
    let room = room_around_cutouts(&[61.0, 72.0, 78.0, 67.0], 48.0, 8.0, &lanes, 16.0);
    assert!(near(room[0], 0.0), "{room:?}");
    assert!(near(room[1], 253.0 - 117.0), "HOME starts the right lane: {room:?}");
    // INSERT and VIEW fill the right lane and run on past it (the strip scrolls): no more room
    assert!(near(room[2], 0.0) && near(room[3], 0.0), "{room:?}");
}

#[test]
fn a_strip_of_three_lanes_hops_each_gap() {
    // two cutouts: three lanes 0..100, 140..260 and 300..400
    let lanes = [(0.0, 100.0), (140.0, 260.0), (300.0, 400.0)];
    let room = room_around_cutouts(&[60.0, 60.0, 60.0, 60.0], 16.0, 8.0, &lanes, 16.0);
    // 16..76 in the first; 84..144 would end in the gap: 156..216 in the second; 224..284 would not
    // fit the rest of it: 316..376 in the third; the fourth runs on
    assert!(near(room[0], 0.0), "{room:?}");
    assert!(near(room[1], 156.0 - 84.0), "{room:?}");
    assert!(near(room[2], 316.0 - 224.0), "{room:?}");
    assert!(near(room[3], 0.0), "{room:?}");
}

#[test]
fn a_pill_wider_than_every_lane_and_numbers_that_are_not_numbers_do_not_crash() {
    let lanes = [(0.0, 175.0), (237.0, 412.0)];
    // 300 wide fits nowhere: it is placed in the last lane (where the strip clips it) and nothing panics
    let room = room_around_cutouts(&[300.0, 50.0], 16.0, 8.0, &lanes, 16.0);
    assert!(room.iter().all(|r| r.is_finite() && *r >= 0.0), "{room:?}");
    for bad in [f32::NAN, f32::INFINITY, -5.0, 0.0] {
        let room = room_around_cutouts(&[bad, 40.0], 16.0, 8.0, &lanes, 16.0);
        assert!(room.iter().all(|r| r.is_finite() && *r >= 0.0), "{bad}: {room:?}");
    }
    assert_eq!(
        room_around_cutouts(&[40.0], f32::NAN, 8.0, &lanes, 16.0),
        vec![0.0]
    );
}

/// Every pill of a longer row stands whole in a lane or runs on past the last one: never in a gap.
#[test]
fn no_pill_of_any_strip_stands_in_a_gap_at_rest() {
    for preset in [
        DisplayPreset::Pixel8,
        DisplayPreset::IPhoneDynamicIsland,
        DisplayPreset::IPhoneNotch,
        DisplayPreset::CornerHole,
        DisplayPreset::DoubleHole,
    ] {
        for width in [360.0, 390.0, 412.0, 430.0] {
            let band = band_of(preset, width, 800.0);
            let (lanes, gaps) = (band.lane_spans(), band.gaps());
            for count in 2..14 {
                let widths: Vec<f32> = (0..count).map(|i| 48.0 + (i * 13 % 40) as f32).collect();
                let lead = lanes.first().map_or(0.0, |l| l.0) + 16.0;
                let room = room_around_cutouts(&widths, lead, 8.0, &lanes, 16.0);
                let mut x = lead;
                let last_lane_end = lanes.last().map_or(width, |l| l.1);
                for (w, extra) in widths.iter().zip(&room) {
                    assert!(*extra >= 0.0);
                    x += extra;
                    let span = (x, x + w);
                    // a pill that starts inside the window stands in no gap (the insets at the very
                    // edge are gaps too: the first pill starts after the first lane's edge)
                    if span.0 < last_lane_end - 16.0 {
                        for gap in &gaps {
                            let overlap = span.1.min(gap.1) - span.0.max(gap.0);
                            assert!(
                                overlap <= 0.01 || gap.1 >= last_lane_end,
                                "{preset:?} {width}: pill {span:?} of {count} stands in the gap {gap:?}: {room:?}"
                            );
                        }
                    }
                    x += w + 8.0;
                }
            }
        }
    }
}
