//! The reading pane: the selected task's details (title, steps, due date and time, repeat,
//! reminder, priority, flag, list, tags, notes, attachments, created / completed), or the
//! selection's bulk commands, or a list's settings, or an empty state.
//!
//! Text is typed into drafts (`Tasks::drafts`) and written into the task on Enter, on
//! leaving the field and when the selection changes; every other control writes at once.
//! Element ids for scripts: `#detail`, `#detail-title`, `#detail-notes`, `#add-step`,
//! `#add-tag`, `#detail-repeat`, `#detail-reminder`, `#detail-list`, `#detail-priority`,
//! `#detail-delete`, `#attach`.

use std::path::PathBuf;

use azul::{
    callbacks::{
        ButtonOnClickCallbackType, CheckBoxOnToggleCallbackType, ChipOnClickCallbackType,
        ChipOnRemoveCallbackType,
        DatePickerOnChangeCallbackType, DropDownOnChoiceChangeCallbackType,
        DateRepeatPickerOnChangeCallbackType, SegmentedOnChangeCallbackType,
        SwitchOnToggleCallbackType, TextAreaOnFocusLostCallbackType,
        TextAreaOnTextInputCallbackType, TextInputOnFocusLostCallbackType,
        TextInputOnTextInputCallbackType, TextInputOnVirtualKeyDownCallbackType,
        TimePickerOnChangeCallbackType,
    },
    dialog::{FileDialog, FileOpenResult},
    dom::VirtualKeyCode,
    option::OptionFileTypeList,
    prelude::*,
    shells::ShellEmptyState,
    str::String as AzString,
    vec::StringVec,
    widgets::{
        ButtonType, CheckBoxState, Chip, ChipState, DatePicker, DatePickerState, DropDown,
        OnTextInputReturn, DateRepeatPicker, DateRepeatRule, Segmented, SegmentedState, Switch,
        SwitchState, TextArea, TextAreaState, TextInputState, TextInputValid, TimePicker,
        TimePickerState,
    },
};
use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, NaiveTime, Timelike, Weekday};

use crate::{
    ids,
    model::{self, Priority, Reminder, Subtask, Task},
    recur::{Repeat, Unit},
    reminders::{self, Preset},
    repeat_form,
    state::{self, Tasks},
    views,
};

// ==== Styles ====

const PANE: &str = "display: flex; flex-direction: column; flex-grow: 1; min-height: 0px; \
                    overflow-y: auto; padding: 12px 16px; gap: 10px;";
const TITLE_ROW: &str = "display: flex; flex-direction: row; align-items: center; gap: 8px;";
const FIELD: &str = "display: flex; flex-direction: row; align-items: center; gap: 8px; \
                     flex-wrap: wrap;";
const LABEL: &str = "font-size: 12px; color: system:secondary-text; min-width: 72px;";
const GROUP_TITLE: &str = "font-size: 11px; font-weight: bold; letter-spacing: 0.5px; color: \
                           system:secondary-text; margin-top: 6px;";
const STEP_ROW: &str = "display: flex; flex-direction: row; align-items: center; gap: 8px; \
                        padding: 2px 0px;";
const STEP_DONE: &str = "color: system:secondary-text; text-decoration: line-through; \
                         flex-grow: 1;";
const STEP: &str = "flex-grow: 1;";
const META: &str = "font-size: 11px; color: system:secondary-text;";
const RULE: &str = "height: 1px; background: system:separator; margin: 4px 0px;";

/// The repeat control's presets; the last opens the editor.
pub const REPEATS: [&str; 8] = [
    "Never",
    "Daily",
    "Weekdays",
    "Weekly",
    "Every 2 weeks",
    "Monthly",
    "Yearly",
    "Custom...",
];

/// Which preset a rule is (`REPEATS.len() - 1` for any other rule).
#[must_use]
pub fn repeat_preset(rule: Option<&Repeat>) -> usize {
    let Some(r) = rule else {
        return 0;
    };
    let plain_week = r.weekdays.len() <= 1;
    match (r.unit, r.every, r.from_completion) {
        (_, _, true) => 7,
        (Unit::Day, 1, _) => 1,
        (Unit::Week, 1, _) if r.is_weekdays() => 2,
        (Unit::Week, 1, _) if plain_week => 3,
        (Unit::Week, 2, _) if r.weekdays.is_empty() => 4,
        (Unit::Month, 1, _) => 5,
        (Unit::Year, 1, _) => 6,
        _ => 7,
    }
}

/// The rule preset `index` sets on a task due on `due`.
#[must_use]
pub fn preset_repeat(index: usize, due: NaiveDate) -> Option<Repeat> {
    let rule = match index {
        1 => Repeat::daily(),
        2 => Repeat::weekdays(),
        3 => Repeat::weekly().on_weekdays(&[due.weekday()]),
        4 => Repeat::new(2, Unit::Week),
        5 => Repeat::monthly(),
        6 => Repeat::yearly(),
        _ => return None,
    };
    Some(rule.anchored(due))
}

fn strings(items: &[&str]) -> StringVec {
    StringVec::from(items.iter().map(|s| AzString::from(*s)).collect::<Vec<_>>())
}

fn owned(items: Vec<String>) -> StringVec {
    StringVec::from(items.into_iter().map(AzString::from).collect::<Vec<_>>())
}

fn label(text: &str) -> Dom {
    Dom::create_span_with_text(text).with_css(LABEL)
}

fn field(name: &str, control: Dom) -> Dom {
    Dom::create_div().with_css(FIELD).with_child(label(name)).with_child(control)
}

