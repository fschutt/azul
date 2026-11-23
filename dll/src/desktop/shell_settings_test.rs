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
    assert_eq!(none.e2e_script, None);
    assert_eq!(none.e2e_scenario, None);
    assert_eq!(none.e2e_runtime, None);
    assert!(!none.e2e_run);
    assert!(!none.no_color);
    assert!(!none.log_stderr_off);
    assert_eq!(none.log_file, None);
    assert_eq!(none.headless_snapshot_path, None);
    assert!(!none.focus_trace);
    assert!(!none.reconcile_debug);
    assert!(!none.layout_trace);
    assert!(!none.validate);
    assert!(none.native_backbuffer, "the native back buffer is ON by default");
    assert!(none.inert_knobs().is_empty());
    assert!(!none.pace_trace);
    assert_eq!(none.cpu_present, None);
    assert!(!none.keep_caches_minimized);
    assert!(none.halo_release_full, "a hidden window gives the halos back by default");
    assert_eq!(none.a11y_eager, None);
    assert!(!none.cursor_set_always);
    assert_eq!(ShellSettings::from_lookup(&MapEnv::new()), none);
}

#[test]
fn the_macos_switches_read_what_they_always_read() {
    // set at all
    for value in ["", "0", "1"] {
        let settings = ShellSettings::from_lookup(&MapEnv::new().with("AZ_PACE_TRACE", value));
        assert!(settings.pace_trace, "AZ_PACE_TRACE={value:?}");
    }
    // set and not `0` (an empty value is a value that is not `0`)
    for (value, on) in [("1", true), ("", true), ("0", false)] {
        let env = MapEnv::new()
            .with("AZ_KEEP_CACHES_MINIMIZED", value)
            .with("AZ_CURSOR_SET_ALWAYS", value);
        let settings = ShellSettings::from_lookup(&env);
        assert_eq!(settings.keep_caches_minimized, on, "keep caches {value:?}");
        assert_eq!(settings.cursor_set_always, on, "cursor {value:?}");
    }
    // only `lists` keeps the halo pictures
    for (value, full) in [("lists", false), ("Lists", true), ("all", true), ("", true)] {
        let settings = ShellSettings::from_lookup(&MapEnv::new().with("AZ_HALO_RELEASE", value));
        assert_eq!(settings.halo_release_full, full, "AZ_HALO_RELEASE={value:?}");
    }
    // the words are the macOS shell's to parse
    let env = MapEnv::new()
        .with("AZ_CPU_PRESENT", "bitmap,layer")
        .with("AZ_A11Y_EAGER", "1");
    let settings = ShellSettings::from_lookup(&env);
    assert_eq!(settings.cpu_present.as_deref(), Some("bitmap,layer"));
    assert_eq!(settings.a11y_eager.as_deref(), Some("1"));
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

#[test]
fn the_scripts_are_paths_that_are_not_empty() {
    let env = MapEnv::new()
        .with("AZ_E2E", "/tmp/tests")
        .with("AZ_E2E_TEST", "/tmp/scenario.json")
        .with("AZ_E2E_RUNTIME", "/usr/bin/bun")
        .with("AZ_HEADLESS_SNAPSHOT_PATH", "/tmp/out.png");
    let settings = ShellSettings::from_lookup(&env);
    assert_eq!(settings.e2e_runtime.as_deref(), Some("/usr/bin/bun"));
    assert_eq!(settings.e2e_script.as_deref(), Some("/tmp/tests"));
    assert_eq!(settings.e2e_scenario.as_deref(), Some("/tmp/scenario.json"));
    assert_eq!(settings.headless_snapshot_path.as_deref(), Some("/tmp/out.png"));
    // an empty script is none; an empty snapshot path stays what it was (a path to fail on)
    let env = MapEnv::new()
        .with("AZ_E2E", "")
        .with("AZ_E2E_TEST", "")
        .with("AZ_E2E_RUNTIME", "")
        .with("AZ_HEADLESS_SNAPSHOT_PATH", "");
    let settings = ShellSettings::from_lookup(&env);
    assert_eq!(settings.e2e_runtime, None);
    assert_eq!(settings.e2e_script, None);
    assert_eq!(settings.e2e_scenario, None);
    assert_eq!(settings.headless_snapshot_path.as_deref(), Some(""));
}

#[test]
fn the_reconcile_debug_is_on_when_the_variable_is_set_at_all() {
    for value in ["", "0", "1"] {
        let settings =
            ShellSettings::from_lookup(&MapEnv::new().with("AZ_RECONCILE_DEBUG", value));
        assert!(settings.reconcile_debug, "AZ_RECONCILE_DEBUG={value:?}");
    }
    for value in ["", "0", "1"] {
        let settings = ShellSettings::from_lookup(&MapEnv::new().with("AZ_FOCUS_TRACE", value));
        assert!(settings.focus_trace, "AZ_FOCUS_TRACE={value:?}");
    }
}

#[test]
fn the_layout_trace_needs_a_value_that_is_not_empty_and_not_zero() {
    for (value, on) in [("1", true), ("yes", true), ("0", false), ("", false)] {
        let settings = ShellSettings::from_lookup(&MapEnv::new().with("AZ_LAYOUT_TRACE", value));
        assert_eq!(settings.layout_trace, on, "AZ_LAYOUT_TRACE={value:?}");
    }
}

#[test]
fn the_native_back_buffer_goes_off_for_a_zero_and_only_for_a_zero() {
    for (value, on) in [("0", false), ("1", true), ("", true), ("off", true)] {
        let settings =
            ShellSettings::from_lookup(&MapEnv::new().with("AZ_NATIVE_BACKBUFFER", value));
        assert_eq!(settings.native_backbuffer, on, "AZ_NATIVE_BACKBUFFER={value:?}");
    }
}

/// The parsing of the validation gate: off unless it says otherwise.
#[test]
fn the_validate_variable_is_off_unless_it_says_otherwise() {
    assert!(!validation_from_value(None), "unset must be off");
    for off in ["", "  ", "0", "off", "false", "no", "none", "OFF", "False"] {
        assert!(!validation_from_value(Some(off)), "`{off}` must be off");
    }
    for on in ["1", "yes", "true", "on", "  1 ", "anything"] {
        assert!(validation_from_value(Some(on)), "`{on}` must be on");
    }
    for (value, on) in [("1", true), ("0", false), ("", false)] {
        let settings = ShellSettings::from_lookup(&MapEnv::new().with("AZ_VALIDATE", value));
        assert_eq!(settings.validate, on, "AZ_VALIDATE={value:?}");
    }
}

#[test]
fn a_debug_build_validates_whatever_the_environment_says() {
    let off = ShellSettings::default();
    let on = ShellSettings::from_lookup(&MapEnv::new().with("AZ_VALIDATE", "1"));
    assert!(on.validation_enabled());
    assert_eq!(off.validation_enabled(), cfg!(debug_assertions));
}

#[test]
fn a_knob_that_needs_a_feature_this_build_lacks_is_reported_when_it_is_set() {
    // every knob of the table, set: those whose feature is compiled out are inert
    let env = MapEnv::new()
        .with("AZ_E2E", "x")
        .with("AZ_DEBUG", "1")
        .with("AZ_E2E_TEST", "x")
        .with("AZ_HEADLESS_SNAPSHOT_PATH", "x")
        .with("AZ_DUMP_FRAME_DIR", "x")
        .with("AZ_PROFILE", "cpu");
    let all = ShellSettings::from_lookup(&env);
    let expected: Vec<&str> = gated_knobs()
        .iter()
        .filter(|knob| !knob.compiled)
        .map(|knob| knob.name)
        .collect();
    let reported: Vec<&str> = all.inert_knobs().iter().map(|knob| knob.name).collect();
    assert_eq!(reported, expected);
    // an empty value is not a knob that is set
    let empty = ShellSettings::from_lookup(&MapEnv::new().with("AZ_E2E", ""));
    assert!(empty.inert_knobs().is_empty());
    // a knob that is not set is not reported, whatever this build has
    assert!(ShellSettings::default().inert_knobs().is_empty());
    for knob in all.inert_knobs() {
        assert!(!knob.feature.is_empty() && knob.how.starts_with("cargo build"));
    }
}

#[test]
fn the_android_present_report_comes_from_the_variable_or_the_older_property() {
    // the variable: set at all
    for value in ["", "0", "1"] {
        let settings =
            ShellSettings::from_lookup(&MapEnv::new().with("AZ_DUMP_PRESENT", value));
        assert!(settings.dump_present, "AZ_DUMP_PRESENT={value:?}");
    }
    // the property `debug.az.present`, which the Android reader answers as AZ_PRESENT: neither
    // empty, nor `0`, nor `false`
    for (value, on) in [("1", true), ("yes", true), ("", false), ("0", false), ("false", false)] {
        let settings = ShellSettings::from_lookup(&MapEnv::new().with("AZ_PRESENT", value));
        assert_eq!(settings.dump_present, on, "AZ_PRESENT={value:?}");
    }
    assert!(!ShellSettings::default().dump_present);
}

#[test]
fn a_scripted_run_and_the_colour_switch_are_on_when_the_variable_is_set_at_all() {
    for value in ["", "0", "/tmp/x.json"] {
        let settings = ShellSettings::from_lookup(&MapEnv::new().with("AZ_E2E", value));
        assert!(settings.e2e_run, "AZ_E2E={value:?}");
        let settings = ShellSettings::from_lookup(&MapEnv::new().with("NO_COLOR", value));
        assert!(settings.no_color, "NO_COLOR={value:?}");
    }
    // an empty script is no script, but the run is still a scripted one for the panic modal
    let settings = ShellSettings::from_lookup(&MapEnv::new().with("AZ_E2E", ""));
    assert_eq!(settings.e2e_script, None);
}

#[test]
fn the_log_gate_asks_for_a_file_and_can_switch_its_stderr_copy_off() {
    for (value, off) in [
        ("0", true),
        ("off", true),
        (" OFF ", true),
        ("False", true),
        ("no", true),
        ("none", true),
        ("1", false),
        ("", false),
        ("on", false),
        ("2", false),
    ] {
        let settings = ShellSettings::from_lookup(&MapEnv::new().with("AZ_LOG_STDERR", value));
        assert_eq!(settings.log_stderr_off, off, "AZ_LOG_STDERR={value:?}");
    }
    let settings = ShellSettings::from_lookup(&MapEnv::new().with("AZ_LOG_FILE", "/tmp/azul.log"));
    assert_eq!(settings.log_file.as_deref(), Some("/tmp/azul.log"));
}
