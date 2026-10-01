//! The command line: where the notes live, the sample library, the screen,
//! theme and mode to start in (AzWriter's `args.rs` is the model).

use std::path::PathBuf;

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
    pub const NAMES: [(&'static str, Screen); 6] = [
        ("notes", Screen::Notes),
        ("settings", Screen::Settings),
        ("about", Screen::About),
        ("shortcuts", Screen::Shortcuts),
        ("history", Screen::History),
        ("palette", Screen::Palette),
    ];
}

/// The parsed command line.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Args {
    /// The AzNotes folder (`--data`; else `AZNOTES_DATA`; else `AzNotes` in
    /// the user's data folder).
    pub data: Option<PathBuf>,
    /// Write the sample library first (files that exist are kept).
    pub sample: bool,
    pub screen: Screen,
    /// `flat` / `flora`.
    pub theme: Option<String>,
    /// `light` / `dark`.
    pub mode: Option<String>,
    pub size: Option<(f32, f32)>,
    /// Open this note (an id) first.
    pub note: Option<String>,
}

pub type ParseError = String;

pub const HELP: &str = "\
aznotes - notes as Markdown files, with a rich-text editor

USAGE:
    aznotes [OPTIONS]

OPTIONS:
    --data <DIR>         The AzNotes folder (default: AZNOTES_DATA, else AzNotes in the
                         user's data folder); notes live in <DIR>/notes/
    --sample             Write the sample notebooks and notes first (keeps existing files)
    --screen <NAME>      notes | settings | about | shortcuts | history | palette
    --theme <NAME>       flat | flora
    --mode <NAME>        light | dark
    --size <WxH>         Initial window size, e.g. --size 1200x760
    --note <ID>          Open this note first
    -h, --help           Print this help
";

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
                "--data" => a.data = Some(PathBuf::from(value("folder")?)),
                "--sample" => a.sample = true,
                "--screen" => {
                    let v = value("name")?;
                    a.screen = Screen::NAMES
                        .iter()
                        .find(|(n, _)| *n == v)
                        .map(|(_, s)| *s)
                        .ok_or_else(|| {
                            format!(
                                "--screen: expected notes|settings|about|shortcuts|history|palette, \
                                 got {v:?}"
                            )
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
                    if !matches!(v.as_str(), "light" | "dark") {
                        return Err(format!("--mode: expected light|dark, got {v:?}"));
                    }
                    a.mode = Some(v);
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
                "--note" => a.note = Some(value("note id")?),
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

    fn parse(args: &[&str]) -> Result<Args, ParseError> {
        Args::parse(args.iter().copied())
    }

    #[test]
    fn no_arguments_is_the_notes_screen_in_the_default_folder() {
        assert_eq!(parse(&[]).unwrap(), Args::default());
    }

    #[test]
    fn the_flags_parse_in_both_spellings() {
        let a = parse(&[
            "--data=/tmp/n",
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
        assert_eq!(a.data, Some(PathBuf::from("/tmp/n")));
        assert!(a.sample);
        assert_eq!(a.screen, Screen::Settings);
        assert_eq!(a.theme.as_deref(), Some("flora"));
        assert_eq!(a.mode.as_deref(), Some("dark"));
        assert_eq!(a.size, Some((900.0, 600.0)));
        assert_eq!(a.note.as_deref(), Some("abc"));
    }

    #[test]
    fn a_bad_option_is_rejected_rather_than_ignored() {
        for bad in ["--screen=inbox", "--theme=native", "--mode=dim", "--size=big", "--nonsense", "x"] {
            assert!(parse(&[bad]).is_err(), "{bad} must be rejected");
        }
        assert!(parse(&["--data"]).is_err(), "a flag missing its value");
    }

    #[test]
    fn help_is_an_error_carrying_the_usage() {
        let e = parse(&["--help"]).unwrap_err();
        assert!(e.starts_with("aznotes -") && e.contains("USAGE"));
    }
}
