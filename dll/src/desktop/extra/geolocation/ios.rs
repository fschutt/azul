//! iOS geolocation backend — currently a stub.
//!
//! Final shape (queued for the next tick):
//!
//! 1. `Subscribe { config }` → if `Info.plist` lacks
//!    `NSLocationWhenInUseUsageDescription` (and `…AlwaysAndWhenInUse…`
//!    when `config.background == true`), log a warning and surface
//!    `Capability::Geolocation = Restricted`.
//! 2. Alloc a singleton `CLLocationManager`, set its delegate to a
//!    registered `AzulLocationDelegate` NSObject subclass (same
//!    pattern as `AzulGestureTarget` / `AzulDocumentPickerDelegate`),
//!    flip `desiredAccuracy` based on `config.high_accuracy`, then
//!    `[mgr requestWhenInUseAuthorization]` + `[mgr startUpdatingLocation]`.
//!    For `config.background == true` swap to
//!    `requestAlwaysAuthorization` + enable background-location
//!    capability.
//! 3. Delegate methods:
//!    - `locationManager:didUpdateLocations:` → convert the last
//!      `CLLocation` to `LocationFix` and call
//!      `GeolocationManager::set_latest_fix`.
//!    - `locationManager:didChangeAuthorizationStatus:` → translate
//!      iOS `CLAuthorizationStatus` into `PermissionState` and call
//!      `PermissionManager::set_status(Capability::Geolocation, ...)`.
//!    - `locationManager:didFailWithError:` → log and surface via a
//!      `On::GeolocationError` event filter (P3+ once those filters
//!      land).
//! 4. `Reconfigure { config }` → update `desiredAccuracy` +
//!    `distanceFilter` on the existing manager — no need to tear down
//!    the subscription.
//! 5. `Release` → `[mgr stopUpdatingLocation]`. Keep the manager
//!    instance for re-use; iOS doesn't punish keeping it around.

use azul_layout::managers::geolocation::GeolocationDiffEvent;

pub fn handle_event(event: &GeolocationDiffEvent) {
    let _ = event;
    // TODO(P3.1+): CLLocationManager subscribe / release / reconfigure.
}
