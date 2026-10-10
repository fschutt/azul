//! The CLIENT HEALTH of this device: one number from 0 to 100 for how much background work it
//! should take on now - 100 a desktop on mains power and a free network, lower as the battery
//! empties, the system saves power, the device runs hot or the network costs the user, 0
//! offline. AzDrive scales its sync, its recompression, its drive index upkeep and its search
//! indexing by it; AzMeet can weigh who relays a call and which video quality to offer by it
//! (AzMeet's ROADMAP, "Client health").
//!
//! Computed LOCALLY from azul's device-state readings (`azul::sensor`: `PowerState`,
//! `BatteryState`, `NetworkState`), which read no identifier at all. Plain Rust: [`Device`]
//! holds the readings' parts as plain values, [`client_health`] makes the number, and the unit
//! tests need no libazul. With the `look` feature, `Device::of_azul` / `Device::query` fill a
//! [`Device`] from azul's readings.
//!
//! PRIVACY: the parts never leave the device. If the number ever travels (to a peer, to a
//! server), only the `u8` does - never whether the device is on battery, its level, its
//! temperature or what its network costs.
//!
//! # The weights
//!
//! Not connected: 0. Otherwise 100, less:
//!
//! | condition                                             | less |
//! |-------------------------------------------------------|------|
//! | on battery (not on mains, not on a charger)           |   15 |
//! | ... and its level known: at most 10 %                 | + 45 |
//! | ... at most 20 %                                      | + 30 |
//! | ... at most 40 %                                      | + 15 |
//! | Low Power Mode (Battery Saver, power save)            |   20 |
//! | thermal Fair                                          |   10 |
//! | thermal Serious                                       |   30 |
//! | thermal Critical                                      |   60 |
//! | metered network                                       |   20 |
//! | a hotspot (on top of metered: a phone's battery too)  |   15 |
//! | constrained (Low Data Mode, Data Saver)               |   25 |
//!
//! A connected device never goes under 1: 0 means offline, and only that. Unknown parts (a
//! level the platform does not say, an unknown thermal state) count as nothing to hold back.
//! So: a laptop on battery at 80 % is 85, at 23 % 70, at 23 % on a phone's hotspot 35; a phone
//! at 8 % in Low Power Mode 20.

/// How hot the device runs, as its system judges it (azul's `ThermalState`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Thermal {
    Nominal,
    Fair,
    Serious,
    Critical,
    Unknown,
}

/// The parts the client health is made of, as plain values (see the module documentation).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Device {
    /// On mains power, or on a charger.
    pub on_mains: bool,
    /// The battery's level in percent, where the device has a battery and says its level.
    pub battery_percent: Option<u8>,
    /// The system saves power (Low Power Mode, Battery Saver, power save).
    pub low_power_mode: bool,
    pub thermal: Thermal,
    /// The device can reach the internet.
    pub connected: bool,
    /// The connection costs the user (a mobile plan, a hotspot, a connection set metered).
    pub metered: bool,
    /// The user asked to save data (Low Data Mode, Data Saver).
    pub constrained: bool,
    /// The connection is a phone's hotspot (an estimate of azul's).
    pub hotspot: bool,
}

impl Device {
    /// A desktop on mains power, a cool device, a free wired network: health 100.
    pub const DESK: Device = Device {
        on_mains: true,
        battery_percent: None,
        low_power_mode: false,
        thermal: Thermal::Nominal,
        connected: true,
        metered: false,
        constrained: false,
        hotspot: false,
    };
}

/// The client health of `device`, 0 to 100 (the weights are in the module documentation).
#[must_use]
pub fn client_health(device: &Device) -> u8 {
    let _ = device;
    100
}

#[cfg(test)]
mod tests {
    use super::{client_health, Device, Thermal};

    fn on_battery(percent: Option<u8>) -> Device {
        Device {
            on_mains: false,
            battery_percent: percent,
            ..Device::DESK
        }
    }

    #[test]
    fn the_client_health_follows_its_table() {
        let hotspot = Device {
            metered: true,
            hotspot: true,
            ..on_battery(Some(23))
        };
        let table: &[(&str, Device, u8)] = &[
            ("a desktop on mains and a free network", Device::DESK, 100),
            (
                "on mains, the battery level does not count",
                Device {
                    battery_percent: Some(5),
                    ..Device::DESK
                },
                100,
            ),
            ("on battery, its level unknown", on_battery(None), 85),
            ("on battery at 80 %", on_battery(Some(80)), 85),
            ("on battery at 41 %", on_battery(Some(41)), 85),
            ("on battery at 40 %", on_battery(Some(40)), 70),
            ("on battery at 23 %", on_battery(Some(23)), 70),
            ("on battery at 20 %", on_battery(Some(20)), 55),
            ("on battery at 10 %", on_battery(Some(10)), 40),
            ("on battery at 0 %", on_battery(Some(0)), 40),
            (
                "Low Power Mode on mains",
                Device {
                    low_power_mode: true,
                    ..Device::DESK
                },
                80,
            ),
            (
                "a phone at 8 % in Low Power Mode",
                Device {
                    low_power_mode: true,
                    ..on_battery(Some(8))
                },
                20,
            ),
            (
                "fair",
                Device {
                    thermal: Thermal::Fair,
                    ..Device::DESK
                },
                90,
            ),
            (
                "serious",
                Device {
                    thermal: Thermal::Serious,
                    ..Device::DESK
                },
                70,
            ),
            (
                "critical",
                Device {
                    thermal: Thermal::Critical,
                    ..Device::DESK
                },
                40,
            ),
            (
                "an unknown thermal state holds nothing back",
                Device {
                    thermal: Thermal::Unknown,
                    ..Device::DESK
                },
                100,
            ),
            (
                "a metered network",
                Device {
                    metered: true,
                    ..Device::DESK
                },
                80,
            ),
            (
                "a hotspot on mains",
                Device {
                    metered: true,
                    hotspot: true,
                    ..Device::DESK
                },
                65,
            ),
            (
                "Low Data Mode",
                Device {
                    constrained: true,
                    ..Device::DESK
                },
                75,
            ),
            ("23 % on a phone's hotspot", hotspot, 35),
            (
                "everything at once stays connected",
                Device {
                    low_power_mode: true,
                    thermal: Thermal::Critical,
                    constrained: true,
                    ..on_battery(Some(3))
                },
                1,
            ),
            (
                "offline",
                Device {
                    connected: false,
                    ..Device::DESK
                },
                0,
            ),
            (
                "offline on a hotspot",
                Device {
                    connected: false,
                    ..hotspot
                },
                0,
            ),
        ];
        for (what, device, health) in table {
            assert_eq!(client_health(device), *health, "{what}: {device:?}");
        }
    }

    #[test]
    fn the_client_health_never_rises_as_a_part_worsens() {
        let levels = [Some(100), Some(60), Some(40), Some(20), Some(10), Some(0)];
        let mut last = u8::MAX;
        for percent in levels {
            let health = client_health(&on_battery(percent));
            assert!(health <= last, "{percent:?}: {health} after {last}");
            last = health;
        }
        let mut last = u8::MAX;
        for thermal in [
            Thermal::Nominal,
            Thermal::Fair,
            Thermal::Serious,
            Thermal::Critical,
        ] {
            let health = client_health(&Device {
                thermal,
                ..Device::DESK
            });
            assert!(health < last, "{thermal:?}: {health} after {last}");
            last = health;
        }
    }
}
