//! Audio FILES decoded - the `AudioFileDecoder` handle.
//!
//! A music player reads MP3, AAC (in MP4 / M4A), ALAC, FLAC, Ogg Vorbis, WAV and AIFF; a video
//! player reads the AAC track of an MP4. [`AudioFileDecoder`] opens such a file (a path, or its
//! bytes), says what is in it ([`AudioFileInfo`]: the format, the length, the tags and the cover
//! art) and hands its audio out as [`AudioFrame`]s - interleaved `f32` at the file's own rate -
//! with sample-accurate seeking. [`super::player::AudioPlayer`] plays such files through an
//! `AudioSink`.
//!
//! **Engine:** Symphonia (pure Rust, MPL-2.0; feature `audio-decode`) demuxes every container
//! and decodes every codec above. Opus (in Ogg, MKV or MP4) is demuxed by Symphonia and decoded
//! by azul's own `AudioDecoder` (AudioToolbox on Apple); elsewhere an Opus file says so. Without
//! the feature the handle is closed and says why - never an open-looking one that decodes
//! nothing.
//!
//! The handle is honest like `AudioSink`: `open` / `create` give a CLOSED handle
//! (`is_open()` false) when the file does not open, and `error_message()` says why.

use core::ffi::c_void;

use azul_core::audio::{AudioFrame, OptionAudioFrame};
use azul_css::{AzString, F32Vec, OptionString, U8Vec};

/// What an audio file holds: its format, length and tags. Every text is empty when the file does
/// not say; numbers are 0.
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AudioFileInfo {
    /// The track's title (`TITLE`, ID3 `TIT2`, MP4 `©nam`).
    pub title: AzString,
    /// The performer.
    pub artist: AzString,
    /// The album.
    pub album: AzString,
    /// The album's artist (a compilation's "Various Artists").
    pub album_artist: AzString,
    /// The genre.
    pub genre: AzString,
    /// The date or year the file gives, as it gives it ("2024", "2024-05-01").
    pub date: AzString,
    /// Unsynchronised lyrics, when the file carries them.
    pub lyrics: AzString,
    /// The codec ("mp3", "aac", "flac", "vorbis", "opus", "pcm_s16le", ...).
    pub codec: AzString,
    /// The container ("wave", "flac", "isomp4", "ogg", "mkv", ...).
    pub container: AzString,
    /// The cover art's media type ("image/jpeg", "image/png"), empty without a cover.
    pub cover_mime: AzString,
    /// The cover art, encoded as stored (decode it with `RawImage::decode_image_bytes_any`);
    /// empty without a cover. The front cover when the file has several pictures.
    pub cover: U8Vec,
    /// The length in seconds (0 when the file does not say and it was not measured).
    pub duration_s: f64,
    /// Samples per second per channel.
    pub sample_rate: u32,
    /// The track's number on its album (0 = not given).
    pub track_number: u32,
    /// The track count of the album (0 = not given).
    pub track_total: u32,
    /// The disc's number (0 = not given).
    pub disc_number: u32,
    /// Channels (1 mono, 2 stereo, ...).
    pub channels: u16,
}

azul_css::impl_option!(
    AudioFileInfo,
    OptionAudioFileInfo,
    copy = false,
    [Debug, Clone, PartialEq]
);

/// One open audio file, decoding: what the player and the handle share. Rust-only.
pub(crate) struct FileSource {
    _private: (),
}

impl FileSource {
    /// Opens the file at `path` (the extension is a hint for the probe).
    pub(crate) fn open_path(path: &str) -> Result<FileSource, String> {
        let _ = path;
        Err(String::from("not built yet"))
    }

    /// Opens a file held in memory (`extension`: "mp3", "flac", ...; a hint, may be empty).
    pub(crate) fn open_bytes(bytes: Vec<u8>, extension: &str) -> Result<FileSource, String> {
        let _ = (bytes, extension);
        Err(String::from("not built yet"))
    }

    /// What is in the file.
    pub(crate) fn info(&self) -> &AudioFileInfo {
        unimplemented!()
    }

    /// Samples per second per channel of what [`next_samples`](Self::next_samples) hands out.
    pub(crate) fn rate(&self) -> u32 {
        0
    }

    /// Channels of what [`next_samples`](Self::next_samples) hands out.
    pub(crate) fn channels(&self) -> u16 {
        0
    }

    /// The media time (seconds) of the next sample frame handed out.
    pub(crate) fn position_s(&self) -> f64 {
        0.0
    }

    /// The next decoded samples (interleaved), `None` at the end of the file.
    pub(crate) fn next_samples(&mut self) -> Option<Vec<f32>> {
        None
    }

