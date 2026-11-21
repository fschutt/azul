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

/// A sysfs attribute's text without its new line; empty when it cannot be read (the power
/// state's Linux reading shares it).
pub(crate) fn read_trimmed(path: &Path) -> String {
    std::fs::read_to_string(path)
        .map(|text| text.trim().to_string())
        .unwrap_or_default()
}

/// `NSProcessInfoThermalState` (0 nominal, 1 fair, 2 serious, 3 critical).
pub(crate) fn thermal_of_apple(state: isize) -> ThermalState {
    match state {
        0 => ThermalState::Nominal,
        1 => ThermalState::Fair,
        2 => ThermalState::Serious,
        3 => ThermalState::Critical,
        _ => ThermalState::Unknown,
    }
}

/// `PowerManager.getCurrentThermalStatus()` (`THERMAL_STATUS_NONE` 0 ... `SHUTDOWN` 6): light
/// and moderate throttling "do not largely impact" the user (Fair), severe does (Serious),
/// critical, emergency and shutdown are Critical.
pub(crate) fn thermal_of_android(status: i32) -> ThermalState {
    match status {
        0 => ThermalState::Nominal,
        1 | 2 => ThermalState::Fair,
        3 => ThermalState::Serious,
        4..=6 => ThermalState::Critical,
        _ => ThermalState::Unknown,
    }
}

/// A level from a fraction of 1 (`UIDevice.batteryLevel`: -1 when unknown).
pub(crate) fn level_of_fraction(fraction: f32) -> u8 {
    if (0.0..=1.0).contains(&fraction) {
        // 0.0..=100.0 after the multiplication: the cast cannot truncate.
        (fraction * 100.0).round() as u8
    } else {
        BatteryState::LEVEL_UNKNOWN
    }
}

/// A level from a current and a maximum capacity (IOKit's `Current Capacity` / `Max
/// Capacity`), rounded down; a battery over its maximum is full.
pub(crate) fn level_of_capacity(current: i64, max: i64) -> u8 {
    if max <= 0 || current < 0 {
        return BatteryState::LEVEL_UNKNOWN;
    }
    level_of_percent((current.saturating_mul(100) / max).min(100))
}

/// A level from a percent the platform may not know (Android's `BATTERY_PROPERTY_CAPACITY`:
/// `Integer.MIN_VALUE` when unsupported; Windows' `BatteryLifePercent`: 255 when unknown).
pub(crate) fn level_of_percent(percent: i64) -> u8 {
    match u8::try_from(percent) {
        Ok(level) if level <= 100 => level,
        _ => BatteryState::LEVEL_UNKNOWN,
    }
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
    /// `BatteryFlag`: charging, no system battery, unknown.
    const CHARGING: u8 = 8;
    const NO_SYSTEM_BATTERY: u8 = 128;
    const UNKNOWN: u8 = 255;
    let present = battery_flag != NO_SYSTEM_BATTERY && battery_flag != UNKNOWN;
    BatteryState {
        present,
        charging: present && (battery_flag & CHARGING != 0 || ac_line_status == 1),
        level_percent: if present {
            level_of_percent(i64::from(battery_life_percent))
        } else {
            BatteryState::LEVEL_UNKNOWN
        },
        low_power_mode: system_status_flag == 1,
        thermal: ThermalState::Unknown,
    }
}

/// The system batteries under `root` (`/sys/class/power_supply`): `(present, charging,
/// level_percent)`. A battery of a device (`scope` `Device`: a mouse, a headset) does not
/// count. On its charger: one battery `Charging`, or `Full` / `Not charging` while a mains or
/// USB supply is online. The level is the mean `capacity` of the batteries that say one.
pub(crate) fn of_power_supplies(root: &Path) -> (bool, bool, u8) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return (false, false, BatteryState::LEVEL_UNKNOWN);
    };
    let (mut present, mut charging_now, mut full_or_held, mut mains_online) =
        (false, false, false, false);
    let mut levels: Vec<u32> = Vec::new();
    for entry in entries.flatten() {
        let supply = entry.path();
        match read_trimmed(&supply.join("type")).as_str() {
            "Mains" | "USB" => {
                if read_trimmed(&supply.join("online")) == "1" {
                    mains_online = true;
                }
            }
            "Battery" => {
                if read_trimmed(&supply.join("scope")) == "Device"
                    || read_trimmed(&supply.join("present")) == "0"
                {
                    continue;
                }
                present = true;
                match read_trimmed(&supply.join("status")).as_str() {
                    "Charging" => charging_now = true,
                    "Full" | "Not charging" => full_or_held = true,
                    _ => {}
                }
                let level = read_trimmed(&supply.join("capacity"))
                    .parse::<i64>()
                    .map_or(BatteryState::LEVEL_UNKNOWN, level_of_percent);
                if level != BatteryState::LEVEL_UNKNOWN {
                    levels.push(u32::from(level));
                }
            }
            _ => {}
        }
    }
    let level = match u32::try_from(levels.len()) {
        Ok(count) if count > 0 => {
            u8::try_from(levels.iter().sum::<u32>() / count).unwrap_or(BatteryState::LEVEL_UNKNOWN)
        }
        _ => BatteryState::LEVEL_UNKNOWN,
    };
    let charging = present && (charging_now || (full_or_held && mains_online));
    (present, charging, level)
}

