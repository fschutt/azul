//! Reference picker widget - type to find one record of a large list and
//! pick it: the customer of an invoice, the contact of a mail, the account
//! of a booking, the asset of a ticket.
//!
//! ```text
//!   +--------------------------------+
//!   | acm                          v |      the field: what the user typed
//!   +--------------------------------+
//!   | ACME GmbH                      |      the records found, each with a
//!   |   Customer 1042, Berlin        |      quieter detail line
//!   | Acme Corp                      |
//!   |   Supplier 77, Ohio            |
//!   | Create customer "acm"          |      the "create new" row
//!   | 3 more - type to narrow        |      the list's status line
//!   +--------------------------------+
//! ```
//!
//! BUILT ON THE COMBOBOX (no twin of it): the field, the popup list, the
//! keys (Down / Up / Home / End make an option active, Enter picks it,
//! Escape and Tab close), the active option and the accessibility of the
//! picker are [`super::combobox::ComboBox`]'s. What the reference picker
//! adds is the meaning: records with an id (`u64`) and a detail, the
//! search, the "create new" row and the list's status.
//!
//! THE APP OWNS THE STATE AND THE RECORDS: every action is a
//! [`ReferencePickerEvent`] - `Query` (the text typed, after the debounce),
//! `Pick` (a record: its id and label), `Create` (the "create" row: the text
//! typed). The app stores what it needs and rebuilds: with the query it
//! typed ([`ReferencePicker::with_query`]) and either
//!
//! - [`ReferencePickerFilter::Local`]: ALL its records - the picker filters
//!   them by the query (case-insensitive, label or detail), or
//! - [`ReferencePickerFilter::App`]: the records its own search found for the
//!   query, ASYNC if it likes - with `loading` on while it searches (the list
//!   says "Searching...") and the results in a later rebuild.
//!
//! The list stays open across those rebuilds (the engine keeps the popup;
//! the combobox asks it). At most `max_rows` records are listed (a list of
//! 100,000 customers is typed into, not scrolled); the status line says how
//! many more there are.
//!
//! DEBOUNCE: a query is reported `debounce_ms` after the last keystroke
//! (default 150 ms; 0 reports every keystroke), so a fast typist starts one
//! search, not ten.
//!
//! Key types: [`ReferencePicker`], [`ReferencePickerItem`],
//! [`ReferencePickerEvent`], [`ReferencePickerFilter`].

use alloc::{format, string::String, vec::Vec};

use azul_core::{
    callbacks::{TimerCallbackReturn, Update},
    dom::Dom,
    refany::RefAny,
    task::{Duration, TerminateTimer, TimerId},
};
use azul_css::{
    corety::OptionU64, impl_option, impl_vec, impl_vec_clone, impl_vec_debug, impl_vec_mut,
    AzString, OptionString, StringVec,
};

use crate::{
    callbacks::CallbackInfo,
    timer::{Timer, TimerCallback, TimerCallbackInfo},
    widgets::{
        combobox::{
            ComboBox, ComboBoxOnSelectCallbackType, ComboBoxOnTextInputCallbackType, ComboBoxState,
        },
        themes::{OptionUiTheme, UiTheme},
    },
};

/// The class on the picker's root (the combobox's wrapper).
pub const REFERENCE_PICKER_CLASS: &str = "__azul-native-reference-picker";

/// The records listed at most unless the app says otherwise.
pub const DEFAULT_MAX_ROWS: usize = 50;

/// The debounce unless the app says otherwise, in ms.
pub const DEFAULT_DEBOUNCE_MS: u32 = 150;

// ---- the types the app sees ----

/// One record the picker can pick: its id, its label and a detail line.
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ReferencePickerItem {
    /// What the record is called ("ACME GmbH") - the field shows it once
    /// picked.
    pub label: AzString,
    /// A quieter second line ("Customer 1042, Berlin"); empty for none.
    pub detail: AzString,
    /// The app's id of the record.
    pub id: u64,
}

