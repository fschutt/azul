use azul_css::env::{MapEnv, NoEnv};

use super::*;

fn env(pairs: &[(&str, &str)]) -> MapEnv {
    let mut map = MapEnv::new();
    for (name, value) in pairs {
        map.set(name, value);
    }
    map
}

/// `Default` is the behaviour of an EMPTY environment, for every group.
#[test]
fn an_empty_environment_is_the_default_settings() {
    let from_nothing = CoreSettings::from_lookup(&NoEnv);
    assert_eq!(from_nothing, CoreSettings::default());
    assert_eq!(from_nothing.log, LogSettings::default());
    assert_eq!(from_nothing.profile, ProfileSettings::default());
    assert_eq!(from_nothing.diag, DiagSettings::new());
    assert_eq!(from_nothing.diag, DiagSettings::default());
    assert_eq!(from_nothing.text_antialiasing, TextAntialiasing::default());
    assert_eq!(DebugState::from_lookup(&NoEnv), DebugState::default());
}

#[test]
fn a_set_of_suppressed_tags_is_a_list_without_case_or_blanks() {
    let set = SuppressSet::from_list(" Var_Fallback , ,IMAGE_churn,");
    assert!(set.contains("var_fallback") && set.contains("image_churn"));
    assert!(!set.contains("all") && !set.contains("other"));
    assert!(!set.contains_or_all("other"));
    let all = SuppressSet::from_list("all");
    assert!(all.contains_or_all("anything"));
    assert!(!all.contains("anything"), "only the var fallback honours `all`");
    assert!(!SuppressSet::new().contains_or_all("anything"));
}

#[test]
fn the_diagnostic_dials_read_their_variables_and_the_alias() {
    let diag = DiagSettings::from_lookup(&env(&[("AZ_SUPPRESS", "var_fallback")]));
    assert!(diag.suppress.contains("var_fallback"));
    // the misspelt alias counts where AZ_SUPPRESS is unset ...
    let diag = DiagSettings::from_lookup(&env(&[("AZ_SUPRESS", "image_churn")]));
    assert!(diag.suppress.contains("image_churn"));
    // ... and AZ_SUPPRESS wins when both are set, even set to nothing
    let diag = DiagSettings::from_lookup(&env(&[("AZ_SUPPRESS", ""), ("AZ_SUPRESS", "all")]));
    assert!(!diag.suppress.contains("all"));
    // the trace is on when the variable is set at all, the flat build when it is not 0 or empty
    assert!(DiagSettings::from_lookup(&env(&[("AZ_CASCADE_TRACE", "")])).cascade_trace);
    assert!(!DiagSettings::from_lookup(&NoEnv).cascade_trace);
    for (value, on) in [("1", true), ("yes", true), ("0", false), ("", false)] {
        let diag = DiagSettings::from_lookup(&env(&[("AZ_FLAT_WIDGETS", value)]));
        assert_eq!(diag.flat_widgets, on, "AZ_FLAT_WIDGETS={value:?}");
    }
}

#[test]
fn the_profile_settings_name_the_modes_the_destination_and_what_to_say() {
    let quiet = ProfileSettings::from_lookup(&env(&[("AZ_PROFILE", "cpu,memory")]));
    assert!(quiet.flags.cpu && quiet.flags.memory && !quiet.flags.heap);
    assert!(quiet.warnings.is_empty(), "{:?}", quiet.warnings);
    assert_eq!(quiet.out_path, None);

    let typo = ProfileSettings::from_lookup(&env(&[("AZ_PROFILE", "cpu,phases")]));
    assert!(typo.flags.cpu);
    assert_eq!(typo.warnings.len(), 1, "{:?}", typo.warnings);
    assert!(typo.warnings[0].contains("phases"));

    let heap_only = ProfileSettings::from_lookup(&env(&[("AZ_PROFILE", "heap")]));
    assert_eq!(heap_only.warnings.len(), 1, "{:?}", heap_only.warnings);
    assert!(heap_only.warnings[0].contains("emits nothing"));

    let no_destination = ProfileSettings::from_lookup(&env(&[("AZ_PROFILE", "heap,jsonl")]));
    assert_eq!(no_destination.warnings.len(), 1);
    assert!(no_destination.warnings[0].contains("AZ_PROFILE_OUT is not"));

    let full = ProfileSettings::from_lookup(&env(&[
        ("AZ_PROFILE", "heap,jsonl,detail"),
        ("AZ_PROFILE_OUT", "/tmp/run.jsonl"),
    ]));
    assert!(full.flags.heap && full.flags.jsonl && full.flags.detail);
    assert_eq!(full.out_path.as_deref(), Some("/tmp/run.jsonl"));
    assert!(full.warnings.is_empty());
}

#[test]
fn the_text_antialiasing_of_the_environment_keeps_its_defaults_and_overrides() {
    let default = TextAntialiasing::default();
    assert_eq!(TextAntialiasing::from_lookup(&NoEnv), default);
    // the pretile is ON by default, and the numbers have defaults
    assert!(default.lcd_pretile && default.lcd_coverage_gamma == 220);
    let asked = TextAntialiasing::from_lookup(&env(&[
        ("AZ_TEXT_AA", "grayscale"),
        ("AZ_LCD_CONTRAST", "40"),
        ("AZ_LCD_COVERAGE_GAMMA", "not a number"),
        ("AZ_LCD_PRETILE", "0"),
    ]));
    assert_eq!(asked.mode, TextAntialiasMode::Grayscale);
    assert_eq!(asked.lcd_contrast, 40);
    assert_eq!(asked.lcd_coverage_gamma, 220, "garbage keeps the default");
    assert!(!asked.lcd_pretile);
    let legacy = TextAntialiasing::from_lookup(&env(&[("AZ_LCD_BLEND", "Legacy")]));
    assert_eq!(legacy.mode, TextAntialiasMode::LcdLegacy);
    let no_tiles = TextAntialiasing::from_lookup(&env(&[("AZ_NO_LCD_PRETILE", "")]));
    assert!(!no_tiles.lcd_pretile);
    // a closure of the old shape is still an environment
    let by_closure = TextAntialiasing::from_vars(|name| (name == "AZ_TEXT_AA").then(|| "none".into()));
    assert_eq!(by_closure.mode, TextAntialiasMode::Aliased);
}

#[test]
fn the_overlay_verbs_come_from_az_overlay() {
    let overlay = DebugState::from_lookup(&env(&[("AZ_OVERLAY", "hit-test, overdraw")]));
    assert!(overlay.show_hit_test_areas && overlay.show_overdraw && !overlay.profiler_dbg);
    assert_eq!(DebugState::from_lookup(&env(&[("AZ_OVERLAY", "")])), DebugState::default());
}

#[test]
fn the_test_hooks_are_not_even_looked_at_without_the_feature() {
    let asked = env(&[("AZ_BACKEND", "headless"), ("AZ_E2E_TEST", "x")]);
    let settings = CoreSettings::from_lookup(&asked);
    if crate::run_mode::AVAILABLE {
        assert!(settings.test_hooks.fake_sign_in);
    } else {
        assert_eq!(settings.test_hooks, crate::run_mode::TestHooks::NONE);
    }
}
