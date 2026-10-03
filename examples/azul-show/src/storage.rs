//! Decks are files, in the per-user layout the S3 bucket will have:
//! `show/<deck id>/deck.json` and the pictures under `show/<deck id>/media/`,
//! written through azul-storage's [`Drive`] (a [`LocalDrive`] rooted at the
//! user's data folder today, an `S3Drive` later with no other change). The
//! blocking calls run on an azul `Thread` (see `lib.rs`), never in a
//! callback: [`run_job`] is the whole thread body, and it is tested here on
//! a `LocalDrive` in a temporary folder.

use std::path::PathBuf;

use azul_storage::{Drive, DriveError, ListRequest, LocalDrive};

use crate::model::Deck;

/// The app's folder in the user's storage.
pub const APP_FOLDER: &str = "show";

/// `show/<id>/deck.json`.
#[must_use]
pub fn deck_key(id: &str) -> String {
    format!("{APP_FOLDER}/{id}/deck.json")
}

/// The deck id of a `show/<id>/deck.json` key; `None` for any other key (a
/// picture under `media/`, a stray file).
#[must_use]
pub fn deck_id_of(key: &str) -> Option<&str> {
    let id = key
        .strip_prefix(APP_FOLDER)?
        .strip_prefix('/')?
        .strip_suffix("/deck.json")?;
    (!id.is_empty() && !id.contains('/')).then_some(id)
}

/// `show/exports/<name>`: where an export (a PDF, a slide picture) lands -
/// in the data tree, through the drive, like every durable file.
#[must_use]
pub fn export_key(name: &str) -> String {
    format!("{APP_FOLDER}/exports/{name}")
}

/// `show/<id>/<media>`, `media` being an element's `media/<name>` key.
#[must_use]
pub fn media_key(id: &str, media: &str) -> String {
    format!("{APP_FOLDER}/{id}/{media}")
}

/// A deck as the Open page lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeckSummary {
    pub id: String,
    pub title: String,
    pub slides: usize,
    /// Seconds since 1970, when the backend knows.
    pub modified: Option<u64>,
}

/// What the storage thread does.
#[derive(Debug, Clone)]
pub enum Job {
    /// Writes `deck.json`.
    Save { deck: Box<Deck> },
    /// Reads a deck and every picture it uses.
    Load { id: String },
    /// Lists the decks.
    List,
    /// Stores a picture under the deck's `media/` as `name`.
    PutMedia { deck: String, name: String, bytes: Vec<u8> },
    /// Reads a picture the user picked and stores it under the deck's
    /// `media/` (a fresh name, the file's extension).
    ImportFile { deck: String, path: PathBuf },
    /// Writes an export (`name` with its extension) to `show/exports/`.
    Export { name: String, bytes: Vec<u8> },
}

/// What came back.
#[derive(Debug, Clone)]
pub enum Outcome {
    /// The deck's id, or why it was not written.
    Saved(Result<String, String>),
    /// The deck and its pictures (media key, bytes), or why not.
    Loaded(Result<(Box<Deck>, Vec<(String, Vec<u8>)>), String>),
    /// The decks, newest first, or why not.
    Listed(Result<Vec<DeckSummary>, String>),
    /// The picture's media key (`media/<name>`) and its bytes, or why not.
    MediaStored(Result<(String, Vec<u8>), String>),
    /// The export's key (`show/exports/<name>`), or why not.
    Exported(Result<String, String>),
}

fn why(e: DriveError) -> String {
    e.to_string()
}

