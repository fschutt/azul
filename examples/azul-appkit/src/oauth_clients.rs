//! The OAuth clients of the apps' sign-ins (AzDrive's Google Drive, Dropbox, OneDrive), from
//! the shared Azlin config ([`crate::azlin_config`]) and the environment.
//!
//! ```json
//! {
//!   "oauth": {
//!     "google": {
//!       "client_id": "123-abc.apps.googleusercontent.com",
//!       "client_secret": "GOCSPX-...",
//!       "redirect_uri": "http://127.0.0.1/oauth2redirect"
//!     },
//!     "dropbox": { "client_id": "abcdefghijklmno" },
//!     "onedrive": { "client_id": "00000000-0000-0000-0000-000000000000" }
//!   }
//! }
//! ```
//!
//! An OAuth client id is CONFIGURATION, never built into an app: every distributor registers
//! its own at each provider (Google Cloud console, Dropbox App Console, Microsoft Entra), and a
//! build without one says which setting is missing. Per provider: `client_id`, `client_secret`
//! (only Google's desktop clients have one; in an app anybody can download it is no secret, but
//! Google's token endpoint wants it), `redirect_uri` (the default is a loopback one), and for
//! tests `authorize_url`, `token_url` and `scope`.
//!
//! The environment outranks the file ([`resolve`]): `<PREFIX>_<PROVIDER>_<KEY>`, AzDrive's
//! `AZDRIVE_GOOGLE_CLIENT_ID`, `AZDRIVE_DROPBOX_CLIENT_ID`, `AZDRIVE_ONEDRIVE_CLIENT_ID`, ...
//! ([`env_var`]). Read through the config's own reader and its JSON text, like
//! [`crate::shared_endpoint`]: a missing section, provider or key, a blank or a non-string
//! value is no opinion. No azul types here: tested without a window.

use std::{fmt, path::Path};

use serde_json::Value;

use crate::azlin_config::AzlinConfig;

/// The section's key in the shared config.
pub const SECTION: &str = "oauth";

/// The settings of one provider, by their keys in the section (and in the variables' names).
pub const KEYS: [&str; 6] = [
    "client_id",
    "client_secret",
    "redirect_uri",
    "authorize_url",
    "token_url",
    "scope",
];

/// One provider's settings; `None` is no opinion. `Debug` hides the client secret.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct OAuthSettings {
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
    pub redirect_uri: Option<String>,
    pub authorize_url: Option<String>,
    pub token_url: Option<String>,
    pub scope: Option<String>,
}

impl fmt::Debug for OAuthSettings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OAuthSettings")
            .field("client_id", &self.client_id)
            .field(
                "client_secret",
                &self.client_secret.as_ref().map(|_| "<hidden>"),
            )
            .field("redirect_uri", &self.redirect_uri)
            .field("authorize_url", &self.authorize_url)
            .field("token_url", &self.token_url)
            .field("scope", &self.scope)
            .finish()
    }
}

impl OAuthSettings {
    /// The setting `key` (one of [`KEYS`]).
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&str> {
        match key {
            "client_id" => self.client_id.as_deref(),
            "client_secret" => self.client_secret.as_deref(),
            "redirect_uri" => self.redirect_uri.as_deref(),
            "authorize_url" => self.authorize_url.as_deref(),
            "token_url" => self.token_url.as_deref(),
            "scope" => self.scope.as_deref(),
            _ => None,
        }
    }

    /// Sets `key` (one of [`KEYS`]) to `value`, trimmed; a blank one is no opinion.
    pub fn set(&mut self, key: &str, value: Option<&str>) {
        let _ = (key, value);
    }

    /// `self` with every setting `over` has an opinion on replaced.
    #[must_use]
    pub fn overlaid(self, over: OAuthSettings) -> OAuthSettings {
        let _ = over;
        self
    }
}

/// `oauth.<provider>` of `config`.
#[must_use]
pub fn of(config: &AzlinConfig, provider: &str) -> OAuthSettings {
    let _ = (config, provider);
    OAuthSettings::default()
}

/// `oauth.<provider>` of the config file at `path` (the kit's `Kit::config_path`; none in a
/// `--shot` run).
#[must_use]
pub fn in_file(path: &Path, provider: &str) -> OAuthSettings {
    let (config, _problem) = AzlinConfig::load(path);
    of(&config, provider)
}

/// The variable of `key` for `provider` under `prefix`: `AZDRIVE_GOOGLE_CLIENT_ID`.
#[must_use]
pub fn env_var(prefix: &str, provider: &str, key: &str) -> String {
    let _ = (prefix, provider, key);
    String::new()
}

