//! What AzReader asks of its data folder, done on an azul `Thread` through azul-storage's
//! `Drive` (a `LocalDrive` at the data root today, the user's bucket later) - never from a
//! callback:
//!
//! - [`Job::Scan`]: the library - every book's `info.json` and `state.json`, its cover;
//! - [`Job::Import`]: a file the user picked becomes a book folder (the file, `info.json`,
//!   `state.json`, a small `cover.png`);
//! - [`Job::Open`]: a book's file read into its container and its package;
//! - [`Job::Delete`]: a book's folder removed.
//!
//! The work itself ([`scan`], [`import`], [`open`], [`delete`]) is plain functions on a
//! `&dyn Drive`, tested on a folder.

use std::{path::PathBuf, sync::Arc};

use azul::{
    callbacks::{CallbackInfo, RefAny, WriteBackCallbackType},
    error::{ResultRawImageDecodeImageError, ResultU8VecEncodeImageError},
    image::{ImageRef, RawImage},
    task::{Thread, ThreadId, ThreadReceiveMsg, ThreadReceiver, ThreadSender, ThreadWriteBackMsg},
    vec::U8VecRef,
};
use azul_storage::{Drive, DriveError, LocalDrive};

use crate::{
    epub::{self, Book, Container},
    library::{self, BookInfo, BookState, Format, LibraryEntry},
    plainbook,
};

/// A cover is kept at most this big (px).
pub const COVER_W: u32 = 240;
pub const COVER_H: u32 = 360;

/// One thing to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Job {
    Scan,
    /// A file outside the data tree (picked, dropped, named on the command line).
    Import {
        path: PathBuf,
    },
    /// A file the app made (the sample book): its name and bytes.
    ImportBytes {
        name: String,
        bytes: Vec<u8>,
    },
    Open {
        id: String,
        format: Format,
        title: String,
    },
    Delete {
        id: String,
    },
}

/// What a job did.
pub enum Done {
    Library {
        entries: Vec<LibraryEntry>,
        /// The covers, decoded: `(book id, image)`.
        covers: Vec<(String, ImageRef)>,
        errors: Vec<String>,
    },
    Imported {
        result: Result<LibraryEntry, String>,
        cover: Option<ImageRef>,
    },
    Opened {
        id: String,
        result: Result<(Arc<Container>, Book), String>,
    },
    Deleted {
        id: String,
        result: Result<(), String>,
    },
}

/// The library on `drive`: the entries, the cover files (`(id, bytes)`) and what could not
/// be read.
pub fn scan(drive: &dyn Drive) -> (Vec<LibraryEntry>, Vec<(String, Vec<u8>)>, Vec<String>) {
    let mut errors = Vec::new();
    let keys = match azul_appkit::files::list_all(drive, library::BOOKS) {
        Ok(keys) => keys,
        Err(e) => return (Vec::new(), Vec::new(), vec![e.to_string()]),
    };
    let mut read = |suffix: &str| -> Vec<(String, Vec<u8>)> {
        keys.iter()
            .filter(|k| k.ends_with(suffix))
            .filter_map(|k| match drive.get(k) {
                Ok(bytes) => Some((k.clone(), bytes)),
                Err(DriveError::NotFound { .. }) => None,
                Err(e) => {
                    errors.push(e.to_string());
                    None
                }
            })
            .collect()
    };
    let infos = read("/info.json");
    let states = read("/state.json");
    let covers: Vec<(String, Vec<u8>)> = read("/cover.png")
        .into_iter()
        .filter_map(|(k, bytes)| Some((library::id_of_key(&k)?.to_string(), bytes)))
        .collect();
    (library::entries_from(&infos, &states), covers, errors)
}

/// `bytes` as a small PNG cover (`None` when azul cannot decode them).
#[must_use]
pub fn cover_png(bytes: &[u8]) -> Option<Vec<u8>> {
    let raw = match RawImage::decode_image_bytes_any(U8VecRef::from(bytes)) {
        ResultRawImageDecodeImageError::Ok(raw) => raw,
        ResultRawImageDecodeImageError::Err(_) => return None,
    };
    let small = raw.thumbnail(COVER_W, COVER_H).into_option().unwrap_or(raw);
    match small.encode_png() {
        ResultU8VecEncodeImageError::Ok(png) => Some(png.as_slice().to_vec()),
        ResultU8VecEncodeImageError::Err(_) => None,
    }
}

/// A decoded cover for the library (`None` when the bytes are no picture).
#[must_use]
pub fn cover_image(bytes: &[u8]) -> Option<ImageRef> {
    match RawImage::decode_image_bytes_any(U8VecRef::from(bytes)) {
        ResultRawImageDecodeImageError::Ok(raw) => ImageRef::create_rawimage(raw).into_option(),
        ResultRawImageDecodeImageError::Err(_) => None,
    }
}

