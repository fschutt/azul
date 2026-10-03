//! The app's state and the glue between an open file and azul's CodeView:
//! the data callback (a line's text and colours from the piece table and
//! the highlighter), the edits applied, undo / redo, find / replace, go to
//! line.

use std::path::PathBuf;

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
    workspace::{file_name, Root, TabDoc, Tabs, Workspace},
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
    /// The workspace key (`src/main.rs`).
    pub key: String,
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
        &self.key
    }
}

impl Doc {
    /// A file read from the workspace: `bytes` (UTF-8, invalid bytes
    /// replaced).
    #[must_use]
    pub fn open(id: u64, key: &str, bytes: &[u8]) -> Doc {
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
            key: key.to_string(),
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

    /// The line count and the dirty flag read again from the text.
    pub fn refresh(&mut self) {
        if let Some((count, dirty)) = self.with_text(|t| (t.buffer.line_count(), t.buffer.is_dirty())) {
            self.line_count = u32::try_from(count).unwrap_or(u32::MAX);
            self.dirty = dirty;
        }
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

/// A save in flight: the file, and the undo depth its bytes are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingSave {
    pub key: String,
    pub depth: usize,
}

/// Everything the window shows.
pub struct AppState {
    pub kit: RefAny,
    pub data_root: PathBuf,
    /// `--sample`: open the sample workspace (written on first use).
    pub sample: bool,
    /// A folder named on the command line, opened when the window exists.
    pub workspace_to_open: Option<Root>,
    pub workspace: Option<Workspace>,
    pub tabs: Tabs<Doc>,
    pub side: Side,
    pub find: FindState,
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
            workspace: None,
            tabs: Tabs::default(),
            side: Side::Explorer,
            find: FindState::default(),
            goto: None,
            notice: String::new(),
            asking_close: false,
            close_after_save: false,
            saving: Vec::new(),
            writing_sample: false,
            window: (1280.0, 800.0),
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
            (None, Some(w)) => format!("{w} - AzCode"),
            _ => "AzCode".to_string(),
        }
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
            doc.view.select(position(found.start_pos()), position(found.end_pos()));
            doc.view.reveal_line(position(found.start_pos()).line);
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
