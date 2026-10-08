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
//! [`CodeViewView::top_line`] until its box is full, the columns from
//! [`CodeViewView::left_column`]. The wheel, the keyboard and the view's own
//! scroll bar move `top_line` / `left_column`, never a pixel offset - a
//! million lines need no 19-million-pixel scroll extent (f32 pixel offsets
//! are exact only to 2^24 px), so the view is exact for any length.
//!
//! THE LINES ARE A `VirtualView`'S DOM: the view node (the focus stop, the
//! handlers, what a screen reader hears) holds one `VirtualView`, and that
//! view builds the lines that fit its real box. So the lines can be rendered
//! again ALONE: a scroll renders them again and nothing else - the window
//! around them is not rebuilt (as the terminal view's rows and the explorer
//! of a file tree are). Scrolling by rebuilding the window cost the app's
//! layout callback, the cascade and the layout of every node of the window
//! for every notch of the wheel (AzCode: ~45 ms a notch, where the page of
//! AzWidgets scrolls with no DOM work at all).
//!
//! THE APP OWNS THE TEXT AND THE STATE: the cursors, the scroll position and
//! a drag in progress are the [`CodeViewView`] the app hands in; every
//! action reports a [`CodeViewEvent`] whose `view` is the NEXT view. An edit
//! comes as [`CodeViewEdit`]s - "replace the text from `start` to `end` with
//! `text`", positions in the text BEFORE the edit, ordered from the last in
//! the text to the first so applying them in order never moves one still to
//! come. The app applies them to its buffer (a piece table, a rope), stores
//! `event.view` and rebuilds. A [`CodeViewEventKind::Scroll`] is the one
//! event that needs no rebuild: the view has rendered its lines again
//! itself, the app stores `event.view` (the next build starts from it) and
//! answers `Update::DoNothing`. Undo and redo are the app's history: the
//! view reports [`CodeViewEventKind::Undo`] / `Redo` and the app puts the
//! caret back with [`CodeViewView::set_cursor`].
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
//! ACCESSIBILITY: the view is a `Text`-role node named by
//! [`CodeView::with_accessibility_name`]; its value says where the caret is
//! ("Line 12 of 400, byte 5").
//!
//! Key types: [`CodeView`], [`CodeViewView`], [`CodeViewLine`],
//! [`CodeViewSpan`], [`CodeTokenKind`], [`CodeViewEvent`], [`CodeViewEdit`].

use alloc::{string::String, vec::Vec};

use azul_core::{
    callbacks::{CoreCallbackData, Update, VirtualViewCallback, VirtualViewCallbackInfo, VirtualViewReturn},
    dom::{Dom, DomVec, EventFilter, HoverEventFilter, IdOrClass, IdOrClass::Class, IdOrClassVec},
    events::FocusEventFilter,
    geom::{LogicalPosition, LogicalRect},
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
/// both modes; a highlighter maps its own categories (`TextMate` scopes,
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
    pub const ALL: [Self; CODE_TOKEN_KINDS] = [
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
    /// Only the scroll position changed. The view has rendered its lines
    /// again itself: store `view`, the window needs no rebuild.
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
    /// The px the view fills (gutter and scroll bar included), a hint for
    /// the handlers until the view is laid out: its lines are built for the
    /// box it really has (its `VirtualView`'s), and it measures that box at
    /// every action for paging and for keeping the caret in sight.
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
    let tab = tab.max(1);
    let mut column = 0_u32;
    for (i, ch) in text.char_indices() {
        if len32(&text[..i]) >= byte {
            break;
        }
        column = advance(column, ch, tab);
    }
    column
}

/// The visual column after `ch`, which starts at `column`.
fn advance(column: u32, ch: char, tab: u32) -> u32 {
    if ch == '\t' {
        (column / tab).saturating_add(1).saturating_mul(tab)
    } else {
        column.saturating_add(1)
    }
}

/// The byte of `text` nearest to visual column `column` (a column inside a
/// tab goes to the nearer side of it; past the end is the end).
pub(crate) fn byte_at_visual(text: &str, column: u32, tab: u32) -> u32 {
    let tab = tab.max(1);
    let mut at = 0_u32;
    for (i, ch) in text.char_indices() {
        let next = advance(at, ch, tab);
        if column < next {
            let width = next - at;
            let before = (column - at).saturating_mul(2) <= width;
            let byte = if before { i } else { i + ch.len_utf8() };
            return u32::try_from(byte).unwrap_or(u32::MAX);
        }
        at = next;
    }
    len32(text)
}

/// `byte` kept inside `text` and moved back onto a character boundary.
pub(crate) fn clamp_to_char(text: &str, byte: u32) -> u32 {
    let mut b = (byte as usize).min(text.len());
    while b > 0 && !text.is_char_boundary(b) {
        b -= 1;
    }
    u32::try_from(b).unwrap_or(u32::MAX)
}

/// The boundary after the character at `byte` (the end stays the end).
pub(crate) fn next_char(text: &str, byte: u32) -> u32 {
    let b = clamp_to_char(text, byte) as usize;
    let next = text[b..].chars().next().map_or(b, |c| b + c.len_utf8());
    u32::try_from(next).unwrap_or(u32::MAX)
}

/// The boundary before the character left of `byte` (0 stays 0).
pub(crate) fn prev_char(text: &str, byte: u32) -> u32 {
    let b = clamp_to_char(text, byte) as usize;
    let prev = text[..b].chars().next_back().map_or(0, |c| b - c.len_utf8());
    u32::try_from(prev).unwrap_or(u32::MAX)
}

/// What a character is to a word jump.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CharClass {
    Blank,
    Word,
    Punct,
}

fn class_of(c: char) -> CharClass {
    if c.is_whitespace() {
        CharClass::Blank
    } else if c.is_alphanumeric() || c == '_' {
        CharClass::Word
    } else {
        CharClass::Punct
    }
}

/// Byte `b` moved left over the characters of class `class`.
fn skip_left(text: &str, mut b: usize, class: CharClass) -> usize {
    while let Some(c) = text[..b].chars().next_back() {
        if class_of(c) != class {
            break;
        }
        b -= c.len_utf8();
    }
    b
}

/// Byte `b` moved right over the characters of class `class`.
fn skip_right(text: &str, mut b: usize, class: CharClass) -> usize {
    while let Some(c) = text[b..].chars().next() {
        if class_of(c) != class {
            break;
        }
        b += c.len_utf8();
    }
    b
}

/// Where a word jump to the left from `byte` lands: over blanks, then over
/// one run of word characters or of punctuation.
pub(crate) fn word_left(text: &str, byte: u32) -> u32 {
    let mut b = skip_left(text, clamp_to_char(text, byte) as usize, CharClass::Blank);
    if let Some(c) = text[..b].chars().next_back() {
        b = skip_left(text, b, class_of(c));
    }
    u32::try_from(b).unwrap_or(u32::MAX)
}

/// Where a word jump to the right from `byte` lands (the mirror of
/// [`word_left`]).
pub(crate) fn word_right(text: &str, byte: u32) -> u32 {
    let mut b = skip_right(text, clamp_to_char(text, byte) as usize, CharClass::Blank);
    if let Some(c) = text[b..].chars().next() {
        b = skip_right(text, b, class_of(c));
    }
    u32::try_from(b).unwrap_or(u32::MAX)
}

/// The word (a run of letters, digits and `_`) at or just left of `byte`;
/// an empty range where there is none.
pub(crate) fn word_at(text: &str, byte: u32) -> (u32, u32) {
    let b = clamp_to_char(text, byte) as usize;
    let here = text[b..].chars().next().map(class_of) == Some(CharClass::Word);
    let left = text[..b].chars().next_back().map(class_of) == Some(CharClass::Word);
    if !here && !left {
        let b = u32::try_from(b).unwrap_or(u32::MAX);
        return (b, b);
    }
    let start = skip_left(text, b, CharClass::Word);
    let end = skip_right(text, b, CharClass::Word);
    (
        u32::try_from(start).unwrap_or(u32::MAX),
        u32::try_from(end).unwrap_or(u32::MAX),
    )
}

/// The byte of the line's first character that is not blank (the end for
/// a blank line).
pub(crate) fn first_non_blank(text: &str) -> u32 {
    text.char_indices()
        .find(|(_, c)| !c.is_whitespace())
        .map_or(len32(text), |(i, _)| u32::try_from(i).unwrap_or(u32::MAX))
}

/// `text` with its tabs expanded to spaces, `text` starting at visual
/// column `start`.
pub(crate) fn expand_tabs(text: &str, start: u32, tab: u32) -> String {
    let tab = tab.max(1);
    let mut out = String::with_capacity(text.len());
    let mut column = start;
    for ch in text.chars() {
        let next = advance(column, ch, tab);
        if ch == '\t' {
            for _ in column..next {
                out.push(' ');
            }
        } else {
            out.push(ch);
        }
        column = next;
    }
    out
}

/// Whether `c` is part of a word (whole-word matching).
fn is_word_char(c: char) -> bool {
    class_of(c) == CharClass::Word
}

/// `at` kept inside the text: its line on a line that exists, its column
/// inside that line and on a character boundary.
pub(crate) fn clamp_pos(lines: &dyn Lines, at: CodeViewPosition) -> CodeViewPosition {
    let line = at.line.min(last_line(lines));
    let text = lines.text(line);
    CodeViewPosition::create(line, clamp_to_char(&text, at.column))
}

