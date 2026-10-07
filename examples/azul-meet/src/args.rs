//! The command line: the switches every Azlin app understands (azul-appkit's `--screen`,
//! `--size`, `--theme <flat|flora>`, `--mode <system|light|dark>`, `--shot`, `--sample`,
//! `--data-dir`) with AzMeet's screens (`lobby | waiting | call | settings`), plus its own
//! `--name <name>` and one switch per `AZMEET_*` environment variable ([`SWITCHES`]: `--worker`,
//! `--join`, `--relay`, `--relay-only`, `--test-tone`, ...). A switch wins over its variable;
//! the variable is still read when the switch is not given, so older scripts keep working.
//! `-h` / `--help` lists them all. A script (or a screenshot run) opens AzMeet where it wants,
//! in the look it wants. Pure: no azul types (appkit's plain modules only), unit-tested here.

use std::collections::BTreeMap;

use azul_appkit::{args::help, AppArgs, AppSpec};

/// Which screen opens first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Screen {
    /// The start screen: new meeting, join with a link, the meeting server.
    #[default]
    Lobby,
    /// A new meeting's waiting room: the camera preview, the switches, the devices, the name,
    /// the link, "Start meeting"; a preview of one when no meeting server answers (with
    /// `--shot`, a screenshot of it).
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

