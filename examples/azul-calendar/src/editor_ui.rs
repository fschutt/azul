//! The event editor: a window of its own (Outlook's appointment window), on the same Office
//! scaffold as the main window - its title row, a ribbon (Save & Close, Delete, Delete This
//! Occurrence, Add AzMeet Link, Close) and the form (`editor.rs` holds what it edits): subject,
//! location, attendees, start and end (date and time), all day, repeat (Segmented rows a script
//! can click), every N, ends, reminder, calendar, "Add AzMeet link", notes.
//!
//! One editor window at a time: the window's layout callback reads `CalState::editor`, so two
//! windows would show one form; asking for a second says so in the main window instead. The
//! window's id is `azcalendar-editor`, what a script routes its requests by. Save writes the
//! event (a new AzMeet link is made here, pending: the main window's sync timer registers it),
//! closes the window and rebuilds the main one.

use azul::{
    callbacks::{
        ButtonOnClickCallbackType, CheckBoxOnToggleCallbackType, DatePickerOnChangeCallbackType,
        DropDownOnChoiceChangeCallbackType, SegmentedOnChangeCallbackType,
        TextAreaOnTextInputCallbackType, TextInputOnTextInputCallbackType,
        TimePickerOnChangeCallbackType,
    },
    dom::VirtualKeyCode,
    prelude::*,
    shells::{OfficeShell, ShellPane, ShellPaneKind, ShellThemeAccent, ShellThemeScope},
    str::String as AzString,
    vec::StringVec,
    widgets::{
        ButtonType, CheckBoxState, DatePicker, DatePickerState, DropDown, OnTextInputReturn,
        Ribbon, RibbonButton, RibbonGroup, RibbonItem, RibbonTab, Segmented, SegmentedState,
        TextArea, TextAreaState, TextInputState, TextInputValid, TimePicker, TimePickerState,
        Titlebar,
    },
    window::WindowDecorations,
};
use chrono::{Datelike, NaiveDate, NaiveTime, Timelike};

use crate::{
    editor::{self, EditorForm, Ends, Repeat, REMINDERS},
    event, timegrid, week, CalState, BODY, EDITOR_WINDOW_ID, ERROR, LABEL, PAGE, SECONDARY,
};

/// What the form's "Add AzMeet link" line says once it is ticked.
const WILL_MINT: &str = "A new AzMeet link is made when you save. It works offline too: the \
                         meeting server gets it as soon as it answers.";

// ==== Opening ====

/// Ribbon / menu "New Appointment".
pub(crate) extern "C" fn on_new_appointment(mut data: RefAny, mut info: CallbackInfo) -> Update {
    open_new(&mut data, &mut info, false)
}

/// Ribbon / menu "New Meeting": an appointment with attendees and an AzMeet link.
pub(crate) extern "C" fn on_new_meeting(mut data: RefAny, mut info: CallbackInfo) -> Update {
    open_new(&mut data, &mut info, true)
}

/// A new appointment (or meeting) where `crate::new_slot` puts it.
pub(crate) fn open_new(data: &mut RefAny, info: &mut CallbackInfo, meeting: bool) -> Update {
    let Some(slot) = data
        .downcast_ref::<CalState>()
        .map(|s| crate::new_slot(&s, chrono::Local::now().time()))
    else {
        return Update::DoNothing;
    };
    open_new_at(data, info, slot.0, slot.1, slot.2, meeting)
}

/// A new appointment (or meeting) on `date` from `start` to `end`, in the first calendar shown.
pub(crate) fn open_new_at(
    data: &mut RefAny,
    info: &mut CallbackInfo,
    date: NaiveDate,
    start: NaiveTime,
    end: NaiveTime,
    meeting: bool,
) -> Update {
    let form = {
        let Some(mut s) = data.downcast_mut::<CalState>() else {
            return Update::DoNothing;
        };
        s.editors_opened += 1;
        let calendar = s.calendar_for_new();
        let id = event::new_event_id();
        if meeting {
            EditorForm::new_meeting(s.editors_opened, &id, date, start, end, &calendar)
        } else {
            EditorForm::new_event(s.editors_opened, &id, date, start, end, &calendar)
        }
    };
    open_form(data, info, form, None)
}

