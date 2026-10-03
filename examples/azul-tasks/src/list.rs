//! The task list pane (the shell's list slot): the reminder banner and the undo line, the
//! view's header, the quick-add line with the chips of what it recognised, and the tasks in
//! their sections - each a row (the plan's `TaskRow`): the check box, the title with its
//! priority mark and flag, and a line of the due chip (red when overdue), the repeat, the
//! list (in a smart list), the steps "2/4", the tags, a clip for attachments and the first
//! line of the notes.
//!
//! A click selects (Ctrl / Cmd adds, Shift extends); rows drag onto rows to reorder (or to
//! move to another list in All / Flagged); the check box completes. Element ids for scripts:
//! `#quick-add`, `#quick-add-button`, `#task-list`, `#task-<id>`, `#check-<id>`,
//! `#section-<key>`, `#reminder-banner`, `#undo`.

use azul::{
    callbacks::{
        ButtonOnClickCallbackType, CheckBoxOnToggleCallbackType, ChipOnClickCallbackType,
        TextInputOnTextInputCallbackType, TextInputOnVirtualKeyDownCallbackType,
    },
    dom::{AttributeType, TabIndex, VirtualKeyCode},
    prelude::*,
    shells::ShellEmptyState,
    widgets::{
        AlertKind, ButtonType, CheckBoxState, Chip, ChipKind, ChipState, InfoBar, OnTextInputReturn,
        TextInputState, TextInputValid,
    },
};
use chrono::NaiveDateTime;

use crate::{
    ids,
    model::{self, Priority, Reminder, Task},
    parse::PartKind,
    reminders,
    state::{self, Confirm, Tasks},
    views::{self, Section, SectionKind, Smart, View},
};

/// The drag's data type.
const DRAG_MIME: &str = "application/x-aztasks-task";

// ==== Styles (system colours: they follow the mode; the accent from the theme scope) ====

const PANE: &str = "display: flex; flex-direction: column; flex-grow: 1; min-height: 0px; \
                    min-width: 0px;";
const HEADER: &str = "display: flex; flex-direction: row; align-items: center; padding: 12px \
                      16px 4px 16px; gap: 8px;";
const HEADER_TITLE: &str = "font-size: 20px; font-weight: bold;";
const HEADER_SUB: &str = "font-size: 12px; color: system:secondary-text;";
const QUICK_ROW: &str = "display: flex; flex-direction: row; align-items: center; padding: 6px \
                         16px; gap: 8px;";
const CHIPS_ROW: &str = "display: flex; flex-direction: row; flex-wrap: wrap; align-items: \
                         center; padding: 0px 16px 6px 16px; gap: 6px;";
const HINT: &str = "font-size: 11px; color: system:secondary-text;";
const SCROLL: &str = "display: flex; flex-direction: column; flex-grow: 1; min-height: 0px; \
                      overflow-y: auto; padding: 0px 8px 12px 8px;";
const SECTION_TITLE: &str = "font-size: 11px; font-weight: bold; letter-spacing: 0.5px; \
                             color: system:secondary-text; padding: 12px 8px 4px 8px;";
const SECTION_OVERDUE: &str = "font-size: 11px; font-weight: bold; letter-spacing: 0.5px; \
                               padding: 12px 8px 4px 8px; color: #b3261e; @media \
                               (prefers-color-scheme: dark) { color: #ff8a80; }";
const ROW: &str = "display: flex; flex-direction: row; align-items: flex-start; padding: 6px \
                   8px; gap: 8px; border-radius: 6px; border-bottom: 1px solid \
                   system:separator; cursor: default;";
const ROW_SELECTED: &str = "display: flex; flex-direction: row; align-items: flex-start; \
                            padding: 6px 8px; gap: 8px; border-radius: 6px; border-bottom: 1px \
                            solid transparent; background: var(--az-accent-soft, #e1e6e1); \
                            @media (prefers-color-scheme: dark) { background: \
                            var(--az-accent-soft, #2f4c39); }";
const ROW_TEXT: &str = "display: flex; flex-direction: column; flex-grow: 1; min-width: 0px; \
                        gap: 2px;";
