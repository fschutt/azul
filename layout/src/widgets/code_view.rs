//! Code view widget - the editing surface of a code editor, a log viewer,
//! a configuration file, a hex editor's text column: monospace lines under
//! a line-number gutter, the current line, syntax colours, one or more
//! cursors with their selections, over as many lines as the app has (a
//! million is the yardstick).
//!
//! GENERIC OVER ITS DATA: the view holds no text. It asks the app for the
//! lines it shows through a DATA callback ([`CodeView::with_data_source`]:
//! `line -> CodeViewLine { text, spans }`, the spans being the line's
//! syntax colours as byte ranges and [`CodeTokenKind`]s). Only the lines in
//! view are asked for and built; a key that needs a neighbour (Backspace at
//! a line's start, Up, a word jump) asks for that one line.
//!
//! VIRTUALISED IN WHOLE LINES (the scroll window of [`super::cell_grid`] and
//! [`super::data_table`]): the view shows the lines from
//! [`CodeViewView::top_line`] until its viewport ([`CodeView::with_viewport`])
//! is full, the columns from [`CodeViewView::left_column`]. The wheel, the
//! keyboard and the view's own scroll bar move `top_line` / `left_column`,
//! never a pixel offset - a million lines need no 19-million-pixel scroll
//! extent (f32 pixel offsets are exact only to 2^24 px), so the view is
//! exact for any length.
//!
//! THE APP OWNS THE TEXT AND THE STATE: the cursors, the scroll position and
//! a drag in progress are the [`CodeViewView`] the app hands in; every
//! action reports a [`CodeViewEvent`] whose `view` is the NEXT view. An edit
//! comes as [`CodeViewEdit`]s - "replace the text from `start` to `end` with
//! `text`", positions in the text BEFORE the edit, ordered from the last in
//! the text to the first so applying them in order never moves one still to
//! come. The app applies them to its buffer (a piece table, a rope), stores
//! `event.view` and rebuilds. Undo and redo are the app's history: the view
//! reports [`CodeViewEventKind::Undo`] / `Redo` and the app puts the caret
//! back with [`CodeViewView::set_cursor`].
//!
//! A POSITION is a line and a BYTE offset in its UTF-8 text
//! ([`CodeViewPosition`]); what a user sees as a column - tabs expanded to
//! the tab stops - is the VISUAL column, used for the horizontal window,
//! Up / Down (which keep the column the caret had) and the pointer.
//!
//! KEYBOARD (the view is ONE Tab stop): typing replaces every selection;
//! the arrows move every cursor (Shift extends; the word modifier - Option
//! on macOS, Ctrl elsewhere - jumps words; Cmd+Left / Right on macOS go to
//! the line's ends, Cmd+Up / Down to the text's), Home (the first
//! non-blank, then the line's start) / End, Ctrl+Home / End, Page Up /
//! Down; Enter keeps the line's indentation; Tab inserts spaces to the next
//! tab stop or indents the selected lines, Shift+Tab outdents; Backspace /
//! Delete (the word modifier: a word); Ctrl/Cmd+A selects all, +C / +X copy
//! / cut (a whole line without a selection), +V pastes (one line per cursor
//! when the clipboard has as many lines as there are cursors), +Z / +Shift+Z
//! / +Y undo / redo, +D selects the word, then adds its next occurrence as
//! a cursor; Escape drops the extra cursors. Keys the view does not take
//! (Ctrl/Cmd+F, +S, +G, F-keys) bubble to the app.
//!
//! POINTER: a press places the caret (Shift extends, Alt adds a cursor), a
//! drag selects (scrolling at the edges), a double-click selects a word, a
//! press on a line number selects the line, the wheel scrolls whole lines
//! (Shift: columns), the scroll bar's thumb drags through all lines.
//!
//! ACCESSIBILITY: the view is a `TextInput`-role node (multi-line) named by
//! [`CodeView::with_accessibility_name`]; its value says where the caret is
//! ("Line 12 of 400, column 5").
//!
//! Key types: [`CodeView`], [`CodeViewView`], [`CodeViewLine`],
//! [`CodeViewSpan`], [`CodeTokenKind`], [`CodeViewEvent`], [`CodeViewEdit`].

use alloc::{string::String, vec::Vec};

use azul_core::{
    callbacks::{CoreCallbackData, Update},
    dom::{Dom, DomVec, EventFilter, HoverEventFilter, IdOrClass, IdOrClass::Class, IdOrClassVec},
    events::FocusEventFilter,
    refany::RefAny,
    window::VirtualKeyCode,
};
use azul_css::{
    dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
    impl_option, impl_vec, impl_vec_clone, impl_vec_debug, impl_vec_mut, impl_vec_partialeq,
    AzString,
};

use crate::callbacks::CallbackInfo;
use crate::widgets::themes::decl::{px_height, px_left, px_top, px_width, simple};

#[cfg(test)]
#[path = "code_view_tests.rs"]
mod code_view_tests;

// ---- the types the app sees ----

/// A place in the text: a line and a BYTE offset into its UTF-8 text (the
/// line break not counted). Ordered by line, then column.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct CodeViewPosition {
    /// The line, 0-based.
    pub line: u32,
    /// The byte offset into the line's text, 0-based.
    pub column: u32,
}

impl CodeViewPosition {
    /// Byte `column` of line `line`.
    #[must_use]
    pub const fn create(line: u32, column: u32) -> Self {
        Self { line, column }
    }
}

impl_option!(
    CodeViewPosition,
    OptionCodeViewPosition,
    [Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash]
);

/// No goal column: Up / Down take the caret's own visual column.
pub const CODE_VIEW_NO_GOAL: u32 = u32::MAX;

/// One cursor: the caret (`head`) and where its selection started
/// (`anchor`; the same place when nothing is selected).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CodeViewCursor {
    /// Where the selection started.
    pub anchor: CodeViewPosition,
    /// The caret.
    pub head: CodeViewPosition,
    /// The visual column Up / Down keep through shorter lines
    /// ([`CODE_VIEW_NO_GOAL`]: none yet).
    pub goal: u32,
}

