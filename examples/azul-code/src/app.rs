//! The app's state and the glue between an open file and azul's CodeView:
//! the data callback (a line's text and colours from the piece table and
//! the highlighter), the edits applied, undo / redo, find / replace, go to
//! line; the palette, the search over the folder.

use std::{
    collections::BTreeSet,
    path::PathBuf,
    sync::{atomic::AtomicBool, Arc},
};

use azul::{
    callbacks::RefAny,
    str::String as AzString,
    widgets::{CodeTokenKind, CodeViewEdit, CodeViewLine, CodeViewPosition, CodeViewSpan, CodeViewView},
};
use azul_appkit::find::TextMatch;

use crate::{
    buffer::{Edit, LineEnding, Pos, TextBuffer},
    highlight::{syntax_for, Highlighter, TokenKind},
    search::{self, Found},
    storage::FileHits,
    terminal::Panel,
    workspace::{doc_ident, file_name, quick_matches, Root, TabDoc, Tabs, Workspace, QUICK_MAX},
};

/// The text of an open file: the CodeView's data (its own `RefAny`, apart
/// from the app's state, so the data callback borrows only this).
pub struct DocText {
    pub buffer: TextBuffer,
    pub highlighter: Highlighter,
    /// A line too far for the UI thread asked for colours: the walk the
    /// highlight timer starts.
    pub walk_to: Option<usize>,
    /// A walk is running.
    pub walking: bool,
}

/// One open file (a tab).
pub struct Doc {
    pub id: u64,
    /// Where the file is: the workspace's root, or its own folder for a file
    /// opened alone (Open File..., a file named on the command line).
    pub root: Root,
    /// The key in `root` (`src/main.rs`).
    pub key: String,
    /// The file's place on disk ([`doc_ident`]): what tells two open files
    /// apart.
    pub ident: String,
    pub name: String,
    /// The [`DocText`].
    pub text: RefAny,
    /// The CodeView's state (cursors, scroll).
    pub view: CodeViewView,
    /// Kept from the text after every change (the window reads these
    /// without borrowing the text).
    pub line_count: u32,
    pub dirty: bool,
    pub language: String,
    pub ending: LineEnding,
}

impl TabDoc for Doc {
    fn key(&self) -> &str {
        &self.ident
    }
}

impl Doc {
    /// File `key` of `root`, read: `bytes` (UTF-8, invalid bytes
    /// replaced).
    #[must_use]
    pub fn open(id: u64, root: &Root, key: &str, bytes: &[u8]) -> Doc {
        let text = String::from_utf8_lossy(bytes);
        let buffer = TextBuffer::from_text(&text);
        let first_line = buffer.line(0);
        let name = file_name(key).to_string();
        let highlighter = Highlighter::new(syntax_for(&name, &first_line));
        let language = highlighter.language().to_string();
        let line_count = u32::try_from(buffer.line_count()).unwrap_or(u32::MAX);
        let ending = buffer.line_ending();
        Doc {
            id,
            root: root.clone(),
            key: key.to_string(),
            ident: doc_ident(root, key),
            name,
            text: RefAny::new(DocText {
                buffer,
                highlighter,
                walk_to: None,
                walking: false,
            }),
            view: CodeViewView::create(),
            line_count,
            dirty: false,
            language,
            ending,
        }
    }

    /// Runs `f` on the text (`None` while it is borrowed elsewhere).
    pub fn with_text<R>(&self, f: impl FnOnce(&mut DocText) -> R) -> Option<R> {
        let mut handle = self.text.clone();
        let mut guard = handle.downcast_mut::<DocText>()?;
        Some(f(&mut guard))
    }

    /// The line count and the dirty flag read again from the text (stdout
    /// `AZCODE_DIRTY <key> 1|0` when the flag turns).
    pub fn refresh(&mut self) {
        if let Some((count, dirty)) = self.with_text(|t| (t.buffer.line_count(), t.buffer.is_dirty())) {
            self.line_count = u32::try_from(count).unwrap_or(u32::MAX);
            if dirty != self.dirty {
                println!("AZCODE_DIRTY {} {}", self.key, u8::from(dirty));
            }
            self.dirty = dirty;
        }
    }

    /// Selects `found` and brings it in sight.
    pub fn select(&mut self, found: Found) {
        self.view.select(position(found.start_pos()), position(found.end_pos()));
        self.view.reveal_line(position(found.start_pos()).line);
    }

