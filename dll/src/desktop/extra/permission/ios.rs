//! iOS permission backend — currently a stub.
//!
//! Subscribe path → `AVCaptureDevice.requestAccess(for:.video)` for
//! camera, `CLLocationManager.requestWhenInUseAuthorization` for geo,
//! `PHPhotoLibrary.requestAuthorization(for:.readWrite)` for photos,
//! `LAContext.evaluatePolicy` for biometric, etc. Each requires
//! corresponding `Info.plist` keys (`NSCameraUsageDescription`, …) —
//! missing keys SIGABRT the app on the first prompt.
//!
//! Release path → drop the matching native session; the framework
//! releases its retain on the `AVCaptureSession` / `CLLocationManager`
//! / `CMMotionManager` and CoreText reclaims memory.
//!
//! Probe path → sync class methods (`AVCaptureDevice.authorizationStatus(for:)`,
//! `CLLocationManager.authorizationStatus()`, …) collapsed onto the
//! `PermissionState` enum per the mapping in research/08 §2.
//!
//! All three paths are unimplemented in this initial scaffold. The
//! dispatcher (`super::apply_diff_events`) calls `handle_event` for
//! every Subscribe / Release / Reconfigure; for now we just log so
//! later ticks can see the wire-up alive in the event log.

use azul_layout::managers::permission::{Capability, PermissionDiffEvent, PermissionState};

pub fn handle_event(event: &PermissionDiffEvent) {
    let _ = event;
    // TODO(P1.2+): issue the matching `request<X>Access` / native release.
}

pub fn probe_status(capability: Capability) -> PermissionState {
    let _ = capability;
    PermissionState::NotDetermined
}
