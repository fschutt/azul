//! Summary list widget - a list of summaries, each a title line, a second
//! line and a preview: a mail window's middle pane (Outlook 2010's message
//! list), a notes app's note list. A search field with scope buttons over it,
//! the sort header ("Arrange by: Date" and the "Newest on top" toggle), then
//! the rows, grouped under headers ("Today", "Yesterday"), each row an icon
//! beside its title in bold when unread, the subject, a preview line, the
//! date at the right and a mark (a flag, or a pin). Named `MessageList` until
//! wave 7 (DEDUP_WIDGETS_API F20); its fields still use the mail words.
//!
//! THOUSANDS OF ROWS: the list is virtualised. The app hands it the WINDOW
//! of rows it rendered ([`SummaryList::rows`], starting at
//! [`SummaryList::first_row`] of [`SummaryList::total_rows`]) and every row's
//! height ([`SummaryList::row_height`]); the list draws spacers for the rows
//! above and below the window, and when a scroll settles it tells the app
//! which rows are now in view (`on_scroll`, [`SummaryListEventKind::Scroll`]:
//! `ListView::visible_row_range` over the scroll box's offset and size) so
//! the app rebuilds with those rows. The scroll box listens for the SETTLED
//! gesture, never the wheel, which stays the page's.
//!
//! THE APP OWNS THE SELECTION: a row carries [`SummaryRow::selected`], a
//! click or an arrow reports [`SummaryListEventKind::Select`] with the
//! modifiers held, and the shared list selection model
//! ([`crate::widgets::list_selection::ListSelection::select`], keyed by the
//! row's index in the whole list)
//! turns that into the next selection (plain: this row; Ctrl: toggle it;
//! Shift: the range from the anchor). The app stores it and rebuilds.
//!
//! KEYBOARD (WAI-ARIA APG listbox): the message rows are ONE Tab stop - the
//! selected row, or the first. Up / Down move to the neighbouring message
//! (group headers are skipped), Home / End to the first / last row of the
//! list (asking the app for a window it has not rendered), PageUp / PageDown
//! by a box's worth of rows; a plain move selects (Shift extends, Ctrl moves
//! focus alone). Enter opens the row, Delete deletes it, the flag button
//! flags it.
//!
//! Key types: [`SummaryList`], [`SummaryRow`], [`SummaryListEvent`];
//! the selection is a [`crate::widgets::list_selection::ListSelection`].

use alloc::vec::Vec;

use azul_core::{
    callbacks::{CoreCallback, CoreCallbackData, Update},
    dom::{
        Dom, DomVec, EventFilter, HoverEventFilter, IdOrClass, IdOrClass::Class, IdOrClassVec,
        TabIndex,
    },
    events::FocusEventFilter,
    refany::{OptionRefAny, RefAny},
    window::VirtualKeyCode,
};
use azul_css::{
    dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
    impl_option, impl_vec, impl_vec_clone, impl_vec_debug, impl_vec_mut,
    props::{
        basic::length::FloatValue,
        layout::{
            LayoutAlignItems, LayoutDisplay, LayoutFlexDirection, LayoutFlexGrow,
            LayoutFlexShrink, LayoutHeight, LayoutMinHeight, LayoutMinWidth, LayoutOverflow,
        },
        property::CssProperty,
        style::{StyleCursor, StyleUserSelect},
    },
    AzString, StringVec,
};

use crate::{
    callbacks::{Callback, CallbackInfo},
    widgets::{
        button::{Button, ButtonOnClickCallbackType, ButtonType},
        list_view::{scroll_settled_hook, scroll_window_of, ListView},
        roving::{self, Step},
        segmented::{Segmented, SegmentedOnChangeCallbackType, SegmentedState},
        text_input::{
            OnTextInputReturn, TextInput, TextInputOnTextInputCallbackType, TextInputState,
            TextInputValid,
        },
        tile::{TILE_COLUMN_BASE, TILE_ICON_BASE, TILE_LINE_BASE},
    },
};

static LIST_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-summary-list"))];
static TOOLBAR_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-summary-list-toolbar",
))];
static SEARCH_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-summary-list-search",
))];
static SCOPES_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-summary-list-scopes",
))];
static SORT_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-summary-list-sort",
))];
static SORT_FIELD_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-summary-list-sort-field",
))];
static SORT_DIRECTION_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-summary-list-sort-direction",
))];
static ROWS_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-summary-list-rows",
))];
static SPACER_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-summary-list-spacer",
))];
static ROW_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-summary-list-row",
))];
/// The class string of [`ROW_CLASS`], for the key handler to find the rows.
const ROW_CLASS_NAME: &str = "__azul-native-summary-list-row";
static ROW_UNREAD_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-summary-list-row-unread",
))];
static ROW_SELECTED_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-summary-list-row-selected",
))];
static GROUP_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-summary-list-group",
))];
static ICON_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-summary-list-icon",
))];
static TEXT_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-summary-list-text",
))];
static FROM_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-summary-list-from",
))];
static SUBJECT_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-summary-list-subject",
))];
static PREVIEW_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-summary-list-preview",
))];
static META_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-summary-list-meta",
))];
static DATE_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-summary-list-date",
))];
static MARKS_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-summary-list-marks",
))];
static ATTACHMENT_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-summary-list-attachment",
))];
static FLAG_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-summary-list-flag",
))];

/// What happened in the list.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SummaryListEventKind {
    /// A row was clicked or reached with the keyboard: `index`, `id`, and
    /// the modifiers held (`shift` extends the selection from the anchor,
    /// `ctrl` toggles the row) - see
    /// [`crate::widgets::list_selection::ListSelection::select`].
    Select,
    /// A row was double-clicked or Enter was pressed on it: open it.
    Open,
    /// A row's flag was clicked: flag or unflag it.
    Flag,
    /// Delete was pressed on a row: delete it.
    Delete,
    /// The "Arrange by" field was clicked: `text` is the field; the app
    /// opens a menu of fields.
    Sort,
    /// The direction toggle was clicked: reverse the order.
    SortDirection,
    /// The search box changed: `text` is its text.
    Search,
    /// A scope button was clicked: `index`.
    Scope,
    /// A scroll settled: the rows `index..end` are in view - render them.
    Scroll,
}

/// One action in the list: what, on which row, with what.
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SummaryListEvent {
    /// The search text (`Search`) or the sort field (`Sort`); empty otherwise.
    pub text: AzString,
    /// The row's id (`Select`, `Open`, `Flag`, `Delete`); 0 otherwise.
    pub id: u64,
    /// The row's index in the whole list (`Select`, `Open`, `Flag`,
    /// `Delete`), the first row in view (`Scroll`), the scope (`Scope`); 0
    /// otherwise.
    pub index: usize,
    /// One past the last row in view (`Scroll`); 0 otherwise.
    pub end: usize,
    /// What happened.
    pub kind: SummaryListEventKind,
    /// `Select`: Shift was held.
    pub shift: bool,
    /// `Select`: the primary modifier was held (Cmd on macOS, Ctrl elsewhere).
    pub ctrl: bool,
}

impl SummaryListEvent {
    /// A `kind` event on row `index` (id `id`), no text, no modifiers.
    #[must_use]
    pub const fn create(kind: SummaryListEventKind, index: usize, id: u64) -> Self {
        Self {
            text: AzString::from_const_str(""),
            id,
            index,
            end: 0,
            kind,
            shift: false,
            ctrl: false,
        }
    }
}

/// Callback invoked for an action in the list.
pub type SummaryListOnEventCallbackType =
    extern "C" fn(RefAny, CallbackInfo, SummaryListEvent) -> Update;
impl_widget_callback!(
    SummaryListOnEvent,
    OptionSummaryListOnEvent,
    SummaryListOnEventCallback,
    SummaryListOnEventCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        SummaryListOnEventCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: SUMMARY_LIST_ON_EVENT_INVOKER,
    invoker_ty:     AzSummaryListOnEventCallbackInvoker,
    thunk_fn:       az_summary_list_on_event_callback_thunk,
    setter_fn:      AzApp_setSummaryListOnEventCallbackInvoker,
    from_handle_fn: AzSummaryListOnEventCallback_createFromHostHandle,
    from_handle_byref_fn: AzSummaryListOnEventCallback_createFromHostHandleByref,
    extra_args:     [ event: SummaryListEvent ],
}

/// What a row of the list is.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum SummaryRowKind {
    /// A message.
    #[default]
    Message,
    /// A group header ("Today", "Last week"): its `subject` is the title;
    /// it is not selectable and the arrow keys skip it.
    Group,
}

/// What the mark at the end of a message row stands for: mail's follow-up
/// flag, a note's pin, or nothing. A press on a mark reports
/// [`SummaryListEventKind::Flag`] whatever it stands for; the mark names
/// what the press does.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum SummaryListMark {
    /// The follow-up flag on every row ("Flag" / "Unflag"): a mail list.
    #[default]
    Flag,
    /// The pin on the PINNED rows only ("Unpin"): a notes list, whose
    /// notes are pinned by a command of the app. A row's `flagged` is its
    /// pinned state.
    Pin,
    /// No mark on any row.
    None,
}