/// The event `id`, opened on its occurrence of `date`.
pub(crate) fn open_event(
    data: &mut RefAny,
    info: &mut CallbackInfo,
    id: &str,
    date: NaiveDate,
) -> Update {
    let opened = {
        let Some(mut s) = data.downcast_mut::<CalState>() else {
            return Update::DoNothing;
        };
        s.editors_opened += 1;
        let serial = s.editors_opened;
        s.selected = Some((id.to_string(), date));
        s.events.iter().find(|e| e.id == id).map(|e| {
            let occurrence = e.repeat.is_some().then_some(date);
            (EditorForm::from_event(serial, e), occurrence)
        })
    };
    match opened {
        Some((form, occurrence)) => open_form(data, info, form, occurrence),
        None => Update::RefreshDom,
    }
}

/// Opens the editor window on `form` (`occurrence`: the day of the repeating event's
/// occurrence it was opened on). While one is open, says so instead.
pub(crate) fn open_form(
    data: &mut RefAny,
    info: &mut CallbackInfo,
    form: EditorForm,
    occurrence: Option<NaiveDate>,
) -> Update {
    let Some(mut s) = data.downcast_mut::<CalState>() else {
        return Update::DoNothing;
    };
    if s.editor.is_some() {
        s.notice =
            String::from("An appointment is open in its own window: save or close it first.");
        return Update::RefreshDom;
    }
    let title = form.window_title();
    s.editor = Some(form);
    s.editor_occurrence = occurrence;
    s.draft = None;
    s.notice.clear();
    println!("AZCAL_EDITOR open");
    let mut window = WindowCreateOptions::create(editor_layout);
    window.window_state.size.dimensions = LogicalSize::create(760.0, 820.0);
    window.window_state.title = AzString::from(title.as_str());
    window.window_state.window_id = AzString::from(EDITOR_WINDOW_ID);
    window.window_state.flags.decorations = WindowDecorations::NoTitle;
    info.create_window(window);
    Update::RefreshDom
}

/// The editor is gone (saved, cancelled, deleted, its window closed): the main window shows the
/// calendar as it is now.
fn closed(s: &mut CalState) {
    if s.editor.take().is_some() {
        println!("AZCAL_EDITOR closed");
    }
    s.editor_occurrence = None;
}

// ==== The window ====

extern "C" fn editor_layout(mut data: RefAny, info: LayoutCallbackInfo) -> Dom {
    // Reading the mode makes a light / dark switch rebuild the window.
    let _mode = info.get_mode();
    let app = data.clone();
    let Some(guard) = data.downcast_ref::<CalState>() else {
        return Dom::create_body();
    };
    let s = &*guard;
    let shell = match &s.editor {
        Some(form) => OfficeShell::create()
            .with_title_row(
                Titlebar::create(form.window_title().as_str())
                    .without_border_bottom()
                    .dom(),
            )
            .with_ribbon(ribbon(s, form, &app))
            .with_pane(
                ShellPane::create("editor-form", form_dom(s, form, &app))
                    .with_kind(ShellPaneKind::Main)
                    .with_label(form.window_title().as_str()),
            )
            .dom(),
        None => Dom::create_div()
            .with_css(PAGE)
            .with_child(Dom::create_span_with_text("This appointment is closed.")),
    };
    Dom::create_body()
        .with_css(BODY)
        .with_callback(
            EventFilter::Window(WindowEventFilter::CloseRequested),
            app.clone(),
            on_editor_close_requested,
        )
        .with_callback(
            EventFilter::Window(WindowEventFilter::VirtualKeyDown),
            app.clone(),
            on_editor_key,
        )
        .with_child(
            ShellThemeScope::create(shell)
                .with_accent(ShellThemeAccent::Blue)
                .dom(),
        )
}

