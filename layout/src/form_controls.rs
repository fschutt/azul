//! Raw HTML form controls become azul widgets.
//!
//! `Dom::create_input("range", ..)`, `<input type="range">` in XML, a
//! `<select>` and a `<textarea>` are REPLACED - before the cascade - by the
//! widget their type names: a [`Slider`], a [`DropDown`], a [`TextArea`] and
//! so on. A raw `NodeType::Input` lays out as an empty inline box and
//! handles nothing; the widget is what the app meant.
//!
//! # Why this runs on a `Dom`, BEFORE the cascade
//!
//! For exactly the reason `<icon>` resolution does (see
//! `azul_core::icon::resolve_icons_in_dom`): a replacement is a whole
//! styled SUBTREE - a text input is a container, a value line and a text
//! leaf, a date picker is a field plus a popup calendar - and a `StyledDom`
//! is a flat arena in DFS order that a subtree cannot be spliced into. On
//! the `Dom` tree the widget is grafted in place, nothing is cascaded twice
//! and there is no property cache to invalidate. It runs BEFORE icon
//! resolution, because the widgets themselves contain `<icon>`s (the
//! drop-down's arrow, the date picker's calendar glyph).
//!
//! Every path from an app `Dom` to a `StyledDom` runs it: the layout
//! callback's DOM (`LayoutWindow::resolve_form_controls`, called by the
//! shell BEFORE the pre-cascade fingerprint, so the fast path's by-index
//! transfers see the same nodes as the retained `StyledDom`), a
//! VirtualView's DOM and a measured DOM (`LayoutWindow::style_user_dom*`),
//! and an XML document mounted by the E2E runner
//! (`xml::parse_xml_to_styled_dom_resolving_icons`).
//!
//! # The mapping
//!
//! ONE table, [`INPUT_TYPE_WIDGETS`]: an HTML `type` (or `<select>` /
//! `<textarea>` / `<button>` / `<form>`) to a [`FormWidget`]. A missing or unknown `type` is the
//! text state, as in HTML. A text-like input whose `list` attribute names a
//! `<datalist>` with options becomes a [`ComboBox`]. Every type has a widget
//! of its own, and the variant carries the mode that widget is built in:
//! `password` is `FormWidget::TextInput(TextInputKind::Password)`, `week` is
//! `FormWidget::DatePicker(DatePickerMode::Week)`, `submit` is
//! `FormWidget::Button(ButtonFormAction::Submit)`, so building the widget
//! needs no second table.
//!
//! # `<form>`
//!
//! A raw `<form>` becomes a [`Form`] around its content (a `Form` the app
//! built is left alone), so FormData, Enter-to-submit and the submit / reset
//! buttons work on raw and XML forms too. Its controls are resolved INSIDE
//! the Form, after it recorded their initial values from the raw nodes -
//! their HTML defaults. It keeps the raw form's block layout (not the Form
//! widget's column) and everything the graft carries.
//!
//! The app's own `Submit` / `Reset` handlers on the raw form (plain
//! `(RefAny, CallbackInfo) -> Update` callbacks on `HoverEventFilter::Submit`
//! / `Reset`) become the Form's `on_submit` / `on_reset`: they run whenever
//! the Form submits or resets - a submit button, Enter in a field, the
//! engine's own event - exactly once, with the `CallbackInfo` of the event
//! that triggered it, and read the values with
//! `crate::widgets::form::collect_form_data(&mut info, info.get_hit_node())`.
//! They are NOT left on the form node as well: the engine's `Submit` reaches
//! every handler there, so they would run twice.
//!
//! # What moves from the raw node to the widget
//!
//! * The HTML attributes are READ into the widget: `value`, `placeholder`,
//!   `min` / `max` / `step`, `checked`, `maxlength`, `size` / `rows` / `cols`,
//!   `list`, the `<option>`s of a `<select>`, the text of a `<textarea>`,
//!   `aria-label` (or the node's accessibility name, or `title`) as the
//!   widget's accessible name.
//! * Every OTHER attribute - ids, classes, `name`, `type`, `required`,
//!   `pattern`, `min`/`max`, `disabled`, `autofocus`, `data-*`, ... - is
//!   carried onto the widget's ROOT, where form validation, the soft
//!   keyboard's purpose, reset detection and CSS selectors read them. The
//!   live-state attributes (`value`, `checked`, `selected`, `placeholder`)
//!   are not: the widget owns that state now, and a stale copy would lie to
//!   assistive technology.
//! * The node's inline style is appended AFTER the widget's own, so the
//!   app's declarations win. Of its scoped `with_css` sheets, the
//!   declarations that target the node itself (`* {..}`, `*:hover {..}`)
//!   move into the root's inline style - on a void `<input>` they could only
//!   ever mean the control; a `:hover` there becomes a `:hover` condition,
//!   the form the node's own style is read in - and every other rule stays a
//!   scoped sheet on the widget subtree, where it can reach the widget's
//!   parts by class.
//! * The node's callbacks are APPENDED to the widget root's: they fire for
//!   the same pointer and focus interactions as on the raw input, after
//!   the widget's own handler has updated the widget. They are carried, not
//!   mapped onto the widget's typed hooks - an app callback has the generic
//!   `(RefAny, CallbackInfo) -> Update` shape, and the typed hooks belong to
//!   the replacement (see below). For a composite widget whose focus target
//!   is an inner part (a radio's row, a combobox's field), focus-scoped
//!   events land on that part and only bubbling ones reach the root.
//! * Tab index (when the root is focusable), key, marker, context menu,
//!   menu bar, component origin, accessibility description / labelled-by /
//!   described-by, and the dataset when the widget root has none.
//! * `disabled`: every node of the widget loses its callbacks, its tab
//!   index and its editability, the root is dimmed and announced as
//!   unavailable, and the app's callbacks are not attached either (a
//!   disabled control fires nothing in HTML). `readonly` on a text-like
//!   control turns editing off but keeps it focusable.
//!
//! # State across the app's rebuilds
//!
//! A raw-input app keeps no state of its own: nothing wrote the checked
//! flag of `<input type="checkbox">` back into the model it is rebuilt
//! from. So the replacement installs a RECORDER as the widget's typed change
//! hook (`on_toggle`, `on_value_change`, `on_choice_change`, ...), which
//! writes the user's value into the window's [`FormControlMemory`], and the
//! next build reads it back - but only while the app's DEFAULT is unchanged
//! (HTML's "dirty value" rule: the user's value stands until the app
//! changes the attribute, then the app is the truth again). Controls are
//! identified by `(scope, key | id | tree path + name)`; give a control an
//! id or a key when its position in the tree moves between builds.
//!
//! A drop-down only shows a pick by being rebuilt, and a radio unchecks its
//! siblings of the same `name` by the rebuild, so those two recorders ask
//! for `Update::RefreshDom`; the others leave the widget's own imperative
//! update on screen. Typed text additionally survives through the engine's
//! text overlay, exactly as for a hand-built `TextInput`.
//!
//! # Opting out
//!
//! A node carrying `data-azul-widget="none"` ([`OPT_OUT_ATTRIBUTE`] /
//! [`OPT_OUT_VALUE`], e.g. `<input type="range" data-azul-widget="none">` or
//! [`opt_out_attribute`] from Rust) stays the raw node, for an app that
//! styles or handles a plain input itself. The web backend renders the app
//! DOM to real HTML without this pass, so it keeps the browser's own
//! controls.

use alloc::{
    collections::BTreeMap,
    string::{String, ToString},
    sync::Arc,
    vec::Vec,
};
use core::hash::{Hash, Hasher};
use std::sync::Mutex;

use azul_core::{
    a11y::AccessibilityState,
    callbacks::{CoreCallbackData, CoreCallbackDataVec, Update},
    dom::{AttributeNameValue, AttributeType, Dom, EventFilter, HoverEventFilter, NodeData, NodeType},
    refany::{OptionRefAny, RefAny},
};
use azul_css::{
    css::{Css, CssPathSelector, CssRuleBlock},
    dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
    props::{basic::color::ColorU, layout::LayoutDisplay, property::CssProperty},
    AzString, OptionString, StringVec,
};

use crate::{
    callbacks::{Callback, CallbackInfo},
    widgets::{
        button::{Button, ButtonFormAction},
        check_box::{CheckBox, CheckBoxOnToggleCallbackType, CheckBoxState},
        color_input::{
            color_from_hex, color_to_hex, ColorInput, ColorInputOnValueChangeCallbackType,
            ColorInputState,
        },
        combobox::{
            ComboBox, ComboBoxOnSelectCallbackType, ComboBoxOnTextInputCallbackType, ComboBoxState,
        },
        date_picker::{
            format_value, iso_week_monday, iso_week_of, iso_weeks_in_year, DatePicker,
            DatePickerMode, DatePickerOnChangeCallbackType, DatePickerState,
        },
        datetime_local::{
            DateTimeLocalPicker, DateTimeLocalPickerOnChangeCallbackType, DateTimeLocalPickerState,
        },
        drop_down::{DropDown, DropDownOnChoiceChangeCallbackType},
        file_input::{FileInput, FileInputOnPathChangeCallbackType, FileInputState},
        form::{Form, FormData, FormOnResetCallbackType, FormOnSubmitCallbackType, HiddenInput},
        number_input::{NumberInput, NumberInputOnValueChangeCallbackType, NumberInputState},
        radio_group::{RadioGroup, RadioGroupOnChangeCallbackType, RadioGroupState},
        slider::{Slider, SliderOnValueChangeCallbackType, SliderState},
        text_area::{TextArea, TextAreaOnTextInputCallbackType, TextAreaState},
        text_input::{
            OnTextInputReturn, TextInput, TextInputKind, TextInputOnTextInputCallbackType,
            TextInputState, TextInputValid,
        },
        time_picker::{TimePicker, TimePickerOnChangeCallbackType, TimePickerState},
    },
};

/// The attribute that keeps a form control raw: `data-azul-widget="none"`.
pub const OPT_OUT_ATTRIBUTE: &str = "data-azul-widget";
/// The [`OPT_OUT_ATTRIBUTE`] value that opts out (ASCII-case-insensitive).
pub const OPT_OUT_VALUE: &str = "none";
/// Stamped on every widget root this pass produced, naming the widget
/// ([`FormWidget::name`]): how a tool - or form submission - finds the
/// controls that started life as raw form nodes.
pub const REPLACED_MARKER_ATTRIBUTE: &str = "data-azul-form-control";
/// Stamped on the root of every replaced control that holds a value: the
/// key (decimal) under which the window's [`FormControlMemory`] knows it -
/// how a form reset forgets, and a form submit reads, what the user gave it.
pub const MEMORY_KEY_ATTRIBUTE: &str = "data-azul-form-key";

/// The scope of the layout callback's DOM. A `VirtualView`'s DOM gets its own
/// (`form_scope_of_virtual_view`, used by
/// `LayoutWindow::style_user_dom_in_scope`), so a control at the same tree
/// path in two DOMs is two controls.
pub use crate::window::{form_scope_of_virtual_view, FORM_SCOPE_MEASURE, FORM_SCOPE_ROOT};

/// `data-azul-widget="none"`, for `Dom::with_attribute`.
#[must_use]
pub const fn opt_out_attribute() -> AttributeType {
    AttributeType::Data(AttributeNameValue {
        attr_name: AzString::from_const_str(OPT_OUT_ATTRIBUTE),
        value: AzString::from_const_str(OPT_OUT_VALUE),
    })
}

