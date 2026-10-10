//! The device's BATTERY for background work: whether it has one, whether it is on its charger,
//! how full it is, whether the user asked the system to save power (Low Power Mode, Battery
//! Saver, power save), and how hot the device runs (its THERMAL state). An app asks before it
//! starts work that can wait, and slows down as the battery empties or the device heats up
//! (AzDrive's client health).
//!
//! A synchronous READING of a cached value, cheap enough to ask at every poll, from any thread.
//! A monitor thread (started by the first query, once for the process's life) reads the
//! platform every [`POLL`] and keeps what it saw; a query only reads that, so it never waits
//! for the platform. Until the monitor's first reading (a few milliseconds after the first
//! query), and where the platform does not say, the answer is [`BatteryState::UNKNOWN`]: no
//! battery known, its level unknown, the temperature unknown. An app that decides at its start
//! asks once early, so the monitor has answered by then.
//!
//! * macOS: IOKit's power sources (`IOPSCopyPowerSourcesInfo`: the internal battery's current
//!   and maximum capacity, `Is Charging` and its power source state), and `NSProcessInfo`'s
//!   `thermalState` and `isLowPowerModeEnabled` (macOS 12 and later; false before).
//! * iOS: `UIDevice`'s battery monitoring (`batteryLevel`, `batteryState`; UIKit's, so read on
//!   the main queue), and `NSProcessInfo` as on macOS.
//! * Windows: `GetSystemPowerStatus` (the battery's percent and flags; `SystemStatusFlag` is
//!   Battery Saver). Windows has no cheap thermal reading: the thermal state is unknown.
//! * Linux: `/sys/class/power_supply` (the system batteries' `capacity` and `status`; a mouse's
//!   battery does not count), `/sys/firmware/acpi/platform_profile` (`low-power`: what
//!   power-profiles-daemon's power saver sets) and `/sys/class/thermal` (each zone's
//!   temperature against its own trip points; the hottest zone decides).
//! * Android: `BatteryManager` (`BATTERY_PROPERTY_CAPACITY`, `isCharging`) and `PowerManager`
//!   (`isPowerSaveMode`; `getCurrentThermalStatus` on Android 10 and later) through JNI.
//! * Other targets: UNKNOWN.
//! * A headless or E2E run (`AZ_BACKEND=headless`, `AZ_E2E_TEST`): [`BatteryState::HEADLESS`]
//!   (no battery, a cool device), or what the file named by `AZ_BATTERY_STATE_FILE`
//!   ([`BATTERY_STATE_FILE_VAR`]) says, read at every query - so a test drains the battery
//!   while the app runs, in words ([`BatteryState::from_words`]: `battery 23 discharging
//!   low-power`). No test depends on the battery of the machine it runs on.
//!
//! What is read, and what is not: see the privacy section of `crate::desktop::extra::sensors`.

use std::{
    path::Path,
    sync::{Mutex, PoisonError},
    time::Duration,
};

pub(super) mod readings;

/// The variable naming the file whose words a headless / E2E run's battery is
/// ([`BatteryState::from_words`]; ignored by every other run).
pub const BATTERY_STATE_FILE_VAR: &str = "AZ_BATTERY_STATE_FILE";

/// How often the monitor reads the platform.
pub const POLL: Duration = Duration::from_secs(20);

/// How hot the device runs, as the system judges it (its own thresholds, not degrees).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ThermalState {
    /// Normal: nothing to hold back.
    Nominal,
    /// Slightly warm: the system may start its fans or slow down a little.
    Fair,
    /// Hot: the system slows the device down; background work should wait.
    Serious,
    /// Very hot: the device must cool down; only what the user waits for should run.
    Critical,
    /// The platform does not say.
    Unknown,
}

