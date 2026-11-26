//! The background scan: builds the symbol index of a workspace OUTWARDS from the file being looked
//! at, on a few low-priority threads, and stays out of the way of the window.
//!
//! # Order
//!
//! Every file and folder waiting has a [`Rank`] against the focus (the file the reviewer is looking
//! at): [`Class::Focus`] (the file), [`Class::SameDir`], [`Class::Module`] (the module tree around
//! it: its parent module and the crate root, and the files its `use`, `mod`, `import` and `#include`
//! lines name, resolved to paths), [`Class::SameCrate`] (the nearest folder with a manifest),
//! [`Class::ImportedCrate`] (the other packages of the workspace it imports from), [`Class::Rest`];
//! within a class the nearer folder first. The queue serves three tiers: (A) the focus's
//! neighbourhood and the manifests, (B) the folders still to be listed (the walk itself, which finds
//! the manifests that tell the packages apart), (C) the rest of the files by rank. Looking at another
//! file ([`Indexer::set_focus`]) ranks everything that is still waiting again. The index is
//! rebuilt at every open and kept in memory only, so the order matters on every start.
//!
//! # Being polite
//!
//! - The pool is half the cores (1 or 2, see [`AUTO_WORKERS_MAX`]), each thread at a low priority ([`apply_thread_priority`]:
//!   the macOS utility QoS class, nice on Linux, below-normal on Windows).
//! - The app owns an [`ActivityGate`] and touches it when the user draws or scrolls; while it is
//!   busy only tier A is served (the neighbourhood is a few files and what the reviewer is waiting
//!   for), the rest resumes after `quiet_ms` of quiet.
//! - Reading is bounded (one file at most [`Exclusions::max_file_bytes`]), the index is bounded by
//!   its caps, and nothing here blocks the caller: [`Indexer::set_focus`] and
//!   [`Indexer::file_saved`] only leave a note for a worker, and a query takes a read lock that a
//!   worker holds for the few microseconds a file's definitions take to put in.
//!
//! No global state: the gate, the queue and the index are fields of what [`Indexer::start`]
//! returns, and stopping (or dropping) it joins the threads.
//!
//! # Plugging it in
//!
//! ```text
//! let gate = ActivityGate::new();              // clone it into the pen and scroll handlers: gate.touch()
//! let indexer = Indexer::start(Arc::new(DiskSource::new(root)), open_file.as_deref(),
//!                              IndexerSettings::default(), gate.clone());
//! indexer.set_focus(rel_path);                 // when another file is shown
//! indexer.file_saved(rel_path);                // after a save or an accepted export
//! let p = indexer.progress();                  // t_args(p.l10n_key(), &p.l10n_args()) for the status line
//! let found = indexer.index().lookup(&["css", "error"]);
//! ```

use std::{
    cmp::Reverse,
    collections::{BinaryHeap, HashSet},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Condvar, Mutex, MutexGuard, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use crate::{
    symbols::{parent_dir, Caps, Index, Kind, Layout, Scanned},
    symbols_manifest::scan_manifest,
    symbols_scan::{scan_code, ScanCaps},
    symbols_source::{classify, inspect, is_manifest_name, DirEntry, EntryKind, Exclusions, FileClass, FileSource, Lang, ReadError, SkipReason},
};

/// The most files and folders a focus names as its neighbours; the rest of an import list is left out.
pub const MAX_FOCUS_CANDIDATES: usize = 4_000;
/// The most import lines read for a focus.
const MAX_FOCUS_IMPORTS: usize = 2_000;
/// The most `super::` steps followed.
const MAX_SUPER: usize = 64;

// ---------------------------------------------------------------------------------------------
// Rank
// ---------------------------------------------------------------------------------------------

/// How a file stands to the focus, nearest first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Class {
    /// The file looked at.
    Focus,
    /// A file in the same folder.
    SameDir,
    /// Its parent module, the crate's root, and what it names in its `use`, `mod`, `import` and
    /// `#include` lines.
    Module,
    /// Another file of the same package (the nearest folder with a manifest).
    SameCrate,
    /// A file of a package of the workspace the focus imports from.
    ImportedCrate,
    /// Everything else.
    Rest,
}

/// A class and, within it, the distance between the folders.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Rank {
    pub class: Class,
    pub distance: u32,
}

/// The sort key of a rank: the class, then the distance, then `seq` (first come first served).
#[must_use]
pub fn rank_key(rank: Rank, seq: u32) -> u64 {
    ((rank.class as u64) << 48) | (u64::from(rank.distance.min(0xFFFF)) << 32) | u64::from(seq)
}

fn depth_of(dir: &str) -> usize {
    if dir.is_empty() {
        0
    } else {
        dir.split('/').count()
    }
}

/// The steps between two folders: up from `a` to the folder they share, down to `b`.
#[must_use]
pub fn path_distance(a: &str, b: &str) -> u32 {
    let common = if a.is_empty() || b.is_empty() {
        0
    } else {
        a.split('/').zip(b.split('/')).take_while(|(x, y)| x == y).count()
    };
    u32::try_from(depth_of(a) + depth_of(b) - 2 * common).unwrap_or(u32::MAX)
}

/// What the focus is and what it names.
#[derive(Clone, Debug, Default)]
pub struct Focus {
    pub file: String,
    pub dir: String,
    /// The nearest folder with a manifest.
    pub crate_dir: Option<String>,
    /// Files the focus's module tree and import lines point at (candidates: they may not exist).
    pub module_files: HashSet<String>,
    /// Folders of the packages it imports from.
    pub import_crates: HashSet<String>,
    /// The packages it imports whose folder is not known (yet): when a manifest names one the focus
    /// is worked out again.
    pub unresolved: HashSet<String>,
}

