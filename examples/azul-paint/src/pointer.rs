//! One finger, one stroke.
//!
//! A phone's shell keeps the mouse pipe beside the touch pipe: the primary finger moves the cursor and holds
//! the left button, so that an app that only knows the mouse works with a finger. One finger therefore raises
//! `TouchStart` and `MouseDown` (and the moves and the releases after them) for the same gesture, and a canvas
//! that listens to both drew every stroke twice: the QA's one swipe on the canvas printed `AZPAINT_STROKES 2`
//! and Undo needed two presses.
//!
//! [`Gesture`] says which source a gesture belongs to: the one that pressed first. The events of the other source
//! are its echo and are ignored while the owner is active; the owner is forgotten `ECHO_MS` after its last event,
//! so a mouse picked up after a finger (or the other way round) is not shut out. A desktop with a mouse only sees
//! one source and everything is accepted.

/// How long the events of the other source are taken for the echo of the owner's: the owner's last event
/// refreshes it. A whole gesture's echo arrives within a frame or two of the owner's own events.
pub(crate) const ECHO_MS: u64 = 600;

/// Where a pointer event came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Source {
    /// `MouseDown` / `MouseMove` / `MouseUp`: a mouse, a pen, and the primary finger the shell mirrors into the mouse pipe.
    Mouse,
    /// `TouchStart` / `TouchMove` / `TouchEnd`.
    Touch,
}

/// The source that owns the gesture in progress (or the one that had it last), and when it last spoke.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Gesture {
    owner: Option<(Source, u64)>,
}

impl Gesture {
    /// Whether an event of `from` at `now_ms` is the echo of the owner's gesture.
    fn is_echo(&self, from: Source, now_ms: u64) -> bool {
        matches!(
            self.owner,
            Some((owner, last)) if owner != from && now_ms.saturating_sub(last) < ECHO_MS
        )
    }

    /// A press of `from`: `true` when it starts a stroke, `false` for the echo of a gesture the other source owns.
    pub(crate) fn press(&mut self, from: Source, now_ms: u64) -> bool {
        if self.is_echo(from, now_ms) {
            return false;
        }
        self.owner = Some((from, now_ms));
        true
    }

    /// A move of `from`: `true` when it belongs to the pointer (the owner's own, or nobody's yet: a hover).
    pub(crate) fn moves(&mut self, from: Source, now_ms: u64) -> bool {
        if self.is_echo(from, now_ms) {
            return false;
        }
        if matches!(self.owner, Some((owner, _)) if owner == from) {
            self.owner = Some((from, now_ms));
        }
        true
    }

    /// A release of `from`: `true` when it ends the stroke, `false` for the echo of the owner's release. The owner
    /// stays the owner for the echo window, so the other source's release after it is recognised.
    pub(crate) fn release(&mut self, from: Source, now_ms: u64) -> bool {
        if self.is_echo(from, now_ms) {
            return false;
        }
        self.owner = Some((from, now_ms));
        true
    }

    /// The pointer left the canvas: nobody owns anything.
    pub(crate) fn gone(&mut self) {
        self.owner = None;
    }
}

/// Milliseconds on a clock every event of a gesture reads alike (the wall clock: a gesture lasts seconds).
pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}
