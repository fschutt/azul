//! Where a cutout that is sent to the whiteboard by a button goes: the first free place of the view, in reading order.
//! No window, no azul.

use super::*;

const VIEW: Rect = Rect { x: 0.0, y: 0.0, w: 400.0, h: 600.0 };

fn r(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect::new(x, y, w, h)
}

#[test]
fn an_empty_view_puts_it_at_the_margin() {
    assert_eq!(free_spot(&[], VIEW, Size::new(120.0, 80.0), GAP), Spot::InView(Pt::new(16.0, 16.0)));
    let moved = r(-300.0, 500.0, 400.0, 600.0);
    assert_eq!(free_spot(&[], moved, Size::new(120.0, 80.0), GAP).at(), Pt::new(-284.0, 516.0), "the margin of the view, wherever it is");
}

#[test]
fn it_goes_right_of_what_is_there_when_the_row_has_room() {
    let taken = [r(16.0, 16.0, 120.0, 80.0)];
    assert_eq!(free_spot(&taken, VIEW, Size::new(120.0, 80.0), GAP), Spot::InView(Pt::new(152.0, 16.0)), "a gap past its right edge");
}

#[test]
fn it_goes_below_when_the_row_has_no_room() {
    // 16 + 150 + 16 + 150 = 332; a third 150 wide would end at 498 + 16 > 400.
    let taken = [r(16.0, 16.0, 150.0, 80.0), r(182.0, 16.0, 150.0, 80.0)];
    assert_eq!(free_spot(&taken, VIEW, Size::new(150.0, 80.0), GAP), Spot::InView(Pt::new(16.0, 112.0)), "the second row, at the margin");
}

#[test]
fn a_hole_in_a_row_above_is_used_before_a_new_row() {
    // A tall cutout on the left, a short one beside it: the next cutout goes under the short one, beside the tall one.
    let taken = [r(16.0, 16.0, 150.0, 300.0), r(182.0, 16.0, 150.0, 60.0)];
    assert_eq!(free_spot(&taken, VIEW, Size::new(150.0, 80.0), GAP), Spot::InView(Pt::new(182.0, 92.0)));
}

#[test]
fn a_full_view_puts_it_under_everything() {
    let taken = [r(16.0, 16.0, 368.0, 568.0)];
    assert_eq!(free_spot(&taken, VIEW, Size::new(120.0, 80.0), GAP), Spot::Below(Pt::new(16.0, 600.0)), "a gap under the lowest");
    // What lies far below in a view that is full is what it goes under.
    let taken = [r(16.0, 16.0, 368.0, 568.0), r(16.0, 2_000.0, 50.0, 50.0)];
    assert_eq!(free_spot(&taken, VIEW, Size::new(120.0, 80.0), GAP).at(), Pt::new(16.0, 2_066.0));
}

#[test]
fn a_cutout_larger_than_the_view_starts_at_the_margin_and_overlaps_nothing() {
    let big = Size::new(800.0, 900.0);
    assert_eq!(free_spot(&[], VIEW, big, GAP), Spot::InView(Pt::new(16.0, 16.0)));
    // Something in the way, even out of the view: it is a cutout that reaches there, so it is looked at.
    let taken = [r(500.0, 100.0, 50.0, 50.0)];
    let at = free_spot(&taken, VIEW, big, GAP).at();
    assert!(!r(at.x - GAP, at.y - GAP, big.w + 2.0 * GAP, big.h + 2.0 * GAP).intersects(&taken[0]), "{at:?}");
}

#[test]
fn what_is_out_of_the_view_is_no_obstacle_and_what_reaches_into_it_is() {
    let far = [r(1_000.0, 0.0, 200.0, 200.0), r(-900.0, 0.0, 200.0, 200.0), r(0.0, 2_000.0, 200.0, 200.0)];
    assert_eq!(free_spot(&far, VIEW, Size::new(120.0, 80.0), GAP).at(), Pt::new(16.0, 16.0));
    let reaching = [r(-200.0, 0.0, 250.0, 100.0)];
    assert_eq!(free_spot(&reaching, VIEW, Size::new(120.0, 80.0), GAP).at(), Pt::new(66.0, 16.0), "right of the part that is in view");
}

