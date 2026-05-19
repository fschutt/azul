//! Android permission backend — currently a stub.
//!
//! Subscribe path → `ActivityCompat.requestPermissions(activity,
//! permissions, requestCode)` via the JNI bridge (same pattern as
//! `NativeGestureBridge.java`). Dangerous-level permissions
//! (`CAMERA`, `ACCESS_FINE_LOCATION`, `READ_MEDIA_IMAGES`, …) need
//! both a manifest declaration *and* the runtime prompt. Special
//! permissions (`MANAGE_EXTERNAL_STORAGE`, `SYSTEM_ALERT_WINDOW`,
//! `POST_NOTIFICATIONS`) need a settings-app intent.
//!
//! Release path → drop the matching listener (e.g.
//! `LocationManager.removeUpdates(listener)`). Camera/mic sessions
//! release on session destroy.
//!
//! Probe path → `ContextCompat.checkSelfPermission(context, perm) ==
//! PERMISSION_GRANTED`, with `shouldShowRequestPermissionRationale`
//! distinguishing `NotDetermined` from `Denied`.

use azul_layout::managers::permission::{Capability, PermissionDiffEvent, PermissionState};

pub fn handle_event(event: &PermissionDiffEvent) {
    let _ = event;
    // TODO(P1.2+): JNI call into AzulPermissions.requestPermission(...)
}

pub fn probe_status(capability: Capability) -> PermissionState {
    let _ = capability;
    PermissionState::NotDetermined
}
