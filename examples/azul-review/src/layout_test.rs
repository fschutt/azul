//! The window's edges as numbers: where the file tree, the pages and the right panel sit, how wide each is while
//! it slides, what a drag of the tree's edge makes of its width, and what the remembered values may be.

use super::*;

fn panels() -> Panels {
    Panels::default()
}

// ---- the tree pushes, or covers ----

#[test]
fn below_the_overlay_width_the_tree_covers_the_pages_and_from_it_on_it_pushes_them() {
    assert!(is_overlay(OVERLAY_BELOW_W - 0.1));
    assert!(!is_overlay(OVERLAY_BELOW_W));
    assert!(!is_overlay(1280.0));
    assert!(is_overlay(0.0));
}

#[test]
fn the_tree_pushes_the_pages_to_the_right_and_that_is_said_in_one_place() {
    assert_eq!(tree_pushes_toward(), Side::Right);
}

#[test]
fn an_open_tree_makes_the_pages_narrower_by_its_width_and_a_covering_one_does_not() {
    let open = Panels { tree_open: true, right_open: false, ..panels() };
    let slide = Slide::settled(&open);
    let closed = Slide::settled(&Panels { tree_open: false, ..open });
    let wide = solve(1280.0, &open, &slide, 0.0);
    let none = solve(1280.0, &Panels { tree_open: false, ..open }, &closed, 0.0);
    assert!(!wide.overlay);
    assert_eq!(wide.tree_w, TREE_W_DEFAULT);
    assert_eq!(none.tree_w, 0.0);
    assert_eq!(none.pages_w - wide.pages_w, TREE_W_DEFAULT, "the pages give up what the tree takes");

    let narrow = solve(600.0, &open, &slide, 0.0);
    let narrow_closed = solve(600.0, &Panels { tree_open: false, ..open }, &closed, 0.0);
    assert!(narrow.overlay);
    assert_eq!(narrow.pages_w, narrow_closed.pages_w, "a covering tree leaves the pages their room");
    assert!(narrow.tree_w > 0.0);
}

#[test]
fn the_pages_keep_their_minimum_while_the_window_has_the_room() {
    let both = Panels { tree_open: true, right_open: true, tree_w: TREE_W_MAX, right_w: RIGHT_W_MAX };
    let slide = Slide::settled(&both);
    for w in [810.0, 900.0, 1000.0, 1280.0, 1920.0, 3000.0] {
        let s = solve(w, &both, &slide, 0.0);
        assert!(s.pages_w >= PAGES_MIN_W - 0.01, "window {w}: {s:?}");
    }
}

#[test]
fn the_right_panel_gives_way_before_the_tree_does() {
    // 1000 wide with both open at their wanted widths: the panel gives what the pages need before the tree is touched.
    let both = Panels { tree_open: true, right_open: true, tree_w: 300.0, right_w: 500.0 };
    let s = solve(1000.0, &both, &Slide::settled(&both), 0.0);
    assert_eq!(s.tree_w, 300.0, "{s:?}");
    assert!(s.right_w < 500.0, "{s:?}");
    assert!(s.right_w >= RIGHT_W_MIN - 0.01, "{s:?}");
}

// ---- widths ----

#[test]
fn a_tree_width_is_held_between_its_minimum_and_what_the_window_can_spare() {
    assert_eq!(clamp_tree_w(10.0, 1280.0), TREE_W_MIN);
    assert_eq!(clamp_tree_w(5000.0, 1280.0), TREE_W_MAX);
    assert_eq!(clamp_tree_w(300.0, 1280.0), 300.0);
    // a window that cannot spare the maximum: the pages keep their minimum beside the strip and the bar
    let spare = 900.0 - PAGES_MIN_W - STRIP_W - RIGHT_BAR_W;
    assert_eq!(clamp_tree_w(TREE_W_MAX, 900.0), spare);
}