    /// The CodeView's edits applied as one undo step; the highlighter
    /// forgets what they changed.
    pub fn apply_edits(&mut self, edits: &[CodeViewEdit]) {
        let edits: Vec<Edit> = edits.iter().map(to_edit).collect();
        self.with_text(|t| {
            for change in t.buffer.apply(&edits) {
                t.highlighter.edited(change.first, change.removed, change.added);
            }
        });
        self.refresh();
    }

    /// `edits` (the app's own: replace all) applied as one undo step.
    pub fn apply_own(&mut self, edits: &[Edit]) {
        self.with_text(|t| {
            for change in t.buffer.apply(edits) {
                t.highlighter.edited(change.first, change.removed, change.added);
            }
        });
        self.refresh();
    }

    /// Undo (`redo == false`) or redo; the caret goes where the text came
    /// back.
    pub fn undo_redo(&mut self, redo: bool) {
        let caret = self
            .with_text(|t| {
                let done = if redo { t.buffer.redo() } else { t.buffer.undo() }?;
                for change in &done.changes {
                    t.highlighter.edited(change.first, change.removed, change.added);
                }
                Some(done.caret)
            })
            .flatten();
        if let Some(at) = caret {
            self.view.set_cursor(position(at));
            self.view.reveal_line(position(at).line);
        }
        self.refresh();
    }

    /// The caret's line and visual column, 1-based ("Ln 4, Col 25").
    #[must_use]
    pub fn caret_label(&self, tab: usize) -> String {
        let head = self.view.primary().head;
        let column = self
            .with_text(|t| {
                let text = t.buffer.line(head.line as usize);
                let mut col = 0;
                for (i, ch) in text.char_indices() {
                    if i >= head.column as usize {
                        break;
                    }
                    col = if ch == '\t' { (col / tab + 1) * tab } else { col + 1 };
                }
                col
            })
            .unwrap_or(0);
        format!("Ln {}, Col {}", head.line + 1, column + 1)
    }

    /// The file's bytes as they go to the disk, and the undo depth they are.
    #[must_use]
    pub fn file_bytes(&self) -> Option<(Vec<u8>, usize)> {
        self.with_text(|t| (t.buffer.to_file_text().into_bytes(), t.buffer.depth()))
    }
}

/// A buffer position as a CodeView one.
#[must_use]
pub fn position(at: Pos) -> CodeViewPosition {
    CodeViewPosition {
        line: u32::try_from(at.line).unwrap_or(u32::MAX),
        column: u32::try_from(at.column).unwrap_or(u32::MAX),
    }
}

/// A CodeView edit as a buffer edit.
#[must_use]
pub fn to_edit(e: &CodeViewEdit) -> Edit {
    Edit {
        start: Pos::new(e.start.line as usize, e.start.column as usize),
        end: Pos::new(e.end.line as usize, e.end.column as usize),
        text: e.text.as_str().to_string(),
    }
}

/// The highlighter's kind as azul's.
#[must_use]
pub fn token_kind(kind: TokenKind) -> CodeTokenKind {
    match kind {
        TokenKind::Plain => CodeTokenKind::Plain,
        TokenKind::Keyword => CodeTokenKind::Keyword,
        TokenKind::Type => CodeTokenKind::Type,
        TokenKind::Function => CodeTokenKind::Function,
        TokenKind::StringLiteral => CodeTokenKind::StringLiteral,
        TokenKind::Number => CodeTokenKind::Number,
        TokenKind::Comment => CodeTokenKind::Comment,
        TokenKind::Constant => CodeTokenKind::Constant,
        TokenKind::Macro => CodeTokenKind::Macro,
        TokenKind::Attribute => CodeTokenKind::Attribute,
        TokenKind::Operator => CodeTokenKind::Operator,
        TokenKind::Punctuation => CodeTokenKind::Punctuation,
        TokenKind::Variable => CodeTokenKind::Variable,
        TokenKind::Tag => CodeTokenKind::Tag,
        TokenKind::Heading => CodeTokenKind::Heading,
        TokenKind::Link => CodeTokenKind::Link,
        TokenKind::Invalid => CodeTokenKind::Invalid,
    }
}

