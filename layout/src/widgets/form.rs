//! `<form>`: named controls, a submit that collects them, a reset that
//! restores them.
//!
//! # Design (deliberately small)
//!
//! azul already had the ENGINE half of a form: `NodeType::Form`, the `Submit`
//! / `Reset` / `Invalid` events, Enter-to-submit and reset-button default
//! actions, and HTML constraint validation over DOM attributes
//! (`crate::form`). What it lacked is the WIDGET half: the values live in the
//! widgets' states, not in the DOM, and nothing collected them. This module is
//! that half and nothing more:
//!
//! * A control PARTICIPATES by carrying a `name` attribute on the node that
//!   holds its state as its dataset - `TextInput::with_name`,
//!   `DatePicker::with_name`, `DateTimeLocalPicker::with_name`,
//!   `HiddenInput`. Any other node with `name` and `value` attributes
//!   contributes its `value`. Controls without a name, and disabled ones,
//!   are left out, as in HTML.
//! * A RAW control (`<input>`, `<select>`, `<textarea>`) participates too:
//!   the form-control replacement (`crate::form_controls`) turns it into a
//!   widget and knows how HTML spells its value - a checkbox's `value` ("on"
//!   by default) only while checked, a radio group's checked value, a
//!   select's chosen option VALUE, a range's number - both for the form's
//!   initial values (its HTML default) and for a submit (the user's value,
//!   read from the window's `FormControlMemory`).
//! * [`Form`] renders a `NodeType::Form` node (so the engine's defaults find
//!   it) holding a [`FormStateWrapper`] as its dataset (a raw `<form>` keeps
//!   the APP's dataset there; the state is then found through the node's
//!   `Submit` handler, [`form_state_of`]). At BUILD it records
//!   each named control's value as the form's INITIAL [`FormData`] - HTML's
//!   default values are the values the page was built with, and a widget is
//!   built from the app's state.
//! * SUBMIT (a submit/image button, Enter in a text field, or the engine's
//!   `Submit` event) walks the form's subtree in document order and hands the
//!   app's `on_submit` the CURRENT values, plus the names of the controls that
//!   fail their constraints, whose fields it marks with the `:user-invalid`
//!   look. HTML would refuse to submit an invalid form; azul has no browser
//!   bubble to show instead, so the app decides (`FormData::is_valid`).
//! * RESET (a reset button, or the engine's `Reset` event) puts every text
//!   field back to its initial value itself and hands the app's `on_reset` the
//!   initial `FormData`, from which it restores whatever else it built the
//!   form from (pickers are rebuilt from the app's state).
//!
//! Key types: [`Form`], [`FormData`], [`FormEntry`], [`FormOnSubmit`],
//! [`FormOnReset`]; free functions [`submit_form`], [`reset_form`],
//! [`collect_form_data`].
//!
//! A raw `<form>` (`Dom::create_form*`, XML) becomes a [`Form`] before the
//! cascade (`crate::form_controls`), and so do its raw controls - into the
//! widgets this module reads.

use alloc::{string::String, vec::Vec};

use azul_core::{
    callbacks::{CoreCallback, Update},
    dom::{
        AttributeType, Dom, DomNodeId, DomVec, EventFilter, HoverEventFilter, NodeData, NodeId,
        NodeType,
    },
    refany::{OptionRefAny, RefAny},
    styled_dom::NodeHierarchyItemId,
};
use azul_css::{
    dynamic_selector::{CssPropertyWithConditionsVec, OptionCssPropertyWithConditionsVec},
    impl_option, impl_vec, impl_vec_clone, impl_vec_debug, impl_vec_mut,
    impl_vec_partialeq, AzString, OptionString, StringVec,
};

use crate::{
    callbacks::CallbackInfo,
    form_controls::{FormValue, Submission},
    widgets::{
        button::ButtonFormAction,
        combobox::ComboBoxStateWrapper,
        date_picker::DatePickerData,
        radio_group::RadioGroupStateWrapper,
        datetime_local::DateTimeLocalPickerStateWrapper,
        text_area::TextAreaStateWrapper,
        text_input::{TextInputKind, TextInputStateWrapper},
    },
};

/// The class of the form node.
pub const FORM_CLASS: &str = "__azul-native-form";

/// The form node's structure, the same in every theme (R5): its content
/// stacked in a column. Each theme's skin (the gap between the controls)
/// comes after it, so the merge (`themes::theme_blocks`) declares it once,
/// outside every `@theme` block.
#[must_use]
pub(crate) fn base_form() -> Vec<azul_css::dynamic_selector::CssPropertyWithConditions> {
    use azul_css::{
        dynamic_selector::CssPropertyWithConditions,
        props::{
            layout::{LayoutDisplay, LayoutFlexDirection},
            property::CssProperty,
        },
    };
    alloc::vec![
        CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
        CssPropertyWithConditions::simple(CssProperty::const_flex_direction(
            LayoutFlexDirection::Column,
        )),
    ]
}

/// One named value of a [`FormData`].
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct FormEntry {
    pub name: AzString,
    pub value: AzString,
}

impl_option!(
    FormEntry,
    OptionFormEntry,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
impl_vec!(
    FormEntry,
    FormEntryVec,
    FormEntryVecDestructor,
    FormEntryVecDestructorType,
    FormEntryVecSlice,
    OptionFormEntry
);
impl_vec_clone!(FormEntry, FormEntryVec, FormEntryVecDestructor);
impl_vec_debug!(FormEntry, FormEntryVec);
impl_vec_mut!(FormEntry, FormEntryVec);
impl_vec_partialeq!(FormEntry, FormEntryVec);

/// A form's values: HTML's `FormData`, a MULTIMAP (one name may carry several
/// values) in document order, plus the names of the controls whose values
/// fail their constraints.
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct FormData {
    /// Every named control's value, in document order.
    pub entries: FormEntryVec,
    /// The names of the controls that fail their constraints (`pattern`,
    /// e-mail / URL syntax), in document order. Empty for a valid form.
    pub invalid: StringVec,
}

impl Default for FormData {
    fn default() -> Self {
        Self {
            entries: FormEntryVec::from_const_slice(&[]),
            invalid: StringVec::from_const_slice(&[]),
        }
    }
}

impl FormData {
    /// The FIRST value submitted under `name` (HTML's `FormData.get`).
    // owned AzString passed by value per the azul FFI / api.json convention.
    #[allow(clippy::needless_pass_by_value)]
    #[must_use]
    pub fn get(&self, name: AzString) -> OptionString {
        self.entries
            .as_ref()
            .iter()
            .find(|e| e.name == name)
            .map(|e| e.value.clone())
            .into()
    }

    /// EVERY value submitted under `name`, in document order
    /// (HTML's `FormData.getAll`).
    #[allow(clippy::needless_pass_by_value)]
    #[must_use]
    pub fn get_all(&self, name: AzString) -> StringVec {
        self.entries
            .as_ref()
            .iter()
            .filter(|e| e.name == name)
            .map(|e| e.value.clone())
            .collect::<Vec<_>>()
            .into()
    }

    /// Whether any value is submitted under `name`.
    #[allow(clippy::needless_pass_by_value)]
    #[must_use]
    pub fn has(&self, name: AzString) -> bool {
        self.entries.as_ref().iter().any(|e| e.name == name)
    }

    /// Whether every control passed its constraints.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.invalid.as_ref().is_empty()
    }
}

/// Callback type invoked on submit, with the form's CURRENT values.
pub type FormOnSubmitCallbackType = extern "C" fn(RefAny, CallbackInfo, FormData) -> Update;
impl_widget_callback!(
    FormOnSubmit,
    OptionFormOnSubmit,
    FormOnSubmitCallback,
    FormOnSubmitCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        FormOnSubmitCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: FORM_ON_SUBMIT_INVOKER,
    invoker_ty:     AzFormOnSubmitCallbackInvoker,
    thunk_fn:       az_form_on_submit_callback_thunk,
    setter_fn:      AzApp_setFormOnSubmitCallbackInvoker,
    from_handle_fn: AzFormOnSubmitCallback_createFromHostHandle,
    from_handle_byref_fn: AzFormOnSubmitCallback_createFromHostHandleByref,
    extra_args:     [ form_data: FormData ],
}

/// Callback type invoked on reset, with the form's INITIAL values (what the
/// text fields were just put back to).
pub type FormOnResetCallbackType = extern "C" fn(RefAny, CallbackInfo, FormData) -> Update;
impl_widget_callback!(
    FormOnReset,
    OptionFormOnReset,
    FormOnResetCallback,
    FormOnResetCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        FormOnResetCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: FORM_ON_RESET_INVOKER,
    invoker_ty:     AzFormOnResetCallbackInvoker,
    thunk_fn:       az_form_on_reset_callback_thunk,
    setter_fn:      AzApp_setFormOnResetCallbackInvoker,
    from_handle_fn: AzFormOnResetCallback_createFromHostHandle,
    from_handle_byref_fn: AzFormOnResetCallback_createFromHostHandleByref,
    extra_args:     [ form_data: FormData ],
}