impl SummaryListMark {
    /// The glyph of a row's mark (`Dom::create_icon` name), set (`on`) or
    /// not; empty when such a row carries no mark.
    #[must_use]
    pub const fn icon(self, on: bool) -> &'static str {
        match (self, on) {
            (Self::Flag, true) => "flag",
            (Self::Flag, false) => "outlined_flag",
            (Self::Pin, true) => "push_pin",
            (Self::Pin, false) | (Self::None, _) => "",
        }
    }

    /// The mark's name: what a press on it does ("Unflag", "Pin"); empty
    /// when such a row carries no mark.
    #[must_use]
    pub const fn name(self, on: bool) -> &'static str {
        match (self, on) {
            (Self::Flag, true) => "Unflag",
            (Self::Flag, false) => "Flag",
            (Self::Pin, true) => "Unpin",
            (Self::Pin, false) | (Self::None, _) => "",
        }
    }
}

/// One row of the list: a message, or a group header.
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SummaryRow {
    /// The message's id, reported back with every action on the row.
    pub id: u64,
    /// The sender ("Google Mail-Team").
    pub from: AzString,
    /// The subject; a group header's title.
    pub subject: AzString,
    /// The first line of the body, or empty for none.
    pub preview: AzString,
    /// The date as the app writes it ("Mi 21:12", "30.09.2026").
    pub date: AzString,
    /// The row's glyph (a `Dom::create_icon` name: "mail", "drafts",
    /// "reply"), or empty for none.
    pub icon: AzString,
    /// A message or a group header.
    pub kind: SummaryRowKind,
    /// Not read yet: the sender and subject in bold.
    pub unread: bool,
    /// Flagged for follow-up: the flag is filled.
    pub flagged: bool,
    /// Carries an attachment: the paper clip.
    pub has_attachment: bool,
    /// Drawn and announced as selected.
    pub selected: bool,
}

impl SummaryRow {
    /// A read, unflagged message `id` from `from` about `subject`.
    #[must_use]
    pub fn create(id: u64, from: AzString, subject: AzString) -> Self {
        Self {
            id,
            from,
            subject,
            preview: AzString::from_const_str(""),
            date: AzString::from_const_str(""),
            icon: AzString::from_const_str(""),
            kind: SummaryRowKind::Message,
            unread: false,
            flagged: false,
            has_attachment: false,
            selected: false,
        }
    }

    /// A group header titled `title`.
    #[must_use]
    pub fn create_group(title: AzString) -> Self {
        let mut row = Self::create(0, AzString::from_const_str(""), title);
        row.kind = SummaryRowKind::Group;
        row
    }

    /// The preview line.
    #[must_use]
    pub fn with_preview(mut self, preview: AzString) -> Self {
        self.preview = preview;
        self
    }

    /// The date text.
    #[must_use]
    pub fn with_date(mut self, date: AzString) -> Self {
        self.date = date;
        self
    }

    /// The row's glyph.
    #[must_use]
    pub fn with_icon(mut self, icon: AzString) -> Self {
        self.icon = icon;
        self
    }

    /// Not read yet.
    #[must_use]
    pub const fn with_unread(mut self, unread: bool) -> Self {
        self.unread = unread;
        self
    }

    /// Flagged for follow-up.
    #[must_use]
    pub const fn with_flagged(mut self, flagged: bool) -> Self {
        self.flagged = flagged;
        self
    }

    /// Carries an attachment.
    #[must_use]
    pub const fn with_attachment(mut self, has_attachment: bool) -> Self {
        self.has_attachment = has_attachment;
        self
    }

    /// Selected.
    #[must_use]
    pub const fn with_selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
}

impl_option!(
    SummaryRow,
    OptionSummaryRow,
    copy = false,
    [Debug, Clone, PartialEq, Eq]
);
impl_vec!(
    SummaryRow,
    SummaryRowVec,
    SummaryRowVecDestructor,
    SummaryRowVecDestructorType,
    SummaryRowVecSlice,
    OptionSummaryRow
);
impl_vec_clone!(SummaryRow, SummaryRowVec, SummaryRowVecDestructor);
impl_vec_debug!(SummaryRow, SummaryRowVec);
impl_vec_mut!(SummaryRow, SummaryRowVec);

/// The message list: a search row, the sort header and the rows in view.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct SummaryList {
    /// The rows in view (the WINDOW), the first of them being row
    /// `first_row` of the whole list.
    pub rows: SummaryRowVec,
    /// The scope buttons over the rows ("All", "Unread"), or empty for
    /// none.
    pub scopes: StringVec,
    /// The search box's text.
    pub search: AzString,
    /// The search box's prompt when it is empty ("Search Inbox (Ctrl+E)").
    pub search_placeholder: AzString,
    /// The sort header's caption ("Arrange by:").
    pub sort_label: AzString,
    /// The field the rows are sorted by ("Date").
    pub sort_field: AzString,
    /// The direction toggle's text ("Newest on top").
    pub sort_direction_label: AzString,
    /// A row was clicked or reached with the keyboard.
    pub on_select: OptionSummaryListOnEvent,
    /// A row was double-clicked or Enter pressed on it.
    pub on_open: OptionSummaryListOnEvent,
    /// A row's flag was clicked.
    pub on_flag: OptionSummaryListOnEvent,
    /// Delete was pressed on a row.
    pub on_delete: OptionSummaryListOnEvent,
    /// The sort field or the direction toggle was clicked.
    pub on_sort: OptionSummaryListOnEvent,
    /// The search box changed.
    pub on_search: OptionSummaryListOnEvent,
    /// A scope button was clicked.
    pub on_scope: OptionSummaryListOnEvent,
    /// A scroll settled: the rows in view changed.
    pub on_scroll: OptionSummaryListOnEvent,
    /// The whole list's row count (group headers included).
    pub total_rows: usize,
    /// The index of `rows[0]` in the whole list.
    pub first_row: usize,
    /// Every row's height in px (group headers too): what the spacers and
    /// the scroll window are computed from.
    pub row_height: usize,
    /// The active scope.
    pub scope: usize,
    /// What the mark at a row's end stands for (the flag by default).
    pub mark: SummaryListMark,
    /// The widget theme this widget is PINNED to (`with_theme`), or `None`
    /// to follow the app theme.
    pub theme: crate::widgets::themes::OptionUiTheme,
    /// Newest first ("Newest on top"); the toggle shows the other text
    /// when unset.
    pub sort_descending: bool,
}

/// What a theme decides about a message list: the SKIN of each part, laid
/// over the part's base (the list's structure, the same in every theme:
/// `SUMMARY_LIST_*_BASE`) by [`build`].
pub(crate) struct SummaryListLook {
    /// The list.
    pub list: Vec<CssPropertyWithConditions>,
    /// The search row.
    pub toolbar: Vec<CssPropertyWithConditions>,
    /// The box around the search input.
    pub search: Vec<CssPropertyWithConditions>,
    /// The box around the scope buttons.
    pub scopes: Vec<CssPropertyWithConditions>,
    /// The sort header.
    pub sort: Vec<CssPropertyWithConditions>,
    /// The rows box.
    pub rows: Vec<CssPropertyWithConditions>,
    /// A message row at rest, hover and focus included.
    pub row: Vec<CssPropertyWithConditions>,
    /// Added to an unread row.
    pub row_unread: Vec<CssPropertyWithConditions>,
    /// Added to a selected row.
    pub row_selected: Vec<CssPropertyWithConditions>,
    /// A group header row.
    pub group: Vec<CssPropertyWithConditions>,
    /// The row's glyph.
    pub icon: Vec<CssPropertyWithConditions>,
    /// The sender line.
    pub from: Vec<CssPropertyWithConditions>,
    /// The sender line when unread.
    pub from_unread: Vec<CssPropertyWithConditions>,
    /// The subject line.
    pub subject: Vec<CssPropertyWithConditions>,
    /// The preview line.
    pub preview: Vec<CssPropertyWithConditions>,
    /// The date.
    pub date: Vec<CssPropertyWithConditions>,
    /// The paper clip.
    pub attachment: Vec<CssPropertyWithConditions>,
    /// The box around the flag button.
    pub flag: Vec<CssPropertyWithConditions>,
    /// The theme's marker class on the list, if it has one.
    pub marker: Option<&'static str>,
}

// ---- the base: the list's structure, in every theme ----

/// The list: a column that takes its pane and lets its rows box shrink.
pub(crate) static SUMMARY_LIST_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(
        LayoutFlexDirection::Column,
    )),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
    CssPropertyWithConditions::simple(CssProperty::const_min_height(LayoutMinHeight::const_px(0))),
    CssPropertyWithConditions::simple(CssProperty::const_min_width(LayoutMinWidth::const_px(0))),
];

/// A strip of the list (the search row, the sort header, a group header):
/// one row, centred on its midline, never growing, its text never selected
/// by a drag.
pub(crate) static SUMMARY_LIST_STRIP_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(LayoutFlexDirection::Row)),
    CssPropertyWithConditions::simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_flex_shrink(LayoutFlexShrink {
        inner: FloatValue::const_new(0),
    })),
    CssPropertyWithConditions::simple(CssProperty::user_select(StyleUserSelect::None)),
];

/// The search box takes the rest of the search row.
pub(crate) static SUMMARY_LIST_SEARCH_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
    CssPropertyWithConditions::simple(CssProperty::const_min_width(LayoutMinWidth::const_px(0))),
];

/// The scope buttons keep their size.
pub(crate) static SUMMARY_LIST_FIXED_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_flex_shrink(LayoutFlexShrink {
        inner: FloatValue::const_new(0),
    })),
];

/// The rows box: the rest of the list, scrolling.
pub(crate) static SUMMARY_LIST_ROWS_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(
        LayoutFlexDirection::Column,
    )),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
    CssPropertyWithConditions::simple(CssProperty::const_min_height(LayoutMinHeight::const_px(0))),
    CssPropertyWithConditions::simple(CssProperty::const_overflow_y(LayoutOverflow::Auto)),
];