/// What a detail control carries: the app, the task, and a number (a step, a weekday).
struct DetailRef {
    app: RefAny,
    task: String,
    n: usize,
}

fn detail_ref(app: &RefAny, task: &str, n: usize) -> RefAny {
    RefAny::new(DetailRef {
        app: app.clone(),
        task: task.to_string(),
        n,
    })
}

/// Runs `f` on task `task` (index) with the control's number; saves the task, pumps the
/// queue and rebuilds.
fn with_task(
    data: &mut RefAny,
    info: &mut CallbackInfo,
    f: impl FnOnce(&mut CallbackInfo, &RefAny, &mut Tasks, usize, usize),
) -> Update {
    let Some((mut app, task, n)) = data
        .downcast_ref::<DetailRef>()
        .map(|r| (r.app.clone(), r.task.clone(), r.n))
    else {
        return Update::DoNothing;
    };
    crate::with_tasks(&mut app, info, |info, app, s| {
        if let Some(i) = s.index_of(&task) {
            f(info, app, s, i, n);
            if s.tasks.get(i).is_some_and(|t| t.id == task) {
                s.save_task(i);
            }
        }
    })
}

// ==== The pane ====

/// The reading pane at `now`.
pub fn pane(s: &Tasks, app: &RefAny, now: NaiveDateTime) -> Dom {
    let body = if let Some(list) = &s.editing_list {
        crate::listedit::pane(s, app, list)
    } else if s.selection.len() > 1 {
        crate::listedit::bulk(s, app)
    } else if let Some(i) = s.selected_one() {
        task_pane(s, app, &s.tasks[i], now)
    } else {
        ShellEmptyState::create("No task selected")
            .with_icon("checklist")
            .with_detail("Select a task to see its steps, dates, repeat, reminder, tags, notes and files.")
            .dom()
    };
    Dom::create_div()
        .with_id(ids::DETAIL)
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;")
        .with_child(body)
        .with_callback(
            EventFilter::Window(WindowEventFilter::DroppedFile),
            app.clone(),
            on_file_dropped,
        )
}

fn task_pane(s: &Tasks, app: &RefAny, t: &Task, now: NaiveDateTime) -> Dom {
    let today = now.date();
    let title = if s.drafts.task == t.id {
        s.drafts.title.as_str()
    } else {
        t.title.as_str()
    };
    let notes = if s.drafts.task == t.id {
        s.drafts.notes.as_str()
    } else {
        t.notes.as_str()
    };
    let mut pane = Dom::create_div().with_css(PANE);

    // Title, completion, flag.
    pane.add_child(
        Dom::create_div()
            .with_css(TITLE_ROW)
            .with_child(
                CheckBox::create(t.is_done())
                    .with_accessibility_name("Completed")
                    .with_on_toggle(detail_ref(app, &t.id, 0), on_done as CheckBoxOnToggleCallbackType)
                    .dom()
                    .with_id(ids::DETAIL_DONE),
            )
            .with_child(
                TextInput::create()
                    .with_text(title)
                    .with_placeholder("Title")
                    .with_accessibility_name("Title")
                    .with_on_text_input(app.clone(), on_title_text as TextInputOnTextInputCallbackType)
                    .with_on_virtual_key_down(app.clone(), on_title_key as TextInputOnVirtualKeyDownCallbackType)
                    .with_on_focus_lost(app.clone(), on_title_blur as TextInputOnFocusLostCallbackType)
                    .dom()
                    .with_id(ids::DETAIL_TITLE)
                    .with_css("flex-grow: 1; font-size: 16px;"),
            ),
    );
    pane.add_child(field(
        "Priority",
        Segmented::create(strings(&["None", "Low", "Medium", "High"]))
            .with_selected_index(t.priority.index())
            .with_on_change(detail_ref(app, &t.id, 0), on_priority as SegmentedOnChangeCallbackType)
            .dom()
            .with_id(ids::DETAIL_PRIORITY),
    ));
    pane.add_child(field(
        "Flagged",
        Switch::create(t.flagged)
            .with_accessibility_name("Flagged")
            .with_on_toggle(detail_ref(app, &t.id, 0), on_flag as SwitchOnToggleCallbackType)
            .dom()
            .with_id(ids::DETAIL_FLAG),
    ));

    pane.add_child(steps(app, t));
    pane.add_child(Dom::create_div().with_css(RULE));
    pane.add_child(due(s, app, t, today));
    pane.add_child(repeat(s, app, t, today));
    pane.add_child(reminder(s, app, t, today));
    pane.add_child(list_field(s, app, t));
    pane.add_child(tags(s, app, t));
    pane.add_child(Dom::create_div().with_css(RULE));
    pane.add_child(Dom::create_span_with_text("NOTES").with_css(GROUP_TITLE));
    pane.add_child(
        TextArea::create()
            .with_text(notes)
            .with_placeholder("Notes")
            .with_accessibility_name("Notes")
            .with_on_text_input(app.clone(), on_notes_text as TextAreaOnTextInputCallbackType)
            .with_on_focus_lost(app.clone(), on_notes_blur as TextAreaOnFocusLostCallbackType)
            .dom()
            .with_id(ids::DETAIL_NOTES)
            .with_css("min-height: 96px;"),
    );
    pane.add_child(attachments(app, t));
    pane.add_child(Dom::create_div().with_css(RULE));
    pane.add_child(footer(app, t, today));
    pane
}

