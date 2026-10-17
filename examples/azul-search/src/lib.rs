//! azul-search: the one fast search the Azlin apps share.
//!
//! AzDrive's search box and AzCode's find in files each had their own walk: a folder's names
//! filtered on screen, a breadth-first listing read file by file through the drive. Both run on
//! this crate now, built on ripgrep's own libraries:
//!
//! - the walk is [`ignore`]'s parallel walker: every folder below the searched one, hidden files
//!   and folders left out unless asked for, `.gitignore` / `.ignore` files honoured unless turned
//!   off, include and exclude globs (ripgrep's `-g`, gitignore syntax) - [`Filters`];
//! - a name matches a [`Pattern`]: a part of the name (`report` finds `Q3 Report.pdf`), a shell
//!   glob (`*.pdf`; with a `/`, over the path: `src/**/*.rs`) or a regular expression, without
//!   case by default, with ripgrep's smart case on request - [`NameMatcher`];
//! - a file's contents are searched line by line by `grep-searcher` with `grep-regex`'s matcher:
//!   a literal or a regular expression, the line, its column and its matches, a few context
//!   lines; a binary file (a NUL byte) and a file over the size limit are passed over, a UTF-16
//!   file with a byte-order mark is read as text - [`ContentHit`].
//!
//! [`search`] walks for names first, then (a second walk) for contents, and hands every result
//! to its callback as it is found ([`Event`]): the walkers' threads send them through a bounded
//! channel (memory stays bounded however much is found), and the callback runs on the calling
//! thread. A result limit ends the search ([`Limits`]); the cancel flag stops it within
//! milliseconds - every directory entry checks it, and a file being read checks it every 64 KB.
//! Blocking and plain Rust (no azul types): the apps call it from an azul `Thread`.
//!
//! ```no_run
//! use std::sync::atomic::AtomicBool;
//! use azul_search::{search, Event, Pattern, Request};
//!
//! let request = Request::new("/home/me/Documents")
//!     .with_names(Pattern::guess("report"))
//!     .with_contents(Pattern::literal("quarterly"));
//! let cancel = AtomicBool::new(false);
//! let summary = search(&request, &cancel, &mut |event| match event {
//!     Event::Name(hit) => println!("{}", hit.path),
//!     Event::Content(hit) => println!("{}:{}", hit.path, hit.lines[0].line),
//!     _ => {}
//! });
//! ```

mod content;
pub mod pattern;
mod walk;

#[cfg(test)]
mod tests;

use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

pub use pattern::{Case, ContentMatcher, NameMatcher, Pattern, PatternError, PatternKind};

/// What a search walks and what it leaves out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Filters {
    /// Only the files whose path below the searched folder matches one of these globs
    /// (gitignore syntax: `*.rs`, `src/**`, `/top-level.txt`); empty: every file. Folders are
    /// still walked (a folder name is not reported while this is set). As ripgrep's `-g`, the
    /// globs outrank the other rules: a file they name is searched even when it is hidden or
    /// ignored (in a folder that is walked).
    pub include: Vec<String>,
    /// Never the files or folders that match one of these (gitignore syntax: `target/`,
    /// `*.min.js`, `/.azlin/`); they outrank every other rule too.
    pub exclude: Vec<String>,
    /// Hidden files and folders (a name starting with a dot) too.
    pub hidden: bool,
    /// Honour `.gitignore`, `.ignore` and `.git/info/exclude` files (in the searched folder,
    /// below it and above it; not the user's global excludes file, which would make a search
    /// depend on the machine).
    pub ignore_files: bool,
    /// How deep the walk goes: `Some(1)` the searched folder's own items (Explorer's "Current
    /// folder"), `None` every folder below it.
    pub max_depth: Option<usize>,
    /// Only the kinds, sizes and dates this lets through ([`Refine`]).
    pub refine: Refine,
}

impl Default for Filters {
    fn default() -> Self {
        Filters {
            include: Vec::new(),
            exclude: Vec::new(),
            hidden: false,
            ignore_files: true,
            max_depth: None,
            refine: Refine::default(),
        }
    }
}

/// Explorer's Refine: a kind (the files of some extensions), a size range (files), a date range
/// (files and folders). What a range leaves open is open; an empty refine lets everything
/// through, and only a refine with a size or a date costs a stat per entry.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Refine {
    /// Modified at or after this (seconds since 1970).
    pub modified_from: Option<u64>,
    /// Modified before this (seconds since 1970).
    pub modified_until: Option<u64>,
    /// At least this many bytes.
    pub min_size: Option<u64>,
    /// At most this many bytes.
    pub max_size: Option<u64>,
    /// Only files with one of these extensions (without the dot; any case).
    pub extensions: Vec<String>,
}