impl CodeViewCursor {
    /// A caret at `at`, nothing selected.
    #[must_use]
    pub const fn create(at: CodeViewPosition) -> Self {
        Self {
            anchor: at,
            head: at,
            goal: CODE_VIEW_NO_GOAL,
        }
    }

    /// The text from `anchor` to `head` selected, the caret at `head`.
    #[must_use]
    pub const fn create_selection(anchor: CodeViewPosition, head: CodeViewPosition) -> Self {
        Self {
            anchor,
            head,
            goal: CODE_VIEW_NO_GOAL,
        }
    }

    /// Nothing is selected.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }

    /// The selection's first position.
    #[must_use]
    pub fn start(&self) -> CodeViewPosition {
        if self.anchor <= self.head {
            self.anchor
        } else {
            self.head
        }
    }

    /// The selection's end (past its last byte).
    #[must_use]
    pub fn end(&self) -> CodeViewPosition {
        if self.anchor <= self.head {
            self.head
        } else {
            self.anchor
        }
    }
}

impl Default for CodeViewCursor {
    fn default() -> Self {
        Self::create(CodeViewPosition::default())
    }
}

impl_option!(
    CodeViewCursor,
    OptionCodeViewCursor,
    [Debug, Clone, Copy, PartialEq, Eq, Hash]
);
impl_vec!(
    CodeViewCursor,
    CodeViewCursorVec,
    CodeViewCursorVecDestructor,
    CodeViewCursorVecDestructorType,
    CodeViewCursorVecSlice,
    OptionCodeViewCursor
);
impl_vec_clone!(CodeViewCursor, CodeViewCursorVec, CodeViewCursorVecDestructor);
impl_vec_debug!(CodeViewCursor, CodeViewCursorVec);
impl_vec_mut!(CodeViewCursor, CodeViewCursorVec);
impl_vec_partialeq!(CodeViewCursor, CodeViewCursorVec);

/// What a run of a line is, for its colour. The themes colour every kind in
/// both modes; a highlighter maps its own categories (TextMate scopes,
/// tree-sitter captures) onto these.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum CodeTokenKind {
    /// Not highlighted: the view's ink.
    #[default]
    Plain,
    /// `fn`, `if`, `return`, `pub`.
    Keyword,
    /// A type's name, `u32`, `String`.
    Type,
    /// A function's or method's name.
    Function,
    /// A quoted string or character literal.
    StringLiteral,
    /// A number literal.
    Number,
    /// A comment.
    Comment,
    /// A named constant, `true`, `None`.
    Constant,
    /// A macro or preprocessor directive.
    Macro,
    /// An attribute or annotation, `#[derive]`, `@Override`.
    Attribute,
    /// An operator, `+`, `=>`, `&&`.
    Operator,
    /// Brackets, commas, semicolons.
    Punctuation,
    /// A variable, a parameter, a field.
    Variable,
    /// A markup tag, an XML / HTML element name.
    Tag,
    /// A Markdown heading, a section title.
    Heading,
    /// A link, a URL.
    Link,
    /// Invalid code, an error.
    Invalid,
}

/// How many [`CodeTokenKind`]s there are (the themes' ink table).
pub(crate) const CODE_TOKEN_KINDS: usize = 17;

impl CodeTokenKind {
    /// Every kind, in declaration order (`kind as usize` indexes it).
    pub const ALL: [CodeTokenKind; CODE_TOKEN_KINDS] = [
        Self::Plain,
        Self::Keyword,
        Self::Type,
        Self::Function,
        Self::StringLiteral,
        Self::Number,
        Self::Comment,
        Self::Constant,
        Self::Macro,
        Self::Attribute,
        Self::Operator,
        Self::Punctuation,
        Self::Variable,
        Self::Tag,
        Self::Heading,
        Self::Link,
        Self::Invalid,
    ];

    /// The class a run of this kind wears besides the token class (what a
    /// test or a script finds it by).
    #[must_use]
    pub const fn class_name(self) -> &'static str {
        match self {
            Self::Plain => "__azul-native-code-view-plain",
            Self::Keyword => "__azul-native-code-view-keyword",
            Self::Type => "__azul-native-code-view-type",
            Self::Function => "__azul-native-code-view-function",
            Self::StringLiteral => "__azul-native-code-view-string",
            Self::Number => "__azul-native-code-view-number",
            Self::Comment => "__azul-native-code-view-comment",
            Self::Constant => "__azul-native-code-view-constant",
            Self::Macro => "__azul-native-code-view-macro",
            Self::Attribute => "__azul-native-code-view-attribute",
            Self::Operator => "__azul-native-code-view-operator",
            Self::Punctuation => "__azul-native-code-view-punctuation",
            Self::Variable => "__azul-native-code-view-variable",
            Self::Tag => "__azul-native-code-view-tag",
            Self::Heading => "__azul-native-code-view-heading",
            Self::Link => "__azul-native-code-view-link",
            Self::Invalid => "__azul-native-code-view-invalid",
        }
    }
}

/// A coloured run of a line: the bytes `start..end` of its text are `kind`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct CodeViewSpan {
    /// The run's first byte.
    pub start: u32,
    /// Past the run's last byte.
    pub end: u32,
    /// What the run is.
    pub kind: CodeTokenKind,
}

impl CodeViewSpan {
    /// Bytes `start..end` are `kind`.
    #[must_use]
    pub const fn create(start: u32, end: u32, kind: CodeTokenKind) -> Self {
        Self { start, end, kind }
    }
}

impl_option!(
    CodeViewSpan,
    OptionCodeViewSpan,
    [Debug, Clone, Copy, PartialEq, Eq, Hash]
);
impl_vec!(
    CodeViewSpan,
    CodeViewSpanVec,
    CodeViewSpanVecDestructor,
    CodeViewSpanVecDestructorType,
    CodeViewSpanVecSlice,
    OptionCodeViewSpan
);
impl_vec_clone!(CodeViewSpan, CodeViewSpanVec, CodeViewSpanVecDestructor);
impl_vec_debug!(CodeViewSpan, CodeViewSpanVec);
impl_vec_mut!(CodeViewSpan, CodeViewSpanVec);
impl_vec_partialeq!(CodeViewSpan, CodeViewSpanVec);

