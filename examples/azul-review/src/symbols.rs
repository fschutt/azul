//! The symbol index of a workspace, in memory and very rough: for each definition its name, its
//! kind (struct, enum, trait, fn, method, module, class, dependency ...), its file, its line and a
//! capped extent, and the type or class it belongs to. No bodies, no docs, no types: it exists so
//! that what the reviewer SAYS ("the CssError", "the speech session", "the data table cell") can be
//! tied to the place it is defined without a model and without a grep, or, when it cannot be tied
//! for certain, so that the agent is told roughly what to grep for ([`crate::symbols_resolve`]).
//!
//! # What is kept, and what it costs
//!
//! - A **name** is one spelling (`CssError`, `css_error`); its **key** is the lower-case words of
//!   it joined by a space (`css error`), the same for every spelling ([`crate::symbols_tokens`]).
//!   Defs hang on keys, so "css error" finds `CssError`, `css_error` and `CSS_ERROR` alike.
//! - Each key keeps a phonetic skeleton for the hints, and every word of every key is counted
//!   ([`Index::token_count`]): the spoken-code normaliser asks whether `css` is a word of this
//!   workspace before it turns "see ess ess" into it.
//! - Names, keys and words are interned: `new` defined in a thousand files is one name.
//! - Everything is bounded by [`Caps`] (files, defs, defs per file, a name's length, and an estimate
//!   of the heap); when a cap is met the index stops growing, says so ([`Note`]) and goes on
//!   answering from what it holds. [`IndexStats::est_bytes`] is the running estimate the memory cap
//!   reads.
//!
//! The index is a plain value; the background scan ([`crate::symbols_sched`]) keeps it behind a
//! lock and puts a file in when it has been scanned ([`Index::put_scanned`]); a file saved by the
//! user is put in again, which replaces what the file had.

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use crate::{
    symbols_source::{is_manifest_name, Lang, SkipReason},
    symbols_tokens::{concat_of, key_of, phonetic, split_identifier, MAX_PHONETIC},
};

/// What a definition is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Kind {
    Struct,
    Enum,
    Union,
    Trait,
    /// A type alias, or an associated type.
    Type,
    Const,
    Static,
    Fn,
    /// A function that belongs to a type, a class or an impl.
    Method,
    Macro,
    /// A module, a namespace, a package of a language that has them.
    Module,
    Class,
    Interface,
    /// A file, found by its stem ("in the file symbols").
    File,
    /// The package a manifest names (a crate, an npm package, a Go module).
    Package,
    /// A package a manifest depends on, or one a file imports.
    Dependency,
}

impl Kind {
    /// Every kind, in declaration order.
    pub const ALL: [Self; 16] = [
        Self::Struct,
        Self::Enum,
        Self::Union,
        Self::Trait,
        Self::Type,
        Self::Const,
        Self::Static,
        Self::Fn,
        Self::Method,
        Self::Macro,
        Self::Module,
        Self::Class,
        Self::Interface,
        Self::File,
        Self::Package,
        Self::Dependency,
    ];

    /// The word of the code for it (what a language calls it; what the prompt shows).
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Struct => "struct",
            Self::Enum => "enum",
            Self::Union => "union",
            Self::Trait => "trait",
            Self::Type => "type",
            Self::Const => "const",
            Self::Static => "static",
            Self::Fn => "fn",
            Self::Method => "method",
            Self::Macro => "macro",
            Self::Module => "mod",
            Self::Class => "class",
            Self::Interface => "interface",
            Self::File => "file",
            Self::Package => "package",
            Self::Dependency => "dependency",
        }
    }

    /// Whether it names a type (a thing a value can be of).
    #[must_use]
    pub const fn is_type_like(self) -> bool {
        matches!(self, Self::Struct | Self::Enum | Self::Union | Self::Trait | Self::Type | Self::Class | Self::Interface)
    }
}

/// How visible a definition is outside its module.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub enum Vis {
    #[default]
    Private,
    /// `pub(crate)` and the like.
    Restricted,
    Public,
}

/// A definition as a scanner reports it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawDef {
    pub name: String,
    pub kind: Kind,
    pub vis: Vis,
    /// Inside a test module or a test function.
    pub test: bool,
    /// 1-based line of the name.
    pub line: u32,
    /// How many lines the definition spans (1: one line).
    pub lines: u32,
    /// The type, class or module it belongs to.
    pub parent: Option<String>,
}

