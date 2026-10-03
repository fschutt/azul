//! The alarm sounds, synthesised: a few seconds of mono PCM (f32, -1 to 1)
//! for azul's `AudioSink`, so no decoder and no sound file is needed (the
//! plan's ToneGenerator). Each sound is a pattern that loops: the app plays
//! one pattern after the other while the alarm rings.

use serde::{Deserialize, Serialize};

/// The sample rate every tone is made at.
pub const SAMPLE_RATE: u32 = 48_000;

/// An alarm's sound.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Sound {
    /// A bell struck twice a second, ringing out.
    #[default]
    Bells,
    /// Four short beeps, then a pause (a kitchen timer).
    Beep,
    /// Two falling notes (E5, C5), softly.
    Chime,
    /// No sound: the window and the notification only.
    Silent,
}

impl Sound {
    /// In the editor's order.
    pub const ALL: [Sound; 4] = [Sound::Bells, Sound::Beep, Sound::Chime, Sound::Silent];

    /// For the editor's list.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Sound::Bells => "Morning bells",
            Sound::Beep => "Beep",
            Sound::Chime => "Chime",
            Sound::Silent => "Silent",
        }
    }

    /// The position in [`Sound::ALL`].
    #[must_use]
    pub fn index(self) -> usize {
        Sound::ALL.iter().position(|s| *s == self).unwrap_or(0)
    }
}

/// One loop of `sound`: `seconds` of mono samples at [`SAMPLE_RATE`]
/// (empty for [`Sound::Silent`]). `volume` is 0 to 1.
#[must_use]
pub fn pattern(sound: Sound, seconds: f32, volume: f32) -> Vec<f32> {
    let _ = (sound, seconds, volume);
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rms(samples: &[f32]) -> f32 {
        if samples.is_empty() {
            return 0.0;
        }
        (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
    }

    #[test]
    fn a_pattern_is_as_long_as_asked_and_stays_within_full_scale() {
        for sound in [Sound::Bells, Sound::Beep, Sound::Chime] {
            let samples = pattern(sound, 1.5, 1.0);
            assert_eq!(samples.len(), 72_000, "{sound:?}");
            assert!(samples.iter().all(|s| (-1.0..=1.0).contains(s)), "{sound:?} clips");
            assert!(rms(&samples) > 0.05, "{sound:?} is audible");
        }
        assert!(pattern(Sound::Silent, 1.0, 1.0).is_empty());
    }

    #[test]
    fn the_volume_scales_the_sound() {
        let loud = rms(&pattern(Sound::Bells, 1.0, 1.0));
        let soft = rms(&pattern(Sound::Bells, 1.0, 0.25));
        assert!((soft / loud - 0.25).abs() < 0.01, "{soft} / {loud}");
    }

    #[test]
    fn the_beep_pauses_between_its_beeps() {
        let samples = pattern(Sound::Beep, 1.0, 1.0);
        // 4 beeps of 100 ms, each followed by 50 ms of silence, then 400 ms of silence.
        let at = |ms: usize| &samples[ms * 48..(ms + 20) * 48];
        assert!(rms(at(20)) > 0.1, "the first beep");
        assert!(rms(at(120)) < 0.001, "the gap after it");
        assert!(rms(at(170)) > 0.1, "the second beep");
        assert!(rms(at(700)) < 0.001, "the pause after the fourth");
    }

    #[test]
    fn every_sound_has_a_label_and_a_place() {
        for (i, s) in Sound::ALL.iter().enumerate() {
            assert_eq!(s.index(), i);
            assert!(!s.label().is_empty());
        }
        assert_eq!(serde_json::to_string(&Sound::Chime).unwrap(), "\"chime\"");
    }
}
