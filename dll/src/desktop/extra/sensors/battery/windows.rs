//! Windows: `GetSystemPowerStatus` (kernel32, loaded once by the power state's reading, which
//! shares it), read on the monitor thread - [`readings::of_windows_status`]:
//!
//! - present: `BatteryFlag` is neither "no system battery" (128) nor unknown (255);
//! - on its charger: the flag says charging, or `ACLineStatus` is online;
//! - the level: `BatteryLifePercent` (255 unknown);
//! - Low Power Mode: `SystemStatusFlag` 1, Battery Saver is on (Windows 10 and later; 0 before);
//! - the thermal state: unknown. Windows offers no cheap, unprivileged thermal reading (WMI's
//!   `MSAcpi_ThermalZoneTemperature` needs administrator rights, and is a COM query).

use super::{readings, seen};
use crate::desktop::extra::sensors::power::platform::system_power_status;

/// A reading now, kept for the next query; nothing when the call fails.
pub(super) fn refresh() {
    if let Some(status) = system_power_status() {
        seen(readings::of_windows_status(
            status.ac_line_status,
            status.battery_flag,
            status.battery_life_percent,
            status.system_status_flag,
        ));
    }
}
