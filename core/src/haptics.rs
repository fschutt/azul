//! Haptic feedback — the one OUTPUT in the input subsystem.
//!
//! It does not fit the event model, because nothing is being reported: the app
//! is asking the hardware to do something. But it belongs here, because every
//! platform exposes it through its input stack and because the thing being
//! driven is an input device — a trackpad, a controller, a pen.
//!
//! # Why the vocabulary is SEMANTIC and not a waveform
//!
//! Each platform has its own waveform language (Core Haptics patterns, Android
//! `VibrationEffect` compositions, DualSense trigger profiles) and none of them
//! translate. Worse, the same waveform feels different on every actuator: a
//! Taptic Engine, an LRA in a phone, and the ERM motors in a gamepad have
//! different resonant frequencies and rise times, so a millisecond envelope
//! tuned on one is mush on another.
//!
//! So the vocabulary names the INTENT — "a selection changed", "an action was
//! rejected" — and each backend picks the actuator's own best rendering. This
//! is what Apple, Google and Microsoft all recommend for their own APIs, and it
//! is the only way one call site can be correct on a trackpad, a phone and a
//! controller at once.
//!
//! The set is the union of the platform vocabularies rather than their
//! intersection, because the intersection is nearly empty (macOS has THREE
//! patterns) and a caller that wants `TextHandleMove` on Android should not be
//! denied it because macOS would render it as a generic tap. Anything a
//! platform cannot render natively degrades along [`HapticPattern::fallback`]
//! until it reaches something the platform does have.
//!
//! # Platform mapping
//!
//! | pattern | macOS `NSHapticFeedbackPattern` | Android | iOS `UIFeedbackGenerator` |
//! |---|---|---|---|
//! | `Selection` | `Alignment` | `CLOCK_TICK` | `UISelectionFeedbackGenerator` |
//! | `ImpactLight` | `Alignment` | `PRIMITIVE_LOW_TICK` | `.light` |
//! | `ImpactMedium` | `LevelChange` | `PRIMITIVE_TICK` | `.medium` |
//! | `ImpactHeavy` | `Generic` | `PRIMITIVE_CLICK` | `.heavy` |
//! | `ImpactSoft` | `Alignment` | `PRIMITIVE_LOW_TICK` | `.soft` |
//! | `ImpactRigid` | `LevelChange` | `PRIMITIVE_CLICK` | `.rigid` |
//! | `Success` | `LevelChange` | `CONFIRM` | `.success` |
//! | `Warning` | `Generic` | `EFFECT_DOUBLE_CLICK` | `.warning` |
//! | `Error` | `Generic` | `REJECT` | `.error` |
//! | `KeyPress` | `Alignment` | `KEYBOARD_PRESS` | — |
//! | `KeyRelease` | `Alignment` | `KEYBOARD_RELEASE` | — |
//! | `LongPress` | `LevelChange` | `LONG_PRESS` | — |
//! | `ContextClick` | `LevelChange` | `CONTEXT_CLICK` | — |
//! | `TextHandleMove` | `Alignment` | `TEXT_HANDLE_MOVE` | — |
//! | `GestureStart` | `Alignment` | `GESTURE_START` | — |
//! | `GestureEnd` | `Alignment` | `GESTURE_END` | — |
//! | `Rise` | `Alignment` | `PRIMITIVE_QUICK_RISE` | — |
//! | `Fall` | `Alignment` | `PRIMITIVE_QUICK_FALL` | — |
//! | `Spin` | `Alignment` | `PRIMITIVE_SPIN` (API 31+) | — |
//!
//! A dash means the platform has no native equivalent and the backend walks
//! [`HapticPattern::fallback`] to reach one it does.

