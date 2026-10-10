//! The Azlin Bridge as the apps show it ("Use with other programs"): where it keeps its settings,
//! what a mail, file, calendar or contacts program is to be told, where its password is.
//!
//! The bridge (examples/azul-bridge) writes `bridge.json` into its state folder -
//! `$AZUL_BRIDGE_HOME`, else `azul-bridge` in the OS config folder - and keeps its password under
//! the keyring entry [`PASSWORD_ENTRY`]: in the OS keyring (a bridge set up with `--keyring os`)
//! or in the state folder's 0600 secrets file (`--keyring file`). This module only reads them.
//! No azul types: AzMail and AzDrive draw the rows, copy them and ask the OS keyring themselves.

use std::{
    net::{SocketAddr, TcpStream},
    path::{Path, PathBuf},
    time::Duration,
};

use serde::Deserialize;

use crate::{secrets::FileSecrets, state::SECRETS_FILE};

/// The variable naming the bridge's state folder.
pub const HOME_VAR: &str = "AZUL_BRIDGE_HOME";
/// The state folder's name in the OS config folder.
pub const STATE_DIR_NAME: &str = "azul-bridge";
/// The settings file in the state folder.
pub const CONFIG_FILE: &str = "bridge.json";
/// Its `format`.
pub const CONFIG_FORMAT: &str = "azul-bridge.config";
/// The keyring entry of the bridge's password (the OS keyring's, or the secrets file's).
pub const PASSWORD_ENTRY: &str = "AzulBridge/password";
/// Where the bridge answers: this computer only.
pub const HOST: &str = "127.0.0.1";
/// The CalDAV / CardDAV port of a settings file from before the bridge had one.
pub const PIM_PORT: u16 = 1181;

/// The bridge's state folder: `env` (`$AZUL_BRIDGE_HOME`, blank counts as unset), else
/// [`STATE_DIR_NAME`] in the OS config folder.
#[must_use]
pub fn state_dir(env: Option<String>, config_dir: Option<PathBuf>) -> Option<PathBuf> {
    env.filter(|v| !v.trim().is_empty())
        .map(PathBuf::from)
        .or_else(|| config_dir.map(|dir| dir.join(STATE_DIR_NAME)))
}

fn pim_port() -> u16 {
    PIM_PORT
}

fn file_keyring() -> String {
    String::from("file")
}

/// `bridge.json`, the fields the apps show (the bridge writes more).
#[derive(Deserialize)]
struct SettingsFile {
    #[serde(default)]
    format: String,
    #[serde(default)]
    address: String,
    imap_port: u16,
    smtp_port: u16,
    dav_port: u16,
    #[serde(default = "pim_port")]
    pim_port: u16,
    #[serde(default = "file_keyring")]
    keyring: String,
}

/// What the bridge was set up with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BridgeSettings {
    /// Its state folder.
    pub state: PathBuf,
    /// The user name every program signs in with (the account's address).
    pub address: String,
    pub imap_port: u16,
    pub smtp_port: u16,
    pub dav_port: u16,
    pub pim_port: u16,
    /// Where its password is: `os` (the OS keyring) or `file` (the secrets file).
    pub keyring: String,
}

/// Where the bridge's password is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PasswordSource {
    /// The OS keyring's entry: the app asks its keyring (azul's keyring calls).
    Keyring(String),
    /// The state folder's secrets file: [`file_password`] reads it.
    File(PathBuf),
}

/// One setting a program is to be told: what the program calls it, and its value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub label: String,
    pub value: String,
}

fn row(label: &str, value: &str) -> Row {
    Row {
        label: label.to_string(),
        value: value.to_string(),
    }
}

impl BridgeSettings {
    /// `bridge.json` of the state folder `state`; `None` when the bridge was not set up there
    /// (or the file is not the bridge's).
    #[must_use]
    pub fn load(state: &Path) -> Option<BridgeSettings> {
        let bytes = std::fs::read(state.join(CONFIG_FILE)).ok()?;
        let file: SettingsFile = serde_json::from_slice(&bytes).ok()?;
        (file.format == CONFIG_FORMAT && !file.address.trim().is_empty()).then(|| BridgeSettings {
            state: state.to_path_buf(),
            address: file.address.trim().to_string(),
            imap_port: file.imap_port,
            smtp_port: file.smtp_port,
            dav_port: file.dav_port,
            pim_port: file.pim_port,
            keyring: file.keyring,
        })
    }

