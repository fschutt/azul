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
        TextInputOnVirtualKeyDownCallbackType,
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
        DatePickerWeekStart,
        OnTextInputReturn, Ribbon, RibbonAppButton, RibbonButton, RibbonGroup, RibbonItem,
        RibbonTab, StatusBar, StatusBarSegment, StatusBarSync, StatusBarSyncKind, TextInputState,
        TextInputValid, Titlebar, ToDoBar, ToDoBarEvent, ToDoBarEventKind, ToDoTask,
    },
};
use chrono::{Datelike, Duration, NaiveDate};

use crate::{
    args::BackstagePage,
    calendars::{self, Calendar, Colour},
    editor_ui, event, ics, meet_rooms, meeting, settings, tasks, views,
    views::ViewKind,
    views_ui, CalState, ERROR, LABEL, PAGE, SECONDARY,
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
    let mut shell = OfficeShell::create()
        .with_title_row(Titlebar::create("AzCalendar").without_border_bottom().dom());
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
                .with_item(large(
                    app,
                    "video_call",
                    "New Meeting",
                    editor_ui::on_new_meeting,
                )),
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
        .with_group(RibbonGroup::create("Share").with_item(large(
            app,
            "share",
            "Share Calendar",
            on_share,
        )));
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
                .with_item(toggled(
                    app,
                    "checklist",
                    "To-Do Bar",
                    s.todo_bar,
                    on_toggle_todo,
                )),
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

/// The date navigator: the month the navigator shows, inline, today ringed, the days the view
/// shows lit (`DatePicker::with_range`), the view's day selected when it is in that month.
fn date_navigator(s: &CalState, app: &RefAny) -> Dom {
    let (year, month) = s.nav_month;
    let day = if (s.anchor.year(), s.anchor.month()) == (year, month) {
        s.anchor.day()
    } else {
        0
    };
    let state = |d: NaiveDate| DatePickerState {
        year: d.year().max(1) as u32,
        month: d.month(),
        day: d.day(),
    };
    let (first, last) = views::visible_range(s.view, s.anchor);
    DatePicker::create(year.max(1) as u32, month, day)
        .with_inline(true)
        // The calendar's weeks run Monday to Sunday: so do the navigator's rows.
        .with_week_start(DatePickerWeekStart::Monday)
        .with_today(s.today.year().max(1) as u32, s.today.month(), s.today.day())
        .with_range(state(first), state(last))
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
                    Dom::create_span_with_text(c.name.as_str()).with_css(
                        "white-space: nowrap; overflow: hidden; text-overflow: ellipsis;",
                    ),
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
        .map(|(i, t)| ToDoTask::create(i as u64, t.title.as_str()).with_done(t.is_done()))
        .collect();
    ToDoBar::create(
        s.anchor.year().max(1) as u32,
        s.anchor.month(),
        s.anchor.day(),
    )
    .with_today(s.today.year().max(1) as u32, s.today.month(), s.today.day())
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

// ==== Backstage ====

/// FILE: the pages, Options and About under a gap.
fn backstage(s: &CalState, app: &RefAny, page: BackstagePage) -> Dom {
    let items: Vec<BackstageNavItem> = BackstagePage::ALL
        .iter()
        .map(|p| {
            let item = BackstageNavItem::create(p.label());
            if *p == BackstagePage::Options {
                item.with_gap_before()
            } else {
                item
            }
        })
        .collect();
    let content = match page {
        BackstagePage::Info => info_page(s, app),
        BackstagePage::Open => open_page(s, app),
        BackstagePage::Print => print_page(app),
        BackstagePage::Calendars => calendars_page(s, app),
        BackstagePage::Options => options_page(s, app),
        BackstagePage::About => about_page(),
    };
    Backstage::create(items)
        .with_active_item(page.index())
        .with_on_nav_select(app.clone(), on_backstage_nav)
        .with_on_back(app.clone(), on_backstage_back)
        .with_content(content.with_id(format!("backstage-{}", page.name())))
        .dom()
}

fn page_title(text: &str) -> Dom {
    Dom::create_span_with_text(text).with_css("font-size: 28px; margin-bottom: 8px;")
}

fn heading(text: &str) -> Dom {
    Dom::create_span_with_text(text)
        .with_css("font-size: 16px; font-weight: bold; margin-top: 22px; margin-bottom: 4px;")
}

fn note(text: &str) -> Dom {
    Dom::create_span_with_text(text)
        .with_css(format!("font-size: 12px; {SECONDARY} margin-top: 4px;"))
}

fn line(children: Vec<Dom>) -> Dom {
    let mut row = Dom::create_div()
        .with_css("display: flex; flex-direction: row; align-items: center; margin-top: 8px;");
    for c in children {
        row.add_child(c);
    }
    row
}

fn button(label: &str, id: &str, app: &RefAny, cb: ButtonOnClickCallbackType) -> Dom {
    Button::create(label)
        .with_on_click(app.clone(), cb)
        .dom()
        .with_id(id)
        .with_css("margin-right: 8px;")
}

fn primary(label: &str, id: &str, app: &RefAny, cb: ButtonOnClickCallbackType) -> Dom {
    Button::with_type(label, ButtonType::Primary)
        .with_on_click(app.clone(), cb)
        .dom()
        .with_id(id)
        .with_css("margin-right: 8px;")
}

/// How the meeting links stand, in a sentence.
fn sync_status(s: &CalState) -> String {
    match (s.pending_links(), s.sync_error.is_empty()) {
        (0, _) => String::from("Every meeting link is on the meeting server."),
        (1, true) => String::from("1 meeting link is being sent to the meeting server."),
        (n, true) => format!("{n} meeting links are being sent to the meeting server."),
        (1, false) => format!("1 meeting link waits: {}", s.sync_error),
        (n, false) => format!("{n} meeting links wait: {}", s.sync_error),
    }
}

/// Info: where the calendar is, what it holds, the meeting server.
fn info_page(s: &CalState, app: &RefAny) -> Dom {
    let repeating = s.events.iter().filter(|e| e.repeat.is_some()).count();
    Dom::create_div()
        .with_css(PAGE)
        .with_child(page_title("Calendar information"))
        .with_child(heading("Calendar"))
        .with_child(note(&format!(
            "{} events ({repeating} repeating) in {} calendars, {} tasks.",
            s.events.len(),
            s.calendars.len(),
            s.tasks.len()
        )))
        .with_child(note(&format!("Data folder: {}", s.data_dir.display())))
        .with_child(heading("Meeting server"))
        .with_child(note(&s.server))
        .with_child(note(&sync_status(s)))
        .with_child(line(vec![button(
            "Sync meeting links now",
            "info-sync",
            app,
            crate::on_sync_now,
        )]))
}

/// The calendars' names for a list, with one choice more at the end (`last`).
fn calendar_names(s: &CalState, last: &str) -> Vec<String> {
    let mut names: Vec<String> = s.calendars.iter().map(|c| c.name.clone()).collect();
    names.push(last.to_string());
    names
}

/// Open & Export: import an .ics file (a path, or Browse), into a calendar; export one (or
/// all) as an .ics file.
fn open_page(s: &CalState, app: &RefAny) -> Dom {
    let mut page = Dom::create_div()
        .with_css(PAGE)
        .with_child(page_title("Open & Export"))
        .with_child(heading("Import an iCalendar file (.ics)"))
        .with_child(note(
            "Events from Outlook, Google Calendar, Apple Calendar and others. An event imported \
             again is updated, not added twice.",
        ))
        .with_child(line(vec![
            crate::text_field(
                &s.import_path,
                "/path/to/calendar.ics",
                "File to import",
                "import-path",
                app.clone(),
                on_import_path,
            ),
            button("Browse\u{2026}", "import-browse", app, on_import_browse),
        ]))
        .with_child(line(vec![
            Dom::create_span_with_text("Into").with_css(format!("margin-right: 8px; {SECONDARY}")),
            crate::drop_down(
                calendar_names(s, "A new calendar named after the file"),
                s.import_calendar.min(s.calendars.len()),
                "Import into",
                "import-calendar",
                app.clone(),
                on_import_calendar,
            ),
            primary("Import", "import-run", app, on_import_run),
        ]))
        .with_child(heading("Export a calendar as an iCalendar file"))
        .with_child(line(vec![
            crate::text_field(
                &s.export_path,
                "/path/to/calendar.ics",
                "File to export to",
                "export-path",
                app.clone(),
                on_export_path,
            ),
            button("Browse\u{2026}", "export-browse", app, on_export_browse),
        ]))
        .with_child(line(vec![
            Dom::create_span_with_text("Calendar")
                .with_css(format!("margin-right: 8px; {SECONDARY}")),
            crate::drop_down(
                calendar_names(s, "All calendars"),
                s.export_calendar.min(s.calendars.len()),
                "Calendar to export",
                "export-calendar",
                app.clone(),
                on_export_calendar,
            ),
            primary("Export", "export-run", app, on_export_run),
        ]));
    if !s.io_message.is_empty() {
        let css = if s.io_failed {
            ERROR.to_string()
        } else {
            format!("font-size: 13px; margin-top: 12px; {SECONDARY}")
        };
        page.add_child(
            Dom::create_span_with_text(s.io_message.as_str())
                .with_id("io-message")
                .with_css(css),
        );
    }
    page
}

/// Print: later.
fn print_page(app: &RefAny) -> Dom {
    ShellEmptyState::create("Printing comes later")
        .with_icon("print")
        .with_detail("Meanwhile, Open & Export saves a calendar as an .ics file.")
        .with_action_label("Open & Export")
        .with_on_action(app.clone(), on_open_page)
        .dom()
}

/// Calendars: each with its name (Enter renames), colour and Remove; and a new one.
fn calendars_page(s: &CalState, app: &RefAny) -> Dom {
    let mut page = Dom::create_div()
        .with_css(PAGE)
        .with_child(page_title("Calendars"))
        .with_child(note(
            "Press Enter in a name to rename its calendar. Removing a calendar moves its events \
             into the first one.",
        ));
    let colours: Vec<String> = Colour::ALL.iter().map(|c| c.label().to_string()).collect();
    for (index, c) in s.calendars.iter().enumerate() {
        let target = || {
            RefAny::new(CalendarRef {
                app: app.clone(),
                id: c.id.clone(),
            })
        };
        let mut row = vec![
            Dom::create_div().with_css(format!(
                "width: 14px; height: 14px; border-radius: 3px; margin-right: 8px; flex-shrink: \
                 0; {}",
                c.colour.swatch_css()
            )),
            Dom::create_div()
                .with_css("display: flex; flex-direction: column; flex-grow: 1; min-width: 0; margin-right: 8px;")
                .with_child(
                    TextInput::create()
                        .with_text(c.name.as_str())
                        .with_accessibility_name(format!("Name of {}", c.name))
                        .with_on_virtual_key_down(target(), on_calendar_rename as TextInputOnVirtualKeyDownCallbackType)
                        .dom()
                        .with_id(format!("calendar-name-{index}")),
                ),
            crate::drop_down(
                colours.clone(),
                Colour::ALL.iter().position(|x| *x == c.colour).unwrap_or(0),
                &format!("Colour of {}", c.name),
                &format!("calendar-colour-{index}"),
                target(),
                on_calendar_colour,
            ),
        ];
        if !c.is_default() {
            row.push(
                Button::create("Remove")
                    .with_on_click(target(), on_calendar_remove)
                    .dom()
                    .with_id(format!("calendar-remove-{index}")),
            );
        }
        page.add_child(line(row));
    }
    page.add_child(heading("New calendar"));
    page.add_child(line(vec![
        crate::text_field(
            &s.calendar_name,
            "Name",
            "New calendar's name",
            "calendar-new",
            app.clone(),
            on_new_calendar_name,
        ),
        primary("Add", "calendar-add", app, on_calendar_add),
    ]));
    if !s.calendar_error.is_empty() {
        page.add_child(Dom::create_span_with_text(s.calendar_error.as_str()).with_css(ERROR));
    }
    page
}

/// Options: the meeting server, and the look.
fn options_page(s: &CalState, app: &RefAny) -> Dom {
    let mut server = Dom::create_div()
        .with_css("display: flex; flex-direction: column;")
        .with_child(Dom::create_span_with_text("Meeting server").with_css(LABEL))
        .with_child(line(vec![crate::text_field(
            &s.server_text,
            "https://meet.example.com",
            "Meeting server",
            "settings-server",
            app.clone(),
            on_server_text,
        )]))
        .with_child(note(
            "AzMeet links are made on this computer, so they work offline; this server gets \
             them as soon as it answers.",
        ))
        .with_child(note(&sync_status(s)));
    if !s.server_error.is_empty() {
        server.add_child(Dom::create_span_with_text(s.server_error.as_str()).with_css(ERROR));
    }
    let server = server.with_child(line(vec![
        button("Sync now", "settings-sync", app, crate::on_sync_now),
        primary("Save", "settings-save", app, on_server_save),
    ]));
    let check = |checked: bool, label: &str, id: &str, cb: CheckBoxOnToggleCallbackType| {
        line(vec![
            CheckBox::create(checked)
                .with_accessibility_name(label)
                .with_on_toggle(app.clone(), cb)
                .dom()
                .with_id(id),
            Dom::create_span_with_text(label).with_css("margin-left: 6px;"),
        ])
    };
    let look = Dom::create_div()
        .with_css("display: flex; flex-direction: column;")
        .with_child(Dom::create_span_with_text("Theme and mode").with_css(LABEL))
        .with_child(line(vec![
            button("Flat", "settings-flat", app, on_flat),
            button("Flora", "settings-flora", app, on_flora),
            button("Light", "settings-light", app, on_light),
            button("Dark", "settings-dark", app, on_dark),
        ]))
        .with_child(check(
            s.todo_bar,
            "Show the To-Do bar",
            "settings-todo",
            on_todo_checked,
        ))
        .with_child(check(
            !s.nav_folded,
            "Show the navigation pane",
            "settings-navigation",
            on_navigation_checked,
        ));
    Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0;")
        .with_child(
            ShellSettingsLayout::create(StringVec::from(vec![
                AzString::from("Meeting server"),
                AzString::from("Appearance"),
            ]))
            .with_section(ShellSettingsSection::create("Meeting server", server))
            .with_section(ShellSettingsSection::create("Appearance", look))
            .with_active_category(s.options_category)
            .with_on_category(app.clone(), on_options_category)
            .dom(),
        )
}

/// About: what this is, and its keys.
fn about_page() -> Dom {
    let keys = [
        ("Ctrl / Cmd + N", "New appointment"),
        ("Ctrl / Cmd + Shift + Q", "New meeting"),
        (
            "Ctrl / Cmd + Alt + 1 .. 6",
            "Day, Work Week, Week, Month, Schedule View, List",
        ),
        ("Ctrl / Cmd + T", "Today"),
        ("Alt + Left / Right", "Back, forward"),
        ("F6 / Shift + F6", "The next / previous pane"),
        ("Ctrl / Cmd + S", "Save & Close, in the event window"),
    ];
    let mut page = Dom::create_div()
        .with_css(PAGE)
        .with_child(page_title("AzCalendar"))
        .with_child(note(&format!("Version {}", env!("CARGO_PKG_VERSION"))))
        .with_child(note(
            "A calendar like Outlook's, on the azul GUI toolkit: events are files, AzMeet links \
             are made offline, .ics files come in and go out.",
        ))
        .with_child(heading("Keyboard shortcuts"));
    for (key, what) in keys {
        page.add_child(line(vec![
            Dom::create_span_with_text(key)
                .with_css("width: 220px; flex-shrink: 0; font-weight: bold;"),
            Dom::create_span_with_text(what),
        ]));
    }
    page
}

// ==== Callbacks: ribbon, menu, views ====

/// Runs `f` on the app's state.
fn with_state(data: &mut RefAny, f: impl FnOnce(&mut CalState) -> Update) -> Update {
    match data.downcast_mut::<CalState>() {
        Some(mut s) => f(&mut *s),
        None => Update::DoNothing,
    }
}

fn show_view(data: &mut RefAny, view: ViewKind) -> Update {
    with_state(data, |s| {
        s.set_view(view);
        Update::RefreshDom
    })
}

pub(crate) extern "C" fn on_view_day(mut data: RefAny, _info: CallbackInfo) -> Update {
    show_view(&mut data, ViewKind::Day)
}

pub(crate) extern "C" fn on_view_work_week(mut data: RefAny, _info: CallbackInfo) -> Update {
    show_view(&mut data, ViewKind::WorkWeek)
}

pub(crate) extern "C" fn on_view_week(mut data: RefAny, _info: CallbackInfo) -> Update {
    show_view(&mut data, ViewKind::Week)
}

pub(crate) extern "C" fn on_view_month(mut data: RefAny, _info: CallbackInfo) -> Update {
    show_view(&mut data, ViewKind::Month)
}

pub(crate) extern "C" fn on_view_schedule(mut data: RefAny, _info: CallbackInfo) -> Update {
    show_view(&mut data, ViewKind::Schedule)
}

pub(crate) extern "C" fn on_view_agenda(mut data: RefAny, _info: CallbackInfo) -> Update {
    show_view(&mut data, ViewKind::Agenda)
}

/// VIEW > Calendar: back from the list to the week.
extern "C" fn on_view_calendar(mut data: RefAny, _info: CallbackInfo) -> Update {
    with_state(&mut data, |s| {
        if s.view == ViewKind::Agenda {
            s.set_view(ViewKind::Week);
            Update::RefreshDom
        } else {
            Update::DoNothing
        }
    })
}

/// Go To Today: the view moves to today.
pub(crate) extern "C" fn on_today(mut data: RefAny, _info: CallbackInfo) -> Update {
    with_state(&mut data, |s| {
        s.today = chrono::Local::now().date_naive();
        let today = s.today;
        s.backstage = None;
        s.set_anchor(today);
        Update::RefreshDom
    })
}

/// Next 7 Days: the list, from today.
extern "C" fn on_next_seven_days(mut data: RefAny, _info: CallbackInfo) -> Update {
    with_state(&mut data, |s| {
        let today = s.today;
        s.set_view(ViewKind::Agenda);
        s.set_anchor(today);
        Update::RefreshDom
    })
}

extern "C" fn on_share(mut data: RefAny, _info: CallbackInfo) -> Update {
    with_state(&mut data, |s| {
        s.notice = String::from(
            "Sharing calendars comes later. Meanwhile, FILE > Open & Export saves a calendar as \
             an .ics file anyone can import.",
        );
        Update::RefreshDom
    })
}

fn show_page(data: &mut RefAny, page: BackstagePage) -> Update {
    with_state(data, |s| {
        s.backstage = Some(page);
        Update::RefreshDom
    })
}

/// FILE: the backstage, on Info.
extern "C" fn on_file(mut data: RefAny, _info: CallbackInfo) -> Update {
    show_page(&mut data, BackstagePage::Info)
}

/// Open Calendar / Open & Export: the backstage page that imports and exports .ics files.
pub(crate) extern "C" fn on_open_page(mut data: RefAny, _info: CallbackInfo) -> Update {
    show_page(&mut data, BackstagePage::Open)
}

pub(crate) extern "C" fn on_calendars_page(mut data: RefAny, _info: CallbackInfo) -> Update {
    show_page(&mut data, BackstagePage::Calendars)
}

pub(crate) extern "C" fn on_options_page(mut data: RefAny, _info: CallbackInfo) -> Update {
    show_page(&mut data, BackstagePage::Options)
}

extern "C" fn on_backstage_nav(mut data: RefAny, _info: CallbackInfo, index: usize) -> Update {
    with_state(&mut data, |s| {
        s.backstage = BackstagePage::at(index).or(s.backstage);
        Update::RefreshDom
    })
}

extern "C" fn on_backstage_back(mut data: RefAny, _info: CallbackInfo) -> Update {
    with_state(&mut data, |s| {
        s.backstage = None;
        Update::RefreshDom
    })
}

extern "C" fn on_ribbon_tab(mut data: RefAny, _info: CallbackInfo, index: usize) -> Update {
    with_state(&mut data, |s| {
        s.ribbon_tab = index;
        Update::RefreshDom
    })
}

extern "C" fn on_options_category(mut data: RefAny, _info: CallbackInfo, index: usize) -> Update {
    with_state(&mut data, |s| {
        s.options_category = index;
        Update::RefreshDom
    })
}

/// Shows or folds the navigation pane, and keeps it so for the next start.
fn set_navigation(s: &mut CalState, shown: bool) {
    s.nav_folded = !shown;
    s.save_setting(&settings::line(
        settings::NAVIGATION_FOLDED_KEY,
        if s.nav_folded { "1" } else { "0" },
    ));
}

/// Shows or hides the To-Do bar, and keeps it so for the next start.
fn set_todo_bar(s: &mut CalState, shown: bool) {
    s.todo_bar = shown;
    s.save_setting(&settings::line(
        settings::TODO_BAR_KEY,
        if shown { "1" } else { "0" },
    ));
}

extern "C" fn on_toggle_navigation(mut data: RefAny, _info: CallbackInfo) -> Update {
    with_state(&mut data, |s| {
        let shown = s.nav_folded;
        set_navigation(s, shown);
        Update::RefreshDom
    })
}

extern "C" fn on_toggle_todo(mut data: RefAny, _info: CallbackInfo) -> Update {
    with_state(&mut data, |s| {
        let shown = !s.todo_bar;
        set_todo_bar(s, shown);
        Update::RefreshDom
    })
}

extern "C" fn on_navigation_checked(
    mut data: RefAny,
    _info: CallbackInfo,
    state: CheckBoxState,
) -> Update {
    with_state(&mut data, |s| {
        set_navigation(s, state.checked);
        Update::RefreshDom
    })
}

extern "C" fn on_todo_checked(
    mut data: RefAny,
    _info: CallbackInfo,
    state: CheckBoxState,
) -> Update {
    with_state(&mut data, |s| {
        set_todo_bar(s, state.checked);
        Update::RefreshDom
    })
}

extern "C" fn on_flat(_data: RefAny, mut info: CallbackInfo) -> Update {
    info.set_theme("flat");
    Update::DoNothing
}

extern "C" fn on_flora(_data: RefAny, mut info: CallbackInfo) -> Update {
    info.set_theme("flora");
    Update::DoNothing
}

extern "C" fn on_light(_data: RefAny, mut info: CallbackInfo) -> Update {
    info.set_mode(OptionDarkLightMode::Some(DarkLightMode::Light));
    Update::DoNothing
}

extern "C" fn on_dark(_data: RefAny, mut info: CallbackInfo) -> Update {
    info.set_mode(OptionDarkLightMode::Some(DarkLightMode::Dark));
    Update::DoNothing
}

// ==== Callbacks: navigation pane ====

/// The date navigator: a day picked moves the view there; ‹ / › turn the navigator's month
/// and leave the view where it is.
extern "C" fn on_nav_date(mut data: RefAny, _info: CallbackInfo, state: DatePickerState) -> Update {
    let Some(date) = crate::picked(state) else {
        return Update::DoNothing;
    };
    with_state(&mut data, |s| {
        let month = (date.year(), date.month());
        if month == s.nav_month {
            s.set_anchor(date);
        } else {
            s.nav_month = month;
        }
        Update::RefreshDom
    })
}

/// A calendar's box in "My calendars": shown or hidden, kept for the next start.
extern "C" fn on_calendar_shown(
    mut data: RefAny,
    _info: CallbackInfo,
    state: CheckBoxState,
) -> Update {
    let Some((mut app, id)) = data
        .downcast_ref::<CalendarRef>()
        .map(|r| (r.app.clone(), r.id.clone()))
    else {
        return Update::DoNothing;
    };
    with_state(&mut app, |s| {
        if state.checked {
            s.hidden.remove(&id);
        } else {
            s.hidden.insert(id);
        }
        let value = calendars::hidden_value(&s.hidden);
        s.save_setting(&settings::line(settings::HIDDEN_CALENDARS_KEY, &value));
        Update::RefreshDom
    })
}

/// Starts the azul app `name` (found as AzMeet is: its `<NAME>_BIN`, else next to AzCalendar).
fn launch_app(s: &mut CalState, name: &str, variable: &str) {
    let program = meeting::sibling_program(
        std::env::var(variable).ok().as_deref(),
        std::env::current_exe().ok().as_deref(),
        name,
    );
    match program.filter(|p| p.is_file()) {
        Some(program) => match std::process::Command::new(&program)
            .env_remove("AZ_DEBUG")
            .stdin(std::process::Stdio::null())
            .spawn()
        {
            Ok(child) => {
                s.notice = format!("Opening {name}\u{2026}");
                s.launched.push(child);
            }
            Err(e) => s.notice = format!("{name} could not be started: {e}"),
        },
        None => s.notice = format!("{name} is not installed next to AzCalendar."),
    }
}

/// The module switcher (Mail starts AzMail; Contacts and Tasks are not built yet) and the
/// pane's fold chevron.
extern "C" fn on_navigation_event(
    mut data: RefAny,
    _info: CallbackInfo,
    event: ShellNavigationPaneEvent,
) -> Update {
    with_state(&mut data, |s| match event.kind {
        ShellNavigationPaneEventKind::ModuleSelected => match event.index {
            CALENDAR_MODULE => Update::DoNothing,
            0 => {
                launch_app(s, "AzMail", "AZMAIL_BIN");
                Update::RefreshDom
            }
            other => {
                let name = MODULES
                    .get(other)
                    .map_or("This module", |(label, _)| *label);
                s.notice = format!("{name} is not part of this build yet.");
                Update::RefreshDom
            }
        },
        ShellNavigationPaneEventKind::CollapseToggled => {
            set_navigation(s, event.expand);
            Update::RefreshDom
        }
        _ => Update::DoNothing,
    })
}

// ==== Callbacks: To-Do bar ====

extern "C" fn on_todo_event(
    mut data: RefAny,
    mut info: CallbackInfo,
    event: ToDoBarEvent,
) -> Update {
    match event.kind {
        ToDoBarEventKind::DatePicked => {
            let Some(date) = crate::picked(event.date) else {
                return Update::DoNothing;
            };
            with_state(&mut data, |s| {
                s.set_anchor(date);
                Update::RefreshDom
            })
        }
        ToDoBarEventKind::TaskAdded => {
            let title = event.text.as_str().to_string();
            with_state(&mut data, |s| {
                s.task_text.clear();
                let order = azul_pim::task::next_order(&s.tasks, &s.task_list);
                let now = chrono::Local::now().naive_local();
                if let Some(task) = tasks::new_task(&title, &s.task_list, order, now) {
                    // Its file is written on the file thread (`writes.rs`).
                    s.store_task(&task);
                    s.tasks.push(task);
                    tasks::sort(&mut s.tasks);
                }
                Update::RefreshDom
            })
        }
        ToDoBarEventKind::TaskToggled => with_state(&mut data, |s| {
            let Some(task) = s.tasks.get_mut(event.id as usize) else {
                return Update::DoNothing;
            };
            // Ticking off a repeating task leaves its next occurrence, as in AzTasks.
            let next = tasks::toggle_done(task, chrono::Local::now().naive_local());
            let task = task.clone();
            for changed in std::iter::once(task).chain(next.clone()) {
                s.store_task(&changed);
            }
            s.tasks.extend(next);
            tasks::sort(&mut s.tasks);
            Update::RefreshDom
        }),
        ToDoBarEventKind::AppointmentOpened => {
            let Some((id, date)) = data.downcast_ref::<CalState>().and_then(|s| {
                upcoming(&s)
                    .get(event.index)
                    .map(|o| (s.events[o.index].id.clone(), o.first))
            }) else {
                return Update::DoNothing;
            };
            editor_ui::open_event(&mut data, &mut info, &id, date)
        }
        ToDoBarEventKind::TaskOpened => Update::DoNothing,
    }
}

// ==== Callbacks: Open & Export ====

fn set_text(
    data: &mut RefAny,
    state: &TextInputState,
    field: fn(&mut CalState) -> &mut String,
) -> OnTextInputReturn {
    let text = state.get_text().as_str().to_string();
    with_state(data, |s| {
        *field(s) = text;
        Update::DoNothing
    });
    crate::typed()
}

extern "C" fn on_import_path(
    mut data: RefAny,
    _info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    set_text(&mut data, &state, |s| &mut s.import_path)
}

extern "C" fn on_export_path(
    mut data: RefAny,
    _info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    set_text(&mut data, &state, |s| &mut s.export_path)
}

extern "C" fn on_new_calendar_name(
    mut data: RefAny,
    _info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    set_text(&mut data, &state, |s| &mut s.calendar_name)
}

extern "C" fn on_server_text(
    mut data: RefAny,
    _info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    set_text(&mut data, &state, |s| &mut s.server_text)
}

extern "C" fn on_import_calendar(mut data: RefAny, _info: CallbackInfo, index: usize) -> Update {
    with_state(&mut data, |s| {
        s.import_calendar = index;
        Update::DoNothing
    })
}

extern "C" fn on_export_calendar(mut data: RefAny, _info: CallbackInfo, index: usize) -> Update {
    with_state(&mut data, |s| {
        s.export_calendar = index;
        Update::DoNothing
    })
}

/// Browse: the system's open dialog; the file picked goes into the path field.
extern "C" fn on_import_browse(data: RefAny, _info: CallbackInfo) -> Update {
    let _request = FileDialog::open_file(
        "Import an iCalendar file",
        OptionString::None,
        OptionFileTypeList::None,
        data,
        on_import_picked as ResumeCallbackType,
    );
    Update::DoNothing
}

extern "C" fn on_import_picked(mut data: RefAny, _info: CallbackInfo, result: RefAny) -> Update {
    let Some(path) = FileOpenResult::downcast(result)
        .into_option()
        .and_then(|picked| picked.path.into_option())
    else {
        return Update::DoNothing;
    };
    with_state(&mut data, |s| {
        s.import_path = path.inner.as_str().to_string();
        Update::RefreshDom
    })
}

/// Browse: the system's save dialog; the file chosen goes into the path field.
extern "C" fn on_export_browse(mut data: RefAny, _info: CallbackInfo) -> Update {
    let suggested = data
        .downcast_ref::<CalState>()
        .map(|s| ics::file_name_for(&export_name(&s)))
        .unwrap_or_else(|| String::from("calendar.ics"));
    let _request = FileDialog::save_file(
        "Export an iCalendar file",
        suggested.as_str(),
        data,
        on_export_picked as ResumeCallbackType,
    );
    Update::DoNothing
}

extern "C" fn on_export_picked(mut data: RefAny, _info: CallbackInfo, result: RefAny) -> Update {
    let Some(path) = SaveTargetResult::downcast(result)
        .into_option()
        .and_then(|picked| picked.target.into_option())
        .and_then(|target| target.as_path().into_option())
    else {
        return Update::DoNothing;
    };
    with_state(&mut data, |s| {
        s.export_path = path.inner.as_str().to_string();
        Update::RefreshDom
    })
}

/// Says what an import or export did (or why it did not).
pub(crate) fn report(s: &mut CalState, failed: bool, message: String) {
    if failed {
        eprintln!("[azcalendar] {message}");
    }
    s.io_failed = failed;
    s.io_message = message;
}

/// Import: reads the file, and writes each of its events as an event file of the calendar
/// chosen (or of a new calendar named after the file). An event whose iCalendar UID is one the
/// calendar has already is updated, not added. The view moves to the first one.
extern "C" fn on_import_run(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let app = data.clone();
    with_state(&mut data, |s| {
        let typed = s.import_path.trim().to_string();
        if typed.is_empty() {
            report(
                s,
                true,
                String::from("Give the file to import, or Browse for it."),
            );
            return Update::RefreshDom;
        }
        // The file is read on a file thread; `import` goes on once it is here.
        report(s, false, format!("Reading {typed}..."));
        crate::writes::read_import(s, &mut info, &app, PathBuf::from(typed));
        Update::RefreshDom
    })
}

/// Imports the .ics `text` read from `path` (`writes::read_import` hands it over).
pub(crate) fn import(s: &mut CalState, path: &std::path::Path, text: &str) {
    let typed = path.display().to_string();
    let parsed = match ics::parse(&text, &chrono::Local) {
        Ok(parsed) => parsed,
        Err(e) => {
            report(s, true, format!("{typed}: {e}"));
            return;
        }
    };
    let calendar = if s.import_calendar < s.calendars.len() {
        s.calendars[s.import_calendar].id.clone()
    } else {
        let name = parsed
            .name
            .clone()
            .or_else(|| {
                path.file_stem()
                    .map(|stem| stem.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| String::from("Imported"));
        let made = Calendar {
            id: calendars::new_calendar_id(),
            name,
            colour: calendars::next_colour(&s.calendars),
        };
        s.store_calendar(&made);
        let id = made.id.clone();
        s.calendars.push(made);
        id
    };
    let (mut added, mut updated) = (0usize, 0usize);
    let mut problems = parsed.notes.clone();
    let mut first: Option<NaiveDate> = None;
    for imported in &parsed.events {
        let existing = if imported.uid.is_empty() {
            None
        } else {
            s.events
                .iter()
                .find(|e| e.ical_uid() == imported.uid)
                .map(|e| e.id.clone())
        };
        let id = existing.clone().unwrap_or_else(event::new_event_id);
        let made = match imported.to_event(&id, &calendar) {
            Ok(made) => made,
            Err(e) => {
                problems.push(format!("{:?} is left out: {e}.", imported.title));
                continue;
            }
        };
        let date = made.date;
        match s.store_event(made) {
            Ok(_) if existing.is_some() => updated += 1,
            Ok(_) => added += 1,
            Err(message) => {
                problems.push(message);
                continue;
            }
        }
        first = Some(first.map_or(date, |f: NaiveDate| f.min(date)));
    }
    println!("AZCAL_IMPORTED {} {}", added + updated, path.display());
    let file = path
        .file_name()
        .map_or(typed.clone(), |n| n.to_string_lossy().into_owned());
    let mut message = format!("Imported {added} new and {updated} updated event(s) from {file}.");
    for problem in problems.iter().take(5) {
        message.push(' ');
        message.push_str(problem);
    }
    let failed = added + updated == 0;
    report(s, failed, message);
    if let Some(day) = first {
        s.notice = s.io_message.clone();
        s.backstage = None;
        s.set_anchor(day);
    }
}

/// The name of the calendar Export writes (all of them: "AzCalendar").
fn export_name(s: &CalState) -> String {
    s.calendars
        .get(s.export_calendar)
        .map_or_else(|| String::from("AzCalendar"), |c| c.name.clone())
}

/// Export: the chosen calendar's events (or all) as an .ics file, at the path given (else in
/// the Documents folder, named after the calendar).
extern "C" fn on_export_run(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let app = data.clone();
    with_state(&mut data, |s| {
        export(s, &mut info, &app);
        Update::RefreshDom
    })
}

/// The export file is written on a file thread (`writes::export`); `AZCAL_EXPORTED` and the
/// report line once it landed.
fn export(s: &mut CalState, info: &mut CallbackInfo, app: &RefAny) {
    let name = export_name(s);
    let path = if s.export_path.trim().is_empty() {
        // The Documents folder, else the data tree's exports folder.
        let file = ics::file_name_for(&name);
        match FilePath::get_document_dir().into_option() {
            Some(dir) => PathBuf::from(dir.inner.as_str()).join(file),
            None => s.data_dir.join(crate::store::export_key(&file)),
        }
    } else {
        PathBuf::from(s.export_path.trim())
    };
    let chosen = s.calendars.get(s.export_calendar).map(|c| c.id.clone());
    let events: Vec<&event::Event> = s
        .events
        .iter()
        .filter(|e| {
            chosen
                .as_ref()
                .map_or(true, |id| s.calendar_id_of(e) == *id)
        })
        .collect();
    let text = ics::write(&events, &name, chrono::Utc::now().naive_utc());
    let count = events.len();
    report(s, false, format!("Exporting {count} event(s) to {}...", path.display()));
    crate::writes::export(s, info, app, path, text, count);
}

// ==== Callbacks: Calendars ====

/// Enter in a calendar's name renames it.
extern "C" fn on_calendar_rename(
    mut data: RefAny,
    info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    let key = info
        .get_current_keyboard_state()
        .current_virtual_keycode
        .into_option();
    if !matches!(
        key,
        Some(VirtualKeyCode::Return | VirtualKeyCode::NumpadEnter)
    ) {
        return crate::typed();
    }
    let name = state.get_text().as_str().trim().to_string();
    let Some((mut app, id)) = data
        .downcast_ref::<CalendarRef>()
        .map(|r| (r.app.clone(), r.id.clone()))
    else {
        return crate::typed();
    };
    let update = with_state(&mut app, |s| {
        if name.is_empty() {
            s.calendar_error = String::from("A calendar needs a name.");
            return Update::RefreshDom;
        }
        let Some(c) = s.calendars.iter_mut().find(|c| c.id == id) else {
            return Update::DoNothing;
        };
        c.name = name;
        let c = c.clone();
        s.store_calendar(&c);
        s.calendar_error.clear();
        Update::RefreshDom
    });
    OnTextInputReturn {
        update,
        valid: TextInputValid::Yes,
    }
}

extern "C" fn on_calendar_colour(mut data: RefAny, _info: CallbackInfo, index: usize) -> Update {
    let Some((mut app, id)) = data
        .downcast_ref::<CalendarRef>()
        .map(|r| (r.app.clone(), r.id.clone()))
    else {
        return Update::DoNothing;
    };
    let Some(colour) = Colour::ALL.get(index).copied() else {
        return Update::DoNothing;
    };
    with_state(&mut app, |s| {
        let Some(c) = s.calendars.iter_mut().find(|c| c.id == id) else {
            return Update::DoNothing;
        };
        c.colour = colour;
        let c = c.clone();
        s.store_calendar(&c);
        s.calendar_error.clear();
        Update::RefreshDom
    })
}

/// Remove: the calendar's events move into the default calendar, then its file goes.
extern "C" fn on_calendar_remove(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some((mut app, id)) = data
        .downcast_ref::<CalendarRef>()
        .map(|r| (r.app.clone(), r.id.clone()))
    else {
        return Update::DoNothing;
    };
    with_state(&mut app, |s| {
        if id.is_empty() {
            return Update::DoNothing;
        }
        let moving: Vec<event::Event> = s
            .events
            .iter()
            .filter(|e| e.calendar == id)
            .cloned()
            .collect();
        for mut e in moving {
            e.calendar = String::new();
            if let Err(message) = s.store_event(e) {
                s.calendar_error = message;
                return Update::RefreshDom;
            }
        }
        s.remove_calendar_file(&id);
        s.calendars.retain(|c| c.id != id);
        s.hidden.remove(&id);
        s.import_calendar = 0;
        s.export_calendar = 0;
        s.calendar_error.clear();
        Update::RefreshDom
    })
}

/// Add: a new calendar with the name typed, in the next colour.
extern "C" fn on_calendar_add(mut data: RefAny, _info: CallbackInfo) -> Update {
    with_state(&mut data, |s| {
        let name = s.calendar_name.trim().to_string();
        if name.is_empty() {
            s.calendar_error = String::from("Give the new calendar a name.");
            return Update::RefreshDom;
        }
        if s.calendars
            .iter()
            .any(|c| c.name.eq_ignore_ascii_case(&name))
        {
            s.calendar_error = format!("There is a calendar named {name:?} already.");
            return Update::RefreshDom;
        }
        let made = Calendar {
            id: calendars::new_calendar_id(),
            name,
            colour: calendars::next_colour(&s.calendars),
        };
        s.store_calendar(&made);
        s.calendars.push(made);
        s.calendar_name.clear();
        s.calendar_error.clear();
        Update::RefreshDom
    })
}

// ==== Callbacks: Options ====

/// Saves the meeting server. Links still waiting are registered with the new one (nobody could
/// have joined them anywhere yet); registered ones stay with the server that has their room.
extern "C" fn on_server_save(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let app = data.clone();
    with_state(&mut data, |s| {
        let Some(server) = meet_rooms::normalize_server(&s.server_text) else {
            s.server_error = String::from(
                "Give the meeting server's address, such as https://meet.example.com or \
                 http://127.0.0.1:8787.",
            );
            return Update::RefreshDom;
        };
        s.save_setting(&meet_rooms::encode_settings(&server));
        eprintln!("[azcalendar] meeting server: {server}");
        let moving: Vec<event::Event> = s
            .events
            .iter()
            .filter(|e| {
                e.meeting
                    .as_ref()
                    .is_some_and(|m| m.pending && m.server != server)
            })
            .cloned()
            .collect();
        for mut e in moving {
            if let Some(m) = e.meeting.as_mut() {
                m.server = server.clone();
            }
            if let Err(message) = s.store_event(e) {
                eprintln!("[azcalendar] {message}");
            }
        }
        s.server = server.clone();
        s.server_text = server;
        s.server_error.clear();
        s.sync_refused.clear();
        crate::sync_links(s, &mut info, &app);
        Update::RefreshDom
    })
}
