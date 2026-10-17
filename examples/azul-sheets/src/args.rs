//! The command line: azul-appkit's switches, the one parser every Azlin app
//! shares (DEDUP_OFFICE D2) - `--screen`, `--size`, `--theme`, `--mode`
//! (`system` too), `--shot`, `--sample`, `--data-dir` and a bare `.xlsx` to
//! open - read into what AzSheets starts on.

use std::path::PathBuf;

use azul_appkit::{AppArgs, AppSpec};

/// The names `--screen` takes; the first is the default.
pub const SCREENS: [&str; 5] = [
    "workbook",
    "backstage-info",
    "backstage-new",
    "backstage-open",
    "settings",
];

/// What AzSheets tells the parser (and the usage text) about itself.
pub const SPEC: AppSpec = AppSpec {
    name: "AzSheets",
    binary: "AzSheets",
    summary: "a spreadsheet: IronCalc behind the CellGrid widget, workbooks as .xlsx files",
    screens: &SCREENS,
    files_help: "an .xlsx workbook to open",
};

/// The screen AzSheets starts on.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Screen {
    /// The workbook: ribbon, formula bar, grid, sheet tabs, status bar.
    #[default]
    Workbook,
    /// The backstage on "Info".
    BackstageInfo,
    /// The backstage on "New".
    BackstageNew,
    /// The backstage on "Open" (the workbooks in the data folder).
    BackstageOpen,
    /// The backstage on "Options" (the settings).
    Options,
}

/// The parsed command line.
#[derive(Clone, Debug, Default)]
pub struct Args {
    /// appkit's switches as given (the kit reads the theme, the mode, the
    /// size, the data folder and `--shot` from them).
    pub kit: AppArgs,
    pub screen: Screen,
    /// The `.xlsx` to import (the bare argument).
    pub open: Option<PathBuf>,
}

impl Args {
    /// Parses `argv` WITHOUT the program name. `Err` carries what to print:
    /// the usage for `-h` / `--help` (it contains "USAGE"), else the mistake.
    pub fn parse<I, S>(argv: I) -> Result<Self, String>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self::from_kit(AppArgs::parse(&SPEC, argv)?)
    }

    /// AzSheets' reading of appkit's switches.
    pub fn from_kit(kit: AppArgs) -> Result<Self, String> {
        if kit.files.len() > 1 {
            return Err(format!("one workbook at a time, not {:?}", kit.files));
        }
        let screen = match kit.screen_or_default(&SPEC) {
            "backstage-info" => Screen::BackstageInfo,
            "backstage-new" => Screen::BackstageNew,
            "backstage-open" => Screen::BackstageOpen,
            "settings" => Screen::Options,
            _ => Screen::Workbook,
        };
        Ok(Self {
            open: kit.files.first().cloned(),
            screen,
            kit,
        })
    }

    /// `--sample`: open the "Budget 2027" workbook.
    #[must_use]
    pub fn sample(&self) -> bool {
        self.kit.sample
    }

    /// `--size`.
    #[must_use]
    pub fn size(&self) -> Option<(f32, f32)> {
        self.kit.size
    }
}

#[cfg(test)]
mod tests {
    use azul_appkit::{ModePref, Theme};

    use super::*;

    fn parse(args: &[&str]) -> Result<Args, String> {
        Args::parse(args.iter().copied())
    }

    #[test]
    fn no_arguments_open_an_empty_workbook_in_the_saved_look() {
        let a = parse(&[]).unwrap();
        assert!(!a.sample());
        assert!(a.open.is_none());
        assert_eq!(a.screen, Screen::Workbook);
        assert!(a.kit.theme.is_none() && a.kit.mode.is_none(), "the settings file decides");
    }

    #[test]
    fn the_sample_screen_theme_mode_size_and_data_folder_switches_parse() {
        let a = parse(&[
            "--sample",
            "--screen",
            "backstage-open",
            "--theme=flora",
            "--mode",
            "dark",
            "--size",
            "1280x800",
            "--data-dir",
            "/tmp/azlin",
        ])
        .unwrap();
        assert!(a.sample());
        assert_eq!(a.screen, Screen::BackstageOpen);
        assert_eq!(a.kit.theme, Some(Theme::Flora));
        assert_eq!(a.kit.mode, Some(ModePref::Dark));
        assert_eq!(a.size(), Some((1280.0, 800.0)));
        assert_eq!(a.kit.data_dir, Some(PathBuf::from("/tmp/azlin")));
        assert_eq!(parse(&["--screen", "settings"]).unwrap().screen, Screen::Options);
    }

    #[test]
    fn a_bare_file_is_opened_and_a_second_one_is_an_error() {
        let a = parse(&["budget.xlsx"]).unwrap();
        assert_eq!(a.open, Some(PathBuf::from("budget.xlsx")));
        assert!(parse(&["a.xlsx", "b.xlsx"]).is_err());
    }

    #[test]
    fn bad_values_and_unknown_options_are_errors_and_help_is_its_own_answer() {
        assert!(parse(&["--theme", "neon"]).is_err());
        assert!(parse(&["--mode"]).is_err());
        assert!(parse(&["--size", "big"]).is_err());
        assert!(parse(&["--frobnicate"]).is_err());
        assert!(parse(&["--screen", "nowhere"]).is_err());
        assert!(parse(&["--help"]).unwrap_err().contains("USAGE"));
    }
}
