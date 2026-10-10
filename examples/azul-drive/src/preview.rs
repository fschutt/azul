//! What the preview pane can show for a file: an image (decoded by azul), the
//! first 64 KB of a text or code file, a PDF's first page (azul's PDF reader
//! and SVG renderer), a WAV sound (read here, played by azul's AudioSink), a
//! video (azul's video widget), or why it cannot (other audio: azul has no
//! audio decoder). No azul types.

use crate::browse;

/// What the preview pane can do with a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PreviewKind {
    /// Decoded by azul (`RawImage::decode_image_bytes_any`).
    Image,
    /// The first [`TEXT_PREVIEW_BYTES`] as text.
    Text,
    /// The first page: azul's PDF reader turns it into SVG, azul's SVG
    /// renderer draws it.
    Pdf,
    /// azul's video widget (H.264 MP4; a cloud file is fetched first).
    Video,
    /// A sound: a WAV file plays (its samples are read here); azul has no
    /// decoder for the other formats, so the pane says so for them.
    Audio,
    /// Nothing to show.
    None,
}

/// Bytes a text preview reads (one ranged GET).
pub const TEXT_PREVIEW_BYTES: u64 = 64 * 1024;
/// Bigger images are not fetched for a preview.
pub const IMAGE_PREVIEW_MAX_BYTES: u64 = 32 * 1024 * 1024;
/// Bigger cloud videos are not fetched for a preview (a local one plays in place).
pub const VIDEO_PREVIEW_MAX_BYTES: u64 = 256 * 1024 * 1024;

/// File names without an extension that are text.
const TEXT_NAMES: &[&str] = &[
    "makefile",
    "dockerfile",
    "readme",
    "license",
    "licence",
    "changelog",
    "authors",
    "copying",
    "todo",
];

/// The preview a file name gets.
#[must_use]
pub fn preview_kind(name: &str) -> PreviewKind {
    let Some(ext) = browse::extension_of(name) else {
        let lower = name.to_ascii_lowercase();
        return if name.starts_with('.') || TEXT_NAMES.contains(&lower.as_str()) {
            PreviewKind::Text
        } else {
            PreviewKind::None
        };
    };
    match ext.to_ascii_lowercase().as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "ico" | "tif" | "tiff" | "tga" => {
            PreviewKind::Image
        }
        "mp4" | "m4v" | "mov" => PreviewKind::Video,
        "mp3" | "wav" | "flac" | "ogg" | "m4a" | "aac" | "opus" => PreviewKind::Audio,
        "pdf" => PreviewKind::Pdf,
        "txt" | "md" | "markdown" | "rs" | "toml" | "json" | "yaml" | "yml" | "xml" | "html"
        | "htm" | "css" | "js" | "mjs" | "ts" | "tsx" | "jsx" | "py" | "c" | "h" | "cpp"
        | "hpp" | "cc" | "java" | "kt" | "go" | "rb" | "php" | "sh" | "bash" | "zsh" | "fish"
        | "csv" | "tsv" | "log" | "ini" | "cfg" | "conf" | "eml" | "ics" | "vcf" | "svg"
        | "lock" | "sql" | "lua" | "swift" | "cs" | "fs" | "hs" | "ml" | "ex" | "exs" | "erl"
        | "clj" | "scala" | "r" | "pl" | "tex" | "rst" | "adoc" | "properties" | "gradle"
        | "cmake" | "mk" | "diff" | "patch" | "gitignore" | "env" | "bat" | "ps1" => {
            PreviewKind::Text
        }
        _ => PreviewKind::None,
    }
}

/// Whether a file of `size` bytes is fetched for its preview: text always
/// (its first 64 KB), an image, a PDF or a video up to its limit, an unknown
/// size never. Audio is not fetched here: a WAV file has its own limit
/// ([`AUDIO_PREVIEW_MAX_BYTES`], checked by the preview job).
#[must_use]
pub fn fits_preview(kind: PreviewKind, size: Option<u64>) -> bool {
    match kind {
        PreviewKind::Text => true,
        PreviewKind::Image | PreviewKind::Pdf => {
            size.is_some_and(|s| s <= IMAGE_PREVIEW_MAX_BYTES)
        }
        PreviewKind::Video => size.is_some_and(|s| s <= VIDEO_PREVIEW_MAX_BYTES),
        PreviewKind::Audio | PreviewKind::None => false,
    }
}

