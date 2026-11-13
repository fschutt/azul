//! This computer's CLIENT HEALTH and what AzDrive does with it.
//!
//! The number is azul-appkit's `client_health` (0 to 100: 100 a desktop on mains power and a
//! free network, lower on battery, in Low Power Mode, hot, on a metered network or a phone's
//! hotspot, 0 offline), made here from azul's device readings (`azul::sensor`). AzDrive scales
//! its background work by it:
//!
//! - the sync's small transfers at once: 4 from [`FULL_SPEED`], 2 from [`GENTLE`], else 1
//!   ([`transfers`]);
//! - under [`GENTLE`] the sync's files over the auto-download size wait, both ways, for a
//!   healthier pass (`sync_view::health_hold`; "Sync anyway on this network" leaves the
//!   network's part out of it);
//! - the recompression pass and the drive index's upkeep run only from [`RECOMPRESS_MIN`]
//!   (besides idle, on mains, on a free network; [`recompress_allowed`]);
//! - the search index's catch-up in the background (at the window's start, and when a search
//!   finds it old) waits under [`INDEX_MIN`] ([`index_allowed`]); an index the user asks for
//!   is made at once.
//!
//! Read at the window's start and by the sync's poll; a change prints
//! `AZDRIVE_CLIENT_HEALTH <n> transfers=<k> recompress=<allowed|held> index=<allowed|held>`
//! ([`marker`]). It shows nowhere but one line of Options > Drives > Sync ([`line`]: "This
//! computer: on battery, 23% - syncing gently"), and it goes nowhere: no peer, no server, no
//! file. If AzDrive ever tells a peer, only the number travels, never its parts.

use azul_appkit::{
    client_health::{client_health, Device, Thermal},
    l10n::{self, Arg},
};

use crate::DriveState;

/// From here on the sync runs at full speed, and nothing waits for the computer.
pub(crate) const FULL_SPEED: u8 = 80;
/// From here on the sync takes 2 transfers at once; under it one, and its big files wait.
pub(crate) const GENTLE: u8 = 40;
/// The recompression pass and the drive index's upkeep run only from here on.
pub(crate) const RECOMPRESS_MIN: u8 = 85;
/// The search index's catch-up in the background runs only from here on.
pub(crate) const INDEX_MIN: u8 = 50;

/// The small transfers a pass of the sync moves at once at `health`.
#[must_use]
pub(crate) fn transfers(health: u8) -> usize {
    if health >= FULL_SPEED {
        4
    } else if health >= GENTLE {
        2
    } else {
        1
    }
}

/// Whether the recompression pass and the drive index's upkeep may run at `health`.
#[must_use]
pub(crate) fn recompress_allowed(health: u8) -> bool {
    health >= RECOMPRESS_MIN
}

/// Whether the search index may catch up in the background at `health` (`None`: not read
/// yet - as a healthy computer).
#[must_use]
pub(crate) fn index_allowed(health: Option<u8>) -> bool {
    health.is_none_or(|health| health >= INDEX_MIN)
}

/// The marker's words after `AZDRIVE_CLIENT_HEALTH`: the number and what it decides.
#[must_use]
pub(crate) fn marker(health: u8) -> String {
    let word = |allowed: bool| if allowed { "allowed" } else { "held" };
    format!(
        "{health} transfers={} recompress={} index={}",
        transfers(health),
        word(recompress_allowed(health)),
        word(index_allowed(Some(health)))
    )
}

/// The Options' line about this computer, in the window's language: its power, what holds the
/// sync back, and the sync's pace ("This computer: on battery, 23% - syncing gently").
#[must_use]
pub(crate) fn line(device: &Device) -> String {
    let mut state = vec![if device.on_mains {
        l10n::t("azdrive-sync-health-mains")
    } else {
        match device.battery_percent {
            Some(percent) => l10n::t_args(
                "azdrive-sync-health-battery-level",
                &[("percent", Arg::from(u64::from(percent)))],
            ),
            None => l10n::t("azdrive-sync-health-battery"),
        }
    }];
    if device.low_power_mode {
        state.push(l10n::t("azdrive-sync-health-low-power"));
    }
    if matches!(device.thermal, Thermal::Serious | Thermal::Critical) {
        state.push(l10n::t("azdrive-sync-health-hot"));
    }
    if !device.connected {
        state.push(l10n::t("azdrive-sync-health-offline"));
    } else {
        if device.hotspot {
            state.push(l10n::t("azdrive-sync-health-hotspot"));
        } else if device.metered {
            state.push(l10n::t("azdrive-sync-health-metered"));
        }
        if device.constrained {
            state.push(l10n::t("azdrive-sync-health-low-data"));
        }
    }
    let health = client_health(device);
    let pace = if health == 0 {
        l10n::t("azdrive-sync-health-waits")
    } else if health >= FULL_SPEED {
        l10n::t("azdrive-sync-health-full")
    } else if health >= GENTLE {
        l10n::t("azdrive-sync-health-gentle")
    } else {
        l10n::t("azdrive-sync-health-slow")
    };
    l10n::t_args(
        "azdrive-sync-health",
        &[
            ("state", Arg::from(state.join(", "))),
            ("pace", Arg::from(pace)),
        ],
    )
}