/// The text from `start` to `end` (`\n` between lines).
pub(crate) fn range_text(lines: &dyn Lines, start: CodeViewPosition, end: CodeViewPosition) -> String {
    let (start, end) = (clamp_pos(lines, start), clamp_pos(lines, end));
    if end <= start {
        return String::new();
    }
    if start.line == end.line {
        let text = lines.text(start.line);
        return String::from(&text[start.column as usize..end.column as usize]);
    }
    let mut out = String::new();
    for line in start.line..=end.line {
        let text = lines.text(line);
        if line == start.line {
            out.push_str(&text[start.column as usize..]);
        } else if line == end.line {
            out.push('\n');
            out.push_str(&text[..end.column as usize]);
        } else {
            out.push('\n');
            out.push_str(&text);
        }
    }
    out
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

/// How many decimal digits `n` has.
pub(crate) fn digits(mut n: u32) -> u32 {
    let mut d = 1;
    while n >= 10 {
        n /= 10;
        d += 1;
    }
    d
}

/// The whole lines the viewport holds and the lines built to fill it (a
/// part of one more shows at the bottom).
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn viewport_lines(cv: &CodeView) -> (u32, u32) {
    let lh = cv.line_height.max(1.0);
    let h = cv.viewport_height.max(0.0);
    let fit = ((h / lh).floor() as u32).max(1);
    let built = ((h / lh).ceil() as u32).max(1);
    (fit, built)
}

/// The whole lines the view shows: as measured at the last action, else
/// what the viewport holds.
pub(crate) fn fit_lines_of(cv: &CodeView) -> u32 {
    if cv.view.visible_lines > 0 {
        cv.view.visible_lines
    } else {
        viewport_lines(cv).0
    }
}

/// Lays the view out: the lines and columns in view, the gutter, the bar.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_precision_loss)]
pub(crate) fn geometry(cv: &CodeView) -> Geometry {
    let count = cv.line_count.max(1);
    let cw = char_width_of(cv);
    let lh = cv.line_height.max(1.0);
    let (_, built) = viewport_lines(cv);
    let fit_lines = fit_lines_of(cv);
    let top = cv.view.top_line.min(count - 1);
    let rows = built.min(count - top);
    let gutter_width = if cv.show_line_numbers {
        digits(count).max(3) as f32 * cw + 2.0 * GUTTER_PAD
    } else {
        0.0
    };
    let text_left = gutter_width + TEXT_PAD;
    let width = cv.viewport_width.max(0.0);
    let height = cv.viewport_height.max(0.0);
    let has_bar = count > fit_lines;
    let bar_px = if has_bar { SCROLL_BAR_PX } else { 0.0 };
    let text_width = (width - text_left - bar_px).max(0.0);
    let fit_columns = if cv.view.visible_columns > 0 {
        cv.view.visible_columns
    } else {
        ((text_width / cw).floor() as u32).max(1)
    };
    let columns = ((text_width / cw).ceil() as u32).max(1).saturating_add(1);
    let vbar = has_bar.then(|| {
        let max_top = (count - 1).max(1) as f32;
        let thumb_len = (height * fit_lines as f32 / (max_top + fit_lines as f32))
            .max(MIN_THUMB_PX)
            .min(height);
        let travel = (height - thumb_len).max(0.0);
        ScrollBar {
            track: ((width - SCROLL_BAR_PX).max(0.0), 0.0, SCROLL_BAR_PX, height),
            thumb_start: travel * top as f32 / max_top,
            thumb_len,
        }
    });
    Geometry {
        top,
        rows,
        fit_lines,
        left: cv.view.left_column,
        columns,
        fit_columns,
        gutter_width,
        text_left,
        char_width: cw,
        line_height: lh,
        width,
        height,
        vbar,
    }
}

/// The view with its `top_line` kept in range for `line_count` lines, and
/// its cursors on lines that exist (their columns are kept inside the
/// line where the line is read).
pub(crate) fn clamp_view(view: &mut CodeViewView, line_count: u32) {
    let last = line_count.max(1) - 1;
    view.top_line = view.top_line.min(last);
    let mut all = view.cursors.as_slice().to_vec();
    if all.is_empty() {
        all.push(CodeViewCursor::default());
    }
    for c in &mut all {
        c.anchor.line = c.anchor.line.min(last);
        c.head.line = c.head.line.min(last);
    }
    view.cursors = CodeViewCursorVec::from_vec(all);
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
    let mut selected = Vec::new();
    let mut carets = Vec::new();
    for c in view.cursors.as_slice() {
        if c.head.line == line {
            carets.push(c.head.column);
        }
        if c.is_empty() {
            continue;
        }
        let (start, end) = (c.start(), c.end());
        if line < start.line || line > end.line {
            continue;
        }
        let from = if line == start.line { start.column } else { 0 };
        let to = if line == end.line { end.column } else { u32::MAX };
        if from < to {
            selected.push((from, to));
        }
    }
    selected.sort_unstable();
    carets.sort_unstable();
    carets.dedup();
    (selected, carets)
}

/// `piece` added to `pieces`, merged into the run before it when that one
/// is of the same kind and selection.
fn push_run(pieces: &mut Vec<Piece>, text: String, kind: CodeTokenKind, selected: bool) {
    if let Some(Piece::Text {
        text: last,
        kind: last_kind,
        selected: last_selected,
    }) = pieces.last_mut()
    {
        if *last_kind == kind && *last_selected == selected {
            last.push_str(&text);
            return;
        }
    }
    pieces.push(Piece::Text { text, kind, selected });
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
    let len = len32(text);
    let right = left.saturating_add(columns);
    let carets: Vec<u32> = carets.iter().map(|&c| clamp_to_char(text, c)).collect();
    let mut cuts: Vec<u32> = alloc::vec![0, len];
    for s in spans {
        cuts.push(clamp_to_char(text, s.start));
        cuts.push(clamp_to_char(text, s.end));
    }
    for &(a, b) in selected {
        cuts.push(clamp_to_char(text, a));
        cuts.push(clamp_to_char(text, b.min(len)));
    }
    cuts.extend(carets.iter().copied());
    cuts.sort_unstable();
    cuts.dedup();
    let kind_at = |b: u32| {
        spans
            .iter()
            .find(|s| s.start <= b && b < s.end)
            .map_or(CodeTokenKind::Plain, |s| s.kind)
    };
    let selected_at = |b: u32| selected.iter().any(|&(s, e)| s <= b && b < e);
    let mut pieces = Vec::new();
    let mut column = 0_u32;
    for w in cuts.windows(2) {
        let (a, b) = (w[0], w[1]);
        if carets.contains(&a) && column >= left && column <= right {
            pieces.push(Piece::Caret);
        }
        let expanded = expand_tabs(&text[a as usize..b as usize], column, tab);
        let width = u32::try_from(expanded.chars().count()).unwrap_or(u32::MAX);
        let (from, to) = (left.max(column), right.min(column.saturating_add(width)));
        if from < to {
            let shown: String = expanded
                .chars()
                .skip((from - column) as usize)
                .take((to - from) as usize)
                .collect();
            push_run(&mut pieces, shown, kind_at(a), selected_at(a));
        }
        column = column.saturating_add(width);
    }
    if carets.contains(&len) && column >= left && column <= right {
        pieces.push(Piece::Caret);
    }
    let eol_selected =
        selected.iter().any(|&(_, e)| e == u32::MAX) && column >= left && column <= right;
    LinePieces {
        pieces,
        eol_selected,
    }
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
    match text.rfind('\n') {
        None => CodeViewPosition::create(start.line, start.column.saturating_add(len32(text))),
        Some(i) => CodeViewPosition::create(
            start.line.saturating_add(newlines(text)),
            len32(&text[i + 1..]),
        ),
    }
}

/// How many line breaks `text` has.
fn newlines(text: &str) -> u32 {
    u32::try_from(text.bytes().filter(|b| *b == b'\n').count()).unwrap_or(u32::MAX)
}

/// The running shift of a pass over sorted changes: the lines gained so
/// far, and where the last change ended before and after it (a position
/// on that old line moves with it).
#[derive(Debug, Clone, Copy, Default)]
struct Shift {
    lines: i64,
    last: Option<(CodeViewPosition, CodeViewPosition)>,
}

impl Shift {
    /// Where `at` (after every change seen so far, before the next) is now.
    fn moved(&self, at: CodeViewPosition) -> CodeViewPosition {
        if let Some((old_end, new_end)) = self.last {
            if at.line == old_end.line && at.column >= old_end.column {
                return CodeViewPosition::create(
                    new_end.line,
                    new_end.column.saturating_add(at.column - old_end.column),
                );
            }
        }
        let line = (i64::from(at.line) + self.lines).clamp(0, i64::from(u32::MAX));
        CodeViewPosition::create(u32::try_from(line).unwrap_or(u32::MAX), at.column)
    }

    /// `change` seen; returns where its inserted text ends now.
    fn step(&mut self, change: &Change) -> CodeViewPosition {
        let new_end = end_of_insert(self.moved(change.start), &change.text);
        self.lines += i64::from(newlines(&change.text))
            - i64::from(change.end.line.saturating_sub(change.start.line));
        self.last = Some((change.end, new_end));
        new_end
    }
}

/// Where `at` (in the text before `changes`, sorted and apart) is after
/// them: a position inside a replaced range - or at an insertion point -
/// goes to the end of what replaced it.
pub(crate) fn map_position(changes: &[Change], at: CodeViewPosition) -> CodeViewPosition {
    let mut shift = Shift::default();
    for c in changes {
        if c.start > at {
            break;
        }
        let new_end = shift.step(c);
        if at <= c.end {
            return new_end;
        }
    }
    shift.moved(at)
}

/// The cursors with a caret on the same place as a later one dropped (the
/// later one - the primary is last - wins); never empty.
fn normalized(all: Vec<CodeViewCursor>) -> Vec<CodeViewCursor> {
    let mut out: Vec<CodeViewCursor> = Vec::with_capacity(all.len());
    for c in all.into_iter().rev() {
        if !out.iter().any(|o| o.head == c.head) {
            out.push(c);
        }
    }
    out.reverse();
    if out.is_empty() {
        out.push(CodeViewCursor::default());
    }
    out
}

/// The text's changes for every cursor applied at once: the edits for the
/// app (last in the text first) and the cursors after them. Changes that
/// overlap an earlier one are dropped.
pub(crate) fn apply_changes(
    view: &CodeViewView,
    per_cursor: Vec<Vec<Change>>,
    rule: CaretRule,
) -> (Vec<CodeViewEdit>, CodeViewView) {
    let mut all: Vec<(usize, Change)> = per_cursor
        .into_iter()
        .enumerate()
        .flat_map(|(i, changes)| changes.into_iter().map(move |c| (i, c)))
        .collect();
    all.sort_by(|a, b| a.1.start.cmp(&b.1.start).then(a.1.end.cmp(&b.1.end)));
    let mut kept: Vec<(usize, Change)> = Vec::with_capacity(all.len());
    for (i, c) in all {
        if let Some((_, prev)) = kept.last() {
            if c.end < prev.end || c.start == prev.start {
                continue;
            }
        }
        kept.push((i, c));
    }
    let changes: Vec<Change> = kept.iter().map(|(_, c)| c.clone()).collect();
    let mut shift = Shift::default();
    let ends: Vec<CodeViewPosition> = changes.iter().map(|c| shift.step(c)).collect();
    let moved: Vec<CodeViewCursor> = view
        .cursors
        .as_slice()
        .iter()
        .enumerate()
        .map(|(ci, cursor)| match rule {
            CaretRule::AfterInsert => match kept.iter().rposition(|(i, _)| *i == ci) {
                Some(k) => CodeViewCursor::create(ends[k]),
                None => CodeViewCursor::create(map_position(&changes, cursor.head)),
            },
            CaretRule::Carry => CodeViewCursor::create_selection(
                map_position(&changes, cursor.anchor),
                map_position(&changes, cursor.head),
            ),
        })
        .collect();
    let mut next = view.clone();
    next.cursors = CodeViewCursorVec::from_vec(normalized(moved));
    let edits = changes
        .into_iter()
        .rev()
        .map(|c| CodeViewEdit::create(c.start, c.end, AzString::from(c.text)))
        .collect();
    (edits, next)
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
    use VirtualKeyCode as K;
    if mods.primary && !mods.alt {
        match key {
            K::A => return Some(select_all(cv, lines)),
            K::C => return Some(copy_event(cv, lines)),
            K::X => return Some(cut_event(cv, lines)),
            K::Z | K::Y if cv.read_only => return None,
            K::Z if mods.shift => return Some(CodeViewEvent::create(CodeViewEventKind::Redo, cv.view.clone())),
            K::Z => return Some(CodeViewEvent::create(CodeViewEventKind::Undo, cv.view.clone())),
            K::Y => return Some(CodeViewEvent::create(CodeViewEventKind::Redo, cv.view.clone())),
            K::D => return next_occurrence(cv, lines),
            _ => {}
        }
    }
    match key {
        K::Left | K::Right | K::Up | K::Down | K::Home | K::End | K::PageUp | K::PageDown => {
            Some(move_key(cv, lines, key, mods))
        }
        K::Back => delete_back(cv, lines, mods),
        K::Delete => delete_forward(cv, lines, mods),
        K::Return | K::NumpadEnter if !mods.primary => enter(cv, lines),
        K::Tab if !mods.primary => tab_key(cv, lines, mods),
        K::Escape => escape(cv),
        _ => None,
    }
}

/// An `Edit` event: `edits` and the next `view`, kept in sight.
fn edit_event(cv: &CodeView, lines: &dyn Lines, edits: Vec<CodeViewEdit>, mut view: CodeViewView) -> CodeViewEvent {
    reveal_in(cv, &mut view, lines);
    let mut e = CodeViewEvent::create(CodeViewEventKind::Edit, view);
    e.edits = CodeViewEditVec::from_vec(edits);
    e
}

/// A `Move` event leaving `view`, kept in sight.
fn move_event(cv: &CodeView, lines: &dyn Lines, mut view: CodeViewView) -> CodeViewEvent {
    reveal_in(cv, &mut view, lines);
    CodeViewEvent::create(CodeViewEventKind::Move, view)
}

/// [`reveal`] with the view's own measure of what fits.
fn reveal_in(cv: &CodeView, view: &mut CodeViewView, lines: &dyn Lines) {
    let geo = geometry(cv);
    reveal(view, lines, cv.tab_width, fit_lines_of(cv), geo.fit_columns);
}

/// The same change for every cursor: `f(cursor)` (clamped to the text)
/// gives its changes.
fn per_cursor(
    cv: &CodeView,
    lines: &dyn Lines,
    mut f: impl FnMut(CodeViewCursor) -> Vec<Change>,
) -> Vec<Vec<Change>> {
    cv.view
        .cursors
        .as_slice()
        .iter()
        .map(|c| {
            let clamped = CodeViewCursor {
                anchor: clamp_pos(lines, c.anchor),
                head: clamp_pos(lines, c.head),
                goal: c.goal,
            };
            f(clamped)
        })
        .collect()
}

/// `per` applied, or `None` when no cursor changes anything.
fn changed(
    cv: &CodeView,
    lines: &dyn Lines,
    per: Vec<Vec<Change>>,
    rule: CaretRule,
) -> Option<CodeViewEvent> {
    if per.iter().all(Vec::is_empty) {
        return None;
    }
    let (edits, view) = apply_changes(&cv.view, per, rule);
    if edits.is_empty() {
        return None;
    }
    Some(edit_event(cv, lines, edits, view))
}

/// Every cursor's selection replaced by `text`.
fn replace_selections(cv: &CodeView, lines: &dyn Lines, text: &str) -> Option<CodeViewEvent> {
    let per = per_cursor(cv, lines, |c| {
        alloc::vec![Change {
            start: c.start(),
            end: c.end(),
            text: String::from(text),
        }]
    });
    changed(cv, lines, per, CaretRule::AfterInsert)
}

/// Ctrl/Cmd+A: one cursor over the whole text.
fn select_all(cv: &CodeView, lines: &dyn Lines) -> CodeViewEvent {
    let mut view = cv.view.clone();
    view.select(CodeViewPosition::default(), text_end(lines));
    move_event(cv, lines, view)
}

/// Ctrl/Cmd+C: the selections (or the carets' lines) as a `Copy` event.
fn copy_event(cv: &CodeView, lines: &dyn Lines) -> CodeViewEvent {
    let mut e = CodeViewEvent::create(CodeViewEventKind::Copy, cv.view.clone());
    e.text = AzString::from(copy_text(&cv.view, lines));
    e
}

/// The whole line `line` with its break (the last line: with the break
/// before it), what a cut without a selection takes.
fn whole_line(lines: &dyn Lines, line: u32) -> (CodeViewPosition, CodeViewPosition) {
    let last = last_line(lines);
    if line < last {
        (CodeViewPosition::create(line, 0), CodeViewPosition::create(line + 1, 0))
    } else if line > 0 {
        (
            CodeViewPosition::create(line - 1, len32(&lines.text(line - 1))),
            CodeViewPosition::create(line, len32(&lines.text(line))),
        )
    } else {
        (CodeViewPosition::create(0, 0), CodeViewPosition::create(0, len32(&lines.text(0))))
    }
}

/// Ctrl/Cmd+X: copied, then removed (a read-only view only copies).
fn cut_event(cv: &CodeView, lines: &dyn Lines) -> CodeViewEvent {
    let copied = copy_event(cv, lines);
    if cv.read_only {
        return copied;
    }
    let whole = cv.view.cursors.as_slice().iter().all(CodeViewCursor::is_empty);
    let per = per_cursor(cv, lines, |c| {
        if whole {
            let (start, end) = whole_line(lines, c.head.line);
            alloc::vec![Change { start, end, text: String::new() }]
        } else if c.is_empty() {
            Vec::new()
        } else {
            alloc::vec![Change {
                start: c.start(),
                end: c.end(),
                text: String::new(),
            }]
        }
    });
    match changed(cv, lines, per, CaretRule::AfterInsert) {
        Some(mut e) => {
            e.text = copied.text;
            e
        }
        None => copied,
    }
}

/// Ctrl/Cmd+D: the word at a lone caret selected; with a selection, its
/// next occurrence (after the primary cursor, round past the end) becomes
/// one more cursor. `None` when every occurrence has one.
fn next_occurrence(cv: &CodeView, lines: &dyn Lines) -> Option<CodeViewEvent> {
    let primary = cv.view.primary();
    let mut view = cv.view.clone();
    if primary.is_empty() {
        let head = clamp_pos(lines, primary.head);
        let (s, e) = word_at(&lines.text(head.line), head.column);
        if s == e {
            return None;
        }
        let mut all = view.cursors.as_slice().to_vec();
        if let Some(last) = all.last_mut() {
            *last = CodeViewCursor::create_selection(
                CodeViewPosition::create(head.line, s),
                CodeViewPosition::create(head.line, e),
            );
        }
        view.cursors = CodeViewCursorVec::from_vec(all);
        return Some(move_event(cv, lines, view));
    }
    let (start, end) = (clamp_pos(lines, primary.start()), clamp_pos(lines, primary.end()));
    if start.line != end.line {
        return None;
    }
    let needle = range_text(lines, start, end);
    if needle.is_empty() {
        return None;
    }
    let whole_word = needle.chars().all(is_word_char);
    let taken: Vec<CodeViewPosition> = view.cursors.as_slice().iter().map(CodeViewCursor::start).collect();
    let count = u64::from(lines.count());
    for step in 0..=count {
        let line = u32::try_from((u64::from(end.line) + step) % count).unwrap_or(0);
        let text = lines.text(line);
        let from = if step == 0 { end.column as usize } else { 0 };
        for (i, _) in text[from.min(text.len())..].match_indices(needle.as_str()) {
            let at = from + i;
            let after = at + needle.len();
            if whole_word {
                let left_ok = text[..at].chars().next_back().is_none_or(|c| !is_word_char(c));
                let right_ok = text[after..].chars().next().is_none_or(|c| !is_word_char(c));
                if !left_ok || !right_ok {
                    continue;
                }
            }
            let found = CodeViewPosition::create(line, u32::try_from(at).unwrap_or(u32::MAX));
            if taken.contains(&found) {
                continue;
            }
            let mut all = view.cursors.as_slice().to_vec();
            all.push(CodeViewCursor::create_selection(
                found,
                CodeViewPosition::create(line, u32::try_from(after).unwrap_or(u32::MAX)),
            ));
            view.cursors = CodeViewCursorVec::from_vec(all);
            return Some(move_event(cv, lines, view));
        }
    }
    None
}

/// Where Up / Down (`delta` lines) take `head`, keeping visual column
/// `goal`: (the place, the goal it keeps).
fn vertical(lines: &dyn Lines, head: CodeViewPosition, goal: u32, delta: i64, tab: u32) -> (CodeViewPosition, u32) {
    let text = lines.text(head.line);
    let goal = if goal == CODE_VIEW_NO_GOAL {
        visual_column(&text, head.column, tab)
    } else {
        goal
    };
    let target = i64::from(head.line) + delta;
    if target < 0 {
        return (CodeViewPosition::default(), CODE_VIEW_NO_GOAL);
    }
    let last = last_line(lines);
    if target > i64::from(last) {
        return (text_end(lines), CODE_VIEW_NO_GOAL);
    }
    let line = u32::try_from(target).unwrap_or(last);
    let column = byte_at_visual(&lines.text(line), goal, tab);
    (CodeViewPosition::create(line, column), goal)
}

/// One character left of `head` (onto the line above at a line's start).
fn char_left(lines: &dyn Lines, head: CodeViewPosition, word: bool) -> CodeViewPosition {
    if head.column > 0 {
        let text = lines.text(head.line);
        let column = if word {
            word_left(&text, head.column)
        } else {
            prev_char(&text, head.column)
        };
        CodeViewPosition::create(head.line, column)
    } else if head.line > 0 {
        CodeViewPosition::create(head.line - 1, len32(&lines.text(head.line - 1)))
    } else {
        head
    }
}

/// One character right of `head` (onto the next line at a line's end).
fn char_right(lines: &dyn Lines, head: CodeViewPosition, word: bool) -> CodeViewPosition {
    let text = lines.text(head.line);
    if head.column < len32(&text) {
        let column = if word {
            word_right(&text, head.column)
        } else {
            next_char(&text, head.column)
        };
        CodeViewPosition::create(head.line, column)
    } else if head.line < last_line(lines) {
        CodeViewPosition::create(head.line + 1, 0)
    } else {
        head
    }
}

/// The arrows, Home / End, Page Up / Down: every cursor moved (Shift
/// keeps its anchor), the view kept on the primary one.
fn move_key(cv: &CodeView, lines: &dyn Lines, key: VirtualKeyCode, mods: Mods) -> CodeViewEvent {
    use VirtualKeyCode as K;
    let tab = cv.tab_width.max(1);
    let page = fit_lines_of(cv).saturating_sub(1).max(1);
    let moved: Vec<CodeViewCursor> = cv
        .view
        .cursors
        .as_slice()
        .iter()
        .map(|c| {
            let head = clamp_pos(lines, c.head);
            if !mods.shift && !c.is_empty() && !mods.word && !mods.line {
                match key {
                    K::Left => return CodeViewCursor::create(clamp_pos(lines, c.start())),
                    K::Right => return CodeViewCursor::create(clamp_pos(lines, c.end())),
                    _ => {}
                }
            }
            let line_start = CodeViewPosition::create(head.line, 0);
            let line_end = CodeViewPosition::create(head.line, len32(&lines.text(head.line)));
            let (to, goal) = match key {
                K::Left if mods.line => (line_start, CODE_VIEW_NO_GOAL),
                K::Right if mods.line => (line_end, CODE_VIEW_NO_GOAL),
                K::Left => (char_left(lines, head, mods.word), CODE_VIEW_NO_GOAL),
                K::Right => (char_right(lines, head, mods.word), CODE_VIEW_NO_GOAL),
                K::Up if mods.line => (CodeViewPosition::default(), CODE_VIEW_NO_GOAL),
                K::Down if mods.line => (text_end(lines), CODE_VIEW_NO_GOAL),
                K::Up => vertical(lines, head, c.goal, -1, tab),
                K::Down => vertical(lines, head, c.goal, 1, tab),
                K::PageUp => vertical(lines, head, c.goal, -i64::from(page), tab),
                K::PageDown => vertical(lines, head, c.goal, i64::from(page), tab),
                K::Home if mods.primary => (CodeViewPosition::default(), CODE_VIEW_NO_GOAL),
                K::End if mods.primary => (text_end(lines), CODE_VIEW_NO_GOAL),
                K::Home => {
                    let first = first_non_blank(&lines.text(head.line));
                    let column = if head.column == first { 0 } else { first };
                    (CodeViewPosition::create(head.line, column), CODE_VIEW_NO_GOAL)
                }
                K::End => (line_end, CODE_VIEW_NO_GOAL),
                _ => (head, c.goal),
            };
            CodeViewCursor {
                anchor: if mods.shift { clamp_pos(lines, c.anchor) } else { to },
                head: to,
                goal,
            }
        })
        .collect();
    let mut view = cv.view.clone();
    view.cursors = CodeViewCursorVec::from_vec(normalized(moved));
    let last = last_line(lines);
    match key {
        K::PageUp => view.top_line = view.top_line.min(last).saturating_sub(page),
        K::PageDown => view.top_line = view.top_line.saturating_add(page).min(last),
        _ => {}
    }
    move_event(cv, lines, view)
}

/// Backspace: every selection, else the character (the word modifier: the
/// word; macOS Cmd: the line up to the caret) left of every caret - at a
/// line's start, the break before it.
fn delete_back(cv: &CodeView, lines: &dyn Lines, mods: Mods) -> Option<CodeViewEvent> {
    if cv.read_only {
        return None;
    }
    let per = per_cursor(cv, lines, |c| {
        if !c.is_empty() {
            return alloc::vec![Change {
                start: c.start(),
                end: c.end(),
                text: String::new(),
            }];
        }
        let head = c.head;
        let start = if head.column > 0 && mods.line {
            CodeViewPosition::create(head.line, 0)
        } else {
            char_left(lines, head, mods.word)
        };
        if start == head {
            return Vec::new();
        }
        alloc::vec![Change {
            start,
            end: head,
            text: String::new(),
        }]
    });
    changed(cv, lines, per, CaretRule::AfterInsert)
}

/// Delete: every selection, else the character (or word) right of every
/// caret - at a line's end, the break after it.
fn delete_forward(cv: &CodeView, lines: &dyn Lines, mods: Mods) -> Option<CodeViewEvent> {
    if cv.read_only {
        return None;
    }
    let per = per_cursor(cv, lines, |c| {
        if !c.is_empty() {
            return alloc::vec![Change {
                start: c.start(),
                end: c.end(),
                text: String::new(),
            }];
        }
        let head = c.head;
        let end = if mods.line {
            CodeViewPosition::create(head.line, len32(&lines.text(head.line)))
        } else {
            char_right(lines, head, mods.word)
        };
        if end == head {
            return Vec::new();
        }
        alloc::vec![Change {
            start: head,
            end,
            text: String::new(),
        }]
    });
    changed(cv, lines, per, CaretRule::AfterInsert)
}

/// One level of indentation in the style of `text`: a tab where the line
/// is indented with tabs, else `tab` spaces.
fn indent_unit(text: &str, tab: u32) -> String {
    if text.starts_with('\t') {
        String::from("\t")
    } else {
        " ".repeat(tab.max(1) as usize)
    }
}

/// Enter: a line break and the line's indentation (one level more after
/// an opening bracket) at every cursor.
fn enter(cv: &CodeView, lines: &dyn Lines) -> Option<CodeViewEvent> {
    if cv.read_only {
        return None;
    }
    let tab = cv.tab_width.max(1);
    let per = per_cursor(cv, lines, |c| {
        let start = c.start();
        let text = lines.text(start.line);
        let caret = start.column as usize;
        let indent_end = (first_non_blank(&text) as usize).min(caret);
        let mut inserted = String::from("\n");
        inserted.push_str(&text[..indent_end]);
        if text[..caret].trim_end().ends_with(&['{', '(', '['][..]) {
            inserted.push_str(&indent_unit(&text, tab));
        }
        alloc::vec![Change {
            start,
            end: c.end(),
            text: inserted,
        }]
    });
    changed(cv, lines, per, CaretRule::AfterInsert)
}

/// The lines a cursor covers for indenting: its first to its last, the
/// last left out when the selection ends at its very start.
fn covered_lines(c: &CodeViewCursor) -> (u32, u32) {
    let (start, end) = (c.start(), c.end());
    let last = if end.line > start.line && end.column == 0 {
        end.line - 1
    } else {
        end.line
    };
    (start.line, last)
}

/// The bytes Shift+Tab removes at the start of `text`: one tab, or up to
/// `tab` spaces.
fn outdent_len(text: &str, tab: u32) -> u32 {
    if text.starts_with('\t') {
        return 1;
    }
    let spaces = text.bytes().take_while(|b| *b == b' ').count();
    u32::try_from(spaces.min(tab.max(1) as usize)).unwrap_or(0)
}

/// Tab / Shift+Tab: a selection over several lines (or Shift) indents /
/// outdents the cursors' lines; a caret gets spaces to the next tab stop.
fn tab_key(cv: &CodeView, lines: &dyn Lines, mods: Mods) -> Option<CodeViewEvent> {
    if cv.read_only {
        return None;
    }
    let tab = cv.tab_width.max(1);
    let multi_line = cv
        .view
        .cursors
        .as_slice()
        .iter()
        .any(|c| !c.is_empty() && c.start().line != c.end().line);
    if mods.shift || multi_line {
        let per = per_cursor(cv, lines, |c| {
            let (first, last) = covered_lines(&c);
            (first..=last)
                .filter_map(|line| {
                    let text = lines.text(line);
                    let at = CodeViewPosition::create(line, 0);
                    if mods.shift {
                        let n = outdent_len(&text, tab);
                        (n > 0).then(|| Change {
                            start: at,
                            end: CodeViewPosition::create(line, n),
                            text: String::new(),
                        })
                    } else {
                        Some(Change {
                            start: at,
                            end: at,
                            text: indent_unit(&text, tab),
                        })
                    }
                })
                .collect()
        });
        return changed(cv, lines, per, CaretRule::Carry);
    }
    let per = per_cursor(cv, lines, |c| {
        let start = c.start();
        let column = visual_column(&lines.text(start.line), start.column, tab);
        let n = tab - column % tab;
        alloc::vec![Change {
            start,
            end: c.end(),
            text: " ".repeat(n as usize),
        }]
    });
    changed(cv, lines, per, CaretRule::AfterInsert)
}

/// Escape: the extra cursors dropped, else the selection collapsed;
/// `None` for a lone caret (the app hears Escape).
fn escape(cv: &CodeView) -> Option<CodeViewEvent> {
    let primary = cv.view.primary();
    let mut view = cv.view.clone();
    if cv.view.cursor_count() > 1 {
        view.cursors = CodeViewCursorVec::from_vec(alloc::vec![primary]);
    } else if !primary.is_empty() {
        view.set_cursor(primary.head);
    } else {
        return None;
    }
    Some(CodeViewEvent::create(CodeViewEventKind::Move, view))
}

/// Typed text replacing every selection.
pub(crate) fn typed_event(cv: &CodeView, lines: &dyn Lines, typed: &str) -> Option<CodeViewEvent> {
    if cv.read_only || typed.is_empty() {
        return None;
    }
    replace_selections(cv, lines, typed)
}

/// The clipboard's text pasted at every cursor (one line each when there
/// are as many lines as cursors).
pub(crate) fn paste_event(cv: &CodeView, lines: &dyn Lines, text: &str) -> Option<CodeViewEvent> {
    if cv.read_only {
        return None;
    }
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    if text.is_empty() {
        return None;
    }
    let count = cv.view.cursor_count();
    let parts: Vec<&str> = text.split('\n').collect();
    if count < 2 || parts.len() != count {
        return replace_selections(cv, lines, &text);
    }
    // One line per cursor, in the order the cursors sit in the text.
    let mut order: Vec<usize> = (0..count).collect();
    let cursors = cv.view.cursors.as_slice();
    order.sort_by_key(|&i| cursors[i].start());
    let mut part_of = alloc::vec![0_usize; count];
    for (rank, &i) in order.iter().enumerate() {
        part_of[i] = rank;
    }
    let mut index = 0;
    let per = per_cursor(cv, lines, |c| {
        let part = parts[part_of[index]];
        index += 1;
        alloc::vec![Change {
            start: c.start(),
            end: c.end(),
            text: String::from(part),
        }]
    });
    changed(cv, lines, per, CaretRule::AfterInsert)
}

/// What a copy takes: every selection (a cursor without one: its whole
/// line and its break), the cursors' texts on lines of their own.
pub(crate) fn copy_text(view: &CodeViewView, lines: &dyn Lines) -> String {
    let cursors = view.cursors.as_slice();
    if cursors.iter().all(CodeViewCursor::is_empty) {
        let mut taken: Vec<u32> = cursors.iter().map(|c| c.head.line).collect();
        taken.sort_unstable();
        taken.dedup();
        return taken
            .into_iter()
            .map(|l| alloc::format!("{}\n", lines.text(l)))
            .collect();
    }
    let mut selections: Vec<&CodeViewCursor> = cursors.iter().filter(|c| !c.is_empty()).collect();
    selections.sort_by_key(|c| c.start());
    selections
        .into_iter()
        .map(|c| range_text(lines, c.start(), c.end()))
        .collect::<Vec<String>>()
        .join("\n")
}

/// The view scrolled the least so the primary caret is in sight:
/// `fit_lines` whole lines and `fit_columns` whole columns show.
pub(crate) fn reveal(view: &mut CodeViewView, lines: &dyn Lines, tab: u32, fit_lines: u32, fit_columns: u32) {
    let head = clamp_pos(lines, view.primary().head);
    let fit = fit_lines.max(1);
    if head.line < view.top_line {
        view.top_line = head.line;
    } else if head.line >= view.top_line.saturating_add(fit) {
        view.top_line = head.line + 1 - fit;
    }
    let column = visual_column(&lines.text(head.line), head.column, tab.max(1));
    let columns = fit_columns.max(1);
    let margin = (columns / 4).min(8);
    if column < view.left_column {
        view.left_column = column.saturating_sub(margin);
    } else if column >= view.left_column.saturating_add(columns) {
        view.left_column = column.saturating_add(margin).saturating_add(1).saturating_sub(columns);
    }
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
    if let Some(bar) = geo.vbar {
        let (bx, by, bw, bh) = bar.track;
        if x >= bx && x <= bx + bw && y >= by && y <= by + bh {
            return if y < bar.thumb_start {
                Hit::TrackAbove
            } else if y <= bar.thumb_start + bar.thumb_len {
                Hit::Thumb
            } else {
                Hit::TrackBelow
            };
        }
    }
    let line = line_at(geo, lines, y);
    if cv.show_line_numbers && x < geo.gutter_width {
        return Hit::Gutter(line);
    }
    Hit::Text(position_in_line(geo, lines, cv.tab_width, line, x))
}

/// The line under `y` (px in the view): above the first is the first,
/// below the last is the last.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn line_at(geo: &Geometry, lines: &dyn Lines, y: f32) -> u32 {
    let row = if y <= 0.0 {
        0
    } else {
        (y / geo.line_height.max(1.0)).floor() as u32
    };
    geo.top.saturating_add(row).min(last_line(lines))
}

