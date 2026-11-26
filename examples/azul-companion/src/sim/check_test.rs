//! The layout check against boxes made by hand: a good screen passes, and each fault is found where it is and no
//! more than once.

use super::*;
use crate::env::{Lang, DEVICES};

fn env() -> Env {
    DEVICES[0].env(false, Lang::En, true)
}

fn zone(x: f32, y: f32, w: f32, h: f32) -> Zone {
    Zone { x, y, w, h }
}

fn node(index: usize, parent: Option<usize>, classes: &[&str], rect: Zone) -> NodeBox {
    NodeBox { index, parent, tag: "div".into(), id: String::new(), classes: classes.iter().map(|c| (*c).to_string()).collect(), text: String::new(), rect }
}

fn text_node(index: usize, parent: Option<usize>, classes: &[&str], rect: Zone, words: &str) -> NodeBox {
    NodeBox { text: words.into(), ..node(index, parent, classes, rect) }
}

/// A phone's worth of good boxes: the body, a header with a key, the pad, four quick buttons.
fn good() -> Vec<NodeBox> {
    let mut nodes = vec![
        node(0, None, &[], zone(0.0, 0.0, 393.0, 852.0)),
        node(1, Some(0), &["az-target"], zone(330.0, 59.0, 44.0, 44.0)),
        text_node(2, Some(0), &["az-text"], zone(16.0, 70.0, 120.0, 20.0), "Studio Mac"),
    ];
    let mut pad = node(3, Some(0), &["az-surface"], zone(16.0, 160.0, 361.0, 389.0));
    pad.id = "pad".into();
    nodes.push(pad);
    let places = [(16.0, 640.0), (203.0, 640.0), (16.0, 710.0), (203.0, 710.0)];
    for (i, (x, y)) in places.into_iter().enumerate() {
        nodes.push(node(4 + i, Some(0), &["az-target", "az-quick"], zone(x, y, 174.0, 60.0)));
    }
    for (i, (x, y)) in places.into_iter().enumerate() {
        nodes.push(text_node(8 + i, Some(4 + i), &["az-text"], zone(x + 20.0, y + 10.0, 80.0, 20.0), "Tone"));
    }
    nodes
}

fn rules(violations: &[Violation]) -> Vec<&'static str> {
    violations.iter().map(|v| v.rule).collect()
}

#[test]
fn a_good_screen_has_no_faults() {
    assert_eq!(check(&good(), &env()), []);
}

#[test]
fn a_key_smaller_than_a_finger_is_found() {
    let mut nodes = good();
    nodes[1].rect = zone(330.0, 59.0, 30.0, 44.0);
    let found = check(&nodes, &env());
    assert_eq!(rules(&found), ["target-size"]);
    assert_eq!(found[0].node, 1);
    assert!(found[0].detail.contains("30"), "{:?}", found[0]);
    // 44 by 44 is the least, and half a pixel of rounding does not count
    nodes[1].rect = zone(330.0, 59.0, 43.6, 44.0);
    assert!(check(&nodes, &env()).is_empty());
}

#[test]
fn two_keys_that_overlap_are_found_once_and_keys_that_touch_are_not() {
    let mut nodes = good();
    nodes[5].rect = zone(180.0, 640.0, 174.0, 60.0);
    let found = check(&nodes, &env());
    assert_eq!(rules(&found), ["target-overlap"], "{found:?}");
    nodes[5].rect = zone(190.0, 640.0, 174.0, 60.0);
    assert!(!rules(&check(&nodes, &env())).contains(&"target-overlap"), "16 + 174 = 190 is where the next starts");
}

#[test]
fn a_key_in_the_notch_or_the_home_indicator_is_found() {
    let mut nodes = good();
    nodes[1].rect = zone(330.0, 20.0, 44.0, 44.0);
    assert_eq!(rules(&check(&nodes, &env())), ["target-outside-safe-area"]);
    let mut nodes = good();
    nodes[4].rect = zone(16.0, 800.0, 174.0, 60.0);
    assert!(rules(&check(&nodes, &env())).contains(&"target-outside-safe-area"));
}

