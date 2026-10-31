//! Windows: WinRT's `NetworkInformation` (Windows 8 and later), through the `windows` crate the
//! sensors and the notifications use. A thread of its own reads the internet connection profile
//! at the start and registers for `NetworkStatusChanged` (raised when a connection comes, goes or
//! its cost changes), whose handler reads it again on a thread of the pool; a query reads the
//! last reading:
//!
//! - connected: a profile whose connectivity level is `InternetAccess` (a captive portal's
//!   `ConstrainedInternetAccess` is not);
//! - metered: its `ConnectionCost`'s `NetworkCostType` is `Fixed` or `Variable` (a connection the
//!   user or the carrier set to metered);
//! - constrained: it approaches or is over its data limit, roams, or its background data is
//!   restricted (Data Saver);
//! - hotspot: a WLAN profile whose cost is `Variable` or that roams - the cost Windows gives a
//!   phone's hotspot it recognises ([`super::hotspot_guess`]);
//! - the kind: a WLAN profile is Wi-Fi, a WWAN one cellular, else the adapter's IANA interface
//!   type (6 Ethernet, 71 Wi-Fi, 243 / 244 mobile broadband) - anything else is other.

use std::sync::OnceLock;

use windows::Networking::Connectivity::{
    ConnectionProfile, NetworkConnectivityLevel, NetworkCostType, NetworkInformation,
    NetworkStatusChangedEventHandler,
};

use super::{hotspot_guess, last_seen, seen, NetworkKind, NetworkState};

/// IANA `ifType` numbers, as the adapter's `IanaInterfaceType` says them.
const IF_TYPE_ETHERNET: u32 = 6;
const IF_TYPE_WIFI: u32 = 71;
const IF_TYPE_WWAN_PP: u32 = 243;
const IF_TYPE_WWAN_PP2: u32 = 244;

/// What carries `profile`.
fn kind_of(profile: &ConnectionProfile) -> NetworkKind {
    if profile.IsWlanConnectionProfile().unwrap_or(false) {
        return NetworkKind::WiFi;
    }
    if profile.IsWwanConnectionProfile().unwrap_or(false) {
        return NetworkKind::Cellular;
    }
    match profile
        .NetworkAdapter()
        .and_then(|adapter| adapter.IanaInterfaceType())
    {
        Ok(IF_TYPE_ETHERNET) => NetworkKind::Wired,
        Ok(IF_TYPE_WIFI) => NetworkKind::WiFi,
        Ok(IF_TYPE_WWAN_PP | IF_TYPE_WWAN_PP2) => NetworkKind::Cellular,
        Ok(_) => NetworkKind::Other,
        Err(_) => NetworkKind::Unknown,
    }
}

/// Whether `profile` is metered, whether it is constrained, and whether its cost is the one
/// Windows gives a phone's hotspot (`Variable`, or roaming).
fn cost_of(profile: &ConnectionProfile) -> (bool, bool, bool) {
    let Ok(cost) = profile.GetConnectionCost() else {
        return (false, false, false);
    };
    let kind = cost.NetworkCostType().unwrap_or(NetworkCostType::Unknown);
    let metered = kind == NetworkCostType::Fixed || kind == NetworkCostType::Variable;
    let roaming = cost.Roaming().unwrap_or(false);
    let constrained = cost.ApproachingDataLimit().unwrap_or(false)
        || cost.OverDataLimit().unwrap_or(false)
        || roaming
        // Windows 10 and later (IConnectionCost2): Data Saver.
        || cost.BackgroundDataUsageRestricted().unwrap_or(false);
    (metered, constrained, kind == NetworkCostType::Variable || roaming)
}

/// The internet connection profile now; `None` when WinRT cannot say.
fn reading() -> Option<NetworkState> {
    let profile = match NetworkInformation::GetInternetConnectionProfile() {
        Ok(profile) => profile,
        // No profile with internet access: a null answer (an error with a success code), not a
        // failure of the call.
        Err(e) if e.code().is_ok() => return Some(NetworkState::OFFLINE),
        Err(_) => return None,
    };
    let kind = kind_of(&profile);
    let level = profile.GetNetworkConnectivityLevel().ok()?;
    if level != NetworkConnectivityLevel::InternetAccess {
        return Some(NetworkState {
            kind,
            ..NetworkState::OFFLINE
        });
    }
    let (metered, constrained, hotspot_cost) = cost_of(&profile);
    Some(NetworkState {
        kind,
        connected: true,
        metered,
        constrained,
        hotspot: hotspot_guess(kind, hotspot_cost),
    })
}

/// A new reading, when WinRT gives one.
fn read_again() {
    if let Some(state) = reading() {
        seen(state);
    }
}

/// Starts the monitor once: the first reading and the change handler, on a thread of their own
/// (the subscription outlives it - the event is a static one of `NetworkInformation`).
fn start() {
    static STARTED: OnceLock<()> = OnceLock::new();
    STARTED.get_or_init(|| {
        let spawned = std::thread::Builder::new()
            .name(String::from("azul-network-monitor"))
            .spawn(|| {
                let handler = NetworkStatusChangedEventHandler::new(|_sender| {
                    read_again();
                    Ok(())
                });
                // Subscribed first, so no change between the reading and the subscription is
                // missed.
                let _ = NetworkInformation::NetworkStatusChanged(&handler);
                read_again();
            });
        if let Err(e) = spawned {
            crate::plog_warn!("[network] no monitor thread ({e}): no network readings");
        }
    });
}

/// The monitor's last reading; `None` before its first one and where WinRT cannot say.
pub(super) fn read() -> Option<NetworkState> {
    start();
    last_seen()
}
