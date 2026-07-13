//! macOS geolocation backend.
//!
//! macOS shares the `CLLocationManager` API with iOS — identical
//! authorization model, delegate method names, and `CLLocation` payload —
//! so this mirrors `geolocation/ios.rs`: a singleton retained
//! `CLLocationManager` + an `AzulMacLocationDelegate` (`ClassDecl`), driven
//! by `handle_event`, whose `locationManager:didUpdateLocations:` converts
//! the newest `CLLocation` to a `LocationFix` and parks it via
//! `push_location_fix`.
//!
//! macOS specifics:
//! - `Info.plist` needs `NSLocationWhenInUseUsageDescription` (legacy
//!   `NSLocationUsageDescription` for 10.14-).
//! - Catalina+ requires the app to be signed/notarized or the prompt is
//!   denied outright.
//! - Most Macs have no GPS; Apple fuses Wi-Fi BSSID + IP lookup, so
//!   accuracy is typically ~100 m.
//!
//! The delegate class name differs from the iOS one (`AzulMacLocationDelegate`
//! vs `AzulLocationDelegate`) so the two never collide if both objc runtimes
//! were ever loaded in one process.

use azul_layout::managers::geolocation::{
    push_location_fix, GeolocationDiffEvent, LocationFix,
};
#[cfg(target_os = "macos")]
use azul_layout::managers::permission::{
    push_async_result, Capability, PermissionQuality, PermissionState,
};

#[cfg(target_os = "macos")]
use objc::declare::ClassDecl;
#[cfg(target_os = "macos")]
use objc::runtime::{Class, Object, Sel};
#[cfg(target_os = "macos")]
use objc::{class, msg_send, sel, sel_impl, Encode, Encoding};
#[cfg(target_os = "macos")]
use std::ptr;
#[cfg(target_os = "macos")]
use std::sync::atomic::{AtomicUsize, Ordering};
#[cfg(target_os = "macos")]
use std::sync::Once;

/// `CLLocationCoordinate2D` — `{ latitude: double, longitude: double }`.
/// Defined locally so `msg_send!` can do the struct-return for
/// `[CLLocation coordinate]` with no CoreLocation-sys dependency.
#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Debug, Default, Copy, Clone)]
struct CLLocationCoordinate2D {
    latitude: f64,
    longitude: f64,
}
#[cfg(target_os = "macos")]
unsafe impl Encode for CLLocationCoordinate2D {
    fn encode() -> Encoding {
        unsafe { Encoding::from_str("{CLLocationCoordinate2D=dd}") }
    }
}

// Retained CLLocationManager + its (weakly-referenced) delegate, stored as
// usize since `*mut Object` isn't Send/Sync. `0` = not yet created.
#[cfg(target_os = "macos")]
static MANAGER: AtomicUsize = AtomicUsize::new(0);
#[cfg(target_os = "macos")]
static DELEGATE: AtomicUsize = AtomicUsize::new(0);