/// A spacer standing in for the rows outside the window: its height is the
/// rows' (set per list), and it never shrinks.
pub(crate) static SUMMARY_LIST_SPACER_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_flex_shrink(LayoutFlexShrink {
        inner: FloatValue::const_new(0),
    })),
];

/// A message row: the glyph, the text column and the meta column on one
/// line, a click target whose text a drag never selects.
pub(crate) static SUMMARY_LIST_ROW_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(LayoutFlexDirection::Row)),
    CssPropertyWithConditions::simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_flex_shrink(LayoutFlexShrink {
        inner: FloatValue::const_new(0),
    })),
    CssPropertyWithConditions::simple(CssProperty::const_cursor(StyleCursor::Default)),
    CssPropertyWithConditions::simple(CssProperty::user_select(StyleUserSelect::None)),
];

/// The meta column (date, clip, flag): set at the row's end, keeping its
/// size.
pub(crate) static SUMMARY_LIST_META_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(
        LayoutFlexDirection::Column,
    )),
    CssPropertyWithConditions::simple(CssProperty::const_align_items(LayoutAlignItems::End)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_flex_shrink(LayoutFlexShrink {
        inner: FloatValue::const_new(0),
    })),
];

impl SummaryList {
    /// A list of `rows`, all of them (the window is the whole list), with
    /// no scopes, no search text, sorted by "Date", newest on top.
    #[must_use]
    pub fn create(rows: SummaryRowVec) -> Self {
        let total_rows = rows.as_ref().len();
        Self {
            rows,
            scopes: StringVec::from_const_slice(&[]),
            search: AzString::from_const_str(""),
            search_placeholder: AzString::from_const_str("Search"),
            sort_label: AzString::from_const_str("Arrange by:"),
            sort_field: AzString::from_const_str("Date"),
            sort_direction_label: AzString::from_const_str("Newest on top"),
            on_select: None.into(),
            on_open: None.into(),
            on_flag: None.into(),
            on_delete: None.into(),
            on_sort: None.into(),
            on_search: None.into(),
            on_scope: None.into(),
            on_scroll: None.into(),
            total_rows,
            first_row: 0,
            row_height: 48,
            scope: 0,
            mark: SummaryListMark::Flag,
            theme: crate::widgets::themes::OptionUiTheme::None,
            sort_descending: true,
        }
    }

    /// The window: `rows` are rows `first_row..` of a list of `total_rows`.
    pub const fn set_window(&mut self, first_row: usize, total_rows: usize) {
        self.first_row = first_row;
        self.total_rows = total_rows;
    }

    /// [`Self::set_window`] for the builder chain.
    #[must_use]
    pub const fn with_window(mut self, first_row: usize, total_rows: usize) -> Self {
        self.set_window(first_row, total_rows);
        self
    }

    /// Every row's height in px.
    pub const fn set_row_height(&mut self, row_height: usize) {
        self.row_height = row_height;
    }

    /// [`Self::set_row_height`] for the builder chain.
    #[must_use]
    pub const fn with_row_height(mut self, row_height: usize) -> Self {
        self.set_row_height(row_height);
        self
    }

    /// What the mark at a row's end stands for.
    pub const fn set_mark(&mut self, mark: SummaryListMark) {
        self.mark = mark;
    }

    /// [`Self::set_mark`] for the builder chain.
    #[must_use]
    pub const fn with_mark(mut self, mark: SummaryListMark) -> Self {
        self.set_mark(mark);
        self
    }

    /// The scope buttons and the active one.
    pub fn set_scopes(&mut self, scopes: StringVec, active: usize) {
        self.scopes = scopes;
        self.scope = active;
    }

    /// [`Self::set_scopes`] for the builder chain.
    #[must_use]
    pub fn with_scopes(mut self, scopes: StringVec, active: usize) -> Self {
        self.set_scopes(scopes, active);
        self
    }

    /// The search box's text.
    pub fn set_search(&mut self, search: AzString) {
        self.search = search;
    }

    /// [`Self::set_search`] for the builder chain.
    #[must_use]
    pub fn with_search(mut self, search: AzString) -> Self {
        self.set_search(search);
        self
    }

    /// The search box's prompt.
    pub fn set_search_placeholder(&mut self, placeholder: AzString) {
        self.search_placeholder = placeholder;
    }

    /// [`Self::set_search_placeholder`] for the builder chain.
    #[must_use]
    pub fn with_search_placeholder(mut self, placeholder: AzString) -> Self {
        self.set_search_placeholder(placeholder);
        self
    }

    /// The sort header: its caption, the field and the direction.
    pub fn set_sort(&mut self, label: AzString, field: AzString, descending: bool) {
        self.sort_label = label;
        self.sort_field = field;
        self.sort_descending = descending;
    }

    /// [`Self::set_sort`] for the builder chain.
    #[must_use]
    pub fn with_sort(mut self, label: AzString, field: AzString, descending: bool) -> Self {
        self.set_sort(label, field, descending);
        self
    }

    /// The direction toggle's text.
    pub fn set_sort_direction_label(&mut self, label: AzString) {
        self.sort_direction_label = label;
    }

    /// [`Self::set_sort_direction_label`] for the builder chain.
    #[must_use]
    pub fn with_sort_direction_label(mut self, label: AzString) -> Self {
        self.set_sort_direction_label(label);
        self
    }

    /// Pin the widget theme; unset, the list follows the app theme.
    pub const fn set_theme(&mut self, theme: crate::widgets::themes::UiTheme) {
        self.theme = crate::widgets::themes::OptionUiTheme::Some(theme);
    }

    /// [`Self::set_theme`] for the builder chain.
    #[must_use]
    pub const fn with_theme(mut self, theme: crate::widgets::themes::UiTheme) -> Self {
        self.set_theme(theme);
        self
    }

    /// A row was clicked or reached with the keyboard.
    pub fn set_on_select<C: Into<SummaryListOnEventCallback>>(&mut self, data: RefAny, cb: C) {
        self.on_select = hook(data, cb);
    }

    /// [`Self::set_on_select`] for the builder chain.
    #[must_use]
    pub fn with_on_select<C: Into<SummaryListOnEventCallback>>(
        mut self,
        data: RefAny,
        cb: C,
    ) -> Self {
        self.set_on_select(data, cb);
        self
    }

    /// A row was double-clicked or Enter pressed on it.
    pub fn set_on_open<C: Into<SummaryListOnEventCallback>>(&mut self, data: RefAny, cb: C) {
        self.on_open = hook(data, cb);
    }

    /// [`Self::set_on_open`] for the builder chain.
    #[must_use]
    pub fn with_on_open<C: Into<SummaryListOnEventCallback>>(
        mut self,
        data: RefAny,
        cb: C,
    ) -> Self {
        self.set_on_open(data, cb);
        self
    }

    /// A row's flag was clicked.
    pub fn set_on_flag<C: Into<SummaryListOnEventCallback>>(&mut self, data: RefAny, cb: C) {
        self.on_flag = hook(data, cb);
    }

    /// [`Self::set_on_flag`] for the builder chain.
    #[must_use]
    pub fn with_on_flag<C: Into<SummaryListOnEventCallback>>(
        mut self,
        data: RefAny,
        cb: C,
    ) -> Self {
        self.set_on_flag(data, cb);
        self
    }

    /// Delete was pressed on a row.
    pub fn set_on_delete<C: Into<SummaryListOnEventCallback>>(&mut self, data: RefAny, cb: C) {
        self.on_delete = hook(data, cb);
    }

    /// [`Self::set_on_delete`] for the builder chain.
    #[must_use]
    pub fn with_on_delete<C: Into<SummaryListOnEventCallback>>(
        mut self,
        data: RefAny,
        cb: C,
    ) -> Self {
        self.set_on_delete(data, cb);
        self
    }

    /// The sort field or the direction toggle was clicked.
    pub fn set_on_sort<C: Into<SummaryListOnEventCallback>>(&mut self, data: RefAny, cb: C) {
        self.on_sort = hook(data, cb);
    }

    /// [`Self::set_on_sort`] for the builder chain.
    #[must_use]
    pub fn with_on_sort<C: Into<SummaryListOnEventCallback>>(
        mut self,
        data: RefAny,
        cb: C,
    ) -> Self {
        self.set_on_sort(data, cb);
        self
    }

    /// The search box changed.
    pub fn set_on_search<C: Into<SummaryListOnEventCallback>>(&mut self, data: RefAny, cb: C) {
        self.on_search = hook(data, cb);
    }

    /// [`Self::set_on_search`] for the builder chain.
    #[must_use]
    pub fn with_on_search<C: Into<SummaryListOnEventCallback>>(
        mut self,
        data: RefAny,
        cb: C,
    ) -> Self {
        self.set_on_search(data, cb);
        self
    }

    /// A scope button was clicked.
    pub fn set_on_scope<C: Into<SummaryListOnEventCallback>>(&mut self, data: RefAny, cb: C) {
        self.on_scope = hook(data, cb);
    }

    /// [`Self::set_on_scope`] for the builder chain.
    #[must_use]
    pub fn with_on_scope<C: Into<SummaryListOnEventCallback>>(
        mut self,
        data: RefAny,
        cb: C,
    ) -> Self {
        self.set_on_scope(data, cb);
        self
    }

    /// A scroll settled: the rows in view changed.
    pub fn set_on_scroll<C: Into<SummaryListOnEventCallback>>(&mut self, data: RefAny, cb: C) {
        self.on_scroll = hook(data, cb);
    }