/// One line - what the DATA callback answers: its text (without the line
/// break) and its coloured runs (sorted, not overlapping; bytes no span
/// covers are [`CodeTokenKind::Plain`]).
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct CodeViewLine {
    /// The line's text, without the line break.
    pub text: AzString,
    /// The coloured runs.
    pub spans: CodeViewSpanVec,
}

impl CodeViewLine {
    /// A line of `text` with the colours `spans`.
    #[must_use]
    pub const fn create(text: AzString, spans: CodeViewSpanVec) -> Self {
        Self { text, spans }
    }

    /// A plain line of `text` (no colours).
    #[must_use]
    pub const fn create_plain(text: AzString) -> Self {
        Self {
            text,
            spans: CodeViewSpanVec::from_const_slice(&[]),
        }
    }

    /// An empty line.
    #[must_use]
    pub const fn empty() -> Self {
        Self::create_plain(AzString::from_const_str(""))
    }
}

impl Default for CodeViewLine {
    fn default() -> Self {
        Self::empty()
    }
}

impl azul_core::host_invoker::HostOut for CodeViewLine {
    fn unwritten() -> Self {
        Self::empty()
    }
}

/// What a drag in progress does.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum CodeViewDragKind {
    /// No drag.
    #[default]
    None,
    /// The pointer selects text from the press.
    Select,
    /// The scroll bar's thumb is dragged.
    ScrollBar,
}

/// Everything a code view remembers between builds - the app keeps it (and
/// stores every event's `view`).
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct CodeViewView {
    /// The cursors, in the order they were made; never empty, the LAST is
    /// the primary one (the one the view keeps in sight).
    pub cursors: CodeViewCursorVec,
    /// The px a visual column is wide (measured once by the view; 0 = not
    /// yet, an estimate from the font size is used).
    pub char_width: f32,
    /// Where a scroll-bar drag was pressed, in px from the view's top.
    pub drag_start_px: f32,
    /// The first line shown.
    pub top_line: u32,
    /// The first visual column shown.
    pub left_column: u32,
    /// The whole lines the view showed at the last action (0 = not known
    /// yet: the viewport's).
    pub visible_lines: u32,
    /// The whole visual columns the view showed at the last action.
    pub visible_columns: u32,
    /// `top_line` when a scroll-bar drag was pressed.
    pub drag_start_line: u32,
    /// A drag in progress.
    pub drag: CodeViewDragKind,
}

impl Default for CodeViewView {
    fn default() -> Self {
        Self::create()
    }
}

impl CodeViewView {
    /// One caret at the start of the text, scrolled to the top.
    #[must_use]
    pub fn create() -> Self {
        Self {
            cursors: CodeViewCursorVec::from_vec(alloc::vec![CodeViewCursor::default()]),
            char_width: 0.0,
            drag_start_px: 0.0,
            top_line: 0,
            left_column: 0,
            visible_lines: 0,
            visible_columns: 0,
            drag_start_line: 0,
            drag: CodeViewDragKind::None,
        }
    }

    /// The primary cursor (the last one).
    #[must_use]
    pub fn primary(&self) -> CodeViewCursor {
        self.cursors.as_slice().last().copied().unwrap_or_default()
    }

    /// How many cursors there are.
    #[must_use]
    pub fn cursor_count(&self) -> usize {
        self.cursors.len()
    }

    /// One caret at `at`, nothing selected (the other cursors dropped).
    pub fn set_cursor(&mut self, at: CodeViewPosition) {
        self.cursors = CodeViewCursorVec::from_vec(alloc::vec![CodeViewCursor::create(at)]);
    }

    /// [`Self::set_cursor`] for the builder chain.
    #[must_use]
    pub fn with_cursor(mut self, at: CodeViewPosition) -> Self {
        self.set_cursor(at);
        self
    }

    /// One cursor selecting from `anchor` to `head` (the others dropped).
    pub fn select(&mut self, anchor: CodeViewPosition, head: CodeViewPosition) {
        self.cursors = CodeViewCursorVec::from_vec(alloc::vec![CodeViewCursor::create_selection(
            anchor, head
        )]);
    }

    /// One more caret at `at`; it becomes the primary one.
    pub fn add_cursor(&mut self, at: CodeViewPosition) {
        let mut all = self.cursors.as_slice().to_vec();
        all.retain(|c| c.head != at);
        all.push(CodeViewCursor::create(at));
        self.cursors = CodeViewCursorVec::from_vec(all);
    }

    /// Scrolls so `line` is the first line shown.
    pub fn scroll_to_line(&mut self, line: u32) {
        self.top_line = line;
    }

    /// Scrolls the least so `line` is shown; a line far off is put a
    /// third down the view (where "go to line" lands).
    pub fn reveal_line(&mut self, line: u32) {
        let fit = self.fit_lines();
        if line >= self.top_line && line < self.top_line.saturating_add(fit) {
            return;
        }
        let near_above = line < self.top_line && self.top_line - line <= fit / 2;
        let near_below = line >= self.top_line.saturating_add(fit)
            && line - self.top_line.saturating_add(fit) < fit / 2;
        self.top_line = if near_above {
            line
        } else if near_below {
            line.saturating_sub(fit.saturating_sub(1))
        } else {
            line.saturating_sub(fit / 3)
        };
    }

    /// The whole lines the view shows (a screenful when not measured yet).
    pub(crate) fn fit_lines(&self) -> u32 {
        if self.visible_lines > 0 {
            self.visible_lines
        } else {
            DEFAULT_FIT_LINES
        }
    }
}

/// The lines a view that was never measured is taken to show.
pub(crate) const DEFAULT_FIT_LINES: u32 = 30;

/// One edit: replace the text from `start` to `end` (positions in the text
/// BEFORE the event's edits) with `text` (`\n` between lines).
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeViewEdit {
    /// What goes in (may be empty: a deletion).
    pub text: AzString,
    /// The first position replaced.
    pub start: CodeViewPosition,
    /// Past the last position replaced (`== start`: an insertion).
    pub end: CodeViewPosition,
}