fn file_stem(file: &str) -> &str {
    let name = file.rsplit('/').next().unwrap_or(file);
    name.rsplit_once('.').map_or(name, |(stem, _)| stem)
}

fn join(base: &str, rel: &str) -> String {
    if base.is_empty() {
        rel.to_string()
    } else if rel.is_empty() {
        base.to_string()
    } else {
        format!("{base}/{rel}")
    }
}

/// `base` and a relative path with `.` and `..` parts, or `None` when it climbs out of the workspace.
fn join_normalised(base: &str, rel: &str) -> Option<String> {
    let mut parts: Vec<&str> = if base.is_empty() { Vec::new() } else { base.split('/').collect() };
    for part in rel.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            other => parts.push(other),
        }
    }
    Some(parts.join("/"))
}

/// `FooBar` as a file name: `foo_bar`.
fn snake_case(item: &str) -> String {
    crate::symbols_tokens::split_identifier(item).join("_")
}

impl Focus {
    fn add_file(&mut self, path: String) {
        if self.module_files.len() < MAX_FOCUS_CANDIDATES {
            self.module_files.insert(path);
        }
    }

    /// `base/seg0/seg1.rs` and `base/seg0/seg1/mod.rs` for every prefix of `segments` that is a
    /// module name, and the file named for a last segment that is an item (`Node` is `node.rs`).
    fn add_rust_module_path(&mut self, base: &str, segments: &[&str]) {
        let mut dir = base.to_string();
        for (n, seg) in segments.iter().enumerate() {
            if seg.is_empty() || *seg == "*" || *seg == "self" {
                break;
            }
            let is_item = seg.chars().next().is_some_and(char::is_uppercase);
            if is_item {
                if n + 1 == segments.len() || n == segments.len().saturating_sub(1) {
                    let file = format!("{}.rs", snake_case(seg));
                    self.add_file(join(&dir, &file));
                }
                break;
            }
            let next = join(&dir, seg);
            self.add_file(format!("{next}.rs"));
            self.add_file(format!("{next}/mod.rs"));
            dir = next;
        }
    }
}

fn is_std_crate(name: &str) -> bool {
    matches!(name, "std" | "core" | "alloc" | "proc_macro" | "test")
}

fn rust_neighbours(f: &mut Focus, imports: &[String], layout: &Layout) {
    let stem = file_stem(&f.file).to_string();
    let dir = f.dir.clone();
    let is_module_file = matches!(stem.as_str(), "mod" | "lib" | "main");
    let src_root = f.crate_dir.as_ref().map(|c| join(c, "src"));
    // The folders that hold the children of this module, of its parent, of the grandparent ...
    let mut levels: Vec<String> = Vec::new();
    levels.push(if is_module_file { dir.clone() } else { join(&dir, &stem) });
    if !is_module_file {
        levels.push(dir.clone());
    }
    while levels.len() <= MAX_SUPER + 1 {
        let last = levels.last().map_or("", String::as_str);
        let up = parent_dir(last).to_string();
        let stop = last.is_empty();
        levels.push(up);
        if stop {
            break;
        }
    }
    // The parent module's file and the crate's root.
    f.add_file(join(&dir, "mod.rs"));
    let dirname = dir.rsplit('/').next().unwrap_or("");
    if !dirname.is_empty() {
        f.add_file(format!("{}.rs", join(parent_dir(&dir), dirname)));
    }
    if let Some(root) = &src_root {
        f.add_file(join(root, "lib.rs"));
        f.add_file(join(root, "main.rs"));
    }
    for import in imports.iter().take(MAX_FOCUS_IMPORTS) {
        let segments: Vec<&str> = import.split("::").collect();
        let Some(&first) = segments.first() else {
            continue;
        };
        match first {
            "crate" => {
                if let Some(root) = &src_root {
                    f.add_rust_module_path(root, &segments[1..]);
                }
            }
            "self" => f.add_rust_module_path(&levels[0], &segments[1..]),
            "super" => {
                let ups = segments.iter().take_while(|s| **s == "super").count().min(MAX_SUPER);
                if let Some(base) = levels.get(ups) {
                    let base = base.clone();
                    f.add_rust_module_path(&base, &segments[ups.min(segments.len())..]);
                }
            }
            "" => {}
            name if is_std_crate(name) => {}
            name => {
                if let Some(crate_dir) = layout.package_dir(name) {
                    let crate_dir = crate_dir.to_string();
                    f.add_rust_module_path(&join(&crate_dir, "src"), &segments[1..]);
                    f.import_crates.insert(crate_dir);
                } else {
                    f.unresolved.insert(name.replace('-', "_").to_lowercase());
                }
            }
        }
    }
}

fn python_neighbours(f: &mut Focus, imports: &[String]) {
    let root = f.crate_dir.clone().unwrap_or_default();
    for import in imports.iter().take(MAX_FOCUS_IMPORTS) {
        let dots = import.chars().take_while(|c| *c == '.').count();
        let rest = &import[dots..];
        let parts: Vec<&str> = rest.split('.').filter(|p| !p.is_empty()).collect();
        let bases: Vec<String> = if dots > 0 {
            let mut base = f.dir.clone();
            for _ in 1..dots {
                base = parent_dir(&base).to_string();
            }
            vec![base]
        } else {
            vec![root.clone(), join(&root, "src"), f.dir.clone()]
        };
        for base in bases {
            let mut dir = base;
            for part in &parts {
                let next = join(&dir, part);
                f.add_file(format!("{next}.py"));
                f.add_file(format!("{next}/__init__.py"));
                dir = next;
            }
        }
    }
}

