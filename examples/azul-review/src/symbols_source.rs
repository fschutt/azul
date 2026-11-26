//! Where the files of a workspace come from, as far as the symbol index is concerned: the
//! [`FileSource`] seam (the disk for the app, memory for the tests), what a file is by its name
//! ([`classify`]: a language, a manifest, or nothing the index reads), which paths are tests
//! ([`is_test_path`]), which files are not worth reading ([`inspect`]: binary, generated,
//! minified), and the walk's rules ([`Exclusions`]).
//!
//! A path is workspace-relative with `/` as the separator, `""` is the root, and a path that leaves
//! the workspace (`..`, an absolute one, a backslash) is refused by both sources
//! ([`ReadError::Outside`]). A symlink is never followed: it lists as [`EntryKind::Other`], so a
//! link cannot loop the walk or read what lies outside the workspace.
//!
//! The AzReview agent owns the app's own folder walk; the index asks for the folders it needs through
//! this trait, so a different walker is a different [`FileSource`].

use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::PathBuf,
    sync::{Mutex, PoisonError},
    time::UNIX_EPOCH,
};

/// The largest file that is read, bytes: a longer one is skipped as [`SkipReason::TooLarge`].
pub const MAX_FILE_BYTES: usize = 2 * 1024 * 1024;

/// How much of a file [`inspect`] looks at for a NUL byte.
const BINARY_HEAD: usize = 8192;
/// How much of a file's top is read for a generated-file marker.
const GENERATED_HEAD: usize = 1024;
/// How much of a file is read for an endless line.
const MINIFIED_HEAD: usize = 64 * 1024;
/// A line longer than this, bytes, is not hand-written code.
const MINIFIED_LINE: usize = 4000;

/// The languages whose definitions are read.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Lang {
    Rust,
    C,
    Cpp,
    Python,
    JavaScript,
    TypeScript,
    Go,
    Java,
    Kotlin,
    CSharp,
    Swift,
    Shell,
}

/// The manifests whose dependencies are read.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ManifestKind {
    Cargo,
    PackageJson,
    GoMod,
    Requirements,
    Pyproject,
    CMake,
}

/// What the index does with a file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileClass {
    /// Its definitions are read.
    Code(Lang),
    /// Its package and dependency names are read.
    Manifest(ManifestKind),
    /// Nothing is read: counted as an unknown type.
    Other,
}

/// Why a file was not read.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SkipReason {
    /// Its extension is not one of a language the index reads.
    UnknownType,
    /// It is over the cap of a file read ([`MAX_FILE_BYTES`] unless changed).
    TooLarge,
    /// It holds a NUL byte: not text.
    Binary,
    /// Its top says a tool wrote it ("DO NOT EDIT", "@generated").
    Generated,
    /// It has a line no one wrote by hand.
    Minified,
    /// It could not be opened or read.
    Unreadable,
}

impl SkipReason {
    /// Every reason, in the order the statistics list them.
    pub const ALL: [Self; 6] = [Self::UnknownType, Self::TooLarge, Self::Binary, Self::Generated, Self::Minified, Self::Unreadable];

    /// The position in [`Self::ALL`].
    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            Self::UnknownType => 0,
            Self::TooLarge => 1,
            Self::Binary => 2,
            Self::Generated => 3,
            Self::Minified => 4,
            Self::Unreadable => 5,
        }
    }

    /// The localization key of the reason's name.
    #[must_use]
    pub const fn l10n_key(self) -> &'static str {
        match self {
            Self::UnknownType => "azreview-symbols-skip-unknown-type",
            Self::TooLarge => "azreview-symbols-skip-too-large",
            Self::Binary => "azreview-symbols-skip-binary",
            Self::Generated => "azreview-symbols-skip-generated",
            Self::Minified => "azreview-symbols-skip-minified",
            Self::Unreadable => "azreview-symbols-skip-unreadable",
        }
    }
}