    /// The bridge of this user: the settings in [`state_dir`].
    #[must_use]
    pub fn find(env: Option<String>, config_dir: Option<PathBuf>) -> Option<BridgeSettings> {
        BridgeSettings::load(&state_dir(env, config_dir)?)
    }

    /// Where the password is.
    #[must_use]
    pub fn password_source(&self) -> PasswordSource {
        if self.keyring == "os" {
            PasswordSource::Keyring(PASSWORD_ENTRY.to_string())
        } else {
            PasswordSource::File(self.state.join(SECRETS_FILE))
        }
    }

    /// For a mail program (Apple Mail, Thunderbird, Outlook): both servers and the user.
    #[must_use]
    pub fn mail_rows(&self) -> Vec<Row> {
        vec![
            row("IMAP server (incoming mail)", HOST),
            row("IMAP port", &self.imap_port.to_string()),
            row("SMTP server (outgoing mail)", HOST),
            row("SMTP port", &self.smtp_port.to_string()),
            row("Connection security", "None (the bridge answers this computer only)"),
            row("User name", &self.address),
        ]
    }

    /// For a file manager (Finder: Go > Connect to Server; Explorer: Map network drive; the
    /// Linux file managers: Other Locations / Connect to Server).
    #[must_use]
    pub fn files_rows(&self) -> Vec<Row> {
        vec![
            row("Server address (WebDAV)", &format!("http://{HOST}:{}/", self.dav_port)),
            row("User name", &self.address),
        ]
    }

    /// For a calendar or contacts program (a CalDAV / CardDAV account).
    #[must_use]
    pub fn calendar_rows(&self) -> Vec<Row> {
        vec![
            row("Server address (CalDAV / CardDAV)", &format!("http://{HOST}:{}/", self.pim_port)),
            row("User name", &self.address),
        ]
    }

    /// Every setting as text to paste, one per line (never the password).
    #[must_use]
    pub fn summary(&self) -> String {
        let mut lines = vec![format!(
            "Mail: IMAP {HOST} port {}, SMTP {HOST} port {}, no SSL / TLS, user {}",
            self.imap_port, self.smtp_port, self.address
        )];
        lines.push(format!("Files: WebDAV http://{HOST}:{}/, user {}", self.dav_port, self.address));
        lines.push(format!(
            "Calendars and contacts: CalDAV / CardDAV http://{HOST}:{}/, user {}",
            self.pim_port, self.address
        ));
        lines.join("\n")
    }

    /// Whether the bridge answers: something listens on its IMAP port of 127.0.0.1.
    #[must_use]
    pub fn running(&self, timeout: Duration) -> bool {
        listening(self.imap_port, timeout)
    }
}

/// Whether something listens on 127.0.0.1:`port`.
#[must_use]
pub fn listening(port: u16, timeout: Duration) -> bool {
    port != 0 && TcpStream::connect_timeout(&SocketAddr::from(([127, 0, 0, 1], port)), timeout).is_ok()
}

/// The password a bridge set up with `--keyring file` keeps in the secrets file `path`.
#[must_use]
pub fn file_password(path: &Path) -> Option<String> {
    FileSecrets::new(path.to_path_buf())
        .get(PASSWORD_ENTRY)
        .ok()
        .flatten()
        .filter(|password| !password.is_empty())
}

#[cfg(test)]
mod tests {
    use azul_storage::testing::TempDir;

    use super::*;

    /// A settings file as `azul-bridge init` writes it (with the fields the apps do not show).
    fn write_settings(dir: &Path, keyring: &str) {
        let text = format!(
            "{{\"format\": \"azul-bridge.config\", \"version\": 1, \"address\": \"ada@example.org\", \
             \"aliases\": [], \"account\": \"ada@example.org\", \"imap_port\": 1143, \"smtp_port\": 1025, \
             \"dav_port\": 1180, \"pim_port\": 1181, \"keyring\": \"{keyring}\"}}"
        );
        std::fs::write(dir.join(CONFIG_FILE), text).unwrap();
    }

