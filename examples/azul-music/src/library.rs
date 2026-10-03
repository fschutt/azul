//! The music library: the tracks of the music folder with their tags, grouped into albums and
//! artists, searched. Plain Rust, tested without a window.
//!
//! The library is ONE file in the data tree, `music/library.json` (the S3 split: durable data are
//! files); the music itself stays where the user keeps it (the music folder) and is read in place.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// The file extensions the library scan picks up (what azul's AudioFileDecoder decodes).
pub const AUDIO_EXTENSIONS: [&str; 13] = [
    "mp3", "m4a", "aac", "mp4", "flac", "ogg", "oga", "opus", "wav", "aif", "aiff", "mka", "caf",
];

/// The library's file, in the app's folder of the data tree.
pub const LIBRARY_FILE: &str = "library.json";

/// Whether `path` names a file the scan reads (by its extension, any case).
#[must_use]
pub fn is_audio_file(path: &Path) -> bool {
    let _ = path;
    false
}

/// One track of the library.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Track {
    /// A stable id (a UUID), kept across rescans of the same file.
    pub id: String,
    /// Where the file is (absolute).
    pub path: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_artist: String,
    pub genre: String,
    pub year: String,
    pub track_no: u32,
    pub disc_no: u32,
    pub duration_s: f64,
}

impl Track {
    /// The title, or the file's name without its extension when the file has none.
    #[must_use]
    pub fn display_title(&self) -> String {
        String::new()
    }

    /// The artist the track is filed under: the album's artist, else the track's, else
    /// "Unknown artist".
    #[must_use]
    pub fn filed_artist(&self) -> String {
        String::new()
    }
}

/// An album: tracks grouped by (filed artist, album title), in disc and track order.
#[derive(Debug, Clone, PartialEq)]
pub struct Album {
    pub title: String,
    pub artist: String,
    pub year: String,
    /// Indices into `Library::tracks`, in play order.
    pub tracks: Vec<usize>,
    pub duration_s: f64,
}

/// The library file.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Library {
    /// The file format's version.
    pub version: u32,
    /// The music folder the tracks were scanned from.
    pub folder: String,
    pub tracks: Vec<Track>,
}

impl Library {
    /// The library from its file's text, or why not.
    pub fn from_json(text: &str) -> Result<Library, String> {
        let _ = text;
        Err(String::from("not built yet"))
    }

    /// The library as its file's text.
    #[must_use]
    pub fn to_json(&self) -> String {
        String::new()
    }

    /// The albums, by artist then title ("Unknown album" for tracks without one).
    #[must_use]
    pub fn albums(&self) -> Vec<Album> {
        Vec::new()
    }

    /// The artists (filed artist) with their track counts, by name.
    #[must_use]
    pub fn artists(&self) -> Vec<(String, usize)> {
        Vec::new()
    }

    /// The tracks whose title, artist, album or genre contain every word of `query` (any case),
    /// in library order; all of them for an empty query.
    #[must_use]
    pub fn search(&self, query: &str) -> Vec<usize> {
        let _ = query;
        Vec::new()
    }

    /// The track with `id`.
    #[must_use]
    pub fn index_of(&self, id: &str) -> Option<usize> {
        let _ = id;
        None
    }

