//! iOS gamepad backend — Game Controller framework (`GCController`).
//!
//! Written against the raw objc runtime rather than waiting for
//! `objc2-game-controller`: the iOS shell already talks to UIKit this way, the
//! surface needed here is a dozen properties, and a whole crate dependency to
//! read them would be a worse trade than the `msg_send!` calls below.
//!
//! macOS is NOT this backend — gilrs covers it — so everything here is
//! iOS/tvOS only.

use azul_core::gamepad::{GamepadButton, GamepadId, GamepadState};
use azul_layout::managers::gamepad::push_gamepad_state;
use objc::runtime::Object;
use objc::{class, msg_send, sel, sel_impl};

use super::{apply_axial_deadzone, apply_radial_deadzone};

/// Begin observing controller connect/disconnect.
///
/// `startWirelessControllerDiscovery` is what makes an unpaired MFi or
/// DualSense controller show up at all — `controllers()` lists only what is
/// already connected, so without this a pad paired at the OS level appears but
/// one being paired now never does.
pub fn start() {
    unsafe {
        let cls = class!(GCController);
        let _: () = msg_send![cls, startWirelessControllerDiscoveryWithCompletionHandler: core::ptr::null::<core::ffi::c_void>()];
    }
}

/// Read a `GCControllerButtonInput.isPressed`.
unsafe fn pressed(button: *mut Object) -> bool {
    if button.is_null() {
        return false;
    }
    msg_send![button, isPressed]
}

/// Read a `GCControllerButtonInput.value` (analog triggers).
unsafe fn analog(button: *mut Object) -> f32 {
    if button.is_null() {
        return 0.0;
    }
    let v: f32 = msg_send![button, value];
    v
}

/// Read a `GCControllerAxisInput.value` off a thumbstick.
unsafe fn axis(stick: *mut Object, sel_name: objc::runtime::Sel) -> f32 {
    if stick.is_null() {
        return 0.0;
    }
    let ax: *mut Object =
        objc::__send_message(&*stick, sel_name, ()).unwrap_or(core::ptr::null_mut());
    if ax.is_null() {
        return 0.0;
    }
    let v: f32 = msg_send![ax, value];
    v
}

