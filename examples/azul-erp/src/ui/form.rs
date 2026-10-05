//! A `form` / `form_modal` view as a form: one control per visible field
//! ([`spec::FieldSpec`]: text, text area, a drop-down for a fixed list,
//! azul's ReferencePicker for a record (type to find it), a date picker for a
//! day, azul's MoneyInput for an amount), the problems the last save named,
//! Save and Cancel (the view's `submit` / `cancel` actions). A `form` view
//! is the RecordsShell's form pane; a `form_modal` view a modal over the page.
//!
//! A typed text goes into the draft without a rebuild; a choice rebuilds
//! (a `condition` may show or hide a field: the declining rate).
//!
//! TODO(WIDGETS9B): DateRangePicker where a filter takes days.

use azul::{
    callbacks::{
        DatePickerOnChangeCallbackType, DropDownOnChoiceChangeCallbackType,
        ModalOnCloseCallbackType, MoneyInputOnChangeCallbackType, MoneyInputOnCommitCallbackType,
        ReferencePickerOnEventCallbackType, TextAreaOnTextInputCallbackType,
        TextInputOnTextInputCallbackType,
    },
    prelude::*,
    str::String as AzString,
    vec::StringVec,
    widgets::{
        DatePicker, DatePickerState, DropDown, Modal, ModalState, MoneyInput, MoneyInputState,
        OnTextInputReturn, ReferencePicker, ReferencePickerEvent, ReferencePickerEventKind,
        ReferencePickerFilter, ReferencePickerItem, TextArea, TextAreaState, TextInputState,
        TextInputValid,
    },
};
use chrono::{Datelike, NaiveDate};

use super::{action_button, column, text, with_erp, Erp};
use crate::{
    app::FormDraft,
    ids, model, money,
    views::{
        rows,
        spec::{self, ActionKind, ActionSpec, FieldKind, FieldSpec},
        View, ViewKind,
    },
};

/// The open form's view, when it is of `kind`.
fn open_view<'a>(s: &'a Erp, kind: ViewKind) -> Option<(&'a View, &'a FormDraft)> {
    let draft = s.state.form.as_ref()?;
    let view = s.state.views.view(&draft.view)?;
    (view.kind == kind).then_some((view, draft))
}

/// The form pane of the RecordsShell (an open `form` view).
#[must_use]
pub fn side_pane(s: &Erp, app: &RefAny) -> Option<Dom> {
    let (view, draft) = open_view(s, ViewKind::Form)?;
    Some(form_dom(s, app, view, draft).with_css(
        "display: flex; flex-direction: column; padding: 12px; width: 360px; overflow-y: auto;",
    ))
}

/// The modal of an open `form_modal` view.
#[must_use]
pub fn modal(s: &Erp, app: &RefAny) -> Option<Dom> {
    let (view, draft) = open_view(s, ViewKind::FormModal)?;
    let title = view.title(&s.state.labels, draft.editing);
    Some(
        Modal::create(
            form_dom(s, app, view, draft)
                .with_css("display: flex; flex-direction: column; min-width: 360px;"),
        )
        .with_title(title.as_str())
        .with_open(true)
        .with_on_close(app.clone(), on_modal_close as ModalOnCloseCallbackType)
        .dom()
        .with_id(ids::MODAL),
    )
}