impl RawDef {
    /// A one-line private definition at `line`.
    #[must_use]
    pub fn new(name: &str, kind: Kind, line: u32) -> Self {
        Self {
            name: name.to_string(),
            kind,
            vis: Vis::Private,
            test: false,
            line,
            lines: 1,
            parent: None,
        }
    }

    #[must_use]
    pub fn lines(mut self, lines: u32) -> Self {
        self.lines = lines;
        self
    }

    #[must_use]
    pub fn parent(mut self, parent: &str) -> Self {
        self.parent = Some(parent.to_string());
        self
    }

    #[must_use]
    pub fn public(mut self) -> Self {
        self.vis = Vis::Public;
        self
    }

    #[must_use]
    pub fn test(mut self) -> Self {
        self.test = true;
        self
    }
}

/// What a scan of one file found.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Scanned {
    /// The language, `None` for a manifest.
    pub lang: Option<Lang>,
    /// Bytes read.
    pub size: u64,
    /// The file's time, milliseconds since the Unix epoch.
    pub mtime_ms: u64,
    pub lines: u32,
    pub defs: Vec<RawDef>,
    /// What the file imports: `use` paths, `import` targets, `#include`s, as written.
    pub imports: Vec<String>,
    /// The file is a test file ([`crate::symbols_source::is_test_path`]): all its defs are test defs.
    pub test_file: bool,
}

/// A definition with its names spelled out: what a query answers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DefInfo {
    pub name: String,
    pub kind: Kind,
    pub vis: Vis,
    pub test: bool,
    /// Workspace-relative path with `/`.
    pub file: String,
    pub line: u32,
    pub lines: u32,
    pub parent: Option<String>,
}

macro_rules! id_type {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(pub u32);
    };
}

id_type!(
    /// A file of the index.
    FileId
);
id_type!(
    /// A definition of the index.
    DefId
);
id_type!(
    /// One spelling of a name.
    NameId
);
id_type!(
    /// The words of a name joined by a space: the key every spelling shares.
    KeyId
);

const NONE: u32 = u32::MAX;

/// A definition as the index keeps it: ids and numbers only.
#[derive(Clone, Copy, Debug)]
pub struct Def {
    pub name: NameId,
    pub key: KeyId,
    pub file: FileId,
    pub line: u32,
    pub lines: u32,
    parent: u32,
    pub kind: Kind,
    pub vis: Vis,
    pub test: bool,
}

impl Def {
    /// The type or class it belongs to, if any.
    #[must_use]
    pub const fn parent(&self) -> Option<NameId> {
        if self.parent == NONE {
            None
        } else {
            Some(NameId(self.parent))
        }
    }
}

/// What the index may hold.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Caps {
    /// Files known (scanned, skipped or waiting).
    pub max_files: usize,
    /// Definitions in all.
    pub max_defs: usize,
    /// Definitions of one file: the first ones are kept.
    pub max_defs_per_file: usize,
    /// Imports of one file.
    pub max_imports_per_file: usize,
    /// Bytes of a name; a longer one is cut on a character boundary.
    pub max_name_bytes: usize,
    /// The estimate of the heap the index may reach.
    pub max_heap_bytes: usize,
}

impl Default for Caps {
    fn default() -> Self {
        Self {
            max_files: 250_000,
            max_defs: 2_000_000,
            max_defs_per_file: 4_000,
            max_imports_per_file: 2_000,
            max_name_bytes: 128,
            max_heap_bytes: 256 * 1024 * 1024,
        }
    }
}

/// What the index tells about itself when it had to leave something out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Note {
    /// A file had more definitions than may be kept: the first `cap` were.
    FileCapped { file: String, cap: usize },
    /// The index holds as many definitions as it may.
    IndexFull { cap: usize },
    /// The index reached the memory it may use.
    MemoryFull { cap_bytes: usize },
    /// More files than the index may know of.
    TooManyFiles { cap: usize },
}