/// The media keys a deck uses (pictures and videos, in groups too).
#[must_use]
pub fn media_of(deck: &Deck) -> Vec<String> {
    use crate::model::{Element, ElementKind};
    fn walk(e: &Element, out: &mut Vec<String>) {
        match &e.kind {
            ElementKind::Image { media, .. } | ElementKind::Video { media } => {
                if !media.is_empty() && !out.contains(media) {
                    out.push(media.clone());
                }
            }
            ElementKind::Group { children } => {
                for c in children {
                    walk(c, out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    for slide in &deck.slides {
        for e in &slide.elements {
            walk(e, &mut out);
        }
    }
    out
}

/// The whole storage thread: one blocking job on `drive`.
#[must_use]
pub fn run_job(drive: &dyn Drive, job: Job) -> Outcome {
    match job {
        Job::Save { deck } => Outcome::Saved(
            drive
                .put(&deck_key(&deck.id), deck.to_json().as_bytes())
                .map(|()| deck.id.clone())
                .map_err(why),
        ),
        Job::Load { id } => Outcome::Loaded((|| -> Result<(Box<Deck>, Vec<(String, Vec<u8>)>), String> {
            let bytes = drive.get(&deck_key(&id)).map_err(why)?;
            let text = String::from_utf8(bytes).map_err(|e| format!("deck.json: {e}"))?;
            let deck = Deck::from_json(&text)?;
            let mut media = Vec::new();
            for key in media_of(&deck) {
                // A missing picture is drawn as a placeholder, not an error.
                if let Ok(bytes) = drive.get(&media_key(&id, &key)) {
                    media.push((key, bytes));
                }
            }
            Ok((Box::new(deck), media))
        })()),
        Job::List => Outcome::Listed((|| -> Result<Vec<DeckSummary>, String> {
            // Every page of the listing (a bucket answers a thousand keys a
            // page); a deck is a `show/<id>/deck.json`, its date comes with
            // the listing (no extra round trip per deck).
            let prefix = format!("{APP_FOLDER}/");
            let objects = azul_storage::ops::list_all(drive, &prefix).map_err(why)?;
            let mut decks = Vec::new();
            for object in objects {
                let Some(id) = deck_id_of(&object.key) else {
                    continue;
                };
                let Ok(bytes) = drive.get(&object.key) else {
                    continue;
                };
                let Ok(deck) = Deck::from_json(&String::from_utf8_lossy(&bytes)) else {
                    continue;
                };
                decks.push(DeckSummary {
                    id: id.to_string(),
                    title: deck.title.clone(),
                    slides: deck.slides.len(),
                    modified: object.modified,
                });
            }
            decks.sort_by(|a, b| b.modified.cmp(&a.modified).then_with(|| a.title.cmp(&b.title)));
            Ok(decks)
        })()),
        Job::ImportFile { deck, path } => match std::fs::read(&path) {
            Ok(bytes) => {
                let ext = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(str::to_ascii_lowercase)
                    .unwrap_or_else(|| String::from("png"));
                let stem = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .map(|s| s.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').collect::<String>())
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| String::from("picture"));
                let nanos = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.subsec_nanos())
                    .unwrap_or(0);
                let name = format!("{stem}-{nanos:08x}.{ext}");
                run_job(drive, Job::PutMedia { deck, name, bytes })
            }
            Err(e) => Outcome::MediaStored(Err(format!("{}: {e}", path.display()))),
        },
        Job::Export { name, bytes } => {
            let key = export_key(&name);
            Outcome::Exported(drive.put(&key, &bytes).map(|()| key).map_err(why))
        }
        Job::PutMedia { deck, name, bytes } => {
            let media = format!("media/{name}");
            Outcome::MediaStored(
                drive
                    .put(&media_key(&deck, &media), &bytes)
                    .map(|()| (media, bytes))
                    .map_err(why),
            )
        }
    }
}

/// The drive the app writes to: a folder under `root`.
#[must_use]
pub fn local_drive(root: PathBuf) -> LocalDrive {
    LocalDrive::new(root)
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{sample_deck, Element, ElementKind, Frame, ImageFit, Theme};

    fn temp_root(tag: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("azshow-{tag}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a temp folder");
        dir
    }

    #[test]
    fn a_saved_deck_lies_at_show_its_id_deck_json_and_loads_back_with_its_pictures() {
        let root = temp_root("save");
        let drive = local_drive(root.clone());
        let mut deck = sample_deck("deck-1", Theme::office());
        let id = deck.mint();
        deck.slides[1].elements.push(Element::new(
            id,
            Frame::new(10.0, 10.0, 100.0, 100.0),
            ElementKind::Image {
                media: String::from("media/cat.png"),
                fit: ImageFit::Contain,
            },
        ));
        match run_job(
            &drive,
            Job::PutMedia {
                deck: String::from("deck-1"),
                name: String::from("cat.png"),
                bytes: vec![1, 2, 3],
            },
        ) {
            Outcome::MediaStored(Ok((key, bytes))) => {
                assert_eq!(key, "media/cat.png");
                assert_eq!(bytes, vec![1, 2, 3]);
            }
            other => panic!("{other:?}"),
        }
        match run_job(&drive, Job::Save { deck: Box::new(deck.clone()) }) {
            Outcome::Saved(Ok(id)) => assert_eq!(id, "deck-1"),
            other => panic!("{other:?}"),
        }
        assert!(root.join("show/deck-1/deck.json").is_file(), "the file the S3 bucket will hold");
        assert!(root.join("show/deck-1/media/cat.png").is_file());
        match run_job(&drive, Job::Load { id: String::from("deck-1") }) {
            Outcome::Loaded(Ok((back, media))) => {
                assert_eq!(*back, deck);
                assert_eq!(media, vec![(String::from("media/cat.png"), vec![1, 2, 3])]);
            }
            other => panic!("{other:?}"),
        }
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn the_open_page_lists_every_deck_by_title() {
        let root = temp_root("list");
        let drive = local_drive(root.clone());
        for (id, title) in [("a", "Alpha"), ("b", "Beta")] {
            let mut deck = sample_deck(id, Theme::office());
            deck.title = title.to_string();
            assert!(matches!(run_job(&drive, Job::Save { deck: Box::new(deck) }), Outcome::Saved(Ok(_))));
        }
        std::fs::create_dir_all(root.join("show/not-a-deck")).expect("a stray folder");
        match run_job(&drive, Job::List) {
            Outcome::Listed(Ok(decks)) => {
                let mut titles: Vec<String> = decks.iter().map(|d| d.title.clone()).collect();
                titles.sort();
                assert_eq!(titles, vec![String::from("Alpha"), String::from("Beta")]);
                assert!(decks.iter().all(|d| d.slides == 10));
            }
            other => panic!("{other:?}"),
        }
        let _ = std::fs::remove_dir_all(root);
    }

    /// A drive that answers two entries per page (an S3 bucket answers a
    /// thousand): a listing that reads one page misses the rest.
    struct TwoPerPage(LocalDrive);

    impl Drive for TwoPerPage {
        fn list(&self, request: &ListRequest) -> Result<azul_storage::ListPage, DriveError> {
            self.0.list(&request.clone().with_max_keys(2))
        }
        fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
            self.0.get(key)
        }
        fn get_range(&self, key: &str, range: azul_storage::ByteRange) -> Result<Vec<u8>, DriveError> {
            self.0.get_range(key, range)
        }
        fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
            self.0.put(key, bytes)
        }
        fn delete(&self, key: &str) -> Result<(), DriveError> {
            self.0.delete(key)
        }
        fn head(&self, key: &str) -> Result<azul_storage::ObjectInfo, DriveError> {
            self.0.head(key)
        }
    }

    /// DEDUP_OFFICE N2 / D4 (an S3 blocker): `Job::List` read ONE page, so
    /// on a bucket the decks past the first thousand keys vanished from
    /// File > Open.
    #[test]
    fn the_open_page_lists_every_deck_past_the_first_page_of_the_listing() {
        let root = temp_root("pages");
        let drive = TwoPerPage(local_drive(root.clone()));
        for i in 0..5 {
            let deck = sample_deck(&format!("deck-{i}"), Theme::office());
            assert!(matches!(run_job(&drive, Job::Save { deck: Box::new(deck) }), Outcome::Saved(Ok(_))));
        }
        let picture = Job::PutMedia {
            deck: String::from("deck-0"),
            name: String::from("a.png"),
            bytes: vec![1, 2, 3],
        };
        assert!(matches!(run_job(&drive, picture), Outcome::MediaStored(Ok(_))), "a picture is no deck");
        match run_job(&drive, Job::List) {
            Outcome::Listed(Ok(decks)) => {
                let mut ids: Vec<String> = decks.iter().map(|d| d.id.clone()).collect();
                ids.sort();
                assert_eq!(ids, vec!["deck-0", "deck-1", "deck-2", "deck-3", "deck-4"]);
                assert!(decks.iter().all(|d| d.modified.is_some()), "the date comes with the listing");
            }
            other => panic!("{other:?}"),
        }
        let _ = std::fs::remove_dir_all(root);
    }

    /// User ruling 2026-10-02: every durable write - exports included - goes
    /// INTO the data tree through the drive (the PDF / PNG went to a save
    /// dialog's path, outside the tree a later S3 sync diffs).
    #[test]
    fn an_export_is_written_into_show_exports_in_the_data_tree() {
        let root = temp_root("export");
        let drive = local_drive(root.clone());
        match run_job(
            &drive,
            Job::Export {
                name: String::from("Azlin Workspace.pdf"),
                bytes: vec![b'%', b'P', b'D', b'F'],
            },
        ) {
            Outcome::Exported(Ok(key)) => assert_eq!(key, "show/exports/Azlin Workspace.pdf"),
            other => panic!("{other:?}"),
        }
        assert_eq!(
            std::fs::read(root.join("show/exports/Azlin Workspace.pdf")).expect("the file"),
            b"%PDF".to_vec()
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_missing_deck_is_an_error_not_a_panic() {
        let root = temp_root("missing");
        let drive = local_drive(root.clone());
        assert!(matches!(
            run_job(&drive, Job::Load { id: String::from("nope") }),
            Outcome::Loaded(Err(_))
        ));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn the_keys_of_a_deck_its_media_and_an_export() {
        assert_eq!(deck_key("x"), "show/x/deck.json");
        assert_eq!(deck_id_of("show/x/deck.json"), Some("x"));
        assert_eq!(deck_id_of("show/x/media/deck.json"), None);
        assert_eq!(deck_id_of("show/exports/a.pdf"), None);
        assert_eq!(export_key("a.pdf"), "show/exports/a.pdf");
        assert_eq!(media_key("x", "media/a.png"), "show/x/media/a.png");
    }
}