/// The editor's ribbon: APPOINTMENT (or MEETING) - Save & Close, Delete, Delete This
/// Occurrence; Add AzMeet Link; Close.
fn ribbon(s: &CalState, form: &EditorForm, app: &RefAny) -> Dom {
    let button = |icon: &str, label: &str, cb: ButtonOnClickCallbackType| {
        RibbonButton::create(icon, label).with_on_click(app.clone(), cb)
    };
    let mut actions = RibbonGroup::create("Actions").with_item(RibbonItem::LargeButton(button(
        "save",
        "Save & Close",
        on_save,
    )));
    if form.existing {
        actions = actions.with_item(RibbonItem::LargeButton(button(
            "delete", "Delete", on_delete,
        )));
        if s.editor_occurrence.is_some() {
            actions = actions.with_item(RibbonItem::LargeButton(button(
                "event_busy",
                "Delete This Occurrence",
                on_delete_occurrence,
            )));
        }
    }
    let meeting = RibbonGroup::create("Meeting").with_item(RibbonItem::LargeButton(
        button("video_call", "Add AzMeet Link", on_meet_label).with_toggled(form.add_meet),
    ));
    let close = RibbonGroup::create("Close")
        .with_item(RibbonItem::LargeButton(button("close", "Close", on_cancel)));
    let tab = if form.meeting_request || !form.attendees.trim().is_empty() {
        "MEETING"
    } else {
        "APPOINTMENT"
    };
    Ribbon::create(vec![RibbonTab::create(tab)
        .with_group(actions)
        .with_group(meeting)
        .with_group(close)])
    .dom_desktop()
}

/// One labelled row of the form: the label column, then the controls.
fn row(label: &str, controls: Vec<Dom>) -> Dom {
    let mut line = Dom::create_div()
        .with_css("display: flex; flex-direction: row; align-items: center; margin-top: 10px;")
        .with_child(Dom::create_span_with_text(label).with_css(format!(
            "width: 96px; flex-shrink: 0; font-size: 13px; {SECONDARY}"
        )));
    for c in controls {
        line.add_child(c);
    }
    line
}

/// A text field of the form, growing to the row's width.
fn field(
    text: &str,
    placeholder: &str,
    name: &str,
    id: &str,
    app: &RefAny,
    cb: TextInputOnTextInputCallbackType,
) -> Dom {
    Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-width: 0;")
        .with_child(
            TextInput::create()
                .with_text(text)
                .with_placeholder(placeholder)
                .with_accessibility_name(name)
                .with_on_text_input(app.clone(), cb)
                .dom()
                .with_id(id),
        )
}

fn date_picker(
    date: NaiveDate,
    name: &str,
    id: &str,
    app: &RefAny,
    cb: DatePickerOnChangeCallbackType,
) -> Dom {
    DatePicker::create(date.year().max(1) as u32, date.month(), date.day())
        .with_accessibility_name(name)
        .with_on_change(app.clone(), cb)
        .dom()
        .with_id(id)
        .with_css("margin-right: 8px;")
}

fn time_picker(
    time: NaiveTime,
    name: &str,
    id: &str,
    app: &RefAny,
    cb: TimePickerOnChangeCallbackType,
) -> Dom {
    TimePicker::create(time.hour(), time.minute())
        .with_24h(true)
        .with_accessibility_name(name)
        .with_on_change(app.clone(), cb)
        .dom()
        .with_id(id)
        .with_css("margin-right: 8px;")
}

fn segmented(
    labels: Vec<String>,
    selected: usize,
    id: &str,
    app: &RefAny,
    cb: SegmentedOnChangeCallbackType,
) -> Dom {
    Segmented::create(StringVec::from(
        labels
            .into_iter()
            .map(AzString::from)
            .collect::<Vec<AzString>>(),
    ))
    .with_selected_index(selected)
    .with_on_change(app.clone(), cb)
    .dom()
    .with_id(id)
}

