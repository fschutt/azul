//! One service's address from the shared Azlin config ([`crate::azlin_config`]):
//! its `endpoints` section, one string per service.
//!
//! ```json
//! {
//!   "endpoints": {
//!     "token": "http://127.0.0.1:8081",
//!     "s3": "http://127.0.0.1:9000",
//!     "meet": "http://127.0.0.1:8790",
//!     "relay": "http://127.0.0.1:3340",
//!     "tiles": "http://127.0.0.1:8099/tiles/{z}/{x}/{y}.pbf"
//!   }
//! }
//! ```
//!
//! An app weighs the value where its own order says (its switch and its
//! variable outrank the shared file, a built-in default comes last): the
//! meeting server of AzCalendar, the tile server of AzMaps, the endpoint a new
//! S3 drive of AzDrive starts with. `AZLIN_CONFIG=<file>` names another config
//! (the local stack's profile); the file is found as the kit finds it
//! ([`crate::azlin_config::config_path`]).
//!
//! Read through the config's own reader and its JSON text, so whatever else
//! the file says is left to it; a missing section, a missing name, a blank or
//! a non-string value is no opinion. No azul types here: tested without a
//! window.

use std::path::Path;

use serde_json::Value;

use crate::azlin_config::{self, AzlinConfig};

/// The section's key in the shared config.
pub const SECTION: &str = "endpoints";

/// `endpoints.<name>` of `config`, trimmed; `None` when it names none.
#[must_use]
pub fn of(config: &AzlinConfig, name: &str) -> Option<String> {
    let json: Value = serde_json::from_str(&config.to_json()).ok()?;
    json.get(SECTION)?
        .get(name)?
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
}

/// `endpoints.<name>` of the shared config the kit reads: `var` is
/// `AZLIN_CONFIG` (`off` or blank: none), else `<home>/.azlin/config.json`.
/// `None` without a config or without the name.
#[must_use]
pub fn read(var: Option<&str>, home: Option<&Path>, name: &str) -> Option<String> {
    in_file(&azlin_config::config_path(var, home)?, name)
}

/// `endpoints.<name>` of the config file at `path` (the kit's
/// `Kit::config_path`, which a `--shot` run leaves empty).
#[must_use]
pub fn in_file(path: &Path, name: &str) -> Option<String> {
    let (config, _problem) = AzlinConfig::load(path);
    of(&config, name)
}

#[cfg(test)]
mod tests {
    use azul_storage::testing::TempDir;

    use super::*;

    #[test]
    fn a_service_address_is_read_from_the_endpoints_section() {
        let (config, problem) = AzlinConfig::parse(
            r#"{"mode": "dark", "endpoints": {"meet": " http://127.0.0.1:8790 ", "s3": "http://127.0.0.1:9000"}}"#,
        );
        assert_eq!(problem, None);
        assert_eq!(
            of(&config, "meet").as_deref(),
            Some("http://127.0.0.1:8790")
        );
        assert_eq!(of(&config, "s3").as_deref(), Some("http://127.0.0.1:9000"));
        assert_eq!(of(&config, "relay"), None, "a name the section lacks");
    }

    #[test]
    fn a_blank_or_non_string_value_or_no_section_is_no_opinion() {
        let (config, _) =
            AzlinConfig::parse(r#"{"endpoints": {"meet": "  ", "s3": 9000, "token": null}}"#);
        assert_eq!(of(&config, "meet"), None);
        assert_eq!(of(&config, "s3"), None);
        assert_eq!(of(&config, "token"), None);
        let (config, _) = AzlinConfig::parse(r#"{"endpoints": "http://127.0.0.1:8790"}"#);
        assert_eq!(of(&config, "meet"), None);
        assert_eq!(of(&AzlinConfig::default(), "meet"), None);
    }

    #[test]
    fn the_file_azlin_config_names_is_the_one_read_and_off_reads_none() {
        let dir = TempDir::new("shared-endpoint");
        let path = dir.0.join("local.json");
        std::fs::write(
            &path,
            r#"{"endpoints": {"token": "http://127.0.0.1:8081"}}"#,
        )
        .unwrap();
        let var = path.to_string_lossy().to_string();
        assert_eq!(
            read(Some(&var), None, "token").as_deref(),
            Some("http://127.0.0.1:8081")
        );
        assert_eq!(read(Some("off"), Some(&dir.0), "token"), None);
        assert_eq!(read(None, None, "token"), None, "no home, no config");
        let home = dir.0.join("home");
        std::fs::create_dir_all(home.join(".azlin")).unwrap();
        std::fs::write(
            home.join(".azlin").join("config.json"),
            r#"{"endpoints": {"meet": "https://meet.example.test"}}"#,
        )
        .unwrap();
        assert_eq!(
            read(None, Some(&home), "meet").as_deref(),
            Some("https://meet.example.test")
        );
    }
}
