//! A dashed or dotted line is cut into its dashes on the whole line, before
//! it is cut into tiles: the pattern runs on across corners and tile edges,
//! and the outline follows the dashes. A walk is drawn dotted.

use alloc::vec::Vec;

use super::{
    super::autotest_generated::cache_at, dash_pattern, dash_runs, MapLatLon, MapLatLonVec,
    MapLineDash, MapOverlay, MapOverlayVec, MapPolyline,
};

fn length(run: &[(f64, f64)]) -> f64 {
    run.windows(2)
        .map(|w| ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt())
        .sum()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

#[test]
fn a_straight_line_is_cut_into_dashes_with_gaps_between_them() {
    let runs = dash_runs(&[(0.0, 0.0), (100.0, 0.0)], 10.0, 10.0);
    let starts: Vec<f64> = runs.iter().map(|r| r[0].0).collect();
    assert_eq!(starts, vec![0.0, 20.0, 40.0, 60.0, 80.0]);
    for run in &runs {
        assert!(close(length(run), 10.0), "{run:?} is one dash long");
    }
}

#[test]
fn a_dash_runs_on_around_a_corner() {
    // 5 along, then 10 down: the first dash turns the corner, the gap ends 4
    // down the second leg and the last dash ends with the line.
    let runs = dash_runs(&[(0.0, 0.0), (5.0, 0.0), (5.0, 10.0)], 6.0, 3.0);
    assert_eq!(runs.len(), 2, "{runs:?}");
    assert_eq!(runs[0], vec![(0.0, 0.0), (5.0, 0.0), (5.0, 1.0)], "the corner is in the dash");
    assert!(close(runs[1][0].1, 4.0) && close(runs[1][1].1, 10.0), "{:?}", runs[1]);
}

#[test]
fn a_dotted_line_is_round_dots_a_width_across_two_widths_apart() {
    let (dot, gap) = dash_pattern(MapLineDash::Dotted, 4.0).expect("a dotted line has a pattern");
    assert!(dot < 0.5, "a dot is a round cap on a stroke of almost no length: {dot}");
    let period = dot + gap;
    assert!(period > 4.0 * 1.4 && period < 4.0 * 3.0, "about two widths apart: {period}");
    let runs = dash_runs(&[(0.0, 0.0), (200.0, 0.0)], dot, gap);
    let expected = (200.0 / period).ceil() as usize;
    assert!(runs.len() + 1 >= expected && runs.len() <= expected, "{} dots, {expected} expected", runs.len());
}

#[test]
fn a_dashed_lines_gaps_stay_open_under_its_round_caps() {
    let width = 5.0;
    let (dash, gap) = dash_pattern(MapLineDash::Dashed, width).expect("a dashed line has a pattern");
    // Each round cap reaches half a width into the gap.
    assert!(gap > f64::from(width), "the gap {gap} is wider than the caps that reach into it");
    assert!(dash > f64::from(width), "a dash {dash} is longer than it is wide");
    assert_eq!(dash_pattern(MapLineDash::Solid, width), None);
}

fn walk(dash: MapLineDash) -> MapOverlayVec {
    // About 140 px of zoom 12 along the equator's south side, inside tile (2048, 2048).
    let points = MapLatLonVec::from_vec(vec![
        MapLatLon {
            lat_deg: -0.01,
            lon_deg: 0.001,
        },
        MapLatLon {
            lat_deg: -0.01,
            lon_deg: 0.05,
        },
    ]);
    MapOverlayVec::from_vec(vec![MapOverlay::Polyline(
        MapPolyline::create(points).with_width(4.0).with_dash(dash),
    )])
}

fn rings_on_the_tile(overlays: MapOverlayVec) -> usize {
    let mut cache = cache_at(-0.01, 0.025, 12.0);
    cache.set_overlays(overlays);
    let all = cache.overlays.clone();
    cache
        .overlay_cache
        .tile_shape(all.as_slice(), 0, 12, 2048, 2048)
        .stroke
        .map_or(0, |stroke| stroke.rings.as_slice().len())
}

#[test]
fn a_dotted_walk_is_drawn_as_many_dots_where_a_solid_line_is_one_stroke() {
    assert_eq!(rings_on_the_tile(walk(MapLineDash::Solid)), 1);
    let dots = rings_on_the_tile(walk(MapLineDash::Dotted));
    assert!(dots > 10, "a 140 px walk is more than ten dots: {dots}");
}

#[test]
fn changing_only_a_lines_dash_draws_it_again() {
    let mut cache = cache_at(-0.01, 0.025, 12.0);
    cache.set_overlays(walk(MapLineDash::Solid));
    let all = cache.overlays.clone();
    let solid = cache.overlay_cache.tile_shape(all.as_slice(), 0, 12, 2048, 2048);
    cache.set_overlays(walk(MapLineDash::Dashed));
    let all = cache.overlays.clone();
    let dashed = cache.overlay_cache.tile_shape(all.as_slice(), 0, 12, 2048, 2048);
    assert_ne!(solid, dashed, "the kept solid cut is not reused for the dashed line");
}