/// The place in `line` nearest to `x` (px in the view).
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_precision_loss)]
fn position_in_line(geo: &Geometry, lines: &dyn Lines, tab: u32, line: u32, x: f32) -> CodeViewPosition {
    let text = lines.text(line);
    let column = geo.left as f32 + ((x - geo.text_left) / geo.char_width.max(0.1)).round();
    let column = if column <= 0.0 { 0 } else { column as u32 };
    CodeViewPosition::create(line, byte_at_visual(&text, column, tab))
}

/// The view's primary cursor replaced by `cursor` (the others dropped).
fn only(view: &mut CodeViewView, cursor: CodeViewCursor) {
    view.cursors = CodeViewCursorVec::from_vec(alloc::vec![cursor]);
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
    let mut view = cv.view.clone();
    let last = last_line(lines);
    let page = geo.fit_lines.saturating_sub(1).max(1);
    let kind = match hit {
        Hit::Thumb => {
            view.drag = CodeViewDragKind::ScrollBar;
            view.drag_start_px = y;
            view.drag_start_line = geo.top;
            CodeViewEventKind::Scroll
        }
        Hit::TrackAbove => {
            view.top_line = geo.top.saturating_sub(page);
            CodeViewEventKind::Scroll
        }
        Hit::TrackBelow => {
            view.top_line = geo.top.saturating_add(page).min(last);
            CodeViewEventKind::Scroll
        }
        Hit::Gutter(line) => {
            let anchor = CodeViewPosition::create(line, 0);
            let head = if line < last {
                CodeViewPosition::create(line + 1, 0)
            } else {
                CodeViewPosition::create(line, len32(&lines.text(line)))
            };
            let anchor = if mods.shift { view.primary().anchor } else { anchor };
            only(&mut view, CodeViewCursor::create_selection(anchor, head));
            view.drag = CodeViewDragKind::Select;
            CodeViewEventKind::Move
        }
        Hit::Text(at) => {
            if mods.shift {
                let anchor = view.primary().anchor;
                only(&mut view, CodeViewCursor::create_selection(anchor, at));
            } else if mods.alt {
                view.add_cursor(at);
            } else {
                view.set_cursor(at);
            }
            view.drag = CodeViewDragKind::Select;
            CodeViewEventKind::Move
        }
    };
    Some(CodeViewEvent::create(kind, view))
}