const TITLE_LINE: &str = "display: flex; flex-direction: row; align-items: center; gap: 6px;";
const TITLE: &str = "font-size: 14px;";
const TITLE_DONE: &str = "font-size: 14px; color: system:secondary-text; text-decoration: \
                          line-through;";
const META_LINE: &str = "display: flex; flex-direction: row; flex-wrap: wrap; align-items: \
                         center; gap: 6px; font-size: 12px; color: system:secondary-text;";
const PRIORITY: &str = "font-size: 13px; font-weight: bold; color: #c25e00; @media \
                        (prefers-color-scheme: dark) { color: #ffb366; }";
const FLAG: &str = "color: #d9730d; @media (prefers-color-scheme: dark) { color: #f2a65a; }";
const NOTES: &str = "font-size: 12px; color: system:secondary-text; overflow: hidden; \
                     white-space: nowrap; text-overflow: ellipsis; max-width: 360px;";
const BAR_ROW: &str = "display: flex; flex-direction: row; align-items: center; gap: 8px; \
                       padding: 6px 16px 0px 16px;";

// ==== Row data ====

/// What a row's callbacks carry: the app and the task's id.
struct RowRef {
    app: RefAny,
    id: String,
}

pub(crate) fn row_ref(app: &RefAny, id: &str) -> RefAny {
    RefAny::new(RowRef {
        app: app.clone(),
        id: id.to_string(),
    })
}

/// Runs `f` on the state with the row's task id; pumps the write queue and rebuilds.
fn with_row(
    data: &mut RefAny,
    info: &mut CallbackInfo,
    f: impl FnOnce(&mut CallbackInfo, &RefAny, &mut Tasks, &str),
) -> Update {
    let Some((mut app, id)) = data
        .downcast_ref::<RowRef>()
        .map(|r| (r.app.clone(), r.id.clone()))
    else {
        return Update::DoNothing;
    };
    crate::with_tasks(&mut app, info, |info, app, s| f(info, app, s, &id))
}

// ==== The pane ====

/// The task list pane at `now`.
pub fn pane(s: &Tasks, app: &RefAny, now: NaiveDateTime) -> Dom {
    let mut pane = Dom::create_div().with_id(ids::TASK_PANE).with_css(PANE);
    if let Some(banner) = reminder_banner(s, app, now) {
        pane.add_child(banner);
    }
    for problem in [&s.load_error, &s.files.last_error] {
        if !problem.is_empty() {
            pane.add_child(
                Dom::create_div().with_css(BAR_ROW).with_child(
                    InfoBar::create(problem.as_str())
                        .with_kind(AlertKind::Danger)
                        .with_icon("error")
                        .dom(),
                ),
            );
        }
    }
    if let Some(line) = undo_line(s, app) {
        pane.add_child(line);
    }
    pane.add_child(header(s, app, now));
    pane.add_child(quick_add(s, app));
    if let Some(chips) = quick_chips(s, app, now) {
        pane.add_child(chips);
    }
    pane.add_child(body(s, app, now));
    pane
}

/// The banner of the reminders going off: "Reminder: Pay rent" with Show, Snooze, Dismiss.
fn reminder_banner(s: &Tasks, app: &RefAny, _now: NaiveDateTime) -> Option<Dom> {
    let titles: Vec<&str> = s
        .banners
        .iter()
        .filter_map(|id| s.index_of(id))
        .map(|i| s.tasks[i].title.as_str())
        .collect();
    if titles.is_empty() {
        return None;
    }
    Some(
        Dom::create_div()
            .with_id(ids::REMINDER_BANNER)
            .with_css(BAR_ROW)
            .with_child(
                InfoBar::create(reminders::banner_text(&titles))
                    .with_icon("alarm")
                    .with_kind(AlertKind::Info)
                    .with_action("Show")
                    .with_on_action(app.clone(), on_banner_show as ButtonOnClickCallbackType)
                    .dom()
                    .with_css("flex-grow: 1;"),
            )
            .with_child(
                Button::create("Snooze 10 min")
                    .with_on_click(app.clone(), on_banner_snooze as ButtonOnClickCallbackType)
                    .dom()
                    .with_id(ids::SNOOZE),
            )
            .with_child(
                Button::create("Dismiss")
                    .with_on_click(app.clone(), on_banner_dismiss as ButtonOnClickCallbackType)
                    .dom()
                    .with_id(ids::DISMISS_REMINDER),
            ),
    )
}

