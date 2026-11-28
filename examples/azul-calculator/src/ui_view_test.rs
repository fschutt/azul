//! Which view a window shows and which mode tabs it offers, by its size. No window.

use super::*;

#[test]
fn a_phone_on_its_side_stays_on_the_standard_calculator() {
    // iPhone 16 Pro and Pixel 8 in landscape: wide, but far too short for the programmer's panel.
    assert_eq!(view_for_size(874.0, 402.0), View::Micro);
    assert_eq!(view_for_size(915.0, 412.0), View::Micro);
    // A window wide and tall enough is the programmer one; one pixel short of the height is not.
    assert_eq!(view_for_size(PROGRAMMER_MIN_WIDTH, PROGRAMMER_MIN_HEIGHT), View::Programmer);
    assert_eq!(view_for_size(PROGRAMMER_MIN_WIDTH, PROGRAMMER_MIN_HEIGHT - 1.0), View::Micro);
    assert_eq!(view_for_size(PROGRAMMER_MIN_WIDTH - 1.0, 900.0), View::Micro);
}

#[test]
fn the_programmer_and_graphing_tabs_are_only_for_a_window_that_can_hold_them() {
    // A phone (portrait or on its side) and the default window: the standard calculator, the
    // dates and the units.
    let small = [Pin::Standard, Pin::Date, Pin::Convert];
    assert_eq!(tab_pins(412.0, 915.0), small);
    assert_eq!(tab_pins(915.0, 412.0), small, "a phone on its side is too short for the programmer's panel");
    assert_eq!(tab_pins(340.0, 560.0), small);
    // Wide and tall enough for the panel beside the keys: the programmer one too.
    let medium = [Pin::Standard, Pin::Programmer, Pin::Date, Pin::Convert];
    assert_eq!(tab_pins(PROGRAMMER_MIN_WIDTH, PROGRAMMER_MIN_HEIGHT), medium);
    assert_eq!(tab_pins(1024.0, 768.0), medium);
    // And for the graph: the graphing one as well.
    let large = [Pin::Standard, Pin::Programmer, Pin::Graphing, Pin::Date, Pin::Convert];
    assert_eq!(tab_pins(GRAPH_MIN_WIDTH, GRAPH_MIN_HEIGHT), large);
    assert_eq!(tab_pins(GRAPH_MIN_WIDTH, GRAPH_MIN_HEIGHT - 1.0), medium, "tall enough is part of it");
    assert!(!tab_pins(340.0, 560.0).contains(&Pin::Auto), "Auto is no tab: it is the window's size");
}

#[test]
fn the_tabs_follow_the_views_the_size_picks() {
    // Whatever view the size alone shows has its tab (the active one is found by its view).
    for (w, h) in [(340.0, 560.0), (412.0, 915.0), (915.0, 412.0), (720.0, 560.0), (1024.0, 768.0), (1400.0, 900.0)] {
        let shown = view_for_size(w, h);
        assert!(
            tab_pins(w, h).iter().any(|p| p.view() == Some(shown)),
            "{w} x {h} shows {shown:?}, which has no tab"
        );
    }
}

#[test]
fn a_pinned_view_a_window_cannot_hold_shows_what_the_size_would() {
    assert_eq!(view_of_pin(Pin::Graphing, 412.0, 915.0), View::Micro);
    assert_eq!(view_of_pin(Pin::Graphing, 800.0, 600.0), View::Programmer);
    assert_eq!(view_of_pin(Pin::Graphing, 1200.0, 800.0), View::Graph);
    assert_eq!(view_of_pin(Pin::Programmer, 412.0, 915.0), View::Micro, "a phone is too narrow for the panel");
    assert_eq!(view_of_pin(Pin::Programmer, 915.0, 412.0), View::Micro, "and a phone on its side too short");
    assert_eq!(view_of_pin(Pin::Programmer, 1200.0, 800.0), View::Programmer, "the user's pin beats the size where it fits");
    assert_eq!(view_of_pin(Pin::Programmer, 1500.0, 1000.0), View::Programmer);
    // The other pins are the user's whatever the window.
    assert_eq!(view_of_pin(Pin::Date, 360.0, 640.0), View::Date);
    assert_eq!(view_of_pin(Pin::Convert, 1920.0, 1080.0), View::Convert);
    assert_eq!(view_of_pin(Pin::Standard, 1920.0, 1080.0), View::Micro);
    assert_eq!(view_of_pin(Pin::Auto, 1920.0, 1080.0), View::Graph);
    assert_eq!(view_of_pin(Pin::Auto, 340.0, 560.0), View::Micro);
}

#[test]
fn every_tab_has_a_view_and_a_word() {
    for pin in tab_pins(2000.0, 1200.0) {
        assert!(pin.view().is_some(), "{}", pin.key());
        assert!(pin.label().starts_with("azcalculator-mode-"), "{}", pin.label());
    }
}

#[test]
fn the_default_desktop_window_stacks_its_tabs_and_the_wider_views_keep_one_row() {
    assert!(DEFAULT_SIZE.0 < STACK_MODES_BELOW, "the default window is narrow");
    assert!(
        PROGRAMMER_MIN_WIDTH >= STACK_MODES_BELOW,
        "the wider views keep the tabs and the buttons in one row"
    );
}
