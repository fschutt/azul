//! iOS gamepad backend — Game Controller framework (`GCController`).
//!
//! Written against the raw objc runtime rather than waiting for
//! `objc2-game-controller`: the iOS shell already talks to UIKit this way, the
//! surface needed here is a dozen properties, and a whole crate dependency to
//! read them would be a worse trade than the `msg_send!` calls below.
//!
//! macOS is NOT this backend — gilrs covers it — so everything here is
//! iOS/tvOS only.
//!
//! # The pad's own IMU and touch surface (8f-i-a)
//!
//! `GCMotion` and `touchpadPrimary` are the two things gilrs cannot see at
//! all, and the Game Controller framework hands both over directly. Two traps
//! made this more than a property read:
//!
//! - **THE SENSORS ARE OFF UNTIL ASKED.** A DualSense reports
//!   `sensorsRequireManualActivation`, and until `sensorsActive` is set every
//!   read returns zeroes forever — which is indistinguishable from a pad with
//!   no gyro.
//! - **The vectors are STRUCT RETURNS**, and a 24-byte struct of three doubles
//!   comes back in registers on arm64 (an HFA) but through a hidden pointer
//!   on x86_64 (`objc_msgSend_stret`). The device target is arm64 and the
//!   SIMULATOR is x86_64, so both paths are live here. That is why those two
//!   reads go through `objc2`, which picks the right variant from the type's
//!   encoding, while the rest of the file stays on the `objc` 0.2 calls
//!   around it.

use azul_core::gamepad::{GamepadButton, GamepadId, GamepadState};
use azul_layout::managers::gamepad::push_gamepad_state;
use objc::runtime::Object;
use objc::{class, msg_send, sel, sel_impl};

use super::{apply_axial_deadzone, apply_radial_deadzone};

/// `GCAcceleration` and `GCRotationRate` are the same shape: three C doubles.
///
/// Declared here rather than pulled from a crate because the whole surface
/// this file needs from GameController is a dozen properties - the same trade
/// the module docs make for `msg_send!` over `objc2-game-controller`.
#[repr(C)]
#[derive(Copy, Clone, Default)]
struct GcVector3 {
    x: f64,
    y: f64,
    z: f64,
}

// SAFETY: the layout matches GCMotion.h's `typedef struct { double x, y, z; }`
// exactly, which is what the encoding has to describe for objc2 to choose
// between `objc_msgSend` and `objc_msgSend_stret`.
unsafe impl objc2::encode::Encode for GcVector3 {
    const ENCODING: objc2::encode::Encoding = objc2::encode::Encoding::Struct(
        "?",
        &[
            objc2::encode::Encoding::Double,
            objc2::encode::Encoding::Double,
            objc2::encode::Encoding::Double,
        ],
    );
}

/// Standard gravity: `GCAcceleration` is in G, `GamepadState::accel_*` is m/s^2.
const G_TO_MS2: f32 = 9.806_65;

/// `GCTouchStateUp` - no finger on the surface.
const GC_TOUCH_STATE_UP: i64 = 0;

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

            // --- The pad's own IMU (8f-i-a) ---
            //
            // `motion` is nil on a pad with no sensors, which is most of them:
            // an Xbox controller has none, a DualShock 4 / DualSense / Switch
            // Pro does.
            let mut gyro = GcVector3::default();
            let mut accel = GcVector3::default();
            let motion: *mut Object = msg_send![controller, motion];
            if !motion.is_null() {
                let motion_responds =
                    |s: objc::runtime::Sel| -> bool { msg_send![motion, respondsToSelector: s] };

                // THE ONE THAT MATTERS. A DualSense keeps its sensors powered
                // down until an app asks, so without this every read returns
                // zeroes for the life of the process - which looks exactly
                // like a pad that has no gyro. iOS 14 / macOS 11, hence the
                // selector probe rather than a version check.
                if motion_responds(sel!(sensorsRequireManualActivation)) {
                    let manual: bool = msg_send![motion, sensorsRequireManualActivation];
                    let active: bool = msg_send![motion, sensorsActive];
                    if manual && !active {
                        let _: () = msg_send![motion, setSensorsActive: true];
                    }
                }

                // `hasRotationRate` is iOS 14+; an older SDK has no such
                // property and simply always has the rate, so an absent
                // selector reads as "yes" rather than as "no".
                let has_rate = !motion_responds(sel!(hasRotationRate)) || {
                    let v: bool = msg_send![motion, hasRotationRate];
                    v
                };
                if has_rate {
                    // RADIANS PER SECOND already, per GCMotion.h - the one
                    // vector here that needs no conversion.
                    gyro = objc2::msg_send![motion as *mut objc2::runtime::AnyObject, rotationRate];
                }

                if motion_responds(sel!(acceleration)) {
                    accel = objc2::msg_send![
                        motion as *mut objc2::runtime::AnyObject,
                        acceleration
                    ];
                } else if motion_responds(sel!(hasGravityAndUserAcceleration)) {
                    // Pre-iOS-14 fallback. `acceleration` is exactly gravity
                    // plus user acceleration, so summing the two older
                    // properties reconstructs it rather than approximating it.
                    let has: bool = msg_send![motion, hasGravityAndUserAcceleration];
                    if has {
                        let g: GcVector3 = objc2::msg_send![
                            motion as *mut objc2::runtime::AnyObject,
                            gravity
                        ];
                        let u: GcVector3 = objc2::msg_send![
                            motion as *mut objc2::runtime::AnyObject,
                            userAcceleration
                        ];
                        accel = GcVector3 {
                            x: g.x + u.x,
                            y: g.y + u.y,
                            z: g.z + u.z,
                        };
                    }
                }
            }

            // --- The pad's touch surface (8f-i-a) ---
            //
            // `touchpadPrimary` exists only on the DualShock and DualSense
            // profiles, so the selector probe is also the "does this pad have
            // a touchpad" test.
            let mut touchpad_x = 0.0f32;
            let mut touchpad_y = 0.0f32;
            let mut touchpad_active = false;
            if responds(sel!(touchpadPrimary)) {
                let tp: *mut Object = msg_send![pad, touchpadPrimary];
                if !tp.is_null() {
                    // The axes are [-1, 1] like a thumbstick;
                    // `GamepadState::touchpad_*` is [0, 1] across the surface.
                    let tx = axis(tp, sel!(xAxis));
                    let ty = axis(tp, sel!(yAxis));
                    touchpad_x = (tx + 1.0) * 0.5;
                    touchpad_y = (ty + 1.0) * 0.5;

                    // `touchState` is the honest answer and lives on
                    // `GCControllerTouchpad`; the property is DECLARED as the
                    // plain `GCControllerDirectionPad` superclass, so whether
                    // the object actually carries it is a runtime question.
                    // The fallback cannot tell "no finger" from "a finger
                    // exactly at the centre", which is why it is a fallback.
                    let has_state: bool = msg_send![tp, respondsToSelector: sel!(touchState)];
                    touchpad_active = if has_state {
                        let st: i64 = msg_send![tp, touchState];
                        st != GC_TOUCH_STATE_UP
                    } else {
                        tx != 0.0 || ty != 0.0
                    };
                }
            }

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
                touchpad_x,
                touchpad_y,
                touchpad_active,
                gyro_x: gyro.x as f32,
                gyro_y: gyro.y as f32,
                gyro_z: gyro.z as f32,
                accel_x: accel.x as f32 * G_TO_MS2,
                accel_y: accel.y as f32 * G_TO_MS2,
                accel_z: accel.z as f32 * G_TO_MS2,
            });
        }
    }
}