/// The form node's dataset: the initial values recorded at build, and the
/// app's callbacks.
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct FormStateWrapper {
    /// Each named control's value when the form was BUILT - what a reset
    /// restores.
    pub initial: FormData,
    pub on_submit: OptionFormOnSubmit,
    pub on_reset: OptionFormOnReset,
}

impl Default for FormStateWrapper {
    fn default() -> Self {
        Self {
            initial: FormData::default(),
            on_submit: None.into(),
            on_reset: None.into(),
        }
    }
}

/// `<form>`: see the module docs.
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct Form {
    /// The form's content: the controls and whatever lays them out.
    pub children: DomVec,
    pub state: FormStateWrapper,
    /// Style for the form node, or `None` for the theme's.
    pub container_style: OptionCssPropertyWithConditionsVec,
    /// What this form is CALLED, for assistive technology.
    pub accessibility_name: OptionString,
    /// The widget theme this form is PINNED to (`with_theme`), or `None` to
    /// follow the app theme (`AppConfig::with_theme`,
    /// `CallbackInfo::set_theme`; flat unless the app chose another).
    pub theme: crate::widgets::themes::OptionUiTheme,
}

impl Default for Form {
    fn default() -> Self {
        Self::create(DomVec::from_const_slice(&[]))
    }
}

impl Form {
    /// A form around `children`.
    #[must_use]
    pub fn create(children: DomVec) -> Self {
        Self {
            children,
            state: FormStateWrapper::default(),
            container_style: OptionCssPropertyWithConditionsVec::None,
            accessibility_name: OptionString::None,
            theme: crate::widgets::themes::OptionUiTheme::None,
        }
    }

    /// Replace the form's content.
    pub fn set_children(&mut self, children: DomVec) {
        self.children = children;
    }

    /// [`Self::set_children`] for the builder chain.
    #[must_use]
    pub fn with_children(mut self, children: DomVec) -> Self {
        self.set_children(children);
        self
    }

    /// Append one child to the form's content.
    #[must_use]
    pub fn with_child(mut self, child: Dom) -> Self {
        let mut v = core::mem::replace(&mut self.children, DomVec::from_const_slice(&[]))
            .into_library_owned_vec();
        v.push(child);
        self.children = v.into();
        self
    }

    /// The callback a submit calls with the form's current values.
    pub fn set_on_submit<C: Into<FormOnSubmitCallback>>(&mut self, data: RefAny, callback: C) {
        self.state.on_submit = Some(FormOnSubmit {
            callback: callback.into(),
            refany: data,
        })
        .into();
    }

    /// [`Self::set_on_submit`] for the builder chain.
    #[must_use]
    pub fn with_on_submit<C: Into<FormOnSubmitCallback>>(
        mut self,
        data: RefAny,
        callback: C,
    ) -> Self {
        self.set_on_submit(data, callback);
        self
    }

    /// The callback a reset calls with the form's initial values.
    pub fn set_on_reset<C: Into<FormOnResetCallback>>(&mut self, data: RefAny, callback: C) {
        self.state.on_reset = Some(FormOnReset {
            callback: callback.into(),
            refany: data,
        })
        .into();
    }

    /// [`Self::set_on_reset`] for the builder chain.
    #[must_use]
    pub fn with_on_reset<C: Into<FormOnResetCallback>>(
        mut self,
        data: RefAny,
        callback: C,
    ) -> Self {
        self.set_on_reset(data, callback);
        self
    }

    /// Replace the form node's style (`None` = the theme's).
    pub fn set_container_style(&mut self, style: CssPropertyWithConditionsVec) {
        self.container_style = OptionCssPropertyWithConditionsVec::Some(style);
    }

    /// [`Self::set_container_style`] for the builder chain.
    #[must_use]
    pub fn with_container_style(mut self, style: CssPropertyWithConditionsVec) -> Self {
        self.set_container_style(style);
        self
    }

    /// Name this form for assistive technology.
    #[must_use]
    pub fn with_accessibility_name<S: Into<AzString>>(mut self, name: S) -> Self {
        self.accessibility_name = Some(name.into()).into();
        self
    }

    /// Pin the widget theme; unset, the form follows the app theme.
    pub const fn set_theme(&mut self, theme: crate::widgets::themes::UiTheme) {
        self.theme = crate::widgets::themes::OptionUiTheme::Some(theme);
    }

    /// [`Self::set_theme`] for the builder chain.
    #[must_use]
    pub const fn with_theme(mut self, theme: crate::widgets::themes::UiTheme) -> Self {
        self.set_theme(theme);
        self
    }

    /// Replaces `self` with the default value and returns the original.
    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::default();
        core::mem::swap(&mut s, self);
        s
    }

    /// Renders the form node, recording each named control's initial value.
    #[must_use]
    pub fn dom(self) -> Dom {
        use crate::widgets::themes::{flat, flora, theme_blocks, UiTheme};

        let mut state = self.state;
        state.initial = initial_form_data(self.children.as_ref());
        let shared = RefAny::new(state);

        let mut form = match self.theme.into_option() {
            Some(UiTheme::Flat) => flat::form(self.children),
            Some(UiTheme::Flora) => flora::form(self.children),
            // No theme: follow the app theme - both looks of the form node,
            // each inside its `@theme(<name>)` block. The content is the
            // caller's and alike in every theme, so it goes in once.
            None => theme_blocks::follow_dom(
                UiTheme::current(),
                flat::form(DomVec::from_const_slice(&[])),
                flora::form(DomVec::from_const_slice(&[])),
            )
            .with_children(self.children),
        };
        if let Some(style) = self.container_style.into_option() {
            form = form.with_css_props(style);
        }
        let mut form = form
            .with_dataset(Some(shared.clone()).into())
            // The engine's own form events land on the form node: Enter in a
            // control that does not edit text (`DefaultAction::SubmitForm`)
            // and Enter on a reset button (`DefaultAction::ResetForm`).
            .with_callback(
                EventFilter::Hover(HoverEventFilter::Submit),
                shared.clone(),
                CoreCallback {
                    cb: default_on_form_submit_event as usize,
                    ctx: OptionRefAny::None,
                },
            )
            .with_callback(
                EventFilter::Hover(HoverEventFilter::Reset),
                shared,
                CoreCallback {
                    cb: default_on_form_reset_event as usize,
                    ctx: OptionRefAny::None,
                },
            );
        if let Some(name) = self.accessibility_name.into_option() {
            // Named, a form is a `form` landmark; the role itself comes from
            // the node type.
            form = form.with_accessibility_info(azul_core::a11y::AccessibilityInfo {
                accessibility_name: Some(name).into(),
                ..Default::default()
            });
        }
        form
    }
}

impl From<Form> for Dom {
    fn from(f: Form) -> Self {
        f.dom()
    }
}

// ---------------------------------------------------------------------------
// Reading a control's value
// ---------------------------------------------------------------------------

/// A participating control's value and whether it passes its constraints.
struct ControlValue {
    value: AzString,
    valid: bool,
}

/// The LIVE value of the control whose state is `dataset`, if it is a widget
/// this module knows: the text the user typed, the date they picked.
fn probe_state(dataset: Option<RefAny>) -> Option<ControlValue> {
    let mut dataset = dataset?;
    // One probe per statement: each shared borrow of the state ends before
    // the next one is taken.
    let text = dataset
        .downcast_ref::<TextInputStateWrapper>()
        .map(|w| ControlValue {
            value: AzString::from(w.inner.get_text()),
            valid: w.inner.compute_validity().is_valid(),
        });
    if text.is_some() {
        return text;
    }
    let area = dataset
        .downcast_ref::<TextAreaStateWrapper>()
        .map(|w| ControlValue {
            value: AzString::from(w.inner.get_text()),
            valid: true,
        });
    if area.is_some() {
        return area;
    }
    let date = dataset
        .downcast_ref::<DatePickerData>()
        .map(|d| ControlValue {
            value: AzString::from(d.form_value()),
            valid: true,
        });
    if date.is_some() {
        return date;
    }
    let date_time = dataset
        .downcast_ref::<DateTimeLocalPickerStateWrapper>()
        .map(|w| ControlValue {
            value: AzString::from(w.inner.to_html_value()),
            valid: true,
        });
    if date_time.is_some() {
        return date_time;
    }
    // A combobox (`<input list>`): its field's text, typed or picked.
    let combo = dataset
        .downcast_ref::<ComboBoxStateWrapper>()
        .map(|w| ControlValue {
            value: w.inner.text.clone(),
            valid: true,
        });
    combo
}

