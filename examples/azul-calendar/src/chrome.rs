//! The main window's chrome on the Office scaffold (`OfficeShell`, as AzMail's): the title row,
//! the ribbon (FILE, HOME, VIEW), the navigation pane (the date navigator with today ringed,
//! "My calendars" with their colours, the module switcher), the calendar pane
//! (`views_ui.rs`), the To-Do bar (appointments and tasks) and the status bar (items, and
//! whether the meeting links reached their server). FILE opens the backstage over all of it:
//! Info, Open & Export (.ics), Print (`print_ui.rs`), Calendars, Options, About.

use std::path::PathBuf;

use azul::{
    callbacks::{
        ButtonOnClickCallbackType, CheckBoxOnToggleCallbackType, ResumeCallbackType,
        TextInputOnVirtualKeyDownCallbackType,
    },
    css::DarkLightMode,
    dialog::{FileDialog, FileOpenResult},
    dom::VirtualKeyCode,
    option::{OptionDarkLightMode, OptionFileTypeList, OptionString},
    prelude::*,
    shells::{
        OfficeShell, ShellNavigationModule, ShellNavigationPane, ShellNavigationPaneEvent,
        ShellNavigationPaneEventKind, ShellPane, ShellPaneKind, ShellSettingsLayout,
        ShellSettingsSection,
    },
    str::String as AzString,
    vec::StringVec,
    widgets::{
        Backstage, BackstageNavItem, CheckBoxState, DatePicker, DatePickerState,
        DatePickerWeekStart, OnTextInputReturn, Ribbon, RibbonAppButton, RibbonGroup, RibbonItem,
        RibbonTab, StatusBar, StatusBarSegment, StatusBarSync, StatusBarSyncKind, TextInputState,
        TextInputValid, Titlebar, ToDoBar, ToDoBarEvent, ToDoBarEventKind, ToDoTask,
    },
};
use azul_appkit::{
    args::LanguagePref,
    l10n::{label, t, t_args, t_text, Arg},
    pieces::{self, flex_row},
    ribbon::callback_button,
};
use chrono::{Datelike, Duration, NaiveDate};

use crate::{
    args::BackstagePage,
    calendars::{self, Calendar, Colour},
    editor_ui, event, ics, ids, meet_rooms, meeting, print, print_ui, settings, tasks, views,
    views::ViewKind,
    views_ui, CalState, ERROR, LABEL, PAGE, SECONDARY,
};

/// The navigation pane's width, and its width folded to the strip of module icons.
const NAV_PX: f32 = 252.0;
const NAV_FOLDED_PX: f32 = 56.0;
/// How many coming appointments the To-Do bar lists.
const TODO_APPOINTMENTS: usize = 8;
/// The module switcher: Outlook's four, Calendar the one this is (keys of the resources).
const MODULES: [(&str, &str); 4] = [
    ("azcalendar-module-mail", "mail"),
    ("azcalendar-module-calendar", "event"),
    ("azcalendar-module-contacts", "person"),
    ("azcalendar-module-tasks", "task_alt"),
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
            ShellPane::create(ids::NAVIGATION_PANE, navigation_pane(s, app))
                .with_kind(ShellPaneKind::Navigation)
                .with_label(label("azcalendar-navigation-pane-label"))
                .with_width(width),
        )
        .with_pane(
            ShellPane::create(
                ids::CALENDAR_PANE,
                views_ui::calendar_pane(s, app, window_height),
            )
            .with_kind(ShellPaneKind::Main)
            .with_label(label("azcalendar-module-calendar")),
        )
        .with_status_bar(status_bar(s, app));
    if s.todo_bar {
        shell = shell
            .with_right_bar(todo_bar(s, app))
            .with_right_bar_label(label("azcalendar-todo-bar"));
    }
    shell.dom()
}

// ==== Ribbon ====

// Each button has its own callback on the app: azul-appkit's one ribbon builder,
// `callback_button`.
fn large(app: &RefAny, icon: &str, label: &str, cb: ButtonOnClickCallbackType) -> RibbonItem {
    RibbonItem::LargeButton(callback_button(icon, label, app.clone(), cb))
}