impl CodeViewEdit {
    /// Replace `start..end` with `text`.
    #[must_use]
    pub const fn create(start: CodeViewPosition, end: CodeViewPosition, text: AzString) -> Self {
        Self { text, start, end }
    }
}

impl_option!(
    CodeViewEdit,
    OptionCodeViewEdit,
    copy = false,
    [Debug, Clone, PartialEq, Eq]
);
impl_vec!(
    CodeViewEdit,
    CodeViewEditVec,
    CodeViewEditVecDestructor,
    CodeViewEditVecDestructorType,
    CodeViewEditVecSlice,
    OptionCodeViewEdit
);
impl_vec_clone!(CodeViewEdit, CodeViewEditVec, CodeViewEditVecDestructor);
impl_vec_debug!(CodeViewEdit, CodeViewEditVec);
impl_vec_mut!(CodeViewEdit, CodeViewEditVec);
impl_vec_partialeq!(CodeViewEdit, CodeViewEditVec);

/// What happened.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CodeViewEventKind {
    /// The text changes: apply `edits` (in order), store `view`.
    Edit,
    /// The cursors or the selections moved (the view may have scrolled).
    Move,
    /// Only the scroll position changed.
    Scroll,
    /// Ctrl/Cmd+Z: undo the app's last edit, put the caret back.
    Undo,
    /// Ctrl/Cmd+Shift+Z or +Y: redo.
    Redo,
    /// The selection was copied to the clipboard (`text`).
    Copy,
}

/// One action in the view.
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct CodeViewEvent {
    /// The view after the action: store it.
    pub view: CodeViewView,
    /// `Edit`: the edits, last in the text first.
    pub edits: CodeViewEditVec,
    /// `Copy` (and the copy half of a cut, which is an `Edit`): the text
    /// put on the clipboard.
    pub text: AzString,
    /// What happened.
    pub kind: CodeViewEventKind,
}

impl CodeViewEvent {
    /// A `kind` event leaving `view`, nothing else set.
    #[must_use]
    pub fn create(kind: CodeViewEventKind, view: CodeViewView) -> Self {
        Self {
            view,
            edits: CodeViewEditVec::from_const_slice(&[]),
            text: AzString::from_const_str(""),
            kind,
        }
    }
}

// ---- callbacks ----

/// Callback invoked for an action in the view.
pub type CodeViewOnEventCallbackType = extern "C" fn(RefAny, CallbackInfo, CodeViewEvent) -> Update;
impl_widget_callback!(
    CodeViewOnEvent,
    OptionCodeViewOnEvent,
    CodeViewOnEventCallback,
    CodeViewOnEventCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        CodeViewOnEventCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: CODE_VIEW_ON_EVENT_INVOKER,
    invoker_ty:     AzCodeViewOnEventCallbackInvoker,
    thunk_fn:       az_code_view_on_event_callback_thunk,
    setter_fn:      AzApp_setCodeViewOnEventCallbackInvoker,
    from_handle_fn: AzCodeViewOnEventCallback_createFromHostHandle,
    from_handle_byref_fn: AzCodeViewOnEventCallback_createFromHostHandleByref,
    extra_args:     [ event: CodeViewEvent ],
}

/// The DATA callback: line `line` (0-based) of the app's text.
pub type CodeViewDataSourceCallbackType = extern "C" fn(RefAny, u32) -> CodeViewLine;
impl_widget_callback!(
    CodeViewDataSource,
    OptionCodeViewDataSource,
    CodeViewDataSourceCallback,
    CodeViewDataSourceCallbackType
);

// Host-invoker plumbing: the line index carries no context, so the thunk
// reads it from the invocation slot.
azul_core::impl_managed_callback! {
    wrapper:        CodeViewDataSourceCallback,
    ctx_field:      ctx,
    data:           data: RefAny,
    args:           [line: u32],
    return_ty:      CodeViewLine,
    default_ret:    CodeViewLine::empty(),
    invoker_static: CODE_VIEW_DATA_SOURCE_INVOKER,
    invoker_ty:     AzCodeViewDataSourceCallbackInvoker,
    thunk_fn:       az_code_view_data_source_callback_thunk,
    setter_fn:      AzApp_setCodeViewDataSourceCallbackInvoker,
    from_handle_fn: AzCodeViewDataSourceCallback_createFromHostHandle,
    from_handle_byref_fn: AzCodeViewDataSourceCallback_createFromHostHandleByref,
}

/// Line `line`, from the data callback.
pub(crate) fn line_content(source: &OptionCodeViewDataSource, line: u32) -> CodeViewLine {
    match source.as_ref() {
        Some(CodeViewDataSource { refany, callback }) => callback.invoke(refany.clone(), line),
        None => CodeViewLine::empty(),
    }
}

// ---- the widget ----

/// The code view. See the module documentation.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct CodeView {
    /// The app-owned state: cursors, scroll, a drag.
    pub view: CodeViewView,
    /// The `id` of the view's node (default "code-view"): what an app or a
    /// script focuses it by.
    pub id: AzString,
    /// What a screen reader calls the view ("main.rs").
    pub accessibility_name: AzString,
    /// Where the lines come from; none = an empty text.
    pub data_source: OptionCodeViewDataSource,
    /// Hears every action.
    pub on_event: OptionCodeViewOnEvent,
    /// The px the view fills (gutter and scroll bar included): how many
    /// lines and columns it builds. A little more than the real box is
    /// fine (the rest is clipped); the view measures its box at every
    /// action for paging and for keeping the caret in sight.
    pub viewport_width: f32,
    /// See `viewport_width`.
    pub viewport_height: f32,
    /// A line's height in px.
    pub line_height: f32,
    /// The text's font size in px (the OS monospace font).
    pub font_size: f32,
    /// How many lines the app's text has (an empty text has one).
    pub line_count: u32,
    /// The columns between two tab stops (default 4).
    pub tab_width: u32,
    /// The widget theme this view is PINNED to (`with_theme`), or `None`
    /// to follow the app theme.
    pub theme: crate::widgets::themes::OptionUiTheme,
    /// Show the line-number gutter (default on).
    pub show_line_numbers: bool,
    /// A view that is read, not edited: no edits (moving, selecting,
    /// copying and scrolling still work).
    pub read_only: bool,
    /// Tint the caret's line (default on).
    pub highlight_current_line: bool,
}