    #[test]
    fn the_bridge_folder_is_the_variable_else_the_config_folder() {
        let os = Some(PathBuf::from("/home/ada/.config"));
        assert_eq!(state_dir(Some(String::from("/b")), os.clone()), Some(PathBuf::from("/b")));
        assert_eq!(state_dir(Some(String::from(" ")), os.clone()), Some(PathBuf::from("/home/ada/.config/azul-bridge")));
        assert_eq!(state_dir(None, None), None);
    }

    #[test]
    fn the_settings_the_bridge_wrote_are_every_row_a_program_needs_and_no_password() {
        let dir = TempDir::new("bridge-settings");
        write_settings(&dir.0, "file");
        let settings = BridgeSettings::find(Some(dir.0.display().to_string()), None).expect("set up");
        assert_eq!(
            (settings.imap_port, settings.smtp_port, settings.dav_port, settings.pim_port),
            (1143, 1025, 1180, 1181)
        );
        let mail = settings.mail_rows();
        assert_eq!(mail[0].value, "127.0.0.1");
        assert!(mail.iter().any(|r| r.label == "IMAP port" && r.value == "1143"), "{mail:?}");
        assert!(mail.iter().any(|r| r.label == "SMTP port" && r.value == "1025"), "{mail:?}");
        assert!(mail.iter().any(|r| r.label == "User name" && r.value == "ada@example.org"), "{mail:?}");
        assert_eq!(settings.files_rows()[0].value, "http://127.0.0.1:1180/");
        assert_eq!(settings.calendar_rows()[0].value, "http://127.0.0.1:1181/");
        let summary = settings.summary();
        assert!(summary.contains("IMAP 127.0.0.1 port 1143") && summary.contains("http://127.0.0.1:1181/"), "{summary}");
        assert!(!summary.to_ascii_lowercase().contains("password"), "{summary}");
    }

    #[test]
    fn a_folder_without_the_bridge_or_with_another_file_has_no_settings() {
        let dir = TempDir::new("bridge-settings-none");
        assert_eq!(BridgeSettings::load(&dir.0), None);
        std::fs::write(dir.0.join(CONFIG_FILE), b"{\"format\": \"other\", \"imap_port\": 1, \"smtp_port\": 1, \"dav_port\": 1}").unwrap();
        assert_eq!(BridgeSettings::load(&dir.0), None);
        std::fs::write(dir.0.join(CONFIG_FILE), b"not json").unwrap();
        assert_eq!(BridgeSettings::load(&dir.0), None);
        // A settings file from before CalDAV and the keyring choice: their defaults.
        std::fs::write(
            dir.0.join(CONFIG_FILE),
            b"{\"format\": \"azul-bridge.config\", \"address\": \"ada@example.org\", \"imap_port\": 1143, \
              \"smtp_port\": 1025, \"dav_port\": 1180}",
        )
        .unwrap();
        let old = BridgeSettings::load(&dir.0).unwrap();
        assert_eq!((old.pim_port, old.keyring.as_str()), (PIM_PORT, "file"));
    }

    #[test]
    fn the_password_is_in_the_os_keyring_or_the_secrets_file() {
        let dir = TempDir::new("bridge-settings-password");
        write_settings(&dir.0, "os");
        let os = BridgeSettings::load(&dir.0).unwrap();
        assert_eq!(os.password_source(), PasswordSource::Keyring(String::from("AzulBridge/password")));
        write_settings(&dir.0, "file");
        let file = BridgeSettings::load(&dir.0).unwrap();
        let PasswordSource::File(path) = file.password_source() else {
            panic!("the secrets file");
        };
        assert_eq!(path, dir.0.join("secrets.json"));
        assert_eq!(file_password(&path), None);
        FileSecrets::new(path.clone()).set(PASSWORD_ENTRY, "k7m2p-9qxat").unwrap();
        assert_eq!(file_password(&path).as_deref(), Some("k7m2p-9qxat"));
    }

    #[test]
    fn a_listening_port_is_found_and_a_closed_one_is_not() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        assert!(listening(port, Duration::from_secs(2)));
        drop(listener);
        assert!(!listening(port, Duration::from_millis(500)));
        assert!(!listening(0, Duration::from_millis(500)));
    }
}
