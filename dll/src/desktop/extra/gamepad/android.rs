//! Android gamepad backend — `InputDevice` / `InputManager` via JNI.
//!
//! Push-based, unlike the desktop gilrs path: Android delivers controller
//! input as ordinary `KeyEvent` and `MotionEvent` through the activity's input
//! queue, so there is nothing to poll — the dispatcher's `poll()` is a no-op
//! here and the Java side calls down whenever a gamepad-source event arrives.
//!
//! The Rust/Java split follows `AzulSensors`: Java owns the
//! `InputManager.InputDeviceListener` and the source filtering, because
//! `InputDevice.getSources()` and the `SOURCE_GAMEPAD` / `SOURCE_JOYSTICK`
//! constants are Java-side API with no NDK equivalent, and calls the entry
//! points below.
//!
//! # Every entry point publishes the WHOLE pad, and that is a fix
//!
//! `GamepadManager::set_state` REPLACES the slot outright, so an entry point
//! that filled its own fields and left the rest at `Default` erased everything
//! the other entry points had just published: pressing a button zeroed the
//! sticks, and moving a stick released every held button. Android is the only
//! backend where this could happen, because it is the only push-driven one -
//! the polled backends build a complete snapshot every frame by construction.
//!
//! So the per-device state is accumulated HERE, in [`PADS`], and each entry
//! point updates its own fields and republishes the full snapshot.
//!
//! # The pad's own IMU (8f-i-a)
//!
//! `InputDevice.getSensorManager()` (API 31) hands back a `SensorManager`
//! SCOPED TO THAT CONTROLLER, so a DualSense's gyro arrives separately from
//! the phone's own. That distinction is the whole point of
//! `GamepadState::gyro_*`: a game that aims with the pad must not read the
//! phone. Both Android units already match azul-core's (m/s^2 and rad/s).
//!
//! The touch surface is filled ONLY under pointer capture (8f-i-a-ii):
//! uncaptured, the platform turns a DualShock touchpad into an on-screen
//! mouse pointer and exposes nothing; while `CallbackInfo::set_pointer_lock`
//! holds `View.requestPointerCapture`, the surface arrives as a
//! `SOURCE_TOUCHPAD` captured-pointer event with absolute positions, which
//! `AzulGamepad.onCapturedPointer` normalises and forwards to
//! `nativeOnTouchpad`. Without a capture `touchpad_active` stays false.

use azul_core::gamepad::{GamepadButton, GamepadId, GamepadState};
use azul_layout::managers::gamepad::push_gamepad_state;

use super::{apply_axial_deadzone, apply_radial_deadzone};

/// Register the input-device listener.
///
/// A no-op on the Rust side: the listener is a Java object, and registering
/// it is the Java half's job at activity start. Kept so the dispatcher's
/// `start()` has the same shape on every platform.
pub fn start() {}

/// Map an Android `KeyEvent` keycode to a `GamepadButton`.
///
/// The `KEYCODE_BUTTON_*` values, which are stable Android API constants.
/// A and B are SWAPPED relative to the Xbox labels on purpose: Android names
/// them by POSITION (A is the bottom face button) and so does
/// `GamepadButton::South`, so the mapping is positional on both sides and a
/// Nintendo-layout pad does not come out mirrored.
fn keycode_to_button(keycode: i32) -> Option<GamepadButton> {
    Some(match keycode {
        96 => GamepadButton::South,         // KEYCODE_BUTTON_A
        97 => GamepadButton::East,          // KEYCODE_BUTTON_B
        99 => GamepadButton::North,         // KEYCODE_BUTTON_X
        100 => GamepadButton::West,         // KEYCODE_BUTTON_Y
        102 => GamepadButton::LeftBumper,   // KEYCODE_BUTTON_L1
        103 => GamepadButton::RightBumper,  // KEYCODE_BUTTON_R1
        104 => GamepadButton::LeftTrigger,  // KEYCODE_BUTTON_L2
        105 => GamepadButton::RightTrigger, // KEYCODE_BUTTON_R2
        106 => GamepadButton::LeftThumb,    // KEYCODE_BUTTON_THUMBL
        107 => GamepadButton::RightThumb,   // KEYCODE_BUTTON_THUMBR
        108 => GamepadButton::Start,        // KEYCODE_BUTTON_START
        109 => GamepadButton::Select,       // KEYCODE_BUTTON_SELECT
        110 => GamepadButton::Mode,         // KEYCODE_BUTTON_MODE
        19 => GamepadButton::DPadUp,        // KEYCODE_DPAD_UP
        20 => GamepadButton::DPadDown,      // KEYCODE_DPAD_DOWN
        21 => GamepadButton::DPadLeft,      // KEYCODE_DPAD_LEFT
        22 => GamepadButton::DPadRight,     // KEYCODE_DPAD_RIGHT
        _ => return None,
    })
}