static SCRIPT_EXTENSIONS: [&str; 8] = ["ts", "tsx", "js", "jsx", "mjs", "cjs", "mts", "cts"];

fn script_neighbours(f: &mut Focus, imports: &[String]) {
    for import in imports.iter().take(MAX_FOCUS_IMPORTS) {
        if !import.starts_with('.') {
            continue;
        }
        let Some(target) = join_normalised(&f.dir, import) else {
            continue;
        };
        f.add_file(target.clone());
        for ext in SCRIPT_EXTENSIONS {
            f.add_file(format!("{target}.{ext}"));
            f.add_file(format!("{target}/index.{ext}"));
        }
    }
}

fn c_neighbours(f: &mut Focus, imports: &[String]) {
    for import in imports.iter().take(MAX_FOCUS_IMPORTS) {
        if import.starts_with('<') || import.is_empty() {
            continue;
        }
        if let Some(path) = join_normalised(&f.dir, import) {
            f.add_file(path);
        }
        if let Some(crate_dir) = f.crate_dir.clone() {
            if let Some(path) = join_normalised(&crate_dir, import) {
                f.add_file(path);
            }
            if let Some(path) = join_normalised(&join(&crate_dir, "include"), import) {
                f.add_file(path);
            }
        }
    }
}

/// The focus of `file`, which imports `imports` (as the scanner wrote them): its folder, its
/// package, and the files its module tree and import lines point at.
#[must_use]
pub fn focus_of(file: &str, imports: &[String], layout: &Layout) -> Focus {
    let mut f = Focus {
        file: file.to_string(),
        dir: parent_dir(file).to_string(),
        crate_dir: layout.crate_dir_of(file).map(str::to_string),
        ..Focus::default()
    };
    match classify(file) {
        FileClass::Code(Lang::Rust) => rust_neighbours(&mut f, imports, layout),
        FileClass::Code(Lang::Python) => python_neighbours(&mut f, imports),
        FileClass::Code(Lang::JavaScript | Lang::TypeScript) => script_neighbours(&mut f, imports),
        FileClass::Code(Lang::C | Lang::Cpp) => c_neighbours(&mut f, imports),
        _ => {}
    }
    f
}

/// How `path` stands to `focus`.
#[must_use]
pub fn rank(focus: &Focus, path: &str, layout: &Layout) -> Rank {
    let dir = parent_dir(path);
    let distance = path_distance(&focus.dir, dir);
    let class = if path == focus.file {
        Class::Focus
    } else if dir == focus.dir {
        Class::SameDir
    } else if focus.module_files.contains(path) {
        Class::Module
    } else {
        match layout.crate_dir_of(path) {
            Some(c) if focus.crate_dir.as_deref() == Some(c) => Class::SameCrate,
            Some(c) if focus.import_crates.contains(c) => Class::ImportedCrate,
            _ => Class::Rest,
        }
    };
    Rank { class, distance }
}

// ---------------------------------------------------------------------------------------------
// The gate
// ---------------------------------------------------------------------------------------------

struct GateInner {
    origin: Instant,
    /// Milliseconds since `origin` of the last touch, plus one (0: never touched).
    last: AtomicU64,
}

/// What the app tells the scan about the user: "now the pen moves, the page scrolls". A cheap
/// handle (clone it into whatever sees the events); the scan reads it between files.
#[derive(Clone)]
pub struct ActivityGate(Arc<GateInner>);

impl Default for ActivityGate {
    fn default() -> Self {
        Self::new()
    }
}

impl ActivityGate {
    #[must_use]
    pub fn new() -> Self {
        Self(Arc::new(GateInner { origin: Instant::now(), last: AtomicU64::new(0) }))
    }

    fn now_ms(&self) -> u64 {
        u64::try_from(self.0.origin.elapsed().as_millis()).unwrap_or(u64::MAX - 1)
    }

    /// The user is active now.
    pub fn touch(&self) {
        self.touch_at(self.now_ms());
    }

    /// The user was active at `ms` on the gate's clock (milliseconds since the gate was made).
    pub fn touch_at(&self, ms: u64) {
        self.0.last.fetch_max(ms.saturating_add(1), Ordering::Relaxed);
    }

    /// Whether the user was active within the last `quiet_ms` before `now_ms`.
    #[must_use]
    pub fn busy_at(&self, now_ms: u64, quiet_ms: u64) -> bool {
        let last = self.0.last.load(Ordering::Relaxed);
        last != 0 && now_ms.saturating_add(1) < last.saturating_add(quiet_ms)
    }

    /// Whether the user was active within the last `quiet_ms`.
    #[must_use]
    pub fn busy_within(&self, quiet_ms: u64) -> bool {
        self.busy_at(self.now_ms(), quiet_ms)
    }
}

// ---------------------------------------------------------------------------------------------
// Thread priority
// ---------------------------------------------------------------------------------------------

/// How much a scan thread yields to the rest of the machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThreadPriority {
    Normal,
    /// The macOS utility class, nice 10, below normal.
    Low,
    /// The macOS background class (I/O throttled too), nice 19, lowest.
    Idle,
}

