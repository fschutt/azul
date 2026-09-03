//! Windows PEN haptics via `SimpleHapticsController` (WinRT).
//!
//! # What this actually adds, which is narrower than the item implied
//!
//! The 9g-i-c note said this "reaches Surface Pen and some gamepads". The
//! gamepad half is already done and must NOT be duplicated here: 9g-i-d routes
//! `HapticTarget::Gamepad` through gilrs, which owns the actuator on Windows
//! too, and a second path would rumble twice for one request.
//!
//! So what is genuinely new is `HapticTarget::Pen` - the Surface Pen's own
//! actuator, which no backend had ever driven. macOS skips it (only Apple
//! Pencil Pro has one and there is no public API), Android has no pen
//! actuator, and this was the remaining platform where a pen can buzz.
//!
//! # The pointer id is the awkward part
//!
//! `PenDevice::GetFromPointerId` is the only route to a pen's controller, and
//! it needs a POINTER ID, which exists only while the pen is being tracked.
//! So the id is remembered from the `WM_POINTER*` stream and the haptic is
//! addressed to whichever pen most recently reported. A request arriving after
//! the pen has left proximity finds a stale id and does nothing, which is the
//! honest outcome - there is no pen to buzz.
//!
//! # Waveforms
//!
//! `KnownSimpleHapticsControllerWaveforms` offers `Click`, `Press`, `Release`,
//! `BuzzContinuous` and `RumbleContinuous`. A pen supports a SUBSET, published
//! through `SupportedFeedback`, so the chosen waveform is looked up in that
//! list rather than assumed - sending an unsupported one fails at runtime.

use azul_core::haptics::{HapticPattern, HapticRequest};

/// The most recent pen pointer id seen by the window procedure.
///
/// `0` is not a valid pointer id, so it doubles as "no pen".
static LAST_PEN_POINTER: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);

/// Record a pen pointer id from the `WM_POINTER*` stream.
pub fn note_pen_pointer(pointer_id: u32) {
    LAST_PEN_POINTER.store(pointer_id, core::sync::atomic::Ordering::Relaxed);
}

/// Play a haptic on the pen, if there is one and it has an actuator.
///
/// Returns whether anything was sent, so the caller can tell "no pen" from
/// "played".
pub fn play_pen(request: &HapticRequest) -> bool {
    use windows::Devices::{Haptics::KnownSimpleHapticsControllerWaveforms, Input::PenDevice};

    let pointer_id = LAST_PEN_POINTER.load(core::sync::atomic::Ordering::Relaxed);
    if pointer_id == 0 {
        return false;
    }

    // Every step is fallible and failure is ORDINARY: a pen that has left
    // proximity, a pen with no actuator, or a Windows build without the
    // interface all land here and all mean "no buzz", not "error".
    let Ok(pen) = PenDevice::GetFromPointerId(pointer_id) else {
        return false;
    };
    let Ok(controller) = pen.SimpleHapticsController() else {
        return false;
    };

    // The waveform this pattern wants, by weight. A pen has no vocabulary of
    // its own beyond click/press/release, so the nineteen collapse onto three.
    let wanted = match request.pattern {
        HapticPattern::KeyPress | HapticPattern::GestureStart => {
            KnownSimpleHapticsControllerWaveforms::Press()
        }
        HapticPattern::KeyRelease | HapticPattern::GestureEnd => {
            KnownSimpleHapticsControllerWaveforms::Release()
        }
        _ => KnownSimpleHapticsControllerWaveforms::Click(),
    };
    let Ok(wanted) = wanted else {
        return false;
    };

    // LOOKED UP, not assumed: a pen supports a SUBSET of the known waveforms
    // and publishes which through `SupportedFeedback`. Sending an unsupported
    // one fails at runtime, so the fallback to whatever the pen does have is
    // what makes this work on more than one pen model.
    let Ok(supported) = controller.SupportedFeedback() else {
        return false;
    };
    let mut chosen = None;
    let mut first = None;
    if let Ok(count) = supported.Size() {
        for i in 0..count {
            let Ok(feedback) = supported.GetAt(i) else {
                continue;
            };
            let Ok(waveform) = feedback.Waveform() else {
                continue;
            };
            if first.is_none() {
                first = Some(feedback.clone());
            }
            if waveform == wanted {
                chosen = Some(feedback);
                break;
            }
        }
    }
    // Falling back to the pen's FIRST supported waveform rather than giving
    // up: a buzz of the wrong texture is closer to the intent than silence,
    // and matches how `HapticPattern::fallback` degrades everywhere else.
    let Some(feedback) = chosen.or(first) else {
        return false;
    };

    // Intensity is honoured only where the pen reports supporting it; the
    // plain call is not equivalent to intensity 1.0 on hardware that does not.
    let intensity = f64::from(request.intensity_clamped());
    let sent = if controller.IsIntensitySupported().unwrap_or(false) {
        controller
            .SendHapticFeedbackWithIntensity(&feedback, intensity)
            .is_ok()
    } else {
        controller.SendHapticFeedback(&feedback).is_ok()
    };
    sent
}
