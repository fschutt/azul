//! The libraries: what the user's folders hold - music, pictures, videos, recorded TV - and the
//! groups Media Center shows them in (albums, artists, genres, songs; folders and months;
//! movies). The files are FOUND here and their tags read on a Thread (`scan.rs`); this part is
//! plain Rust, tested without a window.
//!
//! What was found is kept in ONE file of the data tree, `player/library.json` (the S3 split), so
//! the next start shows the libraries at once while a new scan runs.

use std::{
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

/// The library's file, in the app's folder of the data tree.
pub const LIBRARY_FILE: &str = "library.json";

/// What the music scan picks up (what azul's AudioFileDecoder decodes).
pub const AUDIO_EXTENSIONS: [&str; 11] = [
    "mp3", "m4a", "aac", "flac", "ogg", "oga", "opus", "wav", "aif", "aiff", "caf",
];
/// What the picture scan picks up (what `RawImage::decode_image_bytes_any` decodes).
pub const PICTURE_EXTENSIONS: [&str; 8] = ["jpg", "jpeg", "png", "gif", "bmp", "webp", "tif", "tiff"];
/// What the video scan picks up: what plays here (MP4 / MOV with H.264).
pub const VIDEO_EXTENSIONS: [&str; 3] = ["mp4", "m4v", "mov"];

/// How deep a scan goes into a folder.
pub const MAX_DEPTH: usize = 8;
/// The most files one library takes (the rest is not listed - said on screen).
pub const MAX_FILES: usize = 4000;
/// A video at least this long is a movie (the movie library).
pub const MOVIE_MIN_S: f64 = 40.0 * 60.0;
/// The folder of recorded TV inside the videos folder (Windows Media Center's name).
pub const RECORDED_TV: &str = "Recorded TV";

/// The libraries, as the start strip and the sections name them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Shelf {
    Music,
    Pictures,
    Videos,
    Tv,
}

impl Shelf {
    pub const ALL: [Shelf; 4] = [Shelf::Music, Shelf::Pictures, Shelf::Videos, Shelf::Tv];

    /// Whether `path` is a file of this library (by its extension, any case).
    #[must_use]
    pub fn takes(self, path: &Path) -> bool {
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
            return false;
        };
        let ext = ext.to_ascii_lowercase();
        match self {
            Shelf::Music => AUDIO_EXTENSIONS.contains(&ext.as_str()),
            Shelf::Pictures => PICTURE_EXTENSIONS.contains(&ext.as_str()),
            Shelf::Videos | Shelf::Tv => VIDEO_EXTENSIONS.contains(&ext.as_str()),
        }
    }

    /// The word on screen ("music", "pictures", "videos", "recorded tv").
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            Shelf::Music => "music",
            Shelf::Pictures => "pictures",
            Shelf::Videos => "videos",
            Shelf::Tv => "recorded tv",
        }
    }
}

/// One file of a library.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Item {
    /// Where it is (absolute).
    pub path: String,
    /// The tag's title, else the file's name without its extension.
    pub title: String,
    /// The name of the folder it is in.
    pub folder: String,
    pub size: u64,
    /// When it was last changed (seconds since 1970; 0 = not known).
    pub modified_s: u64,
    /// Its length (0 = not known): the tags of a song, the `mvhd` box of a video.
    pub duration_s: f64,
    /// A song's tags ("" / 0 for the rest).
    pub artist: String,
    pub album: String,
    pub album_artist: String,
    pub genre: String,
    pub year: String,
    pub track_no: u32,
    pub disc_no: u32,
    /// The file has a cover picture (a song's tag).
    pub has_cover: bool,
}

impl Item {
    /// An item for `path` with what the file system says (`title`: the file's name).
    #[must_use]
    pub fn from_path(path: &Path) -> Item {
        let meta = std::fs::metadata(path).ok();
        let modified_s = meta
            .as_ref()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_secs());
        Item {
            path: path.to_string_lossy().into_owned(),
            title: file_title(path),
            folder: path
                .parent()
                .and_then(Path::file_name)
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string(),
            size: meta.as_ref().map_or(0, std::fs::Metadata::len),
            modified_s,
            ..Item::default()
        }
    }

    /// The artist a song is filed under: the album's artist, else the song's, else "unknown
    /// artist".
    #[must_use]
    pub fn filed_artist(&self) -> String {
        [self.album_artist.trim(), self.artist.trim()]
            .into_iter()
            .find(|s| !s.is_empty())
            .unwrap_or("unknown artist")
            .to_string()
    }

    /// The album, or "unknown album".
    #[must_use]
    pub fn album_or_unknown(&self) -> String {
        let album = self.album.trim();
        if album.is_empty() {
            String::from("unknown album")
        } else {
            album.to_string()
        }
    }
}