/// The hottest thermal zone under `root` (`/sys/class/thermal`), each zone's `temp` against
/// its own trip points: at or over a `hot` or `critical` one Critical, a `passive` one (where
/// the kernel slows the processor) Serious, an `active` one (a fan) Fair, else Nominal.
/// Unknown without a zone that has a trip point.
pub(crate) fn thermal_of_zones(root: &Path) -> ThermalState {
    let Ok(entries) = std::fs::read_dir(root) else {
        return ThermalState::Unknown;
    };
    // 0 nominal, 1 fair, 2 serious, 3 critical; `None` while no zone has a trip point.
    let mut hottest: Option<u8> = None;
    for entry in entries.flatten() {
        if !entry
            .file_name()
            .to_string_lossy()
            .starts_with("thermal_zone")
        {
            continue;
        }
        let zone = entry.path();
        // Millidegrees Celsius; a sensor that cannot be read says nothing.
        let Ok(temp) = read_trimmed(&zone.join("temp")).parse::<i64>() else {
            continue;
        };
        let mut level: Option<u8> = None;
        for n in 0..64 {
            let kind = read_trimmed(&zone.join(format!("trip_point_{n}_type")));
            if kind.is_empty() {
                break;
            }
            let step = match kind.as_str() {
                "active" => 1,
                "passive" => 2,
                "hot" | "critical" => 3,
                _ => continue,
            };
            // A trip point at 0 (or below) is a disabled one.
            let trip = read_trimmed(&zone.join(format!("trip_point_{n}_temp")))
                .parse::<i64>()
                .unwrap_or(0);
            if trip <= 0 {
                continue;
            }
            let reached = if temp >= trip { step } else { 0 };
            level = Some(level.map_or(reached, |l| l.max(reached)));
        }
        if let Some(level) = level {
            hottest = Some(hottest.map_or(level, |h| h.max(level)));
        }
    }
    match hottest {
        None => ThermalState::Unknown,
        Some(0) => ThermalState::Nominal,
        Some(1) => ThermalState::Fair,
        Some(2) => ThermalState::Serious,
        Some(_) => ThermalState::Critical,
    }
}

/// Whether `/sys/firmware/acpi/platform_profile` says the power saver (`low-power`).
pub(crate) fn low_power_of_platform_profile(profile: &str) -> bool {
    profile.trim() == "low-power"
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
        assert_eq!(
            level_of_fraction(-1.0),
            BatteryState::LEVEL_UNKNOWN,
            "unknown"
        );
        assert_eq!(level_of_fraction(f32::NAN), BatteryState::LEVEL_UNKNOWN);
        assert_eq!(level_of_capacity(46, 100), 46);
        assert_eq!(level_of_capacity(2_500, 5_000), 50, "capacities in mAh");
        assert_eq!(
            level_of_capacity(5_100, 5_000),
            100,
            "a battery over its maximum"
        );
        assert_eq!(
            level_of_capacity(10, 0),
            BatteryState::LEVEL_UNKNOWN,
            "no maximum"
        );
        assert_eq!(level_of_percent(64), 64);
        assert_eq!(
            level_of_percent(i64::from(i32::MIN)),
            BatteryState::LEVEL_UNKNOWN
        );
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
            let root =
                std::env::temp_dir().join(format!("azul-battery-{name}-{}", std::process::id()));
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
        assert_eq!(
            of_power_supplies(charging.path()),
            (true, true, 50),
            "two batteries"
        );

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
        assert_eq!(
            thermal_of_zones(hot.path()),
            ThermalState::Serious,
            "the hottest zone"
        );

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
        assert!(
            !low_power_of_platform_profile("quiet"),
            "quiet is about the fans"
        );
        assert!(!low_power_of_platform_profile(""));
    }
}
