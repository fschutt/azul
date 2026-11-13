//! macOS and iOS:
//!
//! - Low Power Mode and the thermal state, on both: `NSProcessInfo`'s `isLowPowerModeEnabled`
//!   (iOS 9, macOS 12; asked only where the process info answers it) and `thermalState` (iOS
//!   11, macOS 10.10.3) - [`readings::thermal_of_apple`]. Both are thread-safe, read on the
//!   monitor thread on macOS.
//! - The battery, on macOS: IOKit's power sources (the internal battery's capacity, `Is
//!   Charging`, its power source state), through the power state's reading, which loads them
//!   (`power::platform::internal_battery`). A Mac without an internal battery has none.
//! - The battery, on iOS: `UIDevice`'s battery monitoring (`batteryLevel` -
//!   [`readings::level_of_fraction`] - and `batteryState`: unplugged, charging, full). UIKit's,
//!   so the monitor thread hands each reading to the main queue (`dispatch_async_f`), which
//!   reads the device there and keeps the reading; the monitor thread never waits for it. The
//!   first reading turns the battery monitoring on, so its level may be unknown until the next.

use objc2::runtime::AnyObject;

#[cfg(target_os = "ios")]
use super::readings::level_of_fraction;
use super::{readings, seen, BatteryState, ThermalState};

/// Low Power Mode and the thermal state from `NSProcessInfo`.
fn process_info() -> (bool, ThermalState) {
    objc2::rc::autoreleasepool(|_| {
        // SAFETY: plain Foundation getters; each selector is asked about first where an older
        // system lacks it.
        unsafe {
            let info: *mut AnyObject = objc2::msg_send![objc2::class!(NSProcessInfo), processInfo];
            if info.is_null() {
                return (false, ThermalState::Unknown);
            }
            let has_thermal: bool =
                objc2::msg_send![info, respondsToSelector: objc2::sel!(thermalState)];
            let thermal = if has_thermal {
                let state: isize = objc2::msg_send![info, thermalState];
                readings::thermal_of_apple(state)
            } else {
                ThermalState::Unknown
            };
            let has_low_power: bool =
                objc2::msg_send![info, respondsToSelector: objc2::sel!(isLowPowerModeEnabled)];
            let low_power = has_low_power && {
                let enabled: bool = objc2::msg_send![info, isLowPowerModeEnabled];
                enabled
            };
            (low_power, thermal)
        }
    })
}

/// macOS: a reading now (on the monitor thread), kept for the next query.
#[cfg(target_os = "macos")]
pub(super) fn refresh() {
    let (low_power_mode, thermal) = process_info();
    let (present, charging, level_percent) =
        crate::desktop::extra::sensors::power::platform::internal_battery().unwrap_or((
            false,
            false,
            BatteryState::LEVEL_UNKNOWN,
        ));
    seen(BatteryState {
        present,
        charging,
        level_percent,
        low_power_mode,
        thermal,
    });
}

/// iOS: hands a reading to the main queue (UIKit's), which keeps it for the next query.
#[cfg(target_os = "ios")]
pub(super) fn refresh() {
    use std::ffi::c_void;

    extern "C" {
        /// libdispatch's main queue (`dispatch_get_main_queue()` is a macro for its address).
        static _dispatch_main_q: c_void;
        fn dispatch_async_f(
            queue: *const c_void,
            context: *mut c_void,
            work: extern "C" fn(*mut c_void),
        );
    }

    extern "C" fn on_main(_context: *mut c_void) {
        seen(ios_reading());
    }

    // SAFETY: the main queue lives for the process's life; the work takes no context.
    unsafe {
        dispatch_async_f(
            std::ptr::addr_of!(_dispatch_main_q),
            std::ptr::null_mut(),
            on_main,
        );
    }
}

/// iOS, on the main queue: `UIDevice`'s battery (its monitoring turned on first) and the
/// process info's Low Power Mode and thermal state.
#[cfg(target_os = "ios")]
fn ios_reading() -> BatteryState {
    /// `UIDeviceBatteryState`: unknown, unplugged, charging, full.
    const STATE_UNKNOWN: isize = 0;
    const STATE_CHARGING: isize = 2;
    const STATE_FULL: isize = 3;

    let (low_power_mode, thermal) = process_info();
    let battery = objc2::rc::autoreleasepool(|_| {
        // SAFETY: UIKit getters and the monitoring switch, on the main queue.
        unsafe {
            let device: *mut AnyObject = objc2::msg_send![objc2::class!(UIDevice), currentDevice];
            if device.is_null() {
                return None;
            }
            let monitoring: bool = objc2::msg_send![device, isBatteryMonitoringEnabled];
            if !monitoring {
                let _: () = objc2::msg_send![device, setBatteryMonitoringEnabled: true];
            }
            let level: f32 = objc2::msg_send![device, batteryLevel];
            let state: isize = objc2::msg_send![device, batteryState];
            Some((level, state))
        }
    });
    let (present, charging, level_percent) = match battery {
        Some((level, state)) if state != STATE_UNKNOWN => (
            true,
            state == STATE_CHARGING || state == STATE_FULL,
            level_of_fraction(level),
        ),
        _ => (false, false, BatteryState::LEVEL_UNKNOWN),
    };
    BatteryState {
        present,
        charging,
        level_percent,
        low_power_mode,
        thermal,
    }
}
