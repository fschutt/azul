//! The command-line switches every Azlin app understands (build ledger F2).
//!
//! AzWriter's `args.rs` is the model: `--screen <name>` opens a screen
//! directly, `--size <WxH>` sizes the window, `--theme <flat|flora>` and
//! `--mode <light|dark|system>` pick the app theme and the light / dark mode
//! (they win over the settings file for this run and are not saved),
//! `--shot <PNG>` renders, writes a screenshot and exits (the screenshot
//! regression of F8), `--sample` fills an empty data folder with sample data,
//! `--data-dir <DIR>` points the app at another data root. Bare arguments
//! are files for the app (AzContacts imports `.vcf` files given this way).
//!
//! No azul types here: the parser is tested without a window.

use std::path::PathBuf;

/// The app theme: what a switch, the settings file and the settings page name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Theme {
    #[default]
    Flat,
    Flora,
}

impl Theme {
    /// Every theme, in the order the settings page lists them.
    pub const ALL: [Theme; 2] = [Theme::Flat, Theme::Flora];

    /// The name azul knows the theme by (`AppConfig::with_theme`,
    /// `CallbackInfo::set_theme`): `flat`, `flora`.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Theme::Flat => "flat",
            Theme::Flora => "flora",
        }
    }

    /// The label on the settings page.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Theme::Flat => "Flat",
            Theme::Flora => "Flora",
        }
    }

    /// A theme by name, any case, surrounding blanks ignored.
    #[must_use]
    pub fn parse(name: &str) -> Option<Theme> {
        todo!("RED: parse")
    }

    /// The position in [`Theme::ALL`].
    #[must_use]
    pub fn index(self) -> usize {
        Theme::ALL.iter().position(|t| *t == self).unwrap_or(0)
    }
}

/// Light, dark, or whatever the operating system uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ModePref {
    #[default]
    System,
    Light,
    Dark,
}

impl ModePref {
    /// Every choice, in the order the settings page lists them.
    pub const ALL: [ModePref; 3] = [ModePref::System, ModePref::Light, ModePref::Dark];

    /// `system`, `light`, `dark`.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            ModePref::System => "system",
            ModePref::Light => "light",
            ModePref::Dark => "dark",
        }
    }

    /// The label on the settings page.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            ModePref::System => "System",
            ModePref::Light => "Light",
            ModePref::Dark => "Dark",
        }
    }

    /// A choice by name, any case, surrounding blanks ignored.
    #[must_use]
    pub fn parse(name: &str) -> Option<ModePref> {
        todo!("RED: parse")
    }

    /// The position in [`ModePref::ALL`].
    #[must_use]
    pub fn index(self) -> usize {
        ModePref::ALL.iter().position(|m| *m == self).unwrap_or(0)
    }
}

/// What an app tells the parser about itself.
#[derive(Clone, Copy, Debug)]
pub struct AppSpec {
    /// The product name, `AzCalculator`.
    pub name: &'static str,
    /// The program name in the usage line.
    pub binary: &'static str,
    /// One line under the name.
    pub summary: &'static str,
    /// The names `--screen` accepts, the first is the default screen.
    pub screens: &'static [&'static str],
    /// What bare arguments mean, for the usage text ("" = none accepted).
    pub files_help: &'static str,
}

/// The parsed switches.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct AppArgs {
    /// `--screen`: one of [`AppSpec::screens`].
    pub screen: Option<String>,
    /// `--size WxH`, logical pixels.
    pub size: Option<(f32, f32)>,
    /// `--theme`.
    pub theme: Option<Theme>,
    /// `--mode`.
    pub mode: Option<ModePref>,
    /// `--shot`: the PNG to write before exiting.
    pub shot: Option<PathBuf>,
    /// `--shot-delay-ms`: settle time before the screenshot.
    pub shot_delay_ms: u64,
    /// `--sample`: fill an empty data folder with sample data.
    pub sample: bool,
    /// `--data-dir`: the data root instead of the user's.
    pub data_dir: Option<PathBuf>,
    /// Bare arguments.
    pub files: Vec<PathBuf>,
}

/// The settle time before `--shot` when `--shot-delay-ms` is not given.
pub const DEFAULT_SHOT_DELAY_MS: u64 = 1500;

/// The usage text of an app.
#[must_use]
pub fn help(spec: &AppSpec) -> String {
        todo!("RED: help")
    }

impl AppArgs {
    /// Parses `argv` WITHOUT the program name. `Err` carries the message to
    /// print: the usage for `-h` / `--help`, otherwise what was wrong.
    pub fn parse<I, S>(spec: &AppSpec, argv: I) -> Result<AppArgs, String>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        todo!("RED: parse")
    }

    /// The switches of this process (`std::env::args`, without the program name).
    pub fn from_env(spec: &AppSpec) -> Result<AppArgs, String> {
        AppArgs::parse(spec, std::env::args().skip(1))
    }

    /// The screen to open: `--screen`, else the app's first screen.
    #[must_use]
    pub fn screen_or_default<'a>(&'a self, spec: &'a AppSpec) -> &'a str {
        todo!("RED: screen_or_default")
    }
}

