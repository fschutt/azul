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
        ButtonOnClickCallbackType, CheckBoxOnToggleCallbackType, ChipOnRemoveCallbackType,
        DatePickerOnChangeCallbackType, DropDownOnChoiceChangeCallbackType,
        SegmentedOnChangeCallbackType, SwitchOnToggleCallbackType, TextAreaOnFocusLostCallbackType,
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
        OnTextInputReturn, Segmented, SegmentedState, Switch, SwitchState, TextArea,
        TextAreaState, TextInputState, TextInputValid, TimePicker, TimePickerState,
    },
};
use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, NaiveTime, Timelike, Weekday};

use crate::{
    model::{self, Priority, Reminder, Subtask, Task},
    recur::{self, Repeat, Unit},
    reminders::{self, Preset},
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

/// The custom editor's units.
const UNITS: [&str; 4] = ["days", "weeks", "months", "years"];
/// The custom editor's counts.
const COUNTS: usize = 30;
const WEEK: [Weekday; 7] = [
    Weekday::Mon,
    Weekday::Tue,
    Weekday::Wed,
    Weekday::Thu,
    Weekday::Fri,
    Weekday::Sat,
    Weekday::Sun,
];

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
        .with_id("detail")
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
                    .with_id("detail-done"),
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
                    .with_id("detail-title")
                    .with_css("flex-grow: 1; font-size: 16px;"),
            ),
    );
    pane.add_child(field(
        "Priority",
        Segmented::create(strings(&["None", "Low", "Medium", "High"]))
            .with_selected_index(t.priority.index())
            .with_on_change(detail_ref(app, &t.id, 0), on_priority as SegmentedOnChangeCallbackType)
            .dom()
            .with_id("detail-priority"),
    ));
    pane.add_child(field(
        "Flagged",
        Switch::create(t.flagged)
            .with_accessibility_name("Flagged")
            .with_on_toggle(detail_ref(app, &t.id, 0), on_flag as SwitchOnToggleCallbackType)
            .dom()
            .with_id("detail-flag"),
    ));

    pane.add_child(steps(app, t));
    pane.add_child(Dom::create_div().with_css(RULE));
    pane.add_child(due(app, t, today));
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
            .with_id("detail-notes")
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
        .with_id("steps")
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
                        .with_id(format!("step-{n}")),
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
            .with_id("add-step"),
    )
}