impl Default for CodeView {
    fn default() -> Self {
        Self::create(1)
    }
}

impl CodeView {
    /// A view over `line_count` lines: 13 px text on 19 px lines, a gutter,
    /// no data until [`Self::with_data_source`].
    #[must_use]
    pub fn create(line_count: u32) -> Self {
        Self {
            view: CodeViewView::create(),
            id: AzString::from_const_str("code-view"),
            accessibility_name: AzString::from_const_str("Code"),
            data_source: None.into(),
            on_event: None.into(),
            viewport_width: 1200.0,
            viewport_height: 800.0,
            line_height: 19.0,
            font_size: 13.0,
            line_count: line_count.max(1),
            tab_width: 4,
            theme: None.into(),
            show_line_numbers: true,
            read_only: false,
            highlight_current_line: true,
        }
    }

    /// The state the app keeps (store every event's `view`).
    pub fn set_view(&mut self, view: CodeViewView) {
        self.view = view;
    }

    /// [`Self::set_view`] for the builder chain.
    #[must_use]
    pub fn with_view(mut self, view: CodeViewView) -> Self {
        self.set_view(view);
        self
    }

    /// The node's `id` (unique in the window).
    pub fn set_id(&mut self, id: AzString) {
        self.id = id;
    }

    /// [`Self::set_id`] for the builder chain.
    #[must_use]
    pub fn with_id(mut self, id: AzString) -> Self {
        self.set_id(id);
        self
    }

    /// What a screen reader calls the view.
    pub fn set_accessibility_name(&mut self, name: AzString) {
        self.accessibility_name = name;
    }

    /// [`Self::set_accessibility_name`] for the builder chain.
    #[must_use]
    pub fn with_accessibility_name(mut self, name: AzString) -> Self {
        self.set_accessibility_name(name);
        self
    }

    /// How many lines the app's text has.
    pub fn set_line_count(&mut self, line_count: u32) {
        self.line_count = line_count.max(1);
    }

    /// [`Self::set_line_count`] for the builder chain.
    #[must_use]
    pub fn with_line_count(mut self, line_count: u32) -> Self {
        self.set_line_count(line_count);
        self
    }

    /// Where the lines come from: `callback(data, line)` for every line in
    /// view, and for the lines a key or a copy needs.
    pub fn set_data_source<C: Into<CodeViewDataSourceCallback>>(&mut self, data: RefAny, callback: C) {
        self.data_source = Some(CodeViewDataSource {
            refany: data,
            callback: callback.into(),
        })
        .into();
    }

    /// [`Self::set_data_source`] for the builder chain.
    #[must_use]
    pub fn with_data_source<C: Into<CodeViewDataSourceCallback>>(mut self, data: RefAny, callback: C) -> Self {
        self.set_data_source(data, callback);
        self
    }

    /// The callback that hears every action.
    pub fn set_on_event<C: Into<CodeViewOnEventCallback>>(&mut self, data: RefAny, callback: C) {
        self.on_event = Some(CodeViewOnEvent {
            refany: data,
            callback: callback.into(),
        })
        .into();
    }

    /// [`Self::set_on_event`] for the builder chain.
    #[must_use]
    pub fn with_on_event<C: Into<CodeViewOnEventCallback>>(mut self, data: RefAny, callback: C) -> Self {
        self.set_on_event(data, callback);
        self
    }

    /// The px the view fills.
    pub fn set_viewport(&mut self, width: f32, height: f32) {
        self.viewport_width = width;
        self.viewport_height = height;
    }

    /// [`Self::set_viewport`] for the builder chain.
    #[must_use]
    pub fn with_viewport(mut self, width: f32, height: f32) -> Self {
        self.set_viewport(width, height);
        self
    }

    /// A line's height in px.
    pub fn set_line_height(&mut self, px: f32) {
        self.line_height = px;
    }

    /// [`Self::set_line_height`] for the builder chain.
    #[must_use]
    pub fn with_line_height(mut self, px: f32) -> Self {
        self.set_line_height(px);
        self
    }

    /// The text's font size in px.
    pub fn set_font_size(&mut self, px: f32) {
        self.font_size = px;
    }

    /// [`Self::set_font_size`] for the builder chain.
    #[must_use]
    pub fn with_font_size(mut self, px: f32) -> Self {
        self.set_font_size(px);
        self
    }

    /// The columns between two tab stops.
    pub fn set_tab_width(&mut self, columns: u32) {
        self.tab_width = columns.max(1);
    }

    /// [`Self::set_tab_width`] for the builder chain.
    #[must_use]
    pub fn with_tab_width(mut self, columns: u32) -> Self {
        self.set_tab_width(columns);
        self
    }

    /// Show the line-number gutter.
    pub fn set_show_line_numbers(&mut self, show: bool) {
        self.show_line_numbers = show;
    }

    /// [`Self::set_show_line_numbers`] for the builder chain.
    #[must_use]
    pub fn with_show_line_numbers(mut self, show: bool) -> Self {
        self.set_show_line_numbers(show);
        self
    }

    /// A view that is read, not edited.
    pub fn set_read_only(&mut self, read_only: bool) {
        self.read_only = read_only;
    }

    /// [`Self::set_read_only`] for the builder chain.
    #[must_use]
    pub fn with_read_only(mut self, read_only: bool) -> Self {
        self.set_read_only(read_only);
        self
    }

    /// Tint the caret's line.
    pub fn set_highlight_current_line(&mut self, highlight: bool) {
        self.highlight_current_line = highlight;
    }

    /// [`Self::set_highlight_current_line`] for the builder chain.
    #[must_use]
    pub fn with_highlight_current_line(mut self, highlight: bool) -> Self {
        self.set_highlight_current_line(highlight);
        self
    }

    /// Pins the view to `theme` (instead of following the app theme).
    pub fn set_theme(&mut self, theme: crate::widgets::themes::UiTheme) {
        self.theme = Some(theme).into();
    }