/// Per-pad ACCUMULATED state, keyed by Android device id.
///
/// The accumulator itself lives in the shared `mod.rs` so its behaviour can be
/// TESTED - this file is `cfg`-gated to a target this machine never runs tests
/// on, and the accumulation is exactly the part that goes wrong silently.
static PADS: std::sync::Mutex<Option<super::PadAccumulator>> = std::sync::Mutex::new(None);

/// Update one pad's accumulated state and hand back the FULL snapshot to
/// publish. `None` only if the lock is poisoned.
fn with_pad(device_id: i32, f: impl FnOnce(&mut GamepadState)) -> Option<GamepadState> {
    let mut guard = PADS.lock().ok()?;
    Some(
        guard
            .get_or_insert_with(Default::default)
            .update(device_id as u32, f),
    )
}

/// `Sensor.TYPE_ACCELEROMETER`. The wire code IS Android's own constant,
/// passed straight through from `Sensor.getType()`, so there is no second
/// numbering to drift out of sync with the Java side.
const ANDROID_SENSOR_ACCELEROMETER: i32 = 1;
/// `Sensor.TYPE_GYROSCOPE`.
const ANDROID_SENSOR_GYROSCOPE: i32 = 4;

/// A gamepad button went down or up.
#[no_mangle]
pub unsafe extern "system" fn Java_com_azul_gamepad_AzulGamepad_nativeOnButton(
    _env: *mut core::ffi::c_void,
    _class: *mut core::ffi::c_void,
    device_id: i32,
    keycode: i32,
    is_down: i32,
) {
    let Some(button) = keycode_to_button(keycode) else {
        return;
    };
    let Some(state) = with_pad(device_id, |p| {
        p.connected = true;
        if is_down != 0 {
            p.buttons |= button.bit();
        } else {
            p.buttons &= !button.bit();
        }
    }) else {
        return;
    };
    push_gamepad_state(state);
}

/// The pad's touch surface under pointer capture (8f-i-a-ii): two fingers
/// (8f-i-a-ii-a), normalised 0..1, y up (the Java side flipped it).
/// `active == 0` is the lift-off of that slot.
#[no_mangle]
pub unsafe extern "system" fn Java_com_azul_gamepad_AzulGamepad_nativeOnTouchpad(
    _env: *mut core::ffi::c_void,
    _class: *mut core::ffi::c_void,
    device_id: i32,
    active: i32,
    x: f32,
    y: f32,
    active2: i32,
    x2: f32,
    y2: f32,
) {
    let Some(state) = with_pad(device_id, |p| {
        p.connected = true;
        p.touchpad_active = active != 0;
        if active != 0 {
            p.touchpad_x = x.clamp(0.0, 1.0);
            p.touchpad_y = y.clamp(0.0, 1.0);
        }
        p.touchpad2_active = active2 != 0;
        if active2 != 0 {
            p.touchpad2_x = x2.clamp(0.0, 1.0);
            p.touchpad2_y = y2.clamp(0.0, 1.0);
        }
    }) else {
        return;
    };
    push_gamepad_state(state);
}