/// What kind of file `rel` is, by its name: a manifest by the whole name, code by the extension
/// (case does not matter; a hidden file keeps its extension: `.hidden.rs` is Rust, `.gitignore`
/// has none).
#[must_use]
pub fn classify(rel: &str) -> FileClass {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    match name {
        "Cargo.toml" => return FileClass::Manifest(ManifestKind::Cargo),
        "package.json" => return FileClass::Manifest(ManifestKind::PackageJson),
        "go.mod" => return FileClass::Manifest(ManifestKind::GoMod),
        "pyproject.toml" => return FileClass::Manifest(ManifestKind::Pyproject),
        "CMakeLists.txt" => return FileClass::Manifest(ManifestKind::CMake),
        _ => {}
    }
    if name.starts_with("requirements") && name.ends_with(".txt") {
        return FileClass::Manifest(ManifestKind::Requirements);
    }
    let Some((stem, ext)) = name.rsplit_once('.') else {
        return FileClass::Other;
    };
    if stem.is_empty() {
        return FileClass::Other;
    }
    let lang = match ext.to_ascii_lowercase().as_str() {
        "rs" => Lang::Rust,
        "c" | "h" => Lang::C,
        "cc" | "cpp" | "cxx" | "c++" | "hpp" | "hh" | "hxx" | "h++" | "ipp" | "inl" => Lang::Cpp,
        "py" | "pyi" | "pyw" => Lang::Python,
        "js" | "jsx" | "mjs" | "cjs" => Lang::JavaScript,
        "ts" | "tsx" | "mts" | "cts" => Lang::TypeScript,
        "go" => Lang::Go,
        "java" => Lang::Java,
        "kt" | "kts" => Lang::Kotlin,
        "cs" => Lang::CSharp,
        "swift" => Lang::Swift,
        "sh" | "bash" | "zsh" => Lang::Shell,
        _ => return FileClass::Other,
    };
    FileClass::Code(lang)
}

/// Whether `rel` is a file of tests: in a folder named `test`, `tests`, `__tests__`, `spec` or
/// `specs`, or named `test.*`, `tests.*`, `*_test.*`, `*_tests.*`, `test_*.*`, `*.test.*`,
/// `*.spec.*`, `*Test.*` or `*Tests.*` (a name that only has `test` inside, `latest.rs`, is none).
#[must_use]
pub fn is_test_path(rel: &str) -> bool {
    let comps: Vec<&str> = rel.split('/').collect();
    let (dirs, file) = comps.split_at(comps.len().saturating_sub(1));
    let file = file.first().copied().unwrap_or("");
    if dirs.iter().any(|d| matches!(*d, "test" | "tests" | "__tests__" | "spec" | "specs")) {
        return true;
    }
    if file.contains(".test.") || file.contains(".spec.") {
        return true;
    }
    let stem = file.rsplit_once('.').map_or(file, |(s, _)| s);
    stem == "test"
        || stem == "tests"
        || stem.ends_with("_test")
        || stem.ends_with("_tests")
        || stem.starts_with("test_")
        || (stem.len() > 4 && (stem.ends_with("Test") || stem.ends_with("Tests")))
}

/// What the top of a generated file says (lower case).
static GENERATED_MARKERS: [&str; 9] = [
    "@generated",
    "code generated",
    "auto-generated",
    "autogenerated",
    "automatically generated",
    "generated by",
    "generated file",
    "this file is generated",
    "do not edit",
];

