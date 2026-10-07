//! The command line: the switches every Azlin app understands (azul-appkit's `--screen`,
//! `--size`, `--theme <flat|flora>`, `--mode <system|light|dark>`, `--shot`, `--sample`,
//! `--data-dir`) with AzMeet's screens (`lobby | waiting | call | settings`), plus its own
//! `--name <name>`;
//! `-h` / `--help`. A script (or a screenshot run) opens AzMeet where it wants, in the look it
//! wants. Pure: no azul types (appkit's plain modules only), unit-tested here.

use azul_appkit::{args::help, AppArgs, AppSpec};

/// Which screen opens first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Screen {
    /// The start screen: new meeting, join with a link, the meeting server.
    #[default]
    Lobby,
    /// A new meeting's waiting room: the camera preview, the switches, the devices, the name,
    /// the link, "Start meeting".
    Waiting,
    /// The call view (the in-process demo opens it on its own).
    Call,
    /// The settings.
    Settings,
}

/// What azul-appkit is told about AzMeet.
pub const SPEC: AppSpec = AppSpec {
    name: "AzMeet",
    binary: "AzMeet",
    summary: "video meetings over azul.iroh",
    screens: &["lobby", "waiting", "call", "settings"],
    files_help: "",
};

/// What the command line asked for.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Args {
    pub screen: Screen,
    /// The name others see (else `AZMEET_NAME`, else the login name).
    pub name: Option<String>,
    pub help: bool,
    /// The Azlin switches (azul-appkit): `--theme`, `--mode` (`None`: the saved settings
    /// decide), `--size`, `--shot`, `--sample`, `--data-dir`.
    pub kit: AppArgs,
}

/// The usage text: appkit's, AzMeet's `--name`, and where the environment variables are.
#[must_use]
pub fn usage() -> String {
    let mut text = help(&SPEC);
    text.push_str(
        "\nAZMEET:\n    --name <NAME>            the name others see\n\nThe environment variables \
         are listed in the crate documentation (AZMEET_WORKER, AZMEET_JOIN,\nAZMEET_AUTOCREATE, \
         AZMEET_TEST_TONE, AZMEET_TEST_PATTERN, ...).\n",
    );
    text
}

/// Reads `argv` (without the program name). `--key value` and `--key=value` both work.
pub fn parse<I, S>(argv: I) -> Result<Args, String>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let argv: Vec<String> = argv.into_iter().map(Into::into).collect();
    if argv.iter().any(|a| a == "-h" || a == "--help") {
        return Ok(Args {
            help: true,
            ..Args::default()
        });
    }
    let mut name = None;
    let mut rest = Vec::with_capacity(argv.len());
    let mut i = 0;
    while i < argv.len() {
        let arg = argv[i].as_str();
        if arg == "--name" {
            i += 1;
            name = Some(
                argv.get(i)
                    .cloned()
                    .ok_or_else(|| String::from("--name needs a value"))?,
            );
        } else if let Some(value) = arg.strip_prefix("--name=") {
            name = Some(value.to_string());
        } else {
            rest.push(argv[i].clone());
        }
        i += 1;
    }
    let kit = AppArgs::parse(&SPEC, rest)?;
    let screen = match kit.screen.as_deref() {
        Some("waiting") => Screen::Waiting,
        Some("call") => Screen::Call,
        Some("settings") => Screen::Settings,
        _ => Screen::Lobby,
    };
    Ok(Args {
        screen,
        name,
        help: false,
        kit,
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use azul_appkit::{ModePref, Theme};

    use super::*;

    #[test]
    fn nothing_opens_the_lobby_in_the_saved_look() {
        let args = parse(Vec::<String>::new()).expect("valid");
        assert_eq!(args.screen, Screen::Lobby);
        assert_eq!(args.kit.theme, None, "the settings file decides");
        assert_eq!(args.kit.mode, None);
        assert_eq!(args.name, None);
        assert!(!args.help);
    }

    #[test]
    fn every_switch_is_read_in_both_forms() {
        let args = parse([
            "--screen",
            "settings",
            "--theme=flora",
            "--mode",
            "dark",
            "--name=Ada",
            "--data-dir",
            "/tmp/azlin",
        ])
        .expect("valid");
        assert_eq!(args.screen, Screen::Settings);
        assert_eq!(args.kit.theme, Some(Theme::Flora));
        assert_eq!(args.kit.mode, Some(ModePref::Dark));
        assert_eq!(args.name.as_deref(), Some("Ada"));
        assert_eq!(args.kit.data_dir, Some(PathBuf::from("/tmp/azlin")));
        assert_eq!(parse(["--screen=call"]).map(|a| a.screen), Ok(Screen::Call));
        assert_eq!(parse(["--screen", "waiting"]).map(|a| a.screen), Ok(Screen::Waiting));
        assert_eq!(parse(["--mode", "system"]).map(|a| a.kit.mode), Ok(Some(ModePref::System)));
        assert_eq!(parse(["--name", "Ben"]).map(|a| a.name), Ok(Some(String::from("Ben"))));
        assert_eq!(parse(["-h"]).map(|a| a.help), Ok(true));
        assert!(usage().contains("--name") && usage().contains("--data-dir"));
    }

    #[test]
    fn an_unknown_value_or_switch_or_a_missing_value_says_what_is_wrong() {
        assert!(parse(["--screen", "nowhere"]).unwrap_err().contains("--screen"));
        assert!(parse(["--theme", "neon"]).unwrap_err().contains("flat"));
        assert!(parse(["--mode"]).unwrap_err().contains("--mode"));
        assert!(parse(["--name"]).unwrap_err().contains("--name"));
        assert!(parse(["--fly"]).unwrap_err().contains("--fly"));
    }
}