/// "STEPS 2 of 4", each step's box, title and remove button, then "Add a step".
fn steps(app: &RefAny, t: &Task) -> Dom {
    let heading = match t.subtask_progress() {
        Some((done, total)) => format!("STEPS \u{b7} {done} OF {total}"),
        None => "STEPS".to_string(),
    };
    let mut out = Dom::create_div()
        .with_id(ids::STEPS)
        .with_css("display: flex; flex-direction: column;")
        .with_child(Dom::create_span_with_text(heading).with_css(GROUP_TITLE));
    for (n, step) in t.subtasks.iter().enumerate() {
        out.add_child(
            Dom::create_div()
                .with_css(STEP_ROW)
                .with_child(
                    CheckBox::create(step.done)
                        .with_accessibility_name(format!("Done: {}", step.title))
                        .with_on_toggle(detail_ref(app, &t.id, n), on_step_done as CheckBoxOnToggleCallbackType)
                        .dom()
                        .with_id(ids::step(n)),
                )
                .with_child(
                    Dom::create_span_with_text(step.title.as_str())
                        .with_css(if step.done { STEP_DONE } else { STEP }),
                )
                .with_child(
                    Button::create("")
                        .with_icon("close")
                        .with_on_click(detail_ref(app, &t.id, n), on_step_remove as ButtonOnClickCallbackType)
                        .dom()
                        .with_accessibility_name(format!("Remove the step {}", step.title)),
                ),
        );
    }
    out.with_child(
        TextInput::create()
            .with_placeholder("Add a step")
            .with_accessibility_name("Add a step")
            .with_on_text_input(app.clone(), on_step_text as TextInputOnTextInputCallbackType)
            .with_on_virtual_key_down(app.clone(), on_step_key as TextInputOnVirtualKeyDownCallbackType)
            .dom()
            .with_id(ids::ADD_STEP),
    )
}

/// The due date: quick days when there is none; the date, the time (or "Add time") and
/// "Clear" when there is one.
fn due(s: &Tasks, app: &RefAny, t: &Task, today: NaiveDate) -> Dom {
    let mut row = Dom::create_div().with_css(FIELD).with_child(label("Due"));
    match t.due {
        None => {
            for (n, name) in ["Today", "Tomorrow", "Next week"].iter().enumerate() {
                row.add_child(
                    Button::create(*name)
                        .with_on_click(detail_ref(app, &t.id, n), on_due_quick as ButtonOnClickCallbackType)
                        .dom()
                        .with_id(ids::due_quick(n)),
                );
            }
        }
        Some(date) => {
            row.add_child(
                DatePicker::create(u32::try_from(date.year()).unwrap_or(1970), date.month(), date.day())
                    .with_today(u32::try_from(today.year()).unwrap_or(1970), today.month(), today.day())
                    // The week start setting (Settings > General).
                    .with_week_start(repeat_form::picker_week_start(s.settings.week_start))
                    .with_accessibility_name("Due date")
                    .with_on_change(detail_ref(app, &t.id, 0), on_due_date as DatePickerOnChangeCallbackType)
                    .dom()
                    .with_id(ids::DETAIL_DUE),
            );
            match t.due_time {
                Some(time) => row.add_child(
                    TimePicker::create(time.hour(), time.minute())
                        .with_24h(true)
                        .with_accessibility_name("Due time")
                        .with_on_change(detail_ref(app, &t.id, 0), on_due_time as TimePickerOnChangeCallbackType)
                        .dom()
                        .with_id(ids::DETAIL_TIME),
                ),
                None => row.add_child(
                    Button::create("Add time")
                        .with_icon("schedule")
                        .with_on_click(detail_ref(app, &t.id, 0), on_add_time as ButtonOnClickCallbackType)
                        .dom()
                        .with_id(ids::ADD_TIME),
                ),
            }
            if t.due_time.is_some() {
                row.add_child(
                    Button::create("No time")
                        .with_on_click(detail_ref(app, &t.id, 1), on_add_time as ButtonOnClickCallbackType)
                        .dom(),
                );
            }
            row.add_child(
                Button::create("Clear")
                    .with_on_click(detail_ref(app, &t.id, 3), on_due_quick as ButtonOnClickCallbackType)
                    .dom()
                    .with_id(ids::DUE_CLEAR),
            );
            row.add_child(
                Dom::create_span_with_text(model::day_label(date, today)).with_css(META),
            );
        }
    }
    row
}

/// The repeat: the preset control, and the editor for a custom rule.
fn repeat(s: &Tasks, app: &RefAny, t: &Task, today: NaiveDate) -> Dom {
    let preset = repeat_preset(t.repeat.as_ref());
    let mut out = Dom::create_div().with_css("display: flex; flex-direction: column; gap: 6px;");
    out.add_child(field(
        "Repeat",
        DropDown::create(strings(&REPEATS))
            .with_selected(preset)
            .with_accessibility_name("Repeat")
            .with_on_choice_change(detail_ref(app, &t.id, 0), on_repeat as DropDownOnChoiceChangeCallbackType)
            .dom()
            .with_id(ids::DETAIL_REPEAT),
    ));
    let editing = s.drafts.custom_repeat && s.drafts.task == t.id;
    if let Some(rule) = t.repeat.as_ref().filter(|_| editing || preset == REPEATS.len() - 1) {
        // The custom repeat is azul's DateRepeatPicker (AzCalendar's too): every N days /
        // weeks on days / months / years, from completion; a to-do's repeat has no end and no
        // "second Wednesday", so the editor leaves those out.
        let due = t.due.unwrap_or(today);
        out.add_child(
            DateRepeatPicker::create(repeat_form::rule_of(Some(rule), due))
                .with_week_start(repeat_form::picker_week_start(s.settings.week_start))
                .with_completion_option(true)
                .with_end_option(false)
                .with_month_weekday_option(false)
                .with_accessibility_name("Custom repeat")
                .with_on_change(
                    detail_ref(app, &t.id, 0),
                    on_repeat_rule as DateRepeatPickerOnChangeCallbackType,
                )
                .dom()
                .with_id(ids::REPEAT_EDITOR),
        );
        out.add_child(Dom::create_span_with_text(rule.label()).with_css(META));
    }
    out
}

