//! What the preview pane can show for a file: an image (decoded by azul), the
//! first 64 KB of a text or code file, a video (azul's video widget), or why
//! it cannot (audio and PDF have no renderer in azul yet). No azul types.

use crate::browse;

/// What the preview pane can do with a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PreviewKind {
    /// Decoded by azul (`RawImage::decode_image_bytes_any`).
    Image,
    /// The first [`TEXT_PREVIEW_BYTES`] as text.
    Text,
    /// No PDF renderer in azul's public API: the pane says so.
    Pdf,
    /// azul's video widget (H.264 MP4; a cloud file is fetched first).
    Video,
    /// azul plays raw PCM only, no decoder: the pane says so.
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
/// (its first 64 KB), an image or a video up to its limit, an unknown size
/// never; PDF, audio and the rest have nothing to fetch.
#[must_use]
pub fn fits_preview(kind: PreviewKind, size: Option<u64>) -> bool {
    match kind {
        PreviewKind::Text => true,
        PreviewKind::Image => size.is_some_and(|s| s <= IMAGE_PREVIEW_MAX_BYTES),
        PreviewKind::Video => size.is_some_and(|s| s <= VIDEO_PREVIEW_MAX_BYTES),
        PreviewKind::Pdf | PreviewKind::Audio | PreviewKind::None => false,
    }
}

/// The text of a preview: `bytes` decoded (invalid UTF-8 replaced); a cut
/// file (`truncated`) ends at its last complete line and says it is the
/// start. A NUL byte in the first 8 KB means a binary file: `Err`.
pub fn text_preview(bytes: &[u8], truncated: bool) -> Result<String, &'static str> {
    let probe = &bytes[..bytes.len().min(8192)];
    if probe.contains(&0) {
        return Err("a binary file");
    }
    let mut text = String::from_utf8_lossy(bytes).into_owned();
    if truncated {
        if let Some(end) = text.rfind('\n') {
            text.truncate(end + 1);
        }
        text.push_str("\n[... the first 64 KB of the file]");
    }
    Ok(text)
}

/// Why the pane shows no preview for a kind (a sentence), or `None`.
#[must_use]
pub fn no_preview_reason(kind: PreviewKind) -> Option<&'static str> {
    match kind {
        PreviewKind::Pdf => Some(
            "No preview: azul's public API cannot draw a PDF page yet (its PDF reader turns pages \
             into SVG inside the library only).",
        ),
        PreviewKind::Audio => {
            Some("No preview: azul plays raw PCM samples and has no audio decoder for this format.")
        }
        PreviewKind::None => Some("No preview available."),
        PreviewKind::Image | PreviewKind::Text | PreviewKind::Video => None,
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
        assert!(cut.contains("64 KB"), "says it is the start: {cut}");
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
