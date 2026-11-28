//! The LOCATION block of the first tab (tab 0, "File"): where the user is, as two lines.
//!
//! ```text
//!   azul / assets / fonts /          line 1: the parent folders as ONE line of links
//!   Fonts  ✎                         line 2: the current name, big and bold, with a pen
//! ```
//!
//! The File menu is gone: tab 0 shows the location instead, in a band exactly as tall as every
//! other tab's ribbon band ([`crate::widgets::tabs::TAB_BAND_PX`]), so flipping between File and
//! Home (a vertical swipe on the band, the same on every app) never moves what is below. Hand the
//! block to the tab: `RibbonTab::with_location(LocationBar::...dom())`.
//!
//! The pen turns the name into a text field. The field is LIVE: every key is reported
//! ([`LocationEventKind::EditChanged`]), so the list under the band can follow what is being
//! typed; Enter takes the text ([`LocationEventKind::EditCommitted`]), Escape or a click
//! elsewhere gives the edit up ([`LocationEventKind::EditCancelled`]). The bar owns nothing: the
//! app keeps `editing` and the text, hears every action through ONE callback ([`LocationOnEvent`])
//! and rebuilds. A trail too long for its line folds its leading folders into a "..." link that
//! goes to the nearest folder it hides.
//!
//! The field opens with the NAME in it and the caret behind it (typing appends: a rename). A
//! block whose field is where the user types where to GO (a drive's folder, with the list below
//! narrowing as they type) calls [`LocationBar::with_name_as_placeholder`]: the field opens empty and
//! says the name, typing replaces it, and Enter on an empty field gives the edit up.
//!
//! The same block serves every app whose first tab names a place: a drive's folder, a mail
//! account and its folder ("Outlook Privat / Inbox"), a calendar's year and month, a document's
//! name. An app with nothing to edit leaves the pen out ([`LocationBar::with_editable`]).
//!
//! Key types: [`LocationBar`], [`LocationCrumb`], [`LocationEvent`], [`LocationEventKind`],
//! [`LocationOnEvent`].

use alloc::{format, string::String, vec::Vec};

use azul_core::{
    a11y::{AccessibilityInfo, AccessibilityRole},
    callbacks::{CoreCallback, CoreCallbackData, FocusTarget, Update},
    dom::{ComponentEventFilter, Dom, DomVec, EventFilter, HoverEventFilter, IdOrClass, IdOrClassVec, TabIndex},
    refany::{OptionRefAny, RefAny},
    window::VirtualKeyCode,
};
use azul_css::{
    dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
    impl_option, impl_vec, impl_vec_clone, impl_vec_debug, impl_vec_mut, impl_vec_partialeq,
    props::{basic::StyleFontSize, property::CssProperty},
    AzString,
};

use super::{
    sidebar_layout::{chrome_text_cap, FormFactor, TOUCH_PLATFORM},
    text_input::{
        OnTextInputReturn, TextInput, TextInputOnFocusLostCallbackType,
        TextInputOnTextInputCallbackType, TextInputOnVirtualKeyDownCallbackType, TextInputState,
        TextInputValid,
    },
    themes::{decl, OptionUiTheme, UiTheme},
    widget_p_with_text,
};
use crate::callbacks::CallbackInfo;

/// The block's root.
pub const LOCATION_CLASS: &str = "__azul-native-location";
/// The line of parent folders.
pub const LOCATION_CRUMBS_CLASS: &str = "__azul-native-location-crumbs";
/// A parent folder's link.
pub const LOCATION_CRUMB_CLASS: &str = "__azul-native-location-crumb";
/// The separator between two folders.
pub const LOCATION_SEPARATOR_CLASS: &str = "__azul-native-location-separator";
/// The line with the current name and the pen.
pub const LOCATION_NAME_ROW_CLASS: &str = "__azul-native-location-name-row";
/// The current name, big and bold.
pub const LOCATION_NAME_CLASS: &str = "__azul-native-location-name";
/// The pen.
pub const LOCATION_PEN_CLASS: &str = "__azul-native-location-pen";
/// The "..." button beside the pen: where File's old items (new window, preferences, switch
/// workspace, export, exit) live.
pub const LOCATION_MORE_CLASS: &str = "__azul-native-location-more";
/// The box round the text field while the name is edited.
pub const LOCATION_EDIT_CLASS: &str = "__azul-native-location-edit";