/// A file's name without its folder and extension.
#[must_use]
pub fn file_title(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string()
}

/// The files of `shelf` under `folder` (any depth up to [`MAX_DEPTH`], at most [`MAX_FILES`]),
/// sorted by path; hidden files and folders (a leading dot) are skipped, links are not followed,
/// unreadable folders are skipped silently, and `skip` (a folder another library reads, the
/// recorded TV inside the videos folder) is left out. The `bool` says whether the list was cut
/// at [`MAX_FILES`].
#[must_use]
pub fn find_files(shelf: Shelf, folder: &Path, skip: Option<&Path>) -> (Vec<PathBuf>, bool) {
    fn walk(
        shelf: Shelf,
        dir: &Path,
        skip: Option<&Path>,
        depth: usize,
        out: &mut Vec<PathBuf>,
    ) -> bool {
        if depth > MAX_DEPTH {
            return false;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            return false;
        };
        let mut entries: Vec<std::fs::DirEntry> = entries.flatten().collect();
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries {
            if entry.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            let path = entry.path();
            match entry.file_type() {
                Ok(t) if t.is_dir() => {
                    if skip.is_some_and(|s| s == path.as_path()) {
                        continue;
                    }
                    if walk(shelf, &path, skip, depth + 1, out) {
                        return true;
                    }
                }
                Ok(t) if t.is_file() && shelf.takes(&path) => {
                    if out.len() >= MAX_FILES {
                        return true;
                    }
                    out.push(path);
                }
                _ => {}
            }
        }
        false
    }
    let mut out = Vec::new();
    let cut = walk(shelf, folder, skip, 0, &mut out);
    out.sort();
    (out, cut)
}

/// The length of an MP4 / MOV file from its `moov/mvhd` box, read where it is (the boxes before
/// it are skipped, never read: a movie's `mdat` is gigabytes). `None` when the file has none.
#[must_use]
pub fn mp4_duration_s(path: &Path) -> Option<f64> {
    let mut file = std::fs::File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    mp4_duration_in(&mut file, len)
}

/// [`mp4_duration_s`] over any seekable source of `len` bytes.
pub fn mp4_duration_in<R: Read + Seek>(r: &mut R, len: u64) -> Option<f64> {
    /// The next box at `at` (below `end`): its type, where its body starts, where it ends.
    fn next_box<R: Read + Seek>(r: &mut R, at: u64, end: u64) -> Option<([u8; 4], u64, u64)> {
        if at.checked_add(8)? > end {
            return None;
        }
        r.seek(SeekFrom::Start(at)).ok()?;
        let mut head = [0u8; 8];
        r.read_exact(&mut head).ok()?;
        let size = u64::from(u32::from_be_bytes([head[0], head[1], head[2], head[3]]));
        let kind = [head[4], head[5], head[6], head[7]];
        let (body, box_end) = match size {
            // To the end of the enclosing box.
            0 => (at + 8, end),
            // A 64-bit size follows the type.
            1 => {
                let mut big = [0u8; 8];
                r.read_exact(&mut big).ok()?;
                (at + 16, at.checked_add(u64::from_be_bytes(big))?)
            }
            n if n < 8 => return None,
            n => (at + 8, at.checked_add(n)?),
        };
        if box_end > end || body > box_end {
            return None;
        }
        Some((kind, body, box_end))
    }
    // The top-level boxes, then `moov`'s children; a few dozen at most in a real file.
    let mut at = 0u64;
    for _ in 0..256 {
        let (kind, body, box_end) = next_box(r, at, len)?;
        if &kind == b"moov" {
            let mut child = body;
            for _ in 0..256 {
                let (kind, body, child_end) = next_box(r, child, box_end)?;
                if &kind == b"mvhd" {
                    r.seek(SeekFrom::Start(body)).ok()?;
                    let mut version = [0u8; 4];
                    r.read_exact(&mut version).ok()?;
                    let (timescale, duration) = if version[0] == 1 {
                        let mut b = [0u8; 28];
                        r.read_exact(&mut b).ok()?;
                        let timescale = u32::from_be_bytes([b[16], b[17], b[18], b[19]]);
                        let mut d = [0u8; 8];
                        d.copy_from_slice(&b[20..28]);
                        (timescale, u64::from_be_bytes(d))
                    } else {
                        let mut b = [0u8; 16];
                        r.read_exact(&mut b).ok()?;
                        let timescale = u32::from_be_bytes([b[8], b[9], b[10], b[11]]);
                        let duration = u32::from_be_bytes([b[12], b[13], b[14], b[15]]);
                        (timescale, u64::from(duration))
                    };
                    if timescale == 0 {
                        return None;
                    }
                    #[allow(clippy::cast_precision_loss)]
                    return Some(duration as f64 / f64::from(timescale));
                }
                child = child_end;
            }
            return None;
        }
        at = box_end;
    }
    None
}

