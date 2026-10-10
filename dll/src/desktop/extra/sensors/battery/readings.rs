//! What each platform answers, turned into a [`BatteryState`]'s fields.
//!
//! Compiled on EVERY platform, as the motion sensors' `units` are: the platform files are
//! `cfg`-gated to their own target, so a test in `windows.rs` never runs on a macOS host; the
//! mappings that can go wrong (a flag misread, a thermal level one step off, a mouse's battery
//! counted) live here, where every machine that builds the crate tests them. Linux's sysfs
//! reading takes its root folder, so a test lays out a `/sys/class/...` tree of its own.

// Each mapping is used by one platform's file, and every one is tested on every host.
#![allow(dead_code)]

use std::path::Path;

use super::{BatteryState, ThermalState};

/// `NSProcessInfoThermalState` (0 nominal, 1 fair, 2 serious, 3 critical).
pub(crate) fn thermal_of_apple(state: isize) -> ThermalState {
    let _ = state;
    ThermalState::Unknown
}

/// `PowerManager.getCurrentThermalStatus()` (`THERMAL_STATUS_NONE` 0 ... `SHUTDOWN` 6).
pub(crate) fn thermal_of_android(status: i32) -> ThermalState {
    let _ = status;
    ThermalState::Unknown
}

/// A level from a fraction of 1 (`UIDevice.batteryLevel`: -1 when unknown).
pub(crate) fn level_of_fraction(fraction: f32) -> u8 {
    let _ = fraction;
    BatteryState::LEVEL_UNKNOWN
}

/// A level from a current and a maximum capacity (IOKit's `Current Capacity` / `Max
/// Capacity`).
pub(crate) fn level_of_capacity(current: i64, max: i64) -> u8 {
    let _ = (current, max);
    BatteryState::LEVEL_UNKNOWN
}

/// A level from a percent the platform may not know (Android's `BATTERY_PROPERTY_CAPACITY`:
/// `Integer.MIN_VALUE` when unsupported; Windows' `BatteryLifePercent`: 255 when unknown).
pub(crate) fn level_of_percent(percent: i64) -> u8 {
    let _ = percent;
    BatteryState::LEVEL_UNKNOWN
}

/// `SYSTEM_POWER_STATUS` (winbase.h): `ACLineStatus` (0 off, 1 on, 255 unknown),
/// `BatteryFlag` (8 charging, 128 no system battery, 255 unknown), `BatteryLifePercent` (255
/// unknown) and `SystemStatusFlag` (1: Battery Saver is on). Windows has no thermal reading.
pub(crate) fn of_windows_status(
    ac_line_status: u8,
    battery_flag: u8,
    battery_life_percent: u8,
    system_status_flag: u8,
) -> BatteryState {
    let _ = (
        ac_line_status,
        battery_flag,
        battery_life_percent,
        system_status_flag,
    );
    BatteryState::UNKNOWN
}

/// The system batteries under `root` (`/sys/class/power_supply`): `(present, charging,
/// level_percent)`. A battery of a device (`scope` `Device`: a mouse, a headset) does not
/// count. On its charger: one battery `Charging`, or `Full` / `Not charging` while a mains or
/// USB supply is online. The level is the mean `capacity` of the batteries that say one.
pub(crate) fn of_power_supplies(root: &Path) -> (bool, bool, u8) {
    let _ = root;
    (false, false, BatteryState::LEVEL_UNKNOWN)
}

/// The hottest thermal zone under `root` (`/sys/class/thermal`), each zone's `temp` against
/// its own trip points: at or over a `hot` or `critical` one Critical, a `passive` one (where
/// the kernel slows the processor) Serious, an `active` one (a fan) Fair, else Nominal.
/// Unknown without a zone that has a trip point.
pub(crate) fn thermal_of_zones(root: &Path) -> ThermalState {
    let _ = root;
    ThermalState::Unknown
}

