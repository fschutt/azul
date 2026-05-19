//! macOS permission backend — currently a stub.
//!
//! macOS shares most permission surfaces with iOS — camera / mic / GPS /
//! photos route through the same `AVCaptureDevice` / `CLLocationManager` /
//! `PHPhotoLibrary` APIs. The differences:
//!
//! - macOS adds the `TCC` (Transparency, Consent, Control) database; the
//!   first prompt is OS-modal, subsequent reads come from the cache.
//! - `MotionManager` is replaced by `IOHIDManager` (laptops with motion
//!   sensors are rare; only the M-series MacBook Pro touch bar / lid
//!   sensor exposes it).
//! - `LAContext.evaluatePolicy` for Touch ID is identical.
//! - The "open System Settings" deep link is
//!   `x-apple.systempreferences:com.apple.preference.security?Privacy_<Cap>`
//!   via `NSWorkspace.shared.open(URL)`.
//!
//! Grant keyed to the bundle ID — unsigned binaries can't request most
//! permissions because there's no entry in `TCC.db`. Document that the
//! framework requires code-signing for permission-bearing features.

use azul_layout::managers::permission::{Capability, PermissionDiffEvent, PermissionState};

pub fn handle_event(event: &PermissionDiffEvent) {
    let _ = event;
    // TODO(P1.2+): reuse the iOS objc bindings via cfg(any(ios, macos))
}

pub fn probe_status(capability: Capability) -> PermissionState {
    let _ = capability;
    PermissionState::NotDetermined
}