/// A haptic pattern to play, named by INTENT rather than by waveform.
///
/// See the module docs for the per-platform mapping and for why this names
/// intents instead of envelopes.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum HapticPattern {
    // --- selection ---------------------------------------------------------
    /// A value moved to a new discrete step — a slider hitting a detent, a
    /// picker wheel advancing, a segmented control changing. The lightest
    /// thing in the vocabulary; safe to fire repeatedly during a drag.
    Selection,

    // --- impacts, by weight ------------------------------------------------
    /// A small, light collision — a lightweight object landing.
    ImpactLight,
    /// A moderate collision. The sensible default for "something happened".
    ImpactMedium,
    /// A large, heavy collision — a heavyweight object landing.
    ImpactHeavy,
    /// A dull, compliant thud — the impact of something soft. Distinguished
    /// from `ImpactLight` by texture, not strength.
    ImpactSoft,
    /// A crisp, hard tap — the impact of something inflexible. Distinguished
    /// from `ImpactHeavy` by texture, not strength.
    ImpactRigid,

    // --- notifications -----------------------------------------------------
    /// A task completed successfully.
    Success,
    /// A task completed, but something needs attention.
    Warning,
    /// A task failed, or an action was rejected — an invalid drop target, a
    /// form that will not submit.
    Error,

    // --- discrete UI events ------------------------------------------------
    /// A key went down on an on-screen keyboard.
    KeyPress,
    /// A key came up on an on-screen keyboard.
    KeyRelease,
    /// A long press crossed its threshold and is now committed. Fire this at
    /// the MOMENT the threshold is crossed, not when the finger lifts — the
    /// whole point is to tell the user they can let go.
    LongPress,
    /// A secondary/context action was invoked — a right-click, a long-press
    /// menu opening.
    ContextClick,
    /// A text selection handle moved to a new character position.
    TextHandleMove,

    // --- gesture boundaries ------------------------------------------------
    /// A continuous gesture began and is now tracking.
    GestureStart,
    /// A continuous gesture ended.
    GestureEnd,

    // --- chirps (amplitude/frequency sweeps) -------------------------------
    /// A quick upward sweep — something growing, expanding, being picked up.
    Rise,
    /// A quick downward sweep — something shrinking, collapsing, being
    /// dropped.
    Fall,
    /// A spinning, bidirectional flutter — momentum, a wheel being flicked.
    Spin,
}

impl HapticPattern {
    /// The next-simplest pattern to try when a backend cannot render this one.
    ///
    /// Backends walk this chain rather than silently dropping the request, so
    /// a caller asking for `Spin` on a device whose actuator predates the
    /// primitive still feels *something*. Every chain terminates at
    /// `Selection`, which every haptic device on every platform can render;
    /// `Selection` itself returns `None`, which is the signal to give up.
    ///
    /// This exists because the degradation is a property of the PATTERN, not
    /// of the backend — otherwise all six backends would each invent their own
    /// (differing) fallback and the same call would feel unrelated across
    /// platforms.
    #[must_use]
    pub const fn fallback(self) -> Option<HapticPattern> {
        use HapticPattern::*;
        match self {
            // The terminus: nothing is simpler than a selection tick.
            Selection => None,

            // Impacts collapse toward the middle weight, then to a tick.
            ImpactLight | ImpactSoft => Some(Selection),
            ImpactMedium => Some(ImpactLight),
            ImpactHeavy | ImpactRigid => Some(ImpactMedium),

            // Notifications degrade to an impact of matching weight: a
            // failure should still feel heavier than a success.
            Success => Some(ImpactLight),
            Warning => Some(ImpactMedium),
            Error => Some(ImpactHeavy),

            // Discrete UI events are all light taps underneath.
            KeyPress | KeyRelease | TextHandleMove => Some(Selection),
            LongPress | ContextClick => Some(ImpactMedium),

            // Gesture boundaries are the lightest possible marker.
            GestureStart | GestureEnd => Some(Selection),

            // Chirps have no simple equivalent; a medium impact at least
            // marks the moment.
            Rise | Fall => Some(ImpactLight),
            Spin => Some(ImpactMedium),
        }
    }

    /// Walk [`fallback`](Self::fallback) until `supported` accepts a pattern.
    ///
    /// Backends call this instead of matching on the pattern twice. Returns
    /// `None` if nothing in the chain is supported, which means the request
    /// should be dropped.
    #[must_use]
    pub fn resolve(self, supported: impl Fn(HapticPattern) -> bool) -> Option<HapticPattern> {
        let mut current = Some(self);
        while let Some(p) = current {
            if supported(p) {
                return Some(p);
            }
            current = p.fallback();
        }
        None
    }
}

