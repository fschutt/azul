//! The search box's search, as plain data: the open folder and every folder below it searched
//! by azul-search (ripgrep's walker and searcher - AzCode's find in files runs on it too) - the
//! names first, then, with "File contents" on, the files whose lines hold the text - or, on a
//! cloud drive, the names of a recursive listing (slower; no contents: the files would have to
//! be downloaded) - its folders listed side by side, its last full listing kept in the cache
//! folder ([`CachedListing`]) and shown at once by the next search while the fresh one comes.
//! A drive on this computer may have a full-text index ([`IndexAsk`], azul-search-index): its
//! contents are asked from it first, the files it has not read as they are now read after.
//! The results stream in as rows ([`FindState`]): the folder view shows them in the Details
//! layout with their folder and the line they matched on.
//!
//! The jobs (`jobs::run_find`, `jobs::run_find_remote`) make [`Found`] rows on the worker
//! thread; everything here is tested without a window.

use std::{
    collections::{HashMap, HashSet},
    fs,
    io::{self, Write as _},
    path::{Path, PathBuf},
    sync::{atomic::AtomicBool, Arc},
};

use azul_search::{
    ContentHit, ContentMatcher, Filters, Limits, NameHit, NameMatcher, Pattern, PatternKind,
    Refine, Request,
};
use azul_appkit::l10n::{grouped, Phrase, Text};
use azul_search_index::{IndexStatus, UpdateProgress};
use azul_storage::{key, ListPage, ObjectInfo};
use chrono::{DateTime, Datelike, Days, NaiveDate, TimeZone};
use serde::{Deserialize, Serialize};

use crate::{
    browse::{self, Entry, Place, Sort},
    listing,
    model::Settings,
};

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

/// How a search looks: the Search tab's settings and refine, the View tab's hidden items.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FindOptions {
    /// The files' contents too (a folder on this computer only).
    pub contents: bool,
    /// Hidden items too.
    pub show_hidden: bool,
    /// What .gitignore / .ignore files name is passed over.
    pub ignore_files: bool,
    /// Every folder below the searched one ("All subfolders"); `false`: its own items
    /// ("Current folder").
    pub subfolders: bool,
    /// Only these kinds, sizes and dates.
    pub refine: Refine,
}

impl Default for FindOptions {
    fn default() -> Self {
        FindOptions {
            contents: false,
            show_hidden: false,
            ignore_files: true,
            subfolders: true,
            refine: Refine::default(),
        }
    }
}