/// "Deleted "x". [Undo]" after a delete; the notice otherwise.
fn undo_line(s: &Tasks, app: &RefAny) -> Option<Dom> {
    if s.notice.is_empty() {
        return None;
    }
    let mut row = Dom::create_div()
        .with_id(ids::NOTICE)
        .with_css(BAR_ROW)
        .with_child(Dom::create_span_with_text(s.notice.as_str()).with_css("font-size: 13px;"));
    if s.undo.is_some() {
        row.add_child(
            Button::with_type("Undo", ButtonType::Link)
                .with_on_click(app.clone(), on_undo as ButtonOnClickCallbackType)
                .dom()
                .with_id(ids::UNDO),
        );
    }
    row.add_child(
        Button::create("")
            .with_icon("close")
            .with_on_click(app.clone(), on_notice_close as ButtonOnClickCallbackType)
            .dom()
            .with_accessibility_name("Close the notice"),
    );
    Some(row)
}

/// The view's title, a line under it, and its commands.
fn header(s: &Tasks, app: &RefAny, now: NaiveDateTime) -> Dom {
    let today = now.date();
    let (title, sub) = match &s.view {
        View::Smart(Smart::Today) => (
            "Today".to_string(),
            today.format("%A %-d %B").to_string(),
        ),
        View::Smart(smart) => {
            let n = views::smart_count(*smart, &s.tasks, today);
            let what = if *smart == Smart::Completed { "completed" } else { "open" };
            (smart.label().to_string(), format!("{n} {what}"))
        }
        View::List(id) => {
            let name = s.list_name(id);
            let group = s
                .list_index(id)
                .map(|i| s.lists[i].group.clone())
                .unwrap_or_default();
            let open = views::list_count(id, &s.tasks);
            let sub = if group.is_empty() {
                format!("{open} open")
            } else {
                format!("{group} \u{b7} {open} open")
            };
            (name, sub)
        }
        View::Tag(tag) => (format!("#{tag}"), "Tagged tasks".to_string()),
        View::Search(q) => (format!("Search: {q}"), "Title, notes, tags and steps".to_string()),
    };
    let mut text = Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-grow: 1;")
        .with_child(Dom::create_h2_with_text(title).with_id(ids::VIEW_TITLE).with_css(HEADER_TITLE))
        .with_child(Dom::create_span_with_text(sub).with_css(HEADER_SUB));
    if let View::List(id) = &s.view {
        if let Some(i) = s.list_index(id) {
            let dot = Dom::create_div().with_css(format!(
                "width: 10px; height: 10px; border-radius: 5px; background: {}; @media \
                 (prefers-color-scheme: dark) {{ background: {}; }}",
                s.lists[i].color.hex(false),
                s.lists[i].color.hex(true)
            ));
            text = Dom::create_div()
                .with_css("display: flex; flex-direction: row; align-items: center; gap: 8px; flex-grow: 1;")
                .with_child(dot)
                .with_child(text);
        }
    }
    let mut row = Dom::create_div().with_css(HEADER).with_child(text);
    match &s.view {
        View::List(_) => row.add_child(
            Button::create("List settings")
                .with_icon("tune")
                .with_on_click(app.clone(), on_list_settings as ButtonOnClickCallbackType)
                .dom()
                .with_id(ids::LIST_SETTINGS),
        ),
        View::Smart(Smart::Completed) => row.add_child(
            Button::create("Clear older than 30 days")
                .with_icon("delete_sweep")
                .with_on_click(app.clone(), on_clear_completed as ButtonOnClickCallbackType)
                .dom()
                .with_id(ids::CLEAR_COMPLETED),
        ),
        _ => {}
    }
    // "List | Month" on Scheduled, "List | Board" on a list (`layouts.rs`).
    if let Some(switch) = crate::layouts::switch(s, app) {
        row.add_child(switch);
    }
    row
}

