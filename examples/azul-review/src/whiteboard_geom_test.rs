//! The whiteboard's plane: rectangles, the point where an arrow leaves a box, curves, polygons.
//! No window, no azul.

use super::*;

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-3
}

fn close_pt(a: Pt, b: Pt) -> bool {
    close(a.x, b.x) && close(a.y, b.y)
}

#[test]
fn a_rectangle_from_two_corners_is_the_same_whichever_corner_comes_first() {
    let a = Rect::from_corners(Pt::new(10.0, 40.0), Pt::new(30.0, 20.0));
    let b = Rect::from_corners(Pt::new(30.0, 20.0), Pt::new(10.0, 40.0));
    assert_eq!(a, b);
    assert_eq!(a, Rect::new(10.0, 20.0, 20.0, 20.0));
}

#[test]
fn a_rectangle_around_points_and_none_around_no_points() {
    let pts = [Pt::new(3.0, 4.0), Pt::new(-1.0, 9.0), Pt::new(5.0, 0.0)];
    assert_eq!(Rect::from_points(&pts), Some(Rect::new(-1.0, 0.0, 6.0, 9.0)));
    assert_eq!(Rect::from_points(&[]), None);
}

#[test]
fn a_point_on_the_edge_is_inside_and_just_past_it_is_not() {
    let r = Rect::new(0.0, 0.0, 10.0, 10.0);
    assert!(r.contains(Pt::new(0.0, 0.0)));
    assert!(r.contains(Pt::new(10.0, 10.0)));
    assert!(!r.contains(Pt::new(10.01, 5.0)));
    assert!(!r.contains(Pt::new(5.0, -0.01)));
}

#[test]
fn two_rectangles_that_only_touch_do_not_intersect_and_overlapping_ones_give_the_overlap() {
    let a = Rect::new(0.0, 0.0, 10.0, 10.0);
    let touching = Rect::new(10.0, 0.0, 5.0, 5.0);
    let over = Rect::new(5.0, 6.0, 10.0, 10.0);
    assert!(!a.intersects(&touching));
    assert_eq!(a.intersection(&touching), None);
    assert_eq!(a.intersection(&over), Some(Rect::new(5.0, 6.0, 5.0, 4.0)));
    assert_eq!(a.union(&over), Rect::new(0.0, 0.0, 15.0, 16.0));
}

#[test]
fn the_distance_to_a_rectangle_is_zero_inside_and_the_gap_outside() {
    let r = Rect::new(0.0, 0.0, 10.0, 10.0);
    assert!(close(r.distance_to(Pt::new(5.0, 5.0)), 0.0));
    assert!(close(r.distance_to(Pt::new(13.0, 5.0)), 3.0));
    assert!(close(r.distance_to(Pt::new(13.0, 14.0)), 5.0), "past a corner: the straight distance to the corner");
}

#[test]
fn an_arrow_leaves_a_box_on_the_straight_line_to_its_target() {
    let r = Rect::new(0.0, 0.0, 100.0, 50.0);
    // Straight right: the middle of the right side.
    assert!(close_pt(ray_exit(&r, Pt::new(500.0, 25.0)), Pt::new(100.0, 25.0)));
    // Straight up: the middle of the top side.
    assert!(close_pt(ray_exit(&r, Pt::new(50.0, -300.0)), Pt::new(50.0, 0.0)));
    // A diagonal through a corner of a square leaves at the corner.
    let sq = Rect::new(0.0, 0.0, 40.0, 40.0);
    assert!(close_pt(ray_exit(&sq, Pt::new(200.0, 200.0)), Pt::new(40.0, 40.0)));
    // A shallow diagonal leaves through the long side, not the short one.
    let p = ray_exit(&r, Pt::new(250.0, 75.0));
    assert!(close(p.x, 100.0) && p.y > 25.0 && p.y < 50.0, "{p:?}");
}

#[test]
fn a_target_inside_the_box_or_at_its_centre_still_gives_a_point_on_the_boundary() {
    let r = Rect::new(0.0, 0.0, 100.0, 50.0);
    let p = ray_exit(&r, r.center());
    assert!(close_pt(p, Pt::new(100.0, 25.0)), "no direction: the middle of the right side, got {p:?}");
    let inside = ray_exit(&r, Pt::new(60.0, 25.0));
    assert!(close_pt(inside, Pt::new(100.0, 25.0)), "{inside:?}");
}