/// The most characters of the trail one line shows on a desktop, and on a phone: a trail longer
/// than this folds its leading folders into "...". (A count of characters, not of pixels: the
/// line is laid out in a flex row that clips, so this is the room the design gives, and the same
/// on every font.)
pub const TRAIL_CHARS_DESKTOP: usize = 72;
pub const TRAIL_CHARS_PHONE: usize = 34;
/// The most parent folders the block builds (a path deeper than this keeps its last ones).
pub const MAX_CRUMBS: usize = 64;

/// A parent folder in the trail: a link.
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct LocationCrumb {
    /// The folder's name.
    pub label: AzString,
    /// What the app calls it: handed back when it is clicked.
    pub tag: usize,
}

impl_option!(LocationCrumb, OptionLocationCrumb, copy = false, [Debug, Clone, PartialEq]);
impl_vec!(
    LocationCrumb,
    LocationCrumbVec,
    LocationCrumbVecDestructor,
    LocationCrumbVecDestructorType,
    LocationCrumbVecSlice,
    OptionLocationCrumb
);
impl_vec_clone!(LocationCrumb, LocationCrumbVec, LocationCrumbVecDestructor);
impl_vec_debug!(LocationCrumb, LocationCrumbVec);
impl_vec_partialeq!(LocationCrumb, LocationCrumbVec);
impl_vec_mut!(LocationCrumb, LocationCrumbVec);

impl LocationCrumb {
    /// A folder `label`, called `tag` by the app.
    #[must_use]
    pub const fn create(label: AzString, tag: usize) -> Self {
        Self { label, tag }
    }
}

/// What happened on the block.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum LocationEventKind {
    /// A parent folder (or the "..." that stands for the hidden ones) was clicked: `tag` is its
    /// tag.
    CrumbClicked,
    /// The pen was clicked: the app sets `editing` and rebuilds.
    EditStarted,
    /// A key changed the text of the field: `text` is the text now (the list below follows it).
    EditChanged,
    /// Enter: `text` is what was typed.
    EditCommitted,
    /// Escape, or the field lost the focus: the edit is given up.
    EditCancelled,
    /// The "..." button beside the pen was clicked: the app shows File's old items (a phone's
    /// action sheet, a desktop's menu).
    MoreClicked,
}

/// One report of the block.
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct LocationEvent {
    /// What happened.
    pub kind: LocationEventKind,
    /// The folder's tag (`CrumbClicked`); 0 otherwise.
    pub tag: usize,
    /// The text of the field (`EditChanged`, `EditCommitted`); empty otherwise.
    pub text: AzString,
}

/// Callback invoked for every action on the block.
pub type LocationOnEventCallbackType = extern "C" fn(RefAny, CallbackInfo, LocationEvent) -> Update;
impl_widget_callback!(
    LocationOnEvent,
    OptionLocationOnEvent,
    LocationOnEventCallback,
    LocationOnEventCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        LocationOnEventCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: LOCATION_ON_EVENT_INVOKER,
    invoker_ty:     AzLocationOnEventCallbackInvoker,
    thunk_fn:       az_location_on_event_callback_thunk,
    setter_fn:      AzApp_setLocationOnEventCallbackInvoker,
    from_handle_fn: AzLocationOnEventCallback_createFromHostHandle,
    from_handle_byref_fn: AzLocationOnEventCallback_createFromHostHandleByref,
    extra_args:     [ event: LocationEvent ],
}

