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

pub mod pattern;

#[cfg(test)]
mod tests;

use std::{
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
};

pub use pattern::{Case, ContentMatcher, NameMatcher, Pattern, PatternError, PatternKind};

/// What a search walks and what it leaves out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Filters {
    /// Only the files whose path below the searched folder matches one of these globs
    /// (gitignore syntax: `*.rs`, `src/**`, `/top-level.txt`); empty: every file. Folders are
    /// still walked (a folder name is not reported while this is set).
    pub include: Vec<String>,
    /// Never the files or folders that match one of these (gitignore syntax: `target/`,
    /// `*.min.js`, `/.azlin/`).
    pub exclude: Vec<String>,
    /// Hidden files and folders (a name starting with a dot) too.
    pub hidden: bool,
    /// Honour `.gitignore`, `.ignore` and `.git/info/exclude` files (in the searched folder,
    /// below it and above it; not the user's global excludes file, which would make a search
    /// depend on the machine).
    pub ignore_files: bool,
}

impl Default for Filters {
    fn default() -> Self {
        Filters {
            include: Vec::new(),
            exclude: Vec::new(),
            hidden: false,
            ignore_files: true,
        }
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
    let _ = (request, cancel, on_event, Ordering::SeqCst);
    todo!("SEARCH17: the engine (the walks, the content search) is the next commit")
}
