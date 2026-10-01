//! AzMail's command line: which screen to open, the app theme and the mode, sample data, the
//! window size. Like AzWriter's `args.rs`.

/// The screen AzMail opens on.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Screen {
    /// The mail window (or the Add Account wizard when there is no account).
    #[default]
    Mail,
    /// File > Info.
    Backstage,
    /// File > Add Account (the wizard).
    AddAccount,
    /// File > Account Settings.
    Settings,
    /// The mail window with a New E-mail window over it.
    Compose,
    /// The mail window with a Reply to the newest message of the Inbox over it.
    Reply,
}

/// The app theme (`--theme`): flat or flora.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Theme {
    Flat,
    Flora,
}

impl Theme {
    /// The name `CallbackInfo::set_theme` / `AppConfig` take.
    pub fn name(self) -> &'static str {
        match self {
            Theme::Flat => "flat",
            Theme::Flora => "flora",
        }
    }
}

/// The mode (`--mode`): light or dark.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Light,
    Dark,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Args {
    pub screen: Screen,
    /// `None`: the app theme azul picks (flat).
    pub theme: Option<Theme>,
    /// `None`: the OS mode.
    pub mode: Option<Mode>,
    /// Fill the AzMail folder with the sample account and its mail (for screenshots and
    /// trying AzMail without an account).
    pub sample: bool,
    pub size: Option<(f32, f32)>,
}

pub const HELP: &str = "\
AzMail - a mail client on azul

USAGE:
    AzMail [OPTIONS]

OPTIONS:
    --screen <NAME>     mail | backstage | add-account | settings | compose | reply
    --theme <NAME>      flat | flora
    --mode <NAME>       light | dark
    --sample            Add the sample account and its mail (no network) and open it
    --size <WxH>        Initial window size, e.g. --size 1280x860
    -h, --help          Print this help

ENVIRONMENT:
    AZMAIL_DATA         The AzMail folder (default: AzMail in the user's data folder)
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
                "--screen" => {
                    let v = value("name")?;
                    a.screen = match v.as_str() {
                        "mail" => Screen::Mail,
                        "backstage" => Screen::Backstage,
                        "add-account" => Screen::AddAccount,
                        "settings" => Screen::Settings,
                        "compose" => Screen::Compose,
                        "reply" => Screen::Reply,
                        other => {
                            return Err(format!(
                                "--screen: expected mail|backstage|add-account|settings|compose|\
                                 reply, got {other:?}"
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
                "--sample" => a.sample = true,
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
                other => return Err(format!("unknown argument {other:?}\n\n{HELP}")),
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
    fn no_arguments_is_the_mail_window_in_the_systems_look() {
        let a = parse(&[]).unwrap();
        assert_eq!(a, Args::default());
        assert_eq!(a.screen, Screen::Mail);
        assert!(a.theme.is_none() && a.mode.is_none() && !a.sample && a.size.is_none());
    }

    #[test]
    fn every_screen_theme_and_mode_maps_by_name_in_both_spellings() {
        assert_eq!(parse(&["--screen", "compose"]).unwrap().screen, Screen::Compose);
        assert_eq!(parse(&["--screen=add-account"]).unwrap().screen, Screen::AddAccount);
        assert_eq!(parse(&["--screen", "reply"]).unwrap().screen, Screen::Reply);
        assert_eq!(parse(&["--screen", "settings"]).unwrap().screen, Screen::Settings);
        assert_eq!(parse(&["--screen", "backstage"]).unwrap().screen, Screen::Backstage);
        assert_eq!(parse(&["--theme", "flora"]).unwrap().theme, Some(Theme::Flora));
        assert_eq!(parse(&["--theme=flat"]).unwrap().theme, Some(Theme::Flat));
        assert_eq!(parse(&["--mode", "dark"]).unwrap().mode, Some(Mode::Dark));
        assert_eq!(parse(&["--sample"]).unwrap().sample, true);
        assert_eq!(parse(&["--size", "1280x860"]).unwrap().size, Some((1280.0, 860.0)));
        assert_eq!(Theme::Flora.name(), "flora");
    }

    #[test]
    fn a_bad_argument_is_rejected_rather_than_ignored() {
        for bad in [
            "--screen=inbox",
            "--theme=dark",
            "--mode=dim",
            "--size=wide",
            "--nonsense",
            "positional",
        ] {
            assert!(parse(&[bad]).is_err(), "{bad} must be rejected");
        }
        assert!(parse(&["--screen"]).is_err(), "a flag missing its value");
        assert!(parse(&["--help"]).unwrap_err().contains("USAGE"));
    }
}
