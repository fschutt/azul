//! Linux geolocation backend — currently a stub.
//!
//! Final shape (queued):
//!
//! 1. Connect to the `org.freedesktop.GeoClue2.Manager` D-Bus
//!    interface via the `zbus` async client (Flatpak / Snap
//!    sandboxed environments expose it via the XDG portal too —
//!    `org.freedesktop.portal.Location`).
//! 2. `GetClient` → returns an `org.freedesktop.GeoClue2.Client`
//!    proxy. Set `DesktopId` and `DistanceThreshold`, then
//!    `Start`.
//! 3. Subscribe to the `LocationUpdated` signal — the new
//!    `org.freedesktop.GeoClue2.Location` object exposes
//!    `Latitude`, `Longitude`, `Accuracy`, `Altitude`, etc.
//!    Construct a `LocationFix` and call
//!    `GeolocationManager::set_latest_fix`.
//!
//! Where GeoClue isn't present (servers, lightweight desktops),
//! fall back to a default fix at `(0.0, 0.0)` with `accuracy_m =
//! f32::INFINITY` — gives the layout pass a sentinel "no location
//! available" value the app can fall back on.

use azul_layout::managers::geolocation::GeolocationDiffEvent;

pub fn handle_event(event: &GeolocationDiffEvent) {
    let _ = event;
    // TODO(P3.1+): zbus client to org.freedesktop.GeoClue2.
}
