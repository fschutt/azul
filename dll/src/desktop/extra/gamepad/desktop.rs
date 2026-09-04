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

        // Vendor / product per pad, from the SDL-layout GUID gilrs builds
        // (bytes 4-5 vendor, 8-9 product, little-endian): what pairs a pad
        // with its raw HID twin (8f-i-a-i-b, -c).
        let identities: Vec<PadIdentity> = gilrs
            .gamepads()
            .map(|(gid, pad)| {
                let (guid_vendor, guid_product) = guid_vendor_product(pad.uuid());
                PadIdentity {
                    id: usize::from(gid) as u32,
                    // gilrs's own answer first; the GUID carries the same
                    // numbers for a pad whose backend reports none.
                    vendor: pad.vendor_id().unwrap_or(guid_vendor),
                    product: pad.product_id().unwrap_or(guid_product),
                    serial: pad_serial(&pad),
                }
            })
            .collect();
        {
            let mut ids = PAD_IDENTITIES
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            *ids = identities.clone();
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
            let mut state = GamepadState {
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
                ..Default::default()
            };
            // The pad's touch surface and its gyro/accelerometer, which
            // gilrs does not surface at all, come from its raw HID twin
            // (8f-i-a-i-b) - laid over HERE so the manager sees one writer
            // per slot and an idle pad raises no event.
            let (guid_vendor, guid_product) = guid_vendor_product(pad.uuid());
            let v = pad.vendor_id().unwrap_or(guid_vendor);
            let p = pad.product_id().unwrap_or(guid_product);
            let twins = identities
                .iter()
                .filter(|i| i.vendor == v && i.product == p)
                .count();
            let serial = identities
                .iter()
                .find(|i| i.id == usize::from(gid) as u32)
                .map_or("", |i| i.serial.as_str());
            super::overlay_hid_motion(&mut state, v, p, serial, twins);
            push_gamepad_state(state);
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

// ─── Rumble (9g-i-d) ──────────────────────────────────────────────────

thread_local! {
    /// The effect currently playing on each pad, keyed by the `GamepadId` the
    /// engine uses (`usize::from(gid) as u32`).
    ///
    /// Held because a gilrs `Effect` is a HANDLE: letting it drop removes it
    /// from the force-feedback server's map WITHOUT stopping the motor (see
    /// `Message::HandleDropped` in gilrs's `ff/server.rs`, which only calls
    /// `effects.remove`). So a dropped handle can leave a controller buzzing
    /// with nothing left to stop it - which is exactly the failure this item
    /// warned about, and why every teardown path below calls `stop()`
    /// EXPLICITLY before releasing the handle.
    static EFFECTS: RefCell<Vec<(u32, gilrs::ff::Effect)>> = const { RefCell::new(Vec::new()) };
}

/// Stop and release whatever is playing on `pad`, if anything.
fn stop_effect(pad: u32) {
    EFFECTS.with(|slot| {
        let mut list = slot.borrow_mut();
        if let Some(pos) = list.iter().position(|(id, _)| *id == pad) {
            let (_, effect) = list.remove(pos);
            // Explicit, BEFORE the drop: see the note on EFFECTS.
            let _ = effect.stop();
        }
    });
}

/// Play a rumble on one pad.
///
/// `intensity` is `0.0..=1.0` and `duration_ms` is `0` for the pattern's
/// natural length, matching `HapticRequest`.
pub fn rumble(pad: u32, intensity: f32, duration_ms: u32, strong: bool) {
    use gilrs::ff::{BaseEffect, BaseEffectType, EffectBuilder, Replay, Ticks};

    // Replacing whatever was playing: two overlapping effects on one motor
    // sum in the driver, so a repeated tap would climb to full amplitude and
    // stay there.
    stop_effect(pad);
    if intensity <= 0.0 {
        return;
    }

    GILRS.with(|slot| {
        let mut ctx = slot.borrow_mut();
        let Some(gilrs) = ctx.as_mut() else {
            return;
        };
        // A pad with no actuator must not be handed an effect: gilrs errors,
        // and on some backends the error is only visible as a failed play.
        let Some(gid) = gilrs
            .gamepads()
            .find(|(id, gp)| usize::from(*id) as u32 == pad && gp.is_ff_supported())
            .map(|(id, _)| id)
        else {
            return;
        };

        // gilrs magnitude is a u16 across the FULL range, not a percentage.
        let magnitude = (intensity.clamp(0.0, 1.0) * f32::from(u16::MAX)) as u16;
        // A haptic TAP with no duration is not a rumble the user can feel.
        // The rule and its 150ms figure live in `HapticRequest` so the
        // Android per-controller path answers it identically.
        let ms = azul_core::haptics::rumble_duration_ms(duration_ms);
        let play_for = Ticks::from_ms(ms);

        // STRONG is the low-frequency motor (a heavy thud), WEAK the
        // high-frequency one (a light buzz). Which one a pattern maps to is
        // the caller's decision, not a magnitude split across both: driving
        // both at once is a different, muddier sensation.
        let kind = if strong {
            BaseEffectType::Strong { magnitude }
        } else {
            BaseEffectType::Weak { magnitude }
        };

        let built = EffectBuilder::new()
            .add_effect(BaseEffect {
                kind,
                scheduling: Replay {
                    play_for,
                    // No repeat: `with_delay` is the gap before a REPLAY, and
                    // leaving it at the default would loop the effect for as
                    // long as the handle lives.
                    with_delay: Ticks::from_ms(0),
                    ..Default::default()
                },
                envelope: Default::default(),
            })
            .gamepads(&[gid])
            .finish(gilrs);

        if let Ok(effect) = built {
            if effect.play().is_ok() {
                EFFECTS.with(|s| s.borrow_mut().push((pad, effect)));
            }
        }
    });
}

/// Stop every motor. Called when the app is going away.
///
/// Without this a controller keeps buzzing after the window closes: the OS
/// does not reset an actuator when the process that started it exits on every
/// backend, and the effect handles alone do not stop it.
pub fn stop_all_rumble() {
    EFFECTS.with(|slot| {
        for (_, effect) in slot.borrow_mut().drain(..) {
            let _ = effect.stop();
        }
    });
}

/// What pairs a gilrs pad with its raw HID twin (8f-i-a-i-b, -c).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PadIdentity {
    /// `GamepadId.id`.
    pub id: u32,
    pub vendor: u16,
    pub product: u16,
    /// The pad's serial as the input layer reports it - on Linux evdev's
    /// `uniq`, the SAME string hidraw answers `HIDIOCGRAWUNIQ` with (both
    /// come from `hdev->uniq`), so two identical pads pair exactly. Empty
    /// where the platform's gilrs backend exposes none (macOS, Windows).
    pub serial: String,
}

