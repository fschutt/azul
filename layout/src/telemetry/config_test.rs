use std::ffi::OsString;

use super::*;

fn applied(settings: &TelemetrySettings) -> TelemetryConfig {
    let mut config = TelemetryConfig::default();
    apply_settings(&mut config, settings);
    config
}

#[test]
fn an_empty_environment_changes_nothing() {
    assert_eq!(applied(&TelemetrySettings::default()), TelemetryConfig::default());
}

#[test]
fn the_tier_variable_names_a_tier_and_an_off_pins_it() {
    let tier = |name: &str| {
        applied(&TelemetrySettings {
            tier: Some(name.to_owned()),
            ..TelemetrySettings::default()
        })
    };
    let metrics = tier("metrics");
    assert_eq!(metrics.tier, TelemetryTier::Metrics);
    assert_eq!(metrics.tier_source, TierSource::Env);
    assert!(!metrics.pinned_off);
    let off = tier("OFF");
    assert_eq!(off.tier, TelemetryTier::Off);
    assert!(off.pinned_off, "an env pin of off suppresses the consent dialog");
    // a value that is no tier is reported and ignored: telemetry stays what it was
    let typo = tier("yes-please");
    assert_eq!(typo, TelemetryConfig::default());
}

#[test]
fn observe_one_is_the_local_stack_and_a_url_is_another_collector() {
    let observe = |value: &str| {
        applied(&TelemetrySettings {
            observe: Some(value.to_owned()),
            ..TelemetrySettings::default()
        })
    };
    for local in ["1", "local", "ON", "True"] {
        let config = observe(local);
        assert_eq!(config.tier, TelemetryTier::Metrics, "{local}");
        assert_eq!(config.endpoint, OBSERVE_LOCAL_ENDPOINT, "{local}");
        assert_eq!(config.auth_token.as_deref(), Some(OBSERVE_LOCAL_TOKEN), "{local}");
        assert_eq!(config.flush_interval_secs, 5, "snappy dashboards");
        assert_eq!(config.tier_source, TierSource::Env);
    }
    assert_eq!(observe("http://collector:4318").endpoint, "http://collector:4318");
    for off in ["", "0", "off", "OFF", "  "] {
        assert_eq!(observe(off), TelemetryConfig::default(), "{off:?} is no request");
    }
}

#[test]
fn the_granular_variables_override_a_piece_of_observe() {
    let config = applied(&TelemetrySettings {
        observe: Some("1".to_owned()),
        tier: Some("crashes".to_owned()),
        endpoint: Some("http://other:1".to_owned()),
        token: Some("t0ken".to_owned()),
        flush_secs: Some("30".to_owned()),
        client_id: Some("client-9".to_owned()),
        ..TelemetrySettings::default()
    });
    assert_eq!(config.tier, TelemetryTier::Crashes);
    assert_eq!(config.endpoint, "http://other:1");
    assert_eq!(config.auth_token.as_deref(), Some("t0ken"));
    assert_eq!(config.flush_interval_secs, 30);
    assert_eq!(config.client_id.as_deref(), Some("client-9"));
}

#[test]
fn an_empty_endpoint_token_or_client_id_and_a_garbage_interval_are_ignored() {
    let config = applied(&TelemetrySettings {
        endpoint: Some(String::new()),
        token: Some(String::new()),
        client_id: Some(String::new()),
        flush_secs: Some("soon".to_owned()),
        ..TelemetrySettings::default()
    });
    assert_eq!(config, TelemetryConfig::default());
    let zero = applied(&TelemetrySettings {
        flush_secs: Some("0".to_owned()),
        ..TelemetrySettings::default()
    });
    assert_eq!(zero.flush_interval_secs, 1, "never faster than one second");
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
#[test]
fn the_fallback_folders_follow_the_xdg_rules() {
    let platform = |xdg_config: Option<&str>, xdg_data: Option<&str>, home: Option<&str>| {
        PlatformEnv {
            xdg_config_home: xdg_config.map(OsString::from),
            xdg_data_home: xdg_data.map(OsString::from),
            home: home.map(OsString::from),
            ..PlatformEnv::default()
        }
    };
    let both = platform(Some("/cfg"), Some("/data"), Some("/home/u"));
    assert_eq!(fallback_dir(&both, true), Some(PathBuf::from("/cfg")));
    assert_eq!(fallback_dir(&both, false), Some(PathBuf::from("/data")));
    // a relative XDG folder is no folder: the home's takes over
    let relative = platform(Some("cfg"), Some("data"), Some("/home/u"));
    assert_eq!(fallback_dir(&relative, true), Some(PathBuf::from("/home/u/.config")));
    assert_eq!(fallback_dir(&relative, false), Some(PathBuf::from("/home/u/.local/share")));
    let home_only = platform(None, None, Some("/home/u"));
    assert_eq!(fallback_dir(&home_only, true), Some(PathBuf::from("/home/u/.config")));
    // nothing set, nothing to find
    assert_eq!(fallback_dir(&PlatformEnv::default(), true), None);
    assert_eq!(fallback_dir(&PlatformEnv::default(), false), None);
}
