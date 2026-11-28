//! The main window's sidebar (azul's `Sidebar`): the calendars as coloured check rows - a box in
//! the calendar's colour, its name, "Add a calendar" at the end - and under them the open tasks of
//! the shared task store, also as check rows. What the sidebar shows and what its tags mean is
//! `nav.rs`; this file turns it into widgets and runs what the sidebar reports.
//!
//! Expanded, its header is the date navigator (the month, today ringed, the days the view shows
//! lit); collapsed it is the 18 px rail: Today and, nearest the thumb, New Appointment at the
//! bottom, then the status slot (a pie for the meeting links the meeting server has registered)
//! and the toggle. On a phone the drawer pushes the window aside; adding a calendar closes it.

use azul::{
    callbacks::SidebarOnEventCallbackType,
    css::ColorU,
    prelude::*,
    widgets::{
        DatePicker, DatePickerState, DatePickerWeekStart, PieTone, Sidebar, SidebarEvent, SidebarEventKind,
        SidebarQuickAction, SidebarRow, SidebarRowMark, SidebarSection, SidebarState, SidebarStatus,
        SidebarSurface,
    },
};
use azul_appkit::l10n::{label, t};
use chrono::Datelike;

use crate::{
    args::BackstagePage,
    calendars::Colour,
    chrome, editor_ui, ids,
    nav::{self, LinkTone, NavRow, NavTarget, Quick},
    ui_frame, views, CalState,
};

/// The sidebar of the window now. `dark`: the window is in the dark mode, so a calendar's box
/// takes the dark twin of its colour.
pub(crate) fn sidebar(s: &CalState, app: &RefAny, dark: bool) -> Sidebar {
    let calendars: Vec<(String, Colour, bool)> = s
        .calendars
        .iter()
        .map(|c| (crate::calendar_name(c), c.colour, !s.hidden.contains(&c.id)))
        .collect();
    let left_out = nav::calendars_left_out(calendars.len());
    if left_out > 0 {
        eprintln!(
            "[azcalendar] the sidebar lists {} calendars: {left_out} more are left out",
            nav::MAX_CALENDAR_ROWS
        );
    }
    let titles: Vec<String> = chrome::open_tasks(s)
        .into_iter()
        .map(|i| s.tasks[i].title.clone())
        .collect();
    let plan = nav::plan(&nav::NavInput {
        calendars: &calendars,
        tasks: &titles,
        calendars_title: t("azcalendar-my-calendars"),
        tasks_title: t("azcalendar-tasks-section"),
        add_calendar: t("azcalendar-add-a-calendar"),
    });
    let sections: Vec<SidebarSection> = plan
        .iter()
        .map(|section| {
            SidebarSection::create(section.title.as_str())
                .with_rows(section.rows.iter().map(|row| sidebar_row(row, dark)).collect::<Vec<_>>())
        })
        .collect();
    let mut sidebar = Sidebar::create()
        .with_sections(sections)
        .with_quick_access(quick_actions())
        .with_header(date_navigator(s, app))
        .with_help(label("azcalendar-file-about"))
        .with_label(label("azcalendar-navigation-pane-label"))
        .with_state(s.sidebar)
        .with_surface(SidebarSurface::Theme)
        .with_form(s.form)
        // the date navigator needs its seven columns
        .with_expanded_width(EXPANDED_WIDTH)
        .with_dom_id(ids::SIDEBAR)
        .with_on_event(app.clone(), on_sidebar as SidebarOnEventCallbackType);
    for status in statuses(s) {
        sidebar = sidebar.with_status(status);
    }
    sidebar
}

/// The expanded sidebar's width on a desktop, px: wide enough for the month of the navigator.
const EXPANDED_WIDTH: f32 = 268.0;

/// A row of the plan as a row of the sidebar. None is a button of the rail: the calendars are
/// ticked in the drawer, the rail is for what is quick.
fn sidebar_row(row: &NavRow, dark: bool) -> SidebarRow {
    let mut built = SidebarRow::create(row.label.as_str())
        .with_tag(row.tag)
        .with_in_rail(false);
    if !row.icon.is_empty() {
        built = built.with_icon(row.icon);
    }
    if let Some(checked) = row.checked {
        built = built.with_mark(if checked {
            SidebarRowMark::Checked
        } else {
            SidebarRowMark::Unchecked
        });
    }
    if let Some(colour) = row.colour {
        let (r, g, b) = nav::rgb_of(colour, dark);
        built = built.with_tint(ColorU::rgb(r, g, b));
    }
    built
}

// ---- the date navigator ----

