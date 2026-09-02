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

use super::{apply_axial_deadzone, apply_radial_deadzone};
use std::cell::RefCell;

use gilrs::{Axis, Button, EventType, Gilrs, PowerInfo};

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

/// gilrs `PowerInfo` -> the `battery` field's documented contract.
///
/// `GamepadState::battery` is a `f32` in `[0, 1]` with `-1.0` meaning "not
/// reported" - a sentinel rather than an `Option` because the struct is
/// `#[repr(C)]` and a niche-optimised `Option<f32>` has no stable ABI.
///
/// `Wired` maps to the sentinel and NOT to `1.0`: a wired pad has no battery
/// at all, so reporting it as full would make "plugged in" and "fully charged"
/// indistinguishable, and a UI drawing a battery icon would draw one for a pad
/// that has none. The field's own docs say wired pads report `-1.0`.
///
/// `Charging` reports the level rather than the sentinel: the level is real
/// and known while charging, and an app that dims a low-battery warning during
/// a charge still needs the number to decide.
fn power_info_to_battery(info: PowerInfo) -> f32 {
    match info {
        PowerInfo::Unknown | PowerInfo::Wired => -1.0,
        PowerInfo::Discharging(pct) | PowerInfo::Charging(pct) => {
            // gilrs reports whole percent; clamped because the value comes
            // from a driver and a bad one must not escape the documented
            // range that every consumer trusts.
            f32::from(pct.min(100)) / 100.0
        }
        PowerInfo::Charged => 1.0,
    }
}

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
            // Keep the gilrs::Error — udev missing, permission denied and
            // no-device look identical without it.
            *slot = match Gilrs::new() {
                Ok(g) => Some(g),
                Err(e) => {
                    crate::plog_warn!(
                        "[gamepad] gilrs failed to initialise ({}) — gamepad input \
                         unavailable",
                        e
                    );
                    None
                }
            };
            if let Some(g) = slot.as_ref() {
                crate::plog_info!(
                    "[gamepad] gilrs initialised; {} pad(s) present",
                    g.gamepads().count()
                );
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
            // MWA-C-gamepad: radial deadzone per stick (triggers axial).
            // Raw pad.value() passthrough meant resting-stick jitter differed
            // bitwise between polls, so state_bitwise_eq saw a "change" and
            // the 16ms pump fired GamepadInput events continuously while a
            // pad was merely plugged in.
            let (lx, ly) =
                apply_radial_deadzone(pad.value(Axis::LeftStickX), pad.value(Axis::LeftStickY));
            let (rx, ry) =
                apply_radial_deadzone(pad.value(Axis::RightStickX), pad.value(Axis::RightStickY));
            push_gamepad_state(GamepadState {
                id: GamepadId {
                    id: usize::from(gid) as u32,
                },
                connected: true,
                buttons,
                left_stick_x: lx,
                left_stick_y: ly,
                right_stick_x: rx,
                right_stick_y: ry,
                left_z: apply_axial_deadzone(pad.value(Axis::LeftZ)),
                right_z: apply_axial_deadzone(pad.value(Axis::RightZ)),
                battery: power_info_to_battery(pad.power_info()),
                // Fields this site does not set: the pad touchpad and its
                // gyro/accelerometer, which gilrs does not surface at all -
                // they need SDL or raw HID, and that is 8f-i-a, not something
                // this backend can reach.
                ..Default::default()
            });
        }
    });
}

// Deadzone helpers live in `mod.rs`: every backend needs the same
// treatment and three copies would drift.

#[cfg(test)]
mod tests {
    use super::*;

    /// The `battery` field is a SENTINEL-carrying f32, not an `Option`, so
    /// every "unknown" case has to land on exactly `-1.0`. A backend that
    /// returned `0.0` instead would be indistinguishable from a flat battery.
    #[test]
    fn unknown_power_reports_the_not_reported_sentinel() {
        assert_eq!(power_info_to_battery(PowerInfo::Unknown), -1.0);
    }

    /// A WIRED pad has no cell at all. Reporting it as `1.0` would make
    /// "plugged in" and "fully charged" indistinguishable, and a UI drawing a
    /// battery icon would draw one for a controller that has none. The
    /// field's own docs say wired pads report `-1.0`.
    #[test]
    fn a_wired_pad_reports_no_battery_rather_than_a_full_one() {
        assert_eq!(power_info_to_battery(PowerInfo::Wired), -1.0);
        assert_ne!(power_info_to_battery(PowerInfo::Wired), 1.0);
    }

    #[test]
    fn a_discharging_level_becomes_a_zero_to_one_fraction() {
        assert_eq!(power_info_to_battery(PowerInfo::Discharging(0)), 0.0);
        assert_eq!(power_info_to_battery(PowerInfo::Discharging(50)), 0.5);
        assert_eq!(power_info_to_battery(PowerInfo::Discharging(100)), 1.0);
    }

    /// Charging carries a REAL level, so it must not collapse to the sentinel:
    /// an app dimming a low-battery warning during a charge still needs the
    /// number to decide.
    #[test]
    fn charging_reports_the_level_not_the_sentinel() {
        assert_eq!(power_info_to_battery(PowerInfo::Charging(25)), 0.25);
        assert_ne!(power_info_to_battery(PowerInfo::Charging(25)), -1.0);
    }

    #[test]
    fn charged_is_full() {
        assert_eq!(power_info_to_battery(PowerInfo::Charged), 1.0);
    }

    /// The value comes from a driver, so an out-of-range percent must not
    /// escape the documented `[0, 1]` range that every consumer trusts.
    #[test]
    fn a_bad_driver_percentage_cannot_escape_the_documented_range() {
        assert_eq!(power_info_to_battery(PowerInfo::Discharging(255)), 1.0);
        for info in [
            PowerInfo::Unknown,
            PowerInfo::Wired,
            PowerInfo::Charged,
            PowerInfo::Discharging(200),
            PowerInfo::Charging(200),
        ] {
            let v = power_info_to_battery(info);
            assert!(
                v == -1.0 || (0.0..=1.0).contains(&v),
                "{info:?} produced {v}, outside the sentinel-or-[0,1] contract"
            );
        }
    }
}