/// What a move to `(x, y)` does while a drag is in progress.
#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
pub(crate) fn drag_event(cv: &CodeView, geo: &Geometry, lines: &dyn Lines, x: f32, y: f32) -> Option<CodeViewEvent> {
    let mut view = cv.view.clone();
    let last = last_line(lines);
    match view.drag {
        CodeViewDragKind::None => None,
        CodeViewDragKind::ScrollBar => {
            let bar = geo.vbar?;
            let travel = (geo.height - bar.thumb_len).max(1.0);
            let moved = ((y - view.drag_start_px) / travel * last as f32).round() as i64;
            let top = (i64::from(view.drag_start_line) + moved).clamp(0, i64::from(last));
            let top = u32::try_from(top).unwrap_or(0);
            if top == geo.top {
                return None;
            }
            view.top_line = top;
            Some(CodeViewEvent::create(CodeViewEventKind::Scroll, view))
        }
        CodeViewDragKind::Select => {
            // Past an edge the view scrolls a line per move.
            let mut g = geo.clone();
            if y < 0.0 && g.top > 0 {
                g.top -= 1;
            } else if y > geo.height && g.top < last {
                g.top += 1;
            }
            let line = line_at(&g, lines, y.clamp(0.0, (geo.height - 1.0).max(0.0)));
            let at = position_in_line(&g, lines, cv.tab_width, line, x);
            let primary = view.primary();
            if primary.head == at && g.top == geo.top {
                return None;
            }
            view.top_line = g.top;
            let mut all = view.cursors.as_slice().to_vec();
            if let Some(last_cursor) = all.last_mut() {
                *last_cursor = CodeViewCursor::create_selection(primary.anchor, at);
            }
            view.cursors = CodeViewCursorVec::from_vec(all);
            Some(CodeViewEvent::create(CodeViewEventKind::Move, view))
        }
    }
}