fn drop_down(
    labels: Vec<String>,
    selected: usize,
    name: &str,
    id: &str,
    app: &RefAny,
    cb: DropDownOnChoiceChangeCallbackType,
) -> Dom {
    DropDown::create(StringVec::from(
        labels
            .into_iter()
            .map(AzString::from)
            .collect::<Vec<AzString>>(),
    ))
    .with_selected(selected)
    .with_accessibility_name(name)
    .with_on_choice_change(app.clone(), cb)
    .dom()
    .with_id(id)
}

fn check(
    checked: bool,
    label: &str,
    id: &str,
    app: &RefAny,
    cb: CheckBoxOnToggleCallbackType,
) -> Dom {
    Dom::create_div()
        .with_css("display: flex; flex-direction: row; align-items: center; margin-right: 12px;")
        .with_child(
            CheckBox::create(checked)
                .with_accessibility_name(label)
                .with_on_toggle(app.clone(), cb)
                .dom()
                .with_id(id),
        )
        .with_child(Dom::create_span_with_text(label).with_css("margin-left: 6px;"))
}

/// The form (`#editor-form` holds it): every field of `editor.rs`'s form, the error line and
/// the buttons.
fn form_dom(s: &CalState, form: &EditorForm, app: &RefAny) -> Dom {
    let mut page = Dom::create_div().with_css(PAGE).with_child(row(
        "Subject",
        vec![field(
            &form.title,
            "Add a title",
            "Subject",
            "editor-title",
            app,
            on_title,
        )],
    ));
    let attendees = || {
        row(
            "Attendees",
            vec![field(
                &form.attendees,
                "ana@example.com, bo@example.org",
                "Attendees",
                "editor-attendees",
                app,
                on_attendees,
            )],
        )
    };
    let location = || {
        row(
            "Location",
            vec![field(
                &form.location,
                "Where?",
                "Location",
                "editor-location",
                app,
                on_location,
            )],
        )
    };
    // A meeting asks who first, an appointment where.
    if form.meeting_request {
        page.add_child(attendees());
        page.add_child(location());
    } else {
        page.add_child(location());
        page.add_child(attendees());
    }
    let mut start = vec![date_picker(
        form.date,
        "Start date",
        "editor-start-date",
        app,
        on_start_date,
    )];
    if !form.all_day {
        start.push(time_picker(
            form.start,
            "Start time",
            "editor-start-time",
            app,
            on_start_time,
        ));
    }
    start.push(check(
        form.all_day,
        "All day",
        "editor-all-day",
        app,
        on_all_day,
    ));
    page.add_child(row("Start", start));
    let mut end = Vec::new();
    if form.all_day {
        end.push(date_picker(
            form.last_day,
            "End date",
            "editor-end-date",
            app,
            on_end_date,
        ));
    } else {
        end.push(time_picker(
            form.end,
            "End time",
            "editor-end-time",
            app,
            on_end_time,
        ));
    }
    page.add_child(row("End", end));

    // Repeat: the five (six) segments, the weekly / monthly variant, every N, the end.
    let segments = editor::repeat_segments(form.custom.is_some())
        .into_iter()
        .map(String::from)
        .collect();
    page.add_child(row(
        "Repeat",
        vec![segmented(
            segments,
            editor::repeat_segment(form.repeat),
            "editor-repeat",
            app,
            on_repeat,
        )],
    ));
    let variants = editor::repeat_variants(form.repeat, form.date);
    if !variants.is_empty() {
        let selected = variants
            .iter()
            .position(|(r, _)| *r == form.repeat)
            .unwrap_or(0);
        let labels = variants.into_iter().map(|(_, l)| l).collect();
        page.add_child(row(
            "",
            vec![segmented(
                labels,
                selected,
                "editor-repeat-variant",
                app,
                on_repeat_variant,
            )],
        ));
    }
    if form.repeat == Repeat::Custom {
        page.add_child(row(
            "",
            vec![Dom::create_span_with_text(editor::repeat_label(
                Repeat::Custom,
                form.date,
                form.custom.as_ref(),
            ))],
        ));
    }
    if !matches!(form.repeat, Repeat::Never | Repeat::Custom) {
        page.add_child(row(
            "Every",
            vec![
                Dom::create_div()
                    .with_css("width: 64px; margin-right: 8px;")
                    .with_child(
                        TextInput::create()
                            .with_text(form.interval.to_string().as_str())
                            .with_accessibility_name("Repeat every")
                            .with_on_text_input(app.clone(), on_interval)
                            .dom()
                            .with_id("editor-interval"),
                    ),
                Dom::create_span_with_text(editor::interval_unit(form.repeat)),
            ],
        ));
        let ends: Vec<String> = Ends::CHOICES
            .iter()
            .map(|e| e.label().to_string())
            .collect();
        let selected = Ends::CHOICES
            .iter()
            .position(|e| *e == form.ends)
            .unwrap_or(0);
        let mut ends_row = vec![segmented(ends, selected, "editor-ends", app, on_ends)];
        match form.ends {
            Ends::Never => {}
            Ends::After => ends_row.push(
                Dom::create_div()
                    .with_css("width: 64px; margin-left: 8px; margin-right: 8px;")
                    .with_child(
                        TextInput::create()
                            .with_text(form.count.to_string().as_str())
                            .with_accessibility_name("Number of times")
                            .with_on_text_input(app.clone(), on_count)
                            .dom()
                            .with_id("editor-count"),
                    ),
            ),
            Ends::On => ends_row.push(date_picker(
                form.until,
                "Last date",
                "editor-until",
                app,
                on_until,
            )),
        }
        page.add_child(row("Ends", ends_row));
    }

    let reminders: Vec<String> = REMINDERS.iter().map(|(_, l)| l.to_string()).collect();
    page.add_child(row(
        "Reminder",
        vec![drop_down(
            reminders,
            editor::reminder_index(form.reminder),
            "Reminder",
            "editor-reminder",
            app,
            on_reminder,
        )],
    ));
    let calendars: Vec<String> = s.calendars.iter().map(|c| c.name.clone()).collect();
    let calendar = s
        .calendars
        .iter()
        .position(|c| c.id == form.calendar)
        .unwrap_or(0);
    page.add_child(row(
        "Calendar",
        vec![drop_down(
            calendars,
            calendar,
            "Calendar",
            "editor-calendar",
            app,
            on_calendar,
        )],
    ));
    let mut meet = vec![check(
        form.add_meet,
        "Add AzMeet link",
        "editor-meet",
        app,
        on_meet,
    )];
    if form.add_meet {
        let text = match &form.meeting {
            Some(m) if m.pending => format!("{} (waits for the meeting server)", m.link),
            Some(m) => m.link.clone(),
            None => String::from(WILL_MINT),
        };
        meet.push(
            Dom::create_span_with_text(text).with_css(format!("font-size: 12px; {SECONDARY}")),
        );
    }
    page.add_child(row("AzMeet", meet));
    page.add_child(Dom::create_span_with_text("Notes").with_css(LABEL));
    page.add_child(
        TextArea::create()
            .with_text(form.notes.as_str())
            .with_placeholder("Notes")
            .with_accessibility_name("Notes")
            .with_on_text_input(app.clone(), on_notes as TextAreaOnTextInputCallbackType)
            .dom()
            .with_id("editor-notes")
            .with_css("min-height: 120px;"),
    );
    if !form.error.is_empty() {
        page.add_child(
            Dom::create_span_with_text(form.error.as_str())
                .with_id("editor-error")
                .with_css(ERROR),
        );
    }
    let mut buttons = Dom::create_div()
        .with_css("display: flex; flex-direction: row; align-items: center; margin-top: 20px;");
    if form.existing {
        buttons.add_child(
            Button::with_type("Delete", ButtonType::Danger)
                .with_on_click(app.clone(), on_delete)
                .dom()
                .with_id("editor-delete"),
        );
    }
    buttons.add_child(Dom::create_div().with_css("flex-grow: 1;"));
    buttons.add_child(
        Button::create("Cancel")
            .with_on_click(app.clone(), on_cancel)
            .dom()
            .with_id("editor-cancel")
            .with_css("margin-right: 8px;"),
    );
    buttons.add_child(
        Button::with_type("Save & Close", ButtonType::Primary)
            .with_on_click(app.clone(), on_save)
            .dom()
            .with_id("editor-save"),
    );
    page.with_child(buttons)
}

