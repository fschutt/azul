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
use core::ptr;
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


// ─── Rumble through CoreHaptics (9g-i-d-a-i) ───────────────────────────
//
// A controller's motors are reached through `GCController.haptics`, a
// `GCDeviceHaptics` (iOS 14+) that hands out a CoreHaptics ENGINE per
// locality (`createEngineWithLocality:`) - not a fire-and-forget call like
// every other backend on this path. Playing one buzz is: a
// `CHHapticEventParameter` for intensity and one for sharpness, a
// `CHHapticEvent` of type `CHHapticEventTypeHapticContinuous` over them, a
// `CHHapticPattern` over the event, `createPlayerWithPattern:error:` on the
// engine, and `startAtTime:error:` on the player. Five classes with error
// out-parameters, which is why this is its own section with its own cache.
//
// THE ENGINE IS CACHED PER PAD AND LOCALITY: `startAndReturnError:` is the
// expensive part, and starting an engine per request would stutter every
// pulse. THE PLAYER IS STOPPED BEFORE THE NEXT ONE STARTS on the same
// engine, for the reason the gilrs backend gives - two overlapping patterns
// sum in the driver, so a repeated tap climbs to full amplitude and stays.
//
// CoreHaptics is DLOPEN'D, not linked: it is a separate framework from
// GameController, and a device without it (tvOS 13, the simulator's older
// runtimes) must fail the probe rather than the dynamic link. Its `NSString
// * const` event-type and parameter-id constants are exported symbols whose
// string values are not documented, so each is read through `dlsym` the way
// `media_keys/apple.rs` reads MediaPlayer's keys. The `GCHapticsLocality*`
// constants come from GameController the same way.
//
// Everything here is proven COMPILED on the three iOS targets and nothing
// more: there is no controller on this machine. The locality choice (left
// grip = strong motor) is the one assumption a device would settle in a
// minute - see 9g-i-d-a-i-a.

use super::{rumble_plan, RumbleLocality, RumblePlan};

/// One engine per (pad, locality), plus the player last started on it.
struct PadEngine {
    pad: u32,
    locality: RumbleLocality,
    engine: *mut Object,
    /// The last player, kept so the next rumble can stop it. Retained by us
    /// (`createPlayerWithPattern:` returns an autoreleased object that is
    /// `retain`ed here) and released when replaced.
    player: *mut Object,
}

// The raw pointers are Objective-C objects owned by this cache and only ever
// touched from the main thread the shell pumps on; the mutex exists so the
// cache is a plain static rather than a `static mut`.
unsafe impl Send for PadEngine {}

static ENGINES: std::sync::Mutex<Vec<PadEngine>> = std::sync::Mutex::new(Vec::new());

/// CoreHaptics.framework, dlopen'd once. `None` where it does not exist.
fn core_haptics() -> Option<&'static libloading::Library> {
    static LIB: std::sync::OnceLock<Option<libloading::Library>> = std::sync::OnceLock::new();
    LIB.get_or_init(|| unsafe {
        libloading::Library::new("/System/Library/Frameworks/CoreHaptics.framework/CoreHaptics")
            .ok()
    })
    .as_ref()
}

/// GameController.framework, for its `GCHapticsLocality*` constants. The
/// classes are reachable already (`class!(GCController)` above works), but
/// the constants are data symbols and need a handle to look them up on.
fn game_controller() -> Option<&'static libloading::Library> {
    static LIB: std::sync::OnceLock<Option<libloading::Library>> = std::sync::OnceLock::new();
    LIB.get_or_init(|| unsafe {
        libloading::Library::new(
            "/System/Library/Frameworks/GameController.framework/GameController",
        )
        .ok()
    })
    .as_ref()
}

/// An exported `NSString * const`: `dlsym` gives the address of the variable,
/// the variable holds the object.
unsafe fn string_constant(lib: &libloading::Library, symbol: &[u8]) -> Option<*mut Object> {
    // `Symbol<T>` derefs to the symbol's ADDRESS typed as `T`, so for a
    // variable holding an `NSString *` that is `*mut *mut Object`, and the
    // object is one more dereference away - libloading's own
    // `**awesome_variable` example. `Symbol<*mut Object>` + one `*` would
    // hand the framework the address of the variable as if it were the
    // string.
    let sym: libloading::Symbol<'_, *mut *mut Object> = lib.get(symbol).ok()?;
    let slot: *mut *mut Object = *sym;
    if slot.is_null() {
        return None;
    }
    let value: *mut Object = *slot;
    (!value.is_null()).then_some(value)
}