/// The search of `root` (the open folder on this computer) for the search box's `query`: names
/// (a part of the name, a glob with a wildcard), contents too when asked; hidden items when
/// they show, ignored files (.gitignore, .ignore) skipped when asked, the folder's own items or
/// every folder below it, the refine; the storage crate's temporary files never, and at a
/// drive's root not its bookkeeping (`.azlin/`) - what a listing leaves out. One line of a file
/// previews it.
#[must_use]
pub fn local_request(
    root: PathBuf,
    query: &str,
    at_drive_root: bool,
    options: &FindOptions,
) -> Request {
    let mut exclude = vec![TEMP_GLOB.to_string()];
    if at_drive_root {
        exclude.push(format!("/{}/", azul_storage::manifest::MANIFEST_DIR));
    }
    let contents = options.contents;
    let request = Request::new(root)
        .with_names(Pattern::guess(query))
        .with_filters(Filters {
            include: Vec::new(),
            exclude,
            hidden: options.show_hidden,
            ignore_files: options.ignore_files,
            max_depth: (!options.subfolders).then_some(1),
            refine: options.refine.clone(),
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

/// A row of a result whose key is `key`, with the size and date the search read with it (a row
/// whose stat failed knows neither, and the rows in view get theirs as a scanned row does).
fn result_row(
    key: String,
    name: String,
    is_folder: bool,
    size: Option<u64>,
    modified: Option<u64>,
) -> Entry {
    Entry {
        key,
        name,
        is_folder,
        size: if is_folder { None } else { size },
        modified,
        etag: None,
        known: modified.is_some(),
    }
}

/// A name azul-search found below the folder `prefix` (the drive's key of the searched folder).
#[must_use]
pub fn found_name(prefix: &str, hit: NameHit) -> Found {
    Found {
        entry: result_row(
            format!("{prefix}{}", hit.path),
            hit.name,
            hit.is_dir,
            hit.size,
            hit.modified,
        ),
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
        entry: result_row(key, name, false, hit.size, hit.modified),
        line,
    }
}

/// Whether a path has a hidden part (a name starting with a dot).
pub(crate) fn hidden_path(path: &str) -> bool {
    path.split('/').any(|part| part.starts_with('.'))
}

/// The results of one page of a cloud drive's listing of the folder `prefix` (recursive, or -
/// "Current folder" - its own level, whose folders are common prefixes): the files whose names
/// match (with the size, date and tag the listing has) and the folders on the way to them whose
/// names match - each folder once over every page (`seen`); a folder marker (a key ending in
/// `/`) is its folder. Hidden ones only when they show; the refine holds (a folder has no size
/// or date here: a size, a kind or a date refine leaves it out).
#[must_use]
pub fn remote_names(
    page: &ListPage,
    prefix: &str,
    matcher: &NameMatcher,
    options: &FindOptions,
    seen: &mut HashSet<String>,
) -> Vec<Found> {
    let show_hidden = options.show_hidden;
    let refine = &options.refine;
    let mut found = Vec::new();
    let mut folder_row = |found: &mut Vec<Found>, folder: &str| {
        if !seen.insert(folder.to_string()) || (!show_hidden && hidden_path(folder)) {
            return;
        }
        let name = key::last_segment(folder);
        if matcher.find(name, folder).is_some()
            && refine.admits_name(name, true)
            && refine.admits_facts(None, None)
        {
            let mut entry =
                result_row(format!("{prefix}{folder}/"), name.to_string(), true, None, None);
            // A bucket's folder is a common prefix: it has no date to learn.
            entry.known = true;
            found.push(Found { entry, line: None });
        }
    };
    for common in &page.folders {
        if let Some(rest) = common.strip_prefix(prefix) {
            let folder = rest.trim_end_matches('/');
            if !folder.is_empty() {
                folder_row(&mut found, folder);
            }
        }
    }
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
            folder_row(&mut found, folder);
        }
        if rest.ends_with('/') || (!show_hidden && hidden_path(rest)) {
            continue;
        }
        let name = key::last_segment(rest);
        if matcher.find(name, rest).is_some()
            && refine.admits_name(name, false)
            && refine.admits_facts(Some(object.size), object.modified)
        {
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

/// The key of a row of a search of This PC: its drive's id and its key there (a NUL between
/// them, which no key holds).
#[must_use]
pub fn pc_key(drive_id: &str, key: &str) -> String {
    format!("{drive_id}\u{0}{key}")
}

/// A This PC row's drive id and key; `None` for a row of the open drive.
#[must_use]
pub fn split_pc_key(row: &str) -> Option<(&str, &str)> {
    row.split_once('\u{0}')
}

/// The sync state of the result row `row` (the sync's answer): a This PC row's by its own
/// drive, any other's by the open drive `open_drive`; `None` where nothing syncs.
#[must_use]
pub fn result_sync(
    sync: &dyn crate::sync_lookup::SyncLookup,
    open_drive: Option<&str>,
    row: &str,
) -> Option<crate::sync_lookup::SyncState> {
    match split_pc_key(row) {
        Some((drive, key)) => sync.sync_state(drive, key),
        None => sync.sync_state(open_drive?, row),
    }
}

// ==== Refine (the Search tab's Date modified, Kind and Size) ====

/// Explorer's Date modified.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DateRefine {
    #[default]
    Any,
    Today,
    Yesterday,
    ThisWeek,
    LastWeek,
    ThisMonth,
    LastMonth,
    ThisYear,
    LastYear,
}

impl DateRefine {
    pub const ALL: [DateRefine; 9] = [
        DateRefine::Any,
        DateRefine::Today,
        DateRefine::Yesterday,
        DateRefine::ThisWeek,
        DateRefine::LastWeek,
        DateRefine::ThisMonth,
        DateRefine::LastMonth,
        DateRefine::ThisYear,
        DateRefine::LastYear,
    ];

    /// The choice's words (a key of the resources).
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            DateRefine::Any => "azdrive-refine-date-any",
            DateRefine::Today => "azdrive-refine-date-today",
            DateRefine::Yesterday => "azdrive-refine-date-yesterday",
            DateRefine::ThisWeek => "azdrive-refine-date-this-week",
            DateRefine::LastWeek => "azdrive-refine-date-last-week",
            DateRefine::ThisMonth => "azdrive-refine-date-this-month",
            DateRefine::LastMonth => "azdrive-refine-date-last-month",
            DateRefine::ThisYear => "azdrive-refine-date-this-year",
            DateRefine::LastYear => "azdrive-refine-date-last-year",
        }
    }
}

/// Explorer's Kind: the files of a kind's extensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KindRefine {
    #[default]
    Any,
    Document,
    Picture,
    Music,
    Video,
    Archive,
    Code,
    Mail,
}

