use azul_css::env::{MapEnv, NoEnv};

use super::*;

#[test]
fn an_empty_environment_is_the_default() {
    assert_eq!(TelemetrySettings::from_lookup(&NoEnv), TelemetrySettings::default());
    assert_eq!(TelemetrySettings::from_lookup(&MapEnv::new()), TelemetrySettings::default());
    let none = TelemetrySettings::default();
    assert_eq!(none.observe, None);
    assert_eq!(none.tier, None);
    assert_eq!(none.crash_dump, None);
    assert!(!none.e2e_run);
}

#[test]
fn every_variable_is_read_by_its_name() {
    let env = MapEnv::new()
        .with("AZ_OBSERVE", "local")
        .with("AZ_TELEMETRY", "crashes")
        .with("AZ_TELEMETRY_ENDPOINT", "http://127.0.0.1:4318")
        .with("AZ_TELEMETRY_TOKEN", "t0ken")
        .with("AZ_TELEMETRY_FLUSH_SECS", "7")
        .with("AZ_TELEMETRY_CLIENT_ID", "client-1")
        .with("AZ_CRASH_DUMP", "/tmp/dump.json")
        .with("AZ_E2E", "scenario.json")
        .with("HOME", "/home/u");
    let settings = TelemetrySettings::from_lookup(&env);
    assert_eq!(settings.observe.as_deref(), Some("local"));
    assert_eq!(settings.tier.as_deref(), Some("crashes"));
    assert_eq!(settings.endpoint.as_deref(), Some("http://127.0.0.1:4318"));
    assert_eq!(settings.token.as_deref(), Some("t0ken"));
    assert_eq!(settings.flush_secs.as_deref(), Some("7"));
    assert_eq!(settings.client_id.as_deref(), Some("client-1"));
    assert_eq!(settings.crash_dump, Some(OsString::from("/tmp/dump.json")));
    assert!(settings.e2e_run);
    assert_eq!(settings.platform.home, Some(OsString::from("/home/u")));
}

#[test]
fn the_values_are_kept_as_written_for_the_layer_that_applies_them() {
    // an empty value and a value that is no tier are the layer's to judge (it reports the second
    // and ignores both), not the settings'
    let env = MapEnv::new().with("AZ_TELEMETRY", "yes-please").with("AZ_TELEMETRY_ENDPOINT", "");
    let settings = TelemetrySettings::from_lookup(&env);
    assert_eq!(settings.tier.as_deref(), Some("yes-please"));
    assert_eq!(settings.endpoint.as_deref(), Some(""));
}

#[test]
fn the_names_are_the_documented_ones() {
    assert_eq!(ENV_OBSERVE, "AZ_OBSERVE");
    assert_eq!(ENV_TIER, "AZ_TELEMETRY");
    assert_eq!(ENV_ENDPOINT, "AZ_TELEMETRY_ENDPOINT");
    assert_eq!(ENV_TOKEN, "AZ_TELEMETRY_TOKEN");
    assert_eq!(ENV_FLUSH_SECS, "AZ_TELEMETRY_FLUSH_SECS");
    assert_eq!(ENV_CLIENT_ID, "AZ_TELEMETRY_CLIENT_ID");
    assert_eq!(CRASH_DUMP_ENV, "AZ_CRASH_DUMP");
}
