//! azul-search-index: the full-text index of a drive, for the Azlin apps' search.
//!
//! AzDrive's search box finds names by walking the drive (azul-search) and contents by reading
//! every file - fast for a folder, slow for a home folder, and blind to the text of an office
//! document, a mail or a PDF. A drive's index answers the contents part at once: one tantivy
//! index per drive (`DriveIndex`, in a folder of the app's cache), the text of every file in
//! it ([`extract`]: plain files, Word / Excel / PowerPoint / OpenDocument / EPUB documents,
//! mail, PDFs through the app's reader), kept up to date by [`DriveIndex::update`]: the same
//! walk as the search box's (azul-search's `list_files`, its filters), and only the files whose
//! size or date changed are read again (the list of what the index read sits beside it). A query
//! ([`DriveIndex::query`]) is words and word beginnings - the last word may be half typed
//! ("quarterly rep" finds "quarterly report") -, below a folder of the drive, best first; what
//! the index has not read as it is now ([`DriveIndex::unread`]: changed since its update) a
//! search reads itself, and [`document_text`] / [`file_text`] give a file's text for the line a
//! result shows. A source other than a folder (a cloud drive's listing, an encrypted drive's
//! index, read through the app's reader) is indexed by [`DriveIndex::update_files`].
//!
//! Blocking and plain Rust (no azul types); an update stops within a file when it is cancelled
//! and keeps what it committed. One update at a time per index (tantivy's writer lock): a
//! second one is refused with a sentence.

mod extract;
mod state;
#[cfg(test)]
mod tests;

use std::{
    collections::HashSet,
    fmt,
    fs::{self, File},
    io::{self, Read},
    ops::Bound,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use azul_search::{list_files, FileEntry, Filters};
use tantivy::{
    collector::TopDocs,
    directory::MmapDirectory,
    query::{BooleanQuery, Occur, PhrasePrefixQuery, Query, RangeQuery},
    schema::{Field, Schema, Value, STORED, STRING, TEXT},
    tokenizer::TokenStream,
    Index, IndexReader, IndexWriter, ReloadPolicy, TantivyDocument, Term,
};

pub use extract::{extract, kind_of, ExtractFn, Extractors, Kind, MAX_TEXT_BYTES};

/// The index's format: a folder holding another (an older build's) is emptied and built again.
const FORMAT: &str = "azul-search-index 1";
/// The file in the index's folder that names its format.
const FORMAT_FILE: &str = "azul-search-index.format";
/// The list of the files the index read ([`state`]).
const STATE_FILE: &str = "azul-search-index.files";
/// The memory the index writer may hold before it writes a segment.
const WRITER_MEMORY: usize = 64 * 1024 * 1024;
/// An update commits after this many files read...
const COMMIT_EVERY: usize = 500;
/// ... or after this long, whichever comes first (what it read is kept if it is stopped).
const COMMIT_AFTER: Duration = Duration::from_secs(20);
/// An update says how far it got this often.
const PROGRESS_EVERY: Duration = Duration::from_millis(250);
/// A document (office, mail, PDF) larger than this is not read (bytes).
pub const MAX_DOCUMENT_BYTES: u64 = 64 * 1024 * 1024;
/// The words a half typed last word of a query stands for at most.
const PREFIX_EXPANSIONS: u32 = 1000;

/// Why an index could not be opened, updated or asked - a sentence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexError {
    pub message: String,
}

impl IndexError {
    fn new(message: impl Into<String>) -> IndexError {
        IndexError {
            message: message.into(),
        }
    }
}

impl fmt::Display for IndexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for IndexError {}

impl From<tantivy::TantivyError> for IndexError {
    fn from(e: tantivy::TantivyError) -> IndexError {
        match e {
            tantivy::TantivyError::LockFailure(..) => {
                IndexError::new("another window is updating this index")
            }
            e => IndexError::new(format!("the index: {e}")),
        }
    }
}

impl From<io::Error> for IndexError {
    fn from(e: io::Error) -> IndexError {
        IndexError::new(format!("the index's folder: {e}"))
    }
}