/// The entries of a named control that is no replaced raw control and no
/// widget [`probe_state`] reads: a widget the app built, read off `states`
/// (its state as its dataset, its handlers' payloads, or its descendants'
/// carry it) and spelled like the replaced control of its kind
/// (`crate::form_controls::hand_built_entries`; `own_value` is a checkbox's
/// `value`) - a radio group's chosen option through `radio_label`, which
/// names option `n` from the DOM - or, for none of those, the node's `value`
/// attribute.
fn built_widget_values(
    states: &[RefAny],
    value_attribute: Option<AzString>,
    radio_label: impl Fn(usize) -> Option<String>,
) -> Vec<ControlValue> {
    let as_values = |entries: Vec<String>| -> Vec<ControlValue> {
        entries
            .into_iter()
            .map(|value| ControlValue {
                value: AzString::from(value),
                valid: true,
            })
            .collect()
    };
    let own_value = value_attribute.as_ref().map(|v| v.as_str().to_string());
    for state in states {
        if let Some(entries) = crate::form_controls::hand_built_entries(state, own_value.clone()) {
            return as_values(entries);
        }
        let radio = {
            let mut state = state.clone();
            let selected = state
                .downcast_ref::<RadioGroupStateWrapper>()
                .map(|w| w.inner.selected_index);
            selected
        };
        if let Some(selected) = radio {
            // Nothing while no option is chosen.
            return as_values(radio_label(selected).into_iter().collect());
        }
    }
    value_attribute
        .map(|value| ControlValue { value, valid: true })
        .into_iter()
        .collect()
}

/// What the form-control replacement (`crate::form_controls`) reports for a
/// raw control, as this module's [`ControlValue`]s - its entries, in order:
/// none for "submits nothing", one per picked file of a file input; `unknown`
/// answers when it knows nothing of the node.
fn from_submission(
    submission: Submission,
    unknown: impl FnOnce() -> Vec<ControlValue>,
) -> Vec<ControlValue> {
    match submission {
        Submission::Entries(values) => values
            .into_iter()
            .map(|value| ControlValue {
                value: AzString::from(value),
                valid: true,
            })
            .collect(),
        Submission::Unknown => unknown(),
    }
}

/// What the replacement's memory says the RENDERED replaced control `node`
/// submits now (by the [`crate::form_controls::MEMORY_KEY_ATTRIBUTE`] on its
/// root). [`Submission::Unknown`] for any other node.
fn replaced_submission(info: &CallbackInfo, node: DomNodeId) -> Submission {
    match memory_key_at(info, node) {
        Some(key) => info.get_layout_window().form_control_memory.submission(key),
        None => Submission::Unknown,
    }
}

/// Whether `dataset` is the state of a control this module reads.
fn is_control_state(dataset: &RefAny) -> bool {
    // One probe per statement: each shared borrow must end before the next
    // one is taken.
    let mut d = dataset.clone();
    if d.downcast_ref::<TextInputStateWrapper>().is_some() {
        return true;
    }
    if d.downcast_ref::<TextAreaStateWrapper>().is_some() {
        return true;
    }
    if d.downcast_ref::<DatePickerData>().is_some() {
        return true;
    }
    if d.downcast_ref::<DateTimeLocalPickerStateWrapper>().is_some() {
        return true;
    }
    let is_combobox = d.downcast_ref::<ComboBoxStateWrapper>().is_some();
    is_combobox
}

/// HTML leaves a DISABLED control out of the form data set.
fn is_disabled(attributes: &[AttributeType]) -> bool {
    attributes.iter().any(|a| matches!(a, AttributeType::Disabled))
}

/// The dataset of an UNSTYLED named node: its own, or - for a widget whose
/// root is a wrapper around the field holding the state (a `type=search`
/// row, or a root the form-control replacement put the `name` on) - its first
/// child's, when that is a control's state.
fn built_control_state(named: &Dom) -> Option<RefAny> {
    if let Some(own) = named.root.get_dataset() {
        return Some(own.clone());
    }
    named
        .children
        .as_ref()
        .first()
        .and_then(|child| child.root.get_dataset())
        .filter(|ds| is_control_state(ds))
        .cloned()
}

/// Every state an UNSTYLED control carries, in document order: its dataset,
/// its handlers' payloads, then its descendants' (a time picker keeps its
/// state on its spinners, a radio group on its rows).
fn built_states(control: &Dom) -> Vec<RefAny> {
    fn push(dom: &Dom, out: &mut Vec<RefAny>) {
        out.extend(dom.root.get_dataset().cloned());
        out.extend(dom.root.callbacks.as_ref().iter().map(|c| c.refany.clone()));
        for child in dom.children.as_ref() {
            push(child, out);
        }
    }
    let mut out = Vec::new();
    push(control, &mut out);
    out
}

/// [`built_states`] of the RENDERED control `node`.
fn carried_states(info: &CallbackInfo, node: DomNodeId) -> Vec<RefAny> {
    let Some(id) = node.node.into_crate_internal() else {
        return Vec::new();
    };
    let Some(layout) = info.get_layout_window().get_layout_result(&node.dom) else {
        return Vec::new();
    };
    let node_data = layout.styled_dom.node_data.as_container();
    let end = id.index() + 1 + layout.styled_dom.node_hierarchy.as_container().subtree_len(id);
    let mut out = Vec::new();
    for index in id.index()..end {
        if let Some(data) = node_data.get(NodeId::new(index)) {
            out.extend(data.get_dataset().cloned());
            out.extend(data.callbacks.as_ref().iter().map(|c| c.refany.clone()));
        }
    }
    out
}

/// The text of the `n`-th child of the rendered `node` (a radio group's
/// option row: its label).
fn nth_child_text(info: &CallbackInfo, node: DomNodeId, n: usize) -> Option<String> {
    let mut child = info.get_first_child(node)?;
    for _ in 0..n {
        child = info.get_next_sibling(child)?;
    }
    info.get_node_text_content(child)
}

/// [`built_control_state`] for a RENDERED named node: the node holding the
/// state (the named node or its first child) and the state.
fn control_state(info: &mut CallbackInfo, named: DomNodeId) -> (DomNodeId, Option<RefAny>) {
    if let Some(own) = info.get_dataset(named) {
        return (named, Some(own));
    }
    if let Some(child) = info.get_first_child(named) {
        if let Some(ds) = info.get_dataset(child) {
            if is_control_state(&ds) {
                return (child, Some(ds));
            }
        }
    }
    (named, None)
}

/// The `name` attribute among `attributes`.
fn name_of(attributes: &[AttributeType]) -> Option<AzString> {
    attributes.iter().find_map(|a| match a {
        AttributeType::Name(n) => Some(n.clone()),
        _ => None,
    })
}

/// The `value` attribute among `attributes`.
fn value_attribute_of(attributes: &[AttributeType]) -> Option<AzString> {
    attributes.iter().find_map(|a| match a {
        AttributeType::Value(v) => Some(v.clone()),
        _ => None,
    })
}

/// Collects `name`/value pairs from the UNSTYLED children of a form being
/// built. A named control's own subtree is its business and is not searched.
///
/// A RAW control (`<input>`, `<select>`, `<textarea>` the form-control
/// replacement turns into a widget once the form is built) contributes its
/// HTML default, spelled as that widget will submit it: a checkbox only
/// while checked, a select its chosen option's value.
fn collect_initial(children: &[Dom], entries: &mut Vec<FormEntry>, invalid: &mut Vec<AzString>) {
    for child in children {
        let attributes = child.root.attributes();
        if let Some(name) = name_of(attributes.as_ref()) {
            if name.as_str().is_empty() || is_disabled(attributes.as_ref()) {
                continue;
            }
            let values = from_submission(crate::form_controls::default_submission(child), || {
                if let Some(live) = probe_state(built_control_state(child)) {
                    return alloc::vec![live];
                }
                built_widget_values(
                    &built_states(child),
                    value_attribute_of(attributes.as_ref()),
                    |n| {
                        child
                            .children
                            .as_ref()
                            .get(n)
                            .map(crate::form_controls::text_content)
                    },
                )
            });
            for value in values {
                if !value.valid {
                    invalid.push(name.clone());
                }
                entries.push(FormEntry {
                    name: name.clone(),
                    value: value.value,
                });
            }
            continue;
        }
        collect_initial(child.children.as_ref(), entries, invalid);
    }
}

/// The initial [`FormData`] of a form built around `children`.
fn initial_form_data(children: &[Dom]) -> FormData {
    let mut entries = Vec::new();
    let mut invalid = Vec::new();
    collect_initial(children, &mut entries, &mut invalid);
    FormData {
        entries: entries.into(),
        invalid: invalid.into(),
    }
}

/// Every NAMED control under the rendered `form` node, in document order, as
/// `(node, name)`. A named control's own subtree is not searched.
fn named_controls(info: &CallbackInfo, form: DomNodeId) -> Vec<(DomNodeId, AzString)> {
    let mut out = Vec::new();
    // Depth-first, document order: a node, then its children, then its
    // following siblings.
    let mut stack: Vec<DomNodeId> = info.get_first_child(form).into_iter().collect();
    while let Some(node) = stack.pop() {
        if let Some(next) = info.get_next_sibling(node) {
            stack.push(next);
        }
        if let Some(name) = info.get_node_attribute(node, "name") {
            out.push((node, name));
            continue;
        }
        if let Some(first) = info.get_first_child(node) {
            stack.push(first);
        }
    }
    out
}

