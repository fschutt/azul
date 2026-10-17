//! Android: the activity's `batterymanager` and `power` system services through JNI, read on the
//! monitor thread (a handful of JNI calls; reached the way the network's reading reaches the
//! connectivity service):
//!
//! - the level: `BatteryManager.getIntProperty(BATTERY_PROPERTY_CAPACITY)` (API 21;
//!   `Integer.MIN_VALUE` where the device does not say - [`readings::level_of_percent`]); a
//!   level is a battery (an Android TV box has none);
//! - on its charger: `BatteryManager.isCharging()` (API 23; false before);
//! - Low Power Mode: `PowerManager.isPowerSaveMode()` (Battery Saver);
//! - the thermal state: `PowerManager.getCurrentThermalStatus()` (API 29; unknown before) -
//!   [`readings::thermal_of_android`].
//!
//! No permission is needed for any of them.

use jni::{
    objects::{JObject, JValue},
    JNIEnv,
};

use super::{readings, seen, BatteryState, ThermalState};
use crate::desktop::extra::sensors::{system_service, with_activity};

/// `BatteryManager.BATTERY_PROPERTY_CAPACITY`.
const BATTERY_PROPERTY_CAPACITY: i32 = 4;

/// `object.<method>()`, a `()Z` method; `None` when it throws (the exception cleared).
fn ask_flag(env: &mut JNIEnv<'_>, object: &JObject<'_>, method: &str) -> Option<bool> {
    match env
        .call_method(object, method, "()Z", &[])
        .and_then(|v| v.z())
    {
        Ok(answer) => Some(answer),
        Err(_) => {
            let _ = env.exception_clear();
            None
        }
    }
}

/// The two services' answers; `None` when the battery service cannot be asked.
fn read_with(env: &mut JNIEnv<'_>, activity: &JObject<'_>) -> Option<BatteryState> {
    let battery = system_service(env, activity, "batterymanager")?;
    let capacity = match env
        .call_method(
            &battery,
            "getIntProperty",
            "(I)I",
            &[JValue::Int(BATTERY_PROPERTY_CAPACITY)],
        )
        .and_then(|v| v.i())
    {
        Ok(capacity) => capacity,
        Err(_) => {
            let _ = env.exception_clear();
            i32::MIN
        }
    };
    let level_percent = readings::level_of_percent(i64::from(capacity));
    let present = level_percent != BatteryState::LEVEL_UNKNOWN;
    let charging = present && ask_flag(env, &battery, "isCharging").unwrap_or(false);
    let (low_power_mode, thermal) = match system_service(env, activity, "power") {
        Some(power) => {
            let low_power = ask_flag(env, &power, "isPowerSaveMode").unwrap_or(false);
            let thermal = match env
                .call_method(&power, "getCurrentThermalStatus", "()I", &[])
                .and_then(|v| v.i())
            {
                Ok(status) => readings::thermal_of_android(status),
                // Before API 29.
                Err(_) => {
                    let _ = env.exception_clear();
                    ThermalState::Unknown
                }
            };
            (low_power, thermal)
        }
        None => {
            let _ = env.exception_clear();
            (false, ThermalState::Unknown)
        }
    };
    Some(BatteryState {
        present,
        charging,
        level_percent: if present {
            level_percent
        } else {
            BatteryState::LEVEL_UNKNOWN
        },
        low_power_mode,
        thermal,
    })
}

/// A reading now, kept for the next query; nothing before the activity published its VM.
pub(super) fn refresh() {
    if let Some(state) = with_activity(read_with) {
        seen(state);
    }
}
