//! The walks of a search: `ignore`'s parallel walker over the searched folder (its threads run
//! the visitors below), the results sent through a bounded channel to the calling thread, which
//! hands them to the callback ([`Gate`]: the limits, nothing after a stop).

use std::{
    fs,
    path::{Component, Path},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc::{self, RecvTimeoutError, SyncSender},
    },
    time::{Duration, Instant},
};

use ignore::{
    overrides::{Override, OverrideBuilder},
    DirEntry, WalkBuilder, WalkState,
};

use crate::{
    content::{self, FileResult},
    ContentHit, ContentMatcher, Event, Filters, Limits, NameHit, NameMatcher, PatternError,
    Progress, Summary, PROGRESS_MS,
};

/// Results on their way to the calling thread at most (the walkers wait while it is full):
/// what a search holds in memory is bounded however much it finds.
const CHANNEL_DEPTH: usize = 64;

/// Whether the search must stop: the caller's cancel flag, or a limit reached.
#[derive(Clone, Copy)]
pub(crate) struct Stop<'a> {
    pub(crate) cancel: &'a AtomicBool,
    pub(crate) limit: &'a AtomicBool,
}

impl Stop<'_> {
    pub(crate) fn is_set(&self) -> bool {
        self.cancel.load(Ordering::Relaxed) || self.limit.load(Ordering::Relaxed)
    }
}

/// What the walkers' threads count.
#[derive(Default)]
pub(crate) struct Counters {
    pub(crate) walked: AtomicUsize,
    pub(crate) searched: AtomicUsize,
    pub(crate) binary: AtomicUsize,
    pub(crate) too_large: AtomicUsize,
    pub(crate) errors: AtomicUsize,
}

fn count(counter: &AtomicUsize) {
    counter.fetch_add(1, Ordering::Relaxed);
}

/// What every walk of one search shares.
pub(crate) struct Shared<'a> {
    pub(crate) root: &'a Path,
    pub(crate) filters: &'a Filters,
    /// The include and exclude globs, compiled once.
    pub(crate) overrides: Override,
    pub(crate) limits: Limits,
    pub(crate) context: usize,
    pub(crate) stop: Stop<'a>,
    pub(crate) counters: Counters,
}

/// What a walk looks for.
#[derive(Clone, Copy)]
pub(crate) enum Look<'a> {
    Names(&'a NameMatcher),
    Contents(&'a ContentMatcher),
}

/// A result on its way from a walker's thread.
enum Found {
    Name(NameHit),
    Content(ContentHit),
}

/// The include (`glob`) and exclude (`!glob`) globs as `ignore`'s overrides, relative to `root`.
pub(crate) fn overrides(root: &Path, filters: &Filters) -> Result<Override, PatternError> {
    let mut builder = OverrideBuilder::new(root);
    let globs = filters
        .include
        .iter()
        .map(|glob| (glob.clone(), glob.clone()))
        .chain(filters.exclude.iter().map(|glob| (format!("!{glob}"), glob.clone())));
    for (line, glob) in globs {
        builder
            .add(&line)
            .map_err(|e| PatternError::new(format!("\"{glob}\" is not a glob: {e}")))?;
    }
    builder
        .build()
        .map_err(|e| PatternError::new(format!("the filter globs do not compile: {e}")))
}

/// `ignore`'s walker for the search's folder and filters.
fn walker(shared: &Shared<'_>) -> WalkBuilder {
    let ignore_files = shared.filters.ignore_files;
    let mut builder = WalkBuilder::new(shared.root);
    builder
        .hidden(!shared.filters.hidden)
        .ignore(ignore_files)
        .git_ignore(ignore_files)
        .git_exclude(ignore_files)
        .parents(ignore_files)
        // A .gitignore counts without a repository around it, the user's global excludes
        // never (a search would depend on the machine).
        .require_git(false)
        .git_global(false)
        .follow_links(false)
        .overrides(shared.overrides.clone());
    builder
}

/// The `/`-separated path of `path` below `root` (`None`: the root itself, or a name that is
/// not UTF-8 - no key could name it).
fn relative(root: &Path, path: &Path) -> Option<String> {
    let rest = path.strip_prefix(root).ok()?;
    let mut out = String::new();
    for part in rest.components() {
        let Component::Normal(name) = part else {
            return None;
        };
        if !out.is_empty() {
            out.push('/');
        }
        out.push_str(name.to_str()?);
    }
    (!out.is_empty()).then_some(out)
}

/// Whether an entry is a folder (`Some(true)`), a file (`Some(false)`) or neither (a socket, a
/// broken link). A symbolic link counts as what it points at; it is not walked into.
fn is_folder(entry: &DirEntry) -> Option<bool> {
    let kind = entry.file_type()?;
    if kind.is_symlink() {
        let meta = fs::metadata(entry.path()).ok()?;
        return if meta.is_dir() {
            Some(true)
        } else {
            meta.is_file().then_some(false)
        };
    }
    if kind.is_dir() {
        Some(true)
    } else {
        kind.is_file().then_some(false)
    }
}

/// A walker's visitor: one per thread.
type Visitor<'s> = Box<dyn FnMut(Result<DirEntry, ignore::Error>) -> WalkState + Send + 's>;

/// The visitor of a walk for names: every entry below the root whose name matches.
fn name_visitor<'s>(
    shared: &'s Shared<'s>,
    matcher: &'s NameMatcher,
    count_walked: bool,
    tx: SyncSender<Found>,
) -> Visitor<'s> {
    Box::new(move |result: Result<DirEntry, ignore::Error>| -> WalkState {
        if shared.stop.is_set() {
            return WalkState::Quit;
        }
        let Ok(entry) = result else {
            count(&shared.counters.errors);
            return WalkState::Continue;
        };
        if count_walked {
            count(&shared.counters.walked);
        }
        if entry.depth() == 0 {
            return WalkState::Continue;
        }
        let Some(folder) = is_folder(&entry) else {
            return WalkState::Continue;
        };
        // Include globs name files: a folder's name is no result while they narrow the search.
        if folder && !shared.filters.include.is_empty() {
            return WalkState::Continue;
        }
        let (Some(name), Some(path)) = (
            entry.file_name().to_str(),
            relative(shared.root, entry.path()),
        ) else {
            return WalkState::Continue;
        };
        let Some(range) = matcher.find(name, &path) else {
            return WalkState::Continue;
        };
        let hit = NameHit {
            path: if folder { format!("{path}/") } else { path },
            name: name.to_string(),
            is_dir: folder,
            range,
        };
        if tx.send(Found::Name(hit)).is_err() {
            return WalkState::Quit;
        }
        WalkState::Continue
    })
}