/// The current [`FormData`] of the rendered `form`, with the nodes of the
/// controls that fail their constraints.
fn current_form_data(info: &mut CallbackInfo, form: DomNodeId) -> (FormData, Vec<DomNodeId>) {
    let mut entries = Vec::new();
    let mut invalid = Vec::new();
    let mut invalid_nodes = Vec::new();
    for (node, name) in named_controls(info, form) {
        // HTML: no entry for an unnamed or a disabled control.
        if name.as_str().is_empty() || info.get_node_attribute(node, "disabled").is_some() {
            continue;
        }
        let (field, dataset) = control_state(info, node);
        // The widget's live state first (typed text, a picked date); then
        // what the form-control replacement knows of a raw control whose
        // widget keeps nothing readable on its root (a checkbox, a slider,
        // a drop-down); then the node's `value` attribute.
        let values = match probe_state(dataset) {
            Some(live) => alloc::vec![live],
            None => from_submission(replaced_submission(info, node), || {
                built_widget_values(
                    &carried_states(info, node),
                    info.get_node_attribute(node, "value"),
                    |n| nth_child_text(info, node, n),
                )
            }),
        };
        for value in values {
            if !value.valid {
                invalid.push(name.clone());
                invalid_nodes.push(field);
            }
            entries.push(FormEntry {
                name: name.clone(),
                value: value.value,
            });
        }
    }
    (
        FormData {
            entries: entries.into(),
            invalid: invalid.into(),
        },
        invalid_nodes,
    )
}

/// The rendered form `node` belongs to: the nearest [`Form`] node (see
/// [`form_state_of`]), `node` itself included.
fn enclosing_form(info: &mut CallbackInfo, node: DomNodeId) -> Option<DomNodeId> {
    let mut current = Some(node);
    while let Some(n) = current {
        if form_state(info, n).is_some() {
            return Some(n);
        }
        current = info.get_parent(n);
    }
    None
}

/// Whether `dataset` is a [`Form`]'s state.
fn is_form_state(dataset: &RefAny) -> bool {
    let mut dataset = dataset.clone();
    let is_state = dataset.downcast_ref::<FormStateWrapper>().is_some();
    is_state
}

/// The state of the form node `node`, if it is a [`Form`]'s: its dataset -
/// or, when the APP's dataset sits there (a raw `<form>` keeps the one it
/// carried, see `crate::form_controls`), the payload of the node's own
/// `Submit` handler, which is the same state.
pub(crate) fn form_state_of(node: &NodeData) -> Option<RefAny> {
    if let Some(dataset) = node.get_dataset().filter(|ds| is_form_state(ds)) {
        return Some(dataset.clone());
    }
    node.callbacks
        .as_ref()
        .iter()
        .find(|c| {
            c.callback.cb == default_on_form_submit_event as usize && is_form_state(&c.refany)
        })
        .map(|c| c.refany.clone())
}

/// [`form_state_of`] the rendered node `node`.
fn form_state(info: &CallbackInfo, node: DomNodeId) -> Option<RefAny> {
    let id = node.node.into_crate_internal()?;
    let layout = info.get_layout_window().get_layout_result(&node.dom)?;
    let node_data = layout.styled_dom.node_data.as_container();
    let state = node_data.get(id).and_then(form_state_of);
    state
}

// ---------------------------------------------------------------------------
// Submit and reset
// ---------------------------------------------------------------------------

/// Submit the rendered form at `form`: collect the current values, mark the
/// fields that fail their constraints with the `:user-invalid` look, and hand
/// the values to the app's `on_submit`. Returns its `Update` (`DoNothing`
/// without one, or when `form` is not a form).
pub fn submit_form(info: &mut CallbackInfo, form: DomNodeId) -> Update {
    let Some(mut dataset) = form_state(info, form) else {
        return Update::DoNothing;
    };
    let (data, invalid_nodes) = current_form_data(info, form);

    // A refused submit is the other moment `:user-invalid` starts to apply.
    for node in invalid_nodes {
        let Some(mut field) = info.get_dataset(node) else {
            continue;
        };
        let state = field
            .downcast_ref::<TextInputStateWrapper>()
            .map(|w| w.inner.clone());
        if let Some(state) = state {
            crate::widgets::text_input::mark_user_invalid(info, node, &state);
        }
    }

    // The callback is taken OUT of the form state before it runs: a handler
    // that reads the form (`collect_form_data`) borrows that state again.
    let on_submit = match dataset.downcast_ref::<FormStateWrapper>() {
        Some(wrapper) => wrapper.on_submit.clone(),
        None => return Update::DoNothing,
    };
    match on_submit.as_ref() {
        Some(FormOnSubmit { callback, refany }) => callback.invoke(refany.clone(), *info, data),
        None => Update::DoNothing,
    }
}

/// Reset the rendered form at `form`: every text field goes back to its
/// initial value (mirror and line - whatever the user typed there is
/// superseded), then the app's `on_reset` is handed the initial values to
/// restore everything else from. Returns its `Update`.
pub fn reset_form(info: &mut CallbackInfo, form: DomNodeId) -> Update {
    let Some(mut dataset) = form_state(info, form) else {
        return Update::DoNothing;
    };
    let initial = match dataset.downcast_ref::<FormStateWrapper>() {
        Some(w) => w.initial.clone(),
        None => return Update::DoNothing,
    };

    for (field, value) in reset_values(info, form, &initial) {
        restore_field(info, field, &value);
    }

    // The raw controls the form-control replacement turned into widgets
    // keep the user's values in the window's memory, which would hand them
    // back on the next rebuild and undo this reset. Forget them there; the
    // rebuild that shows their defaults is asked for below. (Before the
    // app's handler runs, so what it reads is already the reset form.)
    let forgot_user_values = forget_replaced_controls(info, form);

    // Taken out of the form state before it runs, as in `submit_form`.
    let on_reset = match dataset.downcast_ref::<FormStateWrapper>() {
        Some(wrapper) => wrapper.on_reset.clone(),
        None => return Update::DoNothing,
    };
    let mut update = match on_reset.as_ref() {
        Some(FormOnReset { callback, refany }) => callback.invoke(refany.clone(), *info, initial),
        None => Update::DoNothing,
    };
    if forgot_user_values {
        // A replaced checkbox, slider or drop-down shows its default only
        // by being rebuilt from the app's (unchanged) DOM.
        update.max_self(Update::RefreshDom);
    }
    update
}

/// The [`crate::form_controls::MEMORY_KEY_ATTRIBUTE`] among `attributes`:
/// the memory key of a replaced control's root.
fn memory_key_of(attributes: &[AttributeType]) -> Option<u64> {
    attributes
        .iter()
        .find(|a| a.name() == crate::form_controls::MEMORY_KEY_ATTRIBUTE)
        .and_then(|a| a.value().as_str().trim().parse::<u64>().ok())
}

/// `pick` over every node inside the rendered `form`, in document order - a
/// node's descendants are the contiguous run after it.
fn within_form<T>(
    info: &CallbackInfo,
    form: DomNodeId,
    mut pick: impl FnMut(NodeId, &NodeData) -> Option<T>,
) -> Vec<T> {
    let Some(form_id) = form.node.into_crate_internal() else {
        return Vec::new();
    };
    let Some(layout) = info.get_layout_window().get_layout_result(&form.dom) else {
        return Vec::new();
    };
    let node_data = layout.styled_dom.node_data.as_container();
    let start = form_id.index() + 1;
    let end = start + layout.styled_dom.node_hierarchy.as_container().subtree_len(form_id);
    (start..end)
        .filter_map(|index| {
            let id = NodeId::new(index);
            node_data.get(id).and_then(|data| pick(id, data))
        })
        .collect()
}

/// The memory keys of the replaced controls inside the rendered `form`, in
/// document order - named or not: a reset resets every control.
fn replaced_controls(info: &CallbackInfo, form: DomNodeId) -> Vec<u64> {
    within_form(info, form, |_, data| memory_key_of(data.attributes().as_ref()))
}

/// Whether `dataset` is the state of a text field - a `TextInput` or a
/// `TextArea` host (the node the user types into).
fn is_text_field_state(dataset: &RefAny) -> bool {
    // One probe per statement: each shared borrow must end before the next
    // one is taken.
    let mut d = dataset.clone();
    if d.downcast_ref::<TextInputStateWrapper>().is_some() {
        return true;
    }
    let is_area = d.downcast_ref::<TextAreaStateWrapper>().is_some();
    is_area
}

/// Every text field inside the rendered `form`, in document order.
fn text_fields(info: &CallbackInfo, form: DomNodeId) -> Vec<DomNodeId> {
    within_form(info, form, |id, data| {
        data.get_dataset()
            .is_some_and(is_text_field_state)
            .then(|| DomNodeId {
                dom: form.dom,
                node: NodeHierarchyItemId::from_crate_internal(Some(id)),
            })
    })
}

