//! The SHARED config of the Azlin apps: `~/.azlin/config.json`, one file for
//! every app a user runs, so the look they chose in one app is the look of
//! all of them ("one style config").
//!
//! ```json
//! {
//!   "currentTheme": "flora:green",
//!   "mode": "dark"
//! }
//! ```
//!
//! - `currentTheme`: the app theme - `flat`, `flora` or one of flora's spins
//!   (`flora:green`, `flora:red`, `flora:purple`, `flora:gold`,
//!   `flora:rose`; [`Theme`]). An app that pins its own theme (AzWriter
//!   flora, AzSheets flora:green, AzShow flora:red) neither reads nor writes
//!   it.
//! - `mode`: `system`, `light` or `dark` - shared by every app, pinned or not.
//! - Any other key is kept as it was: the file is the place for the next
//!   shared preference, and an older app must not drop a newer one's key.
//!
//! Every app reads it once, at its start (`ui::create_kit`, over its own
//! `settings.json`), and writes it whenever its theme or mode changes
//! (`ui::save_settings`). `AZLIN_CONFIG` names another file (a test's, a
//! screenshot run's); `AZLIN_CONFIG=off` (or empty) means no shared config at
//! all. `AZ_THEME` still outranks it for a run: azul resolves the environment
//! over the app's choice.
//!
//! Reading is forgiving like `settings.json`'s: a missing file is no
//! opinion, an unknown theme or mode is ignored (with a line saying so), a
//! file that is not JSON is left alone and reported. Writing goes through a
//! temporary file and a rename, so a crash never leaves half a config.
//!
//! No azul types here: tested without a window.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use crate::args::{ModePref, Theme};

/// The environment variable that names another config file (`off` or empty:
/// none).
pub const CONFIG_VAR: &str = "AZLIN_CONFIG";
/// The folder in the home directory.
pub const CONFIG_DIR: &str = ".azlin";
/// The file in it.
pub const CONFIG_FILE: &str = "config.json";
/// The key of the app theme.
pub const THEME_KEY: &str = "currentTheme";
/// The key of the light / dark mode.
pub const MODE_KEY: &str = "mode";

/// Where the shared config lives: `$AZLIN_CONFIG` if it is set (`None` when
/// it says `off` or nothing), else `<home>/.azlin/config.json`; `None`
/// without a home directory.
#[must_use]
pub fn config_path(var: Option<&str>, home: Option<&Path>) -> Option<PathBuf> {
    match var.map(str::trim) {
        Some(v) if v.is_empty() || v.eq_ignore_ascii_case("off") => None,
        Some(v) => Some(PathBuf::from(v)),
        None => home
            .filter(|h| !h.as_os_str().is_empty())
            .map(|h| h.join(CONFIG_DIR).join(CONFIG_FILE)),
    }
}

/// The shared config.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AzlinConfig {
    /// `currentTheme`; `None` = no opinion.
    pub theme: Option<Theme>,
    /// `mode`; `None` = no opinion.
    pub mode: Option<ModePref>,
    /// Every other key, as it was read.
    rest: Map<String, Value>,
}

impl AzlinConfig {
    /// Reads the config. Never fails: what cannot be read is no opinion, and
    /// the second value says what was wrong (for a log line), if anything.
    #[must_use]
    pub fn parse(json: &str) -> (AzlinConfig, Option<String>) {
        if json.trim().is_empty() {
            return (AzlinConfig::default(), None);
        }
        let mut rest = match serde_json::from_str::<Value>(json) {
            Ok(Value::Object(map)) => map,
            Ok(_) => {
                return (
                    AzlinConfig::default(),
                    Some("not a config: the top level is not an object".to_string()),
                )
            }
            Err(e) => return (AzlinConfig::default(), Some(format!("not a config: {e}"))),
        };
        let mut problems = Vec::new();
        let theme = match rest.remove(THEME_KEY) {
            Some(Value::String(name)) => Theme::parse(&name).or_else(|| {
                problems.push(format!("unknown theme {name:?}"));
                None
            }),
            Some(other) => {
                problems.push(format!("{THEME_KEY} is not a name: {other}"));
                None
            }
            None => None,
        };
        let mode = match rest.remove(MODE_KEY) {
            Some(Value::String(name)) => ModePref::parse(&name).or_else(|| {
                problems.push(format!("unknown mode {name:?}"));
                None
            }),
            Some(other) => {
                problems.push(format!("{MODE_KEY} is not a name: {other}"));
                None
            }
            None => None,
        };
        let problem = (!problems.is_empty()).then(|| problems.join("; "));
        (AzlinConfig { theme, mode, rest }, problem)
    }

