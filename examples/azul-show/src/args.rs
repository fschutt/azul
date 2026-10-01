//! The command line: which screen to start on, the app theme and the mode,
//! the sample deck, a deck to open, the window size, a screenshot to take.

use std::path::PathBuf;

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
}

impl StartScreen {
    /// The names `--screen` takes.
    pub const NAMES: &'static str = "normal|sorter|outline|notes|backstage-new|backstage-open|show";

    fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "normal" => Self::Normal,
            "sorter" => Self::Sorter,
            "outline" => Self::Outline,
            "notes" => Self::Notes,
            "backstage-new" => Self::BackstageNew,
            "backstage-open" => Self::BackstageOpen,
            "show" => Self::Show,
            _ => return None,
        })
    }
}

/// The parsed command line.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Args {
    pub screen: StartScreen,
    /// `flat` or `flora`; `None`: the app's default.
    pub theme: Option<String>,
    /// `light` or `dark`; `None`: follow the OS.
    pub mode: Option<String>,
    /// Start on the sample deck ("Azlin Workspace").
    pub sample: bool,
    /// A deck id under `show/` to open.
    pub open: Option<String>,
    /// The slide to start on, 1-based.
    pub slide: Option<usize>,
    /// The initial window size.
    pub size: Option<(f32, f32)>,
    /// Render, write this screenshot, exit.
    pub shot: Option<PathBuf>,
    pub shot_delay_ms: u64,
    /// A show without the presenter window.
    pub no_presenter: bool,
}

const DEFAULT_SHOT_DELAY_MS: u64 = 2500;

pub const HELP: &str = "\
azshow - a PowerPoint-style presentation editor

USAGE:
    AzShow [OPTIONS] [DECK-ID]