/// The drag ended.
pub(crate) fn drag_end(cv: &CodeView) -> Option<CodeViewEvent> {
    if cv.view.drag == CodeViewDragKind::None {
        return None;
    }
    let mut view = cv.view.clone();
    view.drag = CodeViewDragKind::None;
    Some(CodeViewEvent::create(CodeViewEventKind::Move, view))
}

/// A double-click at `hit`: the word there selected.
pub(crate) fn double_click_event(cv: &CodeView, lines: &dyn Lines, hit: Hit) -> Option<CodeViewEvent> {
    let Hit::Text(at) = hit else {
        return None;
    };
    let at = clamp_pos(lines, at);
    let (start, end) = word_at(&lines.text(at.line), at.column);
    if start == end {
        return None;
    }
    let mut view = cv.view.clone();
    view.select(
        CodeViewPosition::create(at.line, start),
        CodeViewPosition::create(at.line, end),
    );
    view.drag = CodeViewDragKind::None;
    Some(CodeViewEvent::create(CodeViewEventKind::Move, view))
}

/// The view scrolled by whole `rows` and `columns` (the wheel); `None`
/// when it is already at that edge.
pub(crate) fn scroll_event(cv: &CodeView, rows: i64, columns: i64) -> Option<CodeViewEvent> {
    let last = cv.line_count.max(1) - 1;
    // From the VIEW's top line (kept in range), as the column is from the
    // view's: the geometry is the BUILD's, and a second wheel turn before
    // the app's rebuild starts from the view the first one stored.
    let from = cv.view.top_line.min(last);
    let top = (i64::from(from) + rows).clamp(0, i64::from(last));
    let left = (i64::from(cv.view.left_column) + columns).clamp(0, i64::from(u32::MAX));
    let (top, left) = (u32::try_from(top).unwrap_or(0), u32::try_from(left).unwrap_or(0));
    if top == from && left == cv.view.left_column {
        return None;
    }
    let mut view = cv.view.clone();
    view.top_line = top;
    view.left_column = left;
    Some(CodeViewEvent::create(CodeViewEventKind::Scroll, view))
}

// ---- the build ----

/// The view's class; the view node also carries the app's `id`.
pub(crate) const VIEW_CLASS_NAME: &str = "__azul-native-code-view";
/// The root of the lines (the DOM of the view's `VirtualView`).
pub(crate) const LINES_CLASS_NAME: &str = "__azul-native-code-view-lines";
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

/// Lays the view's window out (its `top_line` kept in range) without asking
/// for a line: what the view node is built from - its `VirtualView` asks
/// for the lines in view when it renders them ([`render_lines`]).
pub(crate) fn resolve_window(mut cv: CodeView) -> CodeViewResolved {
    clamp_view(&mut cv.view, cv.line_count);
    let geo = geometry(&cv);
    cv.view.top_line = geo.top;
    CodeViewResolved {
        cv,
        geo,
        lines: Vec::new(),
    }
}

/// Lays the view out and asks the data callback for the lines in view.
pub(crate) fn resolve(cv: CodeView) -> CodeViewResolved {
    let mut resolved = resolve_window(cv);
    let (top, rows) = (resolved.geo.top, resolved.geo.rows);
    let lines: Vec<ResolvedLine> = (top..top.saturating_add(rows))
        .map(|index| ResolvedLine {
            index,
            line: line_content(&resolved.cv.data_source, index),
        })
        .collect();
    resolved.lines = lines;
    resolved
}

impl CodeView {
    /// The view's DOM. The data callback is asked ONCE for the lines in
    /// view; the look comes from the theme module
    /// (`themes::flat::code_view` / `themes::flora::code_view`), `None`
    /// carrying both looks.
    #[must_use]
    pub fn dom(self) -> Dom {
        use crate::widgets::themes::UiTheme;
        let theme = self.theme.into_option();
        // The view node asks for no line: the lines in view are asked for
        // when its VirtualView renders them, once (`render_lines`).
        let resolved = resolve_window(self);
        match theme {
            Some(UiTheme::Flora) => crate::widgets::themes::flora::code_view(resolved),
            Some(UiTheme::Flat) => crate::widgets::themes::flat::code_view(resolved),
            None => crate::widgets::themes::theme_blocks::follow_app_theme(
                resolved,
                crate::widgets::themes::flat::code_view,
                crate::widgets::themes::flora::code_view,
            ),
        }
    }
}

use azul_css::{
    css::CssPropertyValue,
    props::{
        basic::{
            font::{StyleFontFamily, StyleFontFamilyVec},
            length::FloatValue,
            pixel::PixelValue,
            StyleFontSize,
        },
        layout::{
            LayoutAlignItems, LayoutBoxSizing, LayoutDisplay, LayoutFlexDirection, LayoutFlexGrow,
            LayoutFlexShrink, LayoutJustifyContent, LayoutMinHeight, LayoutMinWidth, LayoutOverflow,
            LayoutPaddingLeft, LayoutPaddingRight, LayoutPosition, LayoutWidth,
        },
        property::CssProperty,
        style::{StyleCursor, StyleUserSelect, StyleWhiteSpace},
    },
    system::SystemFontType,
};