/// The quick-add line: the field and "Add".
fn quick_add(s: &Tasks, app: &RefAny) -> Dom {
    let placeholder = match &s.view {
        View::List(id) => format!("Add to {}: \"Pay rent tomorrow 9am #home !high\"", s.list_name(id)),
        _ => "Add a task: \"Pay rent tomorrow 9am #home !high\"".to_string(),
    };
    Dom::create_div()
        .with_css(QUICK_ROW)
        .with_child(
            TextInput::create()
                .with_text(s.quick.text.as_str())
                .with_placeholder(placeholder)
                .with_accessibility_name("Add a task")
                .with_on_text_input(app.clone(), on_quick_text as TextInputOnTextInputCallbackType)
                .with_on_virtual_key_down(app.clone(), on_quick_key as TextInputOnVirtualKeyDownCallbackType)
                .dom()
                .with_id(ids::QUICK_ADD)
                .with_css("flex-grow: 1;"),
        )
        .with_child(
            Button::with_type("Add", ButtonType::Primary)
                .with_icon("add")
                .with_on_click(app.clone(), on_quick_add_click as ButtonOnClickCallbackType)
                .dom()
                .with_id(ids::QUICK_ADD_BUTTON),
        )
}

/// What the quick-add line recognised, as chips (a click takes a part literally).
fn quick_chips(s: &Tasks, app: &RefAny, now: NaiveDateTime) -> Option<Dom> {
    if s.quick.text.trim().is_empty() {
        return None;
    }
    let parsed = s.parse_quick(&s.quick.text, now);
    if parsed.parts.is_empty() {
        return None;
    }
    let mut row = Dom::create_div().with_id(ids::QUICK_CHIPS).with_css(CHIPS_ROW).with_child(
        Dom::create_span_with_text(format!("\u{201c}{}\u{201d}", parsed.title)).with_css("font-size: 12px;"),
    );
    for part in &parsed.parts {
        let kind = match part.kind {
            PartKind::Date | PartKind::Time => ChipKind::Primary,
            PartKind::Repeat => ChipKind::Info,
            PartKind::Priority | PartKind::Flag => ChipKind::Warning,
            PartKind::Tag | PartKind::List => ChipKind::Default,
        };
        row.add_child(
            Chip::with_kind(part.label.as_str(), kind)
                .with_on_click(
                    RefAny::new(ChipRef {
                        app: app.clone(),
                        start: part.start,
                    }),
                    on_chip_click as ChipOnClickCallbackType,
                )
                .dom(),
        );
    }
    row.add_child(Dom::create_span_with_text("Enter to add \u{b7} a click on a part keeps its words").with_css(HINT));
    Some(row)
}

/// The sections, or the view's empty state.
fn body(s: &Tasks, app: &RefAny, now: NaiveDateTime) -> Dom {
    if !s.loaded {
        return ShellEmptyState::create("Reading your tasks...").with_icon("hourglass_empty").dom();
    }
    // The planned month or the board, when the view shows one (`layouts.rs`).
    if let Some(other) = crate::layouts::body(s, app, now) {
        return other;
    }
    let sections = s.sections(now);
    if sections.iter().all(|x| x.tasks.is_empty()) {
        return empty_state(s, app);
    }
    let mut scroll = Dom::create_div()
        .with_id(ids::TASK_LIST)
        .with_css(SCROLL)
        .with_accessibility_name("Tasks");
    for section in &sections {
        scroll.add_child(section_dom(s, app, section, now));
    }
    scroll
}