#[test]
fn a_width_that_is_no_number_is_the_default_and_never_panics() {
    assert_eq!(clamp_tree_w(f32::NAN, 1280.0), TREE_W_DEFAULT);
    assert_eq!(clamp_tree_w(f32::INFINITY, 1280.0), TREE_W_MAX);
    assert_eq!(clamp_tree_w(f32::NEG_INFINITY, 1280.0), TREE_W_MIN);
    assert_eq!(clamp_right_w(f32::NAN, 1280.0, 0.0), RIGHT_W_DEFAULT);
    // a window of no size at all
    assert!(clamp_tree_w(300.0, 0.0) >= 0.0);
    assert!(clamp_tree_w(300.0, f32::NAN) >= 0.0);
}

#[test]
fn dragging_the_trees_edge_moves_its_width_by_the_pointer_and_holds_it_between_the_limits() {
    assert_eq!(dragged_tree_w(260.0, 260.0, 300.0, 1280.0), 300.0);
    assert_eq!(dragged_tree_w(260.0, 260.0, 220.0, 1280.0), 220.0);
    assert_eq!(dragged_tree_w(260.0, 260.0, -500.0, 1280.0), TREE_W_MIN);
    assert_eq!(dragged_tree_w(260.0, 260.0, 9000.0, 1280.0), TREE_W_MAX);
    // the pointer grabbed the edge off its centre: the width follows the pointer's movement, not its position
    assert_eq!(dragged_tree_w(260.0, 257.0, 307.0, 1280.0), 310.0);
}

#[test]
fn dragging_the_right_panels_edge_widens_it_toward_the_left() {
    // The panel's inner edge is its left one: the pointer moving left makes it wider.
    assert_eq!(dragged_right_w(360.0, 900.0, 860.0, 1280.0, 0.0), 400.0);
    assert_eq!(dragged_right_w(360.0, 900.0, 960.0, 1280.0, 0.0), 300.0);
    assert_eq!(dragged_right_w(360.0, 900.0, 0.0, 1280.0, 0.0), 1280.0 - STRIP_W - RIGHT_BAR_W - PAGES_MIN_W);
    assert_eq!(dragged_right_w(360.0, 900.0, 0.0, 3000.0, 0.0), RIGHT_W_MAX);
    assert_eq!(dragged_right_w(360.0, 900.0, 5000.0, 1280.0, 0.0), RIGHT_W_MIN);
}

// ---- the slide ----

#[test]
fn the_ease_starts_at_nothing_ends_at_everything_and_never_goes_back() {
    assert_eq!(ease_out(0.0), 0.0);
    assert_eq!(ease_out(1.0), 1.0);
    assert!(ease_out(0.5) > 0.5, "it starts fast");
    let mut last = 0.0;
    for i in 0..=100 {
        let e = ease_out(i as f32 / 100.0);
        assert!(e >= last, "step {i}");
        last = e;
    }
    assert_eq!(ease_out(-3.0), 0.0);
    assert_eq!(ease_out(7.0), 1.0);
    assert_eq!(ease_out(f32::NAN), 0.0);
}

#[test]
fn a_slide_takes_its_time_and_stops_at_its_end() {
    let open = Panels { tree_open: true, ..panels() };
    let mut slide = Slide { tree: 0.0, right: 0.0 };
    assert!(slide.moving(&open));
    slide = slide.step(&open, TREE_SLIDE_MS / 2.0);
    assert!(slide.tree > 0.4 && slide.tree < 0.6, "{slide:?}");
    slide = slide.step(&open, TREE_SLIDE_MS);
    assert_eq!(slide.tree, 1.0);
    assert!(!slide.moving(&Panels { right_open: false, ..open }));
}

#[test]
fn a_slide_back_takes_the_same_time_and_a_bad_step_changes_nothing() {
    let shut = Panels { tree_open: false, ..panels() };
    let slide = Slide { tree: 1.0, right: 0.0 };
    let half = slide.step(&shut, TREE_SLIDE_MS / 2.0);
    assert!(half.tree > 0.4 && half.tree < 0.6, "{half:?}");
    assert_eq!(slide.step(&shut, 0.0), slide);
    assert_eq!(slide.step(&shut, -5.0), slide);
    assert_eq!(slide.step(&shut, f32::NAN), slide);
    assert_eq!(slide.step(&shut, f32::INFINITY).tree, 0.0);
}