impl KindRefine {
    pub const ALL: [KindRefine; 8] = [
        KindRefine::Any,
        KindRefine::Document,
        KindRefine::Picture,
        KindRefine::Music,
        KindRefine::Video,
        KindRefine::Archive,
        KindRefine::Code,
        KindRefine::Mail,
    ];

    /// The choice's words (a key of the resources).
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            KindRefine::Any => "azdrive-refine-kind-any",
            KindRefine::Document => "azdrive-refine-kind-document",
            KindRefine::Picture => "azdrive-refine-kind-picture",
            KindRefine::Music => "azdrive-refine-kind-music",
            KindRefine::Video => "azdrive-refine-kind-video",
            KindRefine::Archive => "azdrive-refine-kind-archive",
            KindRefine::Code => "azdrive-refine-kind-code",
            KindRefine::Mail => "azdrive-refine-kind-mail",
        }
    }

    /// The extensions of the kind (empty: any).
    #[must_use]
    pub fn extensions(self) -> &'static [&'static str] {
        match self {
            KindRefine::Any => &[],
            KindRefine::Document => &[
                "txt", "md", "rtf", "doc", "docx", "odt", "pdf", "xls", "xlsx", "ods", "csv",
                "ppt", "pptx", "odp", "epub", "pages", "numbers", "key",
            ],
            KindRefine::Picture => &[
                "jpg", "jpeg", "png", "gif", "bmp", "webp", "tif", "tiff", "heic", "heif", "svg",
                "raw", "psd",
            ],
            KindRefine::Music => &["mp3", "wav", "flac", "ogg", "m4a", "aac", "opus", "aiff"],
            KindRefine::Video => &["mp4", "m4v", "mov", "avi", "mkv", "webm", "wmv", "mpg"],
            KindRefine::Archive => &["zip", "gz", "tgz", "bz2", "xz", "7z", "rar", "tar", "zst"],
            KindRefine::Code => &[
                "rs", "js", "ts", "py", "c", "h", "cpp", "hpp", "go", "java", "kt", "swift", "sh",
                "toml", "json", "html", "css", "xml", "yaml", "yml",
            ],
            KindRefine::Mail => &["eml", "msg", "mbox"],
        }
    }
}

const KB: u64 = 1024;
const MB: u64 = 1024 * KB;
const GB: u64 = 1024 * MB;

/// Explorer's Size buckets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SizeRefine {
    #[default]
    Any,
    Empty,
    Tiny,
    Small,
    Medium,
    Large,
    Huge,
    Gigantic,
}

impl SizeRefine {
    pub const ALL: [SizeRefine; 8] = [
        SizeRefine::Any,
        SizeRefine::Empty,
        SizeRefine::Tiny,
        SizeRefine::Small,
        SizeRefine::Medium,
        SizeRefine::Large,
        SizeRefine::Huge,
        SizeRefine::Gigantic,
    ];

    /// The choice's words (a key of the resources).
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            SizeRefine::Any => "azdrive-refine-size-any",
            SizeRefine::Empty => "azdrive-refine-size-empty",
            SizeRefine::Tiny => "azdrive-refine-size-tiny",
            SizeRefine::Small => "azdrive-refine-size-small",
            SizeRefine::Medium => "azdrive-refine-size-medium",
            SizeRefine::Large => "azdrive-refine-size-large",
            SizeRefine::Huge => "azdrive-refine-size-huge",
            SizeRefine::Gigantic => "azdrive-refine-size-gigantic",
        }
    }

    /// The bucket's sizes, both ends included (`None`: open).
    #[must_use]
    pub fn range(self) -> (Option<u64>, Option<u64>) {
        match self {
            SizeRefine::Any => (None, None),
            SizeRefine::Empty => (Some(0), Some(0)),
            SizeRefine::Tiny => (Some(0), Some(16 * KB)),
            SizeRefine::Small => (Some(16 * KB), Some(MB)),
            SizeRefine::Medium => (Some(MB), Some(128 * MB)),
            SizeRefine::Large => (Some(128 * MB), Some(GB)),
            SizeRefine::Huge => (Some(GB), Some(4 * GB)),
            SizeRefine::Gigantic => (Some(4 * GB + 1), None),
        }
    }
}

/// The Search tab's refine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Refines {
    pub date: DateRefine,
    pub kind: KindRefine,
    pub size: SizeRefine,
}

impl Refines {
    /// Whether it lets everything through.
    #[must_use]
    pub fn is_any(&self) -> bool {
        *self == Refines::default()
    }