/// `provider`'s settings from the variables `var` reads ([`env_var`] names them).
#[must_use]
pub fn from_env(
    prefix: &str,
    provider: &str,
    var: &dyn Fn(&str) -> Option<String>,
) -> OAuthSettings {
    let _ = (prefix, provider, var);
    OAuthSettings::default()
}

/// `provider`'s settings as an app weighs them: the environment (`prefix`'s variables) over
/// the shared config file at `config_path` (none: no file).
#[must_use]
pub fn resolve(
    prefix: &str,
    provider: &str,
    var: &dyn Fn(&str) -> Option<String>,
    config_path: Option<&Path>,
) -> OAuthSettings {
    let file = config_path.map_or_else(OAuthSettings::default, |path| in_file(path, provider));
    file.overlaid(from_env(prefix, provider, var))
}

#[cfg(test)]
mod tests {
    use azul_storage::testing::TempDir;

    use super::*;

    const FILE: &str = r#"{
        "mode": "dark",
        "oauth": {
            "google": {
                "client_id": " 123-abc.apps.googleusercontent.com ",
                "client_secret": "GOCSPX-file",
                "redirect_uri": "http://127.0.0.1/oauth2redirect",
                "scope": "",
                "token_url": 5
            },
            "dropbox": { "client_id": "dbx-file" }
        }
    }"#;

    fn env(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
        move |name| {
            pairs
                .iter()
                .find(|(k, _)| *k == name)
                .map(|(_, v)| (*v).to_string())
        }
    }

    #[test]
    fn a_providers_settings_are_read_from_the_oauth_section_trimmed() {
        let (config, problem) = AzlinConfig::parse(FILE);
        assert_eq!(problem, None);
        let google = of(&config, "google");
        assert_eq!(
            google.client_id.as_deref(),
            Some("123-abc.apps.googleusercontent.com")
        );
        assert_eq!(google.client_secret.as_deref(), Some("GOCSPX-file"));
        assert_eq!(
            google.redirect_uri.as_deref(),
            Some("http://127.0.0.1/oauth2redirect")
        );
        assert_eq!(google.scope, None, "a blank value is no opinion");
        assert_eq!(google.token_url, None, "a number is no opinion");
        assert_eq!(
            of(&config, "dropbox").client_id.as_deref(),
            Some("dbx-file")
        );
        assert_eq!(of(&config, "onedrive"), OAuthSettings::default());
        assert_eq!(
            of(&AzlinConfig::default(), "google"),
            OAuthSettings::default()
        );
    }

    #[test]
    fn the_variables_are_named_after_the_app_the_provider_and_the_key() {
        assert_eq!(
            env_var("AZDRIVE", "google", "client_id"),
            "AZDRIVE_GOOGLE_CLIENT_ID"
        );
        assert_eq!(
            env_var("AZDRIVE", "onedrive", "token_url"),
            "AZDRIVE_ONEDRIVE_TOKEN_URL"
        );
    }

    #[test]
    fn the_environment_outranks_the_file_setting_by_setting() {
        let dir = TempDir::new("oauth-clients");
        let path = dir.0.join("config.json");
        std::fs::write(&path, FILE).unwrap();
        let var = env(&[
            ("AZDRIVE_GOOGLE_CLIENT_ID", "env-id"),
            (
                "AZDRIVE_GOOGLE_TOKEN_URL",
                "http://127.0.0.1:8081/oauth/google/token",
            ),
            ("AZDRIVE_GOOGLE_SCOPE", "  "),
        ]);
        let google = resolve("AZDRIVE", "google", &var, Some(&path));
        assert_eq!(google.client_id.as_deref(), Some("env-id"));
        assert_eq!(google.client_secret.as_deref(), Some("GOCSPX-file"), "kept");
        assert_eq!(
            google.token_url.as_deref(),
            Some("http://127.0.0.1:8081/oauth/google/token")
        );
        assert_eq!(google.scope, None, "a blank variable is no opinion");
        let none = resolve("AZDRIVE", "onedrive", &env(&[]), None);
        assert_eq!(none, OAuthSettings::default());
    }

    #[test]
    fn the_debug_output_hides_the_client_secret() {
        let mut settings = OAuthSettings::default();
        settings.set("client_secret", Some("GOCSPX-hidden"));
        settings.set("client_id", Some("visible"));
        let shown = format!("{settings:?}");
        assert!(!shown.contains("GOCSPX"), "{shown}");
        assert!(shown.contains("visible"), "{shown}");
        assert_eq!(settings.get("client_secret"), Some("GOCSPX-hidden"));
    }
}
