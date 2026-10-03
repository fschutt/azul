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
    let query = query.trim().to_lowercase();
    query.is_empty()
        || item.label.as_str().to_lowercase().contains(&query)
        || item.detail.as_str().to_lowercase().contains(&query)
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
    let found: Vec<usize> = items
        .iter()
        .enumerate()
        .filter(|(_, item)| filter == ReferencePickerFilter::App || matches(item, query))
        .map(|(i, _)| i)
        .collect();
    if max_rows == 0 || found.len() <= max_rows {
        return (found, 0);
    }
    let more = found.len() - max_rows;
    (found[..max_rows].to_vec(), more)
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
    let label = create_label?;
    let query = query.trim();
    if query.is_empty() {
        return None;
    }
    let lower = query.to_lowercase();
    if items
        .iter()
        .any(|i| i.label.as_str().trim().to_lowercase() == lower)
    {
        return None;
    }
    Some(format!("{label} \u{201c}{query}\u{201d}"))
}

/// The list's status line: "Searching..." while the app searches, "No
/// matches" when nothing is listed, "N more - type to narrow" when records
/// were left out; `None` otherwise.
#[must_use]
pub(crate) fn status_line(loading: bool, listed: usize, more: usize) -> Option<String> {
    if loading {
        Some(String::from("Searching..."))
    } else if listed == 0 {
        Some(String::from("No matches"))
    } else if more > 0 {
        Some(format!("{more} more - type to narrow"))
    } else {
        None
    }
}

/// The field's text: the query while one is typed, else the picked record's
/// label, else nothing.
#[must_use]
pub(crate) fn field_text(
    query: &str,
    selected: Option<u64>,
    items: &[ReferencePickerItem],
) -> String {
    if !query.is_empty() {
        return String::from(query);
    }
    selected
        .and_then(|id| items.iter().find(|i| i.id == id))
        .map(|i| String::from(i.label.as_str()))
        .unwrap_or_default()
}

// ==== the DOM: a combobox over the listed records ====

/// What the picker's handlers share: the listed records' ids and labels in
/// the list's order, the "create" row's place, the query typed so far, the
/// hook, the debounce and the timer counting it down.
pub(crate) struct ReferenceShared {
    pub(crate) ids: Vec<u64>,
    pub(crate) labels: Vec<AzString>,
    pub(crate) create_index: Option<usize>,
    pub(crate) query: String,
    pub(crate) on_event: OptionReferencePickerOnEvent,
    pub(crate) debounce_ms: u32,
    pub(crate) timer: Option<TimerId>,
}

impl ReferencePicker {
    /// Renders the picker: a combobox listing the records found (each with
    /// its detail line), the "create" row, the status line; the field holds
    /// the query or the picked record's label. Its look is the combobox's
    /// (flat / flora, following the app theme unless pinned).
    #[must_use]
    pub fn dom(self) -> Dom {
        let items = self.items.as_slice();
        let query = self.query.as_str();
        let (rows, more) = shown_rows(items, query, self.filter, self.max_rows);

        let mut labels: Vec<AzString> = rows.iter().map(|i| items[*i].label.clone()).collect();
        let mut details: Vec<AzString> = rows.iter().map(|i| items[*i].detail.clone()).collect();
        let ids: Vec<u64> = rows.iter().map(|i| items[*i].id).collect();
        let selected_row = self
            .selected
            .into_option()
            .and_then(|id| ids.iter().position(|i| *i == id));
        let create_label = self.create_label.as_ref().map(AzString::as_str);
        let create_index = create_row(create_label, query, items).map(|text| {
            labels.push(AzString::from(text));
            details.push(AzString::from_const_str(""));
            labels.len() - 1
        });
        let status = status_line(
            self.loading,
            rows.len() + usize::from(create_index.is_some()),
            more,
        );
        let text = field_text(query, self.selected.into_option(), items);

        let shared = RefAny::new(ReferenceShared {
            ids,
            labels: labels.clone(),
            create_index,
            query: String::from(query),
            on_event: self.on_event.clone(),
            debounce_ms: self.debounce_ms,
            timer: None,
        });
        let on_text: ComboBoxOnTextInputCallbackType = on_reference_text_input;
        let on_select: ComboBoxOnSelectCallbackType = on_reference_select;
        let mut combo = ComboBox::new(StringVec::from_vec(labels))
            .with_item_details(StringVec::from_vec(details))
            .with_open_on_type(true)
            .with_text(AzString::from(text))
            .with_placeholder(self.placeholder.clone())
            .with_on_text_input(shared.clone(), on_text)
            .with_on_select(shared, on_select);
        if let Some(row) = selected_row {
            combo = combo.with_selected(row);
        }
        if let Some(status) = status {
            combo = combo.with_status(AzString::from(status));
        }
        if let Some(name) = self.accessibility_name.into_option() {
            combo = combo.with_accessibility_name(name);
        }
        if let Some(theme) = self.theme.into_option() {
            combo = combo.with_theme(theme);
        }
        let mut dom = combo.dom();
        dom.add_class(AzString::from_const_str(REFERENCE_PICKER_CLASS));
        dom
    }
}