/// What an empty view says.
fn empty_state(s: &Tasks, app: &RefAny) -> Dom {
    if s.tasks.is_empty() {
        return ShellEmptyState::create("No tasks yet")
            .with_icon("task_alt")
            .with_detail("Type one above, like \"Pay rent tomorrow 9am #home !high\", or try the sample lists.")
            .with_action_label("Add the sample tasks")
            .with_on_action(app.clone(), on_load_sample as ButtonOnClickCallbackType)
            .dom();
    }
    let (icon, title, detail) = match &s.view {
        View::Smart(Smart::Today) => ("wb_sunny", "Nothing due today".to_string(), "Enjoy the day, or add a task above."),
        View::Smart(Smart::Upcoming) => ("date_range", "Nothing in the next 7 days".to_string(), "Tasks with a due date this week show here."),
        View::Smart(Smart::Scheduled) => ("event", "Nothing scheduled".to_string(), "Give a task a due date to see it here."),
        View::Smart(Smart::Flagged) => ("flag", "No flagged tasks".to_string(), "Flag a task (\"!\" in the quick-add line) to keep it here."),
        View::Smart(Smart::All) => ("inbox", "All done".to_string(), "Every task is completed."),
        View::Smart(Smart::Completed) => ("task_alt", "Nothing completed yet".to_string(), "Completed tasks are kept here."),
        View::List(id) => ("checklist", format!("No tasks in {}", s.list_name(id)), "Add one above."),
        View::Tag(tag) => ("sell", format!("No open tasks tagged #{tag}"), "Tags are words with a # in the quick-add line."),
        View::Search(q) => ("search", format!("No tasks match \"{q}\""), "Search looks at titles, notes, tags and steps."),
    };
    ShellEmptyState::create(title).with_icon(icon).with_detail(detail).dom()
}

/// One section: its header (a fold button for "Completed (n)") and its rows.
fn section_dom(s: &Tasks, app: &RefAny, section: &Section, now: NaiveDateTime) -> Dom {
    let mut out = Dom::create_section()
        .with_id(ids::section(&section.key))
        .with_css("display: flex; flex-direction: column;");
    let folded = section.kind == SectionKind::Completed && !s.completed_open;
    match section.kind {
        SectionKind::Completed => {
            let arrow = if folded { "\u{25b8}" } else { "\u{25be}" };
            out.add_child(
                Button::with_type(format!("{arrow} {}", section.title), ButtonType::Link)
                    .with_on_click(app.clone(), on_fold_completed as ButtonOnClickCallbackType)
                    .dom()
                    .with_id(ids::COMPLETED_TOGGLE)
                    .with_css("align-self: flex-start; margin: 8px 0px 4px 0px;"),
            );
        }
        SectionKind::Overdue => {
            out.add_child(Dom::create_h3_with_text(section.title.to_uppercase()).with_css(SECTION_OVERDUE));
        }
        SectionKind::Plain if !section.title.is_empty() => {
            out.add_child(Dom::create_h3_with_text(section.title.to_uppercase()).with_css(SECTION_TITLE));
        }
        SectionKind::Plain => {}
    }
    if folded {
        return out;
    }
    let show_list = !matches!(s.view, View::List(_));
    for &i in &section.tasks {
        out.add_child(row(s, app, &s.tasks[i], now, show_list));
    }
    out
}