impl ReferencePickerItem {
    /// The record `id` called `label`.
    #[must_use]
    pub const fn create(id: u64, label: AzString) -> Self {
        Self {
            label,
            detail: AzString::from_const_str(""),
            id,
        }
    }

    /// The record with a detail line.
    #[must_use]
    pub fn with_detail(mut self, detail: AzString) -> Self {
        self.detail = detail;
        self
    }
}

impl_option!(
    ReferencePickerItem,
    OptionReferencePickerItem,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
impl_vec!(
    ReferencePickerItem,
    ReferencePickerItemVec,
    ReferencePickerItemVecDestructor,
    ReferencePickerItemVecDestructorType,
    ReferencePickerItemVecSlice,
    OptionReferencePickerItem
);
impl_vec_clone!(
    ReferencePickerItem,
    ReferencePickerItemVec,
    ReferencePickerItemVecDestructor
);
impl_vec_debug!(ReferencePickerItem, ReferencePickerItemVec);
impl_vec_mut!(ReferencePickerItem, ReferencePickerItemVec);

azul_css::impl_vec_partialeq!(ReferencePickerItem, ReferencePickerItemVec);

/// Who filters the records by the query.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum ReferencePickerFilter {
    /// The picker: the app hands in all its records, the picker lists those
    /// whose label or detail holds the query (case-insensitive).
    #[default]
    Local,
    /// The app: the records handed in ARE the results of its search for the
    /// query, listed as they are.
    App,
}

/// What happened.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum ReferencePickerEventKind {
    /// The user typed: `text` is the query (after the debounce).
    #[default]
    Query,
    /// A record was picked: `id` and its label in `text`.
    Pick,
    /// The "create" row was picked: `text` is what was typed.
    Create,
}

/// One action of the picker.
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferencePickerEvent {
    /// The query (`Query`, `Create`) or the picked record's label (`Pick`).
    pub text: AzString,
    /// The picked record's id (`Pick`; 0 otherwise).
    pub id: u64,
    /// What happened.
    pub kind: ReferencePickerEventKind,
}

/// Callback invoked on every action of the picker.
pub type ReferencePickerOnEventCallbackType =
    extern "C" fn(RefAny, CallbackInfo, ReferencePickerEvent) -> Update;
impl_widget_callback!(
    ReferencePickerOnEvent,
    OptionReferencePickerOnEvent,
    ReferencePickerOnEventCallback,
    ReferencePickerOnEventCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        ReferencePickerOnEventCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: REFERENCE_PICKER_ON_EVENT_INVOKER,
    invoker_ty:     AzReferencePickerOnEventCallbackInvoker,
    thunk_fn:       az_reference_picker_on_event_callback_thunk,
    setter_fn:      AzApp_setReferencePickerOnEventCallbackInvoker,
    from_handle_fn: AzReferencePickerOnEventCallback_createFromHostHandle,
    from_handle_byref_fn: AzReferencePickerOnEventCallback_createFromHostHandleByref,
    extra_args:     [ event: ReferencePickerEvent ],
}

/// Type to find one record and pick it (module docs).
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct ReferencePicker {
    /// The records: all of them (`Local`) or the app's results (`App`).
    pub items: ReferencePickerItemVec,
    /// What the user typed (the app's stored `Query` text).
    pub query: AzString,
    /// The prompt of the empty field ("Customer").
    pub placeholder: AzString,
    /// The label of the "create" row ("Create customer"), shown with the
    /// query when no record is called exactly that; `None`: no such row.
    pub create_label: OptionString,
    /// The hook every action reports to.
    pub on_event: OptionReferencePickerOnEvent,
    /// What this control is CALLED, for assistive technology.
    pub accessibility_name: OptionString,
    /// The id of the record picked, if any: the field shows its label while
    /// nothing is typed.
    pub selected: OptionU64,
    /// The records listed at most (0: all).
    pub max_rows: usize,
    /// How long after the last keystroke a query is reported, in ms.
    pub debounce_ms: u32,
    /// Who filters the records by the query.
    pub filter: ReferencePickerFilter,
    /// The widget theme, or `None` to follow the app theme.
    pub theme: OptionUiTheme,
    /// The app is searching (`App`): the list says "Searching...".
    pub loading: bool,
}