    /// Takes in the tracks of a scan: a file already in the library keeps its id (and so its
    /// place in playlists); files no longer found are dropped. Returns (added, removed).
    pub fn merge_scan(
        &mut self,
        scanned: Vec<Track>,
        new_id: impl FnMut() -> String,
    ) -> (usize, usize) {
        let _ = (scanned, new_id);
        (0, 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(title: &str, artist: &str, album: &str, no: u32, seconds: f64) -> Track {
        Track {
            id: format!("id-{title}"),
            path: format!("/music/{artist}/{album}/{no:02} {title}.flac"),
            title: title.into(),
            artist: artist.into(),
            album: album.into(),
            track_no: no,
            disc_no: 1,
            duration_s: seconds,
            ..Track::default()
        }
    }

    fn sample() -> Library {
        Library {
            version: 1,
            folder: "/music".into(),
            tracks: vec![
                track("Harbour Walk", "Northlight Quartet", "Blue Hour", 2, 337.0),
                track("First Light", "Northlight Quartet", "Blue Hour", 1, 562.0),
                track("Low Tide", "Ada Park", "Field Notes", 1, 241.0),
                track("Untitled", "Ada Park", "", 0, 60.0),
            ],
        }
    }

    #[test]
    fn the_scan_reads_audio_files_by_extension_in_any_case() {
        assert!(is_audio_file(Path::new("/m/a.mp3")));
        assert!(is_audio_file(Path::new("/m/b.FLAC")));
        assert!(is_audio_file(Path::new("/m/c.m4a")));
        assert!(!is_audio_file(Path::new("/m/cover.jpg")));
        assert!(!is_audio_file(Path::new("/m/no-extension")));
    }

    #[test]
    fn a_track_without_a_title_is_called_by_its_file_name() {
        let mut t = track("", "X", "Y", 1, 1.0);
        t.path = "/music/X/Y/07 Some Song.mp3".into();
        assert_eq!(t.display_title(), "07 Some Song");
        assert_eq!(track("Real", "X", "Y", 1, 1.0).display_title(), "Real");
    }

    #[test]
    fn a_track_is_filed_under_its_album_artist_else_its_artist() {
        let mut t = track("A", "Guest", "Comp", 1, 1.0);
        assert_eq!(t.filed_artist(), "Guest");
        t.album_artist = "Various Artists".into();
        assert_eq!(t.filed_artist(), "Various Artists");
        t.album_artist.clear();
        t.artist.clear();
        assert_eq!(t.filed_artist(), "Unknown artist");
    }

    #[test]
    fn albums_group_by_artist_and_title_in_track_order() {
        let lib = sample();
        let albums = lib.albums();
        let names: Vec<(&str, &str)> = albums
            .iter()
            .map(|a| (a.artist.as_str(), a.title.as_str()))
            .collect();
        assert_eq!(
            names,
            vec![
                ("Ada Park", "Field Notes"),
                ("Ada Park", "Unknown album"),
                ("Northlight Quartet", "Blue Hour"),
            ]
        );
        let blue = &albums[2];
        let titles: Vec<&str> = blue
            .tracks
            .iter()
            .map(|i| lib.tracks[*i].title.as_str())
            .collect();
        assert_eq!(
            titles,
            vec!["First Light", "Harbour Walk"],
            "track 1 before track 2"
        );
        assert!((blue.duration_s - 899.0).abs() < 1e-9);
    }

    #[test]
    fn artists_are_counted_by_name() {
        assert_eq!(
            sample().artists(),
            vec![
                ("Ada Park".to_string(), 2),
                ("Northlight Quartet".to_string(), 2)
            ]
        );
    }

    #[test]
    fn search_matches_every_word_anywhere_in_any_case() {
        let lib = sample();
        assert_eq!(lib.search("").len(), 4);
        assert_eq!(lib.search("light"), vec![1]);
        assert_eq!(lib.search("ada TIDE"), vec![2]);
        assert_eq!(lib.search("blue northlight"), vec![0, 1]);
        assert!(lib.search("nothing like this").is_empty());
    }

    #[test]
    fn the_library_file_round_trips_and_says_why_it_does_not_read() {
        let lib = sample();
        let back = Library::from_json(&lib.to_json()).expect("reads back");
        assert_eq!(back, lib);
        assert!(Library::from_json("{ not json").is_err());
        // Fields a newer build adds, or an older one lacks, do not break the file.
        let old = Library::from_json(r#"{"tracks":[{"id":"a","path":"/x.mp3"}]}"#).expect("reads");
        assert_eq!(old.tracks[0].title, "");
    }

    #[test]
    fn a_rescan_keeps_the_ids_of_known_files_and_drops_the_missing() {
        let mut lib = sample();
        let kept = lib.tracks[0].clone();
        let mut again = kept.clone();
        again.id = String::new();
        again.title = "Harbour Walk (remaster)".into();
        let fresh = Track {
            path: "/music/new.mp3".into(),
            ..Track::default()
        };
        let mut n = 0;
        let (added, removed) = lib.merge_scan(vec![again, fresh], || {
            n += 1;
            format!("new-{n}")
        });
        assert_eq!((added, removed), (1, 3));
        assert_eq!(lib.tracks.len(), 2);
        let k = lib.index_of(&kept.id).expect("the known file keeps its id");
        assert_eq!(
            lib.tracks[k].title, "Harbour Walk (remaster)",
            "its tags are the new scan's"
        );
        assert!(lib.index_of("new-1").is_some());
    }
}
