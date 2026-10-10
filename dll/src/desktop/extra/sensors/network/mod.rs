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
//! * macOS, iOS: Network.framework's path monitor (`nw_path_is_expensive` is metered,
//!   `nw_path_is_constrained` - Low Data Mode - constrained). Hotspot: an expensive Wi-Fi path -
//!   Apple marks an iPhone's Personal Hotspot (and an Android hotspot that says it is one)
//!   expensive.
//! * Windows: WinRT's `NetworkInformation` (the connection profile's cost: Fixed / Variable is
//!   metered; near or over the data limit, roaming or Data Saver is constrained). Hotspot: a
//!   WLAN profile whose cost is Variable or roaming - what Windows gives a phone's hotspot it
//!   recognises (a Windows Mobile Hotspot's or an Android hotspot's cost element).
//! * Linux: NetworkManager over D-Bus (`Metered` yes / guess-yes); never constrained. Hotspot:
//!   a Wi-Fi NetworkManager itself guesses metered (guess-yes: Android's DHCP option 43
//!   `ANDROID_METERED`, an access point's vendor element), never one the user set metered.
//!   Without NetworkManager: UNKNOWN.
//! * Android: `ConnectivityManager` through JNI, read every 10 s (`isActiveNetworkMetered`,
//!   Data Saver's `RESTRICT_BACKGROUND_STATUS_ENABLED` is constrained). Hotspot: a metered
//!   Wi-Fi (another phone's hotspot); the phone's own mobile data is cellular, and whether the
//!   phone shares it (tethering) is not asked.
//! * Other targets: UNKNOWN.
//!
//! The hotspot is an ESTIMATE ([`NetworkState::hotspot`]), from the flags above and the kind
//! only. No network name (SSID), access point (BSSID), carrier, address or any other identifier
//! is read for it, or for anything else here.
//! * A headless or E2E run (`AZ_BACKEND=headless`, `AZ_E2E_TEST`): [`NetworkState::HEADLESS`]
//!   (wired, connected, free), or what the file named by `AZ_NETWORK_STATE_FILE`
//!   ([`NETWORK_STATE_FILE_VAR`]) says, read at every query - so a test switches the network
//!   while the app runs, in words ([`NetworkState::from_words`]: `wifi metered`, `offline`).
//!   No test depends on the network of the machine it runs on.

use std::{
    path::Path,
    sync::{Mutex, PoisonError},
};

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
    /// An ESTIMATE that the connection is a phone's hotspot (or a tethered phone's data plan):
    /// a Wi-Fi the system itself marks as costly (each platform's rule is in the module
    /// documentation). Only the system's own flags and the kind are read - never a
    /// network's name, an access point, a carrier or an address - so it can be wrong both ways:
    /// a hotspot the system does not recognise is a free Wi-Fi, and a Wi-Fi the user's
    /// router marks metered is a hotspot. Last in the struct: the FFI layout of the fields
    /// before it is unchanged.
    pub hotspot: bool,
}

impl NetworkState {
    /// What a headless or E2E run reports without a switch file: wired, connected, free.
    pub const HEADLESS: NetworkState = NetworkState {
        kind: NetworkKind::Wired,
        connected: true,
        metered: false,
        constrained: false,
        hotspot: false,
    };

    /// What a platform without a reading reports: connected and free, its kind unknown.
    pub const UNKNOWN: NetworkState = NetworkState {
        kind: NetworkKind::Unknown,
        connected: true,
        metered: false,
        constrained: false,
        hotspot: false,
    };

    /// No connection.
    pub const OFFLINE: NetworkState = NetworkState {
        kind: NetworkKind::Unknown,
        connected: false,
        metered: false,
        constrained: false,
        hotspot: false,
    };