#[cfg(target_os = "macos")]
pub fn handle_event(event: &GeolocationDiffEvent) {
    unsafe {
        match event {
            GeolocationDiffEvent::Subscribe { config } => {
                subscribe(config.high_accuracy, config.background)
            }
            GeolocationDiffEvent::Reconfigure { config } => set_accuracy(config.high_accuracy),
            GeolocationDiffEvent::Release => release(),
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub fn handle_event(event: &GeolocationDiffEvent) {
    let _ = event;
}

#[cfg(target_os = "macos")]
unsafe fn manager() -> *mut Object {
    let existing = MANAGER.load(Ordering::Relaxed);
    if existing != 0 {
        return existing as *mut Object;
    }
    let cls = match Class::get("CLLocationManager") {
        Some(c) => c,
        None => return ptr::null_mut(),
    };
    let mgr: *mut Object = msg_send![cls, new]; // +1 retained, kept for reuse
    if mgr.is_null() {
        return ptr::null_mut();
    }
    let delegate_cls = get_or_create_delegate_class();
    let delegate_alloc: *mut Object = msg_send![delegate_cls, alloc];
    let delegate: *mut Object = msg_send![delegate_alloc, init]; // +1 retained
    let _: () = msg_send![mgr, setDelegate: delegate];
    MANAGER.store(mgr as usize, Ordering::Relaxed);
    DELEGATE.store(delegate as usize, Ordering::Relaxed);
    mgr
}

#[cfg(target_os = "macos")]
unsafe fn subscribe(high_accuracy: bool, background: bool) {
    let mgr = manager();
    if mgr.is_null() {
        return;
    }
    set_accuracy(high_accuracy);
    if background {
        let _: () = msg_send![mgr, requestAlwaysAuthorization];
    } else {
        let _: () = msg_send![mgr, requestWhenInUseAuthorization];
    }
    let _: () = msg_send![mgr, startUpdatingLocation];
}

#[cfg(target_os = "macos")]
unsafe fn set_accuracy(high_accuracy: bool) {
    let mgr = MANAGER.load(Ordering::Relaxed);
    if mgr == 0 {
        return;
    }
    let mgr = mgr as *mut Object;
    // kCLLocationAccuracyBest = -1.0; kCLLocationAccuracyHundredMeters = 100.0.
    let acc: f64 = if high_accuracy { -1.0 } else { 100.0 };
    let _: () = msg_send![mgr, setDesiredAccuracy: acc];
}

#[cfg(target_os = "macos")]
unsafe fn release() {
    let mgr = MANAGER.load(Ordering::Relaxed);
    if mgr == 0 {
        return;
    }
    let mgr = mgr as *mut Object;
    let _: () = msg_send![mgr, stopUpdatingLocation];
    // Keep the manager + delegate retained for the next subscribe.
}

#[cfg(target_os = "macos")]
fn get_or_create_delegate_class() -> &'static Class {
    static ONCE: Once = Once::new();
    static mut CLS: *const Class = ptr::null();
    unsafe {
        ONCE.call_once(|| {
            let superclass = class!(NSObject);
            let mut decl = ClassDecl::new("AzulMacLocationDelegate", superclass).unwrap();
            decl.add_method(
                sel!(locationManager:didUpdateLocations:),
                location_manager_did_update
                    as extern "C" fn(&Object, Sel, *mut Object, *mut Object),
            );
            decl.add_method(
                sel!(locationManagerDidChangeAuthorization:),
                location_manager_did_change_auth
                    as extern "C" fn(&Object, Sel, *mut Object),
            );
            CLS = decl.register();
        });
        &*CLS
    }
}

/// `locationManager:didUpdateLocations:` — convert the newest `CLLocation`
/// to a `LocationFix` and park it. iOS/macOS sentinels: negative accuracy =
/// invalid; `course`/`speed` < 0 = unknown; altitude valid only when
/// `verticalAccuracy > 0`.
#[cfg(target_os = "macos")]
extern "C" fn location_manager_did_update(
    _this: &Object,
    _cmd: Sel,
    _manager: *mut Object,
    locations: *mut Object,
) {
    unsafe {
        if locations.is_null() {
            return;
        }
        let loc: *mut Object = msg_send![locations, lastObject];
        if loc.is_null() {
            return;
        }
        let coord: CLLocationCoordinate2D = msg_send![loc, coordinate];
        let h_acc: f64 = msg_send![loc, horizontalAccuracy];
        let altitude: f64 = msg_send![loc, altitude];
        let v_acc: f64 = msg_send![loc, verticalAccuracy];
        let course: f64 = msg_send![loc, course];
        let speed: f64 = msg_send![loc, speed];

        let valid_alt = v_acc > 0.0;
        push_location_fix(LocationFix {
            latitude_deg: coord.latitude,
            longitude_deg: coord.longitude,
            accuracy_m: if h_acc >= 0.0 { h_acc as f32 } else { f32::NAN },
            altitude_m: if valid_alt { altitude as f32 } else { f32::NAN },
            altitude_accuracy_m: if valid_alt { v_acc as f32 } else { f32::NAN },
            heading_deg: if course >= 0.0 { course as f32 } else { f32::NAN },
            speed_mps: if speed >= 0.0 { speed as f32 } else { f32::NAN },
            timestamp_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0), // MWA-C-geolocation: was hardcoded 0,
        });
    }
}

/// `locationManagerDidChangeAuthorization:` (macOS 11+) — the user changed
/// the location grant. Route the new `CLAuthorizationStatus` into the
/// permission channel so the `PermissionManager` stays live, mirroring
/// Android's `onRequestPermissionsResult`.
#[cfg(target_os = "macos")]
extern "C" fn location_manager_did_change_auth(
    _this: &Object,
    _cmd: Sel,
    manager: *mut Object,
) {
    unsafe {
        if manager.is_null() {
            return;
        }
        let status: isize = msg_send![manager, authorizationStatus];
        push_async_result(Capability::Geolocation, auth_status_to_state(status));
    }
}

/// CLAuthorizationStatus → PermissionState. Both `authorizedAlways` (3) and
/// `authorizedWhenInUse` (4) are foreground-usable, so both map to Granted.
#[cfg(target_os = "macos")]
fn auth_status_to_state(status: isize) -> PermissionState {
    match status {
        0 => PermissionState::NotDetermined,
        1 => PermissionState::Restricted,
        2 => PermissionState::Denied,
        3 | 4 => PermissionState::Granted(PermissionQuality::Full),
        _ => PermissionState::NotDetermined,
    }
}
