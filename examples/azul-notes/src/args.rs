//! The command line: azul-appkit's switches every Azlin app understands
//! (`--screen`, `--size`, `--theme`, `--mode`, `--shot`, `--sample`,
//! `--data-dir`; DEDUP_EDITORS B10: this file was the eighth copy of that
//! parser), plus AzNotes' own `--note <id>` (the note to open first).

use azul_appkit::args::AppArgs;

/// What the window shows first.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Screen {
    #[default]
    Notes,
    Settings,
    About,
    Shortcuts,
    History,
    Palette,
}

impl Screen {
    /// `--screen`'s names, the first the default.
    pub const NAMES: [&'static str; 6] = ["notes", "settings", "about", "shortcuts", "history", "palette"];

    /// The screen `--screen <name>` names (the notes for anything else).
    #[must_use]
    pub fn named(name: Option<&str>) -> Screen {
        match name {
            Some("settings") => Screen::Settings,
            Some("about") => Screen::About,
            Some("shortcuts") => Screen::Shortcuts,
            Some("history") => Screen::History,
            Some("palette") => Screen::Palette,
            _ => Screen::Notes,
        }
    }
}

/// The parsed command line.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Args {
    /// The kit's switches.
    pub app: AppArgs,
    /// The screen `--screen` names.
    pub screen: Screen,
    /// Open this note (an id) first.
    pub note: Option<String>,
}

pub type ParseError = String;

impl Args {
    /// `argv` without the program name. `Err` carries the message to print:
    /// the usage for `-h` / `--help`, else what was wrong.
    pub fn parse<I, S>(argv: I) -> Result<Self, ParseError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let argv: Vec<String> = argv.into_iter().map(Into::into).collect();
        let mut rest: Vec<String> = Vec::new();
        let mut note = None;
        let mut i = 0;
        while i < argv.len() {
            let arg = argv[i].as_str();
            if let Some(id) = arg.strip_prefix("--note=") {
                note = Some(id.to_string());
            } else if arg == "--note" {
                i += 1;
                let id = argv.get(i).ok_or_else(|| "--note needs a note id".to_string())?;
                note = Some(id.clone());
            } else {
                rest.push(argv[i].clone());
            }
            i += 1;
        }
        let app = AppArgs::parse(&crate::SPEC, rest)?;
        let screen = Screen::named(app.screen.as_deref());
        Ok(Args { app, screen, note })
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use azul_appkit::args::{ModePref, Theme};

    use super::*;

    fn parse(args: &[&str]) -> Result<Args, ParseError> {
        Args::parse(args.iter().copied())
    }

    #[test]
    fn no_arguments_is_the_notes_screen_in_the_default_folder() {
        let a = parse(&[]).unwrap();
        assert_eq!(a.screen, Screen::Notes);
        assert_eq!(a.app.data_dir, None);
        assert_eq!(a.note, None);
    }

    #[test]
    fn the_flags_parse_in_both_spellings() {
        let a = parse(&[
            "--data-dir=/tmp/n",
            "--sample",
            "--screen",
            "settings",
            "--theme",
            "flora",
            "--mode=dark",
            "--size",
            "900x600",
            "--note",
            "abc",
        ])
        .unwrap();
        assert_eq!(a.app.data_dir, Some(PathBuf::from("/tmp/n")));
        assert!(a.app.sample);
        assert_eq!(a.screen, Screen::Settings);
        assert_eq!(a.app.theme, Some(Theme::Flora));
        assert_eq!(a.app.mode, Some(ModePref::Dark));
        assert_eq!(a.app.size, Some((900.0, 600.0)));
        assert_eq!(a.note.as_deref(), Some("abc"));
        assert_eq!(parse(&["--note=xyz"]).unwrap().note.as_deref(), Some("xyz"));
    }

    #[test]
    fn a_bad_option_is_rejected_rather_than_ignored() {
        for bad in ["--screen=inbox", "--theme=native", "--mode=dim", "--size=big", "--nonsense", "x"] {
            assert!(parse(&[bad]).is_err(), "{bad} must be rejected");
        }
        assert!(parse(&["--data-dir"]).is_err(), "a flag missing its value");
        assert!(parse(&["--note"]).is_err(), "--note missing its id");
    }

    #[test]
    fn help_is_an_error_carrying_the_usage() {
        let e = parse(&["--help"]).unwrap_err();
        assert!(e.starts_with("AzNotes -") && e.contains("USAGE"));
    }
}
