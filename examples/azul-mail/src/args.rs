//! AzMail's command line and its facts for azul-appkit: the switches every Azlin app has
//! (`--screen`, `--size`, `--theme`, `--mode`, `--shot`, `--sample`, `--data-dir`; parsed by
//! `azul_appkit::AppArgs`), AzMail's screens, the About facts and the keyboard-shortcut table
//! (the settings page lists it; the window key handlers act on it).

use azul_appkit::{AboutInfo, AppArgs, AppSpec, Shortcut};

/// The names `--screen` accepts; the first is the default.
pub const SCREENS: [&str; 6] = [
    "mail",
    "backstage",
    "add-account",
    "settings",
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

/// AzMail's own categories on the kit's settings page (before Appearance, Data, Shortcuts,
/// About): none yet - the accounts have their own page (File > Account Settings).
pub const APP_CATEGORIES: [&str; 0] = [];

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

impl Screen {
    /// The screen `--screen` named (the default without one).
    #[must_use]
    pub fn of(args: &AppArgs) -> Screen {
        match args.screen_or_default(&SPEC) {
            "backstage" => Screen::Backstage,
            "add-account" => Screen::AddAccount,
            "settings" => Screen::Settings,
            "compose" => Screen::Compose,
            "reply" => Screen::Reply,
            _ => Screen::Mail,
        }
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
        assert_eq!(Screen::of(&parse(&["--screen", "backstage"]).unwrap()), Screen::Backstage);
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
    fn the_shortcut_table_names_each_key_once() {
        let mut keys: Vec<(&str, &str)> = SHORTCUTS.iter().map(|s| (s.group, s.keys)).collect();
        keys.sort_unstable();
        let n = keys.len();
        keys.dedup();
        assert_eq!(keys.len(), n, "a key bound twice in one window");
        assert!(SHORTCUTS.iter().all(|s| !s.keys.contains("Ctrl")), "Mod, never Ctrl");
    }
}