/// The reminder: a preset, a date and time for "On a date...", and when it goes off.
fn reminder(s: &Tasks, app: &RefAny, t: &Task, today: NaiveDate) -> Dom {
    let preset = reminders::preset_of(t.reminder);
    let mut row = Dom::create_div().with_css(FIELD).with_child(label("Remind me")).with_child(
        DropDown::create(owned(Preset::ALL.iter().map(|p| p.label().to_string()).collect()))
            .with_selected(preset.index())
            .with_accessibility_name("Reminder")
            .with_on_choice_change(detail_ref(app, &t.id, 0), on_reminder as DropDownOnChoiceChangeCallbackType)
            .dom()
            .with_id(ids::DETAIL_REMINDER),
    );
    if let Some(Reminder::At(at)) = t.reminder {
        row.add_child(
            DatePicker::create(u32::try_from(at.year()).unwrap_or(1970), at.month(), at.day())
                .with_today(u32::try_from(today.year()).unwrap_or(1970), today.month(), today.day())
                .with_week_start(repeat_form::picker_week_start(s.settings.week_start))
                .with_accessibility_name("Reminder date")
                .with_on_change(detail_ref(app, &t.id, 0), on_reminder_date as DatePickerOnChangeCallbackType)
                .dom(),
        );
        row.add_child(
            TimePicker::create(at.hour(), at.minute())
                .with_24h(true)
                .with_accessibility_name("Reminder time")
                .with_on_change(detail_ref(app, &t.id, 0), on_reminder_time as TimePickerOnChangeCallbackType)
                .dom(),
        );
    }
    if let Some(when) = reminders::describe(t, s.settings.reminder_time, today) {
        row.add_child(Dom::create_span_with_text(format!("Reminds {when}")).with_css(META));
    } else if matches!(t.reminder, Some(Reminder::Before(_))) {
        row.add_child(Dom::create_span_with_text("Set a due date for this reminder").with_css(META));
    }
    row
}

/// The task's list.
fn list_field(s: &Tasks, app: &RefAny, t: &Task) -> Dom {
    let order = views::lists_in_nav_order(&s.lists);
    let names: Vec<String> = order
        .iter()
        .map(|&i| {
            let l = &s.lists[i];
            if l.group.is_empty() {
                l.name.clone()
            } else {
                format!("{} \u{203a} {}", l.group, l.name)
            }
        })
        .collect();
    let selected = order.iter().position(|&i| s.lists[i].id == t.list).unwrap_or(0);
    field(
        "List",
        DropDown::create(owned(names))
            .with_selected(selected)
            .with_accessibility_name("List")
            .with_on_choice_change(detail_ref(app, &t.id, 0), on_list_change as DropDownOnChoiceChangeCallbackType)
            .dom()
            .with_id(ids::DETAIL_LIST),
    )
}

/// The tags as removable chips, and "Add a tag".
fn tags(s: &Tasks, app: &RefAny, t: &Task) -> Dom {
    let mut row = Dom::create_div().with_id(ids::DETAIL_TAGS).with_css(FIELD).with_child(label("Tags"));
    for (n, tag) in t.tags.iter().enumerate() {
        row.add_child(
            Chip::create(format!("#{tag}"))
                .with_removable(true)
                .with_on_remove(detail_ref(app, &t.id, n), on_tag_remove as ChipOnRemoveCallbackType)
                .dom(),
        );
    }
    row.with_child(
        TextInput::create()
            .with_text(s.drafts.tag.as_str())
            .with_placeholder("Add a tag")
            .with_accessibility_name("Add a tag")
            .with_on_text_input(app.clone(), on_tag_text as TextInputOnTextInputCallbackType)
            .with_on_virtual_key_down(app.clone(), on_tag_key as TextInputOnVirtualKeyDownCallbackType)
            .dom()
            .with_id(ids::ADD_TAG)
            .with_css("min-width: 120px;"),
    )
    .with_child(tag_suggestions(s, app, t))
}

/// How many of the other tags the tag field offers.
const TAG_SUGGESTIONS: usize = 6;

/// The tags the user gives other tasks, the most used first, as chips a click adds - the
/// suggestions a token field would show (azul has no TokenInput widget yet: chips, the field
/// and these).
fn tag_suggestions(s: &Tasks, app: &RefAny, t: &Task) -> Dom {
    let mut row = Dom::create_div()
        .with_id(ids::TAG_SUGGESTIONS)
        .with_css("display: flex; flex-direction: row; flex-wrap: wrap; align-items: center; gap: 4px;");
    for tag in views::tag_suggestions(&s.tasks, t, TAG_SUGGESTIONS) {
        row.add_child(
            Chip::create(format!("+ #{tag}"))
                .with_on_click(
                    RefAny::new(TagRef {
                        app: app.clone(),
                        task: t.id.clone(),
                        tag,
                    }),
                    on_tag_suggestion as ChipOnClickCallbackType,
                )
                .dom()
                .with_class(ids::TAG_SUGGESTION_CLASS),
        );
    }
    row
}

