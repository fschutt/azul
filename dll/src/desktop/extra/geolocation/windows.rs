//! Windows geolocation backend — currently a stub.
//!
//! Final shape (queued):
//!
//! 1. `Windows.Devices.Geolocation.Geolocator` via the WinRT
//!    bindings. `DesiredAccuracy` flag maps `config.high_accuracy`
//!    to `PositionAccuracy.High`.
//! 2. Hook `Geolocator.PositionChanged` → translate
//!    `PositionChangedEventArgs.Position.Coordinate` into a
//!    `LocationFix` and call `set_latest_fix`.
//! 3. Permission gate via
//!    `Windows.Security.Authorization.AppCapabilityAccess.AppCapability.Create("location").CheckAccessAsync()`.
//!    The first call surfaces the system-modal "allow this app to
//!    access location" sheet. Win32 desktop apps inherit the
//!    location-services switch from Settings → Privacy → Location.
//! 4. `Reconfigure { config }` → set new
//!    `Geolocator.MovementThreshold` / `DesiredAccuracy` —
//!    no need to drop the instance.

use azul_layout::managers::geolocation::GeolocationDiffEvent;

pub fn handle_event(event: &GeolocationDiffEvent) {
    let _ = event;
    // TODO(P3.1+): WinRT Geolocator.PositionChanged.
}