/// A tile of a gallery: one item, or a group of them (an album, an artist, a folder, a month).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Group {
    /// The line on the tile.
    pub title: String,
    /// The line under it (the artist of an album, how many songs, ...).
    pub subtitle: String,
    /// Indices into the library's items, in play order.
    pub items: Vec<usize>,
}

/// Groups `items` by `key` (case-insensitive), the groups in the key's order; inside a group
/// the items keep their order.
fn group_by(
    items: &[Item],
    picked: impl Iterator<Item = usize>,
    key: impl Fn(&Item) -> String,
) -> Vec<(String, Vec<usize>)> {
    let mut groups: std::collections::BTreeMap<String, (String, Vec<usize>)> =
        std::collections::BTreeMap::new();
    for i in picked {
        let Some(item) = items.get(i) else {
            continue;
        };
        let k = key(item);
        groups
            .entry(k.to_lowercase())
            .or_insert_with(|| (k.clone(), Vec::new()))
            .1
            .push(i);
    }
    groups.into_values().collect()
}

/// "1 song", "12 songs".
#[must_use]
pub fn count(n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {many}")
    }
}

/// The albums of a music library: by (filed artist, album), the songs in disc and track order.
#[must_use]
pub fn albums(items: &[Item]) -> Vec<Group> {
    let mut out: Vec<Group> = group_by(items, 0..items.len(), |i| {
        format!("{}\u{1}{}", i.album_or_unknown(), i.filed_artist())
    })
    .into_iter()
    .map(|(key, mut songs)| {
        songs.sort_by(|a, b| {
            let (x, y) = (&items[*a], &items[*b]);
            (x.disc_no, x.track_no, x.title.to_lowercase()).cmp(&(
                y.disc_no,
                y.track_no,
                y.title.to_lowercase(),
            ))
        });
        let mut parts = key.split('\u{1}');
        let title = parts.next().unwrap_or("").to_string();
        let artist = parts.next().unwrap_or("").to_string();
        Group {
            title,
            subtitle: artist,
            items: songs,
        }
    })
    .collect();
    out.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
    out
}

/// The artists of a music library, each with its songs (album by album).
#[must_use]
pub fn artists(items: &[Item]) -> Vec<Group> {
    group_by(items, songs_in_album_order(items).into_iter(), Item::filed_artist)
        .into_iter()
        .map(|(artist, songs)| Group {
            subtitle: count(songs.len(), "song", "songs"),
            title: artist,
            items: songs,
        })
        .collect()
}

/// The genres of a music library ("unknown genre" for songs without one).
#[must_use]
pub fn genres(items: &[Item]) -> Vec<Group> {
    group_by(items, songs_in_album_order(items).into_iter(), |i| {
        let g = i.genre.trim();
        if g.is_empty() {
            String::from("unknown genre")
        } else {
            g.to_string()
        }
    })
    .into_iter()
    .map(|(genre, songs)| Group {
        subtitle: count(songs.len(), "song", "songs"),
        title: genre,
        items: songs,
    })
    .collect()
}

/// Every song, album by album (the order "play all" plays them in when not shuffled).
#[must_use]
pub fn songs_in_album_order(items: &[Item]) -> Vec<usize> {
    albums(items).into_iter().flat_map(|a| a.items).collect()
}

/// Every item, by title (case-insensitive), then by path.
#[must_use]
pub fn by_title(items: &[Item]) -> Vec<usize> {
    let mut out: Vec<usize> = (0..items.len()).collect();
    out.sort_by(|a, b| {
        (items[*a].title.to_lowercase(), &items[*a].path)
            .cmp(&(items[*b].title.to_lowercase(), &items[*b].path))
    });
    out
}