#[cfg(target_os = "macos")]
mod platform {
    use super::ThreadPriority;

    pub const QOS_UTILITY: u32 = 0x11;
    pub const QOS_BACKGROUND: u32 = 0x09;

    extern "C" {
        fn pthread_set_qos_class_self_np(qos_class: u32, relative_priority: i32) -> i32;
        fn pthread_get_qos_class_np(thread: usize, qos_class: *mut u32, relative_priority: *mut i32) -> i32;
        fn pthread_self() -> usize;
    }

    pub fn lower(priority: ThreadPriority) -> bool {
        let class = match priority {
            ThreadPriority::Normal => return true,
            ThreadPriority::Low => QOS_UTILITY,
            ThreadPriority::Idle => QOS_BACKGROUND,
        };
        // SAFETY: libSystem's call takes two plain integers and changes the calling thread only.
        unsafe { pthread_set_qos_class_self_np(class, 0) == 0 }
    }

    pub fn current() -> Option<u32> {
        let mut class = 0u32;
        let mut relative = 0i32;
        // SAFETY: both out-pointers are valid for the call; `pthread_self` is this thread.
        let status = unsafe { pthread_get_qos_class_np(pthread_self(), &mut class, &mut relative) };
        (status == 0).then_some(class)
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use super::ThreadPriority;

    extern "C" {
        fn setpriority(which: i32, who: u32, prio: i32) -> i32;
    }

    pub fn lower(priority: ThreadPriority) -> bool {
        let nice = match priority {
            ThreadPriority::Normal => return true,
            ThreadPriority::Low => 10,
            ThreadPriority::Idle => 19,
        };
        // SAFETY: PRIO_PROCESS (0) with who 0 is the calling thread on Linux; plain integers only.
        unsafe { setpriority(0, 0, nice) == 0 }
    }
}

#[cfg(windows)]
mod platform {
    use super::ThreadPriority;

    extern "system" {
        fn GetCurrentThread() -> isize;
        fn SetThreadPriority(thread: isize, priority: i32) -> i32;
    }

    pub fn lower(priority: ThreadPriority) -> bool {
        let level = match priority {
            ThreadPriority::Normal => return true,
            ThreadPriority::Low => -1,
            ThreadPriority::Idle => -2,
        };
        // SAFETY: the pseudo-handle of the calling thread and a documented priority level.
        unsafe { SetThreadPriority(GetCurrentThread(), level) != 0 }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
mod platform {
    use super::ThreadPriority;

    pub fn lower(priority: ThreadPriority) -> bool {
        priority == ThreadPriority::Normal
    }
}

#[cfg(target_os = "macos")]
pub use platform::{QOS_BACKGROUND, QOS_UTILITY};

/// The QoS class of the calling thread (macOS), for a test or a log.
#[cfg(target_os = "macos")]
#[must_use]
pub fn current_qos() -> Option<u32> {
    platform::current()
}

/// Lowers the calling thread's priority. `true` when the system did it ([`ThreadPriority::Normal`]
/// always is).
pub fn apply_thread_priority(priority: ThreadPriority) -> bool {
    platform::lower(priority)
}

// ---------------------------------------------------------------------------------------------
// The scan
// ---------------------------------------------------------------------------------------------

/// What the scan may do.
#[derive(Clone, Debug)]
pub struct IndexerSettings {
    /// Threads: 0 is half the cores, one to four.
    pub workers: usize,
    pub priority: ThreadPriority,
    pub exclusions: Exclusions,
    pub caps: Caps,
    pub scan_caps: ScanCaps,
    /// How long after the last touch of the gate the far files wait, milliseconds.
    pub quiet_ms: u64,
    /// How often a held scan looks at the gate again, milliseconds.
    pub poll_ms: u64,
}

impl Default for IndexerSettings {
    fn default() -> Self {
        Self {
            workers: 0,
            priority: ThreadPriority::Low,
            exclusions: Exclusions::default(),
            caps: Caps::default(),
            scan_caps: ScanCaps::default(),
            quiet_ms: 250,
            poll_ms: 20,
        }
    }
}

/// How the scan stands, for the status line ("indexing 4,120 / 18,310 files").
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Progress {
    /// Files to read found so far (code and manifests; the walk is still adding to it while
    /// `walking`).
    pub files_found: u64,
    /// Of them, scanned or skipped.
    pub files_done: u64,
    /// Files seen that are of no kind the index reads (counted, not queued).
    pub files_ignored: u64,
    /// Folders listed.
    pub dirs_done: u64,
    /// Folders are still to be listed.
    pub walking: bool,
    /// Nothing is waiting and nothing is being read.
    pub idle: bool,
}

impl Progress {
    /// The localization key of the status sentence: reading what the walk found, indexing, or ready.
    #[must_use]
    pub const fn l10n_key(&self) -> &'static str {
        if self.idle {
            "azreview-symbols-status-ready"
        } else if self.walking {
            "azreview-symbols-status-walking"
        } else {
            "azreview-symbols-status-indexing"
        }
    }

    /// The arguments of the sentence: `found` and `done`.
    #[must_use]
    pub fn l10n_args(&self) -> Vec<(&'static str, String)> {
        vec![("found", self.files_found.to_string()), ("done", self.files_done.to_string())]
    }
}

/// The most threads an automatic pool has: the whole Azul tree (3.1 million lines, 123 MB of source)
/// costs about 1.4 seconds of CPU in all, one thread reads it in 0.9 s and four were not faster on an
/// 8-core Mac that other work was using; more threads only add what the window can feel.
pub const AUTO_WORKERS_MAX: usize = 2;

/// The threads of a pool: `asked` (0: half of `cores`, one to [`AUTO_WORKERS_MAX`]), at most `cores`.
#[must_use]
pub fn workers_for(asked: usize, cores: usize) -> usize {
    let cores = cores.max(1);
    if asked == 0 {
        (cores / 2).clamp(1, AUTO_WORKERS_MAX)
    } else {
        asked.clamp(1, cores)
    }
}

enum TaskKind {
    Dir { rel: String },
    File { rel: String, class: FileClass, mtime_ms: u64 },
}

struct Task {
    key: u64,
    seq: u32,
    /// Served first whatever the focus (the focus file, the folders above it, a saved file).
    urgent: bool,
    kind: TaskKind,
}

/// The tier of a key: 0 the neighbourhood and the manifests, 1 folders to list, 2 the other files.
const TIER_SHIFT: u32 = 56;

struct Queue {
    tasks: Vec<Option<Task>>,
    free: Vec<usize>,
    heap: BinaryHeap<Reverse<(u64, usize)>>,
    seen_dirs: HashSet<String>,
    seen_files: HashSet<String>,
    focus: Focus,
    focus_path: Option<String>,
    focus_imports: Vec<String>,
    /// The focus is to be worked out again (its imports are known, or a package it needs appeared).
    refocus: bool,
    layout: Layout,
    in_flight: usize,
    dirs_waiting: usize,
    dirs_in_flight: usize,
    seq: u32,
}

impl Queue {
    fn new() -> Self {
        Self {
            tasks: Vec::new(),
            free: Vec::new(),
            heap: BinaryHeap::new(),
            seen_dirs: HashSet::new(),
            seen_files: HashSet::new(),
            focus: Focus::default(),
            focus_path: None,
            focus_imports: Vec::new(),
            refocus: false,
            layout: Layout::default(),
            in_flight: 0,
            dirs_waiting: 0,
            dirs_in_flight: 0,
            seq: 0,
        }
    }