/// The widget a raw form control becomes - and, where one widget serves
/// several HTML types, the mode it is built in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FormWidget {
    /// `text`, `password`, `search`, `email`, `tel`, `url`: one widget, one
    /// mode per type.
    TextInput(TextInputKind),
    TextArea,
    NumberInput,
    CheckBox,
    Radio,
    ColorInput,
    FileInput,
    Slider,
    /// `date`, `month`, `week` (ISO 8601).
    DatePicker(DatePickerMode),
    /// `datetime-local`: a date picker and a time picker in one row.
    DateTimeLocal,
    TimePicker,
    /// `button`, `submit`, `reset` - and a `<button>` element, labelled by
    /// its content: what the button does to its form.
    Button(ButtonFormAction),
    /// `image`: a submit button named by its `alt`. With no loader for `src`
    /// here, the `alt` text is also its label - HTML's own rendering of an
    /// image button whose image is not available.
    ImageButton,
    DropDown,
    ComboBox,
    /// `type="hidden"`: an invisible node that keeps its attributes (a form
    /// still submits its `name` / `value`).
    Hidden,
    /// `<form>`: a [`Form`] around the raw form's content, whose controls
    /// are then resolved inside it. The app's own `Submit` / `Reset`
    /// handlers on the raw form become the Form's `on_submit` / `on_reset`.
    Form,
}

impl FormWidget {
    /// The value of [`REPLACED_MARKER_ATTRIBUTE`] on the widget's root.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::TextInput(_) => "text-input",
            Self::TextArea => "text-area",
            Self::NumberInput => "number-input",
            Self::CheckBox => "check-box",
            Self::Radio => "radio",
            Self::ColorInput => "color-input",
            Self::FileInput => "file-input",
            Self::Slider => "slider",
            Self::DatePicker(_) => "date-picker",
            Self::DateTimeLocal => "datetime-local",
            Self::TimePicker => "time-picker",
            Self::Button(_) => "button",
            Self::ImageButton => "image-button",
            Self::DropDown => "drop-down",
            Self::ComboBox => "combobox",
            Self::Hidden => "hidden",
            Self::Form => "form",
        }
    }

    /// Controls holding a value the user can change - the ones the memory
    /// keeps, a form reset puts back and a form submit collects. (A hidden
    /// input's value is the app's; a button's is its label.)
    const fn has_value(self) -> bool {
        !matches!(
            self,
            Self::Button(_) | Self::ImageButton | Self::Hidden | Self::Form
        )
    }

    /// Controls whose value is typed text (`readonly` applies to these).
    const fn is_text_like(self) -> bool {
        matches!(
            self,
            Self::TextInput(_) | Self::TextArea | Self::NumberInput | Self::ComboBox
        )
    }
}

/// THE mapping: an `<input>`'s `type` (lower-case), or the element itself
/// for `<select>` / `<textarea>` / `<button>` / `<form>`, to the widget it
/// becomes.
///
/// Glue for a new widget is one row. A missing or unknown `type` is the
/// text state (HTML's rule), which is why `"text"` is also the fallback in
/// [`widget_for`]. A `<button>` whose `type` is `submit`, `reset` or `button`
/// takes that `<input>` row; any other is the `<button>` row - a submit
/// button, HTML's default for the element.
pub static INPUT_TYPE_WIDGETS: &[(&str, FormWidget)] = &[
    ("text", FormWidget::TextInput(TextInputKind::Text)),
    ("password", FormWidget::TextInput(TextInputKind::Password)),
    ("search", FormWidget::TextInput(TextInputKind::Search)),
    ("email", FormWidget::TextInput(TextInputKind::Email)),
    ("tel", FormWidget::TextInput(TextInputKind::Tel)),
    ("url", FormWidget::TextInput(TextInputKind::Url)),
    ("checkbox", FormWidget::CheckBox),
    ("radio", FormWidget::Radio),
    ("color", FormWidget::ColorInput),
    ("file", FormWidget::FileInput),
    ("number", FormWidget::NumberInput),
    ("range", FormWidget::Slider),
    ("date", FormWidget::DatePicker(DatePickerMode::Date)),
    ("month", FormWidget::DatePicker(DatePickerMode::Month)),
    ("week", FormWidget::DatePicker(DatePickerMode::Week)),
    ("datetime-local", FormWidget::DateTimeLocal),
    ("time", FormWidget::TimePicker),
    ("button", FormWidget::Button(ButtonFormAction::None)),
    ("submit", FormWidget::Button(ButtonFormAction::Submit)),
    ("reset", FormWidget::Button(ButtonFormAction::Reset)),
    ("image", FormWidget::ImageButton),
    ("hidden", FormWidget::Hidden),
    ("<select>", FormWidget::DropDown),
    ("<textarea>", FormWidget::TextArea),
    ("<button>", FormWidget::Button(ButtonFormAction::Submit)),
    ("<form>", FormWidget::Form),
];

/// The text state: a missing or unknown `type`.
const TEXT_STATE: FormWidget = FormWidget::TextInput(TextInputKind::Text);

// ── Memory: the user's values across the app's rebuilds ─────────────────────

/// A value the user gave a replaced control.
#[derive(Debug, Clone, PartialEq)]
pub enum FormValue {
    /// A checkbox's checked flag.
    Checked(bool),
    /// Typed text; for a radio group, the `value` of the checked radio.
    Text(String),
    /// A number input's or a slider's value.
    Number(f32),
    /// A colour input's colour.
    Color(ColorU),
    /// A date picker's date (for a week picker: the Monday of the week).
    Date { year: u32, month: u32, day: u32 },
    /// A time picker's time, hour in 24-hour form.
    Time { hour: u32, minute: u32 },
    /// A `datetime-local` picker's date and time, hour in 24-hour form.
    DateTime {
        year: u32,
        month: u32,
        day: u32,
        hour: u32,
        minute: u32,
    },
    /// A drop-down's choice index.
    Choice(usize),
    /// A file input's picked files, in the order picked (empty: none).
    Files(Vec<String>),
}

/// How many controls a window remembers before it forgets the one touched
/// longest ago. Bounds an app whose control ids never repeat.
const MAX_REMEMBERED: usize = 4096;

#[derive(Debug, Clone)]
struct Remembered {
    /// The app's defaults when the user changed the value.
    defaults: u64,
    value: FormValue,
    /// `MemoryInner::clock` at the last read or write.
    touched: u64,
}

/// What a replaced control contributes to its form's `FormData`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Submission {
    /// Not a control this memory built (or not a raw control at all): ask
    /// the node itself.
    Unknown,
    /// These entries under the control's name, in order, spelled as HTML
    /// submits them: none for an unchecked checkbox or radio and for a
    /// button, one per picked file for a file input (one empty entry when
    /// none is picked), one for every other control.
    Entries(Vec<String>),
}

/// How a replaced control's value is spelled in its form's `FormData`.
#[derive(Debug, Clone, PartialEq)]
struct Spelling {
    kind: FormWidget,
    /// A checkbox's or radio's `value` ("on" by default): what it submits
    /// while checked.
    own_value: String,
    /// A select's option VALUES, by choice index.
    choice_values: Vec<String>,
}

impl Spelling {
    fn of(kind: FormWidget, spec: &Spec, node: &NodeData) -> Self {
        Self {
            kind,
            own_value: radio_value(node),
            choice_values: if kind == FormWidget::DropDown {
                spec.choices.iter().map(|c| c.value.clone()).collect()
            } else {
                Vec::new()
            },
        }
    }

    /// `value` as HTML's form data set spells it: its entries.
    fn spell(&self, value: &FormValue) -> Vec<String> {
        match value {
            // HTML: one entry per file; a file input with nothing picked is
            // one empty file.
            FormValue::Files(paths) if paths.is_empty() => alloc::vec![String::new()],
            FormValue::Files(paths) => paths.clone(),
            _ => self.spell_one(value).into_iter().collect(),
        }
    }

    /// A single-valued `value` as HTML's form data set spells it, or `None`
    /// for no entry.
    fn spell_one(&self, value: &FormValue) -> Option<String> {
        match (self.kind, value) {
            (FormWidget::CheckBox | FormWidget::Radio, FormValue::Checked(checked)) => {
                checked.then(|| self.own_value.clone())
            }
            // A radio group's memory holds WHICH radio is checked.
            (FormWidget::Radio, FormValue::Text(checked)) => {
                (*checked == self.own_value).then(|| self.own_value.clone())
            }
            (FormWidget::DropDown, FormValue::Choice(index)) => {
                self.choice_values.get(*index).cloned()
            }
            (FormWidget::DatePicker(mode), FormValue::Date { year, month, day }) => {
                let date = DatePickerState {
                    year: *year,
                    month: *month,
                    day: *day,
                };
                Some(format_value(&date, mode))
            }
            (_, FormValue::Text(text)) => Some(text.clone()),
            (_, FormValue::Number(number)) => Some(alloc::format!("{number}")),
            (_, FormValue::Color(color)) => Some(color_to_hex(*color)),
            (_, FormValue::Date { year, month, day }) => {
                Some(alloc::format!("{year:04}-{month:02}-{day:02}"))
            }
            (_, FormValue::Time { hour, minute }) => Some(alloc::format!("{hour:02}:{minute:02}")),
            (
                _,
                FormValue::DateTime {
                    year,
                    month,
                    day,
                    hour,
                    minute,
                },
            ) => Some(alloc::format!(
                "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}"
            )),
            (_, FormValue::Checked(_) | FormValue::Choice(_) | FormValue::Files(_)) => None,
        }
    }
}

/// A replaced control of a recent build, by the key on its root
/// ([`MEMORY_KEY_ATTRIBUTE`]).
#[derive(Debug, Clone)]
struct Registered {
    /// The entry holding the user's value: the control's own, or - for a
    /// radio of a named group - its group's.
    value_key: u64,
    spelling: Spelling,
    /// The value the control was built with - the user's, or the default.
    built: FormValue,
    /// The app's default: what a reset brings back.
    default: FormValue,
    /// `MemoryInner::clock` at the last build that produced the control.
    touched: u64,
}

#[derive(Debug, Default)]
struct MemoryInner {
    entries: BTreeMap<u64, Remembered>,
    /// Every replaced control of the recent builds (bounded like `entries`).
    controls: BTreeMap<u64, Registered>,
    clock: u64,
}

/// Drop the entry touched longest ago once `map` holds more than
/// [`MAX_REMEMBERED`].
fn evict_beyond_bound<V>(map: &mut BTreeMap<u64, V>, touched: impl Fn(&V) -> u64) {
    if map.len() <= MAX_REMEMBERED {
        return;
    }
    let oldest = map.iter().min_by_key(|&(_, v)| touched(v)).map(|(k, _)| *k);
    if let Some(oldest) = oldest {
        map.remove(&oldest);
    }
}

/// The values users gave the replaced form controls of one window.
///
/// Shared (`Arc`): the recorders the replacement installs hold a clone, and
/// so does the window that reads it back on the next build.
#[derive(Debug, Clone, Default)]
pub struct FormControlMemory {
    inner: Arc<Mutex<MemoryInner>>,
}

impl FormControlMemory {
    /// Remember `value` for control `key`, given while the app's defaults
    /// hashed to `defaults`.
    pub fn remember(&self, key: u64, defaults: u64, value: FormValue) {
        let Ok(mut inner) = self.inner.lock() else {
            return;
        };
        inner.clock = inner.clock.wrapping_add(1);
        let touched = inner.clock;
        inner.entries.insert(
            key,
            Remembered {
                defaults,
                value,
                touched,
            },
        );
        evict_beyond_bound(&mut inner.entries, |r| r.touched);
    }

    /// The user's value for control `key` - if the app's defaults still
    /// hash to what they were when the user gave it. A changed default means
    /// the app took the control back: the value is forgotten.
    #[must_use]
    pub fn recall(&self, key: u64, defaults: u64) -> Option<FormValue> {
        let Ok(mut inner) = self.inner.lock() else {
            return None;
        };
        inner.clock = inner.clock.wrapping_add(1);
        let now = inner.clock;
        let stale = match inner.entries.get_mut(&key) {
            None => return None,
            Some(entry) if entry.defaults == defaults => {
                entry.touched = now;
                return Some(entry.value.clone());
            }
            Some(_) => true,
        };
        if stale {
            inner.entries.remove(&key);
        }
        None
    }

