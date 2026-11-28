//! The detents' numbers and rules: the heights of the four, where a drag settles, what the keys and a tap do - the part a bottom
//! sheet and the Preview's bottom pane share.

use super::*;

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.001
}

const ALL: [SheetDetent; 4] = [SheetDetent::Collapsed, SheetDetent::Peek, SheetDetent::Half, SheetDetent::Full];

// ---- the heights ----

#[test]
fn the_defaults_are_the_wireframes_30_130_half_and_85_percent() {
    let h = Heights::new(0.0, 0.0, 0.0, 0.0);
    assert!(near(h.collapsed, 30.0) && near(h.peek, 130.0));
    assert!(near(h.half, 400.0) && near(h.full, 680.0), "an unnamed container is 800 px: {h:?}");
}

#[test]
fn a_top_margin_the_app_names_replaces_the_85_percent() {
    let h = Heights::new(914.0, 0.0, 0.0, 120.0);
    assert!(near(h.full, 794.0), "{h:?}");
    assert!(near(Heights::new(914.0, 0.0, 0.0, 0.0).full, 914.0 * 0.85));
    assert!(near(Heights::new(914.0, 0.0, 0.0, f32::NAN).full, 914.0 * 0.85), "a margin that is no number names none");
}

#[test]
fn the_four_heights_are_in_order_whatever_the_numbers() {
    let numbers = [0.0, -5.0, 1.0, 30.0, 130.0, 500.0, 10_000.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY];
    for container in numbers {
        for collapsed in numbers {
            for peek in numbers {
                for margin in numbers {
                    let h = Heights::new(container, collapsed, peek, margin);
                    assert!(
                        h.collapsed <= h.peek && h.peek <= h.half && h.half <= h.full,
                        "{container} {collapsed} {peek} {margin}: {h:?}"
                    );
                    assert!(h.collapsed > 0.0 && h.full.is_finite());
                    assert!(h.full <= length(container, CONTAINER_HEIGHT_DEFAULT).max(h.peek));
                }
            }
        }
    }
}

#[test]
fn a_container_shorter_than_the_peek_makes_every_detent_the_container() {
    let h = Heights::new(100.0, 30.0, 130.0, 56.0);
    assert!(near(h.peek, 100.0) && near(h.half, 100.0) && near(h.full, 100.0));
    assert!(near(h.collapsed, 30.0));
}

#[test]
fn a_bad_number_is_the_default_and_never_an_error() {
    let ok = Heights::new(800.0, 30.0, 130.0, 0.0);
    assert_eq!(Heights::new(f32::NAN, f32::NAN, f32::NAN, f32::NAN), ok);
    assert_eq!(Heights::new(-1.0, -1.0, -1.0, -1.0), ok);
}

#[test]
fn every_detent_has_a_higher_and_a_lower_one_and_the_ends_stay() {
    assert_eq!(SheetDetent::Full.higher(), SheetDetent::Full);
    assert_eq!(SheetDetent::Collapsed.lower(), SheetDetent::Collapsed);
    for d in ALL {
        assert!(d.higher() >= d && d.lower() <= d);
    }
    assert_eq!(SheetDetent::Collapsed.higher().lower(), SheetDetent::Collapsed);
}

// ---- where a drag settles ----

#[test]
fn a_pane_let_go_settles_on_the_nearest_detent_in_range() {
    let h = Heights::new(800.0, 30.0, 130.0, 0.0);
    let (lo, hi) = (SheetDetent::Collapsed, SheetDetent::Full);
    assert_eq!(h.nearest(10.0, lo, hi), SheetDetent::Collapsed);
    assert_eq!(h.nearest(100.0, lo, hi), SheetDetent::Peek);
    assert_eq!(h.nearest(240.0, lo, hi), SheetDetent::Peek);
    assert_eq!(h.nearest(300.0, lo, hi), SheetDetent::Half);
    assert_eq!(h.nearest(500.0, lo, hi), SheetDetent::Half);
    assert_eq!(h.nearest(600.0, lo, hi), SheetDetent::Full, "nearer to the 680 px of the full height than to the 400 of the half");
    // a detent the app ruled out is never the answer
    assert_eq!(h.nearest(10.0, SheetDetent::Peek, hi), SheetDetent::Peek);
    assert_eq!(h.nearest(790.0, lo, SheetDetent::Half), SheetDetent::Half);
    // as near to two: the higher one
    assert_eq!(h.nearest(265.0, lo, hi), SheetDetent::Half);
    // a range named the wrong way round is the same range
    assert_eq!(h.nearest(790.0, hi, lo), SheetDetent::Full);
}

