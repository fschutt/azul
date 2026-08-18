//! Process-wide snapshot of the app-level configuration that ENGINE services
//! (updater, system dialogs) need outside any callback: the app's name and
//! version, where the update manifest lives, the changelog URL, the support
//! mailbox.
//!
//! `AppConfig` is the source of truth; `App::run` publishes it here once at
//! startup (same pattern as `window::set_global_system_animations`). Layout
//! examples and tests that never go through `App::run` call [`set_app_env`]
//! directly.

use std::sync::RwLock;

use azul_core::resources::UpdateMode;

/// The published snapshot. All fields plain owned data — this is read from
/// worker threads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppEnv {
    /// Directory-safe app name (updater state dir, problem-report dir).
    pub app_name: String,
    /// The RUNNING version, the compare target for update checks.
    pub current_version: String,
    /// Requested update behaviour (clamped by install kind at check time).
    pub update_mode: UpdateMode,
    /// Update manifest URL; `None` = checks report a configuration error.
    pub update_manifest: Option<String>,
    /// App changelog (Markdown) URL, the `UpdateVersion` dialog's fallback when
    /// a release has no changelog link of its own.
    pub changelog_md: Option<String>,
    /// Support mailbox for problem reports; `None` = reports save to disk.
    pub report_problem: Option<String>,
}

impl Default for AppEnv {
    fn default() -> Self {
        Self {
            app_name: "azul-app".to_owned(),
            current_version: "0.0.0".to_owned(),
            update_mode: UpdateMode::NotifyOnly,
            update_manifest: None,
            changelog_md: None,
            report_problem: None,
        }
    }
}

impl AppEnv {
    /// The snapshot an [`azul_core::resources::AppConfig`] describes.
    #[must_use]
    pub fn from_config(config: &azul_core::resources::AppConfig) -> Self {
        let opt = |s: &azul_css::OptionString| {
            s.as_ref().map(|v| v.as_str().to_owned()).filter(|v| !v.is_empty())
        };
        Self {
            app_name: config.updates.app_name.as_str().to_owned(),
            current_version: config.updates.current_version.as_str().to_owned(),
            update_mode: config.updates.mode,
            update_manifest: opt(&config.updates.manifest_url),
            changelog_md: opt(&config.changelog_md),
            report_problem: match &config.report_problem {
                azul_core::resources::OptionEmailAddress::Some(e) => {
                    let a = e.address.as_str();
                    if a.is_empty() { None } else { Some(a.to_owned()) }
                }
                azul_core::resources::OptionEmailAddress::None => None,
            },
        }
    }
}

static APP_ENV: RwLock<Option<AppEnv>> = RwLock::new(None);

/// Publishes the snapshot (called by `App::run`; re-callable — tests and
/// multi-`App` processes overwrite the previous value).
pub fn set_app_env(env: AppEnv) {
    if let Ok(mut slot) = APP_ENV.write() {
        *slot = Some(env);
    }
}

/// The current snapshot; a default (no manifest, no mailbox) when nothing
/// was published.
#[must_use]
pub fn app_env() -> AppEnv {
    APP_ENV
        .read()
        .ok()
        .and_then(|slot| slot.clone())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_round_trips_into_the_env() {
        let mut config = azul_core::resources::AppConfig::create();
        config.updates.app_name = "testapp".into();
        config.updates.current_version = "1.2.3".into();
        config.updates.mode = UpdateMode::SelfUpdate;
        config.updates.manifest_url =
            azul_css::OptionString::Some("http://localhost:1/manifest.json".into());
        config.changelog_md = azul_css::OptionString::Some("http://localhost:1/CHANGELOG.md".into());
        config.report_problem = azul_core::resources::OptionEmailAddress::Some(
            azul_core::resources::EmailAddress::new("support@example.test".into()),
        );
        let env = AppEnv::from_config(&config);
        assert_eq!(env.app_name, "testapp");
        assert_eq!(env.current_version, "1.2.3");
        assert_eq!(env.update_mode, UpdateMode::SelfUpdate);
        assert_eq!(env.update_manifest.as_deref(), Some("http://localhost:1/manifest.json"));
        assert_eq!(env.changelog_md.as_deref(), Some("http://localhost:1/CHANGELOG.md"));
        assert_eq!(env.report_problem.as_deref(), Some("support@example.test"));
    }

    #[test]
    fn empty_strings_read_as_unset_not_as_empty_urls() {
        // An empty manifest URL must NOT produce Some("") — the check would
        // then try to fetch "" instead of reporting "not configured".
        let config = azul_core::resources::AppConfig::create();
        let env = AppEnv::from_config(&config);
        assert_eq!(env.update_manifest, None);
        assert_eq!(env.changelog_md, None);
        assert_eq!(env.report_problem, None);
    }
}