/// The CodeView's DATA callback: line `line` of a [`DocText`], its text
/// and its colours (none yet for a line too far down: the highlight timer
/// walks there on a Thread).
pub extern "C" fn doc_line(mut data: RefAny, line: u32) -> CodeViewLine {
    let Some(mut guard) = data.downcast_mut::<DocText>() else {
        return CodeViewLine {
            text: AzString::from(""),
            spans: Vec::<CodeViewSpan>::new().into(),
        };
    };
    let t: &mut DocText = &mut guard;
    let index = line as usize;
    let text = t.buffer.line(index);
    let buffer = &t.buffer;
    let text_of = |i: usize| buffer.line(i);
    let spans = match t.highlighter.line_spans(index, &text_of) {
        Some(spans) => spans,
        None => {
            if !t.walking {
                t.walk_to = Some(index);
            }
            Vec::new()
        }
    };
    let spans: Vec<CodeViewSpan> = spans
        .iter()
        .map(|s| CodeViewSpan {
            start: s.start,
            end: s.end,
            kind: token_kind(s.kind),
        })
        .collect();
    CodeViewLine {
        text: AzString::from(text),
        spans: spans.into(),
    }
}

/// Which panel the side bar shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Side {
    #[default]
    Explorer,
    Search,
}

/// The find bar.
#[derive(Debug, Clone, Default)]
pub struct FindState {
    pub open: bool,
    pub replace_open: bool,
    pub query: String,
    pub replacement: String,
    pub how: TextMatch,
    pub found: Vec<Found>,
    pub current: Option<usize>,
}

/// A save in flight: the file ([`Doc::ident`]), and the undo depth its
/// bytes are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingSave {
    pub ident: String,
    pub depth: usize,
}

/// Where quick open's list of the workspace's files is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IndexState {
    /// Not asked for yet (or the workspace changed).
    #[default]
    None,
    /// The walk runs on a Thread.
    Running,
    /// [`AppState::index`] holds the files.
    Done,
}

/// What the palette over the window lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteKind {
    /// Quick open (Mod+P): the folder's files by name.
    Files,
    /// The command palette (Mod+Shift+P, or `>` typed into quick open):
    /// every command of the window.
    Commands,
}

/// The palette over the window and what was typed into it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Palette {
    pub kind: PaletteKind,
    pub query: String,
}

/// A row of the search results: a file, or one of its matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultRow {
    /// File `file` of [`FolderSearch::results`].
    File(usize),
    /// Match `hit` of file `file`.
    Hit(usize, usize),
}

/// The side bar's search over the folder's files (VSCode's Search view; the
/// find bar, Mod+F, searches the file in front).
#[derive(Debug, Clone, Default)]
pub struct FolderSearch {
    pub query: String,
    pub how: TextMatch,
    /// The files with matches, in the order the walk found them.
    pub results: Vec<FileHits>,
    /// The files read by the last search.
    pub searched: usize,
    /// The last search saw every file and stopped at no cap.
    pub complete: bool,
    /// A search runs on a Thread.
    pub running: bool,
    /// The search the app asked for last (an older reply is dropped).
    pub generation: u64,
    /// Raised to stop the search that runs.
    pub cancel: Option<Arc<AtomicBool>>,
    /// Files whose matches are folded away in the list.
    pub folded: BTreeSet<String>,
}

impl FolderSearch {
    /// Every match of every file.
    #[must_use]
    pub fn hit_count(&self) -> usize {
        self.results.iter().map(|f| f.hits.len()).sum()
    }

    /// The list's rows: each file, then its matches (unless it is folded).
    #[must_use]
    pub fn rows(&self) -> Vec<ResultRow> {
        let mut rows = Vec::with_capacity(self.results.len() + self.hit_count());
        for (f, file) in self.results.iter().enumerate() {
            rows.push(ResultRow::File(f));
            if !self.folded.contains(&file.key) {
                rows.extend((0..file.hits.len()).map(|h| ResultRow::Hit(f, h)));
            }
        }
        rows
    }

    /// "12 results in 3 files" (VSCode's line over the list).
    #[must_use]
    pub fn summary(&self) -> String {
        let hits = self.hit_count();
        let files = self.results.len();
        if self.query.is_empty() {
            return String::new();
        }
        if self.running && hits == 0 {
            return "Searching...".to_string();
        }
        if hits == 0 {
            return "No results found.".to_string();
        }
        let results = if hits == 1 { "1 result".to_string() } else { format!("{hits} results") };
        let in_files = if files == 1 { "1 file".to_string() } else { format!("{files} files") };
        let more = if self.complete { "" } else { " (the first ones)" };
        format!("{results} in {in_files}{more}")
    }
}