/// The text of a preview: `bytes` decoded (invalid UTF-8 replaced); a cut
/// file (`truncated`) ends at its last complete line (the pane says it is the
/// start). A NUL byte in the first 8 KB means a binary file: `Err`.
pub fn text_preview(bytes: &[u8], truncated: bool) -> Result<String, &'static str> {
    let probe = &bytes[..bytes.len().min(8192)];
    if probe.contains(&0) {
        return Err("azdrive-preview-binary");
    }
    let mut text = String::from_utf8_lossy(bytes).into_owned();
    if truncated {
        if let Some(end) = text.rfind('\n') {
            text.truncate(end + 1);
        }
    }
    Ok(text)
}

/// Bigger WAV files are not fetched for a preview.
pub const AUDIO_PREVIEW_MAX_BYTES: u64 = 64 * 1024 * 1024;

/// A WAV file's samples, ready for azul's `AudioSink` (which plays raw f32
/// samples; azul decodes no audio format, but a WAV file IS samples).
#[derive(Debug, Clone, PartialEq)]
pub struct WavSamples {
    pub sample_rate: u32,
    pub channels: u16,
    /// Interleaved, in -1..1.
    pub samples: Vec<f32>,
}

impl WavSamples {
    /// How long the sound plays.
    #[must_use]
    /// Frames (one sample per channel each).
    #[must_use]
    pub fn frames(&self) -> u64 {
        if self.channels == 0 {
            return 0;
        }
        (self.samples.len() / usize::from(self.channels)) as u64
    }

    pub fn seconds(&self) -> f64 {
        if self.sample_rate == 0 || self.channels == 0 {
            return 0.0;
        }
        self.samples.len() as f64 / f64::from(self.channels) / f64::from(self.sample_rate)
    }
}

/// Whether the preview pane can play the file (a WAV file).
#[must_use]
pub fn is_playable_audio(name: &str) -> bool {
    browse::extension_of(name).is_some_and(|e| e.eq_ignore_ascii_case("wav"))
}

/// The samples of a RIFF WAVE file: 8-, 16-, 24- and 32-bit PCM and 32-bit
/// float (also inside WAVE_FORMAT_EXTENSIBLE), as f32 in -1..1. A
/// compressed WAV (ADPCM, ...) or another file is refused, with the reason.
pub fn wav_samples(bytes: &[u8]) -> Result<WavSamples, &'static str> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("azdrive-preview-not-wav");
    }
    let mut at = 12usize;
    let mut format: Option<(u16, u16, u32, u16)> = None;
    while bytes.len().saturating_sub(at) >= 8 {
        let id = &bytes[at..at + 4];
        let len = u32::from_le_bytes([bytes[at + 4], bytes[at + 5], bytes[at + 6], bytes[at + 7]])
            as usize;
        let body_start = at + 8;
        let body_end = body_start.saturating_add(len).min(bytes.len());
        let body = &bytes[body_start..body_end];
        if id == b"fmt " {
            if body.len() < 16 {
                return Err("azdrive-preview-wav-broken");
            }
            let tag = u16::from_le_bytes([body[0], body[1]]);
            let channels = u16::from_le_bytes([body[2], body[3]]);
            let rate = u32::from_le_bytes([body[4], body[5], body[6], body[7]]);
            let bits = u16::from_le_bytes([body[14], body[15]]);
            // WAVE_FORMAT_EXTENSIBLE: the sub-format GUID starts with the tag.
            let tag = if tag == 0xFFFE && body.len() >= 26 {
                u16::from_le_bytes([body[24], body[25]])
            } else {
                tag
            };
            format = Some((tag, channels, rate, bits));
        } else if id == b"data" {
            let Some((tag, channels, rate, bits)) = format else {
                return Err("azdrive-preview-wav-no-format");
            };
            if channels == 0 || rate == 0 {
                return Err("azdrive-preview-wav-no-channels");
            }
            let samples: Vec<f32> = match (tag, bits) {
                (1, 8) => body.iter().map(|b| (f32::from(*b) - 128.0) / 128.0).collect(),
                (1, 16) => body
                    .chunks_exact(2)
                    .map(|c| f32::from(i16::from_le_bytes([c[0], c[1]])) / 32_768.0)
                    .collect(),
                (1, 24) => body
                    .chunks_exact(3)
                    .map(|c| (i32::from_le_bytes([0, c[0], c[1], c[2]]) >> 8) as f32 / 8_388_608.0)
                    .collect(),
                (1, 32) => body
                    .chunks_exact(4)
                    .map(|c| i32::from_le_bytes([c[0], c[1], c[2], c[3]]) as f32 / 2_147_483_648.0)
                    .collect(),
                (3, 32) => body
                    .chunks_exact(4)
                    .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                    .collect(),
                _ => return Err("azdrive-preview-wav-compressed"),
            };
            return Ok(WavSamples {
                sample_rate: rate,
                channels,
                samples,
            });
        }
        // Chunks are padded to an even size.
        at = body_start.saturating_add(len).saturating_add(len & 1);
    }
    Err("azdrive-preview-wav-no-samples")
}

