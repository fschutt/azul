//! The command line: what AzSheets opens and how it looks.

use std::path::PathBuf;

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
}

/// The app theme (`--theme`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Theme {
    Flat,
    Flora,
}

/// The mode (`--mode`); unset follows the system.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Light,
    Dark,
}

#[derive(Clone, Debug, Default)]
pub struct Args {
    /// Open the "Budget 2027" sample (`--sample`).
    pub sample: bool,
    /// Import this `.xlsx` from disk (`--open`, or the positional file).
    pub open: Option<PathBuf>,
    pub screen: Screen,
    pub theme: Option<Theme>,
    pub mode: Option<Mode>,
    /// The window size (`--size 1280x800`).
    pub size: Option<(f32, f32)>,
}

pub const HELP: &str = "\
azsheets - a spreadsheet (IronCalc behind the CellGrid widget)

USAGE:
    azsheets [OPTIONS] [FILE.xlsx]

OPTIONS:
    --sample                 Open the \"Budget 2027\" sample workbook
    --open <FILE>            Import an .xlsx file (same as the positional form)
    --screen <NAME>          workbook | backstage-info | backstage-new | backstage-open
    --theme <NAME>           flat | flora
    --mode <NAME>            light | dark
    --size <WxH>             Initial window size, e.g. --size 1280x800
    -h, --help               Print this help

ENVIRONMENT:
    AZSHEETS_DATA            The data folder (workbooks are sheets/<uuid>.xlsx in it)
";

pub type ParseError = String;

impl Args {
    pub fn parse<I, S>(argv: I) -> Result<Self, ParseError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut a = Self::default();
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
                "--sample" => a.sample = true,
                "--open" => a.open = Some(PathBuf::from(value("file")?)),
                "--screen" => {
                    let v = value("name")?;
                    a.screen = match v.as_str() {
                        "workbook" => Screen::Workbook,
                        "backstage-info" => Screen::BackstageInfo,
                        "backstage-new" => Screen::BackstageNew,
                        "backstage-open" => Screen::BackstageOpen,
                        other => {
                            return Err(format!(
                                "--screen: expected workbook|backstage-info|backstage-new|\
                                 backstage-open, got {other:?}"
                            ))
                        }
                    };
                }
                "--theme" => {
                    let v = value("name")?;
                    a.theme = Some(match v.as_str() {
                        "flat" => Theme::Flat,
                        "flora" => Theme::Flora,
                        other => return Err(format!("--theme: expected flat|flora, got {other:?}")),
                    });
                }
                "--mode" => {
                    let v = value("name")?;
                    a.mode = Some(match v.as_str() {
                        "light" => Mode::Light,
                        "dark" => Mode::Dark,
                        other => return Err(format!("--mode: expected light|dark, got {other:?}")),
                    });
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
                other if other.starts_with('-') => {
                    return Err(format!("unknown option {other:?}\n\n{HELP}"))
                }
                positional => {
                    if a.open.is_some() {
                        return Err(format!("more than one file given ({positional:?})"));
                    }
                    a.open = Some(PathBuf::from(positional));
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
    fn no_arguments_open_an_empty_workbook_in_the_system_look() {
        let a = parse(&[]).unwrap();
        assert!(!a.sample);
        assert!(a.open.is_none());
        assert_eq!(a.screen, Screen::Workbook);
        assert!(a.theme.is_none() && a.mode.is_none());
    }

    #[test]
    fn the_sample_screen_theme_mode_and_size_switches_parse() {
        let a = parse(&[
            "--sample",
            "--screen",
            "backstage-open",
            "--theme=flora",
            "--mode",
            "dark",
            "--size",
            "1280x800",
        ])
        .unwrap();
        assert!(a.sample);
        assert_eq!(a.screen, Screen::BackstageOpen);
        assert_eq!(a.theme, Some(Theme::Flora));
        assert_eq!(a.mode, Some(Mode::Dark));
        assert_eq!(a.size, Some((1280.0, 800.0)));
    }

    #[test]
    fn a_positional_file_is_opened_and_a_second_one_is_an_error() {
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
        assert!(parse(&["--help"]).unwrap_err().starts_with("azsheets -"));
    }
}