/// The visitor of a walk for contents: every file below the root, read with this thread's
/// searcher.
fn content_visitor<'s>(
    shared: &'s Shared<'s>,
    matcher: &'s ContentMatcher,
    count_walked: bool,
    tx: SyncSender<Found>,
) -> Visitor<'s> {
    let mut searcher = content::searcher(shared.context);
    Box::new(move |result: Result<DirEntry, ignore::Error>| -> WalkState {
        if shared.stop.is_set() {
            return WalkState::Quit;
        }
        let Ok(entry) = result else {
            count(&shared.counters.errors);
            return WalkState::Continue;
        };
        if count_walked {
            count(&shared.counters.walked);
        }
        // Files only: a folder is walked into, a symbolic link is not followed.
        if !entry.file_type().is_some_and(|kind| kind.is_file()) {
            return WalkState::Continue;
        }
        let Some(path) = relative(shared.root, entry.path()) else {
            return WalkState::Continue;
        };
        let limits = &shared.limits;
        let counters = &shared.counters;
        match content::search_file(
            entry.path(),
            matcher,
            &mut searcher,
            limits.max_file_size,
            limits.max_lines_per_file,
            shared.stop,
        ) {
            FileResult::Lines(lines, more) => {
                count(&counters.searched);
                let hit = ContentHit { path, lines, more };
                if tx.send(Found::Content(hit)).is_err() {
                    return WalkState::Quit;
                }
            }
            FileResult::Nothing => count(&counters.searched),
            FileResult::Binary => {
                count(&counters.searched);
                count(&counters.binary);
            }
            FileResult::TooLarge => count(&counters.too_large),
            FileResult::Failed => count(&counters.errors),
            FileResult::Stopped => return WalkState::Quit,
        }
        WalkState::Continue
    })
}

/// What reaches the callback: the results in the order they came, until a limit is reached
/// (then nothing more, and the walkers stop) or the search is stopped.
pub(crate) struct Gate<'a> {
    limits: Limits,
    stop: Stop<'a>,
    pub(crate) summary: Summary,
}

impl<'a> Gate<'a> {
    pub(crate) fn new(limits: Limits, stop: Stop<'a>) -> Gate<'a> {
        Gate {
            limits,
            stop,
            summary: Summary::default(),
        }
    }

    /// A limit is reached and there is more: the walkers stop.
    fn full(&mut self) {
        self.summary.limited = true;
        self.stop.limit.store(true, Ordering::SeqCst);
    }

    fn pass(&mut self, found: Found, on_event: &mut dyn FnMut(Event)) {
        if self.stop.is_set() {
            return;
        }
        if self.summary.names + self.summary.files >= self.limits.max_results {
            self.full();
            return;
        }
        match found {
            Found::Name(hit) => {
                self.summary.names += 1;
                on_event(Event::Name(hit));
            }
            Found::Content(mut hit) => {
                let room = self.limits.max_matches.saturating_sub(self.summary.matches);
                if room == 0 {
                    self.full();
                    return;
                }
                let cut = hit.lines.len() > room;
                if cut {
                    hit.lines.truncate(room);
                    hit.more = true;
                }
                self.summary.files += 1;
                self.summary.matches += hit.lines.len();
                on_event(Event::Content(hit));
                if cut {
                    self.full();
                }
            }
        }
    }
}

/// One walk of the searched folder for `look`: the walkers run on their own threads, this
/// thread hands their results through `gate` to `on_event` - and says how far they got every
/// [`PROGRESS_MS`] - until the walk ends (a stop ends it within milliseconds).
pub(crate) fn run(
    shared: &Shared<'_>,
    look: Look<'_>,
    count_walked: bool,
    gate: &mut Gate<'_>,
    on_event: &mut dyn FnMut(Event),
) {
    let (tx, rx) = mpsc::sync_channel::<Found>(CHANNEL_DEPTH);
    let every = Duration::from_millis(PROGRESS_MS);
    std::thread::scope(|scope| {
        scope.spawn(move || {
            walker(shared).build_parallel().run(|| match look {
                Look::Names(matcher) => name_visitor(shared, matcher, count_walked, tx.clone()),
                Look::Contents(matcher) => {
                    content_visitor(shared, matcher, count_walked, tx.clone())
                }
            });
        });
        let mut said = Instant::now();
        loop {
            match rx.recv_timeout(every) {
                Ok(found) => gate.pass(found, &mut *on_event),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
            if said.elapsed() >= every && !shared.stop.is_set() {
                said = Instant::now();
                on_event(Event::Progress(Progress {
                    walked: shared.counters.walked.load(Ordering::Relaxed),
                    searched: shared.counters.searched.load(Ordering::Relaxed),
                }));
            }
        }
    });
}