    /// Goes to `seconds` (clamped to the file); the next samples start exactly there. The media
    /// time reached, or `None` when the file cannot seek.
    pub(crate) fn seek(&mut self, seconds: f64) -> Option<f64> {
        let _ = seconds;
        None
    }
}

/// Engine-side state behind an open [`AudioFileDecoder`].
struct DecoderInner {
    source: FileSource,
}

/// An audio file being decoded. Open one with [`AudioFileDecoder::open`] (a path) or
/// [`AudioFileDecoder::create`] (the file's bytes); read [`info`](Self::info), take
/// [`next_frame`](Self::next_frame)s until `None`, [`seek`](Self::seek) anywhere. Decode on a
/// `Thread`: a whole song takes tens of milliseconds, a long one more.
#[repr(C)]
pub struct AudioFileDecoder {
    /// Opaque pointer to the engine-side state (null when closed).
    pub ptr: *mut c_void,
    /// Why the file did not open; `None` while open, after `close` and on a default handle.
    pub error: OptionString,
    /// Whether this handle owns (and on drop frees) the engine state.
    pub run_destructor: bool,
}

impl Clone for AudioFileDecoder {
    fn clone(&self) -> Self {
        // Non-owning shallow copy (the FFI handle convention): only the original frees.
        AudioFileDecoder {
            ptr: self.ptr,
            error: self.error.clone(),
            run_destructor: false,
        }
    }
}

impl Default for AudioFileDecoder {
    fn default() -> Self {
        AudioFileDecoder {
            ptr: core::ptr::null_mut(),
            error: OptionString::None,
            run_destructor: false,
        }
    }
}

impl Drop for AudioFileDecoder {
    fn drop(&mut self) {
        self.drop_inner();
    }
}

impl AudioFileDecoder {
    /// The decoding engine of this build: "symphonia", or "none" without the `audio-decode`
    /// feature.
    pub fn backend_name() -> AzString {
        AzString::from_const_str("none")
    }

    /// Opens the audio file at `path`. Closed (`is_open()` false, `error_message()` says why)
    /// when the file cannot be read, is not audio, or its codec has no decoder here.
    pub fn open(path: AzString) -> AudioFileDecoder {
        let _ = path;
        AudioFileDecoder::default()
    }

    /// Opens an audio file from its `bytes` (`extension`: "mp3", "m4a", "flac", ... - a hint for
    /// the format probe, may be empty). Closed, saying why, as [`open`](Self::open).
    pub fn create(bytes: U8Vec, extension: AzString) -> AudioFileDecoder {
        let _ = (bytes, extension);
        AudioFileDecoder::default()
    }

    /// Whether the file opened and `close` has not been called.
    pub fn is_open(&self) -> bool {
        !self.ptr.is_null()
    }

    /// Why the file did not open, readable enough to show the user. `None` while open.
    pub fn error_message(&self) -> OptionString {
        self.error.clone()
    }

    /// What is in the file: format, length, tags, cover art (an empty info when closed).
    pub fn info(&self) -> AudioFileInfo {
        AudioFileInfo::default()
    }

    /// The next decoded audio (interleaved `f32` at the file's rate and channels), or `None` at
    /// the end of the file (and on a closed handle).
    pub fn next_frame(&mut self) -> OptionAudioFrame {
        OptionAudioFrame::None
    }

    /// Goes to `position_s` seconds (clamped into the file): the next frame starts exactly there.
    /// False when closed or the file cannot seek.
    pub fn seek(&mut self, position_s: f64) -> bool {
        let _ = position_s;
        false
    }

    /// The media time (seconds) where the next frame starts.
    pub fn position_s(&self) -> f64 {
        0.0
    }

    /// The file's waveform: `buckets` peak levels (`0.0..=1.0`, the loudest sample of each
    /// slice of the file), for the `Waveform` widget. Decodes the whole file from the start
    /// (call it on a `Thread`) and leaves the decoder at the end. Empty when closed.
    pub fn waveform(&mut self, buckets: u32) -> F32Vec {
        let _ = buckets;
        F32Vec::from_vec(Vec::new())
    }

    /// Closes the file. (Dropping the handle does this too.)
    pub fn close(&mut self) {
        self.drop_inner();
    }

    fn drop_inner(&mut self) {
        if self.run_destructor && !self.ptr.is_null() {
            unsafe {
                drop(Box::from_raw(self.ptr as *mut DecoderInner));
            }
        }
        self.ptr = core::ptr::null_mut();
        self.run_destructor = false;
    }
}

