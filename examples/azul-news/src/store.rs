//! AzNews' files in the data tree - the layout of the user's S3 bucket later:
//!
//! ```text
//! news/settings.json                 the kit's settings (azul-appkit)
//! news/subscriptions.opml            the subscription list, folders, AzNews' feed ids (azId)
//! news/feeds/<id>/feed.json          the feed's meta: its title, the HTTP validators, the last error
//! news/feeds/<id>/items.json         its articles (newest first)
//! news/feeds/<id>/state.json         what was read, starred, kept for later
//! ```
//!
//! Every read and write is a [`FileJob`] run on an azul Thread through azul-storage's drive
//! (`azul_appkit::ui::spawn_file_jobs`); this module only says which keys and turns the files
//! into a [`Library`] and back.

use azul_appkit::files::FileJob;
use serde::{Deserialize, Serialize};

use crate::{
    feed::Item,
    library::{FeedData, FeedMeta, Library},
    opml,
    state::ReadState,
};

#[cfg(test)]
use crate::opml::Subscription;

/// The app's folder in the data tree.
pub const APP_FOLDER: &str = "news";
/// The subscription list.
pub const SUBSCRIPTIONS_KEY: &str = "news/subscriptions.opml";
/// Where the feeds' folders are.
pub const FEEDS_PREFIX: &str = "news/feeds/";
/// The title of AzNews' own OPML file.
pub const OPML_TITLE: &str = "AzNews subscriptions";
/// The `format` of an `items.json`.
pub const ITEMS_FORMAT: &str = "aznews.items";
/// The `items.json` version this AzNews writes.
pub const ITEMS_VERSION: u64 = 1;

/// `news/feeds/<id>/feed.json`
#[must_use]
pub fn meta_key(id: &str) -> String {
    format!("{FEEDS_PREFIX}{id}/feed.json")
}

/// `news/feeds/<id>/items.json`
#[must_use]
pub fn items_key(id: &str) -> String {
    format!("{FEEDS_PREFIX}{id}/items.json")
}

/// `news/feeds/<id>/state.json`
#[must_use]
pub fn state_key(id: &str) -> String {
    format!("{FEEDS_PREFIX}{id}/state.json")
}

/// The feed id and the file name of a key under [`FEEDS_PREFIX`] (`None` for any other key).
#[must_use]
pub fn split_key(key: &str) -> Option<(&str, &str)> {
    let rest = key.strip_prefix(FEEDS_PREFIX)?;
    let (id, file) = rest.split_once('/')?;
    if id.is_empty() || file.is_empty() || file.contains('/') {
        return None;
    }
    Some((id, file))
}

/// The `items.json` file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
struct ItemsFile {
    format: String,
    version: u64,
    items: Vec<Item>,
}

/// The text of an `items.json`.
#[must_use]
pub fn items_to_json(items: &[Item]) -> String {
    let file = ItemsFile {
        format: ITEMS_FORMAT.to_string(),
        version: ITEMS_VERSION,
        items: items.to_vec(),
    };
    serde_json::to_string(&file).unwrap_or_default()
}

/// The articles of an `items.json`.
///
/// # Errors
/// A sentence when the file cannot be read.
pub fn items_from_json(text: &str) -> Result<Vec<Item>, String> {
    serde_json::from_str::<ItemsFile>(text)
        .map(|file| file.items)
        .map_err(|e| format!("the articles could not be read: {e}"))
}

/// The jobs that read the whole library when the window opens.
#[must_use]
pub fn load_jobs() -> Vec<FileJob> {
    vec![
        FileJob::Get {
            key: SUBSCRIPTIONS_KEY.to_string(),
        },
        FileJob::GetAll {
            prefix: FEEDS_PREFIX.to_string(),
            suffix: ".json".to_string(),
        },
    ]
}

/// What [`load`] read.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Loaded {
    pub library: Library,
    /// What could not be read, one sentence each.
    pub problems: Vec<String>,
    /// Subscriptions got an id (a list from elsewhere): the list must be written back.
    pub minted: bool,
}