impl Note {
    /// The localization key of the note's sentence.
    #[must_use]
    pub const fn l10n_key(&self) -> &'static str {
        match self {
            Self::FileCapped { .. } => "azreview-symbols-note-file-capped",
            Self::IndexFull { .. } => "azreview-symbols-note-index-full",
            Self::MemoryFull { .. } => "azreview-symbols-note-memory-full",
            Self::TooManyFiles { .. } => "azreview-symbols-note-too-many-files",
        }
    }

    /// The arguments of the sentence.
    #[must_use]
    pub fn l10n_args(&self) -> Vec<(&'static str, String)> {
        match self {
            Self::FileCapped { file, cap } => vec![("file", file.clone()), ("cap", cap.to_string())],
            Self::IndexFull { cap } | Self::TooManyFiles { cap } => vec![("cap", cap.to_string())],
            Self::MemoryFull { cap_bytes } => vec![("mb", (cap_bytes / (1024 * 1024)).to_string())],
        }
    }
}

/// The most notes kept: a scan of a hostile tree must not grow them without end.
const MAX_NOTES: usize = 64;

/// What the index holds, counted.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IndexStats {
    /// Files known: scanned, skipped and waiting.
    pub files: usize,
    pub scanned: usize,
    /// Known and not scanned yet.
    pub pending: usize,
    /// Files skipped, by [`SkipReason::index`].
    pub skipped: [usize; 6],
    pub defs: usize,
    /// Distinct spellings.
    pub names: usize,
    /// Distinct word sequences.
    pub keys: usize,
    /// Distinct words.
    pub tokens: usize,
    /// Lines of the files scanned.
    pub lines: u64,
    /// Bytes of the files scanned.
    pub bytes_read: u64,
    /// The running estimate of the heap the index holds, bytes.
    pub est_bytes: usize,
    /// A cap was met: the index is not complete.
    pub full: bool,
}

/// Where the manifests are: which folders are projects and what their packages are called. The
/// scheduler keeps a copy for ranking, the index one for scoping a mention.
#[derive(Clone, Debug, Default)]
pub struct Layout {
    dirs: HashSet<String>,
    packages: HashMap<String, String>,
}

/// `Azul-Core` and `azul_core` are one package.
fn normalise_package(name: &str) -> String {
    name.chars().map(|c| if c == '-' { '_' } else { c.to_ascii_lowercase() }).collect()
}

/// The folder of `rel` (`""` for a file in the root).
#[must_use]
pub fn parent_dir(rel: &str) -> &str {
    rel.rsplit_once('/').map_or("", |(dir, _)| dir)
}

impl Layout {
    /// A folder holds a manifest.
    pub fn note_manifest_dir(&mut self, dir: &str) {
        if !self.dirs.contains(dir) {
            self.dirs.insert(dir.to_string());
        }
    }

    /// The package `name` lives in `dir`.
    pub fn set_package(&mut self, name: &str, dir: &str) {
        self.packages.insert(normalise_package(name), dir.to_string());
    }

    /// The package `name` is gone from `dir` (a package that moved elsewhere stays).
    pub fn remove_package(&mut self, name: &str, dir: &str) {
        let key = normalise_package(name);
        if self.packages.get(&key).is_some_and(|d| d == dir) {
            self.packages.remove(&key);
        }
    }

    /// The nearest folder at or above `rel`'s folder that holds a manifest.
    #[must_use]
    pub fn crate_dir_of(&self, rel: &str) -> Option<&str> {
        let mut dir = parent_dir(rel);
        loop {
            if let Some(found) = self.dirs.get(dir) {
                return Some(found.as_str());
            }
            if dir.is_empty() {
                return None;
            }
            dir = parent_dir(dir);
        }
    }

    /// The folder of the package `name`.
    #[must_use]
    pub fn package_dir(&self, name: &str) -> Option<&str> {
        self.packages.get(&normalise_package(name)).map(String::as_str)
    }

    /// Whether `dir` holds a manifest.
    #[must_use]
    pub fn is_manifest_dir(&self, dir: &str) -> bool {
        self.dirs.contains(dir)
    }
}

#[derive(Default)]
struct Interner {
    map: HashMap<Arc<str>, u32>,
    strings: Vec<Arc<str>>,
}

impl Interner {
    fn get(&self, s: &str) -> Option<u32> {
        self.map.get(s).copied()
    }