#[test]
fn the_tree_and_the_right_panel_slide_each_to_its_own_end() {
    let p = Panels { tree_open: true, right_open: false, ..panels() };
    let s = Slide { tree: 0.0, right: 1.0 }.step(&p, TREE_SLIDE_MS * 2.0);
    assert_eq!((s.tree, s.right), (1.0, 0.0));
}

// ---- the title row's left pad ----

#[test]
fn the_top_bar_keeps_clear_of_the_traffic_lights_and_the_switch_until_the_tree_is_wider_than_that() {
    let reserved = 120.0;
    assert_eq!(top_bar_left_pad(0.0, reserved), 120.0);
    assert_eq!(top_bar_left_pad(50.0, reserved), 70.0);
    assert_eq!(top_bar_left_pad(120.0, reserved), 0.0);
    assert_eq!(top_bar_left_pad(260.0, reserved), 0.0);
}

// ---- what is remembered ----

fn values<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
    move |key| pairs.iter().find(|(k, _)| *k == key).map(|(_, v)| (*v).to_string())
}

#[test]
fn nothing_remembered_is_the_default_and_says_nothing() {
    let (p, notes) = Panels::remembered(&values(&[]));
    assert_eq!(p, Panels::default());
    assert!(notes.is_empty(), "{notes:?}");
}

#[test]
fn what_was_remembered_comes_back() {
    let (p, notes) = Panels::remembered(&values(&[
        ("tree-open", "0"),
        ("tree-width", "310"),
        ("right-open", "1"),
        ("right-width", "420"),
    ]));
    assert_eq!(p, Panels { tree_open: false, tree_w: 310.0, right_open: true, right_w: 420.0 });
    assert!(notes.is_empty(), "{notes:?}");
    // and what is stored reads back as the same
    let stored = p.to_values();
    let get = |key: &str| stored.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone());
    assert_eq!(Panels::remembered(&get).0, p);
}

#[test]
fn a_remembered_width_out_of_range_is_held_in_it_with_a_word_and_a_word_that_is_no_number_is_the_default() {
    let (p, notes) = Panels::remembered(&values(&[("tree-width", "99999"), ("right-width", "ten"), ("tree-open", "maybe")]));
    assert_eq!(p.tree_w, TREE_W_MAX);
    assert_eq!(p.right_w, RIGHT_W_DEFAULT);
    assert_eq!(p.tree_open, Panels::default().tree_open);
    assert_eq!(notes.len(), 3, "{notes:?}");
    assert!(notes.iter().all(|n| !n.is_empty()));
}

#[test]
fn the_tree_starts_closed_where_it_would_cover_the_pages_unless_it_was_remembered() {
    assert!(initial_tree_open(None, 1280.0));
    assert!(!initial_tree_open(None, 600.0), "a covering tree is not the first thing one sees");
    assert!(initial_tree_open(Some(true), 600.0), "but what the person chose stays");
    assert!(!initial_tree_open(Some(false), 1280.0));
}

// ---- a drag of an edge ----

#[test]
fn a_grip_on_the_trees_edge_gives_the_width_the_pointer_makes_of_it() {
    let grip = Grip { panel: Panel::Tree, start_w: 260.0, grab_x: 260.0 };
    assert_eq!(grip.width_at(310.0, 1280.0, 260.0), 310.0);
    assert_eq!(grip.width_at(-50.0, 1280.0, 260.0), TREE_W_MIN);
}

#[test]
fn a_grip_on_the_right_panels_edge_gives_the_width_the_pointer_makes_of_it() {
    let grip = Grip { panel: Panel::Right, start_w: 360.0, grab_x: 900.0 };
    assert_eq!(grip.width_at(850.0, 1280.0, 0.0), 410.0);
    // beside an open tree the panel has less room
    assert_eq!(grip.width_at(0.0, 1280.0, 300.0), 1280.0 - 300.0 - STRIP_W - RIGHT_BAR_W - PAGES_MIN_W);
}