impl From<ReferencePicker> for Dom {
    fn from(p: ReferencePicker) -> Self {
        p.dom()
    }
}

/// Hands `event` to the app's hook.
fn report(shared: &mut RefAny, info: CallbackInfo, event: ReferencePickerEvent) -> Update {
    let hook = match shared.downcast_ref::<ReferenceShared>() {
        Some(s) => s.on_event.clone(),
        None => return Update::DoNothing,
    };
    match hook.as_ref() {
        Some(ReferencePickerOnEvent { callback, refany }) => {
            callback.invoke(refany.clone(), info, event)
        }
        None => Update::DoNothing,
    }
}

/// Reports the query typed so far.
fn report_query(shared: &mut RefAny, info: CallbackInfo) -> Update {
    let text = match shared.downcast_ref::<ReferenceShared>() {
        Some(s) => AzString::from(s.query.clone()),
        None => return Update::DoNothing,
    };
    report(
        shared,
        info,
        ReferencePickerEvent {
            text,
            id: 0,
            kind: ReferencePickerEventKind::Query,
        },
    )
}

/// The user typed: the query is kept, and reported once the debounce has
/// passed without another keystroke (at once without a debounce).
extern "C" fn on_reference_text_input(
    mut data: RefAny,
    mut info: CallbackInfo,
    state: ComboBoxState,
) -> Update {
    let (debounce_ms, pending) = {
        let Some(mut s) = data.downcast_mut::<ReferenceShared>() else {
            return Update::DoNothing;
        };
        s.query = String::from(state.text.as_str());
        (s.debounce_ms, s.timer.take())
    };
    if let Some(timer) = pending {
        info.remove_timer(timer);
    }
    if debounce_ms == 0 {
        return report_query(&mut data, info);
    }
    let id = TimerId::unique();
    let timer = Timer::create(
        data.clone(),
        TimerCallback::create(on_reference_debounce),
        info.get_system_time_fn(),
    )
    .with_delay(Duration::from_millis(u64::from(debounce_ms)));
    info.add_timer(id, timer);
    if let Some(mut s) = data.downcast_mut::<ReferenceShared>() {
        s.timer = Some(id);
    }
    Update::DoNothing
}

/// The debounce passed: the query is reported, once.
extern "C" fn on_reference_debounce(
    mut data: RefAny,
    info: TimerCallbackInfo,
) -> TimerCallbackReturn {
    if let Some(mut s) = data.downcast_mut::<ReferenceShared>() {
        s.timer = None;
    }
    let update = report_query(&mut data, *info.get_callback_info());
    TimerCallbackReturn::create(update, TerminateTimer::Terminate)
}