/// What the text field hosted at `field` was BUILT with: the text its DOM
/// line holds (`container > p > text`) - the app's value at the build, HTML's
/// default value - not what the engine shows over it. `None` for a password
/// built non-empty: its DOM holds the mask, and the real value is gone.
fn built_value(info: &mut CallbackInfo, field: DomNodeId) -> Option<String> {
    let line = info.get_first_child(field)?;
    let leaf = info.get_first_child(line)?;
    let leaf_id = leaf.node.into_crate_internal()?;
    let text = {
        let layout = info.get_layout_window().get_layout_result(&leaf.dom)?;
        let node_data = layout.styled_dom.node_data.as_container();
        let text = match node_data.get(leaf_id)?.get_node_type() {
            NodeType::Text(t) => t.as_str().to_string(),
            _ => return None,
        };
        text
    };
    let masked = info.get_dataset(field).is_some_and(|mut state| {
        let kind = state
            .downcast_ref::<TextInputStateWrapper>()
            .map(|w| w.inner.kind);
        kind == Some(TextInputKind::Password)
    });
    (!masked || text.is_empty()).then_some(text)
}

/// The key of the replaced control the text field `field` belongs to: on the
/// field itself, or on the row a `type=search` field sits in.
fn replaced_key_of(info: &CallbackInfo, field: DomNodeId) -> Option<u64> {
    memory_key_at(info, field)
        .or_else(|| info.get_parent(field).and_then(|row| memory_key_at(info, row)))
}

/// The [`crate::form_controls::MEMORY_KEY_ATTRIBUTE`] of the rendered `node`.
fn memory_key_at(info: &CallbackInfo, node: DomNodeId) -> Option<u64> {
    let id = node.node.into_crate_internal()?;
    let layout = info.get_layout_window().get_layout_result(&node.dom)?;
    let node_data = layout.styled_dom.node_data.as_container();
    let key = node_data
        .get(id)
        .and_then(|data| memory_key_of(data.attributes().as_ref()));
    key
}

/// The text fields inside the rendered `form` and the value a reset puts
/// each back to, in document order - named or not, a text area as much as a
/// text input (HTML resets every control):
///
/// * a NAMED control: its initial value. The controls in document order are
///   the ones `initial` was recorded from, so a name's n-th control takes
///   that name's n-th initial value;
/// * a replaced raw control: its default (the replacement's memory knows it;
///   the rebuild the reset asks for shows it too);
/// * any other field: the text it was built with ([`built_value`]).
fn reset_values(
    info: &mut CallbackInfo,
    form: DomNodeId,
    initial: &FormData,
) -> Vec<(DomNodeId, String)> {
    let mut out: Vec<(DomNodeId, String)> = Vec::new();
    let mut seen: Vec<(AzString, usize)> = Vec::new();
    for (node, name) in named_controls(info, form) {
        let nth = if let Some((_, count)) = seen.iter_mut().find(|(n, _)| *n == name) {
            *count += 1;
            *count - 1
        } else {
            seen.push((name.clone(), 1));
            0
        };
        let Some(value) = initial
            .entries
            .as_ref()
            .iter()
            .filter(|e| e.name == name)
            .nth(nth)
            .map(|e| e.value.as_str().to_string())
        else {
            continue;
        };
        let (field, _) = control_state(info, node);
        out.push((field, value));
    }
    for field in text_fields(info, form) {
        if out.iter().any(|(named, _)| *named == field) {
            continue;
        }
        let default = replaced_key_of(info, field).and_then(|key| {
            match info.get_layout_window().form_control_memory.default_of(key) {
                Some(FormValue::Text(text)) => Some(text),
                _ => None,
            }
        });
        if let Some(value) = default.or_else(|| built_value(info, field)) {
            out.push((field, value));
        }
    }
    out
}

/// Put the text field hosted at `field` back to `value` (see
/// [`reset_values`]).
fn restore_field(info: &mut CallbackInfo, field: DomNodeId, value: &str) {
    let Some(mut state) = info.get_dataset(field) else {
        return;
    };
    if let Some(mut w) = state.downcast_mut::<TextInputStateWrapper>() {
        crate::widgets::text_input::restore_text_input(info, field, &mut w, value);
        return;
    }
    if let Some(mut w) = state.downcast_mut::<TextAreaStateWrapper>() {
        restore_text_area(info, field, &mut w, value);
    };
}

/// `restore_text_input` for a `TextArea` - the same `container > p > text`
/// shape, no value-dependent looks: the mirror, and the engine's line (the
/// user's typing there superseded).
fn restore_text_area(
    info: &mut CallbackInfo,
    container: DomNodeId,
    wrapper: &mut TextAreaStateWrapper,
    value: &str,
) {
    if wrapper.inner.get_text() != value {
        wrapper.inner.text = value.chars().map(|c| c as u32).collect::<Vec<_>>().into();
        wrapper.inner.cursor_pos = value.len();
    }
    crate::widgets::text_input::replace_engine_line(info, container, value);
}

/// Forget what the user gave the replaced controls inside the rendered
/// `form`, so the next build shows their defaults. Whether any held a value.
fn forget_replaced_controls(info: &CallbackInfo, form: DomNodeId) -> bool {
    let controls = replaced_controls(info, form);
    let memory = &info.get_layout_window().form_control_memory;
    let mut forgot = false;
    for control in controls {
        forgot |= memory.reset_control(control);
    }
    forgot
}

/// The CURRENT values of the form `node` sits in - `node` may be the form
/// itself, a field or a button: what a submit would hand over, without
/// submitting. `None` when `node` is in no form.
///
/// How a plain callback reads a form: the app's own `Submit` handler on a raw
/// `<form>` runs as that Form's `on_submit` (see `crate::form_controls`), with
/// the `CallbackInfo` of whatever triggered the submit, so it calls this with
/// `info.get_hit_node()`.
#[must_use]
pub fn collect_form_data(info: &mut CallbackInfo, node: DomNodeId) -> Option<FormData> {
    let form = enclosing_form(info, node)?;
    Some(current_form_data(info, form).0)
}

impl_option!(
    FormData,
    OptionFormData,
    copy = false,
    [Debug, Clone, PartialEq]
);

impl CallbackInfo {
    /// The CURRENT values of the form `node` sits in - `node` may be the
    /// form, a field or a button: what a submit would hand over, without
    /// submitting. `None` when `node` is in no form.
    ///
    /// How a plain callback reads a form: the app's own `Submit` handler on a
    /// raw `<form>` runs as that form's `on_submit`, with the `CallbackInfo`
    /// of whatever submitted it, so it asks
    /// `info.get_form_data(info.get_hit_node())`; a button's click handler
    /// does the same. (The FFI face of [`collect_form_data`].)
    #[must_use]
    pub fn get_form_data(&mut self, node: DomNodeId) -> OptionFormData {
        collect_form_data(self, node).into()
    }
}

/// Submit the form `node` sits in, if any - HTML's implicit submission (Enter
/// in a text field). `None` when `node` is in no form.
pub(crate) fn submit_enclosing_form(info: &mut CallbackInfo, node: DomNodeId) -> Option<Update> {
    let parent = info.get_parent(node)?;
    let form = enclosing_form(info, parent)?;
    Some(submit_form(info, form))
}

/// The engine dispatched `Submit` on the form node (Enter in a control that
/// does not edit text).
#[must_use]
pub extern "C" fn default_on_form_submit_event(_data: RefAny, mut info: CallbackInfo) -> Update {
    let form = info.get_hit_node();
    submit_form(&mut info, form)
}

/// The engine dispatched `Reset` on the form node (Enter on a reset button).
#[must_use]
pub extern "C" fn default_on_form_reset_event(_data: RefAny, mut info: CallbackInfo) -> Update {
    let form = info.get_hit_node();
    reset_form(&mut info, form)
}

// ---------------------------------------------------------------------------
// <input type=hidden>
// ---------------------------------------------------------------------------

/// `<input type=hidden>`: renders nothing (`display: none`, no tab stop, out
/// of the accessibility tree) and contributes `name=value` to the
/// [`FormData`] of the [`Form`] it sits in. A reset leaves it alone.
///
/// Both themes render it identically - there is nothing to paint; the theme
/// field exists so it builds like every other widget.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct HiddenInput {
    pub name: AzString,
    pub value: AzString,
    pub theme: crate::widgets::themes::OptionUiTheme,
}

impl HiddenInput {
    /// A hidden `name=value` pair.
    #[must_use]
    pub const fn create(name: AzString, value: AzString) -> Self {
        Self {
            name,
            value,
            theme: crate::widgets::themes::OptionUiTheme::None,
        }
    }

    /// Replace the submitted value.
    pub fn set_value(&mut self, value: AzString) {
        self.value = value;
    }

    /// [`Self::set_value`] for the builder chain.
    #[must_use]
    pub fn with_value(mut self, value: AzString) -> Self {
        self.set_value(value);
        self
    }

