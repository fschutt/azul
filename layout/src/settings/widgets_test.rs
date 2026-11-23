//! Tests of the widgets' settings: an empty environment is the default, and every variable's
//! absent, empty, garbage and valid value.

use azul_css::env::{MapEnv, NoEnv};

use super::*;

/// The settings of an environment that sets one variable.
fn asked(name: &str, value: &str) -> WidgetSettings {
    WidgetSettings::from_lookup(&MapEnv::new().with(name, value))
}

#[test]
fn an_empty_environment_is_the_default_and_every_widget_builds_as_shipped() {
    let none = WidgetSettings::from_lookup(&NoEnv);
    assert_eq!(none, WidgetSettings::default());
    assert!(!none.map_debug);
    assert!(!none.map_stats);
    assert!(!none.video_framelog);
}

/// The two A/B variables of widget builders that have no window to ask are gone: setting them
/// changes nothing.
#[test]
fn the_retired_ab_variables_change_nothing() {
    for (name, value) in [("AZ_COLOR_PICKER_CHECKER", "cells"), ("AZ_NODEGRAPH_PAN", "nodes")] {
        assert_eq!(asked(name, value), WidgetSettings::default(), "{name}={value}");
    }
}

#[test]
fn the_map_stats_are_on_for_any_value_that_is_not_blank_or_zero() {
    assert!(asked("AZ_MAP_STATS", "1").map_stats);
    assert!(asked("AZ_MAP_STATS", "yes").map_stats);
    assert!(asked("AZ_MAP_STATS", " 2 ").map_stats);
    assert!(!asked("AZ_MAP_STATS", "0").map_stats);
    assert!(!asked("AZ_MAP_STATS", " 0 ").map_stats);
    assert!(!asked("AZ_MAP_STATS", "").map_stats);
    assert!(!asked("AZ_MAP_STATS", "   ").map_stats);
}

#[test]
fn a_trace_switch_is_on_when_its_variable_is_set() {
    assert!(asked("AZ_MAP_DEBUG", "1").map_debug);
    assert!(asked("AZ_MAP_DEBUG", "").map_debug, "set to nothing is still set");
    assert!(asked("AZ_MAP_DEBUG", "0").map_debug, "set to 0 is still set");
    assert!(asked("AZ_VIDEO_FRAMELOG", "1").video_framelog);
    assert!(asked("AZ_VIDEO_FRAMELOG", "").video_framelog);
    assert!(!asked("AZ_VIDEO_FRAME", "1").video_framelog, "another name is another variable");
}
