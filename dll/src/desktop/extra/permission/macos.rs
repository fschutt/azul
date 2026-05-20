//! macOS permission backend.
//!
//! macOS shares its permission surfaces with iOS — camera / mic / GPS /
//! photos route through the same `AVCaptureDevice` / `CLLocationManager` /
//! `PHPhotoLibrary` class methods, so `probe_status` mirrors the iOS
//! backend (minus `ATTrackingManager`, which is iOS-only). Differences
//! that matter later:
//!
//! - macOS adds the `TCC` (Transparency, Consent, Control) database; the
//!   first prompt is OS-modal, subsequent reads come from the cache.
//! - Grant is keyed to the bundle ID — unsigned binaries can't request
//!   most permissions (no `TCC.db` entry), so permission-bearing features
//!   require code-signing.
//! - Screen capture has a macOS-only preflight (`CGPreflightScreenCaptureAccess`)
//!   not yet wired here.
//!
//! The async request path (`handle_event`) needs ObjC completion blocks
//! (`requestAccessForMediaType:completionHandler:`), same as iOS — left as
//! a no-op until that lands. Classes are resolved with `Class::get` so a
//! missing framework degrades to `NotDetermined` rather than aborting.

use azul_layout::managers::permission::{
    Capability, PermissionDiffEvent, PermissionQuality, PermissionState,
};

#[cfg(target_os = "macos")]
use objc::runtime::{Class, Object};
#[cfg(target_os = "macos")]
use objc::{msg_send, sel, sel_impl};

pub fn handle_event(event: &PermissionDiffEvent) {
    let _ = event;
    // TODO(P1.2+): requestAccessForMediaType:completionHandler: (camera/mic),
    // PHPhotoLibrary requestAuthorizationForAccessLevel:handler:, etc. All
    // take ObjC completion blocks — wired with the iOS request path.
}

#[cfg(target_os = "macos")]
pub fn probe_status(capability: Capability) -> PermissionState {
    match capability {
        Capability::Camera => av_media_status("vide"),
        Capability::Microphone => av_media_status("soun"),
        Capability::Geolocation => cl_status(false),
        Capability::GeolocationBackground => cl_status(true),
        // PHAccessLevel: addOnly = 1, readWrite = 2.
        Capability::PhotoLibrary => ph_status(2),
        Capability::PhotoLibraryWrite => ph_status(1),
        // ScreenCapture has a macOS-only CGPreflightScreenCaptureAccess
        // path (not wired yet); everything else here is iOS-only or has no
        // synchronous macOS status getter.
        _ => PermissionState::NotDetermined,
    }
}

#[cfg(not(target_os = "macos"))]
pub fn probe_status(capability: Capability) -> PermissionState {
    let _ = capability;
    PermissionState::NotDetermined
}

/// Non-panicking class lookup — returns `None` (→ `NotDetermined`) when the
/// owning framework isn't linked, rather than aborting like `class!`.
#[cfg(target_os = "macos")]
fn lookup(name: &str) -> Option<&'static Class> {
    Class::get(name)
}

/// `[NSString stringWithUTF8String: s]`. `s` must be NUL-free (callers pass
/// static ASCII literals). Returns null on failure.
#[cfg(target_os = "macos")]
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
#[cfg(target_os = "macos")]
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

/// `CLLocationManager.authorizationStatus` (+ `accuracyAuthorization`).
/// `background` capabilities are only satisfied by `authorizedAlways`.
#[cfg(target_os = "macos")]
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
            3 => PermissionState::Granted {
                quality: cl_quality(mgr),
            },
            4 => {
                if background {
                    PermissionState::Denied
                } else {
                    PermissionState::Granted {
                        quality: cl_quality(mgr),
                    }
                }
            }
            _ => PermissionState::NotDetermined,
        };
        let _: () = msg_send![mgr, release];
        result
    }
}

/// `accuracyAuthorization`: fullAccuracy 0, reducedAccuracy 1 (macOS 11+).
#[cfg(target_os = "macos")]
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
#[cfg(target_os = "macos")]
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
            3 => PermissionState::Granted {
                quality: PermissionQuality::Full,
            },
            4 => PermissionState::Granted {
                quality: PermissionQuality::Reduced,
            },
            _ => PermissionState::NotDetermined,
        }
    }
}

/// The status layout shared by AVAuthorizationStatus: 0 NotDetermined,
/// 1 Restricted, 2 Denied, 3 Authorized.
#[cfg(target_os = "macos")]
fn map_common(status: isize) -> PermissionState {
    match status {
        0 => PermissionState::NotDetermined,
        1 => PermissionState::Restricted,
        2 => PermissionState::Denied,
        3 => PermissionState::Granted {
            quality: PermissionQuality::Full,
        },
        _ => PermissionState::NotDetermined,
    }
}
