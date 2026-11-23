use azul_css::env::{MapEnv, NoEnv};

use super::*;

#[test]
fn an_empty_environment_asks_for_nothing_and_default_is_that() {
    let none = ShellSettings::from_lookup(&NoEnv);
    assert_eq!(none, ShellSettings::default());
    assert_eq!(none.backend, None);
    assert_eq!(none.window, None);
    assert!(!none.exit_after_first_frame);
    assert!(!none.hit_debug);
    assert_eq!(none.telemetry_channel, "default");
    assert!(!none.dump_system_style);
    assert_eq!(ShellSettings::from_lookup(&MapEnv::new()), none);
}

#[test]
fn the_backend_and_the_window_are_kept_as_typed() {
    let env = MapEnv::new()
        .with("AZ_BACKEND", "Headless")
        .with("AZ_WINDOW", "x11");
    let settings = ShellSettings::from_lookup(&env);
    assert_eq!(settings.backend.as_deref(), Some("Headless"));
    assert_eq!(settings.window.as_deref(), Some("x11"));
    // an empty value is a value: `AZ_WINDOW=` is still an `AZ_WINDOW`
    let empty = ShellSettings::from_lookup(&MapEnv::new().with("AZ_WINDOW", ""));
    assert_eq!(empty.window.as_deref(), Some(""));
}

#[test]
fn the_switches_that_only_need_to_be_set_are_on_for_any_value() {
    for value in ["", "0", "1", "yes"] {
        let env = MapEnv::new()
            .with("AZ_EXIT_SUCCESS_AFTER_FRAME_RENDER", value)
            .with("AZ_HIT_DEBUG", value);
        let settings = ShellSettings::from_lookup(&env);
        assert!(settings.exit_after_first_frame, "set to {value:?}");
        assert!(settings.hit_debug, "set to {value:?}");
    }
}

#[test]
fn a_value_that_is_not_text_is_not_a_switch() {
    // `std::env::var(..).is_ok()` is false for a value that is not UTF-8, and so is the setting:
    // an OS-only value of the map is what `var` does not see
    let env = MapEnv::new().with_os("AZ_HIT_DEBUG", "bytes");
    assert!(!ShellSettings::from_lookup(&env).hit_debug);
}

#[test]
fn the_telemetry_channel_and_the_style_dump() {
    let settings = ShellSettings::from_lookup(&MapEnv::new().with("AZ_TELEMETRY_CHANNEL", "beta"));
    assert_eq!(settings.telemetry_channel, "beta");
    for (value, on) in [("1", true), ("0", false), ("", false), ("true", false)] {
        let settings =
            ShellSettings::from_lookup(&MapEnv::new().with("AZ_DUMP_SYSTEM_STYLE", value));
        assert_eq!(settings.dump_system_style, on, "AZ_DUMP_SYSTEM_STYLE={value:?}");
    }
}
