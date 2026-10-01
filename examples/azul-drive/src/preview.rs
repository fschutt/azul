//! What the preview pane can show for a file: an image (decoded by azul), the
//! first 64 KB of a text or code file, a video (azul's video widget), or why
//! it cannot (audio and PDF have no renderer in azul yet). No azul types.

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
}
