//! The library: every book is a folder of files in the data tree (user ruling, the S3 split),
//! written through azul-storage's `Drive` from an azul `Thread`:
//!
//! ```text
//! reader/books/<uuid>/book.epub    the imported file as it was (book.txt / book.html)
//! reader/books/<uuid>/info.json    what the library shows: title, authors, format, added
//! reader/books/<uuid>/state.json   where the reader is: position, bookmarks, last read
//! reader/books/<uuid>/cover.png    the cover, small (when the book has one)
//! ```
//!
//! The big file is written once; `state.json` is small and written on every page turn that
//! matters - so a later sync moves the small file.

use serde::{Deserialize, Serialize};

use crate::position::Position;

/// AzReader's folder in the data root.
pub const APP_FOLDER: &str = "reader";
/// The books' folder.
pub const BOOKS: &str = "reader/books/";

/// A book's file format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    Epub,
    Text,
    Html,
}

impl Format {
    /// The extension of the book's file in its folder.
    #[must_use]
    pub const fn extension(self) -> &'static str {
        match self {
            Format::Epub => "epub",
            Format::Text => "txt",
            Format::Html => "html",
        }
    }

    /// The format a file name says (`.epub`, `.txt` / `.text`, `.html` / `.htm` / `.xhtml`).
    #[must_use]
    pub fn of_name(name: &str) -> Option<Format> {
        let lower = name.to_ascii_lowercase();
        let ext = lower.rsplit_once('.').map(|(_, e)| e)?;
        match ext {
            "epub" => Some(Format::Epub),
            "txt" | "text" => Some(Format::Text),
            "html" | "htm" | "xhtml" => Some(Format::Html),
            _ => None,
        }
    }

    /// The format of `bytes` when the name says nothing: a zip is an EPUB, text that starts
    /// with markup is HTML, other UTF-8 text is text.
    #[must_use]
    pub fn sniff(bytes: &[u8]) -> Option<Format> {
        if bytes.starts_with(b"PK\x03\x04") {
            return Some(Format::Epub);
        }
        let head = String::from_utf8_lossy(&bytes[..bytes.len().min(512)]).to_ascii_lowercase();
        let head = head.trim_start_matches('\u{feff}').trim_start();
        if head.starts_with("<!doctype html")
            || head.starts_with("<html")
            || head.starts_with("<?xml")
        {
            return Some(Format::Html);
        }
        std::str::from_utf8(&bytes[..bytes.len().min(4096)])
            .ok()
            .or_else(|| (bytes.len() > 4096).then_some(""))
            .map(|_| Format::Text)
    }
}

/// What the library knows about a book (`info.json`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BookInfo {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub language: String,
    pub format: Format,
    /// The name of the file it was imported from.
    #[serde(default)]
    pub original_name: String,
    /// When it was added (seconds since 1970).
    #[serde(default)]
    pub added: u64,
    /// The book file's size in bytes.
    #[serde(default)]
    pub size: u64,
    /// A `cover.png` is in the book's folder.
    #[serde(default)]
    pub has_cover: bool,
}

impl BookInfo {
    /// The authors as one line ("" for none).
    #[must_use]
    pub fn author_line(&self) -> String {
        self.authors.join(", ")
    }
}

/// A bookmark.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bookmark {
    pub id: String,
    pub position: Position,
    /// The text at the bookmark (an excerpt).
    pub label: String,
    /// The chapter's title.
    #[serde(default)]
    pub chapter_title: String,
    /// When it was made (seconds since 1970).
    #[serde(default)]
    pub created: u64,
}

/// Where the reader is in a book (`state.json`).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct BookState {
    #[serde(default)]
    pub position: Position,
    #[serde(default)]
    pub bookmarks: Vec<Bookmark>,
    /// When the book was last read (seconds since 1970; 0 = never opened).
    #[serde(default)]
    pub last_read: u64,
    /// The book's progress at the position (`0..=1`), for the library.
    #[serde(default)]
    pub progress: f32,
    /// The reader reached the last page.
    #[serde(default)]
    pub finished: bool,
}

/// Two positions this close in one chapter are one place (one page holds both).
const SAME_PLACE: f32 = 1e-4;

impl BookState {
    /// The bookmark at `position`, if there is one.
    #[must_use]
    pub fn bookmark_at(&self, position: Position) -> Option<&Bookmark> {
        self.bookmarks.iter().find(|b| {
            b.position.chapter == position.chapter
                && (b.position.fraction - position.fraction).abs() < SAME_PLACE
        })
    }