fn toggled(
    app: &RefAny,
    icon: &str,
    label: &str,
    on: bool,
    cb: ButtonOnClickCallbackType,
) -> RibbonItem {
    RibbonItem::LargeButton(callback_button(icon, label, app.clone(), cb).with_toggled(on))
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
        .fold(RibbonGroup::create(label("azcalendar-arrange")), |group, (view, cb)| {
            group.with_item(toggled(app, view.icon(), view.label(), s.view == view, cb))
        })
}

/// FILE (the backstage), HOME and VIEW.
fn ribbon(s: &CalState, app: &RefAny) -> Dom {
    let home = RibbonTab::create(label("azcalendar-tab-home"))
        .with_group(
            RibbonGroup::create(label("azcalendar-new"))
                .with_item(large(
                    app,
                    "event",
                    "azcalendar-new-appointment",
                    editor_ui::on_new_appointment,
                ))
                .with_item(large(
                    app,
                    "video_call",
                    "azcalendar-new-meeting",
                    editor_ui::on_new_meeting,
                )),
        )
        .with_group(
            RibbonGroup::create(label("azcalendar-go-to"))
                .with_item(large(app, "today", "azcalendar-today", on_today))
                .with_item(large(app, "date_range", "azcalendar-next-7-days", on_next_seven_days)),
        )
        .with_group(arrange_group(s, app))
        .with_group(
            RibbonGroup::create(label("azcalendar-manage-calendars"))
                .with_item(large(app, "folder_open", "azcalendar-open-calendar", on_open_page))
                .with_item(large(app, "edit_calendar", "azcalendar-file-calendars", on_calendars_page)),
        )
        .with_group(RibbonGroup::create(label("azcalendar-share")).with_item(large(
            app,
            "share",
            "azcalendar-share-calendar",
            on_share,
        )));
    let view = RibbonTab::create(label("azcalendar-tab-view"))
        .with_group(
            RibbonGroup::create(label("azcalendar-current-view"))
                .with_item(toggled(
                    app,
                    "calendar_month",
                    "azcalendar-module-calendar",
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
            RibbonGroup::create(label("azcalendar-layout"))
                .with_item(toggled(
                    app,
                    "view_sidebar",
                    "azcalendar-navigation-pane",
                    !s.nav_folded,
                    on_toggle_navigation,
                ))
                .with_item(toggled(
                    app,
                    "checklist",
                    "azcalendar-todo-bar",
                    s.todo_bar,
                    on_toggle_todo,
                )),
        )
        .with_group(
            RibbonGroup::create(label("azcalendar-look"))
                .with_item(large(app, "crop_square", "kit-theme-flat", on_flat))
                .with_item(large(app, "local_florist", "kit-theme-flora", on_flora))
                .with_item(large(app, "light_mode", "kit-mode-light", on_light))
                .with_item(large(app, "dark_mode", "kit-mode-dark", on_dark)),
        );
    Ribbon::create(vec![home, view])
        .with_app_button(RibbonAppButton::create(label("azcalendar-tab-file")).with_on_click(app.clone(), on_file))
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
        .with_accessibility_name(label("azcalendar-date-navigator"))
        .with_on_change(app.clone(), on_nav_date)
        .dom()
        .with_id(ids::DATE_NAVIGATOR)
}

/// "My calendars": a row per calendar - its box (shown or not), its colour, its name.
fn my_calendars(s: &CalState, app: &RefAny) -> Dom {
    let mut list = Dom::create_div()
        .with_id(ids::MY_CALENDARS)
        .with_css("display: flex; flex-direction: column; margin-top: 12px;")
        .with_child(
            Dom::create_span_with_text(label("azcalendar-my-calendars"))
                .with_css("font-size: 12px; font-weight: bold; margin-bottom: 4px;"),
        );
    for (index, c) in s.calendars.iter().enumerate() {
        let target = RefAny::new(CalendarRef {
            app: app.clone(),
            id: c.id.clone(),
        });
        list.add_child(
            Dom::create_div()
                .with_id(ids::calendar_row(index))
                .with_css(
                    "display: flex; flex-direction: row; align-items: center; padding: 2px 0px;",
                )
                .with_child(
                    CheckBox::create(!s.hidden.contains(&c.id))
                        .with_accessibility_name(crate::calendar_name(c))
                        .with_on_toggle(target, on_calendar_shown as CheckBoxOnToggleCallbackType)
                        .dom(),
                )
                .with_child(Dom::create_div().with_css(format!(
                    "width: 10px; height: 10px; border-radius: 2px; margin-left: 6px; \
                     margin-right: 6px; flex-shrink: 0; {}",
                    c.colour.swatch_css()
                )))
                .with_child(
                    Dom::create_span_with_text(crate::calendar_name(c)).with_css(
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
        .with_label(label("azcalendar-navigation-pane-label"))
        .with_header(header)
        .with_active_module(CALENDAR_MODULE)
        .with_collapsed(s.nav_folded)
        .with_on_event(app.clone(), on_navigation_event);
    for (name, icon) in MODULES {
        pane = pane.with_module(ShellNavigationModule::create(label(name), icon));
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
    let count = |key: &str, n: usize| t_args(key, &[("count", Arg::from(n))]);
    let (sync, kind) = if !s.syncing.is_empty() {
        (
            count("azcalendar-status-sending", s.syncing.len()),
            StatusBarSyncKind::Syncing,
        )
    } else if pending == 0 {
        (t("azcalendar-status-links-done"), StatusBarSyncKind::Connected)
    } else if !s.sync_error.is_empty() {
        (
            count("azcalendar-status-links-unreached", pending),
            StatusBarSyncKind::Error,
        )
    } else {
        (
            count("azcalendar-status-links-waiting", pending),
            StatusBarSyncKind::Offline,
        )
    };
    StatusBar::create(vec![
        StatusBarSegment::create(count("azcalendar-status-items", items)),
        StatusBarSegment::create(views::title(s.view, s.anchor)),
    ])
    .with_sync(StatusBarSync::create(sync, kind).with_on_click(app.clone(), crate::on_sync_now))
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
                t("azcalendar-all-day-lower")
            } else {
                e.start.format("%H:%M").to_string()
            };
            let day = crate::day_text(azul_appkit::l10n::DateStyle::ShortWeekdayDay, o.first);
            AzString::from(format!("{day} {when}  {}", e.title))
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
    // The calendar's weeks run Monday to Sunday: so do the To-Do bar's rows (WIDGETS7).
    .with_week_start(DatePickerWeekStart::Monday)
    .with_appointments(StringVec::from(appointments))
    .with_appointments_empty(label("azcalendar-no-upcoming-appointments"))
    .with_task_line(label("azcalendar-type-new-task"), s.task_text.as_str())
    .with_tasks(tasks)
    .with_accessibility_name(label("azcalendar-todo-bar"))
    .with_on_pick(app.clone(), on_todo_event)
    .with_on_task(app.clone(), on_todo_event)
    .with_on_appointment(app.clone(), on_todo_event)
    .dom()
    .with_id(ids::TODO_BAR)
}

// ==== Backstage ====

/// FILE: the pages, Options and About under a gap.
fn backstage(s: &CalState, app: &RefAny, page: BackstagePage) -> Dom {
    let items: Vec<BackstageNavItem> = BackstagePage::ALL
        .iter()
        .map(|p| {
            let item = BackstageNavItem::create(label(p.label()));
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
        BackstagePage::Print => print_ui::print_page(s, app),
        BackstagePage::Calendars => calendars_page(s, app),
        BackstagePage::Options => options_page(s, app),
        BackstagePage::About => about_page(),
    };
    Backstage::create(items)
        .with_active_item(page.index())
        .with_on_nav_select(app.clone(), on_backstage_nav)
        .with_on_back(app.clone(), on_backstage_back)
        .with_content(content.with_id(ids::backstage_page(page.name())))
        .dom()
}

// The pieces take keys of the resources, or words as they are (appkit's label).

pub(crate) fn page_title(text: &str) -> Dom {
    Dom::create_span_with_text(label(text)).with_css("font-size: 28px; margin-bottom: 8px;")
}

fn heading(text: &str) -> Dom {
    Dom::create_span_with_text(label(text))
        .with_css("font-size: 16px; font-weight: bold; margin-top: 22px; margin-bottom: 4px;")
}

fn note(text: &str) -> Dom {
    Dom::create_span_with_text(label(text))
        .with_css(format!("font-size: 12px; {SECONDARY} margin-top: 4px;"))
}

/// A line of a page: the kit's flex row, below the line before it.
fn line(children: Vec<Dom>) -> Dom {
    flex_row("margin-top: 8px;", children)
}

/// The kit's button, with room after it (the buttons of a [`line`] sit side by side).
fn button(label: &str, id: AzString, app: &RefAny, cb: ButtonOnClickCallbackType) -> Dom {
    pieces::button(label, id, app, cb).with_css("margin-right: 8px;")
}

/// The kit's primary button, with room after it.
fn primary(label: &str, id: AzString, app: &RefAny, cb: ButtonOnClickCallbackType) -> Dom {
    pieces::primary(label, id, app, cb).with_css("margin-right: 8px;")
}

/// How the meeting links stand, in a sentence.
fn sync_status(s: &CalState) -> String {
    match (s.pending_links(), s.sync_error.is_empty()) {
        (0, _) => t("azcalendar-sync-all-there"),
        (n, true) => t_args("azcalendar-sync-sending", &[("count", Arg::from(n))]),
        (n, false) => t_args(
            "azcalendar-sync-waiting",
            &[
                ("count", Arg::from(n)),
                ("why", Arg::from(t_text(&s.sync_error))),
            ],
        ),
    }
}

/// Info: where the calendar is, what it holds, the meeting server.
fn info_page(s: &CalState, app: &RefAny) -> Dom {
    let repeating = s.events.iter().filter(|e| e.repeat.is_some()).count();
    Dom::create_div()
        .with_css(PAGE)
        .with_child(page_title("azcalendar-calendar-information"))
        .with_child(heading("azcalendar-module-calendar"))
        .with_child(note(&t_args(
            "azcalendar-info-counts",
            &[
                ("events", Arg::from(s.events.len())),
                ("repeating", Arg::from(repeating)),
                ("calendars", Arg::from(s.calendars.len())),
                ("tasks", Arg::from(s.tasks.len())),
            ],
        )))
        .with_child(note(&t_args(
            "azcalendar-info-data-folder",
            &[("folder", Arg::from(s.data_dir.display().to_string()))],
        )))
        .with_child(heading("azcalendar-meeting-server"))
        .with_child(note(&s.server))
        .with_child(note(&sync_status(s)))
        .with_child(line(vec![button(
            "azcalendar-sync-meeting-links-now",
            ids::INFO_SYNC,
            app,
            crate::on_sync_now,
        )]))
}

/// The calendars' names for a list, with one choice more at the end (`last`).
fn calendar_names(s: &CalState, last: &str) -> Vec<String> {
    let mut names: Vec<String> = s.calendars.iter().map(crate::calendar_name).collect();
    names.push(last.to_string());
    names
}

/// Open & Export: import an .ics file (a path, or Browse), into a calendar; export one (or
/// all) as an .ics file.
fn open_page(s: &CalState, app: &RefAny) -> Dom {
    let mut page = Dom::create_div()
        .with_css(PAGE)
        .with_child(page_title("azcalendar-file-open"))
        .with_child(heading("azcalendar-import-icalendar-file-ics"))
        .with_child(note(
            "azcalendar-events-from-outlook-google",
        ))
        .with_child(line(vec![
            crate::text_field(
                &s.import_path,
                "/path/to/calendar.ics",
                "azcalendar-file-import",
                ids::IMPORT_PATH,
                app.clone(),
                on_import_path,
            ),
            button("azcalendar-browse", ids::IMPORT_BROWSE, app, on_import_browse),
        ]))
        .with_child(line(vec![
            Dom::create_span_with_text(label("azcalendar-into")).with_css(format!("margin-right: 8px; {SECONDARY}")),
            crate::drop_down(
                calendar_names(s, "azcalendar-new-calendar-named-after"),
                s.import_calendar.min(s.calendars.len()),
                "azcalendar-import-into",
                ids::IMPORT_CALENDAR,
                app.clone(),
                on_import_calendar,
            ),
            primary("azcalendar-import", ids::IMPORT_RUN, app, on_import_run),
        ]))
        .with_child(heading("azcalendar-export-calendar-as-icalendar"))
        .with_child(line(vec![
            // Exports go into the data folder's `exports` folder (the data tree a sync sees).
            crate::text_field(
                &s.export_path,
                "azcalendar-calendar-ics-exports-folder",
                "azcalendar-file-name-export",
                ids::EXPORT_PATH,
                app.clone(),
                on_export_path,
            ),
        ]))
        .with_child(line(vec![
            Dom::create_span_with_text(label("azcalendar-module-calendar"))
                .with_css(format!("margin-right: 8px; {SECONDARY}")),
            crate::drop_down(
                calendar_names(s, "azcalendar-all-calendars"),
                s.export_calendar.min(s.calendars.len()),
                "azcalendar-calendar-export",
                ids::EXPORT_CALENDAR,
                app.clone(),
                on_export_calendar,
            ),
            primary("azcalendar-export", ids::EXPORT_RUN, app, on_export_run),
        ]));
    if !s.io_message.is_empty() {
        let css = if s.io_failed {
            ERROR.to_string()
        } else {
            format!("font-size: 13px; margin-top: 12px; {SECONDARY}")
        };
        page.add_child(
            Dom::create_span_with_text(s.io_message.as_str())
                .with_id(ids::IO_MESSAGE)
                .with_css(css),
        );
    }
    page
}

/// Calendars: each with its name (Enter renames), colour and Remove; and a new one.
fn calendars_page(s: &CalState, app: &RefAny) -> Dom {
    let mut page = Dom::create_div()
        .with_css(PAGE)
        .with_child(page_title("azcalendar-file-calendars"))
        .with_child(note(
            "azcalendar-press-enter-name-rename",
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
                        .with_text(crate::calendar_name(c).as_str())
                        .with_accessibility_name(t_args(
                            "azcalendar-calendar-name-of",
                            &[("name", Arg::from(crate::calendar_name(c)))],
                        ))
                        .with_on_virtual_key_down(target(), on_calendar_rename as TextInputOnVirtualKeyDownCallbackType)
                        .dom()
                        .with_id(ids::calendar_name(index)),
                ),
            crate::drop_down(
                colours.clone(),
                Colour::ALL.iter().position(|x| *x == c.colour).unwrap_or(0),
                &t_args(
                    "azcalendar-calendar-colour-of",
                    &[("name", Arg::from(crate::calendar_name(c)))],
                ),
                ids::calendar_colour(index),
                target(),
                on_calendar_colour,
            ),
        ];
        if !c.is_default() {
            row.push(
                Button::create(label("azcalendar-remove"))
                    .with_on_click(target(), on_calendar_remove)
                    .dom()
                    .with_id(ids::calendar_remove(index)),
            );
        }
        page.add_child(line(row));
    }
    page.add_child(heading("azcalendar-new-calendar"));
    page.add_child(line(vec![
        crate::text_field(
            &s.calendar_name,
            "azcalendar-name",
            "azcalendar-new-calendar-s-name",
            ids::CALENDAR_NEW,
            app.clone(),
            on_new_calendar_name,
        ),
        primary("azcalendar-add", ids::CALENDAR_ADD, app, on_calendar_add),
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
        .with_child(Dom::create_span_with_text(label("azcalendar-meeting-server")).with_css(LABEL))
        .with_child(line(vec![crate::text_field(
            &s.server_text,
            "https://meet.example.com",
            "azcalendar-meeting-server",
            ids::SETTINGS_SERVER,
            app.clone(),
            on_server_text,
        )]))
        .with_child(note(
            "azcalendar-azmeet-links-are-made",
        ))
        .with_child(note(&sync_status(s)));
    if !s.server_error.is_empty() {
        server.add_child(Dom::create_span_with_text(s.server_error.as_str()).with_css(ERROR));
    }
    let server = server.with_child(line(vec![
        button("azcalendar-sync-now", ids::SETTINGS_SYNC, app, crate::on_sync_now),
        primary("azcalendar-save", ids::SETTINGS_SAVE, app, on_server_save),
    ]));
    let check = |checked: bool, text: &str, id: AzString, cb: CheckBoxOnToggleCallbackType| {
        line(vec![
            CheckBox::create(checked)
                .with_accessibility_name(label(text))
                .with_on_toggle(app.clone(), cb)
                .dom()
                .with_id(id),
            Dom::create_span_with_text(label(text)).with_css("margin-left: 6px;"),
        ])
    };
    let look = Dom::create_div()
        .with_css("display: flex; flex-direction: column;")
        .with_child(Dom::create_span_with_text(label("azcalendar-theme-mode")).with_css(LABEL))
        .with_child(line(vec![
            button("kit-theme-flat", ids::SETTINGS_FLAT, app, on_flat),
            button("kit-theme-flora", ids::SETTINGS_FLORA, app, on_flora),
            button("kit-mode-light", ids::SETTINGS_LIGHT, app, on_light),
            button("kit-mode-dark", ids::SETTINGS_DARK, app, on_dark),
        ]))
        .with_child(check(
            s.todo_bar,
            "azcalendar-show-todo-bar",
            ids::SETTINGS_TODO,
            on_todo_checked,
        ))
        .with_child(check(
            !s.nav_folded,
            "azcalendar-show-navigation-pane",
            ids::SETTINGS_NAVIGATION,
            on_navigation_checked,
        ))
        // The language of the words: the system's, English, Deutsch (appkit's words).
        .with_child(Dom::create_span_with_text(label("kit-general-language")).with_css(LABEL))
        .with_child(line(vec![
            language_button(s, LanguagePref::System, ids::SETTINGS_LANGUAGE_SYSTEM, app),
            language_button(s, LanguagePref::English, ids::SETTINGS_LANGUAGE_ENGLISH, app),
            language_button(s, LanguagePref::German, ids::SETTINGS_LANGUAGE_GERMAN, app),
        ]));
    Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0;")
        .with_child(
            ShellSettingsLayout::create(StringVec::from(vec![
                AzString::from(label("azcalendar-meeting-server")),
                AzString::from(label("azcalendar-appearance")),
            ]))
            .with_section(ShellSettingsSection::create(label("azcalendar-meeting-server"), server))
            .with_section(ShellSettingsSection::create(label("azcalendar-appearance"), look))
            .with_active_category(s.options_category)
            .with_on_category(app.clone(), on_options_category)
            .dom(),
        )
}

/// A language of Options > Appearance > Language: the chosen one a primary button.
fn language_button(s: &CalState, language: LanguagePref, id: AzString, app: &RefAny) -> Dom {
    let cb: ButtonOnClickCallbackType = match language {
        LanguagePref::System => on_language_system,
        LanguagePref::English => on_language_english,
        LanguagePref::German => on_language_german,
    };
    if s.language == language {
        primary(language.key(), id, app, cb)
    } else {
        button(language.key(), id, app, cb)
    }
}

/// A language chosen: kept in the settings file, the windows' words switch at once.
fn choose_language(data: &mut RefAny, info: &mut CallbackInfo, language: LanguagePref) -> Update {
    with_state(data, |s| {
        s.language = language;
        s.save_setting(&settings::language_line(language));
        println!("AZCAL_LANGUAGE {}", language.name());
        info.set_locale(language.tag());
        Update::RefreshDom
    })
}

extern "C" fn on_language_system(mut data: RefAny, mut info: CallbackInfo) -> Update {
    choose_language(&mut data, &mut info, LanguagePref::System)
}

extern "C" fn on_language_english(mut data: RefAny, mut info: CallbackInfo) -> Update {
    choose_language(&mut data, &mut info, LanguagePref::English)
}

extern "C" fn on_language_german(mut data: RefAny, mut info: CallbackInfo) -> Update {
    choose_language(&mut data, &mut info, LanguagePref::German)
}

/// About: what this is, and its keys.
fn about_page() -> Dom {
    let keys = [
        ("Ctrl / Cmd + N", "azcalendar-keys-new-appointment"),
        ("Ctrl / Cmd + Shift + Q", "azcalendar-keys-new-meeting"),
        ("Ctrl / Cmd + Alt + 1 .. 6", "azcalendar-keys-views"),
        ("Ctrl / Cmd + T", "azcalendar-today"),
        ("Ctrl / Cmd + P", "azcalendar-file-print"),
        ("Alt + Left / Right", "azcalendar-keys-back-forward"),
        ("F6 / Shift + F6", "azcalendar-keys-pane"),
        ("Ctrl / Cmd + S", "azcalendar-keys-save-close"),
    ];
    let mut page = Dom::create_div()
        .with_css(PAGE)
        .with_child(page_title("AzCalendar"))
        .with_child(note(&format!("{} {}", t("kit-about-version"), env!("CARGO_PKG_VERSION"))))
        .with_child(note(
            "azcalendar-calendar-like-outlook-s",
        ))
        .with_child(heading("azcalendar-keyboard-shortcuts"));
    for (key, what) in keys {
        page.add_child(line(vec![
            Dom::create_span_with_text(keys_said(key))
                .with_css("width: 220px; flex-shrink: 0; font-weight: bold;"),
            Dom::create_span_with_text(label(what)),
        ]));
    }
    page
}

/// A key combination as the window's language names its keys (`Ctrl` is `Strg` in German):
/// appkit's key names, other words as they are.
fn keys_said(combo: &str) -> String {
    combo
        .split(' ')
        .map(|part| {
            if part.len() > 1 && part.chars().all(|c| c.is_ascii_alphabetic()) {
                // `kit-key-ctrl`: its words, else the key's own name.
                let what = format!("key-{}", part.to_ascii_lowercase());
                azul_appkit::l10n::app_word("kit", &what, part)
            } else {
                part.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
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
        s.notice = t("azcalendar-share-later");
        Update::RefreshDom
    })
}

/// Shows the backstage on `page`. Print opens on what the window shows (Outlook's way): the
/// view's print style, a page of it around the view's day.
pub(crate) fn open_backstage(s: &mut CalState, page: BackstagePage) {
    if page == BackstagePage::Print && s.backstage != Some(BackstagePage::Print) {
        s.print = print::Settings::for_view(s.view, s.anchor);
        s.print_message.clear();
        s.print_failed = false;
    }
    s.backstage = Some(page);
}

fn show_page(data: &mut RefAny, page: BackstagePage) -> Update {
    with_state(data, |s| {
        open_backstage(s, page);
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

/// FILE > Print (the menu's Print..., Ctrl / Cmd + P).
pub(crate) extern "C" fn on_print_page(mut data: RefAny, _info: CallbackInfo) -> Update {
    show_page(&mut data, BackstagePage::Print)
}

extern "C" fn on_backstage_nav(mut data: RefAny, _info: CallbackInfo, index: usize) -> Update {
    with_state(&mut data, |s| {
        if let Some(page) = BackstagePage::at(index) {
            open_backstage(s, page);
        }
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
                s.notice = t_args("azcalendar-opening", &[("app", Arg::from(name))]);
                s.launched.push(child);
            }
            Err(e) => {
                s.notice = t_args(
                    "azcalendar-app-not-started",
                    &[("app", Arg::from(name)), ("why", Arg::from(e.to_string()))],
                );
            }
        },
        None => s.notice = t_args("azcalendar-app-missing", &[("app", Arg::from(name))]),
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
                    .map_or_else(|| t("azcalendar-this-module"), |(name, _)| t(name));
                s.notice = t_args("azcalendar-module-not-built", &[("module", Arg::from(name))]);
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
        label("azcalendar-import-dialog"),
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
                t("azcalendar-import-give-file"),
            );
            return Update::RefreshDom;
        }
        // The file is read on a file thread; `import` goes on once it is here.
        report(s, false, t_args("azcalendar-reading", &[("file", Arg::from(typed.as_str()))]));
        crate::writes::read_import(s, &mut info, &app, PathBuf::from(typed));
        Update::RefreshDom
    })
}

/// Imports the .ics `text` read from `path` (`writes::read_import` hands it over).
pub(crate) fn import(s: &mut CalState, path: &std::path::Path, text: &str) {
    let typed = path.display().to_string();
    let parsed = match ics::parse(&text, &chrono::Local) {
        Ok(parsed) => parsed,
        // The one refusal: the text is no iCalendar file.
        Err(e) => {
            eprintln!("[azcalendar] {typed}: {e}");
            let said = t("azcalendar-import-not-icalendar");
            report(s, true, format!("{typed}: {said}"));
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
            .unwrap_or_else(|| t("azcalendar-imported-name"));
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
    let mut problems: Vec<String> = parsed.notes.iter().map(import_note).collect();
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
                problems.push(t_args(
                    "azcalendar-import-left-out",
                    &[
                        ("title", Arg::from(imported.title.as_str())),
                        ("why", Arg::from(e.to_string())),
                    ],
                ));
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
    let mut message = t_args(
        "azcalendar-imported",
        &[
            ("added", Arg::from(added)),
            ("updated", Arg::from(updated)),
            ("file", Arg::from(file.as_str())),
        ],
    );
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

/// What an import could not keep as it was (`ics::ImportNote`), in the window's language.
pub(crate) fn import_note(note: &ics::ImportNote) -> String {
    use ics::ImportNote;
    // The messages quote the title or zone as the language quotes.
    let name = |title: &str| Arg::from(title);
    match note {
        ImportNote::ZoneWithoutRules(zone) => t_args(
            "azcalendar-import-zone-without-rules",
            &[("zone", name(zone))],
        ),
        ImportNote::NoStart(title) => {
            t_args("azcalendar-import-no-start", &[("title", name(title))])
        }
        ImportNote::PastMidnight(title) => {
            t_args("azcalendar-import-past-midnight", &[("title", name(title))])
        }
        ImportNote::FirstDateOnly { title, why } => t_args(
            "azcalendar-import-first-date-only",
            &[("title", name(title)), ("why", Arg::from(why.as_str()))],
        ),
        ImportNote::Cancelled(title) => {
            t_args("azcalendar-import-cancelled", &[("title", name(title))])
        }
        ImportNote::NotEvents(count) => t_args(
            "azcalendar-import-not-events",
            &[("count", Arg::from(*count))],
        ),
    }
}

/// The name of the calendar Export writes (all of them: "AzCalendar").
fn export_name(s: &CalState) -> String {
    s.calendars
        .get(s.export_calendar)
        .map_or_else(|| String::from("AzCalendar"), crate::calendar_name)
}

/// Export: the chosen calendar's events (or all) as an .ics file, at the path given (else in
/// the Documents folder, named after the calendar).
extern "C" fn on_export_run(mut data: RefAny, _info: CallbackInfo) -> Update {
    with_state(&mut data, |s| {
        export(s);
        Update::RefreshDom
    })
}

/// The export goes INTO the data tree (user ruling 2026-10-02: every durable write, exports
/// included, through the drive, so the later sync sees it): `exports/<file name>`, written by
/// the file thread; `AZCAL_EXPORTED <count> <path>` once it landed.
fn export(s: &mut CalState) {
    let name = export_name(s);
    let file = match s.export_path.trim() {
        "" => ics::file_name_for(&name),
        typed => typed.to_string(),
    };
    let key = crate::store::export_key(&file);
    let path = s.data_dir.join(&key);
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
    s.data_writes.put(key.clone(), text.into_bytes());
    s.announce_on_landing(&key, format!("AZCAL_EXPORTED {count} {}", path.display()));
    report(
        s,
        false,
        t_args(
            "azcalendar-exported",
            &[("count", Arg::from(count)), ("path", Arg::from(path.display().to_string()))],
        ),
    );
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
            s.calendar_error = t("azcalendar-calendar-needs-name");
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
            s.calendar_error = t("azcalendar-calendar-give-name");
            return Update::RefreshDom;
        }
        if s.calendars
            .iter()
            .any(|c| crate::calendar_name(c).eq_ignore_ascii_case(&name))
        {
            s.calendar_error = t_args(
                "azcalendar-calendar-exists",
                &[("name", Arg::from(name.as_str()))],
            );
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
                "azcalendar-server-give-address",
            );
            return Update::RefreshDom;
        };
        s.save_setting(&settings::meeting_server_line(&server));
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