    /// Pick the widget theme (see the type docs: both render the same).
    pub const fn set_theme(&mut self, theme: crate::widgets::themes::UiTheme) {
        self.theme = crate::widgets::themes::OptionUiTheme::Some(theme);
    }

    /// [`Self::set_theme`] for the builder chain.
    #[must_use]
    pub const fn with_theme(mut self, theme: crate::widgets::themes::UiTheme) -> Self {
        self.set_theme(theme);
        self
    }

    /// The node: an empty, undisplayed `div` carrying `name`, `value`,
    /// `type=hidden` and `hidden`, which is all a form needs to read it.
    #[must_use]
    pub fn dom(self) -> Dom {
        use azul_css::{
            dynamic_selector::CssPropertyWithConditions,
            props::{layout::LayoutDisplay, property::CssProperty},
        };

        Dom::create_div()
            .with_css_props(CssPropertyWithConditionsVec::from_vec(alloc::vec![
                CssPropertyWithConditions::simple(CssProperty::const_display(
                    LayoutDisplay::None
                )),
            ]))
            .with_attribute(AttributeType::InputType(AzString::from_const_str("hidden")))
            .with_attribute(AttributeType::Name(self.name))
            .with_attribute(AttributeType::Value(self.value))
            .with_attribute(AttributeType::Hidden)
    }
}

impl From<HiddenInput> for Dom {
    fn from(h: HiddenInput) -> Self {
        h.dom()
    }
}

