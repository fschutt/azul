//! The tray (menu-bar) item of `azul-bridge-tray` as data: what its menu says and what each entry
//! copies, decided and tested without a window. The binary (feature `tray`, `src/tray_main.rs`)
//! draws it with azul's `TrayIconData` and runs `serve` beside it.

use azcloud_kit::bridge::{BridgeSettings, Row};

/// What a menu entry does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    /// A line that says how the bridge is (greyed).
    Status,
    /// Copy what a mail program is told (IMAP and SMTP).
    CopyMail,
    /// Copy the WebDAV address.
    CopyFiles,
    /// Copy the CalDAV / CardDAV address.
    CopyCalendars,
    /// Copy every setting (no password).
    CopyAll,
    /// Copy the password (the binary has it in memory: `serve` read it at the start).
    CopyPassword,
    /// Make the login item (`true`) or remove it (`false`): [`crate::autostart`].
    StartAtLogin(bool),
    Quit,
    Separator,
}

/// The menu, top to bottom: `(label, entry)`. `serving` is `Err` with why `serve` stopped;
/// `at_login` whether the login item is there; `has_password` whether the password was read.
#[must_use]
pub fn menu(
    settings: Option<&BridgeSettings>,
    serving: Result<(), &str>,
    at_login: bool,
    has_password: bool,
) -> Vec<(String, Entry)> {
    let status = match (serving, settings) {
        (Err(why), _) => format!("Azlin Bridge: not serving ({why})"),
        (Ok(()), Some(settings)) => format!("Azlin Bridge: serving {}", settings.address),
        (Ok(()), None) => String::from("Azlin Bridge: serving"),
    };
    let mut out = vec![(status, Entry::Status), (String::new(), Entry::Separator)];
    if settings.is_some() {
        out.push((String::from("Copy mail settings (IMAP / SMTP)"), Entry::CopyMail));
        out.push((String::from("Copy the WebDAV address (Finder, Explorer)"), Entry::CopyFiles));
        out.push((String::from("Copy the CalDAV / CardDAV address"), Entry::CopyCalendars));
        out.push((String::from("Copy all settings"), Entry::CopyAll));
    }
    if has_password {
        out.push((String::from("Copy password"), Entry::CopyPassword));
    }
    out.push((String::new(), Entry::Separator));
    out.push(if at_login {
        (String::from("Do not start at login"), Entry::StartAtLogin(false))
    } else {
        (String::from("Start at login"), Entry::StartAtLogin(true))
    });
    out.push((String::from("Quit"), Entry::Quit));
    out
}

/// What a Copy entry puts on the clipboard; `None` for every other entry (the password is the
/// binary's to copy).
#[must_use]
pub fn text_of(entry: &Entry, settings: &BridgeSettings) -> Option<String> {
    let lines = |rows: Vec<Row>| {
        rows.iter()
            .map(|row| format!("{}: {}", row.label, row.value))
            .collect::<Vec<_>>()
            .join("\n")
    };
    Some(match entry {
        Entry::CopyMail => lines(settings.mail_rows()),
        Entry::CopyFiles => settings.files_rows().first()?.value.clone(),
        Entry::CopyCalendars => settings.calendar_rows().first()?.value.clone(),
        Entry::CopyAll => settings.summary(),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn settings() -> BridgeSettings {
        BridgeSettings {
            state: PathBuf::from("/state"),
            address: String::from("ada@example.org"),
            imap_port: 1143,
            smtp_port: 1025,
            dav_port: 1180,
            pim_port: 1181,
            keyring: String::from("os"),
        }
    }

    #[test]
    fn the_menu_says_whether_the_bridge_serves_and_offers_each_setting_and_the_login_item() {
        let s = settings();
        let menu = menu(Some(&s), Ok(()), false, true);
        assert_eq!(menu[0], (String::from("Azlin Bridge: serving ada@example.org"), Entry::Status));
        let entries: Vec<&Entry> = menu.iter().map(|(_, entry)| entry).collect();
        for wanted in [
            Entry::CopyMail,
            Entry::CopyFiles,
            Entry::CopyCalendars,
            Entry::CopyAll,
            Entry::CopyPassword,
            Entry::StartAtLogin(true),
            Entry::Quit,
        ] {
            assert!(entries.contains(&&wanted), "{wanted:?}: {menu:?}");
        }
        let stopped = super::menu(None, Err("port 1143 is taken"), true, false);
        assert_eq!(stopped[0].0, "Azlin Bridge: not serving (port 1143 is taken)");
        assert!(stopped.iter().all(|(_, entry)| *entry != Entry::CopyMail && *entry != Entry::CopyPassword));
        assert!(stopped.iter().any(|(_, entry)| *entry == Entry::StartAtLogin(false)));
    }

    #[test]
    fn each_copy_entry_copies_its_setting_and_never_the_password() {
        let s = settings();
        let mail = text_of(&Entry::CopyMail, &s).unwrap();
        assert!(mail.contains("IMAP port: 1143") && mail.contains("User name: ada@example.org"), "{mail}");
        assert_eq!(text_of(&Entry::CopyFiles, &s).as_deref(), Some("http://127.0.0.1:1180/"));
        assert_eq!(text_of(&Entry::CopyCalendars, &s).as_deref(), Some("http://127.0.0.1:1181/"));
        assert!(text_of(&Entry::CopyAll, &s).unwrap().contains("http://127.0.0.1:1180/"));
        assert_eq!(text_of(&Entry::CopyPassword, &s), None);
        assert_eq!(text_of(&Entry::Quit, &s), None);
    }
}
