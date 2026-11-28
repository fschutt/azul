//! The review session: the ONE place that says whether AzReview may listen. Pure: events and a clock go in, the next state and what is
//! to be done with the microphone come out; no window, no microphone, no timer.
//!
//! NOTHING listens until the person starts a session. The window starts `Idle`; hovering, moving the pointer, the pen coming near, the
//! window gaining or losing the focus, a click or a key only ever annotate a session that is already `Recording`: they never start
//! one, never open the microphone and never change the state. Only the explicit events do: `Start`, `Pause`, `Resume`, `Stop`; the
//! window closing and the app quitting end a session, and a microphone that is lost pauses it.
//!
//! ```text
//!   Idle ──Start──▶ Recording ◀──Resume── Paused
//!                    │   ▲ └──Pause──▶     │
//!                    │   └─ (DeviceLost: Recording ─▶ Paused)
//!   Stopped ─Start─▶ │
//!                    └──Stop / WindowClose / AppQuit──▶ Stopped   (also from Paused)
//! ```
//!
//! The table in `step` is the whole policy; `review_session_test.rs` feeds every event to every state.

/// Where the session stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// Nothing was started: nothing listens. How the window starts.
    Idle,
    /// A session runs: the pen's rest may start a dictation and the microphone may be kept ready.
    Recording,
    /// A session was started and is held: nothing listens until it is resumed.
    Paused,
    /// A session ended: nothing listens; a new one needs a new start.
    Stopped,
}

impl State {
    /// The name a script reads.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Recording => "recording",
            Self::Paused => "paused",
            Self::Stopped => "stopped",
        }
    }

    /// Whether the pen's rest and the pen's nearness may be heard: only while recording.
    #[must_use]
    pub const fn listens(self) -> bool {
        matches!(self, Self::Recording)
    }
}

/// What happens, as far as a session cares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    // ---- the explicit ones: the only events that change the state of a session
    /// The person pressed Start (the key, the shortcut).
    Start,
    /// The person pressed Pause.
    Pause,
    /// The person pressed Resume.
    Resume,
    /// The person pressed Stop.
    Stop,
    // ---- the window and the app going away
    /// The window is closing.
    WindowClose,
    /// The app is quitting (azul asks every window to close first, which is `WindowClose`; this is for a host that quits without one).
    AppQuit,
    // ---- the device
    /// The microphone is gone (unplugged, taken by the system, refused).
    DeviceLost,
    // ---- everything else the person does: it annotates a session that runs, it starts nothing
    /// The pointer is over a page.
    Hover,
    /// The pointer moved.
    PointerMove,
    /// The pen came near a page, or into the tablet's range.
    PenNear,
    /// The pointer entered the window or a page.
    PointerEnter,
    /// The pointer left the window or a page.
    PointerLeave,
    /// The window came to the front.
    FocusGained,
    /// The window went behind another.
    FocusLost,
    /// A click or a press of the pen.
    Click,
    /// A key.
    Key,
    /// The pen rested on the code (the dwell).
    PenRest,
}

impl Event {
    /// Every event, for the tests that feed all of them to every state.
    pub const ALL: [Event; 17] = [
        Event::Start,
        Event::Pause,
        Event::Resume,
        Event::Stop,
        Event::WindowClose,
        Event::AppQuit,
        Event::DeviceLost,
        Event::Hover,
        Event::PointerMove,
        Event::PenNear,
        Event::PointerEnter,
        Event::PointerLeave,
        Event::FocusGained,
        Event::FocusLost,
        Event::Click,
        Event::Key,
        Event::PenRest,
    ];

    /// Whether this event is only an annotation: it never changes a session.
    #[must_use]
    pub const fn is_annotation(self) -> bool {
        matches!(
            self,
            Self::Hover
                | Self::PointerMove
                | Self::PenNear
                | Self::PointerEnter
                | Self::PointerLeave
                | Self::FocusGained
                | Self::FocusLost
                | Self::Click
                | Self::Key
                | Self::PenRest
        )
    }
}

/// What the microphone is to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capture {
    /// Nothing: it stays as it is.
    Unchanged,
    /// Open it now (the session starts or resumes: the person asked).
    Open,
    /// Close it now, and end a dictation that listens.
    Close,
}

/// What an event makes of a state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Step {
    pub state: State,
    pub capture: Capture,
    /// The event may be given to what listens (the dwell, the standby): only in `Recording`, and only an annotation.
    pub annotates: bool,
}