/// What a theme decides about a code view: the SKIN of each part, laid
/// over the part's base (the structure, the same in every theme) by
/// [`build`].
pub(crate) struct CodeViewLook {
    /// The view: the face and the ink.
    pub view: Vec<CssPropertyWithConditions>,
    /// The gutter: the line numbers' ink, its face, its rule.
    pub gutter: Vec<CssPropertyWithConditions>,
    /// Added to the caret line's number.
    pub gutter_current: Vec<CssPropertyWithConditions>,
    /// Added to the caret's line.
    pub current_line: Vec<CssPropertyWithConditions>,
    /// Added to selected text.
    pub selection: Vec<CssPropertyWithConditions>,
    /// The caret's bar.
    pub caret: Vec<CssPropertyWithConditions>,
    /// The scroll bar's track.
    pub track: Vec<CssPropertyWithConditions>,
    /// The scroll bar's thumb.
    pub thumb: Vec<CssPropertyWithConditions>,
    /// The ink of every [`CodeTokenKind`], `kind as usize`
    /// ([`CODE_TOKEN_KINDS`] entries).
    pub tokens: Vec<Vec<CssPropertyWithConditions>>,
    /// The theme's marker class on the view, if it has one.
    pub marker: Option<&'static str>,
}

/// The OS monospace face (SF Mono, Consolas, the desktop's monospace).
const MONO_FAMILIES: &[StyleFontFamily] = &[StyleFontFamily::SystemType(SystemFontType::Monospace)];
const MONO_FAMILY: StyleFontFamilyVec = StyleFontFamilyVec::from_const_slice(MONO_FAMILIES);

/// The view: a column of lines that takes its pane, clips what does not
/// fit, is the containing block of the scroll bar and is ONE focus stop
/// whose text the engine never selects (the view draws its own).
pub(crate) static CODE_VIEW_BASE: &[CssPropertyWithConditions] = &[
    simple(CssProperty::const_display(LayoutDisplay::Flex)),
    simple(CssProperty::const_flex_direction(LayoutFlexDirection::Column)),
    simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
    simple(CssProperty::const_min_height(LayoutMinHeight::const_px(0))),
    simple(CssProperty::const_min_width(LayoutMinWidth::const_px(0))),
    simple(CssProperty::const_overflow_x(LayoutOverflow::Hidden)),
    simple(CssProperty::const_overflow_y(LayoutOverflow::Hidden)),
    simple(CssProperty::const_position(LayoutPosition::Relative)),
    simple(CssProperty::const_cursor(StyleCursor::Text)),
    simple(CssProperty::user_select(StyleUserSelect::None)),
];

/// A line: its number, then its text, never shrinking.
pub(crate) static CODE_VIEW_LINE_BASE: &[CssPropertyWithConditions] = &[
    simple(CssProperty::const_display(LayoutDisplay::Flex)),
    simple(CssProperty::const_flex_direction(LayoutFlexDirection::Row)),
    simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    simple(CssProperty::const_flex_shrink(LayoutFlexShrink {
        inner: FloatValue::const_new(0),
    })),
    simple(CssProperty::const_box_sizing(LayoutBoxSizing::BorderBox)),
];

/// A line's number: right-aligned in its width, centred on the line.
pub(crate) static CODE_VIEW_GUTTER_BASE: &[CssPropertyWithConditions] = &[
    simple(CssProperty::const_display(LayoutDisplay::Flex)),
    simple(CssProperty::const_flex_direction(LayoutFlexDirection::Row)),
    simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    simple(CssProperty::const_justify_content(LayoutJustifyContent::End)),
    simple(CssProperty::const_flex_shrink(LayoutFlexShrink {
        inner: FloatValue::const_new(0),
    })),
    simple(CssProperty::const_box_sizing(LayoutBoxSizing::BorderBox)),
    simple(CssProperty::const_padding_left(LayoutPaddingLeft::const_px(10))),
    simple(CssProperty::const_padding_right(LayoutPaddingRight::const_px(10))),
];

/// A line's text: its runs side by side, centred on the line, clipped.
pub(crate) static CODE_VIEW_TEXT_BASE: &[CssPropertyWithConditions] = &[
    simple(CssProperty::const_display(LayoutDisplay::Flex)),
    simple(CssProperty::const_flex_direction(LayoutFlexDirection::Row)),
    simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
    simple(CssProperty::const_min_width(LayoutMinWidth::const_px(0))),
    simple(CssProperty::const_overflow_x(LayoutOverflow::Hidden)),
    simple(CssProperty::const_padding_left(LayoutPaddingLeft::const_px(6))),
];

/// A run of text: one line, its blanks kept.
pub(crate) static CODE_VIEW_TOKEN_BASE: &[CssPropertyWithConditions] = &[
    simple(CssProperty::WhiteSpace(CssPropertyValue::Exact(StyleWhiteSpace::Pre))),
    simple(CssProperty::const_flex_shrink(LayoutFlexShrink {
        inner: FloatValue::const_new(0),
    })),
];

/// The zero-width place a caret stands in, between two runs: its bar
/// hangs from it without pushing the text after it.
pub(crate) static CODE_VIEW_CARET_SLOT_BASE: &[CssPropertyWithConditions] = &[
    simple(CssProperty::const_position(LayoutPosition::Relative)),
    simple(CssProperty::const_width(LayoutWidth::const_px(0))),
    simple(CssProperty::const_flex_shrink(LayoutFlexShrink {
        inner: FloatValue::const_new(0),
    })),
];

/// An overlay (the caret's bar, the scroll bar): placed by px.
pub(crate) static CODE_VIEW_OVERLAY_BASE: &[CssPropertyWithConditions] = &[
    simple(CssProperty::const_position(LayoutPosition::Absolute)),
    simple(CssProperty::const_box_sizing(LayoutBoxSizing::BorderBox)),
];

/// The caret's bar's width, px.
pub(crate) const CARET_PX: f32 = 2.0;

/// The view node's `VirtualView`, the box its lines are rendered into: all
/// of the view, a flex item of the view's column (as the explorer of a file
/// tree places its view).
pub(crate) static CODE_VIEW_LINES_VIEW_BASE: &[CssPropertyWithConditions] = &[
    simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
    simple(CssProperty::const_min_height(LayoutMinHeight::const_px(0))),
    simple(CssProperty::const_min_width(LayoutMinWidth::const_px(0))),
    simple(CssProperty::const_width(LayoutWidth::Px(PixelValue::const_percent(100)))),
];

/// The root of the lines, the `VirtualView`'s DOM: a column of lines at the
/// view's own size (set per render), clipped, the containing block of the
/// scroll bar.
pub(crate) static CODE_VIEW_LINES_BASE: &[CssPropertyWithConditions] = &[
    simple(CssProperty::const_display(LayoutDisplay::Flex)),
    simple(CssProperty::const_flex_direction(LayoutFlexDirection::Column)),
    simple(CssProperty::const_overflow_x(LayoutOverflow::Hidden)),
    simple(CssProperty::const_overflow_y(LayoutOverflow::Hidden)),
    simple(CssProperty::const_position(LayoutPosition::Relative)),
];

static VIEW_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(VIEW_CLASS_NAME))];
static LINES_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(LINES_CLASS_NAME))];
static LINE_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(LINE_CLASS_NAME))];
static CURRENT_LINE_CLASS: &[IdOrClass] = &[
    Class(AzString::from_const_str(LINE_CLASS_NAME)),
    Class(AzString::from_const_str(CURRENT_LINE_CLASS_NAME)),
];
static GUTTER_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(GUTTER_CLASS_NAME))];
static TEXT_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(TEXT_CLASS_NAME))];
static CARET_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(CARET_CLASS_NAME))];
static EOL_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(EOL_CLASS_NAME))];
static TRACK_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(TRACK_CLASS_NAME))];
static THUMB_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(THUMB_CLASS_NAME))];

/// A run of text the view wrote (one line, never selected by the engine).
fn text_run(text: AzString, props: Vec<CssPropertyWithConditions>) -> Dom {
    crate::widgets::widget_p_with_text(text).with_css_props(CssPropertyWithConditionsVec::from_vec(props))
}

/// `left` / `top` / `width` / `height` of an overlay.
fn place(x: f32, y: f32, w: f32, h: f32) -> [CssPropertyWithConditions; 4] {
    [px_left(x), px_top(y), px_width(w.max(0.0)), px_height(h.max(0.0))]
}

/// What a screen reader hears the view say where the caret is.
fn caret_value(cv: &CodeView) -> String {
    let head = cv.view.primary().head;
    alloc::format!(
        "Line {} of {}, byte {}",
        head.line.saturating_add(1),
        cv.line_count.max(1),
        head.column.saturating_add(1)
    )
}

/// The view's DOM in `look`: the view node - the focus stop, the handlers,
/// what a screen reader hears - holding ONE `VirtualView`, which renders the
/// lines ([`render_lines`] -> [`build_lines`]). A scroll renders that view
/// again and nothing else: the lines are its DOM, not the app's.
pub(crate) fn build(resolved: CodeViewResolved, look: &CodeViewLook) -> Dom {
    use azul_core::a11y::{AccessibilityInfo, AccessibilityRole};

    let CodeViewResolved { cv, geo, .. } = resolved;
    let mut classes: Vec<IdOrClass> = VIEW_CLASS.to_vec();
    if let Some(marker) = look.marker {
        classes.push(Class(AzString::from_const_str(marker)));
    }
    let mut props = super::themes::decl::on_base(CODE_VIEW_BASE, &look.view);
    props.push(simple(CssProperty::const_font_family(MONO_FAMILY)));
    props.push(simple(CssProperty::const_font_size(StyleFontSize::px(cv.font_size))));
    let a11y = AccessibilityInfo {
        accessibility_value: Some(AzString::from(caret_value(&cv))).into(),
        ..AccessibilityInfo::named(cv.accessibility_name.clone(), AccessibilityRole::Text)
    };
    let id = cv.id.clone();
    // The lines are built under the app theme the view node is built under,
    // also when they are rendered again without a rebuild of the window (a
    // scroll), where no window pass has entered one.
    let theme = azul_core::app_theme::current_theme();
    let shared = RefAny::new(CodeViewShared { cv, geo, theme });
    Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_vec(classes))
        .with_id(id)
        .with_css_props(CssPropertyWithConditionsVec::from_vec(props))
        .with_tab_index(azul_core::dom::TabIndex::Auto)
        .with_accessibility_info(a11y)
        .with_callbacks(code_view_callbacks(&shared).into())
        .with_child(
            Dom::create_virtual_view(shared, VirtualViewCallback::create(render_lines)).with_css_props(
                CssPropertyWithConditionsVec::from_const_slice(CODE_VIEW_LINES_VIEW_BASE),
            ),
        )
}