/// Everything the window shows.
pub struct AppState {
    pub kit: RefAny,
    pub data_root: PathBuf,
    /// `--sample`: open the sample workspace (written on first use).
    pub sample: bool,
    /// A folder named on the command line, opened when the window exists.
    pub workspace_to_open: Option<Root>,
    /// A file named on the command line, opened when the window exists.
    pub file_to_open: Option<PathBuf>,
    pub workspace: Option<Workspace>,
    /// The git branch of the workspace's folder (the status bar's).
    pub branch: Option<String>,
    pub tabs: Tabs<Doc>,
    pub side: Side,
    pub find: FindState,
    /// The side bar's search over the folder.
    pub search: FolderSearch,
    /// A match to select in a file once it has opened ([`Doc::ident`]).
    pub reveal: Option<(String, Found)>,
    /// The go-to-line bar is open, with what was typed.
    pub goto: Option<String>,
    pub notice: String,
    pub asking_close: bool,
    pub close_after_save: bool,
    pub saving: Vec<PendingSave>,
    /// The sample's files are being written.
    pub writing_sample: bool,
    /// The window's size (the CodeView's viewport hint).
    pub window: (f32, f32),
    /// The side bar shows (Mod+B; a click on the active activity icon hides
    /// it).
    pub side_visible: bool,
    /// The side bar's share of the width beside the editor (its splitter).
    pub side_ratio: f32,
    /// The folders opened last, newest first (kept in the settings).
    pub recent: Vec<String>,
    /// Quick open (Mod+P) or the command palette (Mod+Shift+P) is showing.
    pub palette: Option<Palette>,
    /// Mod+K was pressed: the second key of a chord is awaited (Mod+K
    /// Mod+O opens a folder).
    pub chord: bool,
    /// The workspace's files (keys) for quick open.
    pub index: Vec<String>,
    pub index_state: IndexState,
    /// The terminal panel under the editor.
    pub panel: Panel,
    /// `--shell`: what the terminal panel runs (`None`: the user's shell).
    pub shell: Option<(String, Vec<String>)>,
    next_id: u64,
}

impl AppState {
    #[must_use]
    pub fn new(kit: RefAny, data_root: PathBuf, sample: bool) -> AppState {
        AppState {
            kit,
            data_root,
            sample,
            workspace_to_open: None,
            file_to_open: None,
            workspace: None,
            branch: None,
            tabs: Tabs::default(),
            side: Side::Explorer,
            find: FindState::default(),
            search: FolderSearch::default(),
            reveal: None,
            goto: None,
            notice: String::new(),
            asking_close: false,
            close_after_save: false,
            saving: Vec::new(),
            writing_sample: false,
            window: (1280.0, 800.0),
            side_visible: true,
            side_ratio: 0.22,
            recent: Vec::new(),
            palette: None,
            chord: false,
            index: Vec::new(),
            index_state: IndexState::None,
            panel: Panel::default(),
            shell: None,
            next_id: 1,
        }
    }

    /// A fresh document id.
    pub fn new_id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    /// The window's title: the file in front, the workspace, the app.
    #[must_use]
    pub fn title(&self) -> String {
        let workspace = self.workspace.as_ref().map(|w| w.root.name.clone());
        match (self.tabs.active(), workspace) {
            (Some(doc), Some(w)) => format!("{} - {w} - AzCode", doc.name),
            (Some(doc), None) => format!("{} - AzCode", doc.name),
            (None, Some(w)) => format!("{w} - AzCode"),
            (None, None) => "AzCode".to_string(),
        }
    }

    /// The files quick open lists for what was typed (indices into
    /// [`Self::index`], best first).
    #[must_use]
    pub fn quick_files(&self) -> Vec<usize> {
        let query = self
            .palette
            .as_ref()
            .filter(|p| p.kind == PaletteKind::Files)
            .map_or("", |p| p.query.as_str());
        quick_matches(&self.index, query, QUICK_MAX)
    }

    /// The folder on disk the workspace is (the sample's folder in the data
    /// tree): where a new terminal starts.
    #[must_use]
    pub fn workspace_folder(&self) -> Option<PathBuf> {
        let root = &self.workspace.as_ref()?.root;
        let prefix = root.prefix.trim_end_matches('/');
        Some(if prefix.is_empty() {
            root.drive_root.clone()
        } else {
            root.drive_root.join(prefix)
        })
    }

    /// Some open file has unsaved changes.
    #[must_use]
    pub fn any_dirty(&self) -> bool {
        self.tabs.docs.iter().any(|d| d.dirty)
    }

    /// A save is in flight.
    #[must_use]
    pub fn is_saving(&self) -> bool {
        !self.saving.is_empty()
    }

