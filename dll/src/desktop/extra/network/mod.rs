//! The computer's NETWORK STATE for background transfers: whether it is online, and whether the
//! connection costs the user (METERED: a phone's hotspot, a capped mobile plan) or the user asked
//! to save data (CONSTRAINED: Low Data Mode, Data Saver). An app asks before it moves something
//! big that can wait (AzDrive's sync holds big files back on such a network).
//!
//! A synchronous READING of a cached value, cheap enough to ask at every poll, from any thread.
//! A platform monitor keeps it current: the first query starts it (once, for the process's life)
//! and every query reads what it last saw, so a query never waits for the network stack. Until
//! the monitor's first reading (a few milliseconds after the first query), and where the
//! platform does not say, the answer is [`NetworkState::UNKNOWN`]: connected and free - an app
//! does not stop syncing because a platform cannot say what its connection costs. An app that
//! decides at its start asks once early, so the monitor has answered by then.
//!
//! * A headless or E2E run (`AZ_BACKEND=headless`, `AZ_E2E_TEST`): [`NetworkState::HEADLESS`]
//!   (wired, connected, free), or what the file named by `AZ_NETWORK_STATE_FILE`
//!   ([`NETWORK_STATE_FILE_VAR`]) says, read at every query - so a test switches the network
//!   while the app runs, in words ([`NetworkState::from_words`]: `wifi metered`, `offline`).
//!   No test depends on the network of the machine it runs on.

use std::path::Path;

/// The variable naming the file whose words a headless / E2E run's network is
/// ([`NetworkState::from_words`]; ignored by every other run).
pub const NETWORK_STATE_FILE_VAR: &str = "AZ_NETWORK_STATE_FILE";

/// What carries the connection.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NetworkKind {
    /// The platform does not say, or nothing is connected.
    Unknown,
    /// Ethernet.
    Wired,
    /// Wi-Fi.
    WiFi,
    /// A mobile network.
    Cellular,
    /// Anything else: a VPN, Bluetooth tethering, loopback.
    Other,
}

/// The computer's network state (see the module documentation).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NetworkState {
    /// What carries the connection.
    pub kind: NetworkKind,
    /// Whether the computer can reach the internet now.
    pub connected: bool,
    /// The user or the system marks the connection as costly: a mobile network, a phone's
    /// hotspot, a connection the user set to metered.
    pub metered: bool,
    /// The user asked to save data on it: Low Data Mode, Data Saver, a plan near or over its
    /// data limit, roaming.
    pub constrained: bool,
}

impl NetworkState {
    /// What a headless or E2E run reports without a switch file: wired, connected, free.
    pub const HEADLESS: NetworkState = NetworkState {
        kind: NetworkKind::Wired,
        connected: true,
        metered: false,
        constrained: false,
    };

    /// What a platform without a reading reports: connected and free, its kind unknown.
    pub const UNKNOWN: NetworkState = NetworkState {
        kind: NetworkKind::Unknown,
        connected: true,
        metered: false,
        constrained: false,
    };

    /// No connection.
    pub const OFFLINE: NetworkState = NetworkState {
        kind: NetworkKind::Unknown,
        connected: false,
        metered: false,
        constrained: false,
    };

    /// The network state now (see the module documentation).
    #[must_use]
    pub fn query() -> NetworkState {
        if headless_run() {
            return headless_reading();
        }
        platform::read().unwrap_or(NetworkState::UNKNOWN)
    }

    /// [`NetworkState::HEADLESS`].
    #[must_use]
    pub fn headless() -> NetworkState {
        NetworkState::HEADLESS
    }

    /// Whether big transfers that can wait may run now: connected, neither metered nor
    /// constrained.
    #[must_use]
    pub fn allows_background_transfer(&self) -> bool {
        self.connected
    }

    /// A headless run's network in words, any case, separated by spaces, commas or new lines:
    /// `offline` (or `online`), `metered`, `constrained`, and its kind - `wired`, `wifi`,
    /// `cellular`, `other`, `unknown`. What the words leave out is [`NetworkState::HEADLESS`]'s
    /// (an offline network without a kind is of the unknown kind); words it does not know are
    /// left out. `cellular metered` is a phone's mobile data, `wifi constrained` a Wi-Fi in Low
    /// Data Mode.
    #[must_use]
    pub fn from_words(_text: &str) -> NetworkState {
        NetworkState::HEADLESS
    }
}