#[test]
fn the_side_a_point_is_nearest_to_and_the_rim_band_along_the_edges() {
    let r = Rect::new(100.0, 100.0, 200.0, 100.0);
    assert_eq!(nearest_side(&r, Pt::new(105.0, 150.0)), Side::Left);
    assert_eq!(nearest_side(&r, Pt::new(295.0, 150.0)), Side::Right);
    assert_eq!(nearest_side(&r, Pt::new(200.0, 102.0)), Side::Top);
    assert_eq!(nearest_side(&r, Pt::new(200.0, 199.0)), Side::Bottom);

    assert_eq!(rim_side(&r, Pt::new(103.0, 150.0), 10.0), Some(Side::Left));
    assert_eq!(rim_side(&r, Pt::new(95.0, 150.0), 10.0), Some(Side::Left), "the band reaches outside too");
    assert_eq!(rim_side(&r, Pt::new(200.0, 150.0), 10.0), None, "the middle of the box is no rim");
    assert_eq!(rim_side(&r, Pt::new(60.0, 150.0), 10.0), None, "far outside is no rim");
}

#[test]
fn the_middle_of_a_side() {
    let r = Rect::new(0.0, 0.0, 100.0, 50.0);
    assert_eq!(side_mid(&r, Side::Top), Pt::new(50.0, 0.0));
    assert_eq!(side_mid(&r, Side::Right), Pt::new(100.0, 25.0));
    assert_eq!(side_mid(&r, Side::Bottom), Pt::new(50.0, 50.0));
    assert_eq!(side_mid(&r, Side::Left), Pt::new(0.0, 25.0));
    assert_eq!(Side::Top.opposite(), Side::Bottom);
    assert_eq!(Side::from_name(Side::Left.name()), Some(Side::Left));
    assert_eq!(Side::from_name("diagonal"), None);
}

#[test]
fn a_curve_with_no_bend_is_the_straight_line() {
    let c = Curve::bent(Pt::new(0.0, 0.0), Pt::new(100.0, 0.0), 0.0);
    assert!(close_pt(c.at(0.5), Pt::new(50.0, 0.0)));
    assert!(close(c.length(), 100.0));
    assert!(close(c.distance_to(Pt::new(30.0, 4.0)), 4.0));
}

#[test]
fn a_positive_bend_bulges_to_the_right_of_the_way_the_arrow_goes() {
    // Going right on a screen with y down, the right hand is down.
    let right = Curve::bent(Pt::new(0.0, 0.0), Pt::new(100.0, 0.0), 0.4);
    assert!(right.at(0.5).y > 15.0, "{:?}", right.at(0.5));
    let left = Curve::bent(Pt::new(0.0, 0.0), Pt::new(100.0, 0.0), -0.4);
    assert!(left.at(0.5).y < -15.0);
    // Going left, the same bend bulges up.
    let back = Curve::bent(Pt::new(100.0, 0.0), Pt::new(0.0, 0.0), 0.4);
    assert!(back.at(0.5).y < -15.0);
}

#[test]
fn the_bend_of_a_drawn_stroke_is_read_back_from_its_points() {
    let truth = Curve::bent(Pt::new(0.0, 0.0), Pt::new(200.0, 40.0), 0.35);
    let drawn: Vec<Pt> = (0..=30).map(|i| truth.at(i as f32 / 30.0)).collect();
    let bend = fit_bend(&drawn);
    assert!((bend - 0.35).abs() < 0.03, "{bend}");

    let straight: Vec<Pt> = (0..=30).map(|i| Pt::new(i as f32 * 5.0, i as f32 * 1.0)).collect();
    assert_eq!(fit_bend(&straight), 0.0);
    // A wobble of a few pixels over a long stroke is not a curve.
    let wobbly: Vec<Pt> = (0..=30).map(|i| Pt::new(i as f32 * 10.0, if i % 2 == 0 { 2.0 } else { -2.0 })).collect();
    assert_eq!(fit_bend(&wobbly), 0.0);
    assert_eq!(fit_bend(&[Pt::new(1.0, 1.0)]), 0.0, "one point has no bend");
}

#[test]
fn a_very_bent_stroke_is_capped_not_followed_into_a_loop() {
    let truth = Curve::bent(Pt::new(0.0, 0.0), Pt::new(100.0, 0.0), 0.8);
    let drawn: Vec<Pt> = (0..=30).map(|i| truth.at(i as f32 / 30.0)).collect();
    assert!(fit_bend(&drawn) <= MAX_BEND + 1e-4);
    assert!(fit_bend(&drawn) > 0.5);
}