#[cfg(all(test, feature = "audio-decode"))]
pub(crate) mod fixtures {
    //! Audio files made in the test: a WAV writer and a FLAC writer (verbatim subframes, the
    //! CRCs computed), so the decoder is tested on files whose every sample is known.

    /// A 16-bit PCM WAV of `frames` frames: channel `c` of frame `i` is `sample(i, c)`.
    pub(crate) fn wav(
        rate: u32,
        channels: u16,
        frames: usize,
        sample: impl Fn(usize, usize) -> i16,
    ) -> Vec<u8> {
        let data_len = (frames * usize::from(channels) * 2) as u32;
        let mut out = Vec::with_capacity(44 + data_len as usize);
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(36 + data_len).to_le_bytes());
        out.extend_from_slice(b"WAVEfmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes()); // PCM
        out.extend_from_slice(&channels.to_le_bytes());
        out.extend_from_slice(&rate.to_le_bytes());
        out.extend_from_slice(&(rate * u32::from(channels) * 2).to_le_bytes());
        out.extend_from_slice(&(channels * 2).to_le_bytes());
        out.extend_from_slice(&16u16.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&data_len.to_le_bytes());
        for i in 0..frames {
            for c in 0..usize::from(channels) {
                out.extend_from_slice(&sample(i, c).to_le_bytes());
            }
        }
        out
    }

    /// A sine at `freq` Hz, amplitude `amp` (of full scale), as a 16-bit sample.
    pub(crate) fn sine16(i: usize, rate: u32, freq: f64, amp: f64) -> i16 {
        let v = (2.0 * core::f64::consts::PI * freq * i as f64 / f64::from(rate)).sin() * amp;
        (v * 32767.0).round() as i16
    }

    fn crc8(data: &[u8]) -> u8 {
        let mut crc = 0u8;
        for &b in data {
            crc ^= b;
            for _ in 0..8 {
                crc = if crc & 0x80 != 0 {
                    (crc << 1) ^ 0x07
                } else {
                    crc << 1
                };
            }
        }
        crc
    }

    fn crc16(data: &[u8]) -> u16 {
        let mut crc = 0u16;
        for &b in data {
            crc ^= u16::from(b) << 8;
            for _ in 0..8 {
                crc = if crc & 0x8000 != 0 {
                    (crc << 1) ^ 0x8005
                } else {
                    crc << 1
                };
            }
        }
        crc
    }

    /// FLAC's "UTF-8" coding of a frame number.
    fn utf8_number(n: u32) -> Vec<u8> {
        if n < 0x80 {
            vec![n as u8]
        } else if n < 0x800 {
            vec![0xC0 | (n >> 6) as u8, 0x80 | (n & 0x3F) as u8]
        } else {
            vec![
                0xE0 | (n >> 12) as u8,
                0x80 | ((n >> 6) & 0x3F) as u8,
                0x80 | (n & 0x3F) as u8,
            ]
        }
    }