    /// [`Self::set_on_scroll`] for the builder chain.
    #[must_use]
    pub fn with_on_scroll<C: Into<SummaryListOnEventCallback>>(
        mut self,
        data: RefAny,
        cb: C,
    ) -> Self {
        self.set_on_scroll(data, cb);
        self
    }

    /// Replaces `self` with an empty list and returns the original.
    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create(SummaryRowVec::from_const_slice(&[]));
        core::mem::swap(&mut s, self);
        s
    }

    /// The list's DOM. The look comes from the theme module
    /// (`themes::flat::summary_list` / `themes::flora::summary_list`);
    /// `None` carries both looks, each in its `@theme(<name>)` block, and
    /// the app theme picks.
    #[must_use]
    pub fn dom(self) -> Dom {
        use crate::widgets::themes::UiTheme;
        match self.theme.into_option() {
            Some(UiTheme::Flora) => crate::widgets::themes::flora::summary_list(self),
            Some(UiTheme::Flat) => crate::widgets::themes::flat::summary_list(self),
            None => crate::widgets::themes::theme_blocks::follow_app_theme(
                self,
                crate::widgets::themes::flat::summary_list,
                crate::widgets::themes::flora::summary_list,
            ),
        }
    }
}

impl Default for SummaryList {
    fn default() -> Self {
        Self::create(SummaryRowVec::from_const_slice(&[]))
    }
}

impl From<SummaryList> for Dom {
    fn from(l: SummaryList) -> Self {
        l.dom()
    }
}

/// `cb` on `data`, as an optional hook.
fn hook<C: Into<SummaryListOnEventCallback>>(data: RefAny, cb: C) -> OptionSummaryListOnEvent {
    Some(SummaryListOnEvent {
        refany: data,
        callback: cb.into(),
    })
    .into()
}

/// What every part of one list shares: the app's hooks and the window.
struct ListShared {
    on_select: OptionSummaryListOnEvent,
    on_open: OptionSummaryListOnEvent,
    on_flag: OptionSummaryListOnEvent,
    on_delete: OptionSummaryListOnEvent,
    on_sort: OptionSummaryListOnEvent,
    on_search: OptionSummaryListOnEvent,
    on_scope: OptionSummaryListOnEvent,
    on_scroll: OptionSummaryListOnEvent,
    sort_field: AzString,
    total_rows: usize,
    first_row: usize,
    row_height: usize,
}

/// Hands `event` to `hook`.
fn fire(hook: &OptionSummaryListOnEvent, info: CallbackInfo, event: SummaryListEvent) -> Update {
    match hook.as_ref() {
        Some(SummaryListOnEvent { callback, refany }) => callback.invoke(refany.clone(), info, event),
        None => Update::DoNothing,
    }
}

/// A row's payload: which row it is, and the list's shared state.
struct RowData {
    /// The row's index in the whole list.
    index: usize,
    id: u64,
    shared: RefAny,
}

/// `(index, id)` of the row whose dataset is `node`'s, if it is a message
/// row of this list.
fn row_identity(info: &mut CallbackInfo, node: azul_core::dom::DomNodeId) -> Option<(usize, u64)> {
    let mut dataset = info.get_dataset(node)?;
    let row = dataset.downcast_ref::<RowData>()?;
    Some((row.index, row.id))
}

/// A click on a row: select it, with the modifiers held. The row becomes
/// the rows' one Tab stop (the click already focused it).
extern "C" fn on_row_click(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let (index, id, mut shared) = {
        let Some(row) = data.downcast_ref::<RowData>() else {
            return Update::DoNothing;
        };
        (row.index, row.id, row.shared.clone())
    };
    let clicked = info.get_hit_node();
    if let Some(rows_box) = info.get_parent(clicked) {
        let rows = roving::items_of(&info, rows_box, ROW_CLASS_NAME);
        if let Some(at) = rows.iter().position(|n| *n == clicked) {
            roving::set_stop(&mut info, &rows, at);
        }
    }
    let ks = info.get_current_keyboard_state();
    let (shift, ctrl) = (ks.shift_down(), ks.primary_down());
    let mut event = SummaryListEvent::create(SummaryListEventKind::Select, index, id);
    event.shift = shift;
    event.ctrl = ctrl;
    let Some(shared) = shared.downcast_ref::<ListShared>() else {
        return Update::DoNothing;
    };
    fire(&shared.on_select, info, event)
}

/// A double-click on a row: open it.
extern "C" fn on_row_double_click(mut data: RefAny, info: CallbackInfo) -> Update {
    let (index, id, mut shared) = {
        let Some(row) = data.downcast_ref::<RowData>() else {
            return Update::DoNothing;
        };
        (row.index, row.id, row.shared.clone())
    };
    let Some(shared) = shared.downcast_ref::<ListShared>() else {
        return Update::DoNothing;
    };
    fire(
        &shared.on_open,
        info,
        SummaryListEvent::create(SummaryListEventKind::Open, index, id),
    )
}

/// The flag of a row was clicked: flag it. The click stops at the flag, so
/// the row under it is not selected.
extern "C" fn on_flag_click(mut data: RefAny, mut info: CallbackInfo) -> Update {
    info.stop_propagation();
    let (index, id, mut shared) = {
        let Some(row) = data.downcast_ref::<RowData>() else {
            return Update::DoNothing;
        };
        (row.index, row.id, row.shared.clone())
    };
    let Some(shared) = shared.downcast_ref::<ListShared>() else {
        return Update::DoNothing;
    };
    fire(
        &shared.on_flag,
        info,
        SummaryListEvent::create(SummaryListEventKind::Flag, index, id),
    )
}

/// The keys on a message row (see the module's KEYBOARD).
extern "C" fn on_row_key(mut data: RefAny, mut info: CallbackInfo) -> Update {
    use VirtualKeyCode as K;

    let (index, id, mut shared) = {
        let Some(row) = data.downcast_ref::<RowData>() else {
            return Update::DoNothing;
        };
        (row.index, row.id, row.shared.clone())
    };
    let ks = info.get_current_keyboard_state();
    if ks.alt_down() {
        return Update::DoNothing;
    }
    let Some(key) = ks.current_virtual_keycode.into_option() else {
        return Update::DoNothing;
    };
    let shift = ks.shift_down();
    let ctrl = ks.primary_down();
    let (on_select, on_open, on_delete, total_rows, first_row, row_height) = {
        let Some(shared) = shared.downcast_ref::<ListShared>() else {
            return Update::DoNothing;
        };
        (
            shared.on_select.clone(),
            shared.on_open.clone(),
            shared.on_delete.clone(),
            shared.total_rows,
            shared.first_row,
            shared.row_height,
        )
    };

    match key {
        K::Return | K::NumpadEnter => {
            info.prevent_default();
            return fire(
                &on_open,
                info,
                SummaryListEvent::create(SummaryListEventKind::Open, index, id),
            );
        }
        K::Delete | K::Back => {
            info.prevent_default();
            return fire(
                &on_delete,
                info,
                SummaryListEvent::create(SummaryListEventKind::Delete, index, id),
            );
        }
        K::Up | K::Down | K::Home | K::End | K::PageUp | K::PageDown => {}
        _ => return Update::DoNothing,
    }

    // The message rows of the box, in order (group headers are not among
    // them: they carry another class).
    let focused = info.get_hit_node();
    let Some(rows_box) = info.get_parent(focused) else {
        return Update::DoNothing;
    };
    let rows = roving::items_of(&info, rows_box, ROW_CLASS_NAME);
    let Some(current) = rows.iter().position(|n| *n == focused) else {
        return Update::DoNothing;
    };
    // A page: the rows the box shows at once.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_precision_loss)]
    let page = {
        let height = info.get_node_size(rows_box).map_or(0.0, |s| s.height);
        ((height / row_height.max(1) as f32).floor() as usize).max(1)
    };
    let last = rows.len() - 1;
    let target = match key {
        K::Up => roving::step_target(current, rows.len(), Step::Previous, false),
        K::Down => roving::step_target(current, rows.len(), Step::Next, false),
        K::Home => Some(0),
        K::End => Some(last),
        K::PageUp => Some(current.saturating_sub(page)),
        K::PageDown => Some((current + page).min(last)),
        _ => None,
    };
    // The key is the list's either way: spatial navigation must not walk
    // out of it from an end.
    info.prevent_default();
    let Some(target) = target else {
        return Update::DoNothing;
    };

    // Home, End and a page past the window's edge aim at rows the app has
    // not rendered: ask it for them (a selection there rebuilds the window).
    let beyond = match key {
        K::Home if first_row > 0 => Some(0),
        K::End if first_row + rows.len() < total_rows => Some(total_rows.saturating_sub(1)),
        K::PageUp if target == current && index > 0 => Some(index.saturating_sub(page)),
        K::PageDown if target == current && index + 1 < total_rows => {
            Some((index + page).min(total_rows.saturating_sub(1)))
        }
        _ => None,
    };
    if let Some(index) = beyond {
        if ctrl {
            return Update::DoNothing;
        }
        let mut event = SummaryListEvent::create(SummaryListEventKind::Select, index, 0);
        event.shift = shift;
        return fire(&on_select, info, event);
    }
    if target == current {
        return Update::DoNothing;
    }

    // Moved BEFORE the app hears the selection, so a focus it asks for wins.
    roving::move_stop(&mut info, &rows, target);
    if ctrl {
        // Ctrl + arrow: focus moves, the selection stays.
        return Update::DoNothing;
    }
    let (target_index, target_id) = row_identity(&mut info, rows[target]).unwrap_or((0, 0));
    if !shift {
        roving::announce_chosen(
            &mut info,
            &rows,
            target,
            azul_core::a11y::AccessibilityState::Selected,
            None,
        );
    }
    let mut event = SummaryListEvent::create(SummaryListEventKind::Select, target_index, target_id);
    event.shift = shift;
    fire(&on_select, info, event)
}