impl Refine {
    /// Whether it lets everything through.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Refine::default()
    }

    /// Whether only files pass: a size or a kind says nothing of a folder.
    fn files_only(&self) -> bool {
        self.min_size.is_some() || self.max_size.is_some() || !self.extensions.is_empty()
    }

    /// Whether an item of this name passes the kind (a folder: unless only files pass).
    #[must_use]
    pub fn admits_name(&self, name: &str, is_dir: bool) -> bool {
        if is_dir {
            return !self.files_only();
        }
        if self.extensions.is_empty() {
            return true;
        }
        let Some((_, extension)) = name.rsplit_once('.') else {
            return false;
        };
        self.extensions
            .iter()
            .any(|e| e.eq_ignore_ascii_case(extension))
    }

    /// Whether a size (a file's; `None` for a folder) and a date pass.
    #[must_use]
    pub fn admits_facts(&self, size: Option<u64>, modified: Option<u64>) -> bool {
        let size_ok = match (self.min_size, self.max_size, size) {
            (None, None, _) => true,
            (_, _, None) => false,
            (low, high, Some(size)) => {
                low.is_none_or(|low| size >= low) && high.is_none_or(|high| size <= high)
            }
        };
        let date_ok = match (self.modified_from, self.modified_until, modified) {
            (None, None, _) => true,
            (_, _, None) => false,
            (from, until, Some(at)) => {
                from.is_none_or(|from| at >= from) && until.is_none_or(|until| at < until)
            }
        };
        size_ok && date_ok
    }
}

/// When a search stops before it has seen everything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// The most results handed over: names, and files with matching lines.
    pub max_results: usize,
    /// The most matching lines handed over in all.
    pub max_matches: usize,
    /// The most matching lines kept of one file (its [`ContentHit::more`] says there were more).
    pub max_lines_per_file: usize,
    /// A file larger than this (bytes) is not read; its name is still searched.
    pub max_file_size: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            max_results: 10_000,
            max_matches: 20_000,
            max_lines_per_file: 1_000,
            max_file_size: 64 * 1024 * 1024,
        }
    }
}

/// One search: where, for which names, for which contents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    /// The folder searched, with every folder below it.
    pub root: PathBuf,
    /// The names to report (`None`: no walk for names).
    pub names: Option<Pattern>,
    /// What the files' lines must hold (`None`: no file is read). Reported after every name.
    pub contents: Option<Pattern>,
    pub filters: Filters,
    pub limits: Limits,
    /// The lines kept before and after each matching line.
    pub context: usize,
    /// A file with a UTF-16 byte-order mark is read as text (transcoded to UTF-8); `false`: it
    /// is passed over as binary (its NUL bytes) - for an app that opens files as UTF-8 only,
    /// whose editor could not show the match.
    pub utf16: bool,
}

impl Request {
    /// A search of `root` for nothing yet, with the default filters and limits.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Request {
        Request {
            root: root.into(),
            names: None,
            contents: None,
            filters: Filters::default(),
            limits: Limits::default(),
            context: 0,
            utf16: true,
        }
    }

    #[must_use]
    pub fn with_names(mut self, pattern: Pattern) -> Request {
        self.names = Some(pattern);
        self
    }

    #[must_use]
    pub fn with_contents(mut self, pattern: Pattern) -> Request {
        self.contents = Some(pattern);
        self
    }

    #[must_use]
    pub fn with_filters(mut self, filters: Filters) -> Request {
        self.filters = filters;
        self
    }

    #[must_use]
    pub fn with_limits(mut self, limits: Limits) -> Request {
        self.limits = limits;
        self
    }

    #[must_use]
    pub fn with_context(mut self, lines: usize) -> Request {
        self.context = lines;
        self
    }

    #[must_use]
    pub fn with_utf16(mut self, yes: bool) -> Request {
        self.utf16 = yes;
        self
    }
}

/// The walk a search is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// The walk for names.
    Names,
    /// The walk for contents.
    Contents,
}

