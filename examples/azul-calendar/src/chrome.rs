//! The main window's chrome on the Office scaffold (`OfficeShell`, as AzMail's): the title row,
//! the ribbon (FILE, HOME, VIEW), the navigation pane (the date navigator with today ringed,
//! "My calendars" with their colours, the module switcher), the calendar pane
//! (`views_ui.rs`), the To-Do bar (appointments and tasks) and the status bar (items, and
//! whether the meeting links reached their server). FILE opens the backstage over all of it:
//! Info, Open & Export (.ics), Print, Calendars, Options, About.

use std::path::PathBuf;

use azul::{
    callbacks::{
        ButtonOnClickCallbackType, CheckBoxOnToggleCallbackType, ResumeCallbackType,
        TextInputOnTextInputCallbackType, TextInputOnVirtualKeyDownCallbackType,
    },
    css::DarkLightMode,
    dialog::{FileDialog, FileOpenResult, SaveTargetResult},
    dom::VirtualKeyCode,
    file::FilePath,
    option::{OptionDarkLightMode, OptionFileTypeList, OptionString},
    prelude::*,
    shells::{
        OfficeShell, ShellEmptyState, ShellNavigationModule, ShellNavigationPane,
        ShellNavigationPaneEvent, ShellNavigationPaneEventKind, ShellPane, ShellPaneKind,
        ShellSettingsLayout, ShellSettingsSection,
    },
    str::String as AzString,
    vec::StringVec,
    widgets::{
        Backstage, BackstageNavItem, ButtonType, CheckBoxState, DatePicker, DatePickerState,
        DropDown, OnTextInputReturn, Ribbon, RibbonAppButton, RibbonButton, RibbonGroup,
        RibbonItem, RibbonTab, StatusBar, StatusBarSegment, StatusBarSync, StatusBarSyncKind,
        TextInputState, TextInputValid, Titlebar, ToDoBar, ToDoBarEvent, ToDoBarEventKind,
        ToDoTask,
    },
};
use chrono::{Datelike, Duration, NaiveDate};

use crate::{
    args::BackstagePage,
    calendars::{self, Calendar, Colour},
    editor_ui, event, ics, meet_rooms, meeting, settings, tasks, views,
    views::ViewKind,
    views_ui, week, CalState, ERROR, LABEL, LINE, PAGE, SECONDARY,
};

/// The navigation pane's width, and its width folded to the strip of module icons.
const NAV_PX: f32 = 252.0;
const NAV_FOLDED_PX: f32 = 56.0;
/// How many coming appointments the To-Do bar lists.
const TODO_APPOINTMENTS: usize = 8;
/// The module switcher: Outlook's four, Calendar the one this is.
const MODULES: [(&str, &str); 4] = [
    ("Mail", "mail"),
    ("Calendar", "event"),
    ("Contacts", "person"),
    ("Tasks", "task_alt"),
];
const CALENDAR_MODULE: usize = 1;

/// The window: title row, ribbon or backstage, the panes, the To-Do bar, the status bar.
pub(crate) fn office_shell(s: &CalState, app: &RefAny, window_height: f32) -> Dom {
    let mut shell = OfficeShell::create().with_title_row(
        Titlebar::create("AzCalendar")
            .without_border_bottom()
            .dom(),
    );
    match s.backstage {
        Some(page) => shell = shell.with_backstage(backstage(s, app, page)),
        None => shell = shell.with_ribbon(ribbon(s, app)),
    }
    let width = if s.nav_folded { NAV_FOLDED_PX } else { NAV_PX };
    shell = shell
        .with_pane(
            ShellPane::create("shell-navigation", navigation_pane(s, app))
                .with_kind(ShellPaneKind::Navigation)
                .with_label("Navigation pane")
                .with_width(width),
        )
        .with_pane(
            ShellPane::create(
                "shell-calendar",
                views_ui::calendar_pane(s, app, window_height),
            )
            .with_kind(ShellPaneKind::Main)
            .with_label("Calendar"),
        )
        .with_status_bar(status_bar(s, app));
    if s.todo_bar {
        shell = shell
            .with_right_bar(todo_bar(s, app))
            .with_right_bar_label("To-Do Bar");
    }
    shell.dom()
}

// ==== Ribbon ====

fn large(app: &RefAny, icon: &str, label: &str, cb: ButtonOnClickCallbackType) -> RibbonItem {
    RibbonItem::LargeButton(RibbonButton::create(icon, label).with_on_click(app.clone(), cb))
}

fn toggled(
    app: &RefAny,
    icon: &str,
    label: &str,
    on: bool,
    cb: ButtonOnClickCallbackType,
) -> RibbonItem {
    RibbonItem::LargeButton(
        RibbonButton::create(icon, label)
            .with_toggled(on)
            .with_on_click(app.clone(), cb),
    )
}