    /// Adds `bookmark`, or - when one is at its position already - removes that one (the
    /// bookmark button toggles). `true` = added. The bookmarks stay in reading order.
    pub fn toggle_bookmark(&mut self, bookmark: Bookmark) -> bool {
        if let Some(at) = self.bookmarks.iter().position(|b| {
            b.position.chapter == bookmark.position.chapter
                && (b.position.fraction - bookmark.position.fraction).abs() < SAME_PLACE
        }) {
            self.bookmarks.remove(at);
            return false;
        }
        self.bookmarks.push(bookmark);
        self.bookmarks.sort_by(|a, b| {
            a.position
                .chapter
                .cmp(&b.position.chapter)
                .then(a.position.fraction.total_cmp(&b.position.fraction))
        });
        true
    }

    /// Removes the bookmark `id`; `true` when there was one.
    pub fn remove_bookmark(&mut self, id: &str) -> bool {
        let before = self.bookmarks.len();
        self.bookmarks.retain(|b| b.id != id);
        self.bookmarks.len() != before
    }
}

/// The folder of book `id`: `reader/books/<id>/`.
#[must_use]
pub fn book_folder(id: &str) -> String {
    format!("{BOOKS}{id}/")
}

/// The key of book `id`'s file.
#[must_use]
pub fn book_key(id: &str, format: Format) -> String {
    format!("{}book.{}", book_folder(id), format.extension())
}

/// The key of book `id`'s `info.json`.
#[must_use]
pub fn info_key(id: &str) -> String {
    format!("{}info.json", book_folder(id))
}

/// The key of book `id`'s `state.json`.
#[must_use]
pub fn state_key(id: &str) -> String {
    format!("{}state.json", book_folder(id))
}

/// The key of book `id`'s `cover.png`.
#[must_use]
pub fn cover_key(id: &str) -> String {
    format!("{}cover.png", book_folder(id))
}

/// The book id a key of the books' folder names (`reader/books/<id>/...`).
#[must_use]
pub fn id_of_key(key: &str) -> Option<&str> {
    let rest = key.strip_prefix(BOOKS)?;
    let (id, _) = rest.split_once('/')?;
    (!id.is_empty()).then_some(id)
}

/// A book of the library: what it is and where the reader is in it.
#[derive(Debug, Clone, PartialEq)]
pub struct LibraryEntry {
    pub info: BookInfo,
    pub state: BookState,
}

/// The library from the books' `info.json` and `state.json` files (`(key, bytes)`): a book
/// whose info cannot be read is left out, one without a state starts at its beginning.
#[must_use]
pub fn entries_from(
    infos: &[(String, Vec<u8>)],
    states: &[(String, Vec<u8>)],
) -> Vec<LibraryEntry> {
    infos
        .iter()
        .filter_map(|(key, bytes)| {
            let id = id_of_key(key)?;
            let mut info: BookInfo = serde_json::from_slice(bytes).ok()?;
            info.id = id.to_string();
            let state = states
                .iter()
                .find(|(k, _)| id_of_key(k) == Some(id))
                .and_then(|(_, b)| serde_json::from_slice::<BookState>(b).ok())
                .unwrap_or_default();
            Some(LibraryEntry { info, state })
        })
        .collect()
}

/// How the library is ordered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sort {
    /// Last read first, then last added.
    Recent,
    Title,
    Author,
}

impl Sort {
    pub const ALL: [Sort; 3] = [Sort::Recent, Sort::Title, Sort::Author];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Sort::Recent => "Recent",
            Sort::Title => "Title",
            Sort::Author => "Author",
        }
    }

    #[must_use]
    pub fn index(self) -> usize {
        Self::ALL.iter().position(|s| *s == self).unwrap_or(0)
    }
}

/// Which books the library shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shelf {
    All,
    /// Opened, not finished.
    Reading,
    NotStarted,
    Finished,
}

impl Shelf {
    pub const ALL: [Shelf; 4] = [
        Shelf::All,
        Shelf::Reading,
        Shelf::NotStarted,
        Shelf::Finished,
    ];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Shelf::All => "All books",
            Shelf::Reading => "Reading",
            Shelf::NotStarted => "Not started",
            Shelf::Finished => "Finished",
        }
    }

    #[must_use]
    pub const fn icon(self) -> &'static str {
        match self {
            Shelf::All => "library_books",
            Shelf::Reading => "auto_stories",
            Shelf::NotStarted => "book",
            Shelf::Finished => "task_alt",
        }
    }

    /// Whether `entry` is on this shelf.
    #[must_use]
    pub fn holds(self, entry: &LibraryEntry) -> bool {
        match self {
            Shelf::All => true,
            Shelf::Reading => entry.state.last_read > 0 && !entry.state.finished,
            Shelf::NotStarted => entry.state.last_read == 0,
            Shelf::Finished => entry.state.finished,
        }
    }
}