    /// Forget control `key`.
    pub fn forget(&self, key: u64) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.entries.remove(&key);
        }
    }

    /// A form reset of the replaced control whose root carries `control` in
    /// its [`MEMORY_KEY_ATTRIBUTE`]: forget the user's value, so the next
    /// build shows the app's default again (for a radio: its whole group's).
    /// Whether there was a value to forget - i.e. whether that rebuild would
    /// show anything new.
    #[must_use]
    pub fn reset_control(&self, control: u64) -> bool {
        let Ok(mut inner) = self.inner.lock() else {
            return false;
        };
        // An unregistered control (a memory that never built it) is keyed
        // by its own entry, as every control but a grouped radio is.
        let value_key = match inner.controls.get_mut(&control) {
            Some(registered) => {
                // Until the rebuild, the control reads as its default.
                registered.built = registered.default.clone();
                registered.value_key
            }
            None => control,
        };
        inner.entries.remove(&value_key).is_some()
    }

    /// The app's default of the replaced control whose root carries
    /// `control` in its [`MEMORY_KEY_ATTRIBUTE`] - what a form reset puts it
    /// back to. `None` for a control this memory did not build.
    #[must_use]
    pub fn default_of(&self, control: u64) -> Option<FormValue> {
        let inner = self.inner.lock().ok()?;
        let default = inner.controls.get(&control).map(|r| r.default.clone());
        default
    }

    /// What the replaced control whose root carries `control` in its
    /// [`MEMORY_KEY_ATTRIBUTE`] contributes to its form's `FormData` NOW:
    /// the user's latest value, else the one it was built with.
    #[must_use]
    pub fn submission(&self, control: u64) -> Submission {
        let Ok(inner) = self.inner.lock() else {
            return Submission::Unknown;
        };
        let Some(registered) = inner.controls.get(&control) else {
            return Submission::Unknown;
        };
        let current = inner
            .entries
            .get(&registered.value_key)
            .map_or(&registered.built, |r| &r.value);
        Submission::Entries(registered.spelling.spell(current))
    }

    /// Note that a build produced the replaced control `control`: its user
    /// value lives under `value_key`, it was built with `built`, and the
    /// app's default is `default`.
    fn register(
        &self,
        control: u64,
        value_key: u64,
        spelling: Spelling,
        built: FormValue,
        default: FormValue,
    ) {
        let Ok(mut inner) = self.inner.lock() else {
            return;
        };
        inner.clock = inner.clock.wrapping_add(1);
        let touched = inner.clock;
        inner.controls.insert(
            control,
            Registered {
                value_key,
                spelling,
                built,
                default,
                touched,
            },
        );
        evict_beyond_bound(&mut inner.controls, |r| r.touched);
    }

    /// Forget every control.
    pub fn clear(&self) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.entries.clear();
            inner.controls.clear();
        }
    }

    /// How many controls hold a user value.
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.lock().map_or(0, |inner| inner.entries.len())
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

// ── The pass ────────────────────────────────────────────────────────────────

/// Replace every raw `<input>` / `<select>` / `<textarea>` in `dom` by the
/// widget its type names, and every raw `<form>` by a [`Form`]; give every
/// `<datalist>` HTML's `display: none`. Returns how many were replaced.
///
/// `memory` supplies the values users gave the controls on earlier builds
/// and receives the new ones; `scope` keeps one DOM's controls apart from
/// another's ([`FORM_SCOPE_ROOT`] for the layout callback's DOM). A DOM with
/// no form control costs one walk and is left untouched.
pub fn resolve_form_controls_in_dom(dom: &mut Dom, memory: &FormControlMemory, scope: u64) -> usize {
    let mut pre = Prepass::default();
    prepass(dom, &mut pre);
    if !pre.has_controls && !pre.has_datalists {
        return 0;
    }
    let ctx = Ctx { memory, scope, pre };
    let mut path = Vec::new();
    let replaced = resolve_inner(dom, &ctx, &mut path);
    if replaced > 0 {
        // Replacing a child in place leaves every ancestor's cached
        // descendant count stale; the arena conversion trusts it.
        let _ = dom.fixup_children_estimated();
    }
    replaced
}

/// What one walk over the whole DOM learns before any node is replaced: a
/// control can depend on nodes elsewhere in the tree.
#[derive(Debug, Default)]
struct Prepass {
    has_controls: bool,
    /// A `<datalist>` anywhere: it gets HTML's `display: none`.
    has_datalists: bool,
    /// `<datalist id=..>` -> its options, for `<input list=..>`.
    datalists: BTreeMap<String, Vec<Choice>>,
    /// Radio `name` -> the `value` of the radio checked by DEFAULT (the last
    /// one in tree order, as in HTML; empty when none is).
    radio_defaults: BTreeMap<String, String>,
}

struct Ctx<'a> {
    memory: &'a FormControlMemory,
    scope: u64,
    pre: Prepass,
}

fn prepass(dom: &Dom, out: &mut Prepass) {
    let node = &dom.root;
    match node.get_node_type() {
        NodeType::Form if !opted_out(node) && !is_form_widget(node) => {
            out.has_controls = true;
        }
        NodeType::Button if !opted_out(node) && !is_button_widget(node) => {
            out.has_controls = true;
        }
        NodeType::Input | NodeType::Select | NodeType::TextArea if !opted_out(node) => {
            out.has_controls = true;
            if matches!(node.get_node_type(), NodeType::Input) && input_type(node) == "radio" {
                if let Some(name) = attr_value(node, "name").filter(|n| !n.is_empty()) {
                    let checked = is_checked(node);
                    let entry = out.radio_defaults.entry(name).or_default();
                    if checked {
                        *entry = radio_value(node);
                    }
                }
            }
        }
        NodeType::DataList => {
            out.has_datalists = true;
            if let Some(id) = first_id(node) {
                let mut choices = Vec::new();
                collect_choices(dom, &mut choices, None);
                out.datalists.insert(id, choices);
            }
        }
        _ => {}
    }
    for child in &dom.children {
        prepass(child, out);
    }
}

fn resolve_inner(dom: &mut Dom, ctx: &Ctx<'_>, path: &mut Vec<u32>) -> usize {
    if matches!(dom.root.get_node_type(), NodeType::DataList) {
        // Left in place (the combobox reads it, an app may too), but not
        // shown - and its options are not controls: nothing to replace.
        hide_datalist(&mut dom.root);
        return 0;
    }
    if let Some((kind, replacement)) = replacement_for(dom, ctx, path) {
        *dom = replacement;
        if kind != FormWidget::Form {
            // The widget is never walked: widgets are built from widgets,
            // never from raw form nodes.
            return 1;
        }
        // A form's controls are its content: resolved INSIDE the Form they
        // now sit in, at the same tree paths as before.
        return 1 + resolve_children(dom, ctx, path);
    }
    resolve_children(dom, ctx, path)
}

fn resolve_children(dom: &mut Dom, ctx: &Ctx<'_>, path: &mut Vec<u32>) -> usize {
    let mut replaced = 0;
    for (i, child) in dom.children.as_mut().iter_mut().enumerate() {
        path.push(u32::try_from(i).unwrap_or(u32::MAX));
        replaced += resolve_inner(child, ctx, path);
        path.pop();
    }
    replaced
}

/// The widget `raw` becomes (and which one), or `None` when it is not a form
/// control (or opted out).
fn replacement_for(raw: &Dom, ctx: &Ctx<'_>, path: &[u32]) -> Option<(FormWidget, Dom)> {
    let node = &raw.root;
    if opted_out(node) {
        return None;
    }
    let kind = widget_for(node, &ctx.pre)?;
    let spec = Spec::read(raw, kind);
    let mut widget = build(kind, &spec, raw, ctx, path);
    graft(raw, &mut widget, kind, &spec);
    Some((kind, widget))
}

/// HTML's user-agent sheet: `datalist { display: none }` - a datalist is
/// suggestions for an input, never content. The default goes BEFORE the
/// node's own inline style, so an app that shows its datalist still can;
/// a second pass finds it in place and adds nothing.
fn hide_datalist(node: &mut NodeData) {
    let user_agent: Css = CssPropertyWithConditionsVec::from_vec(alloc::vec![
        CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::None)),
    ])
    .into();
    let mut rules = user_agent.rules.into_library_owned_vec();
    let own = node.style.rules.as_slice();
    if own.starts_with(&rules) {
        return;
    }
    rules.extend(own.iter().cloned());
    node.style.rules = rules.into();
}

/// A form node that already IS a [`Form`] - the app built one, or this pass
/// did on an earlier run - carries the Form's state.
fn is_form_widget(node: &NodeData) -> bool {
    crate::widgets::form::form_state_of(node).is_some()
}

/// The class every Button WIDGET's root wears (`themes::flat::button`,
/// `themes::flora::button`).
const BUTTON_WIDGET_CLASS: &str = "__azul-native-button";

/// A `<button>` node that already IS a Button widget's root - the app built
/// one, or this pass did on an earlier run - is no raw `<button>`.
fn is_button_widget(node: &NodeData) -> bool {
    node.has_class(BUTTON_WIDGET_CLASS)
}

/// Which widget `node` becomes, through [`INPUT_TYPE_WIDGETS`].
fn widget_for(node: &NodeData, pre: &Prepass) -> Option<FormWidget> {
    let row = match node.get_node_type() {
        NodeType::Input => input_type(node),
        // `<optgroup>`s become the drop-down's headings (`collect_choices`).
        NodeType::Select => String::from("<select>"),
        NodeType::TextArea => String::from("<textarea>"),
        NodeType::Form if !is_form_widget(node) => String::from("<form>"),
        NodeType::Button if !is_button_widget(node) => {
            // `type=submit|reset|button` acts as the `<input>` of that type;
            // a missing or unknown `type` is the `<button>` row.
            let ty = input_type(node);
            let as_input = INPUT_TYPE_WIDGETS
                .iter()
                .any(|(t, kind)| *t == ty && matches!(kind, FormWidget::Button(_)));
            if as_input {
                ty
            } else {
                String::from("<button>")
            }
        }
        _ => return None,
    };
    let kind = INPUT_TYPE_WIDGETS
        .iter()
        .find(|(ty, _)| *ty == row)
        .map_or(TEXT_STATE, |(_, kind)| *kind);
    // `list=` naming a datalist with options: a combobox. Not for a
    // password, whose value must not be suggested (HTML ignores `list` there).
    if matches!(kind, FormWidget::TextInput(k) if k != TextInputKind::Password) {
        let listed = attr_value(node, "list")
            .and_then(|id| pre.datalists.get(&id))
            .is_some_and(|choices| !choices.is_empty());
        if listed {
            return Some(FormWidget::ComboBox);
        }
    }
    Some(kind)
}

// ── Reading the raw node ────────────────────────────────────────────────────

/// One `<option>`.
#[derive(Debug, Clone, PartialEq, Hash)]
struct Choice {
    value: String,
    label: String,
    selected: bool,
    disabled: bool,
}

/// One `<optgroup>`: its label, over `len` choices from `first` on.
#[derive(Debug, Clone, PartialEq, Hash)]
struct Group {
    label: String,
    first: usize,
    len: usize,
}

/// Everything the widget is built from, read once off the raw node.
#[derive(Debug, Default)]
struct Spec {
    /// The input `type`, lower-case (empty for select / textarea).
    ty: String,
    value: Option<String>,
    placeholder: Option<String>,
    /// The accessible name.
    label: Option<String>,
    min: Option<String>,
    max: Option<String>,
    step: Option<String>,
    checked: bool,
    disabled: bool,
    readonly: bool,
    maxlength: Option<usize>,
    size: Option<u32>,
    rows: Option<u32>,
    cols: Option<u32>,
    alt: Option<String>,
    /// HTML `pattern`, for the text-like types.
    pattern: Option<String>,
    /// HTML `accept` of a file input, one token per entry.
    accept: Vec<String>,
    /// HTML `multiple` of a file input.
    multiple: bool,
    /// A `<select>`'s options, or the options of the datalist `list` names.
    choices: Vec<Choice>,
    /// A `<select>`'s `<optgroup>`s, over runs of `choices`.
    groups: Vec<Group>,
    /// A `<textarea>`'s text content; a `<button>`'s (its label).
    text: String,
}