/// Click on a submit / reset / image button: act on the form it sits in.
/// Outside a form it does nothing (the button's own `on_click` still runs).
#[must_use]
pub extern "C" fn default_on_form_button_click(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(action) = data.downcast_ref::<ButtonFormAction>().map(|a| *a) else {
        return Update::DoNothing;
    };
    let button = info.get_hit_node();
    let Some(form) = enclosing_form(&mut info, button) else {
        return Update::DoNothing;
    };
    match action {
        ButtonFormAction::Submit => submit_form(&mut info, form),
        ButtonFormAction::Reset => reset_form(&mut info, form),
        ButtonFormAction::None => Update::DoNothing,
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::{BTreeMap, HashMap},
        sync::{Arc, Mutex},
    };

    use azul_core::{
        callbacks::Update,
        dom::{AttributeType, Dom, DomId, DomNodeId, DomVec, EventFilter, HoverEventFilter, NodeId, NodeType},
        geom::{LogicalRect, OptionLogicalPosition},
        gl::OptionGlContextPtr,
        hit_test::ScrollPosition,
        refany::{OptionRefAny, RefAny},
        resources::RendererResources,
        styled_dom::{NodeHierarchyItemId, StyledDom},
        window::{MonitorVec, RawWindowHandle, VirtualKeyCode},
    };
    use azul_css::{AzString, StringVec};
    use rust_fontconfig::FcFontCache;

    use super::*;
    #[cfg(feature = "icu")]
    use crate::icu::IcuLocalizerHandle;
    use crate::{
        callbacks::{CallbackChange, CallbackInfo, CallbackInfoRefData, ExternalSystemCallbacks},
        solver3::{display_list::DisplayList, layout_tree::LayoutTree},
        widgets::{
            button::{Button, ButtonFormAction},
            date_picker::DatePicker,
            datetime_local::DateTimeLocalPicker,
            text_input::{TextInput, TextInputStateWrapper},
        },
        window::{DomLayoutResult, LayoutWindow},
        window_state::FullWindowState,
    };

    // ------------------------------------------------------------------
    // Harness
    // ------------------------------------------------------------------

    fn dom_node(idx: usize) -> DomNodeId {
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(idx))),
        }
    }

    /// Runs `f` with a real `CallbackInfo` over a window holding `styled_dom`
    /// (no layout: the form handlers only walk the node hierarchy and read
    /// datasets and attributes). `key` is the pressed key, if any.
    fn run<R>(
        styled_dom: StyledDom,
        hit: DomNodeId,
        key: Option<VirtualKeyCode>,
        f: impl FnOnce(CallbackInfo) -> R,
    ) -> (R, Vec<CallbackChange>) {
        let mut layout_window =
            LayoutWindow::new(FcFontCache::default()).expect("LayoutWindow::new failed");
        layout_window.layout_results.insert(
            DomId::ROOT_ID,
            DomLayoutResult {
                styled_dom,
                layout_tree: LayoutTree {
                    nodes: Vec::new(),
                    warm: Vec::new(),
                    cold: Vec::new(),
                    root: 0,
                    dom_to_layout: BTreeMap::new(),
                    children_arena: Vec::new(),
                    children_offsets: Vec::new(),
                    subtree_needs_intrinsic: Vec::new(),
                },
                calculated_positions: Vec::new(),
                viewport: LogicalRect::zero(),
                display_list: Arc::new(DisplayList::default()),
                scroll_ids: HashMap::new(),
                scroll_id_to_node_id: HashMap::new(),
            },
        );
        let renderer_resources = RendererResources::default();
        let previous_window_state: Option<FullWindowState> = None;
        let mut current_window_state = FullWindowState::default();
        current_window_state.keyboard_state.current_virtual_keycode = key.into();
        let gl_context = OptionGlContextPtr::None;
        let scroll_states: BTreeMap<DomId, BTreeMap<NodeHierarchyItemId, ScrollPosition>> =
            BTreeMap::new();
        let window_handle = RawWindowHandle::Unsupported;
        let system_callbacks = ExternalSystemCallbacks::rust_internal();
        let ref_data = CallbackInfoRefData {
            layout_window: &layout_window,
            renderer_resources: &renderer_resources,
            previous_window_state: &previous_window_state,
            current_window_state: &current_window_state,
            gl_context: &gl_context,
            current_scroll_manager: &scroll_states,
            current_window_handle: &window_handle,
            system_callbacks: &system_callbacks,
            system_style: Arc::new(azul_css::system::SystemStyle::default()),
            monitors: Arc::new(Mutex::new(MonitorVec::from_const_slice(&[]))),
            #[cfg(feature = "icu")]
            icu_localizer: IcuLocalizerHandle::default(),
            ctx: core::cell::RefCell::new(OptionRefAny::None),
        };
        let changes: Arc<Mutex<Vec<CallbackChange>>> = Arc::new(Mutex::new(Vec::new()));
        let info = CallbackInfo::new(
            &ref_data,
            &changes,
            hit,
            OptionLogicalPosition::None,
            OptionLogicalPosition::None,
        );
        let r = f(info);
        let pushed = info.take_changes();
        (r, pushed)
    }

    /// What the app's form callbacks were handed, in call order.
    #[derive(Default)]
    struct Log {
        submitted: Vec<FormData>,
        reset: Vec<FormData>,
    }

    extern "C" fn record_submit(mut data: RefAny, _: CallbackInfo, form_data: FormData) -> Update {
        if let Some(mut log) = data.downcast_mut::<Log>() {
            log.submitted.push(form_data);
        }
        Update::RefreshDom
    }

    extern "C" fn record_reset(mut data: RefAny, _: CallbackInfo, form_data: FormData) -> Update {
        if let Some(mut log) = data.downcast_mut::<Log>() {
            log.reset.push(form_data);
        }
        Update::RefreshDomAllWindows
    }

    fn submitted(log: &RefAny) -> Vec<FormData> {
        let mut log = log.clone();
        let entries = log.downcast_ref::<Log>().expect("log").submitted.clone();
        entries
    }

    fn resets(log: &RefAny) -> Vec<FormData> {
        let mut log = log.clone();
        let entries = log.downcast_ref::<Log>().expect("log").reset.clone();
        entries
    }

    fn pairs(data: &FormData) -> Vec<(String, String)> {
        data.entries
            .as_ref()
            .iter()
            .map(|e| (e.name.as_str().to_string(), e.value.as_str().to_string()))
            .collect()
    }

    fn invalid_names(data: &FormData) -> Vec<String> {
        data.invalid
            .as_ref()
            .iter()
            .map(|s| s.as_str().to_string())
            .collect()
    }

    /// The flattened index of the first node carrying a callback to `cb`.
    fn node_with_callback(sd: &StyledDom, cb: usize) -> usize {
        sd.node_data
            .as_ref()
            .iter()
            .position(|nd| nd.callbacks.as_ref().iter().any(|c| c.callback.cb == cb))
            .expect("no node carries that callback")
    }

    /// The flattened index of the node named `name`.
    fn named(sd: &StyledDom, name: &str) -> usize {
        sd.node_data
            .as_ref()
            .iter()
            .position(|nd| {
                nd.attributes()
                    .as_ref()
                    .iter()
                    .any(|a| matches!(a, AttributeType::Name(n) if n.as_str() == name))
            })
            .unwrap_or_else(|| panic!("no node is named {name:?}"))
    }

    /// Overwrite the text a rendered TextInput holds, the way typing would.
    fn type_into(sd: &StyledDom, name: &str, text: &str) {
        let idx = named(sd, name);
        let mut ds = sd.node_data.as_ref()[idx]
            .get_dataset()
            .cloned()
            .expect("a TextInput carries its state");
        let mut w = ds
            .downcast_mut::<TextInputStateWrapper>()
            .expect("the dataset is a TextInput's state");
        w.inner.text = text.chars().map(|c| c as u32).collect::<Vec<_>>().into();
    }

    fn text_of(sd: &StyledDom, name: &str) -> String {
        let idx = named(sd, name);
        let mut ds = sd.node_data.as_ref()[idx].get_dataset().cloned().expect("state");
        let w = ds.downcast_ref::<TextInputStateWrapper>().expect("TextInput state");
        w.inner.get_text()
    }

    /// user (text), mail (email, invalid at build), period (month), when
    /// (datetime-local), an UNNAMED field, and the three buttons.
    fn sample_form(log: &RefAny) -> Form {
        Form::create(DomVec::from_vec(vec![
            TextInput::create()
                .with_name("user".into())
                .with_text("ann".into())
                .dom(),
            TextInput::create_email()
                .with_name("mail".into())
                .with_text("not-an-email".into())
                .dom(),
            DatePicker::create_month(2026, 9)
                .with_name("period".into())
                .dom(),
            DateTimeLocalPicker::create(2026, 9, 29, 14, 5)
                .with_name("when".into())
                .dom(),
            TextInput::create().with_text("ignored".into()).dom(),
            Button::create_submit("Send".into()).dom(),
            Button::create_reset("Clear".into()).dom(),
        ]))
        .with_on_submit(log.clone(), record_submit as FormOnSubmitCallbackType)
        .with_on_reset(log.clone(), record_reset as FormOnResetCallbackType)
    }

    // ------------------------------------------------------------------
    // FormData
    // ------------------------------------------------------------------

    #[test]
    fn form_data_is_a_multimap_of_named_values() {
        let data = FormData {
            entries: vec![
                FormEntry {
                    name: "tag".into(),
                    value: "a".into(),
                },
                FormEntry {
                    name: "user".into(),
                    value: "ann".into(),
                },
                FormEntry {
                    name: "tag".into(),
                    value: "b".into(),
                },
            ]
            .into(),
            invalid: StringVec::from_const_slice(&[]),
        };
        assert_eq!(data.get("tag".into()).into_option().as_ref().map(AzString::as_str), Some("a"));
        assert_eq!(data.get("nope".into()).into_option(), None);
        let all: Vec<String> = data
            .get_all("tag".into())
            .as_ref()
            .iter()
            .map(|s| s.as_str().to_string())
            .collect();
        assert_eq!(all, vec!["a".to_string(), "b".to_string()]);
        assert!(data.has("user".into()));
        assert!(!data.has("nope".into()));
        assert!(data.is_valid());
    }

    // ------------------------------------------------------------------
    // Building
    // ------------------------------------------------------------------

    #[test]
    fn a_form_is_a_form_node_around_its_children() {
        let log = RefAny::new(Log::default());
        let dom = sample_form(&log).dom();
        assert!(matches!(dom.root.get_node_type(), NodeType::Form));
        assert_eq!(dom.children.as_ref().len(), 7);
        let events: Vec<EventFilter> = dom.root.callbacks.as_ref().iter().map(|c| c.event).collect();
        assert!(
            events.contains(&EventFilter::Hover(HoverEventFilter::Submit)),
            "the engine's Submit (Enter in a control) must reach the form"
        );
        assert!(events.contains(&EventFilter::Hover(HoverEventFilter::Reset)));
    }

    #[test]
    fn a_form_records_each_named_controls_initial_value_at_build() {
        let log = RefAny::new(Log::default());
        let dom = sample_form(&log).dom();
        let mut ds = dom.root.get_dataset().cloned().expect("the form carries its state");
        let state = ds.downcast_ref::<FormStateWrapper>().expect("form state");
        assert_eq!(
            pairs(&state.initial),
            vec![
                ("user".to_string(), "ann".to_string()),
                ("mail".to_string(), "not-an-email".to_string()),
                ("period".to_string(), "2026-09".to_string()),
                ("when".to_string(), "2026-09-29T14:05".to_string()),
            ],
            "only NAMED controls, in document order"
        );
        assert_eq!(invalid_names(&state.initial), vec!["mail".to_string()]);
    }

    #[test]
    fn submit_reset_and_image_buttons_declare_their_html_type() {
        for (button, ty) in [
            (Button::create_submit("Send".into()), "submit"),
            (Button::create_reset("Clear".into()), "reset"),
            (
                Button::create_image(
                    azul_core::resources::ImageRef::null_image(
                        1,
                        1,
                        azul_core::resources::RawImageFormat::RGBA8,
                        Vec::new(),
                    ),
                    "Go".into(),
                ),
                "image",
            ),
        ] {
            let dom = button.dom();
            assert!(
                dom.root.attributes().as_ref().iter().any(
                    |a| matches!(a, AttributeType::InputType(t) if t.as_str() == ty)
                ),
                "no type={ty}"
            );
            assert!(
                dom.root
                    .callbacks
                    .as_ref()
                    .iter()
                    .any(|c| c.callback.cb == default_on_form_button_click as usize),
                "a {ty} button must act on its form"
            );
        }
        let image = Button::create_image(
            azul_core::resources::ImageRef::null_image(
                1,
                1,
                azul_core::resources::RawImageFormat::RGBA8,
                Vec::new(),
            ),
            "Go".into(),
        );
        assert_eq!(image.form_action, ButtonFormAction::Submit);
        let a11y = image.dom();
        let name = a11y
            .root
            .get_accessibility_info()
            .and_then(|i| i.accessibility_name.as_ref().map(|n| n.as_str().to_string()));
        assert_eq!(name.as_deref(), Some("Go"), "an image button is named by its alt text");
    }

    #[test]
    fn a_plain_button_has_no_form_action() {
        let dom = Button::create("Hi".into()).dom();
        assert!(!dom
            .root
            .callbacks
            .as_ref()
            .iter()
            .any(|c| c.callback.cb == default_on_form_button_click as usize));
    }

    // ------------------------------------------------------------------
    // Submit
    // ------------------------------------------------------------------

    #[test]
    fn a_submit_button_hands_the_app_the_current_values() {
        let log = RefAny::new(Log::default());
        let sd = StyledDom::create_from_dom(sample_form(&log).dom());
        type_into(&sd, "user", "bob");
        let button = node_with_callback(&sd, default_on_form_button_click as usize);
        let payload = sd.node_data.as_ref()[button]
            .callbacks
            .as_ref()
            .iter()
            .find(|c| c.callback.cb == default_on_form_button_click as usize)
            .map(|c| c.refany.clone())
            .expect("payload");
        let (update, _) = run(sd, dom_node(button), None, |info| {
            default_on_form_button_click(payload.clone(), info)
        });
        assert_eq!(update, Update::RefreshDom, "the app's verdict");
        let got = submitted(&log);
        assert_eq!(got.len(), 1);
        assert_eq!(
            got[0].get("user".into()).into_option().map(|s| s.as_str().to_string()),
            Some("bob".to_string()),
            "a submit reads the CURRENT value, not the initial one"
        );
        assert_eq!(invalid_names(&got[0]), vec!["mail".to_string()]);
        assert!(!got[0].is_valid());
    }

    #[test]
    fn a_failed_submit_paints_the_invalid_look_on_the_invalid_fields() {
        let log = RefAny::new(Log::default());
        let sd = StyledDom::create_from_dom(sample_form(&log).dom());
        let mail = named(&sd, "mail");
        let (_, changes) = run(sd, dom_node(0), None, |mut info| submit_form(&mut info, dom_node(0)));
        assert!(
            changes.iter().any(|c| matches!(
                c,
                CallbackChange::OverrideNodeCssProperties { node_id, .. } if *node_id == NodeId::new(mail)
            )),
            "the invalid e-mail field was not marked: {changes:?}"
        );
    }

    #[test]
    fn the_engines_submit_event_on_the_form_runs_the_same_submit() {
        let log = RefAny::new(Log::default());
        let sd = StyledDom::create_from_dom(sample_form(&log).dom());
        let state = sd.node_data.as_ref()[0].get_dataset().cloned().expect("form state");
        let (_, _) = run(sd, dom_node(0), None, |info| {
            default_on_form_submit_event(state.clone(), info)
        });
        assert_eq!(submitted(&log).len(), 1);
    }

    #[test]
    fn enter_in_a_text_field_submits_its_form() {
        let log = RefAny::new(Log::default());
        let sd = StyledDom::create_from_dom(sample_form(&log).dom());
        let user = named(&sd, "user");
        let state = sd.node_data.as_ref()[user].get_dataset().cloned().expect("field state");
        let _ = run(sd, dom_node(user), Some(VirtualKeyCode::Return), |info| {
            crate::widgets::text_input::default_on_virtual_key_down(state.clone(), info)
        });
        assert_eq!(submitted(&log).len(), 1, "HTML's implicit submission");
    }

    /// The FFI face of `collect_form_data`: any plain callback - a raw
    /// `<form>`'s own Submit handler, a button's click - reads the form its
    /// node sits in through `CallbackInfo`, which the bindings expose.
    #[test]
    fn a_callback_reads_the_form_its_node_sits_in_through_callback_info() {
        let log = RefAny::new(Log::default());
        let sd = StyledDom::create_from_dom(sample_form(&log).dom());
        let user = named(&sd, "user");
        let (data, _) = run(sd, dom_node(user), None, |mut info| {
            info.get_form_data(dom_node(user))
        });
        let data = data.into_option().expect("the field sits in a form");
        assert_eq!(
            data.get("user".into())
                .into_option()
                .map(|s| s.as_str().to_string()),
            Some("ann".to_string())
        );
        assert!(submitted(&log).is_empty(), "reading is not submitting");
    }

    #[test]
    fn a_submit_button_outside_a_form_does_nothing() {
        let dom = Button::create_submit("Send".into()).dom();
        let payload = dom
            .root
            .callbacks
            .as_ref()
            .iter()
            .find(|c| c.callback.cb == default_on_form_button_click as usize)
            .map(|c| c.refany.clone())
            .expect("payload");
        let sd = StyledDom::create_from_dom(dom);
        let (update, _) = run(sd, dom_node(0), None, |info| {
            default_on_form_button_click(payload.clone(), info)
        });
        assert_eq!(update, Update::DoNothing);
    }

    // ------------------------------------------------------------------
    // Reset
    // ------------------------------------------------------------------

    #[test]
    fn a_reset_restores_each_text_field_and_hands_the_app_the_initial_values() {
        let log = RefAny::new(Log::default());
        let sd = StyledDom::create_from_dom(sample_form(&log).dom());
        type_into(&sd, "user", "bob");
        let user = named(&sd, "user");
        let (update, changes) = run(sd.clone(), dom_node(0), None, |mut info| {
            reset_form(&mut info, dom_node(0))
        });
        assert_eq!(update, Update::RefreshDomAllWindows, "the app's verdict");
        assert_eq!(text_of(&sd, "user"), "ann", "the field is back at its initial value");
        let got = resets(&log);
        assert_eq!(got.len(), 1);
        assert_eq!(
            got[0].get("user".into()).into_option().map(|s| s.as_str().to_string()),
            Some("ann".to_string())
        );
        // The field's line is re-texted: user(container) > line <p> > text.
        assert!(
            changes.iter().any(|c| matches!(
                c,
                CallbackChange::ChangeNodeText { node_id, text }
                    if *node_id == dom_node(user + 2) && text.as_str() == "ann"
            )),
            "the field still shows the edited text: {changes:?}"
        );
    }

    // ------------------------------------------------------------------
    // <input type=hidden>
    // ------------------------------------------------------------------

    mod hidden {
        use azul_css::props::{layout::LayoutDisplay, property::CssProperty};

        use super::*;

        #[test]
        fn a_hidden_input_renders_nothing_but_carries_its_name_and_value() {
            let dom = HiddenInput::create("token".into(), "abc123".into()).dom();
            assert!(dom.children.as_ref().is_empty(), "a hidden input has no content");
            assert!(dom.root.get_tab_index().is_none(), "a hidden input is not focusable");
            let display = crate::widgets::themes::theme_blocks::checks::live_inline(&dom).iter()
                .filter_map(|(p, _)| match p {
                    CssProperty::Display(v) => v.get_property().cloned(),
                    _ => None,
                })
                .next_back();
            assert_eq!(display, Some(LayoutDisplay::None), "it must take no space");
            let attrs = dom.root.attributes();
            assert!(attrs
                .as_ref()
                .iter()
                .any(|a| matches!(a, AttributeType::Name(n) if n.as_str() == "token")));
            assert!(attrs
                .as_ref()
                .iter()
                .any(|a| matches!(a, AttributeType::Value(v) if v.as_str() == "abc123")));
            assert!(attrs
                .as_ref()
                .iter()
                .any(|a| matches!(a, AttributeType::InputType(t) if t.as_str() == "hidden")));
        }

        #[test]
        fn a_hidden_input_is_submitted_with_its_form() {
            let log = RefAny::new(Log::default());
            let form = Form::create(DomVec::from_vec(vec![
                HiddenInput::create("token".into(), "abc123".into()).dom(),
                TextInput::create()
                    .with_name("user".into())
                    .with_text("ann".into())
                    .dom(),
            ]))
            .with_on_submit(log.clone(), record_submit as FormOnSubmitCallbackType)
            .with_on_reset(log.clone(), record_reset as FormOnResetCallbackType);
            let sd = StyledDom::create_from_dom(form.dom());
            let (_, _) = run(sd.clone(), dom_node(0), None, |mut info| {
                submit_form(&mut info, dom_node(0))
            });
            let got = submitted(&log);
            assert_eq!(got.len(), 1);
            assert_eq!(
                pairs(&got[0]),
                vec![
                    ("token".to_string(), "abc123".to_string()),
                    ("user".to_string(), "ann".to_string()),
                ]
            );

            // A reset leaves it alone and reports it among the initial values.
            let (_, _) = run(sd, dom_node(0), None, |mut info| reset_form(&mut info, dom_node(0)));
            let reset = resets(&log);
            assert_eq!(reset.len(), 1);
            assert!(reset[0].has("token".into()));
        }
    }

    // ------------------------------------------------------------------
    // A name on a wrapper root (the form-control replacement puts it there)
    // ------------------------------------------------------------------

    mod wrapper_roots {
        use super::*;

        #[test]
        fn a_name_on_a_search_rows_wrapper_still_reads_the_field_inside() {
            let log = RefAny::new(Log::default());
            // The replacement grafts `name` onto the widget ROOT, which for a
            // search field is the row around it, not the field holding the
            // state.
            let search = TextInput::create_search()
                .with_text("rust".into())
                .dom()
                .with_attribute(AttributeType::Name("q".into()));
            let form = Form::create(DomVec::from_vec(vec![search]))
                .with_on_submit(log.clone(), record_submit as FormOnSubmitCallbackType);
            let dom = form.dom();
            let mut ds = dom.root.get_dataset().cloned().expect("form state");
            let initial = ds
                .downcast_ref::<FormStateWrapper>()
                .expect("form state")
                .initial
                .clone();
            assert_eq!(pairs(&initial), vec![("q".to_string(), "rust".to_string())]);

            let sd = StyledDom::create_from_dom(dom);
            let _ = run(sd, dom_node(0), None, |mut info| submit_form(&mut info, dom_node(0)));
            assert_eq!(
                submitted(&log)
                    .first()
                    .map(pairs),
                Some(vec![("q".to_string(), "rust".to_string())])
            );
        }
    }
}

