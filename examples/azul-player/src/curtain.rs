//! Opening a video, the way Media Center does it: the picture and the sound get ready OUT OF
//! SIGHT first (the PREROLL: the video widget decodes its first picture, paused and hidden; the
//! audio player opens the file and decodes its first samples, held), then the menus fade to black
//! in order - the text first, the icons second, the blue ground and its light last - and the
//! picture fades in from black while picture and sound START TOGETHER. Nothing moves before both
//! are ready, so the sound never lags the picture at the start.
//!
//! [`Curtain`] is the order of it, driven by the app's tick and by what the widget and the player
//! report; the window reads [`Curtain::stage`] for what to show. Plain Rust, tested without a
//! window.

/// How long the text takes to fade out, ms.
pub const TEXT_FADE_MS: u64 = 220;
/// When the icons start to fade (after the text has begun), and for how long.
pub const ICON_DELAY_MS: u64 = 140;
pub const ICON_FADE_MS: u64 = 240;
/// When the blue ground and its light start to fade to black, and for how long.
pub const GROUND_DELAY_MS: u64 = 300;
pub const GROUND_FADE_MS: u64 = 520;
/// The whole fade to black: the ground's end.
pub const FADE_OUT_MS: u64 = GROUND_DELAY_MS + GROUND_FADE_MS;
/// The picture's fade in from black.
pub const FADE_IN_MS: u64 = 480;
/// The longest the menus wait for the SOUND once the picture is ready (a file whose sound does
/// not open, a machine without an output): then the picture starts alone.
pub const SOUND_WAIT_MS: u64 = 4_000;

/// Where an opening stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Curtain {
    /// No video is being opened (or one plays: [`Curtain::Open`]).
    #[default]
    Closed,
    /// The picture and the sound get ready out of sight; the menus stay.
    Preroll {
        since_ms: u64,
        picture: bool,
        sound: bool,
    },
    /// Both are ready (or the sound's wait ran out): the menus fade to black.
    FadeOut { since_ms: u64 },
    /// The picture fades in from black; picture and sound play.
    FadeIn { since_ms: u64 },
    /// The picture plays, the menus are gone.
    Open,
}

/// What the app does at a step of the curtain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cue {
    /// Rebuild: the menus start to fade.
    FadeOut,
    /// Start the picture and the sound now (together), and fade the picture in.
    Play,
    /// The opening is over: the menus go.
    Opened,
}

/// What the window shows of an opening.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stage {
    /// The menus' text, icons and ground show (`false`: faded to black).
    pub menus: bool,
    /// The picture shows (`false`: hidden, black).
    pub picture: bool,
    /// The menus are in the window at all (they go when the opening is over).
    pub menus_mounted: bool,
}

impl Curtain {
    /// A video starts opening at `now_ms`: nothing is ready yet.
    #[must_use]
    pub const fn preroll(now_ms: u64) -> Curtain {
        Curtain::Preroll {
            since_ms: now_ms,
            picture: false,
            sound: false,
        }
    }

    /// The video widget showed its first picture (held).
    pub fn picture_ready(&mut self) {
        if let Curtain::Preroll { picture, .. } = self {
            *picture = true;
        }
    }

    /// The audio player has the sound decoded and held (or there is no sound to wait for).
    pub fn sound_ready(&mut self) {
        if let Curtain::Preroll { sound, .. } = self {
            *sound = true;
        }
    }

    /// Whether the opening still waits for the picture or the sound.
    #[must_use]
    pub const fn prerolling(&self) -> bool {
        matches!(self, Curtain::Preroll { .. })
    }

    /// Whether a video is being opened or plays (the stage is in the window).
    #[must_use]
    pub const fn active(&self) -> bool {
        !matches!(self, Curtain::Closed)
    }

