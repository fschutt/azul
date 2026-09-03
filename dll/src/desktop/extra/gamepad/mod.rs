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

/// Silence every controller motor.
///
/// Called when the app is going away. A rumble is the one input effect that
/// OUTLIVES the process on some backends - the OS does not necessarily reset an
/// actuator when the process that started it exits - so a pad can keep buzzing
/// after the window is gone with nothing left to stop it. Dropping the effect
/// handle is NOT enough: gilrs's `HandleDropped` only removes the effect from
/// its server's map (`ff/server.rs`), it does not stop the motor.
pub fn stop_all_rumble() {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    desktop::stop_all_rumble();
    #[cfg(target_os = "ios")]
    apple::stop_all_rumble();
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

/// Accumulated per-pad state for a PUSH-DRIVEN backend.
///
/// # Why this has to exist
///
/// `GamepadManager::set_state` REPLACES a pad's slot outright - it stores whole
/// pads, and a partial one overwrites the rest with defaults. That is exactly
/// right for a polled backend, which builds a complete snapshot every frame by
/// construction, and exactly wrong for a push-driven one, where a key event
/// carries one button, a motion event carries the axes and a sensor event
/// carries three floats. Publishing each of those directly meant pressing a
/// button zeroed the sticks and moving a stick released every held button.
///
/// So the union is accumulated here and the FULL snapshot is published every
/// time. Android is the only backend that needs it today; it lives in this
/// shared module rather than inside `android.rs` because that file is
/// `cfg`-gated to a target this machine never runs tests on, and this is
/// precisely the logic that goes wrong silently.
#[derive(Debug, Default)]
pub(crate) struct PadAccumulator {
    pads: std::collections::BTreeMap<u32, azul_core::gamepad::GamepadState>,
}

impl PadAccumulator {
    /// Apply an update to one pad and return the complete state to publish.
    ///
    /// A pad seen for the first time starts CONNECTED with no battery: `-1.0`
    /// is the "not reported" sentinel, and defaulting to `0.0` would make a
    /// pad whose battery the platform never mentions look flat.
    pub(crate) fn update(
        &mut self,
        id: u32,
        f: impl FnOnce(&mut azul_core::gamepad::GamepadState),
    ) -> azul_core::gamepad::GamepadState {
        let slot = self.pads.entry(id).or_insert_with(|| {
            azul_core::gamepad::GamepadState {
                id: azul_core::gamepad::GamepadId { id },
                connected: true,
                battery: -1.0,
                ..Default::default()
            }
        });
        f(slot);
        *slot
    }

    /// Forget a pad that disconnected.
    ///
    /// Without this, a pad reconnecting on the same device id comes back with
    /// whatever it was holding when it left - buttons still down, and now also
    /// a stale gyro reading that never moves again.
    pub(crate) fn forget(&mut self, id: u32) {
        self.pads.remove(&id);
    }
}

/// Which of a controller's actuators a rumble goes to (9g-i-d-a-i).
///
/// The Game Controller framework names actuators by LOCALITY rather than by
/// motor weight. A DualShock/DualSense - the controllers that carry two
/// different motors - puts the large low-frequency motor in the LEFT grip
/// and the small high-frequency one in the RIGHT, which is also the order
/// Apple's own discussion uses ("the left handle actuator as a woofer and
/// the right actuator as a tweeter"). So the "which motor" the gilrs backend
/// answers with `BaseEffectType::Strong` / `Weak` is answered here with a
/// handle. `Default` is the fallback for a pad that reports neither handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RumbleLocality {
    LeftHandle,
    RightHandle,
    Default,
}

/// One rumble, resolved from a request into what CoreHaptics is told.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RumblePlan {
    pub locality: RumbleLocality,
    /// CoreHaptics `HapticIntensity`, 0..=1.
    pub intensity: f32,
    /// CoreHaptics `HapticSharpness`, 0..=1: a thud is dull, a buzz is sharp.
    pub sharpness: f32,
    /// Event duration in seconds - CoreHaptics counts in `NSTimeInterval`.
    pub seconds: f64,
}

/// Turn a rumble request into a [`RumblePlan`], or `None` for one that must
/// not play. Pure so it is tested on the host; the iOS backend applies it.
///
/// - Intensity at or below zero is "do not play", not "play at zero": a
///   zero-intensity continuous event still occupies the actuator and STOPS
///   whatever was playing, which is not what a caller sending 0 meant.
/// - `strong` picks the MOTOR, not the loudness: the low-frequency one thuds
///   (sharpness 0), the high-frequency one buzzes (sharpness 1). Driving both
///   at once is a muddier sensation, not a louder one - the gilrs rule.
/// - `duration_ms` is the ALREADY-RESOLVED duration (`rumble_duration_ms`
///   turned 0 into the 150 ms default); this only converts units.
#[must_use]
pub fn rumble_plan(intensity: f32, duration_ms: u32, strong: bool) -> Option<RumblePlan> {
    if intensity.is_nan() || intensity <= 0.0 {
        return None;
    }
    Some(RumblePlan {
        locality: if strong {
            RumbleLocality::LeftHandle
        } else {
            RumbleLocality::RightHandle
        },
        intensity: intensity.min(1.0),
        sharpness: if strong { 0.0 } else { 1.0 },
        seconds: f64::from(duration_ms) / 1000.0,
    })
}