impl From<azul_search::PatternError> for IndexError {
    fn from(e: azul_search::PatternError) -> IndexError {
        IndexError::new(e.message)
    }
}

/// What an index holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct IndexStatus {
    /// Files the index has read (with text or without).
    pub files: usize,
    /// Files whose text it holds.
    pub documents: u64,
    /// When an update last went over the drive (seconds since 1970); `None`: never.
    pub updated: Option<u64>,
}

/// How far an update got.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct UpdateProgress {
    /// Files the walk found.
    pub listed: usize,
    /// Of them, new or changed since the last update: to be read.
    pub to_read: usize,
    /// Read so far.
    pub read: usize,
}

/// What an update did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct UpdateSummary {
    /// Files the walk found.
    pub listed: usize,
    /// Files read and put into the index (new or changed).
    pub indexed: usize,
    /// New or changed files that hold no text (or could not be read).
    pub without_text: usize,
    /// Files the index held that are gone, taken out.
    pub removed: usize,
    /// Files the index held as they are: not read.
    pub unchanged: usize,
    /// Files the source could not give now (offline): not recorded, asked for again by the next
    /// update.
    pub failed: usize,
    /// The update was cancelled; what it committed stays.
    pub cancelled: bool,
}

/// How an update gets a file's bytes from its source: `(file, its kind, at most this many
/// bytes)` -> the bytes; `Ok(None)` when the source has none to give (too large, no copy on
/// this computer): recorded as read without text; `Err` when it cannot now (offline): not
/// recorded, asked for again by the next update.
pub type ReadFn<'a> = dyn FnMut(&FileEntry, Kind, u64) -> Result<Option<Vec<u8>>, String> + 'a;

/// One drive's index: a tantivy index in a folder of its own.
pub struct DriveIndex {
    dir: PathBuf,
    index: Index,
    reader: IndexReader,
    /// A file's path below the drive's folder (`/`-separated): its key.
    path: Field,
    /// Its text.
    body: Field,
}

/// The index's fields: the path (one term, stored), the text (words with their positions, for
/// phrases).
fn schema() -> (Schema, Field, Field) {
    let mut builder = Schema::builder();
    let path = builder.add_text_field("path", STRING | STORED);
    let body = builder.add_text_field("body", TEXT);
    (builder.build(), path, body)
}

/// Every file and folder in `dir`, removed (the folder stays).
fn clear(dir: &Path) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            fs::remove_dir_all(&path)?;
        } else {
            fs::remove_file(&path)?;
        }
    }
    Ok(())
}

/// The index in `dir`, made when there is none.
fn open_index(dir: &Path, schema: Schema) -> Result<Index, IndexError> {
    let directory = MmapDirectory::open(dir).map_err(|e| IndexError::new(e.to_string()))?;
    Ok(Index::open_or_create(directory, schema)?)
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// The file of `path` (`/`-separated) below `root`.
fn full_path(root: &Path, path: &str) -> PathBuf {
    let mut full = root.to_path_buf();
    for segment in path.split('/').filter(|s| !s.is_empty()) {
        full.push(segment);
    }
    full
}

/// What an index says of one file: an app's "indexed" / "not indexable" overlays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileIndexing {
    /// Read as it is now: its words are in the index (a file without any was read too).
    Indexed,
    /// Never read: its kind holds no text, or it is a document over [`MAX_DOCUMENT_BYTES`].
    NotIndexable,
    /// Not read as it is now (new, or changed since the last update): the next update reads it.
    Unread,
}

/// Whether a file of `name` and `size` bytes is read for its text at all (a text is read in
/// part however long it is; a document only up to [`MAX_DOCUMENT_BYTES`]).
#[must_use]
pub fn indexable(name: &str, size: u64) -> bool {
    match kind_of(name) {
        None => false,
        Some(Kind::Text) => true,
        Some(_) => size <= MAX_DOCUMENT_BYTES,
    }
}

/// The files an index has read as it last committed them, to ask one file at a time without
/// reading the list again ([`indexed_files`]).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IndexedFiles {
    files: std::collections::HashMap<String, (u64, Option<u64>)>,
}