#[test]
fn a_fling_carries_the_pane_on_to_the_detent_it_was_heading_for() {
    let h = Heights::new(800.0, 30.0, 130.0, 0.0);
    let (lo, hi) = (SheetDetent::Collapsed, SheetDetent::Full);
    assert_eq!(h.settle(160.0, 0.0, lo, hi), SheetDetent::Peek);
    assert_eq!(h.settle(160.0, 3.0, lo, hi), SheetDetent::Peek, "a slow drag is no fling");
    assert!(h.settle(160.0, 20.0, lo, hi) >= SheetDetent::Half);
    assert_eq!(h.settle(380.0, 0.0, lo, hi), SheetDetent::Half);
    assert!(h.settle(380.0, -20.0, lo, hi) <= SheetDetent::Peek);
    assert_eq!(h.settle(380.0, f32::NAN, lo, hi), SheetDetent::Half, "a number that is not one is no fling");
}

#[test]
fn a_pane_that_stays_where_it_was_left_snaps_only_within_the_snap_distance() {
    let h = Heights::new(800.0, 30.0, 130.0, 0.0);
    let (lo, hi) = (SheetDetent::Collapsed, SheetDetent::Full);
    assert!(near(h.settle_free(150.0, 0.0, 28.0, lo, hi), 130.0), "20 px above the peek: the peek");
    assert!(near(h.settle_free(250.0, 0.0, 28.0, lo, hi), 250.0), "between two detents: where it was left");
    assert!(near(h.settle_free(250.0, 0.0, 0.0, lo, hi), 250.0), "no snap distance: never a snap");
    assert!(near(h.settle_free(380.0, 0.0, 28.0, lo, hi), 400.0), "20 px under the half: the half");
    // held in the range, and a fling that reaches the snap distance of a detent snaps to it
    assert!(near(h.settle_free(900.0, 0.0, 28.0, lo, hi), 680.0), "above the highest: the highest");
    assert!(near(h.settle_free(f32::NAN, 0.0, 28.0, lo, hi), 30.0), "no height: the lowest");
    // a 12 px step carries it on by 12 x 12 = 144 px: it would land at 394, within 28 px of the half; a 20 px step lands at 490, too far
    // from any detent to snap, so the pane stays where the pointer left it
    assert!(near(h.settle_free(250.0, 12.0, 28.0, lo, hi), 400.0), "flung up from 250: it lands within 28 px of the half");
    assert!(near(h.settle_free(250.0, 20.0, 28.0, lo, hi), 250.0), "a fling that lands far from every detent leaves the pane where it is");
}

#[test]
fn the_pointer_going_up_grows_the_pane_by_the_same_distance_held_to_the_range() {
    let h = Heights::new(800.0, 30.0, 130.0, 56.0);
    let (lo, hi) = (SheetDetent::Peek, SheetDetent::Full);
    assert!(near(h.dragged(130.0, 700.0, 600.0, lo, hi), 230.0));
    assert!(near(h.dragged(130.0, 700.0, 800.0, lo, hi), 130.0), "no lower than the peek");
    assert!(near(h.dragged(130.0, 700.0, 0.0, lo, hi), 744.0), "no higher than the full height");
    assert!(near(h.dragged(300.0, 400.0, f32::NAN, lo, hi), 300.0), "a pointer that is no number keeps the height");
    assert!(near(h.dragged(130.0, 700.0, 0.0, SheetDetent::Full, SheetDetent::Peek), 744.0), "a range the wrong way round");
}

// ---- the keys and the tap ----

#[test]
fn the_keys_on_the_grip_step_the_detents() {
    let (lo, hi) = (SheetDetent::Collapsed, SheetDetent::Full);
    assert_eq!(detent_after_key(VirtualKeyCode::Up, SheetDetent::Peek, lo, hi), Some(SheetDetent::Half));
    assert_eq!(detent_after_key(VirtualKeyCode::Down, SheetDetent::Peek, lo, hi), Some(SheetDetent::Collapsed));
    assert_eq!(detent_after_key(VirtualKeyCode::PageUp, SheetDetent::Half, lo, hi), Some(SheetDetent::Full));
    assert_eq!(detent_after_key(VirtualKeyCode::Home, SheetDetent::Peek, lo, hi), Some(SheetDetent::Full));
    assert_eq!(detent_after_key(VirtualKeyCode::End, SheetDetent::Half, lo, hi), Some(SheetDetent::Collapsed));
    assert_eq!(detent_after_key(VirtualKeyCode::Left, SheetDetent::Half, lo, hi), None, "a key the grip does not know");
    assert_eq!(detent_after_key(VirtualKeyCode::A, SheetDetent::Half, lo, hi), None);
}