/// Whether the bytes of a file are not worth reading, and why: a NUL byte in the first 8 KiB
/// (binary), a marker of a tool in the first KiB (generated), a line over 4,000 bytes in the first
/// 64 KiB (minified). Text that is not UTF-8 is fine: the scanners read bytes.
#[must_use]
pub fn inspect(bytes: &[u8]) -> Option<SkipReason> {
    if bytes[..bytes.len().min(BINARY_HEAD)].contains(&0) {
        return Some(SkipReason::Binary);
    }
    let top = String::from_utf8_lossy(&bytes[..bytes.len().min(GENERATED_HEAD)]).to_ascii_lowercase();
    if GENERATED_MARKERS.iter().any(|m| top.contains(m)) {
        return Some(SkipReason::Generated);
    }
    let mut run = 0usize;
    for &b in &bytes[..bytes.len().min(MINIFIED_HEAD)] {
        if b == b'\n' {
            run = 0;
        } else {
            run += 1;
            if run > MINIFIED_LINE {
                return Some(SkipReason::Minified);
            }
        }
    }
    None
}

/// Whether a file name is a build manifest: a folder that holds one is a project, and a `build` or
/// `dist` folder beside it is its output.
#[must_use]
pub fn is_manifest_name(name: &str) -> bool {
    matches!(
        name,
        "Cargo.toml"
            | "package.json"
            | "go.mod"
            | "pom.xml"
            | "build.gradle"
            | "build.gradle.kts"
            | "CMakeLists.txt"
            | "pyproject.toml"
            | "setup.py"
            | "Makefile"
            | "meson.build"
            | "Package.swift"
            | "Gemfile"
            | "composer.json"
    ) || name.ends_with(".csproj")
        || name.ends_with(".sln")
}

/// The folders that are skipped by name wherever they are.
static SKIPPED_DIRS: [&str; 20] = [
    "node_modules",
    "target",
    "__pycache__",
    "venv",
    "vendor",
    "Pods",
    "DerivedData",
    "bower_components",
    "site-packages",
    "coverage",
    ".git",
    ".hg",
    ".svn",
    ".idea",
    ".vscode",
    ".gradle",
    ".next",
    ".nuxt",
    ".tox",
    ".cache",
];

/// The folders that are build output only beside a manifest (`build` in a folder that has a
/// `Cargo.toml` is output; `build` in a folder of sources is a module).
static OUTPUT_DIRS: [&str; 7] = ["build", "dist", "out", "bin", "obj", "release", "debug"];

/// What the walk leaves out and how much it reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Exclusions {
    /// Folder names skipped wherever they are.
    pub dirs: Vec<String>,
    /// Folder names skipped only beside a manifest.
    pub output_dirs: Vec<String>,
    /// A name that starts with a dot (a folder or a file) is skipped.
    pub skip_hidden: bool,
    /// The deepest folder that is walked (the root is 0).
    pub max_depth: usize,
    /// The largest file read, bytes.
    pub max_file_bytes: usize,
}

impl Default for Exclusions {
    fn default() -> Self {
        Self {
            dirs: SKIPPED_DIRS.iter().map(|s| (*s).to_string()).collect(),
            output_dirs: OUTPUT_DIRS.iter().map(|s| (*s).to_string()).collect(),
            skip_hidden: true,
            max_depth: 24,
            max_file_bytes: MAX_FILE_BYTES,
        }
    }
}

impl Exclusions {
    /// Whether the walk leaves the folder `name` alone. `beside_manifest`: the folder it is in holds
    /// a manifest.
    #[must_use]
    pub fn skips_dir(&self, name: &str, beside_manifest: bool) -> bool {
        (self.skip_hidden && name.starts_with('.'))
            || self.dirs.iter().any(|d| d == name)
            || (beside_manifest && self.output_dirs.iter().any(|d| d == name))
    }

    /// Whether the walk leaves the file `name` alone.
    #[must_use]
    pub fn skips_file(&self, name: &str) -> bool {
        self.skip_hidden && name.starts_with('.')
    }
}

/// What an entry of a folder is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Dir,
    /// A symlink, a device, a socket: never followed or read.
    Other,
}

/// One entry of a folder listing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirEntry {
    pub name: String,
    pub kind: EntryKind,
    /// Bytes (0 for what is no file).
    pub size: u64,
    /// Milliseconds since the Unix epoch (0 where unknown).
    pub mtime_ms: u64,
}

