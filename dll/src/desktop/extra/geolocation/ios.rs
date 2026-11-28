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
//! converts the newest `CLLocation` to a `LocationFix` and parks it in the
//! app's `GeolocationChannel` (reached through `process_app`: the delegate's
//! methods carry no context), which the layout pass folds into the manager.
//!
//! `Info.plist` must carry `NSLocationWhenInUseUsageDescription` (and
//! `NSLocationAlwaysAndWhenInUseUsageDescription` for background) or the
//! authorization request silently no-ops. The `didChangeAuthorization`
//! delegate (routing status into the PermissionManager) is a follow-up;
//! the permission backend already probes location status synchronously.

#[cfg(target_os = "ios")]
use std::ptr;
#[cfg(target_os = "ios")]
use std::sync::atomic::{AtomicUsize, Ordering};
#[cfg(target_os = "ios")]
use std::sync::Once;

use azul_layout::managers::geolocation::{GeolocationDiffEvent, LocationFix};
#[cfg(target_os = "ios")]
use azul_layout::managers::permission::{Capability, PermissionQuality, PermissionState};
#[cfg(target_os = "ios")]
use objc::declare::ClassDecl;
#[cfg(target_os = "ios")]
use objc::runtime::{Class, Object, Sel};
#[cfg(target_os = "ios")]
use objc::{class, msg_send, sel, sel_impl, Encode, Encoding};

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

/// `NSLocationAlwaysAndWhenInUseUsageDescription`: what a request for the background grant shows.
#[cfg(target_os = "ios")]
const LOCATION_ALWAYS_KEY: &str = "NSLocationAlwaysAndWhenInUseUsageDescription";

#[cfg(target_os = "ios")]
unsafe fn subscribe(high_accuracy: bool, background: bool) {
    let mgr = manager();
    if mgr.is_null() {
        crate::log_error!(
            crate::desktop::shell2::common::debug_server::LogCategory::Platform,
            "[geolocation] CoreLocation is not available: no location updates"
        );
        return;
    }
    set_accuracy(high_accuracy);
    request_authorization_on(mgr, background);
    let _: () = msg_send![mgr, startUpdatingLocation];
}

/// The request for the location grant, made only when the system can show it: without the purpose string it shows
/// NOTHING and the status never leaves "not determined" - a silent no-op, which this says out loud (the app is not
/// stopped, as it is for a camera or a motion request). One request, for the grant asked for.
#[cfg(target_os = "ios")]
unsafe fn request_authorization_on(mgr: *mut Object, background: bool) {
    use crate::desktop::extra::info_plist::{has_key, is_bundled, keys, missing_key_message};

    let key = if background { LOCATION_ALWAYS_KEY } else { keys::LOCATION_WHEN_IN_USE };
    if is_bundled() && !has_key(key) {
        crate::log_error!(
            crate::desktop::shell2::common::debug_server::LogCategory::Platform,
            "[geolocation] {}",
            missing_key_message("the location request", key)
        );
    } else if background {
        crate::plog_info!("[geolocation] asking the system for the background location grant (requestAlwaysAuthorization, OS prompt)");
        let _: () = msg_send![mgr, requestAlwaysAuthorization];
    } else {
        crate::plog_info!("[geolocation] asking the system for the location grant (requestWhenInUseAuthorization, OS prompt)");
        let _: () = msg_send![mgr, requestWhenInUseAuthorization];
    }
}