/// A gamepad's axes moved.
///
/// Android names them `AXIS_X`/`AXIS_Y` for the left stick and
/// `AXIS_Z`/`AXIS_RZ` for the right — NOT `AXIS_RX`/`AXIS_RY`, which is the
/// mapping most controllers actually report and the one every Android gamepad
/// sample uses. Getting that wrong swaps the right stick for two dead axes.
///
/// The hat is a pair of axes, not buttons: `AXIS_HAT_X`/`AXIS_HAT_Y` each
/// report -1, 0 or +1, so a d-pad on a gamepad-source device arrives here and
/// a d-pad on a dpad-source device arrives as key events. Both are folded
/// into the same four buttons.
#[no_mangle]
pub unsafe extern "system" fn Java_com_azul_gamepad_AzulGamepad_nativeOnAxes(
    _env: *mut core::ffi::c_void,
    _class: *mut core::ffi::c_void,
    device_id: i32,
    x: f32,
    y: f32,
    z: f32,
    rz: f32,
    ltrigger: f32,
    rtrigger: f32,
    hat_x: f32,
    hat_y: f32,
) {
    let (lx, ly) = apply_radial_deadzone(x, y);
    let (rx, ry) = apply_radial_deadzone(z, rz);

    let hat_bits = {
        let mut b = 0u32;
        if hat_y < -0.5 {
            b |= GamepadButton::DPadUp.bit();
        }
        if hat_y > 0.5 {
            b |= GamepadButton::DPadDown.bit();
        }
        if hat_x < -0.5 {
            b |= GamepadButton::DPadLeft.bit();
        }
        if hat_x > 0.5 {
            b |= GamepadButton::DPadRight.bit();
        }
        b
    };
    // Merged with the key-event buttons rather than replacing them: a pad can
    // report its d-pad either way, and clobbering would make whichever
    // arrived second win.
    let dpad_mask = GamepadButton::DPadUp.bit()
        | GamepadButton::DPadDown.bit()
        | GamepadButton::DPadLeft.bit()
        | GamepadButton::DPadRight.bit();
    let Some(state) = with_pad(device_id, |p| {
        p.connected = true;
        p.buttons = (p.buttons & !dpad_mask) | hat_bits;
        p.left_stick_x = lx;
        p.left_stick_y = ly;
        p.right_stick_x = rx;
        p.right_stick_y = ry;
        p.left_z = apply_axial_deadzone(ltrigger);
        p.right_z = apply_axial_deadzone(rtrigger);
    }) else {
        return;
    };
    push_gamepad_state(state);
}

/// A reading from the CONTROLLER's own accelerometer or gyroscope.
///
/// `InputDevice.getSensorManager()` (API 31) scopes a `SensorManager` to one
/// input device, which is what makes this the PAD's motion and not the
/// phone's - the distinction `GamepadState::gyro_*` exists for.
///
/// `kind` is Android's own `Sensor.getType()`, passed through unchanged: any
/// other type is ignored rather than mapped, so a device that also reports a
/// magnetometer or a step counter cannot land in the gyro fields.
///
/// Units need no conversion - Android reports m/s^2 and rad/s, which are
/// already `GamepadState`'s.
#[no_mangle]
pub unsafe extern "system" fn Java_com_azul_gamepad_AzulGamepad_nativeOnMotionSensor(
    _env: *mut core::ffi::c_void,
    _class: *mut core::ffi::c_void,
    device_id: i32,
    kind: i32,
    x: f32,
    y: f32,
    z: f32,
) {
    let Some(state) = with_pad(device_id, |p| match kind {
        ANDROID_SENSOR_ACCELEROMETER => {
            p.accel_x = x;
            p.accel_y = y;
            p.accel_z = z;
        }
        ANDROID_SENSOR_GYROSCOPE => {
            p.gyro_x = x;
            p.gyro_y = y;
            p.gyro_z = z;
        }
        _ => {}
    }) else {
        return;
    };
    push_gamepad_state(state);
}

/// A gamepad was attached or detached.
#[no_mangle]
pub unsafe extern "system" fn Java_com_azul_gamepad_AzulGamepad_nativeOnDeviceChanged(
    _env: *mut core::ffi::c_void,
    _class: *mut core::ffi::c_void,
    device_id: i32,
    connected: i32,
) {
    if connected == 0 {
        // Drop the accumulated state, or a pad reconnecting on the same device
        // id would come back with its old buttons held and its old gyro.
        if let Ok(mut guard) = PADS.lock() {
            if let Some(acc) = guard.as_mut() {
                acc.forget(device_id as u32);
            }
        }
    }
    push_gamepad_state(GamepadState {
        id: GamepadId {
            id: device_id as u32,
        },
        connected: connected != 0,
        battery: -1.0,
        ..Default::default()
    });
}
