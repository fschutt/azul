//! The touch slop: when a finger that went down is a tap and when it is a pan.
//!
//! The shell turned every touch into the mouse pipe (a press, moves, a release) and ran the one-finger pan beside it:
//! ANY move above a hundredth of a pixel scrolled at once and moved the pointer. A real finger jitters by a few
//! pixels between the touch and the lift, so a plain tap scrolled the page by the jitter, and the click was LOST
//! whenever the node under the finger changed (`adb shell input tap` sends no move, so an emulator never showed it; the
//! companion agent found it by reading the code).
//!
//! The rule every touch toolkit has (Android's `ViewConfiguration.getScaledTouchSlop`, `ScrollView`, `GestureDetector`):
//!
//! * a finger that went down is a TAP CANDIDATE: until it has moved more than the slop from where it LANDED it scrolls
//!   nothing and the pointer stays where it landed, so a jitter is a click at the landing point (and a long press stays a
//!   long press);
//! * the move that crosses the slop makes it a PAN for good: the first pan delta is the displacement beyond the slop (the
//!   page does not jump by the slop), a pan that comes back to where it started is still a pan, and its lift is no click;
//! * a mouse, a stylus and an eraser have no slop: they move the pointer at once, as before.
//!
//! The slop is 8 dp (`ViewConfiguration.TOUCH_SLOP`). The engine's logical pixel IS the Android dp (the frame factor
//! `dpi / 96` is the density scale `density / 160`: `android_core::metrics`), so positions in logical pixels are compared
//! with [`TOUCH_SLOP_DP`] as they are, and the density scale only appears where a physical size is wanted
//! ([`slop_px`]). Positions are plain `(x, y)` pairs: nothing in here reads the engine.

/// The distance a finger may wander before it is a pan, in dp (= the engine's logical pixels).
pub const TOUCH_SLOP_DP: f32 = 8.0;

/// The slop in physical pixels at a density scale (`density / 160`, `metrics::frame_factor`): 22 at 2.75.
#[must_use]
pub fn slop_px(scale: f32) -> f32 {
    TOUCH_SLOP_DP * scale
}

/// What a move of the contact is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Move {
    /// Still inside the slop: no pan, and the pointer stays where the contact landed.
    Hold,
    /// The move that crossed the slop: the pan starts, with this first delta (the displacement beyond the slop).
    Start {
        /// The first pan delta, logical pixels.
        delta: (f32, f32),
    },
    /// A pan under way, or a contact the slop does not apply to: the move is the pointer's and the pan's as it always was.
    Follow,
}

/// What the lift of the contact is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Lift {
    /// A tap, or a contact the slop does not apply to: the release is a click. `at` is where the finger LANDED (the click
    /// is there, whatever the last position was), `None` for a contact that is not a tap candidate (the pointer is where
    /// it is).
    Click {
        /// The landing position of a tap candidate.
        at: Option<(f32, f32)>,
    },
    /// A pan: the release ends a gesture, it clicks nothing.
    Pan,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
enum Contact {
    /// No contact, or one the slop does not apply to.
    #[default]
    Free,
    /// Down and not yet past the slop.
    Candidate {
        down: (f32, f32),
        slop: f32,
    },
    /// Past the slop, for good.
    Pan,
}

/// The slop state of the primary contact.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TapSlop {
    contact: Contact,
}

impl TapSlop {
    /// A contact went down at `pos`. `applies` is false for a mouse, a stylus and an eraser. `slop` is the distance in
    /// logical pixels ([`TOUCH_SLOP_DP`]; the platform's own, read from the `ViewConfiguration`, would be passed here); a
    /// value that makes no sense (zero, negative, NaN) is the default.
    pub fn down(&mut self, applies: bool, pos: (f32, f32), slop: f32) {
        self.contact = if applies {
            Contact::Candidate {
                down: pos,
                slop: if slop > 0.0 { slop } else { TOUCH_SLOP_DP },
            }
        } else {
            Contact::Free
        };
    }

    /// The contact moved to `pos`.
    pub fn moved(&mut self, pos: (f32, f32)) -> Move {
        let Contact::Candidate { down, slop } = self.contact else {
            return Move::Follow;
        };
        let (dx, dy) = (pos.0 - down.0, pos.1 - down.1);
        let distance = (dx * dx + dy * dy).sqrt();
        // `>`, as Android's `Math.abs(delta) > mTouchSlop`: exactly the slop is still a tap
        if distance.is_nan() || distance <= slop {
            return Move::Hold;
        }
        self.contact = Contact::Pan;
        let beyond = (distance - slop) / distance;
        Move::Start {
            delta: (dx * beyond, dy * beyond),
        }
    }

    /// The contact lifted: what its release is. The state is reset.
    pub fn up(&mut self) -> Lift {
        match core::mem::take(&mut self.contact) {
            Contact::Candidate { down, .. } => Lift::Click { at: Some(down) },
            Contact::Free => Lift::Click { at: None },
            Contact::Pan => Lift::Pan,
        }
    }

    /// The system took the gesture away: nothing is left of it.
    pub fn cancel(&mut self) {
        self.contact = Contact::Free;
    }
}

#[cfg(test)]
#[path = "slop_test.rs"]
mod tests;