/// The location block: the trail of parent folders over the current name.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct LocationBar {
    /// Every action on the block.
    pub on_event: OptionLocationOnEvent,
    /// The parent folders, outermost first.
    pub parents: LocationCrumbVec,
    /// The current name, big and bold ("Fonts").
    pub name: AzString,
    /// The text of the field while `editing`; empty: the name (or an empty field, see `name_as_placeholder`).
    pub text: AzString,
    /// What the empty field says ("Type a path or a name").
    pub placeholder: AzString,
    /// What stands between two folders ("/").
    pub separator: AzString,
    /// The widget theme this block is PINNED to, or `None` to follow the app theme.
    pub theme: OptionUiTheme,
    /// Desktop or phone: `Auto` asks the window.
    pub form: FormFactor,
    /// Whether there is a pen.
    pub editable: bool,
    /// Whether the name is a text field now (the app sets it when the pen was clicked).
    pub editing: bool,
    /// Whether a "..." button stands beside the pen: File's old items (new window, preferences,
    /// switch workspace, export, exit) are behind it, the app shows them on a click
    /// ([`LocationEventKind::MoreClicked`]).
    pub more: bool,
    /// Whether the field OPENS EMPTY with the name as its placeholder, so that typing replaces the name (a
    /// location the user types where to go to, the list below narrowing as they type) instead of
    /// appending to it; Enter on an empty field is [`LocationEventKind::EditCancelled`] then.
    pub name_as_placeholder: bool,
}

impl LocationBar {
    /// A block of `name`, no parents, with a pen, not editing.
    #[must_use]
    pub const fn create(name: AzString) -> Self {
        Self {
            parents: LocationCrumbVec::from_const_slice(&[]),
            name,
            editable: true,
            editing: false,
            more: false,
            name_as_placeholder: false,
            text: AzString::from_const_str(""),
            placeholder: AzString::from_const_str(""),
            separator: AzString::from_const_str("/"),
            form: FormFactor::Auto,
            on_event: OptionLocationOnEvent::None,
            theme: OptionUiTheme::None,
        }
    }

    /// Appends a parent folder.
    pub fn add_parent(&mut self, crumb: LocationCrumb) {
        let mut v = self.parents.clone().into_library_owned_vec();
        v.push(crumb);
        self.parents = LocationCrumbVec::from_vec(v);
    }

    /// [`Self::add_parent`] for the builder chain.
    #[must_use]
    pub fn with_parent(mut self, crumb: LocationCrumb) -> Self {
        self.add_parent(crumb);
        self
    }

    /// Replaces the parent folders.
    pub fn set_parents(&mut self, parents: LocationCrumbVec) {
        self.parents = parents;
    }

    /// [`Self::set_parents`] for the builder chain.
    #[must_use]
    pub fn with_parents(mut self, parents: LocationCrumbVec) -> Self {
        self.set_parents(parents);
        self
    }

    /// Whether there is a pen.
    pub const fn set_editable(&mut self, editable: bool) {
        self.editable = editable;
    }

    /// [`Self::set_editable`] for the builder chain.
    #[must_use]
    pub const fn with_editable(mut self, editable: bool) -> Self {
        self.set_editable(editable);
        self
    }

    /// Whether a "..." button stands beside the pen.
    pub const fn set_more(&mut self, more: bool) {
        self.more = more;
    }

    /// [`Self::set_more`] for the builder chain.
    #[must_use]
    pub const fn with_more(mut self, more: bool) -> Self {
        self.set_more(more);
        self
    }

    /// Whether the field opens empty with the name as its placeholder (typing replaces the name).
    pub const fn set_name_as_placeholder(&mut self, name_as_placeholder: bool) {
        self.name_as_placeholder = name_as_placeholder;
    }

    /// [`Self::set_name_as_placeholder`] for the builder chain.
    #[must_use]
    pub const fn with_name_as_placeholder(mut self, name_as_placeholder: bool) -> Self {
        self.set_name_as_placeholder(name_as_placeholder);
        self
    }

    /// The name is a text field holding `text` (empty: the name, or an empty field with the name as its
    /// placeholder where [`Self::name_as_placeholder`] is set).
    pub fn set_editing(&mut self, editing: bool, text: AzString) {
        self.editing = editing;
        self.text = text;
    }

    /// [`Self::set_editing`] for the builder chain.
    #[must_use]
    pub fn with_editing(mut self, editing: bool, text: AzString) -> Self {
        self.set_editing(editing, text);
        self
    }

    /// What the empty field says.
    pub fn set_placeholder(&mut self, placeholder: AzString) {
        self.placeholder = placeholder;
    }

    /// [`Self::set_placeholder`] for the builder chain.
    #[must_use]
    pub fn with_placeholder(mut self, placeholder: AzString) -> Self {
        self.set_placeholder(placeholder);
        self
    }

    /// What stands between two folders.
    pub fn set_separator(&mut self, separator: AzString) {
        self.separator = separator;
    }