/// `WxH` in logical pixels, both positive.
fn parse_size(v: &str) -> Result<(f32, f32), String> {
        todo!("RED: parse_size")
    }

#[cfg(test)]
mod tests {
    use super::*;

    const SPEC: AppSpec = AppSpec {
        name: "AzTest",
        binary: "AzTest",
        summary: "a test app",
        screens: &["main", "settings", "about"],
        files_help: "files to open",
    };

    const NO_FILES: AppSpec = AppSpec {
        files_help: "",
        ..SPEC
    };

    fn parse(args: &[&str]) -> Result<AppArgs, String> {
        AppArgs::parse(&SPEC, args.iter().copied())
    }

    #[test]
    fn no_arguments_is_the_first_screen_with_the_saved_theme_and_mode() {
        let a = parse(&[]).unwrap();
        assert_eq!(a.screen, None);
        assert_eq!(a.screen_or_default(&SPEC), "main");
        assert_eq!(a.theme, None, "no switch: the settings file decides");
        assert_eq!(a.mode, None);
        assert!(!a.sample && a.shot.is_none() && a.data_dir.is_none() && a.files.is_empty());
        assert_eq!(a.shot_delay_ms, DEFAULT_SHOT_DELAY_MS);
    }

    #[test]
    fn both_spellings_of_a_valued_switch_work() {
        assert_eq!(
            parse(&["--size", "800x600"]).unwrap().size,
            Some((800.0, 600.0))
        );
        assert_eq!(
            parse(&["--size=800x600"]).unwrap().size,
            Some((800.0, 600.0))
        );
        assert_eq!(parse(&["--theme=flora"]).unwrap().theme, Some(Theme::Flora));
        assert_eq!(
            parse(&["--mode", "dark"]).unwrap().mode,
            Some(ModePref::Dark)
        );
        assert_eq!(parse(&["--shot-delay-ms=10"]).unwrap().shot_delay_ms, 10);
    }

    #[test]
    fn the_screen_must_be_one_of_the_apps_screens() {
        assert_eq!(
            parse(&["--screen", "Settings"]).unwrap().screen.as_deref(),
            Some("settings"),
            "any case, stored as the app spells it"
        );
        assert!(parse(&["--screen", "nope"]).is_err());
    }

    #[test]
    fn the_theme_and_the_mode_are_names_not_numbers() {
        assert_eq!(Theme::parse(" FLAT "), Some(Theme::Flat));
        assert_eq!(Theme::parse("native"), None);
        assert_eq!(ModePref::parse("System"), Some(ModePref::System));
        assert!(
            parse(&["--theme", "dark"]).is_err(),
            "dark is a mode, not a theme"
        );
        assert!(
            parse(&["--mode", "flora"]).is_err(),
            "flora is a theme, not a mode"
        );
        assert_eq!(Theme::Flora.name(), "flora");
        assert_eq!(ModePref::Light.name(), "light");
        assert_eq!(Theme::Flora.index(), 1);
        assert_eq!(ModePref::Dark.index(), 2);
    }

    #[test]
    fn sample_shot_and_data_dir_are_read() {
        let a = parse(&["--sample", "--shot", "out.png", "--data-dir", "/tmp/azlin"]).unwrap();
        assert!(a.sample);
        assert_eq!(a.shot, Some(PathBuf::from("out.png")));
        assert_eq!(a.data_dir, Some(PathBuf::from("/tmp/azlin")));
        assert!(parse(&["--sample=yes"]).is_err(), "a flag takes no value");
        assert!(
            parse(&["--data-dir="]).is_err(),
            "an empty folder is a mistake"
        );
    }

    #[test]
    fn bare_arguments_are_files_when_the_app_takes_files() {
        let a = parse(&["a.vcf", "--sample", "b.vcf"]).unwrap();
        assert_eq!(
            a.files,
            vec![PathBuf::from("a.vcf"), PathBuf::from("b.vcf")]
        );
        assert!(AppArgs::parse(&NO_FILES, ["a.vcf"]).is_err());
    }

    #[test]
    fn a_bad_switch_is_rejected_rather_than_ignored() {
        for bad in [
            "--sise=1x1",
            "--size=wide",
            "--size=0x10",
            "--size=-5x10",
            "--shot-delay-ms=soon",
            "--nonsense",
        ] {
            assert!(parse(&[bad]).is_err(), "{bad} must be rejected");
        }
        assert!(
            parse(&["--shot"]).is_err(),
            "a switch missing its value is an error"
        );
    }

    #[test]
    fn help_is_an_error_carrying_the_usage_with_the_screens() {
        for flag in ["-h", "--help"] {
            let e = parse(&[flag]).unwrap_err();
            assert!(e.contains("USAGE"), "{flag} must print the usage");
            assert!(e.contains("main | settings | about"));
            assert!(e.contains("[FILE...]"));
        }
        assert!(!help(&NO_FILES).contains("[FILE...]"));
    }
}