impl Default for ReferencePicker {
    fn default() -> Self {
        Self::create(ReferencePickerItemVec::from_const_slice(&[]))
    }
}

impl ReferencePicker {
    /// A picker over `items`, filtering them itself, nothing typed.
    #[must_use]
    pub fn create(items: ReferencePickerItemVec) -> Self {
        Self {
            items,
            query: AzString::from_const_str(""),
            placeholder: AzString::from_const_str(""),
            create_label: OptionString::None,
            on_event: OptionReferencePickerOnEvent::None,
            accessibility_name: OptionString::None,
            selected: OptionU64::None,
            max_rows: DEFAULT_MAX_ROWS,
            debounce_ms: DEFAULT_DEBOUNCE_MS,
            filter: ReferencePickerFilter::Local,
            theme: OptionUiTheme::None,
            loading: false,
        }
    }

    /// Name this control for assistive technology.
    #[must_use]
    pub fn with_accessibility_name<S: Into<AzString>>(mut self, name: S) -> Self {
        self.accessibility_name = Some(name.into()).into();
        self
    }

    /// The records (all, or the app's results).
    pub fn set_items(&mut self, items: ReferencePickerItemVec) {
        self.items = items;
    }

    /// [`Self::set_items`] for the builder chain.
    #[must_use]
    pub fn with_items(mut self, items: ReferencePickerItemVec) -> Self {
        self.set_items(items);
        self
    }

    /// What the user typed (the stored `Query` text).
    pub fn set_query(&mut self, query: AzString) {
        self.query = query;
    }

    /// [`Self::set_query`] for the builder chain.
    #[must_use]
    pub fn with_query(mut self, query: AzString) -> Self {
        self.set_query(query);
        self
    }

    /// The prompt of the empty field.
    pub fn set_placeholder(&mut self, placeholder: AzString) {
        self.placeholder = placeholder;
    }

    /// [`Self::set_placeholder`] for the builder chain.
    #[must_use]
    pub fn with_placeholder(mut self, placeholder: AzString) -> Self {
        self.set_placeholder(placeholder);
        self
    }

    /// Offer a "create" row labelled `label` (shown with the query).
    pub fn set_create_label(&mut self, label: AzString) {
        self.create_label = OptionString::Some(label);
    }

    /// [`Self::set_create_label`] for the builder chain.
    #[must_use]
    pub fn with_create_label(mut self, label: AzString) -> Self {
        self.set_create_label(label);
        self
    }

    /// The record picked (its label shows while nothing is typed).
    pub const fn set_selected(&mut self, id: u64) {
        self.selected = OptionU64::Some(id);
    }

    /// [`Self::set_selected`] for the builder chain.
    #[must_use]
    pub const fn with_selected(mut self, id: u64) -> Self {
        self.set_selected(id);
        self
    }

    /// The records listed at most (0: all).
    pub const fn set_max_rows(&mut self, max_rows: usize) {
        self.max_rows = max_rows;
    }

    /// [`Self::set_max_rows`] for the builder chain.
    #[must_use]
    pub const fn with_max_rows(mut self, max_rows: usize) -> Self {
        self.set_max_rows(max_rows);
        self
    }

    /// The debounce in ms (0: every keystroke is a query).
    pub const fn set_debounce_ms(&mut self, debounce_ms: u32) {
        self.debounce_ms = debounce_ms;
    }

    /// [`Self::set_debounce_ms`] for the builder chain.
    #[must_use]
    pub const fn with_debounce_ms(mut self, debounce_ms: u32) -> Self {
        self.set_debounce_ms(debounce_ms);
        self
    }