/// What a suggested tag's chip carries.
struct TagRef {
    app: RefAny,
    task: String,
    tag: String,
}

/// The files next to the task: open, remove, "Attach a file..." (or drop one here).
fn attachments(app: &RefAny, t: &Task) -> Dom {
    let mut out = Dom::create_div()
        .with_id(ids::ATTACHMENTS)
        .with_css("display: flex; flex-direction: column; gap: 4px;")
        .with_child(Dom::create_span_with_text("FILES").with_css(GROUP_TITLE));
    for (n, a) in t.attachments.iter().enumerate() {
        out.add_child(
            Dom::create_div()
                .with_css(STEP_ROW)
                .with_child(Dom::create_icon("description"))
                .with_child(Dom::create_span_with_text(a.name.as_str()).with_css(STEP))
                .with_child(Dom::create_span_with_text(azul::file::DiskSpace::format_bytes(a.size)).with_css(META))
                .with_child(
                    Button::create("Open")
                        .with_on_click(detail_ref(app, &t.id, n), on_attachment_open as ButtonOnClickCallbackType)
                        .dom(),
                )
                .with_child(
                    Button::create("")
                        .with_icon("close")
                        .with_on_click(detail_ref(app, &t.id, n), on_attachment_remove as ButtonOnClickCallbackType)
                        .dom()
                        .with_accessibility_name(format!("Remove {}", a.name)),
                ),
        );
    }
    out.with_child(
        Dom::create_div()
            .with_css(FIELD)
            .with_child(
                Button::create("Attach a file...")
                    .with_icon("attach_file")
                    .with_on_click(detail_ref(app, &t.id, 0), on_attach as ButtonOnClickCallbackType)
                    .dom()
                    .with_id(ids::ATTACH),
            )
            .with_child(Dom::create_span_with_text("or drop files on the window").with_css(META)),
    )
}

/// Created / completed, and Delete.
fn footer(app: &RefAny, t: &Task, today: NaiveDate) -> Dom {
    let stamp = |at: NaiveDateTime| {
        format!("{} {}", model::day_label(at.date(), today), model::format_time(at.time()))
    };
    let mut text = format!("Created {}", stamp(t.created));
    if let Some(done) = t.completed {
        text.push_str(&format!(" \u{b7} Completed {}", stamp(done)));
    }
    Dom::create_div()
        .with_css(FIELD)
        .with_child(Dom::create_span_with_text(text).with_css("font-size: 11px; color: system:secondary-text; flex-grow: 1;"))
        .with_child(
            Button::with_type("Delete", ButtonType::Danger)
                .with_icon("delete")
                .with_on_click(detail_ref(app, &t.id, 0), on_delete as ButtonOnClickCallbackType)
                .dom()
                .with_id(ids::DETAIL_DELETE),
        )
}

// ==== Callbacks: title, notes, priority, flag, completion ====

const KEEP: OnTextInputReturn = OnTextInputReturn {
    update: Update::DoNothing,
    valid: TextInputValid::Yes,
};

/// The typed text is the app's: take it as seen (a field the app empties or reverts then
/// rebuilds with what the app says).
fn ack_typing(info: &mut CallbackInfo) {
    let revision = info.get_document_text_revision();
    info.mark_text_revision_synced(revision);
}

fn key_of(info: &CallbackInfo) -> Option<VirtualKeyCode> {
    info.get_current_keyboard_state().current_virtual_keycode.into_option()
}

fn is_enter(key: Option<VirtualKeyCode>) -> bool {
    matches!(key, Some(VirtualKeyCode::Return | VirtualKeyCode::NumpadEnter))
}

extern "C" fn on_done(mut data: RefAny, mut info: CallbackInfo, _state: CheckBoxState) -> Update {
    with_task(&mut data, &mut info, |_info, _app, s, i, _| {
        s.toggle_done(i, state::now());
    })
}

extern "C" fn on_title_text(mut data: RefAny, _info: CallbackInfo, state: TextInputState) -> OnTextInputReturn {
    if let Some(mut s) = data.downcast_mut::<Tasks>() {
        // The drafts follow the selection before they take the typing.
        s.sync_drafts();
        s.drafts.title = state.get_text().as_str().to_string();
    }
    KEEP
}

/// Enter keeps the title; Escape puts the task's title back.
extern "C" fn on_title_key(mut data: RefAny, mut info: CallbackInfo, _state: TextInputState) -> OnTextInputReturn {
    let key = key_of(&info);
    if is_enter(key) {
        let update = crate::with_tasks(&mut data, &mut info, |_info, _app, s| s.commit_drafts());
        return OnTextInputReturn {
            update,
            valid: TextInputValid::Yes,
        };
    }
    if key == Some(VirtualKeyCode::Escape) {
        let update = crate::with_tasks(&mut data, &mut info, |info, _app, s| {
            if let Some(i) = s.index_of(&s.drafts.task.clone()) {
                s.drafts.title = s.tasks[i].title.clone();
            }
            ack_typing(info);
        });
        return OnTextInputReturn {
            update,
            valid: TextInputValid::Yes,
        };
    }
    KEEP
}

extern "C" fn on_title_blur(mut data: RefAny, mut info: CallbackInfo, _state: TextInputState) -> Update {
    crate::with_tasks(&mut data, &mut info, |_info, _app, s| s.commit_drafts())
}