    /// The id of `s` and whether it is new.
    fn intern(&mut self, s: &str) -> (u32, bool) {
        if let Some(&id) = self.map.get(s) {
            return (id, false);
        }
        let id = u32::try_from(self.strings.len()).unwrap_or(NONE - 1);
        let shared: Arc<str> = Arc::from(s);
        self.strings.push(Arc::clone(&shared));
        self.map.insert(shared, id);
        (id, true)
    }

    fn str(&self, id: u32) -> &str {
        self.strings.get(id as usize).map_or("", |s| s)
    }
}

#[derive(Clone, Copy)]
struct KeyInfo {
    phon: [u8; MAX_PHONETIC],
    phon_len: u8,
    ntokens: u8,
}

/// A key as the resolver sees it.
#[derive(Clone, Copy, Debug)]
pub struct KeyView<'a> {
    pub id: KeyId,
    /// The words joined by a space.
    pub key: &'a str,
    /// The phonetic skeleton of the words joined.
    pub phonetic: &'a [u8],
    pub ntokens: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FileState {
    Pending,
    Scanned,
    Skipped(SkipReason),
    Removed,
}

struct FileEntry {
    path: Arc<str>,
    state: FileState,
    defs: Vec<u32>,
    imports: Vec<String>,
    size: u64,
    lines: u32,
    /// A package this file (a manifest) named: (name, folder).
    package: Option<(String, String)>,
}

/// Rough costs of what is added, bytes: the estimate behind [`Caps::max_heap_bytes`].
const COST_DEF: usize = std::mem::size_of::<Def>() + 8;
const COST_STRING: usize = 56;
const COST_KEY: usize = 56 + std::mem::size_of::<KeyInfo>() + 24;

/// The symbols of one workspace, in memory.
pub struct Index {
    caps: Caps,
    files: Vec<FileEntry>,
    file_ids: HashMap<Arc<str>, u32>,
    names: Interner,
    /// For each name: its key, or `NONE` for a name that was only ever somebody's parent.
    name_key: Vec<u32>,
    keys: Interner,
    key_defs: Vec<Vec<u32>>,
    key_info: Vec<KeyInfo>,
    tokens: HashMap<Arc<str>, u32>,
    defs: Vec<Def>,
    free: Vec<u32>,
    live_defs: usize,
    layout: Layout,
    notes: Vec<Note>,
    full: bool,
    bytes: usize,
    lines_total: u64,
    bytes_read: u64,
}

impl Index {
    #[must_use]
    pub fn new(caps: Caps) -> Self {
        Self {
            caps,
            files: Vec::new(),
            file_ids: HashMap::new(),
            names: Interner::default(),
            name_key: Vec::new(),
            keys: Interner::default(),
            key_defs: Vec::new(),
            key_info: Vec::new(),
            tokens: HashMap::new(),
            defs: Vec::new(),
            free: Vec::new(),
            live_defs: 0,
            layout: Layout::default(),
            notes: Vec::new(),
            full: false,
            bytes: 0,
            lines_total: 0,
            bytes_read: 0,
        }
    }

    fn note(&mut self, note: Note) {
        if self.notes.len() < MAX_NOTES && !self.notes.contains(&note) {
            self.notes.push(note);
        }
    }

    /// The file `rel`'s id, making it a known (waiting) file when it is new; `None` when the index
    /// knows as many files as it may.
    fn ensure_file(&mut self, rel: &str) -> Option<usize> {
        if let Some(&id) = self.file_ids.get(rel) {
            return Some(id as usize);
        }
        if self.file_ids.len() >= self.caps.max_files {
            let cap = self.caps.max_files;
            self.note(Note::TooManyFiles { cap });
            self.full = true;
            return None;
        }
        let id = self.files.len();
        let path: Arc<str> = Arc::from(rel);
        self.bytes += rel.len() * 2 + COST_STRING + std::mem::size_of::<FileEntry>();
        self.files.push(FileEntry {
            path: Arc::clone(&path),
            state: FileState::Pending,
            defs: Vec::new(),
            imports: Vec::new(),
            size: 0,
            lines: 0,
            package: None,
        });
        self.file_ids.insert(path, u32::try_from(id).unwrap_or(NONE - 1));
        Some(id)
    }

    /// Makes `rel` a known file that is not scanned yet.
    pub fn declare_file(&mut self, rel: &str, size: u64, _mtime_ms: u64) {
        if let Some(id) = self.ensure_file(rel) {
            if self.files[id].state == FileState::Pending {
                self.files[id].size = size;
            }
        }
    }