/// Following the app theme (`theme: None`): the DOM carries every widget
/// theme's `@theme(<name>)` block and renders the app theme's; a pinned
/// widget (`with_theme`) ignores the app theme (T2 migration, T1 report
/// section 4).
#[cfg(test)]
mod app_theme_tests {
    use super::*;
    use crate::widgets::themes::{theme_blocks::checks, UiTheme};

    fn content() -> DomVec {
        DomVec::from_vec(alloc::vec![Dom::create_div(), Dom::create_div()])
    }

    #[test]
    fn a_form_without_a_theme_follows_the_app_theme() {
        checks::assert_follows_the_app_theme(
            "form",
            || Form::create(content()).dom(),
            |t: UiTheme| Form::create(content()).with_theme(t).dom(),
        );
    }

    #[test]
    fn a_forms_content_is_not_split_into_theme_blocks() {
        // The content is the caller's: the same in every theme, so it is
        // carried once, untouched.
        let styled = || {
            DomVec::from_vec(alloc::vec![Dom::create_div().with_css_props(
                CssPropertyWithConditionsVec::from_vec(alloc::vec![
                    azul_css::dynamic_selector::CssPropertyWithConditions::dark_mode(
                        azul_css::props::property::CssProperty::const_display(
                            azul_css::props::layout::LayoutDisplay::Block
                        )
                    )
                ])
            )])
        };
        for theme in checks::BOTH {
            let dom = checks::under(theme, || Form::create(styled()).dom());
            let child = &dom.children.as_ref()[0];
            assert!(
                checks::theme_names(child).is_empty(),
                "built for {}: the caller's content gained theme blocks",
                theme.name()
            );
        }
    }

    /// R5: the widget's structure (display, flex, alignment, cursor, ...) is
    /// the same in every theme, so it is declared ONCE, outside every
    /// `@theme` block - it holds under flat, flora and any theme to come. A
    /// theme's block carries only its skin.
    #[test]
    fn a_form_declares_its_structure_once_for_every_theme() {
        use crate::widgets::themes::theme_checks::assert_structure_is_shared;
        for theme in checks::BOTH {
            let dom = checks::under(theme, || Form::create(content()).dom());
            assert_structure_is_shared(&format!("form built for {}", theme.name()), &dom, &[]);
        }
    }
}
