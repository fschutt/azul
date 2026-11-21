//! Which window an app-level event (a tray click, a notification click, a
//! global hotkey) runs against: `azul_layout::managers::app_target`.
//!
//! The shells used to take whatever their window registry listed first - an
//! `NSWindow`-pointer-ordered map, a `HashMap`, an `HWND`-ordered map - so
//! the target depended on allocation addresses and hash seeds. The rule these
//! tests pin: the most recently focused window, else the oldest; menus and
//! tooltips only when nothing else is open; and never the input order.

use azul_layout::managers::app_target::{
    pick_app_target, AppTargetCandidate, WindowActivationOrder,
};

fn window(key: u32, order: WindowActivationOrder) -> AppTargetCandidate<u32> {
    AppTargetCandidate {
        key,
        order,
        transient: false,
    }
}

fn menu(key: u32, order: WindowActivationOrder) -> AppTargetCandidate<u32> {
    AppTargetCandidate {
        key,
        order,
        transient: true,
    }
}

/// Every permutation's answer must be the same: the registries iterate in
/// pointer / hash / handle order, which is exactly the arbitrariness this
/// rule replaces.
fn pick_in_every_order(candidates: &[AppTargetCandidate<u32>]) -> Option<u32> {
    let first = pick_app_target(candidates);
    let mut reversed = candidates.to_vec();
    reversed.reverse();
    assert_eq!(pick_app_target(&reversed), first, "the order of the slice decided");
    let mut rotated = candidates.to_vec();
    if !rotated.is_empty() {
        rotated.rotate_left(1);
    }
    assert_eq!(pick_app_target(&rotated), first, "the order of the slice decided");
    first
}

#[test]
fn no_window_means_no_target() {
    assert_eq!(pick_app_target::<u32>(&[]), None);
}

#[test]
fn with_no_focus_history_the_oldest_window_wins() {
    let oldest = WindowActivationOrder::for_new_window();
    let middle = WindowActivationOrder::for_new_window();
    let newest = WindowActivationOrder::for_new_window();
    assert!(oldest.created < middle.created && middle.created < newest.created);
    // Keys deliberately NOT in creation order: a registry keyed by pointer
    // would list 1 first.
    let candidates = [window(3, middle), window(1, newest), window(7, oldest)];
    assert_eq!(pick_in_every_order(&candidates), Some(7));
}

#[test]
fn the_most_recently_focused_window_wins_over_the_oldest() {
    let mut main = WindowActivationOrder::for_new_window();
    let mut inspector = WindowActivationOrder::for_new_window();
    let palette = WindowActivationOrder::for_new_window();

    main.note_focused();
    inspector.note_focused();
    let candidates = [window(10, main), window(20, inspector), window(30, palette)];
    assert_eq!(
        pick_in_every_order(&candidates),
        Some(20),
        "the inspector was focused last"
    );

    // The user goes back to the main window: it wins again, although the
    // inspector was focused later than the main window's FIRST focus.
    main.note_focused();
    let candidates = [window(10, main), window(20, inspector), window(30, palette)];
    assert_eq!(pick_in_every_order(&candidates), Some(10));
}

#[test]
fn a_focused_window_beats_one_that_never_had_the_focus_even_if_older() {
    let never_focused_but_oldest = WindowActivationOrder::for_new_window();
    let mut focused = WindowActivationOrder::for_new_window();
    focused.note_focused();
    let candidates = [window(1, never_focused_but_oldest), window(2, focused)];
    assert_eq!(pick_in_every_order(&candidates), Some(2));
}

#[test]
fn menus_and_tooltips_are_skipped_while_a_real_window_exists() {
    let main = WindowActivationOrder::for_new_window();
    let mut context_menu = WindowActivationOrder::for_new_window();
    // The menu holds the focus right now - it is still not where an app's
    // hotkey or tray click belongs.
    context_menu.note_focused();
    let candidates = [menu(99, context_menu), window(1, main)];
    assert_eq!(pick_in_every_order(&candidates), Some(1));
}

#[test]
fn a_transient_window_is_used_when_it_is_all_there_is() {
    let older = WindowActivationOrder::for_new_window();
    let newer = WindowActivationOrder::for_new_window();
    let candidates = [menu(5, newer), menu(4, older)];
    assert_eq!(pick_in_every_order(&candidates), Some(4));
}

#[test]
fn the_activation_clock_never_repeats_a_stamp() {
    let mut a = WindowActivationOrder::for_new_window();
    let b = WindowActivationOrder::for_new_window();
    assert!(!a.was_ever_focused(), "a new window has no focus stamp");
    a.note_focused();
    assert!(a.was_ever_focused());
    assert!(
        a.last_focused > b.created,
        "a later focus is newer than an earlier creation"
    );
}
