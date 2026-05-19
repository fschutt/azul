//! Platform dispatcher for permission requests.
//!
//! The cross-platform state machine lives in
//! `azul_layout::managers::permission::PermissionManager`. This module is
//! the *platform* half — it consumes the `PermissionDiffEvent`s the layout
//! pass enqueues and turns each one into the right native API call:
//!
//! | Event | iOS | Android | macOS | Linux | Windows |
//! |-------|-----|---------|-------|-------|---------|
//! | `Subscribe` | `request<X>Access` on the matching framework | `ActivityCompat.requestPermissions` via the JNI bridge | same as iOS for camera/mic/location; `kCLAuthorizationStatus*` for GPS | xdg-desktop-portal `org.freedesktop.portal.<Cap>` (when present); otherwise auto-grant + log | `Windows.Security.Authorization.AppCapabilityAccess` / `AppCapability.CheckAccessAsync` (UWP-only — Win32 desktop auto-grants and logs) |
//! | `Release` | drop the matching `AVCaptureSession` / `CLLocationManager` / `CMMotionManager` | drop the matching Java-side listener | same as iOS | drop the portal subscription | drop the AppCapability watch |
//! | `Reconfigure` | TODO once `CameraPreview` etc. land — for now a no-op | same | same | same | same |
//!
//! Step 1 (this tick) lands the dispatcher + five **no-op** platform
//! stubs. Each stub logs the event via `debug_server::log` so a tick
//! looking at the wire-up sees activity, but no native call is issued
//! yet. The Subscribe/Release loop becomes real per-platform once the
//! follow-up ticks land `request_camera_access_objc` / the JNI bridge /
//! the xdg-portal client / etc.

use azul_layout::managers::permission::{Capability, PermissionDiffEvent, PermissionState};

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

/// Translate the `PermissionDiffEvent`s the layout pass emitted this frame
/// into native subscribe / release calls. Called once per frame by the
/// platform backend after `LayoutWindow::regenerate_layout`.
pub fn apply_diff_events(events: &[PermissionDiffEvent]) {
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
        // Other targets (wasm32, BSDs, …) silently no-op — these will get
        // their own arm if/when the framework grows a backend for them.
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

/// Sync probe — what the platform reports right now for `capability`
/// without firing a prompt. Used by `CallbackInfo::get_permission_status`
/// to satisfy synchronous reads during the layout callback.
///
/// Stub implementations all return `PermissionState::NotDetermined` for
/// now. Real per-platform reads will land alongside the subscribe path.
pub fn probe_status(capability: Capability) -> PermissionState {
    #[cfg(target_os = "ios")]
    return ios::probe_status(capability);
    #[cfg(target_os = "android")]
    return android::probe_status(capability);
    #[cfg(target_os = "macos")]
    return macos::probe_status(capability);
    #[cfg(target_os = "linux")]
    return linux::probe_status(capability);
    #[cfg(target_os = "windows")]
    return windows::probe_status(capability);
    #[cfg(not(any(
        target_os = "ios",
        target_os = "android",
        target_os = "macos",
        target_os = "linux",
        target_os = "windows",
    )))]
    {
        let _ = capability;
        PermissionState::NotDetermined
    }
}