    fn next_seq(&mut self) -> u32 {
        self.seq = self.seq.wrapping_add(1);
        self.seq
    }

    fn push(&mut self, task: Task) {
        if matches!(task.kind, TaskKind::Dir { .. }) {
            self.dirs_waiting += 1;
        }
        let key = task.key;
        let idx = if let Some(slot) = self.free.pop() {
            self.tasks[slot] = Some(task);
            slot
        } else {
            self.tasks.push(Some(task));
            self.tasks.len() - 1
        };
        self.heap.push(Reverse((key, idx)));
    }

    /// The key of a file waiting: manifests and the focus's neighbourhood first (tier 0), the rest
    /// by rank (tier 2).
    fn file_key(&self, rel: &str, manifest: bool, seq: u32) -> u64 {
        let rank = if manifest { Rank { class: Class::Focus, distance: 1 } } else { rank(&self.focus, rel, &self.layout) };
        let tier: u64 = if rank.class <= Class::Module { 0 } else { 2 };
        (tier << TIER_SHIFT) | rank_key(rank, seq)
    }

    /// The key of a folder waiting to be listed: tier 1, by the rank of a file that could be in it.
    fn dir_key(&self, rel: &str, seq: u32) -> u64 {
        let probe = format!("{rel}/.");
        (1u64 << TIER_SHIFT) | rank_key(rank(&self.focus, &probe, &self.layout), seq)
    }

    fn urgent_key(seq: u32) -> u64 {
        rank_key(Rank { class: Class::Focus, distance: 0 }, seq)
    }

    fn push_file(&mut self, rel: String, class: FileClass, mtime_ms: u64, urgent: bool) {
        let seq = self.next_seq();
        let manifest = matches!(class, FileClass::Manifest(_));
        let key = if urgent { Self::urgent_key(seq) } else { self.file_key(&rel, manifest, seq) };
        self.push(Task { key, seq, urgent, kind: TaskKind::File { rel, class, mtime_ms } });
    }

    fn push_dir(&mut self, rel: String, urgent: bool) {
        if !self.seen_dirs.insert(rel.clone()) {
            return;
        }
        let seq = self.next_seq();
        let key = if urgent { Self::urgent_key(seq) } else { self.dir_key(&rel, seq) };
        self.push(Task { key, seq, urgent, kind: TaskKind::Dir { rel } });
    }

    /// The folders from the root down to the one that holds `file`: listed first, they tell what is
    /// beside the focus and where the manifests are.
    fn seed_ancestors(&mut self, file: &str) {
        let mut dirs: Vec<String> = vec![String::new()];
        let mut acc = String::new();
        for part in parent_dir(file).split('/').filter(|p| !p.is_empty()) {
            acc = join(&acc, part);
            dirs.push(acc.clone());
        }
        for dir in dirs {
            self.push_dir(dir, true);
        }
    }

    fn peek_key(&self) -> Option<u64> {
        self.heap.peek().map(|Reverse((key, _))| *key)
    }

    fn pop(&mut self) -> Option<Task> {
        while let Some(Reverse((key, idx))) = self.heap.pop() {
            if self.tasks.get(idx).and_then(Option::as_ref).is_none_or(|t| t.key != key) {
                continue;
            }
            let task = self.tasks[idx].take();
            self.free.push(idx);
            if let Some(task) = &task {
                if matches!(task.kind, TaskKind::Dir { .. }) {
                    self.dirs_waiting = self.dirs_waiting.saturating_sub(1);
                    self.dirs_in_flight += 1;
                }
            }
            return task;
        }
        None
    }