extern "C" fn on_notes_text(mut data: RefAny, _info: CallbackInfo, state: TextAreaState) -> OnTextInputReturn {
    if let Some(mut s) = data.downcast_mut::<Tasks>() {
        s.sync_drafts();
        s.drafts.notes = state.get_text().to_string();
    }
    KEEP
}

extern "C" fn on_notes_blur(mut data: RefAny, mut info: CallbackInfo, state: TextAreaState) -> Update {
    let text = state.get_text().to_string();
    crate::with_tasks(&mut data, &mut info, |_info, _app, s| {
        s.sync_drafts();
        s.drafts.notes = text;
        s.commit_drafts();
    })
}

extern "C" fn on_priority(mut data: RefAny, mut info: CallbackInfo, state: SegmentedState) -> Update {
    with_task(&mut data, &mut info, |_info, _app, s, i, _| {
        s.tasks[i].priority = Priority::from_index(state.selected_index);
    })
}

extern "C" fn on_flag(mut data: RefAny, mut info: CallbackInfo, state: SwitchState) -> Update {
    with_task(&mut data, &mut info, |_info, _app, s, i, _| {
        s.tasks[i].flagged = state.checked;
    })
}

// ==== Callbacks: steps ====

extern "C" fn on_step_done(mut data: RefAny, mut info: CallbackInfo, state: CheckBoxState) -> Update {
    with_task(&mut data, &mut info, |_info, _app, s, i, n| {
        if let Some(step) = s.tasks[i].subtasks.get_mut(n) {
            step.done = state.checked;
        }
    })
}

extern "C" fn on_step_remove(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_task(&mut data, &mut info, |_info, _app, s, i, n| {
        if n < s.tasks[i].subtasks.len() {
            s.tasks[i].subtasks.remove(n);
        }
    })
}

extern "C" fn on_step_text(mut data: RefAny, _info: CallbackInfo, state: TextInputState) -> OnTextInputReturn {
    if let Some(mut s) = data.downcast_mut::<Tasks>() {
        s.sync_drafts();
        s.drafts.step = state.get_text().as_str().to_string();
    }
    KEEP
}

/// Enter adds the step to the selected task.
extern "C" fn on_step_key(mut data: RefAny, mut info: CallbackInfo, state: TextInputState) -> OnTextInputReturn {
    if !is_enter(key_of(&info)) {
        return KEEP;
    }
    let title = state.get_text().as_str().trim().to_string();
    let update = crate::with_tasks(&mut data, &mut info, |info, _app, s| {
        s.drafts.step.clear();
        ack_typing(info);
        let Some(i) = s.selected_one() else {
            return;
        };
        if title.is_empty() {
            return;
        }
        let next = s.tasks[i].subtasks.len() + 1;
        let mut id = format!("s{next}");
        let mut n = next;
        while s.tasks[i].subtasks.iter().any(|x| x.id == id) {
            n += 1;
            id = format!("s{n}");
        }
        s.tasks[i].subtasks.push(Subtask {
            id,
            title,
            done: false,
        });
        s.save_task(i);
    });
    OnTextInputReturn {
        update,
        valid: TextInputValid::Yes,
    }
}

// ==== Callbacks: due date and time ====

/// The first day of next week (the settings' week start).
fn next_week(today: NaiveDate, start: Weekday) -> NaiveDate {
    let back = (7 + today.weekday().num_days_from_monday() - start.num_days_from_monday()) % 7;
    today - Duration::days(i64::from(back)) + Duration::days(7)
}

/// 0 Today, 1 Tomorrow, 2 Next week, 3 Clear.
extern "C" fn on_due_quick(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_task(&mut data, &mut info, |_info, _app, s, i, n| {
        let today = state::now().date();
        let week_start = s.settings.week_start;
        let t = &mut s.tasks[i];
        t.due = match n {
            0 => Some(today),
            1 => Some(today + Duration::days(1)),
            2 => Some(next_week(today, week_start)),
            _ => None,
        };
        if t.due.is_none() {
            t.due_time = None;
        }
        t.reminded = None;
        state::reanchor(t);
    })
}

extern "C" fn on_due_date(mut data: RefAny, mut info: CallbackInfo, state: DatePickerState) -> Update {
    with_task(&mut data, &mut info, |_info, _app, s, i, _| {
        let Some(date) = NaiveDate::from_ymd_opt(i32::try_from(state.year).unwrap_or(1970), state.month, state.day) else {
            return;
        };
        let t = &mut s.tasks[i];
        t.due = Some(date);
        t.reminded = None;
        state::reanchor(t);
    })
}

/// The hour of a time picker's state, on the 24-hour clock.
fn picked_time(state: &TimePickerState) -> Option<NaiveTime> {
    let hour = if state.is_24h {
        state.hour
    } else {
        state.hour % 12 + if state.is_pm { 12 } else { 0 }
    };
    NaiveTime::from_hms_opt(hour, state.minute, 0)
}

extern "C" fn on_due_time(mut data: RefAny, mut info: CallbackInfo, state: TimePickerState) -> Update {
    with_task(&mut data, &mut info, |_info, _app, s, i, _| {
        if let Some(time) = picked_time(&state) {
            s.tasks[i].due_time = Some(time);
            s.tasks[i].reminded = None;
        }
    })
}

/// 0 "Add time" (the reminder time setting), 1 "No time".
extern "C" fn on_add_time(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_task(&mut data, &mut info, |_info, _app, s, i, n| {
        let time = s.settings.reminder_time;
        s.tasks[i].due_time = if n == 0 { Some(time) } else { None };
        s.tasks[i].reminded = None;
    })
}

