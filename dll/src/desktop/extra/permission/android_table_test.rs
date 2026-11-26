//! The Android permission table, by API level. No device.

use super::*;

static CAPABILITIES: [Capability; 19] = [
    Capability::Camera,
    Capability::Microphone,
    Capability::ScreenCapture,
    Capability::Geolocation,
    Capability::GeolocationBackground,
    Capability::Biometric,
    Capability::Motion,
    Capability::PhotoLibrary,
    Capability::PhotoLibraryWrite,
    Capability::Contacts,
    Capability::Calendars,
    Capability::Reminders,
    Capability::Notifications,
    Capability::Bluetooth,
    Capability::BluetoothBackground,
    Capability::NearbyWifi,
    Capability::LocalNetwork,
    Capability::AppTrackingTransparency,
    Capability::InputMonitoring,
];

/// Location is asked as the PAIR, in one request.
///
/// Mutation: only the fine permission (the system ignores such a request on some Android 12 releases).
#[test]
fn location_is_requested_as_the_fine_and_coarse_pair() {
    for sdk in [24, 29, 31, 33, 34, 35] {
        assert_eq!(
            needs(Capability::Geolocation, sdk),
            Needs::Ask(&[
                "android.permission.ACCESS_FINE_LOCATION",
                "android.permission.ACCESS_COARSE_LOCATION",
            ]),
            "API {sdk}"
        );
    }
}

/// Background location is its own request and comes with the foreground one before API 29.
#[test]
fn background_location_is_alone_and_only_from_api_29() {
    assert_eq!(needs(Capability::GeolocationBackground, 28), Needs::NothingToAsk);
    assert_eq!(
        needs(Capability::GeolocationBackground, 29),
        Needs::Ask(&["android.permission.ACCESS_BACKGROUND_LOCATION"])
    );
}

/// POST_NOTIFICATIONS, READ_MEDIA_IMAGES, BLUETOOTH_CONNECT and NEARBY_WIFI_DEVICES exist from the API
/// level that introduced them; below it the old permission or none.
///
/// Mutation: one table for every API level (a request for an unknown permission, answered Denied: every
/// Android 7-12 phone said notifications were refused).
#[test]
fn a_permission_that_the_api_level_does_not_have_is_not_asked() {
    assert_eq!(needs(Capability::Notifications, 32), Needs::NothingToAsk);
    assert_eq!(needs(Capability::Notifications, 33), Needs::Ask(&["android.permission.POST_NOTIFICATIONS"]));
    assert_eq!(needs(Capability::PhotoLibrary, 32), Needs::Ask(&["android.permission.READ_EXTERNAL_STORAGE"]));
    assert_eq!(needs(Capability::PhotoLibrary, 33), Needs::Ask(&["android.permission.READ_MEDIA_IMAGES"]));
    assert_eq!(needs(Capability::Bluetooth, 30), Needs::NothingToAsk);
    assert_eq!(needs(Capability::Bluetooth, 31), Needs::Ask(&["android.permission.BLUETOOTH_CONNECT"]));
    assert_eq!(needs(Capability::NearbyWifi, 32), Needs::Ask(&["android.permission.ACCESS_FINE_LOCATION"]));
    assert_eq!(needs(Capability::NearbyWifi, 33), Needs::Ask(&["android.permission.NEARBY_WIFI_DEVICES"]));
}

#[test]
fn camera_microphone_contacts_and_calendar_are_one_permission_at_every_level() {
    for sdk in [24, 28, 31, 34] {
        assert_eq!(needs(Capability::Camera, sdk), Needs::Ask(&["android.permission.CAMERA"]));
        assert_eq!(needs(Capability::Microphone, sdk), Needs::Ask(&["android.permission.RECORD_AUDIO"]));
        assert_eq!(needs(Capability::Contacts, sdk), Needs::Ask(&["android.permission.READ_CONTACTS"]));
        assert_eq!(needs(Capability::Calendars, sdk), needs(Capability::Reminders, sdk));
    }
}

/// Every capability has an answer at every level, no runtime request asks for nothing, and the ones that
/// are no Android permission are said so.
#[test]
fn every_capability_has_an_answer_at_every_api_level() {
    for sdk in 24..=36 {
        for capability in CAPABILITIES {
            match needs(capability, sdk) {
                Needs::Ask(list) => {
                    assert!(!list.is_empty(), "{capability:?} at API {sdk} asks for nothing");
                    assert!(list.iter().all(|p| p.starts_with("android.permission.")), "{capability:?}");
                }
                Needs::NothingToAsk | Needs::NotAPermission => {}
            }
        }
    }
    assert_eq!(needs(Capability::ScreenCapture, 34), Needs::NotAPermission);
    assert_eq!(needs(Capability::Motion, 34), Needs::NotAPermission);
    assert_eq!(needs(Capability::Biometric, 34), Needs::NothingToAsk);
}

/// A check of the pair: both granted is full, the approximate one alone is REDUCED (a user can grant
/// "approximate" only), none is denied or not yet asked by the rationale flag.
///
/// Mutation: approximate-only read as a denial.
#[test]
fn approximate_location_alone_is_a_reduced_grant() {
    assert_eq!(
        state_of_check(Capability::Geolocation, &[true, true], false),
        PermissionState::Granted(PermissionQuality::Full)
    );
    assert_eq!(
        state_of_check(Capability::Geolocation, &[false, true], false),
        PermissionState::Granted(PermissionQuality::Reduced)
    );
    assert_eq!(state_of_check(Capability::Geolocation, &[false, false], true), PermissionState::Denied);
    assert_eq!(state_of_check(Capability::Geolocation, &[false, false], false), PermissionState::NotDetermined);
    // fine without coarse is not a thing the system grants, and is not reduced
    assert_eq!(state_of_check(Capability::Geolocation, &[true, false], false), PermissionState::NotDetermined);
}

#[test]
fn a_single_permission_is_granted_or_not() {
    assert_eq!(state_of_check(Capability::Camera, &[true], false), PermissionState::Granted(PermissionQuality::Full));
    assert_eq!(state_of_check(Capability::Camera, &[false], true), PermissionState::Denied);
    assert_eq!(state_of_check(Capability::Camera, &[false], false), PermissionState::NotDetermined);
    assert_eq!(state_of_check(Capability::Camera, &[], false), PermissionState::NotDetermined, "an empty check grants nothing");
}

/// The answer to a request.
#[test]
fn the_answer_to_a_request_is_full_reduced_or_denied() {
    assert_eq!(state_of_result(Capability::Geolocation, true, true), PermissionState::Granted(PermissionQuality::Full));
    assert_eq!(
        state_of_result(Capability::Geolocation, false, true),
        PermissionState::Granted(PermissionQuality::Reduced),
        "approximate only"
    );
    assert_eq!(state_of_result(Capability::Geolocation, false, false), PermissionState::Denied);
    // for anything else a partial grant is no grant
    assert_eq!(state_of_result(Capability::Camera, false, true), PermissionState::Denied);
    assert_eq!(state_of_result(Capability::Camera, true, true), PermissionState::Granted(PermissionQuality::Full));
}