/// The lines of `resolved` in `look`, the DOM of the view's `VirtualView`:
/// lines [line (number, text) ..] and the scroll bar, at the view's size.
#[allow(clippy::too_many_lines)]
pub(crate) fn build_lines(resolved: CodeViewResolved, look: &CodeViewLook) -> Dom {
    use azul_core::a11y::{AccessibilityInfo, AccessibilityRole};

    let part = |base: &[CssPropertyWithConditions], skin: &[CssPropertyWithConditions]| {
        super::themes::decl::on_base(base, skin)
    };
    let CodeViewResolved { cv, geo, lines } = resolved;
    let view = &cv.view;
    let primary = view.primary().head;
    let tab = cv.tab_width.max(1);
    let lh = geo.line_height;
    let mut children: Vec<Dom> = Vec::with_capacity(lines.len() + 1);
    for resolved_line in &lines {
        let index = resolved_line.index;
        let (selected, carets) = line_marks(view, index);
        let built = line_pieces(
            resolved_line.line.text.as_str(),
            resolved_line.line.spans.as_slice(),
            &selected,
            &carets,
            tab,
            geo.left,
            geo.columns,
        );
        let current = cv.highlight_current_line && index == primary.line;
        let mut line_props = part(
            CODE_VIEW_LINE_BASE,
            if current { look.current_line.as_slice() } else { &[] },
        );
        line_props.push(px_height(lh));
        let mut line_children: Vec<Dom> = Vec::with_capacity(2);
        if cv.show_line_numbers {
            let mut g = part(CODE_VIEW_GUTTER_BASE, &look.gutter);
            if current {
                g.extend(look.gutter_current.iter().cloned());
            }
            g.push(px_width(geo.gutter_width));
            line_children.push(
                Dom::create_div()
                    .with_ids_and_classes(IdOrClassVec::from_const_slice(GUTTER_CLASS))
                    .with_css_props(CssPropertyWithConditionsVec::from_vec(g))
                    .with_child(text_run(
                        AzString::from(alloc::format!("{}", index.saturating_add(1))),
                        CODE_VIEW_TOKEN_BASE.to_vec(),
                    )),
            );
        }
        let mut runs: Vec<Dom> = Vec::with_capacity(built.pieces.len() + 1);
        for piece in built.pieces {
            match piece {
                Piece::Text { text, kind, selected } => {
                    let mut p = part(
                        CODE_VIEW_TOKEN_BASE,
                        look.tokens.get(kind as usize).map_or(&[][..], Vec::as_slice),
                    );
                    if selected {
                        p.extend(look.selection.iter().cloned());
                    }
                    runs.push(
                        text_run(AzString::from(text), p).with_ids_and_classes(IdOrClassVec::from_vec(
                            alloc::vec![
                                Class(AzString::from_const_str(TOKEN_CLASS_NAME)),
                                Class(AzString::from_const_str(kind.class_name())),
                            ],
                        )),
                    );
                }
                Piece::Caret => {
                    let mut bar = part(CODE_VIEW_OVERLAY_BASE, &look.caret);
                    bar.extend(place(-CARET_PX / 2.0, -(lh - 4.0).max(2.0) / 2.0, CARET_PX, (lh - 4.0).max(2.0)));
                    let mut slot = CODE_VIEW_CARET_SLOT_BASE.to_vec();
                    slot.push(px_height(0.0));
                    runs.push(
                        Dom::create_div()
                            .with_css_props(CssPropertyWithConditionsVec::from_vec(slot))
                            .with_child(
                                Dom::create_div()
                                    .with_ids_and_classes(IdOrClassVec::from_const_slice(CARET_CLASS))
                                    .with_css_props(CssPropertyWithConditionsVec::from_vec(bar)),
                            ),
                    );
                }
            }
        }
        if built.eol_selected {
            let mut p = part(CODE_VIEW_TOKEN_BASE, &look.selection);
            p.push(px_width((geo.char_width / 2.0).max(3.0)));
            p.push(px_height((lh - 2.0).max(2.0)));
            runs.push(
                Dom::create_div()
                    .with_ids_and_classes(IdOrClassVec::from_const_slice(EOL_CLASS))
                    .with_css_props(CssPropertyWithConditionsVec::from_vec(p)),
            );
        }
        line_children.push(
            Dom::create_div()
                .with_ids_and_classes(IdOrClassVec::from_const_slice(TEXT_CLASS))
                .with_css_props(CssPropertyWithConditionsVec::from_const_slice(CODE_VIEW_TEXT_BASE))
                .with_children(DomVec::from_vec(runs)),
        );
        children.push(
            Dom::create_div()
                .with_ids_and_classes(IdOrClassVec::from_const_slice(if current {
                    CURRENT_LINE_CLASS
                } else {
                    LINE_CLASS
                }))
                .with_css_props(CssPropertyWithConditionsVec::from_vec(line_props))
                .with_children(DomVec::from_vec(line_children)),
        );
    }
    if let Some(bar) = &geo.vbar {
        let (x, y, w, h) = bar.track;
        let mut track = part(CODE_VIEW_OVERLAY_BASE, &look.track);
        track.extend(place(x, y, w, h));
        let mut thumb = part(CODE_VIEW_OVERLAY_BASE, &look.thumb);
        thumb.extend(place(2.0, bar.thumb_start, (w - 4.0).max(1.0), bar.thumb_len));
        children.push(
            Dom::create_div()
                .with_ids_and_classes(IdOrClassVec::from_const_slice(TRACK_CLASS))
                .with_css_props(CssPropertyWithConditionsVec::from_vec(track))
                .with_accessibility_info(AccessibilityInfo::named(
                    AzString::from_const_str("Lines"),
                    AccessibilityRole::ScrollBar,
                ))
                .with_child(
                    Dom::create_div()
                        .with_ids_and_classes(IdOrClassVec::from_const_slice(THUMB_CLASS))
                        .with_css_props(CssPropertyWithConditionsVec::from_vec(thumb)),
                ),
        );
    }

    let mut root = CODE_VIEW_LINES_BASE.to_vec();
    root.push(px_width(geo.width));
    root.push(px_height(geo.height));
    Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(LINES_CLASS))
        .with_css_props(CssPropertyWithConditionsVec::from_vec(root))
        .with_children(DomVec::from_vec(children))
}

/// The lines of `resolved` in the view's look: the theme it is pinned to,
/// or - following the app theme - both themes' looks in the structure of
/// the current one, as the view node is built (`CodeView::dom`).
pub(crate) fn lines_dom(resolved: CodeViewResolved) -> Dom {
    use crate::widgets::themes::UiTheme;
    let theme = resolved.cv.theme.into_option();
    match theme {
        Some(UiTheme::Flora) => flora_lines(resolved),
        Some(UiTheme::Flat) => flat_lines(resolved),
        None => crate::widgets::themes::theme_blocks::follow_app_theme(resolved, flat_lines, flora_lines),
    }
}

/// The lines in flat's look.
fn flat_lines(resolved: CodeViewResolved) -> Dom {
    build_lines(resolved, &crate::widgets::themes::flat::code_view_look())
}

/// The lines in flora's look.
fn flora_lines(resolved: CodeViewResolved) -> Dom {
    build_lines(resolved, &crate::widgets::themes::flora::code_view_look())
}

/// The view's `VirtualView`: the lines that fit the box it really has and
/// the scroll bar, from the view the handlers keep in the shared data -
/// rendered with every build of the window, and again ALONE after a scroll
/// (`deliver`), so a wheel notch costs the lines in view and never the
/// window around them.
extern "C" fn render_lines(mut data: RefAny, info: VirtualViewCallbackInfo) -> VirtualViewReturn {
    let size = info.get_bounds().get_logical_size();
    let rect = LogicalRect::new(LogicalPosition::zero(), size);
    let Some((mut cv, theme)) = data
        .downcast_ref::<CodeViewShared>()
        .map(|s| (s.cv.clone(), s.theme.clone()))
    else {
        return VirtualViewReturn::keep_current(rect, rect);
    };
    // The box the view has, not the app's hint: the lines that fit it are
    // the lines built, and the scroll bar spans it.
    if size.width.is_finite() && size.width > 0.0 {
        cv.viewport_width = size.width;
    }
    if size.height.is_finite() && size.height > 0.0 {
        cv.viewport_height = size.height;
    }
    let (width, height) = (cv.viewport_width, cv.viewport_height);
    // The data callback runs with no borrow of the view's data held.
    let resolved = resolve(cv);
    let (geo, view) = (resolved.geo.clone(), resolved.cv.view.clone());
    let dom = {
        let _theme = azul_core::app_theme::ThemeScope::enter(theme);
        lines_dom(resolved)
    };
    // The handlers hit-test against the lines as they are drawn.
    if let Some(mut s) = data.downcast_mut::<CodeViewShared>() {
        s.geo = geo;
        s.cv.view = view;
        s.cv.viewport_width = width;
        s.cv.viewport_height = height;
    }
    VirtualViewReturn::with_dom(dom, rect, rect)
}

// ---- the handlers: one set on the view node, the view hit-tests itself ----

/// What every handler of one build and the view's `VirtualView` share: the
/// view (its state and callbacks) and where its lines sit. Every event's
/// view is stored here before the app hears it, so the next action - and
/// the lines rendered again after a scroll - start from it before (or
/// without) the app's rebuild; the render writes back the geometry it drew.
#[derive(Debug)]
pub(crate) struct CodeViewShared {
    pub cv: CodeView,
    pub geo: Geometry,
    /// The app theme the view node was built under (the lines are built
    /// under it).
    pub theme: AzString,
}

/// A copy of the view and its geometry from a handler's payload.
fn shared_of(data: &mut RefAny) -> Option<(CodeView, Geometry)> {
    let s = data.downcast_ref::<CodeViewShared>()?;
    Some((s.cv.clone(), s.geo.clone()))
}

/// Records `view` as the view's state in the payload.
fn store_view(data: &mut RefAny, view: &CodeViewView) {
    if let Some(mut s) = data.downcast_mut::<CodeViewShared>() {
        s.cv.view = view.clone();
    }
}

/// Hands `event` to the app.
fn fire(cv: &CodeView, info: CallbackInfo, event: CodeViewEvent) -> Update {
    match cv.on_event.as_ref() {
        Some(CodeViewOnEvent { refany, callback }) => callback.invoke(refany.clone(), info, event),
        None => Update::DoNothing,
    }
}

/// Stores the event's view, then the app hears it. A `Scroll` moves only
/// the lines, so the view renders them again itself (its `VirtualView`, from
/// the view just stored) and the app need not rebuild anything for it: a
/// scroll answered with a rebuild of the window paid the app's layout
/// callback and the cascade and the layout of every node of the window for
/// every notch of the wheel.
fn deliver(data: &mut RefAny, cv: &CodeView, mut info: CallbackInfo, event: CodeViewEvent) -> Update {
    store_view(data, &event.view);
    let scrolled = event.kind == CodeViewEventKind::Scroll;
    let update = fire(cv, info, event);
    if scrolled {
        rerender_lines(&mut info);
    }
    update
}

/// Renders THIS view's lines again: the `VirtualView` that is the first
/// child of the view node the handler runs on (the focused node for a key,
/// the node the listener sits on for the pointer) - not every view of the
/// window (the explorer's tree, a title bar's glyph, a live status label are
/// views too).
fn rerender_lines(info: &mut CallbackInfo) {
    let host = info.get_hit_node();
    let lines = host
        .node
        .into_crate_internal()
        .and_then(|node| info.get_first_child_node(host.dom, node));
    match lines {
        Some(lines) => info.trigger_virtual_view_rerender(host.dom, lines),
        None => info.trigger_all_virtual_view_rerender(),
    }
}