    /// [`Self::set_theme`] for the builder chain.
    #[must_use]
    pub fn with_theme(mut self, theme: crate::widgets::themes::UiTheme) -> Self {
        self.set_theme(theme);
        self
    }

    /// Replaces `self` with an empty view and returns the original.
    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::default();
        core::mem::swap(&mut s, self);
        s
    }
}

// ---- the text, line by line ----

/// The lines the keys and the pointer read: the app's text through the
/// data callback ([`SourceLines`]), or a fixed list in the tests.
pub(crate) trait Lines {
    /// How many lines there are (at least one).
    fn count(&self) -> u32;
    /// Line `line`'s text, without its break ("" past the end).
    fn text(&self, line: u32) -> String;
}

/// The app's text through the data callback.
pub(crate) struct SourceLines<'a> {
    pub source: &'a OptionCodeViewDataSource,
    pub count: u32,
}

impl Lines for SourceLines<'_> {
    fn count(&self) -> u32 {
        self.count.max(1)
    }

    fn text(&self, line: u32) -> String {
        if line >= self.count() {
            return String::new();
        }
        String::from(line_content(self.source, line).text.as_str())
    }
}

/// The last line's index.
pub(crate) fn last_line(lines: &dyn Lines) -> u32 {
    lines.count().max(1) - 1
}

/// Where the text ends.
pub(crate) fn text_end(lines: &dyn Lines) -> CodeViewPosition {
    let last = last_line(lines);
    CodeViewPosition::create(last, len32(&lines.text(last)))
}

/// A text's length in bytes, as a column.
pub(crate) fn len32(text: &str) -> u32 {
    u32::try_from(text.len()).unwrap_or(u32::MAX)
}

// ---- columns: bytes and what the eye sees ----

/// The visual column of byte `byte` of `text`: tabs advance to the next
/// multiple of `tab`, every other character is one column.
pub(crate) fn visual_column(text: &str, byte: u32, tab: u32) -> u32 {
    todo!("GREEN: visual_column {text} {byte} {tab}")
}

/// The byte of `text` nearest to visual column `column` (a column inside a
/// tab goes to the nearer side of it; past the end is the end).
pub(crate) fn byte_at_visual(text: &str, column: u32, tab: u32) -> u32 {
    todo!("GREEN: byte_at_visual {text} {column} {tab}")
}

/// `byte` kept inside `text` and moved back onto a character boundary.
pub(crate) fn clamp_to_char(text: &str, byte: u32) -> u32 {
    todo!("GREEN: clamp_to_char {text} {byte}")
}

/// The boundary after the character at `byte` (the end stays the end).
pub(crate) fn next_char(text: &str, byte: u32) -> u32 {
    todo!("GREEN: next_char {text} {byte}")
}

/// The boundary before the character left of `byte` (0 stays 0).
pub(crate) fn prev_char(text: &str, byte: u32) -> u32 {
    todo!("GREEN: prev_char {text} {byte}")
}

/// Where a word jump to the left from `byte` lands: over blanks, then over
/// one run of word characters or of punctuation.
pub(crate) fn word_left(text: &str, byte: u32) -> u32 {
    todo!("GREEN: word_left {text} {byte}")
}

/// Where a word jump to the right from `byte` lands (the mirror of
/// [`word_left`]).
pub(crate) fn word_right(text: &str, byte: u32) -> u32 {
    todo!("GREEN: word_right {text} {byte}")
}

/// The word (a run of letters, digits and `_`) at or just left of `byte`;
/// an empty range where there is none.
pub(crate) fn word_at(text: &str, byte: u32) -> (u32, u32) {
    todo!("GREEN: word_at {text} {byte}")
}

/// The byte of the line's first character that is not blank (the end for
/// a blank line).
pub(crate) fn first_non_blank(text: &str) -> u32 {
    todo!("GREEN: first_non_blank {text}")
}

/// `text` with its tabs expanded to spaces, `text` starting at visual
/// column `start`.
pub(crate) fn expand_tabs(text: &str, start: u32, tab: u32) -> String {
    todo!("GREEN: expand_tabs {text} {start} {tab}")
}

// ---- the window: which lines and columns are built ----

/// Padding between the gutter's numbers and its edges, px.
pub(crate) const GUTTER_PAD: f32 = 10.0;
/// Space between the gutter and the text, px.
pub(crate) const TEXT_PAD: f32 = 6.0;
/// The vertical scroll bar's width, px.
pub(crate) const SCROLL_BAR_PX: f32 = 12.0;
/// The shortest a scroll-bar thumb gets, px.
pub(crate) const MIN_THUMB_PX: f32 = 24.0;

/// The vertical scroll bar: its track `(x, y, w, h)` and its thumb along it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ScrollBar {
    pub track: (f32, f32, f32, f32),
    pub thumb_start: f32,
    pub thumb_len: f32,
}

/// Where the lines and columns in view sit.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Geometry {
    /// The first line built (the view's, kept in range).
    pub top: u32,
    /// How many lines are built from `top`.
    pub rows: u32,
    /// The whole lines that fit (paging, keeping the caret in sight).
    pub fit_lines: u32,
    /// The first visual column built.
    pub left: u32,
    /// How many visual columns are built from `left`.
    pub columns: u32,
    /// The whole columns that fit.
    pub fit_columns: u32,
    /// The gutter's width, px (0 without line numbers).
    pub gutter_width: f32,
    /// Where visual column `left` starts, px from the view's left.
    pub text_left: f32,
    /// A visual column's width, px.
    pub char_width: f32,
    /// A line's height, px.
    pub line_height: f32,
    /// The viewport, px.
    pub width: f32,
    pub height: f32,
    /// The vertical scroll bar, when the text is longer than the view.
    pub vbar: Option<ScrollBar>,
}

/// The px a visual column is wide: the view's measurement, else an estimate
/// from the font size (monospace faces are ~0.6 em wide).
pub(crate) fn char_width_of(cv: &CodeView) -> f32 {
    if cv.view.char_width > 0.0 {
        cv.view.char_width
    } else {
        (cv.font_size * 0.6).max(1.0)
    }
}

/// Lays the view out: the lines and columns in view, the gutter, the bar.
pub(crate) fn geometry(cv: &CodeView) -> Geometry {
    todo!("GREEN: geometry {}", cv.line_count)
}