/// The items of a picture or video library by the folder they are in.
#[must_use]
pub fn folders(items: &[Item]) -> Vec<Group> {
    group_by(items, by_title(items).into_iter(), |i| {
        if i.folder.is_empty() {
            String::from("(no folder)")
        } else {
            i.folder.clone()
        }
    })
    .into_iter()
    .map(|(folder, files)| Group {
        subtitle: count(files.len(), "item", "items"),
        title: folder,
        items: files,
    })
    .collect()
}

/// The month names, for [`months`].
const MONTHS: [&str; 12] = [
    "january",
    "february",
    "march",
    "april",
    "may",
    "june",
    "july",
    "august",
    "september",
    "october",
    "november",
    "december",
];

/// The (year, month 1-12) of `seconds` since 1970 (UTC; the civil calendar).
#[must_use]
pub fn year_month(seconds: u64) -> (i64, u32) {
    // Howard Hinnant's days-to-civil.
    #[allow(clippy::cast_possible_wrap)]
    let z = (seconds / 86_400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let month = u32::try_from(if mp < 10 { mp + 3 } else { mp - 9 }).unwrap_or(1);
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month)
}

/// The items of a picture or video library by the month they were last changed in, the newest
/// month first ("october 2026"); items without a date last.
#[must_use]
pub fn months(items: &[Item]) -> Vec<Group> {
    let mut groups: std::collections::BTreeMap<(i64, u32), Vec<usize>> =
        std::collections::BTreeMap::new();
    let mut undated = Vec::new();
    let mut order: Vec<usize> = (0..items.len()).collect();
    order.sort_by_key(|i| std::cmp::Reverse(items[*i].modified_s));
    for i in order {
        if items[i].modified_s == 0 {
            undated.push(i);
        } else {
            groups
                .entry(year_month(items[i].modified_s))
                .or_default()
                .push(i);
        }
    }
    let mut out: Vec<Group> = groups
        .into_iter()
        .rev()
        .map(|((year, month), files)| Group {
            title: format!("{} {year}", MONTHS[(month as usize).saturating_sub(1).min(11)]),
            subtitle: count(files.len(), "item", "items"),
            items: files,
        })
        .collect();
    if !undated.is_empty() {
        out.push(Group {
            title: String::from("no date"),
            subtitle: count(undated.len(), "item", "items"),
            items: undated,
        });
    }
    out
}

/// The movies of a video library: the videos at least [`MOVIE_MIN_S`] long, by title.
#[must_use]
pub fn movies(items: &[Item]) -> Vec<usize> {
    by_title(items)
        .into_iter()
        .filter(|i| items[*i].duration_s >= MOVIE_MIN_S)
        .collect()
}

/// Whether `item` matches the search `query` (every word, any case, in its title, artist, album,
/// genre or folder).
#[must_use]
pub fn matches(item: &Item, query: &str) -> bool {
    let text = format!(
        "{} {} {} {} {} {}",
        item.title, item.artist, item.album_artist, item.album, item.genre, item.folder
    )
    .to_lowercase();
    let words: Vec<String> = query
        .split_whitespace()
        .map(str::to_lowercase)
        .collect();
    !words.is_empty() && words.iter().all(|w| text.contains(w.as_str()))
}

/// How a library stands.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Status {
    /// Not looked at yet (or the file came from the last start).
    #[default]
    Unknown,
    /// A scan is running.
    Scanning,
    /// Scanned: the items are what the folder holds.
    Ready,
    /// The folder is not there.
    Missing,
}

/// One library: its folder, its items, how it stands.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Shelved {
    pub folder: String,
    pub items: Vec<Item>,
    #[serde(skip)]
    pub status: Status,
    /// The scan stopped at [`MAX_FILES`].
    #[serde(skip)]
    pub cut: bool,
}

/// The libraries file: every library's items (the last scan's).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Library {
    pub version: u32,
    pub music: Shelved,
    pub pictures: Shelved,
    pub videos: Shelved,
    pub tv: Shelved,
}

impl Library {
    /// The library from its file's text, or why not.
    pub fn from_json(text: &str) -> Result<Library, String> {
        serde_json::from_str(text).map_err(|e| format!("the library file does not read: {e}"))
    }