    /// Ranks everything that waits again, against `focus`.
    fn rerank(&mut self, focus: Focus) {
        self.focus = focus;
        let mut heap = BinaryHeap::with_capacity(self.heap.len());
        for idx in 0..self.tasks.len() {
            let Some(mut task) = self.tasks[idx].take() else {
                continue;
            };
            if !task.urgent {
                task.key = match &task.kind {
                    TaskKind::File { rel, class, .. } => self.file_key(rel, matches!(class, FileClass::Manifest(_)), task.seq),
                    TaskKind::Dir { rel } => self.dir_key(rel, task.seq),
                };
            }
            heap.push(Reverse((task.key, idx)));
            self.tasks[idx] = Some(task);
        }
        self.heap = heap;
    }

    /// Works the focus out again from what is known of it.
    fn refocus_now(&mut self) {
        self.refocus = false;
        if let Some(path) = self.focus_path.clone() {
            let focus = focus_of(&path, &self.focus_imports, &self.layout);
            self.rerank(focus);
        }
    }
}

#[derive(Default)]
struct Mail {
    focus: Option<String>,
    saved: Vec<String>,
}

/// What the focus is known to be before the queue is locked: whether the file was scanned and what
/// it imports.
struct FocusInfo {
    path: String,
    scanned: bool,
    imports: Vec<String>,
}

struct Shared {
    source: Arc<dyn FileSource>,
    index: RwLock<Index>,
    queue: Mutex<Queue>,
    mail: Mutex<Mail>,
    cv: Condvar,
    idle_cv: Condvar,
    stop: AtomicBool,
    activity: ActivityGate,
    settings: IndexerSettings,
    files_found: AtomicU64,
    files_done: AtomicU64,
    files_ignored: AtomicU64,
    dirs_done: AtomicU64,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// What a worker did, to be booked under the queue's lock.
enum Done {
    Dir {
        rel: String,
        entries: Option<Vec<DirEntry>>,
        has_manifest: bool,
    },
    File {
        rel: String,
        /// The first time this file is done.
        first: bool,
        packages: Vec<String>,
        /// The imports of the file when it is the focus.
        imports: Option<Vec<String>>,
    },
}

impl Shared {
    fn is_idle(&self, q: &Queue) -> bool {
        q.heap.is_empty() && q.in_flight == 0 && {
            let mail = lock(&self.mail);
            mail.focus.is_none() && mail.saved.is_empty()
        }
    }

    fn index_read(&self) -> RwLockReadGuard<'_, Index> {
        self.index.read().unwrap_or_else(PoisonError::into_inner)
    }

    fn index_write(&self) -> RwLockWriteGuard<'_, Index> {
        self.index.write().unwrap_or_else(PoisonError::into_inner)
    }

    /// Applies what the app asked for: a new focus (its neighbourhood goes to the front), files saved.
    fn apply_mail(&self, q: &mut Queue, mail: Mail, focus: Option<FocusInfo>) {
        if let Some(info) = focus {
            q.focus_path = Some(info.path.clone());
            q.focus_imports = info.imports;
            q.focus = Focus {
                file: info.path.clone(),
                dir: parent_dir(&info.path).to_string(),
                crate_dir: q.layout.crate_dir_of(&info.path).map(str::to_string),
                ..Focus::default()
            };
            q.seed_ancestors(&info.path);
            if info.scanned {
                q.refocus = true;
            } else if let class @ (FileClass::Code(_) | FileClass::Manifest(_)) = classify(&info.path) {
                if q.seen_files.insert(info.path.clone()) {
                    self.files_found.fetch_add(1, Ordering::Relaxed);
                }
                q.push_file(info.path.clone(), class, 0, true);
            }
            // What waits is ranked against the new focus at once; its imports follow.
            let focus = q.focus.clone();
            q.rerank(focus);
        }
        for path in mail.saved {
            if let class @ (FileClass::Code(_) | FileClass::Manifest(_)) = classify(&path) {
                if q.seen_files.insert(path.clone()) {
                    self.files_found.fetch_add(1, Ordering::Relaxed);
                }
                q.push_file(path, class, 0, true);
            }
        }
    }

    /// Reads one file and puts what it holds in the index. Returns whether it was the first time
    /// the file was done, and the names of the packages a manifest declared.
    fn scan_file(&self, rel: &str, class: FileClass, mtime_ms: u64, imports_wanted: bool) -> (bool, Vec<String>, Option<Vec<String>>) {
        let exclusions = &self.settings.exclusions;
        let bytes = match self.source.read(rel, exclusions.max_file_bytes) {
            Ok(bytes) => bytes,
            Err(ReadError::TooLarge(_)) => return (self.index_write().put_skipped(rel, SkipReason::TooLarge), Vec::new(), None),
            Err(_) => return (self.index_write().put_skipped(rel, SkipReason::Unreadable), Vec::new(), None),
        };
        if let Some(reason) = inspect(&bytes) {
            return (self.index_write().put_skipped(rel, reason), Vec::new(), None);
        }
        let mtime = if mtime_ms == 0 { self.source.stat(rel).map_or(0, |m| m.mtime_ms) } else { mtime_ms };
        let mut scanned: Scanned = match class {
            FileClass::Code(lang) => scan_code(lang, rel, &bytes, &self.settings.scan_caps),
            FileClass::Manifest(kind) => scan_manifest(kind, rel, &bytes, &self.settings.scan_caps),
            FileClass::Other => return (false, Vec::new(), None),
        };
        scanned.mtime_ms = mtime;
        let packages: Vec<String> = scanned.defs.iter().filter(|d| d.kind == Kind::Package).map(|d| d.name.clone()).collect();
        let imports = imports_wanted.then(|| scanned.imports.clone());
        let first = self.index_write().put_scanned(rel, scanned);
        (first, packages, imports)
    }

