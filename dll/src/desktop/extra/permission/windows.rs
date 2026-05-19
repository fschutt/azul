//! Windows permission backend — currently a stub.
//!
//! Two paths:
//!
//! 1. **UWP / WinUI 3 packaged** — `<Capabilities>` declarations in the
//!    appx manifest, runtime probe via
//!    `Windows.Devices.Enumeration.DeviceAccessInformation` returning
//!    `DeviceAccessStatus ∈ { Allowed, DeniedByUser, DeniedBySystem,
//!    Unspecified }`.
//! 2. **Win32 desktop** — most APIs succeed-or-fail without a prompt.
//!    Camera/mic/location honor the system privacy switches via
//!    `Windows.Security.Authorization.AppCapabilityAccess.AppCapability.CheckAccessAsync`,
//!    which works for both packaged and unpackaged callers since 1903.
//!
//! Mapping:
//! - `DeniedBySystem` → `PermissionState::Restricted` (Group Policy /
//!   MDM)
//! - `DeniedByUser` → `PermissionState::Denied`
//! - `Unspecified` → `PermissionState::NotDetermined`
//! - `Allowed` → `PermissionState::Granted{Full}`
//!
//! The "open Settings" deep link is `ms-settings:privacy-<cap>` via
//! `Windows.System.Launcher.LaunchUriAsync`.

use azul_layout::managers::permission::{Capability, PermissionDiffEvent, PermissionState};

pub fn handle_event(event: &PermissionDiffEvent) {
    let _ = event;
    // TODO(P1.2+): WinRT `AppCapability::Create("X").CheckAccessAsync()`
}

pub fn probe_status(capability: Capability) -> PermissionState {
    let _ = capability;
    PermissionState::NotDetermined
}
