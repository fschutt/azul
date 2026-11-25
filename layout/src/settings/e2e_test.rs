use azul_css::env::{MapEnv, NoEnv};

use super::*;

#[test]
fn an_empty_environment_is_the_default() {
    assert_eq!(E2eSettings::from_lookup(&NoEnv), E2eSettings::default());
    assert_eq!(E2eSettings::from_lookup(&MapEnv::new()), E2eSettings::default());
    let none = E2eSettings::default();
    assert_eq!(none.record, None);
    assert_eq!(none.debug_port, None);
    assert!(!none.record_baselines && !none.dump_divergent_pixels);
    assert!(!none.act_debug && !none.anim_debug);
    assert_eq!(none.shot_dir, None);
    assert_eq!(none.synthetic_devices, None);
    assert_eq!(none.allow_http, None);
}

#[test]
fn every_variable_is_read_by_its_name() {
    let env = MapEnv::new()
        .with("AZ_RECORD", "/tmp/azul.log")
        .with("AZ_DEBUG", "8765")
        .with("AZ_E2E_RECORD", "1")
        .with("AZ_E2E_DUMP", "")
        .with("AZ_E2E_SHOT_DIR", "/tmp/shots")
        .with("AZ_ACT_DEBUG", "1")
        .with("AZ_ANIM_DEBUG", "")
        .with("AZ_SYNTHETIC_DEVICES", "camera,screen")
        .with("AZ_E2E_ALLOW_HTTP", "http://127.0.0.1:*");
    assert_eq!(
        E2eSettings::from_lookup(&env),
        E2eSettings {
            record: Some(PathBuf::from("/tmp/azul.log")),
            debug_port: Some(8765),
            record_baselines: true,
            dump_divergent_pixels: true,
            shot_dir: Some(PathBuf::from("/tmp/shots")),
            act_debug: true,
            anim_debug: true,
            synthetic_devices: Some("camera,screen".to_string()),
            allow_http: Some("http://127.0.0.1:*".to_string()),
        }
    );
}

#[test]
fn the_debug_port_is_a_port_or_nothing() {
    let port = |value: &str| E2eSettings::from_lookup(&MapEnv::new().with("AZ_DEBUG", value)).debug_port;
    assert_eq!(port("1"), Some(1));
    assert_eq!(port("65535"), Some(65535));
    assert_eq!(port("65536"), None, "not a port");
    assert_eq!(port("-1"), None);
    assert_eq!(port(""), None);
    assert_eq!(port("localhost"), None);
}

#[test]
fn baseline_recording_is_one_or_true_and_nothing_else() {
    let on = |value: &str| {
        E2eSettings::from_lookup(&MapEnv::new().with("AZ_E2E_RECORD", value)).record_baselines
    };
    assert!(on("1"));
    assert!(on("true"));
    assert!(on("TRUE"));
    assert!(!on("0"));
    assert!(!on("yes"));
    assert!(!on(""));
}

#[test]
fn a_trace_is_on_when_its_variable_is_set_to_anything() {
    let env = MapEnv::new().with("AZ_ACT_DEBUG", "0");
    assert!(E2eSettings::from_lookup(&env).act_debug, "set to 0 is still set");
    assert!(!E2eSettings::from_lookup(&env).anim_debug);
}
