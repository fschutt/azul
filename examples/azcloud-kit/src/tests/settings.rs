//! What a run decides before it talks to anyone: every layer, where each value came from, the
//! report `azcloud config` prints.

use std::path::{Path, PathBuf};

use azul_appkit::azlin_config::{
    AzlinConfig, Endpoint, EndpointFlags, EndpointsSection, Profile, Source,
};

use crate::{
    settings::{redact_url, Flags, OsDirs, Settings},
    transport::TransportPref,
};

fn os() -> OsDirs {
    OsDirs {
        home: Some(PathBuf::from("/home/ann")),
        config: Some(PathBuf::from("/home/ann/.config")),
        data: Some(PathBuf::from("/home/ann/.local/share")),
    }
}

fn no_file(_: &Path) -> (AzlinConfig, Option<String>) {
    (AzlinConfig::default(), None)
}

fn no_env(_: &str) -> Option<String> {
    None
}

#[test]
fn with_nothing_configured_every_folder_and_endpoint_is_a_built_in_default_and_says_so() {
    let s = Settings::resolve(&Flags::default(), &no_env, &os(), &no_file);
    assert_eq!(
        s.config_file.path,
        Some(PathBuf::from("/home/ann/.azlin/config.json"))
    );
    assert_eq!(
        s.state_dir.path,
        Some(PathBuf::from("/home/ann/.config/azcloud"))
    );
    assert_eq!(
        s.data_root.path,
        Some(PathBuf::from("/home/ann/.local/share/Azlin"))
    );
    assert_eq!(s.azlin_home.path, Some(PathBuf::from("/home/ann/.azlin")));
    assert_eq!(s.transport_pref(), TransportPref::Auto);
    let defaults = s.defaults_in_use().join("\n");
    for expected in [
        "profile local",
        "token = http://127.0.0.1:8081 (profile local)",
        "transport = auto",
        "state folder",
        "data root",
    ] {
        assert!(defaults.contains(expected), "{expected:?} in {defaults}");
    }
    assert!(
        !defaults.contains("s3 ="),
        "the S3 endpoint is the drive's, not a default"
    );
}

#[test]
fn the_config_file_the_environment_and_the_flags_are_layered_in_that_order() {
    let load = |_: &Path| -> (AzlinConfig, Option<String>) {
        let mut c = AzlinConfig::default();
        c.endpoints.profile = Some(Profile::Local);
        c.endpoints
            .set(Endpoint::Token, Some("http://127.0.0.1:18081"))
            .unwrap();
        (c, None)
    };
    let env = |name: &str| -> Option<String> {
        match name {
            "AZLIN_CONFIG" => Some(String::from("/tmp/e2e/config.json")),
            "AZCLOUD_TRANSPORT" => Some(String::from("https")),
            "AZCLOUD_HOME" => Some(String::from("/tmp/e2e/state")),
            "AZLIN_DATA" => Some(String::from(" /tmp/e2e/data ")),
            "AZLIN_IROH_NODE" => Some("ab".repeat(32)),
            "AZLIN_IROH_ADDR" => Some(String::from("127.0.0.1:41000")),
            _ => None,
        }
    };
    let s = Settings::resolve(&Flags::default(), &env, &os(), &load);
    assert_eq!(
        s.config_file.path,
        Some(PathBuf::from("/tmp/e2e/config.json"))
    );
    assert_eq!(s.config_file.source, Source::Env("AZLIN_CONFIG"));
    assert_eq!(
        s.endpoints.url(Endpoint::Token),
        Some("http://127.0.0.1:18081")
    );
    assert_eq!(
        s.endpoints.get(Endpoint::Token).source,
        Source::File(PathBuf::from("/tmp/e2e/config.json"))
    );
    assert_eq!(s.transport_pref(), TransportPref::Https);
    assert_eq!(s.state_dir.path, Some(PathBuf::from("/tmp/e2e/state")));
    assert_eq!(s.data_root.path, Some(PathBuf::from("/tmp/e2e/data")));
    assert_eq!(s.data_root.source, Source::Env("AZLIN_DATA"));
    assert_eq!(s.iroh_addr.value.as_deref(), Some("127.0.0.1:41000"));

    let flags = Flags {
        transport: Some(String::from("iroh")),
        state_dir: Some(PathBuf::from("/srv/state")),
        config: Some(PathBuf::from("/srv/config.json")),
        endpoints: EndpointFlags {
            token: Some(String::from("https://token.test")),
            ..EndpointFlags::default()
        },
        ..Flags::default()
    };
    let s = Settings::resolve(&flags, &env, &os(), &load);
    assert_eq!(s.config_file.source, Source::Flag("--config"));
    assert_eq!(s.transport_pref(), TransportPref::Iroh);
    assert_eq!(s.state_dir.path, Some(PathBuf::from("/srv/state")));
    assert_eq!(s.endpoints.url(Endpoint::Token), Some("https://token.test"));
}