    /// Who filters the records by the query.
    pub const fn set_filter(&mut self, filter: ReferencePickerFilter) {
        self.filter = filter;
    }

    /// [`Self::set_filter`] for the builder chain.
    #[must_use]
    pub const fn with_filter(mut self, filter: ReferencePickerFilter) -> Self {
        self.set_filter(filter);
        self
    }

    /// The app is searching: the list says so.
    pub const fn set_loading(&mut self, loading: bool) {
        self.loading = loading;
    }

    /// [`Self::set_loading`] for the builder chain.
    #[must_use]
    pub const fn with_loading(mut self, loading: bool) -> Self {
        self.set_loading(loading);
        self
    }

    /// The hook every action reports to.
    pub fn set_on_event<C: Into<ReferencePickerOnEventCallback>>(
        &mut self,
        data: RefAny,
        callback: C,
    ) {
        self.on_event = Some(ReferencePickerOnEvent::create(data, callback)).into();
    }

    /// [`Self::set_on_event`] for the builder chain.
    #[must_use]
    pub fn with_on_event<C: Into<ReferencePickerOnEventCallback>>(
        mut self,
        data: RefAny,
        callback: C,
    ) -> Self {
        self.set_on_event(data, callback);
        self
    }

    /// Pin the widget theme. Unset (`None`), it follows the app theme.
    pub const fn set_theme(&mut self, theme: UiTheme) {
        self.theme = OptionUiTheme::Some(theme);
    }

    /// [`Self::set_theme`] for the builder chain.
    #[must_use]
    pub const fn with_theme(mut self, theme: UiTheme) -> Self {
        self.set_theme(theme);
        self
    }

    /// Replaces `self` with an empty picker and returns the original.
    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::default();
        core::mem::swap(&mut s, self);
        s
    }
}

// ---- what the list shows (the pure half) ----

/// Whether `item` matches `query`: its label or its detail holds it,
/// ignoring case; an empty (or blank) query matches everything.
#[must_use]
pub(crate) fn matches(item: &ReferencePickerItem, query: &str) -> bool {
    let _ = (item, query);
    false
}

/// The records listed for `query`: their indices in `items` (at most
/// `max_rows`, 0 = all) and how many more matched.
#[must_use]
pub(crate) fn shown_rows(
    items: &[ReferencePickerItem],
    query: &str,
    filter: ReferencePickerFilter,
    max_rows: usize,
) -> (Vec<usize>, usize) {
    let _ = (items, query, filter, max_rows);
    (Vec::new(), 0)
}

/// The "create" row's text for `query`, or `None` when there is no such
/// row: no create label, nothing typed, or a record called exactly the
/// query (ignoring case) - picking it is what the user wants.
#[must_use]
pub(crate) fn create_row(
    create_label: Option<&str>,
    query: &str,
    items: &[ReferencePickerItem],
) -> Option<String> {
    let _ = (create_label, query, items);
    None
}

/// The list's status line: "Searching..." while the app searches, "No
/// matches" when nothing is listed, "N more - type to narrow" when records
/// were left out; `None` otherwise.
#[must_use]
pub(crate) fn status_line(loading: bool, listed: usize, more: usize) -> Option<String> {
    let _ = (loading, listed, more);
    None
}

/// The field's text: the query while one is typed, else the picked record's
/// label, else nothing.
#[must_use]
pub(crate) fn field_text(
    query: &str,
    selected: Option<u64>,
    items: &[ReferencePickerItem],
) -> String {
    let _ = (query, selected, items);
    String::new()
}

#[cfg(test)]
mod list_tests {
    use super::*;

    fn item(id: u64, label: &'static str, detail: &'static str) -> ReferencePickerItem {
        ReferencePickerItem::create(id, AzString::from_const_str(label))
            .with_detail(AzString::from_const_str(detail))
    }