    /// azul-search's refine: the kind's extensions, the size bucket, the date range at `now`.
    #[must_use]
    pub fn to_refine<Tz: TimeZone>(&self, now: &DateTime<Tz>) -> Refine {
        let (min_size, max_size) = self.size.range();
        let (modified_from, modified_until) = match date_range(self.date, now) {
            Some((from, until)) => (Some(from), Some(until)),
            None => (None, None),
        };
        Refine {
            modified_from,
            modified_until,
            min_size,
            max_size,
            extensions: self
                .kind
                .extensions()
                .iter()
                .map(|e| (*e).to_string())
                .collect(),
        }
    }

    /// What the status line says of it: "Date modified: Today, Kind: Document".
    #[must_use]
    pub fn label(&self) -> Text {
        let mut parts = Vec::new();
        if self.date != DateRefine::Any {
            parts.push(("azdrive-refine-part-date", self.date.label()));
        }
        if self.kind != KindRefine::Any {
            parts.push(("azdrive-refine-part-kind", self.kind.label()));
        }
        if self.size != SizeRefine::Any {
            parts.push(("azdrive-refine-part-size", self.size.label()));
        }
        let mut text = Text::default();
        for (at, (name, chosen)) in parts.into_iter().enumerate() {
            if at > 0 {
                text = text.then(", ");
            }
            text = text.then(Text::key(name)).then(" ").then(Text::key(chosen));
        }
        text
    }
}

/// Midnight starting `day` in `zone`, as seconds since 1970 (the earlier one of a day that
/// begins twice; 0 before 1970).
fn midnight<Tz: TimeZone>(zone: &Tz, day: NaiveDate) -> Option<u64> {
    let start = zone.from_local_datetime(&day.and_hms_opt(0, 0, 0)?).earliest()?;
    Some(u64::try_from(start.timestamp()).unwrap_or(0))
}

/// The first day of the month `months` after (negative: before) the month of `day`.
fn month_start(day: NaiveDate, months: i32) -> Option<NaiveDate> {
    let index = day.year() * 12 + i32::try_from(day.month0()).ok()? + months;
    let year = index.div_euclid(12);
    let month = u32::try_from(index.rem_euclid(12)).ok()? + 1;
    NaiveDate::from_ymd_opt(year, month, 1)
}

/// The seconds a date refine stands for at `now` - from a midnight to a midnight in `now`'s
/// zone, the end left out -; `None` for any date. A week starts on Monday.
#[must_use]
pub fn date_range<Tz: TimeZone>(date: DateRefine, now: &DateTime<Tz>) -> Option<(u64, u64)> {
    let zone = now.timezone();
    let today = now.date_naive();
    let monday = today.checked_sub_days(Days::new(u64::from(
        today.weekday().num_days_from_monday(),
    )))?;
    let year_start = |years: i32| NaiveDate::from_ymd_opt(today.year() + years, 1, 1);
    let (from, until) = match date {
        DateRefine::Any => return None,
        DateRefine::Today => (today, today.checked_add_days(Days::new(1))?),
        DateRefine::Yesterday => (today.checked_sub_days(Days::new(1))?, today),
        DateRefine::ThisWeek => (monday, monday.checked_add_days(Days::new(7))?),
        DateRefine::LastWeek => (monday.checked_sub_days(Days::new(7))?, monday),
        DateRefine::ThisMonth => (month_start(today, 0)?, month_start(today, 1)?),
        DateRefine::LastMonth => (month_start(today, -1)?, month_start(today, 0)?),
        DateRefine::ThisYear => (year_start(0)?, year_start(1)?),
        DateRefine::LastYear => (year_start(-1)?, year_start(0)?),
    };
    Some((midnight(&zone, from)?, midnight(&zone, until)?))
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
    /// A cloud drive's last full listing (kept in the cache folder): its names at once, while
    /// the fresh listing comes.
    Cached,
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
    /// The rows a cloud drive's last listing showed that the fresh, complete one has not got
    /// (deleted since): they go.
    pub stale: Vec<String>,
}

