//! Linux: NetworkManager over the D-Bus system bus (zbus, as the geolocation backend reaches
//! GeoClue). A thread of its own reads NetworkManager's manager object at the start and again at
//! every `PropertiesChanged` of it; a query reads the last reading:
//!
//! - connected: `Connectivity` is full; a NetworkManager that does not check its connectivity
//!   (unknown) answers by its `State`, connected-global;
//! - metered: `Metered` is yes or guess-yes (NetworkManager's own guess: a mobile modem, a
//!   phone's hotspot that says it is one, a connection the user set to metered);
//! - constrained: false - Linux has no system-wide data saver;
//! - hotspot: a Wi-Fi whose `Metered` is NetworkManager's own guess-yes, not the user's yes
//!   ([`metered_of`], [`super::hotspot_guess`]);
//! - the kind: `PrimaryConnectionType` - `802-3-ethernet` wired, `802-11-wireless` Wi-Fi, `gsm`
//!   and `cdma` cellular, anything else other.
//!
//! Without a system bus or NetworkManager (systemd-networkd or ConnMan alone, a container) there
//! is no reading, so the query answers [`NetworkState::UNKNOWN`] (connected, not metered); the
//! thread looks for NetworkManager again once a minute. TODO: ConnMan's and systemd-networkd's
//! own metered flags.

use std::{sync::OnceLock, time::Duration};

use zbus::{blocking::Proxy, zvariant::OwnedValue};

use super::{hotspot_guess, last_seen, seen, NetworkKind, NetworkState};

const NM: &str = "org.freedesktop.NetworkManager";
const NM_PATH: &str = "/org/freedesktop/NetworkManager";
const PROPERTIES: &str = "org.freedesktop.DBus.Properties";

/// `NMConnectivityState`: unknown (no checks), full.
const CONNECTIVITY_UNKNOWN: u32 = 0;
const CONNECTIVITY_FULL: u32 = 4;
/// `NMState` connected-global.
const STATE_CONNECTED_GLOBAL: u32 = 70;
/// `NMMetered`: yes, guess-yes.
const METERED_YES: u32 = 1;
const METERED_GUESS_YES: u32 = 3;

/// How long the thread waits before it looks for NetworkManager again.
const RETRY: Duration = Duration::from_secs(60);

/// One property of NetworkManager's manager object.
fn property<T: TryFrom<OwnedValue>>(props: &Proxy<'_>, name: &str) -> Option<T> {
    let value: OwnedValue = props.call("Get", &(NM, name)).ok()?;
    T::try_from(value).ok()
}

/// What carries NetworkManager's primary connection.
fn kind_of(connection_type: &str) -> NetworkKind {
    match connection_type {
        "" => NetworkKind::Unknown,
        "802-3-ethernet" => NetworkKind::Wired,
        "802-11-wireless" => NetworkKind::WiFi,
        "gsm" | "cdma" => NetworkKind::Cellular,
        _ => NetworkKind::Other,
    }
}

/// NetworkManager's `Metered` (`None`: not read): whether the connection costs (yes,
/// guess-yes), and whether that is NetworkManager's own guess - guess-yes: a mobile modem, or a
/// Wi-Fi whose access point says it is a phone's hotspot (Android's DHCP option 43
/// `ANDROID_METERED`, a vendor element of the access point) - rather than the user's setting
/// (yes).
fn metered_of(value: Option<u32>) -> (bool, bool) {
    (
        matches!(value, Some(METERED_YES | METERED_GUESS_YES)),
        value == Some(METERED_GUESS_YES),
    )
}

/// NetworkManager's state now; `None` when it does not answer.
fn reading(props: &Proxy<'_>) -> Option<NetworkState> {
    let connectivity: u32 = property(props, "Connectivity")?;
    let connected = match connectivity {
        CONNECTIVITY_UNKNOWN => property::<u32>(props, "State")? == STATE_CONNECTED_GLOBAL,
        level => level == CONNECTIVITY_FULL,
    };
    let (metered, guessed) = metered_of(property::<u32>(props, "Metered"));
    let kind = property::<String>(props, "PrimaryConnectionType")
        .map_or(NetworkKind::Unknown, |kind| kind_of(&kind));
    Some(NetworkState {
        kind,
        connected,
        metered: connected && metered,
        constrained: false,
        hotspot: connected && hotspot_guess(kind, guessed),
    })
}

/// Reads NetworkManager, and again at every change of it, until the bus or NetworkManager goes
/// away (or never was).
fn watch() -> Option<()> {
    let conn = zbus::blocking::Connection::system().ok()?;
    let props = Proxy::new(&conn, NM, NM_PATH, PROPERTIES).ok()?;
    // Subscribed first, so no change between the first reading and the subscription is missed.
    let changes = props.receive_signal("PropertiesChanged").ok()?;
    seen(reading(&props)?);
    for _ in changes {
        if let Some(state) = reading(&props) {
            seen(state);
        }
    }
    None
}

/// Starts the monitor thread once.
fn start() {
    static STARTED: OnceLock<()> = OnceLock::new();
    STARTED.get_or_init(|| {
        let spawned = std::thread::Builder::new()
            .name(String::from("azul-network-monitor"))
            .spawn(|| loop {
                let _ = watch();
                std::thread::sleep(RETRY);
            });
        if let Err(e) = spawned {
            crate::plog_warn!("[network] no monitor thread ({e}): no network readings");
        }
    });
}

/// The monitor's last reading; `None` before its first one and without NetworkManager.
pub(super) fn read() -> Option<NetworkState> {
    start();
    last_seen()
}

#[cfg(test)]
mod tests {
    use super::{kind_of, metered_of, NetworkKind};

    #[test]
    fn network_managers_own_metered_guess_tells_a_hotspot_from_the_users_setting() {
        assert_eq!(metered_of(Some(3)), (true, true), "guess-yes");
        assert_eq!(metered_of(Some(1)), (true, false), "yes: the user set it");
        assert_eq!(metered_of(Some(4)), (false, false), "guess-no");
        assert_eq!(metered_of(Some(2)), (false, false), "no");
        assert_eq!(metered_of(None), (false, false));
    }

    #[test]
    fn network_managers_connection_types_are_kinds() {
        assert_eq!(kind_of("802-3-ethernet"), NetworkKind::Wired);
        assert_eq!(kind_of("802-11-wireless"), NetworkKind::WiFi);
        assert_eq!(kind_of("gsm"), NetworkKind::Cellular);
        assert_eq!(kind_of("vpn"), NetworkKind::Other);
        assert_eq!(kind_of(""), NetworkKind::Unknown);
    }
}