/// The view's handlers.
pub(crate) fn code_view_callbacks(shared: &RefAny) -> Vec<CoreCallbackData> {
    alloc::vec![
        CoreCallbackData::create(
            EventFilter::Focus(FocusEventFilter::VirtualKeyDown),
            shared.clone(),
            on_key as usize
        ),
        CoreCallbackData::create(
            EventFilter::Focus(FocusEventFilter::TextInput),
            shared.clone(),
            on_text as usize
        ),
        CoreCallbackData::create(
            EventFilter::Focus(FocusEventFilter::Paste),
            shared.clone(),
            on_paste as usize
        ),
        CoreCallbackData::create(
            EventFilter::Hover(HoverEventFilter::LeftMouseDown),
            shared.clone(),
            on_mouse_down as usize
        ),
        CoreCallbackData::create(
            EventFilter::Hover(HoverEventFilter::MouseMove),
            shared.clone(),
            on_mouse_move as usize
        ),
        CoreCallbackData::create(
            EventFilter::Hover(HoverEventFilter::MouseUp),
            shared.clone(),
            on_mouse_up as usize
        ),
        CoreCallbackData::create(
            EventFilter::Hover(HoverEventFilter::DoubleClick),
            shared.clone(),
            on_double_click as usize
        ),
        CoreCallbackData::create(
            EventFilter::Hover(HoverEventFilter::Scroll),
            shared.clone(),
            on_wheel as usize
        ),
    ]
}

/// What the view needs from the window, read at an action: the width of a
/// column (measured once: 64 zeros in the view's face) and its own box
/// (the lines and columns that really fit, for paging and keeping the
/// caret in sight).
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn measure(cv: &mut CodeView, geo: &mut Geometry, info: &CallbackInfo) {
    if cv.view.char_width <= 0.0 {
        if let Some(width) = measured_char_width(cv, info) {
            cv.view.char_width = width;
            geo.char_width = width;
        }
    }
    if let Some(size) = info.get_node_size(info.get_hit_node()) {
        if size.height > 0.0 {
            cv.view.visible_lines = ((size.height / geo.line_height.max(1.0)).floor() as u32).max(1);
            geo.fit_lines = cv.view.visible_lines;
        }
        if size.width > 0.0 {
            let bar = if geo.vbar.is_some() { SCROLL_BAR_PX } else { 0.0 };
            let text_width = (size.width - geo.text_left - bar).max(0.0);
            cv.view.visible_columns = ((text_width / geo.char_width.max(0.1)).floor() as u32).max(1);
            geo.fit_columns = cv.view.visible_columns;
        }
    }
}

/// The width of one column of the view's face: 64 zeros measured in the
/// window's fonts, divided by 64 (`None` when the window cannot measure).
#[cfg(feature = "std")]
fn measured_char_width(cv: &CodeView, info: &CallbackInfo) -> Option<f32> {
    let probe = crate::widgets::widget_p_with_text(AzString::from("0".repeat(64))).with_css_props(
        CssPropertyWithConditionsVec::from_vec(alloc::vec![
            simple(CssProperty::const_font_family(MONO_FAMILY)),
            simple(CssProperty::const_font_size(StyleFontSize::px(cv.font_size))),
            simple(CssProperty::WhiteSpace(CssPropertyValue::Exact(StyleWhiteSpace::Pre))),
        ]),
    );
    let size = info.measure_dom_shrink_to_fit(probe, azul_core::geom::LogicalSize::new(8192.0, 1024.0));
    (size.width > 0.0).then(|| size.width / 64.0)
}

/// Without `std` the window cannot measure: the estimate stays.
#[cfg(not(feature = "std"))]
fn measured_char_width(_cv: &CodeView, _info: &CallbackInfo) -> Option<f32> {
    None
}

/// The view's text, through the data callback.
fn lines_of(cv: &CodeView) -> SourceLines<'_> {
    SourceLines {
        source: &cv.data_source,
        count: cv.line_count,
    }
}

/// `text` onto the clipboard.
fn to_clipboard(info: &mut CallbackInfo, text: &str) {
    info.set_clipboard_content(crate::managers::selection::ClipboardContent {
        plain_text: AzString::from(text),
        styled_runs: crate::managers::selection::StyledTextRunVec::from_const_slice(&[]),
        html: azul_css::OptionString::None,
    });
}

/// The keys (see the module's KEYBOARD).
extern "C" fn on_key(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut cv, mut geo)) = shared_of(&mut data) else {
        return Update::DoNothing;
    };
    let ks = info.get_current_keyboard_state();
    let Some(key) = ks.current_virtual_keycode.into_option() else {
        return Update::DoNothing;
    };
    let mac = azul_core::window::mac_shortcut_conventions();
    let primary = ks.primary_down();
    let alt = ks.alt_down();
    let mods = Mods {
        shift: ks.shift_down(),
        primary,
        word: if mac { alt } else { ks.ctrl_down() },
        line: mac && primary,
        alt,
    };
    // Ctrl/Cmd+V: the Paste event brings the clipboard's text.
    if primary && key == VirtualKeyCode::V {
        return Update::DoNothing;
    }
    measure(&mut cv, &mut geo, &info);
    let event = key_event(&cv, &lines_of(&cv), key, mods);
    let Some(event) = event else {
        return Update::DoNothing;
    };
    // The key is the view's: no spatial navigation, no default action.
    info.prevent_default();
    if !event.text.as_str().is_empty() {
        to_clipboard(&mut info, event.text.as_str());
    }
    deliver(&mut data, &cv, info, event)
}

/// Text typed into the view: into every cursor. The view node holds no
/// text of its own, so the engine's own insertion is cancelled.
extern "C" fn on_text(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut cv, mut geo)) = shared_of(&mut data) else {
        return Update::DoNothing;
    };
    let Some(inserted) = info
        .get_text_changeset()
        .map(|c| String::from(c.inserted_text.as_str()))
    else {
        return Update::DoNothing;
    };
    info.prevent_default();
    let typed: String = inserted.chars().filter(|c| !c.is_control()).collect();
    if typed.is_empty() {
        return Update::DoNothing;
    }
    measure(&mut cv, &mut geo, &info);
    let event = typed_event(&cv, &lines_of(&cv), &typed);
    match event {
        Some(event) => deliver(&mut data, &cv, info, event),
        None => Update::DoNothing,
    }
}

/// Ctrl/Cmd+V: the clipboard's text at every cursor.
extern "C" fn on_paste(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut cv, mut geo)) = shared_of(&mut data) else {
        return Update::DoNothing;
    };
    let text = info
        .get_clipboard_content()
        .map(|c| String::from(c.plain_text.as_str()));
    info.prevent_default();
    let Some(text) = text else {
        return Update::DoNothing;
    };
    measure(&mut cv, &mut geo, &info);
    let event = paste_event(&cv, &lines_of(&cv), &text);
    match event {
        Some(event) => deliver(&mut data, &cv, info, event),
        None => Update::DoNothing,
    }
}

/// A press: the caret, a line, the scroll bar; the view takes the focus
/// and, for a drag, the pointer.
extern "C" fn on_mouse_down(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut cv, mut geo)) = shared_of(&mut data) else {
        return Update::DoNothing;
    };
    let Some((x, y)) = crate::widgets::cell_grid::cursor_in(&info) else {
        return Update::DoNothing;
    };
    measure(&mut cv, &mut geo, &info);
    let ks = info.get_current_keyboard_state();
    let mods = Mods {
        shift: ks.shift_down(),
        alt: ks.alt_down(),
        ..Mods::default()
    };
    let event = {
        let lines = lines_of(&cv);
        let hit = hit_test(&cv, &geo, &lines, x, y);
        press_event(&cv, &geo, &lines, hit, mods, y)
    };
    let Some(event) = event else {
        return Update::DoNothing;
    };
    let node = info.get_hit_node();
    info.set_focus(azul_core::callbacks::FocusTarget::Id(node));
    if event.view.drag != CodeViewDragKind::None {
        info.capture_pointer(node);
    }
    deliver(&mut data, &cv, info, event)
}

/// A move while a drag is in progress.
extern "C" fn on_mouse_move(mut data: RefAny, info: CallbackInfo) -> Update {
    let Some((cv, geo)) = shared_of(&mut data) else {
        return Update::DoNothing;
    };
    if cv.view.drag == CodeViewDragKind::None {
        return Update::DoNothing;
    }
    let Some((x, y)) = crate::widgets::cell_grid::cursor_in(&info) else {
        return Update::DoNothing;
    };
    let event = drag_event(&cv, &geo, &lines_of(&cv), x, y);
    match event {
        Some(event) => deliver(&mut data, &cv, info, event),
        None => Update::DoNothing,
    }
}

/// The drag ends.
extern "C" fn on_mouse_up(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((cv, _)) = shared_of(&mut data) else {
        return Update::DoNothing;
    };
    let Some(event) = drag_end(&cv) else {
        return Update::DoNothing;
    };
    info.release_pointer_capture();
    deliver(&mut data, &cv, info, event)
}

/// A double-click selects the word under the pointer.
extern "C" fn on_double_click(mut data: RefAny, info: CallbackInfo) -> Update {
    let Some((cv, geo)) = shared_of(&mut data) else {
        return Update::DoNothing;
    };
    let Some((x, y)) = crate::widgets::cell_grid::cursor_in(&info) else {
        return Update::DoNothing;
    };
    let event = {
        let lines = lines_of(&cv);
        let hit = hit_test(&cv, &geo, &lines, x, y);
        double_click_event(&cv, &lines, hit)
    };
    match event {
        Some(event) => deliver(&mut data, &cv, info, event),
        None => Update::DoNothing,
    }
}

/// The wheel scrolls by whole lines (Shift: columns). The view IS the
/// scroll surface, so the page under it does not scroll as well.
extern "C" fn on_wheel(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((cv, geo)) = shared_of(&mut data) else {
        return Update::DoNothing;
    };
    // The offset change the wheel asks for (+y = down: rows forward), not
    // the raw delta (+y = the wheel turned up), which scrolled backwards.
    let Some(delta) = info.get_wheel_scroll_by() else {
        return Update::DoNothing;
    };
    info.prevent_default();
    info.stop_propagation();
    let shift = info.get_current_keyboard_state().shift_down();
    let (dx, dy) = if shift && delta.x.abs() < f32::EPSILON {
        (delta.y, 0.0)
    } else {
        (delta.x, delta.y)
    };
    let (rows, columns) =
        crate::widgets::cell_grid::take_wheel(dx, dy, geo.line_height.max(1.0), geo.char_width.max(1.0));
    if rows == 0 && columns == 0 {
        return Update::DoNothing;
    }
    match scroll_event(&cv, rows, columns) {
        Some(event) => deliver(&mut data, &cv, info, event),
        None => Update::DoNothing,
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

    /// The sample as a window shows it: the view node with its lines in
    /// the place of the `VirtualView` that renders them - what the lint
    /// manifest checks, the lines' inks read on the view's own ground.
    pub(crate) fn sample_with_lines() -> Dom {
        let mut dom = sample().dom();
        dom.set_children(DomVec::from_vec(alloc::vec![lines_dom(resolve(sample()))]));
        dom
    }
}