/// The sort field was clicked: the app opens the field menu.
extern "C" fn on_sort_field(mut data: RefAny, info: CallbackInfo) -> Update {
    let Some(shared) = data.downcast_ref::<ListShared>() else {
        return Update::DoNothing;
    };
    let mut event = SummaryListEvent::create(SummaryListEventKind::Sort, 0, 0);
    event.text = shared.sort_field.clone();
    fire(&shared.on_sort, info, event)
}

/// The direction toggle was clicked.
extern "C" fn on_sort_direction(mut data: RefAny, info: CallbackInfo) -> Update {
    let Some(shared) = data.downcast_ref::<ListShared>() else {
        return Update::DoNothing;
    };
    fire(
        &shared.on_sort,
        info,
        SummaryListEvent::create(SummaryListEventKind::SortDirection, 0, 0),
    )
}

/// The search box changed.
extern "C" fn on_search_text(
    mut data: RefAny,
    info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    let update = match data.downcast_ref::<ListShared>() {
        Some(shared) => {
            let mut event = SummaryListEvent::create(SummaryListEventKind::Search, 0, 0);
            event.text = AzString::from(state.get_text());
            fire(&shared.on_search, info, event)
        }
        None => Update::DoNothing,
    };
    OnTextInputReturn {
        update,
        valid: TextInputValid::Yes,
    }
}

/// A scope button was clicked.
extern "C" fn on_scope_change(mut data: RefAny, info: CallbackInfo, state: SegmentedState) -> Update {
    let Some(shared) = data.downcast_ref::<ListShared>() else {
        return Update::DoNothing;
    };
    fire(
        &shared.on_scope,
        info,
        SummaryListEvent::create(SummaryListEventKind::Scope, state.selected_index, 0),
    )
}

/// A scroll over the rows box settled: which rows are in view now.
extern "C" fn on_rows_scroll_settled(mut data: RefAny, info: CallbackInfo) -> Update {
    let Some(shared) = data.downcast_ref::<ListShared>() else {
        return Update::DoNothing;
    };
    let (offset, size) = scroll_window_of(&info, info.get_hit_node());
    #[allow(clippy::cast_precision_loss)]
    let (first, end) = ListView::visible_row_range(
        offset.y,
        size.height,
        shared.row_height as f32,
        shared.total_rows,
    );
    let mut event = SummaryListEvent::create(SummaryListEventKind::Scroll, first, 0);
    event.end = end;
    fire(&shared.on_scroll, info, event)
}

/// A click hook on a row part.
fn click(event: EventFilter, cb: extern "C" fn(RefAny, CallbackInfo) -> Update, data: RefAny) -> CoreCallbackData {
    CoreCallbackData {
        event,
        callback: CoreCallback {
            cb: cb as usize,
            ctx: OptionRefAny::None,
        },
        refany: data,
    }
}

/// A spacer `rows` rows tall.
#[allow(clippy::cast_possible_wrap)]
fn spacer(rows: usize, row_height: usize) -> Dom {
    let mut style = SUMMARY_LIST_SPACER_BASE.to_vec();
    style.push(CssPropertyWithConditions::simple(CssProperty::const_height(
        LayoutHeight::const_px((rows * row_height) as isize),
    )));
    Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(SPACER_CLASS))
        .with_css_props(CssPropertyWithConditionsVec::from_vec(style))
}