/// A headless or E2E run (the power and biometric modules' test, the same variables).
fn headless_run() -> bool {
    std::env::var("AZ_BACKEND").as_deref() == Ok("headless")
        || std::env::var("AZ_E2E_TEST").is_ok()
}

/// A headless run's network: the switch file's words, else [`NetworkState::HEADLESS`].
fn headless_reading() -> NetworkState {
    let path = std::env::var_os(NETWORK_STATE_FILE_VAR);
    reading_of_file(path.as_deref().map(Path::new))
}

/// The network the file at `path` says ([`NetworkState::from_words`]); without a file, or one
/// that cannot be read, [`NetworkState::HEADLESS`].
fn reading_of_file(_path: Option<&Path>) -> NetworkState {
    NetworkState::HEADLESS
}

mod platform {
    use super::NetworkState;

    pub(super) fn read() -> Option<NetworkState> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::{reading_of_file, NetworkKind, NetworkState};

    const WIFI: NetworkState = NetworkState {
        kind: NetworkKind::WiFi,
        connected: true,
        metered: false,
        constrained: false,
    };

    #[test]
    fn big_transfers_wait_on_a_metered_or_low_data_network_and_while_offline() {
        assert!(WIFI.allows_background_transfer());
        assert!(NetworkState::HEADLESS.allows_background_transfer());
        assert!(
            NetworkState::UNKNOWN.allows_background_transfer(),
            "a platform that cannot say does not stop an app's transfers"
        );
        let metered = NetworkState {
            metered: true,
            ..WIFI
        };
        assert!(!metered.allows_background_transfer(), "metered");
        let low_data = NetworkState {
            constrained: true,
            ..WIFI
        };
        assert!(!low_data.allows_background_transfer(), "low data mode");
        assert!(!NetworkState::OFFLINE.allows_background_transfer(), "offline");
    }

    #[test]
    fn a_headless_runs_network_is_written_in_words() {
        assert_eq!(NetworkState::from_words(""), NetworkState::HEADLESS);
        assert_eq!(
            NetworkState::from_words("wifi metered"),
            NetworkState {
                metered: true,
                ..WIFI
            }
        );
        assert_eq!(
            NetworkState::from_words("Cellular, METERED,\nconstrained\n"),
            NetworkState {
                kind: NetworkKind::Cellular,
                connected: true,
                metered: true,
                constrained: true,
            }
        );
        assert_eq!(NetworkState::from_words("offline\n"), NetworkState::OFFLINE);
        assert_eq!(
            NetworkState::from_words("wifi offline"),
            NetworkState {
                connected: false,
                ..WIFI
            },
            "an offline network keeps the kind it was named"
        );
        assert_eq!(
            NetworkState::from_words("wired teleport"),
            NetworkState::HEADLESS,
            "a word it does not know is left out"
        );
    }

    #[test]
    fn a_test_switches_a_headless_runs_network_through_its_file() {
        let path = std::env::temp_dir().join(format!(
            "azul-network-test-{}.txt",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        assert_eq!(reading_of_file(None), NetworkState::HEADLESS);
        assert_eq!(
            reading_of_file(Some(&path)),
            NetworkState::HEADLESS,
            "no file yet"
        );
        std::fs::write(&path, "cellular metered\n").unwrap();
        let metered = reading_of_file(Some(&path));
        assert!(metered.metered && metered.kind == NetworkKind::Cellular, "{metered:?}");
        std::fs::write(&path, "wifi").unwrap();
        assert_eq!(reading_of_file(Some(&path)), WIFI, "read again at every query");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn the_reading_keeps_its_ffi_layout() {
        assert_eq!(core::mem::size_of::<NetworkKind>(), 4);
        assert_eq!(core::mem::size_of::<NetworkState>(), 8);
        assert_eq!(core::mem::align_of::<NetworkState>(), 4);
    }
}