/// The library from the subscription list (`None`: there is none yet) and the files under
/// [`FEEDS_PREFIX`]. A subscription without an id gets one (`new_id`); a feed folder no
/// subscription names is left alone.
#[must_use]
pub fn load(
    subscriptions: Option<&[u8]>,
    files: &[(String, Vec<u8>)],
    new_id: &mut dyn FnMut() -> String,
) -> Loaded {
    let mut loaded = Loaded::default();
    let subs = match subscriptions.map(opml::parse) {
        None => Vec::new(),
        Some(Ok(subs)) => subs,
        Some(Err(e)) => {
            loaded.problems.push(format!("{SUBSCRIPTIONS_KEY}: {e}"));
            Vec::new()
        }
    };
    for mut sub in subs {
        if sub.id.trim().is_empty() {
            sub.id = new_id();
            loaded.minted = true;
        }
        loaded.library.subscribe(sub);
    }
    for (key, bytes) in files {
        let Some((id, file)) = split_key(key) else {
            continue;
        };
        let Some(index) = loaded.library.feed_index(id) else {
            // A folder no subscription names (unsubscribed elsewhere): left alone.
            continue;
        };
        let text = String::from_utf8_lossy(bytes);
        let feed = &mut loaded.library.feeds[index];
        match file {
            "feed.json" => match FeedMeta::from_json(&text) {
                Some(meta) => feed.meta = meta,
                None => loaded
                    .problems
                    .push(format!("{key}: the feed's details could not be read")),
            },
            "items.json" => match items_from_json(&text) {
                Ok(items) => feed.items = items,
                Err(e) => loaded.problems.push(format!("{key}: {e}")),
            },
            "state.json" => {
                let (state, problem) = ReadState::from_json(&text);
                feed.state = state;
                if let Some(p) = problem {
                    loaded.problems.push(format!("{key}: {p}"));
                }
            }
            _ => {}
        }
    }
    loaded
}

/// The job that writes the subscription list.
#[must_use]
pub fn subscriptions_job(library: &Library) -> FileJob {
    FileJob::Put {
        key: SUBSCRIPTIONS_KEY.to_string(),
        bytes: opml::write(&library.subscriptions(), OPML_TITLE).into_bytes(),
    }
}

/// The job that writes one feed's meta.
#[must_use]
pub fn meta_job(feed: &FeedData) -> FileJob {
    FileJob::Put {
        key: meta_key(&feed.sub.id),
        bytes: feed.meta.to_json().into_bytes(),
    }
}

/// The job that writes one feed's articles.
#[must_use]
pub fn items_job(feed: &FeedData) -> FileJob {
    FileJob::Put {
        key: items_key(&feed.sub.id),
        bytes: items_to_json(&feed.items).into_bytes(),
    }
}

/// The job that writes one feed's marks.
#[must_use]
pub fn state_job(feed: &FeedData) -> FileJob {
    FileJob::Put {
        key: state_key(&feed.sub.id),
        bytes: feed.state.to_json().into_bytes(),
    }
}

/// Every file of one feed.
#[must_use]
pub fn feed_jobs(feed: &FeedData) -> Vec<FileJob> {
    vec![meta_job(feed), items_job(feed), state_job(feed)]
}