// ==== The form's callbacks ====

/// Runs `edit` on the open form; it answers whether the window is rebuilt.
fn with_form(data: &mut RefAny, edit: impl FnOnce(&mut EditorForm) -> bool) -> Update {
    let Some(mut s) = data.downcast_mut::<CalState>() else {
        return Update::DoNothing;
    };
    let Some(form) = s.editor.as_mut() else {
        return Update::DoNothing;
    };
    form.error.clear();
    if edit(form) {
        Update::RefreshDom
    } else {
        Update::DoNothing
    }
}

/// What a text field's callback answers: the text is kept, nothing is rebuilt.
fn typed() -> OnTextInputReturn {
    OnTextInputReturn {
        update: Update::DoNothing,
        valid: TextInputValid::Yes,
    }
}

extern "C" fn on_title(
    mut data: RefAny,
    _info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    let text = state.get_text().as_str().to_string();
    with_form(&mut data, |f| {
        f.title = text;
        false
    });
    typed()
}

extern "C" fn on_location(
    mut data: RefAny,
    _info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    let text = state.get_text().as_str().to_string();
    with_form(&mut data, |f| {
        f.location = text;
        false
    });
    typed()
}

extern "C" fn on_attendees(
    mut data: RefAny,
    _info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    let text = state.get_text().as_str().to_string();
    with_form(&mut data, |f| {
        f.attendees = text;
        false
    });
    typed()
}