/// The list's DOM in `look`: list [toolbar [search, scopes?], sort [label,
/// field, direction], rows [spacer, row.., spacer]]. Every part is its base
/// (the structure), then the look's skin; the search box, the scope buttons
/// and the sort buttons are the toolkit's own widgets, pinned to the list's
/// theme (or following the app theme with it).
#[allow(clippy::too_many_lines)]
pub(crate) fn build(list: SummaryList, look: &SummaryListLook) -> Dom {
    let part = |base: &[CssPropertyWithConditions], skin: &[CssPropertyWithConditions]| {
        CssPropertyWithConditionsVec::from_vec(crate::widgets::themes::decl::on_base(base, skin))
    };
    let SummaryList {
        rows,
        scopes,
        search,
        search_placeholder,
        sort_label,
        sort_field,
        sort_direction_label,
        on_select,
        on_open,
        on_flag,
        on_delete,
        on_sort,
        on_search,
        on_scope,
        on_scroll,
        total_rows,
        first_row,
        row_height,
        scope,
        mark,
        theme,
        sort_descending,
    } = list;
    let theme = theme.into_option();
    let row_height = row_height.max(1);
    let has_scroll_hook = on_scroll.is_some();
    let shared = RefAny::new(ListShared {
        on_select,
        on_open,
        on_flag,
        on_delete,
        on_sort,
        on_search,
        on_scope,
        on_scroll,
        sort_field: sort_field.clone(),
        total_rows,
        first_row,
        row_height,
    });

    // The search row: the search box, then the scope buttons.
    let mut search_input = TextInput::create_search()
        .with_text(search)
        .with_placeholder(search_placeholder)
        .with_on_text_input(
            shared.clone(),
            on_search_text as TextInputOnTextInputCallbackType,
        );
    if let Some(theme) = theme {
        search_input = search_input.with_theme(theme);
    }
    let mut toolbar = alloc::vec![Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(SEARCH_CLASS))
        .with_css_props(part(SUMMARY_LIST_SEARCH_BASE, &look.search))
        .with_child(search_input.dom())];
    if !scopes.as_ref().is_empty() {
        let mut segmented = Segmented::create(scopes)
            .with_selected_index(scope)
            .with_on_change(shared.clone(), on_scope_change as SegmentedOnChangeCallbackType);
        if let Some(theme) = theme {
            segmented = segmented.with_theme(theme);
        }
        toolbar.push(
            Dom::create_div()
                .with_ids_and_classes(IdOrClassVec::from_const_slice(SCOPES_CLASS))
                .with_css_props(part(SUMMARY_LIST_FIXED_BASE, &look.scopes))
                .with_child(segmented.dom()),
        );
    }
    let toolbar = Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(TOOLBAR_CLASS))
        .with_css_props(part(SUMMARY_LIST_STRIP_BASE, &look.toolbar))
        .with_children(DomVec::from_vec(toolbar));

    // The sort header: the caption, the field, the direction toggle.
    let link = |label: AzString, icon: &'static str, cb: ButtonOnClickCallbackType| {
        let mut b = Button::with_type(label, ButtonType::Link).with_on_click(shared.clone(), cb);
        if !icon.is_empty() {
            b = b.with_trailing_icon(AzString::from_const_str(icon));
        }
        if let Some(theme) = theme {
            b = b.with_theme(theme);
        }
        b.dom()
    };
    // The toggle shows the order it stands for: an arrow down for newest
    // (or A) on top, up for the reverse.
    let direction_icon = if sort_descending {
        "arrow_downward"
    } else {
        "arrow_upward"
    };
    let sort = Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(SORT_CLASS))
        .with_css_props(part(SUMMARY_LIST_STRIP_BASE, &look.sort))
        .with_children(DomVec::from_vec(alloc::vec![
            crate::widgets::widget_p_with_text(sort_label)
                .with_css_props(part(TILE_LINE_BASE, &[])),
            Dom::create_div()
                .with_ids_and_classes(IdOrClassVec::from_const_slice(SORT_FIELD_CLASS))
                .with_css_props(part(SUMMARY_LIST_FIXED_BASE, &[]))
                .with_child(link(sort_field, "", on_sort_field)),
            Dom::create_div()
                .with_ids_and_classes(IdOrClassVec::from_const_slice(SORT_DIRECTION_CLASS))
                .with_css_props(part(SUMMARY_LIST_SEARCH_BASE, &[]))
                .with_child(link(sort_direction_label, direction_icon, on_sort_direction)),
        ]));

    // The rows: the window between two spacers. The selected row (or the
    // first message) is the rows' one Tab stop.
    let messages: Vec<usize> = rows
        .as_ref()
        .iter()
        .enumerate()
        .filter(|(_, r)| r.kind == SummaryRowKind::Message)
        .map(|(i, _)| i)
        .collect();
    let stop = messages
        .iter()
        .position(|i| rows.as_ref()[*i].selected)
        .unwrap_or(0);
    let mut row_doms: Vec<Dom> = Vec::with_capacity(rows.as_ref().len() + 2);
    row_doms.push(spacer(first_row, row_height));
    let mut message_at = 0usize;
    let window = rows.as_ref().len();
    for (i, row) in rows.into_library_owned_vec().into_iter().enumerate() {
        let index = first_row + i;
        if row.kind == SummaryRowKind::Group {
            row_doms.push(
                Dom::create_div()
                    .with_ids_and_classes(IdOrClassVec::from_const_slice(GROUP_CLASS))
                    .with_css_props(part(SUMMARY_LIST_STRIP_BASE, &look.group))
                    .with_accessibility_info(azul_core::a11y::AccessibilityInfo {
                        role: azul_core::a11y::AccessibilityRole::Grouping,
                        accessibility_name: Some(row.subject.clone()).into(),
                        ..Default::default()
                    })
                    .with_child(
                        crate::widgets::widget_p_with_text(row.subject)
                            .with_css_props(part(TILE_LINE_BASE, &[])),
                    ),
            );
            continue;
        }
        let SummaryRow {
            id,
            from,
            subject,
            preview,
            date,
            icon,
            kind: _,
            unread,
            flagged,
            has_attachment,
            selected,
        } = row;
        let data = RefAny::new(RowData {
            index,
            id,
            shared: shared.clone(),
        });

        let icon_node = if icon.as_str().is_empty() {
            Dom::create_div()
        } else {
            Dom::create_icon(icon)
        }
        .with_ids_and_classes(IdOrClassVec::from_const_slice(ICON_CLASS))
        .with_css_props(part(TILE_ICON_BASE, &look.icon));

        let line = |text: AzString, class: &'static [IdOrClass], skin: &[CssPropertyWithConditions]| {
            crate::widgets::widget_p_with_text(text)
                .with_ids_and_classes(IdOrClassVec::from_const_slice(class))
                .with_css_props(part(TILE_LINE_BASE, skin))
        };
        let mut from_skin = look.from.clone();
        if unread {
            from_skin.extend(look.from_unread.iter().cloned());
        }
        let mut text = alloc::vec![
            line(from.clone(), FROM_CLASS, &from_skin),
            line(subject.clone(), SUBJECT_CLASS, &look.subject),
        ];
        if !preview.as_str().is_empty() {
            text.push(line(preview, PREVIEW_CLASS, &look.preview));
        }
        let text = Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(TEXT_CLASS))
            .with_css_props(part(TILE_COLUMN_BASE, &[]))
            .with_children(DomVec::from_vec(text));

        // The meta column: the date over the clip and the flag.
        let mut marks: Vec<Dom> = Vec::with_capacity(2);
        if has_attachment {
            marks.push(
                Dom::create_icon(AzString::from_const_str("attach_file"))
                    .with_ids_and_classes(IdOrClassVec::from_const_slice(ATTACHMENT_CLASS))
                    .with_css_props(part(SUMMARY_LIST_FIXED_BASE, &look.attachment)),
            );
        }
        // The mark (the flag, or a pinned note's pin): a row of a kind the
        // mark leaves bare carries none.
        let mark_icon = mark.icon(flagged);
        if !mark_icon.is_empty() {
            let mut flag = Button::create(AzString::from_const_str(""))
                .with_icon(AzString::from_const_str(mark_icon))
                .with_on_click(data.clone(), on_flag_click as ButtonOnClickCallbackType);
            // The mark's name says what a press does.
            flag.alt = AzString::from_const_str(mark.name(flagged));
            if let Some(theme) = theme {
                flag = flag.with_theme(theme);
            }
            marks.push(
                Dom::create_div()
                    .with_ids_and_classes(IdOrClassVec::from_const_slice(FLAG_CLASS))
                    .with_css_props(part(SUMMARY_LIST_FIXED_BASE, &look.flag))
                    .with_child(flag.dom()),
            );
        }
        let meta = Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(META_CLASS))
            .with_css_props(part(SUMMARY_LIST_META_BASE, &[]))
            .with_children(DomVec::from_vec(alloc::vec![
                line(date, DATE_CLASS, &look.date),
                Dom::create_div()
                    .with_ids_and_classes(IdOrClassVec::from_const_slice(MARKS_CLASS))
                    .with_css_props(part(SUMMARY_LIST_FIXED_BASE, &[]))
                    .with_children(DomVec::from_vec(marks)),
            ]));

        let mut skin = look.row.clone();
        let mut classes: Vec<IdOrClass> = ROW_CLASS.to_vec();
        if unread {
            skin.extend(look.row_unread.iter().cloned());
            classes.push(ROW_UNREAD_CLASS[0].clone());
        }
        if selected {
            skin.extend(look.row_selected.iter().cloned());
            classes.push(ROW_SELECTED_CLASS[0].clone());
        }
        let callbacks = alloc::vec![
            click(EventFilter::Hover(HoverEventFilter::Click), on_row_click, data.clone()),
            click(
                EventFilter::Hover(HoverEventFilter::DoubleClick),
                on_row_double_click,
                data.clone()
            ),
            click(
                EventFilter::Focus(FocusEventFilter::VirtualKeyDown),
                on_row_key,
                data.clone()
            ),
        ];
        row_doms.push(
            Dom::create_div()
                .with_ids_and_classes(IdOrClassVec::from_vec(classes))
                .with_css_props(part(SUMMARY_LIST_ROW_BASE, &skin))
                .with_tab_index(roving::item_tab_index(message_at, stop))
                // An ITEM of the list, named "<from>: <subject>", selected or
                // not; its dataset says which row it is.
                .with_accessibility_info(azul_core::a11y::AccessibilityInfo {
                    role: azul_core::a11y::AccessibilityRole::ListItem,
                    accessibility_name: Some(AzString::from(alloc::format!(
                        "{}: {}",
                        from.as_str(),
                        subject.as_str()
                    )))
                    .into(),
                    states: if selected {
                        azul_core::a11y::AccessibilityStateVec::from_vec(alloc::vec![
                            azul_core::a11y::AccessibilityState::Selected,
                        ])
                    } else {
                        azul_core::a11y::AccessibilityStateVec::from_const_slice(&[])
                    },
                    ..Default::default()
                })
                .with_dataset(Some(data).into())
                .with_callbacks(callbacks.into())
                .with_children(DomVec::from_vec(alloc::vec![icon_node, text, meta])),
        );
        message_at += 1;
    }
    row_doms.push(spacer(
        total_rows.saturating_sub(first_row + window),
        row_height,
    ));
    let mut rows_box = Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(ROWS_CLASS))
        .with_css_props(part(SUMMARY_LIST_ROWS_BASE, &look.rows))
        // The rows are a LIST (a listbox: one Tab stop, the arrows within).
        .with_accessibility_info(azul_core::a11y::AccessibilityInfo {
            role: azul_core::a11y::AccessibilityRole::List,
            states: azul_core::a11y::AccessibilityStateVec::from_vec(alloc::vec![
                azul_core::a11y::AccessibilityState::Multiselectable,
            ]),
            ..Default::default()
        })
        .with_children(DomVec::from_vec(row_doms));
    if has_scroll_hook {
        rows_box = rows_box.with_callbacks(
            alloc::vec![scroll_settled_hook(on_rows_scroll_settled, shared.clone())].into(),
        );
    }

    let mut classes: Vec<IdOrClass> = LIST_CLASS.to_vec();
    if let Some(marker) = look.marker {
        classes.push(Class(AzString::from_const_str(marker)));
    }
    Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_vec(classes))
        .with_css_props(part(SUMMARY_LIST_BASE, &look.list))
        .with_children(DomVec::from_vec(alloc::vec![toolbar, sort, rows_box]))
}

#[cfg(test)]
mod summary_list_tests {
    use std::sync::{Arc, Mutex};

    use azul_core::{
        dom::{DomId, DomNodeId, NodeId, NodeType},
        styled_dom::{NodeHierarchyItemId, StyledDom},
    };

    use super::*;
    use crate::widgets::{
        roving::test_support as rv,
        themes::{theme_blocks::checks, theme_checks, UiTheme},
    };

    type Log = Arc<Mutex<Vec<(SummaryListEventKind, usize, u64, bool, bool, String)>>>;

    extern "C" fn record(mut data: RefAny, _: CallbackInfo, event: SummaryListEvent) -> Update {
        if let Some(log) = data.downcast_ref::<Log>() {
            log.lock().expect("log").push((
                event.kind,
                event.index,
                event.id,
                event.shift,
                event.ctrl,
                event.text.as_str().to_string(),
            ));
        }
        Update::RefreshDom
    }

    fn kinds(log: &Log) -> Vec<SummaryListEventKind> {
        log.lock().expect("log").iter().map(|e| e.0).collect()
    }

    fn strs(items: &[&str]) -> StringVec {
        StringVec::from_vec(items.iter().map(|s| AzString::from(*s)).collect::<Vec<_>>())
    }

    /// Today: an unread flagged mail (selected), a read one with a clip;
    /// Yesterday: one more.
    fn inbox() -> SummaryRowVec {
        SummaryRowVec::from_vec(vec![
            SummaryRow::create_group(AzString::from("Today")),
            SummaryRow::create(11, AzString::from("Google Mail-Team"), AzString::from("Welcome"))
                .with_preview(AzString::from("Thanks for joining"))
                .with_date(AzString::from("21:12"))
                .with_icon(AzString::from("mail"))
                .with_unread(true)
                .with_flagged(true)
                .with_selected(true),
            SummaryRow::create(12, AzString::from("Alice"), AzString::from("Invoice"))
                .with_date(AzString::from("18:03"))
                .with_attachment(true),
            SummaryRow::create_group(AzString::from("Yesterday")),
            SummaryRow::create(13, AzString::from("Bob"), AzString::from("Lunch?"))
                .with_date(AzString::from("Mo")),
        ])
    }

    fn list(log: &Log) -> SummaryList {
        let data = || RefAny::new(log.clone());
        let cb = record as SummaryListOnEventCallbackType;
        SummaryList::create(inbox())
            .with_scopes(strs(&["All", "Unread"]), 0)
            .with_search_placeholder(AzString::from("Search Inbox (Ctrl+E)"))
            .with_on_select(data(), cb)
            .with_on_open(data(), cb)
            .with_on_flag(data(), cb)
            .with_on_delete(data(), cb)
            .with_on_sort(data(), cb)
            .with_on_search(data(), cb)
            .with_on_scope(data(), cb)
            .with_on_scroll(data(), cb)
    }

