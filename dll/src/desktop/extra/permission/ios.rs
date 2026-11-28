//! iOS permission backend.
//!
//! Request path ([`request`], called by the permission manager when a probe node
//! subscribes to a NotDetermined capability after a user gesture - never at
//! launch, never from a render or timer loop, one capability at a time) →
//! `AVCaptureDevice.requestAccess(for:)` for camera/mic,
//! `CLLocationManager.requestWhenInUse/AlwaysAuthorization` for geo,
//! `UNUserNotificationCenter.requestAuthorization` for notifications. Each
//! requires the corresponding `Info.plist` key (`NSCameraUsageDescription`,
//! …) - a missing key SIGABRTs the app on the first prompt; the backends check
//! it first and say so instead. A capability with no request path on iOS (photos:
//! Photos.framework is not loaded, Motion: its prompt is the first read of the
//! barometer or the step counter, Local Network: the first Bonjour use, Face ID: the
//! first biometric evaluation) is logged, never silently skipped.
//!
//! Probe path (`probe_status`) → the synchronous class/instance status
//! getters, collapsed onto `PermissionState` per research/08 §2. This is
//! implemented: it issues the real Objective-C calls and never prompts.
//! Classes are looked up with `Class::get` (not `class!`) so a missing
//! framework degrades to `NotDetermined` instead of panicking.
//!
//! iOS 14+ is assumed throughout (same baseline as the file picker's
//! `initForOpeningContentTypes:`), so the iOS 14 status APIs
//! (`authorizationStatusForAccessLevel:`, `accuracyAuthorization`) are
//! called directly without a `respondsToSelector:` guard.

use azul_layout::managers::permission::{
    Capability, PermissionDiffEvent, PermissionQuality, PermissionState,
};
use objc::{
    msg_send,
    runtime::{Class, Object},
    sel, sel_impl,
};

/// The permission manager's diff events. A Subscribe is the manager's own business (it calls [`request`] only when the
/// state is NotDetermined and a user gesture has been seen); a Release has nothing to give back here: the capture
/// sessions belong to the camera / microphone workers and the location manager to the geolocation backend.
pub fn handle_event(event: &PermissionDiffEvent) {
    let _ = event;
}

/// Ask the system for `capability`: the prompt of THAT capability only (one request, never a bundle), from the main
/// thread, on behalf of a user action (the caller holds the request until the shell has seen a gesture). The answer
/// reaches the manager through the app's queue (`AppServices::permission`) when the system gives it.
pub fn request(capability: Capability) {
    let Some(services) = crate::desktop::process_app::services() else {
        return;
    };
    match capability {
        #[cfg(feature = "objc2-av-foundation")]
        Capability::Camera => {
            crate::desktop::extra::camera::avf_auth::request_av_access_nonblocking(true, &services.permission);
        }
        #[cfg(feature = "objc2-av-foundation")]
        Capability::Microphone => {
            crate::desktop::extra::camera::avf_auth::request_av_access_nonblocking(false, &services.permission);
        }
        Capability::Geolocation => crate::desktop::extra::geolocation::ios::request_authorization(false),
        Capability::GeolocationBackground => crate::desktop::extra::geolocation::ios::request_authorization(true),
        Capability::Notifications => crate::desktop::notifications::request_permission(),
        other => {
            crate::plog_info!(
                "[permission] ios: {:?} has no request of its own (see extra/permission/ios.rs); it is asked by its first use",
                other
            );
        }
    }
}

pub fn probe_status(capability: Capability) -> PermissionState {
    match capability {
        // AVCaptureDevice gates camera and microphone separately, keyed by
        // the AVMediaType NSString (whose values are the FourCCs "vide" /
        // "soun").
        Capability::Camera => av_media_status("vide"),
        Capability::Microphone => av_media_status("soun"),
        Capability::Geolocation => cl_status(false),
        Capability::GeolocationBackground => cl_status(true),
        // PHAccessLevel: addOnly = 1, readWrite = 2.
        Capability::PhotoLibrary => ph_status(2),
        Capability::PhotoLibraryWrite => ph_status(1),
        Capability::AppTrackingTransparency => att_status(),
        // UN's getter is async-only; the notification backend reads it at
        // launch (did_finish_launching) and keeps the answer.
        Capability::Notifications => crate::desktop::notifications::apple_permission_state(),
        // The remaining capabilities (Motion, Contacts, Calendars,
        // Reminders, Bluetooth*, NearbyWifi, LocalNetwork, Biometric,
        // ScreenCapture) expose async-only or per-framework status APIs and
        // stay NotDetermined until their backend lands.
        _ => PermissionState::NotDetermined,
    }
}

/// Non-panicking class lookup — returns `None` (→ `NotDetermined`) when the
/// owning framework isn't linked, rather than aborting like `class!`.
fn lookup(name: &str) -> Option<&'static Class> {
    Class::get(name)
}

