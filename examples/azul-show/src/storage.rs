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
/// The variable naming the data root (a test, a second profile).
pub const DATA_VAR: &str = "AZSHOW_DATA";

/// `show/<id>/deck.json`.
#[must_use]
pub fn deck_key(id: &str) -> String {
    format!("{APP_FOLDER}/{id}/deck.json")
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
            let page = drive
                .list(&ListRequest::folder(&format!("{APP_FOLDER}/")))
                .map_err(why)?;
            let mut decks = Vec::new();
            for folder in page.folders {
                let id = folder
                    .trim_end_matches('/')
                    .rsplit('/')
                    .next()
                    .unwrap_or_default()
                    .to_string();
                let key = deck_key(&id);
                let Ok(bytes) = drive.get(&key) else {
                    continue;
                };
                let Ok(deck) = Deck::from_json(&String::from_utf8_lossy(&bytes)) else {
                    continue;
                };
                let modified = drive.head(&key).ok().and_then(|o| o.modified);
                decks.push(DeckSummary {
                    id,
                    title: deck.title.clone(),
                    slides: deck.slides.len(),
                    modified,
                });
            }
            decks.sort_by(|a, b| b.modified.cmp(&a.modified).then_with(|| a.title.cmp(&b.title)));
            Ok(decks)
        })()),
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

/// The data root: `AZSHOW_DATA`, else `<the user's data folder>/azul`, else
/// `./azul-data`.
#[must_use]
pub fn data_root(user_data_dir: Option<PathBuf>) -> PathBuf {
    if let Some(v) = std::env::var(DATA_VAR).ok().map(|v| v.trim().to_string()).filter(|v| !v.is_empty()) {
        return PathBuf::from(v);
    }
    match user_data_dir {
        Some(dir) => dir.join("azul"),
        None => PathBuf::from("azul-data"),
    }
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
    fn the_data_root_is_the_users_data_folder_unless_the_variable_says_otherwise() {
        if std::env::var(DATA_VAR).is_err() {
            assert_eq!(
                data_root(Some(PathBuf::from("/home/u/.local/share"))),
                PathBuf::from("/home/u/.local/share/azul")
            );
        }
        assert_eq!(deck_key("x"), "show/x/deck.json");
        assert_eq!(media_key("x", "media/a.png"), "show/x/media/a.png");
    }
}