#[test]
fn a_view_with_more_cutouts_than_are_looked_at_is_full() {
    let many: Vec<Rect> = (0..=MAX_LOOKED_AT).map(|i| r(i as f32 * 3.0, 0.0, 2.0, 2.0)).collect();
    assert!(many.len() > MAX_LOOKED_AT);
    assert!(matches!(free_spot(&many, VIEW, Size::new(10.0, 10.0), GAP), Spot::Below(_)));
    // The same number out of view do not count.
    let out: Vec<Rect> = (0..=MAX_LOOKED_AT).map(|i| r(5_000.0 + i as f32 * 3.0, 0.0, 2.0, 2.0)).collect();
    assert_eq!(free_spot(&out, VIEW, Size::new(10.0, 10.0), GAP), Spot::InView(Pt::new(16.0, 16.0)));
}

/// A small generator, so that the scenes are the same on every run.
struct Lcg(u64);

impl Lcg {
    fn below(&mut self, n: i32) -> i32 {
        self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        i32::try_from((self.0 >> 33) % u64::from(n.max(1).unsigned_abs())).unwrap_or(0)
    }
}

#[test]
fn the_place_is_the_first_free_one_in_reading_order_for_any_scene() {
    let mut rng = Lcg(7);
    let (mut inside, mut below) = (0, 0);
    for scene in 0..400 {
        let gap = 1 + rng.below(6);
        let view = (rng.below(40) - 20, rng.below(40) - 20, 40 + rng.below(160), 40 + rng.below(160));
        let n = rng.below(9);
        let taken: Vec<(i32, i32, i32, i32)> = (0..n).map(|_| (view.0 - 20 + rng.below(view.2 + 40), view.1 - 20 + rng.below(view.3 + 40), 4 + rng.below(70), 4 + rng.below(70))).collect();
        let size = (4 + rng.below(80), 4 + rng.below(80));
        let rects: Vec<Rect> = taken.iter().map(|t| r(t.0 as f32, t.1 as f32, t.2 as f32, t.3 as f32)).collect();
        let got = free_spot(&rects, r(view.0 as f32, view.1 as f32, view.2 as f32, view.3 as f32), Size::new(size.0 as f32, size.1 as f32), gap as f32);

        // The reference: every whole-pixel place in reading order, with whole-pixel arithmetic.
        let (x0, y0) = (view.0 + gap, view.1 + gap);
        let fits_x = size.0 + 2 * gap <= view.2;
        let fits_y = size.1 + 2 * gap <= view.3;
        let max_right = taken.iter().map(|t| t.0 + t.2).max().unwrap_or(0).max(view.0 + view.2);
        let max_bottom = taken.iter().map(|t| t.1 + t.3).max().unwrap_or(0).max(view.1 + view.3);
        let mut want = None;
        'scan: for y in y0..=max_bottom + gap + 1 {
            for x in x0..=max_right + gap + 1 {
                let in_view = (!fits_x || x + size.0 + gap <= view.0 + view.2) && (!fits_y || y + size.1 + gap <= view.1 + view.3);
                let clear = taken.iter().all(|t| !(x - gap < t.0 + t.2 && x + size.0 + gap > t.0 && y - gap < t.1 + t.3 && y + size.1 + gap > t.1));
                if in_view && clear {
                    want = Some((x, y));
                    break 'scan;
                }
            }
        }
        match (want, got) {
            (Some((x, y)), Spot::InView(p)) => {
                inside += 1;
                assert_eq!((p.x, p.y), (x as f32, y as f32), "scene {scene}: {taken:?} in {view:?} for {size:?}, gap {gap}");
            }
            (None, Spot::Below(p)) => {
                below += 1;
                let bottom = taken.iter().map(|t| t.1 + t.3).max().unwrap_or(0);
                assert_eq!(p.x, x0 as f32, "scene {scene}");
                assert_eq!(p.y, (bottom + gap).max(y0) as f32, "scene {scene}");
            }
            (want, got) => panic!("scene {scene}: wanted {want:?}, got {got:?}: {taken:?} in {view:?} for {size:?}, gap {gap}"),
        }
    }
    assert!(inside > 100 && below > 5, "the scenes cover both: {inside} in view, {below} below");
}

#[test]
fn a_size_that_is_no_number_does_not_panic() {
    for size in [Size::new(f32::NAN, 10.0), Size::new(10.0, f32::INFINITY), Size::new(f32::NEG_INFINITY, f32::NAN)] {
        let _ = free_spot(&[r(0.0, 0.0, 10.0, 10.0)], VIEW, size, GAP);
    }
    let _ = free_spot(&[r(f32::NAN, 0.0, 10.0, 10.0)], VIEW, Size::new(10.0, 10.0), GAP);
    let _ = free_spot(&[], r(f32::NAN, 0.0, f32::INFINITY, 10.0), Size::new(10.0, 10.0), GAP);
}