/// The form: the title, a row per visible field, the problems, the buttons.
fn form_dom(s: &Erp, app: &RefAny, view: &View, draft: &FormDraft) -> Dom {
    let title = view.title(&s.state.labels, draft.editing);
    let mut children = vec![text(&title)
        .with_id(ids::FORM_TITLE)
        .with_css("font-size: 15px; font-weight: 600; padding-bottom: 6px;")];
    let fields = spec::fields(&view.fields, &s.state.labels);
    for f in fields.iter().filter(|f| spec::visible(f, &draft.values)) {
        let value = draft.values.get(&f.name).cloned().unwrap_or_default();
        let problem = draft
            .problems
            .iter()
            .find(|(name, _)| *name == f.name)
            .map(|(_, why)| why.as_str());
        children.push(field_row(&f.label, control(s, app, f, &value), problem));
    }
    let general: Vec<&str> = draft
        .problems
        .iter()
        .filter(|(name, _)| name.is_empty() || !fields.iter().any(|f| f.name == *name))
        .map(|(_, why)| why.as_str())
        .collect();
    if !general.is_empty() {
        children.push(
            column(general.iter().map(|why| text(why)).collect())
                .with_id(ids::FORM_PROBLEMS)
                .with_css("padding: 6px 0px; font-size: 12px; color: #c42b1c;"),
        );
    }
    let mut buttons = Dom::create_div().with_css(
        "display: flex; flex-direction: row; justify-content: flex-end; padding-top: 10px;",
    );
    for action in spec::actions(&view.actions, &s.state.labels) {
        let id = match action.kind {
            ActionKind::Submit => ids::FORM_SAVE,
            ActionKind::Cancel => ids::FORM_CANCEL,
            _ => continue,
        };
        buttons.add_child(action_button(app, &action, &draft.params).with_id(id));
    }
    if spec::actions(&view.actions, &s.state.labels).is_empty() {
        let save = ActionSpec {
            label: "Save".to_string(),
            kind: ActionKind::Submit,
            icon: String::new(),
            primary: true,
            condition: None,
        };
        buttons.add_child(action_button(app, &save, &draft.params).with_id(ids::FORM_SAVE));
    }
    children.push(buttons);
    Dom::create_div()
        .with_id(ids::FORM)
        .with_children(DomVec::from_vec(children))
}

/// A labelled field row, with the field's problem under it.
fn field_row(label: &str, control: Dom, problem: Option<&str>) -> Dom {
    let mut row = Dom::create_div()
        .with_class(ids::FIELD_ROW)
        .with_css("display: flex; flex-direction: column; padding: 4px 0px;")
        .with_child(
            text(label)
                .with_class(ids::FIELD_LABEL)
                .with_css("font-size: 12px; opacity: 0.8; padding-bottom: 2px;"),
        )
        .with_child(control);
    if let Some(why) = problem {
        row.add_child(text(why).with_css("font-size: 12px; color: #c42b1c; padding-top: 2px;"));
    }
    row
}

/// What a field's control carries.
struct FieldRef {
    app: RefAny,
    name: String,
}

/// What a drop-down carries: the values its choices stand for.
struct ChoiceRef {
    app: RefAny,
    name: String,
    values: Vec<String>,
}

fn field_ref(app: &RefAny, f: &FieldSpec) -> RefAny {
    RefAny::new(FieldRef {
        app: app.clone(),
        name: f.name.clone(),
    })
}

/// The control of a field holding `value`.
fn control(s: &Erp, app: &RefAny, f: &FieldSpec, value: &str) -> Dom {
    let dom = match &f.kind {
        FieldKind::TextArea => TextArea::create()
            .with_text(value)
            .with_accessibility_name(f.label.as_str())
            .with_on_text_input(
                field_ref(app, f),
                on_area as TextAreaOnTextInputCallbackType,
            )
            .dom(),
        FieldKind::Select(choices) => drop_down(app, f, choices, value),
        FieldKind::Reference(source) => {
            let choices = rows::reference_choices(source, &s.state.book);
            reference_picker(s, app, f, &choices, value)
        }
        FieldKind::Date if f.required => match model::parse_date(value) {
            Some(day) => date_picker(app, f, day),
            None => text_input(app, f, value, "YYYY-MM-DD"),
        },
        FieldKind::Date => text_input(app, f, value, "YYYY-MM-DD"),
        FieldKind::Decimal => money_input(app, f, value),
        FieldKind::Integer => text_input(app, f, value, "0"),
        FieldKind::Text | FieldKind::Password | FieldKind::Switch => text_input(app, f, value, ""),
    };
    dom.with_id(ids::field(&f.name))
}

fn text_input(app: &RefAny, f: &FieldSpec, value: &str, placeholder: &str) -> Dom {
    TextInput::create()
        .with_text(value)
        .with_placeholder(placeholder)
        .with_accessibility_name(f.label.as_str())
        .with_on_text_input(
            field_ref(app, f),
            on_text as TextInputOnTextInputCallbackType,
        )
        .dom()
}