/// What an AzMeet switch takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Takes {
    /// No value: the switch stands for its variable set to this (`"1"`, or `"0"` for a `--no-`
    /// switch).
    Nothing(&'static str),
    /// Any text but an empty one, shown as this in the help (`<LINK>`).
    Text(&'static str),
    /// A whole number, shown as this in the help (`<N>`).
    Number(&'static str),
    /// One of these words, in any case.
    OneOf(&'static [&'static str]),
    /// An http(s) address, or one of the words (`--relay off`), shown as this in the help.
    Url(&'static str, &'static [&'static str]),
}

/// One of AzMeet's switches: the command-line form of an `AZMEET_*` environment variable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Switch {
    /// `--relay`.
    pub flag: &'static str,
    /// `AZMEET_RELAY`.
    pub var: &'static str,
    pub takes: Takes,
    /// One line for `--help`.
    pub help: &'static str,
}

/// Every AzMeet switch, in the order `--help` lists them. `AZMEET_NAME` is `--name` (its own
/// field: the name typed in the lobby last time sits between the two).
pub const SWITCHES: &[Switch] = &[
    Switch {
        flag: "--worker",
        var: "AZMEET_WORKER",
        takes: Takes::Url("<URL>", &[]),
        help: "the meeting server, over the one saved last time",
    },
    Switch {
        flag: "--join",
        var: "AZMEET_JOIN",
        takes: Takes::Text("<LINK>"),
        help: "join that meeting (a link or a code) at start",
    },
    Switch {
        flag: "--autocreate",
        var: "AZMEET_AUTOCREATE",
        takes: Takes::Nothing("1"),
        help: "create a meeting at start and print AZMEET_LINK <link>",
    },
    Switch {
        flag: "--waiting-room",
        var: "AZMEET_WAITING_ROOM",
        takes: Takes::Nothing("1"),
        help: "with --join / --autocreate: stop in the waiting room (AZMEET_WAITING <link>)",
    },
    Switch {
        flag: "--relay",
        var: "AZMEET_RELAY",
        takes: Takes::Url("<URL>", &["off", "default"]),
        help: "the iroh relays (default: off for a meeting server on this machine)",
    },
    Switch {
        flag: "--relay-only",
        var: "AZMEET_RELAY_ONLY",
        takes: Takes::Nothing("1"),
        help: "never a direct path: every packet goes through the relay",
    },
    Switch {
        flag: "--test-tone",
        var: "AZMEET_TEST_TONE",
        takes: Takes::Nothing("1"),
        help: "a 440 Hz tone is the microphone, switched on",
    },
    Switch {
        flag: "--test-pattern",
        var: "AZMEET_TEST_PATTERN",
        takes: Takes::Nothing("1"),
        help: "colour bars are the camera, switched on; a \"Drop a video packet\" button",
    },
    Switch {
        flag: "--no-echo-cancel",
        var: "AZMEET_ECHO_CANCEL",
        takes: Takes::Nothing("0"),
        help: "send the microphone as it is (with headphones)",
    },
    Switch {
        flag: "--video-codec",
        var: "AZMEET_VIDEO_CODEC",
        takes: Takes::OneOf(&["h264", "jpeg"]),
        help: "jpeg sends JPEG even where H.264 works",
    },
    Switch {
        flag: "--mesh-cap",
        var: "AZMEET_MESH_CAP",
        takes: Takes::Number("<N>"),
        help: "rooms of up to N people send everything directly (default 4)",
    },
    Switch {
        flag: "--uplink-kbps",
        var: "AZMEET_UPLINK_KBPS",
        takes: Takes::Number("<KBPS>"),
        help: "report this uplink instead of the estimate",
    },
    Switch {
        flag: "--no-forward",
        var: "AZMEET_NO_FORWARD",
        takes: Takes::Nothing("1"),
        help: "never forward other people's media",
    },
    Switch {
        flag: "--on-battery",
        var: "AZMEET_ON_BATTERY",
        takes: Takes::Nothing("1"),
        help: "report running on battery (ranked last for the backbone)",
    },
    Switch {
        flag: "--layout",
        var: "AZMEET_LAYOUT",
        takes: Takes::OneOf(&["grid", "speaker"]),
        help: "the call's view at start",
    },
    Switch {
        flag: "--stage",
        var: "AZMEET_STAGE",
        takes: Takes::Text("<NAME>"),
        help: "who is pinned to the stage of the speaker view",
    },
    Switch {
        flag: "--panel",
        var: "AZMEET_PANEL",
        takes: Takes::OneOf(&["people", "chat", "statistics", "closed"]),
        help: "what the call's side panel shows at start",
    },
];

/// What the command line asked for.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Args {
    pub screen: Screen,
    /// The name others see (else the one typed last time, else `AZMEET_NAME`, else the login
    /// name).
    pub name: Option<String>,
    pub help: bool,
    /// The Azlin switches (azul-appkit): `--theme`, `--mode` (`None`: the saved settings
    /// decide), `--size`, `--shot`, `--sample`, `--data-dir`.
    pub kit: AppArgs,
    /// AzMeet's switches given ([`SWITCHES`]), by their variable: `AZMEET_RELAY` -> `off`; a
    /// switch without a value by what it stands for (`AZMEET_TEST_TONE` -> `1`).
    pub switches: BTreeMap<&'static str, String>,
}

impl Args {
    /// The value of `var` (`AZMEET_RELAY`): its switch (`--relay`), else what `env` says of the
    /// variable; trimmed, `None` when neither is set or the value is blank.
    pub fn setting_with(&self, var: &str, env: impl Fn(&str) -> Option<String>) -> Option<String> {
        if let Some(value) = self.switches.get(var) {
            return Some(value.clone());
        }
        env(var)
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    }

    /// [`Args::setting_with`] over this process's environment.
    pub fn setting(&self, var: &str) -> Option<String> {
        self.setting_with(var, |name| std::env::var(name).ok())
    }

    /// Whether `var` is `"1"`: its switch was given, or the variable says so.
    pub fn on(&self, var: &str) -> bool {
        self.setting(var).as_deref() == Some("1")
    }

    /// The value of `var`'s switch, only when it was given on the command line.
    pub fn switch(&self, var: &str) -> Option<&str> {
        self.switches.get(var).map(String::as_str)
    }
}

impl Takes {
    /// What `--help` shows after the flag (`<off|default|URL>`); empty when nothing.
    #[must_use]
    pub fn shown(&self) -> String {
        match self {
            Takes::Nothing(_) => String::new(),
            Takes::Text(what) | Takes::Number(what) => (*what).to_string(),
            Takes::OneOf(words) => format!("<{}>", words.join("|")),
            Takes::Url(what, words) if words.is_empty() => (*what).to_string(),
            Takes::Url(what, words) => format!(
                "<{}|{}>",
                words.join("|"),
                what.trim_start_matches('<').trim_end_matches('>')
            ),
        }
    }
}

impl Switch {
    /// `value` as `flag` takes it (trimmed; a word in its own spelling), or what is wrong.
    pub fn check(&self, value: &str) -> Result<String, String> {
        let v = value.trim();
        let flag = self.flag;
        match self.takes {
            Takes::Nothing(_) => Err(format!("{flag} takes no value")),
            Takes::Text(what) if v.is_empty() => Err(format!("{flag} needs a value {what}")),
            Takes::Text(_) => Ok(v.to_string()),
            Takes::Number(what) => v
                .parse::<u32>()
                .map(|n| n.to_string())
                .map_err(|_| format!("{flag}: expected a number {what}, got {value:?}")),
            Takes::OneOf(words) => words
                .iter()
                .find(|w| w.eq_ignore_ascii_case(v))
                .map(|w| (*w).to_string())
                .ok_or_else(|| format!("{flag}: expected {}, got {value:?}", words.join("|"))),
            Takes::Url(_, words) => {
                if let Some(word) = words.iter().find(|w| w.eq_ignore_ascii_case(v)) {
                    return Ok((*word).to_string());
                }
                let lower = v.to_ascii_lowercase();
                let rest = lower
                    .strip_prefix("https://")
                    .or_else(|| lower.strip_prefix("http://"))
                    .unwrap_or("");
                if rest.is_empty() || v.contains(char::is_whitespace) {
                    return Err(format!(
                        "{flag}: expected {}, got {value:?}",
                        self.takes.shown()
                    ));
                }
                Ok(v.to_string())
            }
        }
    }
}

/// The usage text: appkit's, then `--name` and every AzMeet switch with its variable.
#[must_use]
pub fn usage() -> String {
    let mut text = help(&SPEC);
    text.push_str("\nAZMEET:\n");
    text.push_str(&option_line(
        "--name <NAME>",
        "the name others see (else the one typed last time, AZMEET_NAME, the login name)",
    ));
    for switch in SWITCHES {
        let shown = switch.takes.shown();
        let left = if shown.is_empty() {
            switch.flag.to_string()
        } else {
            format!("{} {shown}", switch.flag)
        };
        text.push_str(&option_line(&left, &format!("{} ({})", switch.help, switch.var)));
    }
    text.push_str(
        "\nEach AzMeet switch but --name reads the environment variable in brackets when it is \
         not given\n(\"1\" for a switch without a value, AZMEET_ECHO_CANCEL=0 for --no-echo-cancel).\n",
    );
    text
}

/// One option of the usage text: the description at appkit's column (29), or on a line of its
/// own under an option too long for it.
fn option_line(left: &str, description: &str) -> String {
    const COLUMN: usize = 24;
    if left.len() <= COLUMN {
        format!("    {left:<COLUMN$} {description}\n")
    } else {
        format!("    {left}\n    {:<COLUMN$} {description}\n", "")
    }
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
    let mut switches = BTreeMap::new();
    let mut rest = Vec::with_capacity(argv.len());
    let mut i = 0;
    while i < argv.len() {
        let arg = argv[i].as_str();
        let (key, inline) = match arg.split_once('=') {
            Some((key, value)) if key.starts_with("--") => (key, Some(value.to_string())),
            _ => (arg, None),
        };
        let switch = SWITCHES.iter().find(|s| s.flag == key);
        if key == "--name" || switch.is_some() {
            let takes_value = !matches!(switch.map(|s| s.takes), Some(Takes::Nothing(_)));
            let value = match inline {
                Some(_) if !takes_value => return Err(format!("{key} takes no value")),
                Some(value) => Some(value),
                None if takes_value => {
                    i += 1;
                    Some(
                        argv.get(i)
                            .cloned()
                            .ok_or_else(|| format!("{key} needs a value"))?,
                    )
                }
                None => None,
            };
            match (switch, value) {
                (Some(switch), Some(value)) => {
                    switches.insert(switch.var, switch.check(&value)?);
                }
                (Some(switch), None) => {
                    if let Takes::Nothing(set) = switch.takes {
                        switches.insert(switch.var, set.to_string());
                    }
                }
                (None, value) => name = value,
            }
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
        switches,
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
        assert!(args.switches.is_empty());
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

    /// Every `AZMEET_*` variable a script set is a switch now: the E2E starts Ben with
    /// `--worker <url> --join <link> --waiting-room --relay <url> --relay-only --test-tone
    /// --test-pattern --panel statistics`.
    #[test]
    fn every_azmeet_variable_has_a_switch_that_wins_over_it() {
        let args = parse([
            "--worker",
            "http://127.0.0.1:8790",
            "--join=azlin://meet/abc",
            "--waiting-room",
            "--relay",
            "http://127.0.0.1:3340",
            "--relay-only",
            "--test-tone",
            "--test-pattern",
            "--no-echo-cancel",
            "--video-codec=JPEG",
            "--mesh-cap",
            "2",
            "--uplink-kbps=1000",
            "--no-forward",
            "--on-battery",
            "--layout",
            "Speaker",
            "--stage",
            "Ben",
            "--panel=statistics",
            "--autocreate",
        ])
        .expect("valid");
        let given: Vec<&str> = args.switches.keys().copied().collect();
        let every: Vec<&str> = {
            let mut vars: Vec<&str> = SWITCHES.iter().map(|s| s.var).collect();
            vars.sort_unstable();
            vars
        };
        assert_eq!(given, every, "one switch per variable, all given here");
        let none = |_: &str| None;
        assert_eq!(args.setting_with("AZMEET_WORKER", none).as_deref(), Some("http://127.0.0.1:8790"));
        assert_eq!(args.setting_with("AZMEET_JOIN", none).as_deref(), Some("azlin://meet/abc"));
        assert_eq!(args.setting_with("AZMEET_RELAY", none).as_deref(), Some("http://127.0.0.1:3340"));
        assert_eq!(args.setting_with("AZMEET_RELAY_ONLY", none).as_deref(), Some("1"));
        assert_eq!(args.setting_with("AZMEET_ECHO_CANCEL", none).as_deref(), Some("0"));
        assert_eq!(args.setting_with("AZMEET_VIDEO_CODEC", none).as_deref(), Some("jpeg"));
        assert_eq!(args.setting_with("AZMEET_MESH_CAP", none).as_deref(), Some("2"));
        assert_eq!(args.setting_with("AZMEET_LAYOUT", none).as_deref(), Some("speaker"));
        assert_eq!(args.setting_with("AZMEET_PANEL", none).as_deref(), Some("statistics"));
        // A switch wins over its variable.
        let env = |_: &str| Some(String::from("off"));
        assert_eq!(args.setting_with("AZMEET_RELAY", env).as_deref(), Some("http://127.0.0.1:3340"));
        assert_eq!(args.switch("AZMEET_WORKER"), Some("http://127.0.0.1:8790"));
        for switch in SWITCHES {
            assert!(usage().contains(switch.flag), "--help lists {}", switch.flag);
            assert!(usage().contains(switch.var), "--help names {}", switch.var);
        }
    }

    /// An older script's variables still work where no switch is given; a blank one is unset.
    #[test]
    fn a_variable_is_read_where_its_switch_is_not_given() {
        let args = parse(["--test-tone"]).expect("valid");
        let env = |name: &str| match name {
            "AZMEET_JOIN" => Some(String::from(" azlin://meet/xyz ")),
            "AZMEET_TEST_PATTERN" => Some(String::from("1")),
            "AZMEET_STAGE" => Some(String::from("   ")),
            _ => None,
        };
        assert_eq!(args.setting_with("AZMEET_JOIN", env).as_deref(), Some("azlin://meet/xyz"));
        assert_eq!(args.setting_with("AZMEET_TEST_PATTERN", env).as_deref(), Some("1"));
        assert_eq!(args.setting_with("AZMEET_TEST_TONE", env).as_deref(), Some("1"));
        assert_eq!(args.setting_with("AZMEET_STAGE", env), None, "blank is unset");
        assert_eq!(args.setting_with("AZMEET_RELAY", env), None);
        assert_eq!(args.switch("AZMEET_JOIN"), None, "only the environment named it");
    }

    #[test]
    fn a_switch_with_a_wrong_value_or_none_says_which() {
        assert!(parse(["--relay", "nowhere"]).unwrap_err().contains("--relay"));
        assert!(parse(["--relay=ftp://x"]).unwrap_err().contains("off|default|URL"));
        assert_eq!(
            parse(["--relay", "Off"]).map(|a| a.switches.get("AZMEET_RELAY").cloned()),
            Ok(Some(String::from("off")))
        );
        assert!(parse(["--worker", "localhost:8787"]).unwrap_err().contains("--worker"));
        assert!(parse(["--mesh-cap", "many"]).unwrap_err().contains("--mesh-cap"));
        assert!(parse(["--panel", "files"]).unwrap_err().contains("people|chat|statistics|closed"));
        assert!(parse(["--join"]).unwrap_err().contains("--join"));
        assert!(parse(["--join", " "]).unwrap_err().contains("--join"));
        assert!(parse(["--relay-only=yes"]).unwrap_err().contains("takes no value"));
    }
}
