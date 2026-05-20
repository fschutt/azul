//! iOS gamepad backend — Game Controller framework (`GCController`).
//!
//! Will enumerate `GCController.controllers()` (+ observe the
//! `GCControllerDidConnect`/`DidDisconnect` notifications), reading each
//! controller's `extendedGamepad` profile every frame in [`poll`] into a
//! [`azul_core::gamepad::GamepadState`] parked via `push_gamepad_state`.
//! (macOS uses the gilrs desktop backend instead, so this is iOS-only.)
//!
//! Pending (objc2-game-controller). Until it lands, `start`/`poll` are
//! no-ops and `get_gamepad_state` returns `None` on iOS.

/// Begin observing controller connect/disconnect (no-op until the
/// `GCController` backend lands).
pub fn start() {}

/// Snapshot the connected controllers (no-op until the backend lands).
pub fn poll() {}
