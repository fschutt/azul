//! AzKeys' facts (the switches, About, shortcuts), its settings and the window's state.
//!
//! The settings are values of azul-appkit's settings file (`keys/settings.json`): the idle
//! minutes before the vault locks, the seconds before a copied secret is cleared, and per vault
//! how THIS device unlocks it without the password (`device_unlock.<vault-id>`: `biometric` - the
//! vault key is in the OS keyring behind the OS biometric prompt; `prompt` - in the keyring, read
//! after azul's biometric prompt (a keychain that cannot bind an item to biometry); `keyring` - in
//! the keyring, no prompt (no biometrics on this system)).

use std::path::PathBuf;

use zeroize::Zeroizing;

use azul::callbacks::RefAny;
use azul_appkit::{about::AboutInfo, args::AppArgs, args::AppSpec, shortcuts::Shortcut};

use crate::clipboard::{ClipboardGuard, DEFAULT_CLEAR_SECONDS};
use crate::lock::{Attempts, AutoLock};
use crate::session::Session;
use crate::store::{self, VaultFile};

// ==== The app's facts ====

/// The screens `--screen` names (the first is the default).
pub const SCREENS: [&str; 9] = [
    "unlock",
    "create",
    "vault",
    "edit",
    "generator",
    "audit",
    "import",
    "settings",
    "shortcuts",
];

pub const SPEC: AppSpec = AppSpec {
    name: "AzKeys",
    binary: "AzKeys",
    summary: "a password manager: encrypted vault files, one-time codes, a generator",
    screens: &SCREENS,
    files_help: "a CSV or Bitwarden JSON export to import (after unlocking)",
};

pub const ABOUT: AboutInfo = AboutInfo {
    name: "AzKeys",
    version: env!("CARGO_PKG_VERSION"),
    summary: "Your passwords, cards, notes, identities, SSH keys and one-time codes in an \
              encrypted vault file (Argon2id and XChaCha20-Poly1305), unlocked with a master \
              password or, on this device, with biometrics. Part of the Azlin apps, built with azul.",
    license: "MIT",
    app_folder: store::APP_FOLDER,
};

/// The keyboard shortcuts (the settings page and F1 list them).
pub const SHORTCUTS: [Shortcut; 12] = [
    Shortcut::new("Vault", "Mod+N", "New login"),
    Shortcut::new("Vault", "Mod+L", "Lock the vault"),
    Shortcut::new("Vault", "Mod+G", "Password generator"),
    Shortcut::new("Vault", "Up / Down", "Previous / next item"),
    Shortcut::new("Item", "Mod+C", "Copy the password of the selected login"),
    Shortcut::new("Item", "Mod+B", "Copy the user name"),
    Shortcut::new("Item", "Mod+Shift+C", "Copy the one-time code"),
    Shortcut::new("Item", "Mod+E", "Edit the selected item"),
    Shortcut::new("Item", "Mod+S", "Save the item being edited"),
    Shortcut::new("Item", "Escape", "Cancel editing, close a panel"),
    Shortcut::new("Item", "Delete", "Delete the selected item (asks first)"),
    Shortcut::new("Unlock", "Enter", "Unlock with the master password"),
];

/// The settings page's own categories (before the kit's Appearance, Data, Shortcuts, About).
pub const APP_CATEGORIES: [&str; 2] = ["Security", "Vault"];

// ==== Settings ====

/// How this device unlocks a vault without its password.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceUnlock {
    /// The vault key in the OS keyring, bound to the OS biometric prompt.
    Biometric,
    /// The vault key in the OS keyring, read after azul's biometric prompt.
    Prompt,
    /// The vault key in the OS keyring, no prompt.
    Keyring,
}

impl DeviceUnlock {
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            DeviceUnlock::Biometric => "biometric",
            DeviceUnlock::Prompt => "prompt",
            DeviceUnlock::Keyring => "keyring",
        }
    }

    #[must_use]
    pub fn parse(name: &str) -> Option<DeviceUnlock> {
        match name.trim() {
            "biometric" => Some(DeviceUnlock::Biometric),
            "prompt" => Some(DeviceUnlock::Prompt),
            "keyring" => Some(DeviceUnlock::Keyring),
            _ => None,
        }
    }
}

/// The settings key of a vault's device unlock.
#[must_use]
pub fn device_unlock_key(vault_id: &str) -> String {
    format!("device_unlock.{vault_id}")
}