/// The search open in the window: what it looks for, its results so far.
pub struct FindState {
    /// The search box's text, trimmed.
    pub query: String,
    /// The files' contents are searched too ("File contents").
    pub contents: bool,
    /// A cloud drive: names over a listing, slower.
    pub remote: bool,
    /// An encrypted drive: its names from its drive index on this computer.
    pub drive_index: bool,
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
    /// The order a click on a column's header asked for; `None`: as found.
    pub sort: Option<Sort>,
    /// The rows' positions in `sort`'s order (while there is one).
    view: Vec<usize>,
    /// The jobs still running (This PC runs one per drive); the last one's end is the search's.
    pub pending: usize,
    /// What the jobs that ended so far came to.
    ended: Option<FindEnd>,
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
            drive_index: false,
            serial,
            cancel,
            rows: Vec::new(),
            lines: HashMap::new(),
            index: HashMap::new(),
            phase: FindPhase::Waiting,
            searched: 0,
            end: None,
            stats_asked: HashSet::new(),
            sort: None,
            view: Vec::new(),
            pending: 1,
            ended: None,
        }
    }

    /// The rows in the order they show: sorted by the column clicked, else as found.
    #[must_use]
    pub fn shown(&self) -> Vec<&Entry> {
        match self.sort {
            Some(_) => self.view.iter().filter_map(|&i| self.rows.get(i)).collect(),
            None => self.rows.iter().collect(),
        }
    }

    /// Sorts the rows by a column (`None`: back to the order they were found in).
    pub fn set_sort(&mut self, sort: Option<Sort>) {
        self.sort = sort;
        self.resort();
    }

    /// The view's order again (new rows, new sizes or dates).
    fn resort(&mut self) {
        let Some(sort) = self.sort else {
            self.view.clear();
            return;
        };
        let rows = &self.rows;
        let mut view: Vec<usize> = (0..rows.len()).collect();
        view.sort_by(|&a, &b| browse::compare_entries(&rows[a], &rows[b], sort));
        self.view = view;
    }

    /// The sizes and dates a stat found join the rows; how many changed.
    pub fn apply_stats(&mut self, stats: &[listing::Stat]) -> usize {
        let changed = listing::apply_stats(&mut self.rows, stats);
        if changed > 0 {
            self.resort();
        }
        changed
    }

    /// A job of the search ended with `end`: whether it was the last one (the search's end is
    /// then every job's - a limit or an error of one is the search's).
    pub fn job_ended(&mut self, end: FindEnd) -> bool {
        self.pending = self.pending.saturating_sub(1);
        let merged = match self.ended.take() {
            None => end,
            Some(before) => FindEnd {
                limited: before.limited || end.limited,
                error: before.error.or(end.error),
                stale: [before.stale, end.stale].concat(),
            },
        };
        if self.pending == 0 {
            self.end = Some(merged);
            true
        } else {
            self.ended = Some(merged);
            false
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
        self.resort();
    }

    /// Rows that went (a cloud drive's last listing had them, the fresh one has not): out of the
    /// results with their lines; the others keep their order.
    pub fn remove(&mut self, keys: &[String]) {
        if keys.is_empty() {
            return;
        }
        let gone: HashSet<&str> = keys.iter().map(String::as_str).collect();
        self.rows.retain(|row| !gone.contains(row.key.as_str()));
        for key in keys {
            self.lines.remove(key);
            self.stats_asked.remove(key);
        }
        self.index = self
            .rows
            .iter()
            .enumerate()
            .map(|(i, row)| (row.key.clone(), i))
            .collect();
        self.resort();
    }

    /// The row of `key`.
    #[must_use]
    pub fn entry(&self, key: &str) -> Option<&Entry> {
        self.index.get(key).and_then(|&i| self.rows.get(i))
    }

    /// The status line: "Searching... 1,234 found" while it runs (the contents' walk and a
    /// cloud drive's slower one say so), the count once it is done (`$count` the number,
    /// `$n` it grouped in the window's language).
    #[must_use]
    pub fn status_text(&self) -> Phrase {
        let said = match &self.end {
            None if self.phase == FindPhase::Contents => "azdrive-find-status-contents",
            None if self.drive_index => "azdrive-find-status-drive-names",
            None if self.remote && self.phase == FindPhase::Cached => "azdrive-find-status-cached",
            None if self.remote => "azdrive-find-status-cloud",
            None => "azdrive-find-status-searching",
            Some(FindEnd {
                error: Some(error), ..
            }) => {
                return Phrase::new("azdrive-find-status-stopped").arg("error", error.as_str());
            }
            Some(_) if self.rows.is_empty() => return Phrase::new("azdrive-find-none"),
            Some(end) if end.limited => "azdrive-find-status-found-first",
            Some(_) => "azdrive-find-status-found",
        };
        Phrase::new(said)
            .arg("count", self.rows.len())
            .arg("n", grouped(self.rows.len() as u64))
    }

    /// The note over a cloud or encrypted drive's results: where its names and contents come
    /// from (a key of the resources).
    #[must_use]
    pub fn cloud_note_text(&self) -> &'static str {
        match (self.drive_index, self.contents) {
            (true, true) => "azdrive-find-note-index-contents",
            (true, false) => "azdrive-find-note-index",
            (false, true) => "azdrive-find-note-cloud-contents",
            (false, false) => "azdrive-find-note-cloud",
        }
    }

    /// What the empty results say: still searching, or what was searched (keys of the
    /// resources; the second is empty while it searches).
    #[must_use]
    pub fn empty_text(&self) -> (&'static str, &'static str) {
        if self.running() {
            return ("azdrive-find-empty-searching", "");
        }
        let what = if self.remote {
            "azdrive-find-empty-cloud"
        } else if self.contents {
            "azdrive-find-empty-contents"
        } else {
            "azdrive-find-empty-names"
        };
        ("azdrive-find-none", what)
    }
}

