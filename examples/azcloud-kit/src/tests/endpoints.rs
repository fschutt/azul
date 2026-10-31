//! Where the token server is: the flag, the environment, the shared config file, the profile.

use std::collections::HashMap;

use crate::endpoints::token_endpoint;

/// A folder of its own under the system's temporary folder, removed when dropped.
struct Folder(std::path::PathBuf);

impl Folder {
    fn new(what: &str) -> Folder {
        let path = std::env::temp_dir().join(format!(
            "azcloud-kit-test-{what}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Folder(path)
    }
}

impl Drop for Folder {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn env_of(pairs: &[(&str, String)]) -> impl Fn(&str) -> Option<String> {
    let map: HashMap<String, String> = pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect();
    move |var: &str| map.get(var).cloned()
}

#[test]
fn without_any_setting_the_token_server_is_the_local_profiles() {
    let env = env_of(&[("AZLIN_CONFIG", "off".to_string())]);
    let found = token_endpoint(None, None, &env, None);
    assert_eq!(found.url.as_deref(), Some("http://127.0.0.1:8081"));
    assert!(found.development, "the local profile's server is a development one");
    assert!(found.source.contains("profile"), "{}", found.source);
}

#[test]
fn the_config_file_the_environment_and_the_flag_each_win_over_the_one_before() {
    let folder = Folder::new("endpoints");
    let config = folder.0.join("config.json");
    std::fs::write(
        &config,
        r#"{"endpoints": {"profile": "production", "token": "https://token.example"}}"#,
    )
    .unwrap();
    let path = config.to_string_lossy().into_owned();

    let env = env_of(&[("AZLIN_CONFIG", path.clone())]);
    let found = token_endpoint(None, None, &env, None);
    assert_eq!(found.url.as_deref(), Some("https://token.example"));
    assert!(!found.development, "a production server sells drives, it has no test drives");
    assert!(found.source.contains("config file"), "{}", found.source);

    let env = env_of(&[
        ("AZLIN_CONFIG", path.clone()),
        ("AZLIN_TOKEN_URL", "http://127.0.0.1:18081".to_string()),
    ]);
    let found = token_endpoint(None, None, &env, None);
    assert_eq!(found.url.as_deref(), Some("http://127.0.0.1:18081"));
    assert!(found.development, "a token server on this computer is a development one");

    let found = token_endpoint(Some("http://localhost:28081"), None, &env, None);
    assert_eq!(found.url.as_deref(), Some("http://localhost:28081"));
    assert!(found.source.contains("--token-url"), "{}", found.source);
}

#[test]
fn a_profile_flag_picks_the_profiles_server() {
    let env = env_of(&[("AZLIN_CONFIG", "off".to_string())]);
    let found = token_endpoint(None, Some("production"), &env, None);
    assert!(found.url.as_deref().is_some_and(|u| u.starts_with("https://")));
    assert!(!found.development);
}