/// A file or folder whose name matches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameHit {
    /// Its path below the searched folder, `/`-separated (`docs/Q3 Report.pdf`); a folder's
    /// ends in `/` (`docs/reports/`).
    pub path: String,
    /// Its own name (`Q3 Report.pdf`, `reports`).
    pub name: String,
    pub is_dir: bool,
    /// The bytes of `name` the pattern matched (a glob: all of it).
    pub range: (usize, usize),
    /// A file's size (bytes; `None` for a folder, or when it could not be read).
    pub size: Option<u64>,
    /// When it was last modified (seconds since 1970).
    pub modified: Option<u64>,
}

/// A matching line of a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineMatch {
    /// The line's number, from 1.
    pub line: u64,
    /// Where the first match starts in the line: characters from 1.
    pub column: usize,
    /// The line without its line break, as UTF-8 (an invalid byte becomes U+FFFD); a line
    /// longer than [`MAX_LINE_BYTES`] is cut to a window around its first match.
    pub text: String,
    /// Where `text` starts in the line (bytes; 0 unless the line was cut).
    pub text_offset: usize,
    /// The matches in the line, as bytes from the line's start - each one inside `text`
    /// (`text[start - text_offset..end - text_offset]`).
    pub ranges: Vec<(usize, usize)>,
    /// The context lines before it (the ones not already after the match before it).
    pub before: Vec<String>,
    /// The context lines after it.
    pub after: Vec<String>,
}

/// The longest line text a [`LineMatch`] keeps (bytes); a longer line keeps a window of about
/// this many bytes around its first match.
pub const MAX_LINE_BYTES: usize = 2048;

/// A file whose contents match.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentHit {
    /// Its path below the searched folder, `/`-separated.
    pub path: String,
    /// Its matching lines, top to bottom.
    pub lines: Vec<LineMatch>,
    /// It has more matching lines than `lines` holds (a limit).
    pub more: bool,
    /// Its size (bytes).
    pub size: Option<u64>,
    /// When it was last modified (seconds since 1970).
    pub modified: Option<u64>,
}

/// A file [`list_files`] found: its path below the folder, its size and date.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    /// `/`-separated, below the listed folder.
    pub path: String,
    pub size: u64,
    /// Seconds since 1970.
    pub modified: Option<u64>,
}

/// How far a search got.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Progress {
    /// Files and folders walked so far.
    pub walked: usize,
    /// Files whose contents were read so far.
    pub searched: usize,
}

/// What a search hands its callback, in order: a phase, the results of its walk, the next
/// phase, its results; progress every [`PROGRESS_MS`] milliseconds in between.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// A walk begins: [`Phase::Names`] first, then [`Phase::Contents`].
    Phase(Phase),
    Name(NameHit),
    Content(ContentHit),
    Progress(Progress),
}

/// How often a search says how far it got (milliseconds).
pub const PROGRESS_MS: u64 = 100;

/// What a search did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Summary {
    /// Names handed over.
    pub names: usize,
    /// Files with matching lines handed over.
    pub files: usize,
    /// Matching lines handed over.
    pub matches: usize,
    /// Files and folders walked (by the first walk).
    pub walked: usize,
    /// Files whose contents were read (a binary one too).
    pub searched: usize,
    /// Files passed over as binary (a NUL byte).
    pub binary: usize,
    /// Files passed over as larger than [`Limits::max_file_size`].
    pub too_large: usize,
    /// Files or folders that could not be read.
    pub errors: usize,
    /// The cancel flag stopped the search.
    pub cancelled: bool,
    /// A limit stopped it: there was more than it handed over.
    pub limited: bool,
}

/// Searches `request.root` and every folder below it: the names matching `request.names`
/// first, then the files whose lines match `request.contents`, each result handed to
/// `on_event` (on this thread) as it is found. Blocks until the walks end, a limit is reached
/// or `cancel` is set - then nothing more is handed over, and it returns within milliseconds.
///
/// # Errors
///
/// A pattern or a filter glob that does not compile: nothing is walked.
pub fn search(
    request: &Request,
    cancel: &AtomicBool,
    on_event: &mut dyn FnMut(Event),
) -> Result<Summary, PatternError> {
    let names = request.names.as_ref().map(NameMatcher::new).transpose()?;
    let contents = request.contents.as_ref().map(ContentMatcher::new).transpose()?;
    let overrides = walk::overrides(&request.root, &request.filters)?;
    let limit = AtomicBool::new(false);
    let shared = walk::Shared {
        root: &request.root,
        filters: &request.filters,
        overrides,
        limits: request.limits,
        context: request.context,
        utf16: request.utf16,
        stop: walk::Stop {
            cancel,
            limit: &limit,
        },
        counters: walk::Counters::default(),
    };
    let mut gate = walk::Gate::new(request.limits, shared.stop);
    let walks = [
        names.as_ref().map(|m| (Phase::Names, walk::Look::Names(m), true)),
        contents
            .as_ref()
            .map(|m| (Phase::Contents, walk::Look::Contents(m), names.is_none())),
    ];
    for (phase, look, count_walked) in walks.into_iter().flatten() {
        if shared.stop.is_set() {
            break;
        }
        on_event(Event::Phase(phase));
        walk::run(&shared, look, count_walked, &mut |delivery| match delivery {
            walk::Delivery::Found(found) => gate.pass(found, &mut *on_event),
            walk::Delivery::Progress(progress) => on_event(Event::Progress(progress)),
        });
    }
    Ok(summarize(gate.summary, &shared, cancel))
}

