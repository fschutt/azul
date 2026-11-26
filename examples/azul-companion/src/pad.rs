//! The touch pad's gestures, as a state machine that knows nothing of azul: pointer down / move / up and the pinch of
//! the shell go in, what the phone sends to the Mac comes out. A drag moves the surface (by default the content follows
//! the finger and the viewport goes the other way: the sign is `viewport::finger_to_pan`'s, said once there), a pinch
//! zooms around the pen, two quick taps fit the width, a tap on the strip along the edge turns the page.
//!
//! A drag does not start until the finger has moved a few pixels (the slop): a tap, a double tap and the jitter of a
//! resting finger send nothing to the Mac. Once it has started, every update carries the CUMULATIVE pan and zoom of
//! the gesture (see `azul_review_link::viewport`), so a dropped update loses nothing and the same numbers drive the
//! rectangle the phone draws at once.

use azul_review_link::frame::Phase;
use azul_review_link::link::Millis;
use azul_review_link::viewport::{finger_to_pan, PanDirection};

/// How far a finger must move before it is a drag, in logical pixels.
pub const SLOP: f32 = 6.0;
/// Two taps this close in time are a double tap.
pub const DOUBLE_TAP_MS: u64 = 300;
/// Two taps this close in space (logical pixels) are a double tap.
pub const DOUBLE_TAP_DISTANCE: f32 = 40.0;
/// A swipe on the page strip must be this long to turn a page.
pub const STRIP_SWIPE: f32 = 36.0;

/// What the pad needs to turn pixels into the viewport's movement.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PadScale {
    /// Pixels a unit of the surface takes on the pad (`padscene::px_per_unit`): the sheets under the finger move by the
    /// finger's own number of pixels.
    pub px_per_unit: f32,
    /// Which way the viewport goes when the finger moves (the Mac's setting; see `viewport::finger_to_pan`).
    pub direction: PanDirection,
}

/// What the pad asks of the Mac.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PadOut {
    /// A drag and / or a pinch, cumulative since the gesture `id` began.
    Gesture { id: u32, phase: Phase, pan: (f32, f32), zoom: f32 },
    Fit,
    Reset,
}

/// A gesture in progress, for the rectangle the phone draws before the Mac answers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Live {
    pub id: u32,
    pub pan: (f32, f32),
    pub zoom: f32,
}

/// The pad's memory between events.
#[derive(Debug, Clone, Default)]
pub struct PadState {
    next_id: u32,
    pressed: Option<Pressed>,
    gesture: Option<Active>,
    pinching: bool,
    last_tap: Option<(Millis, f32, f32)>,
}

#[derive(Debug, Clone, Copy)]
struct Pressed {
    start: (f32, f32),
}

#[derive(Debug, Clone, Copy)]
struct Active {
    id: u32,
    /// Where the finger was at the last move (the drag adds up what it moves from here).
    last: (f32, f32),
    pan: (f32, f32),
    zoom: f32,
    zoom_base: f32,
    /// A pinch ended while the finger stayed down: the next move starts from where it is, so the pan does not jump.
    rebase: bool,
}

fn gesture(a: &Active, phase: Phase) -> PadOut {
    PadOut::Gesture { id: a.id, phase, pan: a.pan, zoom: a.zoom }
}

impl PadState {
    /// A pad with nothing going on.
    #[must_use]
    pub fn new() -> Self {
        PadState::default()
    }

    fn begin(&mut self, at: (f32, f32)) -> Active {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        Active { id, last: at, pan: (0.0, 0.0), zoom: 1.0, zoom_base: 1.0, rebase: false }
    }

    /// The finger went down at (`x`, `y`) in the pad's pixels.
    pub fn pointer_down(&mut self, now: Millis, x: f32, y: f32) -> Vec<PadOut> {
        let _ = now;
        if x.is_finite() && y.is_finite() {
            self.pressed = Some(Pressed { start: (x, y) });
        }
        Vec::new()
    }

