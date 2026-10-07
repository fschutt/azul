//! Keeping the sound with the picture, and the controls out of the way. Plain Rust, tested without
//! a window.
//!
//! The picture is azul's VideoWidget, which runs its own clock and reports its position about
//! four times a second; the sound is azul's AudioPlayer playing the same file's audio track. Both
//! start together, and both are told every pause and seek; what is left is drift (two clocks) and
//! the moments one of them stalls (a slow decode, a seek). The rule: the PICTURE leads (moving it
//! costs a decode from a keyframe), the sound follows it when they are more than
//! [`DEAD_BAND_S`] apart - lip sync is noticeable from about 0.1 s.

/// How far the sound may be from the picture before it is moved to it.
pub const DEAD_BAND_S: f64 = 0.15;
/// How long the controls stay after the pointer stops moving while the video plays.
pub const HIDE_AFTER_MS: u64 = 2_000;

/// Where the sound should be moved to (the picture's position), or `None` while the two are
/// within [`DEAD_BAND_S`] (or a clock is not running yet).
#[must_use]
pub fn audio_correction(video_s: f64, audio_s: f64) -> Option<f64> {
    if !video_s.is_finite() || video_s < 0.0 {
        return None;
    }
    if !audio_s.is_finite() || (video_s - audio_s).abs() > DEAD_BAND_S {
        Some(video_s)
    } else {
        None
    }
}

/// A drift this large is a real jump (a seek the sound missed), moved at once.
pub const JUMP_S: f64 = 1.0;
/// How long after one move of the sound the next small one may come, ms.
pub const CORRECTION_COOLDOWN_MS: u64 = 1_500;

/// When to act on [`audio_correction`]: a SMALL drift is moved only when two reports running
/// say so, and not within [`CORRECTION_COOLDOWN_MS`] of the last move; a jump of [`JUMP_S`] or
/// more at once.
///
/// Every move of the sound is a seek the ear hears (a click, a repeated syllable). A report can
/// be late on its way to the UI thread (a rebuild of the window, a busy frame), and a decoder
/// can stall for a moment; acting on every report made such a moment a stutter of the sound
/// four times a second.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SyncGuard {
    /// Reports running whose drift was outside the dead band.
    strikes: u8,
    /// When the sound was last moved (ms on the app's clock).
    last_move_ms: Option<u64>,
}

impl SyncGuard {
    /// Where to move the sound now (`now_ms` on the app's clock), or `None`.
    pub fn correct(&mut self, video_s: f64, audio_s: f64, now_ms: u64) -> Option<f64> {
        let Some(target) = audio_correction(video_s, audio_s) else {
            self.strikes = 0;
            return None;
        };
        self.strikes = self.strikes.saturating_add(1);
        let jump = !audio_s.is_finite() || (video_s - audio_s).abs() >= JUMP_S;
        let cooled = self
            .last_move_ms
            .is_none_or(|t| now_ms.saturating_sub(t) >= CORRECTION_COOLDOWN_MS);
        if jump || (self.strikes >= 2 && cooled) {
            self.strikes = 0;
            self.last_move_ms = Some(now_ms);
            Some(target)
        } else {
            None
        }
    }

    /// Both clocks were just moved together (a seek, play after pause): start counting again.
    pub fn reset(&mut self) {
        self.strikes = 0;
    }
}

/// Whether the controls over the video show: always while paused (or nothing plays), and while
/// playing for [`HIDE_AFTER_MS`] after the pointer last moved (or a key was pressed).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ControlsVisibility {
    last_activity_ms: u64,
}

impl ControlsVisibility {
    /// The user did something at `now_ms`.
    pub fn activity(&mut self, now_ms: u64) {
        self.last_activity_ms = now_ms;
    }

    /// Whether the controls show at `now_ms`.
    #[must_use]
    pub fn visible(&self, now_ms: u64, playing: bool) -> bool {
        !playing || now_ms.saturating_sub(self.last_activity_ms) < HIDE_AFTER_MS
    }
}

/// The short text the on-screen display shows for a volume (`0.0..=1.0`): "Volume 70 %", or
/// "Muted".
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn volume_osd(volume: f32, muted: bool) -> String {
    if muted {
        return String::from("Muted");
    }
    let percent = if volume.is_finite() {
        (volume.clamp(0.0, 1.0) * 100.0).round() as u32
    } else {
        0
    };
    format!("Volume {percent} %")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sound_follows_the_picture_only_beyond_the_dead_band() {
        assert_eq!(audio_correction(10.0, 10.1), None);
        assert_eq!(audio_correction(10.0, 9.9), None);
        assert_eq!(audio_correction(10.0, 10.4), Some(10.0));
        assert_eq!(
            audio_correction(30.0, 2.0),
            Some(30.0),
            "after a seek the sound jumps too"
        );
        assert_eq!(
            audio_correction(f64::NAN, 2.0),
            None,
            "no picture clock yet"
        );
        assert_eq!(audio_correction(-1.0, 2.0), None);
    }

    #[test]
    fn a_small_drift_moves_the_sound_only_when_it_lasts_and_not_too_often() {
        let mut g = SyncGuard::default();
        // One late report: no move.
        assert_eq!(g.correct(10.0, 10.3, 1_000), None);
        // Back in step: the count starts over.
        assert_eq!(g.correct(10.25, 10.3, 1_250), None);
        assert_eq!(g.correct(10.5, 10.8, 1_500), None);
        // Two reports running: moved.
        assert_eq!(g.correct(10.75, 11.05, 1_750), Some(10.75));
        // Out of step again right after: two more reports, but within the cooldown.
        assert_eq!(g.correct(11.0, 11.3, 2_000), None);
        assert_eq!(g.correct(11.25, 11.55, 2_250), None);
        // Cooled down: the next lasting drift moves it.
        assert_eq!(g.correct(12.5, 12.8, 3_300), Some(12.5));
    }

    #[test]
    fn a_jump_moves_the_sound_at_once() {
        let mut g = SyncGuard::default();
        assert_eq!(g.correct(30.0, 2.0, 500), Some(30.0));
        // Even right after a move.
        assert_eq!(g.correct(60.0, 31.0, 600), Some(60.0));
        assert_eq!(g.correct(5.0, f64::NAN, 700), Some(5.0));
        // No picture clock: nothing.
        assert_eq!(g.correct(f64::NAN, 2.0, 800), None);
    }

    #[test]
    fn a_reset_forgets_a_strike() {
        let mut g = SyncGuard::default();
        assert_eq!(g.correct(10.0, 10.3, 1_000), None);
        g.reset();
        assert_eq!(g.correct(20.0, 20.3, 1_250), None, "one strike after the reset");
    }

    #[test]
    fn the_controls_hide_two_seconds_after_the_last_move_while_playing() {
        let mut c = ControlsVisibility::default();
        c.activity(1_000);
        assert!(c.visible(2_500, true));
        assert!(!c.visible(3_100, true), "playing, 2.1 s idle");
        assert!(c.visible(9_000, false), "paused: always");
        c.activity(9_500);
        assert!(c.visible(10_000, true));
    }

    #[test]
    fn the_osd_says_the_volume_in_percent_or_muted() {
        assert_eq!(volume_osd(0.7, false), "Volume 70 %");
        assert_eq!(volume_osd(1.0, false), "Volume 100 %");
        assert_eq!(volume_osd(0.7, true), "Muted");
    }
}