    fn customers() -> Vec<ReferencePickerItem> {
        alloc::vec![
            item(1042, "ACME GmbH", "Customer 1042, Berlin"),
            item(77, "Acme Corp", "Supplier 77, Ohio"),
            item(5, "Globex", "Customer 5, Paris"),
            item(9, "Initech", "Customer 9, Austin"),
        ]
    }

    #[test]
    fn a_record_matches_its_label_or_detail_ignoring_case() {
        let c = customers();
        assert!(matches(&c[0], "acme"));
        assert!(matches(&c[0], "BERLIN"));
        assert!(matches(&c[2], "paris"));
        assert!(!matches(&c[2], "acme"));
        assert!(matches(&c[3], ""), "nothing typed matches everything");
        assert!(matches(&c[3], "   "));
        assert!(matches(&c[0], " acme "), "the query is trimmed");
    }

    #[test]
    fn the_picker_lists_the_matching_records_in_order() {
        let c = customers();
        assert_eq!(
            shown_rows(&c, "acme", ReferencePickerFilter::Local, 50),
            (alloc::vec![0, 1], 0)
        );
        assert_eq!(
            shown_rows(&c, "customer", ReferencePickerFilter::Local, 50),
            (alloc::vec![0, 2, 3], 0)
        );
        assert_eq!(
            shown_rows(&c, "zzz", ReferencePickerFilter::Local, 50),
            (Vec::new(), 0)
        );
    }

    #[test]
    fn the_apps_results_are_listed_as_they_are() {
        let c = customers();
        assert_eq!(
            shown_rows(&c, "zzz", ReferencePickerFilter::App, 50),
            (alloc::vec![0, 1, 2, 3], 0),
            "the app searched: the picker does not filter again"
        );
    }

    #[test]
    fn at_most_max_rows_are_listed_and_the_rest_counted() {
        let c = customers();
        assert_eq!(
            shown_rows(&c, "", ReferencePickerFilter::Local, 2),
            (alloc::vec![0, 1], 2)
        );
        assert_eq!(
            shown_rows(&c, "", ReferencePickerFilter::Local, 0),
            (alloc::vec![0, 1, 2, 3], 0)
        );
        assert_eq!(
            shown_rows(&c, "customer", ReferencePickerFilter::Local, 1),
            (alloc::vec![0], 2)
        );
    }

    #[test]
    fn the_create_row_offers_the_query_unless_a_record_is_called_that() {
        let c = customers();
        assert_eq!(
            create_row(Some("Create customer"), "Umbrella", &c).as_deref(),
            Some("Create customer \u{201c}Umbrella\u{201d}")
        );
        assert_eq!(
            create_row(Some("Create customer"), "globex", &c),
            None,
            "Globex exists"
        );
        assert_eq!(
            create_row(Some("Create customer"), "  ", &c),
            None,
            "nothing typed"
        );
        assert_eq!(create_row(None, "Umbrella", &c), None, "no create label");
        assert_eq!(
            create_row(Some("New"), " Umbrella ", &c).as_deref(),
            Some("New \u{201c}Umbrella\u{201d}")
        );
    }

    #[test]
    fn the_status_line_says_searching_nothing_or_how_many_more() {
        assert_eq!(status_line(true, 3, 0).as_deref(), Some("Searching..."));
        assert_eq!(status_line(false, 0, 0).as_deref(), Some("No matches"));
        assert_eq!(
            status_line(false, 50, 12).as_deref(),
            Some("12 more - type to narrow")
        );
        assert_eq!(status_line(false, 3, 0), None);
    }

    #[test]
    fn the_field_shows_the_query_or_the_picked_records_label() {
        let c = customers();
        assert_eq!(field_text("acm", Some(5), &c), "acm");
        assert_eq!(field_text("", Some(5), &c), "Globex");
        assert_eq!(
            field_text("", Some(12345), &c),
            "",
            "an id the list does not have"
        );
        assert_eq!(field_text("", None, &c), "");
    }
}