    /// The finger moved to (`x`, `y`).
    pub fn pointer_move(&mut self, now: Millis, x: f32, y: f32, scale: &PadScale) -> Vec<PadOut> {
        let _ = now;
        let Some(pressed) = self.pressed else {
            return Vec::new();
        };
        if !x.is_finite() || !y.is_finite() || self.pinching {
            return Vec::new();
        }
        match self.gesture {
            None => {
                if (x - pressed.start.0).hypot(y - pressed.start.1) < SLOP {
                    return Vec::new();
                }
                let active = self.begin((x, y));
                self.gesture = Some(active);
                vec![gesture(&active, Phase::Begin), gesture(&active, Phase::Update)]
            }
            Some(mut active) => {
                if active.rebase {
                    active.rebase = false;
                } else {
                    let (dx, dy) = finger_to_pan((x - active.last.0, y - active.last.1), scale.px_per_unit, scale.direction);
                    active.pan = (active.pan.0 + dx, active.pan.1 + dy);
                }
                active.last = (x, y);
                self.gesture = Some(active);
                vec![gesture(&active, Phase::Update)]
            }
        }
    }

    /// The finger lifted at (`x`, `y`).
    pub fn pointer_up(&mut self, now: Millis, x: f32, y: f32) -> Vec<PadOut> {
        if self.pressed.take().is_none() {
            return Vec::new();
        }
        if let Some(active) = self.gesture.take() {
            // the finger lifted: the gesture is over, whether or not a pinch said so (its end comes late and is ignored)
            self.pinching = false;
            return vec![gesture(&active, Phase::End)];
        }
        if !x.is_finite() || !y.is_finite() {
            return Vec::new();
        }
        if let Some((t0, x0, y0)) = self.last_tap {
            if now >= t0 && now - t0 <= DOUBLE_TAP_MS && (x - x0).hypot(y - y0) <= DOUBLE_TAP_DISTANCE {
                self.last_tap = None;
                return vec![PadOut::Fit];
            }
        }
        self.last_tap = Some((now, x, y));
        Vec::new()
    }

    /// A pinch update from the shell: `scale` is the factor since the pinch began, `began` and `ended` its first and
    /// last update.
    pub fn pinch(&mut self, now: Millis, scale: f32, began: bool, ended: bool) -> Vec<PadOut> {
        let _ = now;
        let sane = scale.is_finite() && scale > 0.0;
        if (!sane && (!ended || !self.pinching)) || (ended && self.gesture.is_none()) {
            return Vec::new();
        }
        let mut out = Vec::new();
        let mut active = match self.gesture {
            Some(active) => active,
            None => {
                let active = self.begin((0.0, 0.0));
                out.push(gesture(&active, Phase::Begin));
                active
            }
        };
        if (began || !self.pinching) && sane {
            // a pinch starts from the zoom the gesture has so far
            active.zoom_base = active.zoom;
            self.pinching = true;
        }
        if sane {
            active.zoom = active.zoom_base * scale;
        }
        out.push(gesture(&active, Phase::Update));
        if ended {
            self.pinching = false;
            active.zoom_base = active.zoom;
            active.rebase = true;
            if self.pressed.is_none() {
                out.push(gesture(&active, Phase::End));
                self.gesture = None;
                return out;
            }
        }
        self.gesture = Some(active);
        out
    }

    /// A two-finger tap (or a right click in the simulator): back to the default view.
    pub fn secondary_tap(&mut self) -> Vec<PadOut> {
        vec![PadOut::Reset]
    }

    /// The gesture is cancelled (the pointer left the pad, the system took the touch): it ends where it is.
    pub fn cancel(&mut self) -> Vec<PadOut> {
        self.pressed = None;
        self.pinching = false;
        match self.gesture.take() {
            Some(active) => vec![gesture(&active, Phase::End)],
            None => Vec::new(),
        }
    }

    /// The gesture in progress (`None`: none, or a finger that has not moved enough to be a drag yet).
    #[must_use]
    pub fn live(&self) -> Option<Live> {
        self.gesture.map(|a| Live { id: a.id, pan: a.pan, zoom: a.zoom })
    }
}

/// What a touch on the page strip turns: a swipe of `STRIP_SWIPE` or more pixels (up: the next page, down: the previous
/// one), or a tap (the lower half: next, the upper half: previous). `None` for a move that is neither.
#[must_use]
pub fn strip_step(start_y: f32, end_y: f32, strip_h: f32) -> Option<i8> {
    if !(start_y.is_finite() && end_y.is_finite() && strip_h.is_finite() && strip_h > 0.0) {
        return None;
    }
    let dy = end_y - start_y;
    if dy.abs() >= STRIP_SWIPE {
        return Some(if dy < 0.0 { 1 } else { -1 });
    }
    if dy.abs() <= SLOP {
        return Some(if start_y >= strip_h / 2.0 { 1 } else { -1 });
    }
    None
}

#[cfg(test)]
#[path = "pad_test.rs"]
mod pad_test;