/// `[NSString stringWithUTF8String: s]`. `s` must be NUL-free (callers pass
/// static ASCII literals). Returns null on failure.
unsafe fn ns_string(s: &str) -> *mut Object {
    let cls = match lookup("NSString") {
        Some(c) => c,
        None => return core::ptr::null_mut(),
    };
    let cstr = match std::ffi::CString::new(s) {
        Ok(c) => c,
        Err(_) => return core::ptr::null_mut(),
    };
    msg_send![cls, stringWithUTF8String: cstr.as_ptr()]
}

/// `[AVCaptureDevice authorizationStatusForMediaType:]` → `PermissionState`.
fn av_media_status(media_four_cc: &str) -> PermissionState {
    let cls = match lookup("AVCaptureDevice") {
        Some(c) => c,
        None => return PermissionState::NotDetermined,
    };
    unsafe {
        let media = ns_string(media_four_cc);
        if media.is_null() {
            return PermissionState::NotDetermined;
        }
        let status: isize = msg_send![cls, authorizationStatusForMediaType: media];
        map_common(status)
    }
}

/// `CLLocationManager.authorizationStatus` (+ `accuracyAuthorization` to
/// distinguish precise vs reduced). `background` capabilities are only
/// satisfied by `authorizedAlways`; `authorizedWhenInUse` is foreground-only.
///
/// The status is the CLASS method (no manager is made per probe: the permission manager polls this); only the
/// accuracy of a grant needs an instance (iOS 14+), made at that moment.
fn cl_status(background: bool) -> PermissionState {
    let cls = match lookup("CLLocationManager") {
        Some(c) => c,
        None => return PermissionState::NotDetermined,
    };
    unsafe {
        // CLAuthorizationStatus: notDetermined 0, restricted 1, denied 2,
        // authorizedAlways 3, authorizedWhenInUse 4.
        let status: isize = msg_send![cls, authorizationStatus];
        match status {
            0 => PermissionState::NotDetermined,
            1 => PermissionState::Restricted,
            2 => PermissionState::Denied,
            3 => PermissionState::Granted(cl_quality(cls)),
            4 => {
                if background {
                    // When-in-use cannot satisfy a background subscription, and it is not a refusal either: the
                    // app may still ask for the upgrade to Always (once). `Denied` read as final.
                    PermissionState::NotDetermined
                } else {
                    PermissionState::Granted(cl_quality(cls))
                }
            }
            _ => PermissionState::NotDetermined,
        }
    }
}

/// `accuracyAuthorization`: fullAccuracy 0, reducedAccuracy 1 (iOS 14+): an instance property, read from a manager made
/// for the question and released.
unsafe fn cl_quality(cls: &Class) -> PermissionQuality {
    let mgr: *mut Object = msg_send![cls, new];
    if mgr.is_null() {
        return PermissionQuality::Full;
    }
    let acc: isize = msg_send![mgr, accuracyAuthorization];
    let _: () = msg_send![mgr, release];
    if acc == 1 {
        PermissionQuality::Reduced
    } else {
        PermissionQuality::Full
    }
}

/// `[PHPhotoLibrary authorizationStatusForAccessLevel:]` → `PermissionState`.
/// `limited` (Selected Photos) maps to `Granted { Reduced }`.
fn ph_status(access_level: isize) -> PermissionState {
    let cls = match lookup("PHPhotoLibrary") {
        Some(c) => c,
        None => return PermissionState::NotDetermined,
    };
    unsafe {
        // PHAuthorizationStatus: notDetermined 0, restricted 1, denied 2,
        // authorized 3, limited 4.
        let status: isize = msg_send![cls, authorizationStatusForAccessLevel: access_level];
        match status {
            0 => PermissionState::NotDetermined,
            1 => PermissionState::Restricted,
            2 => PermissionState::Denied,
            3 => PermissionState::Granted(PermissionQuality::Full),
            4 => PermissionState::Granted(PermissionQuality::Reduced),
            _ => PermissionState::NotDetermined,
        }
    }
}

/// `[ATTrackingManager trackingAuthorizationStatus]` → `PermissionState`.
fn att_status() -> PermissionState {
    let cls = match lookup("ATTrackingManager") {
        Some(c) => c,
        None => return PermissionState::NotDetermined,
    };
    unsafe {
        let status: isize = msg_send![cls, trackingAuthorizationStatus];
        map_common(status)
    }
}

/// The status layout shared by AVAuthorizationStatus and
/// ATTrackingManagerAuthorizationStatus: 0 NotDetermined, 1 Restricted,
/// 2 Denied, 3 Authorized.
fn map_common(status: isize) -> PermissionState {
    match status {
        0 => PermissionState::NotDetermined,
        1 => PermissionState::Restricted,
        2 => PermissionState::Denied,
        3 => PermissionState::Granted(PermissionQuality::Full),
        _ => PermissionState::NotDetermined,
    }
}
