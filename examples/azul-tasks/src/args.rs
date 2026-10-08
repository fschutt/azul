//! The command line (AzWriter's `args.rs` is the model).

use std::path::PathBuf;

use crate::views::View;

/// What the window shows first.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Screen {
    /// The three panes.
    #[default]
    Main,
    /// FILE > Settings.
    Settings,
    /// FILE > Keyboard shortcuts.
    Shortcuts,
    /// FILE > About.
    About,
    /// The command palette open over the main window.
    Palette,
}

impl Screen {
    pub const ALL: [(Screen, &'static str); 5] = [
        (Screen::Main, "main"),
        (Screen::Settings, "settings"),
        (Screen::Shortcuts, "shortcuts"),
        (Screen::About, "about"),
        (Screen::Palette, "palette"),
    ];
}

/// Light or dark, or whatever the OS says.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Light,
    Dark,
    System,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Args {
    /// Fill an empty data folder with the sample lists and tasks.
    pub sample: bool,
    /// The data folder (the root of the `tasks/` files).
    pub data: Option<PathBuf>,
    pub screen: Screen,
    /// `flat` or `flora`.
    pub theme: Option<String>,
    pub mode: Option<Mode>,
    /// The list shown first.
    pub view: Option<View>,
    pub size: Option<(f32, f32)>,
}

pub const HELP: &str = "\
AzTasks - to-dos and reminders, one file per task

USAGE:
    AzTasks [OPTIONS]

OPTIONS:
    --sample                 Fill an empty data folder with sample lists and tasks
    --data <DIR>             The data folder (default: AZTASKS_DATA, else AZLIN_DATA, else
                             <data dir>/Azlin)
    --screen <NAME>          main | settings | shortcuts | about | palette
    --theme <NAME>           flat | flora
    --mode <NAME>            light | dark | system
    --view <NAME>            today | upcoming | scheduled | flagged | all | completed |
                             list:<id> | tag:<tag> | search:<words>
    --size <WxH>             Initial window size, e.g. --size 1280x800
    -h, --help               Print this help
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
                "--data" => a.data = Some(PathBuf::from(value("folder")?)),
                "--screen" => {
                    let v = value("name")?;
                    a.screen = Screen::ALL
                        .iter()
                        .find(|(_, n)| *n == v)
                        .map(|(s, _)| *s)
                        .ok_or_else(|| {
                            format!("--screen: expected main|settings|shortcuts|about|palette, got {v:?}")
                        })?;
                }
                "--theme" => {
                    let v = value("name")?;
                    if !matches!(v.as_str(), "flat" | "flora") {
                        return Err(format!("--theme: expected flat|flora, got {v:?}"));
                    }
                    a.theme = Some(v);
                }
                "--mode" => {
                    let v = value("name")?;
                    a.mode = Some(match v.as_str() {
                        "light" => Mode::Light,
                        "dark" => Mode::Dark,
                        "system" => Mode::System,
                        other => return Err(format!("--mode: expected light|dark|system, got {other:?}")),
                    });
                }
                "--view" => {
                    let v = value("name")?;
                    a.view = Some(View::from_name(&v).ok_or_else(|| format!("--view: unknown view {v:?}"))?);
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
                other => return Err(format!("unknown option {other:?}\n\n{HELP}")),
            }
            i += 1;
        }
        Ok(a)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::views::Smart;

    fn parse(args: &[&str]) -> Result<Args, ParseError> {
        Args::parse(args.iter().copied())
    }

    #[test]
    fn no_arguments_is_the_main_window_without_sample_data() {
        let a = parse(&[]).unwrap();
        assert_eq!(a, Args::default());
        assert_eq!(a.screen, Screen::Main);
        assert!(!a.sample);
    }

    #[test]
    fn every_switch_is_read_in_both_spellings() {
        let a = parse(&[
            "--sample",
            "--data",
            "/tmp/x",
            "--screen=settings",
            "--theme",
            "flora",
            "--mode=dark",
            "--view",
            "upcoming",
            "--size=1280x800",
        ])
        .unwrap();
        assert!(a.sample);
        assert_eq!(a.data, Some(PathBuf::from("/tmp/x")));
        assert_eq!(a.screen, Screen::Settings);
        assert_eq!(a.theme.as_deref(), Some("flora"));
        assert_eq!(a.mode, Some(Mode::Dark));
        assert_eq!(a.view, Some(View::Smart(Smart::Upcoming)));
        assert_eq!(a.size, Some((1280.0, 800.0)));
    }

    #[test]
    fn a_bad_switch_is_refused_rather_than_ignored() {
        for bad in [
            &["--screen=backstage"][..],
            &["--theme", "aero"],
            &["--mode=dim"],
            &["--view=tomorrow"],
            &["--size=wide"],
            &["--data"],
            &["--nonsense"],
            &["notes.txt"],
        ] {
            assert!(parse(bad).is_err(), "{bad:?} must be refused");
        }
        assert_eq!(parse(&["--help"]).unwrap_err(), HELP);
    }
}