/// The view with its `top_line` kept in range for `line_count` lines.
pub(crate) fn clamp_view(view: &mut CodeViewView, line_count: u32) {
    todo!("GREEN: clamp_view {} {line_count}", view.top_line)
}

// ---- a line's pieces: colours, selections, carets ----

/// One piece of a built line, left to right.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Piece {
    /// A run of text (tabs expanded) of one kind, selected or not.
    Text {
        text: String,
        kind: CodeTokenKind,
        selected: bool,
    },
    /// A caret between two runs.
    Caret,
}

/// A built line: its pieces and whether its line break is selected.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct LinePieces {
    pub pieces: Vec<Piece>,
    pub eol_selected: bool,
}

/// What of line `line` the cursors select (byte ranges; an end of
/// `u32::MAX` takes the line break too) and where their carets are.
pub(crate) fn line_marks(view: &CodeViewView, line: u32) -> (Vec<(u32, u32)>, Vec<u32>) {
    todo!("GREEN: line_marks {line} {}", view.cursor_count())
}

/// Line `text` cut into its pieces: at its spans' edges, its selections'
/// edges and its carets, tabs expanded, only visual columns
/// `left..left + columns` kept; equal neighbours merged.
pub(crate) fn line_pieces(
    text: &str,
    spans: &[CodeViewSpan],
    selected: &[(u32, u32)],
    carets: &[u32],
    tab: u32,
    left: u32,
    columns: u32,
) -> LinePieces {
    todo!(
        "GREEN: line_pieces {text} {} {} {} {tab} {left} {columns}",
        spans.len(),
        selected.len(),
        carets.len()
    )
}

// ---- editing: keys to edits and the next view ----

/// One replacement a cursor asks for, in the text before the action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Change {
    pub start: CodeViewPosition,
    pub end: CodeViewPosition,
    pub text: String,
}

/// Where the cursors go after their changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CaretRule {
    /// After what each cursor inserted, nothing selected (typing, deleting).
    AfterInsert,
    /// Anchor and head carried through the changes (indenting lines keeps
    /// them selected).
    Carry,
}

/// Where `start` is after `text` is inserted there.
pub(crate) fn end_of_insert(start: CodeViewPosition, text: &str) -> CodeViewPosition {
    todo!("GREEN: end_of_insert {} {text}", start.line)
}

/// The text's changes for every cursor applied at once: the edits for the
/// app (last in the text first) and the cursors after them. Changes that
/// overlap an earlier one are dropped.
pub(crate) fn apply_changes(
    view: &CodeViewView,
    per_cursor: Vec<Vec<Change>>,
    rule: CaretRule,
) -> (Vec<CodeViewEdit>, CodeViewView) {
    todo!(
        "GREEN: apply_changes {} {} {rule:?}",
        view.cursor_count(),
        per_cursor.len()
    )
}

/// The modifiers of a key, read the platform's way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Mods {
    /// Shift: extend the selection.
    pub shift: bool,
    /// The primary modifier (Cmd on macOS, Ctrl elsewhere): the shortcuts.
    pub primary: bool,
    /// The word modifier (Option on macOS, Ctrl elsewhere): word jumps.
    pub word: bool,
    /// macOS Cmd with an arrow: to the line's or the text's ends.
    pub line: bool,
    /// Alt held.
    pub alt: bool,
}

/// What `key` does to the view (the pure half of the key handler); `None`
/// leaves the key to the app.
pub(crate) fn key_event(
    cv: &CodeView,
    lines: &dyn Lines,
    key: VirtualKeyCode,
    mods: Mods,
) -> Option<CodeViewEvent> {
    todo!("GREEN: key_event {key:?} {mods:?} {}", lines.count() + cv.line_count)
}

/// Typed text replacing every selection.
pub(crate) fn typed_event(cv: &CodeView, lines: &dyn Lines, typed: &str) -> Option<CodeViewEvent> {
    todo!("GREEN: typed_event {typed} {}", lines.count() + cv.line_count)
}

/// The clipboard's text pasted at every cursor (one line each when there
/// are as many lines as cursors).
pub(crate) fn paste_event(cv: &CodeView, lines: &dyn Lines, text: &str) -> Option<CodeViewEvent> {
    todo!("GREEN: paste_event {text} {}", lines.count() + cv.line_count)
}

/// What a copy takes: every selection (a cursor without one: its whole
/// line and its break), the cursors' texts on lines of their own.
pub(crate) fn copy_text(view: &CodeViewView, lines: &dyn Lines) -> String {
    todo!("GREEN: copy_text {} {}", view.cursor_count(), lines.count())
}

/// The view scrolled so the primary caret is in sight.
pub(crate) fn reveal(view: &mut CodeViewView, lines: &dyn Lines, tab: u32) {
    todo!("GREEN: reveal {} {tab}", lines.count() + view.top_line)
}

// ---- the pointer ----

/// What a pointer at `(x, y)` (px in the view) is over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Hit {
    /// The text, nearest to this position.
    Text(CodeViewPosition),
    /// The line number of this line.
    Gutter(u32),
    /// The scroll bar's thumb.
    Thumb,
    /// The scroll bar's track above the thumb.
    TrackAbove,
    /// The scroll bar's track below the thumb.
    TrackBelow,
}

/// What `(x, y)` is over.
pub(crate) fn hit_test(cv: &CodeView, geo: &Geometry, lines: &dyn Lines, x: f32, y: f32) -> Hit {
    todo!("GREEN: hit_test {x} {y} {} {}", lines.count() + cv.line_count, geo.top)
}

/// What a press at `hit` does (the pure half of the handler).
pub(crate) fn press_event(
    cv: &CodeView,
    geo: &Geometry,
    lines: &dyn Lines,
    hit: Hit,
    mods: Mods,
    y: f32,
) -> Option<CodeViewEvent> {
    todo!(
        "GREEN: press_event {hit:?} {mods:?} {y} {} {}",
        lines.count() + cv.line_count,
        geo.top
    )
}

