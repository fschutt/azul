//! A tab's view beyond the engine's display offset: the slide by pixels,
//! where the view was when it last drew, the lines that came in below it.
//!
//! The engine (alacritty_terminal) scrolls in whole lines: its display
//! offset is how many lines the view is up from the output, and while it is
//! not 0 every line of output that comes in raises it by one - the content
//! in view stays put. azul's TerminalView scrolls by pixels: it shows the
//! rows of the offset slid up by a fraction of a line
//! (`TerminalScreen::scroll_fraction`). That fraction is the tab's, kept
//! here beside the engine's offset, so a tab switched to shows its own.
//!
//! The view works a scroll out from the screen it showed last, and a view
//! scrolled up is not drawn on every chunk of output - by the time its event
//! comes the engine's offset may be lines higher. So the event is applied as
//! the MOVE it is from what the view showed ([`ViewScroll::shown`]), onto
//! where the view is now; a scroll to the output follows it, wherever that
//! is.

/// Within this of a whole line a position is the whole line (no slide of
/// float dust).
const SNAP_LINES: f64 = 1e-3;

/// `up` lines up from the output as the engine's display offset and the
/// view's slide: the offset is `up` rounded UP - any position off the output
/// is offset 1 or more, where the engine keeps what is in view while output
/// comes in - the slide the rest, a fraction of a line; within `history`.
#[must_use]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)] // clamped to 0..=history
pub fn split(up: f64, history: usize) -> (usize, f32) {
    if !up.is_finite() {
        return (0, 0.0);
    }
    let up = up.clamp(0.0, history as f64);
    let mut lines = up.ceil();
    let mut fraction = lines - up;
    if fraction < SNAP_LINES {
        fraction = 0.0;
    } else if fraction > 1.0 - SNAP_LINES {
        lines -= 1.0;
        fraction = 0.0;
    }
    (lines as usize, fraction as f32)
}

/// A tab's view beyond the engine's display offset.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ViewScroll {
    /// The rows of the display offset are slid up by this fraction of a line
    /// (0 at the output).
    pub fraction: f32,
    /// Lines up from the output the view showed last (the data callback's
    /// answer): what its scroll events are worked out from.
    pub shown: f64,
    /// The display offset as last seen.
    pub seen: usize,
    /// Lines of output that came in below the view since it left the output.
    pub new_lines: u32,
}

impl ViewScroll {
    /// Lines up from the output at display offset `offset`, the slide
    /// applied.
    #[must_use]
    #[allow(clippy::cast_precision_loss)] // line counts far below 2^52
    pub fn up(&self, offset: usize) -> f64 {
        if offset == 0 {
            0.0
        } else {
            offset as f64 - f64::from(self.fraction)
        }
    }

    /// The engine's display offset is `offset` now: a rise since it was last
    /// seen is output that came in below the view (the engine keeps what is
    /// in view); at the output there is no slide and nothing to count.
    pub fn note(&mut self, offset: usize) {
        if offset > self.seen {
            let more = u32::try_from(offset - self.seen).unwrap_or(u32::MAX);
            self.new_lines = self.new_lines.saturating_add(more);
        }
        self.seen = offset;
        if offset == 0 {
            self.follow();
        }
    }

    /// The view follows the output: no slide, nothing new below it.
    pub fn follow(&mut self) {
        self.fraction = 0.0;
        self.new_lines = 0;
        self.seen = 0;
    }

    /// The view was moved - by the user or the app, not by output - to
    /// display offset `offset` slid by `fraction`.
    pub fn moved_to(&mut self, offset: usize, fraction: f32) {
        self.seen = offset;
        self.fraction = if offset > 0 && fraction.is_finite() {
            fraction.clamp(0.0, 0.999)
        } else {
            0.0
        };
        if offset == 0 {
            self.follow();
        }
    }

    /// Where a scroll to `to` lines up - worked out from what the view
    /// showed - takes the view now that the engine is at `offset` with
    /// `history` lines of scrollback: the output for 0 (following it,
    /// wherever it is); otherwise as far from where the view is as it moved
    /// from what it showed. The display offset and the slide.
    #[must_use]
    pub fn target(&self, to: f64, offset: usize, history: usize) -> (usize, f32) {
        if to.is_nan() || to <= 0.0 {
            return (0, 0.0);
        }
        split(self.up(offset) + (to - self.shown), history)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_position_splits_into_whole_lines_rounded_up_and_a_slide() {
        assert_eq!(split(0.0, 100), (0, 0.0));
        assert_eq!(split(2.25, 100), (3, 0.75));
        assert_eq!(split(3.0, 100), (3, 0.0));
        assert_eq!(split(3.000_000_1, 100), (3, 0.0));
        assert_eq!(split(2.999_999_9, 100), (3, 0.0));
        // Off the output by a hair of a line: offset 1, where the engine
        // keeps what is in view.
        assert_eq!(split(0.2, 100).0, 1);
        assert_eq!(split(150.0, 100), (100, 0.0));
        assert_eq!(split(-1.0, 100), (0, 0.0));
        assert_eq!(split(f64::NAN, 100), (0, 0.0));
        assert_eq!(split(0.5, 0), (0, 0.0));
    }

    #[test]
    fn output_raises_the_offset_and_is_counted_until_the_view_follows_again() {
        let mut v = ViewScroll::default();
        v.moved_to(10, 0.5);
        v.note(10);
        assert_eq!(v.new_lines, 0);
        v.note(60);
        assert_eq!(v.new_lines, 50);
        v.note(60);
        assert_eq!(v.new_lines, 50);
        // A scroll is no output.
        v.moved_to(61, 0.5);
        v.note(61);
        assert_eq!(v.new_lines, 50);
        assert!((v.fraction - 0.5).abs() < f32::EPSILON);
        // Back at the output (typing, Cmd+End): following, nothing counted.
        v.note(0);
        assert_eq!(v.new_lines, 0);
        assert!(v.fraction.abs() < f32::EPSILON);
    }

    #[test]
    fn a_scroll_moves_from_what_the_view_showed_onto_where_it_is_now() {
        let mut v = ViewScroll::default();
        v.moved_to(10, 0.5);
        v.shown = v.up(10);
        assert!((v.shown - 9.5).abs() < 1e-9);
        // 100 lines came in: the engine is at 110 with the same rows in view.
        // One line further up from what is in view, one line down.
        assert_eq!(v.target(10.5, 110, 1_000), (111, 0.5));
        assert_eq!(v.target(8.5, 110, 1_000), (109, 0.5));
        // The output: followed, wherever it is.
        assert_eq!(v.target(0.0, 110, 1_000), (0, 0.0));
        // Past the oldest line: the oldest line.
        assert_eq!(v.target(5_000.0, 110, 1_000), (1_000, 0.0));
    }
}