    /// Every text of the subtree, in document order.
    /// The visible texts. An icon's empty text leaf (`Dom::create_icon`
    /// holds one for the resolved glyph) is not a text the user reads.
    fn texts(node: &Dom, out: &mut Vec<String>) {
        if let NodeType::Text(s) = node.root.get_node_type() {
            if !s.as_ref().as_str().is_empty() {
                out.push(s.as_ref().as_str().to_string());
            }
        }
        for c in node.children.as_ref() {
            texts(c, out);
        }
    }

    fn id(n: NodeId) -> DomNodeId {
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(n)),
        }
    }

    /// The nodes of `styled` carrying `class`, in document order.
    fn nodes_with(styled: &StyledDom, class: &str) -> Vec<NodeId> {
        styled
            .node_data
            .as_ref()
            .iter()
            .enumerate()
            .filter(|(_, nd)| {
                nd.get_ids_and_classes()
                    .as_ref()
                    .iter()
                    .any(|c| matches!(c, Class(s) if s.as_str() == class))
            })
            .map(|(i, _)| NodeId::new(i))
            .collect()
    }

    fn summary_rows(styled: &StyledDom) -> Vec<NodeId> {
        nodes_with(styled, ROW_CLASS_NAME)
    }

    #[test]
    fn the_list_is_the_search_row_the_sort_header_and_the_rows_box() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        for theme in checks::BOTH {
            let dom = list(&log).with_theme(theme).dom();
            let parts = dom.children.as_ref();
            assert_eq!(parts.len(), 3, "{}: toolbar, sort, rows", theme.name());
            assert!(theme_checks::has_class(&parts[0], "__azul-native-summary-list-toolbar"));
            assert_eq!(
                parts[0].children.as_ref().len(),
                2,
                "{}: the search box and the scopes",
                theme.name()
            );
            assert!(
                theme_checks::find(&parts[0], "__azul-native-segmented").is_some()
                    || theme_checks::find(&parts[0], "__azul-native-summary-list-scopes").is_some(),
                "{}: the scope buttons",
                theme.name()
            );
            let mut sort = Vec::new();
            texts(&parts[1], &mut sort);
            for t in ["Arrange by:", "Date", "Newest on top"] {
                assert!(sort.iter().any(|s| s == t), "{}: {t} in {sort:?}", theme.name());
            }
            assert!(theme_checks::has_class(&parts[2], "__azul-native-summary-list-rows"));
            assert_eq!(
                parts[2].root.get_accessibility_info().map(|i| i.role),
                Some(azul_core::a11y::AccessibilityRole::List)
            );
        }
    }

    #[test]
    fn rows_show_their_sender_subject_preview_date_and_marks_and_groups_their_title() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let dom = list(&log).with_theme(UiTheme::Flat).dom();
        let rows = dom.children.as_ref()[2].children.as_ref();
        assert_eq!(rows.len(), 7, "spacer, 5 rows, spacer");
        assert!(theme_checks::has_class(&rows[0], "__azul-native-summary-list-spacer"));
        assert!(theme_checks::has_class(&rows[6], "__azul-native-summary-list-spacer"));
        let group = &rows[1];
        assert!(theme_checks::has_class(group, "__azul-native-summary-list-group"));
        assert!(group.root.get_tab_index().is_none(), "a group header takes no focus");
        assert!(group.root.get_callbacks().as_ref().is_empty());
        let welcome = &rows[2];
        assert!(theme_checks::has_class(welcome, ROW_CLASS_NAME));
        assert!(theme_checks::has_class(welcome, "__azul-native-summary-list-row-unread"));
        assert!(theme_checks::has_class(welcome, "__azul-native-summary-list-row-selected"));
        let mut found = Vec::new();
        texts(welcome, &mut found);
        assert_eq!(found, vec!["Google Mail-Team", "Welcome", "Thanks for joining", "21:12"]);
        assert!(
            theme_checks::find(welcome, "__azul-native-summary-list-flag").is_some(),
            "every message has its flag"
        );
        assert!(theme_checks::find(welcome, "__azul-native-summary-list-attachment").is_none());
        let invoice = &rows[3];
        assert!(
            theme_checks::find(invoice, "__azul-native-summary-list-attachment").is_some(),
            "the clip"
        );
        assert!(!theme_checks::has_class(invoice, "__azul-native-summary-list-row-unread"));
        let info = welcome.root.get_accessibility_info().expect("a role");
        assert_eq!(info.role, azul_core::a11y::AccessibilityRole::ListItem);
        assert_eq!(
            info.accessibility_name.as_ref().map(|n| n.as_str()),
            Some("Google Mail-Team: Welcome")
        );
        assert_eq!(
            info.states.as_ref(),
            &[azul_core::a11y::AccessibilityState::Selected]
        );
        assert_ne!(
            welcome.root.get_style(),
            invoice.root.get_style(),
            "an unread selected row is painted differently"
        );
    }

    #[test]
    fn the_selected_row_is_the_one_tab_stop_of_the_rows() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let styled = StyledDom::create_from_dom(list(&log).with_theme(UiTheme::Flat).dom());
        let rows = summary_rows(&styled);
        assert_eq!(rows.len(), 3);
        let stops: Vec<bool> = rows
            .iter()
            .map(|r| styled.node_data.as_ref()[r.index()].get_tab_index() == Some(TabIndex::Auto))
            .collect();
        assert_eq!(stops, vec![true, false, false], "the selected row holds the stop");
    }

    #[test]
    fn spacers_stand_in_for_the_rows_outside_the_window_and_the_box_hears_a_settled_scroll() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let dom = list(&log)
            .with_window(100, 1000)
            .with_row_height(40)
            .with_theme(UiTheme::Flat)
            .dom();
        let rows_box = &dom.children.as_ref()[2];
        let rows = rows_box.children.as_ref();
        let height = |n: &Dom| {
            n.root
                .get_style()
                .iter_inline_properties()
                .find_map(|(property, _)| match property {
                    CssProperty::Height(h) => Some(format!("{h:?}")),
                    _ => None,
                })
                .unwrap_or_default()
        };
        assert!(height(&rows[0]).contains("4000"), "100 rows above: {}", height(&rows[0]));
        assert!(
            height(&rows[6]).contains("35800"),
            "895 rows below: {}",
            height(&rows[6])
        );
        let events: Vec<EventFilter> = rows_box
            .root
            .get_callbacks()
            .as_ref()
            .iter()
            .map(|cb| cb.event)
            .collect();
        assert_eq!(events, vec![EventFilter::Hover(HoverEventFilter::ScrollEnd)]);

        let plain = SummaryList::create(inbox()).with_theme(UiTheme::Flat).dom();
        assert!(
            plain.children.as_ref()[2]
                .root
                .get_callbacks()
                .as_ref()
                .is_empty(),
            "no scroll hook, no listener"
        );
        let rows = plain.children.as_ref()[2].children.as_ref();
        assert!(
            !height(&rows[0]).contains("4000"),
            "the whole list in view: nothing above it ({})",
            height(&rows[0])
        );

        // The settled scroll reports the window the app should render.
        let styled = StyledDom::create_from_dom(dom);
        let rows_box = nodes_with(&styled, "__azul-native-summary-list-rows")[0];
        let (update, _) = rv::fire(
            &styled,
            id(rows_box),
            EventFilter::Hover(HoverEventFilter::ScrollEnd),
        )
        .expect("the box hears the settled scroll");
        assert_eq!(update, Update::RefreshDom);
        assert_eq!(kinds(&log), vec![SummaryListEventKind::Scroll]);
    }

    #[test]
    fn a_click_selects_with_its_modifiers_a_double_click_opens_and_the_flag_flags() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let styled = StyledDom::create_from_dom(list(&log).with_theme(UiTheme::Flat).dom());
        let rows = summary_rows(&styled);
        let invoice = rows[1];
        let (update, _) = rv::fire(&styled, id(invoice), EventFilter::Hover(HoverEventFilter::Click))
            .expect("a row takes the click");
        assert_eq!(update, Update::RefreshDom, "the app's verdict is forwarded");
        rv::fire(
            &styled,
            id(invoice),
            EventFilter::Hover(HoverEventFilter::DoubleClick),
        )
        .expect("a row takes the double-click");
        let flag = nodes_with(&styled, "__azul-native-summary-list-flag")[1];
        let button = styled.node_hierarchy.as_ref()[flag.index()]
            .first_child_id(flag)
            .expect("the flag button");
        let (_, changes) = rv::fire(&styled, id(button), EventFilter::Hover(HoverEventFilter::Click))
            .expect("the flag takes the click");
        assert!(
            changes
                .iter()
                .any(|c| matches!(c, crate::callbacks::CallbackChange::StopPropagation)),
            "the click stops at the flag"
        );
        let events = log.lock().expect("log").clone();
        assert_eq!(events.len(), 3);
        assert_eq!(
            (events[0].0, events[0].1, events[0].2, events[0].3, events[0].4),
            (SummaryListEventKind::Select, 2, 12, false, false),
            "row 2 of the list, id 12, no modifiers"
        );
        assert_eq!((events[1].0, events[1].1, events[1].2), (SummaryListEventKind::Open, 2, 12));
        assert_eq!((events[2].0, events[2].1, events[2].2), (SummaryListEventKind::Flag, 2, 12));
    }

    #[test]
    fn arrows_move_over_summary_rows_and_select_enter_opens_delete_deletes() {
        use azul_core::window::VirtualKeyCode as K;

        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let styled = StyledDom::create_from_dom(list(&log).with_theme(UiTheme::Flat).dom());
        let rows = summary_rows(&styled);
        // Down from Invoice skips the "Yesterday" header and lands on Lunch.
        let (_, changes) = rv::press(&styled, id(rows[1]), K::Down, &[]).expect("a key handler");
        assert_eq!(rv::focus_request(&changes), Some(id(rows[2])));
        assert!(rv::prevented(&changes));
        // Up from Welcome, the first message, stays.
        let (_, changes) = rv::press(&styled, id(rows[0]), K::Up, &[]).expect("a key handler");
        assert_eq!(rv::focus_request(&changes), None);
        assert!(rv::prevented(&changes), "the key is the list's even at its end");
        // Shift+Down extends; Ctrl+Down only moves.
        let (_, changes) =
            rv::press(&styled, id(rows[0]), K::Down, &[K::LShift]).expect("a key handler");
        assert_eq!(rv::focus_request(&changes), Some(id(rows[1])));
        let (primary, _) = rv::command_keys();
        let (_, changes) =
            rv::press(&styled, id(rows[0]), K::Down, &[primary]).expect("a key handler");
        assert_eq!(rv::focus_request(&changes), Some(id(rows[1])));
        rv::press(&styled, id(rows[2]), K::Return, &[]).expect("a key handler");
        rv::press(&styled, id(rows[2]), K::Delete, &[]).expect("a key handler");
        let events = log.lock().expect("log").clone();
        let brief: Vec<(SummaryListEventKind, usize, u64, bool, bool)> = events
            .iter()
            .map(|e| (e.0, e.1, e.2, e.3, e.4))
            .collect();
        assert_eq!(
            brief,
            vec![
                (SummaryListEventKind::Select, 4, 13, false, false),
                (SummaryListEventKind::Select, 2, 12, true, false),
                (SummaryListEventKind::Open, 4, 13, false, false),
                (SummaryListEventKind::Delete, 4, 13, false, false),
            ],
            "the moved-to row is reported with its list index and id; Ctrl moves silently"
        );
    }

    #[test]
    fn home_and_end_ask_the_app_for_rows_the_window_does_not_hold() {
        use azul_core::window::VirtualKeyCode as K;

        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let styled = StyledDom::create_from_dom(
            list(&log)
                .with_window(100, 1000)
                .with_theme(UiTheme::Flat)
                .dom(),
        );
        let rows = summary_rows(&styled);
        rv::press(&styled, id(rows[1]), K::Home, &[]).expect("a key handler");
        rv::press(&styled, id(rows[1]), K::End, &[]).expect("a key handler");
        let events = log.lock().expect("log").clone();
        assert_eq!(
            events.iter().map(|e| (e.0, e.1)).collect::<Vec<_>>(),
            vec![
                (SummaryListEventKind::Select, 0),
                (SummaryListEventKind::Select, 999)
            ]
        );
    }

    /// Primary+Down moves the focus and keeps the selection; the OTHER
    /// command key is no modifier of the list's, so Down with it held is a
    /// plain Down that selects. On a Mac Cmd is primary and Ctrl is not;
    /// elsewhere Ctrl is primary and the Win key is not (DEDUP_WIDGETS_API F9).
    #[test]
    fn only_the_platforms_primary_modifier_moves_the_focus_without_selecting() {
        use azul_core::window::VirtualKeyCode as K;

        let (primary, other) = rv::command_keys();
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let styled = StyledDom::create_from_dom(list(&log).with_theme(UiTheme::Flat).dom());
        let rows = summary_rows(&styled);
        rv::press(&styled, id(rows[0]), K::Down, &[primary]).expect("a key handler");
        assert!(log.lock().expect("log").is_empty(), "{primary:?}+Down selects nothing");
        rv::press(&styled, id(rows[0]), K::Down, &[other]).expect("a key handler");
        let events = log.lock().expect("log").clone();
        assert_eq!(
            events.iter().map(|e| (e.0, e.1, e.2, e.3, e.4)).collect::<Vec<_>>(),
            vec![(SummaryListEventKind::Select, 2, 12, false, false)],
            "{other:?}+Down is a plain Down: it selects the next row, with no toggle modifier"
        );
    }

    #[test]
    fn the_search_scopes_and_sort_header_report() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let styled = StyledDom::create_from_dom(list(&log).with_theme(UiTheme::Flat).dom());
        let field = nodes_with(&styled, "__azul-native-summary-list-sort-field")[0];
        let field_button = styled.node_hierarchy.as_ref()[field.index()]
            .first_child_id(field)
            .expect("the field link");
        rv::fire(&styled, id(field_button), EventFilter::Hover(HoverEventFilter::Click))
            .expect("the field takes the click");
        let direction = nodes_with(&styled, "__azul-native-summary-list-sort-direction")[0];
        let direction_button = styled.node_hierarchy.as_ref()[direction.index()]
            .first_child_id(direction)
            .expect("the direction link");
        rv::fire(
            &styled,
            id(direction_button),
            EventFilter::Hover(HoverEventFilter::Click),
        )
        .expect("the toggle takes the click");
        let events = log.lock().expect("log").clone();
        assert_eq!(events[0].0, SummaryListEventKind::Sort);
        assert_eq!(events[0].5, "Date");
        assert_eq!(events[1].0, SummaryListEventKind::SortDirection);
    }

    /// The list's `Select` event (index + Shift / Ctrl) goes through the
    /// shared [`ListSelection`], keyed by the row's index in the whole list.
    #[test]
    fn a_select_event_applies_through_the_shared_list_selection() {
        use crate::widgets::list_selection::ListSelection;
        let rows = |s: &ListSelection| s.keys.as_slice().to_vec();
        let s = ListSelection::create();
        assert!(s.is_empty());
        let s = s.apply(4, false, false);
        assert_eq!(rows(&s), vec![4]);
        let s = s.apply(7, false, true);
        assert_eq!(rows(&s), vec![4, 7], "Ctrl adds");
        let s = s.apply(4, false, true);
        assert_eq!(rows(&s), vec![7], "Ctrl again removes");
        // As in Explorer and Outlook, a Ctrl+click moves the anchor even when
        // it deselects: the next Shift+click ranges from the row last clicked.
        assert_eq!(s.anchor, azul_css::corety::OptionU64::Some(4));
        let s = s.apply(9, true, false);
        assert_eq!(rows(&s), vec![4, 5, 6, 7, 8, 9], "Shift: the range from the anchor");
        assert!(s.contains(5));
        assert_eq!(s.len(), 6);
        let s = s.apply(2, true, false);
        assert_eq!(rows(&s), vec![2, 3, 4], "Shift again: the range flips around the anchor");
        let s = s.apply(9, false, false);
        assert_eq!(rows(&s), vec![9], "a plain click starts over");
    }

    #[test]
    fn a_list_without_a_theme_follows_the_app_theme_and_declares_its_structure_once() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        checks::assert_follows_the_app_theme(
            "summary_list",
            || list(&log).dom(),
            |t: UiTheme| list(&log).with_theme(t).dom(),
        );
        for theme in checks::BOTH {
            let dom = checks::under(theme, || list(&log).dom());
            theme_checks::assert_structure_is_shared(
                &format!("summary_list built for {}", theme.name()),
                &dom,
                &[],
            );
        }
    }

    /// The mark at a row's end and its name, if the row carries one.
    fn mark_of(row: &Dom) -> Option<(Vec<String>, String)> {
        fn icons(node: &Dom, out: &mut Vec<String>) {
            if let NodeType::Icon(name) = node.root.get_node_type() {
                out.push(name.as_ref().as_str().to_string());
            }
            for c in node.children.as_ref() {
                icons(c, out);
            }
        }
        let mark = theme_checks::find(row, "__azul-native-summary-list-flag")?;
        let mut glyphs = Vec::new();
        icons(mark, &mut glyphs);
        let name = mark
            .children
            .as_ref()
            .first()
            .and_then(|button| {
                button.root.attributes().as_ref().iter().find_map(|a| match a {
                    azul_core::dom::AttributeType::Alt(s) => Some(s.as_str().to_string()),
                    _ => None,
                })
            })
            .unwrap_or_default();
        Some((glyphs, name))
    }

    /// A notes list pins instead of flagging: the pinned row carries the pin,
    /// named by what a press does ("Unpin"), an unpinned row no mark (notes
    /// are pinned by a command); a list without marks has none; the default
    /// stays mail's flag on every row. The press reports `Flag` either way.
    #[test]
    fn a_list_names_its_row_mark_after_what_it_stands_for() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let rows_of = |list: SummaryList| -> Vec<Dom> {
            let dom = list.with_theme(UiTheme::Flat).dom();
            dom.children.as_ref()[2].children.as_ref().to_vec()
        };

        // rows: spacer, Today, Welcome (flagged), Invoice, Yesterday, Lunch, spacer
        let flagged = rows_of(list(&log));
        assert_eq!(
            mark_of(&flagged[2]),
            Some((vec!["flag".to_string()], "Unflag".to_string())),
            "the default is the flag"
        );
        assert_eq!(
            mark_of(&flagged[3]),
            Some((vec!["outlined_flag".to_string()], "Flag".to_string()))
        );

        let pinned = rows_of(list(&log).with_mark(SummaryListMark::Pin));
        assert_eq!(
            mark_of(&pinned[2]),
            Some((vec!["push_pin".to_string()], "Unpin".to_string())),
            "a pinned note carries the pin, named by what a press does"
        );
        assert_eq!(mark_of(&pinned[3]), None, "an unpinned note carries no mark");

        let bare = rows_of(list(&log).with_mark(SummaryListMark::None));
        assert!(
            bare.iter().all(|row| mark_of(row).is_none()),
            "a list without marks has none"
        );
    }
}