extern "C" fn on_notes(
    mut data: RefAny,
    _info: CallbackInfo,
    state: TextAreaState,
) -> OnTextInputReturn {
    // A text area's buffer is one u32 per character.
    let text: String = state
        .text
        .as_slice()
        .iter()
        .filter_map(|c| char::from_u32(*c))
        .collect();
    with_form(&mut data, |f| {
        f.notes = text;
        false
    });
    typed()
}

extern "C" fn on_interval(
    mut data: RefAny,
    _info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    let n = editor::parse_count(state.get_text().as_str());
    with_form(&mut data, |f| {
        if let Some(n) = n {
            f.interval = n;
        }
        false
    });
    typed()
}

extern "C" fn on_count(
    mut data: RefAny,
    _info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    let n = editor::parse_count(state.get_text().as_str());
    with_form(&mut data, |f| {
        if let Some(n) = n {
            f.count = n;
        }
        false
    });
    typed()
}

/// The date a date picker reports (its day held to the month's length).
fn picked(state: DatePickerState) -> Option<NaiveDate> {
    week::picked_date(i32::try_from(state.year).ok()?, state.month, state.day)
}

/// The start date: the end date (an all-day event's) and a repeat's last date move along; the
/// repeat's labels name the new day.
extern "C" fn on_start_date(
    mut data: RefAny,
    _info: CallbackInfo,
    state: DatePickerState,
) -> Update {
    let Some(date) = picked(state) else {
        return Update::DoNothing;
    };
    with_form(&mut data, |f| {
        f.set_date(date);
        true
    })
}

extern "C" fn on_end_date(mut data: RefAny, _info: CallbackInfo, state: DatePickerState) -> Update {
    let Some(date) = picked(state) else {
        return Update::DoNothing;
    };
    with_form(&mut data, |f| {
        let turned = (date.year(), date.month()) != (f.last_day.year(), f.last_day.month());
        f.set_last_day(date);
        turned
    })
}

extern "C" fn on_until(mut data: RefAny, _info: CallbackInfo, state: DatePickerState) -> Update {
    let Some(date) = picked(state) else {
        return Update::DoNothing;
    };
    with_form(&mut data, |f| {
        let turned = (date.year(), date.month()) != (f.until.year(), f.until.month());
        f.until = date;
        turned
    })
}