/// `summary` with what the walkers counted, and whether the search was cancelled.
fn summarize(mut summary: Summary, shared: &walk::Shared<'_>, cancel: &AtomicBool) -> Summary {
    let counters = &shared.counters;
    summary.walked = counters.walked.load(Ordering::Relaxed);
    summary.searched = counters.searched.load(Ordering::Relaxed);
    summary.binary = counters.binary.load(Ordering::Relaxed);
    summary.too_large = counters.too_large.load(Ordering::Relaxed);
    summary.errors = counters.errors.load(Ordering::Relaxed);
    summary.cancelled = cancel.load(Ordering::SeqCst);
    summary
}

/// Every file below `root` the filters let through (the refine too), with its size and date, as
/// the parallel walk finds it (in no order): what a full-text index compares with what it holds.
/// `cancel` stops it within milliseconds.
///
/// # Errors
///
/// A filter glob that does not compile.
pub fn list_files(
    root: &Path,
    filters: &Filters,
    cancel: &AtomicBool,
    on_file: &mut dyn FnMut(FileEntry),
) -> Result<Summary, PatternError> {
    let overrides = walk::overrides(root, filters)?;
    let limit = AtomicBool::new(false);
    let shared = walk::Shared {
        root,
        filters,
        overrides,
        limits: Limits::default(),
        context: 0,
        utf16: true,
        stop: walk::Stop {
            cancel,
            limit: &limit,
        },
        counters: walk::Counters::default(),
    };
    if !shared.stop.is_set() {
        walk::run(&shared, walk::Look::Files, true, &mut |delivery| {
            if let walk::Delivery::Found(walk::Found::File(file)) = delivery {
                if !shared.stop.is_set() {
                    on_file(file);
                }
            }
        });
    }
    Ok(summarize(Summary::default(), &shared, cancel))
}

/// Searches the files `paths` (`/`-separated, below `request.root`) for `request.contents` with
/// the request's limits, context and refine - the files a full-text index named; no walk, on
/// this thread, in the order given. A path that cannot be read counts as an error. Without
/// contents to look for, nothing is read.
///
/// # Errors
///
/// A pattern or a filter glob that does not compile.
pub fn search_listed(
    request: &Request,
    paths: &[String],
    cancel: &AtomicBool,
    on_event: &mut dyn FnMut(Event),
) -> Result<Summary, PatternError> {
    let Some(pattern) = &request.contents else {
        return Ok(Summary::default());
    };
    let matcher = ContentMatcher::new(pattern)?;
    let overrides = walk::overrides(&request.root, &request.filters)?;
    let limit = AtomicBool::new(false);
    let shared = walk::Shared {
        root: &request.root,
        filters: &request.filters,
        overrides,
        limits: request.limits,
        context: request.context,
        utf16: request.utf16,
        stop: walk::Stop {
            cancel,
            limit: &limit,
        },
        counters: walk::Counters::default(),
    };
    let mut gate = walk::Gate::new(request.limits, shared.stop);
    let mut searcher = content::searcher(request.context);
    if !shared.stop.is_set() {
        on_event(Event::Phase(Phase::Contents));
    }
    for path in paths {
        if shared.stop.is_set() {
            break;
        }
        let Some(full) = walk::full_path(&request.root, path) else {
            shared.counters.errors.fetch_add(1, Ordering::Relaxed);
            continue;
        };
        match walk::search_one(&shared, &matcher, &mut searcher, &full, path.clone()) {
            walk::OneFile::Hit(hit) => gate.pass(walk::Found::Content(hit), on_event),
            walk::OneFile::Passed => {}
            walk::OneFile::Stop => break,
        }
    }
    Ok(summarize(gate.summary, &shared, cancel))
}