/// Snapshot every connected controller.
pub fn poll() {
    unsafe {
        let cls = class!(GCController);
        let controllers: *mut Object = msg_send![cls, controllers];
        if controllers.is_null() {
            return;
        }
        let count: usize = msg_send![controllers, count];
        for i in 0..count {
            let controller: *mut Object = msg_send![controllers, objectAtIndex: i];
            if controller.is_null() {
                continue;
            }
            // `extendedGamepad` is nil for a controller that only implements
            // the older micro profile (a Siri Remote), which is not a gamepad
            // in any useful sense — skipping is correct, not a gap.
            let pad: *mut Object = msg_send![controller, extendedGamepad];
            if pad.is_null() {
                continue;
            }

            let mut buttons = 0u32;
            let mut set = |b: GamepadButton, on: bool| {
                if on {
                    buttons |= b.bit();
                }
            };

            // Face buttons are POSITIONAL on both sides: GCExtendedGamepad's
            // buttonA is the bottom face button, and so is
            // GamepadButton::South. Mapping by label instead would mirror
            // every Nintendo-layout pad.
            set(GamepadButton::South, pressed(msg_send![pad, buttonA]));
            set(GamepadButton::East, pressed(msg_send![pad, buttonB]));
            set(GamepadButton::North, pressed(msg_send![pad, buttonX]));
            set(GamepadButton::West, pressed(msg_send![pad, buttonY]));
            set(
                GamepadButton::LeftBumper,
                pressed(msg_send![pad, leftShoulder]),
            );
            set(
                GamepadButton::RightBumper,
                pressed(msg_send![pad, rightShoulder]),
            );
            set(
                GamepadButton::LeftTrigger,
                pressed(msg_send![pad, leftTrigger]),
            );
            set(
                GamepadButton::RightTrigger,
                pressed(msg_send![pad, rightTrigger]),
            );

            // Menu / Options / Home are iOS 13+. respondsToSelector rather
            // than a version check, because the selector's presence is the
            // thing that actually matters and a tvOS build reports a
            // different version number for the same API.
            let responds =
                |s: objc::runtime::Sel| -> bool { msg_send![pad, respondsToSelector: s] };
            if responds(sel!(buttonMenu)) {
                set(GamepadButton::Start, pressed(msg_send![pad, buttonMenu]));
            }
            if responds(sel!(buttonOptions)) {
                set(
                    GamepadButton::Select,
                    pressed(msg_send![pad, buttonOptions]),
                );
            }
            if responds(sel!(buttonHome)) {
                set(GamepadButton::Mode, pressed(msg_send![pad, buttonHome]));
            }
            if responds(sel!(leftThumbstickButton)) {
                set(
                    GamepadButton::LeftThumb,
                    pressed(msg_send![pad, leftThumbstickButton]),
                );
            }
            if responds(sel!(rightThumbstickButton)) {
                set(
                    GamepadButton::RightThumb,
                    pressed(msg_send![pad, rightThumbstickButton]),
                );
            }

            let dpad: *mut Object = msg_send![pad, dpad];
            if !dpad.is_null() {
                set(GamepadButton::DPadUp, pressed(msg_send![dpad, up]));
                set(GamepadButton::DPadDown, pressed(msg_send![dpad, down]));
                set(GamepadButton::DPadLeft, pressed(msg_send![dpad, left]));
                set(GamepadButton::DPadRight, pressed(msg_send![dpad, right]));
            }

            let lstick: *mut Object = msg_send![pad, leftThumbstick];
            let rstick: *mut Object = msg_send![pad, rightThumbstick];
            let (lx, ly) =
                apply_radial_deadzone(axis(lstick, sel!(xAxis)), axis(lstick, sel!(yAxis)));
            let (rx, ry) =
                apply_radial_deadzone(axis(rstick, sel!(xAxis)), axis(rstick, sel!(yAxis)));

            // playerIndex is -1 until the OS assigns one, which it does not
            // do for a single controller — so it cannot be the id. The
            // controller's address is stable for its lifetime and unique
            // across pads, which is what the id needs to be.
            let id = (controller as usize as u64 >> 4) as u32;

            // Battery, via `GCDeviceBattery` (macOS 11 / iOS 14+). Probed with
            // `respondsToSelector:` like every other optional control above,
            // rather than by a version check: the selector is the thing that
            // has to exist, and an older SDK simply answers false.
            //
            // `batteryLevel` is already 0..1, which is the field's own range,
            // so no conversion. `batteryState` is consulted only to
            // distinguish a pad that HAS no battery: state 1 (charging) and 2
            // (full) both carry a real level, while a wired pad with no cell
            // reports `unknown` (-1), and reporting that as 0.0 would draw an
            // empty battery icon for a controller that has none.
            const BATTERY_STATE_UNKNOWN: i64 = -1;
            let battery = if msg_send![controller, respondsToSelector: sel!(battery)] {
                let battery_obj: *mut Object = msg_send![controller, battery];
                if battery_obj.is_null() {
                    -1.0
                } else {
                    let state: i64 = msg_send![battery_obj, batteryState];
                    if state == BATTERY_STATE_UNKNOWN {
                        -1.0
                    } else {
                        let level: f32 = msg_send![battery_obj, batteryLevel];
                        level.clamp(0.0, 1.0)
                    }
                }
            } else {
                -1.0
            };

            push_gamepad_state(GamepadState {
                id: GamepadId { id },
                connected: true,
                buttons,
                left_stick_x: lx,
                left_stick_y: ly,
                right_stick_x: rx,
                right_stick_y: ry,
                left_z: apply_axial_deadzone(analog(msg_send![pad, leftTrigger])),
                right_z: apply_axial_deadzone(analog(msg_send![pad, rightTrigger])),
                battery,
                // The pad's own IMU (GCMotion) and its touch surface are still
                // unfilled here — 8f-i-a. Unlike battery, those have no
                // equivalent on the gilrs desktop backend, so filling them
                // only on Apple WOULD make the platforms diverge.
                ..Default::default()
            });
        }
    }
}