impl IndexedFiles {
    /// What the index says of `file` (its path below the drive's folder, its size and date now).
    #[must_use]
    pub fn indexing(&self, file: &FileEntry) -> FileIndexing {
        let name = file.path.rsplit('/').next().unwrap_or(&file.path);
        if !indexable(name, file.size) {
            return FileIndexing::NotIndexable;
        }
        if self.files.get(&file.path) == Some(&(file.size, file.modified)) {
            FileIndexing::Indexed
        } else {
            FileIndexing::Unread
        }
    }
}

/// The files the index in `dir` has read: the list it keeps beside it, read once - the index
/// itself is not opened (an empty answer for a folder without an index).
#[must_use]
pub fn indexed_files(dir: &Path) -> IndexedFiles {
    IndexedFiles {
        files: state::read(&dir.join(STATE_FILE)).files,
    }
}

/// The files of `files` that `state` does not hold as they are: new, or of another size or date.
fn unread_in<'a>(state: &state::State, files: &'a [FileEntry]) -> Vec<&'a FileEntry> {
    files
        .iter()
        .filter(|f| state.files.get(&f.path) != Some(&(f.size, f.modified)))
        .collect()
}

/// A file's text as the index reads it (`path` below `root`, `/`-separated): the bytes its kind
/// needs through [`extract`] - for the line a result of a document shows. `None` for a kind
/// without text (a picture), a file that cannot be read or is too large, one without any text.
#[must_use]
pub fn document_text(root: &Path, path: &str, extractors: &Extractors) -> Option<String> {
    file_text(&full_path(root, path), extractors)
}

/// The text of the file `file` on this computer (a drive's local copy too, named apart from
/// its key), as the index reads it: its kind by its own name.
#[must_use]
pub fn file_text(file: &Path, extractors: &Extractors) -> Option<String> {
    let name = file.file_name()?.to_str()?;
    let kind = kind_of(name)?;
    let size = fs::metadata(file).ok()?.len();
    if kind != Kind::Text && size > MAX_DOCUMENT_BYTES {
        return None;
    }
    let bytes = read_file(file, read_limit(kind))?;
    extract(name, &bytes, extractors)
}

/// The bytes a file of `kind` is read for: a plain file's first [`MAX_TEXT_BYTES`], a document
/// whole (one larger than [`MAX_DOCUMENT_BYTES`] is not read).
fn read_limit(kind: Kind) -> u64 {
    match kind {
        Kind::Text => MAX_TEXT_BYTES as u64,
        _ => MAX_DOCUMENT_BYTES,
    }
}

/// The first `limit` bytes of the file `path`; `None` when it cannot be read.
fn read_file(path: &Path, limit: u64) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open(path)
        .ok()?
        .take(limit)
        .read_to_end(&mut bytes)
        .ok()?;
    Some(bytes)
}