    /// The file's text: pretty-printed JSON with sorted keys and a final
    /// newline (`serde_json`'s map is ordered by key).
    #[must_use]
    pub fn to_json(&self) -> String {
        let mut map = self.rest.clone();
        if let Some(theme) = self.theme {
            map.insert(THEME_KEY.to_string(), Value::String(theme.name().to_string()));
        }
        if let Some(mode) = self.mode {
            map.insert(MODE_KEY.to_string(), Value::String(mode.name().to_string()));
        }
        let mut text =
            serde_json::to_string_pretty(&Value::Object(map)).unwrap_or_else(|_| "{}".to_string());
        text.push('\n');
        text
    }

    /// Reads the config at `path`: a missing file is no opinion and no
    /// problem.
    #[must_use]
    pub fn load(path: &Path) -> (AzlinConfig, Option<String>) {
        match std::fs::read_to_string(path) {
            Ok(text) => AzlinConfig::parse(&text),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (AzlinConfig::default(), None),
            Err(e) => (AzlinConfig::default(), Some(format!("unreadable: {e}"))),
        }
    }

    /// Writes the config to `path`, creating its folder: through a temporary
    /// file beside it and a rename.
    ///
    /// # Errors
    ///
    /// The I/O error that stopped the write.
    pub fn store(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, self.to_json())?;
        std::fs::rename(&tmp, path)
    }

    /// The config at `path` with `theme` (unless `None`: a pinned app leaves
    /// the theme as it is) and `mode` written into it, every other key kept.
    /// What the file says now is read first, so two apps writing in turn
    /// never undo each other's other keys.
    ///
    /// # Errors
    ///
    /// The I/O error that stopped the write.
    pub fn update(path: &Path, theme: Option<Theme>, mode: ModePref) -> std::io::Result<()> {
        let (mut config, _problem) = AzlinConfig::load(path);
        let before = config.clone();
        if theme.is_some() {
            config.theme = theme;
        }
        config.mode = Some(mode);
        if config == before && path.exists() {
            return Ok(());
        }
        config.store(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_config_lives_in_the_home_folder_unless_the_environment_says_otherwise() {
        let home = Path::new("/home/ann");
        assert_eq!(
            config_path(None, Some(home)),
            Some(PathBuf::from("/home/ann/.azlin/config.json"))
        );
        assert_eq!(
            config_path(Some("/tmp/c.json"), Some(home)),
            Some(PathBuf::from("/tmp/c.json"))
        );
        assert_eq!(config_path(Some("off"), Some(home)), None);
        assert_eq!(config_path(Some(" "), Some(home)), None);
        assert_eq!(config_path(None, None), None);
    }

    #[test]
    fn a_config_reads_the_theme_and_the_mode_and_keeps_every_other_key() {
        let (c, problem) = AzlinConfig::parse(
            r#"{"currentTheme": "flora:green", "mode": "dark", "locale": "de"}"#,
        );
        assert_eq!(problem, None);
        assert_eq!(c.theme, Some(Theme::FloraGreen));
        assert_eq!(c.mode, Some(ModePref::Dark));
        let text = c.to_json();
        let (again, _) = AzlinConfig::parse(&text);
        assert_eq!(again, c, "it round-trips");
        assert!(text.contains("\"locale\": \"de\""), "{text}");
    }

    #[test]
    fn an_unknown_theme_is_no_opinion_and_is_reported() {
        let (c, problem) = AzlinConfig::parse(r#"{"currentTheme": "monokai"}"#);
        assert_eq!(c.theme, None);
        assert!(problem.unwrap().contains("monokai"));
        let (c, problem) = AzlinConfig::parse("not json");
        assert_eq!(c, AzlinConfig::default());
        assert!(problem.is_some());
    }

    #[test]
    fn an_update_writes_the_look_and_keeps_the_rest() {
        let dir = std::env::temp_dir().join(format!("azlin-config-test-{}", std::process::id()));
        let path = dir.join(".azlin").join("config.json");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, r#"{"locale": "de"}"#).unwrap();
        AzlinConfig::update(&path, Some(Theme::FloraRed), ModePref::Light).unwrap();
        let (c, _) = AzlinConfig::load(&path);
        assert_eq!(c.theme, Some(Theme::FloraRed));
        assert_eq!(c.mode, Some(ModePref::Light));
        assert!(c.to_json().contains("locale"));
        // A pinned app writes the mode only.
        AzlinConfig::update(&path, None, ModePref::Dark).unwrap();
        let (c, _) = AzlinConfig::load(&path);
        assert_eq!(c.theme, Some(Theme::FloraRed), "the pinned app left the theme");
        assert_eq!(c.mode, Some(ModePref::Dark));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
