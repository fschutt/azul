//! iOS geolocation backend.
//!
//! `handle_event` drives a singleton `CLLocationManager` + an
//! `AzulLocationDelegate` NSObject (registered via `ClassDecl`, same
//! pattern as the file picker's delegate). `Subscribe` sets the accuracy,
//! requests authorization (`requestWhenInUseAuthorization`, or
//! `requestAlwaysAuthorization` for `config.background`), and
//! `startUpdatingLocation`; `Reconfigure` just adjusts `desiredAccuracy`;
//! `Release` calls `stopUpdatingLocation` (the manager is kept retained
//! for reuse). The delegate's `locationManager:didUpdateLocations:`
//! converts the newest `CLLocation` to a `LocationFix` and parks it via
//! `push_location_fix`, which the layout pass folds into the manager.
//!
//! `Info.plist` must carry `NSLocationWhenInUseUsageDescription` (and
//! `NSLocationAlwaysAndWhenInUseUsageDescription` for background) or the
//! authorization request silently no-ops. The `didChangeAuthorization`
//! delegate (routing status into the PermissionManager) is a follow-up;
//! the permission backend already probes location status synchronously.

use azul_layout::managers::geolocation::{
    push_location_fix, GeolocationDiffEvent, LocationFix,
};

#[cfg(target_os = "ios")]
use objc::declare::ClassDecl;
#[cfg(target_os = "ios")]
use objc::runtime::{Class, Object, Sel};
#[cfg(target_os = "ios")]
use objc::{class, msg_send, sel, sel_impl, Encode, Encoding};
#[cfg(target_os = "ios")]
use std::ptr;
#[cfg(target_os = "ios")]
use std::sync::atomic::{AtomicUsize, Ordering};
#[cfg(target_os = "ios")]
use std::sync::Once;

/// `CLLocationCoordinate2D` — `{ latitude: double, longitude: double }`.
/// Defined locally (like `CGPoint` in the iOS shell) so `msg_send!` can do
/// the struct-return for `[CLLocation coordinate]` with no CoreLocation-sys
/// dependency.
#[cfg(target_os = "ios")]
#[repr(C)]
#[derive(Debug, Default, Copy, Clone)]
struct CLLocationCoordinate2D {
    latitude: f64,
    longitude: f64,
}
#[cfg(target_os = "ios")]
unsafe impl Encode for CLLocationCoordinate2D {
    fn encode() -> Encoding {
        unsafe { Encoding::from_str("{CLLocationCoordinate2D=dd}") }
    }
}

// Retained CLLocationManager + its delegate. `CLLocationManager.delegate`
// is weak, so we must own the delegate too. Stored as usize because
// `*mut Object` isn't Send/Sync; `0` = not yet created. One subscription
// at a time (the manager is refcount-driven). All access is on the main
// thread (handle_event runs in the layout pass; CLLocationManager delivers
// on the thread it was started on).
#[cfg(target_os = "ios")]
static MANAGER: AtomicUsize = AtomicUsize::new(0);
#[cfg(target_os = "ios")]
static DELEGATE: AtomicUsize = AtomicUsize::new(0);

#[cfg(target_os = "ios")]
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

#[cfg(not(target_os = "ios"))]
pub fn handle_event(event: &GeolocationDiffEvent) {
    let _ = event;
}

/// Create + retain the manager and its delegate once; returns null if
/// CoreLocation isn't linked.
#[cfg(target_os = "ios")]
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

#[cfg(target_os = "ios")]
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

#[cfg(target_os = "ios")]
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

#[cfg(target_os = "ios")]
unsafe fn release() {
    let mgr = MANAGER.load(Ordering::Relaxed);
    if mgr == 0 {
        return;
    }
    let mgr = mgr as *mut Object;
    let _: () = msg_send![mgr, stopUpdatingLocation];
    // Keep the manager + delegate retained for the next subscribe.
}

#[cfg(target_os = "ios")]
fn get_or_create_delegate_class() -> &'static Class {
    static ONCE: Once = Once::new();
    static mut CLS: *const Class = ptr::null();
    unsafe {
        ONCE.call_once(|| {
            let superclass = class!(NSObject);
            let mut decl = ClassDecl::new("AzulLocationDelegate", superclass).unwrap();
            decl.add_method(
                sel!(locationManager:didUpdateLocations:),
                location_manager_did_update
                    as extern "C" fn(&Object, Sel, *mut Object, *mut Object),
            );
            CLS = decl.register();
        });
        &*CLS
    }
}

/// `locationManager:didUpdateLocations:` — convert the newest `CLLocation`
/// to a `LocationFix` and park it. iOS sentinels: negative accuracy =
/// invalid; `course`/`speed` < 0 = unknown; altitude is valid only when
/// `verticalAccuracy > 0`.
#[cfg(target_os = "ios")]
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
            timestamp_ms: 0,
        });
    }
}
