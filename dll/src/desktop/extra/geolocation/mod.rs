//! Platform dispatcher for geolocation subscriptions.
//!
//! Cross-platform state lives in
//! `azul_layout::managers::geolocation::GeolocationManager`. This module
//! consumes the `GeolocationDiffEvent`s the layout pass enqueues and
//! turns each one into the right native API call:
//!
//! | Event | iOS | Android | macOS | Linux | Windows |
//! |-------|-----|---------|-------|-------|---------|
//! | `Subscribe` | `[CLLocationManager startUpdatingLocation]` after `requestWhenInUseAuthorization` | `LocationManager.requestLocationUpdates` (or FusedLocationProviderClient) after `requestPermissions(ACCESS_FINE_LOCATION)` | same as iOS | `geoclue` D-Bus subscription | `Windows.Devices.Geolocation.Geolocator.PositionChanged` |
//! | `Release` | drop the CLLocationManager / cancel updates | `removeUpdates(listener)` | same as iOS | release portal subscription | drop the Geolocator event handler |
//! | `Reconfigure` | adjust `desiredAccuracy` + `distanceFilter` in place | re-request with new LocationRequest priority | same as iOS | re-subscribe with new accuracy | re-create the Geolocator |
//!
//! Step 1 (this tick) lands the dispatcher + five no-op platform
//! stubs — the GeolocationManager wire-up is already live, but no
//! platform yet reports a fix. Real iOS / Android subscriptions land
//! when `NodeType::GeolocationProbe` lands in the next P3 tick and
//! gives the diff_layout pass something to enumerate.

use azul_layout::managers::geolocation::{GeolocationDiffEvent, LocationFix};

#[cfg(target_os = "android")]
pub mod android;
#[cfg(target_os = "ios")]
pub mod ios;
#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "windows")]
pub mod windows;

/// Translate the `GeolocationDiffEvent`s the layout pass emitted this
/// frame into native subscribe / release / reconfigure calls. Called
/// from `regenerate_layout` after the geolocation diff pass.
pub fn apply_diff_events(events: &[GeolocationDiffEvent]) {
    if events.is_empty() {
        return;
    }
    for event in events {
        #[cfg(target_os = "ios")]
        ios::handle_event(event);
        #[cfg(target_os = "android")]
        android::handle_event(event);
        #[cfg(target_os = "macos")]
        macos::handle_event(event);
        #[cfg(target_os = "linux")]
        linux::handle_event(event);
        #[cfg(target_os = "windows")]
        windows::handle_event(event);
        #[cfg(not(any(
            target_os = "ios",
            target_os = "android",
            target_os = "macos",
            target_os = "linux",
            target_os = "windows",
        )))]
        {
            let _ = event;
        }
    }
}

/// Synchronous probe for the most recently observed fix. Used by the
/// `CallbackInfo::get_geolocation_fix` accessor (a P3 follow-up) once
/// the platform actually feeds fixes back into the manager.
///
/// All stubs return `None`; the manager itself is the storage so any
/// caller reading from the manager directly already sees the last fix.
/// This entry point exists so a future "synthesize a fix from a
/// platform-side cache hit" optimization has somewhere to land.
pub fn probe_last_fix() -> Option<LocationFix> {
    None
}