/// The date navigator: the month it shows, inline, today ringed, the days the view shows lit
/// (`DatePicker::with_range`), the view's day selected when it is in that month.
fn date_navigator(s: &CalState, app: &RefAny) -> Dom {
    let (year, month) = s.nav_month;
    let day = if (s.anchor.year(), s.anchor.month()) == (year, month) {
        s.anchor.day()
    } else {
        0
    };
    let state = |d: chrono::NaiveDate| DatePickerState {
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

/// The date navigator: a day picked moves the view there; the arrows turn the navigator's month
/// and leave the view where it is.
extern "C" fn on_nav_date(mut data: RefAny, _info: CallbackInfo, state: DatePickerState) -> Update {
    let Some(date) = crate::picked(state) else {
        return Update::DoNothing;
    };
    let Some(mut s) = data.downcast_mut::<CalState>() else {
        return Update::DoNothing;
    };
    let month = (date.year(), date.month());
    if month == s.nav_month {
        s.set_anchor(date);
        // on a phone the day chosen is what the drawer was opened for
        let phone = ui_frame::phone_now(&s);
        s.sidebar = ui_frame::after_choice(s.sidebar, phone);
    } else {
        s.nav_month = month;
    }
    Update::RefreshDom
}

// ---- the status slot ----

/// The pie's tone for what the links' state means.
#[must_use]
pub(crate) const fn pie_tone(tone: LinkTone) -> PieTone {
    match tone {
        LinkTone::Good => PieTone::Good,
        LinkTone::Busy => PieTone::Busy,
        LinkTone::Warning => PieTone::Warning,
        LinkTone::Danger => PieTone::Danger,
    }
}

/// What the status slot holds: the pie of the meeting links. The slot is the app's data: with no
/// event that has a link there is nothing to say and the slot is empty.
fn statuses(s: &CalState) -> Vec<SidebarStatus> {
    let total = s.events.iter().filter(|e| e.meeting.is_some()).count();
    let failed = !s.sync_error.is_empty();
    match nav::link_status(total, s.pending_links(), s.syncing.len(), failed) {
        Some(status) => vec![
            SidebarStatus::create(t("azcalendar-status-links-label"), status.fraction, pie_tone(status.tone))
                .with_text(format!("{} %", status.percent).as_str())
                .with_detail(chrome::sync_status(s).as_str()),
        ],
        None => Vec::new(),
    }
}

// ---- the quick-access buttons ----

/// The rail's quick-access buttons: Today, then New Appointment nearest the thumb.
fn quick_actions() -> Vec<SidebarQuickAction> {
    nav::QUICK
        .iter()
        .enumerate()
        .map(|(i, (name, icon, _))| SidebarQuickAction::create(label(name), *icon).with_tag(i))
        .collect()
}

// ---- what the sidebar reports ----

/// Every report of the sidebar: a row chosen, the toggle, Help, a quick-access button.
extern "C" fn on_sidebar(mut data: RefAny, mut info: CallbackInfo, event: SidebarEvent) -> Update {
    if event.kind == SidebarEventKind::QuickAction {
        return match nav::quick_of(event.tag) {
            Some(Quick::Today) => chrome::on_today(data, info),
            Some(Quick::NewEvent) => editor_ui::open_new(&mut data, &mut info, false),
            None => Update::DoNothing,
        };
    }
    let Some(mut s) = data.downcast_mut::<CalState>() else {
        return Update::DoNothing;
    };
    let phone = ui_frame::phone_now(&s);
    match event.kind {
        SidebarEventKind::RowClicked => match nav::target_of(event.tag) {
            Some(NavTarget::Calendar(index)) => {
                if !chrome::toggle_calendar(&mut s, index) {
                    return Update::DoNothing;
                }
            }
            Some(NavTarget::AddCalendar) => {
                chrome::open_backstage(&mut s, BackstagePage::Calendars);
                s.sidebar = ui_frame::after_choice(s.sidebar, phone);
            }
            Some(NavTarget::Task(index)) => {
                if !chrome::toggle_open_task(&mut s, index) {
                    return Update::DoNothing;
                }
            }
            None => return Update::DoNothing,
        },
        // a calendar pressed and held: its page (rename, recolour, remove)
        SidebarEventKind::RowLongPressed => {
            if !matches!(nav::target_of(event.tag), Some(NavTarget::Calendar(_))) {
                return Update::DoNothing;
            }
            chrome::open_backstage(&mut s, BackstagePage::Calendars);
        }
        // the toggle, the 16 px handle, a swipe from the rail, a tap on the pushed-aside view
        SidebarEventKind::StateChanged => {
            s.sidebar = event.state;
            // a desktop remembers the sidebar collapsed or open; a phone's drawer is for now
            if !phone {
                let folded = event.state != SidebarState::Expanded;
                chrome::set_navigation(&mut s, !folded);
            }
        }
        SidebarEventKind::HelpClicked => {
            chrome::open_backstage(&mut s, BackstagePage::About);
            s.sidebar = ui_frame::after_choice(s.sidebar, phone);
        }
        _ => return Update::DoNothing,
    }
    Update::RefreshDom
}