/// The book in `bytes` (a file named `name` of `format`), read: its container and package.
pub fn read_book(bytes: &[u8], format: Format, title: &str) -> Result<(Container, Book), String> {
    match format {
        Format::Epub => {
            let container = Container::from_zip_bytes(bytes).map_err(|e| e.to_string())?;
            let book = epub::parse_book(&container).map_err(|e| e.to_string())?;
            Ok((container, book))
        }
        Format::Text => Ok(plainbook::text_book(&epub::decode_text(bytes), title)),
        Format::Html => Ok(plainbook::html_book(&epub::decode_text(bytes), title)),
    }
}

/// Imports the file `name` (its `bytes`) as book `id`, added at `now`: the book's folder
/// written, its entry and its PNG cover (when it has one) back.
pub fn import(
    drive: &dyn Drive,
    id: &str,
    name: &str,
    bytes: &[u8],
    now: u64,
) -> Result<(LibraryEntry, Option<Vec<u8>>), String> {
    let format = Format::of_name(name)
        .or_else(|| Format::sniff(bytes))
        .ok_or_else(|| format!("{name} is not a book AzReader can open (EPUB, text or HTML)"))?;
    let file_title = library::title_from_file_name(name);
    let (container, book) = read_book(bytes, format, &file_title)?;
    let cover = book
        .cover
        .as_deref()
        .and_then(|path| container.get(path))
        .and_then(cover_png);
    let title = if book.metadata.title.trim().is_empty() {
        file_title
    } else {
        book.metadata.title.clone()
    };
    let info = BookInfo {
        id: id.to_string(),
        title,
        authors: book.metadata.authors.clone(),
        language: book.metadata.language.clone(),
        format,
        original_name: name.to_string(),
        added: now,
        size: bytes.len() as u64,
        has_cover: cover.is_some(),
    };
    let state = BookState::default();
    drive
        .put(&library::book_key(id, format), bytes)
        .map_err(|e| e.to_string())?;
    if let Some(png) = &cover {
        drive
            .put(&library::cover_key(id), png)
            .map_err(|e| e.to_string())?;
    }
    drive
        .put(&library::state_key(id), state_json(&state).as_bytes())
        .map_err(|e| e.to_string())?;
    // The info last: a folder with an info.json is a whole book.
    let info_json = serde_json::to_string_pretty(&info).map_err(|e| e.to_string())?;
    drive
        .put(&library::info_key(id), info_json.as_bytes())
        .map_err(|e| e.to_string())?;
    Ok((LibraryEntry { info, state }, cover))
}

/// The state file of a book as written (`state.json`).
#[must_use]
pub fn state_json(state: &BookState) -> String {
    serde_json::to_string_pretty(state).unwrap_or_default()
}

/// Opens book `id` (`format`, titled `title` for a plain file).
pub fn open(
    drive: &dyn Drive,
    id: &str,
    format: Format,
    title: &str,
) -> Result<(Container, Book), String> {
    let bytes = drive
        .get(&library::book_key(id, format))
        .map_err(|e| e.to_string())?;
    read_book(&bytes, format, title)
}

/// Removes book `id`'s folder: every file in it, the info first (a folder without an info
/// is no book even if a removal fails half-way).
pub fn delete(drive: &dyn Drive, id: &str) -> Result<(), String> {
    drive
        .delete(&library::info_key(id))
        .map_err(|e| e.to_string())?;
    let keys = azul_appkit::files::list_all(drive, &library::book_folder(id))
        .map_err(|e| e.to_string())?;
    for key in keys {
        drive.delete(&key).map_err(|e| e.to_string())?;
    }
    Ok(())
}

// ==== On a Thread ====

struct JobInit {
    root: PathBuf,
    job: Option<Job>,
    on_done: WriteBackCallbackType,
}

/// An import's outcome with its cover decoded.
fn imported(result: Result<(LibraryEntry, Option<Vec<u8>>), String>) -> Done {
    match result {
        Ok((entry, cover)) => Done::Imported {
            result: Ok(entry),
            cover: cover.as_deref().and_then(cover_image),
        },
        Err(e) => Done::Imported {
            result: Err(e),
            cover: None,
        },
    }
}

/// Runs `job` against the data root's drive.
fn run(root: PathBuf, job: Job) -> Done {
    let drive = LocalDrive::new(root);
    match job {
        Job::Scan => {
            let (entries, cover_files, errors) = scan(&drive);
            let covers = cover_files
                .into_iter()
                .filter_map(|(id, bytes)| Some((id, cover_image(&bytes)?)))
                .collect();
            Done::Library {
                entries,
                covers,
                errors,
            }
        }
        Job::Import { path } => {
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("book")
                .to_string();
            let result = azul_appkit::files::read_outside(&path).and_then(|bytes| {
                import(
                    &drive,
                    &azul_storage::ids::new_uuid(),
                    &name,
                    &bytes,
                    library::now_secs(),
                )
            });
            imported(result)
        }
        Job::ImportBytes { name, bytes } => imported(import(
            &drive,
            &azul_storage::ids::new_uuid(),
            &name,
            &bytes,
            library::now_secs(),
        )),
        Job::Open { id, format, title } => Done::Opened {
            result: open(&drive, &id, format, &title).map(|(c, b)| (Arc::new(c), b)),
            id,
        },
        Job::Delete { id } => Done::Deleted {
            result: delete(&drive, &id),
            id,
        },
    }
}

