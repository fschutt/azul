use azul_css::env::{MapEnv, NoEnv};

use super::*;

fn with(pairs: &[(&str, &str)]) -> ExtraSettings {
    let mut env = MapEnv::new();
    for (name, value) in pairs {
        env.set(name, value);
    }
    ExtraSettings::from_lookup(&env)
}

#[test]
fn the_empty_environment_is_the_default() {
    let none = ExtraSettings::from_lookup(&NoEnv);
    assert_eq!(none, ExtraSettings::default());
    assert_eq!(
        none,
        ExtraSettings {
            pagination_tokens: false,
            screencap_debug: false,
            screencap_start_timeout_secs: 300,
        }
    );
}

#[test]
fn only_the_word_tokens_selects_the_token_engine() {
    assert!(with(&[("AZ_PAGINATION_ENGINE", "tokens")]).pagination_tokens);
    for other in ["", "slicer", "Tokens", "token", "1"] {
        assert!(!with(&[("AZ_PAGINATION_ENGINE", other)]).pagination_tokens, "{other:?}");
    }
}

#[test]
fn the_capture_trace_is_on_when_the_variable_is_set_to_anything() {
    assert!(with(&[("AZ_SCREENCAP_DEBUG", "")]).screencap_debug);
    assert!(with(&[("AZ_SCREENCAP_DEBUG", "0")]).screencap_debug);
    assert!(!with(&[]).screencap_debug);
}

#[test]
fn the_share_dialog_timeout_is_a_number_of_seconds_or_five_minutes() {
    assert_eq!(with(&[("AZ_SCREENCAP_START_TIMEOUT_SECS", "20")]).screencap_start_timeout_secs, 20);
    assert_eq!(with(&[("AZ_SCREENCAP_START_TIMEOUT_SECS", "0")]).screencap_start_timeout_secs, 0);
    for bad in ["", "soon", "-5", "1.5", "99999999999999999999999"] {
        assert_eq!(
            with(&[("AZ_SCREENCAP_START_TIMEOUT_SECS", bad)]).screencap_start_timeout_secs,
            SCREENCAP_START_TIMEOUT_SECS,
            "{bad:?}"
        );
    }
}

#[test]
fn the_names_of_the_headless_stand_in_switches_are_kept() {
    assert_eq!(BATTERY_STATE_FILE_VAR, "AZ_BATTERY_STATE_FILE");
    assert_eq!(NETWORK_STATE_FILE_VAR, "AZ_NETWORK_STATE_FILE");
    assert_eq!(SPEECH_LOG_VAR, "AZ_SPEECH_LOG");
    assert_eq!(SPEECH_HEARD_VAR, "AZ_SPEECH_HEARD");
    assert_eq!(SPEECH_SCRIPT_VAR, "AZ_SPEECH_SCRIPT");
}