    /// The library as its file's text.
    #[must_use]
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    #[must_use]
    pub fn shelf(&self, shelf: Shelf) -> &Shelved {
        match shelf {
            Shelf::Music => &self.music,
            Shelf::Pictures => &self.pictures,
            Shelf::Videos => &self.videos,
            Shelf::Tv => &self.tv,
        }
    }

    pub fn shelf_mut(&mut self, shelf: Shelf) -> &mut Shelved {
        match shelf {
            Shelf::Music => &mut self.music,
            Shelf::Pictures => &mut self.pictures,
            Shelf::Videos => &mut self.videos,
            Shelf::Tv => &mut self.tv,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song(title: &str, artist: &str, album: &str, track: u32) -> Item {
        Item {
            path: format!("/m/{artist}/{album}/{title}.mp3"),
            title: title.into(),
            artist: artist.into(),
            album: album.into(),
            track_no: track,
            ..Item::default()
        }
    }

    #[test]
    fn the_scan_finds_a_librarys_files_sorted_skipping_hidden_ones_and_the_skipped_folder() {
        let root = std::env::temp_dir().join(format!("azplayer-scan-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for p in [
            "b/02 Two.mp4",
            "a/01 One.MOV",
            "a/cover.jpg",
            "a/deep/er/x.m4v",
            ".hidden/no.mp4",
            "a/.no.mp4",
            "Recorded TV/show.mp4",
            "notes.txt",
        ] {
            let path = root.join(p);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, b"x").unwrap();
        }
        let tv = root.join(RECORDED_TV);
        let (found, cut) = find_files(Shelf::Videos, &root, Some(&tv));
        let names: Vec<String> = found
            .iter()
            .map(|p| p.strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/"))
            .collect();
        assert_eq!(names, vec!["a/01 One.MOV", "a/deep/er/x.m4v", "b/02 Two.mp4"]);
        assert!(!cut);
        let (pictures, _) = find_files(Shelf::Pictures, &root, None);
        assert_eq!(pictures.len(), 1);
        let (tv_found, _) = find_files(Shelf::Tv, &tv, None);
        assert_eq!(tv_found.len(), 1);
        assert!(find_files(Shelf::Music, &root.join("missing"), None).0.is_empty());
        let item = Item::from_path(&found[0]);
        assert_eq!(item.title, "01 One");
        assert_eq!(item.folder, "a");
        assert_eq!(item.size, 1);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A box: its size, its type, its body.
    fn mp4_box(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut out = u32::try_from(8 + body.len()).unwrap().to_be_bytes().to_vec();
        out.extend_from_slice(kind);
        out.extend_from_slice(body);
        out
    }

    #[test]
    fn a_videos_length_is_read_from_its_mvhd_box_past_a_big_media_box() {
        // mvhd version 0: version+flags, creation, modification, timescale 600, duration 3000.
        let mut mvhd = vec![0, 0, 0, 0];
        mvhd.extend_from_slice(&[0; 8]);
        mvhd.extend_from_slice(&600u32.to_be_bytes());
        mvhd.extend_from_slice(&3000u32.to_be_bytes());
        mvhd.extend_from_slice(&[0; 80]);
        let mut file = mp4_box(b"ftyp", b"isom\0\0\0\0");
        // An mdat with a 64-bit size before the moov (a "fast start" file has it after).
        file.extend_from_slice(&1u32.to_be_bytes());
        file.extend_from_slice(b"mdat");
        file.extend_from_slice(&(16u64 + 64).to_be_bytes());
        file.extend_from_slice(&[7; 64]);
        file.extend(mp4_box(b"moov", &[mp4_box(b"trak", &[1; 12]), mp4_box(b"mvhd", &mvhd)].concat()));
        let len = file.len() as u64;
        let seconds = mp4_duration_in(&mut std::io::Cursor::new(file), len).expect("a length");
        assert!((seconds - 5.0).abs() < 1e-9, "{seconds}");

        // Version 1: 64-bit times.
        let mut mvhd1 = vec![1, 0, 0, 0];
        mvhd1.extend_from_slice(&[0; 16]);
        mvhd1.extend_from_slice(&1000u32.to_be_bytes());
        mvhd1.extend_from_slice(&7_200_000u64.to_be_bytes());
        mvhd1.extend_from_slice(&[0; 80]);
        let file1 = mp4_box(b"moov", &mp4_box(b"mvhd", &mvhd1));
        let len1 = file1.len() as u64;
        let seconds = mp4_duration_in(&mut std::io::Cursor::new(file1), len1).expect("a length");
        assert!((seconds - 7200.0).abs() < 1e-9, "{seconds}");

        // Not an MP4: no length, no panic.
        let junk = b"this is no mp4 file at all".to_vec();
        let len = junk.len() as u64;
        assert_eq!(mp4_duration_in(&mut std::io::Cursor::new(junk), len), None);
        assert_eq!(mp4_duration_in(&mut std::io::Cursor::new(Vec::new()), 0), None);
    }

    #[test]
    fn songs_group_into_albums_artists_and_genres_in_track_order() {
        let mut items = vec![
            song("Second", "Band", "Record", 2),
            song("First", "Band", "Record", 1),
            song("Alone", "Solo", "", 0),
        ];
        items[0].genre = "Rock".into();
        items[1].genre = "rock".into();
        let a = albums(&items);
        assert_eq!(a.len(), 2);
        assert_eq!(a[0].title, "Record");
        assert_eq!(a[0].subtitle, "Band");
        assert_eq!(a[0].items, vec![1, 0], "track order");
        assert_eq!(a[1].title, "unknown album");
        let ar = artists(&items);
        assert_eq!(ar.iter().map(|g| g.title.as_str()).collect::<Vec<_>>(), ["Band", "Solo"]);
        assert_eq!(ar[0].subtitle, "2 songs");
        let g = genres(&items);
        assert_eq!(g.len(), 2, "genres match in any case");
        assert_eq!(g[0].items.len(), 2);
        assert_eq!(g[1].title, "unknown genre");
        assert_eq!(songs_in_album_order(&items), vec![1, 0, 2]);
        assert_eq!(by_title(&items), vec![2, 1, 0]);
    }

    #[test]
    fn pictures_group_by_folder_and_by_month_newest_first() {
        let mut items = vec![Item::default(), Item::default(), Item::default()];
        items[0].title = "b".into();
        items[0].folder = "Trip".into();
        items[0].modified_s = 1_759_276_800; // 2025-10-01
        items[1].title = "a".into();
        items[1].folder = "Trip".into();
        items[1].modified_s = 1_704_067_200; // 2024-01-01
        items[2].title = "c".into();
        let f = folders(&items);
        assert_eq!(f[0].title, "(no folder)");
        assert_eq!(f[1].title, "Trip");
        assert_eq!(f[1].items, vec![1, 0], "by title inside a folder");
        let m = months(&items);
        assert_eq!(m[0].title, "october 2025");
        assert_eq!(m[1].title, "january 2024");
        assert_eq!(m[2].title, "no date");
        assert_eq!(year_month(0), (1970, 1));
        assert_eq!(year_month(951_782_400), (2000, 2)); // 2000-02-29
    }

    #[test]
    fn a_movie_is_a_long_video_and_a_search_needs_every_word() {
        let mut items = vec![Item::default(), Item::default()];
        items[0].title = "Short clip".into();
        items[0].duration_s = 120.0;
        items[1].title = "The Long Film".into();
        items[1].duration_s = MOVIE_MIN_S + 1.0;
        assert_eq!(movies(&items), vec![1]);
        assert!(matches(&items[1], "long FILM"));
        assert!(!matches(&items[1], "long clip"));
        assert!(!matches(&items[1], "   "), "an empty query finds nothing");
        assert_eq!(count(1, "song", "songs"), "1 song");
        assert_eq!(count(3, "song", "songs"), "3 songs");
    }

    #[test]
    fn the_library_file_round_trips_without_the_scan_state() {
        let mut lib = Library::default();
        lib.music.folder = "/m".into();
        lib.music.items.push(song("One", "A", "B", 1));
        lib.music.status = Status::Ready;
        let back = Library::from_json(&lib.to_json()).expect("reads");
        assert_eq!(back.music.items, lib.music.items);
        assert_eq!(back.music.status, Status::Unknown, "a scan state is not saved");
        assert!(Library::from_json("nope").is_err());
        assert_eq!(lib.shelf(Shelf::Music).items.len(), 1);
        assert!(Shelf::Music.takes(Path::new("/a/b.FLAC")));
        assert!(!Shelf::Videos.takes(Path::new("/a/b.mkv")), "only what plays here");
    }
}
