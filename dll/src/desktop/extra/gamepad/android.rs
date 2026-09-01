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
        96 => GamepadButton::South,        // KEYCODE_BUTTON_A
        97 => GamepadButton::East,         // KEYCODE_BUTTON_B
        99 => GamepadButton::North,        // KEYCODE_BUTTON_X
        100 => GamepadButton::West,        // KEYCODE_BUTTON_Y
        102 => GamepadButton::LeftBumper,  // KEYCODE_BUTTON_L1
        103 => GamepadButton::RightBumper, // KEYCODE_BUTTON_R1
        104 => GamepadButton::LeftTrigger, // KEYCODE_BUTTON_L2
        105 => GamepadButton::RightTrigger, // KEYCODE_BUTTON_R2
        106 => GamepadButton::LeftThumb,   // KEYCODE_BUTTON_THUMBL
        107 => GamepadButton::RightThumb,  // KEYCODE_BUTTON_THUMBR
        108 => GamepadButton::Start,       // KEYCODE_BUTTON_START
        109 => GamepadButton::Select,      // KEYCODE_BUTTON_SELECT
        110 => GamepadButton::Mode,        // KEYCODE_BUTTON_MODE
        19 => GamepadButton::DPadUp,       // KEYCODE_DPAD_UP
        20 => GamepadButton::DPadDown,     // KEYCODE_DPAD_DOWN
        21 => GamepadButton::DPadLeft,     // KEYCODE_DPAD_LEFT
        22 => GamepadButton::DPadRight,    // KEYCODE_DPAD_RIGHT
        _ => return None,
    })
}

/// Per-pad button bitsets, keyed by Android device id.
///
/// Android reports button presses as individual key events, not as a state
/// snapshot, so the held set has to be accumulated here — otherwise every
/// event would publish a state in which exactly one button was down.
static BUTTONS: std::sync::Mutex<Option<std::collections::BTreeMap<i32, u32>>> =
    std::sync::Mutex::new(None);

fn with_buttons<R>(f: impl FnOnce(&mut std::collections::BTreeMap<i32, u32>) -> R) -> Option<R> {
    let mut guard = BUTTONS.lock().ok()?;
    Some(f(guard.get_or_insert_with(Default::default)))
}

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
    let bits = with_buttons(|m| {
        let e = m.entry(device_id).or_insert(0);
        if is_down != 0 {
            *e |= button.bit();
        } else {
            *e &= !button.bit();
        }
        *e
    });
    let Some(buttons) = bits else { return };
    push_gamepad_state(GamepadState {
        id: GamepadId {
            id: device_id as u32,
        },
        connected: true,
        buttons,
        ..Default::default()
    });
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
    let buttons = with_buttons(|m| {
        let e = m.entry(device_id).or_insert(0);
        *e = (*e & !dpad_mask) | hat_bits;
        *e
    })
    .unwrap_or(hat_bits);

    push_gamepad_state(GamepadState {
        id: GamepadId {
            id: device_id as u32,
        },
        connected: true,
        buttons,
        left_stick_x: lx,
        left_stick_y: ly,
        right_stick_x: rx,
        right_stick_y: ry,
        left_z: apply_axial_deadzone(ltrigger),
        right_z: apply_axial_deadzone(rtrigger),
        ..Default::default()
    });
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
        // Drop the accumulated button state, or a pad reconnecting on the
        // same device id would come back with its old buttons held.
        let _ = with_buttons(|m| m.remove(&device_id));
    }
    push_gamepad_state(GamepadState {
        id: GamepadId {
            id: device_id as u32,
        },
        connected: connected != 0,
        ..Default::default()
    });
}
