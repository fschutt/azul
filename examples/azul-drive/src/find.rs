//! The search box's search, as plain data: the open folder and every folder below it searched
//! by azul-search (ripgrep's walker and searcher - AzCode's find in files runs on it too) - the
//! names first, then, with "File contents" on, the files whose lines hold the text - or, on a
//! cloud drive, the names of a recursive listing (slower; no contents: the files would have to
//! be downloaded). The results stream in as rows ([`FindState`]): the folder view shows them in
//! the Details layout with their folder and the line they matched on.
//!
//! The jobs (`jobs::run_find`, `jobs::run_find_remote`) make [`Found`] rows on the worker
//! thread; everything here is tested without a window.

use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{atomic::AtomicBool, Arc},
};

use azul_search::{ContentHit, Filters, Limits, NameHit, NameMatcher, Pattern, PatternKind, Request};
use azul_storage::{key, ListPage};

use crate::{browse::Entry, listing};

/// The most results a search shows ("the first ones" after that).
pub const FIND_MAX: usize = 10_000;

/// A file larger than this is not read for its contents (its name is still searched).
pub const FIND_MAX_FILE: u64 = 64 * 1024 * 1024;

/// A search starts this long after the last key typed (each key stops the search before it,
/// which has not read a folder yet).
pub const DEBOUNCE_MS: u64 = 150;

/// The first batch of results is handed over as soon as it holds this many (the first screen),
/// then a batch every [`listing::BATCH_MS`].
pub const FIRST_BATCH: usize = 64;

/// The characters of a matching line kept before the match in the Match column.
pub const PREVIEW_LEAD: usize = 30;

/// The storage crate's half-written temporary files (`.<name>.azul-storage-<pid>-<n>.tmp`),
/// which no listing shows, as an exclude glob.
pub const TEMP_GLOB: &str = ".*.azul-storage-*.tmp";

/// Whether the text of the search box searches contents too when "File contents" is on: not a
/// glob, which names files.
#[must_use]
pub fn searches_contents(query: &str) -> bool {
    Pattern::guess(query).kind != PatternKind::Glob
}

/// The search of `root` (the open folder on this computer) for the search box's `query`: names
/// (a part of the name, a glob with a wildcard), contents too when `contents` is on; hidden
/// items when they show, ignored files (.gitignore, .ignore) skipped when `ignore_files`; the
/// storage crate's temporary files never, and at a drive's root not its bookkeeping
/// (`.azlin/`) - what a listing leaves out. One line of a file previews it.
#[must_use]
pub fn local_request(
    root: PathBuf,
    query: &str,
    contents: bool,
    show_hidden: bool,
    ignore_files: bool,
    at_drive_root: bool,
) -> Request {
    let mut exclude = vec![TEMP_GLOB.to_string()];
    if at_drive_root {
        exclude.push(format!("/{}/", azul_storage::manifest::MANIFEST_DIR));
    }
    let request = Request::new(root)
        .with_names(Pattern::guess(query))
        .with_filters(Filters {
            include: Vec::new(),
            exclude,
            hidden: show_hidden,
            ignore_files,
        })
        .with_limits(Limits {
            max_results: FIND_MAX,
            max_matches: FIND_MAX,
            max_lines_per_file: 1,
            max_file_size: FIND_MAX_FILE,
        });
    if contents && searches_contents(query) {
        request.with_contents(Pattern::literal(query))
    } else {
        request
    }
}

/// The line a file's contents matched on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoundLine {
    /// The line's number, from 1.
    pub line: u64,
    /// The line (a long one cut around the match).
    pub text: String,
    /// The match in `text` (bytes).
    pub start: usize,
    pub end: usize,
}

impl FoundLine {
    /// The Match column's text: before the match (without the indentation; a long lead cut to
    /// its last [`PREVIEW_LEAD`] characters after an ellipsis), the match, after it.
    #[must_use]
    pub fn preview(&self) -> (String, String, String) {
        let text = self.text.as_str();
        let start = self.start.min(text.len());
        let end = self.end.clamp(start, text.len());
        let (Some(lead), Some(matched), Some(rest)) =
            (text.get(..start), text.get(start..end), text.get(end..))
        else {
            return (String::new(), text.to_string(), String::new());
        };
        let lead = lead.trim_start();
        let count = lead.chars().count();
        let before = if count > PREVIEW_LEAD {
            let kept: String = lead.chars().skip(count - PREVIEW_LEAD).collect();
            format!("\u{2026}{kept}")
        } else {
            lead.to_string()
        };
        (before, matched.to_string(), rest.to_string())
    }
}