#[test]
fn the_direction_at_the_end_of_a_curve_points_along_the_last_stretch() {
    let c = Curve::bent(Pt::new(0.0, 0.0), Pt::new(100.0, 0.0), 0.0);
    assert!(close_pt(c.tangent_at(1.0), Pt::new(1.0, 0.0)));
    let bent = Curve::bent(Pt::new(0.0, 0.0), Pt::new(100.0, 0.0), 0.5);
    let t = bent.tangent_at(1.0);
    assert!(t.x > 0.0 && t.y < 0.0, "a bulge down comes back up to the end: {t:?}");
    assert!(close(t.len(), 1.0));
}

#[test]
fn an_arrowhead_sits_behind_the_tip_across_the_direction() {
    let [tip, a, b] = arrowhead(Pt::new(100.0, 0.0), Pt::new(1.0, 0.0), 10.0, 4.0);
    assert_eq!(tip, Pt::new(100.0, 0.0));
    assert!(close(a.x, 90.0) && close(b.x, 90.0));
    assert!(close(a.y.abs(), 4.0) && close(b.y.abs(), 4.0) && a.y != b.y);
}

fn square() -> Vec<Pt> {
    vec![Pt::new(0.0, 0.0), Pt::new(10.0, 0.0), Pt::new(10.0, 10.0), Pt::new(0.0, 10.0)]
}

#[test]
fn the_area_of_a_polygon_does_not_depend_on_its_direction() {
    let mut sq = square();
    assert!(close(polygon_area(&sq).abs(), 100.0));
    let forward = polygon_area(&sq);
    sq.reverse();
    assert!(close(polygon_area(&sq), -forward));
    assert_eq!(polygon_area(&[Pt::new(0.0, 0.0), Pt::new(5.0, 5.0)]), 0.0, "two points enclose nothing");
}

/// An L: the unit square 10x10 without its top-right 5x5.
fn el() -> Vec<Pt> {
    vec![
        Pt::new(0.0, 0.0),
        Pt::new(5.0, 0.0),
        Pt::new(5.0, 5.0),
        Pt::new(10.0, 5.0),
        Pt::new(10.0, 10.0),
        Pt::new(0.0, 10.0),
    ]
}

#[test]
fn a_point_is_inside_a_concave_polygon_only_where_the_polygon_is() {
    let l = el();
    assert!(point_in_polygon(&l, Pt::new(2.0, 2.0)));
    assert!(point_in_polygon(&l, Pt::new(8.0, 8.0)));
    assert!(!point_in_polygon(&l, Pt::new(8.0, 2.0)), "the notch");
    assert!(!point_in_polygon(&l, Pt::new(-1.0, 5.0)));
    assert!(!point_in_polygon(&[Pt::new(0.0, 0.0), Pt::new(1.0, 1.0)], Pt::new(0.5, 0.5)));
}

#[test]
fn the_overlap_of_a_polygon_and_a_rectangle_is_the_area_they_share() {
    let sq = square();
    assert!(close(polygon_rect_overlap(&sq, &Rect::new(5.0, 5.0, 20.0, 20.0)), 25.0));
    assert!(close(polygon_rect_overlap(&sq, &Rect::new(-5.0, -5.0, 50.0, 50.0)), 100.0), "the polygon is inside the rectangle");
    assert!(close(polygon_rect_overlap(&sq, &Rect::new(2.0, 2.0, 3.0, 3.0)), 9.0), "the rectangle is inside the polygon");
    assert!(close(polygon_rect_overlap(&sq, &Rect::new(20.0, 20.0, 5.0, 5.0)), 0.0));
}

#[test]
fn the_overlap_with_a_concave_polygon_leaves_out_the_notch() {
    let l = el();
    // The whole bounding box: 100 minus the 25 of the notch.
    assert!(close(polygon_rect_overlap(&l, &Rect::new(0.0, 0.0, 10.0, 10.0)), 75.0));
    // A strip across the notch: 5 wide of the 10, 2 tall.
    assert!(close(polygon_rect_overlap(&l, &Rect::new(0.0, 2.0, 10.0, 2.0)), 10.0));
}

#[test]
fn simplifying_a_line_with_points_on_it_keeps_its_two_ends_and_a_corner_stays() {
    let line: Vec<Pt> = (0..=20).map(|i| Pt::new(i as f32, i as f32 * 0.5)).collect();
    assert_eq!(simplify(&line, 0.5), vec![line[0], line[20]]);

    let mut corner: Vec<Pt> = (0..=10).map(|i| Pt::new(i as f32, 0.0)).collect();
    corner.extend((1..=10).map(|i| Pt::new(10.0, i as f32)));
    let s = simplify(&corner, 0.5);
    assert_eq!(s, vec![Pt::new(0.0, 0.0), Pt::new(10.0, 0.0), Pt::new(10.0, 10.0)]);
}