/// What a file is: its size and its time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileMeta {
    pub size: u64,
    pub mtime_ms: u64,
}

/// Why a read or a listing failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReadError {
    NotFound,
    /// The file has this many bytes, more than the cap of the call.
    TooLarge(u64),
    /// The path leaves the workspace (or is no plain relative path).
    Outside,
    Io(String),
}

/// The folders and files of one workspace. Implementations are shared between the scanning threads.
pub trait FileSource: Send + Sync {
    /// The entries of the folder `dir`, sorted by name (`""` is the root).
    ///
    /// # Errors
    ///
    /// The folder is not there, not readable, or outside the workspace.
    fn list(&self, dir: &str) -> Result<Vec<DirEntry>, ReadError>;

    /// The bytes of the file `rel`, at most `max_bytes` of them: a longer file is an error and is
    /// not read.
    ///
    /// # Errors
    ///
    /// The file is not there, not a plain file, too large, not readable, or outside the workspace.
    fn read(&self, rel: &str, max_bytes: usize) -> Result<Vec<u8>, ReadError>;

    /// The size and the time of the file `rel` (`None` for a folder, a link, a missing path).
    fn stat(&self, rel: &str) -> Option<FileMeta>;
}

/// A workspace path that cannot leave the workspace: no `..`, no `.`, no empty part, no backslash
/// or NUL; one trailing `/` is dropped.
fn clean(rel: &str) -> Result<&str, ReadError> {
    if rel.is_empty() {
        return Ok(rel);
    }
    if rel.starts_with('/') || rel.contains('\\') || rel.contains('\0') {
        return Err(ReadError::Outside);
    }
    let trimmed = rel.strip_suffix('/').unwrap_or(rel);
    if trimmed.split('/').any(|part| part.is_empty() || part == "." || part == "..") {
        return Err(ReadError::Outside);
    }
    Ok(trimmed)
}

/// Files in memory: the tests' workspace, and a workspace that is not on a disk.
#[derive(Default)]
pub struct MemSource {
    files: BTreeMap<String, (Vec<u8>, u64)>,
    reads: Mutex<Vec<String>>,
}

impl MemSource {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Puts a file (replacing one of the same path); its time is 1.
    pub fn put(&mut self, rel: &str, bytes: Vec<u8>) {
        self.put_at(rel, bytes, 1);
    }

    /// Puts a file with a time.
    pub fn put_at(&mut self, rel: &str, bytes: Vec<u8>, mtime_ms: u64) {
        self.files.insert(rel.to_string(), (bytes, mtime_ms));
    }

    /// The paths asked of [`FileSource::read`], in the order they were asked (a path that was not
    /// there too): what the tests of the scan order read.
    #[must_use]
    pub fn reads(&self) -> Vec<String> {
        self.reads.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }
}

impl FileSource for MemSource {
    fn list(&self, dir: &str) -> Result<Vec<DirEntry>, ReadError> {
        let dir = clean(dir)?;
        let prefix = if dir.is_empty() { String::new() } else { format!("{dir}/") };
        let mut found: BTreeMap<String, DirEntry> = BTreeMap::new();
        for (path, (bytes, mtime)) in &self.files {
            let Some(rest) = path.strip_prefix(&prefix) else {
                continue;
            };
            match rest.split_once('/') {
                Some((child, _)) => {
                    found.entry(child.to_string()).or_insert_with(|| DirEntry {
                        name: child.to_string(),
                        kind: EntryKind::Dir,
                        size: 0,
                        mtime_ms: 0,
                    });
                }
                None => {
                    found.insert(
                        rest.to_string(),
                        DirEntry {
                            name: rest.to_string(),
                            kind: EntryKind::File,
                            size: bytes.len() as u64,
                            mtime_ms: *mtime,
                        },
                    );
                }
            }
        }
        Ok(found.into_values().collect())
    }