impl Spec {
    fn read(raw: &Dom, kind: FormWidget) -> Self {
        let node = &raw.root;
        let label = attr_value(node, "aria-label")
            .or_else(|| {
                node.get_accessibility_info()
                    .and_then(|a| a.accessibility_name.as_ref().map(|n| n.as_str().to_string()))
            })
            .or_else(|| attr_value(node, "title"))
            .filter(|l| !l.trim().is_empty());
        let mut choices = Vec::new();
        let mut groups = Vec::new();
        if kind == FormWidget::DropDown {
            collect_choices(raw, &mut choices, Some(&mut groups));
        }
        let text = if kind == FormWidget::TextArea {
            let content = text_content(raw);
            // HTML drops ONE newline straight after `<textarea>`.
            let content = content
                .strip_prefix("\r\n")
                .or_else(|| content.strip_prefix('\n'))
                .map_or_else(|| content.clone(), str::to_string);
            if content.is_empty() {
                attr_value(node, "value").unwrap_or_default()
            } else {
                content
            }
        } else if matches!(node.get_node_type(), NodeType::Button) {
            text_content(raw).trim().to_string()
        } else {
            String::new()
        };
        Self {
            ty: input_type(node),
            value: attr_value(node, "value"),
            placeholder: attr_value(node, "placeholder").filter(|p| !p.is_empty()),
            label,
            min: attr_value(node, "min"),
            max: attr_value(node, "max"),
            step: attr_value(node, "step"),
            checked: is_checked(node),
            disabled: flag(node, "disabled"),
            readonly: flag(node, "readonly"),
            maxlength: attr_value(node, "maxlength").and_then(|n| n.trim().parse::<usize>().ok()),
            size: positive(attr_value(node, "size")),
            rows: positive(attr_value(node, "rows")),
            cols: positive(attr_value(node, "cols")),
            alt: attr_value(node, "alt"),
            pattern: attr_value(node, "pattern").filter(|p| !p.is_empty()),
            accept: attr_value(node, "accept")
                .map(|a| {
                    a.split(',')
                        .map(|t| t.trim().to_string())
                        .filter(|t| !t.is_empty())
                        .collect()
                })
                .unwrap_or_default(),
            multiple: flag(node, "multiple"),
            choices,
            groups,
            text,
        }
    }

    /// What the app said the control starts as. The user's value is
    /// honoured only while this is unchanged.
    fn defaults(&self, kind: FormWidget) -> u64 {
        let mut h = azul_core::hash::DefaultHasher::new();
        kind.hash(&mut h);
        self.ty.hash(&mut h);
        self.value.hash(&mut h);
        self.checked.hash(&mut h);
        self.min.hash(&mut h);
        self.max.hash(&mut h);
        self.step.hash(&mut h);
        self.choices.hash(&mut h);
        self.text.hash(&mut h);
        h.finish()
    }
}

fn positive(v: Option<String>) -> Option<u32> {
    v.and_then(|n| n.trim().parse::<u32>().ok()).filter(|n| *n > 0)
}

/// The attribute `name` (HTML spelling, ASCII-case-insensitive), whatever
/// variant carries it: typed (`AttributeType::Min`), `data-*` or custom.
fn attr_value(node: &NodeData, name: &str) -> Option<String> {
    node.attributes()
        .iter()
        .find(|a| a.name().eq_ignore_ascii_case(name))
        .map(|a| a.value().as_str().to_string())
}

/// A boolean attribute: present means on (HTML), except an explicit
/// `"false"` on a custom / data spelling.
fn flag(node: &NodeData, name: &str) -> bool {
    node.attributes().iter().any(|a| {
        a.name().eq_ignore_ascii_case(name)
            && (a.is_boolean() || !a.value().as_str().trim().eq_ignore_ascii_case("false"))
    })
}

fn is_checked(node: &NodeData) -> bool {
    for a in node.attributes() {
        match a {
            AttributeType::CheckedTrue => return true,
            AttributeType::CheckedFalse => return false,
            AttributeType::Custom(nv) | AttributeType::Data(nv)
                if nv.attr_name.as_str().eq_ignore_ascii_case("checked") =>
            {
                return !nv.value.as_str().trim().eq_ignore_ascii_case("false");
            }
            _ => {}
        }
    }
    false
}

fn input_type(node: &NodeData) -> String {
    attr_value(node, "type")
        .map(|t| t.trim().to_ascii_lowercase())
        .unwrap_or_default()
}

fn opted_out(node: &NodeData) -> bool {
    attr_value(node, OPT_OUT_ATTRIBUTE)
        .is_some_and(|v| v.trim().eq_ignore_ascii_case(OPT_OUT_VALUE))
}

fn first_id(node: &NodeData) -> Option<String> {
    node.attributes().iter().find_map(|a| match a {
        AttributeType::Id(id) => Some(id.as_str().to_string()),
        _ => None,
    })
}

/// A radio's `value`; HTML's default is `"on"`.
fn radio_value(node: &NodeData) -> String {
    attr_value(node, "value").unwrap_or_else(|| String::from("on"))
}

/// Every text leaf under `dom`, concatenated - the text a reader sees: an
/// `<icon>`'s spec text names a glyph and is left out.
pub(crate) fn text_content(dom: &Dom) -> String {
    let mut out = String::new();
    push_text(dom, &mut out);
    out
}

fn push_text(dom: &Dom, out: &mut String) {
    match dom.root.get_node_type() {
        NodeType::Icon(_) => return,
        NodeType::Text(t) => out.push_str(t.as_str()),
        _ => {}
    }
    for child in &dom.children {
        push_text(child, out);
    }
}

/// The `<option>`s of a `<select>` or `<datalist>`, in order, looking
/// through `<optgroup>`s - which, when `groups` asks for them, are recorded
/// as headings over their options. HTML's optgroups do not nest; an option
/// in a group inside a group belongs to the outer one.
fn collect_choices(dom: &Dom, out: &mut Vec<Choice>, mut groups: Option<&mut Vec<Group>>) {
    for child in &dom.children {
        match child.root.get_node_type() {
            NodeType::SelectOption => {
                let text = text_content(child).trim().to_string();
                let value = attr_value(&child.root, "value").unwrap_or_else(|| text.clone());
                let label = attr_value(&child.root, "label")
                    .filter(|l| !l.is_empty())
                    .or_else(|| (!text.is_empty()).then(|| text.clone()))
                    .unwrap_or_else(|| value.clone());
                out.push(Choice {
                    value,
                    label,
                    selected: flag(&child.root, "selected"),
                    disabled: flag(&child.root, "disabled"),
                });
            }
            NodeType::OptGroup => {
                let first = out.len();
                collect_choices(child, out, None);
                if let Some(groups) = groups.as_deref_mut() {
                    // `label` is HTML's; `Dom::create_optgroup*` names the
                    // group through its aria-label.
                    let label = attr_value(&child.root, "label")
                        .or_else(|| attr_value(&child.root, "aria-label"))
                        .unwrap_or_default();
                    groups.push(Group {
                        label,
                        first,
                        len: out.len() - first,
                    });
                }
            }
            _ => {}
        }
    }
}

/// HTML's selectedness: the last `selected` option, else the first one that
/// is not disabled.
fn default_choice(choices: &[Choice]) -> usize {
    choices
        .iter()
        .rposition(|c| c.selected)
        .or_else(|| choices.iter().position(|c| !c.disabled))
        .unwrap_or(0)
}

/// A drop-down over `choices`, each `<optgroup>` a heading over its run of
/// options (`DropDown::add_optgroup`), the ungrouped options around the
/// groups in document order.
fn grouped_drop_down(choices: &[Choice], groups: &[Group]) -> DropDown {
    fn labels(choices: &[Choice]) -> Vec<AzString> {
        choices.iter().map(|c| AzString::from(c.label.clone())).collect()
    }
    fn append(dd: &mut DropDown, choices: &[Choice]) {
        if choices.is_empty() {
            return;
        }
        let mut all = core::mem::replace(&mut dd.choices, StringVec::from_const_slice(&[]))
            .into_library_owned_vec();
        all.extend(labels(choices));
        dd.choices = all.into();
    }

    let mut dd = DropDown::new(StringVec::from_const_slice(&[]));
    let mut at = 0;
    for group in groups {
        let end = (group.first + group.len).min(choices.len()).max(at);
        let start = group.first.clamp(at, end);
        append(&mut dd, choices.get(at..start).unwrap_or(&[]));
        dd.add_optgroup(
            AzString::from(group.label.clone()),
            StringVec::from_vec(labels(choices.get(start..end).unwrap_or(&[]))),
        );
        at = end;
    }
    append(&mut dd, choices.get(at..).unwrap_or(&[]));
    dd
}

fn parse_f32(v: Option<&String>) -> Option<f32> {
    v.and_then(|s| s.trim().parse::<f32>().ok())
        .filter(|f| f.is_finite())
}

/// `(year, month, day)` from a `date` / `month` / `week` /
/// `datetime-local` value. A `month` is its first day; a `week` is the
/// MONDAY of that ISO 8601 week (week 1 holds the year's first Thursday), and
/// a week the year does not have (`W00`, `W53` of a 52-week year) is no
/// value at all, as in HTML.
fn parse_date(ty: &str, value: &str) -> Option<(u32, u32, u32)> {
    let v = value.trim();
    match ty {
        "month" => {
            let (y, m) = v.split_once('-')?;
            let month: u32 = m.parse().ok()?;
            if !(1..=12).contains(&month) {
                return None;
            }
            Some((y.parse().ok()?, month, 1))
        }
        "week" => {
            let (y, w) = v.split_once("-W").or_else(|| v.split_once("-w"))?;
            let year: u32 = y.parse().ok()?;
            let week: u32 = w.parse().ok()?;
            if week == 0 || week > iso_weeks_in_year(year) {
                return None;
            }
            Some(iso_week_monday(year, week))
        }
        _ => {
            // `date`, and the date half of `datetime-local`.
            let date = v.split(['T', 't', ' ']).next()?;
            let mut parts = date.split('-');
            let year = parts.next()?.parse().ok()?;
            let month = parts.next()?.parse().ok()?;
            let day = parts.next()?.parse().ok()?;
            Some((year, month, day))
        }
    }
}

/// `(hour, minute)` from `HH:MM[:SS]`.
fn parse_time(value: &str) -> Option<(u32, u32)> {
    let mut parts = value.trim().split(':');
    let hour = parts.next()?.trim().parse().ok()?;
    let minute = parts.next()?.trim().parse().ok()?;
    Some((hour, minute))
}

/// `((year, month, day), (hour, minute))` from a `datetime-local` value,
/// `YYYY-MM-DDTHH:MM[:SS]` (HTML also accepts a space for the `T`).
fn parse_datetime(value: &str) -> Option<((u32, u32, u32), (u32, u32))> {
    let (date, time) = value.trim().split_once(['T', 't', ' '])?;
    Some((parse_date("date", date)?, parse_time(time)?))
}

/// The date a picker in `mode` shows when the app gave it no usable value
/// (HTML shows an empty field; a picker always shows a date).
fn fallback_date(mode: DatePickerMode) -> (u32, u32, u32) {
    match mode {
        DatePickerMode::Week => iso_week_monday(2000, 1),
        DatePickerMode::Date | DatePickerMode::Month => (2000, 1, 1),
    }
}