#[test]
fn the_rows_of_a_list_may_run_past_the_screen_because_the_list_scrolls() {
    let mut nodes = good();
    nodes.push(node(20, Some(0), &["az-target", "az-row"], zone(0.0, 1500.0, 393.0, 56.0)));
    assert!(check(&nodes, &env()).is_empty());
}

#[test]
fn the_part_of_a_row_that_a_list_cuts_off_does_not_overlap_the_keys_below_it() {
    let mut nodes = good();
    nodes.push(node(20, Some(0), &["az-scroll"], zone(0.0, 100.0, 393.0, 300.0)));
    // a row that starts in the list and runs on over the key at (16, 640): the list cuts it off at 400
    nodes.push(node(21, Some(20), &["az-target", "az-row"], zone(0.0, 380.0, 393.0, 300.0)));
    assert!(check(&nodes, &env()).is_empty(), "{:?}", check(&nodes, &env()));
}

#[test]
fn a_key_inside_a_scroll_container_may_be_scrolled_out_of_the_safe_area() {
    let mut nodes = good();
    nodes.push(node(20, Some(0), &["az-scroll"], zone(0.0, 100.0, 393.0, 300.0)));
    nodes.push(node(21, Some(20), &["az-target"], zone(16.0, 1100.0, 186.0, 48.0)));
    assert!(check(&nodes, &env()).is_empty());
}

/// A page made to fit: a box with the class `az-fit` from the top of the safe area to its bottom, a key inside it.
fn fitting_page(key: Zone) -> Vec<NodeBox> {
    let mut nodes = good();
    nodes.push(node(20, Some(0), &["az-scroll", "az-fit"], zone(0.0, 59.0, 393.0, 759.0)));
    nodes.push(node(21, Some(20), &["az-target"], key));
    nodes
}

#[test]
fn a_page_made_to_fit_that_would_scroll_is_found() {
    // the key ends at 818 = the bottom of the page: nothing beyond it
    let fits = fitting_page(zone(16.0, 700.0, 361.0, 52.0));
    assert!(!rules(&check(&fits, &env())).contains(&"page-overflow"), "{:?}", check(&fits, &env()));
    // 40 px past the page's end: it would scroll
    let overflows = fitting_page(zone(16.0, 806.0, 361.0, 52.0));
    let found = check(&overflows, &env());
    assert!(rules(&found).contains(&"page-overflow"), "{found:?}");
    assert_eq!(found.iter().find(|v| v.rule == "page-overflow").map(|v| v.node), Some(21));
    // a page that is not made to fit (no class) may scroll
    let mut plain = overflows.clone();
    plain[plain.len() - 2].classes = vec!["az-scroll".to_string()];
    assert!(!rules(&check(&plain, &env())).contains(&"page-overflow"));
}

#[test]
fn the_lowest_key_of_a_page_made_to_fit_keeps_48_px_clear_of_the_bar() {
    // the page ends at 818: a key that ends at 770 keeps exactly 48, one that ends at 780 keeps 38
    let clear = fitting_page(zone(16.0, 718.0, 361.0, 52.0));
    assert!(!rules(&check(&clear, &env())).contains(&"key-near-bar"), "{:?}", check(&clear, &env()));
    let near = fitting_page(zone(16.0, 728.0, 361.0, 52.0));
    let found = check(&near, &env());
    assert!(rules(&found).contains(&"key-near-bar"), "{found:?}");
    assert!(found.iter().find(|v| v.rule == "key-near-bar").is_some_and(|v| v.detail.contains("38")), "{found:?}");
}