    /// The network state now (see the module documentation).
    #[must_use]
    pub fn query() -> NetworkState {
        if super::headless_run() {
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
        self.connected && !self.metered && !self.constrained
    }

    /// A headless run's network in words, any case, separated by spaces, commas or new lines:
    /// `offline` (or `online`), `metered`, `constrained`, `hotspot`, and its kind - `wired`,
    /// `wifi`, `cellular`, `other`, `unknown`. What the words leave out is
    /// [`NetworkState::HEADLESS`]'s (an offline network without a kind is of the unknown kind);
    /// words it does not know are left out. `hotspot` is a phone's hotspot as the systems see
    /// one: metered, and a Wi-Fi unless the words name another kind. `cellular metered` is a
    /// phone's mobile data, `wifi constrained` a Wi-Fi in Low Data Mode.
    #[must_use]
    pub fn from_words(text: &str) -> NetworkState {
        let mut state = NetworkState::HEADLESS;
        let mut named_kind = false;
        let words = text
            .split(|c: char| c.is_whitespace() || c == ',')
            .filter(|word| !word.is_empty());
        for word in words {
            let kind = match word.to_ascii_lowercase().as_str() {
                "offline" => {
                    state.connected = false;
                    None
                }
                "online" => {
                    state.connected = true;
                    None
                }
                "metered" => {
                    state.metered = true;
                    None
                }
                "constrained" => {
                    state.constrained = true;
                    None
                }
                "hotspot" => {
                    state.hotspot = true;
                    state.metered = true;
                    None
                }
                "wired" => Some(NetworkKind::Wired),
                "wifi" => Some(NetworkKind::WiFi),
                "cellular" => Some(NetworkKind::Cellular),
                "other" => Some(NetworkKind::Other),
                "unknown" => Some(NetworkKind::Unknown),
                _ => None,
            };
            if let Some(kind) = kind {
                state.kind = kind;
                named_kind = true;
            }
        }
        if state.hotspot && !named_kind {
            state.kind = NetworkKind::WiFi;
        } else if !state.connected && !named_kind {
            state.kind = NetworkKind::Unknown;
        }
        state
    }
}

/// The hotspot estimate ([`NetworkState::hotspot`]): a Wi-Fi the system itself marks as costly
/// (`system_says_costly`: Apple's expensive path, Windows' variable or roaming cost,
/// NetworkManager's own metered guess, Android's metered network). A costly mobile network is
/// the device's own data plan, not a hotspot; a costly wired one is the user's setting.
#[cfg_attr(
    not(any(
        target_os = "android",
        target_os = "macos",
        target_os = "ios",
        target_os = "linux",
        target_os = "windows"
    )),
    allow(dead_code)
)]
fn hotspot_guess(kind: NetworkKind, system_says_costly: bool) -> bool {
    kind == NetworkKind::WiFi && system_says_costly
}

/// A headless run's network: the switch file's words, else [`NetworkState::HEADLESS`].
fn headless_reading() -> NetworkState {
    let path = std::env::var_os(NETWORK_STATE_FILE_VAR);
    reading_of_file(path.as_deref().map(Path::new))
}

/// The network the file at `path` says ([`NetworkState::from_words`]); without a file, or one
/// that cannot be read, [`NetworkState::HEADLESS`].
fn reading_of_file(path: Option<&Path>) -> NetworkState {
    super::switch_file_words(path).map_or(NetworkState::HEADLESS, |text| {
        NetworkState::from_words(&text)
    })
}

/// What the platform monitor saw last; `None` before its first reading.
static LAST_SEEN: Mutex<Option<NetworkState>> = Mutex::new(None);

/// A platform monitor's new reading: what the next query answers.
#[cfg_attr(
    not(any(
        target_os = "android",
        target_os = "macos",
        target_os = "ios",
        target_os = "linux",
        target_os = "windows"
    )),
    allow(dead_code)
)]
fn seen(state: NetworkState) {
    *LAST_SEEN.lock().unwrap_or_else(PoisonError::into_inner) = Some(state);
}