/// A range's value, HTML-style: snapped to `step` from `min` (default step
/// 1, `"any"` = no snapping), then clamped to `[min, max]`.
fn snap_to_step(value: f32, min: f32, max: f32, step: Option<&String>) -> f32 {
    let step = match step.map(|s| s.trim()) {
        Some(s) if s.eq_ignore_ascii_case("any") => None,
        Some(s) => s.parse::<f32>().ok().filter(|s| s.is_finite() && *s > 0.0),
        None => Some(1.0),
    };
    let snapped = match step {
        Some(step) => min + ((value - min) / step).round() * step,
        None => value,
    };
    if snapped.is_nan() {
        min
    } else if snapped < min {
        min
    } else if snapped > max {
        max
    } else {
        snapped
    }
}

// ── Identity ────────────────────────────────────────────────────────────────

/// Which control this is, across builds: its key, else its id, else where
/// it sits in the tree (and its name) - always within its DOM's scope.
fn identity_key(scope: u64, path: &[u32], node: &NodeData, kind: FormWidget) -> u64 {
    let mut h = azul_core::hash::DefaultHasher::new();
    scope.hash(&mut h);
    kind.hash(&mut h);
    if let Some(key) = node.get_key() {
        1u8.hash(&mut h);
        key.hash(&mut h);
    } else if let Some(id) = first_id(node) {
        2u8.hash(&mut h);
        id.hash(&mut h);
    } else {
        3u8.hash(&mut h);
        path.hash(&mut h);
        attr_value(node, "name").hash(&mut h);
    }
    h.finish()
}

/// A named radio group is ONE control: its value is which radio is checked.
fn radio_group_key(scope: u64, name: &str) -> u64 {
    let mut h = azul_core::hash::DefaultHasher::new();
    scope.hash(&mut h);
    4u8.hash(&mut h);
    name.hash(&mut h);
    h.finish()
}

// ── Recorders: the widgets' typed change hooks ──────────────────────────────

/// The payload of every recorder: where the user's value goes.
#[derive(Debug, Clone)]
struct Recorder {
    memory: FormControlMemory,
    key: u64,
    defaults: u64,
    /// A radio's own `value` - what its group remembers when it is checked.
    radio_value: String,
}

fn remember_with(data: &mut RefAny, make: impl FnOnce(&Recorder) -> FormValue) {
    let Some(recorder) = data.downcast_ref::<Recorder>() else {
        return;
    };
    let value = make(&recorder);
    recorder
        .memory
        .remember(recorder.key, recorder.defaults, value);
}

extern "C" fn record_check_box(mut data: RefAny, _info: CallbackInfo, state: CheckBoxState) -> Update {
    remember_with(&mut data, |_| FormValue::Checked(state.checked));
    Update::DoNothing
}

/// A radio was checked: its group remembers it, and the rebuild this asks
/// for unchecks the group's other radios.
extern "C" fn record_radio(mut data: RefAny, _info: CallbackInfo, _state: RadioGroupState) -> Update {
    remember_with(&mut data, |r| FormValue::Text(r.radio_value.clone()));
    Update::RefreshDom
}

extern "C" fn record_color(mut data: RefAny, _info: CallbackInfo, state: ColorInputState) -> Update {
    remember_with(&mut data, |_| FormValue::Color(state.color));
    Update::DoNothing
}

extern "C" fn record_file(mut data: RefAny, _info: CallbackInfo, state: FileInputState) -> Update {
    // Every picked file; a state that names only `path` holds that one.
    let mut paths: Vec<String> = state
        .paths
        .as_ref()
        .iter()
        .map(|p| p.as_str().to_string())
        .collect();
    if paths.is_empty() {
        paths.extend(state.path.as_ref().map(|p| p.as_str().to_string()));
    }
    remember_with(&mut data, |_| FormValue::Files(paths));
    // The file input relabels by a rebuild (it asks for one itself too).
    Update::RefreshDom
}

extern "C" fn record_number(mut data: RefAny, _info: CallbackInfo, state: NumberInputState) -> Update {
    remember_with(&mut data, |_| FormValue::Number(state.number));
    Update::DoNothing
}

extern "C" fn record_slider(mut data: RefAny, _info: CallbackInfo, state: SliderState) -> Update {
    remember_with(&mut data, |_| FormValue::Number(state.value));
    Update::DoNothing
}

extern "C" fn record_date(mut data: RefAny, _info: CallbackInfo, state: DatePickerState) -> Update {
    remember_with(&mut data, |_| FormValue::Date {
        year: state.year,
        month: state.month,
        day: state.day,
    });
    Update::DoNothing
}

extern "C" fn record_time(mut data: RefAny, _info: CallbackInfo, state: TimePickerState) -> Update {
    let hour = state.canonical_hour();
    remember_with(&mut data, |_| FormValue::Time {
        hour,
        minute: state.minute,
    });
    Update::DoNothing
}

extern "C" fn record_datetime_local(
    mut data: RefAny,
    _info: CallbackInfo,
    state: DateTimeLocalPickerState,
) -> Update {
    let hour = state.time.canonical_hour();
    remember_with(&mut data, |_| FormValue::DateTime {
        year: state.date.year,
        month: state.date.month,
        day: state.date.day,
        hour,
        minute: state.time.minute,
    });
    Update::DoNothing
}

/// A drop-down shows a choice only by being rebuilt with it.
extern "C" fn record_choice(mut data: RefAny, _info: CallbackInfo, choice: usize) -> Update {
    remember_with(&mut data, |_| FormValue::Choice(choice));
    Update::RefreshDom
}

extern "C" fn record_combobox(mut data: RefAny, _info: CallbackInfo, state: ComboBoxState) -> Update {
    let text = state.text.as_str().to_string();
    remember_with(&mut data, |_| FormValue::Text(text));
    Update::DoNothing
}

extern "C" fn record_text_input(
    mut data: RefAny,
    _info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    let text = state.get_text();
    remember_with(&mut data, |_| FormValue::Text(text));
    OnTextInputReturn {
        update: Update::DoNothing,
        valid: TextInputValid::Yes,
    }
}

extern "C" fn record_text_area(
    mut data: RefAny,
    _info: CallbackInfo,
    state: TextAreaState,
) -> OnTextInputReturn {
    let text = state.get_text();
    remember_with(&mut data, |_| FormValue::Text(text));
    OnTextInputReturn {
        update: Update::DoNothing,
        valid: TextInputValid::Yes,
    }
}

// ── A raw <form>'s own handlers ─────────────────────────────────────────────

/// The engine's form events: `Submit` and `Reset` on the form node.
const fn is_form_event(event: EventFilter) -> bool {
    matches!(
        event,
        EventFilter::Hover(HoverEventFilter::Submit | HoverEventFilter::Reset)
    )
}

/// The app's own `Submit` (or `Reset`) handlers of a raw `<form>` - plain
/// callbacks - carried into the Form's `on_submit` (`on_reset`).
#[derive(Debug, Clone)]
struct CarriedFormHandlers {
    handlers: Vec<CoreCallbackData>,
}

/// Run every carried handler, in the order the app attached them, with the
/// `CallbackInfo` of the event that submitted (reset) the form. The values
/// are one `collect_form_data` away for them; the `FormData` argument has no
/// place in their signature.
fn run_carried(data: &mut RefAny, info: CallbackInfo) -> Update {
    let handlers = match data.downcast_ref::<CarriedFormHandlers>() {
        Some(carried) => carried.handlers.clone(),
        None => return Update::DoNothing,
    };
    let mut update = Update::DoNothing;
    for handler in handlers {
        let result = Callback::from_core(handler.callback).invoke(handler.refany, info);
        update.max_self(result);
    }
    update
}

extern "C" fn run_carried_submit(mut data: RefAny, info: CallbackInfo, _values: FormData) -> Update {
    run_carried(&mut data, info)
}

extern "C" fn run_carried_reset(mut data: RefAny, info: CallbackInfo, _initial: FormData) -> Update {
    run_carried(&mut data, info)
}

/// The Form a raw `<form>` becomes: its content (still raw - the caller
/// resolves it inside), the app's handlers as `on_submit` / `on_reset`, and
/// the raw form's block layout rather than the Form widget's column.
fn form_for(raw: &Dom, label: Option<String>) -> Dom {
    let (mut submit, mut reset) = (Vec::new(), Vec::new());
    for handler in raw.root.callbacks.as_ref() {
        match handler.event {
            EventFilter::Hover(HoverEventFilter::Submit) => submit.push(handler.clone()),
            EventFilter::Hover(HoverEventFilter::Reset) => reset.push(handler.clone()),
            _ => {}
        }
    }
    let mut form = Form::create(raw.children.clone()).with_container_style(
        CssPropertyWithConditionsVec::from_vec(alloc::vec![CssPropertyWithConditions::simple(
            CssProperty::const_display(LayoutDisplay::Block)
        )]),
    );
    if !submit.is_empty() {
        let run: FormOnSubmitCallbackType = run_carried_submit;
        form = form.with_on_submit(RefAny::new(CarriedFormHandlers { handlers: submit }), run);
    }
    if !reset.is_empty() {
        let run: FormOnResetCallbackType = run_carried_reset;
        form = form.with_on_reset(RefAny::new(CarriedFormHandlers { handlers: reset }), run);
    }
    if let Some(label) = label {
        form = form.with_accessibility_name(label);
    }
    let mut dom = form.dom();
    // The raw form's dataset is the APP's - what its handlers find with
    // `get_dataset(form)`. It stays on the form node; the Form finds its own
    // state through its `Submit` handler (`widgets::form::form_state_of`).
    if let Some(dataset) = raw.root.get_dataset() {
        dom.root.set_dataset(OptionRefAny::Some(dataset.clone()));
    }
    dom
}

// ── Building the widget ─────────────────────────────────────────────────────

const BLACK: ColorU = ColorU {
    r: 0,
    g: 0,
    b: 0,
    a: 255,
};

