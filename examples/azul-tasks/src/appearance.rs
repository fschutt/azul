//! The theme and the mode, kept across restarts in `<data root>/aztasks/settings.json` -
//! azul-appkit's settings file, the one every Azlin app keeps its appearance in (the task
//! store's `tasks/settings.json` is shared with the other apps' To-Do bars and holds no
//! appearance), and the language of the words. `--theme` / `--mode` / `--language` win for one
//! run; the file keeps its own.
//!
//! The file is read once at start (before the window, not in a callback) and written through
//! the task write queue (on the file thread) when the Appearance settings change.

use azul_appkit::{
    args::{LanguagePref, ModePref, Theme},
    data::app_key,
    settings::{AppSettings, SETTINGS_FILE},
};

use crate::args::{Args, Mode};

/// AzTasks' own folder in the data root (its tasks are in the shared `tasks/`).
pub const APP_FOLDER: &str = "aztasks";

/// The settings file's key.
#[must_use]
pub fn settings_key() -> String {
    app_key(APP_FOLDER, SETTINGS_FILE)
}

/// The theme and mode this run shows: the command line's, else the file's.
#[must_use]
pub fn effective(args: &Args, saved: &AppSettings) -> (Theme, ModePref) {
    let theme = args
        .theme
        .as_deref()
        .and_then(Theme::parse)
        .unwrap_or(saved.theme);
    let mode = match args.mode {
        Some(Mode::Light) => ModePref::Light,
        Some(Mode::Dark) => ModePref::Dark,
        Some(Mode::System) => ModePref::System,
        None => saved.mode,
    };
    (theme, mode)
}

/// The language of the words this run: the command line's, else the file's.
#[must_use]
pub fn language(args: &Args, saved: &AppSettings) -> LanguagePref {
    args.language.unwrap_or_else(|| saved.language())
}

/// The Mode control's segment of `mode`: System, Light, Dark.
#[must_use]
pub fn mode_index(mode: ModePref) -> usize {
    match mode {
        ModePref::System => 0,
        ModePref::Light => 1,
        ModePref::Dark => 2,
    }
}

/// The mode of the Mode control's segment `index` (System for one that is not there).
#[must_use]
pub fn mode_of_index(index: usize) -> ModePref {
    match index {
        1 => ModePref::Light,
        2 => ModePref::Dark,
        _ => ModePref::System,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_command_line_wins_over_the_file_for_one_run() {
        let mut saved = AppSettings::default();
        saved.theme = Theme::Flora;
        saved.mode = ModePref::Dark;
        assert_eq!(effective(&Args::default(), &saved), (Theme::Flora, ModePref::Dark));
        let args = Args {
            theme: Some("flat".to_string()),
            mode: Some(Mode::Light),
            ..Args::default()
        };
        assert_eq!(effective(&args, &saved), (Theme::Flat, ModePref::Light));
        let system = Args {
            mode: Some(Mode::System),
            ..Args::default()
        };
        assert_eq!(effective(&system, &saved), (Theme::Flora, ModePref::System));
        // A first run: flat, following the system.
        assert_eq!(
            effective(&Args::default(), &AppSettings::default()),
            (Theme::Flat, ModePref::System)
        );
    }

    #[test]
    fn the_mode_control_reads_back_its_own_segments() {
        for mode in [ModePref::System, ModePref::Light, ModePref::Dark] {
            assert_eq!(mode_of_index(mode_index(mode)), mode);
        }
        assert_eq!(mode_index(ModePref::System), 0);
        assert_eq!(mode_of_index(7), ModePref::System);
    }

    #[test]
    fn the_appearance_is_kept_in_the_apps_own_folder() {
        assert_eq!(settings_key(), "aztasks/settings.json");
        let mut saved = AppSettings::default();
        saved.theme = Theme::Flora;
        saved.mode = ModePref::Dark;
        assert_eq!(AppSettings::parse(&saved.to_json()).0, saved);
    }
}
