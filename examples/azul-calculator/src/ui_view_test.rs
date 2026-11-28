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
fn the_graphing_tab_is_only_for_a_window_that_can_hold_it() {
    let phone: Vec<Pin> = tab_pins(412.0, 915.0);
    assert_eq!(phone, [Pin::Standard, Pin::Programmer, Pin::Date, Pin::Convert]);
    let desktop = tab_pins(GRAPH_MIN_WIDTH, GRAPH_MIN_HEIGHT);
    assert_eq!(desktop, [Pin::Standard, Pin::Programmer, Pin::Graphing, Pin::Date, Pin::Convert]);
    assert_eq!(tab_pins(GRAPH_MIN_WIDTH, GRAPH_MIN_HEIGHT - 1.0).len(), 4, "tall enough is part of it");
    assert!(!tab_pins(340.0, 560.0).contains(&Pin::Auto), "Auto is no tab: it is the window's size");
}

#[test]
fn a_pinned_graphing_view_in_a_small_window_shows_what_the_size_would() {
    assert_eq!(view_of_pin(Pin::Graphing, 412.0, 915.0), View::Micro);
    assert_eq!(view_of_pin(Pin::Graphing, 800.0, 600.0), View::Programmer);
    assert_eq!(view_of_pin(Pin::Graphing, 1200.0, 800.0), View::Graph);
    // The other pins are the user's whatever the window.
    assert_eq!(view_of_pin(Pin::Programmer, 412.0, 915.0), View::Programmer);
    assert_eq!(view_of_pin(Pin::Date, 360.0, 640.0), View::Date);
    assert_eq!(view_of_pin(Pin::Convert, 1920.0, 1080.0), View::Convert);
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