/// The widget for `kind`, seeded from `spec` - or from the user's value, if
/// the memory holds one for this control and the app's defaults are the
/// ones it was given under.
#[allow(clippy::too_many_lines)] // one arm per widget: the table's other half
fn build(kind: FormWidget, spec: &Spec, raw: &Dom, ctx: &Ctx<'_>, path: &[u32]) -> Dom {
    let node = &raw.root;
    // A named radio group is one control; everything else is its own.
    let group = (kind == FormWidget::Radio)
        .then(|| attr_value(node, "name"))
        .flatten()
        .filter(|n| !n.is_empty());
    let (key, defaults) = match &group {
        Some(name) => {
            let group_default = ctx.pre.radio_defaults.get(name).cloned().unwrap_or_default();
            let mut h = azul_core::hash::DefaultHasher::new();
            group_default.hash(&mut h);
            (radio_group_key(ctx.scope, name), h.finish())
        }
        None => (identity_key(ctx.scope, path, node, kind), spec.defaults(kind)),
    };
    let own_value = radio_value(node);
    // What the widget is built with: the user's value while the app's
    // defaults are the ones it was given under, else the default. `None`
    // for the controls that hold no value.
    let default = default_value(kind, spec);
    let value = default
        .as_ref()
        .map(|d| resolved_value(ctx.memory.recall(key, defaults), d, &own_value));
    let recorder = RefAny::new(Recorder {
        memory: ctx.memory.clone(),
        key,
        defaults,
        radio_value: own_value,
    });
    let name = spec.label.clone();

    let mut dom = match kind {
        FormWidget::TextInput(text_kind) => {
            let text = text_of(value.as_ref());
            // The kind masks a password, adds a search's clear button and
            // checks an e-mail or URL; `name` / `type` reach the root by the
            // graft, like every other attribute.
            let mut w = TextInput::create_with_kind(text_kind).with_text(text.into());
            if let Some(p) = &spec.placeholder {
                w = w.with_placeholder(p.clone().into());
            }
            if let Some(p) = &spec.pattern {
                w = w.with_pattern(p.clone().into());
            }
            if let Some(max) = spec.maxlength {
                w.text_input_state.inner.max_len = max;
            }
            if let Some(n) = name {
                w = w.with_accessibility_name(n);
            }
            let hook: TextInputOnTextInputCallbackType = record_text_input;
            w.with_on_text_input(recorder, hook).dom()
        }
        FormWidget::TextArea => {
            let text = text_of(value.as_ref());
            let mut w = TextArea::create().with_text(text.into());
            if let Some(p) = &spec.placeholder {
                w = w.with_placeholder(p.clone().into());
            }
            if let Some(max) = spec.maxlength {
                w.text_area_state.inner.max_len = max;
            }
            if let Some(n) = name {
                w = w.with_accessibility_name(n);
            }
            let hook: TextAreaOnTextInputCallbackType = record_text_area;
            w.with_on_text_input(recorder, hook).dom()
        }
        FormWidget::NumberInput => {
            let mut w = NumberInput::create(number_of(value.as_ref()));
            if let Some(min) = parse_f32(spec.min.as_ref()) {
                w.number_input_state.inner.min = min;
            }
            if let Some(max) = parse_f32(spec.max.as_ref()) {
                w.number_input_state.inner.max = max;
            }
            if let Some(p) = &spec.placeholder {
                w.text_input.set_placeholder(p.clone().into());
            }
            if let Some(n) = name {
                w = w.with_accessibility_name(n);
            }
            let hook: NumberInputOnValueChangeCallbackType = record_number;
            w.with_on_value_change(recorder, hook).dom()
        }
        FormWidget::CheckBox => {
            let checked = matches!(value, Some(FormValue::Checked(true)));
            let hook: CheckBoxOnToggleCallbackType = record_check_box;
            let mut w = CheckBox::create(checked).with_on_toggle(recorder, hook);
            if let Some(n) = name {
                w = w.with_accessibility_name(n);
            }
            w.dom()
        }
        FormWidget::Radio => {
            // One raw radio is one radio: a single-option group whose option
            // is selected (index 0) or not (an index past the end). The
            // label is the page's business (`<label>`), as in HTML. A group
            // remembers which radio is checked; `resolved_value` turned that
            // into this radio's flag.
            let checked = matches!(value, Some(FormValue::Checked(true)));
            let hook: RadioGroupOnChangeCallbackType = record_radio;
            let mut w = RadioGroup::create(StringVec::from_vec(alloc::vec![AzString::from(
                String::new()
            )]))
            .with_selected_index(if checked { 0 } else { usize::MAX })
            .with_on_change(recorder, hook);
            if let Some(n) = name {
                w = w.with_accessibility_name(n);
            }
            w.dom()
        }
        FormWidget::ColorInput => {
            let color = match value {
                Some(FormValue::Color(c)) => c,
                _ => BLACK,
            };
            let hook: ColorInputOnValueChangeCallbackType = record_color;
            let mut w = ColorInput::create(color).with_on_value_change(recorder, hook);
            if let Some(n) = name {
                w = w.with_accessibility_name(n);
            }
            w.dom()
        }
        FormWidget::FileInput => {
            // A file input's value cannot be set by the page (HTML); only
            // the user's pick is shown. `accept` narrows what its dialog
            // offers, `multiple` lets it pick several files.
            let paths: Vec<AzString> = match &value {
                Some(FormValue::Files(paths)) => paths.iter().cloned().map(AzString::from).collect(),
                _ => Vec::new(),
            };
            let accept: Vec<AzString> = spec.accept.iter().cloned().map(AzString::from).collect();
            let hook: FileInputOnPathChangeCallbackType = record_file;
            FileInput::create(OptionString::None)
                .with_paths(StringVec::from_vec(paths))
                .with_accept(StringVec::from_vec(accept))
                .with_multiple(spec.multiple)
                .with_on_path_change(recorder, hook)
                .dom()
        }
        FormWidget::Slider => {
            let (min, max) = slider_range(spec);
            let hook: SliderOnValueChangeCallbackType = record_slider;
            let mut w = Slider::create(number_of(value.as_ref()), min, max)
                .with_on_value_change(recorder, hook);
            if let Some(n) = name {
                w = w.with_accessibility_name(n);
            }
            w.dom()
        }
        FormWidget::DatePicker(mode) => {
            let (year, month, day) = match value {
                Some(FormValue::Date { year, month, day }) => (year, month, day),
                _ => fallback_date(mode),
            };
            let picker = match mode {
                DatePickerMode::Date => DatePicker::create(year, month, day),
                DatePickerMode::Month => DatePicker::create_month(year, month),
                DatePickerMode::Week => {
                    // Whichever day of the week was stored, the picker holds
                    // the week it falls in (ISO numbering).
                    let (week_year, week) = iso_week_of(year, month, day);
                    DatePicker::create_week(week_year, week)
                }
            };
            let hook: DatePickerOnChangeCallbackType = record_date;
            let mut w = picker.with_on_change(recorder, hook);
            if let Some(n) = name {
                w = w.with_accessibility_name(n);
            }
            w.dom()
        }
        FormWidget::DateTimeLocal => {
            let ((year, month, day), (hour, minute)) = match value {
                Some(FormValue::DateTime {
                    year,
                    month,
                    day,
                    hour,
                    minute,
                }) => ((year, month, day), (hour, minute)),
                _ => ((2000, 1, 1), (0, 0)),
            };
            let hook: DateTimeLocalPickerOnChangeCallbackType = record_datetime_local;
            let mut w = DateTimeLocalPicker::create(year, month, day, hour, minute)
                .with_on_change(recorder, hook);
            if let Some(n) = name {
                w = w.with_accessibility_name(n);
            }
            w.dom()
        }
        FormWidget::TimePicker => {
            let (hour, minute) = match value {
                Some(FormValue::Time { hour, minute }) => (hour, minute),
                _ => (0, 0),
            };
            let hook: TimePickerOnChangeCallbackType = record_time;
            let mut w = TimePicker::create(hour, minute)
                .with_24h(true)
                .with_on_change(recorder, hook);
            if let Some(n) = name {
                w = w.with_accessibility_name(n);
            }
            w.dom()
        }
        FormWidget::Button(action) => {
            // An `<input>`'s label is its `value` (HTML's defaults
            // otherwise); a `<button>`'s is its content - its text, or, when
            // that content is richer than text (an icon, an image, markup),
            // the content itself, as the app wrote it. The form action is
            // what makes submit / reset act on their form.
            let element = matches!(node.get_node_type(), NodeType::Button);
            let rich = element
                && !raw
                    .children
                    .iter()
                    .all(|c| matches!(c.root.get_node_type(), NodeType::Text(_)));
            let label = if element {
                if rich {
                    String::new()
                } else {
                    spec.text.clone()
                }
            } else {
                spec.value.clone().unwrap_or_else(|| match action {
                    ButtonFormAction::Submit => String::from("Submit"),
                    ButtonFormAction::Reset => String::from("Reset"),
                    ButtonFormAction::None => String::new(),
                })
            };
            let w = match action {
                ButtonFormAction::Submit => Button::create_submit(label.into()),
                ButtonFormAction::Reset => Button::create_reset(label.into()),
                ButtonFormAction::None => Button::create(label.into()),
            };
            let mut dom = w.dom();
            if rich {
                dom.children = raw.children.clone();
            }
            // The app's name (`aria-label`, `title`), else - rich content
            // announces no label of its own - the content's text.
            let a11y_name = name.or_else(|| (rich && !spec.text.is_empty()).then(|| spec.text.clone()));
            if let Some(a11y_name) = a11y_name {
                if let Some(mut a11y) = dom.root.get_accessibility_info().cloned() {
                    a11y.accessibility_name = Some(AzString::from(a11y_name)).into();
                    dom.root.set_accessibility_info(a11y);
                }
            }
            dom
        }
        FormWidget::ImageButton => {
            // Nothing here loads `src`, so there is no image to show: HTML
            // shows the `alt` text in its place, and so does this submit
            // button - named by the same text. The root keeps `type=image`
            // (the graft lets the app's `type` win).
            let alt = spec
                .alt
                .clone()
                .filter(|a| !a.trim().is_empty())
                .unwrap_or_else(|| String::from("Submit"));
            let mut w = Button::create_submit(alt.clone().into());
            w.alt = alt.into();
            w.dom()
        }
        FormWidget::DropDown => {
            let count = spec.choices.len();
            let selected = match value {
                Some(FormValue::Choice(i)) if i < count => i,
                _ => default_choice(&spec.choices),
            };
            let hook: DropDownOnChoiceChangeCallbackType = record_choice;
            let mut w = grouped_drop_down(&spec.choices, &spec.groups)
                .with_selected(selected)
                .with_on_choice_change(recorder, hook);
            if let Some(n) = name {
                w = w.with_accessibility_name(n);
            }
            w.dom()
        }
        FormWidget::ComboBox => {
            let items: Vec<AzString> = attr_value(node, "list")
                .and_then(|id| ctx.pre.datalists.get(&id))
                .map(|choices| {
                    choices
                        .iter()
                        .map(|c| AzString::from(c.value.clone()))
                        .collect()
                })
                .unwrap_or_default();
            let text = text_of(value.as_ref());
            let selected = items.iter().position(|i| i.as_str() == text);
            // A pick AND typing: what the user typed is the value too.
            let picked: ComboBoxOnSelectCallbackType = record_combobox;
            let typed: ComboBoxOnTextInputCallbackType = record_combobox;
            let mut w = ComboBox::new(StringVec::from_vec(items))
                .with_text(text.into())
                .with_on_select(recorder.clone(), picked)
                .with_on_text_input(recorder, typed);
            if let Some(i) = selected {
                w = w.with_selected(i);
            }
            if let Some(p) = &spec.placeholder {
                w = w.with_placeholder(p.clone().into());
            }
            if let Some(n) = name {
                w = w.with_accessibility_name(n);
            }
            w.dom()
        }
        FormWidget::Hidden => HiddenInput::create(
            attr_value(node, "name").unwrap_or_default().into(),
            spec.value.clone().unwrap_or_default().into(),
        )
        .dom(),
        FormWidget::Form => form_for(raw, name),
    };

    // HTML's character / line counts, as a size the app's own style (added
    // after this, in `graft`) can still override.
    if let Some(size) = spec.size.filter(|_| kind.is_text_like() && kind != FormWidget::TextArea) {
        dom.root
            .set_css(&alloc::format!("width: {:.2}em;", char_widths_to_em(size)));
    }
    if kind == FormWidget::TextArea {
        if let Some(cols) = spec.cols {
            dom.root
                .set_css(&alloc::format!("width: {:.2}em;", char_widths_to_em(cols)));
        }
        if let Some(rows) = spec.rows {
            dom.root.set_css(&alloc::format!(
                "height: {:.2}em;",
                rows as f32 * LINE_EM + FIELD_CHROME_EM
            ));
        }
    }

    // How a form finds this control in the memory: a reset forgets its user
    // value there, a submit reads its value there. One key per CONTROL - for
    // a grouped radio its own, not the group's, which the memory maps it to.
    if let (Some(default), Some(built)) = (default, value) {
        let control = identity_key(ctx.scope, path, node, kind);
        let spelling = Spelling::of(kind, spec, node);
        ctx.memory.register(control, key, spelling, built, default);
        dom = dom.with_attribute(AttributeType::Data(AttributeNameValue {
            attr_name: AzString::from_const_str(MEMORY_KEY_ATTRIBUTE),
            value: AzString::from(control.to_string()),
        }));
    }
    dom
}

fn text_of(value: Option<&FormValue>) -> String {
    match value {
        Some(FormValue::Text(text)) => text.clone(),
        _ => String::new(),
    }
}

const fn number_of(value: Option<&FormValue>) -> f32 {
    match value {
        Some(FormValue::Number(number)) => *number,
        _ => 0.0,
    }
}

