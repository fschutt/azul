//! Which Android runtime permissions a capability needs, and what their answers mean, by API level.
//!
//! Pure tables, compiled on every host (the JNI that asks the system is in `android.rs`). The shell asked
//! for ONE permission per capability whatever the API level, which is wrong in each of these ways, all
//! from the platform documentation:
//!
//! * Location: "don't request the ACCESS_FINE_LOCATION permission by itself. Instead, request both
//!   ACCESS_FINE_LOCATION and ACCESS_COARSE_LOCATION in a single runtime request. If you try to request
//!   only ACCESS_FINE_LOCATION, the system ignores the request on some releases of Android 12"
//!   (developer.android.com/develop/sensors-and-location/location/permissions/runtime). A user may grant
//!   the approximate one only: that is `Granted(Reduced)`, not a denial.
//! * Notifications: POST_NOTIFICATIONS exists from API 33. Below it nothing is asked and notifications are
//!   allowed (`areNotificationsEnabled`): the request was made for an unknown permission and answered
//!   `Denied` on every Android 7-12 phone.
//! * Photos: READ_MEDIA_IMAGES is API 33; below it the photo library is READ_EXTERNAL_STORAGE.
//! * Bluetooth: BLUETOOTH_CONNECT is API 31; below it BLUETOOTH is an install-time permission.
//! * Nearby Wi-Fi: NEARBY_WIFI_DEVICES is API 33; before it Wi-Fi scans need the fine location.
//! * Background location is its own request, never together with the foreground one ("If you request a
//!   foreground location permission and the background location permission at the same time, the system
//!   ignores the request"), and below API 29 it comes with the foreground permission.

use azul_layout::managers::permission::{Capability, PermissionQuality, PermissionState};

/// What a capability needs at an API level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Needs {
    /// Nothing to ask: an install-time permission, or none at this API level. The capability is usable.
    NothingToAsk,
    /// Runtime permissions to ask for TOGETHER, in ONE request.
    Ask(&'static [&'static str]),
    /// Not a runtime permission of Android (a consent dialog, a concept of another platform).
    NotAPermission,
}

/// What `capability` needs on a device running API level `sdk_int`.
#[must_use]
pub fn needs(capability: Capability, sdk_int: u32) -> Needs {
    use Capability::*;
    match capability {
        Camera => Needs::Ask(&["android.permission.CAMERA"]),
        Microphone => Needs::Ask(&["android.permission.RECORD_AUDIO"]),
        Geolocation => Needs::Ask(&[
            "android.permission.ACCESS_FINE_LOCATION",
            "android.permission.ACCESS_COARSE_LOCATION",
        ]),
        GeolocationBackground => {
            if sdk_int >= 29 {
                Needs::Ask(&["android.permission.ACCESS_BACKGROUND_LOCATION"])
            } else {
                Needs::NothingToAsk
            }
        }
        PhotoLibrary => {
            if sdk_int >= 33 {
                Needs::Ask(&["android.permission.READ_MEDIA_IMAGES"])
            } else {
                Needs::Ask(&["android.permission.READ_EXTERNAL_STORAGE"])
            }
        }
        Contacts => Needs::Ask(&["android.permission.READ_CONTACTS"]),
        // Android folds reminders into the calendar permission.
        Calendars | Reminders => Needs::Ask(&["android.permission.READ_CALENDAR"]),
        Notifications => {
            if sdk_int >= 33 {
                Needs::Ask(&["android.permission.POST_NOTIFICATIONS"])
            } else {
                Needs::NothingToAsk
            }
        }
        Bluetooth => {
            if sdk_int >= 31 {
                Needs::Ask(&["android.permission.BLUETOOTH_CONNECT"])
            } else {
                Needs::NothingToAsk
            }
        }
        NearbyWifi => {
            if sdk_int >= 33 {
                Needs::Ask(&["android.permission.NEARBY_WIFI_DEVICES"])
            } else {
                Needs::Ask(&["android.permission.ACCESS_FINE_LOCATION"])
            }
        }
        // USE_BIOMETRIC is a normal (install-time) permission; whether a finger is enrolled is the
        // biometric module's question, not a permission's.
        Biometric => Needs::NothingToAsk,
        // Scoped storage needs no runtime permission to ADD media; the motion sensors need none;
        // ScreenCapture is a MediaProjection consent dialog; the rest are concepts of other platforms
        // (and `InputMonitoring` is macOS's TCC gate: Android exposes no raw HID at all).
        PhotoLibraryWrite
        | ScreenCapture
        | Motion
        | BluetoothBackground
        | LocalNetwork
        | AppTrackingTransparency
        | InputMonitoring => Needs::NotAPermission,
    }
}

/// The state from a CHECK (`checkSelfPermission` of each permission of the request, in order) and
/// `shouldShowRequestPermissionRationale` (true only after a real denial of one of them).
#[must_use]
pub fn state_of_check(capability: Capability, granted: &[bool], rationale: bool) -> PermissionState {
    if !granted.is_empty() && granted.iter().all(|g| *g) {
        return PermissionState::Granted(PermissionQuality::Full);
    }
    // Approximate location only: the second permission of the pair without the first.
    if capability == Capability::Geolocation && granted.len() == 2 && granted[1] && !granted[0] {
        return PermissionState::Granted(PermissionQuality::Reduced);
    }
    if rationale {
        PermissionState::Denied
    } else {
        // A fresh install and "don't ask again" both read false: asking learns which it was.
        PermissionState::NotDetermined
    }
}

/// The state from the answer to a request: whether every permission of it was granted, and whether any
/// was. A user who grants the approximate location only has granted a REDUCED location, not denied it.
#[must_use]
pub fn state_of_result(capability: Capability, all_granted: bool, any_granted: bool) -> PermissionState {
    if all_granted {
        PermissionState::Granted(PermissionQuality::Full)
    } else if any_granted && capability == Capability::Geolocation {
        PermissionState::Granted(PermissionQuality::Reduced)
    } else {
        PermissionState::Denied
    }
}

#[cfg(test)]
#[path = "android_table_test.rs"]
mod tests;
