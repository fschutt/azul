//! iOS permission backend.
//!
//! Subscribe path (`handle_event`) → `AVCaptureDevice.requestAccess(for:)`
//! for camera/mic, `CLLocationManager.requestWhenInUseAuthorization` for
//! geo, `PHPhotoLibrary.requestAuthorization(for:)` for photos, etc. Each
//! requires the corresponding `Info.plist` key (`NSCameraUsageDescription`,
//! …) — a missing key SIGABRTs the app on the first prompt. These async
//! prompts land with the `CLLocationManager` / `AVCaptureSession` session
//! work; `handle_event` is still a no-op below.
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
use objc::runtime::{Class, Object};
use objc::{msg_send, sel, sel_impl};

pub fn handle_event(event: &PermissionDiffEvent) {
    let _ = event;
    // TODO(P1.2+): issue the matching request<X>Access / native release.
    // The synchronous read path (probe_status) is wired below; the async
    // subscribe/release prompts land with the CLLocationManager /
    // AVCaptureSession session work.
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
        // The remaining capabilities (Motion, Contacts, Calendars,
        // Reminders, Notifications, Bluetooth*, NearbyWifi, LocalNetwork,
        // Biometric, ScreenCapture) expose async-only or per-framework
        // status APIs and stay NotDetermined until their backend lands.
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
fn cl_status(background: bool) -> PermissionState {
    let cls = match lookup("CLLocationManager") {
        Some(c) => c,
        None => return PermissionState::NotDetermined,
    };
    unsafe {
        let mgr: *mut Object = msg_send![cls, new];
        if mgr.is_null() {
            return PermissionState::NotDetermined;
        }
        // CLAuthorizationStatus: notDetermined 0, restricted 1, denied 2,
        // authorizedAlways 3, authorizedWhenInUse 4.
        let status: isize = msg_send![mgr, authorizationStatus];
        let result = match status {
            0 => PermissionState::NotDetermined,
            1 => PermissionState::Restricted,
            2 => PermissionState::Denied,
            3 => PermissionState::Granted(cl_quality(mgr)),
            4 => {
                if background {
                    // When-in-use can't satisfy a background subscription.
                    PermissionState::Denied
                } else {
                    PermissionState::Granted(cl_quality(mgr))
                }
            }
            _ => PermissionState::NotDetermined,
        };
        let _: () = msg_send![mgr, release];
        result
    }
}

/// `accuracyAuthorization`: fullAccuracy 0, reducedAccuracy 1 (iOS 14+).
unsafe fn cl_quality(mgr: *mut Object) -> PermissionQuality {
    let acc: isize = msg_send![mgr, accuracyAuthorization];
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