// ==== Callbacks: repeat ====

extern "C" fn on_repeat(mut data: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    with_task(&mut data, &mut info, |_info, _app, s, i, _| {
        let today = state::now().date();
        if index == REPEATS.len() - 1 {
            s.drafts.custom_repeat = true;
            let due = s.tasks[i].due.unwrap_or(today);
            let t = &mut s.tasks[i];
            if t.repeat.is_none() {
                t.repeat = Some(Repeat::weekly().on_weekdays(&[due.weekday()]));
                t.due.get_or_insert(due);
            }
            return;
        }
        s.drafts.custom_repeat = false;
        let t = &mut s.tasks[i];
        let due = t.due.unwrap_or(today);
        t.repeat = preset_repeat(index, due);
        if let Some(rule) = &t.repeat {
            if t.due.is_none() {
                t.due = Some(rule.first_on_or_after(today));
            }
        }
    })
}

/// The custom repeat's editor changed the rule: the task takes it ("Never" takes the repeat
/// away). Typing the "every N" number does not rebuild the pane (the field keeps its caret).
extern "C" fn on_repeat_rule(mut data: RefAny, mut info: CallbackInfo, rule: DateRepeatRule) -> Update {
    let mut rebuild = true;
    let update = with_task(&mut data, &mut info, |_info, _app, s, i, _| {
        let today = state::now().date();
        let t = &mut s.tasks[i];
        let due = *t.due.get_or_insert(today);
        let next = repeat_form::repeat_of(&rule, t.repeat.as_ref(), due);
        rebuild = !repeat_form::only_the_number_changed(t.repeat.as_ref(), next.as_ref());
        t.repeat = next;
        s.drafts.custom_repeat = true;
    });
    if rebuild {
        update
    } else {
        Update::DoNothing
    }
}

// ==== Callbacks: reminder ====

extern "C" fn on_reminder(mut data: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    with_task(&mut data, &mut info, |_info, _app, s, i, _| {
        let preset = Preset::ALL.get(index).copied().unwrap_or(Preset::None);
        let reminder_time = s.settings.reminder_time;
        let now = state::now();
        let t = &s.tasks[i];
        // Where a moment starts when the task has none: its due day at its time (or the
        // reminder time), else an hour from now.
        let fallback = t
            .due
            .map(|d| d.and_time(t.due_time.unwrap_or(reminder_time)))
            .unwrap_or(now + Duration::hours(1));
        let reminder = reminders::reminder_for(preset, t, reminder_time, fallback);
        let t = &mut s.tasks[i];
        t.reminder = reminder;
        t.reminded = None;
    })
}

extern "C" fn on_reminder_date(mut data: RefAny, mut info: CallbackInfo, state: DatePickerState) -> Update {
    with_task(&mut data, &mut info, |_info, _app, s, i, _| {
        let Some(date) = NaiveDate::from_ymd_opt(i32::try_from(state.year).unwrap_or(1970), state.month, state.day) else {
            return;
        };
        if let Some(Reminder::At(at)) = s.tasks[i].reminder {
            s.tasks[i].reminder = Some(Reminder::At(date.and_time(at.time())));
            s.tasks[i].reminded = None;
        }
    })
}

extern "C" fn on_reminder_time(mut data: RefAny, mut info: CallbackInfo, state: TimePickerState) -> Update {
    with_task(&mut data, &mut info, |_info, _app, s, i, _| {
        let Some(time) = picked_time(&state) else {
            return;
        };
        if let Some(Reminder::At(at)) = s.tasks[i].reminder {
            s.tasks[i].reminder = Some(Reminder::At(at.date().and_time(time)));
            s.tasks[i].reminded = None;
        }
    })
}

// ==== Callbacks: list, tags ====

extern "C" fn on_list_change(mut data: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    with_task(&mut data, &mut info, |info, app, s, i, _| {
        let order = views::lists_in_nav_order(&s.lists);
        let Some(&li) = order.get(index) else {
            return;
        };
        let list = s.lists[li].id.clone();
        let id = s.tasks[i].id.clone();
        let moves = s.move_tasks(&[id], &list);
        crate::jobs::move_files(info, app, s, moves);
    })
}

extern "C" fn on_tag_remove(mut data: RefAny, mut info: CallbackInfo, _state: ChipState) -> Update {
    with_task(&mut data, &mut info, |_info, _app, s, i, n| {
        if n < s.tasks[i].tags.len() {
            s.tasks[i].tags.remove(n);
        }
    })
}

/// A click on a suggested tag adds it to the task.
extern "C" fn on_tag_suggestion(mut data: RefAny, mut info: CallbackInfo, _state: ChipState) -> Update {
    let Some((mut app, task, tag)) = data
        .downcast_ref::<TagRef>()
        .map(|r| (r.app.clone(), r.task.clone(), r.tag.clone()))
    else {
        return Update::DoNothing;
    };
    crate::with_tasks(&mut app, &mut info, |_info, _app, s| {
        if let Some(i) = s.index_of(&task) {
            if s.tasks[i].add_tag(&tag) {
                s.save_task(i);
            }
        }
    })
}

extern "C" fn on_tag_text(mut data: RefAny, _info: CallbackInfo, state: TextInputState) -> OnTextInputReturn {
    if let Some(mut s) = data.downcast_mut::<Tasks>() {
        s.sync_drafts();
        s.drafts.tag = state.get_text().as_str().to_string();
    }
    KEEP
}