/// What the platform monitor saw last; `None` before its first reading.
#[cfg_attr(
    not(any(
        target_os = "android",
        target_os = "macos",
        target_os = "ios",
        target_os = "linux",
        target_os = "windows"
    )),
    allow(dead_code)
)]
fn last_seen() -> Option<NetworkState> {
    *LAST_SEEN.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(target_os = "android")]
mod android;
#[cfg(any(target_os = "macos", target_os = "ios"))]
mod apple;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "android")]
use self::android as platform;
#[cfg(any(target_os = "macos", target_os = "ios"))]
use self::apple as platform;
#[cfg(target_os = "linux")]
use self::linux as platform;
#[cfg(target_os = "windows")]
use self::windows as platform;

#[cfg(not(any(
    target_os = "android",
    target_os = "macos",
    target_os = "ios",
    target_os = "linux",
    target_os = "windows"
)))]
mod platform {
    use super::NetworkState;

    pub(super) fn read() -> Option<NetworkState> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::{hotspot_guess, reading_of_file, NetworkKind, NetworkState};

    const WIFI: NetworkState = NetworkState {
        kind: NetworkKind::WiFi,
        connected: true,
        metered: false,
        constrained: false,
        hotspot: false,
    };

    #[test]
    fn a_hotspot_is_a_wifi_the_system_itself_marks_costly() {
        assert!(hotspot_guess(NetworkKind::WiFi, true));
        assert!(!hotspot_guess(NetworkKind::WiFi, false), "a free Wi-Fi");
        assert!(
            !hotspot_guess(NetworkKind::Cellular, true),
            "on the phone itself it is its data plan"
        );
        assert!(!hotspot_guess(NetworkKind::Wired, true), "a wired line set to metered");
        assert!(!hotspot_guess(NetworkKind::Unknown, true));
    }

    #[test]
    fn a_hotspot_holds_big_transfers_back_as_a_metered_network_does() {
        let hotspot = NetworkState::from_words("hotspot");
        assert!(!hotspot.allows_background_transfer(), "{hotspot:?}");
    }

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
        assert!(
            !NetworkState::OFFLINE.allows_background_transfer(),
            "offline"
        );
    }

    #[test]
    fn a_headless_runs_network_is_written_in_words() {
        assert_eq!(NetworkState::from_words(""), NetworkState::HEADLESS);
        let hotspot = NetworkState {
            metered: true,
            hotspot: true,
            ..WIFI
        };
        assert_eq!(NetworkState::from_words("hotspot"), hotspot, "a Wi-Fi, metered");
        assert_eq!(NetworkState::from_words("Hotspot wifi"), hotspot);
        assert_eq!(
            NetworkState::from_words("other hotspot"),
            NetworkState {
                kind: NetworkKind::Other,
                ..hotspot
            },
            "Bluetooth tethering: the kind named"
        );
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
                hotspot: false,
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
        let path =
            std::env::temp_dir().join(format!("azul-network-test-{}.txt", std::process::id()));
        let _ = std::fs::remove_file(&path);
        assert_eq!(reading_of_file(None), NetworkState::HEADLESS);
        assert_eq!(
            reading_of_file(Some(&path)),
            NetworkState::HEADLESS,
            "no file yet"
        );
        std::fs::write(&path, "cellular metered\n").unwrap();
        let metered = reading_of_file(Some(&path));
        assert!(
            metered.metered && metered.kind == NetworkKind::Cellular,
            "{metered:?}"
        );
        std::fs::write(&path, "wifi").unwrap();
        assert_eq!(
            reading_of_file(Some(&path)),
            WIFI,
            "read again at every query"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn the_reading_keeps_its_ffi_layout() {
        assert_eq!(core::mem::size_of::<NetworkKind>(), 4);
        assert_eq!(core::mem::size_of::<NetworkState>(), 8);
        assert_eq!(core::mem::align_of::<NetworkState>(), 4);
        assert_eq!(
            core::mem::offset_of!(NetworkState, constrained),
            6,
            "the fields before the hotspot keep their places"
        );
        assert_eq!(core::mem::offset_of!(NetworkState, hotspot), 7);
    }
}