/// The jobs that remove a feed's files (it was unsubscribed).
#[must_use]
pub fn delete_jobs(id: &str) -> Vec<FileJob> {
    [meta_key(id), items_key(id), state_key(id)]
        .into_iter()
        .map(|key| FileJob::Delete { key })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn library() -> Library {
        let mut lib = Library::default();
        for (id, title, folder) in [("f1", "Example Weekly", "Tech"), ("f2", "Bakery", "")] {
            let i = lib.subscribe(Subscription {
                id: id.into(),
                title: title.into(),
                url: format!("https://{id}.example.org/feed"),
                site: format!("https://{id}.example.org/"),
                folder: folder.into(),
                paused: false,
            });
            lib.feeds[i].items = vec![Item {
                id: format!("{id}-a"),
                title: "An article".into(),
                published: Some(1_790_757_720),
                seen: 1_790_757_800,
                ..Item::default()
            }];
            lib.feeds[i].meta.etag = format!("\"{id}\"");
            lib.feeds[i].state.toggle_starred(&format!("{id}-a"));
        }
        lib
    }

    /// What the jobs would write, as the files a load reads back.
    fn written(lib: &Library) -> (Vec<u8>, Vec<(String, Vec<u8>)>) {
        let mut subscriptions = Vec::new();
        let mut files = Vec::new();
        let mut jobs = vec![subscriptions_job(lib)];
        for feed in &lib.feeds {
            jobs.extend(feed_jobs(feed));
        }
        for job in jobs {
            match job {
                FileJob::Put { key, bytes } if key == SUBSCRIPTIONS_KEY => subscriptions = bytes,
                FileJob::Put { key, bytes } => files.push((key, bytes)),
                other => panic!("not a write: {other:?}"),
            }
        }
        (subscriptions, files)
    }

    #[test]
    fn the_keys_are_the_layout_of_the_bucket() {
        assert_eq!(meta_key("abc"), "news/feeds/abc/feed.json");
        assert_eq!(items_key("abc"), "news/feeds/abc/items.json");
        assert_eq!(state_key("abc"), "news/feeds/abc/state.json");
        assert_eq!(
            split_key("news/feeds/abc/state.json"),
            Some(("abc", "state.json"))
        );
        assert_eq!(split_key("news/settings.json"), None);
        assert_eq!(split_key("news/feeds/abc/deeper/x.json"), None);
        assert_eq!(
            load_jobs(),
            vec![
                FileJob::Get {
                    key: SUBSCRIPTIONS_KEY.to_string()
                },
                FileJob::GetAll {
                    prefix: FEEDS_PREFIX.to_string(),
                    suffix: ".json".to_string()
                },
            ]
        );
    }

    #[test]
    fn what_is_written_loads_back_the_same() {
        let lib = library();
        let (subscriptions, files) = written(&lib);
        let loaded = load(Some(&subscriptions), &files, &mut || {
            panic!("every feed has an id")
        });
        assert_eq!(loaded.problems, Vec::<String>::new());
        assert!(!loaded.minted);
        assert_eq!(loaded.library.feeds.len(), 2);
        for (a, b) in loaded.library.feeds.iter().zip(&lib.feeds) {
            assert_eq!(a.sub, b.sub);
            assert_eq!(a.items, b.items);
            assert_eq!(a.state.starred, b.state.starred);
            assert_eq!(a.meta.etag, b.meta.etag);
        }
    }

    #[test]
    fn a_list_from_elsewhere_gets_ids_and_asks_to_be_written() {
        let opml = opml::write(
            &[Subscription {
                title: "No id".into(),
                url: "https://x.example.org/feed".into(),
                ..Subscription::default()
            }],
            "Imported",
        );
        let mut n = 0;
        let loaded = load(Some(opml.as_bytes()), &[], &mut || {
            n += 1;
            format!("minted-{n}")
        });
        assert!(loaded.minted);
        assert_eq!(loaded.library.feeds[0].sub.id, "minted-1");
        assert!(loaded.library.feeds[0].items.is_empty());
    }

    #[test]
    fn a_broken_file_is_reported_and_the_rest_loads() {
        let lib = library();
        let (subscriptions, mut files) = written(&lib);
        for (key, bytes) in &mut files {
            if key == "news/feeds/f1/items.json" {
                *bytes = b"{ broken".to_vec();
            }
        }
        files.push((
            "news/feeds/orphan/items.json".to_string(),
            items_to_json(&[]).into_bytes(),
        ));
        let loaded = load(Some(&subscriptions), &files, &mut || "x".to_string());
        assert_eq!(loaded.problems.len(), 1, "{:?}", loaded.problems);
        assert!(loaded.library.feeds[0].items.is_empty());
        assert_eq!(loaded.library.feeds[1].items.len(), 1);
        assert_eq!(
            loaded.library.feeds.len(),
            2,
            "an orphan folder is no subscription"
        );
        assert_eq!(
            load(None, &[], &mut || "x".to_string()).library.feeds.len(),
            0
        );
    }

    #[test]
    fn unsubscribing_deletes_the_three_files() {
        assert_eq!(
            delete_jobs("f1"),
            vec![
                FileJob::Delete {
                    key: meta_key("f1")
                },
                FileJob::Delete {
                    key: items_key("f1")
                },
                FileJob::Delete {
                    key: state_key("f1")
                },
            ]
        );
    }

    #[test]
    fn the_items_file_round_trips() {
        let items = library().feeds[0].items.clone();
        let text = items_to_json(&items);
        assert!(text.contains("aznews.items"));
        assert_eq!(items_from_json(&text), Ok(items));
        assert!(items_from_json("[1, 2").is_err());
    }
}