// ==== A cloud drive's last listing ====

/// The most objects a kept listing holds (a bigger drive is listed afresh every time).
pub const CACHE_MAX_OBJECTS: usize = 200_000;

/// The first word of a kept listing's file: a file of another format is not read.
const LISTING_FORMAT: &str = "azdrive-listing 1";

/// A cloud drive's last complete recursive listing of a folder (`prefix`, `""`: the drive's
/// root), kept in the cache folder: the next search below it shows its names at once. It holds
/// the bucket's keys as the listing named them (an encrypted drive's names must not be kept in
/// the clear: such a drive keeps none).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedListing {
    pub prefix: String,
    /// When it was listed, in seconds since 1970-01-01 UTC.
    pub at: u64,
    pub objects: Vec<ObjectInfo>,
}

/// The file in `dir` that keeps the drive `drive_id`'s listing (named safely after the drive).
#[must_use]
pub fn listing_file(dir: &Path, drive_id: &str) -> PathBuf {
    dir.join(format!("{}.tsv", cache_name(drive_id)))
}

/// The name of a drive's file or folder in the cache: the id's letters and digits (the rest
/// become `_`) and a hash of the whole id (two ids that read alike do not share it).
fn cache_name(drive_id: &str) -> String {
    let safe: String = drive_id
        .chars()
        .take(48)
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    // FNV-1a: stable across runs and builds (std's hasher is not).
    let hash = drive_id.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    });
    format!("{safe}-{hash:016x}")
}

/// A key, tag or prefix on one line of the file: `\\`, tabs and line breaks escaped.
fn escape_field(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c => out.push(c),
        }
    }
    out
}

/// [`escape_field`] undone; `None` for an escape it never writes.
fn unescape_field(text: &str) -> Option<String> {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        out.push(match chars.next()? {
            '\\' => '\\',
            't' => '\t',
            'n' => '\n',
            'r' => '\r',
            _ => return None,
        });
    }
    Some(out)
}

/// Writes a kept listing (its folder made first), through a temporary file beside it: a
/// reader never sees half a file. An empty tag counts as none.
///
/// # Errors
/// The cache folder or the file could not be written.
pub fn write_listing(path: &Path, listing: &CachedListing) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let mut temp = path.as_os_str().to_owned();
    temp.push(".tmp");
    let temp = PathBuf::from(temp);
    let mut out = io::BufWriter::new(fs::File::create(&temp)?);
    writeln!(
        out,
        "{LISTING_FORMAT}\t{}\t{}",
        escape_field(&listing.prefix),
        listing.at
    )?;
    for object in &listing.objects {
        writeln!(
            out,
            "{}\t{}\t{}\t{}",
            object.size,
            object.modified.map_or_else(String::new, |m| m.to_string()),
            object.etag.as_deref().map_or_else(String::new, escape_field),
            escape_field(&object.key)
        )?;
    }
    out.into_inner().map_err(io::IntoInnerError::into_error)?.sync_all()?;
    fs::rename(&temp, path)
}

/// A kept listing read back; `None` when there is none, or it is of another format or damaged.
#[must_use]
pub fn read_listing(path: &Path) -> Option<CachedListing> {
    let text = fs::read_to_string(path).ok()?;
    let mut lines = text.lines();
    let mut head = lines.next()?.split('\t');
    if head.next()? != LISTING_FORMAT {
        return None;
    }
    let prefix = unescape_field(head.next()?)?;
    let at = head.next()?.parse().ok()?;
    let mut objects = Vec::new();
    for line in lines {
        let mut fields = line.splitn(4, '\t');
        let size = fields.next()?.parse().ok()?;
        let modified = match fields.next()? {
            "" => None,
            m => Some(m.parse().ok()?),
        };
        let etag = match fields.next()? {
            "" => None,
            tag => Some(unescape_field(tag)?),
        };
        let key = unescape_field(fields.next()?)?;
        objects.push(ObjectInfo {
            key,
            size,
            modified,
            etag,
        });
    }
    Some(CachedListing {
        prefix,
        at,
        objects,
    })
}