/// One task's row.
fn row(s: &Tasks, app: &RefAny, t: &Task, now: NaiveDateTime, show_list: bool) -> Dom {
    let today = now.date();
    let selected = s.is_selected(&t.id);
    let check = CheckBox::create(t.is_done())
        .with_accessibility_name(format!("Complete {}", t.title))
        .with_on_toggle(row_ref(app, &t.id), on_check as CheckBoxOnToggleCallbackType)
        .dom()
        .with_id(ids::task_check(&t.id));

    let mut title_line = Dom::create_div().with_css(TITLE_LINE);
    if t.priority != Priority::None {
        title_line.add_child(
            Dom::create_span_with_text(t.priority.mark())
                .with_css(PRIORITY)
                .with_accessibility_name(format!("{} priority", t.priority.label())),
        );
    }
    title_line.add_child(
        Dom::create_span_with_text(t.title.as_str())
            .with_class(ids::TASK_TITLE_CLASS)
            .with_css(if t.is_done() { TITLE_DONE } else { TITLE }),
    );
    if t.flagged {
        title_line.add_child(Dom::create_icon("flag").with_css(FLAG).with_accessibility_name("Flagged"));
    }

    let mut meta = Dom::create_div().with_css(META_LINE);
    let mut any_meta = false;
    if let Some(done) = t.completed {
        meta.add_child(Dom::create_span_with_text(format!(
            "Completed {}",
            model::day_label(done.date(), today)
        )));
        any_meta = true;
    } else if let Some(label) = views::due_label(t, today) {
        let kind = if views::is_overdue(t, now) {
            ChipKind::Danger
        } else if t.due == Some(today) {
            ChipKind::Primary
        } else {
            ChipKind::Default
        };
        meta.add_child(Chip::with_kind(label, kind).dom().with_class(ids::DUE_CHIP_CLASS));
        any_meta = true;
    }
    if let Some(rule) = &t.repeat {
        meta.add_child(Dom::create_span_with_text(format!("\u{21bb} {}", rule.label())));
        any_meta = true;
    }
    if t.reminder.is_some() && !t.is_done() {
        if let Some(when) = reminders::describe(t, s.settings.reminder_time, today) {
            meta.add_child(
                Dom::create_icon("alarm").with_accessibility_name(format!("Reminder {when}")),
            );
            any_meta = true;
        }
    }
    if show_list {
        if let Some(li) = s.list_index(&t.list) {
            let l = &s.lists[li];
            meta.add_child(Dom::create_span_with_text(l.name.as_str()).with_css(format!(
                "color: {}; @media (prefers-color-scheme: dark) {{ color: {}; }}",
                l.color.hex(false),
                l.color.hex(true)
            )));
            any_meta = true;
        }
    }
    if let Some((done, total)) = t.subtask_progress() {
        meta.add_child(Dom::create_span_with_text(format!("\u{2611} {done}/{total}")));
        any_meta = true;
    }
    for tag in &t.tags {
        meta.add_child(Dom::create_span_with_text(format!("#{tag}")));
        any_meta = true;
    }
    if !t.attachments.is_empty() {
        meta.add_child(Dom::create_icon("attach_file").with_accessibility_name(format!(
            "{} attachment(s)",
            t.attachments.len()
        )));
        any_meta = true;
    }

    let mut text = Dom::create_div().with_css(ROW_TEXT).with_child(title_line);
    if any_meta {
        text.add_child(meta);
    }
    if let Some(first) = t.notes.lines().find(|l| !l.trim().is_empty()) {
        text.add_child(Dom::create_span_with_text(first.trim()).with_css(NOTES));
    }

    let mut row = Dom::create_div()
        .with_id(ids::task_row(&t.id))
        .with_class(ids::TASK_ROW_CLASS)
        .with_css(if selected { ROW_SELECTED } else { ROW })
        .with_tab_index(TabIndex::Auto)
        .with_accessibility_name(t.title.as_str())
        .with_attribute(AttributeType::draggable(true))
        .with_child(check)
        .with_child(text);
    if selected {
        row.add_class(ids::TASK_ROW_SELECTED_CLASS);
    }
    row.add_callback(
        EventFilter::Hover(HoverEventFilter::MouseDown),
        row_ref(app, &t.id),
        on_row_down,
    );
    row.add_callback(
        EventFilter::Hover(HoverEventFilter::DragStart),
        row_ref(app, &t.id),
        on_row_drag_start,
    );
    row.add_callback(
        EventFilter::Hover(HoverEventFilter::DragOver),
        row_ref(app, &t.id),
        on_row_drag_over,
    );
    row.add_callback(
        EventFilter::Hover(HoverEventFilter::Drop),
        row_ref(app, &t.id),
        on_row_drop,
    );
    row
}

// ==== Callbacks: rows ====

pub(crate) extern "C" fn on_row_down(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let mods = info.get_key_modifiers();
    with_row(&mut data, &mut info, |_info, _app, s, id| {
        s.select(id, mods.shift, mods.primary_down());
        s.sync_drafts();
    })
}

