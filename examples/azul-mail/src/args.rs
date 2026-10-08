//! AzMail's command line and its facts for azul-appkit: the switches every Azlin app has
//! (`--screen`, `--size`, `--theme`, `--mode`, `--shot`, `--sample`, `--data-dir`; parsed by
//! `azul_appkit::AppArgs`), AzMail's own (`--azlin-token-url`, `--azlin-s3-url`: where the Azlin
//! services are, over the shared Azlin config and the environment), AzMail's screens, the About
//! facts and the keyboard-shortcut table (the settings page lists it; the window key handlers
//! act on it).

use azul_appkit::{AboutInfo, AppArgs, AppSpec, Shortcut};

use crate::azlin::{self, Endpoints};

/// The names `--screen` accepts; the first is the default.
pub const SCREENS: [&str; 7] = [
    "mail",
    "backstage",
    "add-account",
    "settings",
    "options",
    "compose",
    "reply",
];

pub const SPEC: AppSpec = AppSpec {
    name: "AzMail",
    binary: "AzMail",
    summary: "a mail client: IMAP to files on this computer, sending directly or through an \
              SMTP server",
    screens: &SCREENS,
    files_help: "",
};

/// AzMail's own switches, after azul-appkit's in the usage text.
const AZMAIL_HELP: &str = concat!(
    "\nAZMAIL:\n",
    "    --azlin-token-url <URL>  The Azlin token server new Azlin accounts sign in at (default:\n",
    "                             the endpoints of the shared Azlin config, else $AZLIN_TOKEN_URL)\n",
    "    --azlin-s3-url <URL>     The S3 endpoint of the Azlin drives instead of the one the token\n",
    "                             server reports (default: the shared config, else $AZLIN_S3_URL)\n",
);

/// The usage text: azul-appkit's, then AzMail's own switches.
#[must_use]
pub fn usage() -> String {
    let mut text = azul_appkit::args::help(&SPEC);
    text.push_str(AZMAIL_HELP);
    text
}

/// The parsed command line: azul-appkit's switches and AzMail's own.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MailArgs {
    pub kit: AppArgs,
    /// `--azlin-token-url`, `--azlin-s3-url` as given (the start resolves them over the shared
    /// config and the environment: `azlin::Endpoints::resolve`).
    pub endpoints: Endpoints,
}

impl MailArgs {
    /// Parses `argv` without the program name: AzMail's own switches here, the rest by
    /// azul-appkit. `Err` carries the usage (`-h`) or what was wrong.
    pub fn parse<I, S>(argv: I) -> Result<MailArgs, String>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let argv: Vec<String> = argv.into_iter().map(Into::into).collect();
        if argv.iter().any(|a| a == "-h" || a == "--help") {
            return Err(usage());
        }
        let mut endpoints = Endpoints::default();
        let mut rest = Vec::with_capacity(argv.len());
        let mut i = 0;
        while i < argv.len() {
            let arg = argv[i].as_str();
            let (name, inline) = match arg.split_once('=') {
                Some((n, v)) if n.starts_with("--") => (n, Some(v.to_string())),
                _ => (arg, None),
            };
            if name != azlin::TOKEN_URL_FLAG && name != azlin::S3_URL_FLAG {
                rest.push(argv[i].clone());
                i += 1;
                continue;
            }
            let value = match inline {
                Some(v) => v,
                None => {
                    i += 1;
                    argv.get(i)
                        .cloned()
                        .ok_or_else(|| format!("{name} needs a URL"))?
                }
            };
            let value = value.trim().to_string();
            if value.is_empty() {
                return Err(format!("{name} needs a URL"));
            }
            if name == azlin::TOKEN_URL_FLAG {
                endpoints.token_url = Some(value);
            } else {
                endpoints.s3_url = Some(value);
            }
            i += 1;
        }
        Ok(MailArgs {
            kit: AppArgs::parse(&SPEC, rest)?,
            endpoints,
        })
    }

    /// The switches of this process (`std::env::args`, without the program name).
    pub fn from_env() -> Result<MailArgs, String> {
        MailArgs::parse(std::env::args().skip(1))
    }
}

/// AzMail's folder in the data root is `mail/` (the user's bucket later).
pub const APP_FOLDER: &str = "mail";

pub const ABOUT: AboutInfo = AboutInfo {
    name: "AzMail",
    version: env!("CARGO_PKG_VERSION"),
    summary: "Your mail as plain files: every folder of your IMAP account synced to this \
              computer, read in the Outlook 2010 layout, written in azul's rich-text editor and \
              sent directly or through an SMTP server. Part of the Azlin apps, built with azul.",
    license: "MIT",
    app_folder: APP_FOLDER,
};

/// The keyboard shortcuts (`Mod` = Cmd on macOS, Ctrl elsewhere). The key handlers of the
/// main window (`ui_main::on_main_key`) and of a message window (`ui_compose::on_compose_key`)
/// act on exactly these; the kit adds Mod+, (settings), F1 (this table) and Escape.
pub const SHORTCUTS: [Shortcut; 11] = [
    Shortcut::new("Mail", "Mod+N", "New E-mail"),
    Shortcut::new("Mail", "Mod+R", "Reply"),
    Shortcut::new("Mail", "Mod+Shift+R", "Reply All"),
    Shortcut::new("Mail", "Mod+F", "Forward"),
    Shortcut::new("Mail", "F9", "Send/Receive All Folders"),
    Shortcut::new("Mail", "Escape", "Leave the File tab"),
    Shortcut::new("Message", "Mod+Enter", "Send"),
    Shortcut::new("Message", "Mod+S", "Save the draft"),
    Shortcut::new("Message", "Mod+B", "Bold"),
    Shortcut::new("Message", "Mod+I", "Italic"),
    Shortcut::new("Message", "Mod+U", "Underline"),
];