/// The time a time picker reports.
fn time_of(state: TimePickerState) -> Option<NaiveTime> {
    let hour = if state.is_24h {
        state.hour
    } else {
        state.hour % 12 + if state.is_pm { 12 } else { 0 }
    };
    NaiveTime::from_hms_opt(hour, state.minute, 0)
}

/// The start time: the end moves with it, keeping the event's length (held to the day).
extern "C" fn on_start_time(
    mut data: RefAny,
    _info: CallbackInfo,
    state: TimePickerState,
) -> Update {
    let Some(start) = time_of(state) else {
        return Update::DoNothing;
    };
    with_form(&mut data, |f| {
        let length = week::minute_of_day(f.end).saturating_sub(week::minute_of_day(f.start));
        f.start = start;
        f.end = week::time_of_minute(week::minute_of_day(start) + length.max(15));
        true
    })
}

extern "C" fn on_end_time(mut data: RefAny, _info: CallbackInfo, state: TimePickerState) -> Update {
    let Some(end) = time_of(state) else {
        return Update::DoNothing;
    };
    with_form(&mut data, |f| {
        f.end = end;
        false
    })
}

extern "C" fn on_all_day(mut data: RefAny, _info: CallbackInfo, state: CheckBoxState) -> Update {
    with_form(&mut data, |f| {
        f.set_all_day(state.checked);
        true
    })
}

extern "C" fn on_repeat(mut data: RefAny, _info: CallbackInfo, state: SegmentedState) -> Update {
    with_form(&mut data, |f| {
        f.repeat = editor::repeat_of_segment(state.selected_index, f.repeat);
        if f.repeat == Repeat::Custom && f.custom.is_none() {
            f.repeat = Repeat::Never;
        }
        true
    })
}

extern "C" fn on_repeat_variant(
    mut data: RefAny,
    _info: CallbackInfo,
    state: SegmentedState,
) -> Update {
    with_form(&mut data, |f| {
        if let Some((repeat, _)) = editor::repeat_variants(f.repeat, f.date)
            .into_iter()
            .nth(state.selected_index)
        {
            f.repeat = repeat;
        }
        true
    })
}

extern "C" fn on_ends(mut data: RefAny, _info: CallbackInfo, state: SegmentedState) -> Update {
    with_form(&mut data, |f| {
        if let Some(ends) = Ends::CHOICES.get(state.selected_index) {
            f.ends = *ends;
        }
        true
    })
}

extern "C" fn on_reminder(mut data: RefAny, _info: CallbackInfo, index: usize) -> Update {
    with_form(&mut data, |f| {
        if let Some((minutes, _)) = REMINDERS.get(index) {
            f.reminder = *minutes;
        }
        false
    })
}

extern "C" fn on_calendar(mut data: RefAny, _info: CallbackInfo, index: usize) -> Update {
    let Some(mut guard) = data.downcast_mut::<CalState>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    let Some(id) = s.calendars.get(index).map(|c| c.id.clone()) else {
        return Update::DoNothing;
    };
    if let Some(form) = s.editor.as_mut() {
        form.calendar = id;
    }
    Update::DoNothing
}

extern "C" fn on_meet(mut data: RefAny, _info: CallbackInfo, state: CheckBoxState) -> Update {
    with_form(&mut data, |f| {
        f.add_meet = state.checked;
        true
    })
}

/// The ribbon's "Add AzMeet Link": ticks or clears it.
extern "C" fn on_meet_label(mut data: RefAny, _info: CallbackInfo) -> Update {
    with_form(&mut data, |f| {
        f.add_meet = !f.add_meet;
        true
    })
}

// ==== Save, delete, close ====

/// Save & Close (Ctrl / Cmd + S): writes the event (with "Add AzMeet link", a link made here,
/// pending until the main window's sync registers it), closes the window and shows the event
/// in the main one.
extern "C" fn on_save(mut data: RefAny, mut info: CallbackInfo) -> Update {
    save(&mut data, &mut info)
}