impl DriveIndex {
    /// The index in `dir` (made, and the folder too, when there is none; one of another format
    /// is thrown away and made again).
    ///
    /// # Errors
    ///
    /// The folder cannot be made or read, the index cannot be opened.
    pub fn open(dir: &Path) -> Result<DriveIndex, IndexError> {
        fs::create_dir_all(dir)?;
        let format = dir.join(FORMAT_FILE);
        if fs::read_to_string(&format).ok().as_deref() != Some(FORMAT) {
            clear(dir)?;
            fs::write(&format, FORMAT)?;
        }
        let (schema, path, body) = schema();
        let index = match open_index(dir, schema.clone()) {
            Ok(index) => index,
            Err(_) => {
                // Broken, or of another schema: built again from nothing.
                clear(dir)?;
                fs::write(&format, FORMAT)?;
                open_index(dir, schema)?
            }
        };
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::Manual)
            .try_into()?;
        Ok(DriveIndex {
            dir: dir.to_path_buf(),
            index,
            reader,
            path,
            body,
        })
    }

    /// What the index holds.
    #[must_use]
    pub fn status(&self) -> IndexStatus {
        let state = state::read(&self.dir.join(STATE_FILE));
        IndexStatus {
            files: state.files.len(),
            documents: self.reader.searcher().num_docs(),
            updated: state.updated,
        }
    }

    /// Brings the index up to the drive's folder `root` as `filters` walk it: the files that
    /// are new or whose size or date changed are read again (their text through `extractors`
    /// for the kinds the app reads), the gone ones taken out; the rest is not read. Commits every
    /// [`COMMIT_EVERY`] files or [`COMMIT_AFTER`]; a cancel stops it within a file and keeps
    /// what was committed. `on_progress` hears how far it got every quarter second.
    ///
    /// # Errors
    ///
    /// The walk's filters do not compile, the index cannot be written (another update holds
    /// it), its list cannot be saved.
    pub fn update(
        &self,
        root: &Path,
        filters: &Filters,
        extractors: &Extractors,
        cancel: &AtomicBool,
        on_progress: &mut dyn FnMut(UpdateProgress),
    ) -> Result<UpdateSummary, IndexError> {
        let mut files: Vec<FileEntry> = Vec::new();
        list_files(root, filters, cancel, &mut |file| files.push(file))?;
        if cancel.load(Ordering::SeqCst) {
            return Ok(UpdateSummary {
                listed: files.len(),
                cancelled: true,
                ..UpdateSummary::default()
            });
        }
        // A file that cannot be read now is recorded without text, as the walk found it.
        let mut read = |file: &FileEntry, _kind: Kind, limit: u64| -> Result<_, String> {
            Ok(read_file(&full_path(root, &file.path), limit))
        };
        self.update_files(&files, &mut read, extractors, cancel, on_progress)
    }

    /// Brings the index up to `files` - every file a source has (a folder's walk, a cloud
    /// drive's listing, an encrypted drive's index), its path the index's key - reading the new
    /// and changed ones through `read` (the bytes their kind needs; a document larger than
    /// [`MAX_DOCUMENT_BYTES`] is not asked for), taking the gone ones out; the rest is not read.
    /// A file `read` cannot give now is not recorded: the next update asks again. Commits every
    /// [`COMMIT_EVERY`] files or [`COMMIT_AFTER`]; a cancel stops it between files and keeps
    /// what was committed. `on_progress` hears how far it got every quarter second.
    ///
    /// # Errors
    ///
    /// The index cannot be written (another update holds it), its list cannot be saved.
    pub fn update_files(
        &self,
        files: &[FileEntry],
        read: &mut ReadFn<'_>,
        extractors: &Extractors,
        cancel: &AtomicBool,
        on_progress: &mut dyn FnMut(UpdateProgress),
    ) -> Result<UpdateSummary, IndexError> {
        let state_file = self.dir.join(STATE_FILE);
        let mut state = state::read(&state_file);
        let mut summary = UpdateSummary {
            listed: files.len(),
            ..UpdateSummary::default()
        };
        let present: HashSet<&str> = files.iter().map(|f| f.path.as_str()).collect();
        let removed: Vec<String> = state
            .files
            .keys()
            .filter(|path| !present.contains(String::as_str(path)))
            .cloned()
            .collect();
        let mut changed = unread_in(&state, files);
        changed.sort_by(|a, b| a.path.cmp(&b.path));
        summary.unchanged = files.len() - changed.len();
        summary.removed = removed.len();
        let mut progress = UpdateProgress {
            listed: files.len(),
            to_read: changed.len(),
            read: 0,
        };
        on_progress(progress);
        if !removed.is_empty() || !changed.is_empty() {
            let mut writer: IndexWriter = self.index.writer_with_num_threads(1, WRITER_MEMORY)?;
            for path in &removed {
                writer.delete_term(Term::from_field_text(self.path, path));
                state.files.remove(path);
            }
            let mut since_commit = 0;
            let mut last_commit = Instant::now();
            let mut last_progress = Instant::now();
            for file in changed {
                if cancel.load(Ordering::Relaxed) {
                    summary.cancelled = true;
                    break;
                }
                let name = file.path.rsplit('/').next().unwrap_or("");
                let text = match kind_of(name) {
                    None => Ok(None),
                    Some(kind) if kind != Kind::Text && file.size > MAX_DOCUMENT_BYTES => Ok(None),
                    Some(kind) => read(file, kind, read_limit(kind))
                        .map(|bytes| bytes.and_then(|bytes| extract(name, &bytes, extractors))),
                };
                progress.read += 1;
                match text {
                    Ok(text) => {
                        writer.delete_term(Term::from_field_text(self.path, &file.path));
                        match text {
                            Some(text) => {
                                let mut document = TantivyDocument::default();
                                document.add_text(self.path, &file.path);
                                document.add_text(self.body, &text);
                                writer.add_document(document)?;
                                summary.indexed += 1;
                            }
                            None => summary.without_text += 1,
                        }
                        state
                            .files
                            .insert(file.path.clone(), (file.size, file.modified));
                        since_commit += 1;
                    }
                    // Not recorded: the next update asks for it again (its old text stays).
                    Err(_) => summary.failed += 1,
                }
                if since_commit >= COMMIT_EVERY || last_commit.elapsed() >= COMMIT_AFTER {
                    writer.commit()?;
                    state::write(&state_file, &state)?;
                    since_commit = 0;
                    last_commit = Instant::now();
                }
                if last_progress.elapsed() >= PROGRESS_EVERY {
                    last_progress = Instant::now();
                    on_progress(progress);
                }
            }
            writer.commit()?;
        }
        if !summary.cancelled {
            state.updated = Some(now_secs());
        }
        state::write(&state_file, &state)?;
        self.reader.reload()?;
        on_progress(progress);
        Ok(summary)
    }

    /// The files of `files` (a walk of the drive's folder, [`azul_search::list_files`]) the
    /// index has not read as they are now: new ones, and the ones whose size or date changed
    /// since its update - what a search still reads itself.
    #[must_use]
    pub fn unread<'a>(&self, files: &'a [FileEntry]) -> Vec<&'a FileEntry> {
        unread_in(&state::read(&self.dir.join(STATE_FILE)), files)
    }

    /// The files below `under` (a folder's key, `docs/`; `""`: the whole drive) whose text holds
    /// the words of `text` in a row - the last one may be the beginning of a word -, best first,
    /// `limit` at most. Words are what the index's tokenizer makes of the text (letters and
    /// digits, without case); a text without any finds nothing.
    ///
    /// # Errors
    ///
    /// The index cannot be read.
    pub fn query(&self, text: &str, under: &str, limit: usize) -> Result<Vec<String>, IndexError> {
        let mut analyzer = self.index.tokenizer_for_field(self.body)?;
        let mut terms = Vec::new();
        analyzer
            .token_stream(text)
            .process(&mut |token| terms.push(Term::from_field_text(self.body, &token.text)));
        if terms.is_empty() || limit == 0 {
            return Ok(Vec::new());
        }
        let mut words = PhrasePrefixQuery::new(terms);
        words.set_max_expansions(PREFIX_EXPANSIONS);
        let query: Box<dyn Query> = if under.is_empty() {
            Box::new(words)
        } else {
            let lower = Bound::Included(Term::from_field_text(self.path, under));
            let upper = Bound::Excluded(Term::from_field_text(
                self.path,
                &format!("{under}\u{10FFFF}"),
            ));
            Box::new(BooleanQuery::new(vec![
                (Occur::Must, Box::new(words) as Box<dyn Query>),
                (Occur::Must, Box::new(RangeQuery::new(lower, upper))),
            ]))
        };
        let searcher = self.reader.searcher();
        let top = searcher.search(&*query, &TopDocs::with_limit(limit).order_by_score())?;
        let mut paths = Vec::with_capacity(top.len());
        for (_score, address) in top {
            let document: TantivyDocument = searcher.doc(address)?;
            if let Some(path) = document.get_first(self.path).and_then(|v| v.as_str()) {
                paths.push(path.to_string());
            }
        }
        Ok(paths)
    }
}