/// Enter adds the typed tags (split at commas and spaces) to the selected task.
extern "C" fn on_tag_key(mut data: RefAny, mut info: CallbackInfo, state: TextInputState) -> OnTextInputReturn {
    if !is_enter(key_of(&info)) {
        return KEEP;
    }
    let text = state.get_text().as_str().to_string();
    let update = crate::with_tasks(&mut data, &mut info, |info, _app, s| {
        s.drafts.tag.clear();
        ack_typing(info);
        let Some(i) = s.selected_one() else {
            return;
        };
        let mut changed = false;
        for tag in text.split([',', ' ']) {
            changed |= s.tasks[i].add_tag(tag);
        }
        if changed {
            s.save_task(i);
        }
    });
    OnTextInputReturn {
        update,
        valid: TextInputValid::Yes,
    }
}

// ==== Callbacks: attachments, delete ====

/// Copies the file at `path` next to task `task` (on a thread).
fn attach(info: &mut CallbackInfo, app: &RefAny, s: &mut Tasks, task: &str, path: PathBuf) {
    let Some(i) = s.index_of(task) else {
        return;
    };
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let Some(key) = model::attachment_key(&s.tasks[i].list, task, &name) else {
        s.files.last_error = format!("\"{name}\" cannot be a file name in the data folder.");
        return;
    };
    let name = azul_storage::key::last_segment(&key).to_string();
    crate::jobs::spawn(
        info,
        app,
        s,
        crate::jobs::Job::Attach {
            task: task.to_string(),
            name,
            key,
            source: path,
        },
    );
}

extern "C" fn on_attach(data: RefAny, _info: CallbackInfo) -> Update {
    let _request = FileDialog::open_file(
        "Attach a file",
        OptionString::None,
        OptionFileTypeList::None,
        data,
        on_attach_picked,
    );
    Update::DoNothing
}

extern "C" fn on_attach_picked(mut data: RefAny, mut info: CallbackInfo, result: RefAny) -> Update {
    let Some(picked) = FileOpenResult::downcast(result).into_option() else {
        return Update::DoNothing;
    };
    let Some(path) = picked.path.into_option() else {
        return Update::DoNothing; // cancelled
    };
    let source = PathBuf::from(path.as_string().as_str());
    with_task(&mut data, &mut info, |info, app, s, i, _| {
        let id = s.tasks[i].id.clone();
        attach(info, app, s, &id, source);
    })
}

/// Files dropped on the window are attached to the selected task.
extern "C" fn on_file_dropped(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let files: Vec<PathBuf> = info
        .get_dropped_files()
        .as_slice()
        .iter()
        .map(|f| PathBuf::from(f.as_str()))
        .collect();
    crate::with_tasks(&mut data, &mut info, |info, app, s| {
        let Some(i) = s.selected_one() else {
            s.notice = "Select a task to attach the dropped files to.".to_string();
            return;
        };
        let id = s.tasks[i].id.clone();
        for path in files {
            attach(info, app, s, &id, path);
        }
    })
}

extern "C" fn on_attachment_open(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_task(&mut data, &mut info, |info, app, s, i, n| {
        let t = &s.tasks[i];
        let Some(a) = t.attachments.get(n) else {
            return;
        };
        let Some(key) = model::attachment_key(&t.list, &t.id, &a.name) else {
            return;
        };
        let dir = std::env::temp_dir().join("AzTasks-open").join(&t.id);
        crate::jobs::spawn(info, app, s, crate::jobs::Job::Open { key, dir });
    })
}

extern "C" fn on_attachment_remove(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_task(&mut data, &mut info, |_info, _app, s, i, n| {
        if n >= s.tasks[i].attachments.len() {
            return;
        }
        let a = s.tasks[i].attachments.remove(n);
        if let Some(key) = model::attachment_key(&s.tasks[i].list, &s.tasks[i].id, &a.name) {
            s.queue.delete(key);
        }
    })
}

extern "C" fn on_delete(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, task)) = data
        .downcast_ref::<DetailRef>()
        .map(|r| (r.app.clone(), r.task.clone()))
    else {
        return Update::DoNothing;
    };
    crate::with_tasks(&mut app, &mut info, |info, app, s| {
        let gone = s.delete_tasks(&[task]);
        crate::jobs::delete_files(info, app, s, gone);
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn every_repeat_preset_reads_back_as_itself() {
        let due = day(2026, 10, 1);
        for index in 1..REPEATS.len() - 1 {
            let rule = preset_repeat(index, due);
            assert_eq!(repeat_preset(rule.as_ref()), index, "{}", REPEATS[index]);
        }
        assert_eq!(repeat_preset(None), 0);
        assert_eq!(preset_repeat(0, due), None);
        assert_eq!(
            repeat_preset(Some(&Repeat::new(3, Unit::Day))),
            REPEATS.len() - 1,
            "every 3 days is a custom rule"
        );
        assert_eq!(
            repeat_preset(Some(&Repeat::daily().counting_from_completion(true))),
            REPEATS.len() - 1
        );
    }

    #[test]
    fn next_week_starts_on_the_week_start_setting() {
        let thursday = day(2026, 10, 1);
        assert_eq!(next_week(thursday, Weekday::Mon), day(2026, 10, 5));
        assert_eq!(next_week(thursday, Weekday::Sun), day(2026, 10, 4));
        assert_eq!(next_week(day(2026, 10, 5), Weekday::Mon), day(2026, 10, 12));
    }
}
