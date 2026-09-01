//! Haptic feedback — the one OUTPUT in the input subsystem.
//!
//! It does not fit the event model, because nothing is being reported: the app
//! is asking the hardware to do something. But it belongs here, because every
//! platform exposes it through its input stack and because the thing being
//! driven is an input device — a trackpad, a controller, a pen.
//!
//! The vocabulary is deliberately small. Each platform has its own waveform
//! language (Core Haptics patterns, Android `VibrationEffect` compositions,
//! DualSense trigger profiles) and none of them translate, so exposing a
//! lowest common denominator that works everywhere is more useful than a rich
//! API that only works on one. These four are the ones every platform has a
//! direct equivalent for.

/// A haptic pattern to play.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum HapticPattern {
    /// A single light tick — a value snapping to a detent, a toggle flipping.
    /// macOS `NSHapticFeedbackPattern::Alignment`, Android `CLOCK_TICK`.
    Tick,
    /// A distinct click — a button committing.
    /// macOS `LevelChange`, Android `EFFECT_CLICK`.
    Click,
    /// A heavier thud — a drag landing, a snap into place.
    /// macOS `Generic`, Android `EFFECT_HEAVY_CLICK`.
    Thud,
    /// Two quick pulses — a rejected action, an invalid drop target.
    /// Composed from the above where a platform has no native equivalent.
    Warning,
}

/// Which device should play the pattern.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum HapticTarget {
    /// Whatever the system considers the default — the trackpad on a laptop,
    /// the body of a phone.
    System,
    /// A specific gamepad, by its `GamepadId`. Rumble rather than a tap: a
    /// controller's actuators are motors, so `Tick` and `Click` become short
    /// low-amplitude pulses rather than the crisp taps a trackpad gives.
    Gamepad(u32),
    /// The pen currently in proximity. Only Apple Pencil Pro has an actuator;
    /// a request to any other pen is silently ignored.
    Pen,
}

/// A queued haptic request.
///
/// Queued rather than played synchronously because a callback runs on the
/// layout thread, and the platform APIs that drive actuators want to be called
/// from the event loop — the same reason clipboard writes and menu opens are
/// deferred.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct HapticRequest {
    pub pattern: HapticPattern,
    pub target: HapticTarget,
}

/// Collects haptic requests for the platform backend to play.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HapticManager {
    pending: alloc::vec::Vec<HapticRequest>,
}

impl HapticManager {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Queue a pattern.
    pub fn play(&mut self, pattern: HapticPattern, target: HapticTarget) {
        // Coalesced: a callback that fires per-frame during a drag would
        // otherwise queue a tick per frame and the device would buzz
        // continuously instead of ticking once.
        if self.pending.last().is_some_and(|r| r.pattern == pattern && r.target == target) {
            return;
        }
        self.pending.push(HapticRequest { pattern, target });
    }

    /// Drain the queue — called by the shell each pass.
    pub fn take_pending(&mut self) -> alloc::vec::Vec<HapticRequest> {
        core::mem::take(&mut self.pending)
    }
}