    /// [`Self::set_separator`] for the builder chain.
    #[must_use]
    pub fn with_separator(mut self, separator: AzString) -> Self {
        self.set_separator(separator);
        self
    }

    /// Desktop or phone: `Auto` (the default) asks the window.
    pub const fn set_form(&mut self, form: FormFactor) {
        self.form = form;
    }

    /// [`Self::set_form`] for the builder chain.
    #[must_use]
    pub const fn with_form(mut self, form: FormFactor) -> Self {
        self.set_form(form);
        self
    }

    /// Pin the widget theme; unset, the block follows the app theme.
    pub const fn set_theme(&mut self, theme: UiTheme) {
        self.theme = OptionUiTheme::Some(theme);
    }

    /// [`Self::set_theme`] for the builder chain.
    #[must_use]
    pub const fn with_theme(mut self, theme: UiTheme) -> Self {
        self.set_theme(theme);
        self
    }

    /// Every action on the block.
    pub fn set_on_event<C: Into<LocationOnEventCallback>>(&mut self, data: RefAny, on_event: C) {
        self.on_event = OptionLocationOnEvent::Some(LocationOnEvent {
            refany: data,
            callback: on_event.into(),
        });
    }

    /// [`Self::set_on_event`] for the builder chain.
    #[must_use]
    pub fn with_on_event<C: Into<LocationOnEventCallback>>(mut self, data: RefAny, on_event: C) -> Self {
        self.set_on_event(data, on_event);
        self
    }

    /// The block's DOM: it fills the tab band (`width: 100%`, `height: 100%`).
    #[must_use]
    pub fn dom(self) -> Dom {
        build(self)
    }
}

impl Default for LocationBar {
    fn default() -> Self {
        Self::create(AzString::from_const_str(""))
    }
}

impl From<LocationBar> for Dom {
    fn from(l: LocationBar) -> Self {
        l.dom()
    }
}

/// How many of the leading folders of a trail fold into "...": the fewest that make what is left
/// fit `max_chars` (each folder counts its name and the separator after it). The last folder is
/// never folded, however long it is.
#[must_use]
pub fn folded_parents(labels: &[&str], separator: &str, max_chars: usize) -> usize {
    let sep = separator.chars().count() + 2; // a space either side
    let width = |label: &&str| label.chars().count() + sep;
    let mut total: usize = labels.iter().map(width).sum();
    let mut folded = 0usize;
    // the "..." link takes room too
    while total > max_chars && folded + 1 < labels.len() {
        total -= width(&labels[folded]);
        folded += 1;
        if folded == 1 {
            total += 3 + sep;
        }
    }
    folded
}

// ---------------------------------------------------------------------------
// The report
// ---------------------------------------------------------------------------

/// What a part carries: the app's callback and the event of the part.
struct PartRef {
    on_event: OptionLocationOnEvent,
    kind: LocationEventKind,
    tag: usize,
    /// Enter on an empty field gives the edit up (the field opened empty: nothing was typed).
    empty_cancels: bool,
}

/// What a key in the field reports: Enter commits what was typed (an empty field cancels where it opened empty),
/// Escape gives the edit up, any other key nothing.
fn key_kind(key: Option<VirtualKeyCode>, empty: bool, empty_cancels: bool) -> Option<LocationEventKind> {
    match key {
        Some(VirtualKeyCode::Return | VirtualKeyCode::NumpadEnter) => {
            Some(if empty && empty_cancels { LocationEventKind::EditCancelled } else { LocationEventKind::EditCommitted })
        }
        Some(VirtualKeyCode::Escape) => Some(LocationEventKind::EditCancelled),
        _ => None,
    }
}

fn emit(on_event: &OptionLocationOnEvent, info: CallbackInfo, kind: LocationEventKind, tag: usize, text: AzString) -> Update {
    match on_event.as_ref() {
        Some(LocationOnEvent { callback, refany }) => {
            callback.invoke(refany.clone(), info, LocationEvent { kind, tag, text })
        }
        None => Update::DoNothing,
    }
}

extern "C" fn on_part_click(mut data: RefAny, info: CallbackInfo) -> Update {
    let Some(part) = data.downcast_ref::<PartRef>() else {
        return Update::DoNothing;
    };
    emit(&part.on_event, info, part.kind, part.tag, AzString::from_const_str(""))
}