#[test]
fn a_line_of_text_that_runs_out_of_its_box_is_found_and_one_that_is_cut_on_purpose_is_not() {
    let mut nodes = good();
    nodes[8].rect = zone(36.0, 650.0, 190.0, 20.0);
    let found = check(&nodes, &env());
    assert_eq!(rules(&found), ["text-overflow"], "{found:?}");
    assert_eq!(found[0].node, 8);
    assert!(found[0].detail.contains("Tone"), "{:?}", found[0]);
    nodes[8].classes = vec!["az-text".into(), "az-clip".into()];
    assert!(check(&nodes, &env()).is_empty(), "a name cut to its line on purpose");
}

#[test]
fn text_that_wraps_taller_than_its_box_is_found() {
    let mut nodes = good();
    nodes[8].rect = zone(36.0, 650.0, 80.0, 70.0);
    assert_eq!(rules(&check(&nodes, &env())), ["text-overflow"]);
}

#[test]
fn text_in_a_scroll_container_may_run_past_its_box() {
    let mut nodes = good();
    nodes.push(node(20, Some(0), &["az-scroll"], zone(0.0, 100.0, 393.0, 300.0)));
    nodes.push(text_node(21, Some(20), &["az-text"], zone(16.0, 380.0, 360.0, 400.0), "A long note"));
    assert!(check(&nodes, &env()).is_empty());
}

#[test]
fn text_in_the_status_bar_is_found_and_text_in_a_row_that_scrolls_away_is_not() {
    let mut nodes = good();
    nodes[2].rect = zone(16.0, 20.0, 120.0, 20.0);
    assert_eq!(rules(&check(&nodes, &env())), ["text-outside-safe-area"]);
    let mut nodes = good();
    nodes.push(node(20, Some(0), &["az-target", "az-row"], zone(0.0, 1500.0, 393.0, 56.0)));
    nodes.push(text_node(21, Some(20), &["az-text"], zone(40.0, 1520.0, 200.0, 20.0), "src"));
    assert!(check(&nodes, &env()).is_empty());
}

#[test]
fn the_pad_must_be_big_enough_to_use() {
    let mut nodes = good();
    nodes[3].rect = zone(16.0, 160.0, 150.0, 120.0);
    assert_eq!(rules(&check(&nodes, &env())), ["pad-size"]);
}

#[test]
fn a_remote_without_its_four_quick_buttons_is_found_and_so_is_a_button_too_small() {
    let mut nodes = good();
    nodes.retain(|n| n.index != 7 && n.index != 11);
    assert_eq!(rules(&check(&nodes, &env())), ["quick-buttons"]);
    let mut nodes = good();
    nodes[4].rect = zone(16.0, 640.0, 174.0, 46.0);
    assert_eq!(rules(&check(&nodes, &env())), ["quick-buttons"]);
    let none: Vec<NodeBox> = good().into_iter().filter(|n| !n.has("az-quick") && n.id != "pad" && n.parent != Some(4) && n.parent != Some(5) && n.parent != Some(6) && n.parent != Some(7)).collect();
    assert!(check(&none, &env()).is_empty(), "a screen with no pad has no quick buttons to ask for");
}

#[test]
fn the_violations_are_told_in_lines_with_the_words_of_the_node() {
    let mut nodes = good();
    nodes[8].rect = zone(36.0, 650.0, 190.0, 20.0);
    let found = check(&nodes, &env());
    let lines = describe(&found, &nodes);
    assert_eq!(lines.len(), 1);
    assert!(lines[0].contains("text-overflow") && lines[0].contains("Tone"), "{}", lines[0]);
}

#[test]
fn a_big_screen_of_boxes_is_checked_in_a_blink() {
    let mut nodes = good();
    for i in 0..3000 {
        nodes.push(node(100 + i, Some(0), &["az-target", "az-row"], zone(0.0, 900.0 + i as f32 * 56.0, 393.0, 56.0)));
    }
    let started = std::time::Instant::now();
    let found = check(&nodes, &env());
    assert!(found.is_empty(), "{:?}", found.first());
    assert!(started.elapsed().as_millis() < 500, "{:?}", started.elapsed());
}