OPTIONS:
    --screen <NAME>          normal | sorter | outline | notes | backstage-new | backstage-open | show
    --theme <NAME>           flat | flora
    --mode <NAME>            light | dark
    --sample                 Open the sample deck (\"Azlin Workspace\")
    --open <DECK-ID>         Open show/<DECK-ID>/deck.json (same as the positional form)
    --slide <N>              Start on slide N (1-based)
    --size <WxH>             Initial window size, e.g. --size 1280x800
    --shot <PNG>             Render, write this screenshot, exit
    --shot-delay-ms <MS>     Settle time before --shot (default 2500)
    --no-presenter           Run the show without the presenter window
    -h, --help               Print this help

ENVIRONMENT:
    AZSHOW_DATA              The data root (default: <the user's data folder>/azul)
";

pub type ParseError = String;

impl Args {
    pub fn parse<I, S>(argv: I) -> Result<Self, ParseError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut a = Self {
            shot_delay_ms: DEFAULT_SHOT_DELAY_MS,
            ..Self::default()
        };
        let argv: Vec<String> = argv.into_iter().map(Into::into).collect();
        let mut i = 0;
        while i < argv.len() {
            let arg = argv[i].as_str();
            let (name, inline) = match arg.split_once('=') {
                Some((n, v)) if n.starts_with("--") => (n, Some(v.to_string())),
                _ => (arg, None),
            };
            let mut value = |what: &str| -> Result<String, ParseError> {
                if let Some(v) = inline.clone() {
                    return Ok(v);
                }
                i += 1;
                argv.get(i)
                    .cloned()
                    .ok_or_else(|| format!("{name} needs a {what}"))
            };
            match name {
                "-h" | "--help" => return Err(HELP.to_string()),
                "--screen" => {
                    let v = value("name")?;
                    a.screen = StartScreen::from_name(&v).ok_or_else(|| {
                        format!("--screen: expected {}, got {v:?}", StartScreen::NAMES)
                    })?;
                }
                "--theme" => {
                    let v = value("name")?;
                    if v != "flat" && v != "flora" {
                        return Err(format!("--theme: expected flat|flora, got {v:?}"));
                    }
                    a.theme = Some(v);
                }
                "--mode" => {
                    let v = value("name")?;
                    if v != "light" && v != "dark" {
                        return Err(format!("--mode: expected light|dark, got {v:?}"));
                    }
                    a.mode = Some(v);
                }
                "--sample" => a.sample = true,
                "--open" => a.open = Some(value("deck id")?),
                "--slide" => {
                    let v = value("number")?;
                    match v.parse::<usize>() {
                        Ok(n) if n >= 1 => a.slide = Some(n),
                        _ => return Err(format!("--slide: expected a slide number from 1, got {v:?}")),
                    }
                }
                "--size" => {
                    let v = value("WxH")?;
                    let (w, h) = v
                        .split_once('x')
                        .ok_or_else(|| format!("--size: expected WxH, got {v:?}"))?;
                    match (w.parse::<f32>(), h.parse::<f32>()) {
                        (Ok(w), Ok(h)) if w > 0.0 && h > 0.0 => a.size = Some((w, h)),
                        _ => return Err(format!("--size: expected WxH in pixels, got {v:?}")),
                    }
                }
                "--shot" => a.shot = Some(PathBuf::from(value("path")?)),
                "--shot-delay-ms" => {
                    let v = value("number")?;
                    a.shot_delay_ms = v
                        .parse()
                        .map_err(|_| format!("--shot-delay-ms: expected a number, got {v:?}"))?;
                }
                "--no-presenter" => a.no_presenter = true,
                other if other.starts_with('-') => {
                    return Err(format!("unknown option {other:?}\n\n{HELP}"))
                }
                positional => {
                    if a.open.is_some() {
                        return Err(format!("more than one deck given ({positional:?})"));
                    }
                    a.open = Some(positional.to_string());
                }
            }
            i += 1;
        }
        Ok(a)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Args, ParseError> {
        Args::parse(args.iter().copied())
    }

    #[test]
    fn no_arguments_is_the_normal_view_following_the_os() {
        let a = parse(&[]).expect("parses");
        assert_eq!(a.screen, StartScreen::Normal);
        assert!(a.theme.is_none() && a.mode.is_none() && !a.sample && a.open.is_none());
        assert_eq!(a.shot_delay_ms, DEFAULT_SHOT_DELAY_MS);
    }

    #[test]
    fn the_screens_the_theme_and_the_mode_map_by_name() {
        let a = parse(&["--screen", "sorter", "--theme=flora", "--mode", "dark", "--sample"]).expect("parses");
        assert_eq!(a.screen, StartScreen::Sorter);
        assert_eq!(a.theme.as_deref(), Some("flora"));
        assert_eq!(a.mode.as_deref(), Some("dark"));
        assert!(a.sample);
        for (name, screen) in [
            ("normal", StartScreen::Normal),
            ("outline", StartScreen::Outline),
            ("notes", StartScreen::Notes),
            ("backstage-new", StartScreen::BackstageNew),
            ("backstage-open", StartScreen::BackstageOpen),
            ("show", StartScreen::Show),
        ] {
            assert_eq!(parse(&["--screen", name]).expect("parses").screen, screen);
        }
    }

    #[test]
    fn a_bare_word_is_the_deck_to_open_and_the_slide_is_one_based() {
        let a = parse(&["6f1c", "--slide", "3"]).expect("parses");
        assert_eq!(a.open.as_deref(), Some("6f1c"));
        assert_eq!(a.slide, Some(3));
        assert!(parse(&["a", "b"]).is_err());
        assert!(parse(&["--slide", "0"]).is_err());
    }

    #[test]
    fn a_bad_option_is_rejected_rather_than_ignored() {
        for bad in ["--theme=dark", "--mode=flora", "--screen=editor", "--size=wide", "--nonsense"] {
            assert!(parse(&[bad]).is_err(), "{bad} must be rejected");
        }
        assert!(parse(&["--shot"]).is_err());
        assert!(parse(&["--help"]).expect_err("help").contains("USAGE"));
    }
}