extern "C" fn job_worker(mut init: RefAny, mut sender: ThreadSender, _receiver: ThreadReceiver) {
    let Some((root, job, on_done)) = init
        .downcast_mut::<JobInit>()
        .and_then(|mut i| Some((i.root.clone(), i.job.take()?, i.on_done)))
    else {
        return;
    };
    let done = run(root, job);
    let _sent = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::create(
        on_done,
        RefAny::new(Some(done)),
    )));
}

/// Runs `job` on an azul `Thread` against the drive at `root`; `on_done(app, Done, info)`
/// gets the outcome on the UI thread ([`take_done`]).
pub fn spawn(
    info: &mut CallbackInfo,
    app: &RefAny,
    root: &std::path::Path,
    job: Job,
    on_done: WriteBackCallbackType,
) {
    info.add_thread(
        ThreadId::unique(),
        Thread::create(
            RefAny::new(JobInit {
                root: root.to_path_buf(),
                job: Some(job),
                on_done,
            }),
            app.clone(),
            job_worker,
        ),
    );
}

/// The outcome out of a write-back's message (`None` if it is not one).
#[must_use]
pub fn take_done(msg: &mut RefAny) -> Option<Done> {
    let mut guard = msg.downcast_mut::<Option<Done>>()?;
    guard.take()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::epub::fixtures;
    // The one temporary folder for tests (a fresh folder, removed on drop).
    use azul_storage::testing::TempDir;

    fn epub_bytes() -> Vec<u8> {
        let mut zip = azul::zip::Zip::create();
        let book = fixtures::book3();
        for path in book.paths().map(str::to_string).collect::<Vec<_>>() {
            let bytes = book.get(&path).unwrap_or_default().to_vec();
            zip.add_file(path, bytes);
        }
        zip.to_bytes().as_slice().to_vec()
    }

    #[test]
    fn an_imported_epub_is_a_folder_of_files_and_the_library_finds_it() {
        let dir = TempDir::new("import");
        let drive = LocalDrive::new(&dir.0);
        let (entry, cover) =
            import(&drive, "u1", "tale.epub", &epub_bytes(), 1234).expect("imported");
        assert_eq!(entry.info.title, "The Tale of Two Files");
        assert_eq!(entry.info.authors.len(), 2);
        assert_eq!(entry.info.format, Format::Epub);
        assert_eq!(entry.info.added, 1234);
        assert!(cover.is_none(), "the fixture's cover is no real picture");
        assert!(dir.0.join("reader/books/u1/book.epub").is_file());
        assert!(dir.0.join("reader/books/u1/info.json").is_file());
        assert!(dir.0.join("reader/books/u1/state.json").is_file());
        let (entries, covers, errors) = scan(&drive);
        assert!(errors.is_empty(), "{errors:?}");
        assert!(covers.is_empty());
        assert_eq!(entries, vec![entry]);
        let (container, book) = open(&drive, "u1", Format::Epub, "x").expect("opened");
        assert_eq!(book.spine.len(), 2);
        assert!(container.get("OEBPS/text/ch1.xhtml").is_some());
        delete(&drive, "u1").expect("deleted");
        assert!(scan(&drive).0.is_empty());
        assert!(!dir.0.join("reader/books/u1/book.epub").exists());
    }

    #[test]
    fn a_text_file_is_imported_under_its_file_name_and_what_is_no_book_says_so() {
        let dir = TempDir::new("text");
        let drive = LocalDrive::new(&dir.0);
        let (entry, _) =
            import(&drive, "t1", "my_notes.txt", b"CHAPTER I\n\nHello.", 5).expect("imported");
        assert_eq!(entry.info.title, "my notes");
        assert_eq!(entry.info.format, Format::Text);
        let (_, book) = open(&drive, "t1", Format::Text, &entry.info.title).expect("opened");
        assert_eq!(book.toc[0].label, "CHAPTER I");
        let refused = import(&drive, "x", "photo.jpg", &[0xFF, 0xD8, 0xFF, 0x00, 0xFE], 5);
        assert!(refused.is_err());
        let broken = import(&drive, "y", "broken.epub", b"PK\x03\x04 not really", 5);
        assert!(broken.is_err(), "a broken zip is no book");
        assert!(
            !dir.0.join("reader/books/y/info.json").exists(),
            "nothing is written for it"
        );
    }
}