    /// A stereo 16-bit FLAC at 44.1 kHz of `frames` (left, right), in blocks of `block` frames
    /// (verbatim subframes), with Vorbis `comments` ("KEY=value") and an optional front cover
    /// (media type, bytes).
    pub(crate) fn flac(
        frames: &[(i16, i16)],
        block: usize,
        comments: &[&str],
        cover: Option<(&str, &[u8])>,
    ) -> Vec<u8> {
        const RATE: u64 = 44_100;
        let mut out = b"fLaC".to_vec();

        let mut stream_info = Vec::new();
        stream_info.extend_from_slice(&(block as u16).to_be_bytes());
        stream_info.extend_from_slice(&(block as u16).to_be_bytes());
        stream_info.extend_from_slice(&[0u8; 6]); // frame sizes unknown
        let packed: u64 =
            (RATE << 44) | (1u64 << 41) | (15u64 << 36) | (frames.len() as u64 & 0xF_FFFF_FFFF);
        stream_info.extend_from_slice(&packed.to_be_bytes());
        stream_info.extend_from_slice(&[0u8; 16]); // MD5 unknown

        let mut comment = Vec::new();
        let vendor = b"azul test";
        comment.extend_from_slice(&(vendor.len() as u32).to_le_bytes());
        comment.extend_from_slice(vendor);
        comment.extend_from_slice(&(comments.len() as u32).to_le_bytes());
        for c in comments {
            comment.extend_from_slice(&(c.len() as u32).to_le_bytes());
            comment.extend_from_slice(c.as_bytes());
        }

        let mut blocks: Vec<(u8, Vec<u8>)> = vec![(0, stream_info), (4, comment)];
        if let Some((mime, data)) = cover {
            let mut picture = Vec::new();
            picture.extend_from_slice(&3u32.to_be_bytes()); // front cover
            picture.extend_from_slice(&(mime.len() as u32).to_be_bytes());
            picture.extend_from_slice(mime.as_bytes());
            picture.extend_from_slice(&0u32.to_be_bytes()); // no description
            for v in [1u32, 1, 24, 0] {
                picture.extend_from_slice(&v.to_be_bytes());
            }
            picture.extend_from_slice(&(data.len() as u32).to_be_bytes());
            picture.extend_from_slice(data);
            blocks.push((6, picture));
        }
        let count = blocks.len();
        for (i, (kind, body)) in blocks.into_iter().enumerate() {
            let last = if i + 1 == count { 0x80 } else { 0 };
            out.push(last | kind);
            out.extend_from_slice(&(body.len() as u32).to_be_bytes()[1..]);
            out.extend_from_slice(&body);
        }

        for (n, chunk) in frames.chunks(block).enumerate() {
            // Fixed blocking; block size from the 16 bits after the number (0111); 44.1 kHz
            // (1001); left / right (0001); 16 bits a sample (100).
            let mut frame = vec![
                0xFF,
                0xF8,
                (0b0111 << 4) | 0b1001,
                (0b0001 << 4) | (0b100 << 1),
            ];
            frame.extend(utf8_number(n as u32));
            frame.extend_from_slice(&((chunk.len() - 1) as u16).to_be_bytes());
            let header_crc = crc8(&frame);
            frame.push(header_crc);
            for channel in 0..2 {
                frame.push(0x02); // a verbatim subframe, no wasted bits
                for &(l, r) in chunk {
                    let v = if channel == 0 { l } else { r };
                    frame.extend_from_slice(&v.to_be_bytes());
                }
            }
            let frame_crc = crc16(&frame);
            frame.extend_from_slice(&frame_crc.to_be_bytes());
            out.extend(frame);
        }
        out
    }
}

#[cfg(all(test, feature = "audio-decode"))]
mod decode_tests {
    use azul_css::{AzString, U8Vec};

    use super::{fixtures, AudioFileDecoder};

    fn open(bytes: Vec<u8>, ext: &str) -> AudioFileDecoder {
        let d = AudioFileDecoder::create(U8Vec::from_vec(bytes), AzString::from(ext));
        assert!(
            d.is_open(),
            "the file opens: {:?}",
            d.error_message()
                .into_option()
                .map(|s| s.as_str().to_string())
        );
        d
    }

    /// Every sample of a decoded file, interleaved, and the frames' (rate, channels).
    fn decode_all(d: &mut AudioFileDecoder) -> (Vec<f32>, Vec<(u32, u16)>) {
        let mut all = Vec::new();
        let mut formats = Vec::new();
        while let Some(frame) = d.next_frame().into_option() {
            formats.push((frame.sample_rate, frame.channels));
            all.extend_from_slice(frame.samples.as_ref());
        }
        (all, formats)
    }

    #[test]
    fn a_wav_file_says_its_format_and_length_and_decodes_every_sample() {
        let wav = fixtures::wav(48_000, 2, 48_000, |i, c| {
            fixtures::sine16(i, 48_000, if c == 0 { 440.0 } else { 880.0 }, 0.5)
        });
        let mut d = open(wav, "wav");
        assert_eq!(AudioFileDecoder::backend_name().as_str(), "symphonia");
        let info = d.info();
        assert_eq!(info.sample_rate, 48_000);
        assert_eq!(info.channels, 2);
        assert!((info.duration_s - 1.0).abs() < 1e-6, "{}", info.duration_s);
        assert_eq!(info.container.as_str(), "wave");
        let (samples, formats) = decode_all(&mut d);
        assert_eq!(samples.len(), 48_000 * 2, "every frame, both channels");
        assert!(formats.iter().all(|f| *f == (48_000, 2)));
        // The samples are the file's, scaled to -1..1.
        for i in [0usize, 1, 100, 12_345, 47_999] {
            let want = f32::from(fixtures::sine16(i, 48_000, 440.0, 0.5)) / 32768.0;
            assert!((samples[i * 2] - want).abs() < 1e-4, "frame {i}");
        }
        assert!(
            d.next_frame().into_option().is_none(),
            "nothing after the end"
        );
    }