    /// Takes the definitions of file `id` out (the file stays known).
    fn clear_defs(&mut self, id: usize) {
        let old = std::mem::take(&mut self.files[id].defs);
        for def_id in old {
            let def = self.defs[def_id as usize];
            if let Some(list) = self.key_defs.get_mut(def.key.0 as usize) {
                list.retain(|d| *d != def_id);
            }
            self.free.push(def_id);
            self.live_defs = self.live_defs.saturating_sub(1);
            self.bytes = self.bytes.saturating_sub(COST_DEF);
        }
        if let Some((name, dir)) = self.files[id].package.take() {
            self.layout.remove_package(&name, &dir);
        }
        let size = self.files[id].size;
        let lines = u64::from(self.files[id].lines);
        if self.files[id].state == FileState::Scanned {
            self.bytes_read = self.bytes_read.saturating_sub(size);
            self.lines_total = self.lines_total.saturating_sub(lines);
        }
        self.files[id].imports = Vec::new();
    }

    /// Puts what a scan of `rel` found in, replacing what the file had. `true` when this is the
    /// first time the file is done (scanned or skipped), or when the index cannot know it.
    pub fn put_scanned(&mut self, rel: &str, scanned: Scanned) -> bool {
        let Some(id) = self.ensure_file(rel) else {
            return true;
        };
        let first = self.files[id].state == FileState::Pending;
        self.clear_defs(id);
        let dir = parent_dir(rel).to_string();
        let file_name = rel.rsplit('/').next().unwrap_or(rel);
        if is_manifest_name(file_name) {
            self.layout.note_manifest_dir(&dir);
        }
        {
            let file = &mut self.files[id];
            file.state = FileState::Scanned;
            file.size = scanned.size;
            file.lines = scanned.lines;
        }
        self.bytes_read += scanned.size;
        self.lines_total += u64::from(scanned.lines);
        let imports: Vec<String> = scanned.imports.into_iter().take(self.caps.max_imports_per_file).collect();
        self.bytes += imports.iter().map(|i| i.len() + 24).sum::<usize>();
        self.files[id].imports = imports;
        for (added, raw) in scanned.defs.into_iter().enumerate() {
            if added >= self.caps.max_defs_per_file {
                let (file, cap) = (rel.to_string(), self.caps.max_defs_per_file);
                self.note(Note::FileCapped { file, cap });
                break;
            }
            if self.live_defs >= self.caps.max_defs {
                let cap = self.caps.max_defs;
                self.note(Note::IndexFull { cap });
                self.full = true;
                break;
            }
            if self.bytes >= self.caps.max_heap_bytes {
                let cap_bytes = self.caps.max_heap_bytes;
                self.note(Note::MemoryFull { cap_bytes });
                self.full = true;
                break;
            }
            if raw.kind == Kind::Package && self.files[id].package.is_none() {
                let name = self.cut_name(&raw.name).to_string();
                self.layout.set_package(&name, &dir);
                self.files[id].package = Some((name, dir.clone()));
            }
            self.add_def(id, &raw, scanned.test_file);
        }
        first
    }

