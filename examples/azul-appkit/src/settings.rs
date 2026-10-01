//! An app's settings file (build ledger F6): `<app>/settings.json` in the
//! data root, so it travels with the user's data to the S3 bucket.
//!
//! Two settings every app has - the app theme (`flat` / `flora`) and the
//! mode (`system` / `light` / `dark`) - and the app's own as string values
//! under `values` (`"grouping": "true"`, `"angle": "deg"`). Reading is
//! forgiving: a missing or unknown field, a theme azul does not know or a
//! file that is not JSON falls back to the default rather than refusing to
//! start; writing is stable (sorted keys, pretty-printed) so the file diffs
//! cleanly under version control or a sync.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::args::{AppArgs, ModePref, Theme};

/// The settings file's name in the app's folder.
pub const SETTINGS_FILE: &str = "settings.json";

/// The file as stored.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
struct SettingsFile {
    #[serde(default)]
    theme: String,
    #[serde(default)]
    mode: String,
    #[serde(default)]
    values: BTreeMap<String, String>,
}

/// An app's settings.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AppSettings {
    /// The app theme.
    pub theme: Theme,
    /// Light, dark or the system's.
    pub mode: ModePref,
    /// The app's own settings.
    pub values: BTreeMap<String, String>,
}

impl AppSettings {
    /// Reads a settings file. Never fails: what cannot be read is the default,
    /// and the second value says what was wrong (for a log line), if anything.
    #[must_use]
    pub fn parse(json: &str) -> (AppSettings, Option<String>) {
        if json.trim().is_empty() {
            return (AppSettings::default(), None);
        }
        match serde_json::from_str::<SettingsFile>(json) {
            Ok(file) => {
                let mut problems = Vec::new();
                let theme = if file.theme.trim().is_empty() {
                    Theme::default()
                } else {
                    Theme::parse(&file.theme).unwrap_or_else(|| {
                        problems.push(format!("unknown theme {:?}", file.theme));
                        Theme::default()
                    })
                };
                let mode = if file.mode.trim().is_empty() {
                    ModePref::default()
                } else {
                    ModePref::parse(&file.mode).unwrap_or_else(|| {
                        problems.push(format!("unknown mode {:?}", file.mode));
                        ModePref::default()
                    })
                };
                let settings = AppSettings {
                    theme,
                    mode,
                    values: file.values,
                };
                let problem = (!problems.is_empty()).then(|| problems.join("; "));
                (settings, problem)
            }
            Err(e) => (
                AppSettings::default(),
                Some(format!("not a settings file: {e}")),
            ),
        }
    }

    /// The file's text: pretty-printed JSON with sorted keys and a final newline.
    #[must_use]
    pub fn to_json(&self) -> String {
        todo!("RED: to_json")
    }

    /// An app value.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(String::as_str)
    }

    /// Sets an app value.
    pub fn set(&mut self, key: &str, value: &str) {
        self.values.insert(key.to_string(), value.to_string());
    }

    /// A yes / no value: `true` / `false` (also `1` / `0`, `yes` / `no`);
    /// anything else, or no value, is `default`.
    #[must_use]
    pub fn get_bool(&self, key: &str, default: bool) -> bool {
        todo!("RED: get_bool")
    }

    /// Sets a yes / no value (`true` / `false`).
    pub fn set_bool(&mut self, key: &str, value: bool) {
        self.set(key, if value { "true" } else { "false" });
    }

    /// The theme and mode this run uses: a `--theme` / `--mode` switch wins
    /// over the file (for this run only; the file keeps its own).
    #[must_use]
    pub fn effective(&self, args: &AppArgs) -> (Theme, ModePref) {
        todo!("RED: effective")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_or_missing_file_is_the_default_without_a_complaint() {
        assert_eq!(AppSettings::parse(""), (AppSettings::default(), None));
        assert_eq!(AppSettings::parse("{}").0, AppSettings::default());
        assert_eq!(AppSettings::default().theme, Theme::Flat);
        assert_eq!(AppSettings::default().mode, ModePref::System);
    }

    #[test]
    fn the_file_round_trips_through_its_text() {
        let mut s = AppSettings {
            theme: Theme::Flora,
            mode: ModePref::Dark,
            ..AppSettings::default()
        };
        s.set("grouping", "true");
        s.set("angle", "deg");
        let text = s.to_json();
        assert!(text.ends_with('\n'));
        assert!(text.contains("\"theme\": \"flora\""), "{text}");
        assert!(
            text.find("\"angle\"").unwrap() < text.find("\"grouping\"").unwrap(),
            "sorted keys: {text}"
        );
        assert_eq!(AppSettings::parse(&text), (s, None));
    }

    #[test]
    fn a_bad_file_falls_back_to_the_default_and_says_why() {
        let (s, problem) = AppSettings::parse("this is not json");
        assert_eq!(s, AppSettings::default());
        assert!(problem.unwrap().contains("not a settings file"));
        let (s, problem) =
            AppSettings::parse(r#"{"theme": "neon", "mode": "dark", "values": {"a": "b"}}"#);
        assert_eq!(s.theme, Theme::Flat, "an unknown theme is the default one");
        assert_eq!(s.mode, ModePref::Dark, "the rest of the file still counts");
        assert_eq!(s.get("a"), Some("b"));
        assert!(problem.unwrap().contains("neon"));
    }

    #[test]
    fn unknown_fields_are_ignored_so_newer_files_still_open() {
        let (s, problem) = AppSettings::parse(r#"{"theme": "flora", "window": {"w": 3}}"#);
        assert_eq!(s.theme, Theme::Flora);
        assert_eq!(problem, None);
    }

    #[test]
    fn yes_no_values_read_the_usual_spellings() {
        let mut s = AppSettings::default();
        assert!(s.get_bool("grouping", true), "no value: the default");
        s.set("grouping", "no");
        assert!(!s.get_bool("grouping", true));
        s.set("grouping", "On");
        assert!(s.get_bool("grouping", false));
        s.set("grouping", "maybe");
        assert!(s.get_bool("grouping", true) && !s.get_bool("grouping", false));
        s.set_bool("grouping", false);
        assert_eq!(s.get("grouping"), Some("false"));
    }

    #[test]
    fn a_switch_wins_over_the_file_for_this_run() {
        let s = AppSettings {
            theme: Theme::Flora,
            mode: ModePref::Light,
            ..AppSettings::default()
        };
        let none = AppArgs::default();
        assert_eq!(s.effective(&none), (Theme::Flora, ModePref::Light));
        let args = AppArgs {
            mode: Some(ModePref::Dark),
            ..AppArgs::default()
        };
        assert_eq!(s.effective(&args), (Theme::Flora, ModePref::Dark));
    }
}