/// Whether `/sys/firmware/acpi/platform_profile` says the power saver (`low-power`).
pub(crate) fn low_power_of_platform_profile(profile: &str) -> bool {
    let _ = profile;
    false
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;

    #[test]
    fn apples_and_androids_thermal_levels_map_to_the_four_states() {
        assert_eq!(thermal_of_apple(0), ThermalState::Nominal);
        assert_eq!(thermal_of_apple(1), ThermalState::Fair);
        assert_eq!(thermal_of_apple(2), ThermalState::Serious);
        assert_eq!(thermal_of_apple(3), ThermalState::Critical);
        assert_eq!(thermal_of_apple(7), ThermalState::Unknown);
        assert_eq!(thermal_of_android(0), ThermalState::Nominal, "none");
        assert_eq!(thermal_of_android(1), ThermalState::Fair, "light");
        assert_eq!(thermal_of_android(2), ThermalState::Fair, "moderate");
        assert_eq!(thermal_of_android(3), ThermalState::Serious, "severe");
        assert_eq!(thermal_of_android(4), ThermalState::Critical, "critical");
        assert_eq!(thermal_of_android(5), ThermalState::Critical, "emergency");
        assert_eq!(thermal_of_android(6), ThermalState::Critical, "shutdown");
        assert_eq!(thermal_of_android(-1), ThermalState::Unknown);
    }

    #[test]
    fn a_level_is_a_percent_or_unknown() {
        assert_eq!(level_of_fraction(0.234), 23);
        assert_eq!(level_of_fraction(1.0), 100);
        assert_eq!(level_of_fraction(0.0), 0);
        assert_eq!(level_of_fraction(-1.0), BatteryState::LEVEL_UNKNOWN, "unknown");
        assert_eq!(level_of_fraction(f32::NAN), BatteryState::LEVEL_UNKNOWN);
        assert_eq!(level_of_capacity(46, 100), 46);
        assert_eq!(level_of_capacity(2_500, 5_000), 50, "capacities in mAh");
        assert_eq!(level_of_capacity(5_100, 5_000), 100, "a battery over its maximum");
        assert_eq!(level_of_capacity(10, 0), BatteryState::LEVEL_UNKNOWN, "no maximum");
        assert_eq!(level_of_percent(64), 64);
        assert_eq!(level_of_percent(i64::from(i32::MIN)), BatteryState::LEVEL_UNKNOWN);
        assert_eq!(level_of_percent(255), BatteryState::LEVEL_UNKNOWN);
    }

    #[test]
    fn windows_power_status_flags_say_the_battery_and_battery_saver() {
        assert_eq!(
            of_windows_status(0, 0, 23, 1),
            BatteryState {
                present: true,
                charging: false,
                level_percent: 23,
                low_power_mode: true,
                thermal: ThermalState::Unknown,
            },
            "on battery, Battery Saver on"
        );
        let charging = of_windows_status(1, 8, 80, 0);
        assert!(charging.present && charging.charging && charging.level_percent == 80);
        let full = of_windows_status(1, 1, 100, 0);
        assert!(full.charging, "full and on its charger");
        let desktop = of_windows_status(1, 128, 255, 0);
        assert!(!desktop.present, "no system battery");
        assert_eq!(desktop.level_percent, BatteryState::LEVEL_UNKNOWN);
        assert!(!desktop.charging, "no battery is charging");
        let unknown = of_windows_status(255, 255, 255, 0);
        assert!(!unknown.present && unknown.level_percent == BatteryState::LEVEL_UNKNOWN);
    }

    /// A sysfs tree of its own, gone when dropped.
    struct Tree(PathBuf);

    impl Tree {
        fn new(name: &str) -> Tree {
            let root = std::env::temp_dir().join(format!(
                "azul-battery-{name}-{}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(&root).unwrap();
            Tree(root)
        }

        fn file(&self, path: &str, text: &str) -> &Tree {
            let path = self.0.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, format!("{text}\n")).unwrap();
            self
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for Tree {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn linux_counts_the_system_batteries_and_not_a_mouse() {
        let laptop = Tree::new("laptop");
        laptop
            .file("BAT0/type", "Battery")
            .file("BAT0/status", "Discharging")
            .file("BAT0/capacity", "23")
            .file("AC/type", "Mains")
            .file("AC/online", "0")
            .file("hidpp_battery_0/type", "Battery")
            .file("hidpp_battery_0/scope", "Device")
            .file("hidpp_battery_0/status", "Charging")
            .file("hidpp_battery_0/capacity", "90");
        assert_eq!(of_power_supplies(laptop.path()), (true, false, 23));

        let charging = Tree::new("charging");
        charging
            .file("BAT0/type", "Battery")
            .file("BAT0/status", "Charging")
            .file("BAT0/capacity", "40")
            .file("BAT1/type", "Battery")
            .file("BAT1/status", "Charging")
            .file("BAT1/capacity", "60");
        assert_eq!(of_power_supplies(charging.path()), (true, true, 50), "two batteries");

        let held = Tree::new("held");
        held.file("BAT0/type", "Battery")
            .file("BAT0/status", "Not charging")
            .file("BAT0/capacity", "80")
            .file("ADP1/type", "Mains")
            .file("ADP1/online", "1");
        assert_eq!(
            of_power_supplies(held.path()),
            (true, true, 80),
            "held at a charge threshold on its charger"
        );

        let desktop = Tree::new("desktop");
        desktop.file("AC/type", "Mains").file("AC/online", "1");
        assert_eq!(
            of_power_supplies(desktop.path()),
            (false, false, BatteryState::LEVEL_UNKNOWN)
        );
        assert_eq!(
            of_power_supplies(&desktop.path().join("missing")),
            (false, false, BatteryState::LEVEL_UNKNOWN)
        );
    }

    #[test]
    fn linux_judges_the_hottest_zone_against_its_own_trip_points() {
        let cool = Tree::new("cool");
        cool.file("thermal_zone0/temp", "45000")
            .file("thermal_zone0/trip_point_0_type", "passive")
            .file("thermal_zone0/trip_point_0_temp", "90000")
            .file("thermal_zone0/trip_point_1_type", "critical")
            .file("thermal_zone0/trip_point_1_temp", "105000")
            .file("cooling_device0/type", "Processor");
        assert_eq!(thermal_of_zones(cool.path()), ThermalState::Nominal);

        let hot = Tree::new("hot");
        hot.file("thermal_zone0/temp", "45000")
            .file("thermal_zone0/trip_point_0_type", "passive")
            .file("thermal_zone0/trip_point_0_temp", "90000")
            .file("thermal_zone1/temp", "92000")
            .file("thermal_zone1/trip_point_0_type", "active")
            .file("thermal_zone1/trip_point_0_temp", "60000")
            .file("thermal_zone1/trip_point_1_type", "passive")
            .file("thermal_zone1/trip_point_1_temp", "90000")
            .file("thermal_zone1/trip_point_2_type", "critical")
            .file("thermal_zone1/trip_point_2_temp", "105000");
        assert_eq!(thermal_of_zones(hot.path()), ThermalState::Serious, "the hottest zone");

        let fan = Tree::new("fan");
        fan.file("thermal_zone0/temp", "65000")
            .file("thermal_zone0/trip_point_0_type", "active")
            .file("thermal_zone0/trip_point_0_temp", "60000")
            .file("thermal_zone0/trip_point_1_type", "hot")
            .file("thermal_zone0/trip_point_1_temp", "0");
        assert_eq!(
            thermal_of_zones(fan.path()),
            ThermalState::Fair,
            "a fan's trip point; a trip point at 0 is disabled"
        );

        let burning = Tree::new("burning");
        burning
            .file("thermal_zone0/temp", "101000")
            .file("thermal_zone0/trip_point_0_type", "hot")
            .file("thermal_zone0/trip_point_0_temp", "100000");
        assert_eq!(thermal_of_zones(burning.path()), ThermalState::Critical);

        let bare = Tree::new("bare");
        bare.file("thermal_zone0/temp", "50000");
        assert_eq!(
            thermal_of_zones(bare.path()),
            ThermalState::Unknown,
            "a zone without a trip point says nothing"
        );
    }

    #[test]
    fn linux_says_low_power_for_the_power_saver_profile_only() {
        assert!(low_power_of_platform_profile("low-power\n"));
        assert!(!low_power_of_platform_profile("balanced"));
        assert!(!low_power_of_platform_profile("quiet"), "quiet is about the fans");
        assert!(!low_power_of_platform_profile(""));
    }
}