    fn cut_name<'a>(&self, name: &'a str) -> &'a str {
        let max = self.caps.max_name_bytes;
        if name.len() <= max {
            return name;
        }
        let mut end = max;
        while end > 0 && !name.is_char_boundary(end) {
            end -= 1;
        }
        &name[..end]
    }

    fn intern_name(&mut self, name: &str, with_key: bool) -> u32 {
        let (id, new) = self.names.intern(name);
        if new {
            self.name_key.push(NONE);
            self.bytes += name.len() + COST_STRING + 4;
        }
        if with_key && self.name_key[id as usize] == NONE {
            let words = split_identifier(name);
            let key = key_of(&words);
            let (key_id, key_new) = self.keys.intern(&key);
            if key_new {
                self.key_defs.push(Vec::new());
                let cat = concat_of(&key);
                let skeleton = phonetic(&cat);
                let mut phon = [0u8; MAX_PHONETIC];
                let n = skeleton.len().min(MAX_PHONETIC);
                phon[..n].copy_from_slice(&skeleton.as_bytes()[..n]);
                self.key_info.push(KeyInfo {
                    phon,
                    phon_len: u8::try_from(n).unwrap_or(0),
                    ntokens: u8::try_from(words.len()).unwrap_or(u8::MAX),
                });
                self.bytes += key.len() + COST_KEY;
                for word in &words {
                    if let Some(count) = self.tokens.get_mut(word.as_str()) {
                        *count += 1;
                    } else {
                        self.bytes += word.len() + COST_STRING;
                        self.tokens.insert(Arc::from(word.as_str()), 1);
                    }
                }
            }
            self.name_key[id as usize] = key_id;
        }
        id
    }

    fn add_def(&mut self, file: usize, raw: &RawDef, test_file: bool) {
        let name = self.cut_name(&raw.name).to_string();
        if name.is_empty() {
            return;
        }
        let name_id = self.intern_name(&name, true);
        let key = self.name_key[name_id as usize];
        let parent = raw.parent.as_deref().map(|p| self.cut_name(p).to_string()).filter(|p| !p.is_empty());
        let parent_id = parent.map_or(NONE, |p| self.intern_name(&p, false));
        let def = Def {
            name: NameId(name_id),
            key: KeyId(key),
            file: FileId(u32::try_from(file).unwrap_or(NONE - 1)),
            line: raw.line,
            lines: raw.lines.max(1),
            parent: parent_id,
            kind: raw.kind,
            vis: raw.vis,
            test: raw.test || test_file,
        };
        let def_id = if let Some(slot) = self.free.pop() {
            self.defs[slot as usize] = def;
            slot
        } else {
            self.defs.push(def);
            u32::try_from(self.defs.len() - 1).unwrap_or(NONE - 1)
        };
        self.key_defs[key as usize].push(def_id);
        self.files[file].defs.push(def_id);
        self.live_defs += 1;
        self.bytes += COST_DEF;
    }

    /// Notes that `rel` was looked at and not read, and why. `true` as [`Self::put_scanned`].
    pub fn put_skipped(&mut self, rel: &str, reason: SkipReason) -> bool {
        let Some(id) = self.ensure_file(rel) else {
            return true;
        };
        let first = self.files[id].state == FileState::Pending;
        self.clear_defs(id);
        self.files[id].state = FileState::Skipped(reason);
        first
    }

    /// Whether `rel` has been scanned or skipped (it is known and not waiting).
    #[must_use]
    pub fn has_scanned(&self, rel: &str) -> bool {
        self.file_ids
            .get(rel)
            .is_some_and(|&id| matches!(self.files[id as usize].state, FileState::Scanned | FileState::Skipped(_)))
    }

    /// Takes a file and its definitions out. `false` when it was not known.
    pub fn remove_file(&mut self, rel: &str) -> bool {
        let Some(&id) = self.file_ids.get(rel) else {
            return false;
        };
        let id = id as usize;
        self.clear_defs(id);
        self.files[id].state = FileState::Removed;
        self.file_ids.remove(rel);
        true
    }

    /// A folder holds a manifest (known before the manifest is read, so the files near it can be
    /// ranked and scoped).
    pub fn note_manifest_dir(&mut self, dir: &str) {
        self.layout.note_manifest_dir(dir);
    }

    /// The nearest folder at or above the folder of `rel` that holds a manifest.
    #[must_use]
    pub fn crate_dir_of(&self, rel: &str) -> Option<&str> {
        self.layout.crate_dir_of(rel)
    }

    /// The folder of the package `name` (`azul-core` and `azul_core` are one).
    #[must_use]
    pub fn package_dir(&self, name: &str) -> Option<&str> {
        self.layout.package_dir(name)
    }

    /// The manifests' folders and packages.
    #[must_use]
    pub const fn layout(&self) -> &Layout {
        &self.layout
    }

    /// What `rel` imports, as written.
    #[must_use]
    pub fn imports_of(&self, rel: &str) -> &[String] {
        self.file_ids.get(rel).map_or(&[][..], |&id| self.files[id as usize].imports.as_slice())
    }

    /// The definition `id` with its names spelled out.
    #[must_use]
    pub fn info(&self, id: DefId) -> DefInfo {
        let def = self.defs[id.0 as usize];
        DefInfo {
            name: self.names.str(def.name.0).to_string(),
            kind: def.kind,
            vis: def.vis,
            test: def.test,
            file: self.files[def.file.0 as usize].path.to_string(),
            line: def.line,
            lines: def.lines,
            parent: def.parent().map(|p| self.names.str(p.0).to_string()),
        }
    }

    /// The definitions whose name has exactly these words (any spelling): `["css", "error"]` finds
    /// `CssError`, `css_error` and `CSS_ERROR`.
    #[must_use]
    pub fn lookup<S: AsRef<str>>(&self, tokens: &[S]) -> Vec<DefInfo> {
        if tokens.is_empty() {
            return Vec::new();
        }
        let lower: Vec<String> = tokens.iter().map(|t| t.as_ref().to_lowercase()).collect();
        self.lookup_key(&key_of(&lower))
    }

    /// The definitions of a key (words joined by a space, lower case).
    #[must_use]
    pub fn lookup_key(&self, key: &str) -> Vec<DefInfo> {
        self.keys
            .get(key)
            .map(|k| self.key_defs[k as usize].iter().map(|&d| self.info(DefId(d))).collect())
            .unwrap_or_default()
    }

    /// The definitions of key `id`.
    #[must_use]
    pub fn defs_of_key(&self, id: KeyId) -> Vec<DefInfo> {
        self.key_defs.get(id.0 as usize).map_or_else(Vec::new, |l| l.iter().map(|&d| self.info(DefId(d))).collect())
    }

    /// How many definitions have key `id`.
    #[must_use]
    pub fn key_len(&self, id: KeyId) -> usize {
        self.key_defs.get(id.0 as usize).map_or(0, Vec::len)
    }

    /// Calls `f` for every key that has at least one definition.
    pub fn for_each_key<'a>(&'a self, mut f: impl FnMut(KeyView<'a>)) {
        for (i, defs) in self.key_defs.iter().enumerate() {
            if defs.is_empty() {
                continue;
            }
            let info = &self.key_info[i];
            let id = u32::try_from(i).unwrap_or(NONE - 1);
            f(KeyView {
                id: KeyId(id),
                key: self.keys.str(id),
                phonetic: &info.phon[..usize::from(info.phon_len)],
                ntokens: usize::from(info.ntokens),
            });
        }
    }

    /// The definitions of file `rel`, in line order.
    #[must_use]
    pub fn defs_of_file(&self, rel: &str) -> Vec<DefInfo> {
        let Some(&id) = self.file_ids.get(rel) else {
            return Vec::new();
        };
        let mut out: Vec<DefInfo> = self.files[id as usize].defs.iter().map(|&d| self.info(DefId(d))).collect();
        out.sort_by(|a, b| a.line.cmp(&b.line).then_with(|| a.name.cmp(&b.name)));
        out
    }

    /// The paths of the files the index knows.
    pub fn for_each_file<'a>(&'a self, mut f: impl FnMut(&'a str)) {
        for file in &self.files {
            if file.state != FileState::Removed {
                f(&file.path);
            }
        }
    }

    /// In how many distinct names the word `token` occurs.
    #[must_use]
    pub fn token_count(&self, token: &str) -> u32 {
        self.tokens.get(token).copied().unwrap_or(0)
    }

    /// Whether any name has the word `token`.
    #[must_use]
    pub fn has_token(&self, token: &str) -> bool {
        self.tokens.contains_key(token)
    }

    /// What was left out and why.
    #[must_use]
    pub fn notes(&self) -> &[Note] {
        &self.notes
    }

    /// The counts.
    #[must_use]
    pub fn stats(&self) -> IndexStats {
        let mut stats = IndexStats {
            defs: self.live_defs,
            names: self.names.strings.len(),
            keys: self.keys.strings.len(),
            tokens: self.tokens.len(),
            lines: self.lines_total,
            bytes_read: self.bytes_read,
            est_bytes: self.bytes,
            full: self.full,
            ..IndexStats::default()
        };
        for file in &self.files {
            match file.state {
                FileState::Removed => continue,
                FileState::Pending => stats.pending += 1,
                FileState::Scanned => stats.scanned += 1,
                FileState::Skipped(reason) => stats.skipped[reason.index()] += 1,
            }
            stats.files += 1;
        }
        stats
    }
}

#[cfg(test)]
#[path = "symbols_test.rs"]
mod symbols_test;