/// The device's battery and temperature (see the module documentation).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BatteryState {
    /// Whether the device has a system battery (a laptop, a phone); `false` for a desktop and
    /// where the platform does not say.
    pub present: bool,
    /// Whether the battery is on its charger: charging, or full and kept full there.
    pub charging: bool,
    /// How full the battery is, 0 to 100; [`BatteryState::LEVEL_UNKNOWN`] (255) without a
    /// battery or where the platform does not say.
    pub level_percent: u8,
    /// Whether the user asked the system to save power: Low Power Mode (macOS, iOS), Battery
    /// Saver (Windows), power save (Android), the power-saver profile (Linux).
    pub low_power_mode: bool,
    /// How hot the device runs.
    pub thermal: ThermalState,
}

impl BatteryState {
    /// The level of a battery whose level is unknown, and of no battery.
    pub const LEVEL_UNKNOWN: u8 = 255;

    /// What a headless or E2E run reports without a switch file: no battery, a cool device.
    pub const HEADLESS: BatteryState = BatteryState {
        present: false,
        charging: false,
        level_percent: BatteryState::LEVEL_UNKNOWN,
        low_power_mode: false,
        thermal: ThermalState::Nominal,
    };

    /// What a platform without a reading reports: no battery known, the temperature unknown.
    pub const UNKNOWN: BatteryState = BatteryState {
        present: false,
        charging: false,
        level_percent: BatteryState::LEVEL_UNKNOWN,
        low_power_mode: false,
        thermal: ThermalState::Unknown,
    };

    /// The battery now (see the module documentation).
    #[must_use]
    pub fn query() -> BatteryState {
        BatteryState::UNKNOWN
    }

    /// [`BatteryState::HEADLESS`].
    #[must_use]
    pub fn headless() -> BatteryState {
        BatteryState::HEADLESS
    }

    /// Whether the device runs from a battery that is under `below_percent` full: a battery,
    /// not on its charger, its level known and lower. `false` on mains power, without a
    /// battery and while its level is unknown.
    #[must_use]
    pub fn runs_low(&self, below_percent: u8) -> bool {
        let _ = below_percent;
        false
    }

    /// A headless run's battery in words, any case, separated by spaces, commas or new lines: a
    /// number from 0 to 100 (its level, `23` or `23%`), `charging` or `discharging`, `battery`
    /// (a battery of an unknown level) or `no-battery`, `low-power` (Low Power Mode), and the
    /// thermal state - `nominal`, `fair`, `serious`, `critical`. A level or a charging word
    /// means a battery. What the words leave out is [`BatteryState::HEADLESS`]'s; words it does
    /// not know are left out. `battery 23 discharging low-power` is a laptop nearly empty in
    /// Low Power Mode, `charging 80 serious` a hot phone on its charger.
    #[must_use]
    pub fn from_words(text: &str) -> BatteryState {
        let _ = text;
        BatteryState::HEADLESS
    }
}

/// A headless run's battery: the switch file's words, else [`BatteryState::HEADLESS`].
pub(super) fn headless_reading() -> BatteryState {
    let path = std::env::var_os(BATTERY_STATE_FILE_VAR);
    reading_of_file(path.as_deref().map(Path::new))
}

/// The battery the file at `path` says ([`BatteryState::from_words`]); without a file, or one
/// that cannot be read, [`BatteryState::HEADLESS`].
fn reading_of_file(path: Option<&Path>) -> BatteryState {
    let _ = path;
    BatteryState::HEADLESS
}

/// What the monitor saw last; `None` before its first reading.
static LAST_SEEN: Mutex<Option<BatteryState>> = Mutex::new(None);

/// The monitor's new reading: what the next query answers.
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
fn seen(state: BatteryState) {
    *LAST_SEEN.lock().unwrap_or_else(PoisonError::into_inner) = Some(state);
}

