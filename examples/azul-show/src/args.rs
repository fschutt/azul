//! The command line: azul-appkit's switches, the one parser every Azlin app
//! shares (DEDUP_OFFICE D2) - `--screen`, `--size`, `--theme`, `--mode`
//! (`system` too), `--shot`, `--sample`, `--data-dir`, a bare deck id to
//! open - plus AzShow's own two: `--slide <n>` and `--no-presenter`.

use azul_appkit::{AppArgs, AppSpec};

/// The names `--screen` takes; the first is the default.
pub const SCREENS: [&str; 8] = [
    "normal",
    "sorter",
    "outline",
    "notes",
    "backstage-new",
    "backstage-open",
    "show",
    "settings",
];

/// What AzShow tells the parser (and the usage text) about itself.
pub const SPEC: AppSpec = AppSpec {
    name: "AzShow",
    binary: "AzShow",
    summary: "a presentation editor: decks as show/<id>/deck.json files",
    screens: &SCREENS,
    files_help: "the id of a deck under show/ to open",
};

/// The screen AzShow starts on.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum StartScreen {
    /// The normal view: rail, slide, notes.
    #[default]
    Normal,
    /// The slide sorter.
    Sorter,
    /// The outline view.
    Outline,
    /// The notes page view.
    Notes,
    /// File > New (the theme picker).
    BackstageNew,
    /// File > Open.
    BackstageOpen,
    /// The slide show (and the presenter window, unless `--no-presenter`).
    Show,
    /// File > Options (the settings).
    Options,
}

/// The parsed command line.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Args {
    /// appkit's switches as given (the kit reads the theme, the mode, the
    /// size, the data folder and `--shot` from them).
    pub kit: AppArgs,
    pub screen: StartScreen,
    /// A deck id under `show/` to open (the bare argument).
    pub open: Option<String>,
    /// The slide to start on, 1-based (`--slide`).
    pub slide: Option<usize>,
    /// A show without the presenter window (`--no-presenter`).
    pub no_presenter: bool,
}

impl Args {
    /// Parses `argv` WITHOUT the program name. `Err` carries what to print:
    /// the usage for `-h` / `--help` (it contains "USAGE"), else the mistake.
    pub fn parse<I, S>(argv: I) -> Result<Self, String>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        // AzShow's own switches first: appkit rejects what it does not know.
        let argv: Vec<String> = argv.into_iter().map(Into::into).collect();
        let mut rest = Vec::with_capacity(argv.len());
        let mut slide = None;
        let mut no_presenter = false;
        let mut i = 0;
        while i < argv.len() {
            let arg = argv[i].as_str();
            match arg.split_once('=') {
                _ if arg == "--no-presenter" => no_presenter = true,
                Some(("--slide", v)) => slide = Some(slide_number(v)?),
                _ if arg == "--slide" => {
                    i += 1;
                    let v = argv.get(i).ok_or_else(|| String::from("--slide needs a number"))?;
                    slide = Some(slide_number(v)?);
                }
                _ => rest.push(argv[i].clone()),
            }
            i += 1;
        }
        let kit = AppArgs::parse(&SPEC, rest)?;
        if kit.files.len() > 1 {
            return Err(format!("one deck at a time, not {:?}", kit.files));
        }
        let screen = match kit.screen_or_default(&SPEC) {
            "sorter" => StartScreen::Sorter,
            "outline" => StartScreen::Outline,
            "notes" => StartScreen::Notes,
            "backstage-new" => StartScreen::BackstageNew,
            "backstage-open" => StartScreen::BackstageOpen,
            "show" => StartScreen::Show,
            "settings" => StartScreen::Options,
            _ => StartScreen::Normal,
        };
        Ok(Self {
            open: kit.files.first().map(|p| p.to_string_lossy().into_owned()),
            screen,
            slide,
            no_presenter,
            kit,
        })
    }

    /// `--sample`: start on the sample deck.
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

/// A slide number from 1.
fn slide_number(v: &str) -> Result<usize, String> {
    match v.trim().parse::<usize>() {
        Ok(n) if n >= 1 => Ok(n),
        _ => Err(format!("--slide: expected a slide number from 1, got {v:?}")),
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
    fn no_arguments_is_the_normal_view_in_the_saved_look() {
        let a = parse(&[]).expect("parses");
        assert_eq!(a.screen, StartScreen::Normal);
        assert!(a.kit.theme.is_none() && a.kit.mode.is_none() && !a.sample() && a.open.is_none());
        assert!(!a.no_presenter && a.slide.is_none());
    }

    #[test]
    fn the_screens_the_theme_and_the_mode_map_by_name() {
        let a = parse(&["--screen", "sorter", "--theme=flora", "--mode", "dark", "--sample"]).expect("parses");
        assert_eq!(a.screen, StartScreen::Sorter);
        assert_eq!(a.kit.theme, Some(Theme::Flora));
        assert_eq!(a.kit.mode, Some(ModePref::Dark));
        assert!(a.sample());
        for (name, screen) in [
            ("normal", StartScreen::Normal),
            ("outline", StartScreen::Outline),
            ("notes", StartScreen::Notes),
            ("backstage-new", StartScreen::BackstageNew),
            ("backstage-open", StartScreen::BackstageOpen),
            ("show", StartScreen::Show),
            ("settings", StartScreen::Options),
        ] {
            assert_eq!(parse(&["--screen", name]).expect("parses").screen, screen);
        }
    }

    #[test]
    fn a_bare_word_is_the_deck_to_open_and_the_slide_is_one_based() {
        let a = parse(&["6f1c", "--slide", "3", "--no-presenter"]).expect("parses");
        assert_eq!(a.open.as_deref(), Some("6f1c"));
        assert_eq!(a.slide, Some(3));
        assert!(a.no_presenter);
        assert_eq!(parse(&["--slide=2"]).expect("parses").slide, Some(2));
        assert!(parse(&["a", "b"]).is_err());
        assert!(parse(&["--slide", "0"]).is_err());
        assert!(parse(&["--slide"]).is_err());
    }

    #[test]
    fn a_bad_option_is_rejected_rather_than_ignored() {
        for bad in ["--theme=dark", "--mode=flora", "--screen=editor", "--size=wide", "--nonsense"] {
            assert!(parse(&[bad]).is_err(), "{bad} must be rejected");
        }
        assert!(parse(&["--shot"]).is_err());
        assert!(parse(&["--help"]).expect_err("help").contains("USAGE"));
        assert_eq!(
            parse(&["--data-dir", "/tmp/azlin"]).expect("parses").kit.data_dir,
            Some(std::path::PathBuf::from("/tmp/azlin"))
        );
    }
}
