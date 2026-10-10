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
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
pub fn wav_tone(seconds: f64, freq: f64) -> Vec<u8> {
    let rate = SAMPLE_RATE;
    let frames = (seconds.max(0.0) * f64::from(rate)).round() as usize;
    let data_len = (frames * 2) as u32;
    let mut out = Vec::with_capacity(44 + frames * 2);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    // 50 ms fades at each end.
    let fade = ((f64::from(rate) * 0.05) as usize).clamp(1, (frames / 2).max(1));
    for i in 0..frames {
        let gain = (i as f64 / fade as f64)
            .min((frames - 1 - i) as f64 / fade as f64)
            .min(1.0);
        let v = (2.0 * std::f64::consts::PI * freq * i as f64 / f64::from(rate)).sin() * 0.5 * gain;
        out.extend_from_slice(&((v * 32767.0).round() as i16).to_le_bytes());
    }
    out
}

/// The sample tracks: two albums of three tones.
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn sample_files() -> Vec<SampleFile> {
    // (album, artist, year, genre, [(title, seconds, Hz)])
    let albums: [(&str, &str, &str, &str, [(&str, f64, f64); 3]); 2] = [
        (
            "Blue Hour",
            "Northlight Quartet",
            "2024",
            "Jazz",
            [
                ("First Light", 12.0, 220.0),
                ("Harbour Walk", 9.0, 261.63),
                ("Grey Morning", 10.0, 293.66),
            ],
        ),
        (
            "Field Notes",
            "Ada Park",
            "2023",
            "Ambient",
            [
                ("Low Tide", 11.0, 329.63),
                ("Salt and Cedar", 8.0, 392.0),
                ("Northwind", 10.0, 440.0),
            ],
        ),
    ];
    let mut files = Vec::new();
    for (album, artist, year, genre, tracks) in albums {
        for (no, (title, seconds, hz)) in tracks.into_iter().enumerate() {
            let bytes = wav_tone(seconds, hz);
            let duration_s = (bytes.len() - 44) as f64 / f64::from(SAMPLE_RATE * 2);
            let track_no = u32::try_from(no + 1).unwrap_or(1);
            files.push(SampleFile {
                key: format!("sample/{album}/{track_no:02} {title}.wav"),
                bytes,
                track: Track {
                    title: title.to_string(),
                    artist: artist.to_string(),
                    album: album.to_string(),
                    album_artist: artist.to_string(),
                    genre: genre.to_string(),
                    year: year.to_string(),
                    track_no,
                    disc_no: 1,
                    duration_s,
                    ..Track::default()
                },
            });
        }
    }
    files
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