/// The entries on `shelf` whose title or authors contain `query` (any case), in `sort` order.
#[must_use]
pub fn shown(entries: &[LibraryEntry], shelf: Shelf, query: &str, sort: Sort) -> Vec<LibraryEntry> {
    let query = query.trim().to_lowercase();
    let mut out: Vec<LibraryEntry> = entries
        .iter()
        .filter(|e| shelf.holds(e))
        .filter(|e| {
            query.is_empty()
                || e.info.title.to_lowercase().contains(&query)
                || e.info.author_line().to_lowercase().contains(&query)
        })
        .cloned()
        .collect();
    let title = |e: &LibraryEntry| e.info.title.to_lowercase();
    match sort {
        Sort::Recent => out.sort_by(|a, b| {
            b.state
                .last_read
                .cmp(&a.state.last_read)
                .then(b.info.added.cmp(&a.info.added))
                .then(title(a).cmp(&title(b)))
        }),
        Sort::Title => out.sort_by(|a, b| title(a).cmp(&title(b)).then(a.info.id.cmp(&b.info.id))),
        Sort::Author => out.sort_by(|a, b| {
            a.info
                .author_line()
                .to_lowercase()
                .cmp(&b.info.author_line().to_lowercase())
                .then(title(a).cmp(&title(b)))
        }),
    }
    out
}

/// A title from a file name: no folder, no extension, `_` as a blank (`My_Book.epub` is
/// "My Book"); "Untitled" when nothing is left.
#[must_use]
pub fn title_from_file_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let stem = match base.rsplit_once('.') {
        Some((stem, _)) if !stem.is_empty() => stem,
        _ => base,
    };
    let title = crate::xmltree::fold_space(&stem.replace('_', " "));
    if title.is_empty() {
        "Untitled".to_string()
    } else {
        title
    }
}