unsafe fn locality_constant(locality: RumbleLocality) -> Option<*mut Object> {
    let lib = game_controller()?;
    let symbol: &[u8] = match locality {
        RumbleLocality::LeftHandle => b"GCHapticsLocalityLeftHandle\0",
        RumbleLocality::RightHandle => b"GCHapticsLocalityRightHandle\0",
        RumbleLocality::Default => b"GCHapticsLocalityDefault\0",
    };
    string_constant(lib, symbol)
}

/// The `GCController` whose address-derived id (see `poll`) is `pad`.
unsafe fn controller_for_pad(pad: u32) -> Option<*mut Object> {
    let cls = class!(GCController);
    let controllers: *mut Object = msg_send![cls, controllers];
    if controllers.is_null() {
        return None;
    }
    let count: usize = msg_send![controllers, count];
    (0..count)
        .map(|i| {
            let c: *mut Object = msg_send![controllers, objectAtIndex: i];
            c
        })
        .find(|c| !c.is_null() && (*c as usize as u64 >> 4) as u32 == pad)
}

/// The engine for `(pad, locality)`, created and STARTED on first use. A pad
/// whose `haptics` is nil (no actuator - a Siri Remote, an MFi pad from
/// before iOS 14) yields `None`, as does a locality the pad does not report
/// in `supportedLocalities` - the caller then retries with `Default`, which
/// the framework guarantees.
unsafe fn engine_for(pad: u32, locality: RumbleLocality) -> Option<*mut Object> {
    if let Ok(cache) = ENGINES.lock() {
        if let Some(e) = cache.iter().find(|e| e.pad == pad && e.locality == locality) {
            return Some(e.engine);
        }
    }
    let controller = controller_for_pad(pad)?;
    let responds: bool = msg_send![controller, respondsToSelector: sel!(haptics)];
    if !responds {
        return None;
    }
    let haptics: *mut Object = msg_send![controller, haptics];
    if haptics.is_null() {
        return None;
    }
    let locality_obj = locality_constant(locality)?;
    if locality != RumbleLocality::Default {
        let supported: *mut Object = msg_send![haptics, supportedLocalities];
        if supported.is_null() {
            return None;
        }
        let has: bool = msg_send![supported, containsObject: locality_obj];
        if !has {
            return None;
        }
    }
    let engine: *mut Object = msg_send![haptics, createEngineWithLocality: locality_obj];
    if engine.is_null() {
        return None;
    }
    let _: () = msg_send![engine, retain];
    // Start now, once: "you need the engine to play that pattern", and a
    // start per request is the stutter the cache exists to avoid. Auto
    // shutdown lets the framework idle the engine and restart it on the next
    // player, so a backgrounded app does not hold a running engine.
    let _: () = msg_send![engine, setAutoShutdownEnabled: true];
    let mut error: *mut Object = ptr::null_mut();
    let started: bool = msg_send![engine, startAndReturnError: &mut error];
    if !started {
        crate::log_warn!(
            crate::desktop::shell2::common::debug_server::LogCategory::Input,
            "[iOS] CHHapticEngine start failed for pad {pad} ({locality:?})"
        );
        let _: () = msg_send![engine, release];
        return None;
    }
    if let Ok(mut cache) = ENGINES.lock() {
        cache.push(PadEngine {
            pad,
            locality,
            engine,
            player: ptr::null_mut(),
        });
    }
    Some(engine)
}

/// Stop and release whatever player the engine last started, then remember
/// the new one (retained). Passing null just stops.
unsafe fn swap_player(engine: *mut Object, new_player: *mut Object) {
    let Ok(mut cache) = ENGINES.lock() else {
        return;
    };
    let Some(entry) = cache.iter_mut().find(|e| e.engine == engine) else {
        return;
    };
    if !entry.player.is_null() {
        let mut error: *mut Object = ptr::null_mut();
        let _: bool = msg_send![entry.player, stopAtTime: 0.0f64 error: &mut error];
        let _: () = msg_send![entry.player, release];
    }
    entry.player = new_player;
}