/// The permission manager's request for the location grant (a probe node subscribed after a user gesture): ask for it
/// without starting the updates, on the main thread.
#[cfg(target_os = "ios")]
pub(crate) fn request_authorization(background: bool) {
    // SAFETY: the manager is made and used on the main thread, where the permission manager runs.
    unsafe {
        let mgr = manager();
        if !mgr.is_null() {
            request_authorization_on(mgr, background);
        }
    }
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
            decl.add_method(
                sel!(locationManagerDidChangeAuthorization:),
                location_manager_did_change_auth as extern "C" fn(&Object, Sel, *mut Object),
            );
            // `locationManager:didFailWithError:`: a denied grant and a failed lookup were swallowed, so the app saw
            // a fix that never came and could not tell why.
            decl.add_method(
                sel!(locationManager:didFailWithError:),
                location_manager_did_fail as extern "C" fn(&Object, Sel, *mut Object, *mut Object),
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

        // "A negative value indicates that the latitude and longitude are invalid" (CLLocation.horizontalAccuracy):
        // such a fix is no position.
        if h_acc < 0.0 {
            return;
        }
        // The moment CoreLocation measured it (`CLLocation.timestamp`), not the moment this callback ran: a cached
        // first fix is older than the callback that delivers it.
        let timestamp: *mut Object = msg_send![loc, timestamp];
        let measured_ms = if timestamp.is_null() {
            None
        } else {
            let seconds: f64 = msg_send![timestamp, timeIntervalSince1970];
            (seconds.is_finite() && seconds >= 0.0).then(|| (seconds * 1000.0) as u64)
        };
        let valid_alt = v_acc > 0.0;
        // The delegate has no context: the App's services are where `App::run` put them.
        let Some(services) = crate::desktop::process_app::services() else {
            return;
        };
        services.geolocation.push_fix(LocationFix {
            latitude_deg: coord.latitude,
            longitude_deg: coord.longitude,
            accuracy_m: if h_acc >= 0.0 { h_acc as f32 } else { f32::NAN },
            altitude_m: if valid_alt { altitude as f32 } else { f32::NAN },
            altitude_accuracy_m: if valid_alt { v_acc as f32 } else { f32::NAN },
            heading_deg: if course >= 0.0 {
                course as f32
            } else {
                f32::NAN
            },
            speed_mps: if speed >= 0.0 { speed as f32 } else { f32::NAN },
            timestamp_ms: measured_ms.unwrap_or_else(|| {
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0)
            }),
        });
    }
}

/// `locationManagerDidChangeAuthorization:` (iOS 14+) — the user changed
/// the location grant (in-app prompt or Settings). Route the new
/// `CLAuthorizationStatus` into the permission channel so the
/// `PermissionManager` stays live, mirroring Android's
/// `onRequestPermissionsResult`. (The location *session* is unaffected;
/// this only updates the permission view of `Capability::Geolocation`.)
#[cfg(target_os = "ios")]
extern "C" fn location_manager_did_change_auth(_this: &Object, _cmd: Sel, manager: *mut Object) {
    unsafe {
        if manager.is_null() {
            return;
        }
        let status: isize = msg_send![manager, authorizationStatus];
        if let Some(services) = crate::desktop::process_app::services() {
            services
                .permission
                .push((Capability::Geolocation, auth_status_to_state(status)));
        }
    }
}

/// `locationManager:didFailWithError:`. `kCLErrorLocationUnknown` (0) is "no fix yet, still trying": not an error. Every
/// other code is delivered to the app through the manager's error channel, and `kCLErrorDenied` (1: the user refused,
/// or Location Services are off) also tells the permission manager.
#[cfg(target_os = "ios")]
extern "C" fn location_manager_did_fail(_this: &Object, _cmd: Sel, _manager: *mut Object, error: *mut Object) {
    unsafe {
        if error.is_null() {
            return;
        }
        let code: isize = msg_send![error, code];
        if code == 0 {
            return;
        }
        let description: *mut Object = msg_send![error, localizedDescription];
        let message = if description.is_null() {
            String::new()
        } else {
            let utf8: *const std::os::raw::c_char = msg_send![description, UTF8String];
            if utf8.is_null() {
                String::new()
            } else {
                std::ffi::CStr::from_ptr(utf8).to_string_lossy().into_owned()
            }
        };
        crate::log_warn!(
            crate::desktop::shell2::common::debug_server::LogCategory::Platform,
            "[geolocation] CoreLocation failed: CLError {} ({})",
            code,
            message
        );
        if let Some(services) = crate::desktop::process_app::services() {
            services.geolocation.push_error(azul_layout::managers::geolocation::LocationError {
                code: u32::try_from(code).unwrap_or(u32::MAX),
                message,
            });
            if code == 1 {
                services.permission.push((Capability::Geolocation, PermissionState::Denied));
            }
        }
    }
}

/// CLAuthorizationStatus → PermissionState. Both `authorizedAlways` (3) and
/// `authorizedWhenInUse` (4) are foreground-usable, so both map to Granted.
#[cfg(target_os = "ios")]
fn auth_status_to_state(status: isize) -> PermissionState {
    match status {
        0 => PermissionState::NotDetermined,
        1 => PermissionState::Restricted,
        2 => PermissionState::Denied,
        3 | 4 => PermissionState::Granted(PermissionQuality::Full),
        _ => PermissionState::NotDetermined,
    }
}
