//! Tests of the layout window's settings: an empty environment is the default (the switches that
//! are on by default included), and every variable's absent, empty, garbage and valid value.

use std::path::PathBuf;

use azul_css::env::{MapEnv, NoEnv};

use super::*;

/// The settings of an environment that sets one variable.
fn asked(name: &str, value: &str) -> WindowSettings {
    WindowSettings::from_lookup(&MapEnv::new().with(name, value))
}

/// `Default` is the behaviour of an EMPTY environment: the two switches that are on by default
/// are on, every step of the diff-first rebuild runs, and nothing else is set.
///
/// Mutation: a derived `Default` (every bool false) fails the first two assertions.
#[test]
fn an_empty_environment_is_the_default_and_ships_the_production_paths() {
    let none = WindowSettings::from_lookup(&NoEnv);
    assert_eq!(none, WindowSettings::default());
    assert!(none.eager_state_colors, "the state colours are bound from the start");
    assert!(none.move_in_place, "a write of left/top moves the boxes where they lie");
    assert_eq!(none.diff_first, None, "unset is every step");
    assert_eq!(none.natural_scroll, None, "no pin");
    assert!(!none.patch_verify);
    assert_eq!(none.patch_verify_dump, None);
    assert!(!none.malloc_trim);
    assert!(!none.map_debug);
    assert!(!none.focus_debug);
    assert!(!none.scroll_contract_trace);
    assert!(!none.vv_reconcile_trace);
    assert!(!none.vv_trace);
    assert!(!none.trace_hit);
    assert!(!none.walk_scroll_boxes);
    assert!(!none.scroll_debug);
}

#[test]
fn a_switch_that_is_on_by_default_goes_off_with_the_value_zero_only() {
    assert!(!asked("AZ_EAGER_STATE_COLORS", "0").eager_state_colors);
    assert!(asked("AZ_EAGER_STATE_COLORS", "1").eager_state_colors);
    assert!(asked("AZ_EAGER_STATE_COLORS", "").eager_state_colors);
    assert!(asked("AZ_EAGER_STATE_COLORS", "off").eager_state_colors);

    assert!(!asked("AZ_MOVE_IN_PLACE", "0").move_in_place);
    assert!(asked("AZ_MOVE_IN_PLACE", "1").move_in_place);
    assert!(asked("AZ_MOVE_IN_PLACE", "").move_in_place);
    assert!(asked("AZ_MOVE_IN_PLACE", "false").move_in_place);
}

#[test]
fn the_diff_first_value_is_kept_as_written() {
    assert_eq!(asked("AZ_DIFF_FIRST", "0").diff_first.as_deref(), Some("0"));
    assert_eq!(
        asked("AZ_DIFF_FIRST", "text,nodes").diff_first.as_deref(),
        Some("text,nodes")
    );
    assert_eq!(asked("AZ_DIFF_FIRST", "").diff_first.as_deref(), Some(""));
}

/// The environment's pin: set (to anything) it outranks the app; `1`, `true` and `TRUE` are
/// natural, every other value is the traditional direction.
#[test]
fn the_natural_scroll_pin_is_set_by_the_variable_and_true_for_three_spellings() {
    assert_eq!(WindowSettings::default().natural_scroll, None);
    assert_eq!(asked("AZ_NATURAL_SCROLL", "1").natural_scroll, Some(true));
    assert_eq!(asked("AZ_NATURAL_SCROLL", "true").natural_scroll, Some(true));
    assert_eq!(asked("AZ_NATURAL_SCROLL", "TRUE").natural_scroll, Some(true));
    assert_eq!(asked("AZ_NATURAL_SCROLL", "0").natural_scroll, Some(false));
    assert_eq!(asked("AZ_NATURAL_SCROLL", "True").natural_scroll, Some(false));
    assert_eq!(asked("AZ_NATURAL_SCROLL", "").natural_scroll, Some(false), "set, and not natural");
}

/// A pin that is not text is a pin all the same: set, and not natural.
#[cfg(unix)]
#[test]
fn a_natural_scroll_value_that_is_not_text_still_pins_the_traditional_direction() {
    use std::os::unix::ffi::OsStringExt;
    let value = std::ffi::OsString::from_vec(vec![0x80, 0x81]);
    let env = MapEnv::new().with_os("AZ_NATURAL_SCROLL", value);
    assert_eq!(WindowSettings::from_lookup(&env).natural_scroll, Some(false));
}

#[test]
fn a_trace_switch_is_on_when_its_variable_is_set_to_anything() {
    type Read = fn(&WindowSettings) -> bool;
    let switches: [(&str, Read); 7] = [
        ("AZ_PATCH_VERIFY", |s| s.patch_verify),
        ("AZ_MAP_DEBUG", |s| s.map_debug),
        ("AZ_FOCUS_DEBUG", |s| s.focus_debug),
        ("AZ_VV_RECONCILE_TRACE", |s| s.vv_reconcile_trace),
        ("AZ_VV_TRACE", |s| s.vv_trace),
        ("AZ_TRACE_HIT", |s| s.trace_hit),
        ("AZ_WALK_SCROLL_BOXES", |s| s.walk_scroll_boxes),
    ];
    for (name, read) in switches {
        assert!(!read(&WindowSettings::default()), "{name} is off when unset");
        assert!(read(&asked(name, "1")), "{name}=1");
        assert!(read(&asked(name, "")), "{name} set to nothing is still set");
        assert!(read(&asked(name, "0")), "{name}=0 is still set");
    }
    // set to text; the scroll-contract trace asked for the text of the variable
    assert!(asked("AZ_SCROLL_CONTRACT_TRACE", "1").scroll_contract_trace);
    assert!(asked("AZ_SCROLL_CONTRACT_TRACE", "").scroll_contract_trace);
    assert!(!WindowSettings::default().scroll_contract_trace);
}

#[test]
fn a_switch_that_wants_one_exact_value_takes_nothing_else() {
    assert!(asked("AZ_MALLOC_TRIM", "1").malloc_trim);
    assert!(!asked("AZ_MALLOC_TRIM", "0").malloc_trim);
    assert!(!asked("AZ_MALLOC_TRIM", "yes").malloc_trim);
    assert!(!asked("AZ_MALLOC_TRIM", "").malloc_trim);

    assert!(asked("AZ_SCROLL_DEBUG", "1").scroll_debug);
    assert!(!asked("AZ_SCROLL_DEBUG", "0").scroll_debug);
    assert!(!asked("AZ_SCROLL_DEBUG", "true").scroll_debug);
    assert!(!asked("AZ_SCROLL_DEBUG", "").scroll_debug);
}

#[test]
fn the_patch_verify_dump_is_a_path() {
    assert_eq!(
        asked("AZ_PATCH_VERIFY_DUMP", "/tmp/patch").patch_verify_dump,
        Some(PathBuf::from("/tmp/patch"))
    );
    assert!(!asked("AZ_PATCH_VERIFY_DUMP", "/tmp/patch").patch_verify, "a dump is not the check");
}