/// The value a control of `kind` starts as when the user gave it none: the
/// app's attributes, with HTML's defaults and sanitation. `None` for the
/// controls that hold no value of the user's (buttons, a hidden input, a
/// form).
fn default_value(kind: FormWidget, spec: &Spec) -> Option<FormValue> {
    let value = match kind {
        FormWidget::TextInput(_) | FormWidget::ComboBox => {
            FormValue::Text(spec.value.clone().unwrap_or_default())
        }
        FormWidget::TextArea => FormValue::Text(spec.text.clone()),
        FormWidget::NumberInput => {
            FormValue::Number(parse_f32(spec.value.as_ref()).unwrap_or(0.0))
        }
        FormWidget::CheckBox | FormWidget::Radio => FormValue::Checked(spec.checked),
        // HTML's default colour is black.
        FormWidget::ColorInput => FormValue::Color(
            spec.value
                .as_deref()
                .and_then(color_from_hex)
                .unwrap_or(BLACK),
        ),
        // A file input's value cannot be set by the page (HTML).
        FormWidget::FileInput => FormValue::Files(Vec::new()),
        FormWidget::Slider => {
            // HTML's range default: the midpoint, snapped to the step.
            let (min, max) = slider_range(spec);
            let value = parse_f32(spec.value.as_ref()).unwrap_or(min + (max - min) / 2.0);
            FormValue::Number(snap_to_step(value, min, max, spec.step.as_ref()))
        }
        FormWidget::DatePicker(mode) => {
            let ty = mode.html_type();
            let (year, month, day) = spec
                .value
                .as_deref()
                .and_then(|v| parse_date(ty, v))
                .or_else(|| spec.min.as_deref().and_then(|v| parse_date(ty, v)))
                .unwrap_or_else(|| fallback_date(mode));
            FormValue::Date { year, month, day }
        }
        FormWidget::DateTimeLocal => {
            let ((year, month, day), (hour, minute)) = spec
                .value
                .as_deref()
                .and_then(parse_datetime)
                .or_else(|| spec.min.as_deref().and_then(parse_datetime))
                .unwrap_or(((2000, 1, 1), (0, 0)));
            FormValue::DateTime {
                year,
                month,
                day,
                hour,
                minute,
            }
        }
        FormWidget::TimePicker => {
            let (hour, minute) = spec.value.as_deref().and_then(parse_time).unwrap_or((0, 0));
            FormValue::Time { hour, minute }
        }
        FormWidget::DropDown => FormValue::Choice(default_choice(&spec.choices)),
        FormWidget::Button(_) | FormWidget::ImageButton | FormWidget::Hidden | FormWidget::Form => {
            return None;
        }
    };
    Some(value)
}

/// HTML's range bounds: `[0, 100]` unless given; a `max` below `min` is
/// `min`.
fn slider_range(spec: &Spec) -> (f32, f32) {
    let min = parse_f32(spec.min.as_ref()).unwrap_or(0.0);
    let max = parse_f32(spec.max.as_ref()).unwrap_or(100.0).max(min);
    (min, max)
}

/// What a control is built with: the user's `remembered` value when it fits
/// the control, else the `default`. A radio group remembers WHICH radio is
/// checked (its value); each radio turns that into its own flag.
fn resolved_value(
    remembered: Option<FormValue>,
    default: &FormValue,
    own_radio_value: &str,
) -> FormValue {
    match remembered {
        Some(FormValue::Text(checked)) if matches!(default, FormValue::Checked(_)) => {
            FormValue::Checked(checked == own_radio_value)
        }
        Some(user) if core::mem::discriminant(&user) == core::mem::discriminant(default) => user,
        _ => default.clone(),
    }
}

/// The entries a widget the APP built (no replaced raw control) contributes
/// to its form under its `name`, read off `state` - its state, as its
/// dataset or one of its handlers' payloads carries it - and spelled exactly
/// as the replaced control of the same kind spells them ([`Spelling`]): a
/// checkbox or switch its `own_value` ("on" without one) while checked and
/// nothing otherwise, a slider its number, a colour input its hex, a file
/// input one entry per file (one empty entry for none), a drop-down its
/// chosen label, a time picker `HH:MM`. `None` when `state` is no such
/// widget's. (A radio group's value is its chosen option's LABEL, which only
/// its DOM holds - `widgets::form` reads it there.)
pub(crate) fn hand_built_entries(state: &RefAny, own_value: Option<String>) -> Option<Vec<String>> {
    use crate::widgets::{
        check_box::CheckBoxStateWrapper, color_input::ColorPickerData,
        file_input::FileInputStateWrapper, slider::SliderStateWrapper,
        switch::SwitchStateWrapper, time_picker::TimePickerStateWrapper,
    };

    let spelling = |kind: FormWidget, choice_values: Vec<String>| Spelling {
        kind,
        own_value: own_value.clone().unwrap_or_else(|| String::from("on")),
        choice_values,
    };
    let mut state = state.clone();

    // One probe per statement: each borrow of the state ends before the
    // next one is taken.
    let check_box = state
        .downcast_ref::<CheckBoxStateWrapper>()
        .map(|w| w.inner.checked);
    if let Some(checked) = check_box {
        return Some(spelling(FormWidget::CheckBox, Vec::new()).spell(&FormValue::Checked(checked)));
    }
    let switch = state
        .downcast_ref::<SwitchStateWrapper>()
        .map(|w| w.inner.checked);
    if let Some(checked) = switch {
        return Some(spelling(FormWidget::CheckBox, Vec::new()).spell(&FormValue::Checked(checked)));
    }
    let slider = state
        .downcast_ref::<SliderStateWrapper>()
        .map(|w| w.inner.value);
    if let Some(value) = slider {
        return Some(spelling(FormWidget::Slider, Vec::new()).spell(&FormValue::Number(value)));
    }
    let color = state
        .downcast_ref::<ColorPickerData>()
        .map(|d| d.current_color());
    if let Some(color) = color {
        return Some(spelling(FormWidget::ColorInput, Vec::new()).spell(&FormValue::Color(color)));
    }
    let files = state.downcast_ref::<FileInputStateWrapper>().map(|w| {
        let mut files: Vec<String> = w
            .inner
            .paths
            .as_ref()
            .iter()
            .map(|p| p.as_str().to_string())
            .collect();
        if files.is_empty() {
            files.extend(w.inner.path.as_ref().map(|p| p.as_str().to_string()));
        }
        files
    });
    if let Some(files) = files {
        return Some(spelling(FormWidget::FileInput, Vec::new()).spell(&FormValue::Files(files)));
    }
    let choice = state.downcast_ref::<DropDown>().map(|d| {
        let labels: Vec<String> = d
            .choices
            .as_ref()
            .iter()
            .map(|c| c.as_str().to_string())
            .collect();
        (d.selected, labels)
    });
    if let Some((selected, labels)) = choice {
        return Some(spelling(FormWidget::DropDown, labels).spell(&FormValue::Choice(selected)));
    }
    let time = state
        .downcast_ref::<TimePickerStateWrapper>()
        .map(|w| (w.inner.canonical_hour(), w.inner.minute));
    if let Some((hour, minute)) = time {
        return Some(
            spelling(FormWidget::TimePicker, Vec::new()).spell(&FormValue::Time { hour, minute }),
        );
    }
    None
}

/// What a RAW form control contributes to its form's INITIAL `FormData`:
/// its HTML default, spelled the way the widget it becomes will submit it
/// (a `Form` records its initial values before its content is replaced).
/// [`Submission::Unknown`] for a node that is no raw control, or opted out.
#[must_use]
pub(crate) fn default_submission(raw: &Dom) -> Submission {
    let node = &raw.root;
    if opted_out(node) {
        return Submission::Unknown;
    }
    let Some(kind) = widget_for(node, &Prepass::default()) else {
        return Submission::Unknown;
    };
    let spec = Spec::read(raw, kind);
    match kind {
        FormWidget::Form => Submission::Unknown,
        // The app's value, submitted as it is.
        FormWidget::Hidden => {
            Submission::Entries(alloc::vec![spec.value.unwrap_or_default()])
        }
        _ => match default_value(kind, &spec) {
            Some(default) => Submission::Entries(Spelling::of(kind, &spec, node).spell(&default)),
            // A button submits nothing unless it is the submitter.
            None => Submission::Entries(Vec::new()),
        },
    }
}

/// One average character of the UI font, in em.
const CHAR_EM: f32 = 0.55;
/// One line of text, in em.
const LINE_EM: f32 = 1.3;
/// A field's padding and border, in em.
const FIELD_CHROME_EM: f32 = 0.5;

fn char_widths_to_em(chars: u32) -> f32 {
    chars as f32 * CHAR_EM + FIELD_CHROME_EM
}

// ── Grafting the raw node's identity onto the widget ────────────────────────

/// Attributes the widget consumed as its INITIAL STATE. It owns that state
/// now; a copy on the root would go stale the moment the user changes it.
const fn is_consumed(attr: &AttributeType) -> bool {
    matches!(
        attr,
        AttributeType::Value(_)
            | AttributeType::CheckedTrue
            | AttributeType::CheckedFalse
            | AttributeType::Selected
            | AttributeType::Placeholder(_)
    )
}

/// A rule of a node's scoped sheet that can only mean the node itself:
/// `* { .. }` and `*:hover { .. }` - on a void `<input>` there is nothing
/// else for a bare declaration to reach.
fn targets_the_node_itself(rule: &CssRuleBlock) -> bool {
    let selectors = rule.path.selectors.as_slice();
    matches!(selectors.first(), None | Some(CssPathSelector::Global))
        && selectors
            .iter()
            .skip(1)
            .all(|s| matches!(s, CssPathSelector::PseudoSelector(_)))
}