extern "C" fn on_check(mut data: RefAny, mut info: CallbackInfo, _state: CheckBoxState) -> Update {
    with_row(&mut data, &mut info, |_info, _app, s, id| {
        if let Some(i) = s.index_of(id) {
            s.toggle_done(i, state::now());
        }
    })
}

pub(crate) extern "C" fn on_row_drag_start(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, id)) = data
        .downcast_ref::<RowRef>()
        .map(|r| (r.app.clone(), r.id.clone()))
    else {
        return Update::DoNothing;
    };
    info.set_drag_data(DRAG_MIME, id.clone().into_bytes());
    if let Some(mut s) = app.downcast_mut::<Tasks>() {
        s.drag = Some(id);
    }
    Update::DoNothing
}

extern "C" fn on_row_drag_over(_data: RefAny, mut info: CallbackInfo) -> Update {
    info.accept_drop();
    Update::DoNothing
}

extern "C" fn on_row_drop(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_row(&mut data, &mut info, |info, app, s, target| {
        let Some(moving) = s.drag.take() else {
            return;
        };
        if moving == target {
            return;
        }
        let moves = s.drop_before(&moving, Some(target));
        crate::jobs::move_files(info, app, s, moves);
    })
}

// ==== Callbacks: quick add ====

/// What a quick-add chip carries: the word its part starts at.
struct ChipRef {
    app: RefAny,
    start: usize,
}

extern "C" fn on_chip_click(mut data: RefAny, mut info: CallbackInfo, _state: ChipState) -> Update {
    let Some((mut app, start)) = data
        .downcast_ref::<ChipRef>()
        .map(|r| (r.app.clone(), r.start))
    else {
        return Update::DoNothing;
    };
    crate::with_tasks(&mut app, &mut info, |_info, _app, s| {
        if !s.quick.ignore.contains(&start) {
            s.quick.ignore.push(start);
        }
        s.quick.shown.clear();
    })
}

extern "C" fn on_quick_text(mut data: RefAny, _info: CallbackInfo, state: TextInputState) -> OnTextInputReturn {
    let Some(mut s) = data.downcast_mut::<Tasks>() else {
        return OnTextInputReturn {
            update: Update::DoNothing,
            valid: TextInputValid::Yes,
        };
    };
    let text = state.get_text().as_str().to_string();
    if text.trim().is_empty() {
        s.quick.ignore.clear();
    }
    s.quick.text = text;
    // The chips change only when a part is recognised or lost: rebuild only then.
    let labels: Vec<String> = s
        .parse_quick(&s.quick.text.clone(), state::now())
        .parts
        .into_iter()
        .map(|p| p.label)
        .collect();
    let update = if labels == s.quick.shown {
        Update::DoNothing
    } else {
        s.quick.shown = labels;
        Update::RefreshDom
    };
    OnTextInputReturn {
        update,
        valid: TextInputValid::Yes,
    }
}

/// Adds the line's task and empties the line.
fn add_quick(info: &mut CallbackInfo, s: &mut Tasks) {
    let now = state::now();
    let parsed = s.parse_quick(&s.quick.text.clone(), now);
    if parsed.title.trim().is_empty() {
        return;
    }
    s.add_parsed(parsed, now);
    s.sync_drafts();
    s.quick = state::QuickAdd::default();
    // The emptied line is the app's: take the typing as seen, so the rebuild shows it empty.
    let revision = info.get_document_text_revision();
    info.mark_text_revision_synced(revision);
}

extern "C" fn on_quick_key(mut data: RefAny, mut info: CallbackInfo, state: TextInputState) -> OnTextInputReturn {
    let key = info.get_current_keyboard_state().current_virtual_keycode.into_option();
    let keep = OnTextInputReturn {
        update: Update::DoNothing,
        valid: TextInputValid::Yes,
    };
    match key {
        Some(VirtualKeyCode::Return | VirtualKeyCode::NumpadEnter) => {
            let text = state.get_text().as_str().to_string();
            let update = crate::with_tasks(&mut data, &mut info, |info, _app, s| {
                s.quick.text = text;
                add_quick(info, s);
            });
            info.prevent_default();
            OnTextInputReturn {
                update,
                valid: TextInputValid::Yes,
            }
        }
        Some(VirtualKeyCode::Escape) => {
            let update = crate::with_tasks(&mut data, &mut info, |info, _app, s| {
                s.quick = state::QuickAdd::default();
                let revision = info.get_document_text_revision();
                info.mark_text_revision_synced(revision);
            });
            OnTextInputReturn {
                update,
                valid: TextInputValid::Yes,
            }
        }
        _ => keep,
    }
}

