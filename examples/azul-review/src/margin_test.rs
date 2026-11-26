//! Where the conversations of a page stand in its margin: each at the height of its anchor where it can, pushed down
//! (or up from the bottom) so that no two overlap, and the ones that cannot fit named. Pure numbers.

use super::*;

const H: f32 = 900.0;
const CHIP: f32 = 28.0;
const GAP: f32 = 6.0;

fn slot(id: u64, want_y: f32) -> Slot {
    Slot { id, want_y, keep: 0 }
}

fn tops(p: &Placement) -> Vec<(u64, f32)> {
    p.tops.clone()
}

/// Whether no two of the placed chips overlap and every one is inside the margin.
fn is_sound(p: &Placement, height: f32) -> bool {
    let mut sorted = p.tops.clone();
    sorted.sort_by(|a, b| a.1.total_cmp(&b.1));
    sorted.iter().all(|(_, t)| *t >= -0.001 && *t + CHIP <= height + 0.001)
        && sorted.windows(2).all(|w| w[1].1 - w[0].1 >= CHIP + GAP - 0.001)
}

#[test]
fn a_margin_with_no_conversation_has_nothing_in_it() {
    let p = place(&[], H, CHIP, GAP);
    assert!(p.tops.is_empty() && p.hidden.is_empty());
}

#[test]
fn a_conversation_stands_at_the_height_of_its_anchor() {
    assert_eq!(tops(&place(&[slot(1, 120.0)], H, CHIP, GAP)), vec![(1, 120.0)]);
}

#[test]
fn an_anchor_outside_the_margin_is_brought_inside() {
    assert_eq!(tops(&place(&[slot(1, -40.0)], H, CHIP, GAP)), vec![(1, 0.0)]);
    assert_eq!(tops(&place(&[slot(1, 5_000.0)], H, CHIP, GAP)), vec![(1, H - CHIP)]);
    assert_eq!(tops(&place(&[slot(1, f32::NAN)], H, CHIP, GAP)), vec![(1, 0.0)], "an anchor that is no number is the top");
}

#[test]
fn two_that_collide_keep_their_order_and_the_later_one_is_pushed_down() {
    let p = place(&[slot(2, 110.0), slot(1, 100.0)], H, CHIP, GAP);
    assert_eq!(tops(&p), vec![(1, 100.0), (2, 100.0 + CHIP + GAP)]);
    assert!(is_sound(&p, H));
}

#[test]
fn the_order_is_the_height_of_the_anchor_and_then_the_age() {
    let p = place(&[slot(5, 300.0), slot(3, 300.0), slot(4, 50.0)], H, CHIP, GAP);
    let ids: Vec<u64> = p.tops.iter().map(|t| t.0).collect();
    assert_eq!(ids, vec![4, 3, 5]);
}

#[test]
fn a_stack_that_would_run_off_the_bottom_is_pushed_up_together() {
    let slots: Vec<Slot> = (1..=4).map(|i| slot(i, H - 10.0)).collect();
    let p = place(&slots, H, CHIP, GAP);
    assert!(is_sound(&p, H), "{p:?}");
    let last = p.tops.last().expect("four are placed");
    assert!((last.1 - (H - CHIP)).abs() < 0.001, "the last one rests on the bottom: {p:?}");
    assert_eq!(p.tops.iter().map(|t| t.0).collect::<Vec<_>>(), vec![1, 2, 3, 4], "in the same order");
}

#[test]
fn what_cannot_fit_is_named_and_what_is_kept_does_not_overlap() {
    // fifty conversations in a margin that holds 26
    let slots: Vec<Slot> = (1..=50).map(|i| slot(i, i as f32 * 3.0)).collect();
    let p = place(&slots, H, CHIP, GAP);
    let capacity = capacity(H, CHIP, GAP);
    assert_eq!(capacity, 26);
    assert_eq!(p.tops.len(), capacity);
    assert_eq!(p.hidden.len(), 50 - capacity);
    assert!(is_sound(&p, H), "{p:?}");
    let mut all: Vec<u64> = p.tops.iter().map(|t| t.0).chain(p.hidden.iter().copied()).collect();
    all.sort_unstable();
    assert_eq!(all, (1..=50).collect::<Vec<u64>>(), "every conversation is either placed or named");
}

#[test]
fn the_ones_that_matter_stay_when_the_margin_is_full() {
    // ids 1 to 30; the odd ones are working (keep 2), the rest are done (keep 0)
    let slots: Vec<Slot> = (1..=30u64).map(|i| Slot { id: i, want_y: i as f32 * 10.0, keep: if i % 2 == 1 { 2 } else { 0 } }).collect();
    let p = place(&slots, H, CHIP, GAP);
    assert_eq!(p.tops.len(), 26);
    for odd in (1..=30u64).filter(|i| i % 2 == 1) {
        assert!(p.tops.iter().any(|t| t.0 == odd), "the working conversation {odd} is placed: {p:?}");
    }
    assert_eq!(p.hidden.len(), 4);
    assert!(p.hidden.iter().all(|h| h % 2 == 0), "only done ones are left out: {:?}", p.hidden);
    // of the done ones the later ones go
    assert_eq!(p.hidden, vec![24, 26, 28, 30]);
}

#[test]
fn a_margin_too_small_for_one_chip_places_none_and_names_all() {
    let p = place(&[slot(1, 0.0), slot(2, 10.0)], 20.0, CHIP, GAP);
    assert!(p.tops.is_empty());
    assert_eq!(p.hidden, vec![1, 2]);
}

#[test]
fn the_capacity_is_how_many_chips_and_gaps_fit() {
    assert_eq!(capacity(900.0, 28.0, 6.0), 26);
    assert_eq!(capacity(28.0, 28.0, 6.0), 1);
    assert_eq!(capacity(27.9, 28.0, 6.0), 0);
    assert_eq!(capacity(0.0, 28.0, 6.0), 0);
    assert_eq!(capacity(-5.0, 28.0, 6.0), 0);
    assert_eq!(capacity(100.0, 0.0, 0.0), 0, "a chip with no height is no measure");
}

#[test]
fn the_popover_stands_at_its_chip_and_stays_inside_the_page() {
    assert_eq!(popover_top(100.0, 300.0, 900.0), 100.0);
    assert_eq!(popover_top(800.0, 300.0, 900.0), 600.0, "pushed up so that its foot is on the page");
    assert_eq!(popover_top(-20.0, 300.0, 900.0), 0.0);
    assert_eq!(popover_top(10.0, 1_200.0, 900.0), 0.0, "taller than the page: at the top");
}

#[test]
fn the_popover_stands_at_the_right_edge_of_the_page_where_all_of_the_page_is_in_view() {
    assert_eq!(popover_left(1_000.0, 304.0, 1_000.0), 690.0, "the page's width less the popover's and a little");
    assert_eq!(popover_left(5_000.0, 304.0, 1_000.0), 690.0, "a window wider than the page changes nothing");
}

#[test]
fn the_popover_moves_left_to_stay_in_view_when_the_window_shows_less_than_the_page() {
    // a window that shows 915 px of the page: the popover's right edge is a little inside it
    assert_eq!(popover_left(915.0, 304.0, 1_000.0), 915.0 - 304.0 - 6.0);
    // a page scrolled so that little of it is in view: never off the page's left edge
    assert_eq!(popover_left(200.0, 304.0, 1_000.0), 8.0);
    assert_eq!(popover_left(-50.0, 304.0, 1_000.0), 8.0);
}