// ==== A drive's index ====

/// A drive's full-text index as a search asks it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexAsk {
    /// The index's folder ([`index_dir`]).
    pub dir: PathBuf,
    /// The drive's folder on this computer: the index's paths are below it.
    pub root: PathBuf,
    /// The searched folder's key (`""`: the drive's root; else `Docs/`): the search's root is
    /// `root` with it.
    pub under: String,
}

/// The folder in `dir` (the cache's index folder) that holds the drive `drive_id`'s index.
#[must_use]
pub fn index_dir(dir: &Path, drive_id: &str) -> PathBuf {
    dir.join(cache_name(drive_id))
}

/// The walk a drive's index reads, from the drive's root: the search box's defaults - no hidden
/// items, no files .gitignore / .ignore name -, never the storage crate's temporary files or
/// the drive's bookkeeping. A search that shows more reads the rest itself (it is not in the
/// index: [`azul_search_index::DriveIndex::unread`]).
#[must_use]
pub fn index_filters() -> Filters {
    Filters {
        include: Vec::new(),
        exclude: vec![
            TEMP_GLOB.to_string(),
            format!("/{}/", azul_storage::manifest::MANIFEST_DIR),
        ],
        hidden: false,
        ignore_files: true,
        max_depth: None,
        refine: Refine::default(),
    }
}

/// Whether a file the index named (`rel`, below the searched folder) is one the search's walk
/// comes to: not hidden unless hidden items show, not deeper than "Current folder" goes.
#[must_use]
pub fn index_admits(filters: &Filters, rel: &str) -> bool {
    (filters.hidden || !hidden_path(rel))
        && filters
            .max_depth
            .is_none_or(|depth| rel.split('/').count() <= depth)
}

/// The longest line a document's result shows (bytes); a longer one is cut around its match.
const DOCUMENT_LINE_BYTES: usize = 400;

/// The line a document's result shows: the first line of its text (as the index reads it) the
/// search's text is on, the match marked; a long line cut around the match. `None` when no line
/// holds it (the index's words were apart).
#[must_use]
pub fn document_line(text: &str, matcher: &ContentMatcher) -> Option<FoundLine> {
    for (i, line) in text.lines().enumerate() {
        let Some(&(start, end)) = matcher.find_all(line).first() else {
            continue;
        };
        let (from, to) = if line.len() <= DOCUMENT_LINE_BYTES {
            (0, line.len())
        } else {
            let mut from = start.saturating_sub(DOCUMENT_LINE_BYTES / 4);
            while !line.is_char_boundary(from) {
                from -= 1;
            }
            let mut to = (end + DOCUMENT_LINE_BYTES / 2).min(line.len());
            while !line.is_char_boundary(to) {
                to += 1;
            }
            (from, to)
        };
        return Some(FoundLine {
            line: u64::try_from(i + 1).unwrap_or(u64::MAX),
            text: line[from..to].to_string(),
            start: start - from,
            end: end - from,
        });
    }
    None
}

/// A document the index named, or one read since, below the folder `prefix` (`rel` below it):
/// its row with the size and date read now, and the line its text matched on.
#[must_use]
pub fn found_document(
    prefix: &str,
    rel: &str,
    size: u64,
    modified: Option<u64>,
    line: Option<FoundLine>,
) -> Found {
    let key = format!("{prefix}{rel}");
    let name = key::last_segment(&key).to_string();
    Found {
        entry: result_row(key, name, false, Some(size), modified),
        line,
    }
}

/// A drive's index as the window knows it.
#[derive(Debug, Clone, Default)]
pub struct IndexInfo {
    /// What it holds, as far as known (a former run's index while its first update here runs).
    pub status: Option<IndexStatus>,
    /// How far the update running got (`Some` from its start; `None`: none runs).
    pub progress: Option<UpdateProgress>,
    /// Why the last update could not end.
    pub error: Option<String>,
    /// Raised to stop the update running ("Index this drive" turned off).
    pub cancel: Arc<AtomicBool>,
}

impl IndexInfo {
    /// Whether a search asks it: an update went over the drive (here or in a former run).
    #[must_use]
    pub fn usable(&self) -> bool {
        self.status.is_some_and(|status| status.updated.is_some())
    }

