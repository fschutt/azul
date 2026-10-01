//! The command line: `--screen <lobby|call|settings>`, `--theme <flat|flora>`,
//! `--mode <light|dark|system>`, `--name <name>`, `-h` / `--help`. The same switches as the other
//! azul apps, so a script (or a screenshot run) opens AzMeet where it wants, in the look it
//! wants. Pure: no azul types, unit-tested here.

/// Which screen opens first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Screen {
    /// The lobby: the camera preview, the devices, the name, new meeting / join.
    #[default]
    Lobby,
    /// The call view (the in-process demo opens it on its own).
    Call,
    /// The settings.
    Settings,
}

/// Light, dark, or the system's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    System,
    Light,
    Dark,
}

/// What the command line asked for.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Args {
    pub screen: Screen,
    /// The app theme ("flat", "flora"), or the default.
    pub theme: Option<String>,
    pub mode: Mode,
    /// The name others see (else `AZMEET_NAME`, else the login name).
    pub name: Option<String>,
    pub help: bool,
}

pub const HELP: &str = "\
AzMeet - video meetings over azul.iroh

USAGE:
    AzMeet [OPTIONS]

OPTIONS:
    --screen <NAME>    lobby | call | settings
    --theme <NAME>     flat | flora
    --mode <NAME>      light | dark | system
    --name <NAME>      the name others see
    -h, --help         print this help

The environment variables are listed in the crate documentation (AZMEET_WORKER, AZMEET_JOIN,
AZMEET_AUTOCREATE, AZMEET_TEST_TONE, AZMEET_TEST_PATTERN, ...).
";

/// Reads `argv` (without the program name). `--key value` and `--key=value` both work.
pub fn parse<I, S>(argv: I) -> Result<Args, String>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let argv: Vec<String> = argv.into_iter().map(Into::into).collect();
    let mut args = Args::default();
    let mut i = 0;
    while i < argv.len() {
        let (name, inline) = match argv[i].split_once('=') {
            Some((name, value)) if name.starts_with("--") => (name.to_string(), Some(value.to_string())),
            _ => (argv[i].clone(), None),
        };
        let mut value = || -> Result<String, String> {
            if let Some(value) = inline.clone() {
                return Ok(value);
            }
            i += 1;
            argv.get(i).cloned().ok_or_else(|| format!("{name} needs a value"))
        };
        match name.as_str() {
            "--screen" => {
                args.screen = match value()?.as_str() {
                    "lobby" => Screen::Lobby,
                    "call" => Screen::Call,
                    "settings" => Screen::Settings,
                    other => {
                        return Err(format!("--screen {other}: lobby, call or settings"));
                    }
                }
            }
            "--theme" => {
                let theme = value()?;
                if !matches!(theme.as_str(), "flat" | "flora") {
                    return Err(format!("--theme {theme}: flat or flora"));
                }
                args.theme = Some(theme);
            }
            "--mode" => {
                args.mode = match value()?.as_str() {
                    "light" => Mode::Light,
                    "dark" => Mode::Dark,
                    "system" => Mode::System,
                    other => return Err(format!("--mode {other}: light, dark or system")),
                }
            }
            "--name" => args.name = Some(value()?),
            "-h" | "--help" => args.help = true,
            other => return Err(format!("{other}: not an AzMeet option (see --help)")),
        }
        i += 1;
    }
    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_opens_the_lobby_in_the_default_look() {
        assert_eq!(parse(Vec::<String>::new()), Ok(Args::default()));
    }

    #[test]
    fn every_switch_is_read_in_both_forms() {
        let args = parse(["--screen", "settings", "--theme=flora", "--mode", "dark", "--name=Ada"])
            .expect("valid");
        assert_eq!(
            args,
            Args {
                screen: Screen::Settings,
                theme: Some(String::from("flora")),
                mode: Mode::Dark,
                name: Some(String::from("Ada")),
                help: false,
            }
        );
        assert_eq!(parse(["--screen=call"]).map(|a| a.screen), Ok(Screen::Call));
        assert_eq!(parse(["--mode", "light"]).map(|a| a.mode), Ok(Mode::Light));
        assert_eq!(parse(["-h"]).map(|a| a.help), Ok(true));
    }

    #[test]
    fn an_unknown_value_or_switch_or_a_missing_value_says_what_is_wrong() {
        assert!(parse(["--screen", "nowhere"]).unwrap_err().contains("--screen"));
        assert!(parse(["--theme", "neon"]).unwrap_err().contains("flat"));
        assert!(parse(["--mode"]).unwrap_err().contains("--mode"));
        assert!(parse(["--fly"]).unwrap_err().contains("--fly"));
    }
}
