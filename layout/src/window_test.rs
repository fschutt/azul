//! Tests of what a layout window does with the settings it is given
//! (`LayoutWindow::set_settings`): the switches it answers, the scroll direction the environment
//! pins, the walk of every scroll-driven box, and that a window without settings has the
//! behaviour of an empty environment.

use azul_core::resources::NaturalScroll;
use azul_css::env::MapEnv;
use rust_fontconfig::FcFontCache;

use super::*;
use crate::settings::LayoutSettings;

fn window() -> LayoutWindow {
    LayoutWindow::new(FcFontCache::default()).expect("a layout window")
}

/// The settings an environment that sets `pairs` asks for.
fn settings_of(pairs: &[(&str, &str)]) -> LayoutSettings {
    let mut env = MapEnv::new();
    for (name, value) in pairs {
        env.set(name, value);
    }
    LayoutSettings::from_lookup(&env)
}

/// A window nobody gave settings has the behaviour of an empty environment: every path of a
/// rebuild is on, and nothing is pinned or walked.
///
/// Mutation: a window whose switches start with a path off fails the first three assertions.
#[test]
fn a_window_without_settings_runs_every_path_and_pins_nothing() {
    let lw = window();
    assert!(lw.switches.diff_first_enabled());
    for step in DiffFirstStep::ALL {
        assert!(lw.switches.diff_first_step(step), "{step:?}");
    }
    assert!(lw.switches.state_colors_from_the_build());
    assert!(lw.switches.move_in_place_enabled());
    assert!(!lw.scroll_manager.is_natural_scroll());
    assert!(!lw.gpu_state_manager.walk_every_frame);
    assert_eq!(lw.settings, LayoutSettings::default());
}

/// What the environment asked for reaches the owners of the switches: the window's switches, the
/// scroll manager and the GPU state manager.
#[test]
fn the_settings_a_window_is_given_reach_the_things_that_use_them() {
    let mut lw = window();
    lw.set_settings(settings_of(&[
        ("AZ_DIFF_FIRST", "0"),
        ("AZ_EAGER_STATE_COLORS", "0"),
        ("AZ_MOVE_IN_PLACE", "0"),
        ("AZ_NATURAL_SCROLL", "1"),
        ("AZ_WALK_SCROLL_BOXES", "1"),
    ]));
    assert!(!lw.switches.diff_first_enabled(), "AZ_DIFF_FIRST=0 is no step");
    assert!(!lw.switches.state_colors_from_the_build());
    assert!(!lw.switches.move_in_place_enabled());
    assert!(lw.scroll_manager.is_natural_scroll(), "AZ_NATURAL_SCROLL=1 pins the natural direction");
    assert!(lw.gpu_state_manager.walk_every_frame);
    assert!(lw.settings.window.walk_scroll_boxes);
}

/// `AZ_DIFF_FIRST` names the steps that are on; the window chooses its own on top of them, and
/// `None` is the settings' answer again.
#[test]
fn the_steps_of_the_diff_first_rebuild_are_the_ones_the_settings_name() {
    let mut lw = window();
    lw.set_settings(settings_of(&[("AZ_DIFF_FIRST", "text,nodes")]));
    assert!(lw.switches.diff_first_step(DiffFirstStep::Text));
    assert!(lw.switches.diff_first_step(DiffFirstStep::Nodes));
    assert!(!lw.switches.diff_first_step(DiffFirstStep::Style));
    assert!(!lw.switches.diff_first_step(DiffFirstStep::TreeList));
    lw.switches.set_diff_first(Some(true));
    assert!(lw.switches.diff_first_step(DiffFirstStep::Style), "the window's own choice wins");
    lw.switches.set_diff_first(None);
    assert!(!lw.switches.diff_first_step(DiffFirstStep::Style), "None: the settings' again");
    // another window is not touched by any of it
    assert!(window().switches.diff_first_step(DiffFirstStep::Style));
}

/// The pin of the environment outranks the app's choice, whichever of the two the window hears
/// first; without a pin the app decides.
///
/// Mutation: an `adopt_app_natural_scroll` that ignores the pin fails the first two blocks.
#[test]
fn an_environment_pin_of_the_scroll_direction_outranks_the_app() {
    let pin = [("AZ_NATURAL_SCROLL", "0")];

    let mut app_first = window();
    app_first.scroll_manager.adopt_app_natural_scroll(NaturalScroll::Enabled);
    assert!(app_first.scroll_manager.is_natural_scroll(), "no pin yet: the app decides");
    app_first.set_settings(settings_of(&pin));
    assert!(!app_first.scroll_manager.is_natural_scroll(), "the pin outranks what the app chose");

    let mut pin_first = window();
    pin_first.set_settings(settings_of(&pin));
    pin_first.scroll_manager.adopt_app_natural_scroll(NaturalScroll::Enabled);
    assert!(!pin_first.scroll_manager.is_natural_scroll(), "and the app cannot move a pin");

    let mut unpinned = window();
    unpinned.set_settings(LayoutSettings::default());
    unpinned.scroll_manager.adopt_app_natural_scroll(NaturalScroll::Enabled);
    assert!(unpinned.scroll_manager.is_natural_scroll(), "without a pin the app decides");
}