/// An amount: azul's MoneyInput in the register's [`money::currency`] (it
/// refuses a keystroke that can never make an amount; the bounds are the
/// field's, in cents). The draft holds what the record files write
/// (`"1596.64"`).
fn money_input(app: &RefAny, f: &FieldSpec, value: &str) -> Dom {
    let mut input = match money::parse_amount(value) {
        Ok(cents) if !value.trim().is_empty() => MoneyInput::create(cents, money::currency()),
        _ => MoneyInput::create_empty(money::currency()),
    }
    .with_locale(money::locale())
    .with_show_currency(false)
    .with_allow_negative(f.min.map_or(true, |min| min < 0))
    .with_placeholder("0.00")
    .with_accessibility_name(f.label.as_str())
    .with_on_change(field_ref(app, f), on_money as MoneyInputOnChangeCallbackType)
    .with_on_commit(field_ref(app, f), on_money as MoneyInputOnCommitCallbackType);
    if let Some(min) = f.min {
        input = input.with_min(min.saturating_mul(money::MINOR_PER_MAJOR));
    }
    if let Some(max) = f.max {
        input = input.with_max(max.saturating_mul(money::MINOR_PER_MAJOR));
    }
    input.dom()
}

fn drop_down(app: &RefAny, f: &FieldSpec, choices: &[(String, String)], value: &str) -> Dom {
    let labels: Vec<AzString> = choices
        .iter()
        .map(|(_, label)| AzString::from(label.as_str()))
        .collect();
    let selected = choices.iter().position(|(v, _)| v == value).unwrap_or(0);
    DropDown::create(StringVec::from_vec(labels))
        .with_selected(selected)
        .with_accessibility_name(f.label.as_str())
        .with_on_choice_change(
            RefAny::new(ChoiceRef {
                app: app.clone(),
                name: f.name.clone(),
                values: choices.iter().map(|(v, _)| v.clone()).collect(),
            }),
            on_choice as DropDownOnChoiceChangeCallbackType,
        )
        .dom()
}

/// A record (a category, a location): azul's ReferencePicker over every
/// record of the kind, filtered by what is typed. An item's id is its place in
/// `choices` (the `(none)` choice at 0 is not listed: an emptied field is
/// none). The typed text survives a rebuild ([`Erp::reference_query`]).
fn reference_picker(
    s: &Erp,
    app: &RefAny,
    f: &FieldSpec,
    choices: &[(String, String)],
    value: &str,
) -> Dom {
    let items: Vec<ReferencePickerItem> = choices
        .iter()
        .enumerate()
        .filter(|(_, (id, _))| !id.is_empty())
        .map(|(i, (_, label))| ReferencePickerItem::create(i as u64, label.as_str()))
        .collect();
    let mut picker = ReferencePicker::create(items)
        .with_filter(ReferencePickerFilter::Local)
        .with_placeholder(format!("Type to find a {}", f.label.to_lowercase()))
        .with_accessibility_name(f.label.as_str())
        .with_on_event(
            RefAny::new(ChoiceRef {
                app: app.clone(),
                name: f.name.clone(),
                values: choices.iter().map(|(v, _)| v.clone()).collect(),
            }),
            on_reference as ReferencePickerOnEventCallbackType,
        );
    if let Some(i) = choices.iter().position(|(v, _)| !v.is_empty() && v == value) {
        picker = picker.with_selected(i as u64);
    }
    if let Some((field, query)) = &s.reference_query {
        if *field == f.name {
            picker = picker.with_query(query.as_str());
        }
    }
    picker.dom()
}

fn date_picker(app: &RefAny, f: &FieldSpec, day: NaiveDate) -> Dom {
    let year = u32::try_from(day.year()).unwrap_or(1970);
    DatePicker::create(year, day.month(), day.day())
        .with_accessibility_name(f.label.as_str())
        .with_on_change(field_ref(app, f), on_date as DatePickerOnChangeCallbackType)
        .dom()
}

/// Sets field `name` of the open form (no rebuild: the field shows it).
fn set_quietly(app: &mut RefAny, name: &str, value: &str) {
    if let Some(mut s) = app.downcast_mut::<Erp>() {
        s.state.set_value(name, value);
    }
}

fn keep() -> OnTextInputReturn {
    OnTextInputReturn {
        update: Update::DoNothing,
        valid: TextInputValid::Yes,
    }
}