/// One result as the worker hands it over: its row, and the line its contents matched on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub entry: Entry,
    pub line: Option<FoundLine>,
}

/// A row of a result whose key is `key` (its size and date unknown until the rows in view are
/// stat'ed, as a scanned row's).
fn result_row(key: String, name: String, is_folder: bool) -> Entry {
    Entry {
        key,
        name,
        is_folder,
        size: None,
        modified: None,
        etag: None,
        known: false,
    }
}

/// A name azul-search found below the folder `prefix` (the drive's key of the searched folder).
#[must_use]
pub fn found_name(prefix: &str, hit: NameHit) -> Found {
    Found {
        entry: result_row(format!("{prefix}{}", hit.path), hit.name, hit.is_dir),
        line: None,
    }
}

/// A file azul-search found by its contents below the folder `prefix`: its row and its first
/// matching line.
#[must_use]
pub fn found_content(prefix: &str, hit: ContentHit) -> Found {
    let key = format!("{prefix}{}", hit.path);
    let name = key::last_segment(&key).to_string();
    let line = hit.lines.into_iter().next().map(|line| {
        let (start, end) = line
            .ranges
            .first()
            .map_or((0, 0), |r| (r.0 - line.text_offset, r.1 - line.text_offset));
        FoundLine {
            line: line.line,
            text: line.text,
            start,
            end,
        }
    });
    Found {
        entry: result_row(key, name, false),
        line,
    }
}

/// Whether a path has a hidden part (a name starting with a dot).
fn hidden_path(path: &str) -> bool {
    path.split('/').any(|part| part.starts_with('.'))
}

/// The results of one page of a cloud drive's recursive listing of the folder `prefix`: the
/// files whose names match (with the size, date and tag the listing has) and the folders on the
/// way to them whose names match - each folder once over every page (`seen`); a folder marker
/// (a key ending in `/`) is its folder. Hidden ones only when `show_hidden`.
#[must_use]
pub fn remote_names(
    page: &ListPage,
    prefix: &str,
    matcher: &NameMatcher,
    show_hidden: bool,
    seen: &mut HashSet<String>,
) -> Vec<Found> {
    let mut found = Vec::new();
    for object in &page.objects {
        let Some(rest) = object.key.strip_prefix(prefix) else {
            continue;
        };
        if rest.is_empty() {
            continue;
        }
        let mut at = 0;
        while let Some(i) = rest[at..].find('/') {
            let folder = &rest[..at + i];
            at += i + 1;
            if !seen.insert(folder.to_string()) || (!show_hidden && hidden_path(folder)) {
                continue;
            }
            let name = key::last_segment(folder);
            if matcher.find(name, folder).is_some() {
                let mut entry =
                    result_row(format!("{prefix}{folder}/"), name.to_string(), true);
                entry.known = true;
                found.push(Found { entry, line: None });
            }
        }
        if rest.ends_with('/') || (!show_hidden && hidden_path(rest)) {
            continue;
        }
        let name = key::last_segment(rest);
        if matcher.find(name, rest).is_some() {
            found.push(Found {
                entry: Entry {
                    key: object.key.clone(),
                    name: name.to_string(),
                    is_folder: false,
                    size: Some(object.size),
                    modified: object.modified,
                    etag: object.etag.clone(),
                    known: true,
                },
                line: None,
            });
        }
    }
    found
}

/// The Folder column: where a result is, from the drive's root (`""`: at the root).
#[must_use]
pub fn folder_of(key: &str) -> &str {
    key::folder_of(key.strip_suffix('/').unwrap_or(key)).trim_end_matches('/')
}

/// The walk a search is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FindPhase {
    /// Waiting for the typing to pause.
    #[default]
    Waiting,
    /// The names.
    Names,
    /// The files' contents.
    Contents,
}