/// The field changed: the list below follows the text.
extern "C" fn on_field_text(mut data: RefAny, info: CallbackInfo, state: TextInputState) -> OnTextInputReturn {
    let update = match data.downcast_ref::<PartRef>() {
        Some(part) => emit(&part.on_event, info, LocationEventKind::EditChanged, 0, AzString::from(state.get_text())),
        None => Update::DoNothing,
    };
    OnTextInputReturn {
        update,
        valid: TextInputValid::Yes,
    }
}

/// Enter takes the text, Escape gives the edit up.
extern "C" fn on_field_key(mut data: RefAny, info: CallbackInfo, state: TextInputState) -> OnTextInputReturn {
    let key = info.get_current_keyboard_state().current_virtual_keycode.into_option();
    let text = state.get_text();
    let update = match data.downcast_ref::<PartRef>() {
        Some(part) => match key_kind(key, text.is_empty(), part.empty_cancels) {
            Some(kind) => emit(&part.on_event, info, kind, 0, AzString::from(text)),
            None => Update::DoNothing,
        },
        None => Update::DoNothing,
    };
    OnTextInputReturn {
        update,
        valid: TextInputValid::Yes,
    }
}

/// A click elsewhere: the edit is given up.
extern "C" fn on_field_blur(mut data: RefAny, info: CallbackInfo, _state: TextInputState) -> Update {
    match data.downcast_ref::<PartRef>() {
        Some(part) => emit(&part.on_event, info, LocationEventKind::EditCancelled, 0, AzString::from_const_str("")),
        None => Update::DoNothing,
    }
}

/// The field appeared: it takes the keyboard (the wrapper hands the focus to its first focusable
/// node, the input). Without it neither Escape nor a click elsewhere could end the edit.
extern "C" fn on_field_mounted(_data: RefAny, mut info: CallbackInfo) -> Update {
    let node = info.get_hit_node();
    info.set_focus(FocusTarget::Id(node));
    Update::DoNothing
}

// ---------------------------------------------------------------------------
// The build
// ---------------------------------------------------------------------------