#[cfg(test)]
mod rumble_plan_tests {
    use super::*;

    #[test]
    fn strong_is_the_left_grip_and_dull_weak_is_the_right_grip_and_sharp() {
        let strong = rumble_plan(0.8, 150, true).expect("plays");
        assert_eq!(strong.locality, RumbleLocality::LeftHandle);
        assert_eq!(strong.sharpness, 0.0);
        assert_eq!(strong.intensity, 0.8);
        assert_eq!(strong.seconds, 0.15);
        let weak = rumble_plan(0.8, 150, false).expect("plays");
        assert_eq!(weak.locality, RumbleLocality::RightHandle);
        assert_eq!(weak.sharpness, 1.0);
    }

    #[test]
    fn zero_or_negative_or_nan_intensity_does_not_play() {
        assert!(rumble_plan(0.0, 150, true).is_none());
        assert!(rumble_plan(-1.0, 150, true).is_none());
        assert!(rumble_plan(f32::NAN, 150, true).is_none());
    }

    #[test]
    fn intensity_is_capped_and_duration_is_seconds() {
        let plan = rumble_plan(7.0, 2500, false).expect("plays");
        assert_eq!(plan.intensity, 1.0);
        assert_eq!(plan.seconds, 2.5);
    }
}

#[cfg(test)]
mod pad_accumulator_tests {
    use azul_core::gamepad::GamepadButton;

    use super::*;

    /// THE BUG THIS EXISTS FOR. A button press must not zero the sticks.
    #[test]
    fn an_update_preserves_every_field_another_update_set() {
        let mut acc = PadAccumulator::default();

        acc.update(7, |p| {
            p.left_stick_x = 0.5;
            p.left_stick_y = -0.25;
            p.right_z = 1.0;
        });
        let after_button = acc.update(7, |p| p.buttons |= GamepadButton::South.bit());

        assert_eq!(after_button.left_stick_x, 0.5, "a button press zeroed a stick");
        assert_eq!(after_button.left_stick_y, -0.25);
        assert_eq!(after_button.right_z, 1.0);
        assert_ne!(after_button.buttons & GamepadButton::South.bit(), 0);

        // ...and the other direction: a stick sample must not release buttons.
        let after_stick = acc.update(7, |p| p.left_stick_x = -1.0);
        assert_ne!(
            after_stick.buttons & GamepadButton::South.bit(),
            0,
            "a stick sample released a held button"
        );

        // A pad IMU sample must disturb neither.
        let after_imu = acc.update(7, |p| {
            p.gyro_x = 2.0;
            p.accel_z = 9.8;
        });
        assert_ne!(after_imu.buttons & GamepadButton::South.bit(), 0);
        assert_eq!(after_imu.left_stick_x, -1.0);
        assert_eq!(after_imu.gyro_x, 2.0);
        assert_eq!(after_imu.accel_z, 9.8);
    }

    #[test]
    fn a_first_sighting_reports_no_battery_rather_than_a_flat_one() {
        let mut acc = PadAccumulator::default();
        let s = acc.update(1, |_| {});
        assert_eq!(s.battery, -1.0, "an unknown battery must not read as flat");
        assert!(s.connected);
        assert_eq!(s.id.id, 1);
    }

    #[test]
    fn pads_do_not_share_state() {
        let mut acc = PadAccumulator::default();
        acc.update(1, |p| p.left_stick_x = 1.0);
        let other = acc.update(2, |_| {});
        assert_eq!(other.left_stick_x, 0.0, "one pad's stick leaked into another");
    }

    /// A pad that reconnects on the same id must not come back mid-press.
    #[test]
    fn forgetting_a_pad_clears_everything_it_was_holding() {
        let mut acc = PadAccumulator::default();
        acc.update(3, |p| {
            p.buttons = GamepadButton::South.bit();
            p.gyro_x = 5.0;
        });
        acc.forget(3);
        let back = acc.update(3, |_| {});
        assert_eq!(back.buttons, 0, "a reconnecting pad came back with buttons held");
        assert_eq!(back.gyro_x, 0.0, "and with a stale gyro reading");
    }
}