/// The whole policy: what `event` does to `state`.
#[must_use]
pub const fn step(state: State, event: Event) -> Step {
    use Capture::{Close, Open, Unchanged};
    use State::{Idle, Paused, Recording, Stopped};
    let (next, capture) = match (state, event) {
        // a session starts only by the person's Start, from nothing or from an ended one
        (Idle | Stopped, Event::Start) => (Recording, Open),
        (Paused, Event::Resume) => (Recording, Open),
        (Recording, Event::Pause) => (Paused, Close),
        (Recording | Paused, Event::Stop) => (Stopped, Close),
        // the window and the app going away end a session that exists, and close the microphone
        (Recording | Paused, Event::WindowClose | Event::AppQuit) => (Stopped, Close),
        // a microphone that is lost holds a session that runs: it cannot hear, and it does not start again by itself
        (Recording, Event::DeviceLost) => (Paused, Close),
        // everything else changes nothing: a start of a running session, a pause of a held one, a hover...
        (s, _) => (s, Unchanged),
    };
    Step { state: next, capture, annotates: event.is_annotation() && matches!(state, Recording) }
}

/// A session with its clock: how long it has recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Session {
    state: State,
    /// When the running stretch began, ms of the caller's clock (`None` unless recording).
    since_ms: Option<u64>,
    /// What the stretches before it came to, ms.
    banked_ms: u64,
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

impl Session {
    /// A window's session: idle, nothing recorded.
    #[must_use]
    pub const fn new() -> Self {
        Self { state: State::Idle, since_ms: None, banked_ms: 0 }
    }

    /// A session that records since `now_ms`, for the tests of what listens only while it does.
    #[cfg(test)]
    #[must_use]
    pub const fn recording_for_test(now_ms: u64) -> Self {
        Self { state: State::Recording, since_ms: Some(now_ms), banked_ms: 0 }
    }

    #[must_use]
    pub const fn state(&self) -> State {
        self.state
    }

    /// Whether a session runs now (the pen's rest and nearness are heard).
    #[must_use]
    pub const fn listens(&self) -> bool {
        self.state.listens()
    }

    /// What the event does at `now_ms`: the state and the clock take it, the step is returned.
    pub fn apply(&mut self, event: Event, now_ms: u64) -> Step {
        let step = step(self.state, event);
        if step.state != self.state {
            match (self.state, step.state) {
                // a new session starts its clock from nothing
                (State::Idle | State::Stopped, State::Recording) => {
                    self.banked_ms = 0;
                    self.since_ms = Some(now_ms);
                }
                (State::Paused, State::Recording) => self.since_ms = Some(now_ms),
                (State::Recording, State::Paused | State::Stopped) => {
                    self.banked_ms = self.banked_ms.saturating_add(self.running_ms(now_ms));
                    self.since_ms = None;
                }
                (State::Paused, State::Stopped) => self.since_ms = None,
                _ => {}
            }
            self.state = step.state;
        }
        step
    }

    fn running_ms(&self, now_ms: u64) -> u64 {
        self.since_ms.map_or(0, |since| now_ms.saturating_sub(since))
    }

    /// How long this session has recorded at `now_ms` (what the stretches that are over came to, and the one that runs).
    #[must_use]
    pub fn elapsed_ms(&self, now_ms: u64) -> u64 {
        self.banked_ms.saturating_add(self.running_ms(now_ms))
    }

    /// The event the start-stop key means in this state: Start when nothing runs, Stop when a session does (running or held).
    #[must_use]
    pub const fn toggle_event(&self) -> Event {
        match self.state {
            State::Recording | State::Paused => Event::Stop,
            State::Idle | State::Stopped => Event::Start,
        }
    }

    /// The event the pause key means in this state: Pause while recording, Resume while held, nothing otherwise.
    #[must_use]
    pub const fn pause_event(&self) -> Option<Event> {
        match self.state {
            State::Recording => Some(Event::Pause),
            State::Paused => Some(Event::Resume),
            State::Idle | State::Stopped => None,
        }
    }
}

/// A length as the key shows it: `m:ss`, or `h:mm:ss` from an hour on.
#[must_use]
pub fn length_text(ms: u64) -> String {
    let seconds = ms / 1_000;
    let (hours, minutes, seconds) = (seconds / 3_600, (seconds / 60) % 60, seconds % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

#[cfg(test)]
#[path = "review_session_test.rs"]
mod review_session_test;