/// An option was picked (a click, Enter on the active one): a record - its
/// id and label - or the "create" row - the query.
extern "C" fn on_reference_select(
    mut data: RefAny,
    mut info: CallbackInfo,
    state: ComboBoxState,
) -> Update {
    let (event, pending) = {
        let Some(mut s) = data.downcast_mut::<ReferenceShared>() else {
            return Update::DoNothing;
        };
        let event = if Some(state.selected) == s.create_index {
            ReferencePickerEvent {
                text: AzString::from(s.query.trim().to_string()),
                id: 0,
                kind: ReferencePickerEventKind::Create,
            }
        } else {
            let (Some(id), Some(label)) = (s.ids.get(state.selected), s.labels.get(state.selected))
            else {
                return Update::DoNothing;
            };
            ReferencePickerEvent {
                text: label.clone(),
                id: *id,
                kind: ReferencePickerEventKind::Pick,
            }
        };
        // A pick ends the typing: a query still counting down is dropped.
        (event, s.timer.take())
    };
    if let Some(timer) = pending {
        info.remove_timer(timer);
    }
    report(&mut data, info, event)
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

// ==== fixtures (the widget manifest's sample) ====

/// Samples for the widget manifest (`widgets::label_convention`) and the
/// tests.
#[cfg(test)]
pub(crate) mod fixtures {
    use super::*;

    /// Four customers and suppliers.
    pub(crate) fn customers() -> ReferencePickerItemVec {
        let item = |id: u64, label: &'static str, detail: &'static str| {
            ReferencePickerItem::create(id, AzString::from_const_str(label))
                .with_detail(AzString::from_const_str(detail))
        };
        ReferencePickerItemVec::from_vec(alloc::vec![
            item(1042, "ACME GmbH", "Customer 1042, Berlin"),
            item(77, "Acme Corp", "Supplier 77, Ohio"),
            item(5, "Globex", "Customer 5, Paris"),
            item(9, "Initech", "Customer 9, Austin"),
        ])
    }

    /// "acme" typed: two records and the "create" row.
    pub(crate) fn sample() -> ReferencePicker {
        ReferencePicker::create(customers())
            .with_query(AzString::from_const_str("acme"))
            .with_placeholder(AzString::from_const_str("Customer"))
            .with_create_label(AzString::from_const_str("Create customer"))
            .with_accessibility_name("Customer")
    }
}

#[cfg(test)]
mod dom_tests {
    use std::sync::{Arc, Mutex};

    use azul_core::{
        dom::{DomId, DomNodeId, EventFilter, HoverEventFilter, NodeType},
        id::NodeId,
        styled_dom::{NodeHierarchyItemId, StyledDom},
    };

    use super::{fixtures::*, *};
    use crate::widgets::{roving::test_support as rv, themes::theme_checks as tc};

    const OPTION: &str = "__azul-native-combobox-option";
    const FIELD_TEXT: &str = "__azul-native-combobox-text";

    type Log = Arc<Mutex<Vec<ReferencePickerEvent>>>;

    extern "C" fn record(
        mut data: RefAny,
        _info: CallbackInfo,
        event: ReferencePickerEvent,
    ) -> Update {
        if let Some(log) = data.downcast_ref::<Log>() {
            log.lock().expect("log").push(event);
        }
        Update::RefreshDom
    }

    fn node(i: usize) -> DomNodeId {
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(i))),
        }
    }

    fn styled(p: ReferencePicker) -> (StyledDom, Log) {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let p = p.with_theme(UiTheme::Flat).with_on_event(
            RefAny::new(log.clone()),
            record as ReferencePickerOnEventCallbackType,
        );
        (StyledDom::create_from_dom(p.dom()), log)
    }

    fn options(styled: &StyledDom) -> Vec<usize> {
        styled
            .node_data
            .as_ref()
            .iter()
            .enumerate()
            .filter(|(_, n)| n.has_class(OPTION))
            .map(|(i, _)| i)
            .collect()
    }

    fn texts(dom: &Dom) -> Vec<String> {
        tc::nodes(dom)
            .into_iter()
            .filter_map(|(_, n)| match n.root.get_node_type() {
                NodeType::Text(s) => Some(s.as_str().to_string()),
                _ => None,
            })
            .collect()
    }

    /// The text the field shows.
    fn field_text_of(dom: &Dom) -> String {
        let field = tc::find(dom, FIELD_TEXT).expect("the field's text");
        texts(field).concat()
    }

    #[test]
    fn the_list_holds_the_matching_records_with_details_and_the_create_row() {
        let (s, _) = styled(sample());
        assert_eq!(options(&s).len(), 3, "ACME GmbH, Acme Corp, create");
        let all = texts(&sample().dom());
        for t in [
            "ACME GmbH",
            "Customer 1042, Berlin",
            "Acme Corp",
            "Create customer \u{201c}acme\u{201d}",
        ] {
            assert!(all.iter().any(|x| x == t), "{t:?} in {all:?}");
        }
        assert!(!all.iter().any(|x| x == "Globex"), "Globex does not match");
    }

    #[test]
    fn the_root_is_marked_and_the_field_holds_the_query() {
        let dom = sample().dom();
        assert!(tc::has_class(&dom, REFERENCE_PICKER_CLASS));
        assert_eq!(field_text_of(&dom), "acme");
    }

    #[test]
    fn a_picked_record_shows_its_label_while_nothing_is_typed() {
        let dom = ReferencePicker::create(customers()).with_selected(5).dom();
        assert_eq!(field_text_of(&dom), "Globex");
    }

    #[test]
    fn the_list_says_when_nothing_matches_and_while_the_app_searches() {
        let none = ReferencePicker::create(customers())
            .with_query(AzString::from_const_str("zzz"))
            .dom();
        assert!(texts(&none).iter().any(|t| t == "No matches"));
        let searching = ReferencePicker::create(customers())
            .with_filter(ReferencePickerFilter::App)
            .with_loading(true)
            .dom();
        assert!(texts(&searching).iter().any(|t| t == "Searching..."));
    }

    #[test]
    fn picking_a_record_reports_its_id_and_label() {
        let (s, log) = styled(sample());
        let opts = options(&s);
        let _ = rv::fire(
            &s,
            node(opts[1]),
            EventFilter::Hover(HoverEventFilter::Click),
        )
        .expect("an option takes the click");
        let events = log.lock().expect("log").clone();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, ReferencePickerEventKind::Pick);
        assert_eq!(events[0].id, 77);
        assert_eq!(events[0].text.as_str(), "Acme Corp");
    }

    #[test]
    fn picking_the_create_row_reports_the_query() {
        let (s, log) = styled(sample());
        let opts = options(&s);
        let _ = rv::fire(
            &s,
            node(opts[2]),
            EventFilter::Hover(HoverEventFilter::Click),
        )
        .expect("the create row takes the click");
        let events = log.lock().expect("log").clone();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, ReferencePickerEventKind::Create);
        assert_eq!(events[0].text.as_str(), "acme");
    }

    #[test]
    fn the_list_lists_at_most_max_rows_and_counts_the_rest() {
        let dom = ReferencePicker::create(customers()).with_max_rows(2).dom();
        assert!(texts(&dom).iter().any(|t| t == "2 more - type to narrow"));
    }
}
