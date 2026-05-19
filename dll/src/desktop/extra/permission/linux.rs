//! Linux permission backend — currently a stub.
//!
//! Linux has no standard permission model outside sandboxed runtimes
//! (Flatpak / Snap), so the canonical path is xdg-desktop-portal:
//!
//! ```text
//! service:   org.freedesktop.portal.Desktop
//! object:    /org/freedesktop/portal/desktop
//! interface: org.freedesktop.portal.{Camera, Location, ScreenCast, …}
//! ```
//!
//! Outside Flatpak / Snap most kernel surfaces are auto-granted to any
//! process that can open the device file (`/dev/video0` for cameras,
//! `/dev/input/*` for sensors). The framework defaults to
//! `PermissionState::Granted{Full}` when no portal is reachable, with a
//! warning logged so the user knows the platform isn't enforcing.
//!
//! `ashpd` is the typed portal client we'll pull in here. It depends on
//! `zbus`, which is already a transitive dep of several Wayland crates,
//! so the closure cost is small.

use azul_layout::managers::permission::{Capability, PermissionDiffEvent, PermissionState};

pub fn handle_event(event: &PermissionDiffEvent) {
    let _ = event;
    // TODO(P1.2+): xdg-desktop-portal `<Cap>.AccessDevice` request /
    // close. ashpd has a typed wrapper that maps cleanly onto our
    // Subscribe / Release events.
}

pub fn probe_status(capability: Capability) -> PermissionState {
    let _ = capability;
    PermissionState::NotDetermined
}
