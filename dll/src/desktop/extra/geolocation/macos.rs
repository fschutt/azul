//! macOS geolocation backend — currently a stub.
//!
//! macOS shares the `CLLocationManager` API with iOS — the
//! authorization model is identical (`requestWhenInUseAuthorization` +
//! plist key), the delegate methods have the same names, and the
//! `CLLocation` payload is the same.
//!
//! The differences:
//! - Plist key is `NSLocationUsageDescription` (deprecated since 10.14)
//!   for compatibility, plus `NSLocationWhenInUseUsageDescription`
//!   for modern apps.
//! - macOS Catalina+ requires the app to be signed and notarized for
//!   the location prompt to appear (unsigned binaries get
//!   `kCLAuthorizationStatusDenied` immediately).
//! - macOS uses Apple's location service which fuses Wi-Fi BSSID
//!   lookup + IP geolocation when no GPS hardware is present — most
//!   Macs don't have GPS so the accuracy is ~100m city-block radius.
//!
//! The follow-up tick will reuse the iOS objc bindings via
//! `cfg(any(target_os = "ios", target_os = "macos"))`.

use azul_layout::managers::geolocation::GeolocationDiffEvent;

pub fn handle_event(event: &GeolocationDiffEvent) {
    let _ = event;
    // TODO(P3.1+): CLLocationManager (shared with iOS arm).
}