/// What the monitor saw last; `None` before its first reading.
#[allow(dead_code)]
fn last_seen() -> Option<BatteryState> {
    *LAST_SEEN.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::{reading_of_file, BatteryState, ThermalState};

    const LAPTOP: BatteryState = BatteryState {
        present: true,
        charging: false,
        level_percent: 23,
        low_power_mode: false,
        thermal: ThermalState::Nominal,
    };

    #[test]
    fn a_battery_runs_low_only_off_its_charger_under_the_level() {
        assert!(LAPTOP.runs_low(30));
        assert!(!LAPTOP.runs_low(23), "23 is not under 23");
        assert!(
            !BatteryState {
                charging: true,
                ..LAPTOP
            }
            .runs_low(30),
            "on its charger"
        );
        assert!(
            !BatteryState {
                level_percent: BatteryState::LEVEL_UNKNOWN,
                ..LAPTOP
            }
            .runs_low(100),
            "a level it does not know"
        );
        assert!(!BatteryState::HEADLESS.runs_low(100), "no battery");
        assert!(!BatteryState::UNKNOWN.runs_low(100));
    }

    #[test]
    fn a_headless_runs_battery_is_written_in_words() {
        assert_eq!(BatteryState::from_words(""), BatteryState::HEADLESS);
        assert_eq!(BatteryState::from_words("battery 23 discharging"), LAPTOP);
        assert_eq!(
            BatteryState::from_words("Battery, 23%,\nDISCHARGING low-power\n"),
            BatteryState {
                low_power_mode: true,
                ..LAPTOP
            }
        );
        assert_eq!(
            BatteryState::from_words("charging 80 serious"),
            BatteryState {
                charging: true,
                level_percent: 80,
                thermal: ThermalState::Serious,
                ..LAPTOP
            }
        );
        assert_eq!(
            BatteryState::from_words("battery"),
            BatteryState {
                level_percent: BatteryState::LEVEL_UNKNOWN,
                ..LAPTOP
            },
            "a battery of an unknown level"
        );
        assert_eq!(
            BatteryState::from_words("critical"),
            BatteryState {
                thermal: ThermalState::Critical,
                ..BatteryState::HEADLESS
            },
            "a hot desktop"
        );
        assert_eq!(
            BatteryState::from_words("23 no-battery"),
            BatteryState::HEADLESS,
            "no battery has no level"
        );
        assert_eq!(
            BatteryState::from_words("battery 140 fair"),
            BatteryState {
                level_percent: BatteryState::LEVEL_UNKNOWN,
                thermal: ThermalState::Fair,
                ..LAPTOP
            },
            "a level over 100 is left out"
        );
        assert_eq!(
            BatteryState::from_words("solar"),
            BatteryState::HEADLESS,
            "a word it does not know is left out"
        );
    }

    #[test]
    fn a_test_switches_a_headless_runs_battery_through_its_file() {
        let path =
            std::env::temp_dir().join(format!("azul-battery-test-{}.txt", std::process::id()));
        let _ = std::fs::remove_file(&path);
        assert_eq!(reading_of_file(None), BatteryState::HEADLESS);
        assert_eq!(
            reading_of_file(Some(&path)),
            BatteryState::HEADLESS,
            "no file yet"
        );
        std::fs::write(&path, "battery 23 discharging\n").unwrap();
        assert_eq!(reading_of_file(Some(&path)), LAPTOP);
        std::fs::write(&path, "charging 90").unwrap();
        let charging = reading_of_file(Some(&path));
        assert!(
            charging.charging && charging.level_percent == 90,
            "read again at every query: {charging:?}"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn the_reading_keeps_its_ffi_layout() {
        assert_eq!(core::mem::size_of::<ThermalState>(), 4);
        assert_eq!(core::mem::size_of::<BatteryState>(), 8);
        assert_eq!(core::mem::align_of::<BatteryState>(), 4);
        assert_eq!(core::mem::offset_of!(BatteryState, level_percent), 2);
        assert_eq!(core::mem::offset_of!(BatteryState, thermal), 4);
    }
}
