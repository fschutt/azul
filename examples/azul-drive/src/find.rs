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

use azul_search::{
    ContentHit, Filters, Limits, NameHit, NameMatcher, Pattern, PatternKind, Refine, Request,
};
use azul_storage::{key, ListPage};
use chrono::{DateTime, Datelike, Days, NaiveDate, TimeZone};

use crate::{
    browse::{self, Entry, Sort},
    listing,
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
fn hidden_path(path: &str) -> bool {
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

// ==== Refine (the Search tab's Date modified, Kind and Size) ====

/// Explorer's Date modified.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            DateRefine::Any => "Any date",
            DateRefine::Today => "Today",
            DateRefine::Yesterday => "Yesterday",
            DateRefine::ThisWeek => "This week",
            DateRefine::LastWeek => "Last week",
            DateRefine::ThisMonth => "This month",
            DateRefine::LastMonth => "Last month",
            DateRefine::ThisYear => "This year",
            DateRefine::LastYear => "Last year",
        }
    }
}

/// Explorer's Kind: the files of a kind's extensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            KindRefine::Any => "Any kind",
            KindRefine::Document => "Document",
            KindRefine::Picture => "Picture",
            KindRefine::Music => "Music",
            KindRefine::Video => "Video",
            KindRefine::Archive => "Archive",
            KindRefine::Code => "Code",
            KindRefine::Mail => "E-mail",
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            SizeRefine::Any => "Any size",
            SizeRefine::Empty => "Empty (0 KB)",
            SizeRefine::Tiny => "Tiny (0 - 16 KB)",
            SizeRefine::Small => "Small (16 KB - 1 MB)",
            SizeRefine::Medium => "Medium (1 - 128 MB)",
            SizeRefine::Large => "Large (128 MB - 1 GB)",
            SizeRefine::Huge => "Huge (1 - 4 GB)",
            SizeRefine::Gigantic => "Gigantic (> 4 GB)",
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
    pub fn label(&self) -> String {
        let mut parts = Vec::new();
        if self.date != DateRefine::Any {
            parts.push(format!("Date modified: {}", self.date.label()));
        }
        if self.kind != KindRefine::Any {
            parts.push(format!("Kind: {}", self.kind.label()));
        }
        if self.size != SizeRefine::Any {
            parts.push(format!("Size: {}", self.size.label()));
        }
        parts.join(", ")
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