extern "C" fn on_quick_add_click(mut data: RefAny, mut info: CallbackInfo) -> Update {
    crate::with_tasks(&mut data, &mut info, |info, _app, s| add_quick(info, s))
}

// ==== Callbacks: the rest of the pane ====

extern "C" fn on_fold_completed(mut data: RefAny, mut info: CallbackInfo) -> Update {
    crate::with_tasks(&mut data, &mut info, |_info, _app, s| {
        s.completed_open = !s.completed_open;
    })
}

extern "C" fn on_list_settings(mut data: RefAny, mut info: CallbackInfo) -> Update {
    crate::with_tasks(&mut data, &mut info, |_info, _app, s| {
        if let View::List(id) = s.view.clone() {
            s.commit_drafts();
            s.selection.clear();
            s.editing_list = Some(id.clone());
            if let Some(i) = s.list_index(&id) {
                s.drafts.list = id;
                s.drafts.list_name = s.lists[i].name.clone();
                s.drafts.list_group = s.lists[i].group.clone();
            }
        }
    })
}

extern "C" fn on_clear_completed(mut data: RefAny, mut info: CallbackInfo) -> Update {
    crate::with_tasks(&mut data, &mut info, |_info, _app, s| {
        s.confirm = Some(Confirm::ClearCompleted);
    })
}

extern "C" fn on_undo(mut data: RefAny, mut info: CallbackInfo) -> Update {
    crate::with_tasks(&mut data, &mut info, |_info, _app, s| s.undo_delete())
}

extern "C" fn on_notice_close(mut data: RefAny, mut info: CallbackInfo) -> Update {
    crate::with_tasks(&mut data, &mut info, |info, app, s| {
        s.notice.clear();
        s.files.last_error.clear();
        // The undo is gone with its line: the deleted tasks' files go too.
        if let Some(undo) = s.undo.take() {
            crate::jobs::delete_files(info, app, s, undo.prefixes);
        }
    })
}

extern "C" fn on_load_sample(mut data: RefAny, mut info: CallbackInfo) -> Update {
    crate::with_tasks(&mut data, &mut info, |_info, _app, s| s.add_sample(state::now()))
}

/// "Show": selects the first task reminding (in a view that holds it).
extern "C" fn on_banner_show(mut data: RefAny, mut info: CallbackInfo) -> Update {
    crate::with_tasks(&mut data, &mut info, |_info, _app, s| {
        let Some(id) = s.banners.first().cloned() else {
            return;
        };
        let Some(i) = s.index_of(&id) else {
            return;
        };
        let list = s.tasks[i].list.clone();
        s.show(View::List(list));
        s.select(&id, false, false);
        s.sync_drafts();
        s.banners.clear();
    })
}

/// "Snooze 10 min": every reminding task reminds again in ten minutes.
extern "C" fn on_banner_snooze(mut data: RefAny, mut info: CallbackInfo) -> Update {
    crate::with_tasks(&mut data, &mut info, |_info, _app, s| {
        let at = state::now() + chrono::Duration::minutes(10);
        for id in std::mem::take(&mut s.banners) {
            if let Some(i) = s.index_of(&id) {
                s.tasks[i].reminder = Some(Reminder::At(at));
                s.tasks[i].reminded = None;
                s.save_task(i);
            }
        }
    })
}

extern "C" fn on_banner_dismiss(mut data: RefAny, mut info: CallbackInfo) -> Update {
    crate::with_tasks(&mut data, &mut info, |_info, _app, s| s.banners.clear())
}