/// Every pad gilrs currently sees, as of the last poll.
static PAD_IDENTITIES: std::sync::Mutex<Vec<PadIdentity>> = std::sync::Mutex::new(Vec::new());

/// Snapshot of [`PAD_IDENTITIES`].
pub fn pad_identities() -> Vec<PadIdentity> {
    PAD_IDENTITIES
        .lock()
        .map(|v| v.clone())
        .unwrap_or_default()
}

/// The sysfs `uniq` attribute behind an evdev node: `/dev/input/eventN` ->
/// `/sys/class/input/eventN/device/uniq`. `None` for anything that is not
/// an event node.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn sysfs_uniq_path(devpath: &str) -> Option<String> {
    let node = devpath.strip_prefix("/dev/input/")?;
    if !node.starts_with("event") || !node[5..].bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some(format!("/sys/class/input/{node}/device/uniq"))
}

/// The pad's serial (8f-i-a-i-c). On Linux it is evdev's `uniq`, readable
/// through sysfs beside the pad's event node, which gilrs exposes through
/// the `LinuxGamepadExt` extension trait (`devpath()` - a trait method, so
/// it needs the trait in scope, which is what the first attempt lacked).
/// On macOS it is IOKit's `kIOHIDSerialNumberKey`, read by the gilrs-azul
/// fork (`Gamepad::serial`, 0.11.3 / gilrs-core-azul 0.6.9 - 8f-i-a-i-c-ii):
/// the same IOHIDDeviceRef the raw HID layer enumerated, so the two agree on
/// the string. Windows has no gilrs-side serial and answers empty, leaving
/// the unique-vendor/product rule.
#[allow(unused_variables)]
fn pad_serial(pad: &gilrs::Gamepad<'_>) -> String {
    #[cfg(target_os = "linux")]
    {
        use gilrs::LinuxGamepadExt;
        if let Some(path) = pad.devpath().to_str().and_then(sysfs_uniq_path) {
            if let Ok(uniq) = std::fs::read_to_string(path) {
                return uniq.trim().to_owned();
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        if let Some(serial) = pad.serial() {
            return serial.trim().to_owned();
        }
    }
    String::new()
}

/// Vendor and product out of an SDL-layout GUID: bytes 4-5 and 8-9,
/// little-endian. Zero for a pad whose GUID carries none (a virtual pad).
fn guid_vendor_product(guid: [u8; 16]) -> (u16, u16) {
    (
        u16::from_le_bytes([guid[4], guid[5]]),
        u16::from_le_bytes([guid[8], guid[9]]),
    )
}

#[cfg(test)]
mod guid_tests {
    use super::guid_vendor_product;

    /// An SDL GUID for a USB DualSense: bus 0x03, vendor 0x054c, product
    /// 0x0ce6, version 0x0100 - each little-endian at its slot.
    #[test]
    fn vendor_and_product_sit_at_bytes_4_and_8_little_endian() {
        let mut g = [0u8; 16];
        g[0] = 0x03;
        g[4] = 0x4c;
        g[5] = 0x05;
        g[8] = 0xe6;
        g[9] = 0x0c;
        g[12] = 0x00;
        g[13] = 0x01;
        assert_eq!(guid_vendor_product(g), (0x054c, 0x0ce6));
        assert_eq!(guid_vendor_product([0; 16]), (0, 0));
    }

    #[test]
    fn the_uniq_attribute_sits_beside_the_event_node_in_sysfs() {
        assert_eq!(
            super::sysfs_uniq_path("/dev/input/event5").as_deref(),
            Some("/sys/class/input/event5/device/uniq")
        );
        assert_eq!(super::sysfs_uniq_path("/dev/input/js0"), None);
        assert_eq!(super::sysfs_uniq_path("/dev/input/eventX"), None);
        assert_eq!(super::sysfs_uniq_path(""), None);
    }
}