/// The device now, from azul's readings.
#[must_use]
pub(crate) fn read() -> Device {
    Device::query()
}

/// The computer's health as last read; `None` before.
#[must_use]
pub(crate) fn health_of(s: &DriveState) -> Option<u8> {
    s.sync_view.device.as_ref().map(client_health)
}

/// Keeps `device` as the computer now; a change of its health prints the marker. Whether the
/// device changed (the Options' line says it again).
pub(crate) fn note(s: &mut DriveState, device: Device) -> bool {
    if s.sync_view.device == Some(device) {
        return false;
    }
    let before = health_of(s);
    s.sync_view.device = Some(device);
    let health = client_health(&device);
    if before != Some(health) {
        println!("AZDRIVE_CLIENT_HEALTH {}", marker(health));
    }
    true
}

#[cfg(test)]
mod tests {
    use azul_appkit::client_health::{Device, Thermal};

    use super::{index_allowed, line, marker, recompress_allowed, transfers};

    #[test]
    fn the_sync_takes_fewer_transfers_as_the_computer_weakens() {
        assert_eq!(transfers(100), 4);
        assert_eq!(transfers(80), 4);
        assert_eq!(transfers(79), 2);
        assert_eq!(transfers(40), 2);
        assert_eq!(transfers(39), 1);
        assert_eq!(transfers(1), 1);
        assert_eq!(transfers(0), 1, "offline: a pass says why it failed");
    }

    #[test]
    fn background_work_waits_for_a_healthy_computer() {
        assert!(recompress_allowed(100));
        assert!(recompress_allowed(85));
        assert!(!recompress_allowed(84), "on mains in Low Power Mode is 80");
        assert!(index_allowed(Some(50)));
        assert!(!index_allowed(Some(49)));
        assert!(index_allowed(None), "not read yet");
    }

    #[test]
    fn the_marker_says_the_number_and_what_it_decides() {
        assert_eq!(marker(100), "100 transfers=4 recompress=allowed index=allowed");
        assert_eq!(marker(70), "70 transfers=2 recompress=held index=allowed");
        assert_eq!(marker(35), "35 transfers=1 recompress=held index=held");
    }

    fn on_battery(percent: u8) -> Device {
        Device {
            on_mains: false,
            battery_percent: Some(percent),
            ..Device::DESK
        }
    }

    #[test]
    fn the_options_line_says_this_computers_state_and_the_syncs_pace() {
        crate::l10n::in_english();
        assert_eq!(
            line(&Device::DESK),
            "This computer: on mains power - syncing at full speed"
        );
        assert_eq!(
            line(&on_battery(23)),
            "This computer: on battery, 23% - syncing gently"
        );
        assert_eq!(
            line(&Device {
                metered: true,
                hotspot: true,
                ..on_battery(23)
            }),
            "This computer: on battery, 23%, on a phone's hotspot - syncing slowly, big files wait"
        );
        assert_eq!(
            line(&Device {
                battery_percent: None,
                low_power_mode: true,
                thermal: Thermal::Serious,
                metered: true,
                constrained: true,
                ..on_battery(23)
            }),
            "This computer: on battery, Low Power Mode, running hot, on a metered network, \
             Low Data Mode - syncing slowly, big files wait"
        );
        assert_eq!(
            line(&Device {
                connected: false,
                ..Device::DESK
            }),
            "This computer: on mains power, offline - waiting for a network"
        );
        azul_appkit::l10n::set_locale("de-DE");
        assert_eq!(
            line(&on_battery(23)),
            "Dieser Computer: im Akkubetrieb, 23 % - synchronisiert schonend"
        );
        crate::l10n::in_english();
    }
}
