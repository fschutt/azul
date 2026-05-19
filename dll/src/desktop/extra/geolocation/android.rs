//! Android geolocation backend — currently a stub.
//!
//! Final shape (queued for the next tick):
//!
//! 1. Java side: `scripts/android/AzulGeolocation.java` exposing
//!    `subscribe(Activity, long handleId, boolean highAccuracy)` and
//!    `release(long handleId)`. Internally uses
//!    `FusedLocationProviderClient.requestLocationUpdates(LocationRequest,
//!    callback, Looper.getMainLooper())`. Permission gate:
//!    `ActivityCompat.requestPermissions(activity,
//!    ["android.permission.ACCESS_FINE_LOCATION"], requestCode)` for
//!    `high_accuracy == true`, `ACCESS_COARSE_LOCATION` otherwise.
//!    For `config.background == true` add `ACCESS_BACKGROUND_LOCATION`
//!    (API 29+).
//! 2. `LocationCallback.onLocationResult(LocationResult)` → for each
//!    fix call `nativeOnLocationFix(handleId, lat, lon, accuracy,
//!    altitude, …)`.
//! 3. The matching Rust JNI symbol
//!    `Java_com_azul_geolocation_AzulGeolocation_nativeOnLocationFix`
//!    finds the `GeolocationManager` (via the published Activity / a
//!    global stash, same pattern as `dll/extra/file_picker/android.rs`)
//!    and calls `set_latest_fix`.
//! 4. `Reconfigure { config }` → re-issue `requestLocationUpdates`
//!    with the new `LocationRequest.priority`; the previous callback
//!    is implicitly replaced.

use azul_layout::managers::geolocation::GeolocationDiffEvent;

pub fn handle_event(event: &GeolocationDiffEvent) {
    let _ = event;
    // TODO(P3.1+): JNI to AzulGeolocation.subscribe / release.
}
