//! Platform dispatcher for gamepad / game-controller input
//! (SUPER_PLAN_2 §1 feature 6 + research/03).
//!
//! Cross-platform state lives in
//! `azul_layout::managers::gamepad::GamepadManager`. Poll-driven, like the
//! sensors:
//!
//! | Platform | Backend | Sample → channel |
//! |----------|---------|------------------|
//! | desktop (Win / Linux / **macOS**) | `gilrs` | per-frame [`poll`] snapshots each pad → `push_gamepad_state` |
//! | iOS | `GCController` (raw objc) | [`poll`] reads the current controller snapshot |
//! | Android | `InputDevice` / `InputManager` (JNI) | push from the input callback |
//!
//! `gilrs` covers macOS too, so (unlike the CoreMotion sensor backend) the
//! Apple path here is **iOS-only** `GCController`. [`ensure_started`] does
//! any one-time native subscription (a no-op on desktop — gilrs lazily
//! initialises on first poll); [`poll`] pulls the current state each frame.
//! The layout pass drains the parked states (`drain_gamepad_states`) into
//! the manager, where `CallbackInfo::get_gamepad_state` reads them.
//!
//! All three backends are real. The deadzone helpers are shared here rather
//! than per-backend: a pad's resting jitter is not bitwise stable and
//! `state_bitwise_eq` treats any difference as a change, so a backend without
//! them fires `GamepadInput` continuously while a pad merely sits plugged in.

#[cfg(target_os = "android")]
pub mod android;
#[cfg(target_os = "ios")]
pub mod apple;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod desktop;

/// Stick deadzone radius (Xbox/DualShock resting jitter stays well below
/// 0.15; SDL and XInput use comparable defaults).
pub(crate) const STICK_DEADZONE: f32 = 0.15;
/// Trigger deadzone (triggers rest at exactly 0.0 on most drivers; small
/// guard for worn hardware).
pub(crate) const TRIGGER_DEADZONE: f32 = 0.05;

/// Radial deadzone with rescaling — inside the radius maps to exactly (0,0);
/// outside, magnitude rescales to [0,1] so there is no jump at the deadzone
/// edge and full deflection still reaches 1.0.
///
/// Shared rather than per-backend on purpose. A pad's resting jitter is not
/// bitwise stable, and `state_bitwise_eq` treats any difference as a change —
/// so a backend without this fires `GamepadInput` continuously while a pad
/// merely sits plugged in. Every backend needs the same treatment, and three
/// copies would drift.
pub(crate) fn apply_radial_deadzone(x: f32, y: f32) -> (f32, f32) {
    let mag = (x * x + y * y).sqrt();
    if mag <= STICK_DEADZONE {
        return (0.0, 0.0);
    }
    let scale = ((mag - STICK_DEADZONE) / (1.0 - STICK_DEADZONE)).min(1.0) / mag;
    (x * scale, y * scale)
}

/// Axial deadzone for triggers (1-D), with the same edge rescaling.
pub(crate) fn apply_axial_deadzone(v: f32) -> f32 {
    if v.abs() <= TRIGGER_DEADZONE {
        return 0.0;
    }
    let sign = v.signum();
    sign * ((v.abs() - TRIGGER_DEADZONE) / (1.0 - TRIGGER_DEADZONE)).min(1.0)
}

/// One-time native subscription, guarded so only the first frame does it.
/// No-op on desktop (gilrs initialises lazily inside [`poll`]).
pub fn ensure_started() {
    static STARTED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    STARTED.get_or_init(start);
}

fn start() {
    #[cfg(target_os = "ios")]
    apple::start();
    #[cfg(target_os = "android")]
    android::start();
    // desktop: gilrs lazily initialises on the first `poll`.
}

/// Pull the current state of every connected pad into the async channel.
/// Called once per layout pass (after [`ensure_started`]).
pub fn poll() {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    desktop::poll();
    #[cfg(target_os = "ios")]
    apple::poll();
    // Android is push-based (the InputDevice JNI callback parks states), so
    // there's nothing to pull here.
}