#[test]
fn a_shared_config_switched_off_leaves_the_profile_and_a_bad_value_is_ignored_with_a_reason() {
    let env = |name: &str| -> Option<String> {
        match name {
            "AZLIN_CONFIG" => Some(String::from("off")),
            "AZCLOUD_TRANSPORT" => Some(String::from("carrier-pigeon")),
            "AZLIN_IROH_ADDR" => Some(String::from("localhost")),
            _ => None,
        }
    };
    let s = Settings::resolve(&Flags::default(), &env, &os(), &no_file);
    assert_eq!(s.config_file.path, None);
    assert_eq!(
        s.endpoints.get(Endpoint::Token).source,
        Source::Profile(Profile::Local)
    );
    assert_eq!(s.transport_pref(), TransportPref::Auto);
    assert_eq!(s.transport.rejected.len(), 1);
    assert_eq!(s.iroh_addr.value, None);
    assert_eq!(s.iroh_addr.rejected.len(), 1);
    let (json, text) = s.report(None);
    assert_eq!(json["transport"]["rejected"][0]["value"], "carrier-pigeon");
    assert!(text.contains("IGNORED"), "{text}");
}

#[test]
fn the_report_names_the_source_of_every_endpoint_and_hides_passwords_in_addresses() {
    let env = |name: &str| -> Option<String> {
        (name == "AZLIN_TOKEN_URL").then(|| String::from("http://ann:secret@127.0.0.1:8081"))
    };
    let s = Settings::resolve(&Flags::default(), &env, &os(), &no_file);
    let (json, text) = s.report(Some("http://127.0.0.1:9000"));
    assert_eq!(json["endpoints"]["token"]["kind"], "env");
    assert_eq!(
        json["endpoints"]["token"]["source"],
        "environment AZLIN_TOKEN_URL"
    );
    assert_eq!(json["endpoints"]["s3"]["kind"], "unset");
    assert_eq!(
        json["endpoints"]["s3"]["from_drive"],
        "http://127.0.0.1:9000"
    );
    assert_eq!(json["endpoints"]["meet"]["kind"], "profile");
    assert!(!text.contains("secret"), "{text}");
    assert!(!json.to_string().contains("secret"), "{json}");
    assert_eq!(
        redact_url("https://u:p@host.test:1/x"),
        "https://***@host.test:1/x"
    );
    assert_eq!(redact_url("http://host.test/a@b"), "http://host.test/a@b");
}

#[test]
fn a_file_section_is_read_through_the_loader_it_is_given() {
    let load = |path: &Path| -> (AzlinConfig, Option<String>) {
        assert_eq!(path, Path::new("/home/ann/.azlin/config.json"));
        let (c, problem) = AzlinConfig::parse(
            r#"{"endpoints": {"profile": "trial", "meet": "http://127.0.0.1:8787"}}"#,
        );
        (c, problem)
    };
    let s = Settings::resolve(&Flags::default(), &no_env, &os(), &load);
    assert_eq!(s.endpoints.profile, Profile::Trial);
    assert_eq!(
        s.endpoints.url(Endpoint::Token),
        Profile::Trial.default_for(Endpoint::Token),
        "the trial profile's own token server"
    );
    assert_eq!(
        s.endpoints.url(Endpoint::Meet),
        Some("http://127.0.0.1:8787")
    );
    assert!(EndpointsSection::default().is_empty());
}

#[test]
fn without_a_home_folder_there_is_no_state_folder_to_open() {
    let s = Settings::resolve(&Flags::default(), &no_env, &OsDirs::default(), &no_file);
    assert_eq!(s.state_dir.path, None);
    assert!(s.state_path().is_err());
    assert_eq!(s.config_file.path, None);
    assert!(s.token_url().is_ok(), "the profile still names a token server");
}