/// Which device should play the pattern.
///
/// `#[repr(C, u8)]` rather than `#[repr(C)]` because `Gamepad` carries a
/// payload: a data-carrying enum needs an explicit discriminant type for its
/// layout to be defined across the FFI boundary, and the API's FFI checker
/// rejects `repr(C)` on one for exactly that reason.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
pub enum HapticTarget {
    /// Whatever the system considers the default — the trackpad on a laptop,
    /// the body of a phone.
    System,
    /// A specific gamepad, by its `GamepadId`. Rumble rather than a tap: a
    /// controller's actuators are motors, so the light patterns become short
    /// low-amplitude pulses rather than the crisp taps a trackpad gives.
    Gamepad(u32),
    /// The pen currently in proximity. Only Apple Pencil Pro and some Surface
    /// pens have an actuator; a request to any other pen is silently ignored.
    Pen,
}

/// A queued haptic request.
///
/// Queued rather than played synchronously because a callback runs on the
/// layout thread, and the platform APIs that drive actuators want to be called
/// from the event loop — the same reason clipboard writes and menu opens are
/// deferred.
#[derive(Debug, Copy, Clone, PartialEq)]
#[repr(C)]
pub struct HapticRequest {
    pub pattern: HapticPattern,
    pub target: HapticTarget,
    /// Strength scale in `0.0..=1.0`, where `1.0` is the platform's own
    /// default strength for the pattern.
    ///
    /// Portable because three of the four backends take a scale natively:
    /// Android's composition primitives take a float scale, iOS's
    /// `impactOccurred(intensity:)` takes one, and gamepad rumble IS an
    /// amplitude. macOS is the exception — `NSHapticFeedbackPattern` has no
    /// strength axis at all, so the value is ignored there rather than
    /// emulated, because faking it with repeated taps feels like a stutter.
    pub intensity: f32,
    /// How long the effect should last, in milliseconds. `0` means "the
    /// pattern's own natural duration", which is what every tap-style
    /// actuator wants.
    ///
    /// Only meaningful for continuous actuators — gamepad rumble motors,
    /// which run until told to stop. Tap-style actuators ignore it.
    pub duration_ms: u32,
}

impl HapticRequest {
    /// A request at full strength and natural duration — the common case.
    #[must_use]
    pub const fn new(pattern: HapticPattern, target: HapticTarget) -> Self {
        Self { pattern, target, intensity: 1.0, duration_ms: 0 }
    }

    /// Clamp the scale into the range every backend assumes.
    ///
    /// Callers compute intensity from things like drag velocity, so an
    /// out-of-range or NaN value is expected rather than exceptional; a NaN
    /// reaching Android's `addPrimitive` throws.
    #[must_use]
    pub fn intensity_clamped(&self) -> f32 {
        if self.intensity.is_nan() {
            1.0
        } else {
            self.intensity.clamp(0.0, 1.0)
        }
    }
}

/// Collects haptic requests for the platform backend to play.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HapticManager {
    pending: alloc::vec::Vec<HapticRequest>,
}

impl HapticManager {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Queue a pattern at full strength.
    pub fn play(&mut self, pattern: HapticPattern, target: HapticTarget) {
        self.play_request(HapticRequest::new(pattern, target));
    }

    /// Queue a fully-specified request.
    pub fn play_request(&mut self, request: HapticRequest) {
        // Coalesced: a callback that fires per-frame during a drag would
        // otherwise queue a tick per frame and the device would buzz
        // continuously instead of ticking once.
        if self
            .pending
            .last()
            .is_some_and(|r| r.pattern == request.pattern && r.target == request.target)
        {
            return;
        }
        self.pending.push(request);
    }

    /// Drain the queue — called by the shell each pass.
    pub fn take_pending(&mut self) -> alloc::vec::Vec<HapticRequest> {
        core::mem::take(&mut self.pending)
    }