    /// The next step at `now_ms`, if one is due: the fade starts once both are ready (or the
    /// picture is ready and the sound's wait ran out), the play once the menus are black, the end
    /// once the picture is in.
    pub fn step(&mut self, now_ms: u64) -> Option<Cue> {
        match *self {
            Curtain::Preroll {
                since_ms,
                picture,
                sound,
            } => {
                let waited = now_ms.saturating_sub(since_ms);
                if picture && (sound || waited >= SOUND_WAIT_MS) {
                    *self = Curtain::FadeOut { since_ms: now_ms };
                    Some(Cue::FadeOut)
                } else {
                    None
                }
            }
            Curtain::FadeOut { since_ms } if now_ms.saturating_sub(since_ms) >= FADE_OUT_MS => {
                *self = Curtain::FadeIn { since_ms: now_ms };
                Some(Cue::Play)
            }
            Curtain::FadeIn { since_ms } if now_ms.saturating_sub(since_ms) >= FADE_IN_MS => {
                *self = Curtain::Open;
                Some(Cue::Opened)
            }
            _ => None,
        }
    }

    /// The video could not be opened: the curtain opens at once on its note (no fade waits on a
    /// picture that will never come).
    pub fn failed(&mut self) {
        if self.active() {
            *self = Curtain::Open;
        }
    }

    /// What the window shows now.
    #[must_use]
    pub const fn stage(&self) -> Stage {
        match self {
            Curtain::Closed => Stage {
                menus: true,
                picture: false,
                menus_mounted: true,
            },
            Curtain::Preroll { .. } => Stage {
                menus: true,
                picture: false,
                menus_mounted: true,
            },
            Curtain::FadeOut { .. } => Stage {
                menus: false,
                picture: false,
                menus_mounted: true,
            },
            Curtain::FadeIn { .. } => Stage {
                menus: false,
                picture: true,
                menus_mounted: true,
            },
            Curtain::Open => Stage {
                menus: false,
                picture: true,
                menus_mounted: false,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_moves_until_the_picture_and_the_sound_are_both_ready() {
        let mut c = Curtain::preroll(1_000);
        assert_eq!(c.step(1_100), None);
        c.picture_ready();
        assert_eq!(c.step(1_200), None, "the sound is not ready");
        assert!(c.stage().menus && !c.stage().picture, "the picture stays hidden");
        c.sound_ready();
        assert_eq!(c.step(1_300), Some(Cue::FadeOut));
        assert!(!c.stage().menus && !c.stage().picture, "black between the two");
        assert_eq!(c.step(1_300 + FADE_OUT_MS - 1), None, "the menus fade first");
        assert_eq!(c.step(1_300 + FADE_OUT_MS), Some(Cue::Play), "then both start");
        assert!(c.stage().picture && c.stage().menus_mounted);
        assert_eq!(c.step(1_300 + FADE_OUT_MS + FADE_IN_MS), Some(Cue::Opened));
        assert_eq!(c, Curtain::Open);
        assert!(!c.stage().menus_mounted);
        assert_eq!(c.step(99_999), None);
    }

    #[test]
    fn the_sound_alone_never_starts_anything_and_its_wait_has_an_end() {
        let mut c = Curtain::preroll(0);
        c.sound_ready();
        assert_eq!(c.step(60_000), None, "no picture: nothing starts, however long");
        let mut c = Curtain::preroll(0);
        c.picture_ready();
        assert_eq!(c.step(SOUND_WAIT_MS - 1), None);
        assert_eq!(c.step(SOUND_WAIT_MS), Some(Cue::FadeOut), "the picture starts alone");
    }

    #[test]
    fn the_fades_are_in_order_text_icons_ground() {
        assert!(TEXT_FADE_MS > ICON_DELAY_MS, "the icons start while the text fades");
        assert!(ICON_DELAY_MS + ICON_FADE_MS > GROUND_DELAY_MS);
        assert!(GROUND_DELAY_MS > ICON_DELAY_MS);
        assert_eq!(FADE_OUT_MS, GROUND_DELAY_MS + GROUND_FADE_MS);
    }

    #[test]
    fn a_video_that_fails_opens_the_curtain_at_once_and_a_closed_one_stays_closed() {
        let mut c = Curtain::preroll(0);
        c.failed();
        assert_eq!(c, Curtain::Open);
        let mut closed = Curtain::Closed;
        closed.failed();
        closed.picture_ready();
        assert_eq!(closed, Curtain::Closed);
        assert!(!closed.active() && !closed.prerolling());
    }
}
