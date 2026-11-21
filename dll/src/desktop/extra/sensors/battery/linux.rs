//! Linux: sysfs, read on the monitor thread (a handful of small files, no daemon asked):
//!
//! - the system batteries in `/sys/class/power_supply` (`capacity`, `status`; a mouse's or a
//!   headset's battery, `scope` `Device`, does not count) - [`readings::of_power_supplies`];
//! - Low Power Mode: `/sys/firmware/acpi/platform_profile` is `low-power`, the profile
//!   power-profiles-daemon's power saver sets (GNOME's and KDE's power mode switch) -
//!   [`readings::low_power_of_platform_profile`]; a machine without the ACPI platform profile
//!   says no;
//! - the thermal state: each zone of `/sys/class/thermal` against its own trip points, the
//!   hottest zone deciding - [`readings::thermal_of_zones`].

use std::path::Path;

use super::{readings, seen, BatteryState};

const POWER_SUPPLY: &str = "/sys/class/power_supply";
const PLATFORM_PROFILE: &str = "/sys/firmware/acpi/platform_profile";
const THERMAL: &str = "/sys/class/thermal";

/// A reading now, kept for the next query.
pub(super) fn refresh() {
    let (present, charging, level_percent) = readings::of_power_supplies(Path::new(POWER_SUPPLY));
    let profile = readings::read_trimmed(Path::new(PLATFORM_PROFILE));
    seen(BatteryState {
        present,
        charging,
        level_percent,
        low_power_mode: readings::low_power_of_platform_profile(&profile),
        thermal: readings::thermal_of_zones(Path::new(THERMAL)),
    });
}
