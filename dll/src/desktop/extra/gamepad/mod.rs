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
/// DualSense / DualShock 4 report decoding (8f-i-a-i-b), platform-free.
pub mod playstation;

/// Marks a `GamepadId` minted for a pad the raw HID stream sees but gilrs
/// does not pair with (no unique vendor/product twin): the low bits are the
/// HID instance's. Never collides with gilrs's small dense ids.
pub const HID_PAD_ID_FLAG: u32 = 0x4000_0000;

/// The last decoded PlayStation sample per HID instance (8f-i-a-i-b), with
/// the device it came from. Kept across passes because the gilrs backend
/// rebuilds every pad state from scratch each poll: the motion is laid over
/// that state as it is built, so a pass without a fresh report keeps the
/// last motion rather than snapping to zero.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
static LAST_PS_SAMPLES: std::sync::Mutex<
    std::collections::BTreeMap<u64, (azul_core::hid::HidDevice, playstation::PadSample)>,
> = std::sync::Mutex::new(std::collections::BTreeMap::new());

/// Decode this pass's PlayStation reports (8f-i-a-i-b) from the HID manager
/// - the same slice `get_hid_reports` copies, so nothing is stolen - and
/// publish the pads that have no gilrs twin as their own devices. Pads that
/// DO have a unique gilrs twin get their motion laid over the gilrs state
/// in [`overlay_hid_motion`] instead, at the next poll.
///
/// Returns whether a pad state advanced.
/// Each pad's calibration (8f-i-a-i-b-i), read ONCE per pad instance from its
/// calibration feature report the first time a report of it arrives: `Some`
/// = applied to every sample since, `None` = the pad did not answer (or the
/// platform cannot ask) and the nominal resolutions stay, which is what every
/// user-space reader without the report uses. A pad that vanishes is
/// forgotten so a re-plugged one is asked again.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
static PS_CALIBRATIONS: std::sync::Mutex<
    std::collections::BTreeMap<u64, Option<playstation::PadCalibration>>,
> = std::sync::Mutex::new(std::collections::BTreeMap::new());

/// The calibration of `device`, reading it on first sight. The DualShock 4
/// over Bluetooth is known to answer garbage or zeros to the first request,
/// so a rejected report is asked for once more before settling on nominal.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn calibration_of(
    cache: &mut std::collections::BTreeMap<u64, Option<playstation::PadCalibration>>,
    device: &azul_core::hid::HidDevice,
    pad: playstation::PlayStationPad,
    transport: playstation::Transport,
) -> Option<playstation::PadCalibration> {
    if let Some(known) = cache.get(&device.instance) {
        return *known;
    }
    let (id, len) = playstation::calibration_report(pad, transport);
    let mut found = None;
    for _attempt in 0..2 {
        let Some(bytes) = crate::desktop::extra::hid::feature_report(device, id, len) else {
            break;
        };
        if let Some(cal) = playstation::parse_calibration(pad, transport, &bytes) {
            found = Some(cal);
            break;
        }
    }
    cache.insert(device.instance, found);
    found
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub fn ingest_hid_reports(lw: &mut azul_layout::window::LayoutWindow) -> bool {
    use azul_core::gamepad::{GamepadId, GamepadState};
    use playstation::PlayStationPad;

    let mut last = LAST_PS_SAMPLES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut calibrations = PS_CALIBRATIONS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    // Forget pads that are gone, so a re-plugged one is asked again.
    {
        let present = lw.hid_manager.devices();
        calibrations.retain(|instance, _| present.iter().any(|d| d.instance == *instance));
    }
    for report in lw.hid_manager.reports() {
        let Some(pad) = PlayStationPad::of(&report.device) else {
            continue;
        };
        let bytes = report.bytes.as_ref();
        let calibration = playstation::transport_of(pad, bytes)
            .and_then(|t| calibration_of(&mut calibrations, &report.device, pad, t));
        if let Some(sample) = playstation::parse_with(pad, bytes, calibration.as_ref()) {
            last.insert(report.device.instance, (report.device.clone(), sample));
        }
    }
    if last.is_empty() {
        return false;
    }
    let identities = desktop::pad_identities();
    let mut changed = false;
    for (instance, (device, sample)) in last.iter() {
        // Paired by SERIAL (8f-i-a-i-c, Linux: evdev's uniq is hidraw's
        // uniq), or by being the only pad of its kind: the gilrs poll lays
        // this motion over its own state, nothing to publish here.
        let serial = device.serial.as_str();
        let paired_by_serial =
            !serial.is_empty() && identities.iter().any(|i| i.serial == serial);
        let twins = identities
            .iter()
            .filter(|i| i.vendor == device.vendor_id && i.product == device.product_id)
            .count();
        if paired_by_serial || twins == 1 {
            continue;
        }
        // No twin (Windows: gilrs is XInput and never sees a DualSense) or
        // several identical ones (8f-i-a-i-c): the pad is its own device,
        // complete - buttons, sticks and triggers come from the same report.
        let id = GamepadId {
            id: HID_PAD_ID_FLAG | (*instance as u32 & !HID_PAD_ID_FLAG),
        };
        let mut state = lw
            .gamepad_manager
            .state(id)
            .unwrap_or_else(|| GamepadState::empty(id));
        state.connected = true;
        state.buttons = sample.buttons;
        state.left_stick_x = sample.left_stick.0;
        state.left_stick_y = sample.left_stick.1;
        state.right_stick_x = sample.right_stick.0;
        state.right_stick_y = sample.right_stick.1;
        state.left_z = sample.left_trigger;
        state.right_z = sample.right_trigger;
        apply_sample_motion(&mut state, sample);
        // `set_state` compares bitwise, so an unchanged pad raises nothing.
        changed |= lw.gamepad_manager.set_state(state);
    }
    changed
}

/// Lay the last decoded motion and touch of this gilrs pad's HID twin over
/// `state` (8f-i-a-i-b, -c). The twin is found by SERIAL when the gilrs
/// side has one (Linux), else as the unique vendor/product match. Called by
/// the gilrs poll as it builds each state, so the published state already
/// carries the motion and the manager sees one writer per slot.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub fn overlay_hid_motion(
    state: &mut azul_core::gamepad::GamepadState,
    vendor_id: u16,
    product_id: u16,
    serial: &str,
    twins_of_this_kind: usize,
) {
    let last = LAST_PS_SAMPLES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if !serial.is_empty() {
        if let Some((_, sample)) = last.values().find(|(d, _)| d.serial.as_str() == serial) {
            apply_sample_motion(state, sample);
            return;
        }
    }
    if twins_of_this_kind != 1 {
        return;
    }
    let mut matching = last
        .values()
        .filter(|(d, _)| d.vendor_id == vendor_id && d.product_id == product_id);
    let (Some((_, sample)), None) = (matching.next(), matching.next()) else {
        // No HID sample yet, or several HID instances of this kind - the
        // pairing is ambiguous and nothing is guessed onto this pad.
        return;
    };
    apply_sample_motion(state, sample);
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn apply_sample_motion(state: &mut azul_core::gamepad::GamepadState, sample: &playstation::PadSample) {
    state.gyro_x = sample.gyro[0];
    state.gyro_y = sample.gyro[1];
    state.gyro_z = sample.gyro[2];
    state.accel_x = sample.accel[0];
    state.accel_y = sample.accel[1];
    state.accel_z = sample.accel[2];
    match sample.touch {
        Some((x, y)) => {
            state.touchpad_active = true;
            state.touchpad_x = x;
            state.touchpad_y = y;
        }
        None => {
            state.touchpad_active = false;
        }
    }
}

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
