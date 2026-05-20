//! Android gamepad backend — `InputDevice` / `InputManager` (JNI).
//!
//! Will register an `InputManager.InputDeviceListener` (via a
//! `com.azul.gamepad.AzulGamepad` helper, same Rust/Java split as
//! `AzulSensors`) and translate `KeyEvent` (buttons) + `MotionEvent` (axes,
//! `AXIS_X`/`AXIS_Y`/`AXIS_Z`/`AXIS_RZ`/`AXIS_HAT_*`) from gamepad-source
//! devices, parking a [`azul_core::gamepad::GamepadState`] via
//! `push_gamepad_state` on each change. Push-based (the dispatcher's `poll`
//! is a no-op on Android).
//!
//! Pending (non-Rust): the `AzulGamepad.java` helper. Until it ships,
//! `start` is a no-op and no states flow — like the sensor backend pre-shim.

/// Register the input-device listener (no-op until the `AzulGamepad.java`
/// helper ships).
pub fn start() {}