/// Arrange: the five views, the shown one pressed in.
fn arrange_group(s: &CalState, app: &RefAny) -> RibbonGroup {
    let views: [(ViewKind, ButtonOnClickCallbackType); 5] = [
        (ViewKind::Day, on_view_day),
        (ViewKind::WorkWeek, on_view_work_week),
        (ViewKind::Week, on_view_week),
        (ViewKind::Month, on_view_month),
        (ViewKind::Schedule, on_view_schedule),
    ];
    views
        .into_iter()
        .fold(RibbonGroup::create("Arrange"), |group, (view, cb)| {
            group.with_item(toggled(app, view.icon(), view.label(), s.view == view, cb))
        })
}

/// FILE (the backstage), HOME and VIEW.
fn ribbon(s: &CalState, app: &RefAny) -> Dom {
    let home = RibbonTab::create("HOME")
        .with_group(
            RibbonGroup::create("New")
                .with_item(large(
                    app,
                    "event",
                    "New Appointment",
                    editor_ui::on_new_appointment,
                ))
                .with_item(large(app, "video_call", "New Meeting", editor_ui::on_new_meeting)),
        )
        .with_group(
            RibbonGroup::create("Go To")
                .with_item(large(app, "today", "Today", on_today))
                .with_item(large(app, "date_range", "Next 7 Days", on_next_seven_days)),
        )
        .with_group(arrange_group(s, app))
        .with_group(
            RibbonGroup::create("Manage Calendars")
                .with_item(large(app, "folder_open", "Open Calendar", on_open_page))
                .with_item(large(app, "edit_calendar", "Calendars", on_calendars_page)),
        )
        .with_group(
            RibbonGroup::create("Share")
                .with_item(large(app, "share", "Share Calendar", on_share)),
        );
    let view = RibbonTab::create("VIEW")
        .with_group(
            RibbonGroup::create("Current View")
                .with_item(toggled(
                    app,
                    "calendar_month",
                    "Calendar",
                    s.view != ViewKind::Agenda,
                    on_view_calendar,
                ))
                .with_item(toggled(
                    app,
                    ViewKind::Agenda.icon(),
                    ViewKind::Agenda.label(),
                    s.view == ViewKind::Agenda,
                    on_view_agenda,
                )),
        )
        .with_group(arrange_group(s, app))
        .with_group(
            RibbonGroup::create("Layout")
                .with_item(toggled(
                    app,
                    "view_sidebar",
                    "Navigation Pane",
                    !s.nav_folded,
                    on_toggle_navigation,
                ))
                .with_item(toggled(app, "checklist", "To-Do Bar", s.todo_bar, on_toggle_todo)),
        )
        .with_group(
            RibbonGroup::create("Look")
                .with_item(large(app, "crop_square", "Flat", on_flat))
                .with_item(large(app, "local_florist", "Flora", on_flora))
                .with_item(large(app, "light_mode", "Light", on_light))
                .with_item(large(app, "dark_mode", "Dark", on_dark)),
        );
    Ribbon::create(vec![home, view])
        .with_app_button(RibbonAppButton::create("FILE").with_on_click(app.clone(), on_file))
        .with_active_tab(s.ribbon_tab)
        .with_on_tab_click(app.clone(), on_ribbon_tab)
        .dom_desktop()
}

// ==== Navigation pane ====

/// A calendar, for the callbacks of its row.
struct CalendarRef {
    app: RefAny,
    id: String,
}

/// The date navigator: the month the navigator shows, inline, today ringed, the view's day
/// selected (when it is in that month).
fn date_navigator(s: &CalState, app: &RefAny) -> Dom {
    let (year, month) = s.nav_month;
    let day = if (s.anchor.year(), s.anchor.month()) == (year, month) {
        s.anchor.day()
    } else {
        0
    };
    DatePicker::create(year.max(1) as u32, month, day)
        .with_inline(true)
        .with_today(
            s.today.year().max(1) as u32,
            s.today.month(),
            s.today.day(),
        )
        .with_accessibility_name("Date navigator")
        .with_on_change(app.clone(), on_nav_date)
        .dom()
        .with_id("date-navigator")
}