    fn read(&self, rel: &str, max_bytes: usize) -> Result<Vec<u8>, ReadError> {
        let rel = clean(rel)?;
        self.reads.lock().unwrap_or_else(PoisonError::into_inner).push(rel.to_string());
        let (bytes, _) = self.files.get(rel).ok_or(ReadError::NotFound)?;
        if bytes.len() > max_bytes {
            return Err(ReadError::TooLarge(bytes.len() as u64));
        }
        Ok(bytes.clone())
    }

    fn stat(&self, rel: &str) -> Option<FileMeta> {
        let rel = clean(rel).ok()?;
        self.files.get(rel).map(|(bytes, mtime)| FileMeta {
            size: bytes.len() as u64,
            mtime_ms: *mtime,
        })
    }
}

/// The files on a disk below one root.
#[derive(Clone, Debug)]
pub struct DiskSource {
    root: PathBuf,
}

impl DiskSource {
    #[must_use]
    pub const fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// The root folder.
    #[must_use]
    pub fn root(&self) -> &std::path::Path {
        &self.root
    }

    fn abs(&self, rel: &str) -> PathBuf {
        if rel.is_empty() {
            self.root.clone()
        } else {
            self.root.join(rel)
        }
    }
}

fn io_error(e: &std::io::Error) -> ReadError {
    if e.kind() == std::io::ErrorKind::NotFound {
        ReadError::NotFound
    } else {
        ReadError::Io(e.to_string())
    }
}

fn mtime_ms(meta: &fs::Metadata) -> u64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

impl FileSource for DiskSource {
    fn list(&self, dir: &str) -> Result<Vec<DirEntry>, ReadError> {
        let dir = clean(dir)?;
        let reader = fs::read_dir(self.abs(dir)).map_err(|e| io_error(&e))?;
        let mut out = Vec::new();
        for entry in reader.flatten() {
            let Ok(name) = entry.file_name().into_string() else {
                continue;
            };
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            let (kind, size, mtime) = if file_type.is_dir() {
                (EntryKind::Dir, 0, 0)
            } else if file_type.is_file() {
                let (size, mtime) = entry.metadata().map_or((0, 0), |m| (m.len(), mtime_ms(&m)));
                (EntryKind::File, size, mtime)
            } else {
                (EntryKind::Other, 0, 0)
            };
            out.push(DirEntry { name, kind, size, mtime_ms: mtime });
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(out)
    }

    fn read(&self, rel: &str, max_bytes: usize) -> Result<Vec<u8>, ReadError> {
        let rel = clean(rel)?;
        if rel.is_empty() {
            return Err(ReadError::Io("the root is no file".to_string()));
        }
        let path = self.abs(rel);
        let meta = fs::symlink_metadata(&path).map_err(|e| io_error(&e))?;
        if !meta.is_file() {
            return Err(ReadError::Io("not a plain file".to_string()));
        }
        if meta.len() > max_bytes as u64 {
            return Err(ReadError::TooLarge(meta.len()));
        }
        let file = fs::File::open(&path).map_err(|e| io_error(&e))?;
        // The length was checked against the cap above; a file that grew since is cut at the cap.
        let mut bytes = Vec::with_capacity(usize::try_from(meta.len()).unwrap_or(0).min(max_bytes));
        file.take(max_bytes as u64 + 1).read_to_end(&mut bytes).map_err(|e| io_error(&e))?;
        if bytes.len() > max_bytes {
            return Err(ReadError::TooLarge(bytes.len() as u64));
        }
        Ok(bytes)
    }

    fn stat(&self, rel: &str) -> Option<FileMeta> {
        let rel = clean(rel).ok()?;
        if rel.is_empty() {
            return None;
        }
        let meta = fs::symlink_metadata(self.abs(rel)).ok()?;
        meta.is_file().then(|| FileMeta {
            size: meta.len(),
            mtime_ms: mtime_ms(&meta),
        })
    }
}

#[cfg(test)]
#[path = "symbols_source_test.rs"]
mod symbols_source_test;