/// AzKeys' own settings (the theme and the mode are the kit's).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    /// Minutes without input before the vault locks (0 = never).
    pub idle_minutes: u64,
    /// Seconds before a copied secret is cleared from the clipboard (0 = never).
    pub clear_seconds: u64,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            idle_minutes: 5,
            clear_seconds: DEFAULT_CLEAR_SECONDS,
        }
    }
}

/// The choices the settings page offers.
pub const IDLE_CHOICES: [u64; 6] = [1, 2, 5, 15, 60, 0];
pub const CLEAR_CHOICES: [u64; 5] = [10, 30, 60, 120, 0];

impl Settings {
    /// The settings' values as the kit's settings file keeps them.
    #[must_use]
    pub fn values(&self) -> [(&'static str, String); 2] {
        [
            ("idle_minutes", self.idle_minutes.to_string()),
            ("clear_seconds", self.clear_seconds.to_string()),
        ]
    }

    /// The settings of the values `get` answers; unknown or bad values keep the defaults.
    #[must_use]
    pub fn from_values(get: impl Fn(&str) -> Option<String>) -> Settings {
        let mut s = Settings::default();
        if let Some(m) = get("idle_minutes").and_then(|v| v.trim().parse::<u64>().ok()) {
            s.idle_minutes = m.min(24 * 60);
        }
        if let Some(c) = get("clear_seconds").and_then(|v| v.trim().parse::<u64>().ok()) {
            s.clear_seconds = c.min(3600);
        }
        s
    }
}

/// A choice's words ("5 minutes", "Never").
#[must_use]
pub fn minutes_label(m: u64) -> String {
    match m {
        0 => "Never".to_string(),
        1 => "1 minute".to_string(),
        60 => "1 hour".to_string(),
        m => format!("{m} minutes"),
    }
}

/// A choice's words ("30 seconds", "Never").
#[must_use]
pub fn seconds_label(s: u64) -> String {
    match s {
        0 => "Never".to_string(),
        s if s % 60 == 0 => format!("{} minute{}", s / 60, if s == 60 { "" } else { "s" }),
        s => format!("{s} seconds"),
    }
}

/// Seconds since 1970.
#[must_use]
pub fn now() -> u64 {
    azul_storage::time::now_unix()
}

// ==== The window's state ====

/// What the window shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Screen {
    /// Pick a vault and unlock it (or the empty state when there is none).
    #[default]
    Unlock,
    /// A new vault's name and master password.
    Create,
    /// The unlocked vault.
    Vault,
}

/// The unlock screen's fields.
#[derive(Default)]
pub struct UnlockForm {
    /// The picked vault (an index into the listed vaults).
    pub chosen: usize,
    pub password: Zeroizing<String>,
    /// A wrong password, a damaged file, a keyring problem ("" = none).
    pub message: String,
}

/// The new-vault form.
#[derive(Default)]
pub struct CreateForm {
    pub name: String,
    pub password: Zeroizing<String>,
    pub confirm: Zeroizing<String>,
    pub message: String,
}

impl CreateForm {
    /// What keeps the form from making a vault ("" = nothing).
    #[must_use]
    pub fn problem(&self) -> String {
        if self.name.trim().is_empty() {
            return "Give the vault a name.".to_string();
        }
        if self.password.chars().count() < 8 {
            return "The master password needs 8 characters or more.".to_string();
        }
        if *self.password != *self.confirm {
            return "The two passwords differ.".to_string();
        }
        String::new()
    }
}

/// The settings page's "change the master password" fields.
#[derive(Default)]
pub struct ChangeForm {
    pub password: Zeroizing<String>,
    pub confirm: Zeroizing<String>,
    pub message: String,
}

/// A keyring request in flight (the keyring answers one at a time).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeyringOp {
    /// Storing a vault key so this device unlocks the vault (`mode` says how).
    Store {
        vault_id: String,
        mode: DeviceUnlock,
    },
    /// Reading a vault key to unlock the vault file at `key`.
    Get { vault_id: String, key: String },
    /// Forgetting a vault key.
    Delete { vault_id: String },
}