/// "My calendars": a row per calendar - its box (shown or not), its colour, its name.
fn my_calendars(s: &CalState, app: &RefAny) -> Dom {
    let mut list = Dom::create_div()
        .with_id("my-calendars")
        .with_css("display: flex; flex-direction: column; margin-top: 12px;")
        .with_child(
            Dom::create_span_with_text("My calendars")
                .with_css("font-size: 12px; font-weight: bold; margin-bottom: 4px;"),
        );
    for (index, c) in s.calendars.iter().enumerate() {
        let target = RefAny::new(CalendarRef {
            app: app.clone(),
            id: c.id.clone(),
        });
        list.add_child(
            Dom::create_div()
                .with_id(format!("calendar-{index}"))
                .with_css(
                    "display: flex; flex-direction: row; align-items: center; padding: 2px 0px;",
                )
                .with_child(
                    CheckBox::create(!s.hidden.contains(&c.id))
                        .with_accessibility_name(c.name.as_str())
                        .with_on_toggle(target, on_calendar_shown as CheckBoxOnToggleCallbackType)
                        .dom(),
                )
                .with_child(Dom::create_div().with_css(format!(
                    "width: 10px; height: 10px; border-radius: 2px; margin-left: 6px; \
                     margin-right: 6px; flex-shrink: 0; {}",
                    c.colour.swatch_css()
                )))
                .with_child(
                    Dom::create_span_with_text(c.name.as_str())
                        .with_css("white-space: nowrap; overflow: hidden; text-overflow: ellipsis;"),
                ),
        );
    }
    list
}

fn navigation_pane(s: &CalState, app: &RefAny) -> Dom {
    let header = Dom::create_div()
        .with_css("display: flex; flex-direction: column; min-width: 0;")
        .with_child(date_navigator(s, app))
        .with_child(my_calendars(s, app));
    let mut pane = ShellNavigationPane::create()
        .with_label("Navigation pane")
        .with_header(header)
        .with_active_module(CALENDAR_MODULE)
        .with_collapsed(s.nav_folded)
        .with_on_event(app.clone(), on_navigation_event);
    for (label, icon) in MODULES {
        pane = pane.with_module(ShellNavigationModule::create(label, icon));
    }
    pane.dom()
}

// ==== Status bar ====

/// "Items: 12", the range shown, and the meeting links' state as the sync segment (a click
/// sends them now).
fn status_bar(s: &CalState, app: &RefAny) -> Dom {
    let (first, last) = views::visible_range(s.view, s.anchor);
    let items = s.occurrences(first, last).len();
    let pending = s.pending_links();
    let (label, kind) = if !s.syncing.is_empty() {
        (
            format!("Sending {} meeting link(s)\u{2026}", s.syncing.len()),
            StatusBarSyncKind::Syncing,
        )
    } else if pending == 0 {
        (
            String::from("Meeting links up to date"),
            StatusBarSyncKind::Connected,
        )
    } else if !s.sync_error.is_empty() {
        (
            format!("{pending} meeting link(s) wait: the server is not reached"),
            StatusBarSyncKind::Error,
        )
    } else {
        (
            format!("{pending} meeting link(s) wait for the server"),
            StatusBarSyncKind::Offline,
        )
    };
    StatusBar::create(vec![
        StatusBarSegment::create(format!("Items: {items}")),
        StatusBarSegment::create(views::title(s.view, s.anchor)),
    ])
    .with_sync(StatusBarSync::create(label, kind).with_on_click(app.clone(), crate::on_sync_now))
    .dom()
}

// ==== To-Do bar ====

/// The coming occurrences the To-Do bar lists: from today, the next seven days, at most
/// `TODO_APPOINTMENTS`, by start.
fn upcoming(s: &CalState) -> Vec<views::Occurrence> {
    let now = chrono::Local::now().naive_local();
    s.occurrences(s.today, s.today + Duration::days(7))
        .into_iter()
        .filter(|o| {
            let e = &s.events[o.index];
            e.all_day || o.first.and_time(e.end) >= now
        })
        .take(TODO_APPOINTMENTS)
        .collect()
}

fn todo_bar(s: &CalState, app: &RefAny) -> Dom {
    let appointments: Vec<AzString> = upcoming(s)
        .iter()
        .map(|o| {
            let e = &s.events[o.index];
            let when = if e.all_day {
                String::from("all day")
            } else {
                e.start.format("%H:%M").to_string()
            };
            AzString::from(format!("{} {when}  {}", o.first.format("%a %-d"), e.title))
        })
        .collect();
    let tasks: Vec<ToDoTask> = s
        .tasks
        .iter()
        .enumerate()
        .map(|(i, t)| ToDoTask::create(i as u64, t.title.as_str()).with_done(t.done))
        .collect();
    ToDoBar::create(
        s.anchor.year().max(1) as u32,
        s.anchor.month(),
        s.anchor.day(),
    )
    .with_today(
        s.today.year().max(1) as u32,
        s.today.month(),
        s.today.day(),
    )
    .with_appointments(StringVec::from(appointments))
    .with_appointments_empty("No upcoming appointments.")
    .with_task_line("Type a new task", s.task_text.as_str())
    .with_tasks(tasks)
    .with_accessibility_name("To-Do Bar")
    .with_on_pick(app.clone(), on_todo_event)
    .with_on_task(app.clone(), on_todo_event)
    .with_on_appointment(app.clone(), on_todo_event)
    .dom()
    .with_id("todo-bar")
}