#[test]
fn simplifying_a_circle_keeps_it_round_within_the_tolerance() {
    let circle: Vec<Pt> = (0..200)
        .map(|i| {
            let a = i as f32 / 200.0 * std::f32::consts::TAU;
            Pt::new(100.0 + 50.0 * a.cos(), 100.0 + 50.0 * a.sin())
        })
        .collect();
    let s = simplify(&circle, 1.0);
    assert!(s.len() < 60 && s.len() > 12, "{}", s.len());
    // No dropped point is further than the tolerance from the simplified outline.
    for p in &circle {
        let d = s.windows(2).map(|w| dist_point_segment(*p, w[0], w[1])).fold(f32::MAX, f32::min);
        assert!(d <= 1.0 + 1e-3, "{d}");
    }
}

#[test]
fn a_huge_stroke_is_thinned_first_so_the_work_stays_bounded() {
    // 50 000 points on a zigzag, every one of them a corner: the worst case for the line's
    // subdivision. It is looked at as at most MAX_SIMPLIFY_POINTS, in a loop, not a call per point.
    let zig: Vec<Pt> = (0..50_001).map(|i| Pt::new(i as f32, if i % 2 == 0 { 0.0 } else { 20.0 })).collect();
    let began = std::time::Instant::now();
    let s = simplify(&zig, 1.0);
    assert!(s.len() <= MAX_SIMPLIFY_POINTS + 1, "{}", s.len());
    assert_eq!(s.first(), zig.first());
    assert_eq!(s.last(), zig.last(), "the end stays");
    assert!(began.elapsed().as_secs() < 5, "{:?}", began.elapsed());
}

#[test]
fn the_distance_to_a_segment_and_where_two_segments_cross() {
    assert!(close(dist_point_segment(Pt::new(5.0, 3.0), Pt::new(0.0, 0.0), Pt::new(10.0, 0.0)), 3.0));
    assert!(close(dist_point_segment(Pt::new(13.0, 4.0), Pt::new(0.0, 0.0), Pt::new(10.0, 0.0)), 5.0));
    assert!(close(dist_point_segment(Pt::new(1.0, 1.0), Pt::new(2.0, 2.0), Pt::new(2.0, 2.0)), 2f32.sqrt()), "a segment of one point");

    let hit = segment_crossing(Pt::new(0.0, 0.0), Pt::new(10.0, 10.0), Pt::new(0.0, 10.0), Pt::new(10.0, 0.0));
    assert!(hit.is_some_and(|p| close_pt(p, Pt::new(5.0, 5.0))));
    assert_eq!(segment_crossing(Pt::new(0.0, 0.0), Pt::new(10.0, 0.0), Pt::new(0.0, 5.0), Pt::new(10.0, 5.0)), None, "parallel");
    assert_eq!(segment_crossing(Pt::new(0.0, 0.0), Pt::new(2.0, 2.0), Pt::new(6.0, 0.0), Pt::new(0.0, 6.0)), None, "their lines cross at (3, 3), past the end of the first");
}

#[test]
fn the_length_of_a_path() {
    let p = [Pt::new(0.0, 0.0), Pt::new(3.0, 4.0), Pt::new(3.0, 14.0)];
    assert!(close(path_length(&p), 15.0));
    assert_eq!(path_length(&[]), 0.0);
}

#[test]
fn a_polygon_as_an_svg_path_is_a_closed_line_with_short_numbers() {
    let d = svg_path(&[Pt::new(1.0, 2.0), Pt::new(10.5, 2.0), Pt::new(10.5, 8.25)]);
    assert_eq!(d, "M1 2L10.5 2L10.5 8.25Z");
    assert_eq!(svg_path(&[]), "");
}

#[test]
fn a_view_maps_the_screen_to_the_board_and_back_and_zooms_around_a_point() {
    let v = View { pan: Pt::new(100.0, 50.0), zoom: 2.0 };
    let board = v.to_board(Pt::new(40.0, 20.0));
    assert!(close_pt(board, Pt::new(120.0, 60.0)));
    assert!(close_pt(v.to_screen(board), Pt::new(40.0, 20.0)));

    // Zooming around the cursor keeps the board point under it where it was.
    let at = Pt::new(300.0, 200.0);
    let before = v.to_board(at);
    let z = v.zoomed_at(at, 1.5, 0.2, 4.0);
    assert!(close(z.zoom, 3.0));
    assert!(close_pt(z.to_board(at), before));
    // The zoom stays inside its range.
    assert!(close(v.zoomed_at(at, 100.0, 0.2, 4.0).zoom, 4.0));
    assert!(close(v.zoomed_at(at, 0.0001, 0.2, 4.0).zoom, 0.2));
}