/// The due date: quick days when there is none; the date, the time (or "Add time") and
/// "Clear" when there is one.
fn due(app: &RefAny, t: &Task, today: NaiveDate) -> Dom {
    let mut row = Dom::create_div().with_css(FIELD).with_child(label("Due"));
    match t.due {
        None => {
            for (n, name) in ["Today", "Tomorrow", "Next week"].iter().enumerate() {
                row.add_child(
                    Button::create(*name)
                        .with_on_click(detail_ref(app, &t.id, n), on_due_quick as ButtonOnClickCallbackType)
                        .dom()
                        .with_id(format!("due-quick-{n}")),
                );
            }
        }
        Some(date) => {
            row.add_child(
                DatePicker::create(u32::try_from(date.year()).unwrap_or(1970), date.month(), date.day())
                    .with_today(u32::try_from(today.year()).unwrap_or(1970), today.month(), today.day())
                    .with_accessibility_name("Due date")
                    .with_on_change(detail_ref(app, &t.id, 0), on_due_date as DatePickerOnChangeCallbackType)
                    .dom()
                    .with_id("detail-due"),
            );
            match t.due_time {
                Some(time) => row.add_child(
                    TimePicker::create(time.hour(), time.minute())
                        .with_24h(true)
                        .with_accessibility_name("Due time")
                        .with_on_change(detail_ref(app, &t.id, 0), on_due_time as TimePickerOnChangeCallbackType)
                        .dom()
                        .with_id("detail-time"),
                ),
                None => row.add_child(
                    Button::create("Add time")
                        .with_icon("schedule")
                        .with_on_click(detail_ref(app, &t.id, 0), on_add_time as ButtonOnClickCallbackType)
                        .dom()
                        .with_id("add-time"),
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
                    .with_id("due-clear"),
            );
            row.add_child(
                Dom::create_span_with_text(model::day_label(date, today)).with_css(META),
            );
        }
    }
    row
}

/// The repeat: the preset control, and the editor for a custom rule.
fn repeat(s: &Tasks, app: &RefAny, t: &Task, _today: NaiveDate) -> Dom {
    let preset = repeat_preset(t.repeat.as_ref());
    let mut out = Dom::create_div().with_css("display: flex; flex-direction: column; gap: 6px;");
    out.add_child(field(
        "Repeat",
        DropDown::create(strings(&REPEATS))
            .with_selected(preset)
            .with_accessibility_name("Repeat")
            .with_on_choice_change(detail_ref(app, &t.id, 0), on_repeat as DropDownOnChoiceChangeCallbackType)
            .dom()
            .with_id("detail-repeat"),
    ));
    let editing = s.drafts.custom_repeat && s.drafts.task == t.id;
    if let Some(rule) = t.repeat.as_ref().filter(|_| editing || preset == REPEATS.len() - 1) {
        let counts: Vec<String> = (1..=COUNTS).map(|n| n.to_string()).collect();
        let unit = Unit::ALL.iter().position(|u| *u == rule.unit).unwrap_or(0);
        let mut editor = Dom::create_div()
            .with_id("repeat-editor")
            .with_css(FIELD)
            .with_child(label("Every"))
            .with_child(
                DropDown::create(owned(counts))
                    .with_selected(usize::try_from(rule.every).unwrap_or(1).clamp(1, COUNTS) - 1)
                    .with_accessibility_name("Every how many")
                    .with_on_choice_change(detail_ref(app, &t.id, 0), on_repeat_every as DropDownOnChoiceChangeCallbackType)
                    .dom(),
            )
            .with_child(
                DropDown::create(strings(&UNITS))
                    .with_selected(unit)
                    .with_accessibility_name("Unit")
                    .with_on_choice_change(detail_ref(app, &t.id, 0), on_repeat_unit as DropDownOnChoiceChangeCallbackType)
                    .dom(),
            );
        if rule.unit == Unit::Week {
            for (n, day) in WEEK.iter().enumerate() {
                let on = rule.weekdays.contains(day);
                editor.add_child(
                    Button::with_type(
                        recur::weekday_short(*day),
                        if on { ButtonType::Primary } else { ButtonType::Default },
                    )
                    .with_on_click(detail_ref(app, &t.id, n), on_repeat_day as ButtonOnClickCallbackType)
                    .dom()
                    .with_accessibility_name(format!(
                        "{} {}",
                        recur::weekday_short(*day),
                        if on { "(on)" } else { "(off)" }
                    )),
                );
            }
        }
        editor.add_child(
            CheckBox::create(rule.from_completion)
                .with_accessibility_name("Count from completion")
                .with_on_toggle(detail_ref(app, &t.id, 0), on_repeat_from_completion as CheckBoxOnToggleCallbackType)
                .dom(),
        );
        editor.add_child(Dom::create_span_with_text("after completion").with_css(META));
        out.add_child(editor);
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
            .with_id("detail-reminder"),
    );
    if let Some(Reminder::At(at)) = t.reminder {
        row.add_child(
            DatePicker::create(u32::try_from(at.year()).unwrap_or(1970), at.month(), at.day())
                .with_today(u32::try_from(today.year()).unwrap_or(1970), today.month(), today.day())
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
            .with_id("detail-list"),
    )
}

/// The tags as removable chips, and "Add a tag".
fn tags(s: &Tasks, app: &RefAny, t: &Task) -> Dom {
    let mut row = Dom::create_div().with_id("detail-tags").with_css(FIELD).with_child(label("Tags"));
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
            .with_id("add-tag")
            .with_css("min-width: 120px;"),
    )
}

/// The files next to the task: open, remove, "Attach a file..." (or drop one here).
fn attachments(app: &RefAny, t: &Task) -> Dom {
    let mut out = Dom::create_div()
        .with_id("attachments")
        .with_css("display: flex; flex-direction: column; gap: 4px;")
        .with_child(Dom::create_span_with_text("FILES").with_css(GROUP_TITLE));
    for (n, a) in t.attachments.iter().enumerate() {
        out.add_child(
            Dom::create_div()
                .with_css(STEP_ROW)
                .with_child(Dom::create_icon("description"))
                .with_child(Dom::create_span_with_text(a.name.as_str()).with_css(STEP))
                .with_child(Dom::create_span_with_text(size_text(a.size)).with_css(META))
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
                    .with_id("attach"),
            )
            .with_child(Dom::create_span_with_text("or drop files on the window").with_css(META)),
    )
}

/// "12 KB".
fn size_text(bytes: u64) -> String {
    match bytes {
        b if b < 1024 => format!("{b} B"),
        b if b < 1024 * 1024 => format!("{} KB", b / 1024),
        b => format!("{:.1} MB", b as f64 / (1024.0 * 1024.0)),
    }
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
                .with_id("detail-delete"),
        )
}