fn graft(raw: &Dom, widget: &mut Dom, kind: FormWidget, spec: &Spec) {
    let from = &raw.root;
    // A form has no `disabled` of its own (a `<fieldset>` has; it is not
    // replaced), and its content is the app's, not the widget's to disable.
    let disabled = spec.disabled && kind != FormWidget::Form;

    // What the widget does with `disabled` / `readonly`, BEFORE the app's
    // style lands: the app can still restyle a disabled control.
    if disabled {
        disable(widget);
        widget.root.set_css("opacity: 0.5;");
        if let Some(a11y) = widget.root.accessibility.as_mut() {
            let mut states = a11y.states.clone().into_library_owned_vec();
            if !states.contains(&AccessibilityState::Unavailable) {
                states.push(AccessibilityState::Unavailable);
            }
            a11y.states = states.into();
        }
    } else if spec.readonly && kind.is_text_like() {
        not_editable(widget);
    }

    // 1. Attributes: the widget's, then the raw node's (ids and classes
    //    included), then the marker.
    let mut attrs = widget.root.attributes().as_slice().to_vec();
    for attr in from.attributes() {
        if kind != FormWidget::Hidden && is_consumed(attr) {
            continue;
        }
        if matches!(attr, AttributeType::InputType(_)) {
            // ONE `type`, the app's: an image button is built as a submit
            // button, but it is still `type=image` to the engine, CSS and
            // assistive technology.
            attrs.retain(|a| !matches!(a, AttributeType::InputType(_)));
        }
        if !attrs.contains(attr) {
            attrs.push(attr.clone());
        }
    }
    attrs.push(AttributeType::Data(AttributeNameValue {
        attr_name: AzString::from_const_str(REPLACED_MARKER_ATTRIBUTE),
        value: AzString::from_const_str(kind.name()),
    }));
    widget.root.set_attributes(attrs.into());

    // 2. Inline style: the widget's, then the app's (last match wins), then
    //    the declarations of the app's scoped sheets that target the node.
    let mut rules = widget.root.style.rules.clone().into_library_owned_vec();
    rules.extend(from.style.rules.clone().into_library_owned_vec());
    let mut keyframes = widget.root.style.keyframes.clone().into_library_owned_vec();
    keyframes.extend(from.style.keyframes.clone().into_library_owned_vec());
    let mut sheets = widget.css.clone().into_library_owned_vec();
    for sheet in &raw.css {
        let mut rest = Vec::new();
        for rule in &sheet.rules {
            // On a `<form>` a bare `* { .. }` reaches its whole content, as
            // before: it stays a scoped sheet.
            if kind != FormWidget::Form && targets_the_node_itself(rule) {
                // In the node's OWN style, which the cascade reads by its
                // conditions and never selector-matches: `*:hover` becomes
                // `*` under a `:hover` condition, as `Css::parse_inline` does.
                let mut own = rule.clone();
                own.lower_node_pseudo_states();
                rules.push(own);
            } else {
                rest.push(rule.clone());
            }
        }
        if !rest.is_empty() || !sheet.keyframes.as_slice().is_empty() {
            // After the widget's own sheets: the app's rule for a widget
            // part wins over the widget's default for it.
            sheets.push(Css {
                rules: rest.into(),
                keyframes: sheet.keyframes.clone(),
            });
        }
    }
    widget.root.style.rules = rules.into();
    widget.root.style.keyframes = keyframes.into();
    widget.css = sheets.into();

    // 3. Callbacks: the widget's first, then the app's - unless disabled. A
    //    raw form's Submit / Reset handlers are not among them: they became
    //    the Form's `on_submit` / `on_reset` (`form_for`), and left here too
    //    the engine's Submit would run them a second time.
    if !disabled {
        let mut callbacks = widget.root.callbacks.clone().into_library_owned_vec();
        callbacks.extend(
            from.callbacks
                .as_ref()
                .iter()
                .filter(|c| kind != FormWidget::Form || !is_form_event(c.event))
                .cloned(),
        );
        widget.root.callbacks = callbacks.into();
    }

    // 4. Focus order: onto the root when the root is what takes focus.
    if let Some(tab_index) = from.get_tab_index() {
        if !disabled && widget.root.get_tab_index().is_some() {
            widget.root.set_tab_index(tab_index);
        }
    }

    // 5. Identity and the rest of the node's extras.
    if let Some(key) = from.get_key() {
        widget.root.set_key(key);
    }
    if let Some(marker) = from.get_marker() {
        widget.root.set_marker(OptionString::Some(marker.clone()));
    }
    if let Some(menu) = from.get_context_menu() {
        widget.root.set_context_menu(menu.clone());
    }
    if let Some(menu) = from.get_menu_bar() {
        widget.root.set_menu_bar(menu.clone());
    }
    if widget.root.get_dataset().is_none() {
        if let Some(dataset) = from.get_dataset() {
            widget.root.set_dataset(OptionRefAny::Some(dataset.clone()));
        }
    }
    if widget.root.get_component_origin().is_none() {
        if let Some(origin) = from.get_component_origin() {
            widget.root.set_component_origin(origin.clone());
        }
    }
    if let (Some(src), Some(dst)) = (
        from.get_accessibility_info(),
        widget.root.accessibility.as_mut(),
    ) {
        if dst.description.is_none() {
            dst.description = src.description.clone();
        }
        if dst.labelled_by.is_none() {
            dst.labelled_by = src.labelled_by;
        }
        if dst.described_by.is_none() {
            dst.described_by = src.described_by;
        }
    }
}

/// Nothing in `dom` reacts, takes focus or edits.
fn disable(dom: &mut Dom) {
    dom.root.callbacks = CoreCallbackDataVec::from_const_slice(&[]);
    dom.root.flags.set_tab_index(None);
    dom.root.set_contenteditable(false);
    for child in dom.children.as_mut() {
        disable(child);
    }
}

/// Nothing in `dom` edits; focus and selection stay.
fn not_editable(dom: &mut Dom) {
    dom.root.set_contenteditable(false);
    for child in dom.children.as_mut() {
        not_editable(child);
    }
}

#[cfg(test)]
mod tests {
    use azul_core::dom::InputType;

    use super::*;

    fn raw_input(ty: &str) -> Dom {
        Dom::create_input_no_a11y(ty.into(), "f".into(), "F".into())
    }

    #[test]
    fn every_html_input_type_has_a_row_in_the_table() {
        // `datetime` is obsolete (HTML treats it as text), `text` is also
        // the fallback; every other type must be decided in the table.
        for ty in [
            InputType::Text,
            InputType::Button,
            InputType::Checkbox,
            InputType::Color,
            InputType::Date,
            InputType::DatetimeLocal,
            InputType::Email,
            InputType::File,
            InputType::Hidden,
            InputType::Image,
            InputType::Month,
            InputType::Number,
            InputType::Password,
            InputType::Radio,
            InputType::Range,
            InputType::Reset,
            InputType::Search,
            InputType::Submit,
            InputType::Tel,
            InputType::Time,
            InputType::Url,
            InputType::Week,
        ] {
            assert!(
                INPUT_TYPE_WIDGETS.iter().any(|(t, _)| *t == ty.as_str()),
                "no row for type={}",
                ty.as_str()
            );
        }
    }

    #[test]
    fn the_table_has_no_duplicate_rows() {
        let mut seen: Vec<&str> = INPUT_TYPE_WIDGETS.iter().map(|(t, _)| *t).collect();
        let total = seen.len();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), total);
    }

    #[test]
    fn a_dom_without_form_controls_is_left_alone() {
        let mut dom = Dom::create_body().with_child(Dom::create_div());
        let before = dom.clone();
        let n = resolve_form_controls_in_dom(&mut dom, &FormControlMemory::default(), 0);
        assert_eq!(n, 0);
        assert!(dom == before);
    }

    #[test]
    fn the_descendant_count_is_fixed_up_after_a_replacement() {
        let mut dom = Dom::create_body().with_child(Dom::create_div().with_child(raw_input("range")));
        let n = resolve_form_controls_in_dom(&mut dom, &FormControlMemory::default(), 0);
        assert_eq!(n, 1);
        assert_eq!(
            dom.estimated_total_children,
            dom.recompute_estimated_total_children()
        );
    }

    #[test]
    fn resolving_twice_changes_nothing_the_second_time() {
        let memory = FormControlMemory::default();
        let mut dom = Dom::create_body().with_child(raw_input("checkbox"));
        assert_eq!(resolve_form_controls_in_dom(&mut dom, &memory, 0), 1);
        assert_eq!(resolve_form_controls_in_dom(&mut dom, &memory, 0), 0);
    }

    #[test]
    fn the_widget_root_carries_the_replaced_marker() {
        let mut dom = Dom::create_body().with_child(raw_input("range"));
        let _ = resolve_form_controls_in_dom(&mut dom, &FormControlMemory::default(), 0);
        let root = &dom.children.as_slice()[0].root;
        assert_eq!(
            attr_value(root, REPLACED_MARKER_ATTRIBUTE).as_deref(),
            Some("slider")
        );
    }

    #[test]
    fn a_hidden_input_becomes_an_invisible_node_that_keeps_its_value() {
        let mut dom = Dom::create_body().with_child(
            raw_input("hidden").with_attribute(AttributeType::Value("secret".into())),
        );
        let _ = resolve_form_controls_in_dom(&mut dom, &FormControlMemory::default(), 0);
        let root = &dom.children.as_slice()[0].root;
        assert!(matches!(root.get_node_type(), NodeType::Div));
        assert_eq!(attr_value(root, "value").as_deref(), Some("secret"));
        assert_eq!(attr_value(root, "name").as_deref(), Some("f"));
    }

    #[test]
    fn memory_honours_a_value_only_while_the_defaults_are_unchanged() {
        let memory = FormControlMemory::default();
        memory.remember(7, 100, FormValue::Checked(true));
        assert_eq!(memory.recall(7, 100), Some(FormValue::Checked(true)));
        // The app changed the default: its value wins, and the user's is gone.
        assert_eq!(memory.recall(7, 101), None);
        assert_eq!(memory.recall(7, 100), None);
        assert!(memory.is_empty());
    }

    #[test]
    fn memory_forgets_the_least_recently_touched_control_when_full() {
        let memory = FormControlMemory::default();
        for key in 0..=(MAX_REMEMBERED as u64) {
            memory.remember(key, 0, FormValue::Choice(0));
        }
        assert_eq!(memory.len(), MAX_REMEMBERED);
        assert_eq!(memory.recall(0, 0), None, "the oldest went");
        assert!(memory.recall(MAX_REMEMBERED as u64, 0).is_some());
    }

    #[test]
    fn a_range_value_snaps_to_its_step_and_clamps() {
        let step = |s: &str| Some(String::from(s));
        assert_eq!(snap_to_step(5.4, 0.0, 10.0, None), 5.0);
        assert_eq!(snap_to_step(5.4, 0.0, 10.0, step("any").as_ref()), 5.4);
        assert_eq!(snap_to_step(7.0, 0.0, 10.0, step("5").as_ref()), 5.0);
        assert_eq!(snap_to_step(99.0, 0.0, 10.0, None), 10.0);
        assert_eq!(snap_to_step(-3.0, 0.0, 10.0, None), 0.0);
        assert_eq!(snap_to_step(f32::NAN, 0.0, 10.0, None), 0.0);
    }

    #[test]
    fn date_and_time_values_parse_in_every_supported_shape() {
        assert_eq!(parse_date("date", "2024-03-15"), Some((2024, 3, 15)));
        assert_eq!(parse_date("datetime-local", "2024-03-15T10:30"), Some((2024, 3, 15)));
        assert_eq!(parse_date("month", "2024-03"), Some((2024, 3, 1)));
        assert_eq!(parse_date("week", "2024-W01"), Some((2024, 1, 1)));
        assert_eq!(parse_date("week", "2024-W06"), Some((2024, 2, 5)));
        assert_eq!(parse_date("date", "garbage"), None);
        assert_eq!(parse_time("09:05"), Some((9, 5)));
        assert_eq!(parse_time("23:59:30"), Some((23, 59)));
        assert_eq!(parse_time("x"), None);
    }

    #[test]
    fn a_week_value_names_the_monday_of_that_iso_week() {
        // ISO 8601: week 1 is the week holding the year's first Thursday.
        assert_eq!(parse_date("week", "2024-W11"), Some((2024, 3, 11)));
        assert_eq!(parse_date("week", "2021-W01"), Some((2021, 1, 4)));
        assert_eq!(parse_date("week", "2020-W53"), Some((2020, 12, 28)));
        assert_eq!(parse_date("week", "2026-W01"), Some((2025, 12, 29)));
        // 2021 has 52 weeks: there is no week 53 to name.
        assert_eq!(parse_date("week", "2021-W53"), None);
        assert_eq!(parse_date("week", "2021-W00"), None);
    }

    #[test]
    fn month_and_datetime_values_are_checked_for_their_html_shape() {
        assert_eq!(parse_date("month", "2024-12"), Some((2024, 12, 1)));
        assert_eq!(parse_date("month", "2024-13"), None);
        assert_eq!(
            parse_datetime("2024-03-15T10:30"),
            Some(((2024, 3, 15), (10, 30)))
        );
        // HTML also accepts a space for the `T`.
        assert_eq!(
            parse_datetime("2024-03-15 10:30"),
            Some(((2024, 3, 15), (10, 30)))
        );
        assert_eq!(parse_datetime("2024-03-15"), None, "a date alone is not a datetime");
    }

    #[test]
    fn a_selects_default_choice_is_the_last_selected_else_the_first_enabled() {
        let c = |selected, disabled| Choice {
            value: String::new(),
            label: String::new(),
            selected,
            disabled,
        };
        assert_eq!(default_choice(&[c(false, false), c(true, false), c(true, false)]), 2);
        assert_eq!(default_choice(&[c(false, true), c(false, false)]), 1);
        assert_eq!(default_choice(&[]), 0);
    }

    #[test]
    fn a_scope_only_rule_is_told_apart_from_one_reaching_into_the_subtree() {
        let own = Css::parse_inline("width: 10px; :hover { color: red; }");
        assert!(own.rules.iter().all(targets_the_node_itself));
        let deep = Css::from_string(".x p { color: red; }".into());
        assert!(!deep.rules.iter().any(targets_the_node_itself));
    }
}