/// AzMail's own categories on the kit's settings page (File > Options; before Appearance,
/// Data, Shortcuts, About): Mail, the View tab's switches (Outlook's Options has its Mail
/// page). The accounts have their own pages (File > Info).
pub const APP_CATEGORIES: [&str; 1] = ["Mail"];

/// The screen AzMail opens on.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Screen {
    /// The mail window - with no account too (empty; its list offers Add Account).
    #[default]
    Mail,
    /// File > Info.
    Backstage,
    /// File > Info > Add Account (the wizard).
    AddAccount,
    /// File > Info > Account Settings (the wizard when there is no account).
    Settings,
    /// The mail window with File > Options' window over it (Outlook 2010's Options dialog).
    Options,
    /// The mail window with a New E-mail window over it (with no account too).
    Compose,
    /// The mail window with a Reply to the newest message of the Inbox over it.
    Reply,
}

impl Screen {
    /// The screen `--screen` named (the default without one).
    #[must_use]
    pub fn of(args: &AppArgs) -> Screen {
        match args.screen_or_default(&SPEC) {
            "backstage" => Screen::Backstage,
            "add-account" => Screen::AddAccount,
            "settings" => Screen::Settings,
            "options" => Screen::Options,
            "compose" => Screen::Compose,
            "reply" => Screen::Reply,
            _ => Screen::Mail,
        }
    }

    /// The screen is a window of its own over the mail window (a message, File > Options): a
    /// `--shot` is that window's.
    #[must_use]
    pub fn is_own_window(self) -> bool {
        matches!(self, Screen::Options | Screen::Compose | Screen::Reply)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use azul_appkit::{ModePref, Theme};

    fn parse(args: &[&str]) -> Result<AppArgs, String> {
        AppArgs::parse(&SPEC, args.iter().copied())
    }

    #[test]
    fn no_arguments_is_the_mail_window_in_the_saved_look() {
        let a = parse(&[]).unwrap();
        assert_eq!(Screen::of(&a), Screen::Mail);
        assert!(a.theme.is_none() && a.mode.is_none() && !a.sample && a.size.is_none());
    }

    #[test]
    fn every_screen_maps_by_name_and_the_kit_switches_parse() {
        assert_eq!(Screen::of(&parse(&["--screen", "compose"]).unwrap()), Screen::Compose);
        assert_eq!(Screen::of(&parse(&["--screen=add-account"]).unwrap()), Screen::AddAccount);
        assert_eq!(Screen::of(&parse(&["--screen", "reply"]).unwrap()), Screen::Reply);
        assert_eq!(Screen::of(&parse(&["--screen", "settings"]).unwrap()), Screen::Settings);
        assert_eq!(Screen::of(&parse(&["--screen", "options"]).unwrap()), Screen::Options);
        assert_eq!(Screen::of(&parse(&["--screen", "backstage"]).unwrap()), Screen::Backstage);
        assert!(Screen::Options.is_own_window() && Screen::Compose.is_own_window());
        assert!(!Screen::Settings.is_own_window() && !Screen::Mail.is_own_window());
        let a = parse(&["--theme", "flora", "--mode", "dark", "--sample", "--size", "1280x860"])
            .unwrap();
        assert_eq!(a.theme, Some(Theme::Flora));
        assert_eq!(a.mode, Some(ModePref::Dark));
        assert!(a.sample);
        assert_eq!(a.size, Some((1280.0, 860.0)));
    }

    #[test]
    fn a_bad_argument_is_rejected_rather_than_ignored() {
        for bad in ["--screen=inbox", "--theme=dark", "--mode=dim", "--size=wide", "--nonsense"] {
            assert!(parse(&[bad]).is_err(), "{bad} must be rejected");
        }
        assert!(parse(&["positional"]).is_err(), "AzMail opens no files");
        assert!(parse(&["--screen"]).is_err(), "a flag missing its value");
        assert!(parse(&["--help"]).unwrap_err().contains("USAGE"));
    }

    #[test]
    fn the_azlin_switches_are_azmails_own_and_the_rest_goes_to_the_kit() {
        let a = MailArgs::parse([
            "--azlin-token-url",
            " http://127.0.0.1:8081 ",
            "--screen=add-account",
            "--azlin-s3-url=http://127.0.0.1:9000",
        ])
        .unwrap();
        assert_eq!(a.endpoints.token_url.as_deref(), Some("http://127.0.0.1:8081"));
        assert_eq!(a.endpoints.s3_url.as_deref(), Some("http://127.0.0.1:9000"));
        assert_eq!(Screen::of(&a.kit), Screen::AddAccount);
        assert_eq!(MailArgs::parse(["--sample"]).unwrap().endpoints, Endpoints::default());
        assert!(MailArgs::parse(["--azlin-token-url"]).is_err(), "a switch missing its URL");
        assert!(MailArgs::parse(["--azlin-token-url="]).is_err(), "an empty URL");
        assert!(MailArgs::parse(["--nonsense"]).is_err(), "the kit still refuses the rest");
        let help = MailArgs::parse(["--help"]).unwrap_err();
        assert!(help.contains("USAGE") && help.contains("--azlin-token-url"), "{help}");
    }

    #[test]
    fn the_shortcut_table_names_each_key_once() {
        let mut keys: Vec<(&str, &str)> = SHORTCUTS.iter().map(|s| (s.group, s.keys)).collect();
        keys.sort_unstable();
        let n = keys.len();
        keys.dedup();
        assert_eq!(keys.len(), n, "a key bound twice in one window");
        assert!(SHORTCUTS.iter().all(|s| !s.keys.contains("Ctrl")), "Mod, never Ctrl");
    }
}
