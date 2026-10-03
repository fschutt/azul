//! Finding the music: the audio files under the music folder. The tags of each are read by azul's
//! `AudioFileDecoder` on the scan's Thread (`lib.rs`); this is the plain part, tested without a
//! window.

use std::path::{Path, PathBuf};

use crate::library::is_audio_file;

/// How deep the scan goes into the music folder.
pub const MAX_DEPTH: usize = 12;

/// The audio files under `folder` (any depth up to [`MAX_DEPTH`]), sorted by path; hidden files
/// and folders (a leading dot) are skipped; unreadable folders are skipped silently.
#[must_use]
pub fn audio_files(folder: &Path) -> Vec<PathBuf> {
    let _ = folder;
    Vec::new()
}

/// The default music folder: `~/Music` (`%USERPROFILE%\Music` on Windows).
#[must_use]
pub fn default_music_folder() -> PathBuf {
    PathBuf::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_scan_finds_audio_files_at_any_depth_sorted_and_skips_hidden_ones() {
        let root = std::env::temp_dir().join(format!("azmusic-scan-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for p in [
            "b/02 Two.mp3",
            "a/01 One.flac",
            "a/cover.jpg",
            "a/deep/er/x.ogg",
            ".hidden/no.mp3",
            "a/.no.mp3",
        ] {
            let path = root.join(p);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, b"x").unwrap();
        }
        let found: Vec<String> = audio_files(&root)
            .iter()
            .map(|p| {
                p.strip_prefix(&root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect();
        assert_eq!(
            found,
            vec!["a/01 One.flac", "a/deep/er/x.ogg", "b/02 Two.mp3"]
        );
        assert!(audio_files(&root.join("missing")).is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_default_music_folder_is_music_in_the_home_folder() {
        let folder = default_music_folder();
        assert_eq!(folder.file_name().and_then(|n| n.to_str()), Some("Music"));
    }
}