#[test]
fn the_keys_stay_inside_the_range() {
    let (lo, hi) = (SheetDetent::Peek, SheetDetent::Half);
    assert_eq!(detent_after_key(VirtualKeyCode::Up, SheetDetent::Half, lo, hi), Some(SheetDetent::Half));
    assert_eq!(detent_after_key(VirtualKeyCode::Down, SheetDetent::Peek, lo, hi), Some(SheetDetent::Peek));
    assert_eq!(detent_after_key(VirtualKeyCode::Home, SheetDetent::Peek, lo, hi), Some(SheetDetent::Half));
    assert_eq!(detent_after_key(VirtualKeyCode::End, SheetDetent::Half, lo, hi), Some(SheetDetent::Peek));
    assert_eq!(detent_after_key(VirtualKeyCode::Home, SheetDetent::Peek, hi, lo), Some(SheetDetent::Half), "a range the wrong way round");
}

#[test]
fn a_tap_or_enter_steps_up_and_from_the_highest_back_to_the_lowest() {
    let (lo, hi) = (SheetDetent::Peek, SheetDetent::Full);
    assert_eq!(step_round(SheetDetent::Peek, lo, hi), SheetDetent::Half);
    assert_eq!(step_round(SheetDetent::Half, lo, hi), SheetDetent::Full);
    assert_eq!(step_round(SheetDetent::Full, lo, hi), SheetDetent::Peek);
    for key in [VirtualKeyCode::Return, VirtualKeyCode::NumpadEnter, VirtualKeyCode::Space] {
        assert_eq!(detent_after_key(key, SheetDetent::Half, lo, hi), Some(SheetDetent::Full), "{key:?}");
    }
    assert_eq!(step_round(SheetDetent::Peek, hi, lo), SheetDetent::Half, "a range the wrong way round");
}

// ---- a tap on a splitter's handle: peek <-> half ----

#[test]
fn a_tap_on_a_handle_goes_from_the_peek_to_the_half_and_back() {
    let h = Heights::new(800.0, 30.0, 130.0, 0.0);
    assert!(near(h.peek, 130.0) && near(h.half, 400.0));
    // below the peek (collapsed, or a drag that stopped short of it): up to the peek
    assert!(near(h.toggle_peek_half(30.0, 28.0), 130.0));
    assert!(near(h.toggle_peek_half(100.0, 28.0), 130.0));
    // at the peek (within the snap of it) or between the peek and the half: open to the half
    assert!(near(h.toggle_peek_half(102.0, 28.0), 400.0));
    assert!(near(h.toggle_peek_half(130.0, 28.0), 400.0));
    assert!(near(h.toggle_peek_half(250.0, 28.0), 400.0));
    assert!(near(h.toggle_peek_half(371.0, 28.0), 400.0));
    // at the half (within the snap of it) or above it, the full detent included: back to the peek
    assert!(near(h.toggle_peek_half(372.0, 28.0), 130.0));
    assert!(near(h.toggle_peek_half(400.0, 28.0), 130.0));
    assert!(near(h.toggle_peek_half(680.0, 28.0), 130.0));
}

#[test]
fn a_tap_on_a_handle_with_no_snap_is_exact_and_a_bad_number_is_the_peek() {
    let h = Heights::new(800.0, 30.0, 130.0, 0.0);
    assert!(near(h.toggle_peek_half(129.0, 0.0), 130.0));
    assert!(near(h.toggle_peek_half(130.0, 0.0), 400.0));
    assert!(near(h.toggle_peek_half(399.0, 0.0), 400.0));
    assert!(near(h.toggle_peek_half(400.0, 0.0), 130.0));
    // a snap that is no positive number is none
    assert!(near(h.toggle_peek_half(129.0, f32::NAN), 130.0));
    assert!(near(h.toggle_peek_half(130.0, -5.0), 400.0));
    assert!(near(h.toggle_peek_half(f32::NAN, 28.0), 130.0));
}