    /// Whether anything is queued, so a backend can skip acquiring a native
    /// performer (which on macOS means an Objective-C round trip) when there
    /// is nothing to play. Called once per pass on every window.
    #[must_use]
    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every chain has to TERMINATE, or a backend walking it hangs. A cycle
    /// here would be an infinite loop inside the event loop, on the device
    /// only, which is the worst possible place to discover it.
    #[test]
    fn every_fallback_chain_terminates_at_selection() {
        const ALL: &[HapticPattern] = &[
            HapticPattern::Selection,
            HapticPattern::ImpactLight,
            HapticPattern::ImpactMedium,
            HapticPattern::ImpactHeavy,
            HapticPattern::ImpactSoft,
            HapticPattern::ImpactRigid,
            HapticPattern::Success,
            HapticPattern::Warning,
            HapticPattern::Error,
            HapticPattern::KeyPress,
            HapticPattern::KeyRelease,
            HapticPattern::LongPress,
            HapticPattern::ContextClick,
            HapticPattern::TextHandleMove,
            HapticPattern::GestureStart,
            HapticPattern::GestureEnd,
            HapticPattern::Rise,
            HapticPattern::Fall,
            HapticPattern::Spin,
        ];

        for start in ALL {
            let mut seen = alloc::vec::Vec::new();
            let mut current = Some(*start);
            while let Some(p) = current {
                assert!(
                    !seen.contains(&p),
                    "fallback chain from {start:?} cycles at {p:?} (seen {seen:?})"
                );
                seen.push(p);
                assert!(
                    seen.len() <= ALL.len(),
                    "fallback chain from {start:?} is longer than the vocabulary"
                );
                current = p.fallback();
            }
            assert_eq!(
                seen.last(),
                Some(&HapticPattern::Selection),
                "chain from {start:?} ended at {:?}, not Selection — a backend that only \
                 supports Selection would drop it",
                seen.last()
            );
        }
    }

    /// The point of the chain: a backend supporting only the one universal
    /// pattern still renders every request in the vocabulary.
    #[test]
    fn a_selection_only_backend_resolves_everything() {
        for p in [
            HapticPattern::Spin,
            HapticPattern::Error,
            HapticPattern::ImpactHeavy,
            HapticPattern::TextHandleMove,
        ] {
            assert_eq!(
                p.resolve(|c| c == HapticPattern::Selection),
                Some(HapticPattern::Selection),
                "{p:?} did not degrade to Selection"
            );
        }
    }

    /// A backend that supports the pattern natively must NOT degrade it.
    #[test]
    fn resolve_prefers_the_exact_pattern() {
        assert_eq!(
            HapticPattern::Spin.resolve(|_| true),
            Some(HapticPattern::Spin)
        );
    }

    /// A backend with no actuator at all gets `None` rather than looping.
    #[test]
    fn resolve_gives_up_when_nothing_is_supported() {
        assert_eq!(HapticPattern::Spin.resolve(|_| false), None);
    }

    /// The dedup exists so a per-frame drag callback ticks once, not 60×.
    #[test]
    fn adjacent_identical_requests_coalesce() {
        let mut m = HapticManager::new();
        for _ in 0..60 {
            m.play(HapticPattern::Selection, HapticTarget::System);
        }
        assert_eq!(m.take_pending().len(), 1);
    }

    /// ...but a DIFFERENT pattern in between must survive: coalescing is
    /// adjacent-only, not a set.
    #[test]
    fn a_different_pattern_breaks_the_coalescing_run() {
        let mut m = HapticManager::new();
        m.play(HapticPattern::Selection, HapticTarget::System);
        m.play(HapticPattern::Error, HapticTarget::System);
        m.play(HapticPattern::Selection, HapticTarget::System);
        assert_eq!(m.take_pending().len(), 3);
    }

    /// The same pattern on two different devices is two different requests.
    #[test]
    fn the_target_is_part_of_the_coalescing_key() {
        let mut m = HapticManager::new();
        m.play(HapticPattern::Selection, HapticTarget::System);
        m.play(HapticPattern::Selection, HapticTarget::Gamepad(0));
        assert_eq!(m.take_pending().len(), 2);
    }

    /// A NaN intensity reaching Android's `addPrimitive` throws, and callers
    /// derive intensity from velocities that can divide by zero.
    #[test]
    fn intensity_is_clamped_and_nan_becomes_full_strength() {
        let mk = |i: f32| HapticRequest {
            intensity: i,
            ..HapticRequest::new(HapticPattern::Selection, HapticTarget::System)
        };
        assert_eq!(mk(f32::NAN).intensity_clamped(), 1.0);
        assert_eq!(mk(-3.0).intensity_clamped(), 0.0);
        assert_eq!(mk(9.0).intensity_clamped(), 1.0);
        assert_eq!(mk(0.25).intensity_clamped(), 0.25);
    }

    /// Draining must actually empty the queue, or the coalescing key (the
    /// LAST entry) never changes and every later request is swallowed.
    #[test]
    fn take_pending_empties_the_queue() {
        let mut m = HapticManager::new();
        m.play(HapticPattern::Error, HapticTarget::System);
        assert!(m.has_pending());
        assert_eq!(m.take_pending().len(), 1);
        assert!(!m.has_pending());
        assert!(m.take_pending().is_empty());
    }
}