    /// The status line's words: how far the update got, what the index holds, or why it could
    /// not be brought up to date.
    #[must_use]
    pub fn status_text(&self) -> Phrase {
        if let Some(progress) = self.progress {
            return if progress.to_read == 0 {
                Phrase::new("azdrive-index-status-looking")
            } else {
                Phrase::new("azdrive-index-status-reading")
                    .arg("read", grouped(progress.read as u64))
                    .arg("total", grouped(progress.to_read as u64))
                    .arg("count", progress.to_read)
            };
        }
        if let Some(error) = &self.error {
            return Phrase::new("azdrive-index-status-failed").arg("error", error.as_str());
        }
        match self.status {
            Some(status) if status.updated.is_some() => Phrase::new("azdrive-index-status-indexed")
                .arg("n", grouped(status.files as u64))
                .arg("count", status.files),
            _ => Phrase::new("azdrive-index-status-never"),
        }
    }
}

/// A cloud or encrypted drive's index as its search asks it for the contents.
#[derive(Clone)]
pub struct RemoteContents {
    /// The index's folder.
    pub dir: PathBuf,
    /// The drive's id (the sync's name for it).
    pub drive_id: String,
    /// Where a result's local copy is (its line is read there).
    pub sync: Arc<dyn crate::sync_lookup::SyncLookup>,
}

impl std::fmt::Debug for RemoteContents {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RemoteContents")
            .field("dir", &self.dir)
            .field("drive_id", &self.drive_id)
            .finish_non_exhaustive()
    }
}

// ==== Saved searches (the Search tab's Save search) ====

/// The longest name a saved search takes from its text (characters, the ellipsis included).
const SAVED_NAME_CHARS: usize = 40;

/// A search kept in AzDrive's settings (Save search): its name, the search box's text, the
/// Search tab's choices and the place it searched; Saved searches runs it again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SavedSearch {
    pub name: String,
    /// The search box's text.
    pub query: String,
    /// File contents.
    pub contents: bool,
    /// All subfolders (`false`: Current folder).
    pub subfolders: bool,
    /// Skip ignored files.
    pub ignore_files: bool,
    pub date: DateRefine,
    pub kind: KindRefine,
    pub size: SizeRefine,
    /// The drive searched (`""`: This PC, every drive on this computer).
    pub drive: String,
    /// Its folder (`""`: its root).
    pub prefix: String,
}

impl Default for SavedSearch {
    fn default() -> Self {
        SavedSearch {
            name: String::new(),
            query: String::new(),
            contents: false,
            subfolders: true,
            ignore_files: true,
            date: DateRefine::Any,
            kind: KindRefine::Any,
            size: SizeRefine::Any,
            drive: String::new(),
            prefix: String::new(),
        }
    }
}

impl SavedSearch {
    /// The search of `text` in `place` as the Search tab sets it (`settings`, `refines`), named
    /// after the text (cut to [`SAVED_NAME_CHARS`] with an ellipsis when longer).
    #[must_use]
    pub fn of(text: &str, settings: &Settings, refines: Refines, place: &Place) -> SavedSearch {
        let query = text.trim().to_string();
        let name = if query.chars().count() > SAVED_NAME_CHARS {
            let kept: String = query.chars().take(SAVED_NAME_CHARS - 1).collect();
            format!("{kept}\u{2026}")
        } else {
            query.clone()
        };
        let (drive, prefix) = match place {
            Place::Folder { drive, prefix } => (drive.clone(), prefix.clone()),
            Place::ThisPc | Place::QuickAccess => (String::new(), String::new()),
        };
        SavedSearch {
            name,
            query,
            contents: settings.search_contents,
            subfolders: settings.search_subfolders,
            ignore_files: settings.search_ignore_files,
            date: refines.date,
            kind: refines.kind,
            size: refines.size,
            drive,
            prefix,
        }
    }

    /// Its Refine.
    #[must_use]
    pub fn refines(&self) -> Refines {
        Refines {
            date: self.date,
            kind: self.kind,
            size: self.size,
        }
    }

    /// The place it searches.
    #[must_use]
    pub fn place(&self) -> Place {
        if self.drive.is_empty() {
            Place::ThisPc
        } else {
            Place::folder(&self.drive, &self.prefix)
        }
    }
}

/// Where the saved search named `name` (without case) is in `list`.
#[must_use]
pub fn saved_position(list: &[SavedSearch], name: &str) -> Option<usize> {
    let name = name.to_lowercase();
    list.iter().position(|saved| saved.name.to_lowercase() == name)
}

/// Keeps `saved` in `list`: in place of the saved search of its name (without case), else at
/// the end.
pub fn save_search(list: &mut Vec<SavedSearch>, saved: SavedSearch) {
    match saved_position(list, &saved.name) {
        Some(at) => list[at] = saved,
        None => list.push(saved),
    }
}
