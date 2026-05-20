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
//! | iOS | `GCController` (objc2-game-controller) — pending | [`poll`] reads the current controller snapshot |
//! | Android | `InputDevice` / `InputManager` (JNI) — pending | push from the input callback |
//!
//! `gilrs` covers macOS too, so (unlike the CoreMotion sensor backend) the
//! Apple path here is **iOS-only** `GCController`. [`ensure_started`] does
//! any one-time native subscription (a no-op on desktop — gilrs lazily
//! initialises on first poll); [`poll`] pulls the current state each frame.
//! The layout pass drains the parked states (`drain_gamepad_states`) into
//! the manager, where `CallbackInfo::get_gamepad_state` reads them.
//!
//! This tick lands the dispatcher + the **gilrs desktop** backend (real,
//! and exercisable on the dev host); the iOS `GCController` / Android
//! `InputDevice` backends are follow-ups (their `start`/`poll` are no-ops).

#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod desktop;
#[cfg(target_os = "ios")]
pub mod apple;
#[cfg(target_os = "android")]
pub mod android;

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
