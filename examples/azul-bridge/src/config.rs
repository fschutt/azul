//! The bridge's state folder and its settings file.
//!
//! `<state>` is `--state-dir`, else `$AZUL_BRIDGE_HOME`, else `<OS config folder>/azul-bridge`.
//! It is the bridge's own device state (azcloud-kit's [`StateDir`]: `drives.json`, `azlin.json`,
//! `secrets.json` 0600) plus:
//!
//! - `bridge.json`: the account's address (the user name the mail programs sign in with), the
//!   mail account whose keyring names and sending settings the bridge uses, the ports. No secret.
//! - `spool/`: the outbox the submission port sends from (an AzMail folder of its own).
//! - `imap-uids/`: the UID map of every mailbox ([`crate::uids`]).

use std::path::{Path, PathBuf};

use azcloud_kit::{
    error::CloudResult,
    state::{read_json, write_json},
};
use serde::{Deserialize, Serialize};

/// The settings file's name.
pub const CONFIG_FILE: &str = "bridge.json";
/// Its `format`.
pub const CONFIG_FORMAT: &str = "azul-bridge.config";
/// The variable naming the state folder.
pub const HOME_VAR: &str = "AZUL_BRIDGE_HOME";
/// The default ports: IMAP and submission as Proton Mail Bridge has them, WebDAV next to them.
pub const IMAP_PORT: u16 = 1143;
pub const SMTP_PORT: u16 = 1025;
pub const DAV_PORT: u16 = 1180;
/// The folders inside the state folder.
pub const SPOOL_DIR: &str = "spool";

/// `bridge.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BridgeConfig {
    pub format: String,
    pub version: u32,
    /// The account's address: the mail programs' user name, the sender SMTP allows.
    pub address: String,
    /// More sender addresses SMTP allows (aliases of the same domain's account).
    #[serde(default)]
    pub aliases: Vec<String>,
    /// The mail account whose keyring entries (DKIM key, submission password) the bridge reads,
    /// by AzMail's names (`AzMail/<account>/dkim`); the address unless said otherwise.
    pub account: String,
    pub imap_port: u16,
    pub smtp_port: u16,
    pub dav_port: u16,
    /// AzMail's `sending.json` to send with; `None`: `spool/<account>/sending.json`, else
    /// direct delivery.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sending_file: Option<PathBuf>,
    /// Where the secrets are: `os` (the OS keyring) or `file` (the state folder's 0600 file);
    /// a settings file of a bridge before the choice existed said nothing: `file`.
    #[serde(default = "file_keyring")]
    pub keyring: String,
}

fn file_keyring() -> String {
    String::from("file")
}

impl BridgeConfig {
    #[must_use]
    pub fn new(address: &str) -> BridgeConfig {
        BridgeConfig {
            format: CONFIG_FORMAT.to_string(),
            version: 1,
            address: address.trim().to_string(),
            aliases: Vec::new(),
            account: address.trim().to_string(),
            imap_port: IMAP_PORT,
            smtp_port: SMTP_PORT,
            dav_port: DAV_PORT,
            sending_file: None,
            keyring: crate::secrets::KeyringChoice::default_for_build().name().to_string(),
        }
    }

    /// `bridge.json` of the state folder `state`; `None` when the bridge was not set up.
    ///
    /// # Errors
    ///
    /// When the file cannot be read or is not the bridge's.
    pub fn load(state: &Path) -> CloudResult<Option<BridgeConfig>> {
        let config: Option<BridgeConfig> = read_json(&state.join(CONFIG_FILE))?;
        match config {
            Some(config) if config.format != CONFIG_FORMAT => Err(azcloud_kit::CloudError::failed(format!(
                "{} is not the bridge's settings file",
                state.join(CONFIG_FILE).display()
            ))),
            other => Ok(other),
        }
    }

    /// Writes `bridge.json` into `state`.
    ///
    /// # Errors
    ///
    /// When it cannot be written.
    pub fn save(&self, state: &Path) -> CloudResult<()> {
        write_json(&state.join(CONFIG_FILE), self, false)
    }

    /// Every address SMTP lets the account send as.
    #[must_use]
    pub fn senders(&self) -> Vec<String> {
        let mut out = vec![self.address.clone()];
        out.extend(self.aliases.iter().cloned());
        out
    }
}

/// The state folder: `flag`, else `$AZUL_BRIDGE_HOME`, else `<config folder>/azul-bridge`.
#[must_use]
pub fn state_dir(flag: Option<PathBuf>, env: Option<String>, config_dir: Option<PathBuf>) -> Option<PathBuf> {
    flag.filter(|p| !p.as_os_str().is_empty())
        .or_else(|| env.filter(|v| !v.trim().is_empty()).map(PathBuf::from))
        .or_else(|| config_dir.map(|dir| dir.join("azul-bridge")))
}

#[cfg(test)]
mod tests {
    use azul_storage::testing::TempDir;

    use super::*;

    #[test]
    fn the_state_folder_comes_from_the_flag_then_the_environment_then_the_os() {
        let os = Some(PathBuf::from("/home/ada/.config"));
        assert_eq!(
            state_dir(Some(PathBuf::from("/x")), Some(String::from("/y")), os.clone()),
            Some(PathBuf::from("/x"))
        );
        assert_eq!(state_dir(None, Some(String::from("/y")), os.clone()), Some(PathBuf::from("/y")));
        assert_eq!(state_dir(None, Some(String::from(" ")), os.clone()), Some(PathBuf::from("/home/ada/.config/azul-bridge")));
        assert_eq!(state_dir(None, None, None), None);
    }

    #[test]
    fn the_settings_file_round_trips_and_holds_no_secret() {
        let dir = TempDir::new("bridge-config");
        assert_eq!(BridgeConfig::load(&dir.0).unwrap(), None);
        let mut config = BridgeConfig::new(" ada@example.org ");
        config.aliases.push(String::from("info@example.org"));
        config.save(&dir.0).unwrap();
        let loaded = BridgeConfig::load(&dir.0).unwrap().unwrap();
        assert_eq!(loaded, config);
        assert_eq!(loaded.address, "ada@example.org");
        assert_eq!(loaded.senders(), vec!["ada@example.org", "info@example.org"]);
        let text = std::fs::read_to_string(dir.0.join(CONFIG_FILE)).unwrap();
        assert!(!text.to_ascii_lowercase().contains("password"), "{text}");
        std::fs::write(dir.0.join(CONFIG_FILE), b"{\"format\":\"other\",\"version\":1,\"address\":\"a\",\"account\":\"a\",\"imap_port\":1,\"smtp_port\":1,\"dav_port\":1}").unwrap();
        assert!(BridgeConfig::load(&dir.0).is_err());
    }
}