/// AzKeys' window state.
pub struct KeysApp {
    /// azul-appkit's kit: the settings file, the data root, the settings page.
    pub kit: RefAny,
    pub data_root: PathBuf,
    pub args: AppArgs,
    pub settings: Settings,
    pub screen: Screen,
    /// The `--screen` to show once the vault is unlocked ("" = none).
    pub start_screen: String,
    /// The vault files found (by name).
    pub vaults: Vec<VaultFile>,
    /// The listing answered.
    pub listed: bool,
    pub unlock: UnlockForm,
    pub create: CreateForm,
    pub change: ChangeForm,
    pub attempts: Attempts,
    /// Work on the way ("Unlocking..."; `None` = idle).
    pub busy: Option<String>,
    /// The unlocked vault.
    pub session: Option<Session>,
    pub clipboard: ClipboardGuard,
    pub auto_lock: AutoLock,
    /// The navigation's groups open: Vault, Categories, Tags.
    pub nav_open: [bool; 3],
    /// A line for the status bar ("" = none).
    pub notice: String,
    /// A save is on the way; another is due after it.
    pub saving: bool,
    pub save_again: bool,
    /// The window closes once the save on the way lands.
    pub close_after_save: bool,
    /// The vault locks once the save on the way lands.
    pub lock_pending: bool,
    /// The close guard asks "save the item?".
    pub asking_close: bool,
    pub keyring_waiting: Option<KeyringOp>,
    /// The vault (id, file key) waiting for azul's biometric prompt.
    pub biometric_for: Option<(String, String)>,
    pub timer_started: bool,
    /// The TOTP period count last shown (the window is rebuilt when it changes).
    pub shown_tick: u64,
    /// Files named on the command line, imported after the unlock.
    pub import_files: Vec<PathBuf>,
    /// `--sample`: an empty folder gets the sample vault.
    pub sample: bool,
}

impl KeysApp {
    /// The vault the unlock screen picked.
    #[must_use]
    pub fn chosen_vault(&self) -> Option<&VaultFile> {
        self.vaults.get(self.unlock.chosen)
    }

    /// The device unlock of a vault on this device (from the settings file).
    #[must_use]
    pub fn device_unlock(&self, vault_id: &str) -> Option<DeviceUnlock> {
        let mut kit = self.kit.clone();
        let value = kit.downcast_ref::<azul_appkit::ui::Kit>().and_then(|k| {
            k.settings
                .get(&device_unlock_key(vault_id))
                .map(str::to_string)
        });
        value.as_deref().and_then(DeviceUnlock::parse)
    }

    /// Whether the window has edits that a close would lose (an edited item).
    #[must_use]
    pub fn has_unsaved_form(&self) -> bool {
        self.session.as_ref().is_some_and(
            |s| matches!(&s.reading, crate::session::Reading::Edit(form) if form.changed()),
        )
    }

    /// Input happened at `now`: the idle lock starts over.
    pub fn touch(&mut self, now: u64) {
        self.auto_lock.touch(now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values_of<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |key| {
            pairs
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| (*v).to_string())
        }
    }

    #[test]
    fn settings_round_trip_and_bad_values_keep_the_defaults() {
        let s = Settings {
            idle_minutes: 15,
            clear_seconds: 60,
        };
        let values = s.values();
        let pairs: Vec<(&str, &str)> = values.iter().map(|(k, v)| (*k, v.as_str())).collect();
        assert_eq!(Settings::from_values(values_of(&pairs)), s);
        assert_eq!(
            Settings::from_values(values_of(&[("idle_minutes", "soon")])),
            Settings::default()
        );
        assert_eq!(
            Settings::from_values(values_of(&[("clear_seconds", "99999")])).clear_seconds,
            3600
        );
    }

    #[test]
    fn device_unlock_names_round_trip() {
        for mode in [
            DeviceUnlock::Biometric,
            DeviceUnlock::Prompt,
            DeviceUnlock::Keyring,
        ] {
            assert_eq!(DeviceUnlock::parse(mode.name()), Some(mode));
        }
        assert_eq!(DeviceUnlock::parse(""), None);
        assert_eq!(device_unlock_key("abc"), "device_unlock.abc");
    }

    #[test]
    fn a_new_vault_needs_a_name_and_a_confirmed_password_of_eight_characters() {
        let mut form = CreateForm::default();
        assert!(!form.problem().is_empty());
        form.name = "Personal".to_string();
        form.password = Zeroizing::new("short".to_string());
        assert!(form.problem().contains("8 characters"));
        form.password = Zeroizing::new("long enough".to_string());
        form.confirm = Zeroizing::new("long enougH".to_string());
        assert!(form.problem().contains("differ"));
        form.confirm = Zeroizing::new("long enough".to_string());
        assert_eq!(form.problem(), "");
    }

    #[test]
    fn choices_read_as_words() {
        assert_eq!(minutes_label(0), "Never");
        assert_eq!(minutes_label(5), "5 minutes");
        assert_eq!(seconds_label(30), "30 seconds");
        assert_eq!(seconds_label(120), "2 minutes");
        assert_eq!(seconds_label(60), "1 minute");
    }
}