    /// The find bar's matches computed again for the file in front, the
    /// current one the first at or after the caret.
    pub fn refresh_find(&mut self) {
        let (query, how) = (self.find.query.clone(), self.find.how);
        let Some(doc) = self.tabs.active() else {
            self.find.found.clear();
            self.find.current = None;
            return;
        };
        let head = doc.view.primary().head;
        let found = doc
            .with_text(|t| search::find_all(&t.buffer, &query, how))
            .unwrap_or_default();
        self.find.current = search::next_after(&found, Pos::new(head.line as usize, head.column as usize));
        self.find.found = found;
    }

    /// Selects match `index` in the file in front and brings it in sight.
    pub fn select_match(&mut self, index: usize) {
        let Some(found) = self.find.found.get(index).copied() else {
            return;
        };
        self.find.current = Some(index);
        if let Some(doc) = self.tabs.active_mut() {
            doc.select(found);
        }
    }

    /// The next (`forward`) or previous match from the caret, selected.
    pub fn step_match(&mut self, forward: bool) {
        let Some(doc) = self.tabs.active() else {
            return;
        };
        let cursor = doc.view.primary();
        let at = if forward { cursor.end() } else { cursor.start() };
        let at = Pos::new(at.line as usize, at.column as usize);
        let next = if forward {
            search::next_after(&self.find.found, at)
        } else {
            search::previous_before(&self.find.found, at)
        };
        if let Some(i) = next {
            self.select_match(i);
        }
    }

    /// The current match replaced (then the next one selected).
    pub fn replace_current(&mut self) -> bool {
        let Some(found) = self.find.current.and_then(|i| self.find.found.get(i).copied()) else {
            return false;
        };
        let replacement = self.find.replacement.clone();
        if let Some(doc) = self.tabs.active_mut() {
            doc.apply_own(&[Edit::new(found.start_pos(), found.end_pos(), &replacement)]);
            let after = Pos::new(found.line, found.start + replacement.len());
            doc.view.set_cursor(position(after));
        }
        self.refresh_find();
        if let Some(i) = self.find.current {
            self.select_match(i);
        }
        true
    }

    /// Every match replaced, one undo step; how many.
    pub fn replace_all(&mut self) -> usize {
        let (query, replacement, how) = (self.find.query.clone(), self.find.replacement.clone(), self.find.how);
        let Some(doc) = self.tabs.active_mut() else {
            return 0;
        };
        let edits = doc
            .with_text(|t| search::replace_all(&t.buffer, &query, &replacement, how))
            .unwrap_or_default();
        let n = edits.len();
        if n > 0 {
            doc.apply_own(&edits);
        }
        self.refresh_find();
        n
    }

    /// The go-to-line bar's text applied: the caret on that line.
    pub fn go_to(&mut self, input: &str) -> bool {
        let Some(doc) = self.tabs.active_mut() else {
            return false;
        };
        let Some(at) = search::go_to_line(input, doc.line_count as usize) else {
            return false;
        };
        let at = doc
            .with_text(|t| {
                let len = t.buffer.line(at.line).len();
                Pos::new(at.line, at.column.min(len))
            })
            .unwrap_or(at);
        doc.view.set_cursor(position(at));
        doc.view.reveal_line(position(at).line);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::Hit;

    fn hit(line: usize) -> Hit {
        Hit {
            line,
            start: 0,
            end: 6,
            preview: "picked".to_string(),
            preview_start: 0,
            preview_end: 6,
        }
    }

    #[test]
    fn the_search_results_list_each_file_then_its_matches_unless_folded() {
        let mut s = FolderSearch {
            query: "picked".to_string(),
            complete: true,
            ..FolderSearch::default()
        };
        assert_eq!(s.summary(), "No results found.");
        s.results = vec![
            FileHits {
                key: "Cargo.toml".to_string(),
                hits: vec![hit(1)],
            },
            FileHits {
                key: "src/main.rs".to_string(),
                hits: vec![hit(0), hit(4)],
            },
        ];
        assert_eq!(
            s.rows(),
            vec![
                ResultRow::File(0),
                ResultRow::Hit(0, 0),
                ResultRow::File(1),
                ResultRow::Hit(1, 0),
                ResultRow::Hit(1, 1)
            ]
        );
        assert_eq!(s.summary(), "3 results in 2 files");
        s.folded.insert("src/main.rs".to_string());
        assert_eq!(s.rows().len(), 3, "a folded file shows no matches");
        s.complete = false;
        assert_eq!(s.summary(), "3 results in 2 files (the first ones)");
        s.query.clear();
        assert_eq!(s.summary(), "", "no query, no summary");
    }
}
