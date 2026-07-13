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
//! - Grant is keyed to the *responsible process*: for a bundled app that's
//!   the bundle ID; for a bare binary launched from a terminal it's the
//!   terminal app — which CAN be granted camera/mic access, so unsigned
//!   demo binaries still get a working prompt.
//! - Screen capture has a macOS-only preflight
//!   (`CGPreflightScreenCaptureAccess`), wired in `probe_status` via a
//!   runtime `dlopen` of CoreGraphics.
//!
//! The async request path (`handle_event`) needs ObjC completion blocks
//! (`requestAccessForMediaType:completionHandler:`) which the old `objc`
//! crate used below has no support for — camera/mic requests route through
//! the objc2-based `camera::avf_auth` helper instead (fire-and-forget, the
//! completion block just logs; the next `probe_status` poll observes the new
//! state). Classes are resolved with `Class::get` so a missing framework
//! degrades to `NotDetermined` rather than aborting.

use azul_layout::managers::permission::{
    Capability, PermissionDiffEvent, PermissionQuality, PermissionState,
};

#[cfg(target_os = "macos")]
use objc::runtime::{Class, Object};
#[cfg(target_os = "macos")]
use objc::{msg_send, sel, sel_impl};

pub fn handle_event(event: &PermissionDiffEvent) {
    match event {
        PermissionDiffEvent::Subscribe { capability, .. } => {
            // MWA-C-permission: seed the manager with the CURRENT TCC state
            // on every Subscribe (probe_status was implemented but had zero
            // callers — dead code while the manager cache stayed
            // NotDetermined). Non-prompting reads only; skip NotDetermined
            // so an unknown probe never regresses a live state.
            let probed = probe_status(*capability);
            if probed != PermissionState::NotDetermined {
                azul_layout::managers::permission::push_async_result(*capability, probed);
            }
            match capability {
                Capability::Camera => request_av_capture_access(true),
                Capability::Microphone => request_av_capture_access(false),
                // Geolocation: the CLLocationManager prompt is owned by the
                // parallel geolocation backend (extra/geolocation/macos.rs);
                // its auth-change delegate pushes async results. Seeding
                // above keeps get_permission_status truthful meanwhile.
                // ScreenCapture: preflight seeded above; the prompt fires
                // from the screencap backend on first use (which now pushes
                // its outcome too).
                // PhotoLibrary etc.: probe-seeded; prompt wiring lands with
                // the matching widgets.
                _ => {}
            }
        }
        // Release / Reconfigure: the capture sessions are owned by the
        // camera / mic worker threads (torn down via their close paths), so
        // there is nothing to release on the permission side.
        PermissionDiffEvent::Release { .. } | PermissionDiffEvent::Reconfigure { .. } => {}
    }
}

/// Fire-and-forget `requestAccessForMediaType:completionHandler:` for camera
/// (`video == true`) / mic. Routed through the objc2-based
/// [`crate::desktop::extra::camera::avf_auth`] helper because the old `objc`
/// crate used in this file has no block support. Non-blocking — safe from the
/// frame loop; the completion block just logs, and the next `probe_status`
/// poll picks up the new state.
#[cfg(all(target_os = "macos", feature = "objc2-av-foundation"))]
fn request_av_capture_access(video: bool) {
    crate::desktop::extra::camera::avf_auth::request_av_access_nonblocking(video);
}

/// Without the AVFoundation backend there is no request path — log so the
/// silence is explainable, and let `probe_status` keep reporting the state.
#[cfg(not(all(target_os = "macos", feature = "objc2-av-foundation")))]
fn request_av_capture_access(video: bool) {
    crate::plog_warn!(
        "[permission] macos: no AVFoundation request path built in (feature \
         objc2-av-foundation off) — cannot prompt for {} access",
        if video { "camera" } else { "microphone" }
    );
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
        Capability::ScreenCapture => screen_capture_status(),
        // Everything else here is iOS-only or has no synchronous macOS
        // status getter.
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
            3 => PermissionState::Granted(cl_quality(mgr)),
            4 => {
                if background {
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
            3 => PermissionState::Granted(PermissionQuality::Full),
            4 => PermissionState::Granted(PermissionQuality::Reduced),
            _ => PermissionState::NotDetermined,
        }
    }
}

/// `CGPreflightScreenCaptureAccess()` (macOS 10.15+), resolved at runtime
/// via `dlopen` of CoreGraphics so a missing symbol (pre-10.15) degrades to
/// `NotDetermined` rather than failing to link.
///
/// `false` from the preflight means "not currently granted" — TCC does not
/// distinguish never-asked from user-denied on this path, and a request
/// (`CGRequestScreenCaptureAccess`) may still produce a prompt. So `false`
/// maps to `NotDetermined` (keeps `could_re_prompt()` true) rather than
/// `Denied`.
#[cfg(all(target_os = "macos", feature = "libloading"))]
fn screen_capture_status() -> PermissionState {
    // Resolved once (probes can run per-frame); the Library is leaked so the
    // cached fn pointer stays valid — CoreGraphics never unloads anyway.
    static PREFLIGHT: std::sync::OnceLock<Option<unsafe extern "C" fn() -> bool>> =
        std::sync::OnceLock::new();
    let preflight = PREFLIGHT.get_or_init(|| unsafe {
        let lib = libloading::Library::new(
            "/System/Library/Frameworks/CoreGraphics.framework/CoreGraphics",
        )
        .ok()?;
        let sym: libloading::Symbol<'_, unsafe extern "C" fn() -> bool> =
            lib.get(b"CGPreflightScreenCaptureAccess\0").ok()?;
        let f = *sym;
        std::mem::forget(lib);
        Some(f)
    });
    match preflight {
        Some(f) => {
            if unsafe { f() } {
                PermissionState::Granted(PermissionQuality::Full)
            } else {
                PermissionState::NotDetermined
            }
        }
        None => PermissionState::NotDetermined,
    }
}

/// No `libloading` → no runtime CoreGraphics lookup; report `NotDetermined`.
#[cfg(all(target_os = "macos", not(feature = "libloading")))]
fn screen_capture_status() -> PermissionState {
    PermissionState::NotDetermined
}

/// The status layout shared by AVAuthorizationStatus: 0 NotDetermined,
/// 1 Restricted, 2 Denied, 3 Authorized.
#[cfg(target_os = "macos")]
fn map_common(status: isize) -> PermissionState {
    match status {
        0 => PermissionState::NotDetermined,
        1 => PermissionState::Restricted,
        2 => PermissionState::Denied,
        3 => PermissionState::Granted(PermissionQuality::Full),
        _ => PermissionState::NotDetermined,
    }
}