/// What a move to `(x, y)` does while a drag is in progress.
pub(crate) fn drag_event(cv: &CodeView, geo: &Geometry, lines: &dyn Lines, x: f32, y: f32) -> Option<CodeViewEvent> {
    todo!("GREEN: drag_event {x} {y} {} {}", lines.count() + cv.line_count, geo.top)
}

/// A double-click at `hit`: the word there selected.
pub(crate) fn double_click_event(cv: &CodeView, lines: &dyn Lines, hit: Hit) -> Option<CodeViewEvent> {
    todo!("GREEN: double_click_event {hit:?} {}", lines.count() + cv.line_count)
}

/// The view scrolled by whole `rows` and `columns` (the wheel); `None`
/// when it is already at that edge.
pub(crate) fn scroll_event(cv: &CodeView, geo: &Geometry, rows: i64, columns: i64) -> Option<CodeViewEvent> {
    todo!("GREEN: scroll_event {rows} {columns} {} {}", cv.line_count, geo.top)
}

// ---- the build ----

/// The view's class; the view node also carries the app's `id`.
pub(crate) const VIEW_CLASS_NAME: &str = "__azul-native-code-view";
/// A line (its number and its text).
pub(crate) const LINE_CLASS_NAME: &str = "__azul-native-code-view-line";
/// Added to the caret's line.
pub(crate) const CURRENT_LINE_CLASS_NAME: &str = "__azul-native-code-view-current-line";
/// A line's number.
pub(crate) const GUTTER_CLASS_NAME: &str = "__azul-native-code-view-gutter";
/// A line's text.
pub(crate) const TEXT_CLASS_NAME: &str = "__azul-native-code-view-text";
/// A run of a line's text (it also wears its kind's class).
pub(crate) const TOKEN_CLASS_NAME: &str = "__azul-native-code-view-token";
/// A caret.
pub(crate) const CARET_CLASS_NAME: &str = "__azul-native-code-view-caret";
/// The selected line break at the end of a line.
pub(crate) const EOL_CLASS_NAME: &str = "__azul-native-code-view-eol";
/// The scroll bar's track.
pub(crate) const TRACK_CLASS_NAME: &str = "__azul-native-code-view-track";
/// The scroll bar's thumb.
pub(crate) const THUMB_CLASS_NAME: &str = "__azul-native-code-view-thumb";

/// One line in view: its index and what the data callback answered.
#[derive(Debug, Clone)]
pub(crate) struct ResolvedLine {
    pub index: u32,
    pub line: CodeViewLine,
}

/// The view with its window laid out and the lines in view asked for ONCE
/// (both looks are built from it when the view follows the app theme).
#[derive(Debug, Clone)]
pub(crate) struct CodeViewResolved {
    /// The view, its `top_line` kept in range.
    pub cv: CodeView,
    /// Where the lines and columns in view sit.
    pub geo: Geometry,
    /// The lines in view, top to bottom.
    pub lines: Vec<ResolvedLine>,
}

/// Lays the view out and asks the data callback for the lines in view.
pub(crate) fn resolve(cv: CodeView) -> CodeViewResolved {
    todo!("GREEN: resolve {}", cv.line_count)
}

impl CodeView {
    /// The view's DOM. The data callback is asked ONCE for the lines in
    /// view; the look comes from the theme module
    /// (`themes::flat::code_view` / `themes::flora::code_view`), `None`
    /// carrying both looks.
    #[must_use]
    pub fn dom(self) -> Dom {
        todo!("GREEN: dom {}", self.line_count)
    }
}

impl From<CodeView> for Dom {
    fn from(cv: CodeView) -> Self {
        cv.dom()
    }
}

#[cfg(test)]
pub(crate) mod fixtures {
    //! Code views over fixed texts, for the widget's own tests and the lint
    //! manifest (`widgets::label_convention::every_widget_dom`).
    use super::*;

    /// A fixed text, line by line.
    pub(crate) struct VecLines(pub Vec<String>);

    impl VecLines {
        pub(crate) fn of(text: &str) -> Self {
            Self(text.split('\n').map(String::from).collect())
        }
    }

    impl Lines for VecLines {
        fn count(&self) -> u32 {
            u32::try_from(self.0.len()).unwrap_or(u32::MAX).max(1)
        }

        fn text(&self, line: u32) -> String {
            self.0.get(line as usize).cloned().unwrap_or_default()
        }
    }

    /// The data a fixture view reads: its lines, and how many times a line
    /// was asked for (the window test counts them).
    pub(crate) struct FixtureText {
        pub lines: Vec<String>,
        pub asked: core::cell::Cell<u32>,
    }

    /// Every line of a fixture's text: `fn` coloured as a keyword where a
    /// line starts with it, the rest plain.
    pub(crate) extern "C" fn fixture_line(mut data: RefAny, line: u32) -> CodeViewLine {
        let Some(t) = data.downcast_ref::<FixtureText>() else {
            return CodeViewLine::empty();
        };
        t.asked.set(t.asked.get() + 1);
        let text = t.lines.get(line as usize).cloned().unwrap_or_default();
        let spans = if text.starts_with("fn ") {
            alloc::vec![CodeViewSpan::create(0, 2, CodeTokenKind::Keyword)]
        } else {
            Vec::new()
        };
        CodeViewLine::create(AzString::from(text), CodeViewSpanVec::from_vec(spans))
    }

    /// A view over `text` (lines split at `\n`), 400 x 190 px (ten lines).
    pub(crate) fn over(text: &str) -> CodeView {
        let lines: Vec<String> = text.split('\n').map(String::from).collect();
        let count = u32::try_from(lines.len()).unwrap_or(u32::MAX);
        CodeView::create(count)
            .with_viewport(400.0, 190.0)
            .with_data_source(
                RefAny::new(FixtureText {
                    lines,
                    asked: core::cell::Cell::new(0),
                }),
                fixture_line as CodeViewDataSourceCallbackType,
            )
    }

    /// The sample: a short Rust function, the caret on its second line.
    pub(crate) fn sample() -> CodeView {
        let mut cv = over("fn main() {\n\tlet answer = 42;\n    println!(\"{answer}\");\n}");
        cv.view.set_cursor(CodeViewPosition::create(1, 5));
        cv
    }
}