/// Build the one-event pattern the plan describes and play it on `engine`.
unsafe fn play_on(engine: *mut Object, plan: RumblePlan) -> bool {
    let Some(lib) = core_haptics() else {
        return false;
    };
    let (Some(ty_continuous), Some(id_intensity), Some(id_sharpness)) = (
        string_constant(lib, b"CHHapticEventTypeHapticContinuous\0"),
        string_constant(lib, b"CHHapticEventParameterIDHapticIntensity\0"),
        string_constant(lib, b"CHHapticEventParameterIDHapticSharpness\0"),
    ) else {
        return false;
    };
    let (Some(param_cls), Some(event_cls), Some(pattern_cls), Some(array_cls)) = (
        objc::runtime::Class::get("CHHapticEventParameter"),
        objc::runtime::Class::get("CHHapticEvent"),
        objc::runtime::Class::get("CHHapticPattern"),
        objc::runtime::Class::get("NSArray"),
    ) else {
        return false;
    };

    let intensity: *mut Object = msg_send![param_cls, alloc];
    let intensity: *mut Object =
        msg_send![intensity, initWithParameterID: id_intensity value: plan.intensity];
    let sharpness: *mut Object = msg_send![param_cls, alloc];
    let sharpness: *mut Object =
        msg_send![sharpness, initWithParameterID: id_sharpness value: plan.sharpness];
    if intensity.is_null() || sharpness.is_null() {
        return false;
    }
    let params_raw: [*mut Object; 2] = [intensity, sharpness];
    let params: *mut Object =
        msg_send![array_cls, arrayWithObjects: params_raw.as_ptr() count: 2usize];

    let event: *mut Object = msg_send![event_cls, alloc];
    let event: *mut Object = msg_send![
        event,
        initWithEventType: ty_continuous
        parameters: params
        relativeTime: 0.0f64
        duration: plan.seconds
    ];
    if event.is_null() {
        return false;
    }
    let events_raw: [*mut Object; 1] = [event];
    let events: *mut Object =
        msg_send![array_cls, arrayWithObjects: events_raw.as_ptr() count: 1usize];
    let empty: *mut Object = msg_send![array_cls, array];

    let mut error: *mut Object = ptr::null_mut();
    let pattern: *mut Object = msg_send![pattern_cls, alloc];
    let pattern: *mut Object = msg_send![
        pattern,
        initWithEvents: events
        parameters: empty
        error: &mut error
    ];
    if pattern.is_null() {
        return false;
    }
    let player: *mut Object =
        msg_send![engine, createPlayerWithPattern: pattern error: &mut error];
    if player.is_null() {
        return false;
    }
    let _: () = msg_send![player, retain];
    swap_player(engine, player);
    // 0.0 is `CHHapticTimeImmediate`: the macro's value in CHHapticEngine.h,
    // spelled out because a macro has no symbol to dlsym.
    let started: bool =
        msg_send![player, startAtTime: 0.0f64 error: &mut error];
    started
}

/// Rumble `pad` - the id `poll` reports on `GamepadState` - with the given
/// intensity for `duration_ms` (already resolved, see `rumble_duration_ms`),
/// on the strong (left grip) or weak (right grip) actuator. Returns whether a
/// player actually started. A locality the pad does not have falls back to
/// `GCHapticsLocalityDefault`, which every haptics-capable pad reports.
pub fn rumble(pad: u32, intensity: f32, duration_ms: u32, strong: bool) -> bool {
    let Some(plan) = rumble_plan(intensity, duration_ms, strong) else {
        // Not playing is also "stop what was playing": intensity 0 is the
        // documented way to end a rumble early on every backend.
        stop_pad(pad);
        return false;
    };
    unsafe {
        let engine = engine_for(pad, plan.locality).or_else(|| {
            engine_for(
                pad,
                RumbleLocality::Default,
            )
        });
        let Some(engine) = engine else {
            return false;
        };
        play_on(engine, plan)
    }
}

fn stop_pad(pad: u32) {
    let engines: Vec<*mut Object> = ENGINES
        .lock()
        .map(|c| c.iter().filter(|e| e.pad == pad).map(|e| e.engine).collect())
        .unwrap_or_default();
    for engine in engines {
        unsafe { swap_player(engine, ptr::null_mut()) };
    }
}

/// Stop every player on every pad. Run at termination for the reason the
/// gilrs backend runs its twin: the process exits without destructors, and a
/// motor that was mid-pattern keeps going until the pattern ends on its own -
/// which for a long rumble is after the app is gone.
pub fn stop_all_rumble() {
    let engines: Vec<*mut Object> = ENGINES
        .lock()
        .map(|c| c.iter().map(|e| e.engine).collect())
        .unwrap_or_default();
    for engine in engines {
        unsafe { swap_player(engine, ptr::null_mut()) };
    }
}
