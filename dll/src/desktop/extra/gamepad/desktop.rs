//! Desktop gamepad backend — `gilrs` (Windows / Linux / macOS).
//!
//! [`poll`] pumps gilrs's event queue (which refreshes its internal per-pad
//! state), then snapshots every connected pad into a [`GamepadState`] and
//! parks it via `push_gamepad_state`; the layout pass folds the latest per
//! id into the `GamepadManager`. A `Disconnected` event parks an empty
//! (`connected = false`) state so the manager can clear that pad.
//!
//! The `Gilrs` context is `!Send`/`!Sync` (it owns platform device handles),
//! so it lives in a `thread_local` and initialises lazily on the first poll
//! — which runs on the layout/event-loop thread, the same thread every
//! frame.
//!
//! Button naming differs from azul-core's: gilrs `LeftTrigger`/`RightTrigger`
//! are the **shoulder** buttons (L1/R1), and `LeftTrigger2`/`RightTrigger2`
//! the analog triggers (L2/R2). [`BUTTON_MAP`] translates to azul-core's
//! `LeftBumper`/`RightBumper` + `LeftTrigger`/`RightTrigger`.

use std::cell::RefCell;

use gilrs::{Axis, Button, EventType, Gilrs};

use azul_core::gamepad::{GamepadButton, GamepadId, GamepadState};
use azul_layout::managers::gamepad::push_gamepad_state;

thread_local! {
    /// The process's gilrs context (per the layout thread). `None` until the
    /// first successful `poll`; stays `None` if gilrs can't initialise.
    static GILRS: RefCell<Option<Gilrs>> = const { RefCell::new(None) };
}

/// azul-core button → gilrs button. (gilrs `LeftTrigger` = L1 shoulder,
/// `LeftTrigger2` = L2 analog trigger; azul-core splits them as
/// `LeftBumper` / `LeftTrigger`.)
const BUTTON_MAP: [(GamepadButton, Button); 17] = [
    (GamepadButton::South, Button::South),
    (GamepadButton::East, Button::East),
    (GamepadButton::North, Button::North),
    (GamepadButton::West, Button::West),
    (GamepadButton::LeftBumper, Button::LeftTrigger),
    (GamepadButton::RightBumper, Button::RightTrigger),
    (GamepadButton::LeftTrigger, Button::LeftTrigger2),
    (GamepadButton::RightTrigger, Button::RightTrigger2),
    (GamepadButton::Select, Button::Select),
    (GamepadButton::Start, Button::Start),
    (GamepadButton::Mode, Button::Mode),
    (GamepadButton::LeftThumb, Button::LeftThumb),
    (GamepadButton::RightThumb, Button::RightThumb),
    (GamepadButton::DPadUp, Button::DPadUp),
    (GamepadButton::DPadDown, Button::DPadDown),
    (GamepadButton::DPadLeft, Button::DPadLeft),
    (GamepadButton::DPadRight, Button::DPadRight),
];

pub fn poll() {
    GILRS.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.is_none() {
            // Lazy init on first poll (same thread every frame). These two logs
            // bracket gilrs's libudev/evdev enumeration — the suspect for the
            // reported Linux "double free in tcache2" (C5). If the CI/self-test
            // log shows "initialising gilrs" with no following line, the abort is
            // inside Gilrs::new (a gilrs/libudev issue), not azul code.
            crate::plog_info!("[gamepad] initialising gilrs (libudev/evdev enumeration)");
            *slot = Gilrs::new().ok();
            match slot.as_ref() {
                Some(g) => crate::plog_info!(
                    "[gamepad] gilrs initialised; {} pad(s) present",
                    g.gamepads().count()
                ),
                None => crate::plog_warn!(
                    "[gamepad] gilrs failed to initialise — gamepad input unavailable"
                ),
            }
        }
        let Some(gilrs) = slot.as_mut() else {
            return;
        };

        // Pump the event queue to refresh gilrs's internal state; surface
        // disconnects so the manager can clear that pad's slot.
        while let Some(ev) = gilrs.next_event() {
            if matches!(ev.event, EventType::Disconnected) {
                crate::plog_info!("[gamepad] pad {} disconnected", usize::from(ev.id));
                push_gamepad_state(GamepadState::empty(GamepadId {
                    id: usize::from(ev.id) as u32,
                }));
            }
        }

        // Snapshot every currently-connected pad.
        for (gid, pad) in gilrs.gamepads() {
            let mut buttons = 0u32;
            for (mine, theirs) in BUTTON_MAP {
                if pad.is_pressed(theirs) {
                    buttons |= mine.bit();
                }
            }
            push_gamepad_state(GamepadState {
                id: GamepadId {
                    id: usize::from(gid) as u32,
                },
                connected: true,
                buttons,
                left_stick_x: pad.value(Axis::LeftStickX),
                left_stick_y: pad.value(Axis::LeftStickY),
                right_stick_x: pad.value(Axis::RightStickX),
                right_stick_y: pad.value(Axis::RightStickY),
                left_z: pad.value(Axis::LeftZ),
                right_z: pad.value(Axis::RightZ),
            });
        }
    });
}