    /// Does the work of a task; no lock held but the index's for a put.
    fn execute(&self, task: &Task, is_focus: bool) -> Done {
        match &task.kind {
            TaskKind::Dir { rel } => {
                let entries = self.source.list(rel).ok();
                let has_manifest = entries
                    .as_ref()
                    .is_some_and(|list| list.iter().any(|e| e.kind == EntryKind::File && is_manifest_name(&e.name)));
                if has_manifest {
                    self.index_write().note_manifest_dir(rel);
                }
                Done::Dir { rel: rel.clone(), entries, has_manifest }
            }
            TaskKind::File { rel, class, mtime_ms } => {
                // A file that waited while it was scanned for another reason is not read twice.
                if !task.urgent && self.index_read().has_scanned(rel) {
                    return Done::File { rel: rel.clone(), first: false, packages: Vec::new(), imports: None };
                }
                let (first, packages, imports) = self.scan_file(rel, *class, *mtime_ms, is_focus);
                Done::File { rel: rel.clone(), first, packages, imports }
            }
        }
    }

    /// Books what a worker did: new folders and files queued, packages and manifests known.
    fn book(&self, q: &mut Queue, done: Done) {
        match done {
            Done::Dir { rel, entries, has_manifest } => {
                q.dirs_in_flight = q.dirs_in_flight.saturating_sub(1);
                self.dirs_done.fetch_add(1, Ordering::Relaxed);
                if has_manifest {
                    q.layout.note_manifest_dir(&rel);
                    // A manifest above the focus tells which package it is in: its module tree and
                    // its imports can be resolved now.
                    let changed = q.focus_path.as_deref().is_some_and(|p| q.layout.crate_dir_of(p) != q.focus.crate_dir.as_deref());
                    if changed {
                        q.refocus = true;
                    }
                }
                let Some(entries) = entries else {
                    return;
                };
                let ex = &self.settings.exclusions;
                let depth = depth_of(&rel);
                for entry in entries {
                    let path = join(&rel, &entry.name);
                    match entry.kind {
                        EntryKind::Dir => {
                            if depth < ex.max_depth && !ex.skips_dir(&entry.name, has_manifest) {
                                q.push_dir(path, false);
                            }
                        }
                        EntryKind::File => {
                            if ex.skips_file(&entry.name) {
                                continue;
                            }
                            match classify(&path) {
                                class @ (FileClass::Code(_) | FileClass::Manifest(_)) => {
                                    if q.seen_files.insert(path.clone()) {
                                        self.files_found.fetch_add(1, Ordering::Relaxed);
                                        q.push_file(path, class, entry.mtime_ms, false);
                                    }
                                }
                                FileClass::Other => {
                                    self.files_ignored.fetch_add(1, Ordering::Relaxed);
                                }
                            }
                        }
                        EntryKind::Other => {}
                    }
                }
            }
            Done::File { rel, first, packages, imports } => {
                if first {
                    self.files_done.fetch_add(1, Ordering::Relaxed);
                }
                let dir = parent_dir(&rel).to_string();
                for name in &packages {
                    q.layout.set_package(name, &dir);
                    if q.focus.unresolved.contains(&name.replace('-', "_").to_lowercase()) {
                        q.refocus = true;
                    }
                }
                if let Some(imports) = imports {
                    if q.focus_path.as_deref() == Some(rel.as_str()) {
                        q.focus_imports = imports;
                        q.refocus = true;
                    }
                }
            }
        }
    }
}

fn worker(shared: &Arc<Shared>) {
    apply_thread_priority(shared.settings.priority);
    let poll = Duration::from_millis(shared.settings.poll_ms.max(1));
    loop {
        if shared.stop.load(Ordering::Relaxed) {
            return;
        }
        // What the focus is known to be, read before the queue is locked (the index is never locked under it).
        let peeked = lock(&shared.mail).focus.clone().map(|path| {
            let idx = shared.index_read();
            FocusInfo { scanned: idx.has_scanned(&path), imports: idx.imports_of(&path).to_vec(), path }
        });
        let (task, is_focus) = {
            let mut q = lock(&shared.queue);
            // The mail is taken under the queue's lock, so that "idle" is never seen between taking what
            // the app asked for and queueing it.
            let mut mail = std::mem::take(&mut *lock(&shared.mail));
            let focus = mail.focus.take().map(|path| match peeked {
                Some(info) if info.path == path => info,
                _ => FocusInfo { path, scanned: false, imports: Vec::new() },
            });
            if focus.is_some() || !mail.saved.is_empty() {
                shared.apply_mail(&mut q, mail, focus);
                shared.cv.notify_all();
            }
            if q.refocus {
                q.refocus_now();
            }
            let Some(key) = q.peek_key() else {
                if shared.is_idle(&q) {
                    shared.idle_cv.notify_all();
                }
                drop(shared.cv.wait_timeout(q, Duration::from_millis(50)).unwrap_or_else(PoisonError::into_inner));
                continue;
            };
            if key >> TIER_SHIFT >= 1 && shared.activity.busy_within(shared.settings.quiet_ms) {
                // The user draws: only the neighbourhood is served. Look again soon.
                drop(shared.cv.wait_timeout(q, poll).unwrap_or_else(PoisonError::into_inner));
                continue;
            }
            let Some(task) = q.pop() else {
                continue;
            };
            q.in_flight += 1;
            let is_focus = matches!(&task.kind, TaskKind::File { rel, .. } if q.focus_path.as_deref() == Some(rel.as_str()));
            (task, is_focus)
        };
        let done = shared.execute(&task, is_focus);
        let mut q = lock(&shared.queue);
        shared.book(&mut q, done);
        q.in_flight = q.in_flight.saturating_sub(1);
        if q.refocus {
            q.refocus_now();
        }
        if shared.is_idle(&q) {
            shared.idle_cv.notify_all();
        }
        drop(q);
        // What this task queued may be for another worker.
        shared.cv.notify_all();
    }
}

/// The background scan of one workspace and the index it fills.
pub struct Indexer {
    shared: Arc<Shared>,
    handles: Vec<JoinHandle<()>>,
}

impl Indexer {
    /// Starts the scan of the workspace `source`, outwards from `focus` (a workspace-relative path)
    /// when there is one. Returns at once; the index fills in the background.
    #[must_use]
    pub fn start(source: Arc<dyn FileSource>, focus: Option<&str>, settings: IndexerSettings, activity: ActivityGate) -> Self {
        let cores = thread::available_parallelism().map_or(1, usize::from);
        let workers = workers_for(settings.workers, cores);
        let mut queue = Queue::new();
        let mut found = 0u64;
        match focus {
            Some(path) => {
                queue.focus_path = Some(path.to_string());
                queue.focus = Focus { file: path.to_string(), dir: parent_dir(path).to_string(), ..Focus::default() };
                if let class @ (FileClass::Code(_) | FileClass::Manifest(_)) = classify(path) {
                    queue.seen_files.insert(path.to_string());
                    queue.push_file(path.to_string(), class, 0, true);
                    found = 1;
                }
                queue.seed_ancestors(path);
            }
            None => queue.push_dir(String::new(), true),
        }
        let shared = Arc::new(Shared {
            source,
            index: RwLock::new(Index::new(settings.caps.clone())),
            queue: Mutex::new(queue),
            mail: Mutex::new(Mail::default()),
            cv: Condvar::new(),
            idle_cv: Condvar::new(),
            stop: AtomicBool::new(false),
            activity,
            settings,
            files_found: AtomicU64::new(found),
            files_done: AtomicU64::new(0),
            files_ignored: AtomicU64::new(0),
            dirs_done: AtomicU64::new(0),
        });
        let handles = (0..workers)
            .filter_map(|n| {
                let shared = Arc::clone(&shared);
                thread::Builder::new().name(format!("symbols-scan-{n}")).spawn(move || worker(&shared)).ok()
            })
            .collect();
        Self { shared, handles }
    }