/// Why the pane shows no preview for a kind (a sentence's key), or `None`.
#[must_use]
pub fn no_preview_reason(kind: PreviewKind) -> Option<&'static str> {
    match kind {
        PreviewKind::Audio => Some("azdrive-preview-audio-format"),
        PreviewKind::None => Some("azdrive-preview-none"),
        PreviewKind::Image | PreviewKind::Pdf | PreviewKind::Text | PreviewKind::Video => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_kind_of_preview_comes_from_the_extension() {
        assert_eq!(preview_kind("photo.JPG"), PreviewKind::Image);
        assert_eq!(preview_kind("icon.png"), PreviewKind::Image);
        assert_eq!(preview_kind("notes.txt"), PreviewKind::Text);
        assert_eq!(preview_kind("main.rs"), PreviewKind::Text);
        assert_eq!(preview_kind("Cargo.toml"), PreviewKind::Text);
        assert_eq!(preview_kind("Makefile"), PreviewKind::Text);
        assert_eq!(preview_kind("clip.mp4"), PreviewKind::Video);
        assert_eq!(preview_kind("song.mp3"), PreviewKind::Audio);
        assert_eq!(preview_kind("paper.pdf"), PreviewKind::Pdf);
        assert_eq!(preview_kind("archive.zip"), PreviewKind::None);
        assert_eq!(preview_kind("program.exe"), PreviewKind::None);
    }

    #[test]
    fn a_text_preview_is_the_decoded_text_cut_at_a_line_end() {
        let text = text_preview(b"line one\nline two\n", false).unwrap();
        assert_eq!(text, "line one\nline two\n");
        let cut = text_preview(b"line one\nline tw", true).unwrap();
        assert!(cut.starts_with("line one\n"), "{cut}");
        assert!(!cut.contains("line tw"), "a cut line is dropped: {cut}");
        assert!(cut.ends_with('\n'), "it ends at a line's end: {cut}");
        // Invalid UTF-8 is shown, replaced; NUL bytes mean a binary file.
        assert!(text_preview(&[b'a', 0xff, b'b'], false)
            .unwrap()
            .contains('a'));
        assert!(text_preview(&[0x7f, b'E', b'L', b'F', 0, 0, 0, 1], false).is_err());
    }

    #[test]
    fn previews_have_limits_so_a_huge_file_is_never_fetched_whole() {
        assert_eq!(TEXT_PREVIEW_BYTES, 64 * 1024);
        assert!(fits_preview(PreviewKind::Image, Some(5 * 1024 * 1024)));
        assert!(!fits_preview(
            PreviewKind::Image,
            Some(IMAGE_PREVIEW_MAX_BYTES + 1)
        ));
        assert!(
            fits_preview(PreviewKind::Text, Some(u64::MAX)),
            "text reads its first 64 KB"
        );
        assert!(
            !fits_preview(PreviewKind::Image, None),
            "an unknown size is not fetched"
        );
        assert!(!fits_preview(PreviewKind::None, Some(1)));
    }

    /// A RIFF WAVE file of 16-bit PCM, 8-bit PCM or 32-bit float samples.
    fn wav(format: u16, channels: u16, rate: u32, bits: u16, data: &[u8]) -> Vec<u8> {
        let block = channels * bits / 8;
        let mut out = Vec::new();
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
        out.extend_from_slice(b"WAVE");
        out.extend_from_slice(b"fmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&format.to_le_bytes());
        out.extend_from_slice(&channels.to_le_bytes());
        out.extend_from_slice(&rate.to_le_bytes());
        out.extend_from_slice(&(rate * u32::from(block)).to_le_bytes());
        out.extend_from_slice(&block.to_le_bytes());
        out.extend_from_slice(&bits.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(data);
        out
    }

    /// azul plays raw f32 samples (`AudioSink`) and decodes no audio format;
    /// a WAV file IS raw samples, so its preview plays: the samples come out
    /// as f32 in -1..1, interleaved, with the rate and the channel count.
    #[test]
    fn a_wav_file_becomes_f32_samples_and_other_audio_is_refused() {
        let pcm16: Vec<u8> = [0i16, 16384, -32768, 32767]
            .iter()
            .flat_map(|s| s.to_le_bytes())
            .collect();
        let decoded = wav_samples(&wav(1, 2, 44_100, 16, &pcm16)).unwrap();
        assert_eq!((decoded.sample_rate, decoded.channels), (44_100, 2));
        assert_eq!(decoded.samples.len(), 4);
        assert_eq!(decoded.samples[0], 0.0);
        assert!((decoded.samples[1] - 0.5).abs() < 1e-4);
        assert_eq!(decoded.samples[2], -1.0);
        assert!((decoded.samples[3] - 1.0).abs() < 1e-4);
        assert!((decoded.seconds() - 2.0 / 44_100.0).abs() < 1e-6);

        let pcm8 = wav(1, 1, 8_000, 8, &[128, 255, 0]);
        let decoded = wav_samples(&pcm8).unwrap();
        assert_eq!(decoded.samples.len(), 3);
        assert_eq!(decoded.samples[0], 0.0, "8-bit PCM is unsigned around 128");
        assert_eq!(decoded.samples[2], -1.0);

        let float: Vec<u8> = [0.25f32, -0.5].iter().flat_map(|s| s.to_le_bytes()).collect();
        let decoded = wav_samples(&wav(3, 1, 48_000, 32, &float)).unwrap();
        assert_eq!(decoded.samples, vec![0.25, -0.5]);

        assert!(wav_samples(b"ID3\x03 an mp3").is_err());
        assert!(wav_samples(&wav(2, 1, 8_000, 4, &[0, 0])).is_err(), "ADPCM is not PCM");
        assert!(is_playable_audio("loop.WAV"));
        assert!(!is_playable_audio("song.mp3"));
    }

    /// A PDF previews its first page (azul's PDF reader turns pages into SVG,
    /// azul's SVG renderer draws it): fetched up to the image limit; audio
    /// still says why not.
    #[test]
    fn a_pdf_previews_its_first_page_and_audio_says_why_not() {
        assert!(fits_preview(PreviewKind::Pdf, Some(2 * 1024 * 1024)));
        assert!(!fits_preview(PreviewKind::Pdf, Some(IMAGE_PREVIEW_MAX_BYTES + 1)));
        assert!(!fits_preview(PreviewKind::Pdf, None));
        assert_eq!(no_preview_reason(PreviewKind::Pdf), None);
        assert!(no_preview_reason(PreviewKind::Audio).is_some());
        assert!(no_preview_reason(PreviewKind::None).is_some());
    }
}
