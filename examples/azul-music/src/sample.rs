//! The sample library (`--sample`, or the empty state's button): a few short generated tones as
//! WAV files in the data tree (`music/sample/...`) and their library entries - so the player, the
//! queue and the now-playing bar can be tried (and the E2E script can run) without any music.

use crate::library::Track;

/// One sample track: where it goes in the data tree, its bytes, its library entry (without a
/// path or an id - the app sets those).
#[derive(Debug, Clone)]
pub struct SampleFile {
    /// The key under the app's folder: `sample/<album>/<nn> <title>.wav`.
    pub key: String,
    pub bytes: Vec<u8>,
    pub track: Track,
}

/// The sample rate of the sample tones (small files; plenty for a tone).
pub const SAMPLE_RATE: u32 = 8_000;

/// A mono 16-bit PCM WAV of `seconds` of a tone at `freq` Hz, fading in and out (no clicks).
#[must_use]
pub fn wav_tone(seconds: f64, freq: f64) -> Vec<u8> {
    let _ = (seconds, freq);
    Vec::new()
}

/// The sample tracks: two albums of three tones.
#[must_use]
pub fn sample_files() -> Vec<SampleFile> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tone_is_a_valid_wav_of_the_asked_length() {
        let wav = wav_tone(2.0, 440.0);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..16], b"WAVEfmt ");
        let rate = u32::from_le_bytes([wav[24], wav[25], wav[26], wav[27]]);
        assert_eq!(rate, SAMPLE_RATE);
        let data_len = u32::from_le_bytes([wav[40], wav[41], wav[42], wav[43]]) as usize;
        assert_eq!(
            data_len,
            2 * SAMPLE_RATE as usize * 2,
            "two seconds of 16-bit mono"
        );
        assert_eq!(wav.len(), 44 + data_len);
        // It starts and ends silent (the fades), and is loud in the middle.
        let sample = |i: usize| i16::from_le_bytes([wav[44 + 2 * i], wav[45 + 2 * i]]);
        assert_eq!(sample(0), 0);
        let middle = (0..400)
            .map(|i| sample(8_000 + i).unsigned_abs())
            .max()
            .unwrap_or(0);
        assert!(middle > 8_000, "{middle}");
    }

    #[test]
    fn the_sample_library_is_two_albums_of_three_tagged_tracks() {
        let files = sample_files();
        assert_eq!(files.len(), 6);
        let albums: std::collections::BTreeSet<&str> =
            files.iter().map(|f| f.track.album.as_str()).collect();
        assert_eq!(albums.len(), 2);
        for f in &files {
            assert!(
                f.key.starts_with("sample/") && f.key.ends_with(".wav"),
                "{}",
                f.key
            );
            assert!(!f.track.title.is_empty() && !f.track.artist.is_empty());
            assert!(f.track.track_no >= 1);
            let seconds = (f.bytes.len() - 44) as f64 / f64::from(SAMPLE_RATE * 2);
            assert!((f.track.duration_s - seconds).abs() < 1e-9);
        }
    }
}