/// How a search ended.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FindEnd {
    /// It stopped at [`FIND_MAX`] results: there were more.
    pub limited: bool,
    /// Why it stopped early (a listing that failed).
    pub error: Option<String>,
}

/// The search open in the window: what it looks for, its results so far.
pub struct FindState {
    /// The search box's text, trimmed.
    pub query: String,
    /// The files' contents are searched too ("File contents").
    pub contents: bool,
    /// A cloud drive: names over a listing, slower.
    pub remote: bool,
    /// Which search this is: a batch of an older one is dropped.
    pub serial: u64,
    /// Raised to stop it (a new key, Escape, another folder).
    pub cancel: Arc<AtomicBool>,
    /// The rows found, in the order they came: names first, then contents.
    pub rows: Vec<Entry>,
    /// The line a row's contents matched on, by key.
    pub lines: HashMap<String, FoundLine>,
    /// Where each key is in `rows`.
    index: HashMap<String, usize>,
    pub phase: FindPhase,
    /// Files read (a cloud drive: keys listed) so far.
    pub searched: usize,
    /// How it ended (`None` while it runs).
    pub end: Option<FindEnd>,
    /// The rows whose size and date were asked for (the ones in view).
    pub stats_asked: HashSet<String>,
}

impl FindState {
    #[must_use]
    pub fn new(
        query: String,
        contents: bool,
        remote: bool,
        serial: u64,
        cancel: Arc<AtomicBool>,
    ) -> FindState {
        FindState {
            query,
            contents,
            remote,
            serial,
            cancel,
            rows: Vec::new(),
            lines: HashMap::new(),
            index: HashMap::new(),
            phase: FindPhase::Waiting,
            searched: 0,
            end: None,
            stats_asked: HashSet::new(),
        }
    }

    #[must_use]
    pub fn running(&self) -> bool {
        self.end.is_none()
    }

    /// A batch's results join the rows: a new key at the end, a known one keeps its place (a
    /// content match of a row found by its name brings it its line; its first line stays).
    pub fn merge(&mut self, batch: Vec<Found>) {
        for found in batch {
            let key = found.entry.key.clone();
            if !self.index.contains_key(&key) {
                self.index.insert(key.clone(), self.rows.len());
                self.rows.push(found.entry);
            }
            if let Some(line) = found.line {
                self.lines.entry(key).or_insert(line);
            }
        }
    }

    /// The row of `key`.
    #[must_use]
    pub fn entry(&self, key: &str) -> Option<&Entry> {
        self.index.get(key).and_then(|&i| self.rows.get(i))
    }

    /// The status line: "Searching... 1,234 found" while it runs (the contents' walk and a
    /// cloud drive's slower one say so), the count once it is done.
    #[must_use]
    pub fn status_text(&self) -> String {
        let n = listing::grouped_digits(self.rows.len());
        match &self.end {
            None if self.remote => format!("Searching names in the cloud (slower)... {n} found"),
            None if self.phase == FindPhase::Contents => {
                format!("Searching file contents... {n} found")
            }
            None => format!("Searching... {n} found"),
            Some(FindEnd {
                error: Some(error), ..
            }) => format!("The search stopped: {error}"),
            Some(_) if self.rows.is_empty() => String::from("No items match your search."),
            Some(end) => {
                let noun = if self.rows.len() == 1 { "item" } else { "items" };
                let more = if end.limited { " (the first ones)" } else { "" };
                format!("{n} {noun} found{more}")
            }
        }
    }

    /// What the empty results say: still searching, or what was searched.
    #[must_use]
    pub fn empty_text(&self) -> (&'static str, String) {
        if self.running() {
            return ("Searching...", String::new());
        }
        let what = if self.remote {
            "The search looked at the names in this folder and every folder below it; a cloud \
             drive's files are not read."
        } else if self.contents {
            "The search looked at the names and the contents of the files in this folder and \
             every folder below it."
        } else {
            "The search looked at the names in this folder and every folder below it; File \
             contents (the Search tab) reads the files too."
        };
        ("No items match your search.", what.to_string())
    }
}