extern "C" fn on_text(
    mut data: RefAny,
    _info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    let Some((mut app, name)) = data
        .downcast_ref::<FieldRef>()
        .map(|f| (f.app.clone(), f.name.clone()))
    else {
        return keep();
    };
    let value = state.get_text().as_str().to_string();
    set_quietly(&mut app, &name, &value);
    keep()
}

extern "C" fn on_area(
    mut data: RefAny,
    _info: CallbackInfo,
    state: TextAreaState,
) -> OnTextInputReturn {
    let Some((mut app, name)) = data
        .downcast_ref::<FieldRef>()
        .map(|f| (f.app.clone(), f.name.clone()))
    else {
        return keep();
    };
    let value = state.get_text().as_str().to_string();
    set_quietly(&mut app, &name, &value);
    keep()
}

/// An amount field changed (a keystroke) or was left (the canonical text): the
/// draft takes the amount, or "" while the field is empty or not an amount
/// yet (the save then says what is missing).
extern "C" fn on_money(mut data: RefAny, _info: CallbackInfo, state: MoneyInputState) -> Update {
    let Some((mut app, name)) = data
        .downcast_ref::<FieldRef>()
        .map(|f| (f.app.clone(), f.name.clone()))
    else {
        return Update::DoNothing;
    };
    let value = state
        .amount
        .into_option()
        .map(money::file_amount)
        .unwrap_or_default();
    set_quietly(&mut app, &name, &value);
    Update::DoNothing
}

extern "C" fn on_choice(mut data: RefAny, mut info: CallbackInfo, choice: usize) -> Update {
    let Some((mut app, name, value)) = data.downcast_ref::<ChoiceRef>().map(|c| {
        (
            c.app.clone(),
            c.name.clone(),
            c.values.get(choice).cloned().unwrap_or_default(),
        )
    }) else {
        return Update::DoNothing;
    };
    with_erp(&mut app, &mut info, |s, _info| {
        s.state.set_value(&name, &value)
    })
}

/// A record field: a pick puts the record's id into the draft; a query is
/// kept for the rebuild (the picker filters by it), and an emptied field is
/// no record - typed empty, or cleared with the picker's x.
extern "C" fn on_reference(
    mut data: RefAny,
    mut info: CallbackInfo,
    event: ReferencePickerEvent,
) -> Update {
    let Some((mut app, name, values)) = data
        .downcast_ref::<ChoiceRef>()
        .map(|c| (c.app.clone(), c.name.clone(), c.values.clone()))
    else {
        return Update::DoNothing;
    };
    let text = event.text.as_str().to_string();
    match event.kind {
        ReferencePickerEventKind::Pick => {
            let value = usize::try_from(event.id)
                .ok()
                .and_then(|i| values.get(i))
                .cloned()
                .unwrap_or_default();
            with_erp(&mut app, &mut info, |s, _info| {
                s.reference_query = None;
                s.state.set_value(&name, &value);
            })
        }
        ReferencePickerEventKind::Query => with_erp(&mut app, &mut info, |s, _info| {
            if text.trim().is_empty() {
                s.state.set_value(&name, "");
            }
            s.reference_query = Some((name.clone(), text.clone()));
        }),
        ReferencePickerEventKind::Clear => with_erp(&mut app, &mut info, |s, _info| {
            s.reference_query = None;
            s.state.set_value(&name, "");
        }),
        _ => Update::DoNothing,
    }
}

extern "C" fn on_date(mut data: RefAny, mut info: CallbackInfo, state: DatePickerState) -> Update {
    let Some((mut app, name)) = data
        .downcast_ref::<FieldRef>()
        .map(|f| (f.app.clone(), f.name.clone()))
    else {
        return Update::DoNothing;
    };
    let year = i32::try_from(state.year).unwrap_or(1970);
    let Some(day) = NaiveDate::from_ymd_opt(year, state.month, state.day) else {
        return Update::DoNothing;
    };
    with_erp(&mut app, &mut info, |s, _info| {
        s.state.set_value(&name, &model::format_date(day));
    })
}

extern "C" fn on_modal_close(
    mut data: RefAny,
    mut info: CallbackInfo,
    _state: ModalState,
) -> Update {
    with_erp(&mut data, &mut info, |s, _info| s.state.cancel_form())
}
