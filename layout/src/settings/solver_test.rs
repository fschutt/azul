use azul_css::env::{MapEnv, NoEnv};

use super::*;

/// The settings an environment with the one variable `name` set to `value` asks for.
fn with(name: &str, value: &str) -> SolverSettings {
    SolverSettings::from_lookup(&MapEnv::new().with(name, value))
}

#[test]
fn the_default_is_what_an_empty_environment_gives() {
    assert_eq!(SolverSettings::default(), SolverSettings::from_lookup(&NoEnv));
}

#[test]
fn the_default_is_the_shipped_behaviour() {
    let d = SolverSettings::default();
    assert!(d.patching, "the display list is patched");
    assert_eq!(d.moved_runs, MovedRunPolicy::TweenFramesTranslate);
    assert!(!d.skip, "a display list is built");
    assert_eq!(d.dense_text, DenseTextMode::On);
    assert!(d.abs_interior_reuse, "an absolute box's interior is reused");
    assert!(d.taffy_measure_journal, "the measures of taffy are journaled");
    assert!(!d.check_abs_interior_reuse);
    assert!(!d.trace_reconcile);
    assert!(!d.trace_patch);
    assert!(!d.trace_virtual_view_bars);
    assert!(!d.trace_animation);
    assert!(!d.trace_taffy);
    assert!(!d.dump_scrollbar_os);
}

#[test]
fn a_switch_that_turns_a_thing_off_is_set_by_any_value_an_empty_one_included() {
    for value in ["1", "", "0", "anything"] {
        assert_eq!(
            with("AZ_NO_DL_PATCH", value),
            SolverSettings {
                patching: false,
                ..SolverSettings::default()
            },
            "AZ_NO_DL_PATCH={value:?}"
        );
        assert_eq!(
            with("AZ_NO_ABS_INTERIOR_REUSE", value),
            SolverSettings {
                abs_interior_reuse: false,
                ..SolverSettings::default()
            },
            "AZ_NO_ABS_INTERIOR_REUSE={value:?}"
        );
        assert_eq!(
            with("AZ_NO_TAFFY_MEASURE_JOURNAL", value),
            SolverSettings {
                taffy_measure_journal: false,
                ..SolverSettings::default()
            },
            "AZ_NO_TAFFY_MEASURE_JOURNAL={value:?}"
        );
    }
}

#[test]
fn a_check_or_a_trace_is_on_when_its_variable_is_set_to_anything_an_empty_value_included() {
    for value in ["1", "", "0"] {
        assert_eq!(
            with("AZ_CHECK_ABS_INTERIOR_REUSE", value),
            SolverSettings {
                check_abs_interior_reuse: true,
                ..SolverSettings::default()
            },
            "AZ_CHECK_ABS_INTERIOR_REUSE={value:?}"
        );
        assert_eq!(
            with("AZ_RECON_DEBUG", value),
            SolverSettings {
                trace_reconcile: true,
                ..SolverSettings::default()
            },
            "AZ_RECON_DEBUG={value:?}"
        );
        assert_eq!(
            with("AZ_PATCH_DEBUG", value),
            SolverSettings {
                trace_patch: true,
                ..SolverSettings::default()
            },
            "AZ_PATCH_DEBUG={value:?}"
        );
        assert_eq!(
            with("AZ_VV_BAR_TRACE", value),
            SolverSettings {
                trace_virtual_view_bars: true,
                ..SolverSettings::default()
            },
            "AZ_VV_BAR_TRACE={value:?}"
        );
        assert_eq!(
            with("AZ_ANIM_DEBUG", value),
            SolverSettings {
                trace_animation: true,
                ..SolverSettings::default()
            },
            "AZ_ANIM_DEBUG={value:?}"
        );
        assert_eq!(
            with("AZ_TAFFY_DEBUG", value),
            SolverSettings {
                trace_taffy: true,
                ..SolverSettings::default()
            },
            "AZ_TAFFY_DEBUG={value:?}"
        );
        assert_eq!(
            with("AZ_DUMP_SCROLLBAR_OS", value),
            SolverSettings {
                dump_scrollbar_os: true,
                ..SolverSettings::default()
            },
            "AZ_DUMP_SCROLLBAR_OS={value:?}"
        );
    }
}

#[test]
fn the_policy_for_moved_runs_reads_its_value_and_anything_else_is_the_default() {
    let policy = |value: &str| with("AZ_DL_TRANSLATE_MOVED_RUNS", value).moved_runs;
    assert_eq!(policy("1"), MovedRunPolicy::AlwaysTranslate);
    assert_eq!(policy("0"), MovedRunPolicy::NeverTranslate);
    for value in ["", "2", "true", "01", " 1"] {
        assert_eq!(
            policy(value),
            MovedRunPolicy::TweenFramesTranslate,
            "AZ_DL_TRANSLATE_MOVED_RUNS={value:?}"
        );
    }
    // the policy alone: no other switch moves with it
    assert_eq!(
        with("AZ_DL_TRANSLATE_MOVED_RUNS", "1"),
        SolverSettings {
            moved_runs: MovedRunPolicy::AlwaysTranslate,
            ..SolverSettings::default()
        }
    );
}

#[test]
fn the_dense_text_mode_reads_its_value_and_anything_else_is_on() {
    let mode = |value: &str| with("AZ_DENSE_TEXT", value).dense_text;
    assert_eq!(mode("0"), DenseTextMode::Off);
    assert_eq!(mode("off"), DenseTextMode::Off);
    assert_eq!(mode("verify"), DenseTextMode::Verify);
    for value in ["1", "on", "", "Verify", "OFF", "2"] {
        assert_eq!(mode(value), DenseTextMode::On, "AZ_DENSE_TEXT={value:?}");
    }
    assert_eq!(DenseTextMode::default(), DenseTextMode::On);
}

#[test]
fn a_closure_is_an_environment_too() {
    let by_closure = |name: &str| (name == "AZ_NO_DL_PATCH").then(|| String::from("1"));
    assert!(!SolverSettings::from_lookup(&by_closure).patching);
}