    /// The reviewer looks at another file: the files near it are scanned next (and at once, even
    /// while the user draws). Never blocks.
    pub fn set_focus(&self, rel: &str) {
        lock(&self.shared.mail).focus = Some(rel.to_string());
        self.shared.cv.notify_all();
    }

    /// A file was saved (by the user or the agent): it is scanned again at once and what the index
    /// had of it is replaced. Never blocks.
    pub fn file_saved(&self, rel: &str) {
        lock(&self.shared.mail).saved.push(rel.to_string());
        self.shared.cv.notify_all();
    }

    /// How the scan stands.
    #[must_use]
    pub fn progress(&self) -> Progress {
        let q = lock(&self.shared.queue);
        Progress {
            files_found: self.shared.files_found.load(Ordering::Relaxed),
            files_done: self.shared.files_done.load(Ordering::Relaxed),
            files_ignored: self.shared.files_ignored.load(Ordering::Relaxed),
            dirs_done: self.shared.dirs_done.load(Ordering::Relaxed),
            walking: q.dirs_waiting > 0 || q.dirs_in_flight > 0,
            idle: self.shared.is_idle(&q),
        }
    }

    /// The index, for a query. Held for the query only: a worker waits for it to put a file in.
    pub fn index(&self) -> RwLockReadGuard<'_, Index> {
        self.shared.index_read()
    }

    /// Waits until nothing is waiting and nothing is being read, at most `timeout`. `true` when
    /// the scan is idle. A far file held back for the user's drawing keeps the scan busy.
    #[must_use]
    pub fn wait_idle(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        let mut q = lock(&self.shared.queue);
        loop {
            if self.shared.is_idle(&q) {
                return true;
            }
            let now = Instant::now();
            if now >= deadline {
                return false;
            }
            let step = (deadline - now).min(Duration::from_millis(20));
            q = self.shared.idle_cv.wait_timeout(q, step).unwrap_or_else(PoisonError::into_inner).0;
        }
    }

    /// Ends the threads (what is queued is dropped). The index stays readable.
    pub fn stop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        self.shared.cv.notify_all();
        for handle in self.handles.drain(..) {
            let _ = handle.join();
        }
    }
}

impl Drop for Indexer {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
#[path = "symbols_sched_test.rs"]
mod symbols_sched_test;