fn classes(list: &[&'static str]) -> IdOrClassVec {
    IdOrClassVec::from_vec(
        list.iter()
            .map(|c| IdOrClass::Class(AzString::from_const_str(c)))
            .collect::<Vec<_>>(),
    )
}

fn styled(mut dom: Dom, css: &str) -> Dom {
    dom.root.set_css(css);
    dom
}

fn click(dom: Dom, data: RefAny) -> Dom {
    dom.with_callbacks(
        alloc::vec![CoreCallbackData {
            event: EventFilter::Hover(HoverEventFilter::Click),
            callback: CoreCallback {
                cb: on_part_click as usize,
                ctx: OptionRefAny::None,
            },
            refany: data,
        }]
        .into(),
    )
}

/// One link of the trail.
fn crumb(label: &str, tag: usize, kind: LocationEventKind, on_event: &OptionLocationOnEvent, phone: bool) -> Dom {
    let h = if phone { 32 } else { 20 };
    let node = styled(
        widget_p_with_text(AzString::from(label)),
        &format!(
            "margin: 0px; flex-shrink: 0; min-height: {h}px; display: flex; align-items: center; cursor: pointer; \
             font-size: 13px; color: system:link; white-space: nowrap; :hover {{ text-decoration: underline; }}"
        ),
    )
    .with_ids_and_classes(classes(&[LOCATION_CRUMB_CLASS]))
    .with_accessibility_info(AccessibilityInfo::named(label, AccessibilityRole::Link))
    .with_tab_index(TabIndex::Auto);
    click(
        node,
        RefAny::new(PartRef {
            on_event: on_event.clone(),
            kind,
            tag,
            empty_cancels: false,
        }),
    )
}

fn separator(text: &str) -> Dom {
    styled(
        widget_p_with_text(AzString::from(text)),
        "margin: 0px; flex-shrink: 0; font-size: 13px; color: system:secondary-text; white-space: nowrap;",
    )
    .with_ids_and_classes(classes(&[LOCATION_SEPARATOR_CLASS]))
}

fn build(bar: LocationBar) -> Dom {
    let LocationBar {
        parents,
        name,
        editable,
        editing,
        text,
        placeholder,
        separator: sep,
        form,
        on_event,
        theme,
        more,
        name_as_placeholder,
    } = bar;
    let phone = form.is_phone();
    let mut parents = parents.into_library_owned_vec();
    if parents.len() > MAX_CRUMBS {
        let extra = parents.len() - MAX_CRUMBS;
        parents.drain(..extra);
        azul_core::diagnostics::emit_once(0x10ca_7101, || {
            format!("LocationBar: the {extra} outermost parent folder(s) past the limit of {MAX_CRUMBS} were dropped")
        });
    }
    let sep_text = if sep.as_str().is_empty() { "/" } else { sep.as_str() };

    // ---- line 1: the trail ----
    let labels: Vec<&str> = parents.iter().map(|c| c.label.as_str()).collect();
    let folded = folded_parents(
        &labels,
        sep_text,
        if phone { TRAIL_CHARS_PHONE } else { TRAIL_CHARS_DESKTOP },
    );
    let mut trail: Vec<Dom> = Vec::with_capacity(parents.len() * 2 + 2);
    if folded > 0 {
        // the "..." goes to the nearest folder it hides
        trail.push(crumb("...", parents[folded - 1].tag, LocationEventKind::CrumbClicked, &on_event, phone));
        trail.push(separator(sep_text));
    }
    for (i, p) in parents.iter().enumerate().skip(folded) {
        trail.push(crumb(p.label.as_str(), p.tag, LocationEventKind::CrumbClicked, &on_event, phone));
        trail.push(separator(sep_text));
        let _ = i;
    }
    let crumbs = styled(
        Dom::create_div().with_children(DomVec::from_vec(trail)),
        "display: flex; flex-direction: row; align-items: center; gap: 6px; flex-shrink: 0; min-height: 20px; \
         overflow: hidden; white-space: nowrap;",
    )
    .with_ids_and_classes(classes(&[LOCATION_CRUMBS_CLASS]));

    // ---- line 2: the name and the pen, or the field ----
    let more_data = RefAny::new(PartRef {
        on_event: on_event.clone(),
        kind: LocationEventKind::MoreClicked,
        tag: 0,
        empty_cancels: false,
    });
    let shared = RefAny::new(PartRef {
        on_event,
        kind: LocationEventKind::EditStarted,
        tag: 0,
        empty_cancels: name_as_placeholder,
    });
    let mut row: Vec<Dom> = Vec::with_capacity(2);
    if editing {
        let on_input: TextInputOnTextInputCallbackType = on_field_text;
        let on_key: TextInputOnVirtualKeyDownCallbackType = on_field_key;
        let on_blur: TextInputOnFocusLostCallbackType = on_field_blur;
        // the name in the field, caret behind it (typing appends), or an empty field that says the name (typing replaces)
        let (start, placeholder) = if name_as_placeholder {
            (text, name)
        } else if text.as_str().is_empty() {
            (name, placeholder)
        } else {
            (text, placeholder)
        };
        let mut input = TextInput::create()
            .with_text(start)
            .with_accessibility_name(AzString::from_const_str("Location"))
            .with_on_text_input(shared.clone(), on_input)
            .with_on_virtual_key_down(shared.clone(), on_key)
            .with_on_focus_lost(shared.clone(), on_blur);
        if !placeholder.as_str().is_empty() {
            input = input.with_placeholder(placeholder);
        }
        if let Some(t) = theme.into_option() {
            input = input.with_theme(t);
        }
        // the big bold name, as a field
        let mut label_style = input.resolved_label_style().into_library_owned_vec();
        label_style.push(CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(
            if phone { 22 } else { 24 },
        ))));
        label_style.push(decl::bold());
        let input = input.with_label_style(CssPropertyWithConditionsVec::from_vec(label_style));
        row.push(
            styled(
                Dom::create_div().with_child(input.dom()),
                "display: flex; flex-direction: row; align-items: center; flex-grow: 1; min-width: 0px;",
            )
            .with_ids_and_classes(classes(&[LOCATION_EDIT_CLASS]))
            .with_callbacks(
                alloc::vec![CoreCallbackData {
                    event: EventFilter::Component(ComponentEventFilter::AfterMount),
                    callback: CoreCallback {
                        cb: on_field_mounted as usize,
                        ctx: OptionRefAny::None,
                    },
                    refany: shared,
                }]
                .into(),
            ),
        );
    } else {
        row.push(
            styled(
                widget_p_with_text(name),
                &format!(
                    "margin: 0px; flex-shrink: 1; min-width: 0px; overflow: hidden; white-space: nowrap; \
                     text-overflow: ellipsis; font-size: {}px; font-weight: 700; line-height: 1.15;",
                    if phone { 26 } else { 28 }
                ),
            )
            .with_ids_and_classes(classes(&[LOCATION_NAME_CLASS])),
        );
        if editable {
            let pen = styled(
                Dom::create_div().with_child(styled(
                    Dom::create_icon(AzString::from_const_str("edit")),
                    "font-size: 18px; width: 20px; text-align: center;",
                )),
                &format!(
                    "display: flex; align-items: center; justify-content: center; flex-shrink: 0; \
                     box-sizing: border-box; width: {s}px; height: {s}px; border-radius: 6px; cursor: pointer; \
                     color: system:secondary-text; :hover {{ background: rgba(127, 127, 127, 0.18); }} \
                     :active {{ background: rgba(127, 127, 127, 0.28); }} \
                     :focus-visible {{ box-shadow: inset 0px 0px 0px 2px #4a8be8; }}",
                    s = if phone || TOUCH_PLATFORM { 48 } else { 30 }
                ),
            )
            .with_ids_and_classes(classes(&[LOCATION_PEN_CLASS]))
            .with_accessibility_info(AccessibilityInfo::named("Edit the location", AccessibilityRole::PushButton))
            .with_tab_index(TabIndex::Auto);
            row.push(click(pen, shared));
        }
        // File's old items live behind a "..." beside the pen
        if more {
            let button = styled(
                Dom::create_div().with_child(styled(
                    Dom::create_icon(AzString::from_const_str("more_horiz")),
                    "font-size: 20px; width: 22px; text-align: center;",
                )),
                &format!(
                    "display: flex; align-items: center; justify-content: center; flex-shrink: 0; \
                     box-sizing: border-box; width: {s}px; height: {s}px; border-radius: 6px; cursor: pointer; \
                     color: system:secondary-text; :hover {{ background: rgba(127, 127, 127, 0.18); }} \
                     :active {{ background: rgba(127, 127, 127, 0.28); }} \
                     :focus-visible {{ box-shadow: inset 0px 0px 0px 2px #4a8be8; }}",
                    s = if phone || TOUCH_PLATFORM { 48 } else { 30 }
                ),
            )
            .with_ids_and_classes(classes(&[LOCATION_MORE_CLASS]))
            .with_accessibility_info(AccessibilityInfo::named("More", AccessibilityRole::PushButton))
            .with_tab_index(TabIndex::Auto);
            row.push(click(button, more_data));
        }
    }
    let name_row = styled(
        Dom::create_div().with_children(DomVec::from_vec(row)),
        "display: flex; flex-direction: row; align-items: center; gap: 8px; flex-shrink: 0; min-width: 0px;",
    )
    .with_ids_and_classes(classes(&[LOCATION_NAME_ROW_CLASS]));

    styled(
        Dom::create_div().with_children(DomVec::from_vec(alloc::vec![crumbs, name_row])),
        &format!(
            "display: flex; flex-direction: column; justify-content: center; gap: 2px; box-sizing: border-box; \
             width: 100%; height: 100%; padding: 8px {}px; overflow: hidden; {}",
            if phone { 12 } else { 16 },
            // the block stands in the tab band, a fixed 106 px: its text keeps to it whatever the text size is
            chrome_text_cap()
        ),
    )
    .with_ids_and_classes(classes(&[LOCATION_CLASS]))
}

/// The text of a string for tests that read what a block says.
#[cfg(test)]
pub(crate) fn text_of(dom: &Dom) -> String {
    use azul_core::dom::NodeType;
    let mut out = String::new();
    if let NodeType::Text(t) = dom.root.get_node_type() {
        out.push_str(t.as_ref().as_str());
    }
    for child in dom.children.as_ref() {
        out.push_str(&text_of(child));
    }
    out
}

#[cfg(test)]
#[path = "location_bar_test.rs"]
mod location_bar_test;