/// Now, in seconds since 1970.
#[must_use]
pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, title: &str, author: &str, last_read: u64, finished: bool) -> LibraryEntry {
        LibraryEntry {
            info: BookInfo {
                id: id.to_string(),
                title: title.to_string(),
                authors: vec![author.to_string()],
                language: String::new(),
                format: Format::Epub,
                original_name: String::new(),
                added: 0,
                size: 0,
                has_cover: false,
            },
            state: BookState {
                last_read,
                finished,
                ..BookState::default()
            },
        }
    }

    #[test]
    fn a_book_is_a_folder_of_files_named_by_its_id() {
        assert_eq!(book_key("u1", Format::Epub), "reader/books/u1/book.epub");
        assert_eq!(info_key("u1"), "reader/books/u1/info.json");
        assert_eq!(state_key("u1"), "reader/books/u1/state.json");
        assert_eq!(cover_key("u1"), "reader/books/u1/cover.png");
        assert_eq!(id_of_key("reader/books/u1/state.json"), Some("u1"));
        assert_eq!(id_of_key("reader/settings.json"), None);
        assert_eq!(id_of_key("reader/books//x"), None);
    }

    #[test]
    fn the_format_comes_from_the_name_else_from_the_bytes() {
        assert_eq!(Format::of_name("A.EPUB"), Some(Format::Epub));
        assert_eq!(Format::of_name("notes.txt"), Some(Format::Text));
        assert_eq!(Format::of_name("page.htm"), Some(Format::Html));
        assert_eq!(Format::of_name("photo.jpg"), None);
        assert_eq!(Format::sniff(b"PK\x03\x04rest"), Some(Format::Epub));
        assert_eq!(
            Format::sniff(b"\xEF\xBB\xBF<!DOCTYPE html><p>x"),
            Some(Format::Html)
        );
        assert_eq!(Format::sniff(b"Once upon a time"), Some(Format::Text));
        assert_eq!(Format::sniff(&[0xFF, 0xFE, 0x00, 0xD8]), None);
    }

    #[test]
    fn info_and_state_files_read_back_and_a_missing_state_starts_at_the_beginning() {
        let info = entry("x", "T", "A", 0, false).info;
        let state = BookState {
            position: Position {
                chapter: 2,
                fraction: 0.25,
            },
            last_read: 99,
            progress: 0.4,
            ..BookState::default()
        };
        let infos = vec![
            (
                "reader/books/u1/info.json".to_string(),
                serde_json::to_vec(&info).expect("json"),
            ),
            (
                "reader/books/u2/info.json".to_string(),
                serde_json::to_vec(&info).expect("json"),
            ),
            (
                "reader/books/u3/info.json".to_string(),
                b"not json".to_vec(),
            ),
        ];
        let states = vec![(
            "reader/books/u1/state.json".to_string(),
            serde_json::to_vec(&state).expect("json"),
        )];
        let entries = entries_from(&infos, &states);
        assert_eq!(entries.len(), 2, "the unreadable info is left out");
        assert_eq!(entries[0].info.id, "u1", "the id is the folder's");
        assert_eq!(entries[0].state, state);
        assert_eq!(entries[1].state, BookState::default());
        let old: BookState =
            serde_json::from_str("{\"position\":{\"chapter\":1,\"fraction\":0.5}}")
                .expect("old file");
        assert_eq!(old.position.chapter, 1);
        assert!(old.bookmarks.is_empty());
    }

    #[test]
    fn the_bookmark_button_toggles_and_bookmarks_stay_in_reading_order() {
        let mark = |id: &str, chapter: usize, fraction: f32| Bookmark {
            id: id.to_string(),
            position: Position { chapter, fraction },
            label: id.to_string(),
            chapter_title: String::new(),
            created: 0,
        };
        let mut state = BookState::default();
        assert!(state.toggle_bookmark(mark("b", 2, 0.5)));
        assert!(state.toggle_bookmark(mark("a", 0, 0.9)));
        assert!(state.toggle_bookmark(mark("c", 2, 0.1)));
        let order: Vec<&str> = state.bookmarks.iter().map(|b| b.id.as_str()).collect();
        assert_eq!(order, vec!["a", "c", "b"]);
        assert!(state
            .bookmark_at(Position {
                chapter: 2,
                fraction: 0.5
            })
            .is_some());
        assert!(
            !state.toggle_bookmark(mark("again", 2, 0.5)),
            "the same place: removed"
        );
        assert!(state
            .bookmark_at(Position {
                chapter: 2,
                fraction: 0.5
            })
            .is_none());
        assert!(state.remove_bookmark("a"));
        assert!(!state.remove_bookmark("a"));
        assert_eq!(state.bookmarks.len(), 1);
    }

    #[test]
    fn the_library_shows_a_shelf_filtered_and_sorted() {
        let entries = vec![
            entry("1", "Zebra", "Bo", 10, false),
            entry("2", "apple", "Ann", 0, false),
            entry("3", "Mango", "Cy", 30, true),
            entry("4", "Kiwi", "Ann", 20, false),
        ];
        let ids = |v: Vec<LibraryEntry>| v.into_iter().map(|e| e.info.id).collect::<Vec<_>>();
        assert_eq!(
            ids(shown(&entries, Shelf::All, "", Sort::Recent)),
            vec!["3", "4", "1", "2"]
        );
        assert_eq!(
            ids(shown(&entries, Shelf::All, "", Sort::Title)),
            vec!["2", "4", "3", "1"]
        );
        assert_eq!(
            ids(shown(&entries, Shelf::All, "", Sort::Author)),
            vec!["2", "4", "1", "3"]
        );
        assert_eq!(
            ids(shown(&entries, Shelf::Reading, "", Sort::Title)),
            vec!["4", "1"]
        );
        assert_eq!(
            ids(shown(&entries, Shelf::NotStarted, "", Sort::Title)),
            vec!["2"]
        );
        assert_eq!(
            ids(shown(&entries, Shelf::Finished, "", Sort::Title)),
            vec!["3"]
        );
        assert_eq!(
            ids(shown(&entries, Shelf::All, "ANN", Sort::Title)),
            vec!["2", "4"],
            "by author too"
        );
    }

    #[test]
    fn a_title_is_made_from_a_file_name() {
        assert_eq!(
            title_from_file_name("/books/My_Great__Book.epub"),
            "My Great Book"
        );
        assert_eq!(title_from_file_name("C:\\x\\notes.txt"), "notes");
        assert_eq!(title_from_file_name(".epub"), ".epub");
        assert_eq!(title_from_file_name("   "), "Untitled");
    }
}