    #[test]
    fn a_flac_file_gives_its_tags_and_cover_and_decodes_losslessly() {
        let frames: Vec<(i16, i16)> = (0..10_000)
            .map(|i| (fixtures::sine16(i, 44_100, 440.0, 0.4), -(i as i16 % 300)))
            .collect();
        let cover = b"\x89PNG not really a png".to_vec();
        let flac = fixtures::flac(
            &frames,
            4096,
            &[
                "TITLE=First Light",
                "ARTIST=Northlight Quartet",
                "ALBUM=Blue Hour",
                "ALBUMARTIST=Northlight Quartet",
                "TRACKNUMBER=1/7",
                "DISCNUMBER=1",
                "DATE=2024",
                "GENRE=Jazz",
            ],
            Some(("image/png", &cover)),
        );
        let mut d = open(flac, "flac");
        let info = d.info();
        assert_eq!(info.title.as_str(), "First Light");
        assert_eq!(info.artist.as_str(), "Northlight Quartet");
        assert_eq!(info.album.as_str(), "Blue Hour");
        assert_eq!(info.album_artist.as_str(), "Northlight Quartet");
        assert_eq!(info.track_number, 1);
        assert_eq!(info.track_total, 7);
        assert_eq!(info.disc_number, 1);
        assert_eq!(info.date.as_str(), "2024");
        assert_eq!(info.genre.as_str(), "Jazz");
        assert_eq!(info.codec.as_str(), "flac");
        assert_eq!(info.sample_rate, 44_100);
        assert_eq!(info.channels, 2);
        assert!((info.duration_s - 10_000.0 / 44_100.0).abs() < 1e-6);
        assert_eq!(info.cover_mime.as_str(), "image/png");
        assert_eq!(info.cover.as_ref(), cover.as_slice());

        let (samples, _) = decode_all(&mut d);
        assert_eq!(samples.len(), 10_000 * 2);
        for (i, (l, r)) in frames.iter().enumerate() {
            assert_eq!(samples[i * 2], f32::from(*l) / 32768.0, "left {i}");
            assert_eq!(samples[i * 2 + 1], f32::from(*r) / 32768.0, "right {i}");
        }
    }

    #[test]
    fn a_seek_lands_on_the_exact_frame_asked_for() {
        // Each frame's left sample is its index (mod 30000), so the first frame after a seek
        // says where it is.
        let wav = fixtures::wav(8_000, 2, 24_000, |i, c| {
            if c == 0 {
                (i % 30_000) as i16
            } else {
                0
            }
        });
        let mut d = open(wav, "wav");
        assert!(d.seek(1.5));
        assert!((d.position_s() - 1.5).abs() < 1e-6);
        let frame = d.next_frame().into_option().expect("audio after the seek");
        let first = (frame.samples.as_ref()[0] * 32768.0).round() as i64;
        assert_eq!(first, 12_000, "1.5 s at 8 kHz is frame 12000");
        // Back to the start.
        assert!(d.seek(0.0));
        let frame = d.next_frame().into_option().expect("audio at the start");
        assert_eq!((frame.samples.as_ref()[0] * 32768.0).round() as i64, 0);
        // Past the end: clamped, then nothing more.
        assert!(d.seek(99.0));
        let mut rest = 0;
        while let Some(f) = d.next_frame().into_option() {
            rest += f.samples.as_ref().len();
        }
        assert!(
            rest <= 2 * 8,
            "at most a sliver after a seek past the end: {rest}"
        );
    }

    #[test]
    fn a_file_that_is_not_audio_gives_a_closed_handle_that_says_why() {
        let d = AudioFileDecoder::create(
            U8Vec::from_vec(b"this is a text file, not audio".to_vec()),
            AzString::from("mp3"),
        );
        assert!(!d.is_open());
        let why = d.error_message().into_option().expect("a reason");
        assert!(!why.as_str().is_empty());
        let missing = AudioFileDecoder::open(AzString::from("/nonexistent/song.flac"));
        assert!(!missing.is_open());
        assert!(missing
            .error_message()
            .into_option()
            .expect("a reason")
            .as_str()
            .contains("song.flac"));
    }

    #[test]
    fn the_waveform_is_the_peak_of_each_slice_of_the_file() {
        // One second loud (0.8), one second quiet (0.2).
        let wav = fixtures::wav(8_000, 1, 16_000, |i, _| {
            let amp = if i < 8_000 { 0.8 } else { 0.2 };
            fixtures::sine16(i, 8_000, 100.0, amp)
        });
        let mut d = open(wav, "wav");
        let peaks = d.waveform(4);
        let peaks = peaks.as_ref();
        assert_eq!(peaks.len(), 4);
        for p in &peaks[..2] {
            assert!((p - 0.8).abs() < 0.01, "{peaks:?}");
        }
        for p in &peaks[2..] {
            assert!((p - 0.2).abs() < 0.01, "{peaks:?}");
        }
    }
}