fn save(data: &mut RefAny, info: &mut CallbackInfo) -> Update {
    let Some(mut guard) = data.downcast_mut::<CalState>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    let server = s.server.clone();
    let Some(form) = s.editor.as_mut() else {
        return Update::DoNothing;
    };
    form.error.clear();
    let meeting = if form.add_meet {
        Some(
            form.meeting
                .get_or_insert_with(|| crate::new_meeting(&server))
                .clone(),
        )
    } else {
        None
    };
    let event = match form.event(meeting) {
        Ok(event) => event,
        Err(message) => {
            eprintln!("[azcalendar] cannot save: {message}");
            form.error = message;
            return Update::RefreshDom;
        }
    };
    let (date, start, title) = (event.date, event.start, event.title.clone());
    match s.store_event(event) {
        Ok(_) => {
            s.notice = format!("Saved \"{title}\".");
            closed(s);
            timegrid::reveal(s, info, date, start);
            info.close_window();
            Update::RefreshDomAllWindows
        }
        Err(message) => {
            eprintln!("[azcalendar] {message}");
            if let Some(form) = s.editor.as_mut() {
                form.error = message;
            }
            Update::RefreshDom
        }
    }
}

/// Cancel / Close: the window goes, nothing is saved.
extern "C" fn on_cancel(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(mut s) = data.downcast_mut::<CalState>() else {
        return Update::DoNothing;
    };
    closed(&mut s);
    info.close_window();
    Update::RefreshDomAllWindows
}

/// Delete: the event's file goes (every occurrence of a repeating one).
extern "C" fn on_delete(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(mut guard) = data.downcast_mut::<CalState>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    let Some((id, title, existing)) = s
        .editor
        .as_ref()
        .map(|f| (f.id.clone(), f.title.clone(), f.existing))
    else {
        return Update::DoNothing;
    };
    if existing {
        if let Err(e) = event::remove(&s.data_dir, &id) {
            if let Some(form) = s.editor.as_mut() {
                form.error = format!("Could not delete the event: {e}");
            }
            return Update::RefreshDom;
        }
        s.events.retain(|e| e.id != id);
        s.selected = None;
        println!("AZCAL_DELETED {id}");
        s.notice = format!("Deleted \"{title}\".");
    }
    closed(s);
    info.close_window();
    Update::RefreshDomAllWindows
}

/// Delete This Occurrence: the repeating event skips the day it was opened on.
extern "C" fn on_delete_occurrence(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(mut guard) = data.downcast_mut::<CalState>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    let (Some(day), Some(id)) = (s.editor_occurrence, s.editor.as_ref().map(|f| f.id.clone()))
    else {
        return Update::DoNothing;
    };
    let Some(mut event) = s.events.iter().find(|e| e.id == id).cloned() else {
        return Update::DoNothing;
    };
    event.except.push(day);
    let stored = event
        .check()
        .map_err(|e| editor::error_text(&e))
        .and_then(|event| s.store_event(event));
    match stored {
        Ok(_) => {
            s.selected = None;
            s.notice = format!("Removed the occurrence of {}.", day.format("%A %-d %B"));
            closed(s);
            info.close_window();
            Update::RefreshDomAllWindows
        }
        Err(message) => {
            if let Some(form) = s.editor.as_mut() {
                form.error = message;
            }
            Update::RefreshDom
        }
    }
}

/// The window is closed by its close button (or the system): the form goes unsaved.
extern "C" fn on_editor_close_requested(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some(mut s) = data.downcast_mut::<CalState>() else {
        return Update::DoNothing;
    };
    if s.editor.is_none() {
        return Update::DoNothing;
    }
    closed(&mut s);
    Update::RefreshDomAllWindows
}

/// Ctrl / Cmd + S (or + Enter) saves and closes.
extern "C" fn on_editor_key(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let key = info
        .get_current_keyboard_state()
        .current_virtual_keycode
        .into_option();
    let m = info.get_key_modifiers();
    if (m.ctrl || m.meta)
        && matches!(
            key,
            Some(VirtualKeyCode::S | VirtualKeyCode::Return | VirtualKeyCode::NumpadEnter)
        )
    {
        info.prevent_default();
        return save(&mut data, &mut info);
    }
    Update::DoNothing
}
